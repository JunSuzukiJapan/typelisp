<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Kiến thức cơ bản về kiểu

typelisp là một ngôn ngữ có kiểu tĩnh. Chương này giải thích bộ kiểm tra kiểu làm gì cho bạn, các kiểu
bạn sẽ dùng nhiều nhất (`Option`, `Result`, struct và enum), và generics. Chương này giả định bạn đã
đọc [Bắt đầu](intro.md).

## 1. Kiểu tĩnh nghĩa là gì

Trong typelisp, kiểu của mọi biểu thức được xác định trước khi chương trình chạy. Một biểu thức có kiểu
không khớp là lỗi trước khi bất cứ thứ gì chạy.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; lỗi kiểu

(main)
```

Chạy tệp này sẽ dừng với một lỗi kiểu mà thậm chí không in ra `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Bạn cần viết kiểu cho đối số và giá trị trả về của hàm, biến toàn cục và trường của struct. Kiểu của một
biến `let` được lấy từ giá trị ban đầu của nó.

Các kiểu chính:

| Kiểu | Giá trị ví dụ |
|---|---|
| `int` | `42`, `-7` (số nguyên độ chính xác tùy ý) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Số nguyên có độ rộng cố định |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Kiểu trả về của hàm không trả về giá trị |

Không có cách hỏi kiểu của một giá trị lúc chạy (không có `typep` hay `type-of` của Common Lisp), vì
mọi kiểu đã được biết trước khi chương trình chạy.

## 2. `Option<T>`: giá trị có thể không có

typelisp không có `nil`. "Có thể không có giá trị" được biểu diễn bằng kiểu `Option<T>`. Một giá trị
`Option<T>` hoặc là `some`, giữ một giá trị kiểu `T`, hoặc là `none`, không giữ gì.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` không phải `int`, nên không thể dùng nguyên trạng trong phép tính số học.
`(+ (safe-div 10 2) 1)` là lỗi kiểu. Để dùng thứ bên trong, hãy tách `some` khỏi `none` bằng `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- Ở nhánh `(some q)`, nội dung được gán vào biến `q`.
- `match` kiểm tra rằng các nhánh của nó **bao phủ mọi trường hợp**. Quên nhánh `(none)` là lỗi kiểu.

### Vì sao không có nil

Trong nhiều ngôn ngữ, `nil` (`null`) có thể thế chỗ cho giá trị của bất kỳ kiểu nào. Kết quả là việc
quên xử lý trường hợp "không có giá trị" không bị phát hiện cho đến khi chương trình chạy. Trong
typelisp, một chỗ có thể thiếu giá trị có kiểu `Option<T>`, và mã không qua được bộ kiểm tra kiểu trừ
khi `match` xử lý trường hợp `none`. Một trường hợp bị quên được phát hiện trước khi chương trình chạy.

Điều kiện cũng theo cùng ý tưởng: chỉ một `bool` mới có thể là điều kiện của `if`. Không có quy tắc
như "mọi thứ khác `nil` đều đúng" của Common Lisp.

### Các thao tác thường dùng

| Dạng | Ý nghĩa |
|---|---|
| `(unwrap-or opt default)` | Nội dung với `some`; giá trị mặc định với `none` |
| `(unwrap opt)` | Lấy nội dung ra. Dừng chương trình với `none` |
| `(is-some opt)` / `(is-none opt)` | Kiểm tra nó là cái nào |

Nhiều hàm của thư viện chuẩn trả về `Option`. Ví dụ, `position` trả về vị trí trong `some` nếu tìm thấy
phần tử và `none` nếu không.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: thao tác có thể thất bại

Một thao tác có thể thất bại trả về `Result<T,E>`: `ok` giữ một giá trị kiểu `T` khi thành công, hoặc
`err` giữ một lỗi `E` khi thất bại.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Các hàm của riêng bạn cũng có thể trả về `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Hãy dùng `Option` khi giá trị bị thiếu không cần giải thích, và `Result` khi bạn muốn nói vì sao một việc
thất bại. [Xử lý lỗi](errors.md) đi sâu vào cách xử lý lỗi.

## 4. `defstruct`: struct

Một kiểu có các trường được đặt tên được định nghĩa bằng `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

Định nghĩa này cho bạn những thứ sau:

```lisp
(let ((p (point::new 3 4)))     ; tạo một giá trị (đối số theo thứ tự trường)
  (println "~a" p::x)           ; đọc một trường; (x p) cũng dùng được
  (setf p::x 10)                ; thay đổi nó
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Để đưa cho struct các hàm của riêng nó, hãy dùng `defmethod`. Kiểu của đối số đầu tiên (`self`) quyết
định phương thức thuộc về kiểu nào.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Viết chỉ tên kiểu thay cho đối số `self` sẽ tạo một hàm được gọi dưới dạng `point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: một trong nhiều hình dạng

Một giá trị là một trong nhiều hình dạng, như "một hình tròn, một hình chữ nhật hoặc một điểm", được
định nghĩa bằng `defenum`. Mỗi hình dạng được gọi là một **variant**. Mỗi variant có thể giữ số lượng
và kiểu giá trị khác nhau.

```lisp
(defenum shape
  (circle int)        ; bán kính
  (rect int int)      ; chiều rộng và chiều cao
  (dot))              ; không giữ giá trị nào
```

Giá trị được tạo với tên kiểu ở phía trước, như `shape::circle`. Trong `match`, chúng được tách ra theo
tên variant.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Ở đây `match` cũng kiểm tra rằng mọi trường hợp đều được bao phủ. Nếu sau này bạn thêm một variant vào
`shape`, mọi `match` không xử lý nó sẽ trở thành lỗi kiểu, nên không bỏ sót chỗ nào cần sửa.

Sau `(use shape)`, bạn có thể viết `(rect 5 6)` mà không cần tên kiểu.

`Option` và `Result` là các enum được xây dựng bằng cùng cơ chế này.

## 6. Generics

Một hàm hoạt động với mọi kiểu được định nghĩa với một **tham số kiểu** `<T>` sau tên của nó.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Bạn không đưa kiểu khi gọi. `T` được suy ra từ các đối số.

```lisp
(first-or ints 7)          ; T là int
(first-or names "none")    ; T là string
(first-or ints "none")     ; lỗi kiểu: ints là Vector<int>, nên T là int
```

Struct và enum cũng có thể là generic. `Vector<T>`, `Option<T>` và `Result<T,E>` là các kiểu loại này.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Bên trong một hàm generic không biết gì về `T`, nên bạn không thể so sánh hay cộng các giá trị kiểu `T`.
Để yêu cầu điều như "mọi kiểu có thể so sánh được", hãy dùng trait ([Trait](traits.md)).

## 7. Đặt cho kiểu một tên khác

`deftype` đặt cho một kiểu một tên khác.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` chỉ là một cách viết khác của `int`, không phải kiểu mới. Truyền một `int` thông thường vào chỗ
mong đợi `meters` không phải lỗi. Nếu bạn muốn tách chúng ra, hãy tạo một struct, như
`(defstruct meters (value int))`.

## 8. Nên đọc gì tiếp theo

- [Trait](traits.md): đưa cho các kiểu những thao tác chung
- [Kiểu](../reference/types.md): các kiểu dựng sẵn và những trait mà mỗi kiểu triển khai
