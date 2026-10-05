<!-- translated-from: docs/ja/reference/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 类型一览

typelisp 中的类型，以及各类型实现的标准 trait 一览。类型的写法见[语法参考第 2 章](syntax.md#2-类型的写法)，各类型的
函数和方法见[内置函数](functions/README.md)。

## 1. 基本类型

| 类型 | 内容 | 详情 |
|---|---|---|
| `int` | 任意精度整数。63 位放得下时是立即值，超过后自动变为多精度。未注解的整数字面量的默认类型 | [数值第 3 章](functions/numbers.md#3-任意精度整数-int) |
| `i8` `i16` `i32` | 有符号固定宽度整数 | [数值第 1 章](functions/numbers.md#1-固定宽度整数) |
| `u8` `u16` `u32` | 无符号固定宽度整数 | 同上 |
| `f32` `f64` | IEEE-754 浮点数。小数字面量默认为 `f64` | [数值第 4 章](functions/numbers.md#4-浮点数f64--f32) |
| `ratio` | 既约有理数 | [数值第 5 章](functions/numbers.md#5-有理数-ratio) |
| `bool` | `true` / `false` | [数值第 7 章](functions/numbers.md#7-布尔值) |
| `char` | Unicode 标量值 | [字符](functions/collections.md#2-字符-char) |
| `string` | 不可变字符串 | [字符串](functions/collections.md#1-字符串-string) |
| `symbol` | 符号。关键字（`:name`）也是这个类型 | [符号](functions/sequences.md#3-符号) |
| `()` | Unit 类型。值也是 `()` | |
| `!` | Never 类型。`panic` 等不返回的表达式的类型。可以放在任何类型的位置 | |
| `ptr` `c-long` `c-ulong` | 专用于与 C 交换值的字。只有在 `unsafe` 中才能成为值，可以出现的位置也有限 | [数值第 2 章](functions/numbers.md#2-c-边界上的原始字ptr--c-long--c-ulong) |
| `random-state` | 随机数生成器的状态 | [数值第 12 章](functions/numbers.md#12-随机数) |

没有 64 位整数类型。不在意宽度的整数请使用 `int`。

## 2. 内置泛型类型

| 类型 | 内容 | 详情 |
|---|---|---|
| `Option<T>` | 有值或没有值。`some` / `none` | [Option 与 Result](functions/option-result.md) |
| `Result<T,E>` | 成功或失败。`ok` / `err` | 同上 |
| `Vector<T>` | 可变长数组 | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | 哈希表。键的类型须实现 `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | 任务的句柄 | [任务](functions/concurrency.md#1-taskt--任务句柄) |
| `Thread<T>` | 在专用 OS 线程上运行的任务的句柄 | [Thread](functions/concurrency.md#7-threadt--专用-os-线程) |
| `Chan<T>` | 通道 | [通道](functions/concurrency.md#2-chant--通道) |

函数类型写成 `(fn (参数类型...) 返回值类型)`，trait 对象写成 `:dyn Trait`（[语法参考第 2 章](syntax.md#2-类型的写法)）。

## 3. S 表达式数据

| 类型 | 内容 | 详情 |
|---|---|---|
| `Sexpr` | 非空的 S 表达式。`int`、`i8` 至 `u32`、`f32`、`f64`、`char`、`bool`、`sym`、`str`、`cons`、`ratio`、`path` 共 16 种变体 | [S 表达式数据](functions/sequences.md#2-s-表达式数据-sexpr) |
| `Option<Sexpr>` | 一般的 S 表达式数据。空列表 `()` 是 `none` | 同上 |

## 4. 标准库中的类型

标准库（prelude）用 `defstruct` / `defenum` 定义的类型。与自己编写的类型同等对待，`defstruct` 能做的事全都可以做。

| 类型 | 内容 | 详情 |
|---|---|---|
| `cons-cell<A,B>` | 序对。`cons`/`car`/`cdr` | [序对](functions/sequences.md#1-序对-cons-cellab) |
| `complex` | 复数（分量为 `f64`） | [数值第 6 章](functions/numbers.md#6-复数-complex) |
| `Array<T>` | 多维数组 | [Array](functions/collections.md#5-arrayt多维数组) |
| `BitVector` | 固定长度的位序列 | [BitVector](functions/collections.md#6-bitvector位向量) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | 各集合的 `iter` 返回的迭代器 | [Iter](functions/traits.md#1-iter-trait-与迭代) |
| `WaitGroup` | 等待 N 个完成 | [WaitGroup](functions/concurrency.md#4-waitgroup--等待-n-个完成) |
| `Mutex<T>` | 共享数据的互斥 | [Mutex](functions/concurrency.md#6-mutext--共享数据的互斥) |
| `pathname` | 拆分后的文件名 | [路径名](functions/streams-files.md#9-路径名-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | 流 | [流](functions/streams-files.md#3-具体流类型) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | 组合流 | [组合流](functions/streams-files.md#4-组合流) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | 网络 | [网络](functions/network.md#1-类型) |
| `ReadOutcome` | `read-sexpr` 的结果。`datum` / `eof` | [流](functions/streams-files.md#6-泛型函数与文件操作) |
| `universal-time` `internal-time` `decoded-time` | 时间 | [时间](functions/system.md#1-时间) |
| `heap-info` | 堆的现状 | [实现工具](functions/system.md#51-heap-info-的字段) |

## 5. 错误类型

`Error` 不是类型而是 trait，以下类型实现了它。不区分种类处理时写 `:dyn Error`。

| 类型 | 来源 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 文件、流操作 |
| `NetError` | 网络操作 |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

详情见[错误类型与 Error trait](functions/option-result.md#3-错误类型与-error-trait)。

## 6. 标准 trait 的实现

哪些类型实现了哪些 trait。各 trait 的方法见[标准 trait](functions/traits.md)以及表格最右列所列的章节。

### 6.1 比较、哈希与打印

| trait | 实现的类型 | 详情 |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord比较) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | 同上 |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` 以及所有内置错误类型 | [print-object](functions/printing.md#5-print-object按类型的打印表示) |

`cons-cell<A,B>` 的 `Eq`/`Ord` 在元素类型实现了 `Eq`/`Ord` 时可用。

### 6.2 算术

| trait | 实现的类型 |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

详情见[算术 trait](functions/traits.md#3-算术-traitadd--sub--mul--div--rem--bits--number)。

### 6.3 迭代

| trait | 实现的类型 |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 流

| 类型 | 实现的 trait |
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

所有流都实现 `Stream`，输入侧还实现 `InputStream`，输出侧还实现 `OutputStream`。`socket-listener` 和 `udp-socket`
只实现 `Stream`（`close` / `open-stream-p`）。详情见[流](functions/streams-files.md#1-trait-层次)。

### 6.5 其他

| trait | 实现的类型 | 详情 |
|---|---|---|
| `Error` | 第 5 章的所有错误类型 | [错误类型](functions/option-result.md#3-错误类型与-error-trait) |
| `Pathish` | `string` `pathname` | [路径名](functions/streams-files.md#91-路径名指定符-trait-pathish) |
