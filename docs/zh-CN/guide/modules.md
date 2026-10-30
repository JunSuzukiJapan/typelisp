<!-- translated-from: docs/ja/guide/modules.md @ b4f23852eb3f4b11e3fd96c05da870bdd383e84f -->
# 模块与文件结构

本指南说明如何组织由多个文件构成的程序。详细的语法规则见[语法参考](../reference/syntax.md#310-module--use--命名空间)
的 3.10 至 3.13 节。

## 1. 一个文件就是一个模块

在 typelisp 中，**文件本身就是模块**。从源代码根目录看到的文件相对路径就是模块路径。

| 文件 | 模块 |
|---|---|
| `<根目录>/geometry.typl` | `geometry` |
| `<根目录>/geo/shapes.typl` | `geo::shapes` |
| `<根目录>/net/http/client.typl` | `net::http::client` |

不需要在文件开头写模块声明。

## 2. 创建项目

在项目根目录放一个名为 `typelisp.toml` 的文件。内容可以为空。

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

想把源代码放在 `src/` 下时，在 `typelisp.toml` 中写下这一行：

```toml
src = "src"
```

`typl` 从要运行的文件所在目录开始向上查找 `typelisp.toml`，把找到的位置作为源代码根目录。找不到时，要运行的文件
所在目录就是根目录（REPL 中是当前目录）。

## 3. 公开定义并使用

`geometry.typl`：

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; 没有 pub 的字段不能从外部读写

(defun square ((n i32)) i32 (* n n))   ; 没有 pub 的函数也不能从外部调用

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(impl point
  (pub new ((x i32) (y i32) (tag string)) point
    (make-instance 'point :x x :y y :tag tag))
  (pub origin () point (point::new 0 0 "o")))
```

`main.typl`：

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

写下 `(use geometry)` 时就会读入 `geometry.typl`。不需要事先读入。

### 公开的单位

- 函数、结构体、枚举、trait、全局变量、宏，各自只有加了 `pub` 的才能被其他模块看到。写法是像 `(pub defun ...)`
  这样，把 `pub` 放在定义之前。
- 写在 `impl` 中的方法，只有按项目写成 `(pub new ...)` 的才能从外部使用。
- 结构体的**类型公开与字段公开是分开的**。`(pub defstruct point ...)` 使类型可见，各字段只有写成
  `(pub x i32)` 的才能从外部读写。
- 在外部模块中能写进 `make-instance` 的也只有 `pub` 的字段。上面的 `point` 的 `tag` 不是 `pub`，所以在外部
  不能用 `make-instance` 创建，要用 `point::new`。
- 从外部使用未公开的名字，会得到 `unresolved path: geometry::square` 这样"无法解析"的错误。这与拼错名字时的
  消息相同，所以拼写正确却无法解析时，请怀疑是否忘了加 `pub`。

可以加 `pub` 的定义一览见[语法参考 3.13](../reference/syntax.md#313-pub--公开)。

## 4. `use` 的写法

```lisp
(use geometry)              ; 引入模块。写成 geometry::dist2 来使用
(use geometry::dist2)       ; 引入函数。可以用裸名字 dist2
(use geometry::point)       ; 引入类型。可以写 point::new、point::origin 以及类型注解中的 point
(use geometry::point::new)  ; 引入类型的静态方法。可以用裸名字 new
(use geometry::shape::*)    ; 让类型的构造函数和公开的静态方法全部可以用裸名字使用
(use a::f b::g)             ; 可以一次写多个
```

- **`use` 只对其后的形式生效。** 请放在文件开头。在 `use` 之前写 `geometry::dist2` 会得到 `unresolved path`。
- 不 `use` 模块而直接写完整路径 `geometry::dist2` 也无法解析。读入文件的契机只有 `use`。
- 对已被占用的裸名字再 `use` 同名的东西会产生警告。明知如此仍要引入时，使用 `shadowing-import`。如果是类型的构造函数或静态方法，则不是警告而是错误。
- `(use geometry::point)` 引入的只有类型的名字。想把 `point::new` 写成 `new` 时，写 `(use geometry::point::new)` 或 `(use geometry::point::*)`。
- 目录中的模块写成 `(use geo::shapes)`，引入后用最后一部分（`shapes::...`）引用。

### 使用与标准库相同的名字

用 `length`、`first` 这类标准库公开的名字定义函数等会出错。

```
type error: defun: `length` is a name the standard library offers every module; write `(shadow length)` before defining it here
```

决定在该模块中使用自己的定义时，在定义之前写 `shadow`。

```lisp
(shadow first)

(defun first ((a int) (b int)) int a)

(println "~a" (first 10 20))                                          ; 本模块的 first => 10
(println "~a" (std::seq::first (iter (the Vector<int> #(1 2 3)))))    ; 标准库的 first => (some 1)
```

`shadow` 之后，在该模块中写 `first` 指的是自己的定义。要使用标准库的 `first`，就用路径写成
`std::seq::first`。`shadow` 只在写它的模块中生效，其他模块中的 `first` 仍是标准库的。`if`、`let` 这
类特殊形式的名字不能 `shadow`，也不能用于定义。

### trait 方法的调用方式

在 `impl` 中实现的方法**属于类型**而不是模块的函数，所以调用时不加模块名。

```lisp
(use shapes::core)
(println "~a" (area (make-instance 'core::square :side 4)))    ; 是 area 而不是 core::area
```

trait 的 `impl` 的方法即使不写 `pub` 也总是公开的。固有 `impl`（`(impl point ...)`）的方法只有加了 `pub`
的才公开。

trait 也只有写成 `(pub deftrait ...)` 的才能被其他模块看到。在其他模块中，像 `shapes::Shape` 这样用路径来写，或者用 `use` 引入，再写到 `impl` 中。

## 5. 在文件内划分命名空间

想在一个文件内进一步划分命名空间时，使用 `module`。它嵌套在文件自身的模块内部。

```lisp
;; 在 main.typl 中
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

要把文件剩余部分全部放进一个命名空间，也可以不用括号包裹，写成 `(in-module util)`。

## 6. 依赖关系的限制

- **不能形成循环。** `a.typl` 中 `(use b)`、`b.typl` 中 `(use a)` 时，会得到
  `circular module dependency: a -> b -> a` 错误。请把双方都需要的定义移到第三个模块。
- **类型和函数都不能前向引用。** 即使在同一个文件中，也不能在定义之前使用。相互递归的函数用 `defsignature` 先声明
  其中一个（[语法参考 3.2](../reference/syntax.md#32-defsignature--前向声明)）。

## 7. 执行顺序

用 `typl main.typl` 运行时，按以下顺序进行：

1. 读入 `main.typl` 以及从它 `use` 的所有文件，并进行类型检查。**只要任何一处有类型错误，就什么都不执行。**
2. 被 `use` 的模块的顶层表达式先于使用方执行。
3. `main.typl` 的顶层表达式从上到下依次执行。

把程序入口整理成 `main` 函数，并在文件末尾调用 `(main)`，同一个文件也能直接用于
[AOT 编译](compile.md#3-用-aot-编译生成可执行文件)。

## 8. 与 `load` 的区别

`(load "path")` 与 Common Lisp 的 `load` 一样，把文件内容**原样读入当前命名空间**。它不用模块包裹，与 `pub` 也
无关。用于读入设置文件、在 REPL 中重新读入手边的文件等。把程序拆分成部件时，请使用 `use`。
