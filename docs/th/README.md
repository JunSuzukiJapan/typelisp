<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# เอกสารประกอบ typelisp (ภาษาไทย)

typelisp คือภาษา Lisp ที่มีระบบชนิดแบบสถิต (statically typed) สำหรับการติดตั้งและการบิลด์ ดู
[README.md](../../README.md) (ภาษาอังกฤษ) ที่ระดับบนสุดของรีโพซิทอรี

## บทเรียน

หากเพิ่งเริ่มใช้ typelisp ให้อ่านตามลำดับนี้

- [เริ่มต้นใช้งาน](tutorial/intro.md): REPL, ฟังก์ชัน, ตัวแปร, เงื่อนไข, ลูป, ลิสต์ และ `Vector`
- [พื้นฐานของชนิด](tutorial/types.md): ชนิดแบบสถิต, `Option`, `Result`, struct, enum, generics
- [Trait](tutorial/traits.md): `deftrait` / `impl`, ขอบเขตของ trait (trait bound), `:dyn`
- [แมโคร](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [การจัดการข้อผิดพลาด](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [การทำงานพร้อมกัน](tutorial/concurrency.md): task, channel, `select`, `Mutex`, `thread`

## คู่มือ

- [โมดูลและการจัดวางไฟล์](guide/modules.md): `use`, `pub`, การที่ไฟล์สอดคล้องกับโมดูล
- [การคอมไพล์](guide/compile.md): JIT, การสร้างไฟล์ปฏิบัติการด้วย AOT, dump
- [I/O ของไฟล์ สตรีม และเครือข่าย](guide/io.md): ไฟล์, pathname, TCP / TLS / UDP, การแปลงชื่อ (name resolution)
- [FFI สำหรับ C](guide/ffi.md): การเรียกฟังก์ชัน C ด้วย `defffi` (รวมถึง callback และโครงสร้าง C ด้วย `def-c-struct`)
- [การใช้งานร่วมกับโปรแกรมแก้ไขข้อความ](guide/editors.md): `typl-lsp` และการตั้งค่า VS Code / Emacs
- [สำหรับโปรแกรมเมอร์ Common Lisp](guide/from-common-lisp.md): typelisp ต่างจาก CL อย่างไร และจะเขียนโค้ด CL ใหม่อย่างไร

## เอกสารอ้างอิง

- [เอกสารอ้างอิงไวยากรณ์](reference/syntax.md): ไวยากรณ์ระดับคำศัพท์, การเขียนชนิด, การนิยาม, ฟอร์มควบคุม, การคอมไพล์, การทำงานพร้อมกัน
- [ฟังก์ชันที่มีให้ในตัว](reference/functions/README.md): ฟังก์ชันที่มีให้ในตัว, เมทอด และไลบรารีมาตรฐาน
- [ชนิด](reference/types.md): ชนิดต่าง ๆ และ trait ที่แต่ละชนิดสร้างให้
- [ข้อความแสดงข้อผิดพลาด](reference/errors.md): ความหมายของข้อผิดพลาดที่พบบ่อยและวิธีแก้ไข

## การใช้งานร่วมกับโปรแกรมแก้ไขข้อความ

ขั้นตอนการตั้งค่าอยู่ใน[คู่มือการใช้งานร่วมกับโปรแกรมแก้ไขข้อความ](guide/editors.md) ส่วนปุ่มลัดและ
การตั้งค่าของแต่ละโปรแกรมแก้ไขข้อความมีอยู่ในเอกสารเหล่านี้

- [Emacs (typelisp-mode)](../../editor/emacs/README_th.md)
- [VS Code](../../editor/vscode/README_th.md)
