<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Datei-E/A, Streams und Netzwerk

Dieser Leitfaden zeigt die Grundlagen des Lesens und Schreibens von Dateien, der Pfadnamen und der
Socket-Kommunikation. Die Funktionslisten stehen unter [Streams und Dateien](../reference/functions/streams-files.md)
und [Netzwerk](../reference/functions/network.md).

## 1. Fehlschläge kommen als `Result` zurück

Operationen, die je nach Umgebung fehlschlagen können, etwa das Öffnen einer Datei oder das Verbinden, geben
ein `Result` zurück. Eine fehlende Datei ist kein Fehler im Programm, daher gibt es keinen `panic`. Die
Ergebnisse trennt man mit `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; wenn die Datei fehlt:
;; error: config.txt: No such file or directory (os error 2)
```

Wenn feststeht, dass eine Operation nicht fehlschlagen kann, oder in einem kleinen Skript, in dem ein Abbruch
beim Fehlschlag in Ordnung ist, holt `unwrap` den Wert heraus. Ist es ein `Err`, kommt es zu einem Panic.

## 2. Eine ganze Datei lesen und schreiben

Die einfachsten Funktionen behandeln die ganze Datei auf einmal.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. Mit Streams lesen und schreiben

Um stückweise zu lesen oder zu schreiben, öffnet man mit `with-open-file` einen Stream. Wie auch immer der
Rumpf verlassen wird, der Stream wird geschlossen. Der Wert ist `Result<Wert des Rumpfes, FileError>`.

```lisp
;; schreiben
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; zeilenweise lesen
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Es gibt drei Richtungen, eine Datei zu öffnen: `direction-input` (lesen), `direction-output` (schreiben;
  vorhandener Inhalt wird verworfen) und `direction-append` (ans Ende anhängen).
- `read-line` gibt am Dateiende `none` zurück.
- Wer eine Datei mit `open-file` statt `with-open-file` öffnet, ruft immer `close` auf. Der GC schließt keine
  Streams.

Um Bytes zu lesen und zu schreiben, öffnet man die Datei mit `open-binary-input` / `open-binary-output` und
verwendet `read-byte` / `write-byte`. Zeichen-Streams und Byte-Streams sind verschiedene Typen, daher ist der
Versuch, Bytes aus einem Zeichen-Stream zu lesen, ein Typfehler.

### Eine Zeichenkette als Stream verwenden

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standardein- und -ausgabe

Auch `*standard-input*`, `*standard-output*` und `*error-output*` sind Streams.
`(read-line *standard-input*)` liest eine Zeile.

### Lese- und Schreibfunktionen generisch machen

Jede Art von Stream ist ein eigener Typ, aber die gemeinsamen Operationen sind in Traits zusammengefasst. Eine
Funktion, die ihr Argument mit `(where (CharInput S))` entgegennimmt, kann aus einer Datei, einer Zeichenkette
oder einem Socket lesen.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pfadnamen

Funktionen, die einen Dateinamen nehmen, akzeptieren sowohl eine Zeichenkette als auch einen `pathname`. Um
einen Pfad in Teile zu zerlegen oder einen zu bauen, verwendet man einen `pathname`.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; nur die Endung ersetzen
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; einen Dateinamen in ein Verzeichnis setzen
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Operationen auf dem Dateisystem:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; samt Elternverzeichnissen anlegen
(probe-file "out/deep")                           ; => true (es existiert)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => die Liste des Inhalts
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Das Trennzeichen ist immer `/`. Es gibt keine Host-, Geräte- oder Versionskomponenten wie in den Pfadnamen von
Common Lisp, und weder Platzhalter noch logische Pfadnamen.

## 5. TCP

Auch eine Socket-Verbindung ist ein Stream, daher funktionieren `read-line` und `write-line` unverändert
darauf.

### Server

Das Grundmuster ist, pro Verbindung einen Task zu starten. `accept` und `read-line` halten **nur diesen Task**
an, bis Daten ankommen, daher läuft die Bearbeitung anderer Verbindungen weiter.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; die Gegenseite hat geschlossen
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` nimmt nur Verbindungen von diesem Rechner an, `"0.0.0.0"` nimmt sie auf allen Schnittstellen an.
Zu Tasks siehe [Kapitel 12 der Syntaxreferenz](../reference/syntax.md#12-nebenläufigkeit-tasks).

### Client

`with-connection` verbindet, führt seinen Rumpf aus und schließt die Verbindung am Ende. Der Wert ist
`Result<Wert des Rumpfes, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Um eine Operation zeitlich zu begrenzen, übergibt man `:timeout` (in Sekunden).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; eine Uhr an das nächste Lesen hängen
```

### Wenn die Gegenseite die Verbindung trennt

Setzt die Gegenseite die Verbindung zurück oder bricht sie mittendrin ab, gibt es keinen `panic`. Spätere
Lesevorgänge geben `none` zurück, und Schreibvorgänge werden stillschweigend verworfen. Um zu unterscheiden, ob
die Verbindung sauber geschlossen wurde oder fehlgeschlagen ist, prüft man `(socket-error c)`.

## 6. Namensauflösung (DNS)

Eine eigene Funktion für die Namensauflösung gibt es nicht. Übergibt man einen Hostnamen an `tcp-connect`,
`tls-connect` oder `send-to`, wird er darin aufgelöst. Die Auflösung geschieht auf einem anderen Thread, daher
laufen andere Tasks während des Wartens weiter. Hat ein Name mehrere Adressen, werden sie der Reihe nach
versucht.

Lässt sich der Name nicht auflösen, wird ein `Err` zurückgegeben.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` wird wie `tcp-connect` verwendet und führt nach dem Verbinden den TLS-Handshake durch. Das
Serverzertifikat wird gegen die Standard-Stammzertifikate geprüft. Das Ergebnis ist ein gewöhnlicher
`socket-stream`, daher funktionieren Lesen und Schreiben wie bei TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Einen TLS-Server erzeugt man, indem man `tls-listen` Zertifikats- und Schlüsseldatei (PEM) übergibt.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; das serve von TCP funktioniert unverändert
          ((err e) (println "accept: ~a" (message e))))))
```

Beim Ausprobieren mit einem selbstsignierten Zertifikat übergibt man auf der Client-Seite
`:ca-file "cert.pem"`, damit dieses Zertifikat als vertrauenswürdig gilt. Gegenseitiges TLS und das Bedienen
mehrerer Sites über einen Listener werden unter
[Netzwerk](../reference/functions/network.md#2-tcp--tls--unix-domain) behandelt.

## 8. UDP

UDP ist kein Stream; es sendet und empfängt jeweils ein Datagramm. Die Daten sind eine Bytefolge
(`Vector<int>`); umgewandelt von und zu Zeichenketten wird mit `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; auch zum Senden braucht es einen Socket; 0 überlässt den Port dem BS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` ist das `ip:port` des Absenders und lässt sich unverändert als Ziel von `send-to` verwenden.

## 9. Unix-Domain-Sockets

Man übergibt `unix-listen` / `unix-connect` den Pfad der Socket-Datei. Der erhaltene Wert ist derselbe
`socket-stream` wie bei TCP. `unix-listen` gibt ein `Err` zurück, wenn die Datei bereits existiert. Das
Schließen des Listeners entfernt die Datei.
