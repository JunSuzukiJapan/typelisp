<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result và các kiểu lỗi

## 1. `Option<T>` / `Result<T,E>`

Hàm khởi tạo: `Option<T>` có `Some(T)` / `None`. `Result<T,E>` có `Ok(T)` / `Err(E)`. `E` có thể là bất kỳ
kiểu nào: các kiểu lỗi cụ thể dựng sẵn, và các kiểu bạn tự viết bằng `defstruct`/`defenum`, đều vừa ở đó
như nhau (chương 3).

| Tên | Dạng | Option | Result | Mô tả |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Lấy giá trị ra. Panic với `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Giá trị, hoặc giá trị mặc định |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Có phải là `Some` không |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Có phải là `None` không |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Có phải là `Ok` không |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Có phải là `Err` không |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Lấy giá trị ra. Gặp `None`/`Err` thì panic với `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Giá trị, hoặc kết quả của `f`. `f` chỉ được gọi khi gặp `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Áp dụng `f` lên nội dung của `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Áp dụng `f` lên nội dung của `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Gặp `Some`/`Ok` thì đưa nội dung cho `f` và trả về kết quả của nó |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Gặp `None`/`Err` thì trả về kết quả của `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Biến `Some(v)` thành `Ok(v)` và `None` thành `Err(e)` |

Các hàm khởi tạo là `Option::some`/`Option::none`/`Result::ok`/`Result::err` (hoặc, sau
`(use option)`/`(use result)`, các tên trần `some`/`none`/`ok`/`err`).

Rẽ nhánh được viết tường minh bằng `match`, hoặc nối chuỗi bằng `map`/`and-then` và các phương thức
khác ở trên. Không có cú pháp tương ứng với `?` của Rust.

`map` của `Option`/`Result` là một phương thức, khác với `map` của [dãy](sequences.md). Nó được gọi
khi kiểu của đối số đầu tiên là `Option`/`Result`.

Macro `->` lần lượt truyền một giá trị làm đối số đầu tiên cho từng dạng tiếp theo (giống `->` của
Clojure). `(-> x (f a) (g b))` trở thành `(g (f x a) b)`. Một tên không có ngoặc, `h`, được xem là
`(h x)`. Đối số đầu tiên của phương thức là đối tượng nhận, nên các combinator nối chuỗi được nguyên
như vậy:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. Biểu diễn lúc chạy của `Option<T>`

Như trong Rust, **`Option<T>` thường không tạo hộp (box)**. `some v` chính là `v` và `none` là giá trị
danh sách rỗng, không cấp phát và không gián tiếp. `Option<Sexpr>` (nơi danh sách rỗng là `none`),
`Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` và `Option<(fn ...)>` đều dùng dạng
này.

Hộp chỉ được dùng khi một giá trị kiểu `T` không thể phân biệt với giá trị danh sách rỗng:

| `T` | Biểu diễn | Lý do |
|---|---|---|
| `Option<U>` (lồng nhau) | Hộp | `none` bên trong sẽ là cùng giá trị với `none` bên ngoài |
| `()` | Hộp | Giá trị của `()` chính là giá trị danh sách rỗng |
| `ptr` / `c-long` / `c-ulong` | Hộp | Cả 64 bit đều là giá trị, không còn chỗ để phân biệt |
| Mọi thứ khác | Không hộp | — |

Biểu diễn được quyết định chỉ bởi kiểu và không thể đọc ra từ một giá trị. Khi in, `(some ...)`/`none` được
dựng lại từ kiểu tĩnh, nên `(format false "~a" opt)` in ra `(some 1)`. Có hai hạn chế:

- **Không thể đưa nó vào `:dyn Trait`** (truyền một giá trị `Option<int>` mà bạn đã viết
  `(impl Speak Option<int> ...)` cho một `:dyn Speak` là lỗi).
- Một phép downcast `(the Option<T> ...)` từ một `Sexpr` **nêu tên một hàm khởi tạo**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Dạng gán toàn bộ giá trị,
  `(the Option<int> o)`, là lỗi.

## 3. Các kiểu lỗi và trait `Error`

Theo `std::error::Error` của Rust, **`Error` không phải là một kiểu mà là một trait**. Các kiểu cụ thể
biểu diễn lỗi tách riêng cho từng mục đích, và mỗi kiểu triển khai `Error`.

| Kiểu | Được tạo bởi |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Các thao tác tệp và stream ([Stream và tệp](streams-files.md)) |
| `NetError` | Các thao tác mạng ([Mạng](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` của CL: lựa chọn mặc định khi bạn chỉ muốn nói điều gì đã xảy ra |
| `WrappedError` | `(wrap-error msg cause)`. Một kiểu mang cả thông điệp của riêng bạn lẫn nguyên nhân; đó là lý do trait `Error` có `source` |

`ParseIntError` đến `NetError` mỗi kiểu là "một enum với một variant duy nhất giữ một chuỗi thông điệp",
và tên kiểu và tên variant giống nhau (`(match e ((ParseIntError m) m))`, được tạo bằng
`(ParseIntError::ParseIntError "...")`). Không có gì đặc biệt ở chúng: chúng được đối xử y hệt các kiểu
lỗi của riêng bạn viết bằng `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Thông điệp lỗi (một phương thức của trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Nguyên nhân mà lỗi này bao bọc, hoặc `None` nếu không có (`Error::source` của Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` triển khai `Error`) | Mở rộng một kiểu lỗi cụ thể thành trait object |
| `describe-error` | `(describe-error e)` | `E→string` (`E` triển khai `Error`) | Thông điệp và chuỗi nguyên nhân tìm được bằng cách theo `source`, mỗi nguyên nhân một dòng. CL không có thứ tương ứng ("caused by" của Rust) |

Nếu bạn triển khai `Error` cho kiểu lỗi của riêng mình, nó có thể được xử lý **giống hệt** các lỗi dựng
sẵn:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; kiểu cụ thể đi vào E nguyên trạng
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; xử lý mọi loại một cách đồng nhất
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Để gom nhiều kiểu lỗi vào một `Result`, hãy dùng `Result<T, :dyn Error>` (tương ứng với `Box<dyn Error>`
của Rust), và mở rộng các lỗi cụ thể bằng `as-dyn-error`. Vì không có `?`, phép chuyển đổi này được
viết tường minh:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Kiểu và trait dùng chung một không gian tên** (như trong Rust). Trong một module, một
`defstruct`/`defenum` và một trait không thể trùng tên, và việc viết tên trait ở vị trí kiểu được báo là
"`error` is a trait, not a type — write `:dyn error`".

Các thất bại không thể khôi phục được biểu diễn bằng `panic`. Về `panic` và `catch`/`throw`, xem
[Tham chiếu cú pháp](../syntax.md#8-lối-thoát-phi-cục-bộ-catch--throw--unwind-protect); về chính sách xử lý
lỗi, xem [chương 9 của cùng tài liệu](../syntax.md#9-chính-sách-xử-lý-lỗi).
