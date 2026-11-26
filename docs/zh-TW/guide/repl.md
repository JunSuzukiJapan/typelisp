<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

不帶引數啟動 `typl` 就會進入 REPL（互動模式）。本文說明 REPL 除了對運算式求值之外還能做什麼：
REPL 的命令、輸入輔助、參照先前的結果、中斷計算，以及出錯後的調查。

## 啟動與結束

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

用 `:quit`（簡寫 `:q`）或 `Ctrl+D` 結束。輸入的歷史記錄保存在 `~/.typl_history`，下次啟動時也能用
`Ctrl+P` / `Ctrl+N` 或 `Ctrl+R`（搜尋歷史記錄）叫出來。

啟動後，在第一個提示字元之前依序執行以下內容。它們都不顯示執行得到的值。

1. 如果有 `~/.typlrc`，執行其內容。加上 `--no-init` 則不讀取。
2. 命令列的 `--eval FORM` 和 `--load FILE`，依書寫順序執行。兩者都可以寫任意多次。

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`、`--load` 和 `--no-init` 只用於啟動 REPL。執行檔案時加上它們會出錯。

啟動時的模組是 `user`。用 `(in-module foo)` 移到其他模組後，提示字元會顯示該模組。

```
typl> (in-module foo)
typl user::foo>
```

## REPL 的命令

寫在行首的 `:名稱` 是 REPL 的命令。`:help` 顯示命令列表。列表中沒有的 `:名稱` 不是命令，而是當作
一般的關鍵字求值。

| 命令 | 意義 |
|---|---|
| `:help`（`:h`、`:?`） | 顯示命令列表 |
| `:type FORM`（`:t`） | 不求值，只顯示 FORM 的型別 |
| `:describe NAME`（`:d`） | 顯示定義的種類、型別、欄位或變體、方法、實作的 trait、定義的位置和文件字串 |
| `:apropos TEXT` | 在所有模組中尋找名稱包含 TEXT 的定義（不區分大小寫） |
| `:src NAME` | 顯示定義的原始碼 |
| `:who-calls NAME` | 顯示呼叫函式或方法 NAME 的函式和方法 |
| `:inspect FORM`（`:i`） | 對 FORM 求值，並依型別展開其值 |
| `:backtrace`（`:bt`） | 由內而外依序顯示上一行失敗時正在進行的呼叫 |
| `:locals [N]` | 顯示 `:backtrace` 第 N 個（省略時為第 0 個）呼叫的引數 |
| `:cd [DIR]` | 把目前的資料夾移到 DIR（省略時為家目錄） |
| `:pwd` | 顯示目前的資料夾 |
| `:quit`（`:exit`、`:q`） | 結束 REPL |

NAME 除了 `inner` 這樣的名稱，也可以寫 `std::seq::reverse` 或 `Option::unwrap` 這樣的路徑。

```
typl> (defun inner ((x int) (s string)) int "把 x 加 1。" (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

把 x 加 1。
typl> :src inner
(defun inner ((x int) (s string)) int "把 x 加 1。" (+ x 1))
```

對於從檔案載入的定義，`:src` 從該檔案顯示；對於在 REPL 中輸入的定義，則從輸入的文字顯示。標準
函式庫的定義沒有記錄原始碼的位置，因此無法顯示。

`:who-calls` 尋找的是目前工作階段中函式的本體。對於泛型函式，只有以具體型別使用過的部分才是對象。

`:cd` 會改變 `(load ...)` 尋找的資料夾，但 `use` 尋找檔案的資料夾仍是啟動時的資料夾。

## 展開值（`:inspect`）

`:inspect` 依型別展開值。結構體展開為欄位，列舉（包括 `Option`）展開為其變體的欄位，`Vector`
展開為元素，`HashTable` 展開為鍵與值。輸入編號就進入該部分，`u` 回到上一層，`q` 結束。

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

串列和 `Sexpr` 不展開，依原樣顯示。

## 輸入輔助

在終端機中輸入時，可以使用以下功能。

- 用 `Tab` 補全。補全的對象是名稱（定義，以及 `if`、`defun` 這樣的語法）、包含 `::` 的路徑、行首
  的 REPL 命令，在字串中則是檔名。
- 在函式名稱後輸入空白，會在行尾以淡色顯示該函式的型別。
- 強調與游標旁的括號成對的括號。
- 形式尚未結束就換行時，下一行會依未閉合括號的數量縮排。

## 先前的結果（`:*`、`:**`、`:***`）

在 REPL 中，`:*` 是上一次顯示的值，`:**` 是再前一個，`:***` 是更前一個。各自具有該值的型別。

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

- 沒有顯示值的輸入（例如定義）和出錯的輸入，不會推進結果。
- 結果還沒有那麼多時寫 `:**` 或 `:***` 會出錯。
- 加上引號的 `':*` 是一般的關鍵字。
- 寫在函式本體中的 `:*` 會一直指向定義時的結果。Common Lisp 的 `*` 讀取的是呼叫函式時的值，而
  typelisp 為了決定型別，在定義時就決定下來。
- 在 REPL 之外（執行檔案，或在 `~/.typlrc`、`--load`、`--eval` 中），`:*` 是一般的關鍵字。

## 中斷計算（`Ctrl+C`）

在求值途中按 `Ctrl+C`，會中斷該計算並回到提示字元。之前定義的內容會保留。無論是在直譯器中執行的
迴圈、編譯後程式碼中的迴圈，或是在 `sleep` 或 `wait` 中等待，都能中斷。進行中的 `unwind-protect`
的善後處理會被執行。

```
typl> (loop)
^Cerror: interrupted
typl>
```

在輸入途中按下的 `Ctrl+C` 只會丟棄正在輸入的那一行。

## 出錯後的調查（`:backtrace`、`:locals`）

因錯誤停下後，用 `:backtrace`（`:bt`）由內而外依序顯示當時正在進行的函式呼叫。`:locals N`
依宣告的型別顯示第 N 個呼叫的引數。

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

- 對於在函式最後一個運算式中進行的呼叫（尾呼叫），不顯示呼叫端的函式。正因如此，尾遞迴的迴圈
  無論繞幾圈，都只佔用一次呼叫的空間。
- 不顯示編譯後函式內部的呼叫，也不顯示匿名函式（`lambda`）。
- `:locals` 只顯示引數，不顯示用 `let` 繫結的變數。
- 對下一行求值後，上一個錯誤的記錄就會消失。
