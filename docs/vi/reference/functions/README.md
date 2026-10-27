<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Hàm dựng sẵn

Danh sách các hàm dựng sẵn, phương thức và thư viện chuẩn. Về cú pháp (các dạng đặc biệt và cách định
nghĩa), xem [Tham chiếu cú pháp](../syntax.md); về danh sách các kiểu, xem [Kiểu](../types.md).

## Các dạng gọi

Có ba dạng gọi.

- Hàm tự do: `(name args...)`
- Phương thức thể hiện: `(name receiver args...)` (được phân giải từ kiểu tĩnh của đối số đầu tiên)
- Phương thức tĩnh (hàm liên kết): `(Type::name args...)`

Mỗi kiểu có thể có phương thức cùng tên của riêng nó. `(+ a b)` gọi `+` của kiểu của `a`.

## Các tên cần `use`

Trong các tên của thư viện chuẩn, những tên sau dùng được trong mọi module mà không phải viết gì
([tham chiếu cú pháp 3.10](../syntax.md#310-module--use--không-gian-tên)):

- các kiểu nguyên thủy, `Option`/`Result`/`Vector`, phép lặp, biểu thức S và danh sách, các hàm dãy,
  các macro điều khiển, phép so sánh và các trait số học
- mọi tên cũng là tên trong gói `COMMON-LISP` của Common Lisp (`length`, `format`, `open`, v.v.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, các trait luồng `Stream`,
  `InputStream` và `OutputStream`, cùng `Hash`, mà khóa của một `HashTable` cài đặt
- các kiểu mà hàm và biến của chúng nhắc tới

Mọi tên khác được đưa vào bằng `use`, như `(use std::net::tcp-connect)`, hoặc viết theo đường dẫn,
`std::net::tcp-connect`. Bảng liệt kê các tên đó và module của từng tên.

| Module | Tên |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Cách đọc các bảng

Các bảng trong mỗi chương có các cột "tên, dạng, kiểu, mô tả". Cột kiểu được viết là
`(kiểu-đối-số,...)→kiểu-trả-về`.

- Một chữ cái viết hoa như `T`, `A` hay `B` là một biến kiểu.
- Một ghi chú như `where Eq A` là một ràng buộc trait mà biến kiểu phải thỏa mãn.
- `Iter<A>` nghĩa là "mọi triển khai của `Iter` có `Item` là `A`".
- Các đối số được đánh dấu `&optional` / `&key` có thể bỏ qua.

## Các chương

| Tệp | Nội dung |
|---|---|
| [numbers.md](numbers.md) | Số nguyên, số dấu phẩy động, số hữu tỉ, số phức, boolean, phép toán bit, số ngẫu nhiên |
| [sequences.md](sequences.md) | Cặp `cons-cell`, dữ liệu S-expression `Sexpr`, symbol, hàm trên dãy, iterator lười `lazy`, hàm bậc cao |
| [collections.md](collections.md) | Chuỗi, ký tự, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, các kiểu lỗi và trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, các trait số học |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, các biến điều khiển bộ in |
| [format.md](format.md) | Các chỉ thị định dạng |
| [streams-files.md](streams-files.md) | Stream, thao tác tệp, pathname, readtable |
| [concurrency.md](concurrency.md) | Task, kênh, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix domain socket, UDP |
| [system.md](system.md) | Thời gian, môi trường chạy, công cụ triển khai, `read`/`eval`, docstring, các hàm liên quan đến macro |
