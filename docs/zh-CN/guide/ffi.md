<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C FFI（defffi）

本指南说明如何从 typelisp 调用 C 函数。可以声明的类型和限制一览见
[语法参考 3.3](../reference/syntax.md#33-defffi--声明-c-函数ffi)。

## 1. 声明并调用函数

用 `defffi` 声明 C 函数的名称和类型。

```lisp
(defffi (c-getpid "getpid") () i32)            ; typelisp 中的名字和 C 的符号名
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; 在 libm 中查找
```

调用要用 `(unsafe ...)` 包裹。

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

之所以需要 `unsafe`，是因为编译器无法确认声明的类型是否与 C 侧真正的类型一致。写下 `unsafe` 表示由编写者承担这项确认。
忘了写会得到说明这一点的错误。

## 2. 编写安全的包装

预期的用法是把 `unsafe` 限制在一处，对外呈现为普通函数。

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; 调用方不需要 unsafe
(str-len "hello")  ; => 5
```

## 3. 类型的对应

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | 同宽度的整数 |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool`（`_Bool`） |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long`（也包括 `size_t`、`int64_t` 等） |
| `ptr` | 任意指针（`void *`、`FILE *` 等） |
| `(ptr T)` | 指向 `T` 的指针（[第 7 节](#7-c-结构体)） |

### 字符串

- 传递 `string` 时，会先复制成以 NUL 结尾的 C 字符串再传入，调用结束后释放。字符串中间有 NUL 会报错。
- 返回 `string` 的函数的结果也会被复制，C 侧的内存不会被释放。对于返回需要调用方释放的字符串的函数（`strdup` 等），
  请用 `ptr` 接收并自己 `free`。
- 声明为返回 `string` 的函数返回 NULL 时会报错。可能返回 NULL 的函数（`getenv` 等）请用 `ptr` 接收。

### `c-long` / `c-ulong` / `ptr`

这些类型只用于跨越与 C 的边界传值，**不能进行算术运算**。要作为 typelisp 的整数使用时，用 `as` 转换。

```lisp
(as int (unsafe (c-strlen s)))      ; int 不会丢失 64 位的值
(try-as i32 (unsafe (c-strlen s)))  ; 放不进 i32 时为 none
(unsafe (c-malloc 16))              ; 整数字面量可以直接传递
```

`ptr` 是用来再传回 C 函数的值。在 typelisp 一侧没有读取其内容的手段。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

这些类型只能出现在函数的参数、返回值和局部变量中。不能作为结构体的字段、全局变量，也不能作为 `Vector` 等的类型参数。

## 4. 指定库

省略 `:library` 时，从进程中已经链接的内容（libc 等）里查找符号。其他库中的函数用 `:library` 指定。

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- `"sqlite3"` 这样的短名称会依次查找 `libsqlite3.dylib`、`libsqlite3.so`。
- 包含 `/` 的名称视为路径。
- 找不到声明的符号时，会得到点出该名称的错误。

## 5. AOT 编译

使用了 `defffi` 的程序也可以直接用 [`compile-file`](compile.md#3-用-aot-编译生成可执行文件) 生成可执行文件。
`:library` 指定的库会在链接时自动加入，所以不需要给 `compile-file` 加参数。

## 6. 回调

可以把 typelisp 函数传给 C 函数，让它回调。在 `defffi` 的参数类型中写函数类型，调用时在该位置写函数名或 `lambda`
表达式。

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") 返回 p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- 只能传递**没有自由变量**的函数。顶层函数、`lambda`、`labels` 的局部函数都可以，但如果引用了外层的局部变量，
  类型检查时就会报错。C 只会传递声明过的参数，没有办法把捕获的变量送过去。需要保存状态时请使用全局变量。
- 不能传递存放函数的变量。请在该位置直接写函数名或 `lambda` 表达式。
- 回调中发生的 panic 或 `throw` 会在 C 函数返回后传到调用方。
- 只有在 typelisp 调用的 C 函数运行期间才能回调。不能用于从 `atexit` 或信号处理函数调用之类的场景。

## 7. C 结构体

想把结构体数组等传给 C 函数时，用 `def-c-struct` 声明与 C 布局相同的结构体，并在 `unsafe` 中分配。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; 在顶层的 unsafe 中声明

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; 4 个 item，内容为 0
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` 分配 `n` 个 `T` 并返回 `(ptr T)`。`(c-ref p i)` 是指向第 `i` 个元素的指针，`p::field` 是字段，
  `(c-deref p)` 是指向 `i32` 等标量的指针所指的内容。它们都可以用 `setf` 写入。
- 用 `(as ptr p)` 变成无类型的 `ptr`，传给接受 `void *` 的 C 函数。
- `item` 的大小（这里是 8）和各字段的位置按与 C 相同的规则确定。

### 分配的内存的生命周期

分配的内存在离开该函数中最外层的 `unsafe` 时释放。因 panic 或 `throw` 离开时也一样。因此 `(ptr T)` 的值不能
带出 `unsafe` 之外。把它作为 `unsafe` 的值、在闭包中捕获、传给 `task`、用 `throw` 抛出，都会在类型检查时报错。
想在外面使用的值，请在 `unsafe` 中复制成数值或 `defstruct`。

在 `lambda` 或 `labels` 的函数中分配时，请在其中写 `unsafe`。

### C 分配的内存

作为 `(ptr T)` 从 C 收到的指针（`defffi` 的返回值、回调的参数等），如果不是指向用 `c-alloc` 分配的内存内部，就会
报错。接收 C 用 `malloc` 分配的内存或 NULL 的函数，请用无类型的 `ptr` 声明。

## 8. 做不到的事

- **可变参数函数**（`printf` 等）不能声明。可变部分按与固定参数不同的规则传递。请按使用的参数个数分别用不同的名字声明。
- **按值传递或返回结构体**不可行。请使用通过指针传递的函数。
- **泛型声明**不可行。
- **与内置函数同名**不可行。
- **不能作为函数值传递。** 不能像 `(map xs c-abs)` 这样传递，请用 `lambda` 包裹。

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
