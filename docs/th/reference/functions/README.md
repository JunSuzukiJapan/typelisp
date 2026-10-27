<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# ฟังก์ชันที่มีให้ในตัว

รายการฟังก์ชันที่มีให้ในตัว เมทอด และไลบรารีมาตรฐาน สำหรับไวยากรณ์ (ฟอร์มพิเศษและวิธีนิยามสิ่งต่าง ๆ)
ดู[เอกสารอ้างอิงไวยากรณ์](../syntax.md) สำหรับรายการชนิด ดู
[ชนิด](../types.md)

## รูปแบบการเรียก

มีรูปแบบการเรียกสามแบบ

- ฟังก์ชันอิสระ: `(name args...)`
- เมทอดของอินสแตนซ์: `(name receiver args...)` (แก้ไขจากชนิดสถิตของอาร์กิวเมนต์แรก)
- เมทอดสถิต (associated function): `(Type::name args...)`

แต่ละชนิดอาจมีเมทอดชื่อเดียวกันของตัวเอง `(+ a b)` เรียก `+` ของชนิดของ `a`

## ชื่อที่ต้อง `use`

ในบรรดาชื่อของไลบรารีมาตรฐาน ชื่อต่อไปนี้ใช้ได้ในทุกโมดูลโดยไม่ต้องเขียนอะไร
([เอกสารอ้างอิงไวยากรณ์ 3.10](../syntax.md#310-module--use--เนมสเปซ))

- ชนิดพื้นฐาน `Option`/`Result`/`Vector` การวนซ้ำ นิพจน์ S และลิสต์ ฟังก์ชันลำดับ แมโครควบคุม
  การเปรียบเทียบ และ trait ทางคณิตศาสตร์
- ชื่อทุกชื่อที่เป็นชื่อในแพ็กเกจ `COMMON-LISP` ของ Common Lisp ด้วย (`length` `format` `open` เป็นต้น)
- `Error` `assert` `print-object` `HashTable` `Array` `BitVector` trait ของสตรีม `Stream`
  `InputStream` และ `OutputStream` และ `Hash` ซึ่งคีย์ของ `HashTable` ต้อง implement
- ชนิดที่ฟังก์ชันและตัวแปรของสิ่งเหล่านี้กล่าวถึง

ชื่ออื่นใดให้นำเข้าด้วย `use` เช่น `(use std::net::tcp-connect)` หรือเขียนด้วยพาธ
`std::net::tcp-connect` ตารางต่อไปนี้ให้ชื่อเหล่านั้นและโมดูลของแต่ละชื่อ

| โมดูล | ชื่อ |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char` `digit-weight` `name->char` |
| `std::collections` | `Deque` `HashSet` `Scope` `SortedTable` `hashtable-iter` |
| `std::env` | `command-line-args` `getenv` `home-directory` |
| `std::error` | `WrappedError` `as-dyn-error` `assert-eq` `describe-error` `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append` `direction-input` `direction-output` `directory-p` `open-binary` `open-binary-input` `open-binary-output` `open-file` `open-input` `open-output` `read-file-lines` `read-file-string` `write-file-string` |
| `std::hash` | `*sxhash-mask*` `sxhash-string` |
| `std::io` | `ByteInput` `ByteOutput` `PeekInput` `binary-file-stream` `copy-stream` `make-peek-stream` `peek-stream` `read-lines` `write-lines` |
| `std::net` | `NetError` `byte-stream-of` `char-stream-of` `datagram` `socket-byte-stream` `socket-listener` `socket-stream` `tcp-connect` `tcp-listen` `tls-connect` `tls-listen` `udp-bind` `udp-socket` `unix-connect` `unix-listen` `with-connection` |
| `std::num` | `ParseFloatError` `ParseIntError` `atan2` `parse-float` `parse-int` |
| `std::path` | `Pathish` `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome` `read-from-string-preserving-whitespace` `read-sexpr` `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim` `right-trim` `string->utf8` `trim` `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan` `Context` `Mutex` `WaitGroup` `after` `with-lock` |
| `std::thread` | `Task` `Thread` `yield` |
| `std::time` | `internal-time-seconds` `timezone-daylight-p` `timezone-offset-seconds` |

## การอ่านตาราง

ตารางในแต่ละบทมีคอลัมน์ "ชื่อ, รูปแบบ, ชนิด, คำอธิบาย" คอลัมน์ชนิดเขียนเป็น
`(ชนิดของอาร์กิวเมนต์,...)→ชนิดของค่าที่คืน`

- ตัวอักษรพิมพ์ใหญ่ตัวเดียว เช่น `T`, `A` หรือ `B` เป็นตัวแปรชนิด
- หมายเหตุอย่าง `where Eq A` คือ trait bound ที่ตัวแปรชนิดต้องเป็นไปตาม
- `Iter<A>` หมายถึง "การ implement ใดก็ได้ของ `Iter` ที่ `Item` เป็น `A`"
- อาร์กิวเมนต์ที่ทำเครื่องหมาย `&optional` / `&key` ละได้

## บท

| ไฟล์ | เนื้อหา |
|---|---|
| [numbers.md](numbers.md) | จำนวนเต็ม จำนวนทศนิยม จำนวนตรรกยะ จำนวนเชิงซ้อน บูลีน การดำเนินการระดับบิต จำนวนสุ่ม |
| [sequences.md](sequences.md) | คู่ `cons-cell` ข้อมูล S-expression `Sexpr` สัญลักษณ์ ฟังก์ชันของลำดับ อิเทอเรเตอร์แบบขี้เกียจ `lazy` ฟังก์ชันอันดับสูง |
| [collections.md](collections.md) | สตริง อักขระ `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, ชนิดข้อผิดพลาด และ trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, trait การคำนวณ |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, ตัวแปรควบคุมการพิมพ์ |
| [format.md](format.md) | คำสั่งรูปแบบ |
| [streams-files.md](streams-files.md) | สตรีม การดำเนินการกับไฟล์ pathname readtable |
| [concurrency.md](concurrency.md) | Task, channel, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix domain socket, UDP |
| [system.md](system.md) | เวลา สภาพแวดล้อมขณะรัน เครื่องมือของการ implement `read`/`eval` docstring ฟังก์ชันที่เกี่ยวกับแมโคร |
