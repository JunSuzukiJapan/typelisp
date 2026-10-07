<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# สตรีมและไฟล์

trait และเมทอดของสตรีม ชนิดสตรีมที่เป็นรูปธรรม การดำเนินการกับไฟล์ และ pathname ซ็อกเก็ตเครือข่ายก็เป็น
สตรีมเช่นกัน และอยู่ใน[เครือข่าย](network.md)

## 1. ลำดับชั้นของ trait

สิ่งที่ CL แสดงด้วยลำดับชั้นของคลาส ที่นี่แสดงด้วย **ลำดับชั้นของ trait** ทั้ง
ทิศทาง (อินพุต / เอาต์พุต) และชนิดของสมาชิกถูกตัดสิน **แบบสถิต** จึงไม่จำเป็นต้องถาม
ตอนรันว่า "สตรีมนี้อ่านได้หรือไม่"

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

ฟังก์ชันที่อ่านอักขระรับชนิดสตรีมใดก็ได้ ไม่ว่าจะมีให้ในตัวหรือผู้ใช้นิยาม หากรับ
`(where (CharInput S))` หรือ `:dyn CharInput`

## 2. เมทอด

ทุกเมทอดของ `CharInput` มี implementation เริ่มต้น implementation เขียนเฉพาะ `read-item`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | สมาชิกถัดไป `none` เมื่อถึงจุดสิ้นสุด **เมทอดเดียวที่ต้อง implement** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | อักขระถัดไป |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | จนถึงการขึ้นบรรทัดใหม่ถัดไป (การขึ้นบรรทัดใหม่ถูกใช้และตัดออก) บรรทัดสุดท้ายที่ไม่ลงท้ายด้วยการขึ้นบรรทัดใหม่ก็ถูกคืนด้วย |
| `read-all` | `(read-all s)` | `(S)→string` | ทุกอย่างที่เหลืออยู่ |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | เฉพาะอักขระที่อยู่ในมืออยู่แล้ว `none` แทนที่จะรอ |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | ผลักอักขระได้ถึง `n` ตัวลงใน `v` และคืนว่าอ่านได้จริงกี่ตัว น้อยกว่า `n` เฉพาะที่จุดสิ้นสุด |

`listen` อยู่ใน `InputStream` (แม่ของ `CharInput`):

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | การอ่านครั้งถัดไปตอบได้โดยไม่ต้องรอหรือไม่ ค่าเริ่มต้นคือ `false` **ด้านที่ไม่เคยโกหก**: `true` จะเป็นการเดา และการเดาผิดจะทำให้ `read-char-no-hang` บล็อก สตรีมที่มีให้ในตัวทั้งหมด override มัน **สำหรับสตรีมที่ผู้ใช้นิยามซึ่งไม่ override `read-char-no-hang` จะคืน `none` เสมอ** |

`PeekInput` (ที่สืบทอดจาก `CharInput`) เพิ่ม **การดันอักขระกลับหนึ่งตัว** เฉพาะตัวสตรีม
เองที่มีที่เก็บอักขระที่ดันกลับ ดังนั้นสิ่งนี้มี implementation เริ่มต้นไม่ได้ และ
เป็น trait แยก `file-stream`/`string-input-stream`/`standard-stream` implement มัน และสตรีมอื่นใด
ก็ได้มันเมื่อห่อด้วย `make-peek-stream` (บทที่ 4)

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | ทำให้การอ่านครั้งถัดไปคืน `c` **เมทอดเดียวที่ต้อง implement** เหมือนใน CL รับประกันเพียงหนึ่งอักขระ |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | ดูอักขระถัดไปโดยไม่ใช้มัน |

ในทำนองเดียวกัน สำหรับ `CharOutput` implementation เขียนเฉพาะ `write-item`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | เขียนหนึ่งสมาชิก **เมทอดเดียวที่ต้อง implement** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | เขียนหนึ่งอักขระ |
| `write-string` | `(write-string s str)` | `(S,string)→()` | เขียนสตริง |
| `write-line` | `(write-line s str)` | `(S,string)→()` | สตริงและการขึ้นบรรทัดใหม่ |
| `terpri` | `(terpri s)` | `(S)→()` | การขึ้นบรรทัดใหม่หนึ่งครั้ง (ชื่อของ CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | การขึ้นบรรทัดใหม่หนึ่งครั้งเว้นแต่อยู่ต้นบรรทัด |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | อักขระถัดไปที่เขียนจะเริ่มบรรทัดหรือไม่ ค่าเริ่มต้นคือ `false` (ดังนั้น `fresh-line` เขียนการขึ้นบรรทัดใหม่: เมื่อสงสัย การเขียนเป็นด้านที่ปลอดภัย) สตรีมที่มีให้ในตัวทั้งหมด override มัน |
| `finish-output` | `(finish-output s)` | `(S)→()` | flush บัฟเฟอร์ |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | เขียนอักขระทั้งหมดของ `v` ตามลำดับ |

`at-line-start` จำ **เฉพาะสิ่งที่เขียนผ่านสตรีมนั้น** `print`/`println`/
`(format true ...)` เขียนไปยังเอาต์พุตมาตรฐานโดยไม่ผ่าน `*standard-output*` ดังนั้นหากคุณผสม
ทั้งสอง `(fresh-line *standard-output*)` ไม่ทราบเกี่ยวกับการขึ้นบรรทัดใหม่ที่ `println` เขียน ให้ยึดกับ
อย่างใดอย่างหนึ่ง

`Stream` เป็นสิ่งร่วมของทุกสตรีม:

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | ยังเปิดอยู่หรือไม่ |
| `close` | `(close s)` | `(S)→()` | ปิด **GC ไม่ปิดสตรีม** ดังนั้นให้ทำอย่างชัดเจน (หรือด้วย `with-open-file`) |

## 3. ชนิดสตรีมที่เป็นรูปธรรม

| ชนิด | วิธีสร้าง | trait ที่ implement |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` เป็นหนึ่งในสามค่าคงที่ `direction-input` / `direction-output` /
`direction-append` `open-file` คืน `Err(FileError)` หากเปิดไฟล์ไม่ได้ (ไฟล์ที่หายไปเป็น
ผลลัพธ์ธรรมดา ไม่ใช่ panic) ชื่อไฟล์เป็นสตริงหรือ `pathname` ก็ได้ (`Pathish` ใน
บทที่ 9)

`(get-output-stream-string s)` คืนสิ่งที่ถูกเขียนลงใน `string-output-stream` และทำให้มัน
ว่าง เหมือนใน CL ดึงออกได้แม้หลัง `close`

**I/O แบบไบต์** ใช้ `ByteInput`/`ByteOutput` สิ่งเหล่านี้กำหนด `Item` ของ `InputStream`/`OutputStream` เป็น
`int` เช่นเดียวกับที่ `CharInput`/`CharOutput` กำหนดเป็น `char`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | ไบต์ถัดไป `none` ที่จุดสิ้นสุดของไฟล์ |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | เขียนหนึ่งไบต์ ข้อผิดพลาดนอก 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | เวอร์ชันอักขระ ในหน่วยไบต์ |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | เหมือนข้างบน |

CL ตัดสินชนิดสมาชิกใน **การเรียก** เช่น `(open name :element-type '(unsigned-byte 8))`
แต่ที่นี่ชนิดสมาชิกคือ **ชนิด** ของสตรีม ดังนั้นสิ่งที่ต่างกันคือฟังก์ชันที่เปิดมัน
การอ่านไบต์จากสตรีมอักขระเป็นข้อผิดพลาดของชนิด (`string-input-stream` ไม่ implement
`ByteInput`) การอ่านไบต์ทันทีหลังดันอักขระกลับด้วย `unread-char` ก็เป็นข้อผิดพลาดเช่นกัน

## 4. สตรีมแบบประกอบ

ทั้งหมดเป็น `defstruct` ในไลบรารีมาตรฐานและซ้อนกันได้

| ชื่อ | รูปแบบ | คำอธิบาย |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | เขียนไปยังทุกตัวของ `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | อ่านจาก `in` และเขียนไปยัง `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | อ่านจาก `in` และเขียนอักขระที่อ่านไปยัง `out` ด้วย |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | อ่าน `Vector<:dyn CharInput>` ทีละตัวต่อกัน |
| `make-peek-stream` | `(make-peek-stream in)` | เพิ่มการดันกลับหนึ่งอักขระให้ `:dyn CharInput` ใดก็ได้ ทำให้เป็น `PeekInput` (สำหรับ `read-sexpr`) |

## 5. แมโคร

| ชื่อ | รูปแบบ | คำอธิบาย |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | เปิด รันตัวเนื้อหา ปิด `Result<ค่าของตัวเนื้อหา, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | อ่านจากสตริง |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | คืนสิ่งที่ถูกเขียน |

## 6. ฟังก์ชัน generic และการดำเนินการกับไฟล์

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | ถ่ายโอนทุกอย่าง |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | ทุกบรรทัดที่เหลือ |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | อ่านหนึ่ง `Sexpr` (`read` ของ CL) `Ok(eof)` ที่จุดสิ้นสุดของอินพุต `Ok(datum d)` เมื่ออ่านได้หนึ่งตัว `Err` หากไม่ใช่ข้อมูล มัน **ใช้อักขระช่องว่างหนึ่งตัว** ที่จบ datum (เหมือนใน CL) `ReadOutcome` ไม่ใช่ `Option<Sexpr>` เพื่อให้การอ่านลิสต์ว่าง `()` กับจุดสิ้นสุดของอินพุตไม่เป็นค่าเดียวกัน |
| `read-sexpr-preserving-whitespace` | เหมือนข้างบน | เหมือนข้างบน | เหมือนกัน แต่ทิ้งช่องว่างไว้ (`read-preserving-whitespace` ของ CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | อ่านจนถึง `ch` และสร้างลิสต์ `ch` ถูกใช้ `Err` หากอินพุตหมด |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | เขียนทีละบรรทัด |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | เนื้อหาทั้งหมด |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | ทุกบรรทัด |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | เขียนออกไป |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | มีอยู่หรือไม่ |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | ลบ เปลี่ยนชื่อ (อาร์กิวเมนต์เป็น `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | พาธสัมบูรณ์ที่แก้ symbolic link และ `.`/`..` แล้ว `Err` หากไม่มีอยู่ |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | เวลาของการแก้ไขครั้งล่าสุด เป็น **universal time** ดังนั้น `decode-universal-time` ([เวลา](system.md#2-การถอดรหัสและการเข้ารหัสวันที่)) อ่านได้ |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | ชื่อล็อกอินของเจ้าของ `Err` หากไฟล์ไม่มีอยู่ `Ok(none)` หาก uid ของเจ้าของไม่มีรายการในฐานข้อมูลรหัสผ่าน: สองกรณีที่ CL แยกกันถูกคงไว้แยกกัน |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | เป็นไดเรกทอรีหรือไม่ **เป็น `false` ด้วยหากไม่มีอยู่** ใช้ `probe-file` เพื่อแยกสองกรณี |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | แสดงเนื้อหาตาม truename (พาธสัมบูรณ์ที่แก้ symbolic link แล้ว เหมือน `truename`) symbolic link ที่เป้าหมายหายไปถูกละไว้ `.`/`..` ถูกละไว้ ลำดับเป็นไปตามที่ OS ให้ |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | สร้างพร้อมไดเรกทอรีแม่ สำเร็จหากมีอยู่แล้ว |

ทุกอาร์กิวเมนต์ที่ระบุไฟล์ **เป็นสตริงหรือ `pathname` ก็ได้** นี่เป็นการปฏิบัติเดียวกับ
pathname designator ของ CL แก้ไขผ่าน trait `Pathish` แทนการทดสอบชนิดตอนรัน
(บทที่ 9)

อักขระสิ้นสุดของ `read-delimited-list` **จบโทเค็นด้วย** มีผลเฉพาะที่ความลึก 0:
ใน `(1 2]` `]` ถูกอ่านเป็นส่วนหนึ่งของข้อความของลิสต์เองและรายงานว่าลิสต์เสีย
ไม่มีสิ่งที่ตรงกับอาร์กิวเมนต์ที่สาม `recursive-p` ของ CL

## 7. การทำให้ชนิดของคุณเองเป็นสตรีม

เขียน `write-item` หนึ่งตัวและ implementation เริ่มต้นนำส่วนที่เหลือมาให้ ใส่ในสตรีมแบบประกอบก็ได้

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

อินพุตก็ทำงานแบบเดียวกัน: คุณเขียนเฉพาะ `read-item` แม้ชนิดที่ไม่มีการดันกลับของตัวเอง
ก็ `read` ได้เมื่อห่อแล้ว เช่น `(read-sexpr (make-peek-stream my-stream))`

## 8. readtable

| ชื่อ | การเรียก | ชนิด | คำอธิบาย |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` อ่านอักขระ `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | คืนสิ่งที่ลงทะเบียนไว้ |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` อ่านลำดับสองอักขระ `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | เหมือนข้างบน |

`F` คือ `(fn (string-input-stream char) Option<Sexpr>)` วิธีใช้ เวลาที่มีผล และความแตกต่างจาก CL
อยู่ใน[เอกสารอ้างอิงไวยากรณ์](../syntax.md#11-ตัวอ่านแมโคร-readtable)

## 9. Pathname (`pathname`)

ชื่อไฟล์ที่แยกเป็นส่วน ๆ เก็บส่วนประกอบไดเรกทอรีที่คั่นด้วย `/` ชื่อ ชนิด
(นามสกุล) และว่าเริ่มที่รากหรือไม่

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 trait ตัวกำหนด pathname `Pathish`

ในที่ที่ CL รับ pathname designator (สตริงหรือ pathname) ภาษานี้รับ `Pathish`
ทั้ง `string` และ `pathname` implement มัน และ **ทุกการดำเนินการกับไฟล์รับมันแบบ generic** ดังนั้น
`(open-input "a.txt")` และ `(open-input p)` เป็นการเรียกธรรมดาทั้งคู่ (ไม่มีการทดสอบชนิดตอนรัน)
`namestring` ของสตริงเพียงคืนตัวเอง ดังนั้นตราบที่คุณส่งสตริง จะไม่มีการ parse เกิดขึ้น

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | รูปแบบสตริง ต้อง implement |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | แปลงเป็น `pathname` (ฟังก์ชัน `pathname` ของ CL เปลี่ยนชื่อเพราะจะชนกับชื่อชนิด) ต้อง implement |

### 9.2 ฟังก์ชัน

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | แยกสตริงเป็นส่วน ๆ `/` ท้าย (หรือชื่อว่าง) หมายถึง "ไม่มีชื่อ" คือเป็นไดเรกทอรี |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | สร้างจากเฉพาะส่วนประกอบที่ให้ (ทั้งหมดเป็น `&key`) ชื่อหรือชนิดที่ละไว้ยังคง "ไม่มี" และเป็นสิ่งที่ `merge-pathnames` เติมให้ |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | ส่วนประกอบไดเรกทอรี ชั้นนอกสุดก่อน |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | ชื่อที่ไม่มีชนิด `none` สำหรับไดเรกทอรี |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | ส่วนหลังจุดสุดท้าย จุดนำหน้าไม่นับ (`.gitignore` ทั้งหมดเป็นชื่อ) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | เริ่มที่รากหรือไม่ |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | ไดเรกทอรีโฮม `none` หากไม่มี `$HOME` (CL ยอมให้ `NIL` ด้วย) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | ส่วนจนถึง `/` สุดท้าย |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | เฉพาะส่วน `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | เติมส่วนประกอบที่ขาดจาก `p` จาก `default` `p` แบบสัมพัทธ์ไปอยู่ใต้ไดเรกทอรีของ `default` `p` แบบสัมบูรณ์คงไดเรกทอรีของตัวเอง |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | รูปแบบที่สัมพัทธ์กับ `default` ทั้งหมดของ `p` หากไม่ได้อยู่ใต้ฐาน |

อาร์กิวเมนต์ชนิดทั้งหมดมี `(where (Pathish P))`

## 10. ความแตกต่างจาก CL

- **ลำดับชั้นของ trait ไม่ใช่ลำดับชั้นของคลาส** ไม่มี `input-stream-p` / `output-stream-p`: ชนิด
  พาทิศทาง จึงไม่ใช่คำถามที่ต้องถามตอนรัน
- **`read` มีชื่อต่างกันสำหรับเวอร์ชันสตริงและสตรีม** `(read "...")` (สอดคล้องกับ
  ค่าแรกของ `read-from-string` ของ CL หากต้องการตำแหน่งที่การอ่านจบด้วย ให้ใช้
  `read-from-string`) และ `(read-sexpr s)` (`read` ของ CL) การเรียกหนึ่งแก้ไขไปยังชนิดตัวรับหนึ่งชนิด ดังนั้น
  ชื่อเดียวกัน overload ไม่ได้
- **การดันกลับเป็น trait แยก** (`PeekInput`) ดังนั้นชนิดที่ต้องการเพียง `read-char` ไม่ถูกบังคับให้
  implement `unread-char`
- **การปิดเป็นแบบชัดเจน** GC ไม่ปิดสตรีม (GC รันในเวลาที่คาดเดาไม่ได้ ดังนั้น
  การปล่อยให้ GC ทำจะทำให้ช่วงเวลาปิดคาดเดาไม่ได้ด้วย) การใช้ `with-open-file` เป็น
  วิธีที่ปลอดภัย
- **Pathname ไม่มีส่วนประกอบ host, device หรือ version** ไม่มี wildcard pathname และไม่มี
  logical pathname (`logical-pathname`) ตัวคั่นคือ `/` เสมอ
- **ฟังก์ชัน `pathname` คือ `to-pathname`** เพราะชนิด trait และฟังก์ชันใช้เนมสเปซเดียวกัน
- **ไม่มีการจับคู่ด้วย wildcard** ดังนั้น `directory` เป็นฟังก์ชันที่ "แสดงเนื้อหาของไดเรกทอรี
  นั้น" และไม่มีอะไรมากกว่านั้น `directory` ของ CL จับคู่กับรูปแบบ pathname
