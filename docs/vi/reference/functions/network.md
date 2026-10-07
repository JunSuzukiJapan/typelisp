<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Mạng (TCP / TLS / Unix domain / UDP)

Socket là thành viên của các [stream](streams-files.md). Một kết nối được nhìn qua **hai góc nhìn của cùng
một kết nối**, `socket-stream` (ký tự) và `socket-byte-stream` (byte), và `read-line`/`write-line`/
`read-byte`/`format` hoạt động trên chúng nguyên trạng. Một listener là một `socket-listener`. **TCP, TLS và
Unix domain socket dùng chung một kiểu** (như `net.Conn` của Go): khi đã kết nối, việc đọc và ghi giống
nhau, chỉ cách tạo ra là khác. UDP không phải là stream mà là các datagram (`udp-socket`).

**Thứ chờ là task, không phải thread.** `accept`, `read-line`, `write-string`, `tcp-connect` (gồm cả phân giải
tên) và `recv-from` đều dừng *task đó* nếu chưa sẵn sàng (như `sleep`/`recv`), và các task khác tiếp tục
chạy. Đó là lý do một server có thể được viết cùng dạng như trong Go, `(task (serve c))` cho mỗi kết nối
([Tham chiếu cú pháp 12.5](../syntax.md#125-nơi-các-task-chuyển-đổi)).

## 1. Kiểu

| Kiểu | Các trait được triển khai | Cách lấy |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` của các hàm bên dưới |

Một kiểu stream có một kiểu phần tử (vì cùng lý do như `file-stream`/`binary-file-stream`), nên ký tự và
byte là các kiểu khác nhau. `byte-stream-of`/`char-stream-of` trả về các giá trị trỏ tới **cùng một kết
nối** và dùng chung bộ đệm nhận: đây là cách bạn viết những thứ như HTTP, nơi các header được đọc dưới
dạng ký tự và phần thân dưới dạng byte. Đọc một byte ngay sau `unread-char` của một ký tự là lỗi (cùng
quy tắc như với tệp).

## 2. TCP / TLS / Unix domain

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Kết nối. `host` có thể là một tên hoặc một địa chỉ. Nếu một tên có nhiều địa chỉ, chúng được thử lần lượt (`localhost` là `::1` và `127.0.0.1`). Phân giải tên thất bại, kết nối bị từ chối, hoặc vượt quá `:timeout` giây cho `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS sau `tcp-connect`. Chứng chỉ được xác minh theo tên `host` (`:server-name` nếu tên cần xác minh khác với nơi bạn kết nối) bằng các chứng chỉ gốc của Mozilla. Nếu có `:ca-file` (PEM), nó chỉ tin **các chứng chỉ trong đó** (một CA riêng, hoặc chính chứng chỉ mà `tls-listen` của bạn đưa ra). `:cert-file`/`:key-file` (cả hai hoặc không cái nào) là chứng chỉ của chúng ta, được đưa ra khi server yêu cầu (mutual TLS). Quá trình bắt tay hoàn tất ở đây, nên nếu chứng chỉ của server không đạt, lời gọi này trả về `Err`. Kết quả là một `socket-stream` thông thường |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Kết nối tới Unix domain socket `path`. Nó là cục bộ, nên không có chờ bắt tay và không có timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Lắng nghe. `"127.0.0.1"` chỉ là máy này, `"0.0.0.0"` là mọi giao diện mạng. Truyền `0` làm `port` để HĐH chọn |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | Phiên bản TLS của `tcp-listen`. `cert-file` là chuỗi chứng chỉ (PEM, chứng chỉ của chính mình đứng đầu), và `key-file` là khóa riêng. Cả hai được đọc và kiểm tra ở đây, nên một khóa không khớp cho `Err` từ lời gọi này, không phải ở client đầu tiên. `accept` trả về **trước khi bắt tay**, và lần đọc hoặc ghi đầu tiên của task xử lý kết nối hoàn tất bắt tay (như `tls.Conn` của Go), nên một client bắt tay chậm không giữ chân các `accept` khác. Nếu có `:client-ca` (PEM), nó **yêu cầu mọi client** đưa ra một chứng chỉ do một CA trong đó cấp (mutual TLS). Không có nó thì không yêu cầu |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Lắng nghe tại `path`. **`Err` nếu tệp đã tồn tại** (nó có thể thuộc về một tiến trình khác đang chạy, nên không bị thay thế một cách im lặng). `close` xóa tệp |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Kết nối tiếp theo. Dừng task cho đến khi có một kết nối đến. Bỏ cuộc với `Err` sau `:timeout` giây |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Cho đến khi có thể đọc mà không phải chờ, hoặc `secs` giây. `true` nghĩa là vế trước (gồm cả dữ liệu đã nằm sẵn trong bộ đệm). Cách đặt đồng hồ cho một lần đọc: `(if (wait-readable c 5.0) (read-line c) ...)`. Điều nó đảm bảo là **lần đọc tiếp theo không dừng**; `read-line` có thể chờ phần còn lại của dòng |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Cho đến khi có thể ghi, hoặc `secs` giây |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Thêm một chứng chỉ nữa vào listener của `tls-listen`, được đưa ra cho các client yêu cầu `name` (SNI): nhiều site trên một listener. Việc chuỗi có dành cho `name` hay không được kiểm tra ở đây, và nếu không, lời gọi này trả về `Err`. Các client yêu cầu một tên chưa ai thêm, hoặc không yêu cầu tên nào, nhận chứng chỉ của `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Subject của chứng chỉ phía đối tác (`CN=client,O=Example,C=JP`, dạng RFC 4514, cụ thể nhất đứng đầu). Cách một server mutual-TLS biết "ai đã kết nối" (sau lần đọc đầu tiên, vốn hoàn tất bắt tay). Phía client, là tên của chứng chỉ server. `none` với kết nối thường, trước khi bắt tay, hoặc nếu đối tác không đưa ra chứng chỉ |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Trên một kết nối TLS phía server, tên mà client đã yêu cầu (SNI). Cách một server có nhiều site thêm bằng `tls-add-certificate` biết "site nào". `none` với kết nối thường, phía client, hoặc khi không có tên (kết nối bằng địa chỉ) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Tắt Nagle (`TCP_NODELAY`). `write-string` đi hết tới socket mỗi lần, nên khi bật Nagle, một phản hồi ghi thành hai phần, header và thân, phải chờ ACK trì hoãn của đối tác: hãy dùng `true` cho các giao thức yêu cầu/phản hồi. Unix domain socket không có Nagle, và nó đơn giản là thành công |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Thăm dò đối tác khi nhàn rỗi (`SO_KEEPALIVE`). Phát hiện một đối tác biến mất mà không đóng (rút cáp, máy chủ ngừng hoạt động) và đặt lại kết nối. Chu kỳ mặc định của HĐH dài (thường là 2 giờ), nên hãy ghép nó với `set-keepalive-period` bên dưới. Unix domain socket không có tính năng này, nên nó panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Số giây nhàn rỗi trước lần thăm dò đầu tiên và khoảng cách giữa các lần thăm dò (số giây nguyên, ít nhất 1). Như `SetKeepAlivePeriod` của Go, cả hai được đặt cùng một giá trị |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Nếu **đối tác** đã làm hỏng kết nối này, là thất bại đầu tiên của nó (một lần reset, một cảnh báo TLS, một lần ngắt kết nối khi đang ghi). `none` nếu khỏe mạnh: EOF do đối tác đóng gọn gàng không phải là thất bại. Xem "Thất bại phía đối tác" bên dưới |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` phía chúng ta. Cách biết cổng đã chọn sau `(tcp-listen h 0)`. Với Unix socket, là đường dẫn (đầu kết nối là `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` của đối tác, hoặc đường dẫn với Unix socket |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Chỉ đóng phía gửi (đóng một nửa). Đối tác đọc được EOF, và phía này vẫn đọc được. Tín hiệu cho "tôi đã gửi xong toàn bộ yêu cầu". Với TLS nó cũng gửi `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Phiên bản byte của cùng kết nối |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Phiên bản ký tự của cùng kết nối |
| `close` | `(close s)` | `Stream` | Đẩy bộ đệm ra, rồi đóng. GC không đóng nó |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Kết nối, chạy thân, đóng. `Result<giá trị của thân, NetError>` (cùng dạng với `with-open-file`) |

`write-string`/`write-line` **trả về sau khi mọi thứ đã được ghi** (như `net.Conn.Write` của Go). Để gộp các
lần ghi nhỏ, hãy tích lũy chúng trong một `string-output-stream` và ghi một lần. `listen` chỉ là `true` khi
có gì đó trong bộ đệm nhận, nên `read-char-no-hang` hoạt động đúng như tên của nó.

**Thất bại phía đối tác không gây panic.** Nếu đầu bên kia đặt lại kết nối, từ chối bắt tay TLS, hoặc làm
rơi nó giữa lúc ghi, đó không phải sai sót của chương trình này, nên server không dừng cùng với các client
khác của nó. Thất bại đầu tiên được ghi lại trên kết nối; các lần đọc sau đó trả về `none` (trông giống
EOF), và các lần ghi không đi đâu và trả về một cách im lặng. Để phân biệt chúng, hãy dùng `socket-error`,
cùng dạng với `bufio.Scanner.Err` của Go (`read-item` là một `Option` và không có kênh nào khác để báo
cáo). Chỉ những sai sót của chính chương trình mới panic (một handle đã đóng, đọc một byte ngay sau
`unread-char`).

**Timeout là các đối số tường minh hoặc `wait-readable`.** Không có dạng, như `SetReadDeadline` của Go, nơi
một stream mang một thời hạn và `read-line` thất bại: `read-item` trả về `Option<Item>` và không có cách
nào trả về lỗi. Cần đồng hồ ở ba nơi, kết nối, chấp nhận và "lần đọc tiếp theo", và mỗi nơi có một đối số
cho nó.

```lisp
;; server: một task cho mỗi kết nối
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

;; server TLS (chứng chỉ có thể được tạo, ví dụ, bằng
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; serve ở trên nguyên trạng; read-line đầu tiên là lần bắt tay
          ((err e) (println "accept: ~a" (message e))))))
;; client của nó: kết nối và tin chứng chỉ của chính nó
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; mutual TLS: server yêu cầu một chứng chỉ client do ca.pem cấp, và client đưa ra một chứng chỉ
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; phía server, sau read-line đầu tiên: ai đã kết nối
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; hai site trên một listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; trên kết nối, (requested-server-name c) cho biết site nào
```

Với mutual TLS, một client không đưa ra chứng chỉ (hoặc đưa ra chứng chỉ không đạt) hoàn tất phía bắt tay của
chính nó trước khi server quyết định, theo TLS 1.3, nên `tls-connect` trả về `Ok` và **lần đọc đầu tiên trả về
`none`** (với cảnh báo nằm trong `socket-error`). Phía server, lần đọc đầu tiên của cùng kết nối cũng trả về
`none`. Cả hai phía đều không panic.

## 3. UDP

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Tạo một socket. Cần ngay cả khi chỉ để gửi (`port` là `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Gửi một datagram. Tên được phân giải. Không biết nó có đến nơi hay không (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Datagram tiếp theo. `from` là `ip:port` và có thể được truyền nguyên trạng làm `host` của `send-to` |

Chuyển đổi giữa chuỗi và dãy byte bằng `string->utf8` / `utf8->string`
([Chuỗi](collections.md#1-chuỗi-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Những gì không có

- Cách đọc bất cứ thứ gì của chứng chỉ phía đối tác **ngoài subject** (SAN, thời hạn hiệu lực, issuer).
- **Chờ trên một socket và một kênh cùng lúc bằng `select`.** Như trong Go, hãy viết thành "khởi động một
  task đọc, và để nó cấp dữ liệu cho một kênh".
- **HTTP/2**. **Datagram** của Unix domain (`SOCK_DGRAM`).
- **Thời hạn do một stream mang** (vì các lý do ở trên, đồng hồ là các đối số).
