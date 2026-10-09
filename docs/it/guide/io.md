<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# I/O su file, stream e rete

Questa guida mostra le basi della lettura e scrittura di file, dei pathname e della comunicazione
tramite socket. Gli elenchi delle funzioni si trovano in [Stream e file](../reference/functions/streams-files.md) e
[Rete](../reference/functions/network.md).

## 1. I fallimenti tornano come `Result`

Le operazioni che possono fallire a seconda dell'ambiente, come aprire un file o connettersi,
restituiscono un `Result`. Un file mancante non è un errore del programma, quindi non produce `panic`.
Separa gli esiti con `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

Quando sai che un'operazione non può fallire, o in un piccolo script in cui va bene fermarsi in caso di
fallimento, `unwrap` estrae il valore. Se è un `Err`, va in panic.

## 2. Leggere e scrivere un intero file

Le funzioni più semplici trattano l'intero file in un colpo solo.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. Leggere e scrivere con gli stream

Per leggere o scrivere poco alla volta, apri uno stream con `with-open-file`. In qualunque modo si esca
dal corpo, lo stream viene chiuso. Il valore è `Result<valore del corpo, FileError>`.

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

- Ci sono tre direzioni per aprire un file: `direction-input` (lettura), `direction-output`
  (scrittura; il contenuto esistente viene scartato) e `direction-append` (aggiunta in fondo).
- `read-line` restituisce `none` alla fine del file.
- Se apri un file con `open-file` invece che con `with-open-file`, chiama sempre `close`. Il GC non
  chiude gli stream.

Per leggere e scrivere byte, apri il file con `open-binary-input` / `open-binary-output` e usa
`read-byte` / `write-byte`. Gli stream di caratteri e gli stream di byte sono tipi diversi, quindi
tentare di leggere byte da uno stream di caratteri è un errore di tipo.

### Usare una stringa come stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standard input e output

Anche `*standard-input*`, `*standard-output*` e `*error-output*` sono stream.
`(read-line *standard-input*)` legge una riga.

### Rendere generiche le funzioni di lettura e scrittura

Ogni tipo di stream è un tipo a sé, ma le operazioni comuni sono raccolte in trait. Una funzione che
prende il proprio argomento con `(where (CharInput S))` può leggere da un file, da una stringa o da un
socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathname

Le funzioni che accettano un nome di file accettano una stringa oppure un `pathname`. Usa un
`pathname` per dividere un percorso in parti o per costruirne uno.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; replace just the extension
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; put a file name inside a directory
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Operazioni sul file system:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Il separatore è sempre `/`. Non ci sono componenti di host, dispositivo o versione come nei pathname di
Common Lisp, né caratteri jolly o pathname logici.

## 5. TCP

Anche una connessione socket è uno stream, quindi `read-line` e `write-line` funzionano su di essa così
come sono.

### Server

Il modello di base è avviare un task per ogni connessione. `accept` e `read-line` fermano **solo quel
task** finché non arrivano dati, quindi la gestione delle altre connessioni prosegue.

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

`"127.0.0.1"` accetta connessioni solo da questa macchina, mentre `"0.0.0.0"` le accetta su ogni
interfaccia. Per i task, si veda il [capitolo 12 del Riferimento della sintassi](../reference/syntax.md#12-concorrenza-task).

### Client

`with-connection` si connette, esegue il proprio corpo e chiude la connessione alla fine. Il valore è
`Result<valore del corpo, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Per porre un limite di tempo a un'operazione, passa `:timeout` (in secondi).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### Quando l'altro lato si disconnette

Se l'altro lato reimposta la connessione o la interrompe a metà, non si produce `panic`. Le letture
successive restituiscono `none` e le scritture vengono scartate in silenzio. Per sapere se la
connessione è stata chiusa in modo pulito o è fallita, controlla `(socket-error c)`.

## 6. Risoluzione dei nomi (DNS)

Non esiste una funzione dedicata alla risoluzione dei nomi. Passando un nome di host a `tcp-connect`,
`tls-connect` o `send-to`, esso viene risolto al loro interno. La risoluzione avviene su un altro
thread, quindi gli altri task continuano a funzionare mentre si attende. Se un nome ha più indirizzi,
vengono provati a turno.

Se il nome non può essere risolto, viene restituito un `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` si usa come `tcp-connect` ed esegue l'handshake TLS dopo la connessione. Il certificato
del server viene verificato rispetto ai certificati radice standard. Il risultato è un normale
`socket-stream`, quindi la lettura e la scrittura funzionano come con TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Un server TLS si crea passando a `tls-listen` i file del certificato e della chiave privata (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

Quando provi con un certificato autofirmato, passa `:ca-file "cert.pem"` dal lato client in modo che si
fidi di quel certificato. Il TLS reciproco e il servire più siti da un unico listener sono trattati in
[Rete](../reference/functions/network.md#2-tcp--tls--unix-domain).

## 8. UDP

UDP non è uno stream; invia e riceve un datagramma alla volta. I dati sono una sequenza di byte
(`Vector<int>`); converti da e verso le stringhe con `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` è l'`ip:porta` del mittente, che può essere usato così com'è come destinazione di `send-to`.

## 9. Socket di dominio Unix

Passa il percorso del file del socket a `unix-listen` / `unix-connect`. Il valore ottenuto è lo stesso
`socket-stream` di TCP. `unix-listen` restituisce un `Err` se il file esiste già. Chiudere il listener
rimuove il file.
