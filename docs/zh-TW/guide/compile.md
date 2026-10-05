<!-- translated-from: docs/ja/guide/compile.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
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
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

省略 `-o` 時，會在原始檔所在資料夾產生以原始檔名去掉 `.typl` 命名的執行檔。原始檔名不以 `.typl` 結尾時需要 `-o`。使用 `-c`
（`--compile`）時，不能指定 `--image`、`--heap-cells`、`--feature`。

在 REPL 或程式中呼叫 `compile-file` 也能做同樣的事。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
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

- 進入點檔案的頂層只能寫定義（`defun` `defmethod` `defvar` `defconstant` `defstruct` `defenum` `deftype` `deftrait` `impl`
  `defffi`、`(unsafe (def-c-struct ...))`）以及 `use` `module`。除了最後的 `(main)`，不能寫 `(println ...)` 這樣的頂層運算式。
  處理請寫在 `main` 中。
- 進入點檔案中不能寫 `defmacro`。巨集請在其他模組中以 `(pub defmacro ...)` 定義，再 `use` 來使用。
- 含有 `defsignature` 的檔案，無論是進入點檔案還是被 `use` 的模組，都不能進行 AOT 編譯。
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
