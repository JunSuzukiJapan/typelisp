<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 文件 I/O、流与网络

本指南说明文件读写、路径名以及套接字通信的基本写法。函数一览见[流与文件](../reference/functions/streams-files.md)
和[网络](../reference/functions/network.md)。

## 1. 失败以 `Result` 返回

打开文件、建立连接等取决于环境而可能失败的操作返回 `Result`。文件不存在不是程序的错误，所以不会 panic。用 `match`
区分结果。

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; 文件不存在时：
;; error: config.txt: No such file or directory (os error 2)
```

知道不会失败时，或者在失败了停止也无妨的小脚本中，可以用 `unwrap` 取出值。如果是 `Err` 则会 panic。

## 2. 一次读写整个文件

最简便的是一次处理整个文件的函数。

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. 用流读写

一点一点读写时，用 `with-open-file` 打开流。无论以何种方式退出主体，流都会被关闭。值是 `Result<主体的值, FileError>`。

```lisp
;; 写入
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; 逐行读取
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- 打开方向有三种：`direction-input`（读）、`direction-output`（写，原有内容会被清除）、`direction-append`（追加到末尾）。
- `read-line` 在文件末尾返回 `none`。
- 不用 `with-open-file` 而用 `open-file` 打开时，请务必调用 `close`。GC 不会关闭流。

按字节读写时，用 `open-binary-input` / `open-binary-output` 打开，并使用 `read-byte` / `write-byte`。字符流和
字节流是不同的类型，从字符流读取字节是类型错误。

### 把字符串当作流使用

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### 标准输入输出

`*standard-input*`、`*standard-output*`、`*error-output*` 也是流。用 `(read-line *standard-input*)` 读取一行。

### 让读写函数通用

流的类型各不相同，但共同的操作归纳在 trait 中。用 `(where (CharInput S))` 接收参数，函数就能从文件、字符串、套接字
中的任何一种读取。

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. 路径名

接受文件名的函数既接受字符串也接受 `pathname`。要把路径拆分成部分或组装路径时使用 `pathname`。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; 只替换扩展名
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; 把文件名放进目录中
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

文件系统操作：

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; 连同父目录一起创建
(probe-file "out/deep")                           ; => true（存在）
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => 内容一览
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

分隔符固定为 `/`。没有 Common Lisp 路径名中的主机、设备、版本成分，也没有通配符和逻辑路径名。

## 5. TCP

套接字连接也是流，所以 `read-line` 和 `write-line` 可以直接使用。

### 服务器

基本形式是每个连接启动一个任务。`accept` 和 `read-line` 在数据到来之前**只让该任务**停下，其他连接的处理会继续。

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; 对方关闭了
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` 只接受来自本机的连接，`"0.0.0.0"` 在所有网络接口上接受连接。关于任务，请参阅
[语法参考第 12 章](../reference/syntax.md#12-并发任务)。

### 客户端

`with-connection` 建立连接、执行主体，最后关闭。值是 `Result<主体的值, NetError>`。

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

要限制时间时，传入 `:timeout`（秒）。

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; 给下一次读取加上计时
```

### 对方断开连接时

即使连接的对方重置或中途断开，也不会 panic。之后的读取返回 `none`，写入会被静默丢弃。想区分是正常关闭还是失败时，
检查 `(socket-error c)`。

## 6. 名称解析（DNS）

没有专门用于名称解析的函数。把主机名传给 `tcp-connect`、`tls-connect`、`send-to`，就会在其中解析。解析在另一个
线程中进行，所以等待期间其他任务照常运行。一个名字有多个地址时会依次尝试连接。

无法解析时返回 `Err`。

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` 的用法与 `tcp-connect` 相同，在连接之后进行 TLS 握手。服务器证书用标准根证书验证。结果是普通的
`socket-stream`，读写与 TCP 相同。

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS 服务器通过把证书和私钥文件（PEM）传给 `tls-listen` 来创建。

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; 可以直接使用 TCP 的 serve
          ((err e) (println "accept: ~a" (message e))))))
```

用自签名证书试验时，在客户端传入 `:ca-file "cert.pem"` 以信任该证书。双向 TLS 以及用一个监听器服务多个站点的方法见
[网络](../reference/functions/network.md#2-tcp--tls--unix-域)。

## 8. UDP

UDP 不是流，而是一次收发一个数据报。数据是字节序列（`Vector<int>`），与字符串之间用 `string->utf8` /
`utf8->string` 转换。

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; 只发送也需要套接字。0 表示交给 OS 选择
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` 是发送方的 `ip:port`，可以直接用作 `send-to` 的目标。

## 9. Unix 域套接字

把套接字文件的路径传给 `unix-listen` / `unix-connect`。得到的值与 TCP 一样是 `socket-stream`。文件已存在时
`unix-listen` 返回 `Err`。关闭监听器时文件会被删除。
