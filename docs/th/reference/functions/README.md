<!-- translated-from: docs/ja/reference/functions/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# ฟังก์ชันที่มีให้ในตัว

รายการฟังก์ชันที่มีให้ในตัว เมทอด และไลบรารีมาตรฐาน สำหรับไวยากรณ์ (ฟอร์มพิเศษและวิธีนิยามสิ่งต่าง ๆ)
ดู[เอกสารอ้างอิงไวยากรณ์](../syntax.md) สำหรับรายการชนิด ดู
[ชนิด](../types.md)

## รูปแบบการเรียก

มีรูปแบบการเรียกสามแบบ

- ฟังก์ชันอิสระ: `(name args...)`
- เมทอดของอินสแตนซ์: `(name receiver args...)` (แก้ไขจากชนิดสถิตของอาร์กิวเมนต์แรก)
- เมทอดสถิต (associated function): `(Type::name args...)`

แต่ละชนิดอาจมีเมทอดชื่อเดียวกันของตัวเอง `(+ a b)` เรียก `+` ของชนิดของ `a`

## การอ่านตาราง

ตารางในแต่ละบทมีคอลัมน์ "ชื่อ, รูปแบบ, ชนิด, คำอธิบาย" คอลัมน์ชนิดเขียนเป็น
`(ชนิดของอาร์กิวเมนต์,...)→ชนิดของค่าที่คืน`

- ตัวอักษรพิมพ์ใหญ่ตัวเดียว เช่น `T`, `A` หรือ `B` เป็นตัวแปรชนิด
- หมายเหตุอย่าง `where Eq A` คือ trait bound ที่ตัวแปรชนิดต้องเป็นไปตาม
- `Iter<A>` หมายถึง "การ implement ใดก็ได้ของ `Iter` ที่ `Item` เป็น `A`"
- อาร์กิวเมนต์ที่ทำเครื่องหมาย `&optional` / `&key` ละได้

## บท

| ไฟล์ | เนื้อหา |
|---|---|
| [numbers.md](numbers.md) | จำนวนเต็ม จำนวนทศนิยม จำนวนตรรกยะ จำนวนเชิงซ้อน บูลีน การดำเนินการระดับบิต จำนวนสุ่ม |
| [sequences.md](sequences.md) | คู่ `cons-cell` ข้อมูล S-expression `Sexpr` สัญลักษณ์ ฟังก์ชันของลำดับ ฟังก์ชันอันดับสูง |
| [collections.md](collections.md) | สตริง อักขระ `Vector`, `HashTable`, `Array`, `BitVector` |
| [option-result.md](option-result.md) | `Option`, `Result`, ชนิดข้อผิดพลาด และ trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, trait การคำนวณ |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, ตัวแปรควบคุมการพิมพ์ |
| [format.md](format.md) | คำสั่งรูปแบบ |
| [streams-files.md](streams-files.md) | สตรีม การดำเนินการกับไฟล์ pathname readtable |
| [concurrency.md](concurrency.md) | Task, channel, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix domain socket, UDP |
| [system.md](system.md) | เวลา สภาพแวดล้อมขณะรัน เครื่องมือของการ implement `read`/`eval` docstring ฟังก์ชันที่เกี่ยวกับแมโคร |
