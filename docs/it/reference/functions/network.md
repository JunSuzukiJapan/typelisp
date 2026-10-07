<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Rete (TCP / TLS / socket di dominio Unix / UDP)

I socket fanno parte degli [stream](streams-files.md). Una connessione si vede in **due viste della stessa
connessione**, `socket-stream` (caratteri) e `socket-byte-stream` (byte), e
`read-line`/`write-line`/`read-byte`/`format` funzionano su di esse così come sono. Un listener è un
`socket-listener`. **TCP, TLS e i socket di dominio Unix condividono un unico tipo** (come il `net.Conn`
di Go): una volta connessi, la lettura e la scrittura sono uguali, e differisce solo il modo in cui
vengono creati. UDP non è uno stream ma datagrammi (`udp-socket`).

**A attendere è il task, non il thread.** `accept`, `read-line`, `write-string`, `tcp-connect` (inclusa la
risoluzione dei nomi) e `recv-from` fermano tutti *quel task* se non sono pronti (come
`sleep`/`recv`), e gli altri task continuano a funzionare. Per questo un server può essere scritto nella
stessa forma che in Go, `(task (serve c))` per ogni connessione
([Riferimento della sintassi 12.5](../syntax.md#125-dove-i-task-si-alternano)).

## 1. Tipi

| Tipo | Trait implementati | Come ottenerne uno |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | L'`Err` delle funzioni seguenti |

Un tipo di stream ha un solo tipo di elemento (per lo stesso motivo di `file-stream`/`binary-file-stream`),
quindi caratteri e byte sono tipi diversi. `byte-stream-of`/`char-stream-of` restituiscono valori che
puntano alla **stessa connessione** e condividono il buffer di ricezione: è così che si scrivono cose
come HTTP, in cui le intestazioni vengono lette come caratteri e il corpo come byte. Leggere un byte
subito dopo `unread-char` di un carattere è un errore (la stessa regola dei file).

## 2. TCP / TLS / Unix domain

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Si connette. `host` può essere un nome o un indirizzo. Se un nome ha più indirizzi, vengono provati a turno (`localhost` è `::1` e `127.0.0.1`). Una risoluzione del nome fallita, una connessione rifiutata o il superamento di `:timeout` secondi danno `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS dopo `tcp-connect`. Il certificato viene verificato rispetto al nome `host` (`:server-name` se il nome da verificare differisce da dove ci si connette) con i certificati radice di Mozilla. Dato `:ca-file` (PEM), si fida **solo dei certificati in esso contenuti** (una CA privata, o lo stesso certificato che presenta il tuo `tls-listen`). `:cert-file`/`:key-file` (entrambi o nessuno) sono il nostro certificato, presentato quando il server ne chiede uno (TLS reciproco). L'handshake si completa qui, quindi se il certificato del server non passa, questa chiamata restituisce `Err`. Il risultato è un normale `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Si connette al socket di dominio Unix `path`. È locale, quindi non c'è attesa di handshake né timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Si mette in ascolto. `"127.0.0.1"` è solo questa macchina, `"0.0.0.0"` è ogni interfaccia. Passare `0` come `port` lascia scegliere al sistema operativo |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | La versione TLS di `tcp-listen`. `cert-file` è la catena di certificati (PEM, prima il proprio certificato), e `key-file` la chiave privata. Entrambi vengono letti e controllati qui, quindi una chiave che non corrisponde dà `Err` da questa chiamata, non al primo client. `accept` restituisce **prima dell'handshake**, e la prima lettura o scrittura del task che gestisce la connessione completa l'handshake (come il `tls.Conn` di Go), quindi un client lento nell'handshake non blocca gli altri `accept`. Dato `:client-ca` (PEM), **richiede a ogni client** di presentare un certificato emesso da una CA in esso contenuta (TLS reciproco). Senza, non ne viene chiesto nessuno |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Si mette in ascolto su `path`. **`Err` se il file esiste già** (potrebbe appartenere a un altro processo in esecuzione, quindi non viene sostituito in silenzio). `close` rimuove il file |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | La connessione successiva. Ferma il task finché non ne arriva una. Rinuncia con `Err` dopo `:timeout` secondi |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Finché si può leggere senza attendere, oppure `secs` secondi. `true` significa la prima cosa (inclusi i dati già nel buffer). Il modo di mettere un orologio a una lettura: `(if (wait-readable c 5.0) (read-line c) ...)`. Ciò che promette è che **la lettura successiva non si ferma**; `read-line` può attendere il resto della riga |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Finché si può scrivere, oppure `secs` secondi |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Aggiunge un altro certificato a un listener `tls-listen`, presentato ai client che chiedono `name` (SNI): più siti su un unico listener. Se la catena è per `name` viene controllato qui, e in caso contrario questa chiamata restituisce `Err`. I client che chiedono un nome che nessuno ha aggiunto, o nessun nome, ricevono il certificato di `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Il soggetto del certificato della controparte (`CN=client,O=Example,C=JP`, forma RFC 4514, prima il più specifico). Il modo in cui un server TLS reciproco scopre "chi si è connesso" (dopo la prima lettura, che completa l'handshake). Sul lato client, il nome del certificato del server. `none` per le connessioni in chiaro, prima dell'handshake, o se la controparte non ha presentato alcun certificato |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Su una connessione TLS lato server, il nome richiesto dal client (SNI). Il modo in cui un server con più siti aggiunti da `tls-add-certificate` scopre "quale sito". `none` per le connessioni in chiaro, sul lato client, o senza nome (connesso tramite indirizzo) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Disattiva Nagle (`TCP_NODELAY`). `write-string` arriva ogni volta fino al socket, quindi con Nagle attivo una risposta scritta in due parti, intestazione e corpo, attende l'ACK ritardato della controparte: usa `true` per i protocolli richiesta/risposta. I socket di dominio Unix non hanno Nagle, e semplicemente riesce |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sonda la controparte quando è inattiva (`SO_KEEPALIVE`). Rileva una controparte sparita senza chiudere (un cavo staccato, un host fermo) e reimposta la connessione. Il periodo predefinito del sistema operativo è lungo (spesso 2 ore), quindi abbinalo a `set-keepalive-period` qui sotto. I socket di dominio Unix non lo hanno, quindi va in panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | I secondi di inattività prima della prima sonda e l'intervallo tra le sonde (secondi interi, almeno 1). Come `SetKeepAlivePeriod` di Go, entrambi vengono impostati allo stesso valore |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Se **la controparte** ha interrotto questa connessione, il suo primo fallimento (un reset, un alert TLS, una disconnessione durante una scrittura). `none` se è sana: un EOF dovuto alla chiusura pulita della controparte non è un fallimento. Si veda "Fallimenti della controparte" più sotto |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | L'`host:port` del nostro lato. Il modo di conoscere la porta scelta dopo `(tcp-listen h 0)`. Per i socket Unix, il percorso (l'estremità che si connette è `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | L'`host:port` della controparte, o il percorso per i socket Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Chiude solo il lato di invio (una half-close). La controparte legge EOF, e questo lato può ancora leggere. Il segnale per "ho inviato l'intera richiesta". Per TLS invia anche `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | La versione a byte della stessa connessione |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | La versione a caratteri della stessa connessione |
| `close` | `(close s)` | `Stream` | Invia il buffer, poi chiude. Il GC non la chiude |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Connette, esegue il corpo, chiude. `Result<valore del corpo, NetError>` (la stessa forma di `with-open-file`) |

`write-string`/`write-line` **ritornano dopo che tutto è stato scritto** (come `net.Conn.Write` di Go). Per
raggruppare piccole scritture, accumulale in uno `string-output-stream` e scrivi una volta sola. `listen` è
`true` solo quando c'è qualcosa nel buffer di ricezione, quindi `read-char-no-hang` funziona come dice il
suo nome.

**I fallimenti della controparte non producono panic.** Se l'altra estremità reimposta la connessione,
rifiuta l'handshake TLS o la interrompe nel mezzo di una scrittura, non è un errore di questo programma,
quindi il server non si ferma insieme ai suoi altri client. Il primo fallimento viene registrato sulla
connessione; le letture successive restituiscono `none` (con lo stesso aspetto di un EOF), e le scritture
non vanno da nessuna parte e ritornano in silenzio. Per distinguerli, usa `socket-error`, della stessa
forma di `bufio.Scanner.Err` di Go (`read-item` è un `Option` e non ha altro canale su cui riferire).
Solo gli errori del programma stesso producono panic (un handle chiuso, la lettura di un byte subito dopo
`unread-char`).

**I timeout sono argomenti espliciti o `wait-readable`.** Non esiste una forma, come il `SetReadDeadline` di
Go, in cui uno stream porta con sé una scadenza e `read-line` fallisce: `read-item` restituisce
`Option<Item>` e non ha modo di restituire un errore. Serve un orologio in tre punti, la connessione,
l'accettazione e "la lettura successiva", e ciascuno ha un argomento per esso.

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

Con il TLS reciproco, un client che non presenta alcun certificato (o ne presenta uno che non passa)
termina il proprio lato dell'handshake prima che il server decida, sotto TLS 1.3, quindi `tls-connect`
restituisce `Ok` e **la prima lettura restituisce `none`** (con l'alert in `socket-error`). Sul lato
server, anche la prima lettura della stessa connessione restituisce `none`. Nessuno dei due lati va in
panic.

## 3. UDP

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Crea un socket. Serve anche solo per inviare (`port` è `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Invia un datagramma. I nomi vengono risolti. Se sia arrivato non è noto (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Il datagramma successivo. `from` è `ip:port` e può essere passato così com'è come `host` di `send-to` |

Converti tra stringhe e sequenze di byte con `string->utf8` / `utf8->string`
([Stringhe](collections.md#1-stringhe-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Che cosa non c'è

- Un modo di leggere qualsiasi cosa del certificato della controparte **diversa dal soggetto** (SAN,
  periodo di validità, emittente).
- **Attendere contemporaneamente su un socket e su un canale con `select`.** Come in Go, scrivilo come
  "avvia un task che legge e fagli alimentare un canale".
- **HTTP/2**. I **datagrammi** di dominio Unix (`SOCK_DGRAM`).
- **Scadenze portate da uno stream** (per i motivi sopra esposti, gli orologi sono argomenti).
