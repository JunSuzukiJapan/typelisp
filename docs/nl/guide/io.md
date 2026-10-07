<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Bestands-I/O, streams en netwerk

Deze handleiding toont de basis van het lezen en schrijven van bestanden, padnamen en
socketcommunicatie. De lijsten met functies staan in [Streams en bestanden](../reference/functions/streams-files.md)
en [Netwerk](../reference/functions/network.md).

## 1. Fouten komen terug als `Result`

Bewerkingen die afhankelijk van de omgeving kunnen mislukken, zoals een bestand openen of verbinding
maken, geven een `Result` terug. Een ontbrekend bestand is geen fout in het programma, dus er volgt
geen `panic`. Scheid de uitkomsten met `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

Wanneer je weet dat een bewerking niet kan mislukken, of in een klein script waarin stoppen bij een
fout prima is, haalt `unwrap` de waarde eruit. Is het een `Err`, dan geeft het een panic.

## 2. Een heel bestand lezen en schrijven

De eenvoudigste functies verwerken het hele bestand in één keer.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Lezen en schrijven met streams

Om beetje bij beetje te lezen of te schrijven, open je een stream met `with-open-file`. Hoe de body
ook wordt verlaten, de stream wordt gesloten. De waarde is `Result<waarde van de body, FileError>`.

```lisp
;; write
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; read one line at a time
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Er zijn drie richtingen om een bestand te openen: `direction-input` (lezen), `direction-output`
  (schrijven; bestaande inhoud wordt weggegooid) en `direction-append` (aan het einde toevoegen).
- `read-line` geeft aan het einde van het bestand `none` terug.
- Open je een bestand met `open-file` in plaats van `with-open-file`, roep dan altijd `close` aan. De
  GC sluit streams niet.

Om bytes te lezen en te schrijven, open je het bestand met `open-binary-input` /
`open-binary-output` en gebruik je `read-byte` / `write-byte`. Tekenstreams en bytestreams zijn
verschillende types, dus proberen bytes uit een tekenstream te lezen is een typefout.

### Een string als stream gebruiken

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standaardinvoer en standaarduitvoer

`*standard-input*`, `*standard-output*` en `*error-output*` zijn ook streams.
`(read-line *standard-input*)` leest één regel.

### Lees- en schrijffuncties generiek maken

Elk soort stream is een eigen type, maar de gedeelde bewerkingen zijn in traits samengebracht. Een
functie die haar argument met `(where (CharInput S))` neemt, kan uit een bestand, een string of een
socket lezen.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Padnamen

Functies die een bestandsnaam nemen accepteren een string of een `pathname`. Gebruik een `pathname`
om een pad in delen te splitsen of er een op te bouwen.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; replace just the extension
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; put a file name inside a directory
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Bewerkingen op het bestandssysteem:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Het scheidingsteken is altijd `/`. Er zijn geen host-, apparaat- of versiecomponenten zoals bij de
padnamen van Common Lisp, en geen jokertekens of logische padnamen.

## 5. TCP

Een socketverbinding is ook een stream, dus `read-line` en `write-line` werken er direct op.

### Server

Het basispatroon is om per verbinding één taak te starten. `accept` en `read-line` laten **alleen die
taak** stilstaan totdat er gegevens binnenkomen, dus de afhandeling van andere verbindingen gaat door.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; the other side closed
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` accepteert alleen verbindingen vanaf deze machine, en `"0.0.0.0"` accepteert ze op elke
interface. Voor taken zie [Syntaxreferentie hoofdstuk 12](../reference/syntax.md#12-gelijktijdigheid-taken).

### Client

`with-connection` maakt verbinding, voert zijn body uit en sluit de verbinding aan het einde. De
waarde is `Result<waarde van de body, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Om een bewerking een tijdslimiet te geven, geef je `:timeout` (in seconden) mee.

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### Wanneer de andere kant de verbinding verbreekt

Als de andere kant de verbinding reset of halverwege laat vallen, volgt er geen `panic`. Latere
leesbewerkingen geven `none` terug en schrijfbewerkingen worden stilzwijgend weggegooid. Om vast te
stellen of de verbinding netjes is gesloten of is mislukt, controleer je `(socket-error c)`.

## 6. Naamresolutie (DNS)

Er is geen speciale functie voor naamresolutie. Een hostnaam doorgeven aan `tcp-connect`,
`tls-connect` of `send-to` lost hem daarbinnen op. De resolutie gebeurt op een andere thread, dus
andere taken blijven draaien terwijl erop wordt gewacht. Heeft een naam meerdere adressen, dan worden
ze om de beurt geprobeerd.

Kan de naam niet worden opgelost, dan wordt een `Err` teruggegeven.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` wordt gebruikt zoals `tcp-connect` en voert na het verbinden de TLS-handshake uit. Het
servercertificaat wordt geverifieerd tegen de standaard rootcertificaten. Het resultaat is een
gewone `socket-stream`, dus lezen en schrijven werken zoals bij TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Een TLS-server maak je door certificaat- en privésleutelbestanden (PEM) aan `tls-listen` door te
geven.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

Als je het uitprobeert met een zelfondertekend certificaat, geef dan aan de clientkant
`:ca-file "cert.pem"` mee, zodat hij dat certificaat vertrouwt. Wederzijdse TLS en het bedienen van
meerdere sites vanaf één listener staan in
[Netwerk](../reference/functions/network.md#2-tcp--tls--unix-domain).

## 8. UDP

UDP is geen stream; het verzendt en ontvangt datagram voor datagram. De gegevens zijn een bytereeks
(`Vector<int>`); converteer van en naar strings met `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` is het `ip:port` van de afzender, dat zoals het is als bestemming van `send-to` kan worden
gebruikt.

## 9. Unix-domainsockets

Geef het pad van het socketbestand aan `unix-listen` / `unix-connect`. De waarde die je krijgt is
dezelfde `socket-stream` als bij TCP. `unix-listen` geeft een `Err` terug als het bestand al bestaat.
Het sluiten van de listener verwijdert het bestand.
