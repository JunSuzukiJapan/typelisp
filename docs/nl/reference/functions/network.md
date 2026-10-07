<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Netwerk (TCP / TLS / Unix-domain / UDP)

Sockets zijn leden van de [streams](streams-files.md). Een verbinding wordt gezien in **twee
weergaven van dezelfde verbinding**, `socket-stream` (tekens) en `socket-byte-stream` (bytes), en
`read-line`/`write-line`/`read-byte`/`format` werken er direct op. Een listener is een
`socket-listener`. **TCP, TLS en Unix-domainsockets delen één type** (zoals `net.Conn` van Go):
eenmaal verbonden zijn lezen en schrijven hetzelfde, en alleen de manier waarop ze worden gemaakt
verschilt. UDP is geen stream maar datagrammen (`udp-socket`).

**Wat wacht is de taak, niet de thread.** `accept`, `read-line`, `write-string`, `tcp-connect`
(inclusief naamresolutie) en `recv-from` stoppen allemaal *die taak* als ze niet gereed zijn (zoals
`sleep`/`recv`), en andere taken blijven draaien. Daarom kan een server in dezelfde vorm als in Go
worden geschreven, `(task (serve c))` per verbinding
([Syntaxreferentie 12.5](../syntax.md#125-waar-taken-wisselen)).

## 1. Types

| Type | Geïmplementeerde traits | Hoe je er een krijgt |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | De `Err` van de onderstaande functies |

Een streamtype heeft één elementtype (om dezelfde reden als bij `file-stream`/`binary-file-stream`),
dus tekens en bytes zijn verschillende types. `byte-stream-of`/`char-stream-of` geven waarden terug
die naar **dezelfde verbinding** wijzen en de ontvangstbuffer delen: zo schrijf je zaken als HTTP,
waarbij de headers als tekens en de body als bytes worden gelezen. Een byte lezen direct na
`unread-char` van een teken is een fout (dezelfde regel als bij bestanden).

## 2. TCP / TLS / Unix-domain

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Maakt verbinding. `host` kan een naam of een adres zijn. Heeft een naam meerdere adressen, dan worden ze om de beurt geprobeerd (`localhost` is `::1` en `127.0.0.1`). Een mislukte naamopzoeking, een geweigerde verbinding of het overschrijden van `:timeout` seconden geeft `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS na `tcp-connect`. Het certificaat wordt geverifieerd tegen de naam `host` (`:server-name` als de te verifiëren naam verschilt van waar je verbinding maakt) met de rootcertificaten van Mozilla. Met `:ca-file` (PEM) vertrouwt het **alleen de certificaten daarin** (een privé-CA, of juist het certificaat dat je eigen `tls-listen` presenteert). `:cert-file`/`:key-file` (beide of geen van beide) zijn ons certificaat, dat wordt gepresenteerd wanneer de server erom vraagt (wederzijdse TLS). De handshake wordt hier voltooid, dus als het certificaat van de server niet slaagt, geeft deze aanroep `Err`. Het resultaat is een gewone `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Maakt verbinding met de Unix-domainsocket `path`. Het is lokaal, dus er is geen handshake-wachttijd en geen timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Luistert. `"127.0.0.1"` is alleen deze machine, `"0.0.0.0"` is elke interface. `0` als `port` doorgeven laat het OS kiezen |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | De TLS-versie van `tcp-listen`. `cert-file` is de certificaatketen (PEM, eigen certificaat eerst), en `key-file` de privésleutel. Beide worden hier gelezen en gecontroleerd, dus een sleutel die niet past geeft `Err` van deze aanroep, niet bij de eerste client. `accept` keert terug **vóór de handshake**, en het eerste lezen of schrijven door de taak die de verbinding afhandelt voltooit de handshake (zoals `tls.Conn` van Go), dus een client die traag is met de handshake houdt andere `accept`s niet op. Met `:client-ca` (PEM) **eist het van elke client** een certificaat dat is uitgegeven door een CA daarin (wederzijdse TLS). Zonder wordt er geen gevraagd |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Luistert op `path`. **`Err` als het bestand al bestaat** (het kan bij een ander draaiend proces horen, dus het wordt niet stilzwijgend vervangen). `close` verwijdert het bestand |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | De volgende verbinding. Stopt de taak totdat er een komt. Geeft na `:timeout` seconden op met `Err` |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Totdat er zonder wachten kan worden gelezen, of `secs` seconden. `true` betekent het eerste (inclusief al gebufferde gegevens). De manier om een klok op een leesbewerking te zetten: `(if (wait-readable c 5.0) (read-line c) ...)`. Wat het belooft is dat **het volgende lezen niet stopt**; `read-line` kan wel op de rest van de regel wachten |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Totdat er kan worden geschreven, of `secs` seconden |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Voegt nog een certificaat toe aan een `tls-listen`-listener, gepresenteerd aan clients die om `name` vragen (SNI): meerdere sites op één listener. Of de keten voor `name` is, wordt hier gecontroleerd, en zo niet geeft deze aanroep `Err`. Clients die vragen om een naam die niemand heeft toegevoegd, of om geen naam, krijgen het certificaat van `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Het subject van het certificaat van de peer (`CN=client,O=Example,C=JP`, RFC 4514-vorm, meest specifiek eerst). Zo komt een server met wederzijdse TLS te weten "wie er verbond" (na de eerste leesbewerking, die de handshake voltooit). Aan de clientkant de naam van het servercertificaat. `none` voor gewone verbindingen, vóór de handshake, of als de peer geen certificaat presenteerde |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Bij een TLS-verbinding aan de serverkant de naam waar de client om vroeg (SNI). Zo komt een server met meerdere sites die met `tls-add-certificate` zijn toegevoegd te weten "welke site". `none` voor gewone verbindingen, aan de clientkant, of zonder naam (verbonden via adres) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Zet Nagle uit (`TCP_NODELAY`). `write-string` gaat elke keer helemaal tot de socket, dus met Nagle aan wacht een antwoord dat in twee delen is geschreven, header en body, op de vertraagde ACK van de peer: gebruik `true` voor request/response-protocollen. Unix-domainsockets hebben geen Nagle, en het slaagt gewoon |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Peilt de peer terwijl er niets gebeurt (`SO_KEEPALIVE`). Detecteert een peer die verdween zonder te sluiten (een uitgetrokken kabel, een gestopte host) en reset de verbinding. De standaardperiode van het OS is lang (vaak 2 uur), dus combineer het met `set-keepalive-period` hieronder. Unix-domainsockets hebben het niet, dus het geeft een panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | De inactieve seconden vóór de eerste peiling en het interval tussen peilingen (hele seconden, minstens 1). Net als `SetKeepAlivePeriod` van Go worden beide op dezelfde waarde gezet |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Als **de peer** deze verbinding verbrak, zijn eerste mislukking (een reset, een TLS-alert, een verbreking tijdens een schrijfbewerking). `none` als alles gezond is: een EOF doordat de peer netjes sluit is geen mislukking. Zie "Mislukkingen van de peer" hieronder |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` van onze kant. De manier om de gekozen poort te leren na `(tcp-listen h 0)`. Bij Unix-sockets het pad (het verbindende uiteinde is `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` van de peer, of het pad bij Unix-sockets |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Sluit alleen de verzendende kant (een half-close). De peer leest EOF, en deze kant kan nog lezen. Het signaal voor "ik heb het hele verzoek verzonden". Bij TLS stuurt het ook `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | De byteversie van dezelfde verbinding |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | De tekenversie van dezelfde verbinding |
| `close` | `(close s)` | `Stream` | Verstuurt de buffer en sluit dan. De GC sluit het niet |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Verbinden, de body uitvoeren, sluiten. `Result<waarde van de body, NetError>` (dezelfde vorm als `with-open-file`) |

`write-string`/`write-line` **keren terug nadat alles is geschreven** (zoals `net.Conn.Write` van Go).
Om kleine schrijfbewerkingen te bundelen, verzamel je ze in een `string-output-stream` en schrijf je
één keer. `listen` is alleen `true` wanneer er iets in de ontvangstbuffer zit, dus
`read-char-no-hang` werkt zoals zijn naam zegt.

**Mislukkingen van de peer geven geen panic.** Als de andere kant de verbinding reset, de
TLS-handshake afwijst of hem midden in een schrijfbewerking laat vallen, is dat geen fout van dit
programma, dus de server stopt niet samen met zijn andere clients. De eerste mislukking wordt op de
verbinding vastgelegd; latere leesbewerkingen geven `none` terug (en zien eruit als EOF), en
schrijfbewerkingen gaan nergens heen en keren stilzwijgend terug. Gebruik `socket-error` om ze te
onderscheiden, dezelfde vorm als `bufio.Scanner.Err` van Go (`read-item` is een `Option` en heeft geen
ander kanaal om via te melden). Alleen fouten van het programma zelf geven een panic (een gesloten
handle, een byte lezen direct na `unread-char`).

**Timeouts zijn expliciete argumenten of `wait-readable`.** Er is geen vorm, zoals `SetReadDeadline`
van Go, waarbij een stream een deadline draagt en `read-line` mislukt: `read-item` geeft
`Option<Item>` terug en kan geen fout teruggeven. Een klok is nodig op drie plekken, verbinden,
accepteren en "het volgende lezen", en elk heeft er een argument voor.

```lisp
;; server: one task per connection
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; client
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

;; TLS server (the certificate can be made, for example, with
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; the serve above as it is; the first read-line is the handshake
          ((err e) (println "accept: ~a" (message e))))))
;; its client: connect trusting its own certificate
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; mutual TLS: the server requires a client certificate issued by ca.pem, and the client presents one
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; on the server side, after the first read-line: who connected
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; two sites on one listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; on the connection, (requested-server-name c) says which
```

Bij wederzijdse TLS voltooit een client die geen certificaat presenteert (of een dat niet slaagt)
onder TLS 1.3 zijn eigen kant van de handshake voordat de server beslist, dus `tls-connect` geeft
`Ok` terug en **het eerste lezen geeft `none` terug** (met de alert in `socket-error`). Aan de
serverkant geeft het eerste lezen van dezelfde verbinding ook `none` terug. Geen van beide kanten
geeft een panic.

## 3. UDP

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Maakt een socket. Nodig, zelfs om alleen te verzenden (`port` is `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Verzendt één datagram. Namen worden opgelost. Of het aankwam is onbekend (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Het volgende datagram. `from` is `ip:port` en kan zoals het is als `host` van `send-to` worden doorgegeven |

Converteer tussen strings en bytereeksen met `string->utf8` / `utf8->string`
([Strings](collections.md#1-strings-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Wat er niet is

- Een manier om iets van het certificaat van de peer te lezen **anders dan het subject** (SAN,
  geldigheidsperiode, uitgever).
- **Met `select` tegelijk op een socket en een kanaal wachten.** Net als in Go schrijf je het als
  "start een taak die leest en laat die een kanaal voeden".
- **HTTP/2**. Unix-domain-**datagrammen** (`SOCK_DGRAM`).
- **Deadlines die door een stream worden gedragen** (om de bovenstaande redenen zijn klokken
  argumenten).
