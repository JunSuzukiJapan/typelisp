<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C FFI (defffi)

Tài liệu này giải thích cách gọi hàm C từ typelisp. Danh sách các kiểu có thể khai báo và các hạn chế
nằm ở [Tham chiếu cú pháp 3.3](../reference/syntax.md#33-defffi--khai-báo-hàm-c-ffi).

## 1. Khai báo và gọi một hàm

`defffi` khai báo tên và các kiểu của một hàm C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; tên trong typelisp và tên ký hiệu C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; tra trong libm
```

Các lời gọi được bao trong `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` là cần thiết vì trình biên dịch không thể kiểm tra rằng các kiểu đã khai báo khớp với kiểu
thật ở phía C. Viết `unsafe` nghĩa là bạn, người viết, nhận trách nhiệm kiểm tra điều đó. Quên nó sẽ
cho một lỗi giải thích điều này.

## 2. Viết một wrapper an toàn

Cách dùng dự kiến là giới hạn `unsafe` ở một chỗ và đưa ra bên ngoài một hàm thông thường.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; bên gọi không cần unsafe
(str-len "hello")  ; => 5
```

## 3. Các kiểu tương ứng nhau thế nào

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Số nguyên cùng độ rộng |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (cũng là `size_t`, `int64_t`, v.v.) |
| `ptr` | Con trỏ bất kỳ (`void *`, `FILE *`, v.v.) |
| `(ptr T)` | Con trỏ tới `T` ([mục 7](#7-struct-c)) |

### Chuỗi

- Một `string` bạn truyền vào được sao chép thành chuỗi C kết thúc bằng NUL, và được giải phóng sau khi
  lời gọi trả về. Một NUL ở giữa chuỗi là lỗi.
- Kết quả của hàm trả về `string` cũng được sao chép. Bộ nhớ phía C không được giải phóng. Với các hàm
  trả về chuỗi mà bên gọi phải giải phóng (như `strdup`), hãy nhận kết quả dưới dạng `ptr` và tự
  `free` nó.
- Nếu một hàm được khai báo trả về `string` mà trả về NULL, đó là lỗi. Hãy nhận kết quả của các hàm có
  thể trả về NULL (như `getenv`) dưới dạng `ptr`.

### `c-long` / `c-ulong` / `ptr`

Các kiểu này chỉ tồn tại để truyền giá trị qua ranh giới với C, và **không hỗ trợ phép toán số học
nào**. Để dùng chúng như số nguyên của typelisp, hãy chuyển đổi bằng `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int không làm mất bit nào của giá trị 64 bit
(try-as i32 (unsafe (c-strlen s)))  ; none nếu không vừa với i32
(unsafe (c-malloc 16))              ; có thể truyền nguyên hằng số nguyên
```

`ptr` là giá trị để đưa lại cho các hàm C. Không có cách nào đọc nội dung nó trỏ tới từ phía typelisp.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Các kiểu này chỉ có thể xuất hiện làm đối số hàm, giá trị trả về và biến cục bộ. Chúng không thể là
trường struct, biến toàn cục, hay đối số kiểu của `Vector` và những thứ tương tự.

## 4. Chỉ định thư viện

Khi không có `:library`, ký hiệu được tra trong những gì đã được liên kết sẵn vào tiến trình (libc
v.v.). Các hàm từ thư viện khác cần `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Một tên ngắn như `"sqlite3"` được tra thành `libsqlite3.dylib`, rồi `libsqlite3.so`.
- Một tên chứa `/` được xem là đường dẫn.
- Nếu không tìm thấy ký hiệu đã khai báo, lỗi sẽ nêu tên nó.

## 5. Biên dịch AOT

Các chương trình dùng `defffi` có thể được biến thành tệp thực thi bằng
[`compile-file`](compile.md#3-tạo-tệp-thực-thi-bằng-biên-dịch-aot) nguyên trạng. Các thư viện được nêu
bằng `:library` được thêm vào lúc liên kết một cách tự động, nên `compile-file` không cần đối số bổ
sung.

## 6. Callback

Bạn có thể truyền một hàm typelisp cho một hàm C và để nó được gọi ngược lại. Hãy viết một kiểu hàm
trong số các kiểu đối số của `defffi`, và ở chỗ gọi hãy đặt một tên hàm hoặc một biểu thức `lambda`
vào vị trí đó.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") trả về p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Chỉ có thể truyền những hàm **không có biến tự do**. Hàm cấp cao nhất, `lambda` và hàm `labels` cục bộ
  đều dùng được, nhưng tham chiếu tới biến cục bộ của phạm vi bao ngoài là lỗi ở thời điểm kiểm tra
  kiểu. C chỉ truyền các đối số đã khai báo, nên không có cách nào chuyển các biến được bắt giữ. Để giữ
  trạng thái, hãy dùng biến toàn cục.
- Không thể truyền một biến đang giữ một hàm. Hãy viết tên hàm hoặc biểu thức `lambda` ngay tại chỗ.
- Một `panic` hoặc `throw` bên trong callback sẽ đến bên gọi sau khi hàm C trả về.
- Callback chỉ có thể được gọi khi hàm C mà typelisp đã gọi đang chạy. Nó không thể dùng từ những thứ
  như `atexit` hay trình xử lý tín hiệu.

## 7. Struct C

Để truyền thứ như một mảng struct cho hàm C, hãy khai báo một struct có cùng bố cục như trong C bằng
`def-c-struct`, và cấp phát nó bên trong `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; khai báo bên trong unsafe ở cấp cao nhất

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; bốn item, tất cả bằng không
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` cấp phát `n` giá trị kiểu `T` và trả về một `(ptr T)`. `(c-ref p i)` là con trỏ tới
  phần tử thứ `i`, `p::field` là một trường, và `(c-deref p)` là thứ mà một con trỏ tới kiểu vô hướng
  như `i32` trỏ tới. Tất cả đều có thể viết với `setf`.
- `(as ptr p)` biến nó thành `ptr` không kiểu để truyền cho các hàm C nhận `void *`.
- Kích thước của `item` (ở đây là 8) và vị trí của từng trường được xác định theo cùng các quy tắc như
  trong C.

### Vòng đời của bộ nhớ đã cấp phát

Bộ nhớ đã cấp phát được giải phóng khi luồng điều khiển rời khỏi `unsafe` ngoài cùng nhất trong hàm đó.
Điều tương tự xảy ra khi rời đi bằng `panic` hoặc `throw`. Vì vậy, không thể đưa một giá trị `(ptr T)`
ra ngoài `unsafe`. Làm nó thành giá trị của `unsafe`, bắt giữ nó trong closure, truyền nó cho một
`task` và ném nó bằng `throw` đều là lỗi kiểu. Hãy sao chép những giá trị bạn muốn dùng bên ngoài vào
số hoặc một `defstruct` bên trong `unsafe`.

Khi cấp phát bên trong một `lambda` hoặc một hàm `labels`, hãy viết một `unsafe` bên trong hàm đó.

### Bộ nhớ do C cấp phát

Một con trỏ nhận từ C dưới dạng `(ptr T)` (giá trị trả về của `defffi`, đối số của callback, v.v.) là lỗi
trừ khi nó trỏ vào bên trong bộ nhớ được cấp phát bằng `c-alloc`. Hãy khai báo các hàm nhận bộ nhớ do C
cấp phát bằng `malloc`, hoặc NULL, với `ptr` không kiểu.

## 8. Những gì không thể làm

- **Hàm có số đối số biến đổi** (`printf` và tương tự) không thể khai báo. Phần đối số biến đổi được
  truyền theo quy tắc khác với các đối số cố định. Hãy khai báo một tên riêng cho mỗi số lượng đối số
  bạn dùng.
- **Truyền hoặc trả về struct theo giá trị** là không thể. Hãy dùng các hàm truyền con trỏ.
- **Khai báo generic** là không thể.
- **Trùng tên với một hàm dựng sẵn** là không được.
- **Chúng không thể được truyền như giá trị hàm.** Bạn không thể truyền như trong `(map xs c-abs)`; hãy
  bọc nó trong một `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
