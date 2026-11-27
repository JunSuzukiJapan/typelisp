<!-- translated-from: docs/ja/guide/compile.md @ e26625a7799b8b032c3d524896a6b548c7c107af -->
# 编译

如果不做任何处理，typelisp 程序由解释器执行。除此之外，还有两种编译为本机代码的方法，以及一种保存环境的方法。
规范的详情见[语法参考第 10 章](../reference/syntax.md#10-编译)。

| 方法 | 用法 | 结果 |
|---|---|---|
| JIT 编译 | `(compile name)` | 正在运行的会话中的函数被替换为本机代码 |
| AOT 编译 | `typl -c src.typl` 或 `(compile-file "src.typl" "out")` | 得到可独立运行的可执行文件 |
| 转储 | `(dump "file.typld")` | 保存定义，可以用 `typl --image` 从同样的环境启动 |

## 1. 准备

编译使用 LLVM 22。如果已经按照 [README.md](../../../README.md) 的步骤构建了 `typl`，就不需要额外准备。

AOT 编译生成的可执行文件会链接静态库 `libtypelisp_front.a`。发布版构建的 `typl`（包括用 `cargo install` 安装的）
内部带有这个库，所以不需要准备。首次编译时，它会写出到 `~/.typelisp/lib/<构建ID>/`（如果设置了环境变量
`TYPELISP_HOME`，则为 `$TYPELISP_HOME/lib/<构建ID>/`），之后使用写出的那一份。可以用 `typl --remove-lib` 删除
（加 `--others` 删除其他版本的 `typl` 写出的，加 `--all` 删除全部）。调试版构建的 `typl` 使用构建它的仓库中
`target/debug/` 下的库。要使用放在其他位置的库，在启动 `typl` 时用 `--lib-dir` 指定其所在文件夹（3.2 节）。
在 macOS 上链接需要 Xcode Command Line Tools。

## 2. JIT 编译

把已经定义的函数当场编译为本机代码。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; 之后的调用运行编译后的代码
```

- `name` 不会被求值。直接写函数名（不是字符串）。方法要像 `(compile point::norm)` 这样加上类型名。
- 它调用的函数也会一起编译。
- **泛型函数不能编译。** 因为每种类型的实体是在每个使用处生成的。请编译以具体类型调用它的那个函数。
- `trace`、`step`、`disassemble`、`compile`、`compile-file`、`dump` 是解释器的操作，调用它们的函数不能编译。
  尝试编译时会得到说明原因的错误。

查看编译结果使用 `disassemble`。

```lisp
(disassemble fib)          ; 主机的机器码
(disassemble fib true)     ; LLVM IR
```

## 3. 用 AOT 编译生成可执行文件

### 3.1 编写程序

作为入口，定义一个**不接受参数的 `main` 函数**。

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

文件末尾的 `(main)` 是为了在用 `typl hello.typl` 运行时调用 `main`。`compile-file` 会跳过这个末尾的 `(main)`，
所以同一个文件既可以用解释器运行，也可以用于 AOT。

### 3.2 编译

在命令行中使用 `typl -c`（`typl --compile` 相同）。

```sh
$ typl -c hello.typl            # 生成 hello
$ typl -c hello.typl -o fib     # 把可执行文件命名为 fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

省略 `-o` 时，会在源文件所在文件夹生成以源文件名去掉 `.typl` 命名的可执行文件。源文件名不以 `.typl` 结尾时需要
`-o`。使用 `-c`（`--compile`）时，不能指定 `--image`、`--feature`。

给 `typl` 加上 `--release` 就是**发布构建**，会连同参数一起移除 `eprint`/`eprintln`（用于调查的标准错误输出。[打印](../reference/functions/printing.md)）。被移除的部分也会进行类型检查，所以其中有类型错误时编译会失败。`--release` 是整个 `typl` 的选项，不仅对 `-c`，对运行文件、REPL 和 `typl test` 也同样有效。在加了 `--release` 的 `typl` 中调用的 `compile-file` 也会是发布构建，在这样生成的可执行文件中用 `eval` 执行的代码里，`eprint`/`eprintln` 也会被移除。

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # 写在 -c 之后也一样
```

用 `-O0` `-O1` `-O2` `-O3` 指定**优化级别**。默认是 `-O0`，不做优化。从 `-O1` 起，编译器先做一些简单的优化，例如把只绑定了字面量的变量替换为它的值、预先算好常量之间的整数运算，其余交给 LLVM 同名级别的优化。无论哪个级别，计算结果都不变。与 `--release` 一样，这是整个 `typl` 的选项：除了 `-c` 生成的可执行文件，也作用于运行文件、REPL 和 `typl test` 中编译的代码，以及在那里调用的 `compile-file`。解释器执行的代码不做优化。

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # 写在 -c 之后也一样
```

在 REPL 或程序中调用 `compile-file` 也能做同样的事。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

需要反复构建时，可以把这一行写进文件，用 `typl build.typl` 运行。

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

文件名从**启动 `typl` 时的当前目录**解析，而不是从 `build.typl` 所在位置。

要链接放在 `typl` 所用位置以外的 `libtypelisp_front.a` 时，用 `--lib-dir` 指定其所在文件夹。对 `typl -c` 和
`compile-file` 都有效。

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

指定的文件夹中没有 `libtypelisp_front.a` 时，会报错停止。这个文件只能与同时构建的 `typl` 一起使用。重新构建
`typl` 后，请重新复制。

### 3.3 可以进行 AOT 编译的文件形式

- 入口文件的顶层只能写定义（`defun` `defvar` `defparameter` `defconstant` `defmacro`
  `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`、`(unsafe (def-c-struct ...))`）
  以及 `use` `module`。除了末尾的 `(main)`，不能写 `(println ...)` 这样的顶层表达式。处理请写在 `main` 中。
- 没有不带参数的 `main` 时会报错。
- 被 `use` 的模块文件也会一起编译，合并成一个可执行文件。
- `defffi` 的 `:library` 中指定的库会自动链接（[C FFI](ffi.md)）。
- 标准库的所有函数都可以在 AOT 中使用。`eval` 也可以使用，但这时类型检查器和解释器会进入可执行文件，使文件变大，
  启动也更费时间。不调用 `eval` 的程序不会包含它们。

### 3.4 可执行文件的行为

- 无论用 `typl hello.typl a b` 运行还是用 `./hello a b` 运行，`(command-line-args)` 都返回相同形式的
  `Vector<string>`。第一个元素是程序名。
- 退出码用 `(exit n)` 指定。`main` 正常返回时为 0。
- 发生 panic 时显示消息，并以非 0 退出。

### 3.5 可执行文件使用的资源

`typl -c` 有一些选项，用来指定可执行文件运行时可以使用的资源。

```sh
$ typl --heap-cells 100000 -c hello.typl    # 把 cons 区域固定为 100000 个单元
$ typl -c hello.typl --max-memory 64M       # 最多分配 64 MiB 内存
$ typl -c hello.typl --no-cons              # 不使用 cons 单元
```

- `--heap-cells N` 把 cons 单元区域的容量固定为 `N` 个单元。区域不会再变大。垃圾回收后仍无法腾出空单元时，以运行时错误停止。不指定时从 65536 个单元开始，按需变大。
- `--max-memory SIZE` 是可执行文件分配的内存总量的上限。cons 区域、字符串、向量和哈希表等值、运行时自身使用的内存全部计算在内。`SIZE` 是字节数，可以加上 `K` `M` `G`（分别为 1024 倍、1024² 倍、1024³ 倍）。试图超过上限分配时，会显示这一情况并以退出码 1 结束。这种结束无法用 `catch` 捕获。不指定时没有上限。
- `--no-cons` 生成不使用 cons 单元的可执行文件。它没有 cons 区域，因此进行需要 cons 单元的操作的程序会成为**编译错误**。错误中会显示该操作，以及从 `main` 到达那里的函数序列。需要 cons 单元的情况是：构造 `Sexpr` 列表（包括引用的列表和用准引用构造的列表）或 `&rest` 参数时，以及使用 `read`、`eval`、`macroexpand` 时。数值、字符串、向量、哈希表、结构体、通道等的操作，以及 `print`/`println`/`format` 都不使用 cons 单元。不能与 `--heap-cells` 一起指定，写在 `-c` 之后。

`--max-memory` 写在 `-c` 之后。`--heap-cells` 写在 `-c` 之前或之后都可以，这时它是可执行文件的 cons 区域的容量。cons 区域也计入内存上限，所以指定了放不下区域的上限时，程序会在开始之前停止。

### 3.6 编译为 WebAssembly

加上 `--target wasm32-wasip1` 后，`typl -c` 生成的不是可执行文件，而是 WebAssembly 模块（WASI preview 1 命令）。省略 `-o` 时，名字是把源文件的 `.typl` 换成 `.wasm`。

```sh
$ typl -c hello.typl --target wasm32-wasip1      # 生成 hello.wasm
$ wasmtime hello.wasm a b                        # 用支持 WASI 的运行环境执行
args: #(hello.wasm a b)
fib(25) = 75025
```

- 用 `install.sh` 安装的 `typl` 可以直接使用。用 `cargo install` 安装时需要准备（同时也是 3.7 节的准备）。首先用 rustup 执行 `rustup target add wasm32-wasip1 wasm32-unknown-unknown`。然后加上 `--features wasm` 安装（`cargo install --locked --path . --features wasm`）。
  不加此项安装的 `typl` 没有 WebAssembly 运行时，`--target` 会报错。使用仓库中的调试版 `typl` 时，改为在仓库中执行 `scripts/build-wasm-runtime.sh`。
- 链接使用 rustup 工具链的链接器（`rust-lld`），因此编译时也需要 rustup。
- 模块的行为与可执行文件相同：标准输出、标准错误和退出码、`(command-line-args)`、
  文件读写、`throw`/`catch`/`panic`、任务、通道和 `sleep`，以及 `eval` 都可以使用。
  任务在一个线程上依次执行。文件只能在运行环境向模块公开的目录中读写
  （wasmtime 用 `--dir` 指定）。
- 使用 WebAssembly 所没有的东西的程序是**编译错误**。错误会显示用到的东西，
  以及从 `main` 到达那里的函数链。不能使用的有：套接字（TCP、UDP、Unix 域、
  TLS、名称解析）、操作系统线程（`Thread`）、C 函数（`defffi`）、`get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  以及 `ed`。
- WASI 没有获知时区的手段，因此本地时间未知。省略 `zone` 的 `decode-universal-time`、`encode-universal-time` 以及 `get-decoded-time` 会在运行时 panic。传入 `zone` 即可使用。
- `--heap-cells`、`--max-memory`、`--no-cons`、`-O<N>`、`--release` 照常可用。

### 3.7 在浏览器或 Node.js 中运行

加上 `--target wasm32-unknown-unknown` 后，`typl -c` 生成由 JavaScript 加载并运行的 WebAssembly 模块。在模块（`hello.wasm`）旁边的同一文件夹中，还会生成加载并运行它的 JavaScript（`hello.mjs`）。准备工作与 3.6 节相同。

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # 生成 hello.wasm 和 hello.mjs
$ node hello.mjs                                       # 用 Node.js 运行
args: #()
fib(25) = 75025
```

在其他 JavaScript 中，调用 `hello.mjs` 的 `run`。`write` 以字符串形式接收输出（`fd` 标准输出为 1，标准错误为 2）。`run` 的值是退出码。

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* 例如显示在页面上 */ } });
```

- 不传 `write` 时，Node.js 输出到标准输出和标准错误，浏览器则逐行输出到控制台。
- 在浏览器中，请通过 HTTP 从与 `hello.mjs` 相同的位置提供 `hello.wasm`（无法从 `file://` 加载）。
- `sleep` 用 `Atomics.wait` 等待。浏览器只允许在跨源隔离页面的 Worker 中使用它，因此使用 `sleep` 的程序请在 Worker 中运行。
- 本地时间（省略 `zone` 的 `decode-universal-time` 等）按 JavaScript 的 `Date` 报告的时区求得。
- 除了 3.6 节中不能使用的东西，文件和目录操作、环境变量、`(command-line-args)`、主目录和 `dribble` 也是**编译错误**。标准输入读作空。

### 3.8 为微控制器（Cortex-M、RISC-V）编译

加上 `--target thumbv7em-none-eabihf`，`typl -c` 会为没有操作系统的 Cortex-M4F、M7 开发板生成可执行文件（ELF）。省略 `-o` 时，名字是把源文件的 `.typl` 换成 `.elf`。加上 `--target thumbv8m.main-none-eabihf` 则是面向 Cortex-M33 开发板的。加上 `--target riscv32imac-unknown-none-elf` 则面向 32 位 RISC-V（M、A、C 扩展）的开发板。加上 `--target thumbv6m-none-eabi` 则面向 Cortex-M0+（M0）的开发板。QEMU 没有 Cortex-M0+ 的开发板，所以面向它的可执行文件在能原样执行相同指令的 Cortex-M3 `mps2-an385` 上运行（省略 `--linker-script` 时的布局也是这块开发板的）。

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # 生成 hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M4 上运行
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M33 上运行
$ typl -c hello.typl --target thumbv6m-none-eabi
$ qemu-system-arm -M mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M3 上运行
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 RISC-V 上运行
```

- 用 `install.sh` 安装的 `typl` 可以直接使用。用 `cargo install` 安装时，先运行 `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf thumbv6m-none-eabi riscv32imac-unknown-none-elf`，再加上 `--features embedded` 安装（与 wasm 两者都要则用 `--features wasm,embedded`）。使用仓库的调试构建 `typl` 时，运行 `scripts/build-embedded-runtime.sh`。链接使用 rustup 工具链的链接器。
- 用 `--linker-script FILE` 给出开发板的内存布局。写法与 `cortex-m-rt` 的 `memory.x` 相同，写 FLASH 和 RAM 的地址与大小。写 `_typelisp_stack_size = 32K;` 这样的一行，就是 RAM 中栈占用的大小（不写则为 64KiB）。RAM 的其余部分是堆。省略时使用 QEMU 的 `mps2-an386`（Cortex-M33 为 `mps2-an505`） 的布局。

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  在 RISC-V 上，省略时使用 QEMU 的 `virt` 的布局。编写时与 `riscv-rt` 的 `memory.x` 一样，还要用 `REGION_ALIAS` 写出各类段所放的区域。

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

- 在 RAM 较小的开发板上，可以用 `--ram SIZE`（如 `16K`）代替 `--linker-script`，只指定数据、堆和栈所用 RAM 的大小。默认布局的 RAM 变为这个大小，栈最多占其四分之一。在像 RISC-V 的 `virt` 这样代码也放在 RAM 中的开发板上，开发板 RAM 的其余部分成为代码的区域。运行时最初分配的量会配合堆的大小而变小。不能与 `--linker-script` 一起使用。

- 开发板没有操作系统，栈溢出时没有任何东西能阻止它。因此可执行文件在进入程序的每个函数时检查栈还剩多少；不够时写出 `stack overflow: the program needs more than the ... bytes of the stack` 并以退出码 1 结束。要让栈更大，请增大 `--linker-script` 文件中的 `_typelisp_stack_size`，或增大 `--ram`。加上 `--no-stack-check` 则不插入此检查（可执行文件会稍小、稍快，但溢出的栈可能悄无声息地破坏堆）。无论哪种情况，当栈在运行时内部溢出并越过栈底时，随后的 panic 或 HardFault 报告之前都会写出 `stack overflow: the stack has run past its bottom into the heap`。 程序运行到结束时也会写出同样的内容，并以退出码 1 结束。另外，递归的深度不占用栈（调用记录放在堆上，过深的递归会导致内存不足）；深度使用栈的是运行时的处理。

- 由谁提供输出、输入、结束和时钟（开发板的函数），用 `--board-functions` 选择。
  - `semihosting`（默认）：通过 semihosting 提供。在 QEMU（`-semihosting-config enable=on`）或调试器下运行时使用。
  - `c`：由开发板以 C 函数提供。用 `--link FILE` 传入定义了下列函数的目标文件或静态库（`--link` 可以写多次；`defffi` 调用的厂商库也同样传入）。

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 是标准输出，2 是标准错误
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0；读到的字节数，结束时为 0
  void     typelisp_host_exit(int32_t code);                              // 不返回
  uint64_t typelisp_host_now_us(void);                                    // 不会倒退的时钟的微秒数
  int64_t  typelisp_host_unix_us(void);                                   // 自 1970-01-01 起的微秒数
  void     typelisp_host_sleep_us(uint64_t us);                           // 等待 us 微秒
  ```

- 板子的寄存器（映射到内存的外设）用 `volatile-read-u8`、`volatile-read-u16`、`volatile-read-u32` 读取，用 `volatile-write-u8`、`volatile-write-u16`、`volatile-write-u32` 写入。地址以 `int` 或 `u16` 等固定宽度的整数传入，读到的值是该宽度的类型（`u8` 等）。每个都在写下它的位置恰好读写一次，编译器既不省略也不移动它。它们都只能在 `unsafe` 中使用。地址为负数或不是宽度的倍数时，在读写之前就是运行时错误。

  ```lisp
  ;; QEMU 的 mps2-an386 的 UART 0：数据 0x40004000，状态 0x40004004，控制 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; 发送中则等待
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; 启用发送
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" 和换行
  ```

- 中断到来时被调用的函数，写成带有 `#[interrupt(N)]`（外设中断 `N`）或 `#[exception(SysTick)]` 这类属性的 `defun`。中断用 `(enable-interrupt N)` 启用。处理程序当场执行，只有在运行时处理途中到来时，才会在该处理结束后执行。处理程序中不能等待（`sleep` 是编译错误）。详见 [语法参考](../reference/syntax.md) 的 3.17 节。在 Cortex-M0+ 上，`N` 为 0～31，`#[exception(NAME)]` 只能写 `NonMaskableInt`、`SVCall`、`PendSV`、`SysTick`。在 RISC-V 上，`N` 是 `mcause` 中的中断编号（0～31。3 是软件中断，7 是定时器，11 是外部中断），不能使用 `#[exception(NAME)]`。

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; 自己引发中断 5（NVIC 的挂起寄存器）
  ```

- 启动（向量表和 RAM 的准备）和内存分配由运行时完成。发生 HardFault（在 RISC-V 上为异常） 或未处理的中断时，以与 panic 相同的退出码（1）结束。
- `throw`/`catch`/`panic`、bignum、ratio、浮点数，以及任务、通道和 `sleep` 的行为与可执行文件相同。任务在唯一的核心上依次执行。也可以使用 C 函数（`defffi`）。
- 使用开发板没有的东西的程序是**编译错误**。除了 3.7 节中不能使用的东西，还不能使用 `eval` 和 `macroexpand`。本地时间未知（与 3.6 节相同）。
- cons 区域从 1024 个单元开始，不够时会增长。`--heap-cells`、`--max-memory`、`--no-cons`、`-O<N>`、`--release` 可以照常使用。

### 3.9 为 Arduino Uno（AVR）编译

加上 `--target avr-atmega328p`，会生成面向 Arduino Uno 的微控制器（ATmega328P）的可执行文件（ELF）。下面的程序让开发板上的 LED 闪烁。

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
  (unsafe (volatile-write-u8 DDRB LED))           ; 设为输出
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; 翻转
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # 生成 blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # 在 QEMU 中运行
```

- 由于没有运行时，使用运行时的程序会成为**编译错误**。错误会指出需要什么，以及经由哪些函数到达。可以使用的有：固定宽度整数（`i8` 到 `u32`）、`f32`、`f64`、`bool`、`char` 的计算，函数调用（包括递归），条件分支，循环，保存这些值的 `defvar` 和 `defconstant`，`volatile-read-u8` 等，以及 C 函数的调用。
- 不能使用的有：字符串、列表、结构体等放在堆上的值；`catch`、函数值的调用；传递字符串或函数的 C 函数的调用；`sin`、`expt` 等浮点数的数学函数（`sqrt` 和 `floor` 等取整可以使用）。
- 中断处理函数用 `#[interrupt(N)]` 书写（[语法](../reference/syntax.md) 3.17 节）。`N` 是 ATmega328P 的中断编号，以复位为 0 计，从 1 到 25（定时器 1 溢出是 13）。请看下面的例子。
- 除以 0 时，微控制器会当场停止（中断也不再到来）。用 `/` 将有符号的最小值除以 `-1` 也一样。
- `int` 没有固定宽度，因此不能使用（编译错误）。请使用 `i16`、`u16` 等固定宽度的类型。地址也可以用固定宽度的整数计算。写成数字的地址和 `defconstant` 的 `int` 这类在编译时就确定值的东西可以使用。
- 递归使栈不足时，微控制器会停止。
- C 函数用 `defffi` 声明，并用 `--link FILE` 传入为 AVR 编译的目标文件。
- 什么都不做的循环在 `-O1` 及以上会被删除。在等待的循环中，请读取寄存器等。
- `main` 结束后，微控制器停止。
- 内存布局可以用 `--linker-script FILE` 更改。省略时为 32KiB 闪存去掉末尾 512 字节的引导程序后的部分，以及 2KiB RAM。RAM 的地址要加上 `0x800000` 来写，例如 `0x800100`。
- `f32` 和 `f64` 的计算由辅助函数完成。只有用到的辅助函数会放进闪存（`f64` 的更大）。
- 链接与 3.8 节一样，使用 rustup 工具链的链接器。

用定时器中断让 LED 闪烁的例子。中断在外设自己的寄存器中启用。

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; 定时器 1 溢出
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; 翻转
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; 启用溢出中断
          (volatile-write-u8 TCCR1B 3))           ; 以时钟的 1/64 计数
  (while true))
```

- 有处理函数的程序在全局变量初始化完成后接受中断。请在 `main` 的开头设置外设。
- 不能使用 `enable-interrupt` 和 `#[exception(NAME)]`（编译错误）。
- 处理函数执行期间，其他中断等待。处理函数与其他函数一样，不能使用运行时。
- 全局变量的一次读取或写入，即使是两字节以上的值，也不会被处理函数打断。为此，在读取处理函数写入的变量时，以及写入处理函数读取或写入的变量时，会禁止中断，并在编译时以警告指出该变量。先读后写的更新（`(setf n (+ n 1))`）之间会有中断到来。要更新处理函数也写入的变量时，请读取并保存状态寄存器（地址 `#x5F`），写入 0 以停止中断，完成后写回保存的值。
- 没有处理函数的中断到来时，微控制器会停止。

## 4. 转储

可以把当前会话的定义保存到一个文件，下次从它启动。

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

也可以像 `typl --image session.typld prog.typl` 这样用于运行文件。

- 保存的是**定义**。在会话中求值过的表达式不会保存。
- `compile` 过的函数以编译后的形式保存。
- 全局变量恢复为**重新执行初始化表达式得到的值**，而不是保存时的值。
- 与写出转储的 `typl` 版本不同的 `typl` 无法读入（会报错）。

运行文件并在其中 `(dump ...)` 时，该文件的定义位于以文件名命名的模块中。在 `dp.typl` 中定义的函数名为 `dp::sq`，
从其他文件调用需要 `pub`（[模块与文件结构](modules.md)）。

## 5. 关于编译后的模块文件

没有 Common Lisp 的 `.fasl` 那样把每个模块的编译结果写入文件的格式。`compile-file` 直接从源代码生成可执行文件，
不会留下中间文件。
