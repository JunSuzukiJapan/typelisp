<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Thông báo lỗi

Ý nghĩa của các thông báo lỗi chính từ `typl` và cách sửa chúng.

## 1. Đọc một thông báo lỗi

Lỗi được ghi ra đầu ra lỗi chuẩn theo dạng này:

```text
error: file:line:column: kind: message
```

`kind` cho bạn biết lỗi được phát hiện vào lúc nào.

| Loại | Khi nào | Ý nghĩa |
|---|---|---|
| `type error` | Trước khi chạy (lúc kiểm tra) | Sai sót về kiểu hoặc tên. Dạng đó không được chạy |
| (không có loại) | Khi đọc hoặc kiểm tra | Lỗi cú pháp như ngoặc không cân bằng, hoặc một tên không tìm thấy |
| `panic` | Khi đang chạy | Một thất bại không thể khôi phục. Chương trình dừng sau khi chạy phần dọn dẹp của `unwind-protect` |

Các dòng bắt đầu bằng `warning:` là cảnh báo, và quá trình xử lý vẫn tiếp tục.

`file:line:column` trỏ vào biểu thức có sai sót. Với lỗi lúc chạy xảy ra bên trong một hàm của thư viện
chuẩn, nó trỏ vào chỗ chương trình đã gọi hàm đó. Một số lỗi không có vị trí (như
`error: panic: ...`).

Ví dụ:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Điều này nghĩa là biểu thức ở dòng 1, cột 24 của `main.typl` là một `string` ở chỗ cần một `i32`.

## 2. Lỗi lúc kiểm tra

Các sai sót được phát hiện trước khi chạy. Dạng đó không được chạy cho đến khi chúng được sửa.

### 2.1 Kiểu

| Thông báo | Ý nghĩa và cách sửa |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Một biểu thức kiểu `U` đang ở chỗ cần kiểu `T`. Không có chuyển đổi ngầm; với số, hãy chuyển đổi bằng `(as T x)`. `int` và `i32` cũng là các kiểu khác nhau |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Literal không vừa với kiểu. Nếu bạn muốn cắt bớt, hãy viết `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Không có kiểu nào mang tên đó. Hãy định nghĩa kiểu trước dạng đầu tiên dùng nó (kiểu không có khai báo trước). Nếu bạn muốn nói một biến kiểu, hãy viết nó ở vị trí khai báo như `<foo>` sau tên hàm ([Tham chiếu cú pháp 3.6](syntax.md#36-defstruct--struct-kiểu-do-người-dùng-định-nghĩa)) |
| ``cannot infer type argument `t` for `vector::new` `` | Không xác định được một đối số kiểu. Hãy viết kiểu bằng `the`, như `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` không xử lý mọi variant. Hãy thêm các nhánh cho những variant còn thiếu, hoặc một nhánh `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Hàm yêu cầu một trait mà kiểu bạn truyền vào không triển khai. Hãy viết `(impl Eq pt ...)` ([Các trait chuẩn](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Một giá trị của kiểu không triển khai trait được truyền vào chỗ mong đợi `:dyn`. Hãy viết `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Một tên trait được viết ở chỗ cần kiểu. Hãy viết `:dyn Error` |
| ``if: (if cond then else)`` | `if` có dạng sai. `if` yêu cầu một nhánh else. Khi bạn không cần nó, hãy dùng `when` |

### 2.2 Tên

| Thông báo | Ý nghĩa và cách sửa |
|---|---|
| `no such function: bar` | Không có hàm hay phương thức nào mang tên đó. Kiểm tra chính tả |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Phương thức được chọn theo kiểu của đối số đầu tiên. Có một phương thức tên đó, nhưng không dành cho kiểu của đối số đầu tiên (ở đây là `int`). Cuối thông báo liệt kê các kiểu có phương thức đó |
| `unbound variable: y` | Không có biến nào mang tên đó. Kiểm tra chính tả và phạm vi của ràng buộc (có dùng ngoài `let` của nó không?) |
| ``use: unresolved `nosuch` `` | Không tìm thấy module được nêu trong `use`. Về cách tên tệp ánh xạ sang đường dẫn module, xem [Tham chiếu cú pháp 3.11](syntax.md#311-tệp-và-module-dự-án-nhiều-tệp) |
| `unresolved path: c::hidden` | Module tồn tại, nhưng tên thì không, hoặc nó không hiển thị vì thiếu `pub` |
| `circular module dependency: a -> b -> a` | Các module `use` lẫn nhau. Hãy chuyển phần dùng chung sang một module riêng |
| ``return-from: no enclosing block named `nope` `` | Không có `block` nào mang tên đưa cho `return-from` bao quanh nó. Block của một hàm chỉ dùng được bên trong hàm đó |

### 2.3 Lời gọi

| Thông báo | Ý nghĩa và cách sửa |
|---|---|
| `f: expected 1 argument(s), got 2` | Số đối số không khớp |
| `f: unknown keyword argument :b` | Đã truyền một đối số keyword mà hàm không có |
| `new: expected 1 field(s), got 2` | Số giá trị truyền cho hàm khởi tạo struct không khớp với số trường |
| ``setf: cannot assign to constant `k` `` | Đã gán cho một tên định nghĩa bằng `defconstant`. Nếu cần thay đổi, hãy dùng `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Một hàm được khai báo bằng `defsignature` chưa được định nghĩa |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Không kiểu đối số nào có phương thức được gọi bằng `~/name/` ([Các chỉ thị định dạng chương 5](functions/format.md#5-name)) |

## 3. Lỗi khi đọc

| Thông báo | Ý nghĩa và cách sửa |
|---|---|
| `unexpected end of input while reading a list` | Thiếu một dấu ngoặc đóng. Vị trí trỏ vào chỗ việc đọc kết thúc (như cuối tệp), nên hãy tìm dấu ngoặc mở |

## 4. Lỗi lúc chạy (panic)

| Thông báo | Ý nghĩa và cách sửa |
|---|---|
| `panic: divide by zero` | Chia cho không với số nguyên hoặc ratio. Phép chia dấu phẩy động cho không không panic; nó cho `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` được áp dụng cho `none`. Hãy xử lý trường hợp `none` bằng `match` hoặc `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Chỉ số ngoài phạm vi. Kiểm tra độ dài bằng `len`, hoặc dùng một hàm trả về `none` khi ngoài phạm vi (`nth`, `pop`, v.v.) |
| `panic: an integer argument does not fit a fixnum` | Một `int` không vừa trong 63 bit được truyền cho một đối số nhận chỉ số hoặc số đếm |
| `throw: no enclosing (catch 'oops) for this throw` | Một `throw` chạy mà không có `catch` bao quanh cùng tag |
| `panic: <message>` | Chương trình gọi `(panic "<message>")`. Một `assert` thất bại cho `assertion failed: ...` |

Một `panic` dừng toàn bộ tiến trình ngay cả khi nó xảy ra bên trong một task
([Tham chiếu cú pháp 12.4](syntax.md#124-tương-tác-với-các-tính-năng-khác)). Hãy biểu diễn các thất bại
bạn muốn khôi phục bằng `Result` ([Tham chiếu cú pháp chương 9](syntax.md#9-chính-sách-xử-lý-lỗi)).

## 5. Cảnh báo

| Thông báo | Ý nghĩa |
|---|---|
| ``warning: redefining function `f` `` | Một hàm cùng tên được định nghĩa lại. Định nghĩa sau có hiệu lực. Nó xuất hiện bình thường khi bạn sửa một định nghĩa trong REPL |
