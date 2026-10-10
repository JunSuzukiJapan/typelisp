<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# 内置函数

内置函数、方法和标准库的一览。语法（特殊形式、定义方法）见[语法参考](../syntax.md)，类型一览见[类型一览](../types.md)。

## 调用形式

调用形式有三种。

- 自由函数：`(name args...)`
- 实例方法：`(name receiver args...)`（根据第一个参数的静态类型解析）
- 静态方法（关联函数）：`(Type::name args...)`

每个类型都可以有同名的方法。`(+ a b)` 调用的是 `a` 的类型的 `+`。

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
| [traits.md](traits.md) | `Iter`、`Eq`/`Ord`、算术 trait |
| [printing.md](printing.md) | `print`/`println`/`format`、pretty printer、`print-object`、打印控制变量 |
| [format.md](format.md) | 格式指令 |
| [streams-files.md](streams-files.md) | 流、文件操作、路径名、readtable |
| [concurrency.md](concurrency.md) | 任务、通道、`WaitGroup`、`Mutex`、`Thread` |
| [network.md](network.md) | TCP、TLS、Unix 域套接字、UDP |
| [system.md](system.md) | 时间、运行环境、实现工具、`read`/`eval`、文档字符串、宏相关 |
