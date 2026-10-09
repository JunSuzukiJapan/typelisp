<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# I/O ของไฟล์ สตรีม และเครือข่าย

คู่มือนี้แสดงพื้นฐานของการอ่านและเขียนไฟล์ pathname และการสื่อสารผ่านซ็อกเก็ต รายการ
ฟังก์ชันอยู่ใน[สตรีมและไฟล์](../reference/functions/streams-files.md) และ
[เครือข่าย](../reference/functions/network.md)

## 1. ความล้มเหลวคืนกลับมาเป็น `Result`

การดำเนินการที่อาจล้มเหลวตามสภาพแวดล้อม เช่น การเปิดไฟล์หรือการเชื่อมต่อ จะคืน
`Result` ไฟล์ที่หายไปไม่ใช่ความผิดพลาดของโปรแกรม จึงไม่ `panic` ให้แยกผลลัพธ์ด้วย
`match`

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

เมื่อรู้ว่าการดำเนินการไม่มีทางล้มเหลว หรือในสคริปต์เล็ก ๆ ที่หยุดเมื่อล้มเหลวได้
`unwrap` จะดึงค่าออกมา หากเป็น `Err` จะ panic

## 2. การอ่านและเขียนทั้งไฟล์

ฟังก์ชันที่ง่ายที่สุดจัดการทั้งไฟล์ในคราวเดียว

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. การอ่านและเขียนด้วยสตรีม

หากต้องการอ่านหรือเขียนทีละน้อย ให้เปิดสตรีมด้วย `with-open-file` ไม่ว่าจะออกจากตัวเนื้อหา
ด้วยวิธีใด สตรีมจะถูกปิด ค่าที่ได้คือ `Result<ค่าของตัวเนื้อหา, FileError>`

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

- ทิศทางการเปิดไฟล์มีสามแบบ: `direction-input` (อ่าน), `direction-output` (เขียน;
  เนื้อหาเดิมถูกทิ้ง) และ `direction-append` (ต่อท้าย)
- `read-line` คืน `none` เมื่อถึงจุดสิ้นสุดของไฟล์
- หากเปิดไฟล์ด้วย `open-file` แทน `with-open-file` ต้องเรียก `close` เสมอ GC
  ไม่ปิดสตรีมให้

หากต้องการอ่านและเขียนไบต์ ให้เปิดไฟล์ด้วย `open-binary-input` / `open-binary-output` และใช้
`read-byte` / `write-byte` สตรีมอักขระและสตรีมไบต์เป็นชนิดที่ต่างกัน ดังนั้นการพยายาม
อ่านไบต์จากสตรีมอักขระเป็นข้อผิดพลาดของชนิด

### การใช้สตริงเป็นสตรีม

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### อินพุตและเอาต์พุตมาตรฐาน

`*standard-input*`, `*standard-output*` และ `*error-output*` ก็เป็นสตรีมเช่นกัน
`(read-line *standard-input*)` อ่านหนึ่งบรรทัด

### ทำให้ฟังก์ชันอ่านและเขียนเป็น generic

สตรีมแต่ละแบบเป็นชนิดของตัวเอง แต่การดำเนินการที่ใช้ร่วมกันถูกรวมไว้ใน trait ฟังก์ชัน
ที่รับอาร์กิวเมนต์ด้วย `(where (CharInput S))` อ่านได้จากไฟล์ สตริง หรือซ็อกเก็ต

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathname

ฟังก์ชันที่รับชื่อไฟล์รับได้ทั้งสตริงหรือ `pathname` ใช้ `pathname` เมื่อต้องการแยก
พาธเป็นส่วน ๆ หรือสร้างพาธ

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

การดำเนินการกับระบบไฟล์:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

ตัวคั่นคือ `/` เสมอ ไม่มีส่วนประกอบ host, device หรือ version อย่างใน pathname ของ
Common Lisp และไม่มี wildcard หรือ logical pathname

## 5. TCP

การเชื่อมต่อซ็อกเก็ตก็เป็นสตรีมเช่นกัน ดังนั้น `read-line` และ `write-line` ใช้ได้ตามที่เป็น

### เซิร์ฟเวอร์

รูปแบบพื้นฐานคือเริ่มหนึ่ง task ต่อหนึ่งการเชื่อมต่อ `accept` และ `read-line` หยุด **เฉพาะ
task นั้น** จนกว่าข้อมูลจะมาถึง ดังนั้นการจัดการการเชื่อมต่ออื่นจึงดำเนินต่อไป

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

`"127.0.0.1"` รับการเชื่อมต่อจากเครื่องนี้เท่านั้น และ `"0.0.0.0"` รับการเชื่อมต่อบนทุก
อินเทอร์เฟซ สำหรับ task ดู[เอกสารอ้างอิงไวยากรณ์ บทที่ 12](../reference/syntax.md#12-การทำงานพร้อมกัน-task)

### ไคลเอนต์

`with-connection` เชื่อมต่อ รันตัวเนื้อหา และปิดการเชื่อมต่อตอนท้าย ค่าที่ได้คือ
`Result<ค่าของตัวเนื้อหา, NetError>`

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

หากต้องการจำกัดเวลาของการดำเนินการ ให้ส่ง `:timeout` (หน่วยเป็นวินาที)

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### เมื่อปลายทางอีกฝั่งตัดการเชื่อมต่อ

หากอีกฝั่งรีเซ็ตการเชื่อมต่อหรือตัดกลางคัน จะไม่ `panic` การอ่านต่อจากนั้นคืน
`none` และการเขียนถูกทิ้งโดยไม่แจ้ง หากต้องการแยกว่าการเชื่อมต่อถูกปิดอย่างเรียบร้อยหรือ
ล้มเหลว ให้ตรวจสอบ `(socket-error c)`

## 6. การแปลงชื่อ (DNS)

ไม่มีฟังก์ชันเฉพาะสำหรับการแปลงชื่อ การส่งชื่อโฮสต์ให้ `tcp-connect`,
`tls-connect` หรือ `send-to` จะแปลงชื่อภายในฟังก์ชันเหล่านั้น การแปลงทำงานบนอีกเธรดหนึ่ง ดังนั้น task
อื่นยังคงทำงานต่อขณะรอ หากชื่อหนึ่งมีหลายที่อยู่ จะลองทีละที่อยู่ตามลำดับ

หากแปลงชื่อไม่ได้ จะคืน `Err`

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` ใช้เหมือน `tcp-connect` และทำ TLS handshake หลังจากเชื่อมต่อ ใบรับรองของเซิร์ฟเวอร์
ถูกตรวจสอบกับใบรับรองรากมาตรฐาน ผลลัพธ์เป็น `socket-stream` ธรรมดา ดังนั้นการอ่านและเขียน
ทำงานเหมือนกับ TCP

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

เซิร์ฟเวอร์ TLS สร้างโดยส่งไฟล์ใบรับรองและกุญแจส่วนตัว (PEM) ให้ `tls-listen`

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

เมื่อทดลองด้วยใบรับรองที่ลงนามด้วยตนเอง ให้ส่ง `:ca-file "cert.pem"` ที่ฝั่งไคลเอนต์เพื่อ
ให้เชื่อถือใบรับรองนั้น Mutual TLS และการให้บริการหลายไซต์จากลิสเทนเนอร์เดียว
อยู่ใน[เครือข่าย](../reference/functions/network.md#2-tcp--tls--unix-domain)

## 8. UDP

UDP ไม่ใช่สตรีม แต่ส่งและรับทีละ datagram ข้อมูลเป็นลำดับไบต์
(`Vector<int>`) แปลงไปมากับสตริงด้วย `string->utf8` / `utf8->string`

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` คือ `ip:port` ของผู้ส่ง ซึ่งใช้เป็นปลายทางของ `send-to` ได้เลย

## 9. Unix domain socket

ส่งพาธของไฟล์ซ็อกเก็ตให้ `unix-listen` / `unix-connect` ค่าที่ได้เป็น
`socket-stream` ชนิดเดียวกับ TCP `unix-listen` คืน `Err` หากไฟล์มีอยู่แล้ว การปิด
ลิสเทนเนอร์จะลบไฟล์
