<!-- translated-from: docs/ja/tutorial/intro.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Bắt đầu

Bắt đầu từ việc đánh giá biểu thức trong REPL, chương này lần lượt trình bày hàm, biến, rẽ nhánh,
vòng lặp, danh sách và `Vector`. Về cách build `typl`, xem [README.md](../../../README.md).

## 1. Khởi động REPL

Khi khởi động không có đối số, `typl` vào REPL (chế độ tương tác). Gõ một biểu thức sau `typl>` và nó
được đánh giá ngay tại chỗ rồi giá trị được in ra. `:quit` thoát khỏi REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Từ đây trở đi, dữ liệu nhập và kết quả trong REPL được trình bày theo dạng này.

## 2. Đánh giá biểu thức

typelisp là một Lisp, nên một biểu thức được bao trong ngoặc với **toán tử hoặc tên hàm đứng đầu**.
Bạn viết `(+ 1 2)`, không phải `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Số có các loại sau:

- **Số nguyên** có kiểu `int`. Không có giới hạn trên về độ lớn.
- **Số thập phân** có kiểu `f64`. Hãy viết chúng với dấu thập phân, như `1.5` hoặc `2.0`.
- Bạn không thể trộn `int` và `f64` trong một phép tính. `(+ 1 2.0)` là lỗi kiểu. Để chuyển đổi, hãy
  viết `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` trên hai số nguyên cho một số nguyên với phần thập phân bị bỏ đi (nó không tạo ra phân số như
Common Lisp). Hãy dùng `(mod 7 2)` để lấy phần dư.

Các giá trị boolean là `true` và `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Định nghĩa hàm

Hàm được định nghĩa bằng `defun`. **Kiểu của các đối số và kiểu trả về luôn phải được viết ra.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` nghĩa là "một đối số `n` kiểu `int`". Với nhiều đối số, hãy liệt kê chúng:
  `((a int) (b int))`.
- `int` sau danh sách đối số là kiểu trả về.
- Giá trị của biểu thức cuối cùng trong thân là giá trị trả về của hàm. Bạn không viết `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Một lời gọi có kiểu không khớp được báo là lỗi kiểu **trước khi chạy**. Khi bạn chạy một tệp, chỉ cần
một lỗi kiểu ở bất cứ đâu là không một dòng nào của chương trình được chạy.

Để làm một đối số tùy chọn, hãy dùng `&optional`. Nếu bạn đưa ra giá trị mặc định, đối số nhận giá trị
đó khi bị bỏ qua.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`false` được đưa làm đối số đầu tiên của `format` nghĩa là "trả kết quả dưới dạng chuỗi thay vì in ra".
Mỗi `~a` được thay bằng đối số tiếp theo.

## 4. Biến

Biến cục bộ được tạo bằng `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Kiểu của một biến `let` được lấy từ giá trị ban đầu của nó. Bạn không cần viết ra.
- Các biến của cùng một `let` không thể tham chiếu lẫn nhau. Để dựng một biến từ biến đứng trước nó,
  hãy dùng `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Để thay đổi giá trị của một biến, hãy dùng `setf`. **Phép gán không thể thay đổi kiểu của biến.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Biến toàn cục được định nghĩa bằng `defvar`. Ở đây bạn phải viết kiểu.

```lisp
(defvar (counter int) 0)
```

## 5. Rẽ nhánh

### if

Hãy viết `(if điều-kiện biểu-thức-then biểu-thức-else)`. **Biểu thức else không thể bỏ.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Chỉ biểu thức kiểu `bool` mới có thể làm điều kiện. Viết một số, như `(if 0 ...)`, là lỗi kiểu.
- Biểu thức then và else phải có cùng kiểu.

Khi không cần làm gì ở trường hợp sai, hãy dùng `when` (và `unless` cho trường hợp ngược lại).

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

Với ba điều kiện trở lên, `cond` dễ đọc hơn. `else` cuối cùng được chọn khi không điều kiện nào đúng.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Để rẽ nhánh theo hình dạng của một giá trị, hãy dùng `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` khớp với mọi giá trị. Vì `int` có vô số giá trị, bỏ nhánh `_` là lỗi cho biết không phải mọi
trường hợp đều được bao phủ. Chỗ `match` thực sự phát huy là khi tách `Option` và các kiểu do bạn tự
định nghĩa, sẽ xuất hiện ở chương tiếp theo, [Kiến thức cơ bản về kiểu](types.md).

## 6. Vòng lặp

Một hàm có thể tự gọi chính nó.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Để lặp một số lần cố định, hãy dùng `dotimes`. `i` chạy từ 0 đến `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Cũng có `while`, `do` và `loop` mở rộng của Common Lisp. Các từ mệnh đề của `loop` mở rộng được viết
dưới dạng keyword (`:for`, `:collect`, v.v.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#(1 4 9 16 25)
```

## 7. Danh sách và Vector

### Vector

Để chứa một dãy giá trị cùng kiểu, hãy dùng `Vector<T>`. `T` là kiểu phần tử.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; in ra #(3 1 2)
```

- Riêng `(Vector::new)` không xác định được kiểu phần tử, nên hãy đưa kiểu bằng
  `(the Vector<int> ...)`.
- `(push v x)` nối vào cuối, `(get v i)` đọc phần tử `i`, và `(len v)` cho độ dài.
- Một `get` với chỉ số ngoài phạm vi sẽ dừng chương trình với một lỗi.

### lambda và hàm bậc cao

Hàm ẩn danh được tạo bằng `lambda`. Cũng như `defun`, bạn viết kiểu của đối số và kiểu trả về.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` và các hàm tương tự nhận một `Vector` đã được biến thành
**iterator** bằng `(iter v)`. Tập hợp đứng trước và hàm đứng sau. `v` ở trên được gắn bằng `let` nên
không dùng được bên ngoài `let` đó. Ví dụ tiếp theo định nghĩa `v` bằng `defvar` trước.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #(30 10 20)
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #(3 2)
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #(1 2 3)
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Để xử lý từng phần tử một, hãy dùng `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Một hàm nhận một hàm làm đối số viết kiểu của đối số đó là
`(fn (các-kiểu-đối-số...) kiểu-trả-về)`. Một hàm định nghĩa bằng `defun` có thể được truyền bằng tên
của nó như một giá trị.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Danh sách (S-expression)

Các danh sách tạo bằng `'(1 2 3)` hoặc `(list 1 2 3)` là **dữ liệu S-expression**. Các phần tử của chúng
không nhất thiết phải cùng kiểu.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Dữ liệu S-expression chủ yếu dùng để xử lý chính các chương trình, trong macro ([Macro](macros.md)) và
với `read`. Với dữ liệu có phần tử mang kiểu đã biết, hãy dùng `Vector<T>`. Một danh sách S-expression
có thể được duyệt bằng `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Một cặp hai giá trị được tạo bằng `cons` và tách ra bằng `car` và `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Viết chương trình trong một tệp

Một chương trình có thể được viết trong một tệp (có phần mở rộng `.typl`) và chạy bằng
`typl tên-tệp`. Hãy dùng `println` để hiển thị kết quả.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` in bằng các chỉ thị giống như `format` và kết thúc bằng một dòng mới. `print` không thêm
  dòng mới.
- `~a` nhúng một giá trị ở dạng con người đọc được, còn `~s` ở dạng có thể đọc ngược lại (chuỗi được
  giữ dấu `"`).
- Một tệp được đọc từ trên xuống dưới. **Không thể gọi một hàm trước định nghĩa của nó.**

## 9. Nên đọc gì tiếp theo

- [Kiến thức cơ bản về kiểu](types.md): `Option`, `Result`, struct, enum, generics
- [Dành cho lập trình viên Common Lisp](../guide/from-common-lisp.md): danh sách các điểm khác biệt
  cho những ai đã biết Common Lisp
