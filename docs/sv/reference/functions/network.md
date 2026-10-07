<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Nätverk (TCP / TLS / Unix-domän / UDP)

Socketar är medlemmar av [strömmarna](streams-files.md). En anslutning ses i **två vyer av samma
anslutning**, `socket-stream` (tecken) och `socket-byte-stream` (byte), och
`read-line`/`write-line`/`read-byte`/`format` fungerar på dem som de är. En lyssnare är en
`socket-listener`. **TCP, TLS och Unix-domänsocketar delar en typ** (som Gos `net.Conn`): när
anslutningen väl är upprättad är läsning och skrivning desamma, och bara hur de skapas skiljer sig. UDP
är inte en ström utan datagram (`udp-socket`).

**Det som väntar är tasken, inte tråden.** `accept`, `read-line`, `write-string`, `tcp-connect` (inklusive
namnuppslagning) och `recv-from` stoppar alla *den tasken* om de inte är redo (som `sleep`/`recv`), och
andra tasks fortsätter köra. Därför kan en server skrivas i samma form som i Go, `(task (serve c))` per
anslutning ([Syntaxreferens 12.5](../syntax.md#125-var-tasks-byter)).

## 1. Typer

| Typ | Implementerade traits | Hur man får en |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` från funktionerna nedan |

En strömtyp har en elementtyp (av samma skäl som `file-stream`/`binary-file-stream`), så tecken och byte
är olika typer. `byte-stream-of`/`char-stream-of` returnerar värden som pekar på **samma anslutning** och
delar mottagningsbufferten: så skriver man sådant som HTTP, där rubrikerna läses som tecken och
innehållet som byte. Att läsa ett byte direkt efter `unread-char` av ett tecken är ett fel (samma regel
som för filer).

## 2. TCP / TLS / Unix-domän

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Ansluter. `host` kan vara ett namn eller en adress. Om ett namn har flera adresser prövas de i tur och ordning (`localhost` är `::1` och `127.0.0.1`). En misslyckad namnuppslagning, en nekad anslutning eller att `:timeout` sekunder överskrids ger `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS efter `tcp-connect`. Certifikatet verifieras mot `host`-namnet (`:server-name` om namnet som ska verifieras skiljer sig från där du ansluter) med Mozillas rotcertifikat. Med `:ca-file` (PEM) litar den på **bara certifikaten i den** (en privat CA, eller just det certifikat som din egen `tls-listen` presenterar). `:cert-file`/`:key-file` (båda eller ingen) är vårt certifikat, som presenteras när servern ber om ett (ömsesidig TLS). Handskakningen slutförs här, så om serverns certifikat inte godkänns returnerar det här anropet `Err`. Resultatet är en vanlig `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Ansluter till Unix-domänsocketen `path`. Den är lokal, så det finns ingen väntan på handskakning och ingen timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Lyssnar. `"127.0.0.1"` är bara den här maskinen, `"0.0.0.0"` är alla gränssnitt. Att skicka `0` som `port` låter OS välja |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | TLS-versionen av `tcp-listen`. `cert-file` är certifikatkedjan (PEM, eget certifikat först) och `key-file` den privata nyckeln. Båda läses och kontrolleras här, så en nyckel som inte stämmer ger `Err` från det här anropet, inte vid den första klienten. `accept` returnerar **före handskakningen**, och den första läsningen eller skrivningen av tasken som hanterar anslutningen slutför handskakningen (som Gos `tls.Conn`), så en klient som är långsam med handskakningen håller inte upp andra `accept`. Med `:client-ca` (PEM) **kräver den att varje klient** presenterar ett certifikat utfärdat av en CA i den (ömsesidig TLS). Utan den efterfrågas inget |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Lyssnar vid `path`. **`Err` om filen redan finns** (den kan tillhöra en annan process som körs, så den ersätts inte tyst). `close` tar bort filen |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Nästa anslutning. Stoppar tasken tills en kommer. Ger upp med `Err` efter `:timeout` sekunder |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Tills det kan läsas utan att vänta, eller `secs` sekunder. `true` betyder det förra (inklusive data som redan ligger i bufferten). Sättet att sätta en klocka på en läsning: `(if (wait-readable c 5.0) (read-line c) ...)`. Det den lovar är att **nästa läsning inte stoppar**; `read-line` kan vänta på resten av raden |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Tills det kan skrivas, eller `secs` sekunder |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Lägger till ytterligare ett certifikat till en `tls-listen`-lyssnare, som presenteras för klienter som ber om `name` (SNI): flera webbplatser på en lyssnare. Om kedjan är för `name` kontrolleras här, och om inte returnerar det här anropet `Err`. Klienter som ber om ett namn som ingen lagt till, eller om inget namn, får certifikatet från `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Subjektet i motpartens certifikat (`CN=client,O=Example,C=JP`, RFC 4514-form, mest specifikt först). Så får en server med ömsesidig TLS veta "vem som anslöt" (efter den första läsningen, som slutför handskakningen). På klientsidan, servercertifikatets namn. `none` för vanliga anslutningar, före handskakningen, eller om motparten inte presenterade något certifikat |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | På en TLS-anslutning på serversidan, namnet klienten bad om (SNI). Så får en server med flera webbplatser som lagts till med `tls-add-certificate` veta "vilken webbplats". `none` för vanliga anslutningar, på klientsidan, eller utan namn (ansluten via adress) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Stänger av Nagle (`TCP_NODELAY`). `write-string` går hela vägen till socketen varje gång, så med Nagle på väntar ett svar som skrivs i två delar, rubrik och innehåll, på motpartens fördröjda ACK: använd `true` för begäran/svar-protokoll. Unix-domänsocketar har ingen Nagle, och det lyckas bara |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sonderar motparten under inaktivitet (`SO_KEEPALIVE`). Upptäcker en motpart som försvunnit utan att stänga (en utdragen kabel, en stoppad värd) och återställer anslutningen. OS-standardperioden är lång (ofta 2 timmar), så kombinera med `set-keepalive-period` nedan. Unix-domänsocketar har det inte, så det ger panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Antalet inaktiva sekunder före den första sonderingen och intervallet mellan sonderingar (hela sekunder, minst 1). Som Gos `SetKeepAlivePeriod` sätts båda till samma värde |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Om **motparten** bröt den här anslutningen, dess första misslyckande (en återställning, ett TLS-larm, en nedkoppling under en skrivning). `none` om den är frisk: ett EOF från att motparten stängde rent är inte ett misslyckande. Se "Misslyckanden hos motparten" nedan |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | Vår sidas `host:port`. Sättet att få veta den valda porten efter `(tcp-listen h 0)`. För Unix-socketar, sökvägen (den anslutande änden är `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | Motpartens `host:port`, eller sökvägen för Unix-socketar |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Stänger bara sändsidan (en halvstängning). Motparten läser EOF, och den här sidan kan fortfarande läsa. Signalen för "jag har skickat hela begäran". För TLS skickar den också `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Byteversionen av samma anslutning |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Teckenversionen av samma anslutning |
| `close` | `(close s)` | `Stream` | Skickar ut bufferten och stänger sedan. GC stänger den inte |
| `with-connection` | `(with-connection (var host port) body...)` | Makro | Anslut, kör kroppen, stäng. `Result<kroppens värde, NetError>` (samma form som `with-open-file`) |

`write-string`/`write-line` **returnerar när allt är skrivet** (som Gos `net.Conn.Write`). För att
bunta ihop små skrivningar samlar man dem i en `string-output-stream` och skriver en gång. `listen` är
`true` bara när något finns i mottagningsbufferten, så `read-char-no-hang` fungerar som namnet säger.

**Misslyckanden hos motparten ger inte panic.** Om andra änden återställer anslutningen, avvisar
TLS-handskakningen eller släpper den mitt i en skrivning är det inget fel i det här programmet, så
servern stoppar inte tillsammans med sina andra klienter. Det första misslyckandet registreras på
anslutningen; senare läsningar returnerar `none` (ser likadant ut som EOF), och skrivningar går ingenstans
och returnerar tyst. För att skilja dem åt används `socket-error`, samma form som Gos
`bufio.Scanner.Err` (`read-item` är ett `Option` och har ingen annan kanal att rapportera genom). Bara
fel i själva programmet ger panic (ett stängt handtag, att läsa ett byte direkt efter `unread-char`).

**Timeouts är explicita argument eller `wait-readable`.** Det finns ingen form, som Gos
`SetReadDeadline`, där en ström bär en tidsgräns och `read-line` misslyckas: `read-item` returnerar
`Option<Item>` och har inget sätt att returnera ett fel. En klocka behövs på tre ställen, att ansluta, att
ta emot anslutningar och "nästa läsning", och vart och ett har ett argument för det.

```lisp
;; server: en task per anslutning
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; klient
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; TLS-server (certifikatet kan till exempel göras med
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; serve ovan som den är; den första read-line är handskakningen
          ((err e) (println "accept: ~a" (message e))))))
;; dess klient: anslut och lita på dess eget certifikat
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; ömsesidig TLS: servern kräver ett klientcertifikat utfärdat av ca.pem, och klienten presenterar ett
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; på serversidan, efter den första read-line: vem som anslöt
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; två webbplatser på en lyssnare (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; på anslutningen säger (requested-server-name c) vilken
```

Med ömsesidig TLS slutför en klient som inte presenterar något certifikat (eller ett som inte godkänns)
sin egen sida av handskakningen innan servern avgör, under TLS 1.3, så `tls-connect` returnerar `Ok` och
**den första läsningen returnerar `none`** (med larmet i `socket-error`). På serversidan returnerar den
första läsningen av samma anslutning också `none`. Ingen av sidorna ger panic.

## 3. UDP

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Skapar en socket. Behövs även bara för att skicka (`port` är `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Skickar ett datagram. Namn löses upp. Om det kom fram är okänt (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Nästa datagram. `from` är `ip:port` och kan skickas som det är som `host` till `send-to` |

Konvertera mellan strängar och bytesekvenser med `string->utf8` / `utf8->string`
([Strängar](collections.md#1-strängar-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Vad som inte finns

- Ett sätt att läsa något av motpartens certifikat **annat än subjektet** (SAN, giltighetstid,
  utfärdare).
- **Att vänta på en socket och en kanal samtidigt med `select`.** Som i Go skriver man det som "starta
  en task som läser och låt den mata en kanal".
- **HTTP/2**. Unix-domän-**datagram** (`SOCK_DGRAM`).
- **Tidsgränser som bärs av en ström** (av skälen ovan är klockor argument).
