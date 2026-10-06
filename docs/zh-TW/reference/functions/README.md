<!-- translated-from: docs/ja/reference/functions/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 內建函式

內建函式、方法與標準函式庫的一覽。語法（特殊形式、定義方式）見[語法參考](../syntax.md)，型別一覽見[型別一覽](../types.md)。

## 呼叫形式

呼叫形式有三種。

- 自由函式：`(name args...)`
- 實例方法：`(name receiver args...)`（依第一個引數的靜態型別解析）
- 靜態方法（關聯函式）：`(Type::name args...)`

每個型別都可以有同名的方法。`(+ a b)` 呼叫的是 `a` 的型別的 `+`。

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
| [sequences.md](sequences.md) | 序對 `cons-cell`、S 運算式資料 `Sexpr`、符號、序列函式、高階函式 |
| [collections.md](collections.md) | 字串、字元、`Vector`、`HashTable`、`Array`、`BitVector` |
| [option-result.md](option-result.md) | `Option`、`Result`、錯誤型別與 `Error` trait |
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算術 trait |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、列印控制變數 |
| [format.md](format.md) | 格式指令 |
| [streams-files.md](streams-files.md) | 串流、檔案操作、路徑名稱、readtable |
| [concurrency.md](concurrency.md) | 任務、通道、`WaitGroup`、`Mutex`、`Thread` |
| [network.md](network.md) | TCP、TLS、Unix 網域 socket、UDP |
| [system.md](system.md) | 時間、執行環境、實作工具、`read`/`eval`、文件字串、巨集相關 |
