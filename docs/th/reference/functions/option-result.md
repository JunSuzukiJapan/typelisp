<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result และชนิดข้อผิดพลาด

## 1. `Option<T>` / `Result<T,E>`

ตัวสร้าง: `Option<T>` มี `Some(T)` / `None` `Result<T,E>` มี `Ok(T)` / `Err(E)` `E` เป็นชนิดใดก็ได้:
ชนิดข้อผิดพลาดที่เป็นรูปธรรมที่มีให้ในตัว และชนิดที่คุณเขียนเองด้วย `defstruct`/`defenum`
ใส่ที่นั่นได้เหมือนกัน (บทที่ 3)

| ชื่อ | รูปแบบ | Option | Result | คำอธิบาย |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | ดึงค่าออกมา panic เมื่อเป็น `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | ค่า หรือค่าเริ่มต้น |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | เป็น `Some` หรือไม่ |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | เป็น `None` หรือไม่ |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | เป็น `Ok` หรือไม่ |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | เป็น `Err` หรือไม่ |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | ดึงค่าออกมา ถ้าเป็น `None`/`Err` จะ panic ด้วย `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | ค่านั้น หรือผลของ `f` โดยจะเรียก `f` เฉพาะเมื่อเป็น `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | ใช้ `f` กับเนื้อในของ `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | ใช้ `f` กับเนื้อในของ `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | ถ้าเป็น `Some`/`Ok` จะส่งเนื้อในให้ `f` แล้วคืนผลของมัน |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | ถ้าเป็น `None`/`Err` จะคืนผลของ `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | เปลี่ยน `Some(v)` เป็น `Ok(v)` และ `None` เป็น `Err(e)` |

ตัวสร้างคือ `Option::some`/`Option::none`/`Result::ok`/`Result::err` (หรือหลัง
`(use option)`/`(use result)` ใช้ชื่อเปล่า `some`/`none`/`ok`/`err`)

การแตกแขนงเขียนอย่างชัดแจ้งด้วย `match` หรือต่อกันเป็นสายด้วย `map`/`and-then` และเมธอดอื่นข้างบน ไม่มีไวยากรณ์ที่ตรงกับ `?` ของ Rust

`map` ของ `Option`/`Result` เป็นเมธอด แยกจาก `map` ของ[ลำดับ](sequences.md) เมื่อชนิดของอาร์กิวเมนต์ตัวแรกเป็น `Option`/`Result` ตัวนี้จะถูกเรียก

แมโคร `->` ส่งค่าเป็นอาร์กิวเมนต์ตัวแรกให้แต่ละฟอร์มถัดไปตามลำดับ (เหมือน `->` ของ Clojure) `(-> x (f a) (g b))` จะกลายเป็น `(g (f x a) b)` ชื่อที่ไม่มีวงเล็บ `h` ถือเป็น `(h x)` อาร์กิวเมนต์ตัวแรกของเมธอดคือตัวรับ จึงต่อคอมบิเนเตอร์เป็นสายได้ตามที่เป็นอยู่:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. การแสดงผลขณะรันของ `Option<T>`

เช่นเดียวกับใน Rust **`Option<T>` มักไม่สร้างกล่อง (box)** `some v` คือ `v` เอง และ `none` คือ
ค่าลิสต์ว่าง ไม่มีการจัดสรรและไม่มีการอ้อม `Option<Sexpr>` (ซึ่งลิสต์ว่างคือ
`none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` และ `Option<(fn ...)>`
ล้วนอยู่ในรูปนี้

จะใช้กล่องเฉพาะเมื่อไม่อาจแยกค่าของ `T` ออกจากค่าลิสต์ว่างได้:

| `T` | การแสดงผล | เหตุผล |
|---|---|---|
| `Option<U>` (ซ้อนกัน) | กล่อง | `none` ภายในจะเป็นค่าเดียวกับ `none` ภายนอก |
| `()` | กล่อง | ค่าของ `()` คือค่าลิสต์ว่างเอง |
| `ptr` / `c-long` / `c-ulong` | กล่อง | ทั้ง 64 บิตเป็นค่า ไม่เหลือที่ให้แยกความแตกต่าง |
| อย่างอื่นทั้งหมด | ไม่มีกล่อง | — |

การแสดงผลถูกกำหนดโดยชนิดเพียงอย่างเดียวและอ่านจากค่าไม่ได้ ตอนพิมพ์
`(some ...)`/`none` ถูกสร้างขึ้นใหม่จากชนิดสถิต ดังนั้น `(format false "~a" opt)` พิมพ์
`(some 1)` มีข้อจำกัดสองข้อ:

- **ใส่ใน `:dyn Trait` ไม่ได้** (การส่งค่า `Option<int>` ที่คุณเขียน
  `(impl Speak Option<int> ...)` ไว้ให้ `:dyn Speak` เป็นข้อผิดพลาด)
- การ downcast `(the Option<T> ...)` จาก `Sexpr` **ต้องระบุตัวสร้าง**:
  `(the Option<int> (some x))` / `(the Option<int> (none))` รูปแบบที่ผูกทั้งค่า
  `(the Option<int> o)` เป็นข้อผิดพลาด

## 3. ชนิดข้อผิดพลาดและ trait `Error`

ตาม `std::error::Error` ของ Rust **`Error` ไม่ใช่ชนิดแต่เป็น trait** ชนิดที่เป็นรูปธรรมซึ่ง
แทนข้อผิดพลาดแยกกันตามวัตถุประสงค์ และแต่ละชนิด implement `Error`

| ชนิด | สร้างโดย |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | การดำเนินการกับไฟล์และสตรีม ([สตรีมและไฟล์](streams-files.md)) |
| `NetError` | การดำเนินการเครือข่าย ([เครือข่าย](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)` `simple-error` ของ CL: ตัวเลือกเริ่มต้นเมื่อคุณเพียงต้องการบอกว่าเกิดอะไรขึ้น |
| `WrappedError` | `(wrap-error msg cause)` ชนิดที่พาทั้งข้อความของคุณเองและสาเหตุ เป็นเหตุผลที่ trait `Error` มี `source` |

`ParseIntError` ถึง `NetError` แต่ละตัวเป็น "enum ที่มี variant เดียวเก็บสตริงข้อความหนึ่ง
สตริง" และชื่อชนิดกับชื่อ variant เหมือนกัน (`(match e ((ParseIntError m) m))` สร้างด้วย
`(ParseIntError::ParseIntError "...")`) ไม่มีอะไรพิเศษ: ถูกปฏิบัติเหมือนกับ
ชนิดข้อผิดพลาดของคุณเองที่เขียนด้วย `(defstruct my-err (...))` / `(defenum my-err ...)`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | ข้อความของข้อผิดพลาด (เมทอดของ trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | สาเหตุที่ข้อผิดพลาดนี้ห่อไว้ หรือ `None` หากไม่มี (`Error::source` ของ Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implement `Error`) | ขยายชนิดข้อผิดพลาดที่เป็นรูปธรรมเป็น trait object |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implement `Error`) | ข้อความและสายของสาเหตุที่ได้จากการตาม `source` หนึ่งสาเหตุต่อบรรทัด CL ไม่มีสิ่งที่ตรงกัน ("caused by" ของ Rust) |

หากคุณ implement `Error` ให้ชนิดข้อผิดพลาดของคุณเอง จะจัดการได้ **แบบเดียวกัน** กับ
ข้อผิดพลาดที่มีให้ในตัว:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

หากต้องการรวมชนิดข้อผิดพลาดหลายชนิดไว้ใน `Result` เดียว ให้ใช้ `Result<T, :dyn Error>` (สอดคล้องกับ
`Box<dyn Error>` ของ Rust) และขยายข้อผิดพลาดที่เป็นรูปธรรมด้วย `as-dyn-error` เนื่องจากไม่มี `?` การ
แปลงนี้จึงเขียนอย่างชัดเจน:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**ชนิดและ trait ใช้เนมสเปซเดียวกัน** (เหมือนใน Rust) ภายในโมดูลเดียว `defstruct`/`defenum` และ
trait ใช้ชื่อเดียวกันไม่ได้ และการเขียนชื่อ trait ในตำแหน่งของชนิดจะถูกรายงานว่า
"`error` is a trait, not a type — write `:dyn error`"

ความล้มเหลวที่กู้คืนไม่ได้แสดงด้วย `panic` สำหรับ `panic` และ `catch`/`throw` ดู
[เอกสารอ้างอิงไวยากรณ์](../syntax.md#8-การออกจากการทำงานแบบไม่เป็นเส้นตรง-catch--throw--unwind-protect) สำหรับนโยบายการจัดการข้อผิดพลาด
ดู[บทที่ 9 ของเอกสารเดียวกัน](../syntax.md#9-นโยบายการจัดการข้อผิดพลาด)
