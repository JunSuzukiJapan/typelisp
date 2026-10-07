<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# FFI สำหรับ C (defffi)

คู่มือนี้อธิบายวิธีเรียกฟังก์ชัน C จาก typelisp รายการชนิดที่ประกาศได้และข้อจำกัดอยู่ใน
[เอกสารอ้างอิงไวยากรณ์ 3.3](../reference/syntax.md#33-defffi--การประกาศฟังก์ชัน-c-ffi)

## 1. การประกาศและการเรียกฟังก์ชัน

`defffi` ประกาศชื่อและชนิดของฟังก์ชัน C

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

การเรียกถูกห่อด้วย `(unsafe ...)`

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

ต้องใช้ `unsafe` เพราะคอมไพเลอร์ไม่สามารถตรวจสอบได้ว่าชนิดที่ประกาศตรงกับชนิดจริงฝั่ง C
การเขียน `unsafe` หมายความว่าคุณผู้เขียนรับผิดชอบการตรวจสอบนั้นเอง หากลืมจะเกิดข้อผิดพลาด
ที่อธิบายเรื่องนี้

## 2. การเขียน wrapper ที่ปลอดภัย

วิธีใช้ที่ตั้งใจไว้คือจำกัด `unsafe` ไว้ที่เดียวและเปิดเผยฟังก์ชันธรรมดาให้ภายนอก

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. ชนิดสอดคล้องกันอย่างไร

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | จำนวนเต็มที่มีความกว้างเท่ากัน |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (รวมถึง `size_t`, `int64_t` และอื่น ๆ) |
| `ptr` | ตัวชี้ใด ๆ (`void *`, `FILE *` และอื่น ๆ) |
| `(ptr T)` | ตัวชี้ไปยัง `T` ([หัวข้อ 7](#7-โครงสร้าง-c)) |

### สตริง

- `string` ที่คุณส่งไปจะถูกคัดลอกเป็นสตริง C ที่ลงท้ายด้วย NUL และถูกคืนหน่วยความจำหลังจากการเรียก
  คืนค่า หาก NUL อยู่กลางสตริงจะเกิดข้อผิดพลาด
- ผลลัพธ์ของฟังก์ชันที่คืนค่า `string` ก็ถูกคัดลอกเช่นกัน หน่วยความจำฝั่ง C ไม่ถูกคืน
  สำหรับฟังก์ชันที่คืนสตริงซึ่งผู้เรียกต้องคืนเอง (เช่น `strdup`) ให้รับผลลัพธ์เป็น
  `ptr` และ `free` ด้วยตนเอง
- หากฟังก์ชันที่ประกาศว่าคืน `string` คืน NULL จะเป็นข้อผิดพลาด ให้รับผลลัพธ์ของ
  ฟังก์ชันที่อาจคืน NULL (เช่น `getenv`) เป็น `ptr`

### `c-long` / `c-ulong` / `ptr`

ชนิดเหล่านี้มีไว้เพื่อส่งค่าข้ามขอบเขตกับ C เท่านั้น และ **ไม่รองรับการคำนวณใด ๆ** หาก
ต้องการใช้เป็นจำนวนเต็มของ typelisp ให้แปลงด้วย `as`

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

`ptr` คือค่าที่ไว้ส่งคืนให้ฟังก์ชัน C ไม่มีวิธีอ่านสิ่งที่มันชี้ไปจากฝั่ง typelisp

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

ชนิดเหล่านี้ปรากฏได้เฉพาะเป็นอาร์กิวเมนต์ของฟังก์ชัน ค่าที่คืน และตัวแปรท้องถิ่น ไม่สามารถเป็น
ฟิลด์ของ struct ตัวแปรโกลบอล หรืออาร์กิวเมนต์ชนิดของ `Vector` และสิ่งอื่นในทำนองเดียวกัน

## 4. การระบุชื่อไลบรารี

หากไม่มี `:library` สัญลักษณ์จะถูกค้นหาในสิ่งที่ลิงก์เข้ากับโพรเซสอยู่แล้ว (libc และ
อื่น ๆ) ฟังก์ชันจากไลบรารีอื่นต้องใช้ `:library`

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- ชื่อสั้นอย่าง `"sqlite3"` จะถูกค้นหาเป็น `libsqlite3.dylib` แล้วจึงเป็น `libsqlite3.so`
- ชื่อที่มี `/` ถือเป็นพาธ
- หากไม่พบสัญลักษณ์ที่ประกาศ ข้อผิดพลาดจะระบุชื่อสัญลักษณ์นั้น

## 5. การคอมไพล์แบบ AOT

โปรแกรมที่ใช้ `defffi` สามารถสร้างเป็นไฟล์ปฏิบัติการด้วย
[`compile-file`](compile.md#3-การสร้างไฟล์ปฏิบัติการด้วยการคอมไพล์แบบ-aot) ได้ตามที่เป็น ไลบรารีที่ระบุ
ด้วย `:library` จะถูกเพิ่มตอนลิงก์โดยอัตโนมัติ ดังนั้น `compile-file` ไม่ต้องใช้อาร์กิวเมนต์เพิ่ม

## 6. Callback

คุณส่งฟังก์ชัน typelisp ให้ฟังก์ชัน C และให้มันเรียกกลับได้ ให้เขียนชนิดฟังก์ชัน
ไว้ในชนิดอาร์กิวเมนต์ของ `defffi` และตอนเรียกให้ใส่ชื่อฟังก์ชันหรือนิพจน์ `lambda` ใน
ตำแหน่งนั้น

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- ส่งได้เฉพาะฟังก์ชัน **ที่ไม่มีตัวแปรอิสระ (free variable)** ฟังก์ชันระดับบนสุด `lambda` และ
  ฟังก์ชัน `labels` ท้องถิ่นใช้ได้ทั้งหมด แต่การอ้างถึงตัวแปรท้องถิ่นของขอบเขตที่ห่อหุ้มอยู่เป็นข้อผิดพลาด
  ตอนตรวจสอบชนิด C ส่งเฉพาะอาร์กิวเมนต์ที่ประกาศ จึงไม่มีทางส่งตัวแปรที่จับไว้ หากต้องการเก็บสถานะ
  ให้ใช้ตัวแปรโกลบอล
- ตัวแปรที่เก็บฟังก์ชันส่งไม่ได้ ให้เขียนชื่อฟังก์ชันหรือนิพจน์ `lambda` ตรงนั้นเลย
- `panic` หรือ `throw` ภายใน callback จะไปถึงผู้เรียกหลังจากฟังก์ชัน C คืนค่า
- callback เรียกได้เฉพาะขณะที่ฟังก์ชัน C ที่ typelisp เรียกกำลังทำงานอยู่ ไม่สามารถ
  ใช้จากสิ่งอย่าง `atexit` หรือตัวจัดการสัญญาณ (signal handler)

## 7. โครงสร้าง C

หากต้องการส่งสิ่งอย่างอาร์เรย์ของโครงสร้างให้ฟังก์ชัน C ให้ประกาศ struct ที่มีเลย์เอาต์เดียวกับ
ใน C ด้วย `def-c-struct` และจัดสรรภายใน `unsafe`

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` จัดสรรค่า `T` จำนวน `n` ค่าและคืน `(ptr T)` `(c-ref p i)` คือตัวชี้ไปยัง
  ตัวที่ `i` `p::field` คือฟิลด์ และ `(c-deref p)` คือสิ่งที่ตัวชี้ไปยังสเกลาร์อย่าง `i32`
  ชี้ไป ทั้งหมดนี้เขียนด้วย `setf` ได้
- `(as ptr p)` แปลงให้เป็น `ptr` ที่ไม่มีชนิดเพื่อส่งให้ฟังก์ชัน C ที่รับ `void *`
- ขนาดของ `item` (ในที่นี้คือ 8) และตำแหน่งของแต่ละฟิลด์ถูกกำหนดด้วยกฎเดียวกับใน
  C

### อายุของหน่วยความจำที่จัดสรร

หน่วยความจำที่จัดสรรจะถูกคืนเมื่อการควบคุมออกจาก `unsafe` ชั้นนอกสุดในฟังก์ชันนั้น กรณีออก
ด้วย `panic` หรือ `throw` ก็เช่นเดียวกัน ด้วยเหตุนี้ ค่า `(ptr T)` จึงนำออกนอก `unsafe` ไม่ได้
การทำให้เป็นค่าของ `unsafe` การจับไว้ใน closure การส่งให้ `task` และการโยนด้วย `throw`
เป็นข้อผิดพลาดของชนิดทั้งหมด ให้คัดลอกค่าที่ต้องการใช้ภายนอกเป็นตัวเลขหรือ `defstruct` ภายใน
`unsafe`

เมื่อจัดสรรภายใน `lambda` หรือฟังก์ชัน `labels` ให้เขียน `unsafe` ภายในฟังก์ชันนั้น

### หน่วยความจำที่ C จัดสรร

ตัวชี้ที่ได้รับจาก C เป็น `(ptr T)` (ค่าที่ `defffi` คืน อาร์กิวเมนต์ของ callback และอื่น ๆ) จะเป็น
ข้อผิดพลาด เว้นแต่ชี้อยู่ภายในหน่วยความจำที่จัดสรรด้วย `c-alloc` ให้ประกาศฟังก์ชันที่รับ
หน่วยความจำที่ C จัดสรรด้วย `malloc` หรือ NULL ด้วย `ptr` ที่ไม่มีชนิด

## 8. สิ่งที่ทำไม่ได้

- **ฟังก์ชัน variadic** (`printf` และทำนองเดียวกัน) ประกาศไม่ได้ ส่วน variadic ถูกส่งด้วย
  กฎที่ต่างจากอาร์กิวเมนต์คงที่ ให้ประกาศชื่อแยกกันสำหรับจำนวนอาร์กิวเมนต์แต่ละแบบที่
  ใช้
- **การส่งหรือคืน struct แบบส่งค่า (by value)** ทำไม่ได้ ให้ใช้ฟังก์ชันที่ส่งตัวชี้
- **การประกาศแบบ generic** ทำไม่ได้
- **ใช้ชื่อเดียวกับฟังก์ชันที่มีให้ในตัวไม่ได้**
- **ส่งเป็นค่าฟังก์ชันไม่ได้** คุณส่งแบบ `(map xs c-abs)` ไม่ได้ ให้ห่อด้วย
  `lambda`

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
