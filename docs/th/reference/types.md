<!-- translated-from: docs/ja/reference/types.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# ชนิด

ชนิดที่ typelisp มี และ trait มาตรฐานที่แต่ละชนิด implement วิธีเขียนชนิดอยู่ใน
[เอกสารอ้างอิงไวยากรณ์ บทที่ 2](syntax.md#2-การเขียนชนิด) ฟังก์ชันและเมทอดของแต่ละชนิดอยู่ใน
[ฟังก์ชันที่มีให้ในตัว](functions/README.md)

## 1. ชนิดพื้นฐาน

| ชนิด | เนื้อหา | รายละเอียด |
|---|---|---|
| `int` | จำนวนเต็มความแม่นยำไม่จำกัด เก็บเป็นค่า immediate ตราบที่พอดีใน 63 บิต และกลายเป็น bignum โดยอัตโนมัติเมื่อเกินนั้น เป็นชนิดเริ่มต้นของ literal จำนวนเต็มที่ไม่มีคำอธิบายชนิด | [ตัวเลข บทที่ 3](functions/numbers.md#3-จำนวนเต็มความแม่นยำไม่จำกัด-int) |
| `i8` `i16` `i32` | จำนวนเต็มความกว้างคงที่แบบมีเครื่องหมาย | [ตัวเลข บทที่ 1](functions/numbers.md#1-จำนวนเต็มความกว้างคงที่) |
| `u8` `u16` `u32` | จำนวนเต็มความกว้างคงที่แบบไม่มีเครื่องหมาย | เหมือนข้างบน |
| `f32` `f64` | จำนวนทศนิยมตาม IEEE-754 literal ทศนิยมมีค่าเริ่มต้นเป็น `f64` | [ตัวเลข บทที่ 4](functions/numbers.md#4-จำนวนทศนิยม-f64--f32) |
| `ratio` | จำนวนตรรกยะในรูปต่ำสุด | [ตัวเลข บทที่ 5](functions/numbers.md#5-จำนวนตรรกยะ-ratio) |
| `bool` | `true` / `false` | [ตัวเลข บทที่ 7](functions/numbers.md#7-บูลีน) |
| `char` | Unicode scalar value | [อักขระ](functions/collections.md#2-อักขระ-char) |
| `string` | สตริงที่เปลี่ยนแปลงไม่ได้ | [สตริง](functions/collections.md#1-สตริง-string) |
| `symbol` | สัญลักษณ์ คีย์เวิร์ด (`:name`) ก็มีชนิดนี้ | [สัญลักษณ์](functions/sequences.md#3-สัญลักษณ์) |
| `()` | ชนิด Unit ค่าของมันก็คือ `()` | |
| `!` | ชนิด Never ชนิดของนิพจน์ที่ไม่คืนค่า เช่น `panic` วางได้ในที่ที่คาดหวังชนิดใดก็ได้ | |
| `ptr` `c-long` `c-ulong` | word ที่ใช้เฉพาะสำหรับส่งค่าไปและกลับจาก C เป็นค่าได้เฉพาะภายใน `unsafe` และที่ที่ปรากฏได้ถูกจำกัด | [ตัวเลข บทที่ 2](functions/numbers.md#2-word-ดิบที่ขอบเขตกับ-c-ptr--c-long--c-ulong) |
| `random-state` | สถานะของตัวสร้างจำนวนสุ่ม | [ตัวเลข บทที่ 12](functions/numbers.md#12-จำนวนสุ่ม) |

ไม่มีชนิดจำนวนเต็ม 64 บิต สำหรับจำนวนเต็มที่ความกว้างไม่สำคัญ ให้ใช้ `int`

## 2. ชนิด generic ที่มีให้ในตัว

| ชนิด | เนื้อหา | รายละเอียด |
|---|---|---|
| `Option<T>` | ค่าที่มีอยู่หรือไม่มี `some` / `none` | [Option และ Result](functions/option-result.md) |
| `Result<T,E>` | สำเร็จหรือล้มเหลว `ok` / `err` | เหมือนข้างบน |
| `Vector<T>` | อาร์เรย์ที่ขยายได้ | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | ตารางแฮช ชนิดของคีย์ต้อง implement `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | ทูเพิล (สมาชิก 1 ถึง 12 ตัว) อ่านสมาชิกด้วย `t::0` | [ไวยากรณ์ บทที่ 2](syntax.md#2-การเขียนชนิด) |
| `Task<T>` | handle ของ task | [Task](functions/concurrency.md#1-taskt--handle-ของ-task) |
| `Thread<T>` | handle ของ task ที่รันบนเธรด OS เฉพาะ | [Thread](functions/concurrency.md#7-threadt--เธรด-os-เฉพาะ) |
| `Chan<T>` | channel | [Channel](functions/concurrency.md#2-chant--channel) |

ชนิดฟังก์ชันเขียนเป็น `(fn (ชนิดของอาร์กิวเมนต์...) ชนิดของค่าที่คืน)` และ trait object เขียนเป็น `:dyn Trait`
([เอกสารอ้างอิงไวยากรณ์ บทที่ 2](syntax.md#2-การเขียนชนิด))

## 3. ข้อมูล S-expression

| ชนิด | เนื้อหา | รายละเอียด |
|---|---|---|
| `Sexpr` | S-expression ที่ไม่ว่าง มี 19 variant: `int`, `i8` ถึง `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [ข้อมูล S-expression](functions/sequences.md#2-ข้อมูล-s-expression-sexpr) |
| `Option<Sexpr>` | ข้อมูล S-expression โดยทั่วไป ลิสต์ว่าง `()` คือ `none` | เหมือนข้างบน |

## 4. ชนิดในไลบรารีมาตรฐาน

ชนิดที่ไลบรารีมาตรฐาน (prelude) นิยามด้วย `defstruct` / `defenum` ถูกปฏิบัติเหมือน
ชนิดที่คุณเขียนเอง และทุกสิ่งที่ทำกับ `defstruct` ได้ก็ทำกับชนิดเหล่านี้ได้

| ชนิด | เนื้อหา | รายละเอียด |
|---|---|---|
| `cons-cell<A,B>` | คู่ `cons`/`car`/`cdr` | [คู่](functions/sequences.md#1-คู่-cons-cellab) |
| `complex` | จำนวนเชิงซ้อน (ส่วนประกอบเป็น `f64`) | [ตัวเลข บทที่ 6](functions/numbers.md#6-จำนวนเชิงซ้อน-complex) |
| `Array<T>` | อาร์เรย์หลายมิติ | [Array](functions/collections.md#5-arrayt-อาร์เรย์หลายมิติ) |
| `BitVector` | ลำดับบิตความยาวคงที่ | [BitVector](functions/collections.md#6-bitvector-เวกเตอร์ของบิต) |
| `HashSet<T>` | กลุ่มของสมาชิกที่ไม่ซ้ำกัน | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | ตารางที่เรียงตามคีย์ | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | ลำดับที่ใส่และเอาออกได้ทั้งสองปลาย | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | ตัววนซ้ำที่ `iter` ของแต่ละคอลเลกชันคืน | [Iter](functions/traits.md#1-trait-iter-และการวนซ้ำ) |
| `lazy::map-iter<I,A,U>` เป็นต้น | อิเทอเรเตอร์ที่ฟังก์ชันของมอดูล `lazy` คืนให้ | [อิเทอเรเตอร์แบบขี้เกียจ](functions/sequences.md#อิเทอเรเตอร์แบบขี้เกียจ-มอดูล-lazy) |
| `WaitGroup` | การรอให้ N สิ่งจบ | [WaitGroup](functions/concurrency.md#4-waitgroup--การรอให้-n-สิ่งเสร็จ) |
| `Mutex<T>` | การกีดกันซึ่งกันและกันสำหรับข้อมูลที่ใช้ร่วมกัน | [Mutex](functions/concurrency.md#6-mutext--การกีดกันซึ่งกันและกันสำหรับข้อมูลที่ใช้ร่วมกัน) |
| `Context` | การยกเลิกแบบร่วมมือ | [Context](functions/concurrency.md#8-context--การยกเลิกแบบร่วมมือ) |
| `pathname` | ชื่อไฟล์ที่แยกเป็นส่วน ๆ | [Pathname](functions/streams-files.md#9-pathname-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | สตรีม | [สตรีม](functions/streams-files.md#3-ชนิดสตรีมที่เป็นรูปธรรม) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | สตรีมแบบประกอบ | [สตรีมแบบประกอบ](functions/streams-files.md#4-สตรีมแบบประกอบ) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | เครือข่าย | [เครือข่าย](functions/network.md#1-ชนิด) |
| `ReadOutcome` | ผลลัพธ์ของ `read-sexpr` `datum` / `eof` | [สตรีม](functions/streams-files.md#6-ฟังก์ชัน-generic-และการดำเนินการกับไฟล์) |
| `universal-time` `internal-time` `decoded-time` | เวลา | [เวลา](functions/system.md#1-เวลา) |
| `heap-info` | สถานะปัจจุบันของฮีป | [เครื่องมือของการ implement](functions/system.md#51-ฟิลด์ของ-heap-info) |

## 5. ชนิดข้อผิดพลาด

`Error` ไม่ใช่ชนิดแต่เป็น trait และชนิดต่อไปนี้ implement มัน หากต้องการจัดการข้อผิดพลาดทุก
ประเภท ให้เขียน `:dyn Error`

| ชนิด | สร้างโดย |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | การดำเนินการกับไฟล์และสตรีม |
| `NetError` | การดำเนินการเครือข่าย |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

รายละเอียดอยู่ใน[ชนิดข้อผิดพลาดและ trait Error](functions/option-result.md#3-ชนิดข้อผิดพลาดและ-trait-error)

## 6. การ implement trait มาตรฐาน

ชนิดใด implement trait ใด เมทอดของแต่ละ trait อยู่ใน
[Trait มาตรฐาน](functions/traits.md)และในบทที่ระบุไว้ในคอลัมน์ขวาสุด

### 6.1 การเปรียบเทียบ การแฮช และการพิมพ์

| Trait | ชนิดที่ implement | รายละเอียด |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-การเปรียบเทียบ) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | เหมือนข้างบน |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `Context` `pathname` `universal-time` `internal-time` และชนิดข้อผิดพลาดที่มีให้ในตัวทั้งหมด | [print-object](functions/printing.md#5-print-object-การแสดงผลของแต่ละชนิด) |

trait ของ `cons-cell<A,B>` และทูเพิล `#{..}` รวมทั้ง `print-object` ของคอลเลกชัน ใช้ได้เมื่อชนิดของสมาชิก implement trait นั้น

### 6.2 การคำนวณ

| Trait | ชนิดที่ implement |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

รายละเอียดอยู่ใน[trait การคำนวณ](functions/traits.md#3-trait-การคำนวณ-add--sub--mul--div--rem--bits--number)

### 6.3 การวนซ้ำ

| Trait | ชนิดที่ implement |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` ชนิดของมอดูล `lazy` (`lazy::map-iter<I,A,U>` เป็นต้น) |

### 6.4 สตรีม

| ชนิด | trait ที่ implement |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

ทุกสตรีม implement `Stream` สตรีมอินพุต implement `InputStream` ด้วย และสตรีมเอาต์พุต implement
`OutputStream` `socket-listener` และ `udp-socket` implement เฉพาะ `Stream` (`close` /
`open-stream-p`) รายละเอียดอยู่ใน[สตรีม](functions/streams-files.md#1-ลำดับชั้นของ-trait)

### 6.5 อื่น ๆ

| Trait | ชนิดที่ implement | รายละเอียด |
|---|---|---|
| `Error` | ชนิดข้อผิดพลาดทั้งหมดในหัวข้อ 5 | [ชนิดข้อผิดพลาด](functions/option-result.md#3-ชนิดข้อผิดพลาดและ-trait-error) |
| `Pathish` | `string` `pathname` | [Pathname](functions/streams-files.md#91-trait-ตัวกำหนด-pathname-pathish) |
