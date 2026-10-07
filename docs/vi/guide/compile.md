<!-- translated-from: docs/ja/guide/compile.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Biên dịch

Nếu bạn không làm gì thêm, chương trình typelisp chạy trong trình thông dịch. Ngoài ra còn có hai
cách biên dịch sang mã native và một cách lưu lại môi trường. Chi tiết đặc tả nằm ở
[Tham chiếu cú pháp chương 10](../reference/syntax.md#10-biên-dịch).

| Phương pháp | Cách làm | Kết quả |
|---|---|---|
| Biên dịch JIT | `(compile name)` | Một hàm trong phiên đang chạy được thay bằng mã native |
| Biên dịch AOT | `typl -c src.typl` hoặc `(compile-file "src.typl" "out")` | Một tệp thực thi độc lập |
| Dump | `(dump "file.typld")` | Lưu các định nghĩa; `typl --image` khởi động lại từ cùng môi trường đó |

## 1. Chuẩn bị

Biên dịch dùng LLVM 22. Nếu bạn đã build `typl` theo [README.md](../../../README.md), không cần chuẩn
bị gì thêm.

Các tệp thực thi tạo bằng biên dịch AOT được liên kết với thư viện tĩnh `libtypelisp_front.a`. Bản
build release của `typl` (kể cả bản cài bằng `cargo install`) mang thư viện này bên trong nó, nên không
cần chuẩn bị gì. Lần biên dịch đầu tiên, nó ghi thư viện ra `~/.typelisp/lib/<build ID>/` (hoặc
`$TYPELISP_HOME/lib/<build ID>/` nếu đặt biến môi trường `TYPELISP_HOME`) và từ đó về sau dùng bản sao
này. `typl --remove-lib` xóa nó (với `--others` là các bản do các phiên bản `typl` khác ghi ra; với
`--all` là tất cả). Bản build debug của `typl` dùng thư viện trong `target/debug/` của repository mà nó
được build ra. Để dùng thư viện đặt ở nơi khác, hãy chỉ ra thư mục của nó bằng `--lib-dir` khi khởi
động `typl` (mục 3.2). Trên macOS, việc liên kết dùng Xcode Command Line Tools.

## 2. Biên dịch JIT

Cách này biến một hàm đã được định nghĩa thành mã native ngay tại chỗ.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; từ đây trở đi, các lời gọi chạy mã đã biên dịch
```

- `name` không được đánh giá. Hãy viết tên hàm nguyên dạng (không phải chuỗi). Với phương thức, hãy
  viết kèm tên kiểu, như `(compile point::norm)`.
- Các hàm mà nó gọi được biên dịch cùng với nó.
- **Hàm generic không thể biên dịch.** Một bản sao cho từng kiểu được tạo ở mỗi nơi nó được dùng. Hãy
  biên dịch hàm gọi nó với các kiểu cụ thể thay vào đó.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` và `dump` là các thao tác của trình thông
  dịch, nên hàm gọi chúng không thể biên dịch. Cố biên dịch sẽ cho lỗi nêu rõ lý do.

Để xem kết quả biên dịch, dùng `disassemble`.

```lisp
(disassemble fib)          ; mã máy của máy chủ
(disassemble fib true)     ; LLVM IR
```

## 3. Tạo tệp thực thi bằng biên dịch AOT

### 3.1 Viết chương trình

Làm điểm vào, hãy định nghĩa một **hàm `main` không nhận đối số**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

`(main)` ở cuối tệp có mặt để `main` được gọi khi bạn chạy `typl hello.typl`. `compile-file` bỏ qua
`(main)` cuối cùng này, nên cùng một tệp dùng được cả trong trình thông dịch lẫn với biên dịch AOT.

### 3.2 Biên dịch

Từ dòng lệnh, dùng `typl -c` (`typl --compile` cũng như vậy).

```sh
$ typl -c hello.typl            # tạo ra hello
$ typl -c hello.typl -o fib     # đặt tên tệp thực thi là fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Nếu không có `-o`, tệp thực thi được đặt tên theo tệp nguồn bỏ đi `.typl` và được đặt trong cùng thư
mục với tệp nguồn. Nếu tên tệp nguồn không kết thúc bằng `.typl`, bắt buộc phải có `-o`. Với `-c`
(`--compile`), không thể chỉ định `--image`, `--heap-cells` và `--feature`.

Bạn có thể làm tương tự bằng cách gọi `compile-file` từ REPL hoặc từ một chương trình.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Nếu build nhiều lần, bạn có thể đặt một dòng này vào một tệp và chạy bằng `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Tên tệp được phân giải từ **thư mục hiện tại nơi `typl` được khởi động**, không phải từ vị trí của
`build.typl`.

Để liên kết một `libtypelisp_front.a` đặt ở nơi khác với chỗ `typl` tìm, hãy chỉ ra thư mục của nó bằng
`--lib-dir`. Tùy chọn này áp dụng cho cả `typl -c` và `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Nếu thư mục được chỉ ra không có `libtypelisp_front.a`, `typl` dừng lại với một lỗi. Tệp này chỉ hoạt động
với `typl` được build cùng với nó. Sau khi build lại `typl`, hãy sao chép lại.

### 3.3 Tệp biên dịch AOT được phép chứa gì

- Cấp cao nhất của tệp đầu vào chỉ được chứa các định nghĩa (`defun` `defmethod` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) và `use` `module`. Các biểu thức cấp cao nhất như
  `(println ...)` không được phép, ngoại trừ `(main)` ở cuối. Hãy đặt công việc vào bên trong `main`.
- Nếu không có `main` không nhận đối số, biên dịch thất bại với một lỗi.
- Các tệp của module được `use` cũng được biên dịch và gộp vào một tệp thực thi duy nhất.
- Các thư viện được nêu bằng `:library` trong `defffi` được liên kết tự động ([C FFI](ffi.md)).
- Mọi hàm của thư viện chuẩn đều dùng được với biên dịch AOT. `eval` cũng dùng được, nhưng khi đó bộ
  kiểm tra kiểu và trình thông dịch được đưa vào tệp thực thi, làm nó lớn hơn và khởi động chậm hơn.
  Các chương trình không gọi `eval` thì không chứa chúng.

### 3.4 Tệp thực thi hoạt động ra sao

- `(command-line-args)` trả về một `Vector<string>` cùng dạng dù chạy bằng `typl hello.typl a b` hay
  bằng `./hello a b`. Phần tử đầu tiên là tên chương trình.
- Đặt mã thoát bằng `(exit n)`. Nếu `main` trả về bình thường, mã thoát là 0.
- Khi `panic`, chương trình in thông điệp và thoát với mã khác 0.

## 4. Dump

Bạn có thể lưu các định nghĩa của phiên hiện tại vào một tệp và khởi động từ đó lần sau.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Nó cũng dùng được khi chạy một tệp, như `typl --image session.typld prog.typl`.

- Những gì được lưu là các **định nghĩa**. Các biểu thức được đánh giá trong phiên không được lưu.
- Các hàm bạn đã `compile` được lưu ở dạng đã biên dịch.
- Biến toàn cục được khôi phục bằng cách **chạy lại các biểu thức khởi tạo của chúng**, không phải
  bằng giá trị chúng có khi dump được ghi.
- Một dump không thể được nạp bởi `typl` có phiên bản khác với phiên bản đã ghi ra nó (đó là lỗi).

Nếu bạn chạy một tệp và `(dump ...)` từ trong đó, các định nghĩa của tệp đó nằm trong một module mang
tên của tệp. Một hàm định nghĩa trong `dp.typl` có tên `dp::sq`, và gọi nó từ tệp khác cần có `pub`
([Module và cách bố trí tệp](modules.md)).

## 5. Về các tệp module đã biên dịch

Không có định dạng nào, như `.fasl` của Common Lisp, để ghi kết quả biên dịch của từng module ra tệp.
`compile-file` tạo tệp thực thi trực tiếp từ mã nguồn. Không có tệp trung gian nào được để lại.
