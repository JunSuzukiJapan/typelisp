<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# การจัดการข้อผิดพลาด

การจัดการข้อผิดพลาดใน typelisp แบ่งความล้มเหลวเป็นสองประเภท

| ประเภทของความล้มเหลว | ตัวอย่าง | วิธีแสดง |
|---|---|---|
| ความล้มเหลวที่เกิดขึ้นได้ (กู้คืนได้) | ไม่พบไฟล์, อินพุตไม่ใช่ตัวเลข | คืน `Result<T,E>` |
| ความผิดพลาดในโปรแกรม (กู้คืนไม่ได้) | ดัชนีเกินช่วง, `unwrap` ของ `none`, การหารด้วยศูนย์ | หยุดด้วย `panic` |

นอกจากนี้ยังมี `catch` / `throw` ที่ออกจากการเรียกฟังก์ชันหลายชั้นในคราวเดียว และ
`unwind-protect` ที่รันการล้างทรัพยากรไม่ว่าตัวเนื้อหาจะออกด้วยวิธีใด บทนี้สมมติว่าคุณอ่าน
หัวข้อ `Result` ของ[พื้นฐานของชนิด](types.md)แล้ว

## 1. คืน `Result` และรับด้วย `match`

นี่คือฟังก์ชันที่อ่านหมายเลขพอร์ตจากสตริง มันล้มเหลวได้สองแบบ: อินพุตไม่ใช่
ตัวเลข หรือเกินช่วง

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

ผู้เรียกแยกความสำเร็จออกจากความล้มเหลวด้วย `match`

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- ค่าของฟังก์ชันที่คืน `Result` ใช้ไม่ได้เว้นแต่ `match` จัดการกรณี `err`
  การลืมจัดการความล้มเหลวเป็นข้อผิดพลาดของชนิด
- ข้อผิดพลาดจาก `parse-int` เป็นค่าชนิด `ParseIntError` `(message e)` ให้สตริงข้อความของมัน

## 2. การส่งความล้มเหลวขึ้นไปให้ผู้เรียก

ไม่มีทางลัดอย่าง `?` ของ Rust เมื่อเรียกฟังก์ชันที่คืน `Result` หลายตัวต่อกัน
ให้เขียนส่วน "คืนความล้มเหลวตามที่เป็น" ด้วย `match`

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

เมื่อรู้ว่าการดำเนินการไม่มีทางล้มเหลว หรือในสคริปต์เล็ก ๆ ที่หยุดเมื่อล้มเหลวได้
`unwrap` จะดึงเนื้อหาออกมา หากค่าเป็น `err` จะ panic หากค่าเริ่มต้นก็เพียงพอ
ให้ใช้ `unwrap-or`

## 3. การสร้างชนิดข้อผิดพลาดของคุณเอง

การแสดงข้อผิดพลาดเป็นชนิดแทนที่จะเป็นสตริงทำให้ผู้เรียกแตกกิ่งตามประเภทของข้อผิดพลาดได้
ชนิดข้อผิดพลาดคือ `defenum` หรือ `defstruct` ธรรมดาที่ implement trait `Error`

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` คืนคำอธิบายของข้อผิดพลาด
- `source` คืนข้อผิดพลาดอื่นที่ทำให้เกิดข้อผิดพลาดนี้ หากไม่มีสาเหตุ จะเป็น `none`

## 4. การรวมข้อผิดพลาดหลายประเภท

หากฟังก์ชันหนึ่งเรียกทั้ง `parse-int` (`ParseIntError`) และ `check-workers` (`config-error`) จะมี
ชนิดข้อผิดพลาดสองชนิด และทั้งสองเป็น `E` ของ `Result<T,E>` เดียวกันไม่ได้ ในกรณีนั้นให้ทำให้ `E` เป็น
`:dyn Error` (ข้อผิดพลาดชนิดใดก็ได้ที่ implement `Error`) แปลงข้อผิดพลาดแต่ละตัวด้วย
`as-dyn-error`

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

เมื่อให้ `"4"`, `"-1"` และ `"abc"` ผลลัพธ์คือ:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

สำหรับ `:dyn` ดูหัวข้อ 5 ของ [Trait](traits.md)

## 5. `panic`: ความผิดพลาดในโปรแกรม

เมื่อโปรแกรมไปถึงสถานะที่ต้องไม่เกิดขึ้น ให้หยุดมันด้วย `panic`

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- ชนิดของ `panic` คือ `!` (ไม่คืนค่า) จึงเขียนได้ทุกที่ที่คาดหวังชนิดใดก็ได้ นั่นคือเหตุผล
  ที่สองกิ่งของ `if` ข้างต้นเข้ากันได้
- การดำเนินการเหล่านี้ก็ panic เช่นกัน: `unwrap` ของ `none` หรือ `err`, `get` ที่ดัชนีเกินช่วง
  และการหารจำนวนเต็มด้วยศูนย์
- `panic` หยุดโปรแกรม แม้เกิดขึ้นภายใน task ทั้งโปรแกรมก็หยุด
- ใน REPL `panic` ไม่ได้จบ REPL แต่รออินพุตถัดไป
- คุณเขียน `(todo)` สำหรับ "ยังไม่ได้เขียน" และ `(unreachable)` สำหรับ "จุดนี้ไม่ควรไปถึง"
  ทั้งสองอย่าง panic

`panic` ไม่ใช่สิ่งทดแทน `Result` สำหรับความล้มเหลวที่เกิดขึ้นได้ เช่น อินพุตของผู้ใช้หรือ
การมีอยู่ของไฟล์ ให้ใช้ `Result`

## 6. `catch` / `throw`: การกระโดดออกข้ามฟังก์ชัน

`throw` กระโดดตรงออกไปยัง `catch` ที่ห่อหุ้มและมีแท็กเดียวกัน ไม่ว่าจะมีการเรียกฟังก์ชันคั่นกลางกี่ชั้น

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

หาก `v` ไม่มีจำนวนลบ `validate` คืน `"all fine"` หากมี `-7` การควบคุมจะกระโดดจากภายใน
`check-all` ออกไปยัง `catch` ซึ่งคืน `"negative: -7"`

- เขียนแท็กเป็นสัญลักษณ์ธรรมดา เช่น `'bad-input`
- **แต่ละแท็กพาค่าชนิดเดียวเท่านั้น** ในตัวอย่างข้างต้น `'bad-input` พา
  `string` ดังนั้นการโยน `int` ด้วยแท็กเดียวกันเป็นข้อผิดพลาดของชนิด ชนิดของตัวเนื้อหา `catch`
  ต้องตรงกับชนิดของแท็กด้วย

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` ที่ไม่มี `catch` แท็กเดียวกันให้ไปถึงเป็นข้อผิดพลาด

หากต้องการเพียงออกก่อนกำหนดจากภายในฟังก์ชัน ให้ใช้ `return-from` แทน `catch` /
`throw` `return-from` ข้ามฟังก์ชันไม่ได้ แต่แลกกับการที่คุณบอกได้ว่ามันกลับไปที่ไหนโดย
อ่านซอร์ส

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: ล้างทรัพยากรเสมอ

`(unwind-protect body cleanup)` รัน cleanup ไม่ว่าตัวเนื้อหาจะออกด้วยวิธีใด: เมื่อจบ
ตามปกติ เมื่อออกด้วย `throw` และเมื่อ panic

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

ใช้กับสิ่งอย่าง "ปิดไฟล์ที่เปิดไว้เสมอ" หรือ "ปล่อยล็อกที่ถือไว้เสมอ" `with-open-file` และ
`with-lock` ของไลบรารีมาตรฐานใช้ `unwind-protect` ภายใน

## 8. เกี่ยวกับระบบ condition ของ Common Lisp

typelisp ไม่ใช้ระบบ condition ของ Common Lisp (`handler-case`, `restart-case` และอื่น ๆ)
เพราะมันไม่แสดงในชนิดว่าฟังก์ชันก่อให้เกิดความล้มเหลวใดได้บ้าง ซึ่งเข้ากับระบบชนิดแบบสถิต
ได้ไม่ดี ความล้มเหลวที่เกิดขึ้นได้เขียนไว้ในชนิดด้วย `Result` และการถ่ายโอนการควบคุม
ทำด้วย `catch` / `throw`

## 9. อ่านอะไรต่อ

- [การทำงานพร้อมกัน](concurrency.md): task และ channel
- [Option, Result และชนิดข้อผิดพลาด](../reference/functions/option-result.md): รายการฟังก์ชัน
- [ข้อความแสดงข้อผิดพลาด](../reference/errors.md): ความหมายของข้อผิดพลาดที่พบบ่อยและวิธีแก้ไข
