<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 网络（TCP / TLS / Unix 域 / UDP）

套接字是[流](streams-files.md)的一员。连接有 `socket-stream`（字符）／`socket-byte-stream`（字节）这**同一连接的两种视图**，
`read-line`/`write-line`/`read-byte`/`format` 可以直接使用。监听器是 `socket-listener`。**TCP、TLS、Unix 域套接字共用一个
类型**（与 Go 的 `net.Conn` 相同）——连接之后的读写都一样，不同的只是创建方式。UDP 不是流，而是数据报（`udp-socket`）。

**等待的是任务而不是线程。** `accept`・`read-line`・`write-string`・`tcp-connect`（包括名称解析）・`recv-from`，如果尚未就绪，
都只让*该任务*停下（与 `sleep`/`recv` 相同），其他任务继续运行。所以可以写出与 Go 同样形式的服务器——每个连接一个
`(task (serve c))`（[语法参考 12.5](../syntax.md#125-切换发生的位置)）。

## 1. 类型

| 类型 | 实现的 trait | 获得方式 |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream`（`close` / `open-stream-p`） | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | ——（`bytes`：`Vector<int>`、`from`：`string`） | `recv-from` |
| `NetError` | `Error` | 下列函数的 `Err` |

流的元素类型每个类型只有一种（与 `file-stream`/`binary-file-stream` 理由相同），所以字符和字节是不同的类型。
`byte-stream-of`/`char-stream-of` 返回指向**同一连接**的值，接收缓冲区也是共享的——像 HTTP 那样用字符读头部、用字节读主体的
用途就这样写。`unread-char` 字符之后紧接着读字节是错误（与文件相同的规则）。

## 2. TCP / TLS / Unix 域

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | 连接。`host` 可以是名字也可以是地址。名字有多个地址时依次尝试（`localhost` 是 `::1` 和 `127.0.0.1`）。名称解析失败、连接被拒绝、超过 `:timeout` 秒都是 `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect` 之后进行 TLS。证书针对 `host` 名（连接目标与要验证的名字不同时用 `:server-name`）以 Mozilla 根证书验证——传入 `:ca-file`（PEM）时**只信任其中的证书**（私有 CA，或者自己的 `tls-listen` 给出的那张证书本身）。`:cert-file`/`:key-file`（两者都给或都不给）是服务器要求时出示的本方证书（双向 TLS）。握手在这里完成，所以服务器的证书通不过时这次调用返回 `Err`。结果是普通的 `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | 连接到 Unix 域套接字 `path`。是本地的，所以没有握手等待，也没有超时 |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | 监听。`"127.0.0.1"` 只限本机，`"0.0.0.0"` 是所有接口。`port` 传入 `0` 则由 OS 选择 |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen` 的 TLS 版本。`cert-file` 是证书链（PEM，自己的在最前），`key-file` 是私钥。两者都在这里读取并检查，所以密钥不匹配时不是在第一个客户端，而是这次调用返回 `Err`。`accept` 在**握手之前**返回，负责该连接的任务的第一次读写完成握手（与 Go 的 `tls.Conn` 相同）——握手慢的客户端不会阻碍其他 `accept`。传入 `:client-ca`（PEM）时，**要求所有客户端**出示由其中的 CA 签发的证书（双向 TLS）。没有时不要求 |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | 在 `path` 上监听。**文件已存在时为 `Err`**（可能属于正在运行的其他进程，所以不会悄悄替换）。`close` 删除文件 |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | 下一个连接。在连接到来之前让任务停下。`:timeout` 秒后放弃并返回 `Err` |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | 等到可以不等待地读取，或者 `secs` 秒。`true` 表示前者（包括已缓冲的情况）。给读取加计时的手段：`(if (wait-readable c 5.0) (read-line c) ...)`。它保证的是**下一次读取不会停下**，`read-line` 可能会等待该行的剩余部分 |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | 等到可以写，或者 `secs` 秒 |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | 给 `tls-listen` 的监听器再加一张证书，出示给请求 `name`（SNI）的客户端——一个监听器服务多个站点。证书链是否适用于 `name` 在这里检查，不适用时这次调用返回 `Err`。请求无人添加过的名字的客户端、不带名字的客户端得到 `tls-listen` 的证书 |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | 对方证书的 subject（`CN=client,O=Example,C=JP`，RFC 4514 形式，具体的在前）。双向 TLS 服务器了解"谁连接了"的手段（在完成握手的第一次读取之后）。在客户端一侧是服务器证书的名字。明文、握手之前、对方没有出示证书时为 `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | 服务器一侧的 TLS 连接中客户端请求的名字（SNI）。用 `tls-add-certificate` 拥有多个站点的服务器了解"是哪个站点"。明文、客户端一侧、没有名字（用地址连接）时为 `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | 关闭 Nagle（`TCP_NODELAY`）。`write-string` 每次都写到套接字，所以 Nagle 生效时，分两次写出头部和主体的响应会等待对方的延迟 ACK——请求／响应型协议请设为 `true`。Unix 域没有 Nagle，直接成功 |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | 在空闲时探测对方（`SO_KEEPALIVE`）。检测没有关闭就消失的对方（拔掉网线、主机停机）并重置连接。OS 默认周期很长（多为 2 小时），请与下面的 `set-keepalive-period` 一起使用。Unix 域没有它，所以会 panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | 第一次探测之前的空闲秒数以及探测间隔（整数秒，1 以上）。与 Go 的 `SetKeepAlivePeriod` 一样，两者设为同一个值 |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | 如果**对方**破坏了这个连接，返回其最初的失败（重置、TLS 告警、写入中断开）。健全时为 `none`——对方正常关闭的 EOF 不是失败。见下文"对方的失败" |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | 本方的 `host:port`。在 `(tcp-listen h 0)` 之后了解所选端口的手段。Unix 时为路径（连接一方的端点是 `(unnamed)`） |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | 对方的 `host:port`，Unix 时为路径 |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | 只关闭发送一侧（半关闭）。对方读到 EOF，本方还能读。"请求已全部发送"的信号。TLS 时还会发送 `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | 同一连接的字节版 |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | 同一连接的字符版 |
| `close` | `(close s)` | `Stream` | 把缓冲区全部送出后关闭。GC 不会关闭 |
| `with-connection` | `(with-connection (var host port) body...)` | 宏 | 连接→主体→关闭。`Result<主体的值, NetError>`（与 `with-open-file` 同形） |

`write-string`/`write-line` **写完之后才返回**（与 Go 的 `net.Conn.Write` 相同）。想把零碎的写入集中起来时，先积存在
`string-output-stream` 中再一次写出。`listen` 只在接收缓冲区中有东西时为 `true`——`read-char-no-hang` 名副其实地工作。

**对方的失败不会 panic。** 连接的另一端重置、拒绝 TLS 握手、在写入中途断开，这些都不是本程序的错误，所以服务器不会连同其他
客户端一起停止。最初的失败记录在连接上，之后的读取返回 `none`（与 EOF 一样的样子），写入无处可达、静默返回。想区分时用
`socket-error`——与 Go 的 `bufio.Scanner.Err` 同形（`read-item` 返回 `Option`，没有别的渠道可以传达）。会 panic 的只有程序
自身的错误（已关闭的句柄、`unread-char` 之后紧接着读字节）。

**超时用显式参数或 `wait-readable`。** 没有像 Go 的 `SetReadDeadline` 那样"让流带有期限、使 `read-line` 失败"的形式——
`read-item` 返回 `Option<Item>`，没有返回错误的渠道。需要计时的地方有连接、接受、"下一次读取"这 3 处，各自都有参数。

```lisp
;; 服务器：每个连接一个任务
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; 客户端
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

;; TLS 服务器（证书例如可以这样创建：
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem）
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; 直接使用上面的 serve。第一次 read-line 就是握手
          ((err e) (println "accept: ~a" (message e))))))
;; 它的客户端：信任自己的证书并连接
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; 双向 TLS：服务器要求由 ca.pem 签发的客户端证书，客户端出示它
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; 服务器一侧，第一次 read-line 之后：谁连接了
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; 一个监听器服务两个站点（SNI）
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; 在连接一侧 (requested-server-name c) 说明是哪个
```

在双向 TLS 中不出示证书（或证书通不过）的客户端，在 TLS 1.3 下客户端一侧的握手先于服务器的判定结束，所以 `tls-connect` 返回
`Ok`，**第一次读取得到 `none`**（告警记录在 `socket-error` 中）。服务器一侧同一连接的第一次读取也是 `none`。双方都不会 panic。

## 3. UDP

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | 创建套接字。只发送也需要（`port` 为 `0`） |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | 发送一个数据报。名字会被解析。是否送达无从得知（UDP） |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | 下一个数据报。`from` 是 `ip:port`，可以直接作为 `send-to` 的 `host` |

字符串与字节序列之间的转换用 `string->utf8` / `utf8->string`（[字符串](collections.md#1-字符串-string)）。

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. 没有的东西

- 读取对方证书中 **subject 以外**内容（SAN、有效期、签发者）的手段。
- **用 `select` 同时等待套接字和 channel** 的形式。与 Go 一样写成"启动一个读取的任务，让它把数据送进 channel"。
- **HTTP/2**。Unix 域的**数据报**（`SOCK_DGRAM`）。
- **让流带有的期限**（由于上述原因，计时用参数）。
