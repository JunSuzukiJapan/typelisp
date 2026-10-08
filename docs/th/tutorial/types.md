<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# พื้นฐานของชนิด

typelisp เป็นภาษาที่มีระบบชนิดแบบสถิต บทนี้อธิบายสิ่งที่ตัวตรวจสอบชนิดทำให้คุณ
ชนิดที่คุณจะใช้บ่อยที่สุด (`Option`, `Result`, struct และ enum) และ generics โดยสมมติว่า
คุณอ่าน[เริ่มต้นใช้งาน](intro.md)แล้ว

## 1. ระบบชนิดแบบสถิตหมายความว่าอย่างไร

ใน typelisp ชนิดของทุกนิพจน์ถูกกำหนดก่อนที่โปรแกรมจะรัน นิพจน์ที่ชนิดไม่เข้ากัน
เป็นข้อผิดพลาดก่อนที่อะไรจะรัน

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

การรันไฟล์นี้หยุดด้วยข้อผิดพลาดของชนิดโดยไม่พิมพ์แม้แต่ `start`

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

คุณต้องเขียนชนิดสำหรับอาร์กิวเมนต์และค่าที่คืนของฟังก์ชัน ตัวแปรโกลบอล และฟิลด์ของ struct
ชนิดของตัวแปร `let` ได้มาจากค่าเริ่มต้นของมัน

ชนิดหลัก:

| ชนิด | ตัวอย่างค่า |
|---|---|
| `int` | `42`, `-7` (จำนวนเต็มความแม่นยำไม่จำกัด) |
| `i8` `i16` `i32` `u8` `u16` `u32` | จำนวนเต็มความกว้างคงที่ |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | ชนิดค่าที่คืนของฟังก์ชันที่ไม่คืนค่า |

ไม่มีวิธีถามชนิดของค่าตอนรัน (ไม่มี `typep` หรือ `type-of` ของ Common Lisp)
เพราะทุกชนิดเป็นที่ทราบก่อนที่โปรแกรมจะรัน

## 2. `Option<T>`: ค่าที่อาจหายไป

typelisp ไม่มี `nil` "อาจไม่มีค่า" แสดงด้วยชนิด `Option<T>` ค่าของ
`Option<T>` เป็นได้ทั้ง `some` ที่เก็บค่า `T` หนึ่งค่า หรือ `none` ที่ไม่เก็บอะไร

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` ไม่ใช่ `int` จึงใช้ในการคำนวณตามที่เป็นไม่ได้ `(+ (safe-div 10 2) 1)` เป็น
ข้อผิดพลาดของชนิด หากต้องการใช้สิ่งที่อยู่ข้างใน ให้แยก `some` จาก `none` ด้วย `match`

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- ในกิ่ง `(some q)` เนื้อหาถูกผูกกับตัวแปร `q`
- `match` ตรวจสอบว่ากิ่งของมัน **ครอบคลุมทุกกรณี** การลืมกิ่ง `(none)` เป็นข้อผิดพลาดของชนิด

### ทำไมจึงไม่มี nil

ในหลายภาษา `nil` (`null`) แทนค่าของชนิดใดก็ได้ ผลคือการลืมจัดการกรณี "ไม่มีค่า" จะไม่ถูกสังเกต
จนกว่าโปรแกรมจะรัน ใน typelisp ที่ใดที่ค่าอาจหายไปจะมีชนิด `Option<T>` และโค้ดจะไม่ผ่านตัวตรวจสอบชนิด
เว้นแต่ `match` จัดการกรณี `none` กรณีที่ลืมจะถูกพบก่อนที่โปรแกรมจะรัน

เงื่อนไขก็ยึดแนวคิดเดียวกัน: มีเพียง `bool` เท่านั้นที่เป็นเงื่อนไขของ `if` ได้ ไม่มีกฎอย่างของ
Common Lisp ที่ว่า "ทุกอย่างที่ไม่ใช่ `nil` เป็นจริง"

### การดำเนินการที่ใช้บ่อย

| ฟอร์ม | ความหมาย |
|---|---|
| `(unwrap-or opt default)` | เนื้อหาสำหรับ `some`; ค่าเริ่มต้นสำหรับ `none` |
| `(unwrap opt)` | ดึงเนื้อหาออกมา หยุดโปรแกรมเมื่อเป็น `none` |
| `(is-some opt)` / `(is-none opt)` | ทดสอบว่าเป็นแบบใด |

ฟังก์ชันของไลบรารีมาตรฐานจำนวนมากคืน `Option` ตัวอย่างเช่น `position` คืนตำแหน่งใน
`some` หากพบสมาชิก และ `none` หากไม่พบ

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: การดำเนินการที่อาจล้มเหลว

การดำเนินการที่อาจล้มเหลวคืน `Result<T,E>`: `ok` ที่เก็บค่า `T` เมื่อสำเร็จ หรือ `err`
ที่เก็บข้อผิดพลาด `E` เมื่อล้มเหลว

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

ฟังก์ชันของคุณเองก็คืน `Result` ได้เช่นกัน

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

ใช้ `Option` เมื่อค่าที่หายไปไม่ต้องการคำอธิบาย และใช้ `Result` เมื่อต้องการบอกว่าทำไม
จึงล้มเหลว [การจัดการข้อผิดพลาด](errors.md) อธิบายการจัดการข้อผิดพลาดโดยละเอียด

## 4. `defstruct`: struct

ชนิดที่มีฟิลด์ที่มีชื่อนิยามด้วย `defstruct`

```lisp
(defstruct point
  (x int)
  (y int))
```

การนิยามให้สิ่งต่อไปนี้แก่คุณ:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

หากต้องการให้ struct มีฟังก์ชันของตัวเอง ให้ใช้ `defmethod` ชนิดของอาร์กิวเมนต์แรก (`self`)
ตัดสินว่าเมทอดเป็นของชนิดใด

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

การเขียนเพียงชื่อชนิดแทนอาร์กิวเมนต์ `self` ทำให้ได้ฟังก์ชันที่เรียกเป็น
`point::origin`

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: หนึ่งในหลายรูปแบบ

ค่าที่เป็นหนึ่งในหลายรูปแบบ เช่น "วงกลม สี่เหลี่ยม หรือจุด" นิยามด้วย
`defenum` แต่ละรูปแบบเรียกว่า **variant** แต่ละ variant เก็บค่าจำนวนและชนิดที่ต่างกันได้

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

ค่าสร้างโดยมีชื่อชนิดนำหน้า เช่น `shape::circle` ใน `match` จะแยกตามชื่อ variant

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

ในกรณีนี้ `match` ก็ตรวจสอบว่าครอบคลุมทุกกรณี หากภายหลังคุณเพิ่ม variant ให้ `shape`
ทุก `match` ที่ไม่จัดการมันจะกลายเป็นข้อผิดพลาดของชนิด ดังนั้นจึงไม่พลาดจุดที่ต้องแก้

หลัง `(use shape)` คุณเขียน `(rect 5 6)` โดยไม่มีชื่อชนิดได้

`Option` และ `Result` เป็น enum ที่สร้างด้วยกลไกเดียวกันนี้

## 6. Generics

ฟังก์ชันที่ใช้ได้กับทุกชนิดนิยามด้วย **พารามิเตอร์ชนิด (type parameter)** `<T>` หลังชื่อของมัน

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

คุณไม่ต้องระบุชนิดตอนเรียก `T` ถูกหาจากอาร์กิวเมนต์

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

struct และ enum ก็เป็น generic ได้เช่นกัน `Vector<T>`, `Option<T>` และ `Result<T,E>` เป็น
ชนิดประเภทนี้

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

ภายในฟังก์ชัน generic ไม่ทราบอะไรเกี่ยวกับ `T` เลย จึงเปรียบเทียบหรือบวกค่า `T` ไม่ได้
หากต้องการกำหนดสิ่งอย่าง "ชนิดใดก็ได้ที่เปรียบเทียบได้" ให้ใช้ trait ([Trait](traits.md))

## 7. การตั้งชื่ออื่นให้ชนิด

`deftype` ตั้งชื่ออื่นให้ชนิด

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` เป็นเพียงการสะกดอีกแบบของ `int` ไม่ใช่ชนิดใหม่ การส่ง `int` ธรรมดาในที่ที่คาดหวัง `meters`
ไม่ใช่ข้อผิดพลาด หากต้องการแยกให้ชัดเจน ให้สร้าง struct เช่น
`(defstruct meters (value int))`

## 8. อ่านอะไรต่อ

- [Trait](traits.md): การให้การดำเนินการร่วมกันแก่ชนิด
- [ชนิด](../reference/types.md): ชนิดที่มีให้ในตัวและ trait ที่แต่ละชนิด implement
