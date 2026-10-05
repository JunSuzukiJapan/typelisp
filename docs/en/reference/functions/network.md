<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Networking (TCP / TLS / Unix Domain / UDP)

Sockets are members of the [streams](streams-files.md). A connection is seen in **two views of the same
connection**, `socket-stream` (characters) and `socket-byte-stream` (bytes), and
`read-line`/`write-line`/`read-byte`/`format` work on them as they are. A listener is a
`socket-listener`. **TCP, TLS and Unix domain sockets share one type** (like Go's `net.Conn`): once
connected, reading and writing are the same, and only how they are created differs. UDP is not a
stream but datagrams (`udp-socket`).

**What waits is the task, not the thread.** `accept`, `read-line`, `write-string`, `tcp-connect`
(including name resolution) and `recv-from` all stop *that task* if they are not ready (like
`sleep`/`recv`), and other tasks keep running. That is why a server can be written in the same shape as
in Go, `(task (serve c))` per connection ([Syntax Reference 12.5](../syntax.md#125-where-tasks-switch)).

## 1. Types

| Type | Implemented traits | How to get one |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | The `Err` of the functions below |

A stream type has one element type (for the same reason as `file-stream`/`binary-file-stream`), so
characters and bytes are different types. `byte-stream-of`/`char-stream-of` return values that point
to **the same connection** and share the receive buffer: this is how you write things like HTTP, where
the headers are read as characters and the body as bytes. Reading a byte right after `unread-char` of a
character is an error (the same rule as for files).

## 2. TCP / TLS / Unix domain

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Connects. `host` can be a name or an address. If a name has several addresses, they are tried in turn (`localhost` is `::1` and `127.0.0.1`). A failed name lookup, a refused connection, or exceeding `:timeout` seconds gives `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS after `tcp-connect`. The certificate is verified against the `host` name (`:server-name` if the name to verify differs from where you connect) with Mozilla's root certificates. Given `:ca-file` (PEM), it trusts **only the certificates in it** (a private CA, or the very certificate your own `tls-listen` presents). `:cert-file`/`:key-file` (both or neither) are our certificate, presented when the server asks for one (mutual TLS). The handshake completes here, so if the server's certificate does not pass, this call returns `Err`. The result is an ordinary `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Connects to the Unix domain socket `path`. It is local, so there is no handshake wait and no timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Listens. `"127.0.0.1"` is this machine only, `"0.0.0.0"` is every interface. Passing `0` as `port` lets the OS choose |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | The TLS version of `tcp-listen`. `cert-file` is the certificate chain (PEM, own certificate first), and `key-file` the private key. Both are read and checked here, so a key that does not match gives `Err` from this call, not at the first client. `accept` returns **before the handshake**, and the first read or write by the task handling the connection completes the handshake (like Go's `tls.Conn`), so a client slow to handshake does not hold up other `accept`s. Given `:client-ca` (PEM), it **requires every client** to present a certificate issued by a CA in it (mutual TLS). Without it, none is asked for |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Listens at `path`. **`Err` if the file already exists** (it might belong to another running process, so it is not silently replaced). `close` removes the file |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | The next connection. Stops the task until one comes. Gives up with `Err` after `:timeout` seconds |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Until it can be read without waiting, or `secs` seconds. `true` means the former (including data already buffered). The way to put a clock on a read: `(if (wait-readable c 5.0) (read-line c) ...)`. What it promises is that **the next read does not stop**; `read-line` may wait for the rest of the line |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Until it can be written, or `secs` seconds |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Adds one more certificate to a `tls-listen` listener, presented to clients that ask for `name` (SNI): several sites on one listener. Whether the chain is for `name` is checked here, and if not, this call returns `Err`. Clients asking for a name no one added, or for no name, get the certificate of `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | The subject of the peer's certificate (`CN=client,O=Example,C=JP`, RFC 4514 form, most specific first). How a mutual-TLS server learns "who connected" (after the first read, which completes the handshake). On the client side, the name of the server certificate. `none` for plain connections, before the handshake, or if the peer presented no certificate |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | On a server-side TLS connection, the name the client asked for (SNI). How a server with several sites added by `tls-add-certificate` learns "which site". `none` for plain connections, on the client side, or with no name (connected by address) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Turns off Nagle (`TCP_NODELAY`). `write-string` goes all the way to the socket every time, so with Nagle on, a response written in two parts, header and body, waits for the peer's delayed ACK: use `true` for request/response protocols. Unix domain sockets have no Nagle, and it simply succeeds |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Probes the peer while idle (`SO_KEEPALIVE`). Detects a peer that vanished without closing (a pulled cable, a halted host) and resets the connection. The OS default period is long (often 2 hours), so pair it with `set-keepalive-period` below. Unix domain sockets do not have it, so it panics |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | The idle seconds before the first probe and the interval between probes (whole seconds, at least 1). Like Go's `SetKeepAlivePeriod`, both are set to the same value |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | If **the peer** broke this connection, its first failure (a reset, a TLS alert, a disconnect during a write). `none` if healthy: an EOF from the peer closing cleanly is not a failure. See "Peer failures" below |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | Our side's `host:port`. The way to learn the chosen port after `(tcp-listen h 0)`. For Unix sockets, the path (the connecting end is `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | The peer's `host:port`, or the path for Unix sockets |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Closes only the sending side (a half-close). The peer reads EOF, and this side can still read. The signal for "I have sent the whole request". For TLS it also sends `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | The byte version of the same connection |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | The character version of the same connection |
| `close` | `(close s)` | `Stream` | Sends out the buffer, then closes. The GC does not close it |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Connect, run the body, close. `Result<value of the body, NetError>` (the same shape as `with-open-file`) |

`write-string`/`write-line` **return after everything is written** (like Go's `net.Conn.Write`). To
bundle small writes, accumulate them in a `string-output-stream` and write once. `listen` is `true` only
when something is in the receive buffer, so `read-char-no-hang` works as its name says.

**Peer failures do not panic.** If the other end resets the connection, rejects the TLS handshake, or
drops it in the middle of a write, that is not a mistake in this program, so the server does not stop
along with its other clients. The first failure is recorded on the connection; later reads return
`none` (looking the same as EOF), and writes go nowhere and return silently. To tell them apart, use
`socket-error`, the same shape as Go's `bufio.Scanner.Err` (`read-item` is an `Option` and has no
other channel to report through). Only mistakes of the program itself panic (a closed handle, reading
a byte right after `unread-char`).

**Timeouts are explicit arguments or `wait-readable`.** There is no form, like Go's `SetReadDeadline`,
where a stream carries a deadline and `read-line` fails: `read-item` returns `Option<Item>` and has no
way to return an error. A clock is needed in three places, connecting, accepting and "the next read",
and each has an argument for it.

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

With mutual TLS, a client that presents no certificate (or one that does not pass) finishes its own
side of the handshake before the server decides, under TLS 1.3, so `tls-connect` returns `Ok` and **the
first read returns `none`** (with the alert in `socket-error`). On the server side, the first read of
the same connection returns `none` too. Neither side panics.

## 3. UDP

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Creates a socket. Needed even just to send (`port` is `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Sends one datagram. Names are resolved. Whether it arrived is unknown (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | The next datagram. `from` is `ip:port` and can be passed as it is as the `host` of `send-to` |

Convert between strings and byte sequences with `string->utf8` / `utf8->string`
([Strings](collections.md#1-strings-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. What is not there

- A way to read anything of the peer's certificate **other than the subject** (SAN, validity period,
  issuer).
- **Waiting on a socket and a channel at once with `select`.** As in Go, write it as "start a task that
  reads, and have it feed a channel".
- **HTTP/2**. Unix domain **datagrams** (`SOCK_DGRAM`).
- **Deadlines carried by a stream** (for the reasons above, clocks are arguments).
