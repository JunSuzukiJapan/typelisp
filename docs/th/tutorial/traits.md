<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait

trait คือคำมั่นสัญญาว่า "ชนิดนี้รองรับการดำเนินการเหล่านี้" trait ทำให้หลายชนิดใช้การดำเนินการ
ชื่อเดียวกันร่วมกันได้ ฟังก์ชันที่ใช้สิ่งเหล่านั้นจึงไม่ต้องเขียนซ้ำสำหรับแต่ละชนิด
trait ทำงานแทบเหมือนกับ trait ของ Rust ทุกประการ บทนี้สมมติว่าคุณอ่าน
[พื้นฐานของชนิด](types.md)แล้ว

## 1. การนิยามและการ implement trait

นิยามการดำเนินการที่คืนพื้นที่และชื่อของรูปร่างเป็น trait `Shape`

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` หลังชื่อ trait คือรายการของ trait ที่มันสืบทอด (หัวข้อ 4) ปล่อยว่างไว้
  เมื่อไม่มี
- แต่ละบรรทัดประกาศเมทอดหนึ่งตัว `Self` หมายถึง "ชนิดที่ implement trait นี้"

หากต้องการ implement trait ให้ชนิดหนึ่ง ให้เขียน `impl`

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

เมทอดที่ implement แล้วเรียกได้เหมือนฟังก์ชันธรรมดา

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

การละเมทอดที่ trait ประกาศไว้แม้เพียงตัวเดียวเป็นข้อผิดพลาดของชนิดที่ `impl`

## 2. ขอบเขตของ trait (trait bound): "ชนิดใดก็ได้ที่ implement trait นี้"

คุณกำหนดเงื่อนไขให้พารามิเตอร์ชนิดของฟังก์ชัน generic ได้ด้วย `where` เรียกว่า
**trait bound**

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

เพราะ `(where (Shape T))` ตัวเนื้อหาจึงใช้ `name` และ `area` กับค่าชนิด `T` ได้ หากไม่มี
bound จะไม่ทราบอะไรเกี่ยวกับ `T` จึงเรียกไม่ได้

การส่งชนิดที่ไม่ได้ implement `Shape` เป็นข้อผิดพลาดของชนิดที่จุดเรียก

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

ฟังก์ชัน generic ได้สำเนาของตัวเองสำหรับแต่ละชนิดที่มันถูกเรียก ไม่มีการทดสอบชนิดหรือ
การแตกกิ่งตอนรันเกี่ยวข้อง

## 3. การ implement เริ่มต้น

หากเมทอดของ trait มีตัวเนื้อหา ตัวเนื้อหานั้นจะถูกใช้เมื่อ `impl` ละเมทอดนั้นไว้

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. การ implement trait มาตรฐาน

ไลบรารีมาตรฐานก็มี trait เช่นกัน การ implement trait หนึ่งทำให้ฟังก์ชันมาตรฐานที่ใช้ trait นั้น
ใช้กับชนิดของคุณได้

| Trait | เมทอดที่ต้อง implement | สิ่งที่เปิดใช้ได้ |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, รูปแบบ `(= expr)` ของ `match` และอื่น ๆ |
| `Ord` | `less` | `less-equal`, `greater` และอื่น ๆ `Ord` สืบทอดจาก `Eq` |
| `print-object` | `print-object` | วิธีที่ `println` และพวกเดียวกันแสดงค่า |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` และอื่น ๆ |
| `Error` | `message`, `source` | ใช้เป็นชนิดข้อผิดพลาด ([การจัดการข้อผิดพลาด](errors.md)) |

ลอง implement `Eq` และ `Ord` สำหรับชนิดที่แทนจำนวนเงิน เนื่องจาก `Ord` สืบทอด
จาก `Eq` `impl` ของ `Eq` จึงต้องมาก่อน

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

การ implement `print-object` ตัดสินว่า `println` แสดงค่าอย่างไร อาร์กิวเมนต์ `escape` เป็น `true`
เมื่อขอรูปแบบที่อ่านกลับได้ เช่นเดียวกับ `~s`

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

เมื่อรวมกับ trait bound คุณเขียนฟังก์ชันที่ใช้ได้กับทุกชนิดที่ implement `Ord` ได้

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

เมื่อให้ `Vector` ที่มีค่า `money` 300, 900 และ 100 ตามลำดับ มันคืน `(some 900 yen)`

## 5. `:dyn`: การจัดการค่าต่างชนิดกันรวมกัน

สมาชิกทั้งหมดของ `Vector<T>` มีชนิดเดียวกัน ดังนั้นค่า `circle` และ `rect` ใส่ใน
`Vector<circle>` เดียวกันไม่ได้ หากต้องการจัดการ "สิ่งที่ implement `Shape`" รวมกัน ให้ใช้ชนิด
`:dyn Shape`

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- ค่า `circle` หรือ `rect` ที่วางในที่ที่คาดหวัง `:dyn Shape` จะถูกแปลงโดยอัตโนมัติ
- การเรียก `(area s)` รัน `area` ของชนิดใดนั้นถูกตัดสินตอนรันจากชนิดของสิ่งที่ `s` เก็บอยู่
- การวางค่าที่ชนิดไม่ได้ implement `Shape` ในที่ที่คาดหวัง `:dyn Shape` เป็นข้อผิดพลาดของ
  ชนิด

การเลือกระหว่าง trait bound ในหัวข้อ 2 กับ `:dyn`:

| | Trait bound (`where`) | `:dyn Trait` |
|---|---|---|
| เมื่อเมทอดที่เรียกถูกตัดสิน | ก่อนรัน | ตอนรัน |
| ผสมชนิดใน `Vector` เดียว | ไม่ได้ | ได้ |
| ชนิดที่ใช้ได้ | ไม่มีข้อจำกัด | struct, enum, `int`, `string`, `f64` และอื่น ๆ (ไม่ใช่ `bool`, `char`, `symbol`, `i32` และทำนองเดียวกัน) |

รายการที่แน่นอนของชนิดที่ใช้ได้อยู่ใน
[เอกสารอ้างอิงไวยากรณ์ 3.9](../reference/syntax.md#39-deftrait--impl--trait)

trait บางตัวใช้กับ `:dyn` ไม่ได้: ตัวที่เมทอดใช้ `Self` กับอาร์กิวเมนต์อื่นนอกจาก
`self` หรือกับค่าที่คืน (เช่น `equals` ใน `Eq`) เนื่องจากไม่ทราบชนิดจนกว่าจะถึงตอน
รัน จึงไม่มีทางสร้าง "ค่าที่เป็นชนิดเดียวกัน"

## 6. ข้อจำกัด

- เก็บการนิยาม trait, `impl` ของมัน และโค้ดที่ใช้มันผ่าน `:dyn` ไว้ในโมดูล (ไฟล์) เดียว
  ยังไม่สามารถทำให้ trait มองเห็นได้จากโมดูลอื่น
- ชนิดและ trait ใช้เนมสเปซเดียวกัน ภายในโมดูลเดียว ชนิดกับ trait ใช้ชื่อเดียวกันไม่ได้

## 7. อ่านอะไรต่อ

- [แมโคร](macros.md): การนิยามไวยากรณ์ของคุณเอง
- [เอกสารอ้างอิงไวยากรณ์ 3.9](../reference/syntax.md#39-deftrait--impl--trait): blanket implementation,
  associated type และอื่น ๆ
- [Trait มาตรฐาน](../reference/functions/traits.md): รายการ trait ในไลบรารีมาตรฐาน
