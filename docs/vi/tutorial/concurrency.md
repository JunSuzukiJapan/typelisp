<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Lập trình đồng thời

Trong typelisp, bạn chạy công việc đồng thời bằng cách khởi động các **task** (thread nhẹ), và các task
truyền giá trị cho nhau qua các **kênh** (channel). Mô hình này gần với goroutine và channel của Go.
Chương này lần lượt trình bày việc khởi động một task và lấy kết quả của nó, kênh, `select`, bảo vệ
dữ liệu dùng chung, và các thread HĐH riêng biệt. Chương này giả định bạn đã đọc
[Kiến thức cơ bản về kiểu](types.md).

## 1. Khởi động một task và chờ kết quả

`(task (hàm đối-số...))` khởi động một lời gọi hàm như một task mới. Bên khởi động không chờ mà tiếp
tục chạy. Giá trị là một handle kiểu `Task<T>`; `(wait handle)` chờ task kết thúc và trả về kết quả của
nó.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; chờ 0,1 giây (chỉ task này dừng)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` chỉ nhận dạng của một lời gọi hàm. Các đối số được đánh giá tại nơi viết `task`; chỉ riêng lời
  gọi chạy trong task mới.
- Để chạy nhiều biểu thức, hãy tạo một `lambda` và gọi nó ngay tại chỗ:
  `(task ((lambda () () (println "start") (work))))`
- Bạn có thể gọi `wait` bao nhiêu lần tùy ý. Kết quả được ghi nhớ.
- Một task vẫn chạy dù bạn không bao giờ `wait` nó.
- **Khi công việc chính kết thúc, chương trình kết thúc.** Các task còn đang chạy bị cắt ngang.

## 2. Truyền giá trị qua kênh

Một kênh `Chan<T>` là đường để các task truyền các giá trị kiểu `T`. Đối số của `Chan::new` là dung
lượng (kênh chứa được bao nhiêu giá trị). Trên một kênh có dung lượng 0, bên gửi và bên nhận đều chờ
cho đến khi bên kia có mặt.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; gửi
  (close ch))                  ; không gửi nữa

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; nhận cho đến khi kênh bị đóng
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` gửi. Nếu kênh đầy, nó chờ cho đến khi có chỗ.
- `(recv ch)` nhận. Nó chờ cho đến khi có giá trị đến. Kết quả là một `Option<T>`; khi kênh đã đóng và
  rỗng, nó trả về `none`.
- Duyệt một kênh bằng `doiter` sẽ liên tục nhận giá trị cho đến khi kênh bị đóng. Kênh cũng có thể được
  truyền thẳng cho `map` hoặc `filter`.
- `send` trên một kênh đã đóng sẽ panic.

### Chia công việc cho nhiều task

Một mẫu thường gặp là dựng một kênh mang công việc và để nhiều worker (các task xử lý công việc) lấy
việc từ đó. Worker nào rảnh sẽ lấy việc kế tiếp, nên ngay cả khi việc chậm và việc nhanh lẫn lộn, công
việc vẫn được phân tán một cách tự nhiên.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; việc này mất bao lâu

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; lấy từng việc một cho đến khi jobs bị đóng
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; đang làm việc; trong lúc đó các worker khác lấy các việc kế tiếp
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; worker này đã làm bao nhiêu việc

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
  (close jobs)                    ; hết việc rồi
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

- A, B và C mỗi worker lấy một trong ba việc đầu tiên.
- Sau 0,1 giây B và C rảnh và lấy các việc còn lại. Trong khi A bận với việc chậm số 1, nó không nhận
  việc mới.
- Cuối cùng A xử lý một việc, B hai việc và C ba việc. Không có gì trong chương trình nói worker nào
  lấy việc nào.
- Đóng `jobs` kết thúc `doiter` của từng worker, các task kết thúc, và mỗi `wait` trả về số việc đếm
  được.

`jobs` là một kênh có dung lượng 0, nên `send` chờ cho đến khi một worker nhận việc. Với dung lượng lớn
hơn, task chính có thể xếp hàng công việc mà không phải chờ các worker.

## 3. `select`: chờ nhiều kênh cùng lúc

`select` thực hiện thao tác kênh nào trong số nhiều thao tác trở nên khả thi trước. `(after seconds)` là
một kênh phát ra một giá trị khi thời gian cho trước đã trôi qua. Kết hợp với `select`, nó cho bạn một
timeout.

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

- `((v (recv ch)) body...)` là một nhánh nhận. `v` nhận một `Option<T>`.
- `((send ch x) body...)` là một nhánh gửi.
- Khi nhiều nhánh có thể tiến hành cùng lúc, một trong số chúng được chọn ngẫu nhiên.
- Với `(else body...)` ở cuối, `else` chạy khi không nhánh nào có thể tiến hành ngay, và `select` không
  chờ.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Bảo vệ dữ liệu dùng chung

Khi nhiều task sửa đổi cùng một giá trị, hãy bảo vệ nó bằng `Mutex<T>`. `with-lock` lấy khóa, gán nội
dung vào một biến, chạy thân, và luôn nhả khóa dù thân được rời đi bằng cách nào. Gán cho biến bằng
`setf` bên trong thân sẽ thay đổi nội dung của `Mutex`.

`WaitGroup` là công cụ để chờ cho đến khi một số task cho trước đã kết thúc. Tăng bộ đếm bằng `add`, để
mỗi task gọi `done` khi kết thúc, và `wait` cho đến khi bộ đếm về 0.

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

Nếu nhiều task sửa đổi cùng một giá trị cùng lúc mà không qua `Mutex` hay kênh, kết quả không được đảm
bảo. Hãy truyền dữ liệu giữa các task qua kênh khi có thể, và chỉ chia sẻ dữ liệu khi cần.

## 5. Nơi các task chuyển đổi

Các task chuyển đổi theo kiểu hợp tác. Một task chỉ nhường cho các task khác tại những điểm sau:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- Một thao tác kênh phải chờ (`send`, `recv`, `select`)
- Một thao tác socket phải chờ (kết nối, đọc, ghi, v.v.)

Đối số của `sleep` là một số giây kiểu `f64`. Hãy viết `(sleep 1.0)`, không phải `(sleep 1)`.

Các task chạy cùng lúc trên nhiều thread HĐH. Tuy nhiên, khi `typl` chạy trực tiếp một chương trình, chỉ
những task đang chạy các hàm [đã biên dịch](../guide/compile.md) mới ra các thread khác. Các task còn
lại chạy trên một thread duy nhất, chuyển đổi tại các điểm nêu trên.

## 6. `thread`: chạy trên một thread HĐH riêng

Công việc không nên giữ chân các task khác, như gọi một hàm C chậm ([C FFI](../guide/ffi.md)), được
khởi động bằng `thread`. Nó được viết giống như `task`, và có một thread HĐH riêng. Chờ nó kết thúc
bằng `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- Handle từ `thread` có kiểu `Thread<T>`. Giống `wait`, `join` có thể được gọi bao nhiêu lần tùy ý.
- `thread` chỉ có thể chạy các hàm có thể biên dịch được. Khi chạy trong `typl`, hàm mà nó gọi được biên
  dịch ngay tại chỗ trước khi chạy.

## 7. Task và các tính năng khác

- Một `panic` bên trong một task dừng toàn bộ chương trình.
- `throw` không vươn ra ngoài một task. Một `throw` sẽ rời khỏi thân của task trở thành một `panic`.
- Đầu ra của một lệnh `println` không bao giờ bị trộn vào giữa một dòng với đầu ra của các task khác.

## 8. Nên đọc gì tiếp theo

- [Task và kênh](../reference/functions/concurrency.md): danh sách các hàm
- [Tham chiếu cú pháp chương 12](../reference/syntax.md#12-lập-trình-đồng-thời-task): chi tiết về nơi
  các task chuyển đổi, và khác biệt so với Go
- [Vào/ra tệp, stream và mạng](../guide/io.md): viết một server bằng task
