<!-- translated-from: docs/ja/reference/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 错误消息

`typl` 给出的主要错误消息的含义和修正方法。

## 1. 如何阅读

错误以如下形式输出到标准错误：

```text
error: 文件:行:列: 种类: 消息
```

从 `种类` 可以知道错误是在什么时候发现的。

| 种类 | 时机 | 含义 |
|---|---|---|
| `type error` | 运行之前（检查时） | 类型或名称的错误。该形式不会被执行 |
| （无种类） | 读取时、检查时 | 括号不匹配之类的语法错误，或找不到名称的错误 |
| `panic` | 运行中 | 不可恢复的失败。执行 `unwind-protect` 的 cleanup 后停止 |

以 `warning:` 开头的行是警告，处理会继续。

`文件:行:列` 指向有错误的表达式的位置。在标准库函数内部发生的运行时错误，指向程序调用该函数的位置。也有没有位置的错误
（如 `error: panic: ...`）。

例：

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

意思是 `main.typl` 第 1 行第 24 列的表达式，在期望 `i32` 的位置是 `string`。

## 2. 检查时的错误

运行之前发现的错误。修正之前该形式不会被执行。

### 2.1 类型

| 消息 | 含义与修正方法 |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | 需要 `T` 类型的位置是 `U` 类型的表达式。没有隐式类型转换，数值请用 `(as T x)` 转换。`int` 和 `i32` 也是不同的类型 |
| ``integer literal 300 is out of range for u8 (0..=255)`` | 字面量放不进该类型。想要截断时写 `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | 没有该名称的类型。类型要在首次使用它的形式之前定义（类型没有前向声明）。如果本意是类型变量，请写在函数名的 `<foo>` 等声明位置（[语法参考 3.6](syntax.md#36-defstruct--结构体用户定义类型)） |
| ``cannot infer type argument `t` for `vector::new` `` | 无法确定类型参数。像 `(the Vector<int> (Vector::new))` 这样用 `the` 写出类型 |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` 没有处理所有变体。添加缺少的变体分支或 `_` 分支 |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | 函数要求某个 trait，但传入的类型没有实现。写 `(impl Eq pt ...)`（[标准 trait](functions/traits.md)） |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | 在 `:dyn` 的位置传入了没有实现该 trait 的类型的值。写 `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | 在类型的位置写了 trait 名。写成 `:dyn Error` |
| ``if: (if cond then else)`` | `if` 的形式不对。`if` 必须有 else。不需要 else 时使用 `when` |

### 2.2 名称

| 消息 | 含义与修正方法 |
|---|---|
| `no such function: bar` | 没有该名称的函数或方法。检查拼写 |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | 方法由第一个参数的类型选择。存在该名称的方法，但第一个参数的类型（这里是 `int`）没有。消息末尾是拥有该方法的类型一览 |
| `unbound variable: y` | 没有该名称的变量。检查拼写以及绑定的范围（是否在 `let` 外使用） |
| ``use: unresolved `nosuch` `` | 找不到 `use` 的模块。文件名与模块路径的对应见[语法参考 3.11](syntax.md#311-文件与模块的对应多文件项目) |
| `unresolved path: c::hidden` | 模块存在，但没有该名称，或者因为没有加 `pub` 而不可见 |
| `circular module dependency: a -> b -> a` | 模块之间互相 `use`。把共同的部分分到另一个模块 |
| ``return-from: no enclosing block named `nope` `` | 没有与 `return-from` 的名字相符的 `block` 包围它。函数名的 block 只能在该函数中使用 |

### 2.3 调用

| 消息 | 含义与修正方法 |
|---|---|
| `f: expected 1 argument(s), got 2` | 参数个数不对 |
| `f: unknown keyword argument :b` | 传入了该函数没有的关键字参数 |
| `new: expected 1 field(s), got 2` | 传给结构体构造函数的值的个数与字段数不符 |
| ``setf: cannot assign to constant `k` `` | 给用 `defconstant` 定义的名称赋值。需要修改的话改用 `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | 没有定义用 `defsignature` 声明的函数 |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | 没有任何参数的类型拥有用 `~/name/` 调用的方法（[格式指令第 5 章](functions/format.md#5-name)） |

## 3. 读取错误

| 消息 | 含义与修正方法 |
|---|---|
| `unexpected end of input while reading a list` | 缺少右括号。位置指向读取结束的地方（文件末尾等），所以请寻找对应的左括号 |

## 4. 运行时错误（panic）

| 消息 | 含义与修正方法 |
|---|---|
| `panic: divide by zero` | 整数或有理数除以零。浮点数除以零不会 panic，而是得到 `inf`/`NaN` |
| `panic: unwrap: called on none` | 对 `none` 进行了 `unwrap`。用 `match` 或 `unwrap-or` 处理 `none` 的情况 |
| `panic: Vector: index 5 out of bounds` | 下标越界。用 `len` 确认长度，或者使用越界时返回 `none` 的函数（`nth`、`pop` 等） |
| `panic: an integer argument does not fit a fixnum` | 给接受下标或个数的参数传入了 63 位放不下的 `int` |
| `throw: no enclosing (catch 'oops) for this throw` | 执行了没有被同一标签的 `catch` 包围的 `throw` |
| `panic: <消息>` | 程序调用了 `(panic "<消息>")`。`assert` 失败时是 `assertion failed: ...` |

panic 即使发生在任务中也会停止整个进程（[语法参考 12.4](syntax.md#124-与其他功能的关系)）。想要恢复的失败用
`Result` 表示（[语法参考第 9 章](syntax.md#9-错误处理方针)）。

## 5. 警告

| 消息 | 含义 |
|---|---|
| ``warning: redefining function `f` `` | 重新定义了同名函数。后面的定义生效。在 REPL 中修改定义时会正常出现 |
