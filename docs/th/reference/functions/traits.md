<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait มาตรฐาน

trait สำหรับการวนซ้ำ การเปรียบเทียบ และการคำนวณ trait มาตรฐานอื่นอยู่ในบทของตัวเอง:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([ชนิดข้อผิดพลาด](option-result.md#3-ชนิดข้อผิดพลาดและ-trait-error)), `print-object`
([การพิมพ์](printing.md#5-print-object-การแสดงผลของแต่ละชนิด)) และ trait ของสตรีมกับ
`Pathish` ([สตรีมและไฟล์](streams-files.md)) ชนิดใด implement trait ใดอยู่ใน
[ชนิด](../types.md) วิธีนิยาม trait อยู่ใน
[เอกสารอ้างอิงไวยากรณ์](../syntax.md#39-deftrait--impl--trait)

## 1. trait `Iter` และการวนซ้ำ

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implement `Iter` ผ่าน `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` ตามลำดับ (รับตัววนซ้ำด้วย `(iter collection)`)
`Chan<T>` เองเป็น `Iter` (`recv` ทำหน้าที่ของ `next`; [Channel](concurrency.md#2-chant--channel))
ลิสต์ `Sexpr` ไม่ implement `Iter` (ชนิดของสมาชิกไม่สม่ำเสมอ) หากคุณ implement `Iter`
ให้ชนิดของคุณเอง จะเดินผ่านด้วย `doiter` ได้ทันที และส่งให้
[ฟังก์ชันของลำดับ](sequences.md#4-ฟังก์ชันของลำดับบน-iter)ได้

## 2. `Eq` / `Ord` (การเปรียบเทียบ)

สอดคล้องกับ `PartialEq`/`PartialOrd` ของ Rust (ตั้งชื่อเป็น `Eq`/`Ord`) ใช้ใน `where`
bound ของฟังก์ชัน generic เพื่อกำหนดให้ชนิดของสมาชิกเปรียบเทียบได้ (`sort`/`member`/`assoc`
และอื่น ๆ)

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
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
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**ภายใน bound คุณเขียนตัวดำเนินการได้** เมื่อตัวรับเป็นตัวแปรชนิดที่ถูกผูกด้วย `where`
ตัวดำเนินการจะถูกอ่านเป็นเมทอดของ trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

เมทอดของ trait ไม่ได้ชื่อ `+` เพราะ `+` เป็นชื่อของเมทอดที่มีให้ในตัว และ `impl` ปฏิเสธที่จะ
นิยามใหม่ (`cannot redefine built-in method`) ไม่มี `Neg`: `(- x)` ขยายเป็น
`(- (- x x) x)` ดังนั้น `Sub` ก็เพียงพอ

ถูก implement สำหรับ: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` บนชนิดตัวเลขทั้งหมด (ยกเว้น `complex`) และ
`Bits` บนชนิดจำนวนเต็มทั้งหมดและ `int`
