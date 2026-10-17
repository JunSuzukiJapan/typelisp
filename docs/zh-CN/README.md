<!-- translated-from: docs/ja/README.md @ 7d830ae3902dd3c581c7cf6fa5365b91b34106da -->
# typelisp 文档（简体中文）

typelisp 是一种静态类型的 Lisp。安装和构建方法请参阅仓库根目录的 [README.md](../../README.md)（英文）。

## 特性

### 任务与通道

启动任务（轻量级线程），通过通道在任务之间传递值。详见[并发](tutorial/concurrency.md)。

```lisp
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### 用 Option 和 Result 代替 nil

没有 `nil`。可能没有值时返回 `Option<T>`，可能失败时返回 `Result<T,E>`，再用 `match` 取出其中的内容。如果把 `Option<string>` 当作 `string` 使用，或者在 `match` 中漏写 `none` 的情况，程序运行之前就会出现类型错误。详见[类型基础](tutorial/types.md)和[错误处理](tutorial/errors.md)。

```lisp
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### 经过类型检查的宏

宏的写法与 Common Lisp 相同，使用 `defmacro`、准引用和 `gensym`。展开后的代码也会像普通代码一样接受类型检查，类型不符时会在调用宏的位置报错。详见[宏](tutorial/macros.md)。

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

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

设置步骤见[编辑器集成指南](guide/editors.md)。各编辑器的按键和设置列表见以下文档：

- [Emacs（typelisp-mode）](../../editor/emacs/README_zh-CN.md)
- [VS Code](../../editor/vscode/README_zh-CN.md)
