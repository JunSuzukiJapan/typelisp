<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Dành cho lập trình viên Common Lisp

typelisp kế thừa cú pháp của Common Lisp (CL) và nhiều tên hàm của nó, nhưng là một ngôn ngữ có kiểu
tĩnh. Vì vậy, mã CL không phải lúc nào cũng chạy được nguyên trạng. Tài liệu này tập hợp những điểm
mà người quen CL hay vấp, kèm cách viết lại mã.

## 1. Không có `nil` và `t`

Các giá trị boolean là `true` và `false`. `nil` và `t` không được định nghĩa.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Chỉ một `bool` mới có thể làm điều kiện.** Viết `0` hay danh sách rỗng làm điều kiện là lỗi kiểu.
  Không có quy tắc "mọi thứ khác nil đều đúng".
- **Nhánh else của `if` không thể bỏ.** `(if c x)` là lỗi. Khi không cần nhánh else, hãy dùng
  `when` / `unless`.
- **"Không có giá trị" được biểu diễn bằng `Option<T>`.** Một hàm trả về nil trong CL với nghĩa "không
  tìm thấy" thì ở đây trả về `(some x)` hoặc `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Danh sách rỗng `()` tùy ngữ cảnh là giá trị của kiểu Unit (giá trị trả về của hàm không trả về gì)
  hoặc là danh sách rỗng của dữ liệu S-expression. Nó là một giá trị khác với `false`.

## 2. Viết kiểu

Đối số và giá trị trả về của hàm phải có kiểu.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; một hàm generic
  (unwrap-or (first (iter v)) default))
```

- Không thể viết một định nghĩa không có kiểu, như `(defun f (x) x)`.
- Biến toàn cục như `defvar` cũng cần kiểu: `(defvar (count int) 0)`.
- `the` không phải là kiểm tra lúc chạy mà là chú thích cho bộ kiểm tra kiểu.
- **Không có cách kiểm tra kiểu lúc chạy.** Không có `typep` hay `type-of`, vì kiểu của mọi giá trị
  được cố định ở thời điểm biên dịch. Để chấp nhận một trong nhiều kiểu, hãy tạo một sum type bằng
  `defenum` hoặc dùng trait.
- `deftype` định nghĩa một bí danh kiểu. Không thể tạo kiểu mô tả một khoảng giá trị, như
  `(deftype small () '(integer 0 9))`.

Kiểu số nguyên mặc định `int` có độ chính xác tùy ý; giống integer của CL, không có giới hạn trên về
độ lớn. Các kiểu độ rộng cố định `i8` đến `i32` và `u8` đến `u32` cũng có. Không có kiểu số nguyên
độ rộng cố định 64 bit.

## 3. Hàm như giá trị

typelisp không tách không gian tên của hàm và của biến. Tên của một hàm có thể được truyền như một
giá trị nguyên dạng. Không có `#'` và không có `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; gọi trực tiếp, không dùng funcall

(apply-to twice 5)                        ; twice, không phải #'twice
```

- Các hàm dựng sẵn như `+` và `1+` cũng có thể được truyền như giá trị nguyên dạng, ở nơi kiểu của đối
  số đã cố định, như `(fn (int) int)`. Khi truyền cho một hàm generic như `foldl` hay `map`, không
  biết `+` của kiểu nào được ý chỉ, nên hãy bọc nó trong một `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Các hàm trên dãy nhận **tập hợp trước và hàm sau**: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. Đây là ngược với `(mapcar f list)` của CL.
- `lambda` không thể dùng `&optional` hay `&key` (`&rest` thì dùng được).
- **Không thể gọi một hàm trước khi nó được định nghĩa.** Trong CL bạn có thể gọi một hàm sẽ định nghĩa
  sau, nhưng ở đây điều đó cho `no such function`. Với các hàm đệ quy lẫn nhau, hãy khai báo trước một
  trong số chúng bằng `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Danh sách và Vector

Thứ tương ứng với list của CL là **dữ liệu S-expression**, có kiểu `Option<Sexpr>` (danh sách rỗng là
`none`). `(list 1 2 3)` và `'(a b c)` có kiểu này. Dữ liệu S-expression là thứ mà macro và `read` làm
việc với; với một vùng chứa dữ liệu thông thường, hãy dùng **`Vector<T>`**.

| Điều bạn muốn | CL | typelisp |
|---|---|---|
| Đầu và phần còn lại của một S-expression | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Duyệt một danh sách S-expression | `(dolist (x xs) ...)` | Giống nhau |
| Một dãy phần tử cùng kiểu | Một list hoặc một vector | `Vector<T>` |
| Một cặp | `(cons a b)` | `(cons a b)` (kiểu của nó là `cons-cell<A,B>`) |
| Ánh xạ | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` là các hàm truy cập của cặp `cons-cell<A,B>` tạo bằng `cons`. Chúng không thể dùng trên
danh sách S-expression.

Tạo một `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Các hàm trên dãy như `map`, `filter`, `sort` và `find` hoạt động trên các giá trị triển khai trait
`Iter`. Hãy truyền một `Vector` sau khi biến nó thành iterator bằng `(iter v)`.

## 5. Không có đa giá trị

Không có `values` và không có `multiple-value-bind`. Các hàm trả về nhiều giá trị trong CL thì ở đây
trả về một cặp hoặc một struct.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → một `cons-cell` có `car` là 3 và `cdr` là 1 |
| `(decode-universal-time t)` → 9 giá trị | Một struct `decoded-time` |
| `(read-from-string s)` → giá trị, vị trí | `(read-from-string s)` trả về một `cons-cell` gồm giá trị và vị trí bên trong một `Result`. Nếu chỉ cần giá trị, dùng `(read s)` |

## 6. Không có biến đặc biệt (ràng buộc động)

`let` luôn ràng buộc theo phạm vi từ vựng. Nếu bạn ràng buộc lại một biến định nghĩa bằng `defvar`
qua `let`, các hàm được gọi từ đó vẫn thấy giá trị ban đầu.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 trong CL, 1 trong typelisp
```

Để thay đổi tạm thời một biến điều khiển như `*print-base*`, hãy dùng `dlet`. Nó gán giá trị và khôi
phục giá trị gốc dù thân hàm được rời đi bằng cách nào.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` ghi đè chính biến toàn cục, nên nó không phải ràng buộc theo từng thread.

## 7. Không dùng hệ thống condition

Không có `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` hay `signal`.
Chúng không hợp với kiểu tĩnh. Thay vào đó, hai thứ sau được dùng cho các mục đích khác nhau:

- **Các lỗi có thể khôi phục trả về `Result<T,E>`.** Bên gọi tách `ok` / `err` bằng `match`. Không có
  cách viết tắt như `?` của Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Các lỗi không thể khôi phục (bug) là `panic`.** `(panic "message")`, truyền `none` cho `unwrap`,
  chia cho 0 và chỉ số ngoài phạm vi thuộc loại này, và chương trình dừng lại. Phần dọn dẹp của
  `unwind-protect` chạy trước khi nó dừng.

Các kiểu lỗi được thống nhất bởi trait `Error`, và `(message e)` cho thông điệp. Cách tạo kiểu lỗi của
riêng bạn nằm ở
[Option, Result và các kiểu lỗi](../reference/functions/option-result.md#3-các-kiểu-lỗi-và-trait-error).
`assert` và `warn` dùng được như trong CL.

`catch` / `throw` / `unwind-protect` có tồn tại. Tuy nhiên, tag của `catch` bị giới hạn ở một ký hiệu
literal không được đánh giá (`'done`), và các giá trị được ném với cùng một tag có một kiểu duy nhất.

## 8. Không có CLOS

Không có `defclass`, `defgeneric` hay kết hợp phương thức.

- Các kiểu dữ liệu được định nghĩa bằng `defstruct` (struct) và `defenum` (sum type).
- `defmethod` định nghĩa các phương thức mà đích của chúng được quyết định chỉ bởi **kiểu tĩnh của đối
  số đầu tiên**. Không có multiple dispatch.
- Để đưa ra các thao tác chung cho nhiều kiểu, hãy dùng trait (`deftrait` / `impl`). Với các giá trị mà
  kiểu cụ thể được quyết định lúc chạy, hãy dùng kiểu `:dyn Trait`
  ([Tham chiếu cú pháp 3.9](../reference/syntax.md#39-deftrait--impl--trait)).

`defstruct` khác ở những điểm sau:

- Hàm khởi tạo là `TypeName::new`: `(point::new 1 2)`. Nếu muốn một tên như `make-point`, tùy chọn
  `(:constructor make-point)` sẽ tạo ra nó.
- Ngoài `(x p)`, hàm truy cập có thể viết là `p::x`. Thay đổi bằng `(setf p::x 5)`.
- Không có vị từ (`point-p`) nào được tạo. Không có `:conc-name`, `:type` hay `:named`.
- `:include` chỉ kế thừa các slot; kiểu không trở thành kiểu con của kiểu cha.

## 9. Module thay cho package

Không có package. Không gian tên là module, và mỗi tệp tự nó là một module. Thay vì `pkg:symbol`, hãy
viết `module::name`, và đưa tên vào bằng `use` ([Module và cách bố trí tệp](modules.md)).

Keyword `:foo` có tồn tại và là các symbol tự đánh giá thành chính chúng. Vì không có package, dấu hai
chấm là một phần của tên: `(symbol->string :foo)` trả về `":foo"`.

## 10. Khác biệt về cách đọc và cú pháp

- Chữ hoa và chữ thường không được phân biệt (symbol trở thành chữ thường khi đọc). Điều này giống CL.
- Không có `#'` (mục 3). Literal số phức `#c(...)` không thể đọc; hãy tạo số phức bằng
  `(complex 1.0 2.0)`.
- Các mệnh đề của `loop` mở rộng được viết bằng keyword: `(loop :for i :from 1 :to 3 :collect i)`.
  Một `loop` không bắt đầu bằng keyword là vòng lặp vô hạn đơn giản, được thoát bằng `(break)` hoặc
  `(return value)`. `return` thoát khỏi vòng lặp trong cùng nhất (để thoát khỏi một hàm, dùng
  `return-from`).
- Đích của `format` là `false` (trả về một chuỗi), `true` (đầu ra chuẩn) hoặc một stream. Các chỉ thị
  định dạng giống như trong CL.
- Đọc từ một chuỗi là `(read "...")`, và đọc từ một stream là `(read-sexpr s)`. Cả hai đều trả về một
  `Result`.
- `eval` kiểm tra kiểu biểu thức được đưa vào trước khi đánh giá, và trả về một `Result`. Không thể
  tham chiếu tiến, giống như trong mã nguồn.
- Không có `eval-when`.
- Tên hàm không dùng hậu tố `?` hay `!`. Vị từ được đặt tên bằng `-p` / `p` như trong CL (`zerop`,
  `sexpr-null`), hoặc với `is-` ở phía trước (`is-some`).

## 11. Các hàm chính có tên khác

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (từ một stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Phiên bản hai đối số của `floor` và các hàm tương tự | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (thứ tự đối số đảo ngược; mục 4) |
| `length` (của một vector) | `len` |
| `hash-table-count` | `count` / `size` |

Danh sách các hàm nằm ở [Hàm dựng sẵn](../reference/functions/README.md).

## 12. Những thứ khác không tồn tại

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` và `copy-readtable`, `readtable-case` (bản thân các reader macro có thể được định
  nghĩa bằng `set-macro-character`)
- Pathname logic và pathname có ký tự đại diện
- `input-stream-p` / `output-stream-p` (hướng của một stream được quyết định bởi kiểu của nó)
