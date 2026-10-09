<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 打印

`print`/`println`/`format`、单参数打印函数、pretty printer、`print-object`，以及控制打印的变量。格式指令一览见
[format.md](format.md)。对流的读写见[流与文件](streams-files.md)。

## 1. `print` / `println` / `format`

`print`/`println`/`format` 都是**解释格式指令（CL 的 `format` 指令）的特殊形式**。第 1 个参数（`format` 是第 2 个）
是**控制字符串**，各指令依次消耗其后的可变参数。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 展开控制字符串，不换行写到标准输出 |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 同上，末尾换行 |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL 的 `format`。返回展开后的字符串。`dest` 为 `true`（CL 的 `t`）时还会写到标准输出；为 `false`（CL 的 `nil`）时不写，只返回字符串 |
| `format`（写到流） | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` 不是 `bool` 时是 CL 的写到流。把展开的字符串写到该流。返回值是 `()`（相当于 CL 的 `nil`），不返回字符串 |

按 `dest` 的类型分为两种含义（属于哪一种是静态确定的）。写到流时，具体流类型、`:dyn CharOutput`、
`(where (CharOutput S))` 的类型变量都可以同样书写。既不是 `bool` 也不是流的 `dest` 是类型错误。

**控制字符串必须是字面量**（与 Rust 的 `format!` 同样的限制）。其中的指令决定取多少个参数、什么类型，所以运行时拼出的
字符串在检查时无法读取。限定为字面量后，**参数的个数和类型在检查时就会被检查**——`(println "~d" "x")` 和
`(println "~a ~a" 1)` 都是检查时的错误。拼错的指令、未闭合的 `~(`、没有任何参数能回应的 `~/name/` 也同样是检查时的
错误。检查规则见 [format.md](format.md#1-写法)。想输出拼出来的字符串时，用 `(format false ...)` 创建，再用
`(println "~a" s)` 打印。

可变参数以各自的类型包装进 `Sexpr` 后传递——`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`，以及用户定义
的 `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` 等都可以直接传递（`(println "~a" my-struct)` 可以直接工作）。

用 `typl file.typl` 运行脚本时**不会输出顶层表达式的值**，所以程序要自己写到标准输出就调用这些函数。
`print`/`println`/`format` 每次调用都会把输出送出（这样即使通过管道，也能在读取标准输入之前看到提示）。

**`Option<Sexpr>` 透明地打印。** S 表达式数据的类型是 `Option<Sexpr>`，所以 `(some x)` 的包装不会出现在打印中，内容
直接输出。空列表输出为 `()`。其他 `Option<T>` 打印为 `(some ...)` / `none`。结构体、枚举、`Vector` 中的 `Option<T>`
字段也一样。`(eval ...)` 的 `Result<Option<Sexpr>,…>` 输出为 `(ok 42)`，`none` 时为 `(ok ())`。

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 不输出，只取得字符串
  (println "~a" s))                   ; => id=42
```

## 2. 单参数打印函数

CLHS 22.1.3 的打印函数。不做格式展开，直接打印一个值。流可以省略（默认为 `*standard-output*`）。

| 名称 | 形式 | 说明 |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | 以可读回的形式（与 `~s` 相同）写出，返回 `x` |
| `princ` | `(princ x [stream])` | 以供人阅读的形式（与 `~a` 相同）写出，返回 `x` |
| `write` | `(write x [stream])` | `*print-escape*` 为真时为 `prin1`，为假时为 `princ`。返回 `x` |
| `prin1-to-string` | `(prin1-to-string x)` | 不写出，以字符串返回（`~s`） |
| `princ-to-string` | `(princ-to-string x)` | 同上（`~a`）。与 `to-string` 相同 |
| `write-to-string` | `(write-to-string x)` | 同上，遵循 `*print-escape*` |

`print`/`println` **不是**这些函数。它们是接受控制字符串的 `format` 的简写，与 CL 的 `print`（换行 → `prin1` → 空格）
是不同的工作，所以两者都保留各自的名字。结果是 **CL 的单参数 `print` 在本语言中没有对应写法**——请写 `prin1`。

它们是宏，因为 `format` 的可变参数不接受类型变量，类型必须在调用处确定。

## 3. 标准输入与标准流

**读取标准输入**不使用专门的函数，而是使用标准流 `*standard-input*` 上 `CharInput` 的方法——
`(read-line *standard-input*)` / `(read-char *standard-input*)` / `(read-all *standard-input*)`
（[流的方法](streams-files.md#2-方法)）。标准输出、标准错误同样有 `*standard-output*` / `*error-output*`，可以像
`(write-line *standard-output* s)` 这样写（`print`/`println`/`format` 是需要格式展开时的捷径，总是写到标准输出）。

## 4. pretty printer

相当于 CL 的 Lisp Pretty Printer（CLHS 22.2）。**按照逻辑块和条件换行的指定，折行超出行宽的输出。**

### 4.1 控制变量

可以赋值的全局变量。`setf` 之后对之后的所有打印生效。临时改变时使用 `dlet`（6.3）。

| 变量 | 类型 | 默认值 | 含义 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 为真时 `~a`/`~s`/`~w` 以及 pretty 指令进入美化路径 |
| `*print-right-margin*` | `int` | `80` | 右边距（列）。0 表示"没有边距＝不折行"。负值是打印错误 |
| `*print-miser-width*` | `int` | `0` | 进入 miser 样式的宽度。0 相当于 CL 的 `nil`（miser 无效）。负值是打印错误 |

`pprint` 系列和 `pprint-logical-block` 无论 `*print-pretty*` 如何总是美化输出（按 CL 的 `pprint` 的定义）。

### 4.2 现成布局（特殊形式）

与 `print` 一样是特殊形式，所以参数可以是任何类型。

| 名称 | 形式 | 说明 |
|---|---|---|
| `pprint` | `(pprint x)` | 以默认布局美化输出。遵循 CL **先输出换行**，末尾不输出 |
| `pprint-fill` | `(pprint-fill x)` | 一行尽量填满。不输出换行 |
| `pprint-linear` | `(pprint-linear x)` | 所有元素放不进一行时**每行一个元素**。不输出换行 |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 列宽的表格形式（默认 16）。不输出换行。负的 `colinc` 是错误 |

默认布局（`pprint` 以及 `*print-pretty*` 下的 `~a`）仿照 CL 默认的 `*print-pprint-dispatch*`，把 `(quote x)` 简写为
`'x`，并把 `defun`/`let`/`if`/`lambda` 等代码形式排版为"首部加规定个数的参数在第 1 行，其余主体缩进 2 列、每行一个"。
其他列表按填充方式排版。

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 自己构建逻辑块

| 名称 | 形式 | 说明 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 打开逻辑块的特殊形式。`obj` 是 `pprint-pop` 遍历的列表（不遍历时为 `()`）。`:prefix` 与 `:per-line-prefix` 互斥（与 CL 相同） |
| `pprint-newline` | `(pprint-newline kind)` | 条件换行。`kind` 为 `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 缩进。`kind` 为 `:block`（从块起点）/ `:current`（从当前列） |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | 制表。`kind` 为 `:line` / `:section` / `:line-relative` / `:section-relative`。`colnum` 和 `colinc` 非负（为负时是错误） |
| `pprint-pop` | `(pprint-pop)` | 从块的列表中取下一个元素（已用尽时为 `()`） |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | 列表是否已用尽 |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 已用尽时 `break` 出外层的 `loop`（宏） |

逻辑块不接受流参数——**打开的逻辑块是隐式的状态**。最外层的 `pprint-logical-block` 开始它，关闭时一次性排版并写到标准
输出。打开期间，`print`/`println`/`(format true ...)`/`pprint` 的输出也全部进入该块，所以**内容用普通的 `print` 书写，
只用 `pprint-newline` 等指定换行位置**——写出来与 CL 的代码几乎相同。

`pprint-exit-if-list-exhausted` 在 CL 中是从 `pprint-logical-block` 的非局部退出，在这里则是**从外层 `loop` 的 `break`**
（`pprint-logical-block` 不建立 `block`）。CL 一侧的惯用写法也总是写在 `loop` 之中，所以写起来没有区别。

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

条件换行的判定规则（CLHS `pprint-newline`）：

- `:mandatory` —— 总是折行。
- `:linear` —— 外层逻辑块放不进一行时折行。判定以块为单位，所以**同一块中的 `:linear` 会一起折行**（`pprint-linear`
  的"全部一行或每行一个元素"就是这个）。
- `:fill` —— 满足以下任一条件时折行：(a) 下一段放不进行的剩余部分，(b) 上一段没有放进一行，(c) 在 miser 样式下块放不进
  一行。
- `:miser` —— 只在 miser 样式（块的起始列距右边距在 `*print-miser-width*` 以内）时作为 `:linear` 起作用。

## 5. `print-object`（按类型的打印表示）

写下 `impl print-object <类型>`，`print`/`println`/`format`/`pprint` 就会用该实现打印该类型的值——**即使它嵌套在
列表中**。相当于 CL 的泛型函数 `print-object`（CLHS 22.1.4）。

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| 参数 | 含义 |
|---|---|
| `self` | 要打印的值 |
| `escape` | CL 的 `*print-escape*`。`~s`/`prin1`/`pprint` 时为 `true`（可读回的表示），`~a`/`princ` 时为 `false`（供人阅读）。不关心的实现可以忽略 |

返回的 `string` 直接流向输出。没有写 `impl` 的类型以内置表示（`#<point x: 1 y: 2>` 的形式）打印。

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   嵌套时也生效
```

它也与 pretty printer 组合（第 4 章）。`*print-pretty*` 为真时，包含实现所返回字符串的列表会在右边距处折行。

标准库类型的打印表示。CL 中也有的类型采用与 SBCL 相同的形式。REPL 显示结果时也使用与 `~s` 相同的表示。

| 类型 | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`、`#("a" "b")` | `#(1 2 3)`、`#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | 同左 |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>`（数字是实现内部的编号） | 同左 |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | 整数（CL 的 `get-universal-time` / `get-internal-real-time` 的值） | 同左 |
| 错误类型（`ParseIntError`、`SimpleError` 等） | `#<simpleerror "boom">` | 只有消息（`boom`） |
| `complex` | `#C(1.0 2.0)` | 同左 |
| `Array<T>` | `#2A((0 0) (0 0))` | 同左 |
| 流 | `#<file-stream for "file /tmp/a.txt" {7}>`、`#<string-output-stream {5}>`、`#<two-way-stream :input-stream … :output-stream …>` | 同左 |
| 套接字 | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`、`#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | 同左 |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>`（夏令时末尾加 `dst`） | 同左 |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | 同左 |
| `defstruct` 的类型 | `#<point x: 1 y: 2>`（字段名与值） | 同左（字段用 `~a`） |

规则：

- **注册是静态的。** `impl` 作为普通的方法定义进行类型检查，所以类型名的拼写错误和签名不符都是编译错误。
- **对泛型类型也生效。** `(impl print-object box<T> (where (print-object T)) ...)` 会按类型参数跳到不同的函数体——值
  记得包括自身类型参数在内的类型（`box<i32>`）。`Vector<T>` 这样的内置泛型类型也一样。
- **选择发生在打印时。** 哪个指令消耗哪个参数取决于控制字符串在运行时的内容，所以 `~a` 与 `~s` 的区别（即 `escape`）
  只有在打印的那一刻才知道。与 CLOS 中 `print-object` 方法"按类定义，打印时选择"相同。
- **重入时退回内置表示。** 实现中用 `(format false "~a" self)` 打印自身会无限递归，所以正在打印的值再次出现时退回到
  内置表示。判断依据是值的同一性而不是深度限制，所以不会妨碍正当的自引用结构的嵌套打印。
- **所有标量类型都实现了这个 trait。** 这是**为了用作约束**：`format` 的可变参数不能接受类型变量，所以泛型代码要表达
  "可以渲染未知类型的值"，只有这个约束（与 Rust 的 `T: Display` 形式相同）。`Array<T>` 的 `print-object` 就是一例。
- **类型参数不满足约束时，静默使用内置表示。** `(impl print-object Array<T> (where (print-object T)))` 对
  `Array<i32>` 生效，但对元素是没有写 `print-object` 的 `defstruct` 的 `Array` 不生效。仅仅创建数组就报错是说不通的，
  所以不报错。
- CL 的另一种机制 `set-pprint-dispatch` / `*print-pprint-dispatch*`（以类型说明符为键的运行时注册表）**没有采用**。
  它的注册没有检查，不适合静态类型语言。

## 6. 控制打印量

### 6.1 深度、长度与共享

CLHS 22.1.1 中决定"值要打印到什么程度"的控制变量。与 4.1 的 3 个变量一样是可以赋值的全局变量，无论 `*print-pretty*`
真假，都对 `print`/`println`/`format`/`pprint` 全部生效。

| 变量 | 类型 | 默认值 | 含义 |
|---|---|---|---|
| `*print-level*` | `int` | `0` | 嵌套到这个深度及以上的对象替换为 `#`。打印对象本身是深度 0。0 表示无限制 |
| `*print-length*` | `int` | `0` | 列表元素（以及 `defstruct`/`defenum` 值的字段）打印到这个个数，其余为 `...`。0 表示无限制 |
| `*print-circle*` | `bool` | `false` | 为真时在打印前扫描值，**给出现 2 次以上的对象加标签**。第一次出现为 `#n=…`，之后为 `#n#` |

CL 用 `nil` 表示"无限制"，但本语言没有 `nil`，所以与 `*print-right-margin*` 等一样，**0 表示无限制**。负值没有意义，
是打印错误。默认值都是"无限制／无标签"，与 CL 的初始值一致。

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**只有把 `*print-circle*` 设为真时才能打印循环结构。** 在为假（默认）的状态下打印指向自身的值，打印器会不断沿循环前进，
进程会崩溃——CL 也一样（CLHS 规定 `*print-circle*` 为假时打印循环结构的行为未定义）。

循环只能通过"用 `setf` 让 `defstruct` 的字段指向自身"来构造（`Sexpr` 单元创建后不能修改，所以 `'(1 2 3)` 这样的列表
不会循环）：

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a 指向 a 自身
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

标签**按每个打印对象从 1 重新编号**（与 CL 相同）。即使没有循环，同一个对象出现 2 次也会加上 `#1=`/`#1#`——这是把
"这两个是同一个对象"的信息保留在输出中的、符合 CL 规范的行为：

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

没有任何共享的值**完全不会出现标签**，所以即使保持这个变量为真运行日常代码，输出也不会改变。

### 6.2 基数、大小写与可读回

| 变量 | 类型 | 默认值 | 含义 |
|---|---|---|---|
| `*print-base*` | `int` | `10` | 打印整数（固定宽度和 `int`）的基数。2〜36 以外是**打印错误**（CL 也规定了范围） |
| `*print-radix*` | `bool` | `false` | 为真时加上基数标记。`#b`/`#o`/`#x`，其他为 `#NNr`，基数 10 时末尾加 `.`。标记在符号的**前面**（`#x-ff`） |
| `*print-case*` | `symbol` | `:downcase` | 符号名的大小写。`:upcase` / `:downcase` / `:capitalize`（与 CL 相同的写法）。其他符号是打印错误 |
| `*print-readably*` | `bool` | `false` | 为真时以可读回的形式打印。强制转义，并使 `*print-level*`/`*print-length*` 的截断失效 |
| `*print-lines*` | `int` | `0` | pretty printer 可以使用的行数。超出部分截去，末尾加上与 CL 相同的 `..`。0 表示无限制。负值是打印错误 |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string` 做 `prin1` 还是 `princ`。**只有这两个读取它** |
| `*print-array*` | `bool` | `true` | `Vector<T>` 和 `Array<T>` 是否显示内容。为真时使用 CL 的数组语法（`#(1 2 3)` / `#2A((1 2) (3 4))`），为假时只显示类型和形状 `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*` 加上的标记可以被读取器读回（[语法参考](../syntax.md#1-词法元素)的基数记法）。

**`*print-case*` 的默认值与 CL 不同的原因**：CL 的默认值是 `:upcase`，那是因为 CL 的读取器以大写存储符号名——也就是
"按存储的原样"的意思。这个读取器以小写存储，所以含义相同的默认值是 `:downcase`。

**`*print-readably*` 缺少的一半**：CL 对无法读回的值会发出 `print-not-readable`，但本语言没有可以发出的条件，也无法
对 `print-object` 可能以任何方式打印的用户类型判断能否读回。只有强制转义和覆盖截断。

**只有 `write` 读取 `*print-escape*` 的原因**：按照 CLHS，`~s`/`prin1`/`pprint` 把它绑定为真，`~a`/`princ` 绑定为假，
都只在各自调用期间有效。也就是说，在无人绑定的状态下读取它的只有 `write`/`write-to-string`。`print-object` 的实现应当
读取自己的 `escape` 参数，而不是这个全局变量——那个参数携带着指令所选择的值。

**CL 中有而这里没有的**：`*print-gensym*`（没有未 intern 的符号）。

### 6.3 临时替换

CL 用 `let` 绑定这些变量，但本语言的 `let` 是词法绑定，所以使用 `dlet`（[其他](system.md#10-其他)）：

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; 限制只对这一次生效
(with-standard-io-syntax (println "~a" x))   ; 全部恢复为标准值再打印
```

`with-standard-io-syntax` 把所有打印控制变量设为标准值、把 `*read-eval*` 设为 `true`，然后执行主体。
