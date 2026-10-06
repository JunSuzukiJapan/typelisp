<!-- translated-from: docs/ja/guide/io.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# File I/O, Streams and Networking

This guide shows the basics of reading and writing files, pathnames, and socket communication. The
lists of functions are in [Streams and Files](../reference/functions/streams-files.md) and
[Networking](../reference/functions/network.md).

## 1. Failures come back as `Result`

Operations that can fail depending on the environment, such as opening a file or connecting, return
a `Result`. A missing file is not a mistake in the program, so it does not `panic`. Separate the
outcomes with `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

When you know an operation cannot fail, or in a small script where stopping on failure is fine,
`unwrap` takes out the value. If it is an `Err`, it panics.

## 2. Reading and writing a whole file

The easiest functions handle the whole file at once.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Reading and writing with streams

To read or write a little at a time, open a stream with `with-open-file`. However the body is left,
the stream is closed. The value is `Result<value of the body, FileError>`.

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

- There are three directions to open a file: `direction-input` (read), `direction-output` (write;
  existing contents are discarded) and `direction-append` (append to the end).
- `read-line` returns `none` at the end of the file.
- If you open a file with `open-file` instead of `with-open-file`, always call `close`. The GC does
  not close streams.

To read and write bytes, open the file with `open-binary-input` / `open-binary-output` and use
`read-byte` / `write-byte`. Character streams and byte streams are different types, so trying to
read bytes from a character stream is a type error.

### Using a string as a stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standard input and output

`*standard-input*`, `*standard-output*` and `*error-output*` are streams too.
`(read-line *standard-input*)` reads one line.

### Making reading and writing functions generic

Each kind of stream is its own type, but the shared operations are gathered into traits. A function
that takes its argument with `(where (CharInput S))` can read from a file, a string or a socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathnames

Functions that take a file name accept either a string or a `pathname`. Use a `pathname` to split a
path into parts or to build one.

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

File system operations:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

The separator is always `/`. There are no host, device or version components as in Common Lisp
pathnames, and no wildcards or logical pathnames.

## 5. TCP

A socket connection is also a stream, so `read-line` and `write-line` work on it as they are.

### Server

The basic pattern is to start one task per connection. `accept` and `read-line` stop **only that
task** until data arrives, so the handling of other connections continues.

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

`"127.0.0.1"` accepts connections only from this machine, and `"0.0.0.0"` accepts them on every
interface. For tasks, see [Syntax Reference chapter 12](../reference/syntax.md#12-concurrency-tasks).

### Client

`with-connection` connects, runs its body and closes the connection at the end. The value is
`Result<value of the body, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

To put a time limit on an operation, pass `:timeout` (in seconds).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### When the other side disconnects

If the other side resets the connection or drops it midway, it does not `panic`. Later reads return
`none`, and writes are silently discarded. To tell whether the connection was closed cleanly or
failed, check `(socket-error c)`.

## 6. Name resolution (DNS)

There is no dedicated function for name resolution. Passing a host name to `tcp-connect`,
`tls-connect` or `send-to` resolves it inside them. Resolution happens on another thread, so other
tasks keep running while it waits. If a name has several addresses, they are tried in turn.

If the name cannot be resolved, an `Err` is returned.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` is used like `tcp-connect` and performs the TLS handshake after connecting. The server
certificate is verified against the standard root certificates. The result is an ordinary
`socket-stream`, so reading and writing work as with TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

A TLS server is created by passing certificate and private key files (PEM) to `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

When trying it out with a self-signed certificate, pass `:ca-file "cert.pem"` on the client side so
that it trusts that certificate. Mutual TLS, and serving several sites from one listener, are
covered in [Networking](../reference/functions/network.md#2-tcp--tls--unix-domain).

## 8. UDP

UDP is not a stream; it sends and receives one datagram at a time. The data is a byte sequence
(`Vector<int>`); convert to and from strings with `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` is the sender's `ip:port`, which can be used as it is as the destination of `send-to`.

## 9. Unix domain sockets

Pass the path of the socket file to `unix-listen` / `unix-connect`. The value you get is the same
`socket-stream` as with TCP. `unix-listen` returns an `Err` if the file already exists. Closing the
listener removes the file.
