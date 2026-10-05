<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Red (TCP / TLS / dominio Unix / UDP)

Los sockets forman parte de los [streams](streams-files.md). Una conexión se ve con **dos vistas de la misma
conexión**, `socket-stream` (caracteres) y `socket-byte-stream` (bytes), y `read-line`/`write-line`/
`read-byte`/`format` funcionan sobre ellas tal cual. Un listener es un `socket-listener`. **TCP, TLS y los
sockets de dominio Unix comparten un tipo** (como el `net.Conn` de Go): una vez conectados, leer y escribir
es igual, y solo cambia cómo se crean. UDP no es un stream sino datagramas (`udp-socket`).

**Lo que espera es la tarea, no el hilo.** `accept`, `read-line`, `write-string`, `tcp-connect` (incluida la
resolución de nombres) y `recv-from` detienen *esa tarea* si no están listos (como `sleep`/`recv`), y las
demás tareas siguen en marcha. Por eso un servidor se puede escribir con la misma forma que en Go,
`(task (serve c))` por conexión ([Referencia de sintaxis 12.5](../syntax.md#125-dónde-cambian-las-tareas)).

## 1. Tipos

| Tipo | Traits implementados | Cómo obtener uno |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | El `Err` de las funciones de abajo |

Un tipo de stream tiene un único tipo de elemento (por la misma razón que `file-stream`/
`binary-file-stream`), así que los caracteres y los bytes son tipos distintos. `byte-stream-of`/
`char-stream-of` devuelven valores que apuntan a **la misma conexión** y comparten el búfer de recepción: así
se escriben cosas como HTTP, donde las cabeceras se leen como caracteres y el cuerpo como bytes. Leer un byte
justo después de un `unread-char` de un carácter es un error (la misma regla que para los archivos).

## 2. TCP / TLS / dominio Unix

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Conecta. `host` puede ser un nombre o una dirección. Si un nombre tiene varias direcciones, se prueban por turno (`localhost` es `::1` y `127.0.0.1`). Una búsqueda de nombre fallida, una conexión rechazada o superar `:timeout` segundos dan `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS tras `tcp-connect`. El certificado se verifica contra el nombre `host` (`:server-name` si el nombre a verificar difiere de adonde conectas) con los certificados raíz de Mozilla. Con `:ca-file` (PEM), confía **solo en los certificados que contiene** (una CA privada, o el mismo certificado que presenta tu propio `tls-listen`). `:cert-file`/`:key-file` (ambos o ninguno) son nuestro certificado, que se presenta cuando el servidor lo pide (TLS mutuo). El saludo se completa aquí, así que si el certificado del servidor no pasa, esta llamada devuelve `Err`. El resultado es un `socket-stream` normal |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Conecta al socket de dominio Unix `path`. Es local, así que no hay espera de saludo ni tiempo de espera |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Escucha. `"127.0.0.1"` es solo esta máquina, `"0.0.0.0"` es todas las interfaces. Pasar `0` como `port` deja que el SO elija |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | La versión TLS de `tcp-listen`. `cert-file` es la cadena de certificados (PEM, el propio certificado primero) y `key-file` la clave privada. Ambos se leen y comprueban aquí, así que una clave que no corresponde da `Err` en esta llamada, no con el primer cliente. `accept` retorna **antes del saludo**, y la primera lectura o escritura de la tarea que atiende la conexión completa el saludo (como el `tls.Conn` de Go), así que un cliente lento en el saludo no frena otros `accept`. Con `:client-ca` (PEM), **exige a cada cliente** presentar un certificado emitido por una CA que contiene (TLS mutuo). Sin él, no se pide ninguno |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Escucha en `path`. **`Err` si el archivo ya existe** (podría pertenecer a otro proceso en marcha, así que no se sustituye en silencio). `close` elimina el archivo |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | La siguiente conexión. Detiene la tarea hasta que llega una. Desiste con `Err` tras `:timeout` segundos |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Hasta que se pueda leer sin esperar, o `secs` segundos. `true` significa lo primero (incluidos datos ya en el búfer). La forma de poner un reloj a una lectura: `(if (wait-readable c 5.0) (read-line c) ...)`. Lo que promete es que **la siguiente lectura no se detiene**; `read-line` puede esperar al resto de la línea |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Hasta que se pueda escribir, o `secs` segundos |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Añade un certificado más a un listener de `tls-listen`, que se presenta a los clientes que piden `name` (SNI): varios sitios en un listener. Aquí se comprueba si la cadena es para `name`; si no, esta llamada devuelve `Err`. Los clientes que piden un nombre que nadie añadió, o ningún nombre, reciben el certificado de `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | El sujeto del certificado del otro extremo (`CN=client,O=Example,C=JP`, forma RFC 4514, lo más específico primero). Cómo sabe un servidor con TLS mutuo "quién se conectó" (tras la primera lectura, que completa el saludo). En el lado del cliente, el nombre del certificado del servidor. `none` para conexiones en claro, antes del saludo, o si el otro extremo no presentó certificado |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | En una conexión TLS del lado del servidor, el nombre que pidió el cliente (SNI). Cómo sabe un servidor con varios sitios añadidos por `tls-add-certificate` "qué sitio". `none` para conexiones en claro, en el lado del cliente o sin nombre (conectado por dirección) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Desactiva Nagle (`TCP_NODELAY`). `write-string` llega hasta el socket cada vez, así que con Nagle activado, una respuesta escrita en dos partes, cabecera y cuerpo, espera al ACK retardado del otro extremo: usa `true` para protocolos de petición/respuesta. Los sockets de dominio Unix no tienen Nagle y simplemente tiene éxito |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sondea al otro extremo mientras está inactivo (`SO_KEEPALIVE`). Detecta un extremo que desapareció sin cerrar (un cable desenchufado, un host detenido) y reinicia la conexión. El periodo por defecto del SO es largo (a menudo 2 horas), así que combínalo con `set-keepalive-period` de abajo. Los sockets de dominio Unix no lo tienen, así que provoca un panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Los segundos de inactividad antes del primer sondeo y el intervalo entre sondeos (segundos enteros, al menos 1). Como el `SetKeepAlivePeriod` de Go, ambos se fijan al mismo valor |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Si **el otro extremo** rompió esta conexión, su primer fallo (un reinicio, una alerta TLS, una desconexión durante una escritura). `none` si está sana: un EOF porque el otro extremo cerró limpiamente no es un fallo. Consulta "Fallos del otro extremo" más abajo |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | El `host:port` de nuestro lado. La forma de saber el puerto elegido tras `(tcp-listen h 0)`. Para sockets Unix, la ruta (el extremo que conecta es `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | El `host:port` del otro extremo, o la ruta para sockets Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Cierra solo el lado de envío (un semicierre). El otro extremo lee EOF, y este lado aún puede leer. La señal de "he enviado toda la petición". En TLS también envía `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | La versión en bytes de la misma conexión |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | La versión en caracteres de la misma conexión |
| `close` | `(close s)` | `Stream` | Vacía el búfer y después cierra. El GC no la cierra |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Conectar, ejecutar el cuerpo, cerrar. `Result<valor del cuerpo, NetError>` (la misma forma que `with-open-file`) |

`write-string`/`write-line` **retornan cuando todo está escrito** (como el `net.Conn.Write` de Go). Para
agrupar escrituras pequeñas, acumúlalas en un `string-output-stream` y escribe una vez. `listen` es `true`
solo cuando hay algo en el búfer de recepción, así que `read-char-no-hang` hace lo que dice su nombre.

**Los fallos del otro extremo no provocan un panic.** Si el otro extremo reinicia la conexión, rechaza el
saludo TLS o la corta en medio de una escritura, no es un error de este programa, así que el servidor no se
detiene junto con sus otros clientes. El primer fallo se registra en la conexión; las lecturas posteriores
devuelven `none` (igual que un EOF), y las escrituras no van a ninguna parte y retornan en silencio. Para
distinguirlos, usa `socket-error`, con la misma forma que el `bufio.Scanner.Err` de Go (`read-item` es un
`Option` y no tiene otro canal por el que informar). Solo provocan un panic los errores del propio programa
(un manejador cerrado, leer un byte justo después de `unread-char`).

**Los tiempos de espera son argumentos explícitos o `wait-readable`.** No hay una forma, como el
`SetReadDeadline` de Go, en la que un stream lleve un plazo y `read-line` falle: `read-item` devuelve
`Option<Item>` y no tiene forma de devolver un error. Hace falta un reloj en tres sitios, al conectar, al
aceptar y en "la siguiente lectura", y cada uno tiene su argumento.

```lisp
;; servidor: una tarea por conexión
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; cliente
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

;; servidor TLS (el certificado se puede crear, por ejemplo, con
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; el serve de arriba tal cual; el primer read-line es el saludo
          ((err e) (println "accept: ~a" (message e))))))
;; su cliente: conectar confiando en su propio certificado
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; TLS mutuo: el servidor exige un certificado de cliente emitido por ca.pem, y el cliente lo presenta
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; en el lado del servidor, tras el primer read-line: quién se conectó
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; dos sitios en un listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; en la conexión, (requested-server-name c) dice cuál
```

Con TLS mutuo, un cliente que no presenta certificado (o uno que no pasa) termina su lado del saludo antes
de que el servidor decida, con TLS 1.3, así que `tls-connect` devuelve `Ok` y **la primera lectura devuelve
`none`** (con la alerta en `socket-error`). En el lado del servidor, la primera lectura de la misma conexión
también devuelve `none`. Ninguno de los dos lados provoca un panic.

## 3. UDP

| Nombre | Uso | Tipo | Significado |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Crea un socket. Hace falta incluso solo para enviar (`port` es `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Envía un datagrama. Los nombres se resuelven. No se sabe si llegó (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | El siguiente datagrama. `from` es `ip:port` y se puede pasar tal cual como el `host` de `send-to` |

Convierte entre cadenas y secuencias de bytes con `string->utf8` / `utf8->string`
([Cadenas](collections.md#1-cadenas-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Lo que no hay

- Una forma de leer del certificado del otro extremo **algo distinto del sujeto** (SAN, periodo de
  validez, emisor).
- **Esperar a la vez un socket y un canal con `select`.** Como en Go, escríbelo como "iniciar una tarea que
  lee y hacer que alimente un canal".
- **HTTP/2**. **Datagramas** de dominio Unix (`SOCK_DGRAM`).
- **Plazos asociados a un stream** (por las razones de arriba, los relojes son argumentos).
