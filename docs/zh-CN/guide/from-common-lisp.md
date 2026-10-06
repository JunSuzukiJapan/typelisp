<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 写给 Common Lisp 用户

typelisp 继承了 Common Lisp（以下简称 CL）的语法和许多函数名，但它是静态类型语言。因此，CL 的写法并不总能直接通过。
本指南汇总了熟悉 CL 的人容易遇到的问题，以及改写方法。

## 1. 没有 `nil` 和 `t`

布尔值是 `true` 和 `false`。`nil` 和 `t` 没有定义。

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **只有 `bool` 能作为条件。** 把 `0` 或空列表写作条件是类型错误。没有"nil 以外都为真"的规则。
- **`if` 的 else 不能省略。** `(if c x)` 是错误。不需要 else 时使用 `when` / `unless`。
- **"没有值"用 `Option<T>` 表示。** 在 CL 中通过返回 nil 表示"没找到"的函数，在这里返回 `(some x)` 或 `none`。

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- 空列表 `()` 根据上下文，或者是 Unit 类型的值（不返回任何东西的函数的返回值），或者是 S 表达式数据的空列表。它与
  `false` 是不同的值。

## 2. 写出类型

函数的参数和返回值必须写类型。

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; 泛型函数
  (unwrap-or (first (iter v)) default))
```

- 不能写 `(defun f (x) x)` 这样没有类型的定义。
- `defvar` 等全局变量也要写类型：`(defvar (count int) 0)`。
- `the` 不是运行时检查，而是给类型检查器的注解。
- **没有在运行时检查类型的手段。** 没有 `typep` 和 `type-of`，因为值的类型在编译时就已确定。想接受几种类型之一时，
  用 `defenum` 创建和类型，或者使用 trait。
- `deftype` 是类型别名。不能创建 `(deftype small () '(integer 0 9))` 这样表示值范围的类型。

整数的默认类型 `int` 是任意精度的，与 CL 的 integer 一样大小没有上限。也有固定宽度的 `i8` 至 `i32` / `u8` 至
`u32`。没有 64 位的固定长度整数类型。

## 3. 把函数当作值

typelisp 不区分函数与变量的命名空间。函数名可以直接作为值传递。没有 `#'` 和 `funcall`。

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; 不用 funcall，直接调用

(apply-to twice 5)                        ; 是 twice 而不是 #'twice
```

- `+` 或 `1+` 等内置函数，在参数类型像 `(fn (int) int)` 这样确定的位置，也可以直接作为值传递。传给 `foldl`、
  `map` 这类泛型函数时，无法确定是哪个类型的 `+`，请用 `lambda` 包裹。

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- 序列函数**对象在前，函数在后**：`(map it f)`、`(filter it f)`、`(foldl it f init)`。与 CL 的
  `(mapcar f list)` 顺序相反。
- `lambda` 不能使用 `&optional` 和 `&key`（可以使用 `&rest`）。
- **不能调用定义之前的函数。** 在 CL 中可以先调用以后才定义的函数，在这里会得到 `no such function`。相互递归的函数
  用 `defsignature` 先声明其中一个。

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. 列表与 Vector

相当于 CL 列表的是 **S 表达式数据**，其类型是 `Option<Sexpr>`（空列表是 `none`）。`(list 1 2 3)` 和 `'(a b c)` 都
是这个类型。S 表达式数据是供宏和 `read` 处理的，普通的数据容器请使用 **`Vector<T>`**。

| 想做的事 | CL | typelisp |
|---|---|---|
| S 表达式的首部与其余 | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| 依次遍历 S 表达式列表 | `(dolist (x xs) ...)` | 相同 |
| 类型确定的元素序列 | 列表或向量 | `Vector<T>` |
| 序对 | `(cons a b)` | `(cons a b)`（类型是 `cons-cell<A,B>`） |
| 映射 | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` 是用 `cons` 创建的序对 `cons-cell<A,B>` 的访问器，不能用于 S 表达式列表。

创建 `Vector`：

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

`map`、`filter`、`sort`、`find` 等序列函数作用于实现了 `Iter` trait 的值。`Vector` 要用 `(iter v)` 变成迭代器再传入。

## 5. 没有多值

没有 `values` 和 `multiple-value-bind`。在 CL 中返回多个值的函数，返回序对或结构体。

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `car` 为 3、`cdr` 为 1 的 `cons-cell` |
| `(decode-universal-time t)` → 9 个值 | `decoded-time` 结构体 |
| `(read-from-string s)` → 值、位置 | `(read-from-string s)` 把值和位置的 `cons-cell` 放在 `Result` 中返回。只要值时用 `(read s)` |

## 6. 没有特殊变量（动态绑定）

`let` 总是词法绑定。即使用 `let` 重新绑定以 `defvar` 定义的变量，从那里调用的函数看到的仍是原来的值。

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; CL 中为 2，typelisp 中为 1
```

想临时改变 `*print-base*` 这类控制变量时，使用 `dlet`。它赋予新值，并且无论以何种方式退出主体都会恢复原值。

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` 修改的是全局变量本身，所以不是按线程的绑定。

## 7. 没有采用条件系统

没有 `define-condition`、`handler-case`、`handler-bind`、`restart-case`、`error`、`signal`。它们与静态类型不相容。
取而代之，分别使用以下两种机制：

- **可恢复的失败返回 `Result<T,E>`。** 调用方用 `match` 区分 `ok` / `err`。没有相当于 Rust 的 `?` 的简写语法。

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **不可恢复的失败（bug）是 `panic`。** `(panic "message")`、把 `none` 传给 `unwrap`、除以 0、下标越界等都属于此类，
  程序会停止。`unwind-protect` 的清理在停止前执行。

错误类型由 `Error` trait 统一，用 `(message e)` 取出消息。创建自己的错误类型的方法见
[Option、Result 与错误类型](../reference/functions/option-result.md#3-错误类型与-error-trait)。`assert` 和 `warn`
可以像在 CL 中一样使用。

`catch` / `throw` / `unwind-protect` 是有的。不过 `catch` 的标签限于不求值的符号字面量（`'done`），用同一标签抛出
的值的类型只有一种。

## 8. 没有 CLOS

没有 `defclass`、`defgeneric` 和方法组合。

- 数据类型用 `defstruct`（结构体）和 `defenum`（和类型）定义。
- `defmethod` 定义只按**第一个参数的静态类型**确定调用目标的方法。不进行多重分派。
- 要让不同类型拥有共同的操作，使用 trait（`deftrait` / `impl`）。处理内部类型在运行时才确定的值时，使用 `:dyn Trait`
  类型（[语法参考 3.9](../reference/syntax.md#39-deftrait--impl--trait-机制)）。

`defstruct` 的区别：

- 构造函数是 `类型名::new`：`(point::new 1 2)`。想要 `make-point` 这样的名字时，可以用 `(:constructor make-point)`
  选项创建。
- 访问器除了 `(x p)`，也可以写成 `p::x`。用 `(setf p::x 5)` 修改。
- 不会生成谓词（`point-p`）。没有 `:conc-name`、`:type`、`:named`。
- `:include` 只是继承槽，并不会成为父类型的子类型。

## 9. 用模块代替包

没有包。命名空间是模块，文件本身就是模块。用 `module::name` 代替 `pkg:symbol`，用 `use` 引入
（[模块与文件结构](modules.md)）。

关键字 `:foo` 是存在的，它是求值为自身的符号。因为没有包，冒号也是名字的一部分：`(symbol->string :foo)` 返回 `":foo"`。

## 10. 读取与语法的区别

- 不区分大小写（符号在读取时变为小写）。与 CL 相同。
- 没有 `#'`（第 3 节）。不能读取复数字面量 `#c(...)`，复数用 `(complex 1.0 2.0)` 创建。
- 扩展 `loop` 的子句用关键字书写：`(loop :for i :from 1 :to 3 :collect i)`。不以关键字开头的 `loop` 是简单的无限
  循环，用 `(break)` 或 `(return 值)` 退出。`return` 退出的是最内层的循环（退出函数用 `return-from`）。
- `format` 的输出目标是 `false`（返回字符串）、`true`（标准输出）或流。格式指令与 CL 相同。
- 从字符串读取用 `(read "...")`，从流读取用 `(read-sexpr s)`。两者都返回 `Result`。
- `eval` 在求值之前对传入的表达式进行类型检查，返回 `Result`。与源代码中一样不能前向引用。
- 没有 `eval-when`。
- 函数名不使用 `?` 或 `!` 后缀。谓词与 CL 一样用 `-p` / `p`（`zerop`、`sexpr-null`），或者在前面加 `is-`（`is-some`）。

## 11. 名称不同的主要函数

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read`（从流读取） | `read-sexpr` |
| `pathname` | `to-pathname` |
| `floor` 等的双参数版本 | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map`（参数顺序相反。第 4 节） |
| `length`（向量） | `len` |
| `hash-table-count` | `count` / `size` |

函数一览见[内置函数](../reference/functions/README.md)。

## 12. 其他没有的东西

- `progv`、`symbol-function`、`symbol-value`
- `*readtable*` 与 `copy-readtable`、`readtable-case`（读取宏本身可以用 `set-macro-character` 定义）
- 逻辑路径名、通配符路径名
- `input-stream-p` / `output-stream-p`（流的方向由类型决定）
