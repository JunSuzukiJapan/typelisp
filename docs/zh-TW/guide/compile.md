<!-- translated-from: docs/ja/guide/compile.md @ 794bc1c3cb98e547458f7b07a6a476cc10cae3ad -->
# 編譯

不做任何處理的話，typelisp 程式由直譯器執行。此外還有兩種編譯為原生碼的方法，以及一種保存環境的方法。規格的詳情見
[語法參考第 10 章](../reference/syntax.md#10-編譯)。

| 方法 | 用法 | 結果 |
|---|---|---|
| JIT 編譯 | `(compile name)` | 執行中工作階段的函式被換成原生碼 |
| AOT 編譯 | `typl -c src.typl` 或 `(compile-file "src.typl" "out")` | 得到可獨立執行的執行檔 |
| 傾印 | `(dump "file.typld")` | 保存定義，可以用 `typl --image` 從相同的環境啟動 |

## 1. 準備

編譯使用 LLVM 22。如果已經依 [README.md](../../../README.md) 的步驟建置了 `typl`，就不需要額外準備。

AOT 編譯產生的執行檔會連結靜態函式庫 `libtypelisp_front.a`。發行版建置的 `typl`（包括用 `cargo install` 安裝的）內部帶有這個函式庫，
所以不需要準備。第一次編譯時，它會寫出到 `~/.typelisp/lib/<建置ID>/`（如果設定了環境變數 `TYPELISP_HOME`，則為
`$TYPELISP_HOME/lib/<建置ID>/`），之後使用寫出的那一份。可以用 `typl --remove-lib` 刪除（加 `--others` 刪除其他版本的 `typl`
寫出的，加 `--all` 刪除全部）。除錯版建置的 `typl` 使用建置它的儲存庫中 `target/debug/` 底下的函式庫。要使用放在其他位置的函式庫，
在啟動 `typl` 時以 `--lib-dir` 指定其所在資料夾（3.2 節）。在 macOS 上連結需要 Xcode Command Line Tools。

## 2. JIT 編譯

把已經定義的函式當場編譯為原生碼。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; 之後的呼叫執行編譯後的程式碼
```

- `name` 不會被求值。直接寫函式名稱（不是字串）。方法要像 `(compile point::norm)` 這樣加上型別名稱。
- 它呼叫的函式也會一起編譯。
- **泛型函式不能編譯。** 因為每種型別的實體是在每個使用處產生的。請編譯以具體型別呼叫它的那個函式。
- `trace`、`step`、`disassemble`、`compile`、`compile-file`、`dump` 是直譯器的操作，呼叫它們的函式不能編譯。嘗試編譯時會得到說明
  原因的錯誤。

查看編譯結果使用 `disassemble`。

```lisp
(disassemble fib)          ; 主機的機器碼
(disassemble fib true)     ; LLVM IR
```

## 3. 用 AOT 編譯產生執行檔

### 3.1 撰寫程式

作為進入點，定義一個**不接受引數的 `main` 函式**。

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

檔案最後的 `(main)` 是為了以 `typl hello.typl` 執行時呼叫 `main`。`compile-file` 會略過這個最後的 `(main)`，所以同一個檔案既可以用
直譯器執行，也可以用於 AOT。

### 3.2 編譯

在命令列中使用 `typl -c`（`typl --compile` 相同）。

```sh
$ typl -c hello.typl            # 產生 hello
$ typl -c hello.typl -o fib     # 把執行檔命名為 fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

省略 `-o` 時，會在原始檔所在資料夾產生以原始檔名去掉 `.typl` 命名的執行檔。原始檔名不以 `.typl` 結尾時需要 `-o`。使用 `-c`
（`--compile`）時，不能指定 `--image`、`--feature`。

給 `typl` 加上 `--release` 就是**發行建置**，會連同引數一起移除 `eprint`/`eprintln`（用於調查的標準錯誤輸出。[列印](../reference/functions/printing.md)）。被移除的部分也會進行型別檢查，所以其中有型別錯誤時編譯會失敗。`--release` 是整個 `typl` 的選項，不僅對 `-c`，對執行檔案、REPL 和 `typl test` 也同樣有效。在加了 `--release` 的 `typl` 中呼叫的 `compile-file` 也會是發行建置，在這樣產生的執行檔中用 `eval` 執行的程式碼裡，`eprint`/`eprintln` 也會被移除。

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # 寫在 -c 之後也一樣
```

用 `-O0` `-O1` `-O2` `-O3` 指定**最佳化等級**。預設是 `-O0`，不做最佳化。從 `-O1` 起，編譯器先做一些簡單的最佳化，例如把只繫結了字面值的變數替換為它的值、預先算好常數之間的整數運算，其餘交給 LLVM 同名等級的最佳化。無論哪個等級，計算結果都不變。與 `--release` 一樣，這是整個 `typl` 的選項：除了 `-c` 產生的執行檔，也作用於執行檔案、REPL 和 `typl test` 中編譯的程式碼，以及在那裡呼叫的 `compile-file`。直譯器執行的程式碼不做最佳化。

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # 寫在 -c 之後也一樣
```

在 REPL 或程式中呼叫 `compile-file` 也能做同樣的事。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

需要反覆建置時，可以把這一行寫進檔案，用 `typl build.typl` 執行。

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

檔名從**啟動 `typl` 時的目前目錄**解析，而不是從 `build.typl` 所在位置。

要連結放在 `typl` 所使用位置以外的 `libtypelisp_front.a` 時，以 `--lib-dir` 指定其所在資料夾。對 `typl -c` 與 `compile-file` 都有效。

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

指定的資料夾中沒有 `libtypelisp_front.a` 時，會因錯誤而停止。這個檔案只能與同時建置的 `typl` 一起使用。重新建置 `typl` 後，請重新複製。

### 3.3 可以進行 AOT 編譯的檔案形式

- 進入點檔案的頂層只能寫定義（`defun` `defvar` `defparameter` `defconstant` `defmacro`
  `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`、`(unsafe (def-c-struct ...))`）
  以及 `use` `module`。除了最後的 `(main)`，不能寫 `(println ...)` 這樣的頂層運算式。處理請寫在 `main` 中。
- 沒有不接受引數的 `main` 時會出錯。
- 被 `use` 的模組檔案也會一起編譯，合併成一個執行檔。
- `defffi` 的 `:library` 中指定的函式庫會自動連結（[C FFI](ffi.md)）。
- 標準函式庫的所有函式都可以在 AOT 中使用。`eval` 也可以使用，但這時型別檢查器與直譯器會進入執行檔，使檔案變大，啟動也較花時間。
  不呼叫 `eval` 的程式不會包含它們。

### 3.4 執行檔的行為

- 無論以 `typl hello.typl a b` 執行還是以 `./hello a b` 執行，`(command-line-args)` 都回傳相同形式的 `Vector<string>`。第一個元素是
  程式名稱。
- 結束碼以 `(exit n)` 指定。`main` 正常返回時為 0。
- 發生 panic 時會顯示訊息，並以非 0 結束。

### 3.5 執行檔使用的資源

`typl -c` 有一些選項，用來指定執行檔執行時可以使用的資源。

```sh
$ typl --heap-cells 100000 -c hello.typl    # 把 cons 區域固定為 100000 個單元
$ typl -c hello.typl --max-memory 64M       # 最多配置 64 MiB 記憶體
$ typl -c hello.typl --no-cons              # 不使用 cons 單元
```

- `--heap-cells N` 把 cons 單元區域的容量固定為 `N` 個單元。區域不會再變大。垃圾回收後仍無法騰出空單元時，以執行期錯誤停止。不指定時從 65536 個單元開始，視需要變大。
- `--max-memory SIZE` 是執行檔配置的記憶體總量的上限。cons 區域、字串、向量與雜湊表等值、執行環境本身使用的記憶體全部計算在內。`SIZE` 是位元組數，可以加上 `K` `M` `G`（分別為 1024 倍、1024² 倍、1024³ 倍）。試圖超過上限配置時，會顯示這件事並以結束碼 1 結束。這種結束無法用 `catch` 捕捉。不指定時沒有上限。
- `--no-cons` 產生不使用 cons 單元的執行檔。它沒有 cons 區域，因此進行需要 cons 單元之操作的程式會成為**編譯錯誤**。錯誤中會顯示該操作，以及從 `main` 到達那裡的函式序列。需要 cons 單元的情況是：建構 `Sexpr` 串列（包括引用的串列與用準引用建構的串列）或 `&rest` 引數時，以及使用 `read`、`eval`、`macroexpand` 時。數值、字串、向量、雜湊表、結構、通道等的操作，以及 `print`/`println`/`format` 都不使用 cons 單元。不能與 `--heap-cells` 一起指定，寫在 `-c` 之後。

`--max-memory` 寫在 `-c` 之後。`--heap-cells` 寫在 `-c` 之前或之後都可以，這時它是執行檔的 cons 區域的容量。cons 區域也計入記憶體上限，所以指定了放不下區域的上限時，程式會在開始之前停止。

### 3.6 編譯為 WebAssembly

加上 `--target wasm32-wasip1` 後，`typl -c` 產生的不是執行檔，而是 WebAssembly 模組（WASI preview 1 命令）。省略 `-o` 時，名稱是把原始檔的 `.typl` 換成 `.wasm`。

```sh
$ typl -c hello.typl --target wasm32-wasip1      # 產生 hello.wasm
$ wasmtime hello.wasm a b                        # 用支援 WASI 的執行環境執行
args: #(hello.wasm a b)
fib(25) = 75025
```

- 用 `install.sh` 安裝的 `typl` 可以直接使用。用 `cargo install` 安裝時需要準備（同時也是 3.7 節的準備）。首先用 rustup 執行 `rustup target add wasm32-wasip1 wasm32-unknown-unknown`。然後加上 `--features wasm` 安裝（`cargo install --locked --path . --features wasm`）。
  不加此項安裝的 `typl` 沒有 WebAssembly 執行環境，`--target` 會出錯。使用儲存庫中的除錯版 `typl` 時，改為在儲存庫中執行 `scripts/build-wasm-runtime.sh`。
- 連結使用 rustup 工具鏈的連結器（`rust-lld`），因此編譯時也需要 rustup。
- 模組的行為與執行檔相同：標準輸出、標準錯誤和結束碼、`(command-line-args)`、
  檔案讀寫、`throw`/`catch`/`panic`、任務、通道和 `sleep`，以及 `eval` 都可以使用。
  任務在一個執行緒上依序執行。檔案只能在執行環境向模組公開的目錄中讀寫
  （wasmtime 用 `--dir` 指定）。
- 使用 WebAssembly 所沒有的東西的程式是**編譯錯誤**。錯誤會顯示用到的東西，
  以及從 `main` 到達那裡的函式鏈。不能使用的有：通訊端（TCP、UDP、Unix 網域、
  TLS、名稱解析）、作業系統執行緒（`Thread`）、C 函式（`defffi`）、`get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  以及 `ed`。
- WASI 沒有得知時區的手段，因此本地時間未知。省略 `zone` 的 `decode-universal-time`、`encode-universal-time` 以及 `get-decoded-time` 會在執行時 panic。傳入 `zone` 即可使用。
- `--heap-cells`、`--max-memory`、`--no-cons`、`-O<N>`、`--release` 照常可用。

### 3.7 在瀏覽器或 Node.js 中執行

加上 `--target wasm32-unknown-unknown` 後，`typl -c` 產生由 JavaScript 載入並執行的 WebAssembly 模組。在模組（`hello.wasm`）旁邊的同一資料夾中，還會產生載入並執行它的 JavaScript（`hello.mjs`）。準備工作與 3.6 節相同。

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # 產生 hello.wasm 和 hello.mjs
$ node hello.mjs                                       # 用 Node.js 執行
args: #()
fib(25) = 75025
```

在其他 JavaScript 中，呼叫 `hello.mjs` 的 `run`。`write` 以字串形式接收輸出（`fd` 標準輸出為 1，標準錯誤為 2）。`run` 的值是結束碼。

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* 例如顯示在頁面上 */ } });
```

- 不傳 `write` 時，Node.js 輸出到標準輸出和標準錯誤，瀏覽器則逐行輸出到主控台。
- 在瀏覽器中，請透過 HTTP 從與 `hello.mjs` 相同的位置提供 `hello.wasm`（無法從 `file://` 載入）。
- `sleep` 用 `Atomics.wait` 等待。瀏覽器只允許在跨來源隔離頁面的 Worker 中使用它，因此使用 `sleep` 的程式請在 Worker 中執行。
- 本地時間（省略 `zone` 的 `decode-universal-time` 等）依 JavaScript 的 `Date` 回報的時區求得。
- 除了 3.6 節中不能使用的東西，檔案和目錄操作、環境變數、`(command-line-args)`、家目錄和 `dribble` 也是**編譯錯誤**。標準輸入讀作空。

### 3.8 為微控制器（Cortex-M、RISC-V）編譯

加上 `--target thumbv7em-none-eabihf`，`typl -c` 會為沒有作業系統的 Cortex-M4F、M7 開發板產生可執行檔（ELF）。省略 `-o` 時，名稱是把原始檔的 `.typl` 換成 `.elf`。加上 `--target thumbv8m.main-none-eabihf` 則是針對 Cortex-M33 開發板的。加上 `--target riscv32imac-unknown-none-elf` 則是給 32 位元 RISC-V（M、A、C 擴充）的開發板。加上 `--target thumbv6m-none-eabi` 則是給 Cortex-M0+（M0）的開發板。QEMU 沒有 Cortex-M0+ 的開發板，所以給它的可執行檔在能原樣執行相同指令的 Cortex-M3 `mps2-an385` 上執行（省略 `--linker-script` 時的配置也是這塊開發板的）。

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # 產生 hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M4 上執行
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M33 上執行
$ typl -c hello.typl --target thumbv6m-none-eabi
$ qemu-system-arm -M mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 Cortex-M3 上執行
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # 在 QEMU 的 RISC-V 上執行
```

- 用 `install.sh` 安裝的 `typl` 可以直接使用。用 `cargo install` 安裝時，先執行 `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf thumbv6m-none-eabi riscv32imac-unknown-none-elf`，再加上 `--features embedded` 安裝（與 wasm 兩者都要則用 `--features wasm,embedded`）。使用儲存庫的除錯建置 `typl` 時，執行 `scripts/build-embedded-runtime.sh`。連結使用 rustup 工具鏈的連結器。
- 用 `--linker-script FILE` 給出開發板的記憶體配置。寫法與 `cortex-m-rt` 的 `memory.x` 相同，寫 FLASH 和 RAM 的位址與大小。寫 `_typelisp_stack_size = 32K;` 這樣的一行，就是 RAM 中堆疊佔用的大小（不寫則為 64KiB）。RAM 的其餘部分是堆積。省略時使用 QEMU 的 `mps2-an386`（Cortex-M33 為 `mps2-an505`） 的配置。

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  在 RISC-V 上，省略時使用 QEMU 的 `virt` 的配置。撰寫時與 `riscv-rt` 的 `memory.x` 相同，還要用 `REGION_ALIAS` 寫出各類區段所放的區域。

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

- 在 RAM 較小的開發板上，可以用 `--ram SIZE`（如 `16K`）代替 `--linker-script`，只指定資料、堆積和堆疊所用 RAM 的大小。預設配置的 RAM 變為這個大小，堆疊最多佔其四分之一。在像 RISC-V 的 `virt` 這樣程式碼也放在 RAM 中的開發板上，開發板 RAM 的其餘部分成為程式碼的區域。執行環境最初配置的量會配合堆積的大小而變小。不能與 `--linker-script` 一起使用。

- 由誰提供輸出、輸入、結束和時鐘（開發板的函式），用 `--board-functions` 選擇。
  - `semihosting`（預設）：透過 semihosting 提供。在 QEMU（`-semihosting-config enable=on`）或除錯器下執行時使用。
  - `c`：由開發板以 C 函式提供。用 `--link FILE` 傳入定義了下列函式的目的檔或靜態函式庫（`--link` 可以寫多次；`defffi` 呼叫的廠商函式庫也同樣傳入）。

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 是標準輸出，2 是標準錯誤
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0；讀到的位元組數，結束時為 0
  void     typelisp_host_exit(int32_t code);                              // 不返回
  uint64_t typelisp_host_now_us(void);                                    // 不會倒退的時鐘的微秒數
  int64_t  typelisp_host_unix_us(void);                                   // 自 1970-01-01 起的微秒數
  void     typelisp_host_sleep_us(uint64_t us);                           // 等待 us 微秒
  ```

- 板子的暫存器（對映到記憶體的周邊裝置）用 `volatile-read-u8`、`volatile-read-u16`、`volatile-read-u32` 讀取，用 `volatile-write-u8`、`volatile-write-u16`、`volatile-write-u32` 寫入。位址以 `int` 或 `u16` 等固定寬度的整數傳入，讀到的值是該寬度的型別（`u8` 等）。每個都在寫下它的位置恰好讀寫一次，編譯器既不省略也不移動它。它們都只能在 `unsafe` 中使用。位址為負數或不是寬度的倍數時，在讀寫之前就是執行期錯誤。

  ```lisp
  ;; QEMU 的 mps2-an386 的 UART 0：資料 0x40004000，狀態 0x40004004，控制 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; 傳送中則等待
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; 啟用傳送
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" 與換行
  ```

- 中斷到來時被呼叫的函式，寫成帶有 `#[interrupt(N)]`（周邊中斷 `N`）或 `#[exception(SysTick)]` 這類屬性的 `defun`。中斷用 `(enable-interrupt N)` 啟用。處理程式當場執行，只有在執行時期處理途中到來時，才會在該處理結束後執行。處理程式中不能等待（`sleep` 是編譯錯誤）。詳見 [語法參考](../reference/syntax.md) 的 3.17 節。在 Cortex-M0+ 上，`N` 為 0～31，`#[exception(NAME)]` 只能寫 `NonMaskableInt`、`SVCall`、`PendSV`、`SysTick`。在 RISC-V 上，`N` 是 `mcause` 中的中斷編號（0～31。3 是軟體中斷，7 是計時器，11 是外部中斷），不能使用 `#[exception(NAME)]`。

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; 自己引發中斷 5（NVIC 的擱置暫存器）
  ```

- 啟動（向量表和 RAM 的準備）和記憶體配置由執行環境完成。發生 HardFault（在 RISC-V 上為例外） 或未處理的中斷時，以與 panic 相同的結束碼（1）結束。
- `throw`/`catch`/`panic`、bignum、ratio、浮點數，以及任務、通道和 `sleep` 的行為與可執行檔相同。任務在唯一的核心上依序執行。也可以使用 C 函式（`defffi`）。
- 使用開發板沒有的東西的程式是**編譯錯誤**。除了 3.7 節中不能使用的東西，還不能使用 `eval` 和 `macroexpand`。本地時間未知（與 3.6 節相同）。
- cons 區域從 1024 個單元開始，不夠時會成長。`--heap-cells`、`--max-memory`、`--no-cons`、`-O<N>`、`--release` 可以照常使用。

### 3.9 為 Arduino Uno（AVR）編譯

加上 `--target avr-atmega328p`，會產生供 Arduino Uno 的微控制器（ATmega328P）使用的執行檔（ELF）。下面的程式讓開發板上的 LED 閃爍。

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
  (unsafe (volatile-write-u8 DDRB LED))           ; 設為輸出
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; 反轉
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # 產生 blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # 在 QEMU 中執行
```

- 由於沒有執行階段，使用執行階段的程式會成為**編譯錯誤**。錯誤會指出需要什麼，以及經由哪些函式到達。可以使用的有：固定寬度整數（`i8` 到 `u32`）、`f32`、`f64`、`bool`、`char` 的計算，函式呼叫（包括遞迴），條件分支，迴圈，保存這些值的 `defvar` 和 `defconstant`，`volatile-read-u8` 等，以及 C 函式的呼叫。
- 不能使用的有：字串、串列、結構等放在堆積上的值；`catch`、函式值的呼叫；傳遞字串或函式的 C 函式的呼叫；`sin`、`expt` 等浮點數的數學函式（`sqrt` 和 `floor` 等取整可以使用）。
- 中斷處理函式用 `#[interrupt(N)]` 撰寫（[語法](../reference/syntax.md) 3.17 節）。`N` 是 ATmega328P 的中斷編號，以重設為 0 計，從 1 到 25（計時器 1 溢位是 13）。請看下面的例子。
- 除以 0 時，微控制器會當場停止（中斷也不再到來）。用 `/` 將有號的最小值除以 `-1` 也一樣。
- `int` 沒有固定寬度，因此不能使用（編譯錯誤）。請使用 `i16`、`u16` 等固定寬度的型別。位址也可以用固定寬度的整數計算。寫成數字的位址和 `defconstant` 的 `int` 這類在編譯時就確定值的東西可以使用。
- 遞迴使堆疊不足時，微控制器會停止。
- C 函式用 `defffi` 宣告，並用 `--link FILE` 傳入為 AVR 編譯的目的檔。
- 什麼都不做的迴圈在 `-O1` 以上會被移除。在等待的迴圈中，請讀取暫存器等。
- `main` 結束後，微控制器停止。
- 記憶體配置可以用 `--linker-script FILE` 更改。省略時為 32KiB 快閃記憶體去掉末尾 512 位元組的開機載入程式後的部分，以及 2KiB RAM。RAM 的位址要加上 `0x800000` 來寫，例如 `0x800100`。
- `f32` 和 `f64` 的計算由輔助函式完成。只有用到的輔助函式會放進快閃記憶體（`f64` 的更大）。
- 連結與 3.8 節一樣，使用 rustup 工具鏈的連結器。

用計時器中斷讓 LED 閃爍的例子。中斷在周邊裝置自己的暫存器中啟用。

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; 計時器 1 溢位
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; 反轉
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; 啟用溢位中斷
          (volatile-write-u8 TCCR1B 3))           ; 以時脈的 1/64 計數
  (while true))
```

- 有處理函式的程式在全域變數初始化完成後接受中斷。請在 `main` 的開頭設定周邊裝置。
- 不能使用 `enable-interrupt` 和 `#[exception(NAME)]`（編譯錯誤）。
- 處理函式執行期間，其他中斷等待。處理函式與其他函式一樣，不能使用執行階段。
- 全域變數的一次讀取或寫入，即使是兩個位元組以上的值，也不會被處理函式打斷。為此，在讀取處理函式寫入的變數時，以及寫入處理函式讀取或寫入的變數時，會禁止中斷，並在編譯時以警告指出該變數。先讀後寫的更新（`(setf n (+ n 1))`）之間會有中斷到來。要更新處理函式也寫入的變數時，請讀取並保存狀態暫存器（位址 `#x5F`），寫入 0 以停止中斷，完成後寫回保存的值。
- 沒有處理函式的中斷到來時，微控制器會停止。

## 4. 傾印

可以把目前工作階段的定義保存到一個檔案，下次從它啟動。

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

也可以像 `typl --image session.typld prog.typl` 這樣用於執行檔案。

- 保存的是**定義**。在工作階段中求值過的運算式不會保存。
- `compile` 過的函式以編譯後的形式保存。
- 全域變數會以**重新執行初始化運算式得到的值**還原，而不是保存時的值。
- 與寫出傾印的 `typl` 版本不同的 `typl` 無法讀入（會出錯）。

執行檔案並在其中 `(dump ...)` 時，該檔案的定義位於以檔名命名的模組中。在 `dp.typl` 中定義的函式名稱為 `dp::sq`，從其他檔案呼叫需要 `pub`
（[模組與檔案結構](modules.md)）。

## 5. 關於編譯後的模組檔案

沒有像 Common Lisp 的 `.fasl` 那樣把每個模組的編譯結果寫入檔案的格式。`compile-file` 直接從原始碼產生執行檔，不會留下中間檔案。
