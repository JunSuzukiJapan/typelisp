<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
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
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | 取出值。遇到 `None`/`Err` 时以 `msg` panic |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | 值，或 `f` 的结果。`f` 只在 `None`/`Err` 时调用 |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | 对 `Some`/`Ok` 的内容应用 `f` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | 对 `Err` 的内容应用 `f` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | 遇到 `Some`/`Ok` 时把内容交给 `f`，返回其结果 |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | 遇到 `None`/`Err` 时返回 `f` 的结果 |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | 把 `Some(v)` 变成 `Ok(v)`，把 `None` 变成 `Err(e)` |

构造函数是 `Option::some`/`Option::none`/`Result::ok`/`Result::err`（或者在 `(use option)`/`(use result)` 之后
使用裸名字 `some`/`none`/`ok`/`err`）。

分支用 `match` 明确写出，或用上面的 `map`/`and-then` 等串起来。没有相当于 Rust 的 `?` 的语法。

`Option`/`Result` 的 `map` 是方法，与[序列](sequences.md)的 `map` 是不同的东西。第 1 个参数的类型是 `Option`/`Result` 时调用的是这一个。

`->` 宏把值依次作为后续各个表达式的第 1 个参数传入（与 Clojure 的 `->` 相同）。`(-> x (f a) (g b))` 变成 `(g (f x a) b)`。不带括号的名字 `h` 当作 `(h x)`。方法的第 1 个参数是接收者，所以组合子可以直接串起来：

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

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
