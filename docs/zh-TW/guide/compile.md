<!-- translated-from: docs/ja/guide/compile.md @ eb0e7d31e1d6e61f64d90fcfb4f0dc5e6bfe813f -->
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
  TLS、名稱解析）、作業系統執行緒（`Thread`）、C 函式（`defffi`）、`get-internal-run-time`, `machine-instance`, `software-version`, `file-author`、
  本地時區，以及 `ed`。
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
- 除 3.6 節中不能使用的之外，檔案和目錄操作也是**編譯錯誤**。標準輸入讀作空，`(command-line-args)` 是空向量。

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
