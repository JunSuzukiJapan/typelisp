<!-- translated-from: docs/ja/guide/compile.md @ e26625a7799b8b032c3d524896a6b548c7c107af -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Nếu không có `-o`, tệp thực thi được đặt tên theo tệp nguồn bỏ đi `.typl` và được đặt trong cùng thư
mục với tệp nguồn. Nếu tên tệp nguồn không kết thúc bằng `.typl`, bắt buộc phải có `-o`. Với `-c`
(`--compile`), không thể chỉ định `--image` và `--feature`.

Với `--release`, `typl` tạo **bản dựng release**, loại bỏ `eprint`/`eprintln` (đầu ra lỗi chuẩn dùng để điều tra; [In](../reference/functions/printing.md)) cùng với các đối số của chúng. Những gì bị loại bỏ vẫn được kiểm tra kiểu, nên một lỗi kiểu ở đó làm việc biên dịch thất bại. `--release` là tùy chọn của toàn bộ `typl`: nó có tác dụng như nhau không chỉ với `-c` mà còn khi chạy một tệp, trong REPL và với `typl test`. `compile-file` được gọi bên trong một `typl` chạy với `--release` cũng tạo bản dựng release, và `eprint`/`eprintln` cũng bị loại bỏ khỏi mã chạy bằng `eval` bên trong tệp thực thi được tạo như vậy.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # viết sau -c cũng như nhau
```

`-O0`, `-O1`, `-O2` và `-O3` đặt **mức tối ưu hóa**. Mặc định là `-O0`, không tối ưu hóa. Từ `-O1` trở lên, trình biên dịch trước hết thực hiện các tối ưu hóa đơn giản, như thay một biến chỉ được gắn với một literal bằng giá trị của nó hoặc tính trước phép toán số nguyên trên các hằng, rồi để phần còn lại cho tối ưu hóa của LLVM ở mức cùng tên. Ở mọi mức, kết quả tính toán đều như nhau. Giống như `--release`, đây là tùy chọn của `typl` nói chung: ngoài tệp thực thi mà `-c` tạo ra, nó còn có hiệu lực với mã được biên dịch khi chạy một tệp, trong REPL và trong `typl test`, cũng như với `compile-file` được gọi ở đó. Mã do trình thông dịch chạy không được tối ưu hóa.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # viết sau -c cũng như nhau
```

Bạn có thể làm tương tự bằng cách gọi `compile-file` từ REPL hoặc từ một chương trình.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- Cấp cao nhất của tệp đầu vào chỉ được chứa các định nghĩa (`defun` `defvar`
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

### 3.5 Tài nguyên tệp thực thi sử dụng

`typl -c` có các tùy chọn cho biết tệp thực thi được dùng những tài nguyên nào trong khi chạy.

```sh
$ typl --heap-cells 100000 -c hello.typl    # cố định vùng cons ở 100000 ô
$ typl -c hello.typl --max-memory 64M       # cấp phát tối đa 64 MiB
$ typl -c hello.typl --no-cons              # không dùng ô cons
```

- `--heap-cells N` cố định dung lượng vùng ô cons ở `N` ô. Vùng không lớn thêm: nếu thu gom rác không giải phóng được ô nào, chương trình dừng với lỗi thời gian chạy. Khi không chỉ định, vùng bắt đầu với 65536 ô và lớn dần khi cần.
- `--max-memory SIZE` giới hạn tổng bộ nhớ tệp thực thi cấp phát. Mọi thứ đều được tính: vùng cons, chuỗi, các giá trị như vector và bảng băm, và bộ nhớ mà chính runtime dùng. `SIZE` là số byte và có thể theo sau bởi `K`, `M` hoặc `G` (gấp 1024, 1024² và 1024³ lần). Một lần cấp phát vượt quá giới hạn sẽ được báo, và chương trình kết thúc với mã thoát 1. Kết thúc này không bắt được bằng `catch`. Khi không chỉ định thì không có giới hạn.
- `--no-cons` tạo tệp thực thi không dùng ô cons nào. Nó không có vùng cons, nên một chương trình làm việc cần đến ô cons là **lỗi biên dịch**, hiển thị thao tác đó và chuỗi hàm từ `main` dẫn tới nó. Ô cons cần cho việc tạo danh sách `Sexpr` (kể cả danh sách được quote và danh sách tạo bằng quasiquote) hoặc đối số `&rest`, và cho `read`, `eval`, `macroexpand`. Các thao tác trên số, chuỗi, vector, bảng băm, struct, kênh và những thứ tương tự, cùng `print`/`println`/`format`, không dùng ô nào. Không thể chỉ định cùng `--heap-cells`, và được viết sau `-c`.

`--max-memory` được viết sau `-c`. `--heap-cells` có thể viết trước hoặc sau `-c`, và khi đó là dung lượng vùng cons của tệp thực thi. Vùng cons được tính vào giới hạn bộ nhớ, nên với một giới hạn không chứa nổi vùng, chương trình dừng trước khi bắt đầu.

### 3.6 Biên dịch sang WebAssembly

Với `--target wasm32-wasip1`, `typl -c` tạo một mô-đun WebAssembly (một lệnh WASI preview 1) thay cho tệp thực thi. Không có `-o` thì tên là tên tệp nguồn với `.typl` đổi thành `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # tạo hello.wasm
$ wasmtime hello.wasm a b                        # chạy bằng môi trường hỗ trợ WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- `typl` cài bằng `install.sh` có thể dùng ngay. Khi cài bằng `cargo install` thì cần chuẩn bị (cũng dùng cho mục 3.7). Trước tiên chạy `rustup target add wasm32-wasip1 wasm32-unknown-unknown` bằng rustup. Sau đó cài với `--features wasm` (`cargo install --locked --path . --features wasm`).
  `typl` cài mà không có tùy chọn này sẽ không có runtime WebAssembly, và `--target` báo lỗi. Với bản build debug của `typl` trong kho mã, hãy chạy `scripts/build-wasm-runtime.sh` trong kho mã thay thế.
- Việc liên kết dùng trình liên kết của toolchain rustup (`rust-lld`), nên khi biên dịch cũng cần rustup.
- Mô-đun hoạt động như tệp thực thi: đầu ra chuẩn, lỗi chuẩn và mã thoát, `(command-line-args)`,
  đọc ghi tệp, `throw`/`catch`/`panic`, tác vụ, kênh và `sleep`, cùng `eval` đều dùng được.
  Các tác vụ chạy lần lượt trên một luồng. Chỉ đọc ghi được tệp trong các thư mục
  mà môi trường cho mô-đun thấy (với wasmtime, chỉ định bằng `--dir`).
- Chương trình dùng thứ mà WebAssembly không có là **lỗi biên dịch**. Lỗi cho biết thứ đã dùng và
  chuỗi hàm từ `main` dẫn tới đó. Không dùng được: socket (TCP, UDP, miền Unix,
  TLS, phân giải tên), luồng của hệ điều hành (`Thread`), hàm C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  và `ed`.
- WASI không có cách nào biết múi giờ, nên giờ địa phương là không biết được. `decode-universal-time` và `encode-universal-time` khi bỏ `zone`, cùng `get-decoded-time`, sẽ panic khi chạy. Chúng dùng được khi đưa `zone`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` và `--release` dùng được như với tệp thực thi.

### 3.7 Chạy trong trình duyệt hoặc Node.js

Với `--target wasm32-unknown-unknown`, `typl -c` tạo một mô-đun WebAssembly để nạp và chạy từ JavaScript. Bên cạnh mô-đun (`hello.wasm`), trong cùng thư mục, có tệp JavaScript nạp và chạy nó (`hello.mjs`). Việc chuẩn bị giống mục 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # tạo hello.wasm và hello.mjs
$ node hello.mjs                                       # chạy bằng Node.js
args: #()
fib(25) = 75025
```

Từ JavaScript khác, hãy gọi `run` trong `hello.mjs`. `write` nhận đầu ra dưới dạng văn bản (`fd` là 1 cho đầu ra chuẩn, 2 cho lỗi chuẩn). Giá trị của `run` là mã thoát.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* ví dụ hiển thị lên trang */ } });
```

- Không có `write` thì Node.js ghi ra đầu ra chuẩn và lỗi chuẩn, còn trình duyệt ghi ra console, từng dòng một.
- Trong trình duyệt, hãy phục vụ `hello.wasm` qua HTTP từ cùng chỗ với `hello.mjs` (không nạp được từ `file://`).
- `sleep` chờ bằng `Atomics.wait`. Trình duyệt chỉ cho phép việc này trong Worker của trang được cô lập cross-origin, nên hãy chạy chương trình dùng `sleep` trong Worker.
- Giờ địa phương (`decode-universal-time` khi bỏ `zone`, v.v.) được tính theo múi giờ mà `Date` của JavaScript báo.
- Ngoài những gì không dùng được ở mục 3.6, các thao tác với tệp và thư mục, biến môi trường, `(command-line-args)`, thư mục home và `dribble` là **lỗi biên dịch**. Đầu vào chuẩn được đọc là rỗng.

### 3.8 Biên dịch cho vi điều khiển (Cortex-M, RISC-V)

Với `--target thumbv7em-none-eabihf`, `typl -c` tạo tệp thực thi (ELF) cho bo mạch Cortex-M4F hoặc M7 không có hệ điều hành. Nếu không có `-o`, tên là tên mã nguồn với `.typl` đổi thành `.elf`. Với `--target thumbv8m.main-none-eabihf`, là cho bo mạch Cortex-M33. Với `--target riscv32imac-unknown-none-elf`, là cho bo mạch có lõi RISC-V 32 bit (các phần mở rộng M, A và C). Với `--target thumbv6m-none-eabi`, là cho bo mạch Cortex-M0+ (M0). QEMU không có bo mạch Cortex-M0+, nên tệp thực thi cho nó chạy trên `mps2-an385`, một Cortex-M3 chạy nguyên các lệnh đó (bố cục khi không có `--linker-script` cũng là của bo mạch này).

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # tạo hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # chạy trên Cortex-M4 của QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # chạy trên Cortex-M33 của QEMU
$ typl -c hello.typl --target thumbv6m-none-eabi
$ qemu-system-arm -M mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # chạy trên Cortex-M3 của QEMU
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # chạy trên RISC-V của QEMU
```

- `typl` cài bằng `install.sh` dùng được ngay. Để cài bằng `cargo install`, chạy `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf thumbv6m-none-eabi riscv32imac-unknown-none-elf` rồi cài với `--features embedded` (`--features wasm,embedded` cho cả hai). Với bản build gỡ lỗi của `typl` trong kho mã, chạy `scripts/build-embedded-runtime.sh`. Việc liên kết dùng trình liên kết của toolchain rustup.
- Cho biết bộ nhớ của bo mạch bằng `--linker-script FILE`. Tệp được viết như `memory.x` của `cortex-m-rt`: địa chỉ và kích thước của FLASH và RAM. Một dòng như `_typelisp_stack_size = 32K;` đặt phần RAM mà ngăn xếp dùng (mặc định 64KiB); phần RAM còn lại là heap. Không có tùy chọn này thì dùng bố trí của `mps2-an386` (`mps2-an505` cho Cortex-M33) trong QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  Trên RISC-V, nếu không chỉ định thì dùng bố cục của `virt` trong QEMU. Tệp được chỉ định còn gọi tên bằng `REGION_ALIAS`, như `memory.x` của `riscv-rt`, vùng mà mỗi loại section được đặt vào.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x20010000, LENGTH = 4M - 64K
    RAM : ORIGIN = 0x80000000, LENGTH = 16K
  }
  REGION_ALIAS("REGION_TEXT", FLASH);
  REGION_ALIAS("REGION_RODATA", FLASH);
  REGION_ALIAS("REGION_DATA", RAM);
  REGION_ALIAS("REGION_BSS", RAM);
  REGION_ALIAS("REGION_HEAP", RAM);
  REGION_ALIAS("REGION_STACK", RAM);
  _typelisp_stack_size = 4K;
  ```

- Trên bo mạch ít RAM, thay cho `--linker-script`, `--ram SIZE` (`16K` v.v.) chỉ cho biết dữ liệu, heap và ngăn xếp có bao nhiêu RAM. RAM của bố cục mặc định có kích thước đó, và ngăn xếp dùng tối đa một phần tư. Trên bo mạch cũng đặt mã trong RAM, như `virt` của RISC-V, mã nằm ở phần RAM còn lại của bo mạch. Những gì runtime cấp phát lúc đầu được thu nhỏ cho vừa heap. Không dùng chung được với `--linker-script`.

- Bo mạch không có hệ điều hành nên không gì chặn được stack bị tràn. Vì vậy tệp thực thi kiểm tra, khi vào mỗi hàm của chương trình, còn bao nhiêu stack; nếu còn quá ít, nó ghi `stack overflow: the program needs more than the ... bytes of the stack` và kết thúc với mã thoát 1. Muốn stack lớn hơn, hãy tăng `_typelisp_stack_size` trong tệp của `--linker-script`, hoặc `--ram`. `--no-stack-check` bỏ việc kiểm tra này (tệp nhỏ và nhanh hơn một chút, nhưng stack bị tràn có thể âm thầm làm hỏng heap). Trong cả hai trường hợp, khi stack tràn bên trong runtime và vượt qua đáy của nó, báo cáo panic hoặc HardFault theo sau sẽ được ghi sau dòng `stack overflow: the stack has run past its bottom into the heap`. Điều tương tự cũng được ghi, với mã thoát 1, khi chương trình chạy đến cuối. Lưu ý rằng độ sâu của đệ quy không dùng stack (bản ghi các lời gọi nằm trên heap, và đệ quy quá sâu sẽ hết bộ nhớ); thứ dùng stack sâu là công việc của runtime.

- `--board-functions` chọn ai cung cấp đầu ra, đầu vào, kết thúc và đồng hồ (các hàm của bo mạch).
  - `semihosting` (mặc định): qua semihosting, để chạy trong QEMU (`-semihosting-config enable=on`) hoặc trình gỡ lỗi.
  - `c`: bo mạch cung cấp chúng dưới dạng hàm C. Truyền bằng `--link FILE` một tệp đối tượng hoặc thư viện tĩnh định nghĩa các hàm dưới đây (`--link` có thể lặp lại; thư viện của nhà sản xuất mà `defffi` gọi cũng truyền như vậy).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 là đầu ra chuẩn, 2 là lỗi chuẩn
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; số byte đã đọc, 0 khi hết
  void     typelisp_host_exit(int32_t code);                              // không trở về
  uint64_t typelisp_host_now_us(void);                                    // micro giây của đồng hồ không bao giờ lùi
  int64_t  typelisp_host_unix_us(void);                                   // micro giây kể từ 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // chờ us micro giây
  ```

- Thanh ghi của bo mạch (thiết bị ngoại vi ánh xạ vào bộ nhớ) được đọc bằng `volatile-read-u8`, `volatile-read-u16` và `volatile-read-u32`, và được ghi bằng `volatile-write-u8`, `volatile-write-u16` và `volatile-write-u32`. Địa chỉ được truyền dưới dạng `int` hoặc số nguyên có độ rộng cố định như `u16`, và giá trị đọc được có kiểu theo độ rộng (`u8`, v.v.). Mỗi lệnh đọc hoặc ghi đúng một lần, tại nơi nó được viết; trình biên dịch không bỏ đi hay di chuyển nó. Tất cả chỉ dùng được bên trong `unsafe`. Địa chỉ âm, hoặc không phải bội số của độ rộng, là lỗi thời gian chạy trước khi đọc hay ghi bất cứ thứ gì.

  ```lisp
  ;; UART 0 của mps2-an386 trong QEMU: dữ liệu 0x40004000, trạng thái 0x40004004, điều khiển 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; chờ khi đang gửi
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; bật gửi
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" và xuống dòng
  ```

- Viết hàm được gọi khi có ngắt đến dưới dạng `defun` có thuộc tính `#[interrupt(N)]` (ngắt ngoại vi `N`) hoặc như `#[exception(SysTick)]`. Bật ngắt bằng `(enable-interrupt N)`. Trình xử lý chạy ngay; chỉ khi ngắt đến giữa lúc runtime đang làm việc thì nó chạy sau khi công việc đó xong. Trình xử lý không thể chờ (`sleep` là lỗi biên dịch). Chi tiết ở mục 3.17 của [tài liệu tham khảo cú pháp](../reference/syntax.md). Trên Cortex-M0+, `N` từ 0 đến 31, và `#[exception(NAME)]` chỉ có thể là `NonMaskableInt`, `SVCall`, `PendSV` và `SysTick`. Trên RISC-V, `N` là số hiệu của ngắt trong `mcause` (0 đến 31: 3 là ngắt phần mềm, 7 là bộ định thời, 11 là ngắt ngoài), và không dùng được `#[exception(NAME)]`.

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; tự gây ra ngắt 5 (thanh ghi chờ xử lý của NVIC)
  ```

- Runtime khởi động chương trình (bảng vector và chuẩn bị RAM) và cấp phát bộ nhớ. Một HardFault (trên RISC-V là một ngoại lệ) hoặc ngắt không được xử lý kết thúc chương trình với mã thoát của panic (1).
- `throw`/`catch`/`panic`, số lớn, phân số và số thực dấu phẩy động, cùng tác vụ, kênh và `sleep` hoạt động như trong tệp thực thi. Các tác vụ chạy lần lượt trên lõi duy nhất. Có thể dùng hàm C (`defffi`).
- Chương trình dùng thứ bo mạch không có là **lỗi biên dịch**: ngoài những gì không dùng được ở mục 3.7, còn `eval` và `macroexpand`. Giờ địa phương không xác định (như ở mục 3.6).
- Vùng cons bắt đầu với 1024 ô và lớn lên khi cần. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` và `--release` dùng như bình thường.

### 3.9 Biên dịch cho Arduino Uno (AVR)

Với `--target avr-atmega328p`, `typl` tạo tệp thực thi (ELF) cho vi điều khiển của Arduino Uno (ATmega328P). Chương trình sau làm nhấp nháy đèn LED trên bo mạch.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; đặt làm ngõ ra
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; đảo trạng thái
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # tạo ra blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # chạy trong QEMU
```

- Không có runtime, nên chương trình dùng runtime là **lỗi biên dịch**. Lỗi cho biết cần gì và đi tới đó qua những hàm nào. Những thứ dùng được: phép tính trên số nguyên có độ rộng cố định (từ `i8` đến `u32`), `f32`, `f64`, `bool` và `char`, lời gọi hàm (kể cả đệ quy), rẽ nhánh, vòng lặp, `defvar` và `defconstant` chứa các giá trị đó, `volatile-read-u8` cùng các hàm tương tự, và lời gọi hàm C.
- Những thứ không dùng được: giá trị nằm trên heap như chuỗi, danh sách và struct; `catch` và lời gọi giá trị hàm; lời gọi hàm C nhận chuỗi hoặc hàm; các hàm toán học của số dấu phẩy động như `sin` và `expt` (`sqrt` và các phép làm tròn như `floor` thì dùng được).
- Trình xử lý ngắt được viết bằng `#[interrupt(N)]` ([Cú pháp](../reference/syntax.md), mục 3.17). `N` là số hiệu ngắt của ATmega328P, từ 1 đến 25 với reset tính là 0 (tràn bộ định thời 1 là 13). Xem ví dụ bên dưới.
- Chia cho 0 làm vi điều khiển dừng ngay tại đó (cũng không còn ngắt nào đến). Phép `/` của giá trị có dấu nhỏ nhất cho `-1` cũng vậy.
- Không dùng được `int` (là lỗi biên dịch), vì nó không có độ rộng cố định. Hãy dùng kiểu có độ rộng cố định như `i16` hoặc `u16`. Địa chỉ cũng tính được bằng số nguyên có độ rộng cố định. Những thứ có giá trị được xác định khi biên dịch thì dùng được, như địa chỉ viết bằng số và `int` của `defconstant`.
- Khi đệ quy làm hết ngăn xếp thì vi điều khiển dừng.
- Khai báo hàm C bằng `defffi`, và truyền tệp đối tượng đã biên dịch cho AVR bằng `--link FILE`.
- Vòng lặp không làm gì sẽ bị xóa từ `-O1` trở lên. Trong vòng lặp chờ, hãy đọc một thanh ghi chẳng hạn.
- Khi `main` kết thúc, vi điều khiển dừng lại.
- Bố trí bộ nhớ được thay đổi bằng `--linker-script FILE`. Nếu bỏ qua, đó là 32 KiB flash trừ 512 byte cuối của bộ nạp khởi động, và 2 KiB RAM. Địa chỉ RAM được viết cộng thêm `0x800000`, ví dụ `0x800100`.
- Phép tính trên `f32` và `f64` do các hàm hỗ trợ thực hiện. Những hàm được dùng sẽ nằm trong flash (các hàm của `f64` lớn hơn).
- Việc liên kết dùng trình liên kết của toolchain rustup như ở mục 3.8.

Ví dụ nháy LED bằng ngắt của bộ định thời. Ngắt được bật trong thanh ghi của chính thiết bị.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; tràn bộ định thời 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; đảo trạng thái
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; bật ngắt tràn
          (volatile-write-u8 TCCR1B 3))           ; đếm ở 1/64 xung nhịp
  (while true))
```

- Chương trình có trình xử lý sẽ nhận ngắt ngay khi các biến toàn cục được khởi tạo xong. Hãy thiết lập thiết bị ở đầu `main`.
- Không dùng được `enable-interrupt` và `#[exception(NAME)]` (lỗi biên dịch).
- Trong lúc một trình xử lý chạy, các ngắt khác phải chờ. Như mọi hàm khác, trình xử lý không dùng được runtime.
- Một lần đọc hoặc ghi biến toàn cục không bị trình xử lý xen vào giữa, kể cả với giá trị từ hai byte trở lên. Để làm vậy, ngắt được tắt khi đọc biến mà trình xử lý ghi và khi ghi biến mà trình xử lý đọc hoặc ghi, và biến đó được nêu trong cảnh báo khi biên dịch. Giữa lúc đọc và lúc ghi của một phép cập nhật (`(setf n (+ n 1))`) thì ngắt có thể đến. Để cập nhật biến mà trình xử lý cũng ghi, hãy đọc và giữ lại thanh ghi trạng thái (địa chỉ `#x5F`), ghi 0 vào đó để dừng ngắt, rồi ghi lại giá trị đã giữ.
- Ngắt không có trình xử lý sẽ làm vi điều khiển dừng.

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
