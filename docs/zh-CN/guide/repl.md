<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

不带参数启动 `typl` 就进入 REPL（交互模式）。本文说明 REPL 除了求值表达式之外还能做什么：REPL
的命令、输入辅助、引用之前的结果、中断计算，以及出错后的排查。

## 启动与退出

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

用 `:quit`（简写 `:q`）或 `Ctrl+D` 退出。输入历史保存在 `~/.typl_history` 中，下次启动时也能用
`Ctrl+P` / `Ctrl+N` 或 `Ctrl+R`（搜索历史）调出。

启动后，在第一个提示符之前按顺序执行以下内容。它们都不显示执行得到的值。

1. 如果存在 `~/.typlrc`，执行其内容。加上 `--no-init` 则不读取。
2. 命令行中的 `--eval FORM` 和 `--load FILE`，按书写顺序执行。两者都可以写任意多次。

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`、`--load` 和 `--no-init` 只用于启动 REPL。运行文件时加上它们会报错。

启动时的模块是 `user`。用 `(in-module foo)` 切换到其他模块后，提示符会显示该模块。

```
typl> (in-module foo)
typl user::foo>
```

## REPL 命令

写在行首的 `:名字` 是 REPL 命令。`:help` 显示命令列表。列表中没有的 `:名字` 不是命令，而是作为
普通关键字求值。

| 命令 | 含义 |
|---|---|
| `:help`（`:h`、`:?`） | 显示命令列表 |
| `:type FORM`（`:t`） | 不求值，只显示 FORM 的类型 |
| `:describe NAME`（`:d`） | 显示定义的种类、类型、字段或变体、方法、实现的 trait、定义位置和文档字符串 |
| `:apropos TEXT` | 在所有模块中查找名字包含 TEXT 的定义（不区分大小写） |
| `:src NAME` | 显示定义的源代码 |
| `:who-calls NAME` | 显示调用函数或方法 NAME 的函数和方法 |
| `:inspect FORM`（`:i`） | 对 FORM 求值，并按类型展开其值 |
| `:backtrace`（`:bt`） | 从内向外依次显示上一行失败时正在进行的调用 |
| `:locals [N]` | 显示 `:backtrace` 中第 N 个（省略时为第 0 个）调用的参数 |
| `:cd [DIR]` | 把当前文件夹切换到 DIR（省略时为主文件夹） |
| `:pwd` | 显示当前文件夹 |
| `:quit`（`:exit`、`:q`） | 退出 REPL |

NAME 除了 `inner` 这样的名字，也可以写 `std::seq::reverse` 或 `Option::unwrap` 这样的路径。

```
typl> (defun inner ((x int) (s string)) int "给 x 加 1。" (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

给 x 加 1。
typl> :src inner
(defun inner ((x int) (s string)) int "给 x 加 1。" (+ x 1))
```

对于从文件加载的定义，`:src` 从该文件中显示；对于在 REPL 中输入的定义，则从输入的文本中显示。
标准库中的定义没有记录源代码位置，因此无法显示。

`:who-calls` 查找的是当前会话中函数的函数体。对于泛型函数，只有以具体类型使用过的部分才会被找到。

`:cd` 会改变 `(load ...)` 查找的文件夹，但 `use` 查找文件的文件夹仍是启动时的文件夹。

## 展开值（`:inspect`）

`:inspect` 按类型展开值。结构体展开为字段，枚举（包括 `Option`）展开为其变体的字段，`Vector`
展开为元素，`HashTable` 展开为键和值。输入编号进入该部分，`u` 返回上一层，`q` 结束。

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

列表和 `Sexpr` 不展开，按原样显示。

## 输入辅助

在终端中输入时，可以使用以下功能。

- 用 `Tab` 补全。补全的对象是名字（定义，以及 `if`、`defun` 这样的语法）、包含 `::` 的路径、行首
  的 REPL 命令，在字符串中则是文件名。
- 在函数名后输入空格，会在行尾以淡色显示该函数的类型。
- 高亮与光标旁边的括号配对的括号。
- 形式尚未结束就换行时，下一行会按未闭合括号的数量缩进。

## 之前的结果（`:*`、`:**`、`:***`）

在 REPL 中，`:*` 是上一次显示的值，`:**` 是再前一个，`:***` 是更前一个。它们各自具有该值的类型。

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- 没有显示值的输入（如定义）和出错的输入不会推进结果。
- 结果还不够那么多时写 `:**` 或 `:***` 会报错。
- 加了引号的 `':*` 是普通关键字。
- 写在函数体中的 `:*` 一直指向定义时的结果。Common Lisp 的 `*` 读取的是调用函数时的值，而
  typelisp 为了确定类型，在定义时就确定下来。
- 在 REPL 之外（运行文件，或在 `~/.typlrc`、`--load`、`--eval` 中），`:*` 是普通关键字。

## 中断计算（`Ctrl+C`）

在求值过程中按 `Ctrl+C`，会中断该计算并回到提示符。之前定义的内容会保留。无论是在解释器中运行的
循环、编译后代码中的循环，还是在 `sleep` 或 `wait` 中等待，都能中断。正在进行的 `unwind-protect`
的清理会被执行。

```
typl> (loop)
^Cerror: interrupted
typl>
```

在输入过程中按下的 `Ctrl+C` 只会丢弃正在输入的行。

## 出错后的排查（`:backtrace`、`:locals`）

因错误停止后，用 `:backtrace`（`:bt`）从内向外依次显示当时正在进行的函数调用。`:locals N`
按声明的类型显示第 N 个调用的参数。

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- 对于在函数最后一个表达式中进行的调用（尾调用），不显示调用方的函数。正因如此，尾递归循环
  无论转多少圈，都只占用一次调用的空间。
- 不显示编译后的函数内部的调用，也不显示匿名函数（`lambda`）。
- `:locals` 只显示参数，不显示用 `let` 绑定的变量。
- 对下一行求值后，上一个错误的记录就会消失。
