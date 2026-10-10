<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Task และ channel

คำศัพท์ของ task (เธรดน้ำหนักเบา) `task` และ `thread` ที่ใช้เริ่ม task และ `select`
ที่ใช้รอหลายสิ่ง เป็นฟอร์มพิเศษและอยู่ใน
[เอกสารอ้างอิงไวยากรณ์](../syntax.md#12-การทำงานพร้อมกัน-task) บทนี้ครอบคลุมส่วนที่เหลือ: ชนิด เมทอด
และฟังก์ชัน

task เป็นแบบ **ร่วมมือ (cooperative)**: task สลับเฉพาะที่จุดที่คุณเขียน task รันพร้อมกันบน
เธรด OS จำนวน `TYPELISP_THREADS` (ใน `typl` เฉพาะ task ที่คอมไพล์แล้วเท่านั้นที่ออกไปยังเธรดอื่น)
จุดที่สลับและความแตกต่างจาก Go อยู่ใน
[เอกสารอ้างอิงไวยากรณ์ 12.5](../syntax.md#125-จุดที่-task-สลับกัน) และ
[12.7](../syntax.md#127-ความแตกต่างจาก-go)

## 1. `Task<T>` — handle ของ task

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | รอให้เสร็จและคืนค่าของมัน |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **คุณ `wait` กี่ครั้งก็ได้** (ค่าถูกแคช) ต่างจาก `JoinHandle::join` ของ Rust ตรงที่ไม่
  ใช้ handle ไป จึงรอจากหลายที่ได้
- **task รันแม้คุณไม่เคย `wait`** การทิ้ง handle ไม่ได้หยุดมัน
- เป็นค่าธรรมดา จึงใส่ใน `Vector<Task<()>>` ได้
- **เมื่อ task หลักจบ โพรเซสก็จบ** (เหมือนใน Go) task อื่นที่กำลังรันถูกตัดทิ้ง และการล้างทรัพยากร
  ของ `unwind-protect` ไม่รัน เพราะนี่คือการออกจากโพรเซส ไม่ใช่การคลี่สแตก (stack unwinding)

## 2. `Chan<T>` — channel

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | channel ความจุ `n` `0` คือ rendezvous (ไม่มีบัฟเฟอร์) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | รอจนมีที่ว่าง แล้วส่งมอบค่า |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | รอจนค่ามาถึง `none` เมื่อถูกปิดและว่าง |
| `close` | `(close ch)` | `(Chan<T>)→()` | ปิด |
| `len` | `(len ch)` | `(Chan<T>)→int` | ตอนนี้มีกี่ค่าในบัฟเฟอร์ |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | ความจุ |

**อาร์กิวเมนต์ชนิดให้ด้วย `the`** (เขียนเหมือน `(the Vector<i32> (Vector::new))`)
**ต้องเขียนความจุเสมอ**: สองกรณีที่ Go เขียนเป็น `make(chan int)` และ
`make(chan int, 16)` เขียนเป็น `(Chan::new 0)` และ `(Chan::new 16)`

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **`Chan<T>` เป็นตัววนซ้ำของตัวเอง** (implement `Iter`) `recv` คืน `Option<T>` ชนิดเดียวกับ
  `Iter::next` ดังนั้น `doiter` และ `map`/`filter`/`foldl` ใช้กับมันได้ทันที
- **`send` บน channel ที่ปิดแล้ว panic** และ **`close` ครั้งที่สองก็ panic** (ทั้งสองอย่างเหมือนใน Go) สิ่งเหล่านี้เป็น
  บั๊กของโปรแกรม ไม่ใช่ความล้มเหลวที่กู้คืนได้ จึงไม่ใช่ `Result`
- **การปิด channel ที่ task กำลังรอ `send` อยู่ทำให้ task นั้น panic** (กฎของ Go)
- `recv` บน channel ที่ปิดแล้วคืนสิ่งที่เหลืออยู่ในบัฟเฟอร์ และเมื่อว่างแล้วจะคืน
  `none` ต่อไปเรื่อย ๆ
- `close` ถูกแก้ไขโดยชนิดของตัวรับ จึงเป็นคนละสิ่งกับ `close` ของ
  trait `Stream` `Chan<T>` ไม่ implement `Stream`
- **ความจุติดลบ panic** (ไม่ถูกปัดเป็น 0 โดยไม่แจ้ง)

## 3. `yield` / `sleep` — การยอมให้ผู้อื่นทำงาน

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | สละช่วงที่เหลือของคิว (`runtime.Gosched` ของ Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | หยุด **เฉพาะ task นั้น** task อื่นยังรันต่อ |

`sleep` หยุด task ไม่ใช่เธรด เฉพาะเมื่อไม่มี task ใดรันได้เลยจึงจะเข้าสู่ `sleep` ของ OS
จนถึงกำหนดเวลาที่ใกล้ที่สุด `(sleep 0.0)` คือ "ยอมให้ผู้อื่น 0 วินาที" ของ CL

เช่นเดียวกับ CL `sleep` รับ **วินาที** จำนวนเต็มไม่ถูกแปลงเป็นจำนวนทศนิยม
โดยอัตโนมัติ ดังนั้น `(sleep 1)` ของ CL เขียนเป็น `(sleep 1.0)` ที่นี่ ค่าติดลบหรือ NaN จะ panic

## 4. `WaitGroup` — การรอให้ N สิ่งเสร็จ

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | กลุ่มที่ไม่มีอะไรค้าง |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | เพิ่มตัวนับ ทำก่อนงานเริ่ม |
| `done` | `(done wg)` | `(WaitGroup)→()` | เสร็จหนึ่งอย่าง เมื่อเป็น 0 ผู้รอทุกตัวถูกปล่อย |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | รอจนถึง 0 จาก task จำนวนเท่าใดก็ได้ |

`(wait wg)` และ `(wait task)` ถูกแก้ไขโดยชนิดของตัวรับ จึงอยู่ร่วมกันภายใต้ชื่อเดียว หาก
ตัวนับต่ำกว่า 0 จะ panic (เรียก `done` มากเกินไป หรือ `add` ติดลบ) เหมือนใน Go กลุ่ม
ที่กลับมาเป็น 0 ใช้ใหม่ได้โดยเริ่มด้วย `add` ไม่มีการอัปเดตใดสูญหายแม้ task รันบน
เธรด OS แยกกัน

**เมื่อมี `Task<T>` ก็จำเป็นน้อยกว่าใน Go**: `(doiter (t tasks) (wait t))` มักเพียงพอ
เป็นเครื่องมือสำหรับงานที่เติบโตแบบไดนามิก หรือเมื่อไม่ต้องการเก็บ handle

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — channel ที่ส่งค่าหลังผ่านเวลาหนึ่ง

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | channel ที่ส่งค่าหนึ่งค่าหลังจาก `sec` วินาที |

`time.After` ของ Go เขียนตามที่เป็นได้ในกิ่ง timeout ของ `select`
([เอกสารอ้างอิงไวยากรณ์ 12.3](../syntax.md#123-select--การรอการดำเนินการ-channel-หลายตัวพร้อมกัน)) ความจุ
ของมันคือ 1 ดังนั้น task ที่ส่งจบได้แม้ไม่มีใครรับ

## 6. `Mutex<T>` — การกีดกันซึ่งกันและกันสำหรับข้อมูลที่ใช้ร่วมกัน

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | mutex ที่ไม่ล็อกซึ่งเก็บ `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | ถือล็อก (รอ) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | ปล่อย panic หากไม่ได้ล็อก |
| `with-lock` | `(with-lock (x m) body...)` | แมโคร | ล็อก ผูกเนื้อหากับ `x` รัน `body` และปล่อย **เสมอ** |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` ไม่ใช่สำเนาของค่าแต่เป็น "place"** (`symbol-macrolet`) `(setf x 42)` เปลี่ยน
  เนื้อหาของ mutex
- `with-lock` ปล่อยด้วย `unwind-protect` ดังนั้นล็อกถูกปล่อยไม่ว่าตัวเนื้อหาจะออกด้วยวิธีใด: จบ
  ตามปกติ, `throw`, `panic` หรือ `break`/`return`/`return-from`
- **การเข้าซ้ำทำให้ deadlock** (ไม่ panic) ตัวตั้งเวลา (scheduler) รายงานว่า "ไม่มีอะไรก้าวหน้าได้"
  สำหรับ task ที่ติดอยู่กับล็อกของตัวเอง
- **`m::v` แตะเนื้อหาจากภายนอกล็อก** ซึ่งไม่นิยามในแง่ที่ว่า task อื่น
  อาจกำลังเปลี่ยนมันอยู่ นี่เป็นจุดยืนเดียวกับ `sync.Mutex` ของ Go: ในภาษาที่ไม่มี
  ownership หรือการตรวจสอบ borrow จึงสร้างการรับประกันแบบสถิตอย่าง `MutexGuard` ไม่ได้

## 7. `Thread<T>` — เธรด OS เฉพาะ

handle ที่ `(thread (f args...))` คืน
([เอกสารอ้างอิงไวยากรณ์ 12.2](../syntax.md#122-thread--การเริ่ม-task-บนเธรด-os-เฉพาะ)) คู่เทียบ
ของ `Task<T>`

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | รอให้เสร็จและคืนค่า (**task** ที่เรียกหยุด เรียกกี่ครั้งก็ได้; ค่าถูกแคช) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | เวอร์ชันฟังก์ชันของ `(thread (f))` (`std::thread::spawn` ของ Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | หมายเลขของเธรด OS ที่กำลังรัน ไม่ซ้ำภายในโพรเซส ไม่มีความหมายนอกจาก "เป็นเธรดเดียวกันหรือไม่" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | จำนวนเธรดที่เครื่องรันได้พร้อมกัน (ค่าเริ่มต้นของ `TYPELISP_THREADS`) panic หาก OS ไม่ตอบ |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- การเรียกฟังก์ชัน C ที่บล็อก (`defffi`) หยุดเฉพาะเธรดนั้น
- `task` ภายใน `thread` รันเป็น task ธรรมดาบนเธรดอื่น
- ใช้ใน `typl` ได้ด้วย เมื่ออินเทอร์พรีต `(thread (f ...))` และ `Thread::spawn` คอมไพล์
  ฟังก์ชันที่จะรันทันทีแล้วรันบนเธรดเฉพาะ `lambda` ที่อ้างถึงตัวแปรท้องถิ่น
  ภายนอกมันคอมไพล์ลำพังไม่ได้และ panic
  ([เอกสารอ้างอิงไวยากรณ์ 12.2](../syntax.md#122-thread--การเริ่ม-task-บนเธรด-os-เฉพาะ)) `lambda` ที่สร้างภายใน
  ฟังก์ชันที่คอมไพล์แล้วส่งได้

## 8. `Context` — การยกเลิกแบบร่วมมือ

`context.Context` ของ Go ใช้ส่งให้งานที่อยากหยุดได้จากภายนอก การหยุดเป็นแบบ**ร่วมมือ** `cancel` ไม่ขัดจังหวะอะไรเลย ทาสก์หรือเธรดจะรู้เองด้วยการตรวจ `is-cancelled` หรือรับจาก `done`

| ชื่อ | การใช้ | ชนิด | ความหมาย |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | คอนเท็กซ์ใหม่ที่เป็นราก |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | สร้างลูกของ `parent` |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | สร้างลูกของ `parent` ที่ยกเลิกตัวเองหลังผ่านไป `sec` วินาที |
| `cancel` | `(cancel ctx)` | `(Context)→()` | ยกเลิก เรียกกี่ครั้งก็ได้ |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | แชนเนลที่ถูกปิดเมื่อคอนเท็กซ์ถูกยกเลิก |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | ถูกยกเลิกแล้วหรือไม่ |

```lisp
(defun worker ((ctx Context) (jobs Chan<int>)) ()
  (loop
    (select
      ((v (recv (done ctx))) (println "stopped") (break))
      ((j (recv jobs)) (match j
                         ((some n) (println "job ~a" n))
                         ((none) (break)))))))

(let* ((ctx (Context::with-timeout (Context::background) 1.0))
       (jobs (the Chan<int> (Chan::new 0))))
  (task (worker ctx jobs))
  (send jobs 1)
  (send jobs 2)
  (cancel ctx)                          ; job 1, job 2 แล้วตามด้วย stopped
  (sleep 0.1))
```

- **การยกเลิกส่งต่อไปยังลูก** คอนเท็กซ์ที่สร้างด้วย `with-cancel`/`with-timeout` จะถูกยกเลิกไปพร้อมกับพ่อแม่ ในทางกลับกัน (จากลูกไปพ่อแม่) จะไม่ส่งต่อ
- ลูกที่สร้างจากคอนเท็กซ์ที่ถูกยกเลิกไปแล้วจะถูกยกเลิกตั้งแต่แรก
- `done` ถูกปิดเท่านั้น ไม่มีการส่งค่า การรับจะได้ `none`
- การเรียก `(Context::background)` แต่ละครั้งสร้างรากแยกกัน `Background()` ของ Go มีอันเดียวและยกเลิกไม่ได้ แต่ที่นี่รากก็ยกเลิกได้ และส่งผลเฉพาะสิ่งที่สร้างจากรากนั้น
- ส่งต่อระหว่างทาสก์และระหว่างเธรดได้

## 9. สิ่งที่ไม่มี

- **`Atomic`** `Mutex` ก็เพียงพอ
- **ตัวแปรท้องถิ่นของ task** (Go ก็ไม่มี)
- **nil channel** เหตุผลและทางเลือกอยู่ใน
  [เอกสารอ้างอิงไวยากรณ์ 12.7](../syntax.md#127-ความแตกต่างจาก-go)
