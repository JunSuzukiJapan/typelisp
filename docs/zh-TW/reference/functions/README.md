<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# 內建函式

內建函式、方法與標準函式庫的一覽。語法（特殊形式、定義方式）見[語法參考](../syntax.md)，型別一覽見[型別一覽](../types.md)。

## 呼叫形式

呼叫形式有三種。

- 自由函式：`(name args...)`
- 實例方法：`(name receiver args...)`（依第一個引數的靜態型別解析）
- 靜態方法（關聯函式）：`(Type::name args...)`

每個型別都可以有同名的方法。`(+ a b)` 呼叫的是 `a` 的型別的 `+`。

## 需要 `use` 的名稱

標準函式庫的名稱中，什麼都不寫就能在任何模組中使用的是以下這些（[語法參考 3.10](../syntax.md#310-module--use--命名空間)）。

- 基本型別、`Option`・`Result`・`Vector`、迭代、S 運算式與串列、序列函式、控制巨集、比較、算術 trait
- 與 Common Lisp 的 `COMMON-LISP` 套件中的名稱同名者（`length`・`format`・`open` 等）
- `Error`・`assert`・`print-object`、`HashTable`・`Array`・`BitVector`、串流的 trait
  `Stream`・`InputStream`・`OutputStream`，以及 `HashTable` 的鍵所實作的 `Hash`
- 這些函式與變數的型別中出現的型別

其他名稱要像 `(use std::net::tcp-connect)` 這樣用 `use` 引入，或寫成路徑 `std::net::tcp-connect`。
下表列出這些名稱及其所在位置。

| 模組 | 名稱 |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`、`digit-weight`、`name->char` |
| `std::collections` | `Deque`、`HashSet`、`Scope`、`SortedTable`、`hashtable-iter` |
| `std::env` | `command-line-args`、`getenv`、`home-directory` |
| `std::error` | `WrappedError`、`as-dyn-error`、`assert-eq`、`describe-error`、`wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`、`direction-input`、`direction-output`、`directory-p`、`open-binary`、`open-binary-input`、`open-binary-output`、`open-file`、`open-input`、`open-output`、`read-file-lines`、`read-file-string`、`write-file-string` |
| `std::hash` | `*sxhash-mask*`、`sxhash-string` |
| `std::io` | `ByteInput`、`ByteOutput`、`PeekInput`、`binary-file-stream`、`copy-stream`、`make-peek-stream`、`peek-stream`、`read-lines`、`write-lines` |
| `std::net` | `NetError`、`byte-stream-of`、`char-stream-of`、`datagram`、`socket-byte-stream`、`socket-listener`、`socket-stream`、`tcp-connect`、`tcp-listen`、`tls-connect`、`tls-listen`、`udp-bind`、`udp-socket`、`unix-connect`、`unix-listen`、`with-connection` |
| `std::num` | `ParseFloatError`、`ParseIntError`、`atan2`、`parse-float`、`parse-int` |
| `std::path` | `Pathish`、`pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`、`read-from-string-preserving-whitespace`、`read-sexpr`、`read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`、`right-trim`、`string->utf8`、`trim`、`utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`、`Context`、`Mutex`、`WaitGroup`、`after`、`with-lock` |
| `std::thread` | `Task`、`Thread`、`yield` |
| `std::time` | `internal-time-seconds`、`timezone-daylight-p`、`timezone-offset-seconds` |

## 表格的讀法

各章的表格有「名稱、形式、型別、說明」幾欄。型別欄寫成 `(引數型別,...)→回傳值型別` 的形式。

- `T`、`A`、`B` 等單一大寫字母是型別變數。
- `where Eq A` 這樣的註記是該型別變數必須滿足的 trait 約束。
- `Iter<A>` 表示「`Item` 為 `A` 的任意 `Iter` 實作型別」。
- 帶有 `&optional` / `&key` 的引數可以省略。

## 各章

| 檔案 | 內容 |
|---|---|
| [numbers.md](numbers.md) | 整數、浮點數、有理數、複數、布林值、位元運算、亂數 |
| [sequences.md](sequences.md) | 序對 `cons-cell`、S 運算式資料 `Sexpr`、符號、序列函式、惰性迭代器 `lazy`、高階函式 |
| [collections.md](collections.md) | 字串、字元、`Vector`、`HashTable`、`Array`、`BitVector`、`HashSet`、`SortedTable`、`Deque` |
| [option-result.md](option-result.md) | `Option`、`Result`、錯誤型別與 `Error` trait |
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算術 trait |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、列印控制變數 |
| [format.md](format.md) | 格式指令 |
| [streams-files.md](streams-files.md) | 串流、檔案操作、路徑名稱、readtable |
| [concurrency.md](concurrency.md) | 任務、通道、`WaitGroup`、`Mutex`、`Thread`、`Context` |
| [network.md](network.md) | TCP、TLS、Unix 網域 socket、UDP |
| [system.md](system.md) | 時間、執行環境、實作工具、`read`/`eval`、文件字串、巨集相關 |
