<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Netzwerk (TCP / TLS / Unix-Domain / UDP)

Sockets gehören zu den [Streams](streams-files.md). Eine Verbindung wird in **zwei Sichten derselben
Verbindung** gesehen, `socket-stream` (Zeichen) und `socket-byte-stream` (Bytes), und
`read-line`/`write-line`/`read-byte`/`format` funktionieren unverändert darauf. Ein Listener ist ein
`socket-listener`. **TCP, TLS und Unix-Domain-Sockets teilen sich einen Typ** (wie Gos `net.Conn`): Ist die
Verbindung hergestellt, sind Lesen und Schreiben gleich, und nur die Art der Erzeugung unterscheidet sich. UDP
ist kein Stream, sondern Datagramme (`udp-socket`).

**Was wartet, ist der Task, nicht der Thread.** `accept`, `read-line`, `write-string`, `tcp-connect`
(einschließlich Namensauflösung) und `recv-from` halten alle *diesen Task* an, wenn sie nicht bereit sind (wie
`sleep`/`recv`), und andere Tasks laufen weiter. Deshalb lässt sich ein Server in derselben Form wie in Go
schreiben, `(task (serve c))` pro Verbindung ([Syntaxreferenz 12.5](../syntax.md#125-wo-tasks-wechseln)).

## 1. Typen

| Typ | Implementierte Traits | Wie man einen bekommt |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | Das `Err` der folgenden Funktionen |

Ein Stream-Typ hat einen Elementtyp (aus demselben Grund wie `file-stream`/`binary-file-stream`), daher sind
Zeichen und Bytes verschiedene Typen. `byte-stream-of`/`char-stream-of` geben Werte zurück, die auf **dieselbe
Verbindung** zeigen und sich den Empfangspuffer teilen: So schreibt man etwa HTTP, wo die Header als Zeichen
und der Rumpf als Bytes gelesen werden. Ein Byte direkt nach dem `unread-char` eines Zeichens zu lesen, ist ein
Fehler (dieselbe Regel wie bei Dateien).

## 2. TCP / TLS / Unix-Domain

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Verbindet. `host` kann ein Name oder eine Adresse sein. Hat ein Name mehrere Adressen, werden sie der Reihe nach versucht (`localhost` ist `::1` und `127.0.0.1`). Eine fehlgeschlagene Namensauflösung, eine abgelehnte Verbindung oder das Überschreiten von `:timeout` Sekunden ergibt `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS nach `tcp-connect`. Das Zertifikat wird gegen den Namen `host` (`:server-name`, wenn sich der zu prüfende Name vom Verbindungsziel unterscheidet) mit den Stammzertifikaten von Mozilla geprüft. Mit `:ca-file` (PEM) vertraut es **nur den darin enthaltenen Zertifikaten** (eine private CA oder genau das Zertifikat, das das eigene `tls-listen` vorzeigt). `:cert-file`/`:key-file` (beide oder keines) sind das eigene Zertifikat, das vorgezeigt wird, wenn der Server danach fragt (gegenseitiges TLS). Der Handshake wird hier abgeschlossen, daher gibt dieser Aufruf `Err` zurück, wenn das Zertifikat des Servers nicht besteht. Das Ergebnis ist ein gewöhnlicher `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Verbindet mit dem Unix-Domain-Socket `path`. Er ist lokal, daher gibt es kein Warten auf einen Handshake und kein Timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Lauscht. `"127.0.0.1"` ist nur dieser Rechner, `"0.0.0.0"` jede Schnittstelle. Übergibt man `0` als `port`, wählt das BS |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | Die TLS-Fassung von `tcp-listen`. `cert-file` ist die Zertifikatskette (PEM, eigenes Zertifikat zuerst) und `key-file` der private Schlüssel. Beide werden hier gelesen und geprüft, daher ergibt ein nicht passender Schlüssel ein `Err` aus diesem Aufruf, nicht erst beim ersten Client. `accept` kehrt **vor dem Handshake** zurück, und das erste Lesen oder Schreiben des Tasks, der die Verbindung bearbeitet, schließt den Handshake ab (wie Gos `tls.Conn`), daher hält ein Client mit langsamem Handshake andere `accept`s nicht auf. Mit `:client-ca` (PEM) **verlangt es von jedem Client** ein Zertifikat, das von einer darin enthaltenen CA ausgestellt ist (gegenseitiges TLS). Ohne wird keines verlangt |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Lauscht an `path`. **`Err`, wenn die Datei bereits existiert** (sie könnte zu einem anderen laufenden Prozess gehören, daher wird sie nicht stillschweigend ersetzt). `close` entfernt die Datei |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Die nächste Verbindung. Hält den Task an, bis eine kommt. Gibt nach `:timeout` Sekunden mit `Err` auf |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Bis ohne Warten gelesen werden kann, oder `secs` Sekunden. `true` bedeutet Ersteres (einschließlich bereits gepufferter Daten). Die Art, einem Lesevorgang eine Uhr anzuhängen: `(if (wait-readable c 5.0) (read-line c) ...)`. Versprochen wird, dass **das nächste Lesen nicht anhält**; `read-line` kann auf den Rest der Zeile warten |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Bis geschrieben werden kann, oder `secs` Sekunden |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Fügt einem `tls-listen`-Listener ein weiteres Zertifikat hinzu, das Clients vorgezeigt wird, die nach `name` fragen (SNI): mehrere Sites auf einem Listener. Ob die Kette für `name` gilt, wird hier geprüft, und wenn nicht, gibt dieser Aufruf `Err` zurück. Clients, die nach einem Namen fragen, den niemand hinzugefügt hat, oder nach keinem Namen, bekommen das Zertifikat von `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Der Subject-Eintrag des Zertifikats der Gegenseite (`CN=client,O=Example,C=JP`, Form nach RFC 4514, das Spezifischste zuerst). So erfährt ein Server mit gegenseitigem TLS, „wer sich verbunden hat“ (nach dem ersten Lesen, das den Handshake abschließt). Auf der Client-Seite der Name des Serverzertifikats. `none` bei einfachen Verbindungen, vor dem Handshake oder wenn die Gegenseite kein Zertifikat vorgezeigt hat |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Auf einer serverseitigen TLS-Verbindung der Name, nach dem der Client gefragt hat (SNI). So erfährt ein Server mit mehreren durch `tls-add-certificate` hinzugefügten Sites, „welche Site“. `none` bei einfachen Verbindungen, auf der Client-Seite oder ohne Namen (über eine Adresse verbunden) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Schaltet Nagle ab (`TCP_NODELAY`). `write-string` geht jedes Mal bis zum Socket durch, daher wartet bei eingeschaltetem Nagle eine in zwei Teilen geschriebene Antwort, Header und Rumpf, auf das verzögerte ACK der Gegenseite: Bei Anfrage/Antwort-Protokollen verwendet man `true`. Unix-Domain-Sockets haben kein Nagle, und es gelingt einfach |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Prüft die Gegenseite im Leerlauf (`SO_KEEPALIVE`). Erkennt eine Gegenseite, die verschwunden ist, ohne zu schließen (ein gezogenes Kabel, ein angehaltener Rechner), und setzt die Verbindung zurück. Die Standardperiode des BS ist lang (oft 2 Stunden), daher kombiniert man es mit `set-keepalive-period` unten. Unix-Domain-Sockets haben es nicht, daher Panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Die Leerlaufsekunden vor der ersten Prüfung und der Abstand zwischen den Prüfungen (ganze Sekunden, mindestens 1). Wie bei Gos `SetKeepAlivePeriod` werden beide auf denselben Wert gesetzt |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Hat **die Gegenseite** diese Verbindung zerstört, ihr erster Fehlschlag (ein Reset, ein TLS-Alert, eine Trennung während eines Schreibvorgangs). `none`, wenn alles in Ordnung ist: Ein EOF, weil die Gegenseite sauber geschlossen hat, ist kein Fehlschlag. Siehe „Fehlschläge der Gegenseite“ unten |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | Das `host:port` der eigenen Seite. Die Art, nach `(tcp-listen h 0)` den gewählten Port zu erfahren. Bei Unix-Sockets der Pfad (das verbindende Ende ist `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | Das `host:port` der Gegenseite oder bei Unix-Sockets der Pfad |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Schließt nur die Sendeseite (ein Halbschluss). Die Gegenseite liest EOF, und diese Seite kann weiterhin lesen. Das Signal für „ich habe die ganze Anfrage gesendet“. Bei TLS sendet es auch `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Die Byte-Fassung derselben Verbindung |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Die Zeichen-Fassung derselben Verbindung |
| `close` | `(close s)` | `Stream` | Schickt den Puffer hinaus und schließt dann. Der GC schließt nicht |
| `with-connection` | `(with-connection (var host port) body...)` | Makro | Verbinden, Rumpf ausführen, schließen. `Result<Wert des Rumpfes, NetError>` (dieselbe Form wie `with-open-file`) |

`write-string`/`write-line` **kehren zurück, nachdem alles geschrieben ist** (wie Gos `net.Conn.Write`). Um
kleine Schreibvorgänge zu bündeln, sammelt man sie in einem `string-output-stream` und schreibt einmal.
`listen` ist nur dann `true`, wenn etwas im Empfangspuffer ist, daher funktioniert `read-char-no-hang` so, wie
sein Name sagt.

**Fehlschläge der Gegenseite lösen keinen Panic aus.** Setzt das andere Ende die Verbindung zurück, lehnt es den
TLS-Handshake ab oder bricht es sie mitten in einem Schreibvorgang ab, ist das kein Fehler dieses Programms,
daher hält der Server nicht samt seinen anderen Clients an. Der erste Fehlschlag wird an der Verbindung
vermerkt; spätere Lesevorgänge geben `none` zurück (sieht aus wie EOF), und Schreibvorgänge gehen ins Leere
und kehren stillschweigend zurück. Um sie zu unterscheiden, verwendet man `socket-error`, dieselbe Form wie
Gos `bufio.Scanner.Err` (`read-item` ist ein `Option` und hat keinen anderen Weg, etwas zu melden). Nur Fehler
des Programms selbst lösen einen Panic aus (ein geschlossenes Handle, ein Byte direkt nach `unread-char`
lesen).

**Timeouts sind ausdrückliche Argumente oder `wait-readable`.** Es gibt keine Form wie Gos `SetReadDeadline`, bei
der ein Stream eine Frist trägt und `read-line` fehlschlägt: `read-item` gibt `Option<Item>` zurück und kann
keinen Fehler zurückgeben. Eine Uhr wird an drei Stellen gebraucht, beim Verbinden, beim Annehmen und beim
„nächsten Lesen“, und jede hat ein Argument dafür.

```lisp
;; Server: ein Task pro Verbindung
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; Client
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

;; TLS-Server (das Zertifikat lässt sich zum Beispiel erzeugen mit
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; das serve von oben unverändert; das erste read-line ist der Handshake
          ((err e) (println "accept: ~a" (message e))))))
;; sein Client: verbinden und dem eigenen Zertifikat vertrauen
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; gegenseitiges TLS: der Server verlangt ein von ca.pem ausgestelltes Client-Zertifikat, und der Client zeigt eines vor
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; auf der Server-Seite nach dem ersten read-line: wer sich verbunden hat
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; zwei Sites auf einem Listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; auf der Verbindung sagt (requested-server-name c), welche
```

Bei gegenseitigem TLS schließt ein Client, der kein Zertifikat vorzeigt (oder eines, das nicht besteht), unter
TLS 1.3 seine eigene Seite des Handshakes ab, bevor der Server entscheidet. Daher gibt `tls-connect` `Ok`
zurück, und **das erste Lesen gibt `none` zurück** (mit dem Alert in `socket-error`). Auf der Server-Seite gibt
das erste Lesen derselben Verbindung ebenfalls `none` zurück. Keine Seite löst einen Panic aus.

## 3. UDP

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Erzeugt einen Socket. Schon zum bloßen Senden nötig (`port` ist `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Sendet ein Datagramm. Namen werden aufgelöst. Ob es ankam, ist unbekannt (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Das nächste Datagramm. `from` ist `ip:port` und lässt sich unverändert als `host` von `send-to` übergeben |

Zwischen Zeichenketten und Bytefolgen wandelt man mit `string->utf8` / `utf8->string` um
([Zeichenketten](collections.md#1-zeichenketten-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Was es nicht gibt

- Eine Möglichkeit, aus dem Zertifikat der Gegenseite etwas **außer dem Subject** zu lesen (SAN,
  Gültigkeitszeitraum, Aussteller).
- **Mit `select` gleichzeitig auf einen Socket und einen Kanal warten.** Wie in Go schreibt man es als „einen
  lesenden Task starten und ihn einen Kanal füttern lassen“.
- **HTTP/2**. Unix-Domain-**Datagramme** (`SOCK_DGRAM`).
- **Von einem Stream getragene Fristen** (aus den oben genannten Gründen sind Uhren Argumente).
