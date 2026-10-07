<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# เครือข่าย (TCP / TLS / Unix Domain / UDP)

ซ็อกเก็ตเป็นสมาชิกของ[สตรีม](streams-files.md) การเชื่อมต่อถูกมองใน **สองมุมมองของการเชื่อมต่อเดียวกัน**
คือ `socket-stream` (อักขระ) และ `socket-byte-stream` (ไบต์) และ
`read-line`/`write-line`/`read-byte`/`format` ใช้กับมันได้ทันที ลิสเทนเนอร์คือ
`socket-listener` **TCP, TLS และ Unix domain socket ใช้ชนิดเดียวกัน** (เหมือน `net.Conn` ของ Go): เมื่อ
เชื่อมต่อแล้ว การอ่านและเขียนเหมือนกัน และต่างกันเฉพาะวิธีสร้าง UDP ไม่ใช่
สตรีมแต่เป็น datagram (`udp-socket`)

**สิ่งที่รอคือ task ไม่ใช่เธรด** `accept`, `read-line`, `write-string`, `tcp-connect`
(รวมการแปลงชื่อ) และ `recv-from` ล้วนหยุด *task นั้น* หากยังไม่พร้อม (เหมือน
`sleep`/`recv`) และ task อื่นยังรันต่อ นั่นคือเหตุผลที่เซิร์ฟเวอร์เขียนในรูปแบบเดียวกับ
ใน Go ได้ คือ `(task (serve c))` ต่อหนึ่งการเชื่อมต่อ ([เอกสารอ้างอิงไวยากรณ์ 12.5](../syntax.md#125-จุดที่-task-สลับกัน))

## 1. ชนิด

| ชนิด | trait ที่ implement | วิธีได้มา |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` ของฟังก์ชันด้านล่าง |

ชนิดสตรีมมีชนิดสมาชิกเดียว (ด้วยเหตุผลเดียวกับ `file-stream`/`binary-file-stream`) ดังนั้น
อักขระและไบต์เป็นคนละชนิด `byte-stream-of`/`char-stream-of` คืนค่าที่ชี้ไปยัง
**การเชื่อมต่อเดียวกัน** และใช้บัฟเฟอร์รับร่วมกัน: นี่คือวิธีเขียนสิ่งอย่าง HTTP ที่
ส่วนหัวอ่านเป็นอักขระและตัวเนื้อหาอ่านเป็นไบต์ การอ่านไบต์ทันทีหลัง `unread-char` ของ
อักขระเป็นข้อผิดพลาด (กฎเดียวกับไฟล์)

## 2. TCP / TLS / Unix domain

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | เชื่อมต่อ `host` เป็นชื่อหรือที่อยู่ก็ได้ หากชื่อมีหลายที่อยู่ จะลองทีละที่อยู่ตามลำดับ (`localhost` คือ `::1` และ `127.0.0.1`) การแปลงชื่อล้มเหลว การเชื่อมต่อถูกปฏิเสธ หรือเกิน `:timeout` วินาที ให้ `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS หลัง `tcp-connect` ใบรับรองถูกตรวจสอบกับชื่อ `host` (`:server-name` หากชื่อที่ต้องตรวจสอบต่างจากที่ที่เชื่อมต่อ) ด้วยใบรับรองรากของ Mozilla เมื่อให้ `:ca-file` (PEM) จะเชื่อถือ **เฉพาะใบรับรองในนั้น** (CA ส่วนตัว หรือใบรับรองที่ `tls-listen` ของคุณเองแสดง) `:cert-file`/`:key-file` (ให้ทั้งคู่หรือไม่ให้เลย) คือใบรับรองของเรา แสดงเมื่อเซิร์ฟเวอร์ขอ (mutual TLS) handshake เสร็จที่นี่ ดังนั้นหากใบรับรองของเซิร์ฟเวอร์ไม่ผ่าน การเรียกนี้คืน `Err` ผลลัพธ์เป็น `socket-stream` ธรรมดา |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | เชื่อมต่อกับ Unix domain socket `path` เป็นการเชื่อมต่อในเครื่อง จึงไม่มีการรอ handshake และไม่มี timeout |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | ฟัง `"127.0.0.1"` คือเครื่องนี้เท่านั้น `"0.0.0.0"` คือทุกอินเทอร์เฟซ การส่ง `0` เป็น `port` ให้ OS เลือก |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | เวอร์ชัน TLS ของ `tcp-listen` `cert-file` คือสายใบรับรอง (PEM ใบรับรองของตัวเองมาก่อน) และ `key-file` คือกุญแจส่วนตัว ทั้งสองถูกอ่านและตรวจสอบที่นี่ ดังนั้นกุญแจที่ไม่ตรงกันให้ `Err` จากการเรียกนี้ ไม่ใช่ที่ไคลเอนต์รายแรก `accept` คืนค่า **ก่อน handshake** และการอ่านหรือเขียนครั้งแรกโดย task ที่จัดการการเชื่อมต่อจะทำ handshake ให้เสร็จ (เหมือน `tls.Conn` ของ Go) ดังนั้นไคลเอนต์ที่ handshake ช้าไม่หน่วง `accept` อื่น เมื่อให้ `:client-ca` (PEM) จะ **กำหนดให้ทุกไคลเอนต์** แสดงใบรับรองที่ออกโดย CA ในนั้น (mutual TLS) หากไม่ให้ ก็ไม่ขอ |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | ฟังที่ `path` **`Err` หากไฟล์มีอยู่แล้ว** (อาจเป็นของอีกโพรเซสที่กำลังรัน จึงไม่แทนที่โดยไม่แจ้ง) `close` ลบไฟล์ |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | การเชื่อมต่อถัดไป หยุด task จนกว่าจะมา ยอมแพ้ด้วย `Err` หลัง `:timeout` วินาที |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | จนกว่าจะอ่านได้โดยไม่ต้องรอ หรือ `secs` วินาที `true` หมายถึงอย่างแรก (รวมข้อมูลที่อยู่ในบัฟเฟอร์แล้ว) วิธีใส่นาฬิกาให้การอ่าน: `(if (wait-readable c 5.0) (read-line c) ...)` สิ่งที่มันรับประกันคือ **การอ่านครั้งถัดไปไม่หยุด**; `read-line` อาจรอส่วนที่เหลือของบรรทัด |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | จนกว่าจะเขียนได้ หรือ `secs` วินาที |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | เพิ่มใบรับรองอีกหนึ่งใบให้ลิสเทนเนอร์ `tls-listen` แสดงให้ไคลเอนต์ที่ขอ `name` (SNI): หลายไซต์บนลิสเทนเนอร์เดียว สายใบรับรองเป็นของ `name` หรือไม่ถูกตรวจสอบที่นี่ และหากไม่ใช่ การเรียกนี้คืน `Err` ไคลเอนต์ที่ขอชื่อที่ไม่มีใครเพิ่ม หรือไม่ขอชื่อ ได้ใบรับรองของ `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | subject ของใบรับรองของอีกฝั่ง (`CN=client,O=Example,C=JP` รูปแบบ RFC 4514 เฉพาะเจาะจงที่สุดก่อน) วิธีที่เซิร์ฟเวอร์ mutual-TLS รู้ว่า "ใครเชื่อมต่อ" (หลังการอ่านครั้งแรกที่ทำ handshake ให้เสร็จ) ฝั่งไคลเอนต์คือชื่อของใบรับรองเซิร์ฟเวอร์ `none` สำหรับการเชื่อมต่อธรรมดา ก่อน handshake หรือหากอีกฝั่งไม่แสดงใบรับรอง |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | บนการเชื่อมต่อ TLS ฝั่งเซิร์ฟเวอร์ คือชื่อที่ไคลเอนต์ขอ (SNI) วิธีที่เซิร์ฟเวอร์ซึ่งมีหลายไซต์ที่เพิ่มด้วย `tls-add-certificate` รู้ว่า "ไซต์ไหน" `none` สำหรับการเชื่อมต่อธรรมดา ฝั่งไคลเอนต์ หรือไม่มีชื่อ (เชื่อมต่อด้วยที่อยู่) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | ปิด Nagle (`TCP_NODELAY`) `write-string` ไปถึงซ็อกเก็ตทุกครั้ง ดังนั้นเมื่อเปิด Nagle การตอบกลับที่เขียนเป็นสองส่วน คือส่วนหัวและตัวเนื้อหา จะรอ ACK ที่ล่าช้าของอีกฝั่ง: ใช้ `true` สำหรับโปรโตคอลแบบคำขอ/การตอบกลับ Unix domain socket ไม่มี Nagle และสำเร็จเฉย ๆ |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | ตรวจสอบอีกฝั่งขณะว่าง (`SO_KEEPALIVE`) ตรวจพบอีกฝั่งที่หายไปโดยไม่ปิด (ถอดสาย โฮสต์หยุดทำงาน) และรีเซ็ตการเชื่อมต่อ ช่วงเวลาเริ่มต้นของ OS ยาว (มักเป็น 2 ชั่วโมง) จึงใช้คู่กับ `set-keepalive-period` ด้านล่าง Unix domain socket ไม่มี จึง panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | วินาทีที่ว่างก่อนการตรวจสอบครั้งแรกและช่วงห่างระหว่างการตรวจสอบ (วินาทีเต็ม อย่างน้อย 1) เหมือน `SetKeepAlivePeriod` ของ Go ทั้งสองถูกตั้งเป็นค่าเดียวกัน |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | หาก **อีกฝั่ง** ทำให้การเชื่อมต่อนี้เสีย จะเป็นความล้มเหลวครั้งแรก (การรีเซ็ต TLS alert การตัดการเชื่อมต่อระหว่างเขียน) `none` หากปกติดี: EOF จากอีกฝั่งที่ปิดอย่างเรียบร้อยไม่ใช่ความล้มเหลว ดู "ความล้มเหลวของอีกฝั่ง" ด้านล่าง |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` ฝั่งเรา วิธีรู้พอร์ตที่ถูกเลือกหลัง `(tcp-listen h 0)` สำหรับ Unix socket คือพาธ (ปลายที่เชื่อมต่อคือ `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` ของอีกฝั่ง หรือพาธสำหรับ Unix socket |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | ปิดเฉพาะฝั่งส่ง (half-close) อีกฝั่งอ่านได้ EOF และฝั่งนี้ยังอ่านได้ สัญญาณสำหรับ "ฉันส่งคำขอครบแล้ว" สำหรับ TLS จะส่ง `close_notify` ด้วย |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | เวอร์ชันไบต์ของการเชื่อมต่อเดียวกัน |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | เวอร์ชันอักขระของการเชื่อมต่อเดียวกัน |
| `close` | `(close s)` | `Stream` | ส่งบัฟเฟอร์ออกไปแล้วปิด GC ไม่ปิดให้ |
| `with-connection` | `(with-connection (var host port) body...)` | แมโคร | เชื่อมต่อ รันตัวเนื้อหา ปิด `Result<ค่าของตัวเนื้อหา, NetError>` (รูปร่างเดียวกับ `with-open-file`) |

`write-string`/`write-line` **คืนค่าหลังจากเขียนทุกอย่างแล้ว** (เหมือน `net.Conn.Write` ของ Go) หากต้อง
รวมการเขียนเล็ก ๆ ให้สะสมใน `string-output-stream` แล้วเขียนครั้งเดียว `listen` เป็น `true` เฉพาะ
เมื่อมีบางอย่างในบัฟเฟอร์รับ ดังนั้น `read-char-no-hang` ทำงานตามชื่อ

**ความล้มเหลวของอีกฝั่งไม่ panic** หากปลายอีกด้านรีเซ็ตการเชื่อมต่อ ปฏิเสธ TLS handshake หรือ
ตัดกลางการเขียน นั่นไม่ใช่ความผิดพลาดของโปรแกรมนี้ ดังนั้นเซิร์ฟเวอร์จึงไม่หยุด
พร้อมกับไคลเอนต์อื่น ความล้มเหลวครั้งแรกถูกบันทึกบนการเชื่อมต่อ การอ่านต่อมาคืน
`none` (ดูเหมือน EOF) และการเขียนไม่ไปไหนและคืนค่าโดยไม่แจ้ง หากต้องการแยกแยะ ให้ใช้
`socket-error` รูปร่างเดียวกับ `bufio.Scanner.Err` ของ Go (`read-item` เป็น `Option` และไม่มี
ช่องทางอื่นให้รายงาน) เฉพาะความผิดพลาดของโปรแกรมเองเท่านั้นที่ panic (handle ที่ปิดแล้ว การอ่าน
ไบต์ทันทีหลัง `unread-char`)

**timeout เป็นอาร์กิวเมนต์ที่ชัดเจนหรือ `wait-readable`** ไม่มีรูปแบบอย่าง `SetReadDeadline` ของ Go
ที่สตรีมพา deadline และ `read-line` ล้มเหลว: `read-item` คืน `Option<Item>` และไม่มี
ทางคืนข้อผิดพลาด ต้องการนาฬิกาในสามที่ คือการเชื่อมต่อ การ accept และ "การอ่านครั้งถัดไป"
และแต่ละที่มีอาร์กิวเมนต์สำหรับมัน

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

ใน mutual TLS ไคลเอนต์ที่ไม่แสดงใบรับรอง (หรือแสดงใบที่ไม่ผ่าน) จะทำ handshake ฝั่งของตัวเองให้เสร็จ
ก่อนที่เซิร์ฟเวอร์จะตัดสินใจ ภายใต้ TLS 1.3 ดังนั้น `tls-connect` คืน `Ok` และ **การอ่าน
ครั้งแรกคืน `none`** (โดยมี alert อยู่ใน `socket-error`) ฝั่งเซิร์ฟเวอร์ การอ่านครั้งแรกของ
การเชื่อมต่อเดียวกันก็คืน `none` เช่นกัน ไม่มีฝั่งใด panic

## 3. UDP

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | สร้างซ็อกเก็ต จำเป็นแม้เพียงเพื่อส่ง (`port` เป็น `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | ส่งหนึ่ง datagram ชื่อถูกแปลง ไม่ทราบว่าไปถึงหรือไม่ (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | datagram ถัดไป `from` คือ `ip:port` และส่งเป็น `host` ของ `send-to` ได้ทันที |

แปลงระหว่างสตริงกับลำดับไบต์ด้วย `string->utf8` / `utf8->string`
([สตริง](collections.md#1-สตริง-string))

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. สิ่งที่ไม่มี

- วิธีอ่านสิ่งใดของใบรับรองอีกฝั่ง **นอกจาก subject** (SAN ช่วงเวลาที่ใช้ได้
  ผู้ออก)
- **การรอซ็อกเก็ตและ channel พร้อมกันด้วย `select`** เหมือนใน Go ให้เขียนเป็น "เริ่ม task ที่
  อ่านและให้มันป้อน channel"
- **HTTP/2** **datagram** ของ Unix domain (`SOCK_DGRAM`)
- **deadline ที่สตรีมพา** (ด้วยเหตุผลข้างต้น นาฬิกาเป็นอาร์กิวเมนต์)
