<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Vào/ra tệp, stream và mạng

Tài liệu này trình bày những điều cơ bản về đọc và ghi tệp, pathname, và giao tiếp qua socket. Danh sách
các hàm nằm ở [Stream và tệp](../reference/functions/streams-files.md) và
[Mạng](../reference/functions/network.md).

## 1. Lỗi được trả về dưới dạng `Result`

Các thao tác có thể thất bại tùy môi trường, như mở tệp hay kết nối, trả về một `Result`. Một tệp bị
thiếu không phải là sai sót của chương trình, nên nó không `panic`. Hãy tách các kết quả bằng `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; nếu tệp bị thiếu:
;; error: config.txt: No such file or directory (os error 2)
```

Khi bạn biết một thao tác không thể thất bại, hoặc trong một script nhỏ mà dừng lại khi thất bại là
chấp nhận được, `unwrap` lấy giá trị ra. Nếu là `Err`, nó panic.

## 2. Đọc và ghi toàn bộ một tệp

Các hàm dễ dùng nhất xử lý cả tệp trong một lần.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. Đọc và ghi bằng stream

Để đọc hoặc ghi từng chút một, hãy mở một stream bằng `with-open-file`. Dù thân hàm được rời đi bằng
cách nào, stream đều được đóng. Giá trị là `Result<giá trị của thân, FileError>`.

```lisp
;; ghi
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; đọc từng dòng một
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Có ba hướng để mở một tệp: `direction-input` (đọc), `direction-output` (ghi; nội dung hiện có bị bỏ
  đi) và `direction-append` (nối vào cuối).
- `read-line` trả về `none` ở cuối tệp.
- Nếu bạn mở tệp bằng `open-file` thay vì `with-open-file`, hãy luôn gọi `close`. GC không đóng
  stream.

Để đọc và ghi byte, hãy mở tệp bằng `open-binary-input` / `open-binary-output` và dùng `read-byte` /
`write-byte`. Stream ký tự và stream byte là các kiểu khác nhau, nên cố đọc byte từ một stream ký tự là
lỗi kiểu.

### Dùng chuỗi như một stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Đầu vào và đầu ra chuẩn

`*standard-input*`, `*standard-output*` và `*error-output*` cũng là stream.
`(read-line *standard-input*)` đọc một dòng.

### Làm cho các hàm đọc và ghi trở nên generic

Mỗi loại stream là một kiểu riêng, nhưng các thao tác chung được gom vào các trait. Một hàm nhận đối số
bằng `(where (CharInput S))` có thể đọc từ một tệp, một chuỗi hoặc một socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathname

Các hàm nhận tên tệp chấp nhận hoặc một chuỗi hoặc một `pathname`. Hãy dùng `pathname` để tách một
đường dẫn thành các phần hoặc để dựng một đường dẫn.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; chỉ thay phần mở rộng
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; đặt một tên tệp vào bên trong một thư mục
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Các thao tác trên hệ thống tệp:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; tạo nó cùng với các thư mục cha
(probe-file "out/deep")                           ; => true (nó tồn tại)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => danh sách nội dung
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Dấu phân cách luôn là `/`. Không có các thành phần host, device hay version như trong pathname của
Common Lisp, và không có ký tự đại diện hay pathname logic.

## 5. TCP

Một kết nối socket cũng là một stream, nên `read-line` và `write-line` dùng được trên nó nguyên trạng.

### Server

Mẫu cơ bản là khởi động một task cho mỗi kết nối. `accept` và `read-line` dừng **chỉ task đó** cho đến
khi có dữ liệu, nên việc xử lý các kết nối khác vẫn tiếp tục.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; phía bên kia đã đóng
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` chỉ chấp nhận kết nối từ chính máy này, còn `"0.0.0.0"` chấp nhận trên mọi giao diện
mạng. Về task, xem [Tham chiếu cú pháp chương 12](../reference/syntax.md#12-lập-trình-đồng-thời-task).

### Client

`with-connection` kết nối, chạy thân hàm của nó và đóng kết nối ở cuối. Giá trị là
`Result<giá trị của thân, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Để đặt giới hạn thời gian cho một thao tác, hãy truyền `:timeout` (tính bằng giây).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; đặt đồng hồ cho lần đọc tiếp theo
```

### Khi phía bên kia ngắt kết nối

Nếu phía bên kia đặt lại kết nối hoặc làm rơi nó giữa chừng, nó không `panic`. Các lần đọc sau đó trả về
`none`, và các lần ghi bị bỏ qua một cách im lặng. Để biết kết nối được đóng gọn gàng hay thất bại,
hãy kiểm tra `(socket-error c)`.

## 6. Phân giải tên (DNS)

Không có hàm riêng để phân giải tên. Truyền một tên host cho `tcp-connect`, `tls-connect` hoặc
`send-to` thì việc phân giải diễn ra bên trong chúng. Việc phân giải diễn ra trên một thread khác, nên
các task khác vẫn chạy trong lúc chờ. Nếu một tên có nhiều địa chỉ, chúng được thử lần lượt.

Nếu không phân giải được tên, một `Err` được trả về.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` được dùng giống `tcp-connect` và thực hiện bắt tay TLS sau khi kết nối. Chứng chỉ của
server được xác minh theo các chứng chỉ gốc chuẩn. Kết quả là một `socket-stream` thông thường, nên
đọc và ghi hoạt động như với TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Một server TLS được tạo bằng cách truyền các tệp chứng chỉ và khóa riêng (PEM) cho `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; serve của TCP dùng nguyên trạng
          ((err e) (println "accept: ~a" (message e))))))
```

Khi thử nghiệm với chứng chỉ tự ký, hãy truyền `:ca-file "cert.pem"` ở phía client để nó tin cậy chứng
chỉ đó. TLS hai chiều (mutual TLS), và việc phục vụ nhiều site từ một listener, được trình bày trong
[Mạng](../reference/functions/network.md#2-tcp--tls--unix-domain).

## 8. UDP

UDP không phải là stream; nó gửi và nhận từng datagram một. Dữ liệu là một dãy byte (`Vector<int>`);
chuyển đổi qua lại với chuỗi bằng `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; gửi cũng cần một socket; 0 để HĐH chọn cổng
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` là `ip:port` của bên gửi, có thể dùng nguyên trạng làm đích của `send-to`.

## 9. Unix domain socket

Hãy truyền đường dẫn của tệp socket cho `unix-listen` / `unix-connect`. Giá trị bạn nhận được là cùng
`socket-stream` như với TCP. `unix-listen` trả về một `Err` nếu tệp đã tồn tại. Đóng listener sẽ xóa
tệp.
