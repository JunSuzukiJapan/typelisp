<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 错误处理

typelisp 的错误处理把失败分为两类来考虑。

| 失败的种类 | 例子 | 表示方式 |
|---|---|---|
| 可能发生的失败（可恢复） | 文件不存在、输入不是数字 | 返回 `Result<T,E>` |
| 程序的错误（不可恢复） | 下标越界、对 `none` 的 `unwrap`、除以 0 | 用 `panic` 停止 |

此外还有一次性跨越多层函数调用跳出的 `catch` / `throw`，以及无论如何退出都会执行清理的 `unwind-protect`。
阅读本章前请先读[类型基础](types.md)中 `Result` 一节。

## 1. 返回 `Result`，用 `match` 接收

编写一个从字符串读取端口号的函数。失败有两种情况：不是数字，或者超出范围。

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

调用方用 `match` 区分成功和失败。

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- 返回 `Result` 的函数的值，不在 `match` 中处理 `err` 的情况就无法使用。忘记处理失败是类型错误。
- `parse-int` 的错误是 `ParseIntError` 类型的值。用 `(message e)` 取出消息字符串。

## 2. 把失败传给调用方

没有 Rust 的 `?` 那样的简写语法。连续调用返回 `Result` 的函数时，用 `match` 写出"失败就原样返回"的处理。

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

知道不会失败时，或者在失败了停止也无妨的小脚本中，可以用 `unwrap` 取出内容。如果是 `err` 则会 panic。用默认值
就够的话，使用 `unwrap-or`。

## 3. 创建自己的错误类型

用类型而不是字符串表示错误，调用方就可以按错误的种类分支。错误类型就是普通的 `defenum` 或 `defstruct`，再实现
`Error` trait。

```lisp
(defenum config-error
  (missing string)          ; 缺少设置项
  (invalid string int))     ; 值不正确

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` 返回错误的说明。
- `source` 返回导致这个错误的另一个错误。没有原因时为 `none`。

## 4. 合并不同种类的错误

在一个函数中同时调用 `parse-int`（`ParseIntError`）和 `check-workers`（`config-error`）时，错误类型有两种，
无法都写在一个 `Result<T,E>` 的 `E` 中。这时把 `E` 设为 `:dyn Error`（实现了 `Error` 的某种错误），各个错误
用 `as-dyn-error` 转换。

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

传入 `"4"`、`"-1"`、`"abc"` 的结果如下：

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

关于 `:dyn`，请参阅 [trait](traits.md) 第 5 节。

## 5. `panic`：程序的错误

遇到绝不应该出现的状态时，用 `panic` 停止程序。

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic` 的类型是 `!`（不返回），可以写在需要任何类型的地方。上例中 `if` 两边的类型能对上就是这个原因。
- 下列操作也会 panic：对 `none` 或 `err` 的 `unwrap`、用越界下标 `get`、整数除以 0。
- `panic` 会停止程序。即使发生在任务中，整个程序也会停止。
- 在 REPL 中，panic 不会结束 REPL，而是等待下一个输入。
- "还没写"可以写成 `(todo)`，"不应该到达这里"可以写成 `(unreachable)`。两者都会 panic。

`panic` 不是 `Result` 的替代品。用户输入、文件是否存在这类可能发生的失败，请使用 `Result`。

## 6. `catch` / `throw`：跨函数跳出

`throw` 会一口气跳到外层同一标签的 `catch`。中间隔着多少层函数调用都没关系。

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

`v` 中没有负数时，`validate` 返回 `"all fine"`；有 `-7` 时，从 `check-all` 内部跳到 `catch`，返回
`"negative: -7"`。

- 标签直接写成 `'bad-input` 这样的符号。
- **每个标签抛出的值的类型只有一种。** 上例中 `'bad-input` 携带 `string`，用同一标签抛出 `int` 是类型错误。
  `catch` 函数体的类型也必须与标签的类型一致。

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` 找不到同一标签的 `catch` 时是错误。

如果只是想在函数中途返回，请使用 `return-from` 而不是 `catch` / `throw`。`return-from` 不能跨越函数，但作为
交换，读源代码就能知道它返回到哪里。

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`：一定执行清理

`(unwind-protect 主体 清理)` 无论以何种方式退出主体，都会执行清理。正常结束、被 `throw` 跳出、发生 panic 时都会执行。

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

用于"打开的文件一定要关闭""取得的锁一定要归还"之类的处理。标准库的 `with-open-file` 和 `with-lock` 内部也使用
了 `unwind-protect`。

## 8. 关于 Common Lisp 的条件系统

typelisp 没有采用 Common Lisp 的条件系统（`handler-case`、`restart-case` 等）。它不会在类型中体现函数可能引发
哪些失败，与静态类型不相容。可能发生的失败用 `Result` 写进类型，控制转移用 `catch` / `throw` 完成。

## 9. 接下来阅读

- [并发](concurrency.md)：任务与通道
- [Option、Result 与错误类型](../reference/functions/option-result.md)：函数一览
- [错误消息](../reference/errors.md)：常见错误的含义与修正方法
