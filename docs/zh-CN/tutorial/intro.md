<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 入门

本章从在 REPL 中求值表达式开始，依次介绍函数、变量、条件分支、循环，以及列表和 `Vector`。
`typl` 的构建方法请参阅 [README.md](../../../README.md)。

## 1. 启动 REPL

不带参数启动 `typl` 即进入 REPL（交互模式）。在 `typl>` 后输入表达式，它会被立即求值并显示结果。
输入 `:quit` 退出。

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

下文用这种形式表示 REPL 中的输入和结果。

## 2. 求值表达式

typelisp 是一种 Lisp，所以表达式用括号括起来，**运算符或函数名写在最前面**。要写 `(+ 1 2)`，而不是
`1 + 2`。

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

数有以下几种：

- **整数**的类型是 `int`。大小没有上限。
- **小数**的类型是 `f64`。像 `1.5`、`2.0` 这样带小数点书写。
- `int` 和 `f64` 不能混合计算。`(+ 1 2.0)` 是类型错误。需要转换时写 `(as f64 1)`。

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

两个整数之间的 `/` 得到舍去小数部分的整数（不会像 Common Lisp 那样得到分数）。余数用 `(mod 7 2)` 求。

布尔值是 `true` 和 `false`。

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. 定义函数

函数用 `defun` 定义。**参数类型和返回值类型必须写出。**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` 表示"`int` 类型的参数 `n`"。有多个参数时排列成 `((a int) (b int))`。
- 参数列表后面的 `int` 是返回值类型。
- 函数体中最后一个表达式的值就是函数的返回值。不写 `return`。

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

类型不匹配的调用会在**执行之前**作为类型错误报告。运行文件时，只要任何一处有类型错误，程序就一行也不会执行。

要让参数可以省略，使用 `&optional`。写上默认值后，省略时参数取该值。

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`format` 的第一个参数 `false` 表示"不输出，而是作为字符串返回"。每个 `~a` 的位置会被依次替换为后面的参数。

## 4. 变量

局部变量用 `let` 创建。

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- `let` 变量的类型由初始值决定，不需要写出。
- 同一个 `let` 中的变量不能互相引用。要用前一个变量构造下一个变量时，使用 `let*`。

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

改变变量的值使用 `setf`。**赋值不能改变变量的类型。**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

全局变量用 `defvar` 定义。这里要写出类型。

```lisp
(defvar (counter int) 0)
```

## 5. 条件分支

### if

写成 `(if 条件 为真时 为假时)`。**为假时的表达式不能省略。**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- 只有 `bool` 类型的表达式才能作为条件。像 `(if 0 ...)` 这样写数字是类型错误。
- 为真时和为假时的表达式必须是同一类型。

为假时什么都不做的话，使用 `when`（相反的情况用 `unless`）。

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

条件有三个以上时，`cond` 更易读。最后的 `else` 在所有条件都不成立时执行。

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

按值的形状分支时使用 `match`。

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` 匹配任何值。`int` 的值有无数个，所以没有 `_` 分支就会报错，说没有覆盖所有情况。`match` 真正的用武之地是
拆解下一章[类型基础](types.md)中出现的 `Option` 以及自己定义的类型。

## 6. 循环

函数可以调用自身。

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

固定次数的循环使用 `dotimes`。`i` 从 0 变化到 `n - 1`。

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

此外还有 `while`、`do`，以及与 Common Lisp 相同的扩展 `loop`。扩展 `loop` 的子句词用关键字书写
（`:for`、`:collect` 等）。

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. 列表与 Vector

### Vector

要并列保存同一类型的值，使用 `Vector<T>`。`T` 是元素类型。

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; 显示 #<vector<int> 3 1 2>
```

- 只写 `(Vector::new)` 无法确定元素类型，所以用 `(the Vector<int> ...)` 指定类型。
- `(push v x)` 追加到末尾，`(get v i)` 读取第 `i` 个元素，`(len v)` 得到长度。
- 用超出范围的下标 `get` 时，程序会报错并停止。

### lambda 与高阶函数

匿名函数用 `lambda` 创建。与 `defun` 一样要写出参数和返回值的类型。

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`、`filter`、`sort`、`foldl` 等接受用 `(iter v)` 变成**迭代器**的 `Vector`。对象在前，函数在后。
上面的 `v` 是用 `let` 绑定的，在该 `let` 之外无法使用。下例先用 `defvar` 定义 `v`。

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

要逐个处理元素，使用 `doiter`。

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

接受函数作为参数的函数，把该参数的类型写成 `(fn (参数类型...) 返回值类型)`。用 `defun` 定义的函数也可以
直接用名字作为值传递。

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### 列表（S 表达式）

用 `'(1 2 3)` 或 `(list 1 2 3)` 创建的列表是 **S 表达式数据**。元素的类型可以不一致。

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S 表达式数据主要用于在宏（[宏](macros.md)）和 `read` 中处理程序本身。要保存元素类型确定的数据，请使用
`Vector<T>`。S 表达式列表可以用 `dolist` 遍历。

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

两个值的组合用 `cons` 创建，用 `car` 和 `cdr` 取出。

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. 写在文件中运行

程序可以写在文件（扩展名 `.typl`）中，用 `typl 文件名` 运行。显示结果使用 `println`。

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` 使用与 `format` 相同的格式输出，并在末尾换行。不换行的是 `print`。
- `~a` 以供人阅读的形式嵌入，`~s` 以可读回的形式嵌入（字符串会带上 `"`）。
- 文件从上到下依次读取。**函数不能在定义之前调用。**

## 9. 接下来阅读

- [类型基础](types.md)：`Option`、`Result`、结构体、枚举、泛型
- [写给 Common Lisp 用户](../guide/from-common-lisp.md)：面向熟悉 Common Lisp 的读者的区别一览
