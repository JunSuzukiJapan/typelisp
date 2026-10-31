<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
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
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implement `Iter` ผ่าน `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` ตามลำดับ (รับตัววนซ้ำด้วย `(iter collection)`)
`Chan<T>` เองเป็น `Iter` (`recv` ทำหน้าที่ของ `next`; [Channel](concurrency.md#2-chant--channel))
ลิสต์ `Sexpr` ไม่ implement `Iter` (ชนิดของสมาชิกไม่สม่ำเสมอ) หากคุณ implement `Iter`
ให้ชนิดของคุณเอง จะเดินผ่านด้วย `doiter` ได้ทันที และส่งให้
[ฟังก์ชันของลำดับ](sequences.md#4-ฟังก์ชันของลำดับบน-iter)ได้

## 2. `Eq` / `Ord` (การเปรียบเทียบ)

สอดคล้องกับ `PartialEq`/`PartialOrd` ของ Rust (ตั้งชื่อเป็น `Eq`/`Ord`) ใช้ใน `:where`
bound ของฟังก์ชัน generic เพื่อกำหนดให้ชนิดของสมาชิกเปรียบเทียบได้ (`sort`/`member`/`assoc`
และอื่น ๆ)

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; must be implemented
  (not-equals (self (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less (self (other Self)) bool)                  ; must be implemented
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

หากต้อง implement `Eq` คุณเขียนเฉพาะ `equals` และสำหรับ `Ord` เฉพาะ `less` การ implement
เริ่มต้นเติมส่วนที่เหลือ `Ord` สืบทอดจาก `Eq` ดังนั้นต้องมี `impl Eq X` ก่อน `impl Ord X`

เมทอดของ trait แต่ละตัวเรียกเป็นฟังก์ชันได้ทันที (ภายใน bound `where (Eq A)`/`(Ord A)` หรือบน
ชนิดที่เป็นรูปธรรมที่ implement มัน):

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | เท่ากันหรือไม่ (`==` ของ Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | ไม่เท่ากันหรือไม่ (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` ถูก implement สำหรับ: ชนิดตัวเลขทั้งหมด (`i8` ถึง `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq` คือเอกลักษณ์ (identity) ใช้โดยรูปแบบค่าของ
`match`) และ `cons-cell<A,B>` (แบบเวียนเกิด เมื่อสมาชิกเป็น `Eq`) `Ord` ถูก implement สำหรับ:
ชนิดตัวเลขทั้งหมด, `char` `string` และ `cons-cell<A,B>` (ตามลำดับพจนานุกรม เมื่อสมาชิกเป็น
`Ord`)

ชื่อเมทอดไม่ซ้ำกับตัวดำเนินการที่มีให้ในตัว (`= /= < <= > >=`) หรือ `eq`/`lt` เพราะ
ของที่มีให้ในตัวนิยามใหม่ไม่ได้ และแต่ละ implementation มอบหมายให้พวกมัน ตัวดำเนินการเปรียบเทียบ
สเกลาร์เองเป็นเมทอดที่มีให้ในตัวของแต่ละชนิดตัวรับ ([ตัวเลข](numbers.md)
[สตริงและอักขระ](collections.md)) ภายใน bound การเขียนตัวดำเนินการจะถูกอ่านเป็น
เมทอดของ trait (บทที่ 3)

## 3. trait การคำนวณ (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

ชั้นสำหรับให้โค้ด generic กำหนดว่าต้องเป็น "ชนิดที่บวกได้" **การคำนวณบนชนิดที่เป็นรูปธรรมใช้
ตัวดำเนินการที่มีให้ในตัว** ([ตัวเลข](numbers.md)) และไม่ผ่านชั้นนี้

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; ไม่มีเมธอด เป็นการรวมห้าตัว
```

**ภายใน bound คุณเขียนตัวดำเนินการได้** เมื่อตัวรับเป็นตัวแปรชนิดที่ถูกผูกด้วย `:where`
ตัวดำเนินการจะถูกอ่านเป็นเมทอดของ trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

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
  (push (self (x Item)) ()))
```

คอลเลกชันที่เพิ่มสมาชิกได้ทีละตัว ฟังก์ชัน generic ขอ "ชนิดที่เพิ่ม `T` ได้" ด้วย `(:where (Push C (Item T)))` แล้วเพิ่มลงในคอลเลกชันแบบนั้นใดก็ได้ด้วย `(push c x)` เดียวกัน

| ชนิด | สิ่งที่ `push` ทำ |
|---|---|
| `Vector<T>` | เพิ่มต่อท้าย |
| `Deque<T>` | เพิ่มต่อท้าย (`push-back`) |
| `Array<T>` | เพิ่มที่ตำแหน่ง fill pointer (`push-extend`) panic หากไม่มี fill pointer |
| `HashSet<T>` | ใส่ลงในเซต (`insert`) เป็น `Push` เฉพาะเมื่อชนิดของสมาชิก implement `Hash` |

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
  (pop (self) Option<Item>))
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
