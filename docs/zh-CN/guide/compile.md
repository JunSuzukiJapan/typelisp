<!-- translated-from: docs/ja/guide/compile.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 编译

如果不做任何处理，typelisp 程序由解释器执行。除此之外，还有两种编译为本机代码的方法，以及一种保存环境的方法。
规范的详情见[语法参考第 10 章](../reference/syntax.md#10-编译)。

| 方法 | 用法 | 结果 |
|---|---|---|
| JIT 编译 | `(compile name)` | 正在运行的会话中的函数被替换为本机代码 |
| AOT 编译 | `typl -c src.typl` 或 `(compile-file "src.typl" "out")` | 得到可独立运行的可执行文件 |
| 转储 | `(dump "file.typld")` | 保存定义，可以用 `typl --image` 从同样的环境启动 |

## 1. 准备

编译使用 LLVM 22。如果已经按照 [README.md](../../../README.md) 的步骤构建了 `typl`，就不需要额外准备。

AOT 编译生成的可执行文件会链接静态库 `libtypelisp_front.a`。发布版构建的 `typl`（包括用 `cargo install` 安装的）
内部带有这个库，所以不需要准备。首次编译时，它会写出到 `~/.typelisp/lib/<构建ID>/`（如果设置了环境变量
`TYPELISP_HOME`，则为 `$TYPELISP_HOME/lib/<构建ID>/`），之后使用写出的那一份。可以用 `typl --remove-lib` 删除
（加 `--others` 删除其他版本的 `typl` 写出的，加 `--all` 删除全部）。调试版构建的 `typl` 使用构建它的仓库中
`target/debug/` 下的库。要使用放在其他位置的库，在启动 `typl` 时用 `--lib-dir` 指定其所在文件夹（3.2 节）。
在 macOS 上链接需要 Xcode Command Line Tools。

## 2. JIT 编译

把已经定义的函数当场编译为本机代码。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; 之后的调用运行编译后的代码
```

- `name` 不会被求值。直接写函数名（不是字符串）。方法要像 `(compile point::norm)` 这样加上类型名。
- 它调用的函数也会一起编译。
- **泛型函数不能编译。** 因为每种类型的实体是在每个使用处生成的。请编译以具体类型调用它的那个函数。
- `trace`、`step`、`disassemble`、`compile`、`compile-file`、`dump` 是解释器的操作，调用它们的函数不能编译。
  尝试编译时会得到说明原因的错误。

查看编译结果使用 `disassemble`。

```lisp
(disassemble fib)          ; 主机的机器码
(disassemble fib true)     ; LLVM IR
```

## 3. 用 AOT 编译生成可执行文件

### 3.1 编写程序

作为入口，定义一个**不接受参数的 `main` 函数**。

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

文件末尾的 `(main)` 是为了在用 `typl hello.typl` 运行时调用 `main`。`compile-file` 会跳过这个末尾的 `(main)`，
所以同一个文件既可以用解释器运行，也可以用于 AOT。

### 3.2 编译

在命令行中使用 `typl -c`（`typl --compile` 相同）。

```sh
$ typl -c hello.typl            # 生成 hello
$ typl -c hello.typl -o fib     # 把可执行文件命名为 fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

省略 `-o` 时，会在源文件所在文件夹生成以源文件名去掉 `.typl` 命名的可执行文件。源文件名不以 `.typl` 结尾时需要
`-o`。使用 `-c`（`--compile`）时，不能指定 `--image`、`--heap-cells`、`--feature`。

在 REPL 或程序中调用 `compile-file` 也能做同样的事。

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

需要反复构建时，可以把这一行写进文件，用 `typl build.typl` 运行。

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

文件名从**启动 `typl` 时的当前目录**解析，而不是从 `build.typl` 所在位置。

要链接放在 `typl` 所用位置以外的 `libtypelisp_front.a` 时，用 `--lib-dir` 指定其所在文件夹。对 `typl -c` 和
`compile-file` 都有效。

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

指定的文件夹中没有 `libtypelisp_front.a` 时，会报错停止。这个文件只能与同时构建的 `typl` 一起使用。重新构建
`typl` 后，请重新复制。

### 3.3 可以进行 AOT 编译的文件形式

- 入口文件的顶层只能写定义（`defun` `defmethod` `defvar` `defparameter` `defconstant` `defmacro`
  `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`、`(unsafe (def-c-struct ...))`）
  以及 `use` `module`。除了末尾的 `(main)`，不能写 `(println ...)` 这样的顶层表达式。处理请写在 `main` 中。
- 没有不带参数的 `main` 时会报错。
- 被 `use` 的模块文件也会一起编译，合并成一个可执行文件。
- `defffi` 的 `:library` 中指定的库会自动链接（[C FFI](ffi.md)）。
- 标准库的所有函数都可以在 AOT 中使用。`eval` 也可以使用，但这时类型检查器和解释器会进入可执行文件，使文件变大，
  启动也更费时间。不调用 `eval` 的程序不会包含它们。

### 3.4 可执行文件的行为

- 无论用 `typl hello.typl a b` 运行还是用 `./hello a b` 运行，`(command-line-args)` 都返回相同形式的
  `Vector<string>`。第一个元素是程序名。
- 退出码用 `(exit n)` 指定。`main` 正常返回时为 0。
- 发生 panic 时显示消息，并以非 0 退出。

## 4. 转储

可以把当前会话的定义保存到一个文件，下次从它启动。

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

也可以像 `typl --image session.typld prog.typl` 这样用于运行文件。

- 保存的是**定义**。在会话中求值过的表达式不会保存。
- `compile` 过的函数以编译后的形式保存。
- 全局变量恢复为**重新执行初始化表达式得到的值**，而不是保存时的值。
- 与写出转储的 `typl` 版本不同的 `typl` 无法读入（会报错）。

运行文件并在其中 `(dump ...)` 时，该文件的定义位于以文件名命名的模块中。在 `dp.typl` 中定义的函数名为 `dp::sq`，
从其他文件调用需要 `pub`（[模块与文件结构](modules.md)）。

## 5. 关于编译后的模块文件

没有 Common Lisp 的 `.fasl` 那样把每个模块的编译结果写入文件的格式。`compile-file` 直接从源代码生成可执行文件，
不会留下中间文件。
