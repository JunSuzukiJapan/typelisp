<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Hàm dựng sẵn

Danh sách các hàm dựng sẵn, phương thức và thư viện chuẩn. Về cú pháp (các dạng đặc biệt và cách định
nghĩa), xem [Tham chiếu cú pháp](../syntax.md); về danh sách các kiểu, xem [Kiểu](../types.md).

## Các dạng gọi

Có ba dạng gọi.

- Hàm tự do: `(name args...)`
- Phương thức thể hiện: `(name receiver args...)` (được phân giải từ kiểu tĩnh của đối số đầu tiên)
- Phương thức tĩnh (hàm liên kết): `(Type::name args...)`

Mỗi kiểu có thể có phương thức cùng tên của riêng nó. `(+ a b)` gọi `+` của kiểu của `a`.

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
