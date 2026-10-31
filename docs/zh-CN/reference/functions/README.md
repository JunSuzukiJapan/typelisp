<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# 内置函数

内置函数、方法和标准库的一览。语法（特殊形式、定义方法）见[语法参考](../syntax.md)，类型一览见[类型一览](../types.md)。

## 调用形式

调用形式有三种。

- 自由函数：`(name args...)`
- 实例方法：`(name receiver args...)`（根据第一个参数的静态类型解析）
- 静态方法（关联函数）：`(Type::name args...)`

每个类型都可以有同名的方法。`(+ a b)` 调用的是 `a` 的类型的 `+`。

## 需要 `use` 的名字

标准库的名字中，什么都不写就能在任何模块中使用的是以下这些（[语法参考 3.10](../syntax.md#310-module--use--命名空间)）。

- 基本类型、`Option`・`Result`・`Vector`、迭代、S 表达式与列表、序列函数、控制宏、比较、算术 trait
- 与 Common Lisp 的 `COMMON-LISP` 包中的名字同名的东西（`length`・`format`・`open` 等）
- `Error`・`assert`・`print-object`、`HashTable`・`Array`・`BitVector`、流的 trait
  `Stream`・`InputStream`・`OutputStream`，以及 `HashTable` 的键所实现的 `Hash`
- 这些函数和变量的类型中出现的类型

其他名字要像 `(use std::net::tcp-connect)` 这样用 `use` 引入，或写成路径 `std::net::tcp-connect`。
下表列出这些名字及其所在位置。

| 模块 | 名字 |
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

## 表格的读法

各章的表格有"名称、形式、类型、说明"几列。类型列写成 `(参数类型,...)→返回值类型` 的形式。

- `T`、`A`、`B` 等单个大写字母是类型变量。
- `where Eq A` 这样的注记是该类型变量必须满足的 trait 约束。
- `Iter<A>` 表示"`Item` 为 `A` 的任意 `Iter` 实现类型"。
- 带有 `&optional` / `&key` 的参数可以省略。

## 各章

| 文件 | 内容 |
|---|---|
| [numbers.md](numbers.md) | 整数、浮点数、有理数、复数、布尔值、位运算、随机数 |
| [sequences.md](sequences.md) | 序对 `cons-cell`、S 表达式数据 `Sexpr`、符号、序列函数、惰性迭代器 `lazy`、高阶函数 |
| [collections.md](collections.md) | 字符串、字符、`Vector`、`HashTable`、`Array`、`BitVector`、`HashSet`、`SortedTable`、`Deque` |
| [option-result.md](option-result.md) | `Option`、`Result`、错误类型与 `Error` trait |
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算术 trait、`Push`、`Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、打印控制变量 |
| [format.md](format.md) | 格式指令 |
| [streams-files.md](streams-files.md) | 流、文件操作、路径名、readtable |
| [concurrency.md](concurrency.md) | 任务、通道、`WaitGroup`、`Mutex`、`Thread`、`Context` |
| [network.md](network.md) | TCP、TLS、Unix 域套接字、UDP |
| [system.md](system.md) | 时间、运行环境、实现工具、`read`/`eval`、文档字符串、宏相关 |
