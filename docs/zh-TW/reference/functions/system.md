<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 時間、執行環境與實作

時間、執行環境的查詢、實作工具、文字的解析與求值、文件字串，以及巨集相關的函式。

## 1. 時間

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `universal-time` | — | `defstruct` | `day`（自 1900-01-01 起的天數）與 `second`（當天內的秒數，0..86399）兩個欄位 |
| `internal-time` | — | `defstruct` | `second` 與 `microsecond`（該秒內，0..999999）兩個欄位 |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | 自 CL 紀元（1900-01-01 UTC）起的時間 |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | 以行程為基準的經過時間 |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | 本行程使用的 **CPU 時間**（使用者＋系統） |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | 換算為秒數。回報兩次讀數之差時的形式 |
| `internal-time-units-per-second` | — | `int` | `1000000`（微秒），即 `microsecond` 欄位的單位。與 CL 一樣，值由實作選擇 |
| `time` | `(time form)` | 巨集 | 執行 `form`，各以一行列印實際時間與 CPU 時間，並原樣回傳 `form` 的值 |

實際時間與 CPU 時間說明的是不同的事。以 I/O 等待為主的處理兩者差異很大，而這個差異正是想知道的資訊，所以 `time` 兩者都輸出。

讓任務停下的 `sleep` 見[任務與通道](concurrency.md#3-yield--sleep--讓出)。

## 2. 日期的分解與合成

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone` 共 **9 個欄位**。把 CL 的 9 個回傳值合為一個結構（因為沒有多值） |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | 把世界時分解為曆法成分。`zone` 是格林威治以西的小時數（與 CL 方向相同）。**省略時為當地時間**（與 CL 相同） |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | 反方向。省略 `zone` 時引數以**當地時間**解讀 |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | 以當地時間分解的現在 |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | 該世界時下當地時間位於格林威治以西的**秒**數 |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | 該世界時是否實施日光節約時間 |

與 CL 一樣，`day-of-week` 中 **0 是星期一，6 是星期日**。

**省略 zone 時為當地時間**——與 CL 相同。當地時間的時差向 OS 詢問，所以結果取決於機器所在的地區。**給出明確的 zone 結果就是確定的**，`0`
是 UTC。

`zone` 的單位與 CL 相同，是「格林威治以西的**小時**數」，UTC+9 讀作 `-9`。不過**引數是整數，結果的 `zone` 欄位是 `f64`**。實際的時差不一定是
整小時（印度是 +5:30，尼泊爾是 +5:45），把回報值捨入就會默默說謊。手寫的 zone 是整小時，所以引數端是 `int`。

明確給出 `zone` 時，依 CL 的規定 `daylight-p` 為 `false`，`zone` 就是傳入的值本身
（*If a time-zone is supplied, daylight saving time information is ignored*）。

處於日光節約時間切換期間的當地時間本來就不唯一，CL 也沒有規定取哪一個。`encode-universal-time` 對這樣的時間也回傳其中一個答案。

## 3. 執行環境

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | 命令列。**第 0 個元素是程式名稱** |
| `getenv` | `(getenv name)` | `string→Option<string>` | 環境變數。未設定或不是 UTF-8 時為 `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`。`user-homedir-pathname`（[路徑名稱](streams-files.md#92-函式)）的基礎 |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | 實作的版本號 |
| `machine-type` | `(machine-type)` | `()→string` | CPU 架構（`x86_64` / `aarch64` …）。是**建置目標**的值 |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | 主機名稱 |
| `machine-version` | `(machine-version)` | `()→Option<string>` | **正在執行**的硬體名稱（`Apple M1` / `Intel(R) Xeon(R) …`）。無法得知的環境中為 `none` |
| `software-type` | `(software-type)` | `()→string` | OS（`macos` / `linux` …） |
| `software-version` | `(software-version)` | `()→Option<string>` | OS 的發行版本（`uname -r`，例如 `24.6.0`） |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | 安裝地點的簡稱。**一律為 `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | 同樣，全稱。**一律為 `none`** |

回傳 `Option` 的是 CL 允許 `NIL` 的項目（*or nil if no such name can be determined*）。POSIX 沒有記錄網站名稱的地方，所以一律為 `none`——SBCL
也回傳同樣的結果。注意 `machine-type` 與 `machine-version` 的差異：前者是這個二進位檔**被建置**時的架構，後者是現在**正在執行**的晶片。

`command-line-args` 的第 0 個元素，對 `typl script.typl a b` 是腳本的路徑，對 AOT 執行檔 `./prog a b` 是執行檔本身。**無論哪種執行方式，都能以
同樣的索引讀到同樣的引數**（`typl` 會去掉自己的名稱與 `--heap-cells` 等選項再傳入）。

## 4. 詢問使用者

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | 以一個字元接受 `y` / `n`。在接受之前反覆詢問 |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | 要求拼出 `yes` / `no`。用於答錯代價高的問題 |

兩者都從 `*standard-input*` 讀取。只有輸入結束才會停止反覆詢問，此時結果為 `false`。

## 5. 實作工具（CLHS 25.2）

實作回答關於自己的問題的一層。`heap-info` / `room` / `dribble` 是一般函式，`trace` / `untrace` / `step` / `disassemble` / `ed` 是**特殊形式**
（`trace` / `untrace` / `disassemble` / `ed` 接受定義的*名稱*，`step` 接受*形式*，都不求值）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | 以結構給出堆積的現況。與 `room` 列印的是同樣的數 |
| `room` | `(room &optional verbose)` | `(bool)→()` | 把 `heap-info` 回報到 `*standard-output*`。`(room true)` 較詳細 |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | 開始把工作階段的輸出記錄到 `path`／不帶引數時結束記錄 |
| `trace` | `(trace name...)` | `Sexpr` | 把所列定義的呼叫回報到 `*trace-output*`。回傳目前正在 trace 的名稱一覽 |
| `untrace` | `(untrace name...)` | `Sexpr` | 停止回報。**不帶引數時全部解除** |
| `step` | `(step form)` | `form` 的型別 | 對 `form` 求值，在每次呼叫時停下詢問 |
| `disassemble` | `(disassemble name [llvm])` | `()` | 列印該定義變成了什麼。預設是主機的機器碼，`true` 時為 LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | 啟動 `$VISUAL`／`$EDITOR`。傳入名稱時開啟寫有該定義的那一行 |

`trace`/`untrace`/`step`/`disassemble` 是直譯器專用的，呼叫它們的函式不能編譯（[語法參考第 10 章](../syntax.md#10-編譯)）。

### 5.1 `heap-info` 的欄位

| 欄位 | 型別 | 內容 |
|---|---|---|
| `capacity` / `live` / `free` | `int` | cons 區域整體與其細目。3 者一律滿足 `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | 堆積所持有的其他 3 種物件的目前數量 |
| `gc-count` | `int` | 自實作啟動以來的回收次數 |
| `growable` | `bool` | 區域是否還能增長 |

除 `growable` 以外的欄位都是 `int`。不回報增長的上限（見 `typl --heap-cells` 的說明）。讀者想知道的是能否增長（`growable`）。

### 5.2 `trace` / `step` 能看到與看不到的東西

- **從直譯執行中的呼叫處，也能看到擁有編譯後本體的定義。**
- **編譯後程式碼*內部*的呼叫處看不到。** trace 擁有編譯後本體的名稱時，會附上一行說明這一點。與 SBCL 對區域呼叫所說的限制相同。
- **經由閉包值的呼叫（`funcall`/`apply`）看不到。** 閉包沒有名稱。
- **泛型定義不在對象之內。** 每種型別的實體在每個使用處產生，沒有可以指名的單一本體（與 `compile` 拒絕的理由、措辭相同）。

`step` 的命令是 `s`（進入這次呼叫；空行也一樣）、`n`（跳過這次呼叫）、`c`（之後不再詢問）、`q`（中止）。**標準輸入不是終端機時，`step` 只是
對 `form` 求值**——這是 CLHS 明確允許的退化，以免腳本或測試在無法回答的提示處卡住。

`ed` 的 `$VISUAL`／`$EDITOR` 依空白分割，所以也可以寫 `EDITOR="code -w"`。兩者都未設定時為 `Err`——不會猜測 `vi`。行號以 `+N` 的形式放在最前面傳入。

`dribble` 記錄工作階段輸出離開行程的全部 3 條途徑：`print`/`println`/`format` 寫出的內容、寫到連接標準輸出的串流的內容，以及在 REPL 中輸入的
行與 REPL 列印回來的值。

## 6. 解析與求值

這些都處理執行期的（程式本身無法控制的）文字與資料，所以失敗時不 panic，而是回傳 `Result` 的 `Err`。錯誤型別是每種操作各自的具體型別
（[錯誤型別](option-result.md#3-錯誤型別與-error-trait)）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL 的 `parse-integer`。略過前後空白（與 `trim` 相同的集合），讀取至多一個正負號 `+`/`-`，再讀取 `radix` 進位（預設 10，2〜36。10 以上的數字大小寫皆可）的數字。位數沒有上限（`int`）。剩下其他字元時為 `Err`。`:junk-allowed true` 時在第一個非數字處停止並忽略其餘——但一個數字都沒有時為 `Err`（相當於 CL 的 `nil`）。不回傳 CL 的第 2 個值（讀取結束位置）。超出範圍的 `radix` 會 panic（是呼叫端的錯誤，不是文字的） |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 浮點數。也接受 `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | 從 `s` 讀取一個 `Sexpr`（與讀取原始碼的是同一個讀取器）。括號不完整、字串未結束等為 `Err`。從串流讀取用 `read-sexpr`（[串流](streams-files.md#6-泛型函式與檔案操作)） |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | 在 `read` 的基礎上附上**讀取結束的位置**。`(car r)` 是值，`(cdr r)` 是下一個要讀的字元位置。`start` 省略時為 0 |
| `read-from-string-preserving-whitespace` | 同上 | 同上 | 同上，但不消耗結束 datum 的空白。差別呈現在回傳的位置上 |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 在執行期對 `form` 進行型別檢查並求值。依 CL 的 `eval` |

CL 從 `read-from-string` 回傳**2 個值**（值與位置），但這個語言沒有多值，所以回傳一個 `cons-cell`。有了位置，逐個 datum 讀取字串就成為迴圈
而不是重新掃描：

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace` 的差別只是**一個空白字元**——CL 的 `read` 消耗結束 datum 的空白，`read-preserving-whitespace` 保留它。
`(read-from-string "12 34")` 回傳位置 3，preserving 版本回傳 2。

讀取器接受的數值記法見[語法參考第 1 章](../syntax.md#1-詞法元素)。`*print-radix*`（[列印](printing.md#62-基數大小寫與可讀回)）列印的
記法可以原樣讀回。沒有 CL 的 `*read-base*`。

### 6.1 `eval` 的意義

依 CLHS 的 `eval`：在**目前的全域環境**（全域的函式、變數、型別、巨集，也包括執行期加入的定義）以及**空的詞法環境**（看不到呼叫端 `let`/
`lambda` 的區域繫結）中求值。運算式與定義（`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`）都可以求值，定義會立即且永久地登記到全域環境。

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; 能看到全域的 x
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 回傳定義的名稱
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 能看到剛才的定義
```

- **回傳值**：運算式時以 `Option<Sexpr>` 回傳求值結果，定義時回傳所定義名稱的符號（與 CL 相同）。要使用結果，以 `match`
  （`(int n)`/`(str s)`/…）拆解 `Sexpr`。
- **由靜態型別帶來的差異（重要）**：CL 回傳結果的實際值，但這個語言只能把回傳型別統一為 `Result<Option<Sexpr>,EvalError>`。另外，
  **靜態撰寫的程式碼不能前置參照 `eval` 在執行期定義的名稱**——直接寫在檔案中的 `(sq 9)` 會在定義 `sq` 的 `eval` 執行之前被檢查，成為
  「未定義」。不過**之後的 `eval` 能看到它**（那個 `eval` 的型別檢查在執行期、定義之後進行）。REPL 逐行檢查與執行，所以以 `eval` 定義的
  名稱可以從下一行直接呼叫。
- **錯誤的處理**：型別錯誤、語法錯誤回傳 `Err`（不會 panic）。求值的程式碼中的**執行期 panic**（除以零等）與直接撰寫的程式碼一樣原樣傳播。
  途中的 `unwind-protect` 的 cleanup 會執行（[語法參考第 8 章](../syntax.md#8-非區域跳出catch--throw--unwind-protect)）。
- **命名空間**：在 `typl file.typl` 的執行中以及 AOT 執行檔中，`eval` 在該腳本模組的命名空間中求值（能看到腳本本身的全域變數）。REPL 在根
  命名空間中求值。
- **編譯**：`read` 與 `eval` 都可以編譯。在 AOT 執行檔中的處理與其後果（eval 的形式被直譯執行）見
  [語法參考 10.2](../syntax.md#102-aot-執行檔中的-eval)。

## 7. 文件字串 / `documentation`

`defun`/`defmethod`（包括 `impl` 內的）/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/`deftype`/`deftrait` 可以擁有文件字串。
位置依 CL 各自的規則：

| 形式 | 文件字串的位置 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 本體的開頭（回傳值型別、`where` 子句之後）。僅當其後至少還有一個本體形式時——單獨的字串仍是回傳值 |
| `defvar` / `defconstant` | 初始值的**後面**：`(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 名稱的**緊後面**，欄位／變體清單之前 |
| `deftype` | 名稱的**緊後面**，型別之前：`(deftype meters "doc" i32)` |
| `deftrait` | 繼承清單之後、項目清單之前。整個 trait 一個。**有預設實作的方法**可以在其本體之前放置自己的文件字串 |

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `documentation` | `(documentation name)` | （特殊形式。`name` 是裸符號或 `Type::method`）→`Option<string>` | 回傳 `name` 的文件字串 |

`documentation` 與 `quote`/`compile` 一樣是特殊形式（不對 `name` 求值，以未求值的名稱讀取）。與 CL 的 `(documentation 'name 'function)` 不同，
它不接受型別引數，而是依**變數→函式→型別→trait→巨集**的順序（與作為運算式求值時裸識別字的優先順序相同）解析裸名稱。`Type::method` 的形式
查找關聯方法／靜態方法的文件字串。

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**值在檢查時決定**：名稱無法解析到任何定義時是檢查時的錯誤（與參照未定義的變數等相同）。能解析但沒有文件字串時為 `Option::none`。

**不在對象之內**：

- 沒有 `(setf documentation)`（在執行期改寫文件字串）。
- 不支援帶模組限定的自由名稱（`mod::name`。`Type::method` 支援）。
- `deftrait` 中**沒有本體**的方法宣告不能擁有文件字串。結尾的字串字面值本身會成為預設實作的本體（＝回傳值），無法區分兩者。

語言伺服器（`typl-lsp`）的懸停中也會顯示文件字串。

## 8. 巨集

巨集的定義方式見[語法參考 3.14](../syntax.md#314-defmacro--巨集定義)。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | 新的符號。名稱是 `" <prefix><n>"`，`n` 是 `*gensym-counter*`。開頭的空格在原始碼中寫不出來，所以產生的繫結不會與寫出的名稱衝突 |
| `*gensym-counter*` | 變數 | `int` | `gensym` 下次使用的編號。與 CL 一樣可以讀取與設定 |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 把巨集呼叫展開一層。`none` 表示「不是巨集呼叫」 |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 反覆展開，直到不再是巨集 |

`macroexpand-1` 回傳的是 `Option`——CL 以第 2 個回傳值傳達「是否展開了」，但沒有多值，所以由 `none` 擔任這一點。**展開為自己呼叫的巨集與
非巨集不可能被混淆。** 展開的一層與型別檢查使用的是同一個，程式看到的與檢查看到的不會產生落差。

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none 以空串列輸出（Option<Sexpr> 是透明的）
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL 中有而這裡沒有的：`eval-when`（`:compile-toplevel`/`:load-toplevel`/`:execute` 一律一致，沒有可選擇的區別）、`define-compiler-macro`、
`load-time-value`、`make-symbol`/`copy-symbol`/`gentemp`（未 intern 的符號。繫結依名稱查找，所以沒有什麼好處）。

## 9. 區域巨集繫結（`macrolet` / `symbol-macrolet`）

兩者都是對**不是值的名稱**進行詞法繫結的特殊形式。執行期什麼也不留下——被編譯的是本體展開後的形式。

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- `macrolet` 的繫結**只在本體期間**遮蔽同名的全域巨集。lambda 清單與 `defmacro` 相同（`&optional`/`&rest`/`&key`）。
- **同一個 `macrolet` 的兄弟之間，從彼此的*本體*中看不到對方**（與 CL 相同。這是與 `labels` 的差異）。展開結果在使用處檢查，所以 `earlier`
  展開為 `(later ...)` 是可以的——在那個位置兩者都可見。
- `symbol-macrolet` 的名稱作為一般繫結進入環境。所以內層的 `let` 會遮蔽同名，外層的變數會被遮蔽——CL 的規則照樣呈現。
- **`setf` 寫到展開目標。** `(setf head 42)` 就是 `(setf (get v 0) 42)`。
- 展開在**使用處的環境**中檢查（而不是繫結處）。

## 10. 其他

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | 為假時 panic。省略訊息時為 `assertion failed: <測試的原文>`（因為是巨集，所以能指出運算式本身）。沒有 CL 的 restart |
| `warn` | `(warn control args...)` | `(string,...)→()` | 對 `*error-output*` 寫一行帶 `WARNING: ` 的內容並**繼續**。既不回傳 `Result` 也不結束程式的回報手段 |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | 只在 `body` 期間替換全域變數，離開時恢復。CL 把它寫成 `let`，但這個語言的 `let` 一律是詞法繫結，所以用別的名稱（與 Emacs Lisp 同名巨集的角色相同）。無論以正常結束、`throw`、panic、`break`/`return` 中哪種方式離開都會恢復。**不是依任務的繫結** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | 把所有列印控制變數設為標準值、把 `*read-eval*` 設為 `true`，然後執行 `body`（[列印](printing.md#6-控制列印量)） |
| `exit` | `(exit code)` | `int→!` | 結束行程 |
| `dump` | `(dump path)` | `string→bool` | 把目前的環境（型別資訊＋編譯後的本體）寫到一個檔案。可以用 `typl --image <path>` 重新啟動。直譯器專用（[語法參考 10.1](../syntax.md#101-傾印)） |
