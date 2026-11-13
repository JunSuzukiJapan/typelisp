<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Trait มาตรฐาน

trait สำหรับการวนซ้ำ การเปรียบเทียบ การคำนวณ และการเพิ่มและนำสมาชิกออก trait มาตรฐานอื่นอยู่ในบทของตัวเอง:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([ชนิดข้อผิดพลาด](option-result.md#3-ชนิดข้อผิดพลาดและ-trait-error)), `print-object`
([การพิมพ์](printing.md#5-print-object-การแสดงผลของแต่ละชนิด)) และ trait ของสตรีมกับ
`Pathish` ([สตรีมและไฟล์](streams-files.md)) ชนิดใด implement trait ใดอยู่ใน
[ชนิด](../types.md) วิธีนิยาม trait อยู่ใน
[เอกสารอ้างอิงไวยากรณ์](../syntax.md#39-deftrait--impl--trait)

## 1. trait `Iter` และการวนซ้ำ

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implement `Iter` ผ่าน `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` ตามลำดับ (รับตัววนซ้ำด้วย `(iter collection)`)
`Chan<T>` เองเป็น `Iter` (`recv` ทำหน้าที่ของ `next`; [Channel](concurrency.md#2-chant--channel))
ลิสต์ `Sexpr` ไม่ implement `Iter` (ชนิดของสมาชิกไม่สม่ำเสมอ) หากคุณ implement `Iter`
ให้ชนิดของคุณเอง จะเดินผ่านด้วย `doiter` ได้ทันที และส่งให้
[ฟังก์ชันของลำดับ](sequences.md#4-ฟังก์ชันของลำดับบน-iter)ได้

## 2. `Equal` / `Ord` (การเปรียบเทียบ)

`Equal` คือ trait ที่ `equal` ใช้ ตรงกับ `PartialEq` ของ Rust ส่วน `Ord` ตรงกับ `PartialOrd` ของ Rust
ใช้ในขอบเขต `:where` ของฟังก์ชันแบบ generic เพื่อกำหนดว่าชนิดของสมาชิกต้องเปรียบเทียบได้

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; ต้อง implement
(deftrait Ord (Equal)                                     ; สืบทอด Equal
  (defmethod less (self (other Self)) bool)                  ; ต้อง implement
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

การ implement `Equal` ให้เขียน `equal` ส่วน `Ord` เขียนแค่ `less` ส่วนที่เหลือของ `Ord` มาจาก implementation
ปริยาย `Ord` สืบทอด `Equal` จึงต้องมี `impl Equal X` ก่อน `impl Ord X`

`(equal a b)` เรียก implementation ของ `Equal` ของชนิดของ `a` ถ้าเขียนกับชนิดที่ไม่ได้ implement `Equal`
จะเป็นข้อผิดพลาดของชนิด ถ้าจะเปรียบเทียบตามเอกลักษณ์ให้ใช้ `eq`/`eql` ถ้าตามโครงสร้างให้ใช้ `equalp`
ทั้งสามใช้กับค่าชนิดใดก็ได้

เมธอดของ trait แต่ละตัวเรียกเป็นฟังก์ชันได้โดยตรง (ภายในขอบเขต `where (Equal A)`/`(Ord A)` หรือกับชนิดจริงที่
implement ไว้แล้ว):

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | เท่ากันหรือไม่ (`==` ของ Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` ถูก implement สำหรับ: ชนิดตัวเลขทั้งหมด (`i8` ถึง `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` และ tuple (ทีละสมาชิก เมื่อสมาชิกเป็น `Equal`)
และ `Option<T>` กับ `Result<T,E>` (เมื่อเป็น variant เดียวกันและเนื้อหาเป็น `equal`) สำหรับชนิดในตัว
เมธอดในตัว `equal` ของชนิดนั้นเองคือ implementation ของ `Equal` ตามที่เป็น ตัวเลขเปรียบเทียบด้วย `=` สตริง
เปรียบเทียบตามเนื้อหาโดยแยกตัวพิมพ์ใหญ่เล็ก `Ord` ถูก implement สำหรับ: ชนิดตัวเลขทั้งหมด, `char` `string`
และ `cons-cell<A,B>` (ตามลำดับพจนานุกรม เมื่อสมาชิกเป็น `Ord`)

`equal` ของลิสต์ (`Sexpr`) คือ `equal` ของ CL: cons เปรียบเทียบตามโครงสร้าง สตริงตามเนื้อหา และ atom อื่น
ด้วย `eql` การเปรียบเทียบนี้ไล่ค่าตอนรันและไม่เรียกเมธอดใด ดังนั้น struct ที่อยู่ในลิสต์จึงเปรียบเทียบตาม
เอกลักษณ์เหมือนใน CL ไม่ว่าชนิดของมันจะมี `impl Equal` แบบใด

ชื่อเมธอดของ `Ord` ไม่ซ้ำกับตัวดำเนินการในตัว (`< <= > >=`) หรือ `lt` เพราะของในตัวนิยามใหม่ไม่ได้และแต่ละ
implementation ส่งต่อไปยังพวกมัน ตัวดำเนินการเปรียบเทียบสเกลาร์เองเป็นเมธอดในตัวตามชนิดของผู้รับ
([ตัวเลข](numbers.md), [สตริงและอักขระ](collections.md)) ภายในขอบเขต ตัวดำเนินการที่เขียนจะถูกอ่านเป็นเมธอด
ของ trait (บทที่ 3) `=` กลายเป็น `equal` และ `/=` กลายเป็น `(not (equal a b))`

## 3. trait การคำนวณ (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

ชั้นสำหรับให้โค้ด generic กำหนดว่าต้องเป็น "ชนิดที่บวกได้" **การคำนวณบนชนิดที่เป็นรูปธรรมใช้
ตัวดำเนินการที่มีให้ในตัว** ([ตัวเลข](numbers.md)) และไม่ผ่านชั้นนี้

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; ไม่มีเมธอด เป็นการรวมห้าตัว
```

**ภายใน bound คุณเขียนตัวดำเนินการได้** เมื่อตัวรับเป็นตัวแปรชนิดที่ถูกผูกด้วย `:where`
ตัวดำเนินการจะถูกอ่านเป็นเมทอดของ trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

เมทอดของ trait ไม่ได้ชื่อ `+` เพราะ `+` เป็นชื่อของเมทอดที่มีให้ในตัว และ `impl` ปฏิเสธที่จะ
นิยามใหม่ (`cannot redefine built-in method`) ไม่มี `Neg`: `(- x)` ขยายเป็น
`(- (- x x) x)` ดังนั้น `Sub` ก็เพียงพอ

ถูก implement สำหรับ: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` บนชนิดตัวเลขทั้งหมด (ยกเว้น `complex`) และ
`Bits` บนชนิดจำนวนเต็มทั้งหมดและ `int`


**`Div` ระบุชนิดของผลลัพธ์ด้วยชนิดที่เกี่ยวข้อง `Output`** เหมือน CL `/` ระหว่างจำนวนเต็มสองตัวเป็น
`rational` ดังนั้น `Output` ของ `int` คือ `rational` ส่วนชนิดตัวเลขอื่นคือชนิดของตัวเอง เพราะ `Output`
ต่างกันตามชนิด `Number` จึงไม่รวม `Div` (รายการ supertrait ต้องตรึงชนิดที่เกี่ยวข้องทุกตัว)
ฟังก์ชันเจเนอริกที่หารจะรับชนิดผลลัพธ์ด้วย `(:where (Div T (Output R)))` หรือขอการหารที่ปิดใน `T` ด้วย
`(:where (Div T (Output T)))` (กรณีนี้ส่ง `int` ไม่ได้)

## 4. trait `Push` (การเพิ่มสมาชิก)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

คอลเลกชันที่เพิ่มสมาชิกได้ทีละตัว ฟังก์ชัน generic ขอ "ชนิดที่เพิ่ม `T` ได้" ด้วย `(:where (Push C (Item T)))` แล้วเพิ่มลงในคอลเลกชันแบบนั้นใดก็ได้ด้วย `(push c x)` เดียวกัน

| ชนิด | สิ่งที่ `push` ทำ |
|---|---|
| `Vector<T>` | เพิ่มต่อท้าย |
| `Deque<T>` | เพิ่มต่อท้าย (`push-back`) |
| `Array<T>` | เพิ่มที่ตำแหน่ง fill pointer (`push-extend`) panic หากไม่มี fill pointer |
| `HashSet<T>` | ใส่ลงในเซต (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

ชนิดที่เขียนเองซึ่ง implement `Push` ส่งให้ฟังก์ชัน generic เดียวกันได้ และยังเป็น trait object ได้ด้วย เช่น `:dyn Push<int>`

## 5. trait `Pop` (นำสมาชิกออก)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

คอลเลกชันที่นำสมาชิกที่ `push` เพิ่มไว้ล่าสุดกลับออกมาได้ ได้ `none` เมื่อว่าง ฟังก์ชันเจเนอริกขอด้วย `(:where (Pop C (Item T)))`

| ชนิด | สิ่งที่ `pop` ทำ |
|---|---|
| `Vector<T>` | นำสมาชิกตัวสุดท้ายออก |
| `Deque<T>` | นำสมาชิกตัวสุดท้ายออก (`pop-back`) |
| `Array<T>` | นำสมาชิกที่อยู่ก่อน fill pointer ออก ถ้าไม่มี fill pointer จะ panic |

`HashSet<T>` ไม่มี "สมาชิกตัวสุดท้าย" จึงเป็น `Push` แต่ไม่เป็น `Pop`

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

เช่นเดียวกับ `Push` ชนิดของคุณเองก็ implement ได้ และเป็น trait object ได้ เช่น `:dyn Pop<int>`
