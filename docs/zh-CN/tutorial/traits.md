<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# trait

trait 是"这个类型支持这些操作"的约定。它让多个类型拥有同名的操作，从而使用这些操作的函数不必为每个类型分别编写。
其机制与 Rust 的 trait 几乎相同。阅读本章前请先读[类型基础](types.md)。

## 1. 定义并实现 trait

把返回图形面积和名称的操作定义为 trait `Shape`。

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `deftrait` 名字后面的 `()` 是所继承的 trait 列表（第 4 节）。没有时留空。
- 每一行是一个方法声明。`Self` 指"实现这个 trait 的类型"。

为类型实现 trait 时写 `impl`。

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

实现的方法可以像普通函数一样调用。

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

只要漏实现 trait 中声明的任何一个方法，就会在 `impl` 处报类型错误。

## 2. trait 约束："实现了这个 trait 的任何类型"

可以用 `where` 给泛型函数的类型参数加条件。这称为 **trait 约束**。

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

因为有 `(where (Shape T))`，函数体中可以对 `T` 的值使用 `name` 和 `area`。没有约束时对 `T` 一无所知，就无法调用。

传入没有实现 `Shape` 的类型时，会在调用处报类型错误。

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

泛型函数会为每个被调用的类型生成专用的函数。不会在运行时检查类型并分支。

## 3. 默认实现

在 trait 的方法中写上函数体，`impl` 省略该方法时就会使用它。

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe 使用默认实现

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; 自己写的优先

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. 实现标准 trait

标准库中也有 trait，实现之后，标准函数就可以用于该类型。

| trait | 要实现的方法 | 可以使用的功能 |
|---|---|---|
| `Eq` | `equals` | `member`、`find`、`position`、`match` 的 `(= 式)` 模式等 |
| `Ord` | `less` | `less-equal`、`greater` 等。`Ord` 继承 `Eq` |
| `print-object` | `print-object` | `println` 等显示时的形式 |
| `Iter` | `next` | `doiter`、`map`、`filter`、`sort` 等 |
| `Error` | `message`、`source` | 作为错误类型使用（[错误处理](errors.md)） |

为表示金额的类型实现 `Eq` 和 `Ord`。`Ord` 继承 `Eq`，所以要先写 `Eq` 的 `impl`。

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true（Ord 的默认实现）
```

实现 `print-object` 可以决定 `println` 的显示方式。当要求可读回的形式时（如 `~s`），参数 `escape` 为 `true`。

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

与 trait 约束结合，就能写出适用于任何实现了 `Ord` 的类型的函数。

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

传入按 300、900、100 顺序放入 `money` 的 `Vector`，返回 `(some 900 yen)`。

## 5. `:dyn`：统一处理不同类型的值

`Vector<T>` 的元素都是同一类型，所以 `circle` 和 `rect` 不能放进同一个 `Vector<circle>`。要统一处理"实现了
`Shape` 的某种东西"，使用 `:dyn Shape` 类型。

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- `circle` 或 `rect` 的值放在需要 `:dyn Shape` 的地方时会自动转换。
- `(area s)` 调用哪个类型的 `area`，在运行时由 `s` 内部值的类型决定。
- 把没有实现 `Shape` 的类型的值放在 `:dyn Shape` 的位置是类型错误。

第 2 节的 trait 约束与 `:dyn` 的选择：

| | trait 约束（`where`） | `:dyn Trait` |
|---|---|---|
| 调用目标何时确定 | 运行前 | 运行时 |
| 在一个 `Vector` 中混合不同类型 | 不可以 | 可以 |
| 可用的类型 | 无限制 | 结构体、枚举、`int`、`string`、`f64` 等（不包括 `bool`、`char`、`symbol`、`i32` 等） |

可以放入的类型的准确列表见[语法参考 3.9](../reference/syntax.md#39-deftrait--impl--trait-机制)。

也有不能用于 `:dyn` 的 trait：方法在 `self` 以外的参数或返回值中使用 `Self` 的情况（如 `Eq` 的 `equals`）。
因为类型要到运行时才知道，无法准备"同一类型的值"。

## 6. 限制

- 请把 trait 的定义、对它的 `impl`，以及通过 `:dyn` 使用该 trait 的代码放在同一个模块（文件）中。目前还不能
  把 trait 公开给其他模块。
- 类型和 trait 位于同一命名空间。在同一模块中，类型和 trait 不能同名。

## 7. 接下来阅读

- [宏](macros.md)：自己定义语法
- [语法参考 3.9](../reference/syntax.md#39-deftrait--impl--trait-机制)：全面实现（blanket implementation）、关联类型等
- [标准 trait](../reference/functions/traits.md)：标准库中的 trait 一览
