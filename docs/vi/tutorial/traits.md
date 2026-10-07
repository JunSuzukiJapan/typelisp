<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Trait

Trait là một lời hứa rằng "kiểu này hỗ trợ những thao tác này". Trait cho phép nhiều kiểu chia sẻ các
thao tác cùng tên, để một hàm dùng chúng không phải viết lại cho từng kiểu. Chúng hoạt động gần như
y hệt trait của Rust. Chương này giả định bạn đã đọc [Kiến thức cơ bản về kiểu](types.md).

## 1. Định nghĩa và triển khai một trait

Hãy định nghĩa các thao tác trả về diện tích và tên của một hình thành trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` sau tên trait là danh sách các trait mà nó kế thừa (mục 4). Để trống khi không có.
- Mỗi dòng khai báo một phương thức. `Self` đại diện cho "kiểu đang triển khai trait này".

Để triển khai một trait cho một kiểu, hãy viết một `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Các phương thức đã triển khai được gọi giống hệt hàm thông thường.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Bỏ sót dù chỉ một trong các phương thức mà trait khai báo là lỗi kiểu tại `impl`.

## 2. Ràng buộc trait: "mọi kiểu triển khai trait này"

Bạn có thể đặt điều kiện lên tham số kiểu của một hàm generic bằng `where`. Điều này được gọi là
**ràng buộc trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Nhờ `(where (Shape T))`, thân hàm có thể dùng `name` và `area` trên các giá trị kiểu `T`. Nếu không có
ràng buộc thì không biết gì về `T`, nên không thể gọi chúng.

Truyền một kiểu không triển khai `Shape` là lỗi kiểu tại lời gọi.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Một hàm generic có bản sao riêng cho mỗi kiểu mà nó được gọi. Không có kiểm tra kiểu hay rẽ nhánh nào
lúc chạy.

## 3. Triển khai mặc định

Nếu một phương thức của trait có thân, thân đó được dùng khi một `impl` bỏ phương thức ra.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe là bản mặc định

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; bản viết ở đây được ưu tiên

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Triển khai các trait chuẩn

Thư viện chuẩn cũng có các trait. Triển khai một trait làm cho các hàm chuẩn dùng nó trở nên khả dụng
cho kiểu của bạn.

| Trait | Phương thức cần triển khai | Nó cho phép gì |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, mẫu `(= expr)` của `match`, v.v. |
| `Ord` | `less` | `less-equal`, `greater`, v.v. `Ord` kế thừa từ `Eq` |
| `print-object` | `print-object` | Cách `println` và các hàm tương tự hiển thị giá trị |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort`, v.v. |
| `Error` | `message`, `source` | Dùng làm kiểu lỗi ([Xử lý lỗi](errors.md)) |

Hãy triển khai `Eq` và `Ord` cho một kiểu biểu diễn một khoản tiền. Vì `Ord` kế thừa từ `Eq`, `impl`
của `Eq` phải đứng trước.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (triển khai mặc định trong Ord)
```

Triển khai `print-object` quyết định cách `println` hiển thị giá trị. Đối số `escape` là `true` khi
cần một dạng có thể đọc ngược lại, như với `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Kết hợp với ràng buộc trait, bạn có thể viết một hàm hoạt động với mọi kiểu triển khai `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Với một `Vector` chứa các giá trị `money` là 300, 900 và 100 theo thứ tự đó, nó trả về
`(some 900 yen)`.

## 5. `:dyn`: xử lý chung các giá trị thuộc kiểu khác nhau

Mọi phần tử của một `Vector<T>` có cùng kiểu, nên các giá trị `circle` và `rect` không thể cùng nằm
trong một `Vector<circle>`. Để xử lý chung "thứ gì đó triển khai `Shape`", hãy dùng kiểu `:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Một giá trị `circle` hoặc `rect` đặt vào chỗ mong đợi `:dyn Shape` được chuyển đổi tự động.
- Lời gọi `(area s)` chạy `area` của kiểu nào được quyết định lúc chạy, theo kiểu của thứ `s` đang giữ.
- Đặt một giá trị có kiểu không triển khai `Shape` vào chỗ mong đợi `:dyn Shape` là lỗi kiểu.

Chọn giữa ràng buộc trait ở mục 2 và `:dyn`:

| | Ràng buộc trait (`where`) | `:dyn Trait` |
|---|---|---|
| Khi nào phương thức được gọi được quyết định | Trước khi chạy | Lúc chạy |
| Trộn các kiểu trong một `Vector` | Không thể | Có thể |
| Các kiểu dùng được | Không hạn chế | Struct, enum, `int`, `string`, `f64` và một số khác (không phải `bool`, `char`, `symbol`, `i32` và tương tự) |

Danh sách chính xác các kiểu có thể dùng nằm ở
[Tham chiếu cú pháp 3.9](../reference/syntax.md#39-deftrait--impl--trait).

Một số trait không thể dùng với `:dyn`: những trait có phương thức dùng `Self` cho một đối số khác
`self` hoặc cho giá trị trả về (như `equals` trong `Eq`). Vì kiểu không được biết cho đến lúc chạy,
không có cách nào tạo ra "một giá trị cùng kiểu".

## 6. Hạn chế

- Hãy giữ định nghĩa của trait, các `impl` cho nó và mã dùng nó qua `:dyn` trong một module (tệp). Hiện
  chưa thể làm cho một trait hiển thị với các module khác.
- Kiểu và trait dùng chung một không gian tên. Trong một module, một kiểu và một trait không thể trùng
  tên.

## 7. Nên đọc gì tiếp theo

- [Macro](macros.md): định nghĩa cú pháp của riêng bạn
- [Tham chiếu cú pháp 3.9](../reference/syntax.md#39-deftrait--impl--trait): blanket implementation,
  kiểu liên kết (associated type) và nhiều hơn nữa
- [Các trait chuẩn](../reference/functions/traits.md): danh sách các trait trong thư viện chuẩn
