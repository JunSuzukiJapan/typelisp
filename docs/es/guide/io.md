<!-- translated-from: docs/ja/guide/io.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# E/S de archivos, streams y red

Esta guía muestra lo básico de leer y escribir archivos, los nombres de ruta y la comunicación por sockets.
Las listas de funciones están en [Streams y archivos](../reference/functions/streams-files.md) y
[Red](../reference/functions/network.md).

## 1. Los fallos vuelven como `Result`

Las operaciones que pueden fallar según el entorno, como abrir un archivo o conectar, devuelven un
`Result`. Que falte un archivo no es un error del programa, así que no provoca un `panic`. Separa los
resultados con `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; si falta el archivo:
;; error: config.txt: No such file or directory (os error 2)
```

Cuando sabes que una operación no puede fallar, o en un script pequeño en el que detenerse ante un fallo
no importa, `unwrap` saca el valor. Si es un `Err`, provoca un panic.

## 2. Leer y escribir un archivo entero

Las funciones más sencillas tratan el archivo entero de una vez.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Leer y escribir con streams

Para leer o escribir poco a poco, abre un stream con `with-open-file`. Se salga como se salga del cuerpo,
el stream se cierra. El valor es `Result<valor del cuerpo, FileError>`.

```lisp
;; escribir
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; leer línea a línea
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Hay tres direcciones para abrir un archivo: `direction-input` (lectura), `direction-output` (escritura;
  el contenido existente se descarta) y `direction-append` (añadir al final).
- `read-line` devuelve `none` al final del archivo.
- Si abres un archivo con `open-file` en lugar de `with-open-file`, llama siempre a `close`. El GC no
  cierra los streams.

Para leer y escribir bytes, abre el archivo con `open-binary-input` / `open-binary-output` y usa
`read-byte` / `write-byte`. Los streams de caracteres y los de bytes son tipos distintos, así que intentar
leer bytes de un stream de caracteres es un error de tipos.

### Usar una cadena como stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Entrada y salida estándar

`*standard-input*`, `*standard-output*` y `*error-output*` también son streams.
`(read-line *standard-input*)` lee una línea.

### Hacer genéricas las funciones de lectura y escritura

Cada clase de stream es un tipo propio, pero las operaciones comunes se reúnen en traits. Una función que
recibe su argumento con `(where (CharInput S))` puede leer de un archivo, de una cadena o de un socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Nombres de ruta

Las funciones que reciben un nombre de archivo aceptan tanto una cadena como un `pathname`. Usa un
`pathname` para dividir una ruta en partes o para construirla.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; sustituir solo la extensión
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; poner un nombre de archivo dentro de un directorio
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Operaciones con el sistema de archivos:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; lo crea junto con sus padres
(probe-file "out/deep")                           ; => true (existe)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => la lista del contenido
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

El separador es siempre `/`. No hay componentes de host, dispositivo ni versión como en los nombres de ruta
de Common Lisp, ni comodines ni nombres de ruta lógicos.

## 5. TCP

Una conexión de socket también es un stream, así que `read-line` y `write-line` funcionan sobre ella tal
cual.

### Servidor

El patrón básico es iniciar una tarea por conexión. `accept` y `read-line` detienen **solo esa tarea**
hasta que llegan datos, así que la atención a otras conexiones continúa.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; el otro lado cerró
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` acepta conexiones solo desde esta máquina, y `"0.0.0.0"` las acepta en todas las
interfaces. Para las tareas, consulta el [capítulo 12 de la Referencia de sintaxis](../reference/syntax.md#12-concurrencia-tareas).

### Cliente

`with-connection` conecta, ejecuta su cuerpo y cierra la conexión al final. El valor es
`Result<valor del cuerpo, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Para poner un límite de tiempo a una operación, pasa `:timeout` (en segundos).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; poner un reloj a la siguiente lectura
```

### Cuando el otro lado se desconecta

Si el otro lado reinicia la conexión o la corta a mitad, no se produce un `panic`. Las lecturas posteriores
devuelven `none` y las escrituras se descartan en silencio. Para saber si la conexión se cerró limpiamente
o falló, comprueba `(socket-error c)`.

## 6. Resolución de nombres (DNS)

No hay una función dedicada a la resolución de nombres. Pasar un nombre de host a `tcp-connect`,
`tls-connect` o `send-to` lo resuelve dentro de ellas. La resolución ocurre en otro hilo, así que las demás
tareas siguen en marcha mientras espera. Si un nombre tiene varias direcciones, se prueban por turno.

Si el nombre no se puede resolver, se devuelve un `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` se usa como `tcp-connect` y realiza el saludo TLS tras conectar. El certificado del servidor
se verifica contra los certificados raíz estándar. El resultado es un `socket-stream` normal, así que la
lectura y la escritura funcionan como con TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Un servidor TLS se crea pasando a `tls-listen` los archivos de certificado y de clave privada (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; el serve de TCP funciona tal cual
          ((err e) (println "accept: ~a" (message e))))))
```

Al probar con un certificado autofirmado, pasa `:ca-file "cert.pem"` en el lado del cliente para que confíe
en ese certificado. El TLS mutuo y servir varios sitios desde un mismo listener se tratan en
[Red](../reference/functions/network.md#2-tcp--tls--dominio-unix).

## 8. UDP

UDP no es un stream; envía y recibe un datagrama cada vez. Los datos son una secuencia de bytes
(`Vector<int>`); se convierten a cadenas y desde ellas con `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; incluso enviar necesita un socket; 0 deja el puerto al SO
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` es el `ip:puerto` del emisor, que se puede usar tal cual como destino de `send-to`.

## 9. Sockets de dominio Unix

Pasa la ruta del archivo de socket a `unix-listen` / `unix-connect`. El valor que obtienes es el mismo
`socket-stream` que con TCP. `unix-listen` devuelve un `Err` si el archivo ya existe. Cerrar el listener
elimina el archivo.
