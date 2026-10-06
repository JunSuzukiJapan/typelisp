<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option、Result 与错误类型

## 1. `Option<T>` / `Result<T,E>`

构造函数：`Option<T>` 是 `Some(T)` / `None`，`Result<T,E>` 是 `Ok(T)` / `Err(E)`。`E` 可以是任何类型——内置的
具体错误类型，以及用 `defstruct`/`defenum` 编写的自己的类型，都可以直接放进去（第 3 章）。

| 名称 | 形式 | Option | Result | 说明 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 取出值。`None`/`Err` 时 panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 值，或默认值 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | 是否为 `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | 是否为 `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | 是否为 `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | 是否为 `Err` |

构造函数是 `Option::some`/`Option::none`/`Result::ok`/`Result::err`（或者在 `(use option)`/`(use result)` 之后
使用裸名字 `some`/`none`/`ok`/`err`）。

分支用 `match` 明确写出。没有相当于 Rust 的 `?` 的语法。

## 2. `Option<T>` 的运行时表示

与 Rust 一样，**`Option<T>` 大多数情况下不创建盒子**。`some v` 就是 `v` 本身，`none` 是空列表的值，既没有分配也
没有间接访问。`Option<Sexpr>`（空列表为 `none`）、`Option<int>`、`Option<string>`、`Option<my-struct>`、
`Option<f64>`、`Option<(fn ...)>` 都是这种形式。

只有当 `T` 的值无法与空列表的值区分时才装箱：

| `T` | 表示 | 原因 |
|---|---|---|
| `Option<U>`（嵌套） | 盒子 | 内层的 `none` 会与外层的 `none` 成为同一个值 |
| `()` | 盒子 | `()` 的值就是空列表的值本身 |
| `ptr` / `c-long` / `c-ulong` | 盒子 | 64 位全部都是值，没有用于区分的余地 |
| 其他 | 无盒子 | — |

表示只由类型决定，无法从值读出。打印时根据静态类型恢复 `(some ...)`/`none`，所以 `(format false "~a" opt)` 输出
`(some 1)`。有两个限制：

- **不能放进 `:dyn Trait`**（把 `(impl Speak Option<int> ...)` 过的值传给 `:dyn Speak` 是错误）。
- 从 `Sexpr` 进行 `(the Option<T> ...)` 向下转换时要**指明构造函数**——`(the Option<int> (some x))` /
  `(the Option<int> (none))`。绑定整体的 `(the Option<int> o)` 形式是错误。

## 3. 错误类型与 `Error` trait

仿照 Rust 的 `std::error::Error`，**`Error` 不是类型而是 trait**。表示错误的具体类型按用途分开，都实现了 `Error`。

| 类型 | 来源 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 文件／流操作（[流与文件](streams-files.md)） |
| `NetError` | 网络操作（[网络](network.md)） |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`。CL 的 `simple-error`——"只想说明发生了什么"时的默认选择 |
| `WrappedError` | `(wrap-error msg cause)`。同时携带自己的消息和原因的类型，也是 `Error` trait 有 `source` 的原因 |

从 `ParseIntError` 到 `NetError` 都是"持有一个消息字符串的单变体枚举"，类型名与变体名相同
（`(match e ((ParseIntError m) m))`，构造为 `(ParseIntError::ParseIntError "...")`）。没有任何特殊处理，与用
`(defstruct my-err (...))` / `(defenum my-err ...)` 编写自己的错误类型时完全一样。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | 错误消息（`Error` trait 的方法） |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | 这个错误包装的原因，没有时为 `None`（Rust 的 `Error::source`） |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`（`E` 实现 `Error`） | 把具体错误类型扩展为 trait 对象 |
| `describe-error` | `(describe-error e)` | `E→string`（`E` 实现 `Error`） | 消息以及沿 `source` 追溯的原因链，每行一个原因。CL 中没有对应物（Rust 的 "caused by"） |

为自己的错误类型实现 `Error`，就能**以同样的方式**处理它和内置错误：

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 把具体类型直接放进 E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 不区分种类统一处理
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

把多种错误类型收集到一个 `Result` 中时，使用 `Result<T, :dyn Error>`（相当于 Rust 的 `Box<dyn Error>`），具体错误用
`as-dyn-error` 扩展。因为没有 `?`，这个转换要明确写出：

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**类型和 trait 共享一个命名空间**（与 Rust 相同）。在同一模块中，`defstruct`/`defenum` 与 trait 不能同名，在类型位置
写 trait 名会报告"`error` is a trait, not a type — write `:dyn error`"。

不可恢复的失败用 panic 表示。panic 的处理和 `catch`/`throw` 见
[语法参考](../syntax.md#8-非局部退出catch--throw--unwind-protect)，错误处理方针见
[同第 9 章](../syntax.md#9-错误处理方针)。
