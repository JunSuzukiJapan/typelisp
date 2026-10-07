<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# การทำงานพร้อมกัน

ใน typelisp คุณทำงานพร้อมกันโดยเริ่ม **task** (เธรดน้ำหนักเบา) และ task ส่งค่าให้กัน
ผ่าน **channel** โมเดลนี้ใกล้เคียงกับ goroutine และ channel ของ Go
บทนี้ครอบคลุมตามลำดับ การเริ่ม task และการรับผลลัพธ์ channel, `select`,
การปกป้องข้อมูลที่ใช้ร่วมกัน และเธรด OS เฉพาะ โดยสมมติว่าคุณอ่าน
[พื้นฐานของชนิด](types.md)แล้ว

## 1. การเริ่ม task และการรอผลลัพธ์

`(task (function arguments...))` เริ่มการเรียกฟังก์ชันเป็น task ใหม่ ฝั่งที่เริ่มไม่
รอและทำงานต่อ ค่าที่ได้คือ handle ชนิด `Task<T>` `(wait handle)` รอให้ task
จบและคืนผลลัพธ์ของมัน

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` รับเฉพาะรูปแบบของการเรียกฟังก์ชัน อาร์กิวเมนต์ถูกประเมินค่าที่ที่เขียน `task`
  มีเพียงตัวการเรียกเองเท่านั้นที่รันใน task ใหม่
- หากต้องการรันหลายนิพจน์ ให้สร้าง `lambda` แล้วเรียกทันที:
  `(task ((lambda () () (println "start") (work))))`
- คุณเรียก `wait` กี่ครั้งก็ได้ ผลลัพธ์ถูกจดจำไว้
- task รันแม้คุณไม่เคย `wait` มัน
- **เมื่องานหลักจบ โปรแกรมก็จบ** task ที่ยังรันอยู่จะถูกตัดทิ้ง

## 2. การส่งค่าผ่าน channel

channel `Chan<T>` คือเส้นทางที่ task ใช้ส่งค่าชนิด `T` อาร์กิวเมนต์ของ
`Chan::new` คือความจุ (จำนวนค่าที่เก็บได้) บน channel ที่ความจุ 0 ผู้ส่ง
และผู้รับต่างรอจนกว่าอีกฝั่งจะมา

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; send
  (close ch))                  ; no more sends

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; receive until it is closed
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` ส่ง หาก channel เต็ม จะรอจนมีที่ว่าง
- `(recv ch)` รับ จะรอจนกว่าค่าจะมาถึง ผลลัพธ์เป็น `Option<T>` เมื่อ
  channel ถูกปิดและว่างเปล่า จะคืน `none`
- การวนซ้ำ channel ด้วย `doiter` รับค่าต่อไปเรื่อย ๆ จนกว่าจะถูกปิด ส่งตรง ๆ ให้ `map` หรือ `filter`
  ก็ได้
- `send` บน channel ที่ปิดแล้วจะ panic

### การแบ่งงานให้หลาย task

รูปแบบที่พบบ่อยคือตั้ง channel หนึ่งที่พางาน และให้ worker หลายตัว (task ที่
ประมวลผลงาน) หยิบงานจากมัน worker ตัวใดว่างจะหยิบงานถัดไป ดังนั้นแม้งานช้า
และงานเร็วปนกัน งานก็กระจายไปอย่างเป็นธรรมชาติ

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; how long this job takes

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; take one job at a time until jobs is closed
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; working; meanwhile other workers take the next jobs
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; how many jobs this worker did

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; that is all the work
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- A, B และ C ต่างหยิบหนึ่งในสามงานแรก
- หลังจาก 0.1 วินาที B และ C ว่างและหยิบงานที่เหลือ ขณะที่ A ยุ่งกับงานที่ 1 ที่ช้า
  มันไม่หยิบงานใหม่
- ในที่สุด A จัดการหนึ่งงาน B สองงาน และ C สามงาน ไม่มีสิ่งใดในโปรแกรมที่บอกว่า worker ตัวไหน
  หยิบงานไหน
- การปิด `jobs` ทำให้ `doiter` ของแต่ละ worker จบ task จบ และแต่ละ `wait` คืนจำนวนที่นับได้

`jobs` เป็น channel ความจุ 0 ดังนั้น `send` รอจนกว่า worker ตัวใดตัวหนึ่งจะหยิบงาน หากความจุ
มากขึ้น task หลักก็เข้าคิวงานได้โดยไม่ต้องรอ worker

## 3. `select`: การรอหลาย channel พร้อมกัน

`select` ทำการดำเนินการ channel ตัวใดตัวหนึ่งในหลายตัวที่เป็นไปได้ก่อน `(after seconds)`
เป็น channel ที่ส่งค่าหนึ่งค่าเมื่อเวลาที่กำหนดผ่านไป เมื่อรวมกับ `select` จะ
ให้ timeout แก่คุณ

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) body...)` เป็นกิ่งรับ `v` ได้ `Option<T>`
- `((send ch x) body...)` เป็นกิ่งส่ง
- เมื่อหลายกิ่งดำเนินต่อได้พร้อมกัน จะเลือกหนึ่งในนั้นแบบสุ่ม
- เมื่อมี `(else body...)` ที่ท้าย `else` จะรันเมื่อไม่มีกิ่งใดดำเนินต่อได้ทันที และ `select`
  ไม่รอ

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. การปกป้องข้อมูลที่ใช้ร่วมกัน

เมื่อหลาย task แก้ไขค่าเดียวกัน ให้ปกป้องด้วย `Mutex<T>` `with-lock` ถือล็อก
ผูกเนื้อหากับตัวแปร รันตัวเนื้อหา และปล่อยล็อกเสมอไม่ว่าตัวเนื้อหาจะออกด้วยวิธีใด
การกำหนดค่าให้ตัวแปรด้วย `setf` ภายในตัวเนื้อหาเปลี่ยนเนื้อหาของ `Mutex`

`WaitGroup` เป็นเครื่องมือสำหรับรอจนกว่า task จำนวนที่กำหนดจะจบ เพิ่มตัวนับด้วย
`add` ให้แต่ละ task เรียก `done` เมื่อจบ และ `wait` จนตัวนับถึง 0

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

หากหลาย task แก้ไขค่าเดียวกันพร้อมกันโดยไม่ผ่าน `Mutex` หรือ channel จะไม่รับประกันผลลัพธ์
ให้ส่งข้อมูลระหว่าง task ผ่าน channel เมื่อทำได้ และใช้ข้อมูลร่วมกันเมื่อจำเป็นเท่านั้น

## 5. จุดที่ task สลับกัน

task สลับกันแบบร่วมมือ (cooperative) task หนึ่งยอมให้ task อื่นทำงานเฉพาะที่จุดเหล่านี้:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- การดำเนินการ channel ที่ต้องรอ (`send`, `recv`, `select`)
- การดำเนินการซ็อกเก็ตที่ต้องรอ (การเชื่อมต่อ การอ่าน การเขียน และอื่น ๆ)

อาร์กิวเมนต์ของ `sleep` เป็นจำนวนวินาทีชนิด `f64` เขียน `(sleep 1.0)` ไม่ใช่ `(sleep 1)`

task รันพร้อมกันบนเธรด OS หลายตัว อย่างไรก็ตาม เมื่อ `typl` รันโปรแกรมโดยตรง
เฉพาะ task ที่รันฟังก์ชัน[ที่คอมไพล์แล้ว](../guide/compile.md)เท่านั้นที่ออกไปยังเธรดอื่น task อื่น
รันบนเธรดเดียว สลับกันที่จุดข้างต้น

## 6. `thread`: การรันบนเธรด OS เฉพาะ

งานที่ไม่ควรหน่วง task อื่น เช่น การเรียกฟังก์ชัน C ที่ช้า ([FFI สำหรับ C](../guide/ffi.md))
เริ่มด้วย `thread` เขียนเหมือน `task` และได้เธรด OS เป็นของตัวเอง
รอให้จบด้วย `join`

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- handle จาก `thread` มีชนิด `Thread<T>` เช่นเดียวกับ `wait` `join` เรียกได้กี่ครั้งก็ได้
- `thread` รันได้เฉพาะฟังก์ชันที่คอมไพล์ได้ เมื่อรันใน `typl` ฟังก์ชันที่มันเรียก
  จะถูกคอมไพล์ทันทีก่อนรัน

## 7. task และความสามารถอื่น

- `panic` ภายใน task หยุดทั้งโปรแกรม
- `throw` ไปไม่ถึงภายนอก task `throw` ที่จะออกจากตัวเนื้อหาของ task กลายเป็น
  `panic`
- เอาต์พุตของ `println` หนึ่งครั้งไม่เคยปนเข้ากลางบรรทัดกับเอาต์พุตของ task อื่น

## 8. อ่านอะไรต่อ

- [Task และ channel](../reference/functions/concurrency.md): รายการฟังก์ชัน
- [เอกสารอ้างอิงไวยากรณ์ บทที่ 12](../reference/syntax.md#12-การทำงานพร้อมกัน-task): จุดที่ task สลับกัน
  โดยละเอียด และความแตกต่างจาก Go
- [I/O ของไฟล์ สตรีม และเครือข่าย](../guide/io.md): การเขียนเซิร์ฟเวอร์ด้วย task
