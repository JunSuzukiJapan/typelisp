<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Macro

Macro là một hàm nhận một chương trình và trả về một chương trình. Macro cho phép bạn tạo cú pháp mới
mà hàm không thể diễn đạt. Macro của typelisp hoạt động giống `defmacro` của Common Lisp. Chương này
giả định bạn đã đọc phần "Danh sách (S-expression)" trong [Bắt đầu](intro.md).

## 1. Macro khác hàm ở đâu

Một hàm nhận các đối số **sau khi chúng được đánh giá**. Một macro nhận chúng **dưới dạng biểu thức,
trước khi đánh giá** (là dữ liệu S-expression), dựng một biểu thức khác và trả về nó. Biểu thức được trả
về thay thế lời gọi macro, và chỉ khi đó nó mới được kiểm tra kiểu và chạy. Việc thay thế này được gọi
là **khai triển**.

Ví dụ, cú pháp như `unless` không thể viết dưới dạng hàm. Nếu là hàm, thân sẽ được đánh giá trước dù
điều kiện đúng.

## 2. `defmacro` và quasiquote

Hãy tạo `my-unless`, chỉ chạy thân của nó khi điều kiện sai.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Đối số của macro không được viết kiểu. Mọi đối số đều là dữ liệu S-expression.
- `&rest body` nhận các đối số còn lại gộp thành một danh sách.
- Một biểu thức bắt đầu bằng `` ` `` (quasiquote) được dựng thành dữ liệu, đúng như đã viết. Bên trong nó:
  - `,test` chèn nội dung của biến `test` vào vị trí đó.
  - `,@body` nối các phần tử của danh sách `body` vào vị trí đó.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Bạn có thể kiểm tra khai triển bằng `macroexpand-1`. Khi viết macro, xem khai triển của nó trước là cách
nhanh nhất để tiến tới.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Khai triển cũng được kiểm tra kiểu

Biểu thức mà macro trả về được kiểm tra kiểu như bất kỳ biểu thức nào bạn tự viết.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Lỗi được báo tại nơi macro được gọi.

Các quy tắc rằng nhánh else của `if` không thể bỏ, và cả hai nhánh của một `if` phải có cùng kiểu, áp
dụng cho khai triển nguyên trạng. `my-unless` ở trên kết thúc bằng `(progn ,@body ())` để, bất kể kiểu
của biểu thức cuối trong thân là gì, cả hai nhánh của `if` đều có kiểu `()`.

## 4. Xung đột tên và `gensym`

Một macro đơn giản hoán đổi giá trị của hai biến trông như sau:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Nó chạy được trong hầu hết trường hợp, nhưng hỏng khi biến của bên gọi tình cờ tên là `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (không được hoán đổi)
```

Khai triển là `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, và `tmp` do macro tạo ra che mất
`tmp` của bên gọi.

Để tránh điều này, hãy tạo tên của các biến dùng bên trong macro bằng `gensym`. `gensym` trả về một
symbol mới mà không thể viết ở bất cứ đâu trong chương trình.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Giống như trong Common Lisp, macro của typelisp không tự động ngăn xung đột tên (chúng không vệ sinh,
unhygienic). Hãy nhớ: **dùng `gensym` cho các ràng buộc mà macro tạo ra.**

Tương tự, một macro lặp lại thân của nó một số lần cho trước có thể viết như sau:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Khai triển khác nhau tùy theo đối số

Thân macro là mã typelisp thông thường, nên nó có thể xem xét các đối số của mình bằng `if` hoặc `match`
và dựng một khai triển khác. Các đối số là dữ liệu S-expression (`Option<Sexpr>`), và danh sách rỗng là
`none`.

Hãy tạo `my-and`, trả về `true` nếu mọi điều kiện của nó đều đúng.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; không có đối số
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; chỉ có một
         `(if ,f (my-and ,@more) false)))             ; hai hoặc nhiều hơn
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` là một mẫu lấy đầu của danh sách vào `f` và phần còn lại vào `more`.
- `sexpr-null` kiểm tra xem dữ liệu S-expression có phải là danh sách rỗng hay không.
- Nhánh `_` cuối cùng là cần thiết vì dữ liệu S-expression có những dạng khác ngoài danh sách (số,
  chuỗi, v.v.), và `match` yêu cầu phải bao phủ cả chúng. Đối số `&rest` luôn là một danh sách, nên
  nhánh này thực tế không bao giờ chạy.
- Một macro có thể gọi chính nó trong khai triển của mình. Việc khai triển lặp lại cho đến khi không
  còn lời gọi macro nào.

## 6. Đối số tùy chọn

`&optional` nhận các đối số có thể bỏ qua. Có thể đưa ra giá trị mặc định.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` nhận các đối số keyword
([Tham chiếu cú pháp 3.14](../reference/syntax.md#314-defmacro--định-nghĩa-macro)).

## 7. `macrolet`: macro chỉ cho một chỗ

Một macro chỉ dùng bên trong một biểu thức có thể được định nghĩa bằng `macrolet`. Nó không nhìn thấy
được từ bên ngoài.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Những điều cần ghi nhớ

- **Một macro chỉ có thể được gọi sau định nghĩa của nó.** Như với hàm, hãy định nghĩa nó gần đầu tệp.
- Làm cho macro khả dụng với các module khác bằng `(pub defmacro ...)`.
- Phần lớn cú pháp chuẩn, gồm `when`, `unless`, `cond`, `and`, `or` và `dotimes`, được định nghĩa dưới
  dạng macro. Bạn có thể xem bên trong có gì bằng `(macroexpand '(when true 1))`.
- Nếu một việc có thể viết dưới dạng hàm, hãy viết dưới dạng hàm. Macro không thể được truyền như giá
  trị, và bạn phải đọc khai triển của chúng để hiểu chúng làm gì.

## 9. Nên đọc gì tiếp theo

- [Xử lý lỗi](errors.md): `Result`, `panic`, `catch` / `throw`
- [Các hàm macro](../reference/functions/system.md#8-macro): `gensym`, `macroexpand` và nhiều hơn nữa
