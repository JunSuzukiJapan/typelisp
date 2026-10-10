<!-- translated-from: docs/ja/reference/types.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# 型別一覽

typelisp 中的型別，以及各型別實作的標準 trait 一覽。型別的寫法見[語法參考第 2 章](syntax.md#2-型別的寫法)，各型別的函式與方法見
[內建函式](functions/README.md)。

## 1. 基本型別

| 型別 | 內容 | 詳情 |
|---|---|---|
| `int` | 任意精度整數。63 位元放得下時是立即值，超過後自動變為多倍精度。未註記的整數字面值的預設型別 | [數值第 3 章](functions/numbers.md#3-任意精度整數-int) |
| `i8` `i16` `i32` | 有號固定寬度整數 | [數值第 1 章](functions/numbers.md#1-固定寬度整數) |
| `u8` `u16` `u32` | 無號固定寬度整數 | 同上 |
| `f32` `f64` | IEEE-754 浮點數。小數字面值預設為 `f64` | [數值第 4 章](functions/numbers.md#4-浮點數f64--f32) |
| `ratio` | 既約有理數 | [數值第 5 章](functions/numbers.md#5-有理數-ratio) |
| `bool` | `true` / `false` | [數值第 7 章](functions/numbers.md#7-布林值) |
| `char` | Unicode 純量值 | [字元](functions/collections.md#2-字元-char) |
| `string` | 不可變的字串 | [字串](functions/collections.md#1-字串-string) |
| `symbol` | 符號。關鍵字（`:name`）也是這個型別 | [符號](functions/sequences.md#3-符號) |
| `()` | Unit 型別。值也是 `()` | |
| `!` | Never 型別。`panic` 等不回傳的運算式的型別。可以放在任何型別的位置 | |
| `ptr` `c-long` `c-ulong` | 專用於與 C 交換值的字。只有在 `unsafe` 中才能成為值，可以出現的位置也有限 | [數值第 2 章](functions/numbers.md#2-c-邊界上的原始字ptr--c-long--c-ulong) |
| `random-state` | 亂數產生器的狀態 | [數值第 12 章](functions/numbers.md#12-亂數) |

沒有 64 位元整數型別。不在意寬度的整數請使用 `int`。

## 2. 內建泛型型別

| 型別 | 內容 | 詳情 |
|---|---|---|
| `Option<T>` | 有值或沒有值。`some` / `none` | [Option 與 Result](functions/option-result.md) |
| `Result<T,E>` | 成功或失敗。`ok` / `err` | 同上 |
| `Vector<T>` | 可變長度陣列 | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | 雜湊表。鍵的型別須實作 `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | 元組（元素 1～12 個）。元素用 `t::0` 讀取 | [語法第 2 章](syntax.md#2-型別的寫法) |
| `Task<T>` | 任務的控制代碼 | [任務](functions/concurrency.md#1-taskt--任務控制代碼) |
| `Thread<T>` | 在專用 OS 執行緒上執行的任務的控制代碼 | [Thread](functions/concurrency.md#7-threadt--專用-os-執行緒) |
| `Chan<T>` | 通道 | [通道](functions/concurrency.md#2-chant--通道) |

函式型別寫成 `(fn (引數型別...) 回傳值型別)`，trait 物件寫成 `:dyn Trait`（[語法參考第 2 章](syntax.md#2-型別的寫法)）。

## 3. S 運算式資料

| 型別 | 內容 | 詳情 |
|---|---|---|
| `Sexpr` | 非空的 S 運算式。`int`、`i8` 至 `u32`、`f32`、`f64`、`char`、`bool`、`sym`、`str`、`cons`、`ratio`、`path`、`vector`、`array`、`tuple` 共 19 種變體 | [S 運算式資料](functions/sequences.md#2-s-運算式資料-sexpr) |
| `Option<Sexpr>` | 一般的 S 運算式資料。空串列 `()` 是 `none` | 同上 |

## 4. 標準函式庫中的型別

標準函式庫（prelude）以 `defstruct` / `defenum` 定義的型別。與自己撰寫的型別同等對待，`defstruct` 能做的事全都可以做。

| 型別 | 內容 | 詳情 |
|---|---|---|
| `cons-cell<A,B>` | 序對。`cons`/`car`/`cdr` | [序對](functions/sequences.md#1-序對-cons-cellab) |
| `complex` | 複數（分量為 `f64`） | [數值第 6 章](functions/numbers.md#6-複數-complex) |
| `Array<T>` | 多維陣列 | [Array](functions/collections.md#5-arrayt多維陣列) |
| `BitVector` | 固定長度的位元序列 | [BitVector](functions/collections.md#6-bitvector位元向量) |
| `HashSet<T>` | 沒有重複元素的集合 | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | 依鍵的順序排列的表 | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | 可以從兩端放入和取出的序列 | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | 各集合的 `iter` 回傳的迭代器 | [Iter](functions/traits.md#1-iter-trait-與迭代) |
| `lazy::map-iter<I,A,U>` 等 | `lazy` 模組的函式傳回的迭代器 | [惰性迭代器](functions/sequences.md#惰性迭代器lazy-模組) |
| `WaitGroup` | 等待 N 個完成 | [WaitGroup](functions/concurrency.md#4-waitgroup--等待-n-個完成) |
| `Mutex<T>` | 共享資料的互斥 | [Mutex](functions/concurrency.md#6-mutext--共享資料的互斥) |
| `Context` | 協作式取消 | [Context](functions/concurrency.md#8-context--協作式取消) |
| `pathname` | 拆解後的檔名 | [路徑名稱](functions/streams-files.md#9-路徑名稱-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | 串流 | [串流](functions/streams-files.md#3-具體串流型別) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | 組合串流 | [組合串流](functions/streams-files.md#4-組合串流) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | 網路 | [網路](functions/network.md#1-型別) |
| `ReadOutcome` | `read-sexpr` 的結果。`datum` / `eof` | [串流](functions/streams-files.md#6-泛型函式與檔案操作) |
| `universal-time` `internal-time` `decoded-time` | 時間 | [時間](functions/system.md#1-時間) |
| `heap-info` | 堆積的現況 | [實作工具](functions/system.md#51-heap-info-的欄位) |

## 5. 錯誤型別

`Error` 不是型別而是 trait，以下型別實作了它。不區分種類處理時寫 `:dyn Error`。

| 型別 | 來源 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 檔案、串流操作 |
| `NetError` | 網路操作 |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

詳情見[錯誤型別與 Error trait](functions/option-result.md#3-錯誤型別與-error-trait)。

## 6. 標準 trait 的實作

哪些型別實作了哪些 trait。各 trait 的方法見[標準 trait](functions/traits.md)以及表格最右欄所列的章節。

### 6.1 比較、雜湊與列印

| trait | 實作的型別 | 詳情 |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord比較) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | 同上 |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `Context` `pathname` `universal-time` `internal-time` 以及所有內建錯誤型別 | [print-object](functions/printing.md#5-print-object依型別的列印表示) |

`cons-cell<A,B>` 與元組 `#{..}` 的 trait，以及集合的 `print-object`，在元素型別實作了該 trait 時可用。

### 6.2 算術

| trait | 實作的型別 |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

詳情見[算術 trait](functions/traits.md#3-算術-traitadd--sub--mul--div--rem--bits--number)。

### 6.3 迭代

| trait | 實作的型別 |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` `lazy` 模組的型別（`lazy::map-iter<I,A,U>` 等） |

### 6.4 串流

| 型別 | 實作的 trait |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

所有串流都實作 `Stream`，輸入端還實作 `InputStream`，輸出端還實作 `OutputStream`。`socket-listener` 與 `udp-socket` 只實作 `Stream`
（`close` / `open-stream-p`）。詳情見[串流](functions/streams-files.md#1-trait-階層)。

### 6.5 其他

| trait | 實作的型別 | 詳情 |
|---|---|---|
| `Error` | 第 5 章的所有錯誤型別 | [錯誤型別](functions/option-result.md#3-錯誤型別與-error-trait) |
| `Pathish` | `string` `pathname` | [路徑名稱](functions/streams-files.md#91-路徑名稱指定子-trait-pathish) |
