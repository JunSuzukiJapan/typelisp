<!-- translated-from: docs/ja/README.md @ 7d830ae3902dd3c581c7cf6fa5365b91b34106da -->
# เอกสารประกอบ typelisp (ภาษาไทย)

typelisp คือภาษา Lisp ที่มีระบบชนิดแบบสถิต (statically typed) สำหรับการติดตั้งและการบิลด์ ดู
[README.md](../../README.md) (ภาษาอังกฤษ) ที่ระดับบนสุดของรีโพซิทอรี

## คุณสมบัติ

### task และ channel

เริ่ม task (เธรดน้ำหนักเบา) แล้วส่งค่าระหว่าง task ผ่าน channel ดูรายละเอียดได้ที่ [การทำงานพร้อมกัน](tutorial/concurrency.md)

```lisp
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### ใช้ Option และ Result แทน nil

ไม่มี `nil` ฟังก์ชันที่อาจไม่มีค่าจะคืน `Option<T>` และฟังก์ชันที่อาจล้มเหลวจะคืน `Result<T,E>` แล้วใช้ `match` ดึงค่าข้างในออกมา หากใช้ `Option<string>` เหมือนเป็น `string` หรือลืมเขียนกรณี `none` ใน `match` จะเกิดข้อผิดพลาดของชนิดข้อมูลก่อนโปรแกรมทำงาน ดูรายละเอียดได้ที่ [พื้นฐานของชนิด](tutorial/types.md) และ [การจัดการข้อผิดพลาด](tutorial/errors.md)

```lisp
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### แมโครที่ผ่านการตรวจชนิดข้อมูล

เขียนแมโครด้วย `defmacro`, quasiquote และ `gensym` เหมือนใน Common Lisp โค้ดที่ได้จากการขยายแมโครก็ผ่านการตรวจชนิดข้อมูลเช่นเดียวกับโค้ดที่เขียนเอง หากชนิดข้อมูลไม่ตรงกัน จะรายงานข้อผิดพลาดที่ตำแหน่งที่เรียกแมโคร ดูรายละเอียดได้ที่ [แมโคร](tutorial/macros.md)

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

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
