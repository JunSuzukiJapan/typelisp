<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 宏

宏是接收程序并返回程序的函数。用宏可以自己创造函数无法实现的新语法。typelisp 的宏与 Common Lisp 的 `defmacro`
是同一种机制。阅读本章前请先读[入门](intro.md)中的"列表（S 表达式）"。

## 1. 宏与函数的区别

函数接收的是参数**求值后的值**。宏接收的是参数**求值之前的表达式本身**（作为 S 表达式数据），组装出另一个表达式
并返回。返回的表达式替换掉宏调用的位置，然后才进行类型检查和执行。这种替换称为**展开**。

例如 `unless` 这样的语法无法写成函数。写成函数的话，即使条件为真，函数体也会先被求值。

## 2. `defmacro` 与准引用

创建一个只在条件为假时执行函数体的 `my-unless`。

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- 宏的参数不写类型。所有参数都是 S 表达式数据。
- `&rest body` 把其余的参数合在一起作为一个列表接收。
- 以 `` ` ``（准引用）开头的表达式按原样作为数据组装。在其中：
  - `,test` 把变量 `test` 的内容嵌入该位置。
  - `,@body` 把列表 `body` 的元素展开嵌入该位置。

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

展开的结果可以用 `macroexpand-1` 确认。编写宏时，先看展开结果是最快的办法。

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. 展开结果也要经过类型检查

宏返回的表达式和手写的表达式一样接受类型检查。

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

错误的位置是调用宏的地方。

`if` 的为假分支不能省略、`if` 两边的类型必须一致，这些规则同样适用于展开结果。上面的 `my-unless` 在最后写成
`(progn ,@body ())`，是为了无论函数体最后一个表达式是什么类型，都能让 `if` 两边统一为 `()` 类型。

## 4. 变量名冲突与 `gensym`

直接写一个交换两个变量值的宏，会是这样：

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

大多数情况下能工作，但调用方的变量恰好叫 `tmp` 时就会出错。

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   （没有交换）
```

展开后是 `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`，宏创建的 `tmp` 遮蔽了调用方的 `tmp`。

为避免这种情况，用 `gensym` 生成宏内部使用的变量名。`gensym` 返回一个在程序任何地方都写不出来的新符号。

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

与 Common Lisp 一样，typelisp 的宏不会自动防止名字冲突（非卫生宏）。请记住：**宏创建的绑定要用 `gensym`**。

按同样的思路，指定次数重复执行函数体的宏可以这样写：

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. 根据参数的形状改变展开

宏的函数体是普通的 typelisp 代码，所以可以用 `if` 或 `match` 检查参数，改变要展开的表达式。参数的类型是
S 表达式数据（`Option<Sexpr>`），空列表是 `none`。

创建一个所有条件都为真时返回 `true` 的 `my-and`。

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; 没有参数
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; 只有一个
         `(if ,f (my-and ,@more) false)))             ; 两个以上
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` 是把列表开头取到 `f`、其余取到 `more` 的模式。
- `sexpr-null` 判断 S 表达式数据是否为空列表。
- 最后的 `_` 分支是必需的，因为 S 表达式数据还有列表以外的形状（数字、字符串等），`match` 要求覆盖它们。
  `&rest` 参数一定是列表，所以这个分支实际上不会执行。
- 宏可以在展开结果中调用自身。展开会反复进行，直到没有宏调用为止。

## 6. 可省略的参数

用 `&optional` 可以接收可省略的参数，也可以写默认值。

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

用 `&key` 也可以接收关键字参数（[语法参考 3.14](../reference/syntax.md#314-defmacro--宏定义)）。

## 7. `macrolet`：只在局部使用的宏

只在一个表达式中使用的宏可以用 `macrolet` 定义，外部看不见。

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. 使用宏时的注意事项

- **宏只能在定义之后调用。** 与函数一样，请在文件靠前的位置定义。
- 供其他模块使用的宏用 `(pub defmacro ...)` 公开。
- `when`、`unless`、`cond`、`and`、`or`、`dotimes` 等许多标准语法也是作为宏定义的。可以用
  `(macroexpand '(when true 1))` 查看其内容。
- 能用函数写的请用函数写。宏不能作为值传递，而且不读展开结果就不清楚它做了什么。

## 9. 接下来阅读

- [错误处理](errors.md)：`Result`、`panic`、`catch` / `throw`
- [宏相关函数](../reference/functions/system.md#8-宏)：`gensym`、`macroexpand` 等
