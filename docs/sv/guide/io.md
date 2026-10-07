<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Fil-I/O, strömmar och nätverk

Den här guiden visar grunderna i att läsa och skriva filer, pathnames och socketkommunikation.
Listorna över funktioner finns i [Strömmar och filer](../reference/functions/streams-files.md) och
[Nätverk](../reference/functions/network.md).

## 1. Misslyckanden kommer tillbaka som `Result`

Operationer som kan misslyckas beroende på miljön, som att öppna en fil eller ansluta, returnerar ett
`Result`. En saknad fil är inget fel i programmet, så den ger inte `panic`. Skilj utfallen åt med `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; om filen saknas:
;; error: config.txt: No such file or directory (os error 2)
```

När du vet att en operation inte kan misslyckas, eller i ett litet skript där det är i sin ordning att
stoppa vid misslyckande, tar `unwrap` ut värdet. Om det är ett `Err` ger det panic.

## 2. Läsa och skriva en hel fil

De enklaste funktionerna hanterar hela filen på en gång.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Läsa och skriva med strömmar

För att läsa eller skriva lite i taget öppnar man en ström med `with-open-file`. Hur kroppen än lämnas
stängs strömmen. Värdet är `Result<kroppens värde, FileError>`.

```lisp
;; skriva
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; läsa en rad i taget
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Det finns tre riktningar för att öppna en fil: `direction-input` (läsa), `direction-output`
  (skriva; befintligt innehåll kastas) och `direction-append` (lägga till sist).
- `read-line` returnerar `none` vid filslut.
- Om du öppnar en fil med `open-file` i stället för `with-open-file`, anropa alltid `close`. GC stänger
  inte strömmar.

För att läsa och skriva byte öppnar man filen med `open-binary-input` / `open-binary-output` och
använder `read-byte` / `write-byte`. Teckenströmmar och byteströmmar är olika typer, så att försöka läsa
byte från en teckenström är ett typfel.

### Använda en sträng som ström

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standard in och ut

`*standard-input*`, `*standard-output*` och `*error-output*` är också strömmar.
`(read-line *standard-input*)` läser en rad.

### Göra läs- och skrivfunktioner generiska

Varje slags ström är en egen typ, men de gemensamma operationerna är samlade i traits. En funktion som
tar sitt argument med `(where (CharInput S))` kan läsa från en fil, en sträng eller en socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathnames

Funktioner som tar ett filnamn accepterar antingen en sträng eller ett `pathname`. Använd ett `pathname`
för att dela upp en sökväg i delar eller bygga en.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; byt bara ut filändelsen
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; lägg ett filnamn i en katalog
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Filsystemsoperationer:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; skapa den tillsammans med sina föräldrar
(probe-file "out/deep")                           ; => true (den finns)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => listan över innehållet
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Avgränsaren är alltid `/`. Det finns inga värd-, enhets- eller versionskomponenter som i Common Lisps
pathnames, och inga jokertecken eller logiska pathnames.

## 5. TCP

En socketanslutning är också en ström, så `read-line` och `write-line` fungerar på den som de är.

### Server

Det grundläggande mönstret är att starta en task per anslutning. `accept` och `read-line` stoppar
**bara den tasken** tills data kommer, så hanteringen av andra anslutningar fortsätter.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; andra sidan stängde
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` tar emot anslutningar bara från den här maskinen, och `"0.0.0.0"` tar emot dem på alla
gränssnitt. För tasks, se [Syntaxreferens kapitel 12](../reference/syntax.md#12-samtidighet-tasks).

### Klient

`with-connection` ansluter, kör sin kropp och stänger anslutningen på slutet. Värdet är
`Result<kroppens värde, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

För att sätta en tidsgräns på en operation skickar man `:timeout` (i sekunder).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; sätt en klocka på nästa läsning
```

### När andra sidan kopplar ner

Om andra sidan återställer anslutningen eller släpper den på vägen ger det inte `panic`. Senare läsningar
returnerar `none`, och skrivningar kastas tyst. För att avgöra om anslutningen stängdes rent eller
misslyckades, kontrollera `(socket-error c)`.

## 6. Namnuppslagning (DNS)

Det finns ingen dedikerad funktion för namnuppslagning. Att skicka ett värdnamn till `tcp-connect`,
`tls-connect` eller `send-to` löser upp det inuti dem. Uppslagningen sker på en annan tråd, så andra
tasks fortsätter köra medan den väntar. Om ett namn har flera adresser prövas de i tur och ordning.

Om namnet inte kan lösas upp returneras ett `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` används som `tcp-connect` och utför TLS-handskakningen efter anslutningen.
Servercertifikatet verifieras mot standardrotcertifikaten. Resultatet är en vanlig `socket-stream`, så
läsning och skrivning fungerar som med TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

En TLS-server skapas genom att skicka certifikat- och privatnyckelfiler (PEM) till `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; TCP-varianten av serve fungerar som den är
          ((err e) (println "accept: ~a" (message e))))))
```

När man prövar med ett självsignerat certifikat skickar man `:ca-file "cert.pem"` på klientsidan så att
den litar på det certifikatet. Ömsesidig TLS och att betjäna flera webbplatser från en lyssnare tas upp i
[Nätverk](../reference/functions/network.md#2-tcp--tls--unix-domän).

## 8. UDP

UDP är inte en ström; det skickar och tar emot ett datagram i taget. Datan är en bytesekvens
(`Vector<int>`); konvertera till och från strängar med `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; även för att skicka behövs en socket; 0 låter OS välja port
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` är avsändarens `ip:port`, som kan användas som den är som mål för `send-to`.

## 9. Unix-domänsocketar

Skicka sökvägen till socketfilen till `unix-listen` / `unix-connect`. Värdet du får är samma
`socket-stream` som med TCP. `unix-listen` returnerar ett `Err` om filen redan finns. Att stänga
lyssnaren tar bort filen.
