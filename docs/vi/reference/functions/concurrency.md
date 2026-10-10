<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Task và kênh

Từ vựng về task (thread nhẹ). `task` và `thread`, vốn khởi động chúng, và `select`, vốn chờ nhiều thứ, là
các dạng đặc biệt và nằm ở [Tham chiếu cú pháp](../syntax.md#12-lập-trình-đồng-thời-task). Chương này trình bày
phần còn lại: kiểu, phương thức và hàm.

Task là **hợp tác**: một task chỉ chuyển đổi tại những điểm bạn viết ra. Các task chạy đồng thời trên
`TYPELISP_THREADS` thread HĐH (trong `typl`, chỉ các task đã biên dịch mới ra các thread khác). Nơi chúng
chuyển đổi và điểm khác với Go nằm ở [Tham chiếu cú pháp 12.5](../syntax.md#125-nơi-các-task-chuyển-đổi) và
[12.7](../syntax.md#127-khác-biệt-so-với-go).

## 1. `Task<T>` — handle tới các task

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Chờ hoàn tất và trả về giá trị của nó |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Bạn có thể `wait` bao nhiêu lần tùy ý** (giá trị được lưu đệm). Không giống `JoinHandle::join` của
  Rust, nó không tiêu thụ handle, nên có thể được chờ từ nhiều nơi.
- **Một task vẫn chạy dù bạn không bao giờ `wait`.** Bỏ handle đi không dừng nó.
- Nó là một giá trị thông thường, nên có thể đưa vào một `Vector<Task<()>>`.
- **Khi task chính kết thúc, tiến trình kết thúc** (như trong Go). Các task khác đang chạy bị cắt ngang,
  và phần dọn dẹp của `unwind-protect` không chạy, vì đây là việc thoát tiến trình, không phải tháo
  stack.

## 2. `Chan<T>` — kênh

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Một kênh có dung lượng `n`. `0` là rendezvous (không có bộ đệm) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Chờ đến khi có chỗ, rồi trao giá trị |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Chờ đến khi có giá trị đến. `none` khi đã đóng và rỗng |
| `close` | `(close ch)` | `(Chan<T>)→()` | Đóng nó |
| `len` | `(len ch)` | `(Chan<T>)→int` | Hiện có bao nhiêu giá trị trong bộ đệm |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Dung lượng |

**Đối số kiểu được đưa bằng `the`** (viết giống `(the Vector<i32> (Vector::new))`). **Dung lượng luôn phải
được viết**: hai trường hợp Go viết là `make(chan int)` và `make(chan int, 16)` được viết là
`(Chan::new 0)` và `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; for v := range ch của Go
```

- **Một `Chan<T>` là iterator của chính nó** (nó triển khai `Iter`). `recv` trả về một `Option<T>`, cùng
  kiểu với `Iter::next`, nên `doiter` và `map`/`filter`/`foldl` đều hoạt động trên nó nguyên trạng.
- **`send` trên một kênh đã đóng sẽ panic**, và **`close` lần thứ hai cũng panic** (cả hai như trong Go).
  Đây là bug của chương trình, không phải thất bại có thể khôi phục, nên chúng không phải `Result`.
- **Đóng một kênh mà một task đang chờ `send` lên đó sẽ làm task đó panic** (quy tắc của Go).
- `recv` trên một kênh đã đóng trả về những gì còn lại trong bộ đệm, và khi nó rỗng thì liên tục trả về
  `none`.
- `close` được phân giải theo kiểu của bên nhận, nên nó là một thứ khác với `close` của trait `Stream`.
  `Chan<T>` không triển khai `Stream`.
- **Dung lượng âm sẽ panic** (nó không bị làm tròn về 0 một cách im lặng).

## 3. `yield` / `sleep` — nhường lượt

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Bỏ phần còn lại của lượt (`runtime.Gosched` của Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Dừng **chỉ task đó**. Các task khác tiếp tục chạy |

`sleep` dừng một task, không phải một thread. Chỉ khi không có task nào có thể chạy thì nó mới vào một
`sleep` của HĐH cho đến hạn gần nhất. `(sleep 0.0)` là "nhường trong 0 giây" của CL.

Như trong CL, `sleep` nhận **giây**. Số nguyên không được tự động chuyển thành số dấu phẩy động, nên
`(sleep 1)` của CL được viết là `(sleep 1.0)` ở đây. Một giá trị âm hoặc NaN sẽ panic.

## 4. `WaitGroup` — chờ N lần hoàn tất

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Một nhóm không có gì còn tồn đọng |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Cộng vào bộ đếm. Hãy làm trước khi công việc bắt đầu |
| `done` | `(done wg)` | `(WaitGroup)→()` | Một việc đã xong. Khi về 0, mọi bên đang chờ được nhả |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Chờ đến khi về 0. Từ bất kỳ số task nào |

`(wait wg)` và `(wait task)` được phân giải theo kiểu của bên nhận, nên chúng cùng tồn tại dưới một tên.
Nếu bộ đếm xuống dưới 0, nó panic (`done` được gọi quá nhiều lần, hoặc `add` âm). Như trong Go, một nhóm đã
về lại 0 có thể được dùng lại bắt đầu bằng `add`. Không có cập nhật nào bị mất ngay cả khi các task chạy
trên các thread HĐH riêng.

**Vì có `Task<T>`, nó ít cần hơn trong Go**: `(doiter (t tasks) (wait t))` thường là đủ. Nó là công cụ cho
công việc tăng trưởng động, hoặc khi bạn không muốn giữ các handle.

```lisp
;; fan-in: khởi động một task cho mỗi đầu vào và gộp chúng lại (ngôn ngữ này không có kênh nil)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — kênh phát giá trị sau một khoảng thời gian

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Một kênh phát một giá trị sau `sec` giây |

`time.After` của Go. Nó có thể được viết nguyên trạng trong nhánh timeout của `select`
([Tham chiếu cú pháp 12.3](../syntax.md#123-select--chờ-nhiều-thao-tác-kênh-cùng-lúc)). Dung
lượng của nó là 1, nên task gửi có thể kết thúc ngay cả khi không ai nhận.

## 6. `Mutex<T>` — loại trừ tương hỗ cho dữ liệu dùng chung

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Một mutex chưa khóa giữ `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Lấy khóa (chờ) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Nhả nó. Panic nếu nó chưa bị khóa |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Khóa, gán nội dung vào `x`, chạy `body`, và **luôn** nhả |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` không phải bản sao của giá trị mà là một "vị trí"** (`symbol-macrolet`). `(setf x 42)` thay đổi
  nội dung của mutex.
- `with-lock` nhả bằng `unwind-protect`, nên khóa được nhả dù thân được rời đi bằng cách nào: hoàn tất
  bình thường, `throw`, `panic`, hay `break`/`return`/`return-from`.
- **Vào lại sẽ bị deadlock** (nó không panic). Bộ lập lịch báo rằng "không có gì có thể tiến triển" đối
  với một task bị kẹt trên khóa của chính nó.
- **`m::v` chạm vào nội dung từ ngoài khóa**, điều này là không xác định theo nghĩa một task khác có thể
  đang thay đổi nó giữa chừng. Đây là cùng vị thế với `sync.Mutex` của Go: trong một ngôn ngữ không có
  kiểm tra quyền sở hữu hay mượn, không thể xây dựng một đảm bảo tĩnh như `MutexGuard`.

## 7. `Thread<T>` — các thread HĐH riêng

Handle được `(thread (f args...))` trả về
([Tham chiếu cú pháp 12.2](../syntax.md#122-thread--khởi-động-một-task-trên-một-thread-hđh-riêng)). Đối ứng
của `Task<T>`.

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Chờ hoàn tất và trả về giá trị (**task** gọi bị dừng. Có thể gọi bao nhiêu lần tùy ý; giá trị được lưu đệm) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | Phiên bản hàm của `(thread (f))` (`std::thread::spawn` của Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Số của thread HĐH đang chạy. Duy nhất trong tiến trình, không có ý nghĩa nào ngoài "có phải cùng thread không" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Số thread mà máy có thể chạy cùng lúc (mặc định của `TYPELISP_THREADS`). Panic nếu HĐH không trả lời |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; một hàm C chặn
(let ((th (thread (sleepy 500000))))
  ...                                            ; các task khác vẫn tiếp tục trong lúc đó
  (join th))                                     ; => 500000
```

- Gọi một hàm C chặn (`defffi`) chỉ dừng thread đó.
- Một `task` bên trong một `thread` chạy như một task thông thường trên các thread khác.
- Nó cũng dùng được trong `typl`. Khi thông dịch, `(thread (f ...))` và `Thread::spawn` biên dịch hàm
  để chạy ngay tại chỗ rồi chạy nó trên thread riêng. Một `lambda` tham chiếu tới biến cục bộ bên ngoài
  nó không thể tự biên dịch và sẽ panic
  ([Tham chiếu cú pháp 12.2](../syntax.md#122-thread--khởi-động-một-task-trên-một-thread-hđh-riêng)). Một
  `lambda` được tạo bên trong một hàm đã biên dịch thì có thể được truyền.

## 8. `Context` — hủy theo kiểu hợp tác

`context.Context` của Go. Truyền cho một công việc mà ta muốn có thể dừng từ bên ngoài. Việc dừng là
**hợp tác**: `cancel` không ngắt gì cả; task hoặc thread tự nhận ra bằng cách tự kiểm tra
`is-cancelled` hoặc nhận từ `done`.

| Tên | Cách dùng | Kiểu | Ý nghĩa |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | Một context mới làm gốc |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | Tạo một con của `parent` |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | Tạo một con của `parent` tự hủy sau `sec` giây |
| `cancel` | `(cancel ctx)` | `(Context)→()` | Hủy. Gọi bao nhiêu lần cũng được |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | Kênh được đóng khi context bị hủy |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | Đã bị hủy hay chưa |

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
  (cancel ctx)                          ; job 1, job 2, rồi stopped
  (sleep 0.1))
```

- **Việc hủy lan tới các con.** Context tạo bằng `with-cancel`/`with-timeout` bị hủy cùng với cha
  của nó. Chiều ngược lại (từ con lên cha) thì không.
- Con được tạo từ một context đã bị hủy thì bị hủy ngay từ đầu.
- `done` chỉ được đóng; không có giá trị nào được gửi. Nhận sẽ trả về `none`.
- Mỗi lần gọi `(Context::background)` tạo một gốc riêng. `Background()` của Go chỉ có một và không
  hủy được; ở đây gốc cũng hủy được, và việc đó chỉ ảnh hưởng tới những gì tạo ra từ nó.
- Context có thể truyền giữa các task và giữa các thread.

## 9. Những gì không có

- **`Atomic`**. `Mutex` là đủ.
- **Biến cục bộ theo task** (Go cũng không có).
- **Kênh nil**. Lý do và giải pháp thay thế nằm ở
  [Tham chiếu cú pháp 12.7](../syntax.md#127-khác-biệt-so-với-go).
