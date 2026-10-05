<!-- translated-from: docs/ja/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# typelisp 文档（简体中文）

typelisp 是一种静态类型的 Lisp。安装和构建方法请参阅仓库根目录的 [README.md](../../README.md)（英文）。

## 教程

初次接触 typelisp 的读者请按顺序阅读。

- [入门](tutorial/intro.md)：REPL、函数、变量、条件分支、循环、列表与 `Vector`
- [类型基础](tutorial/types.md)：静态类型、`Option`、`Result`、结构体、枚举、泛型
- [trait](tutorial/traits.md)：`deftrait` / `impl`、trait 约束、`:dyn`
- [宏](tutorial/macros.md)：`defmacro`、准引用、`gensym`、`macrolet`
- [错误处理](tutorial/errors.md)：`Result`、`panic`、`catch` / `throw`、`unwind-protect`
- [并发](tutorial/concurrency.md)：任务、通道、`select`、`Mutex`、`thread`

## 指南

- [模块与文件结构](guide/modules.md)：`use`、`pub`、文件与模块的对应关系
- [编译](guide/compile.md)：JIT、用 AOT 编译生成可执行文件、转储
- [文件 I/O、流与网络](guide/io.md)：文件、路径名、TCP / TLS / UDP、名称解析
- [C FFI](guide/ffi.md)：用 `defffi` 调用 C 函数（包括回调和用 `def-c-struct` 定义的 C 结构体）
- [编辑器集成](guide/editors.md)：`typl-lsp` 以及 VS Code / Emacs 的设置
- [写给 Common Lisp 用户](guide/from-common-lisp.md)：与 CL 的区别以及改写方法

## 参考

- [语法参考](reference/syntax.md)：词法、类型的写法、定义、控制结构、编译、并发
- [内置函数](reference/functions/README.md)：内置函数、方法与标准库
- [类型一览](reference/types.md)：各种类型以及它们实现的 trait
- [错误消息](reference/errors.md)：常见错误的含义与修正方法

## 编辑器集成

设置步骤见[编辑器集成指南](guide/editors.md)。各编辑器的按键和设置列表见以下文档（日文）：

- [Emacs（typelisp-mode）](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
