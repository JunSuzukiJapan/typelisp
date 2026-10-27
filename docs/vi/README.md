<!-- translated-from: docs/ja/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Tài liệu typelisp (Tiếng Việt)

typelisp là một ngôn ngữ Lisp có kiểu tĩnh. Về cách cài đặt và build, xem [README.md](../../README.md)
(bằng tiếng Anh) ở thư mục gốc của repository.

## Đặc điểm

### Task và kênh

Khởi động các task (thread nhẹ) và truyền giá trị giữa chúng qua các kênh. Xem chi tiết tại [Lập
trình đồng thời](tutorial/concurrency.md).

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Option và Result thay cho nil

Không có `nil`. Hàm có thể không có giá trị thì trả về `Option<T>`, hàm có thể thất bại thì trả về
`Result<T,E>`; nội dung được lấy ra bằng `match`. Dùng `Option<string>` như một `string`, hoặc quên
trường hợp `none` trong `match`, là lỗi kiểu ngay trước khi chương trình chạy. Xem chi tiết tại
[Kiến thức cơ bản về kiểu](tutorial/types.md) và [Xử lý lỗi](tutorial/errors.md).

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Macro được kiểm tra kiểu

Macro được viết bằng `defmacro`, quasiquote và `gensym` giống như Common Lisp. Mã sau khi mở rộng
cũng được kiểm tra kiểu như mã viết tay; nếu không khớp kiểu, lỗi được báo tại chỗ gọi macro. Xem
chi tiết tại [Macro](tutorial/macros.md).

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Hướng dẫn nhập môn

Nếu mới làm quen với typelisp, hãy đọc theo thứ tự sau.

- [Bắt đầu](tutorial/intro.md): REPL, hàm, biến, rẽ nhánh, vòng lặp, danh sách và `Vector`
- [Kiến thức cơ bản về kiểu](tutorial/types.md): kiểu tĩnh, `Option`, `Result`, struct, enum, generics
- [Trait](tutorial/traits.md): `deftrait` / `impl`, ràng buộc trait, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Xử lý lỗi](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Lập trình đồng thời](tutorial/concurrency.md): task, kênh, `select`, `Mutex`, `thread`

## Hướng dẫn chuyên đề

- [Module và cách bố trí tệp](guide/modules.md): `use`, `pub`, cách tệp tương ứng với module
- [Biên dịch](guide/compile.md): JIT, tạo tệp thực thi bằng biên dịch AOT, dump
- [Vào/ra tệp, stream và mạng](guide/io.md): tệp, pathname, TCP / TLS / UDP, phân giải tên
- [C FFI](guide/ffi.md): gọi hàm C bằng `defffi` (gồm cả callback và struct C với `def-c-struct`)
- [Tích hợp trình soạn thảo](guide/editors.md): `typl-lsp` và cách thiết lập VS Code / Emacs
- [Dành cho lập trình viên Common Lisp](guide/from-common-lisp.md): typelisp khác CL ở đâu và cách viết lại mã CL

## Tham chiếu

- [Tham chiếu cú pháp](reference/syntax.md): cú pháp từ vựng, cách viết kiểu, định nghĩa, các dạng điều khiển, biên dịch, lập trình đồng thời
- [Hàm dựng sẵn](reference/functions/README.md): hàm dựng sẵn, phương thức và thư viện chuẩn
- [Kiểu](reference/types.md): các kiểu và những trait mà mỗi kiểu triển khai
- [Thông báo lỗi](reference/errors.md): ý nghĩa của các lỗi thường gặp và cách sửa

## Tích hợp trình soạn thảo

Các bước thiết lập có trong [hướng dẫn Tích hợp trình soạn thảo](guide/editors.md). Phím tắt và
cài đặt của từng trình soạn thảo được liệt kê trong các tài liệu sau:

- [Emacs (typelisp-mode)](../../editor/emacs/README_vi.md)
- [VS Code](../../editor/vscode/README_vi.md)
