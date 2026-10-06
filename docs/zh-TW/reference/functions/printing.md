<!-- translated-from: docs/ja/reference/functions/printing.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 列印

`print`/`println`/`format`、單引數列印函式、pretty printer、`print-object`，以及控制列印的變數。格式指令一覽見 [format.md](format.md)。
對串流的讀寫見[串流與檔案](streams-files.md)。

## 1. `print` / `println` / `format`

`print`/`println`/`format` 都是**解讀格式指令（CL 的 `format` 指令）的特殊形式**。第 1 個引數（`format` 是第 2 個）是**控制字串**，各指令
依序消耗其後的可變引數。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 展開控制字串，不換行寫到標準輸出 |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 同上，最後換行 |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL 的 `format`。回傳展開後的字串。`dest` 為 `true`（CL 的 `t`）時還會寫到標準輸出；為 `false`（CL 的 `nil`）時不寫，只回傳字串 |
| `format`（寫到串流） | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` 不是 `bool` 時是 CL 的寫到串流。把展開的字串寫到該串流。回傳值是 `()`（相當於 CL 的 `nil`），不回傳字串 |

依 `dest` 的型別分為兩種意義（屬於哪一種是靜態決定的）。寫到串流時，具體串流型別、`:dyn CharOutput`、`(where (CharOutput S))` 的型別變數
都可以同樣書寫。既不是 `bool` 也不是串流的 `dest` 是型別錯誤。

**控制字串必須是字面值**（與 Rust 的 `format!` 同樣的限制）。其中的指令決定取幾個引數、什麼型別，所以執行期組出的字串在檢查時無法讀取。
限定為字面值後，**引數的個數與型別在檢查時就會被檢查**——`(println "~d" "x")` 與 `(println "~a ~a" 1)` 都是檢查時的錯誤。拼錯的指令、未
閉合的 `~(`、沒有任何引數能回應的 `~/name/` 也同樣是檢查時的錯誤。檢查規則見 [format.md](format.md#1-寫法)。想輸出組出來的字串時，以
`(format false ...)` 建立，再以 `(println "~a" s)` 列印。

可變引數以各自的型別包裝進 `Sexpr` 後傳遞——`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`，以及使用者定義的 `defstruct`/
`defenum`/`Vector<T>`/`HashTable<K,V>` 等都可以直接傳遞（`(println "~a" my-struct)` 可以直接運作）。

以 `typl file.typl` 執行腳本時**不會輸出頂層運算式的值**，所以程式要自己寫到標準輸出就呼叫這些函式。`print`/`println`/`format` 每次呼叫都會
把輸出送出（這樣即使透過管線，也能在讀取標準輸入之前看到提示）。

**`Option<Sexpr>` 透明地列印。** S 運算式資料的型別是 `Option<Sexpr>`，所以 `(some x)` 的包裝不會出現在列印中，內容直接輸出。空串列輸出為
`()`。其他 `Option<T>` 列印為 `(some ...)` / `none`。結構、列舉、`Vector` 中的 `Option<T>` 欄位也一樣。`(eval ...)` 的
`Result<Option<Sexpr>,…>` 輸出為 `(ok 42)`，`none` 時為 `(ok ())`。

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 不輸出，只取得字串
  (println "~a" s))                   ; => id=42
```

## 2. 單引數列印函式

CLHS 22.1.3 的列印函式。不做格式展開，直接列印一個值。串流可以省略（預設為 `*standard-output*`）。

| 名稱 | 形式 | 說明 |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | 以可讀回的形式（與 `~s` 相同）寫出，回傳 `x` |
| `princ` | `(princ x [stream])` | 以給人閱讀的形式（與 `~a` 相同）寫出，回傳 `x` |
| `write` | `(write x [stream])` | `*print-escape*` 為真時為 `prin1`，為假時為 `princ`。回傳 `x` |
| `prin1-to-string` | `(prin1-to-string x)` | 不寫出，以字串回傳（`~s`） |
| `princ-to-string` | `(princ-to-string x)` | 同上（`~a`）。與 `to-string` 相同 |
| `write-to-string` | `(write-to-string x)` | 同上，依 `*print-escape*` |

`print`/`println` **不是**這些函式。它們是接受控制字串的 `format` 的簡寫，與 CL 的 `print`（換行 → `prin1` → 空格）是不同的工作，所以兩者
都保留各自的名稱。結果是 **CL 的單引數 `print` 在這個語言中沒有對應的寫法**——請寫 `prin1`。

它們是巨集，因為 `format` 的可變引數不接受型別變數，型別必須在呼叫處決定。

## 3. 標準輸入與標準串流

**讀取標準輸入**不使用專門的函式，而是使用標準串流 `*standard-input*` 上 `CharInput` 的方法——`(read-line *standard-input*)` /
`(read-char *standard-input*)` / `(read-all *standard-input*)`（[串流的方法](streams-files.md#2-方法)）。標準輸出、標準錯誤同樣有
`*standard-output*` / `*error-output*`，可以像 `(write-line *standard-output* s)` 這樣寫（`print`/`println`/`format` 是需要格式展開時的
捷徑，一律寫到標準輸出）。

## 4. pretty printer

相當於 CL 的 Lisp Pretty Printer（CLHS 22.2）。**依照邏輯區塊與條件換行的指定，折行超出行寬的輸出。**

### 4.1 控制變數

可以指派的全域變數。`setf` 之後對之後的所有列印生效。暫時改變時使用 `dlet`（6.3）。

| 變數 | 型別 | 預設值 | 意義 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 為真時 `~a`/`~s`/`~w` 以及 pretty 指令進入美化路徑 |
| `*print-right-margin*` | `int` | `80` | 右邊界（欄）。0 表示「沒有邊界＝不折行」。負值是列印錯誤 |
| `*print-miser-width*` | `int` | `0` | 進入 miser 樣式的寬度。0 相當於 CL 的 `nil`（miser 無效）。負值是列印錯誤 |

`pprint` 系列與 `pprint-logical-block` 無論 `*print-pretty*` 如何一律美化輸出（依 CL 的 `pprint` 的定義）。

### 4.2 現成的版面（特殊形式）

與 `print` 一樣是特殊形式，所以引數可以是任何型別。

| 名稱 | 形式 | 說明 |
|---|---|---|
| `pprint` | `(pprint x)` | 以預設版面美化輸出。依 CL **先輸出換行**，最後不輸出 |
| `pprint-fill` | `(pprint-fill x)` | 一行盡量填滿。不輸出換行 |
| `pprint-linear` | `(pprint-linear x)` | 所有元素放不進一行時**每行一個元素**。不輸出換行 |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 欄寬的表格形式（預設 16）。不輸出換行。負的 `colinc` 是錯誤 |

預設版面（`pprint` 以及 `*print-pretty*` 下的 `~a`）仿照 CL 預設的 `*print-pprint-dispatch*`，把 `(quote x)` 簡寫為 `'x`，並把
`defun`/`let`/`if`/`lambda` 等程式碼形式排版為「開頭加規定個數的引數在第 1 行，其餘本體縮排 2 欄、每行一個」。其他串列以填滿方式排版。

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 自己建構邏輯區塊

| 名稱 | 形式 | 說明 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 開啟邏輯區塊的特殊形式。`obj` 是 `pprint-pop` 走訪的串列（不走訪時為 `()`）。`:prefix` 與 `:per-line-prefix` 互斥（與 CL 相同） |
| `pprint-newline` | `(pprint-newline kind)` | 條件換行。`kind` 為 `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 縮排。`kind` 為 `:block`（從區塊起點）/ `:current`（從目前欄） |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | 定位。`kind` 為 `:line` / `:section` / `:line-relative` / `:section-relative`。`colnum` 與 `colinc` 非負（為負時是錯誤） |
| `pprint-pop` | `(pprint-pop)` | 從區塊的串列取下一個元素（已用盡時為 `()`） |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | 串列是否已用盡 |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 已用盡時 `break` 出外層的 `loop`（巨集） |

邏輯區塊不接受串流引數——**開啟中的邏輯區塊是隱含的狀態**。最外層的 `pprint-logical-block` 開始它，關閉時一次排版並寫到標準輸出。開啟期間，
`print`/`println`/`(format true ...)`/`pprint` 的輸出也全部進入該區塊，所以**內容以一般的 `print` 書寫，只以 `pprint-newline` 等指定換行
位置**——寫出來與 CL 的程式碼幾乎相同。

`pprint-exit-if-list-exhausted` 在 CL 中是從 `pprint-logical-block` 的非區域跳出，在這裡則是**從外層 `loop` 的 `break`**（`pprint-logical-block`
不建立 `block`）。CL 端的慣用寫法也一律寫在 `loop` 之中，所以寫起來沒有差別。

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

條件換行的判定規則（CLHS `pprint-newline`）：

- `:mandatory` —— 一律折行。
- `:linear` —— 外層邏輯區塊放不進一行時折行。判定以區塊為單位，所以**同一區塊中的 `:linear` 會一起折行**（`pprint-linear` 的「全部一行或
  每行一個元素」就是這個）。
- `:fill` —— 符合以下任一條件時折行：(a) 下一段放不進該行的剩餘部分，(b) 前一段沒有放進一行，(c) 在 miser 樣式下區塊放不進一行。
- `:miser` —— 只在 miser 樣式（區塊的起始欄距右邊界在 `*print-miser-width*` 以內）時作為 `:linear` 作用。

## 5. `print-object`（依型別的列印表示）

寫下 `impl print-object <型別>`，`print`/`println`/`format`/`pprint` 就會以該實作列印該型別的值——**即使它巢狀在串列中**。相當於 CL 的泛型
函式 `print-object`（CLHS 22.1.4）。

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| 引數 | 意義 |
|---|---|
| `self` | 要列印的值 |
| `escape` | CL 的 `*print-escape*`。`~s`/`prin1`/`pprint` 時為 `true`（可讀回的表示），`~a`/`princ` 時為 `false`（給人閱讀）。不在意的實作可以忽略 |

回傳的 `string` 直接流向輸出。沒有寫 `impl` 的型別以內建表示（`#<point x: 1 y: 2>` 的形式）列印。

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   巢狀時也生效
```

它也與 pretty printer 組合（第 4 章）。`*print-pretty*` 為真時，包含實作所回傳字串的串列會在右邊界折行。

標準函式庫型別的列印表示。CL 中也有的型別採用與 SBCL 相同的形式。REPL 顯示結果時也使用與 `~s` 相同的表示。

| 型別 | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#<vector<int> 1 2 3>` | 同左（元素用 `~a`） |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | 同左 |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>`（數字是實作內部的編號） | 同左 |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | 整數（CL 的 `get-universal-time` / `get-internal-real-time` 的值） | 同左 |
| 錯誤型別（`ParseIntError`、`SimpleError` 等） | `#<simpleerror "boom">` | 只有訊息（`boom`） |
| `complex` | `#C(1.0 2.0)` | 同左 |
| `Array<T>` | `#2A((0 0) (0 0))` | 同左 |
| 串流 | `#<file-stream for "file /tmp/a.txt" {7}>`、`#<string-output-stream {5}>`、`#<two-way-stream :input-stream … :output-stream …>` | 同左 |
| socket | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`、`#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | 同左 |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>`（日光節約時間時最後加 `dst`） | 同左 |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | 同左 |
| `defstruct` 的型別 | `#<point x: 1 y: 2>`（欄位名稱與值） | 同左（欄位用 `~a`） |

規則：

- **登記是靜態的。** `impl` 作為一般的方法定義進行型別檢查，所以型別名稱拼錯與簽章不符都是編譯錯誤。
- **對泛型型別也生效。** `(impl print-object box<T> (where (print-object T)) ...)` 會依型別引數跳到不同的本體——值記得包括自身型別引數在內的
  型別（`box<i32>`）。`Vector<T>` 這樣的內建泛型型別也一樣。
- **選擇發生在列印時。** 哪個指令消耗哪個引數取決於控制字串在執行期的內容，所以 `~a` 與 `~s` 的差別（即 `escape`）只有在列印的那一刻才
  知道。與 CLOS 中 `print-object` 方法「依類別定義，列印時選擇」相同。
- **重新進入時退回內建表示。** 實作中以 `(format false "~a" self)` 列印自己會無限遞迴，所以正在列印的值再次出現時退回內建表示。判斷依據
  是值的同一性而不是深度限制，所以不會妨礙正當的自我參照結構的巢狀列印。
- **所有純量型別都實作了這個 trait。** 這是**為了用作約束**：`format` 的可變引數不能接受型別變數，所以泛型程式碼要表達「可以描繪未知型別
  的值」，只有這個約束（與 Rust 的 `T: Display` 形式相同）。`Array<T>` 的 `print-object` 就是一例。
- **型別引數不滿足約束時，默默使用內建表示。** `(impl print-object Array<T> (where (print-object T)))` 對 `Array<i32>` 生效，但對元素是
  沒有寫 `print-object` 的 `defstruct` 的 `Array` 不生效。光是建立陣列就出錯是說不通的，所以不會出錯。
- CL 的另一種機制 `set-pprint-dispatch` / `*print-pprint-dispatch*`（以型別指定子為鍵的執行期登記表）**沒有採用**。它的登記沒有檢查，
  不適合靜態型別語言。

## 6. 控制列印量

### 6.1 深度、長度與共享

CLHS 22.1.1 中決定「值要列印到什麼程度」的控制變數。與 4.1 的 3 個變數一樣是可以指派的全域變數，無論 `*print-pretty*` 真假，都對
`print`/`println`/`format`/`pprint` 全部生效。

| 變數 | 型別 | 預設值 | 意義 |
|---|---|---|---|
| `*print-level*` | `int` | `0` | 巢狀到這個深度以上的物件替換為 `#`。列印對象本身是深度 0。0 表示無限制 |
| `*print-length*` | `int` | `0` | 串列元素（以及 `defstruct`/`defenum` 值的欄位）列印到這個個數，其餘為 `...`。0 表示無限制 |
| `*print-circle*` | `bool` | `false` | 為真時在列印前掃描值，**為出現 2 次以上的物件加上標籤**。第一次出現為 `#n=…`，之後為 `#n#` |

CL 以 `nil` 表示「無限制」，但這個語言沒有 `nil`，所以與 `*print-right-margin*` 等一樣，**0 表示無限制**。負值沒有意義，是列印錯誤。預設值
都是「無限制／無標籤」，與 CL 的初始值一致。

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**只有把 `*print-circle*` 設為真時才能列印循環結構。** 在為假（預設）的狀態下列印指向自己的值，列印器會不斷沿循環前進，行程會當掉——CL 也
一樣（CLHS 規定 `*print-circle*` 為假時列印循環結構的行為未定義）。

循環只能透過「以 `setf` 讓 `defstruct` 的欄位指向自己」來建立（`Sexpr` 單元建立後不能修改，所以 `'(1 2 3)` 這樣的串列不會循環）：

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a 指向 a 自己
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

標籤**依每個列印對象從 1 重新編號**（與 CL 相同）。即使沒有循環，同一個物件出現 2 次也會加上 `#1=`/`#1#`——這是把「這兩個是同一個物件」的
資訊保留在輸出中、符合 CL 規格的行為：

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

沒有任何共享的值**完全不會出現標籤**，所以即使把這個變數保持為真來執行平常的程式碼，輸出也不會改變。

### 6.2 基數、大小寫與可讀回

| 變數 | 型別 | 預設值 | 意義 |
|---|---|---|---|
| `*print-base*` | `int` | `10` | 列印整數（固定寬度與 `int`）的基數。2〜36 以外是**列印錯誤**（CL 也規定了範圍） |
| `*print-radix*` | `bool` | `false` | 為真時加上基數標記。`#b`/`#o`/`#x`，其他為 `#NNr`，基數 10 時最後加 `.`。標記在正負號的**前面**（`#x-ff`） |
| `*print-case*` | `symbol` | `:downcase` | 符號名稱的大小寫。`:upcase` / `:downcase` / `:capitalize`（與 CL 相同的寫法）。其他符號是列印錯誤 |
| `*print-readably*` | `bool` | `false` | 為真時以可讀回的形式列印。強制跳脫，並使 `*print-level*`/`*print-length*` 的截斷失效 |
| `*print-lines*` | `int` | `0` | pretty printer 可以使用的行數。超出部分截去，最後加上與 CL 相同的 `..`。0 表示無限制。負值是列印錯誤 |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string` 做 `prin1` 還是 `princ`。**只有這兩個會讀取它** |
| `*print-array*` | `bool` | `true` | `Array<T>` 是否顯示內容。為真時以 CL 的陣列語法（`#(1 2 3)` / `#2A((1 2) (3 4))`），為假時只顯示形狀 `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*` 加上的標記可以被讀取器讀回（[語法參考](../syntax.md#1-詞法元素)的基數記法）。

**`*print-case*` 的預設值與 CL 不同的原因**：CL 的預設值是 `:upcase`，那是因為 CL 的讀取器以大寫儲存符號名稱——也就是「照儲存的樣子」的意思。
這個讀取器以小寫儲存，所以意義相同的預設值是 `:downcase`。

**`*print-readably*` 缺少的一半**：CL 對無法讀回的值會發出 `print-not-readable`，但這個語言沒有可以發出的條件，也無法對 `print-object` 可能以
任何方式列印的使用者型別判斷能否讀回。只有強制跳脫與覆寫截斷。

**只有 `write` 讀取 `*print-escape*` 的原因**：依 CLHS，`~s`/`prin1`/`pprint` 把它繫結為真，`~a`/`princ` 繫結為假，都只在各自呼叫期間有效。
也就是說，在沒有人繫結的狀態下讀取它的只有 `write`/`write-to-string`。`print-object` 的實作應該讀取自己的 `escape` 引數，而不是這個全域
變數——那個引數攜帶著指令所選擇的值。

**CL 中有而這裡沒有的**：`*print-gensym*`（沒有未 intern 的符號）。

### 6.3 暫時替換

CL 以 `let` 繫結這些變數，但這個語言的 `let` 是詞法繫結，所以使用 `dlet`（[其他](system.md#10-其他)）：

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; 限制只對這一次生效
(with-standard-io-syntax (println "~a" x))   ; 全部恢復為標準值再列印
```

`with-standard-io-syntax` 把所有列印控制變數設為標準值、把 `*read-eval*` 設為 `true`，然後執行本體。
