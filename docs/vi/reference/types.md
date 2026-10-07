<!-- translated-from: docs/ja/reference/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Kiểu

Các kiểu mà typelisp có, và các trait chuẩn mà mỗi kiểu triển khai. Cách viết kiểu nằm ở
[Tham chiếu cú pháp chương 2](syntax.md#2-cách-viết-kiểu); các hàm và phương thức của từng kiểu nằm ở
[Hàm dựng sẵn](functions/README.md).

## 1. Kiểu nguyên thủy

| Kiểu | Nội dung | Chi tiết |
|---|---|---|
| `int` | Số nguyên độ chính xác tùy ý. Được giữ dưới dạng giá trị tức thời khi còn vừa trong 63 bit, và tự động trở thành bignum khi vượt quá. Kiểu mặc định của các literal số nguyên không chú thích | [Số chương 3](functions/numbers.md#3-số-nguyên-độ-chính-xác-tùy-ý-int) |
| `i8` `i16` `i32` | Số nguyên có dấu độ rộng cố định | [Số chương 1](functions/numbers.md#1-số-nguyên-độ-rộng-cố-định) |
| `u8` `u16` `u32` | Số nguyên không dấu độ rộng cố định | Như trên |
| `f32` `f64` | Số dấu phẩy động IEEE-754. Literal thập phân mặc định là `f64` | [Số chương 4](functions/numbers.md#4-số-dấu-phẩy-động-f64--f32) |
| `ratio` | Số hữu tỉ ở dạng tối giản | [Số chương 5](functions/numbers.md#5-số-hữu-tỉ-ratio) |
| `bool` | `true` / `false` | [Số chương 7](functions/numbers.md#7-boolean) |
| `char` | Một giá trị vô hướng Unicode | [Ký tự](functions/collections.md#2-ký-tự-char) |
| `string` | Một chuỗi bất biến | [Chuỗi](functions/collections.md#1-chuỗi-string) |
| `symbol` | Một symbol. Keyword (`:name`) cũng có kiểu này | [Symbol](functions/sequences.md#3-symbol) |
| `()` | Kiểu Unit. Giá trị của nó cũng là `()` | |
| `!` | Kiểu Never. Kiểu của các biểu thức không trả về, như `panic`. Có thể đặt ở chỗ mong đợi bất kỳ kiểu nào | |
| `ptr` `c-long` `c-ulong` | Các từ máy chỉ dùng để truyền giá trị đến và đi từ C. Chúng chỉ có thể là giá trị bên trong `unsafe`, và các chỗ chúng có thể xuất hiện bị giới hạn | [Số chương 2](functions/numbers.md#2-các-từ-máy-thô-ở-ranh-giới-c-ptr--c-long--c-ulong) |
| `random-state` | Trạng thái của bộ sinh số ngẫu nhiên | [Số chương 12](functions/numbers.md#12-số-ngẫu-nhiên) |

Không có kiểu số nguyên 64 bit. Với các số nguyên mà độ rộng không quan trọng, hãy dùng `int`.

## 2. Các kiểu generic dựng sẵn

| Kiểu | Nội dung | Chi tiết |
|---|---|---|
| `Option<T>` | Một giá trị có mặt hoặc không. `some` / `none` | [Option và Result](functions/option-result.md) |
| `Result<T,E>` | Thành công hoặc thất bại. `ok` / `err` | Như trên |
| `Vector<T>` | Một mảng có thể tăng trưởng | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Một bảng băm. Kiểu khóa phải triển khai `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Một handle tới một task | [Task](functions/concurrency.md#1-taskt--handle-tới-các-task) |
| `Thread<T>` | Một handle tới một task chạy trên thread HĐH riêng | [Thread](functions/concurrency.md#7-threadt--các-thread-hđh-riêng) |
| `Chan<T>` | Một kênh | [Kênh](functions/concurrency.md#2-chant--kênh) |

Kiểu hàm được viết `(fn (các-kiểu-đối-số...) kiểu-trả-về)`, và trait object là `:dyn Trait`
([Tham chiếu cú pháp chương 2](syntax.md#2-cách-viết-kiểu)).

## 3. Dữ liệu S-expression

| Kiểu | Nội dung | Chi tiết |
|---|---|---|
| `Sexpr` | Một S-expression không rỗng. 16 variant: `int`, `i8` đến `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [Dữ liệu S-expression](functions/sequences.md#2-dữ-liệu-s-expression-sexpr) |
| `Option<Sexpr>` | Dữ liệu S-expression nói chung. Danh sách rỗng `()` là `none` | Như trên |

## 4. Các kiểu trong thư viện chuẩn

Các kiểu mà thư viện chuẩn (prelude) định nghĩa bằng `defstruct` / `defenum`. Chúng được đối xử giống
như các kiểu bạn tự viết, và mọi thứ bạn làm được với một `defstruct` đều làm được với chúng.

| Kiểu | Nội dung | Chi tiết |
|---|---|---|
| `cons-cell<A,B>` | Một cặp. `cons`/`car`/`cdr` | [Cặp](functions/sequences.md#1-cặp-cons-cellab) |
| `complex` | Một số phức (các thành phần `f64`) | [Số chương 6](functions/numbers.md#6-số-phức-complex) |
| `Array<T>` | Một mảng nhiều chiều | [Array](functions/collections.md#5-arrayt-mảng-nhiều-chiều) |
| `BitVector` | Một dãy bit có độ dài cố định | [BitVector](functions/collections.md#6-bitvector-vector-bit) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Các iterator được `iter` của từng tập hợp trả về | [Iter](functions/traits.md#1-trait-iter-và-việc-lặp) |
| `WaitGroup` | Chờ N việc kết thúc | [WaitGroup](functions/concurrency.md#4-waitgroup--chờ-n-lần-hoàn-tất) |
| `Mutex<T>` | Loại trừ tương hỗ cho dữ liệu dùng chung | [Mutex](functions/concurrency.md#6-mutext--loại-trừ-tương-hỗ-cho-dữ-liệu-dùng-chung) |
| `pathname` | Một tên tệp được tách thành các phần | [Pathname](functions/streams-files.md#9-pathname-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Các stream | [Stream](functions/streams-files.md#3-các-kiểu-stream-cụ-thể) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Các stream tổ hợp | [Stream tổ hợp](functions/streams-files.md#4-các-stream-tổ-hợp) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Mạng | [Mạng](functions/network.md#1-kiểu) |
| `ReadOutcome` | Kết quả của `read-sexpr`. `datum` / `eof` | [Stream](functions/streams-files.md#6-hàm-generic-và-thao-tác-tệp) |
| `universal-time` `internal-time` `decoded-time` | Thời gian | [Thời gian](functions/system.md#1-thời-gian) |
| `heap-info` | Trạng thái hiện tại của heap | [Công cụ triển khai](functions/system.md#51-các-trường-của-heap-info) |

## 5. Các kiểu lỗi

`Error` không phải là một kiểu mà là một trait, và các kiểu sau triển khai nó. Để xử lý lỗi thuộc loại
bất kỳ, hãy viết `:dyn Error`.

| Kiểu | Được tạo bởi |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Các thao tác tệp và stream |
| `NetError` | Các thao tác mạng |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Chi tiết nằm ở [Các kiểu lỗi và trait Error](functions/option-result.md#3-các-kiểu-lỗi-và-trait-error).

## 6. Các triển khai của trait chuẩn

Kiểu nào triển khai trait nào. Các phương thức của từng trait nằm ở [Các trait chuẩn](functions/traits.md)
và ở các chương được liệt kê trong cột ngoài cùng bên phải.

### 6.1 So sánh, băm và in

| Trait | Các kiểu triển khai | Chi tiết |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-so-sánh) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Như trên |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` và mọi kiểu lỗi dựng sẵn | [print-object](functions/printing.md#5-print-object-biểu-diễn-in-theo-từng-kiểu) |

`Eq`/`Ord` của `cons-cell<A,B>` dùng được khi các kiểu phần tử triển khai `Eq`/`Ord`.

### 6.2 Số học

| Trait | Các kiểu triển khai |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Chi tiết nằm ở [Các trait số học](functions/traits.md#3-các-trait-số-học-add--sub--mul--div--rem--bits--number).

### 6.3 Lặp

| Trait | Các kiểu triển khai |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Stream

| Kiểu | Các trait được triển khai |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Mọi stream đều triển khai `Stream`; các stream đầu vào cũng triển khai `InputStream`, và các stream đầu
ra triển khai `OutputStream`. `socket-listener` và `udp-socket` chỉ triển khai `Stream` (`close` /
`open-stream-p`). Chi tiết nằm ở [Stream](functions/streams-files.md#1-hệ-phân-cấp-trait).

### 6.5 Các trait khác

| Trait | Các kiểu triển khai | Chi tiết |
|---|---|---|
| `Error` | Mọi kiểu lỗi ở chương 5 | [Các kiểu lỗi](functions/option-result.md#3-các-kiểu-lỗi-và-trait-error) |
| `Pathish` | `string` `pathname` | [Pathname](functions/streams-files.md#91-trait-pathname-designator-pathish) |
