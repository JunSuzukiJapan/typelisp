<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

Chạy `typl` không có đối số sẽ mở REPL (chế độ tương tác). Tài liệu này giải thích những gì REPL
làm được ngoài việc đánh giá biểu thức: các lệnh của nó, hỗ trợ khi gõ, tham chiếu kết quả trước
đó, dừng một phép tính và điều tra sau khi có lỗi.

## Khởi động và thoát

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Thoát bằng `:quit` (viết tắt `:q`) hoặc `Ctrl+D`. Lịch sử nhập được lưu trong `~/.typl_history`,
nên lần khởi động sau có thể gọi lại bằng `Ctrl+P` / `Ctrl+N` hoặc `Ctrl+R` (tìm trong lịch sử).

Khi khởi động, trước dấu nhắc đầu tiên, các mục sau được chạy theo thứ tự. Không mục nào in ra giá
trị mà nó tạo ra.

1. Nội dung của `~/.typlrc`, nếu tệp này tồn tại. Với `--no-init` thì tệp không được đọc.
2. `--eval FORM` và `--load FILE` trên dòng lệnh, theo thứ tự đã viết. Cả hai đều có thể đưa bao
   nhiêu lần cũng được.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` và `--no-init` chỉ dùng khi khởi động REPL. Đưa chúng vào khi chạy một tệp là
lỗi.

REPL bắt đầu ở module `user`. Khi chuyển sang module khác bằng `(in-module foo)`, dấu nhắc hiển thị
module đó.

```
typl> (in-module foo)
typl user::foo>
```

## Lệnh REPL

Một `:tên` viết ở đầu dòng là lệnh REPL. `:help` liệt kê các lệnh. Một `:tên` không có trong danh
sách không phải là lệnh; nó được đánh giá như một từ khóa thông thường.

| Lệnh | Ý nghĩa |
|---|---|
| `:help` (`:h`, `:?`) | liệt kê các lệnh |
| `:type FORM` (`:t`) | in kiểu của FORM mà không đánh giá nó |
| `:describe NAME` (`:d`) | in loại định nghĩa, kiểu, các trường hoặc biến thể, các phương thức, các trait mà nó cài đặt, nơi định nghĩa và chuỗi tài liệu |
| `:apropos TEXT` | tìm trong mọi module các định nghĩa có tên chứa TEXT (không phân biệt chữ hoa chữ thường) |
| `:src NAME` | in mã nguồn của một định nghĩa |
| `:who-calls NAME` | in các hàm và phương thức gọi hàm hoặc phương thức NAME |
| `:inspect FORM` (`:i`) | đánh giá FORM và mở giá trị của nó theo kiểu |
| `:backtrace` (`:bt`) | in các lời gọi đang dở khi dòng trước thất bại, từ trong cùng ra ngoài |
| `:locals [N]` | in các đối số của lời gọi thứ N trong `:backtrace` (lời gọi 0 nếu bỏ N) |
| `:cd [DIR]` | chuyển thư mục hiện tại sang DIR (sang thư mục nhà nếu bỏ DIR) |
| `:pwd` | in thư mục hiện tại |
| `:quit` (`:exit`, `:q`) | thoát REPL |

NAME có thể là một tên như `inner`, hoặc một đường dẫn như `std::seq::reverse` hay
`Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Cộng 1 vào x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Cộng 1 vào x.
typl> :src inner
(defun inner ((x int) (s string)) int "Cộng 1 vào x." (+ x 1))
```

`:src` hiển thị định nghĩa được nạp từ tệp dựa trên tệp đó, và định nghĩa gõ trong REPL dựa trên
văn bản đã gõ. Các định nghĩa trong thư viện chuẩn không có vị trí mã nguồn được ghi lại, nên không
thể hiển thị mã nguồn của chúng.

`:who-calls` tìm trong thân các hàm của phiên hiện tại. Với hàm generic, chỉ những chỗ dùng đã được
khởi tạo với kiểu cụ thể mới được tìm thấy.

`:cd` thay đổi thư mục mà `(load ...)` tìm, nhưng `use` vẫn tìm tệp trong thư mục nơi REPL được
khởi động.

## Mở một giá trị (`:inspect`)

`:inspect` mở một giá trị theo kiểu của nó. Struct mở thành các trường, enum (kể cả `Option`) mở
thành các trường của biến thể, `Vector` mở thành các phần tử, và `HashTable` mở thành các khóa và
giá trị. Gõ một số để vào phần đó, `u` để lùi lên một cấp và `q` để kết thúc.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Danh sách và `Sexpr` không được mở; chúng được in nguyên dạng.

## Hỗ trợ khi gõ

Khi gõ trong terminal, có những thứ sau.

- `Tab` tự hoàn thành tên (các định nghĩa và cú pháp như `if`, `defun`), đường dẫn chứa `::`, lệnh
  REPL ở đầu dòng và tên tệp bên trong chuỗi.
- Sau tên một hàm kèm một dấu cách, kiểu của hàm đó được hiển thị mờ ở cuối dòng.
- Dấu ngoặc tương ứng với dấu ngoặc cạnh con trỏ được tô sáng.
- Nếu nhấn Enter trước khi một form kết thúc, dòng tiếp theo bắt đầu thụt lề theo số dấu ngoặc còn
  mở.

## Kết quả trước đó (`:*`, `:**`, `:***`)

Trong REPL, `:*` là giá trị in ra gần nhất, `:**` là giá trị trước đó, và `:***` là giá trị trước
đó nữa. Mỗi cái mang kiểu của giá trị tương ứng.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Đầu vào không in ra giá trị (như một định nghĩa) hoặc bị lỗi sẽ không đẩy các kết quả lên.
- Viết `:**` hoặc `:***` khi chưa có đủ ngần ấy kết quả là lỗi.
- `':*` có quote là một từ khóa thông thường.
- `:*` viết trong thân hàm tiếp tục trỏ tới kết quả lúc hàm được định nghĩa. `*` của Common Lisp
  đọc giá trị lúc hàm được gọi, còn typelisp cố định nó lúc định nghĩa để nó có kiểu.
- Bên ngoài REPL (khi chạy tệp, và trong `~/.typlrc`, `--load`, `--eval`), `:*` là một từ khóa
  thông thường.

## Dừng một phép tính (`Ctrl+C`)

Nhấn `Ctrl+C` trong lúc đánh giá sẽ dừng phép tính đó và quay về dấu nhắc. Những gì đã định nghĩa
trước đó vẫn còn. Điều này áp dụng cho vòng lặp trong trình thông dịch, vòng lặp trong mã đã biên
dịch, và cả lúc đang chờ trong `sleep` hay `wait`. Phần dọn dẹp của các `unwind-protect` đang dở
được thực hiện.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` nhấn trong lúc gõ chỉ bỏ dòng đang gõ.

## Điều tra sau khi có lỗi (`:backtrace`, `:locals`)

Sau khi một lần đánh giá dừng vì lỗi, `:backtrace` (`:bt`) in các lời gọi hàm đang dở vào lúc đó,
từ trong cùng ra ngoài. `:locals N` in các đối số của lời gọi thứ N theo kiểu đã khai báo.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Với lời gọi từ biểu thức cuối cùng của một hàm (lời gọi đuôi), hàm gọi không được hiển thị. Nhờ
  vậy một vòng lặp đệ quy đuôi chỉ dùng chỗ của một lời gọi, dù chạy bao nhiêu vòng.
- Các lời gọi bên trong hàm đã biên dịch và các hàm vô danh (`lambda`) không được hiển thị.
- `:locals` chỉ hiển thị đối số, không hiển thị biến được ràng buộc bằng `let`.
- Khi đánh giá dòng tiếp theo, ghi nhận về lỗi trước đó bị xóa.
