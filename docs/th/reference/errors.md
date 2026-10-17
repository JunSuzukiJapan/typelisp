<!-- translated-from: docs/ja/reference/errors.md @ 919605e85abc17d3889701183c762718842ab3b4 -->
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
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | ฟังก์ชันต้องการ trait ที่ชนิดที่คุณส่งไม่ได้ implement ให้เขียน `(impl Eq pt ...)` ([Trait มาตรฐาน](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | ค่าของชนิดที่ไม่ได้ implement trait ถูกส่งในที่ที่คาดหวัง `:dyn` ให้เขียน `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | ชื่อ trait ถูกเขียนในที่ที่ต้องเป็นชนิด ให้เขียน `:dyn Error` |
| ``if: (if cond then else)`` | `if` มีรูปร่างผิด `if` ต้องมีกิ่ง else เมื่อไม่ต้องการ ให้ใช้ `when` |

### 2.2 ชื่อ

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `no such function: bar` | ไม่มีฟังก์ชันหรือเมทอดชื่อนั้น ตรวจสอบการสะกด |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | เมทอดถูกเลือกโดยชนิดของอาร์กิวเมนต์แรก มีเมทอดชื่อนั้นอยู่ แต่ไม่ใช่สำหรับชนิดของอาร์กิวเมนต์แรก (ในที่นี้คือ `int`) ท้ายข้อความแสดงรายการชนิดที่มีเมทอดนั้น (ถ้าเป็นเมทอดที่เขียนใน `impl` ของชนิดเฉพาะ ก็เป็นชนิดเฉพาะอย่าง `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | เมธอดที่ส่งเป็นค่าจะถูกเลือกตามชนิดของพารามิเตอร์แรกของชนิดฟังก์ชันที่มันถูกส่งไปเป็น ชนิดนั้น (ในที่นี้คือ `box<string>`) ไม่มีเมธอดชื่อนั้น ท้ายข้อความแสดงรายการชนิดที่มีเมธอดนั้น |
| `unbound variable: y` | ไม่มีตัวแปรชื่อนั้น ตรวจสอบการสะกดและขอบเขตของการผูก (ใช้นอก `let` ของมันหรือไม่) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | ในโมดูลเดียว ตั้งชื่อเดียวกันให้สองสิ่งจากฟังก์ชัน ตัวแปรโกลบอล และแมโคร ให้เปลี่ยนชื่อหนึ่งในนั้น ([เอกสารอ้างอิงไวยากรณ์ 3.10](syntax.md#310-module--use--เนมสเปซ)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | ไม่มีการสร้าง `new` ให้ struct โดยอัตโนมัติ ให้สร้างค่าด้วย `make-instance` หรือเขียน `new` ใน `impl` ([เอกสารอ้างอิงไวยากรณ์ 3.5](syntax.md#35-impl--เมทอดของชนิด)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | ไม่มี `defmethod` เมทอดเขียนใน `impl` ของชนิด ([เอกสารอ้างอิงไวยากรณ์ 3.5](syntax.md#35-impl--เมทอดของชนิด)) |
| ``defstruct: (:constructor ...) is gone — ...`` | ไม่มีตัวเลือก `:constructor` ตัวสร้างเขียนใน `impl` เป็นเมทอดสถิต |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | เขียนเมทอดชื่อเดียวกันทั้งใน `impl` ที่เขียนพารามิเตอร์ชนิดเป็นตัวแปร (`Box<T>`) และใน `impl` ของชนิดเฉพาะ (`Box<int>`) (ไม่ขึ้นกับลำดับ) ให้เหลือเพียงฝั่งเดียว ([เอกสารอ้างอิงไวยากรณ์ 3.5](syntax.md#35-impl--เมทอดของชนิด)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | implement trait เดียวกันให้ทั้ง `Box<T>` และ `Box<int>` ให้เหลือเพียงฝั่งเดียว |
| ``use: unresolved `nosuch` `` | หาโมดูลที่ระบุใน `use` ไม่พบ สำหรับการที่ชื่อไฟล์สอดคล้องกับพาธของโมดูล ดู[เอกสารอ้างอิงไวยากรณ์ 3.11](syntax.md#311-ไฟล์และโมดูล-โปรเจกต์หลายไฟล์) |
| `unresolved path: c::hidden` | มีโมดูลอยู่ แต่ไม่มีชื่อนั้น หรือมองไม่เห็นเพราะไม่มี `pub` |
| `circular module dependency: a -> b -> a` | โมดูล `use` กันเอง ให้ย้ายส่วนที่ใช้ร่วมกันไปไว้ในโมดูลแยก |
| ``return-from: no enclosing block named `nope` `` | ไม่มี `block` ที่ชื่อตรงกับที่ให้ `return-from` ห่อหุ้มมันอยู่ block ของฟังก์ชันใช้ได้เฉพาะภายในฟังก์ชันนั้น |

### 2.3 การเรียก

| ข้อความ | ความหมายและวิธีแก้ไข |
|---|---|
| `f: expected 1 argument(s), got 2` | จำนวนอาร์กิวเมนต์ไม่ตรงกัน |
| `f: unknown keyword argument :b` | ส่งอาร์กิวเมนต์แบบคีย์เวิร์ดที่ฟังก์ชันไม่มี |
| `circle: expected 1 field(s), got 2` | จำนวนค่าที่ส่งให้ variant ของ enum ไม่ตรงกับจำนวนฟิลด์ของ variant นั้น |
| ``make-instance point: slot `y` is not given and has no default`` | ไม่ได้ให้ฟิลด์ที่ไม่มีค่าเริ่มต้นแก่ `make-instance` |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | ให้ฟิลด์ที่ struct ไม่มี |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | ให้ฟิลด์ที่ไม่ใช่ `pub` นอกโมดูลที่นิยามชนิด ให้โมดูลนั้นเตรียมตัวสร้างที่เป็น `pub` |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | `impl` ของหลายชนิดเฉพาะมีเมทอด static ชื่อนั้น และไม่มีอะไรตัดสินว่าจะเรียกตัวไหน ให้เขียนชนิดที่คาดหวัง เช่น `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | ส่งเมทอดที่เขียนแยกตามชนิดเฉพาะให้ `compile`, `trace`, `documentation` ฯลฯ ด้วยชื่อชนิดอย่างเดียว ให้เขียนชนิดเฉพาะ เช่น `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | กำหนดค่าให้ชื่อที่นิยามด้วย `defconstant` หากต้องเปลี่ยนค่า ให้ใช้ `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | ฟังก์ชันที่ประกาศด้วย `defsignature` ไม่ได้ถูกนิยาม |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | การเรียก `(nosuch arg colon at)` ที่เขียน ณ ตำแหน่งนั้นแก้ไขไม่ได้ ให้นิยามเมทอดของชนิดอาร์กิวเมนต์หรือฟังก์ชัน ([คำสั่งรูปแบบ บทที่ 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | เขียน `~/name/` ในที่ที่ได้รับสมาชิกของลิสต์ เช่นภายใน `~{` ชนิดของสมาชิกกำหนดสิ่งที่จะเรียกไม่ได้ |

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
