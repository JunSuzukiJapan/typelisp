<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Số

Các phép toán trên số nguyên, số dấu phẩy động, số hữu tỉ, số phức và boolean, cùng các hàm khác liên quan
đến số. Về cách đọc các dạng gọi, xem [Hàm dựng sẵn](README.md).

## 1. Số nguyên độ rộng cố định

Có bảy kiểu số nguyên: **`int`** (`integer` của CL: độ chính xác tùy ý, và là kiểu mặc định của các literal
số nguyên không chú thích; chương 3), và các kiểu độ rộng cố định `i8` `i16` `i32` `u8` `u16` `u32`. Một phép
toán được phân giải cho kiểu nào được quyết định bởi kiểu của đối số đầu tiên (chúng độc lập với nhau,
không có chuyển đổi ngầm). **Không có kiểu số nguyên 64 bit.** Một giá trị lúc chạy là một từ mà các bit thấp
là một tag, nên chỉ còn 63 bit cho một số nguyên tức thời, và một kiểu tuyên bố 64 bit sẽ phải bỏ bit cao
nhất ở đâu đó. `int` trở thành bignum khi vượt qua 63 bit đó, nên nếu độ rộng không quan trọng, hãy dùng
`int`. Bảng dưới đây dành cho sáu kiểu độ rộng cố định (bảng cho `int` nằm ở chương 3).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Bốn phép toán số học. `/` cắt về phía không và panic khi chia cho không |
| `mod` | `(mod a b)` | `(T,T)→T` | Phần dư (`mod` của CL, **chia làm tròn xuống**: dấu theo số chia. `(mod -7 3)`→`2`). Panic khi chia cho không |
| `rem` | `(rem a b)` | `(T,T)→T` | Phần dư (`rem` của CL, **chia cắt cụt**: dấu theo số bị chia. `(rem -7 3)`→`-1`). Panic khi chia cho không |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Tương ứng với `floor`/`ceiling`/`round`/`truncate` hai đối số của CL (`(floor 7 2)`→thương 3, dư 1). Thay vì nhiều giá trị, chúng trả về thương và số dư trong một `cons-cell` (`car`=thương, `cdr`=số dư). `round-div` làm tròn các điểm giữa về số chẵn, như CL |
| `abs` | `(abs x)` | `T→T` | Giá trị tuyệt đối |
| `signum` | `(signum x)` | `T→T` | Dấu (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Ước chung lớn nhất |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Bội chung nhỏ nhất (0 nếu một trong hai là 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Lớn hơn / nhỏ hơn (ba đối số trở lên được khai triển bằng đường cú pháp nhiều đối số ở chương 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | So sánh |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Tất cả giống `=` (không có khác biệt với các số cùng kiểu) |
| `int->float` | `(int->float x)` | `T→f64` | Chuyển đổi mở rộng sang `f64` |
| `int->int` | `(int->int x)` | `T→int` | Chuyển đổi mở rộng sang `int` (luôn chính xác). Việc `(as int x)` làm |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Chuyển đổi mở rộng sang `ratio` (luôn chính xác) |
| `int->char` | `(int->char x)` | `T→char` | Diễn giải giá trị như một giá trị vô hướng Unicode. Panic với giá trị không hợp lệ |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Một phiên bản của `int->char` trả về `None` khi thất bại |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Chuyển đổi độ rộng. Các giá trị không vừa bị cắt cụt (như `as` của Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Cùng phép chuyển đổi dưới dạng một câu hỏi. `None` nếu giá trị không vừa độ rộng đó |

Các phép chuyển đổi này cũng là những gì các dạng đặc biệt `(as Type x)`/`(try-as Type x)`
([Tham chiếu cú pháp](../syntax.md#7-các-dạng-đặc-biệt-khác)) làm. Các phép toán bit (`logand`/`ash`/`ldb`, v.v.)
và các vị từ (`zerop`/`evenp`, v.v.) có cùng dạng trên mọi kiểu, nên chúng được gom ở các chương 11 và 9.

`i8` `i16` `u8` `u16` `u32` có đúng bảng của chương này, và `f32` có đúng bảng `f64` của chương 4.

**Tên kiểu chỉ có nghĩa là độ rộng và tính có dấu, không hơn.** `i32` nghĩa là "coi 32 bit là có dấu" và
`u32` nghĩa là "coi 32 bit là không dấu". `(+ (the u8 200) (the u8 100))` là `44`, `(+ 2147483647 1)` (kiểu
`i32`) là `-2147483648`, và `(lognot (the u32 0))` là `4294967295`. `f32` cũng vậy: một binary32 thật sự.
`(/ (the f32 1.0) (the f32 3.0))` được in là `0.33333334`, một giá trị khác với kết quả `f64`
`0.3333333333333333`.

Danh mục dẫn xuất của CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` và các vị từ của chương 9) tồn tại cho
`int`/`i32`/`f64`/`ratio`. Nếu bạn cần nó cho độ rộng khác, hãy chuyển sang bằng `(as int x)` /
`(as i32 x)` (chuyển đổi độ rộng tồn tại cho mọi cặp).

## 2. Các từ máy thô ở ranh giới C (`ptr` / `c-long` / `c-ulong`)

Ba kiểu chỉ dùng để truyền giá trị đến và đi từ các hàm C được khai báo bằng
[`defffi`](../syntax.md#33-defffi--khai-báo-hàm-c-ffi). `ptr` là một con trỏ mờ, còn
`c-long` / `c-ulong` là `long` / `unsigned long` của C. Để biến một thứ thành giá trị cần phải ở bên trong
`(unsafe ...)`.

**Không có số học.** Không có gì trong bảng của chương 1 áp dụng: cả `(+ p 1)` lẫn `(< n m)` đều không thể
viết. Đây là các từ để đưa cho C, không phải các kiểu để tính toán, nên để tính toán, hãy chuyển sang một
kiểu có độ rộng. `c-long` / `c-ulong` chỉ có các phép chuyển đổi:

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Cùng các phép chuyển đổi độ rộng như chương 1. Các giá trị không vừa bị cắt cụt |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Cùng phép chuyển đổi dưới dạng một câu hỏi |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Đường vào, từ từ máy thô kia và từ các kiểu số nguyên của chương 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Như trên |
| `int->int` | `(int->int x)` | `T→int` | **Luôn chính xác**. Cách trung thực để đọc một `size_t` không vừa trong `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` là những gì chúng làm, và các phép
chuyển đổi tồn tại cho mọi cặp với các kiểu số nguyên của chương 1. `ptr` thậm chí không có bảng này:
không có cách nào để đọc một con trỏ như một số. Nó là một giá trị chỉ được truyền, nhận và chuyển tiếp
cho một hàm C khác.

**Chúng cũng không thể được in.** `(println "~a" x)` không chấp nhận một từ máy thô (nó không có biểu diễn
`Sexpr`), nên hãy chuyển nó sang một kiểu có độ rộng trước, như `(println "~a" (as int n))`.

"Không có kiểu số nguyên 64 bit" ở đầu chương 1 đúng với cả ba kiểu này. Nó đúng **vì chúng không thể được
lưu**: chúng không thể là trường `defstruct`, một `defvar`, bên trong một đối số kiểu hay bên trong một
`Sexpr`, nên chúng là các từ chỉ đi qua một hàm dưới dạng đối số, giá trị trả về và biến cục bộ. Chi tiết
nằm ở [Tham chiếu cú pháp](../syntax.md#ptr--c-long--c-ulong--các-từ-máy-thô).

## 3. Số nguyên độ chính xác tùy ý `int`

`integer` của CL, và **số nguyên** của ngôn ngữ này: các literal số nguyên không chú thích có kiểu này, và
các hàm dựng sẵn trả về một số, như `length` và `char->int`, trả về kiểu này. Một giá trị được giữ dưới
dạng giá trị tức thời 63 bit (fixnum) khi còn vừa, được thăng cấp tự động thành bignum khi kết quả của một
phép toán không còn vừa, và quay lại giá trị tức thời khi lại vừa. `eq` luôn là đồng nhất giá trị trong dải
fixnum, và `eql`/`=` là đồng nhất số trên toàn dải. Nó là một kiểu khác với các kiểu số nguyên độ rộng cố
định (chương 1), không có chuyển đổi ngầm: `(as int x)` là phép mở rộng chính xác từ một độ rộng cố định, và
`(as i32 n)` / `(try-as i32 n)` là phép cắt cụt / kiểm tra từ `int` (cùng ý nghĩa với `int->W` /
`try-int->W` ở chương 1).

Variant số nguyên của `Sexpr` cũng chỉ là `int` (`(int n)` chấp nhận cả fixnum và bignum).

Các hàm dựng sẵn nhận một chỉ số hoặc một số đếm (`substring`, `get` của `Vector`, khoảng dịch của `ash`,
v.v.) chấp nhận `int`, nhưng truyền một giá trị không vừa fixnum là lỗi lúc chạy ("an integer argument does
not fit a fixnum").

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Không bao giờ tràn (chúng thăng cấp) |
| `/` | `(/ a b)` | `(int,int)→int` | Cắt về phía không. Panic khi chia cho không |
| `mod` | `(mod a b)` | `(int,int)→int` | Phần dư của phép chia làm tròn xuống (dấu theo số chia) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Tất cả là `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Giống chương 11 (bù hai với vô hạn bit) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Giống chương 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Cắt cụt / kiểm tra. `W` là một trong sáu độ rộng hoặc `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Đồng nhất (ở phía độ rộng cố định và từ C, `int->int` mở rộng; chương 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Cùng dạng với chương 1. `expt` chỉ chấp nhận số mũ không âm |

## 4. Số dấu phẩy động (`f64` / `f32`)

`f32` có cùng bảng.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Chia cho không không panic; nó cho `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Phần dư của phép chia làm tròn xuống (như trong CL; dấu theo số chia. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Phần dư của phép chia cắt cụt (như trong CL; dấu theo số bị chia. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | So sánh |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Tất cả giống `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Lũy thừa |
| `abs` | `(abs x)` | `f64→f64` | Giá trị tuyệt đối |
| `signum` | `(signum x)` | `f64→f64` | Dấu (`1.0`/`-1.0`; `±0.0`/`NaN` được trả về nguyên trạng. Như trong CL, khác `signum` của Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Lớn hơn / nhỏ hơn (ba đối số trở lên được khai triển bằng đường cú pháp nhiều đối số ở chương 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Các phép toán một ngôi |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Các hàm siêu việt. `log` là logarit tự nhiên |
| `log` (hai đối số) | `(log x base)` | `(f64,f64)→f64` | Logarit theo một cơ số cho trước. Được khai triển thành `(/ (log x) (log base))` (chương 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Tương ứng với các phiên bản hai đối số của CL (`(floor 7.0 2.0)`→thương 3, dư 1). Cùng thiết kế như các hàm cùng tên ở chương 1 (`car`=thương, `cdr`=số dư) |
| `float->int` | `(float->int x)` | `f64→int` | Chuyển thành `int` bằng cách cắt về phía không (`truncate` của CL; chính xác với các giá trị hữu hạn ở mọi cỡ). Panic với vô cực và NaN. Với độ rộng cố định, hãy dùng `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Chuyển thành một `ratio` dưới dạng số hữu tỉ nhị phân chính xác (`rational` của CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Chuyển đổi giữa các độ rộng dấu phẩy động. `float->f32` làm tròn đến gần nhất, `float->f64` luôn chính xác. Việc `(as f32 x)` làm |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Cùng phép chuyển đổi dưới dạng một câu hỏi. `none` nếu việc làm tròn thay đổi giá trị (mở rộng sang `f64` luôn là `some`). Việc `(try-as f32 x)` làm |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Các hàm cùng tên của CL. Là bí danh của `floor`/`ceiling`/`round`/`truncate` ở trên: trong CL các hàm không có tiền tố trả về số nguyên, nên các hàm có tiền tố `f` khớp với hành vi của ngôn ngữ này |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Lần lượt là 2 / 53 / 53 (chỉ độ chính xác của `0.0` là 0). `f64` luôn là IEEE-754 binary64, nên đây là các hằng số |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` hoặc `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Phần định trị (trong `[1/2,1)`, không có dấu) và số mũ. CL trả về ba giá trị, nhưng không có đa giá trị, nên dấu được giao cho `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Cùng phép phân rã với phần định trị là số nguyên 53 bit chính xác. `mantissa * 2^exponent` chính xác bằng giá trị gốc |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Số hữu tỉ đơn giản nhất mà đọc ngược lại ra số thực đó** (`(rationalize 0.1)` là `1/10`). Với giá trị nhị phân chính xác, hãy dùng `float->ratio` |

**Khác biệt so với CL: cách `round` làm tròn.** `round` (và do đó `fround`/`round-div`) làm tròn **ra xa không**
(`(round 2.5)` = `3.0`). CL làm tròn **về số chẵn**, cho `2`.

## 5. Số hữu tỉ `ratio`

Số hữu tỉ độ chính xác tùy ý tương thích CL. Chúng luôn được giữ ở dạng tối giản với mẫu số dương, và được
cấp phát trên heap. Không có chuyển đổi ngầm với các kiểu số nguyên hay `f64` (hãy dùng một phương thức
chuyển đổi tường minh hoặc `as`/`try-as`). Về cú pháp literal ratio, xem
[Tham chiếu cú pháp](../syntax.md#1-cú-pháp-từ-vựng).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Bốn phép toán (kết quả luôn tối giản). `/` panic khi chia cho không |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Phần dư của phép chia làm tròn xuống (như trong CL; dấu theo số chia) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Phần dư của phép chia cắt cụt (như trong CL; dấu theo số bị chia) |
| `abs` | `(abs x)` | `ratio→ratio` | Giá trị tuyệt đối |
| `signum` | `(signum x)` | `ratio→ratio` | Dấu (trả về `1`/`-1`/`0` dưới dạng `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Lũy thừa. Số mũ phải là một `ratio` có giá trị nguyên (nếu không thì panic). Số mũ âm cho nghịch đảo |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Lớn hơn / nhỏ hơn |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` không có phép toán bit (trong CL chúng chỉ dành cho số nguyên) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | So sánh |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Tất cả giống `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Tử số ở dạng tối giản (cùng tên như trong CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Mẫu số ở dạng tối giản (luôn dương) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Phần nguyên (cắt về phía không) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Chuyển thành `f64` |

Các đường vào từ số nguyên độ rộng cố định và `f64` là `int->int`/`int->ratio` (chương 1) và
`float->int`/`float->ratio` (chương 4). `int`/`ratio` là các kiểu riêng, độc lập với `i32` và các kiểu
khác, và số học hỗn hợp cần các chuyển đổi tường minh.

## 6. Số phức `complex`

Một struct (`defstruct`) trong thư viện chuẩn.

**Hai khác biệt so với CL** (cả hai đều xuất phát từ kiểu tĩnh):

1. **Các thành phần luôn là `f64`.** Một số phức CL cũng có thể giữ số hữu tỉ, và `(complex 1 2)` và
   `(complex 1.0 2.0)` là các kiểu khác nhau. Một kiểu tĩnh phải chọn một, và các hàm siêu việt trả về loại
   dấu phẩy động.
2. **`(sqrt -1.0)` là `sqrt` thực (NaN).** Trong CL, `sqrt` có thể trả về một số phức từ một số thực, nhưng
   `sqrt` của `f64` phải trả về một `f64`. Một kết quả phức đến từ một đối số phức:
   `(sqrt (complex -1.0 0.0))` là `i`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Khởi tạo. Các thành phần có thể đọc trực tiếp là `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Phần thực và phần ảo. **Chúng cũng hoạt động trên số thực** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), như trong CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Số phức liên hợp (cũng hoạt động trên số thực) |
| `phase` | `(phase z)` | `complex→f64` | Argument trong (-pi,pi] (cũng hoạt động trên số thực) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Giá trị tuyệt đối. **`abs` duy nhất không trả về kiểu của bên nhận** (như trong CL, giá trị tuyệt đối của số phức là thực) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Số học phức |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Bằng nhau theo từng thành phần. `Eq` cũng được triển khai (không có `Ord`: số phức không có thứ tự, và `<` của CL cũng từ chối chúng) |
| `zerop` | `(zerop z)` | `complex→bool` | Cả hai thành phần có bằng 0 không |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` cho giá trị chính |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | Góc của vector `(x,y)`. **`(atan y x)` hai đối số của CL là đường cú pháp cho hàm này** (nó rẽ nhánh theo số đối số, như `log` hai đối số) |

Nó triển khai `print-object`, nên `~a`/`~s` in nó là `#C(re im)`, như CL (reader của ngôn ngữ này không có
cú pháp `#C` để đọc ngược lại).

## 7. Boolean

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Phủ định |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Tất cả so sánh giá trị về sự bằng nhau |

`and`/`or` cần đánh giá đoản mạch, nên chúng là các dạng đặc biệt
([Tham chiếu cú pháp](../syntax.md#4-ràng-buộc-và-rẽ-nhánh)).

## 8. Hàm số hỗ trợ và đường cú pháp gọi hàm

`abs`/`signum` (mọi kiểu số), `gcd`/`lcm` (chỉ kiểu số nguyên), `rem` (mọi kiểu thực gồm `f64`) và `expt`
(`int`/`f64`/`ratio`) được định nghĩa như các phương thức của từng kiểu số (được phân giải theo kiểu của
bên nhận: `(abs x)` là phương thức của kiểu của `x`). Chi tiết cho từng kiểu nằm ở các chương 1, 3, 4 và 5.
Số nguyên độ rộng cố định không có `expt` (chúng không có thăng cấp và sẽ tràn; hãy chuyển sang `int` bằng
`(as int x)` và dùng `expt` của nó).

### 8.1 Các dạng nhiều đối số và 0/1 đối số

Phép toán số học và so sánh của CL nhận số đối số biến đổi, nhưng phương thức chỉ được phân giải theo kiểu của
bên nhận, không theo số đối số. Vì vậy **bộ kiểm tra khai triển các dạng sau thành các lời gọi hai đối số**.

| Dạng bạn có thể viết | Khai triển | Áp dụng cho |
|---|---|---|
| `(op a b c ...)` | Phép gấp trái `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` với mỗi số hạng được gán vào một biến tạm | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Những hàm ở trên có phần tử đơn vị |
| `(op x)` | Với `+ * max min logand logior logxor`, chính `x`. `(- x)` đổi dấu, `(/ x)` cho nghịch đảo, `(gcd x)`/`(lcm x)` cho `(abs x)` (như trong CL) | Như trên |
| `(cmp x)` | Đánh giá `x` và cho `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Mỗi số hạng được đánh giá đúng một lần, từ trái sang phải (đó là lý do các phép so sánh nhiều đối số đi qua
biến tạm). Dạng nhiều đối số của `/=` so sánh **các cặp liền kề**, khác CL, vốn hỏi liệu mọi cặp có khác nhau
không.

### 8.2 `isqrt` và `expt` số nguyên

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Số nguyên lớn nhất không vượt quá căn bậc hai. Panic với giá trị âm |
| `expt` | `(expt n e)` | `(T,T)→T` | Lũy thừa (bằng bình phương liên tiếp). CL trả về một số hữu tỉ cho số mũ âm, nhưng một kiểu số nguyên không thể biểu diễn nó, nên nó panic; hãy chuyển sang `ratio` trước |

## 9. Vị từ

| Tên | Dạng | Kiểu | Các kiểu |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (chỉ kiểu số nguyên, như trong CL) |

**Không có vị từ kiểu** như `numberp`/`integerp`/`floatp` của CL. Với kiểu tĩnh, kiểu của một giá trị đã được
xác định mà không cần hỏi lúc chạy.

## 10. Hằng số

| Tên | Kiểu | Giá trị |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Các mã phép toán truyền cho `boole` (thay cho các keyword của CL) |

Các hằng số giới hạn số (CLHS 12.1.4.2 / 12.1.3):

| Tên | Kiểu | Mô tả |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Giới hạn trên / dưới của một giá trị tức thời 63 bit (2^62-1 / -2^62). Một `int` vượt quá chúng trở thành bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Các giá trị hữu hạn lớn nhất / nhỏ nhất |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Độ lớn khác không nhỏ nhất, gồm cả các số dưới chuẩn |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Như trên, giới hạn ở các số chuẩn hóa |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Chúng theo định nghĩa của CL (`e` dương nhỏ nhất với `(/= (+ 1 e) 1)`), nên chúng **lớn hơn 1 ULP so với** 2^-53: bản thân 2^-53 làm tròn ngược về `1.0` dưới phép làm tròn đến gần nhất về chẵn |

## 11. Phép toán bit

Được định nghĩa trên bù hai với vô hạn bit (CL 12.10). Chúng được triển khai cho các kiểu số nguyên độ rộng
cố định và `int`, không cho `ratio` (CL cũng chỉ có phép toán bit cho số nguyên).

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Và, hoặc, hoặc loại trừ theo bit (các phiên bản nhiều đối số và không đối số ở 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Phần bù theo bit |
| `ash` | `(ash x count)` | `(T,int)→T` | Dịch số học. Sang trái nếu `count` dương, sang phải nếu âm |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Bit `index` có được đặt không (**thứ tự đối số ngược với CL**; xem bên dưới) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Số bit được đặt (với số âm, là số bit 0) |
| `integer-length` | `(integer-length x)` | `T→T` | Số bit cần để biểu diễn, không tính dấu |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Bảy phép còn lại, được ghép từ các phép trên |

**Chỉ đối số thứ hai của `ash` là `int` thay vì `T`.** Nó là một **khoảng cách** tính bằng bit, không phải một
giá trị của kiểu của bên nhận, nên độ rộng và tính có dấu của bên nhận không nói gì về khoảng cách (vì cùng
lý do `count` trong `(ash integer count)` của CL là một số nguyên bất kỳ). Dịch phải một giá trị không dấu là
dịch logic (`(ash (the u8 200) -3)` = `25`), và một giá trị có dấu là dịch số học làm tròn về phía âm vô cực
(`(ash (the i32 -100) -4)` = `-7`). `index` của `logbitp` là `int` vì cùng lý do.

**Byte specifier.** Thay cho đối tượng mờ mà `byte` của CL trả về, một `cons-cell<int,int>` (`car`=kích
thước, `cdr`=vị trí) được dùng. Cả kích thước lẫn vị trí đều là số bit, nên chúng là `int` bất kể độ rộng
của số nguyên đang được tách.

**Số nguyên là đối số đầu tiên, khác thứ tự của CL.** CL viết `(ldb bytespec integer)`, nhưng ngôn ngữ này
chọn một phương thức theo kiểu của bên nhận (đối số đầu tiên), và với specifier đứng đầu nó không thể chọn
theo kiểu của số nguyên. Mọi phép toán bit khác có dạng `(op integer ...)` (`(logand a b)`,
`(ash x count)`, `(lognot x)`), và chỉ họ `ldb` và `logbitp` là ngược lại, nên chúng được đưa cho đồng bộ.
Các đối số còn lại giữ thứ tự tương đối của CL, nên `(dpb newbyte spec n)` trở thành
`(dpb n newbyte spec)`.

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Tạo một byte specifier |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Lấy một thành phần ra |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Trích byte được chỉ định từ `x`, căn phải |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Có bit nào trong byte được chỉ định được đặt không |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Xóa mọi thứ ngoài byte được chỉ định (giữ nguyên vị trí) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Đặt `newbyte` căn phải vào byte được chỉ định của `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Phiên bản giữ vị trí của `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Một trong 16 phép toán logic hai toán hạng, được chọn bởi `op` (một hằng số `boole-*` từ chương 10) |

`T` là một kiểu triển khai trait `Bits`, cụ thể là `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Chỉ `boole` giữ
`op` ở đầu, vì không có lý do gì để đổi thứ tự của CL ở đó.

## 12. Số ngẫu nhiên

| Tên | Dạng | Kiểu | Mô tả |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Một số ngẫu nhiên từ `0` đến nhưng không gồm `n`. Nếu bỏ qua state, lấy từ `*random-state*` và tiến nó lên |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Không có đối số, một state mới; có một đối số, một bản sao của nó (bản sao phát lại cùng dãy) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Luôn là `true` (kiểu tĩnh đã loại trừ các kiểu khác; nó chỉ tồn tại để tương ứng với CL) |
| `*random-state*` | — | `random-state` | State mặc định của `random`. Một biến toàn cục có thể gán (thay nó bằng `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | State mà số nguyên đặt tên. Cùng seed luôn phát lại cùng dãy |

Bộ sinh là xorshift64 và trả về cùng một dãy dù thông dịch hay biên dịch.

Một state mới từ `make-random-state` được gieo từ đồng hồ treo tường, nên không thể tái tạo giữa các lần
chạy. Để tái tạo, hãy dùng `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; in ra cùng ba số ở mỗi lần chạy
```

**CL không có cách di động để đưa một seed** (`make-random-state` chỉ nhận `nil`/`t`/một state), nên tên này
theo `sb-ext:seed-random-state` của SBCL chứ không phải CL.

Các seed khác nhau cho các dãy khác nhau. `(seed-random-state 0)` và `(seed-random-state 1)` cho các dãy khác
nhau, và `-7` và `7` cũng vậy.
