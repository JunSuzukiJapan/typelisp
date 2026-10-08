<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# 类型基础

typelisp 是静态类型语言。本章介绍类型检查能为你做什么、常用的类型（`Option`、`Result`、结构体、枚举）以及
泛型。阅读本章前请先读[入门](intro.md)。

## 1. 什么是静态类型

在 typelisp 中，所有表达式的类型都在程序运行前确定。类型不匹配的表达式在运行前就会报错。

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; 类型错误

(main)
```

运行这个文件时，连 `start` 都不会显示，就因类型错误而停止。

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

需要写出类型的地方是函数的参数和返回值、全局变量以及结构体的字段。`let` 变量的类型由初始值决定。

主要类型：

| 类型 | 值的例子 |
|---|---|
| `int` | `42`、`-7`（任意精度整数） |
| `i8` `i16` `i32` `u8` `u16` `u32` | 固定宽度整数 |
| `f64` `f32` | `1.5` |
| `bool` | `true`、`false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`、`:key` |
| `()` | 不返回值的函数的返回类型 |

没有在运行时查询值的类型的手段（没有 Common Lisp 的 `typep` 或 `type-of`），因为所有类型在运行前都已知。

## 2. `Option<T>`：可能没有的值

typelisp 中没有 `nil`。"可能没有值"用 `Option<T>` 类型表示。`Option<T>` 的值要么是持有一个 `T` 值的
`some`，要么是什么都不持有的 `none`。

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` 不是 `int`，不能直接用于计算。`(+ (safe-div 10 2) 1)` 是类型错误。要使用其中的值，用
`match` 区分 `some` 和 `none`。

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- 在 `(some q)` 分支中，内容被绑定到变量 `q`。
- `match` 会检查各分支是否**覆盖了所有情况**。忘写 `(none)` 分支是类型错误。

### 为什么没有 nil

在许多语言中，`nil`（`null`）可以充当任何类型的值。因此，即使忘了处理"没有值"的情况，也要到运行时才会发现。
在 typelisp 中，可能没有值的地方是 `Option<T>` 类型，不在 `match` 中处理 `none` 的情况就无法通过类型检查。
遗漏在运行前就会被发现。

条件也遵循同样的思路：`if` 的条件只能是 `bool`。没有 Common Lisp 那种"`nil` 以外都为真"的规则。

### 常用操作

| 写法 | 含义 |
|---|---|
| `(unwrap-or opt 默认值)` | `some` 时取内容，`none` 时取默认值 |
| `(unwrap opt)` | 取出内容。`none` 时程序停止 |
| `(is-some opt)` / `(is-none opt)` | 判断是哪一种 |

标准库中也有很多返回 `Option` 的函数。例如 `position` 找到时用 `some` 返回位置，找不到时返回 `none`。

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`：可能失败的操作

可能失败的操作返回 `Result<T,E>`：成功时是持有值 `T` 的 `ok`，失败时是持有错误 `E` 的 `err`。

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

自己的函数也可以返回 `Result`。

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

"没有"不需要理由时用 `Option`，想说明失败原因时用 `Result`。错误的处理方式在[错误处理](errors.md)中详细介绍。

## 4. `defstruct`：结构体

带有具名字段的类型用 `defstruct` 定义。

```lisp
(defstruct point
  (x int)
  (y int))
```

定义之后可以使用以下内容：

```lisp
(let ((p (point::new 3 4)))     ; 创建（按字段顺序传参）
  (println "~a" p::x)           ; 读取字段，也可写成 (x p)
  (setf p::x 10)                ; 修改
  (println "~a" p))             ; #<point x: 10 y: 4>
```

要给结构体添加函数，使用 `defmethod`。第一个参数（`self`）的类型决定方法属于哪个类型。

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

不写 `self` 而只写类型名，就得到以 `point::origin` 形式调用的函数。

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`：几种形状之一

像"圆、矩形或点"这样属于几种形状之一的值，用 `defenum` 定义。每种形状称为**变体**。每个变体可以持有不同数量、
不同类型的值。

```lisp
(defenum shape
  (circle int)        ; 半径
  (rect int int)      ; 宽和高
  (dot))              ; 不持有值
```

值要像 `shape::circle` 这样加上类型名创建。在 `match` 中按变体名拆解。

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

这里 `match` 同样检查是否覆盖了所有情况。之后给 `shape` 增加变体时，所有没有处理该变体的 `match` 都会变成类型
错误，因此不会遗漏需要修改的地方。

写了 `(use shape)` 之后，就可以不带类型名写 `(rect 5 6)`。

`Option` 和 `Result` 也是用这种机制构建的枚举。

## 6. 泛型

适用于任何类型的函数，在名字后面写上 `<T>` 这样的**类型参数**来定义。

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

调用时不指定类型，`T` 由参数决定。

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T 是 int
(first-or names "none")    ; T 是 string
(first-or ints "none")     ; 类型错误：ints 是 Vector<int>，所以 T 是 int
```

结构体和枚举也可以是泛型的。`Vector<T>`、`Option<T>`、`Result<T,E>` 就是这种类型。

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

在泛型函数中对 `T` 一无所知，所以不能比较或相加 `T` 的值。要加上"只要能比较的类型就行"之类的条件，使用
trait（[trait](traits.md)）。

## 7. 给类型起别名

`deftype` 可以给类型起别名。

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` 只是 `int` 的另一种写法，并不是新类型。在需要 `meters` 的地方传入普通的 `int` 也不会报错。想区分它们
时，像 `(defstruct meters (value int))` 这样创建结构体。

## 8. 接下来阅读

- [trait](traits.md)：让类型拥有共同的操作
- [类型一览](../reference/types.md)：内置类型以及各自实现的 trait
