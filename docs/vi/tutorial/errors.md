<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Xử lý lỗi

Việc xử lý lỗi trong typelisp chia các thất bại thành hai loại.

| Loại thất bại | Ví dụ | Cách biểu diễn |
|---|---|---|
| Thất bại có thể xảy ra (có thể khôi phục) | Thiếu một tệp, đầu vào không phải là số | Trả về một `Result<T,E>` |
| Sai sót trong chương trình (không thể khôi phục) | Chỉ số ngoài phạm vi, `unwrap` của `none`, chia cho không | Dừng bằng `panic` |

Ngoài ra còn có `catch` / `throw`, thoát khỏi nhiều lời gọi hàm cùng lúc, và `unwind-protect`, chạy
phần dọn dẹp dù thân được rời đi bằng cách nào. Chương này giả định bạn đã đọc phần về `Result` trong
[Kiến thức cơ bản về kiểu](types.md).

## 1. Trả về một `Result` và nhận nó bằng `match`

Đây là một hàm đọc số cổng từ một chuỗi. Nó có thể thất bại theo hai cách: đầu vào không phải là số,
hoặc nằm ngoài phạm vi.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Bên gọi tách thành công khỏi thất bại bằng `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Giá trị của một hàm trả về `Result` không thể dùng trừ khi `match` xử lý trường hợp `err`. Quên xử lý
  thất bại là lỗi kiểu.
- Lỗi từ `parse-int` là một giá trị kiểu `ParseIntError`. `(message e)` cho chuỗi thông điệp của nó.

## 2. Chuyển một thất bại lên cho bên gọi

Không có cách viết tắt như `?` của Rust. Khi lần lượt gọi nhiều hàm trả về `Result`, hãy viết phần "trả
thất bại về nguyên trạng" bằng `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Khi bạn biết một thao tác không thể thất bại, hoặc trong một script nhỏ mà dừng lại khi thất bại là
chấp nhận được, `unwrap` lấy nội dung ra. Nếu giá trị là `err`, nó panic. Nếu một giá trị mặc định là
đủ, hãy dùng `unwrap-or`.

## 3. Tạo kiểu lỗi của riêng bạn

Biểu diễn lỗi bằng một kiểu thay vì một chuỗi cho phép bên gọi rẽ nhánh theo loại lỗi. Một kiểu lỗi là
một `defenum` hoặc `defstruct` thông thường triển khai trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; thiếu một thiết lập
  (invalid string int))     ; một giá trị sai

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` trả về mô tả của lỗi.
- `source` trả về một lỗi khác đã gây ra lỗi này. Khi không có nguyên nhân, nó là `none`.

## 4. Kết hợp các loại lỗi khác nhau

Nếu một hàm gọi cả `parse-int` (`ParseIntError`) lẫn `check-workers` (`config-error`), có hai kiểu lỗi,
và chúng không thể cùng là `E` của một `Result<T,E>`. Trong trường hợp đó, hãy đặt `E` là `:dyn Error`
(một lỗi thuộc kiểu bất kỳ triển khai `Error`). Chuyển đổi từng lỗi bằng `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Với `"4"`, `"-1"` và `"abc"`, các kết quả là:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Về `:dyn`, xem mục 5 của [Trait](traits.md).

## 5. `panic`: sai sót trong chương trình

Khi chương trình đạt tới một trạng thái không bao giờ được phép xảy ra, hãy dừng nó bằng `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Kiểu của `panic` là `!` (nó không trả về), nên có thể viết nó ở bất cứ chỗ nào mong đợi một kiểu
  nào đó. Đó là lý do hai nhánh của `if` ở trên khớp nhau.
- Những thao tác này cũng panic: `unwrap` của `none` hoặc `err`, `get` với chỉ số ngoài phạm vi, và
  phép chia số nguyên cho không.
- `panic` dừng chương trình. Ngay cả khi nó xảy ra bên trong một task, toàn bộ chương trình dừng lại.
- Trong REPL, một `panic` không kết thúc REPL; nó chờ lần nhập tiếp theo.
- Bạn có thể viết `(todo)` cho "chưa viết" và `(unreachable)` cho "điểm này không bao giờ được tới".
  Cả hai đều panic.

`panic` không phải là sự thay thế cho `Result`. Với các thất bại có thể xảy ra, như đầu vào của người
dùng hay việc một tệp có tồn tại hay không, hãy dùng `Result`.

## 6. `catch` / `throw`: nhảy ra xuyên qua các hàm

`throw` nhảy thẳng ra `catch` bao ngoài có cùng tag, dù ở giữa có bao nhiêu lời gọi hàm.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Nếu `v` không có số âm, `validate` trả về `"all fine"`; nếu nó chứa `-7`, luồng điều khiển nhảy từ bên
trong `check-all` ra `catch`, và `catch` trả về `"negative: -7"`.

- Hãy viết tag dưới dạng một symbol thông thường, như `'bad-input`.
- **Mỗi tag mang các giá trị của đúng một kiểu.** Trong ví dụ trên `'bad-input` mang một `string`, nên
  ném một `int` với cùng tag là lỗi kiểu. Kiểu của thân `catch` cũng phải khớp với kiểu của tag.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Một `throw` không có `catch` cùng tag để tới là một lỗi.

Nếu bạn chỉ muốn thoát sớm từ bên trong một hàm, hãy dùng `return-from` thay cho `catch` / `throw`.
`return-from` không thể đi qua các hàm, nhưng đổi lại bạn có thể biết nó trả về đâu bằng cách đọc mã
nguồn.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: luôn dọn dẹp

`(unwind-protect body cleanup)` chạy phần dọn dẹp dù thân được rời đi bằng cách nào: khi nó kết thúc
bình thường, khi nó được rời đi bằng `throw`, và khi nó panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Hãy dùng nó cho những việc như "luôn đóng tệp bạn đã mở" hay "luôn nhả khóa bạn đã lấy". `with-open-file`
và `with-lock` của thư viện chuẩn dùng `unwind-protect` bên trong.

## 8. Về hệ thống condition của Common Lisp

typelisp không áp dụng hệ thống condition của Common Lisp (`handler-case`, `restart-case`, v.v.). Nó
không thể hiện trong kiểu những thất bại nào một hàm có thể gây ra, điều này không hợp với kiểu tĩnh.
Các thất bại có thể xảy ra được viết vào kiểu bằng `Result`, và việc chuyển điều khiển được thực hiện
bằng `catch` / `throw`.

## 9. Nên đọc gì tiếp theo

- [Lập trình đồng thời](concurrency.md): task và kênh
- [Option, Result và các kiểu lỗi](../reference/functions/option-result.md): danh sách các hàm
- [Thông báo lỗi](../reference/errors.md): ý nghĩa của các lỗi thường gặp và cách sửa
