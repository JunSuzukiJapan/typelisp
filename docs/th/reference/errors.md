<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# ข้อความแสดงข้อผิดพลาด

ความหมายของข้อความแสดงข้อผิดพลาดหลักจาก `typl` และวิธีแก้ไข

## 1. การอ่านข้อผิดพลาด

ข้อผิดพลาดถูกเขียนไปยังข้อผิดพลาดมาตรฐาน (standard error) ในรูปแบบนี้:

```text
error: file:line:column: kind: message
```

`kind` บอกว่าพบข้อผิดพลาดเมื่อใด

| Kind | เมื่อใด | ความหมาย |
|---|---|---|
| `type error` | ก่อนรัน (ตอนตรวจสอบ) | ความผิดพลาดของชนิดหรือชื่อ ฟอร์มนั้นจะไม่ถูกรัน |
| (ไม่มี kind) | ตอนอ่านหรือตรวจสอบ | ความผิดพลาดของไวยากรณ์ เช่น วงเล็บไม่สมดุล หรือหาชื่อไม่พบ |
| `panic` | ขณะรัน | ความล้มเหลวที่กู้คืนไม่ได้ โปรแกรมหยุดหลังจากรันการล้างทรัพยากรของ `unwind-protect` |

บรรทัดที่ขึ้นต้นด้วย `warning:` เป็นคำเตือน และการประมวลผลดำเนินต่อไป

`file:line:column` ชี้ไปที่นิพจน์ที่ผิดพลาด สำหรับข้อผิดพลาดตอนรันที่เกิดขึ้น
ภายในฟังก์ชันของไลบรารีมาตรฐาน จะชี้ไปที่ตำแหน่งที่โปรแกรมเรียกฟังก์ชันนั้น
ข้อผิดพลาดบางอย่างไม่มีตำแหน่ง (เช่น `error: panic: ...`)

ตัวอย่าง:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

หมายความว่านิพจน์ที่บรรทัด 1 คอลัมน์ 24 ของ `main.typl` เป็น `string` ในที่ที่คาดหวัง `i32`

## 2. ข้อผิดพลาดตอนตรวจสอบ

ความผิดพลาดที่พบก่อนรัน ฟอร์มจะไม่ถูกรันจนกว่าจะแก้ไข

### 2.1 ชนิด

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | นิพจน์ชนิด `U` อยู่ในที่ที่ต้องการชนิด `T` ไม่มีการแปลงโดยปริยาย สำหรับตัวเลข ให้แปลงด้วย `(as T x)` `int` และ `i32` ก็เป็นคนละชนิดเช่นกัน |
| ``integer literal 300 is out of range for u8 (0..=255)`` | literal ไม่พอดีกับชนิด หากต้องการให้ตัดทอน ให้เขียน `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | ไม่มีชนิดชื่อนั้น ให้นิยามชนิดก่อนฟอร์มแรกที่ใช้มัน (ชนิดไม่มีการประกาศล่วงหน้า) หากหมายถึงตัวแปรชนิด ให้เขียนในตำแหน่งที่ประกาศ เช่น `<foo>` หลังชื่อฟังก์ชัน ([เอกสารอ้างอิงไวยากรณ์ 3.6](syntax.md#36-defstruct--struct-ชนิดที่ผู้ใช้นิยาม)) |
| ``cannot infer type argument `t` for `vector::new` `` | ไม่สามารถกำหนดอาร์กิวเมนต์ชนิดได้ ให้เขียนชนิดด้วย `the` เช่น `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` ไม่จัดการทุก variant ให้เพิ่มกิ่งสำหรับ variant ที่ขาด หรือกิ่ง `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | ฟังก์ชันต้องการ trait ที่ชนิดที่คุณส่งไม่ได้ implement ให้เขียน `(impl Eq pt ...)` ([Trait มาตรฐาน](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | ค่าของชนิดที่ไม่ได้ implement trait ถูกส่งในที่ที่คาดหวัง `:dyn` ให้เขียน `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | ชื่อ trait ถูกเขียนในที่ที่ต้องเป็นชนิด ให้เขียน `:dyn Error` |
| ``if: (if cond then else)`` | `if` มีรูปร่างผิด `if` ต้องมีกิ่ง else เมื่อไม่ต้องการ ให้ใช้ `when` |

### 2.2 ชื่อ

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `no such function: bar` | ไม่มีฟังก์ชันหรือเมทอดชื่อนั้น ตรวจสอบการสะกด |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | เมทอดถูกเลือกโดยชนิดของอาร์กิวเมนต์แรก มีเมทอดชื่อนั้นอยู่ แต่ไม่ใช่สำหรับชนิดของอาร์กิวเมนต์แรก (ในที่นี้คือ `int`) ท้ายข้อความแสดงรายการชนิดที่มีเมทอดนั้น |
| `unbound variable: y` | ไม่มีตัวแปรชื่อนั้น ตรวจสอบการสะกดและขอบเขตของการผูก (ใช้นอก `let` ของมันหรือไม่) |
| ``use: unresolved `nosuch` `` | หาโมดูลที่ระบุใน `use` ไม่พบ สำหรับการที่ชื่อไฟล์สอดคล้องกับพาธของโมดูล ดู[เอกสารอ้างอิงไวยากรณ์ 3.11](syntax.md#311-ไฟล์และโมดูล-โปรเจกต์หลายไฟล์) |
| `unresolved path: c::hidden` | มีโมดูลอยู่ แต่ไม่มีชื่อนั้น หรือมองไม่เห็นเพราะไม่มี `pub` |
| `circular module dependency: a -> b -> a` | โมดูล `use` กันเอง ให้ย้ายส่วนที่ใช้ร่วมกันไปไว้ในโมดูลแยก |
| ``return-from: no enclosing block named `nope` `` | ไม่มี `block` ที่ชื่อตรงกับที่ให้ `return-from` ห่อหุ้มมันอยู่ block ของฟังก์ชันใช้ได้เฉพาะภายในฟังก์ชันนั้น |

### 2.3 การเรียก

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `f: expected 1 argument(s), got 2` | จำนวนอาร์กิวเมนต์ไม่ตรงกัน |
| `f: unknown keyword argument :b` | ส่งอาร์กิวเมนต์แบบคีย์เวิร์ดที่ฟังก์ชันไม่มี |
| `new: expected 1 field(s), got 2` | จำนวนค่าที่ส่งให้ตัวสร้างของ struct ไม่ตรงกับจำนวนฟิลด์ |
| ``setf: cannot assign to constant `k` `` | กำหนดค่าให้ชื่อที่นิยามด้วย `defconstant` หากต้องเปลี่ยนค่า ให้ใช้ `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | ฟังก์ชันที่ประกาศด้วย `defsignature` ไม่ได้ถูกนิยาม |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | ไม่มีชนิดของอาร์กิวเมนต์ใดมีเมทอดที่เรียกด้วย `~/name/` ([คำสั่งรูปแบบ บทที่ 5](functions/format.md#5-name)) |

## 3. ข้อผิดพลาดในการอ่าน

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `unexpected end of input while reading a list` | ขาดวงเล็บปิด ตำแหน่งชี้ไปที่จุดที่การอ่านสิ้นสุด (เช่น ท้ายไฟล์) ดังนั้นให้มองหาวงเล็บเปิด |

## 4. ข้อผิดพลาดตอนรัน (panic)

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `panic: divide by zero` | การหารด้วยศูนย์ด้วยจำนวนเต็มหรือ ratio การหารทศนิยมด้วยศูนย์ไม่ panic แต่ให้ `inf`/`NaN` |
| `panic: unwrap: called on none` | ใช้ `unwrap` กับ `none` ให้จัดการกรณี `none` ด้วย `match` หรือ `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | ดัชนีเกินช่วง ตรวจสอบความยาวด้วย `len` หรือใช้ฟังก์ชันที่คืน `none` เมื่อเกินช่วง (`nth`, `pop` และอื่น ๆ) |
| `panic: an integer argument does not fit a fixnum` | ส่ง `int` ที่ไม่พอดีใน 63 บิตให้อาร์กิวเมนต์ที่รับดัชนีหรือจำนวนนับ |
| `throw: no enclosing (catch 'oops) for this throw` | `throw` รันโดยไม่มี `catch` แท็กเดียวกันห่อหุ้มอยู่ |
| `panic: <message>` | โปรแกรมเรียก `(panic "<message>")` `assert` ที่ล้มเหลวให้ `assertion failed: ...` |

`panic` หยุดทั้งโพรเซสแม้เกิดขึ้นภายใน task
([เอกสารอ้างอิงไวยากรณ์ 12.4](syntax.md#124-ปฏิสัมพันธ์กับความสามารถอื่น)) ความล้มเหลวที่ต้องการกู้คืนให้แสดงด้วย
`Result` ([เอกสารอ้างอิงไวยากรณ์ บทที่ 9](syntax.md#9-นโยบายการจัดการข้อผิดพลาด))

## 5. คำเตือน

| ข้อความ | ความหมาย |
|---|---|
| ``warning: redefining function `f` `` | นิยามฟังก์ชันชื่อเดียวกันอีกครั้ง การนิยามหลังมีผล ปรากฏเป็นปกติเมื่อคุณแก้การนิยามใน REPL |
