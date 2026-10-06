<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 时间、运行环境与实现

时间、运行环境的查询、实现工具、文本的解析与求值、文档字符串，以及宏相关的函数。

## 1. 时间

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `universal-time` | — | `defstruct` | `day`（自 1900-01-01 起的天数）和 `second`（当天内的秒数，0..86399）两个字段 |
| `internal-time` | — | `defstruct` | `second` 和 `microsecond`（该秒内，0..999999）两个字段 |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | 自 CL 纪元（1900-01-01 UTC）起的时间 |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | 以进程为基准的经过时间 |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | 本进程使用的 **CPU 时间**（用户＋系统） |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | 换算为秒数。报告两次读数之差时的形式 |
| `internal-time-units-per-second` | — | `int` | `1000000`（微秒），即 `microsecond` 字段的单位。与 CL 一样，值由实现选择 |
| `time` | `(time form)` | 宏 | 执行 `form`，各用一行打印实际时间和 CPU 时间，并原样返回 `form` 的值 |

实际时间和 CPU 时间说明的是不同的事情。以 I/O 等待为主的处理两者相差很大，而这个差正是想知道的信息，所以 `time` 两者都输出。

让任务停下的 `sleep` 见[任务与通道](concurrency.md#3-yield--sleep--让出)。

## 2. 日期的分解与合成

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone` 共 **9 个字段**。把 CL 的 9 个返回值合为一个结构体（因为没有多值） |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | 把世界时分解为日历成分。`zone` 是格林尼治以西的小时数（与 CL 方向相同）。**省略时为本地时间**（与 CL 相同） |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | 反方向。省略 `zone` 时参数按**本地时间**解读 |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | 以本地时间分解的现在 |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | 该世界时下本地时间位于格林尼治以西的**秒**数 |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | 该世界时是否实行夏令时 |

与 CL 一样，`day-of-week` 中 **0 是星期一，6 是星期日**。

**省略 zone 时为本地时间**——与 CL 相同。本地时间的偏移向 OS 询问，所以结果取决于机器所在的地区。**给出显式的 zone 结果就是
确定的**，`0` 是 UTC。

`zone` 的单位与 CL 相同，是"格林尼治以西的**小时**数"，UTC+9 读作 `-9`。不过 **参数是整数，结果的 `zone` 字段是 `f64`**。
实际的偏移不一定是整小时（印度是 +5:30，尼泊尔是 +5:45），把报告值舍入就会悄悄说谎。手写的 zone 是整小时，所以参数一侧是 `int`。

显式给出 `zone` 时，按 CL 的规定 `daylight-p` 为 `false`，`zone` 就是传入的值本身
（*If a time-zone is supplied, daylight saving time information is ignored*）。

处于夏令时切换期间的本地时间本来就不唯一，CL 也没有规定取哪一个。`encode-universal-time` 对这样的时间也返回其中一个答案。

## 3. 运行环境

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | 命令行。**第 0 个元素是程序名** |
| `getenv` | `(getenv name)` | `string→Option<string>` | 环境变量。未设置或不是 UTF-8 时为 `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`。`user-homedir-pathname`（[路径名](streams-files.md#92-函数)）的基础 |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | 实现的版本号 |
| `machine-type` | `(machine-type)` | `()→string` | CPU 架构（`x86_64` / `aarch64` …）。是**构建目标**的值 |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | 主机名 |
| `machine-version` | `(machine-version)` | `()→Option<string>` | **正在运行**的硬件名（`Apple M1` / `Intel(R) Xeon(R) …`）。无法得知的环境中为 `none` |
| `software-type` | `(software-type)` | `()→string` | OS（`macos` / `linux` …） |
| `software-version` | `(software-version)` | `()→Option<string>` | OS 的发行版本（`uname -r`，例如 `24.6.0`） |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | 安装地点的简称。**总是 `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | 同样，全称。**总是 `none`** |

返回 `Option` 的是 CL 允许 `NIL` 的项目（*or nil if no such name can be determined*）。POSIX 没有记录站点名的地方，所以总是
`none`——SBCL 也返回同样的结果。注意 `machine-type` 与 `machine-version` 的区别：前者是这个二进制**被构建**时的架构，后者是
现在**正在运行**的芯片。

`command-line-args` 的第 0 个元素，对 `typl script.typl a b` 是脚本的路径，对 AOT 可执行文件 `./prog a b` 是可执行文件自身。
**无论哪种运行方式，都能用同样的下标读到同样的参数**（`typl` 会去掉自己的名字和 `--heap-cells` 等选项再传入）。

## 4. 询问用户

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | 用一个字符接受 `y` / `n`。在接受之前反复询问 |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | 要求拼出 `yes` / `no`。用于答错代价高的问题 |

两者都从 `*standard-input*` 读取。只有输入结束才会停止重复询问，此时结果为 `false`。

## 5. 实现工具（CLHS 25.2）

实现回答关于自身问题的一层。`heap-info` / `room` / `dribble` 是普通函数，`trace` / `untrace` / `step` / `disassemble` / `ed`
是**特殊形式**（`trace` / `untrace` / `disassemble` / `ed` 接受定义的*名字*，`step` 接受*形式*，都不求值）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | 以结构体给出堆的现状。与 `room` 打印的是同样的数 |
| `room` | `(room &optional verbose)` | `(bool)→()` | 把 `heap-info` 报告到 `*standard-output*`。`(room true)` 更详细 |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | 开始把会话的输出记录到 `path`／不带参数时结束记录 |
| `trace` | `(trace name...)` | `Sexpr` | 把所列定义的调用报告到 `*trace-output*`。返回当前正在 trace 的名字一览 |
| `untrace` | `(untrace name...)` | `Sexpr` | 停止报告。**不带参数时全部解除** |
| `step` | `(step form)` | `form` 的类型 | 求值 `form`，在每次调用时停下询问 |
| `disassemble` | `(disassemble name [llvm])` | `()` | 打印该定义变成了什么。默认是主机的机器码，`true` 时为 LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | 启动 `$VISUAL`／`$EDITOR`。传入名字时打开写有该定义的行 |

`trace`/`untrace`/`step`/`disassemble` 是解释器专用的，调用它们的函数不能编译（[语法参考第 10 章](../syntax.md#10-编译)）。

### 5.1 `heap-info` 的字段

| 字段 | 类型 | 内容 |
|---|---|---|
| `capacity` / `live` / `free` | `int` | cons 区域整体及其细分。3 者总是满足 `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | 堆所持有的其他 3 种对象的当前数量 |
| `gc-count` | `int` | 自实现启动以来的回收次数 |
| `growable` | `bool` | 区域是否还能增长 |

除 `growable` 以外的字段都是 `int`。不报告增长的上限（见 `typl --heap-cells` 的说明）。读者想知道的是能否增长（`growable`）。

### 5.2 `trace` / `step` 能看到和看不到的东西

- **从解释执行中的调用处，也能看到拥有编译后函数体的定义。**
- **编译后代码*内部*的调用处看不到。** trace 拥有编译后函数体的名字时，会附上一行说明这一点。与 SBCL 对本地调用所说的限制相同。
- **经由闭包值的调用（`funcall`/`apply`）看不到。** 闭包没有名字。
- **泛型定义不在对象之内。** 每种类型的实体在每个使用处生成，没有可以指名的单一函数体（与 `compile` 拒绝的理由、措辞相同）。

`step` 的命令是 `s`（进入这次调用；空行也一样）、`n`（跳过这次调用）、`c`（之后不再询问）、`q`（中止）。**标准输入不是终端时，
`step` 只是求值 `form`**——这是 CLHS 明确允许的退化，以免脚本或测试在无法回答的提示处卡住。

`ed` 的 `$VISUAL`／`$EDITOR` 按空白分割，所以也可以写 `EDITOR="code -w"`。两者都未设置时为 `Err`——不会猜测 `vi`。行号以
`+N` 的形式放在最前面传入。

`dribble` 记录会话输出离开进程的全部 3 条途径：`print`/`println`/`format` 写出的内容、写到连接标准输出的流的内容，以及在 REPL
中输入的行和 REPL 打印回来的值。

## 6. 解析与求值

这些都处理运行时的（程序本身无法控制的）文本和数据，所以失败时不 panic，而是返回 `Result` 的 `Err`。错误类型是每种操作各自的
具体类型（[错误类型](option-result.md#3-错误类型与-error-trait)）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL 的 `parse-integer`。跳过前后空白（与 `trim` 相同的集合），读取至多一个符号 `+`/`-`，再读取 `radix` 进制（默认 10，2〜36。10 以上的数字大小写均可）的数字。位数没有上限（`int`）。剩下其他字符时为 `Err`。`:junk-allowed true` 时在第一个非数字处停止并忽略其余——但一个数字都没有时为 `Err`（相当于 CL 的 `nil`）。不返回 CL 的第 2 个值（读取结束位置）。超出范围的 `radix` 会 panic（是调用方的错误，不是文本的） |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 浮点数。也接受 `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | 从 `s` 读取一个 `Sexpr`（与读取源代码的是同一个读取器）。括号不完整、字符串未结束等为 `Err`。从流读取用 `read-sexpr`（[流](streams-files.md#6-泛型函数与文件操作)） |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | 在 `read` 的基础上附上**读取结束的位置**。`(car r)` 是值，`(cdr r)` 是下一个要读的字符位置。`start` 省略时为 0 |
| `read-from-string-preserving-whitespace` | 同上 | 同上 | 同上，但不消耗结束 datum 的空白。区别体现在返回的位置上 |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 在运行时对 `form` 进行类型检查并求值。遵循 CL 的 `eval` |

CL 从 `read-from-string` 返回**2 个值**（值与位置），但本语言没有多值，所以返回一个 `cons-cell`。有了位置，逐个 datum 读取字符串
就成为循环而不是重新扫描：

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace` 的区别只是**一个空白字符**——CL 的 `read` 消耗结束 datum 的空白，`read-preserving-whitespace` 保留它。
`(read-from-string "12 34")` 返回位置 3，preserving 版本返回 2。

读取器接受的数值记法见[语法参考第 1 章](../syntax.md#1-词法元素)。`*print-radix*`（[打印](printing.md#62-基数大小写与可读回)）
打印的记法可以原样读回。没有 CL 的 `*read-base*`。

### 6.1 `eval` 的含义

遵循 CLHS 的 `eval`：在**当前的全局环境**（全局的函数、变量、类型、宏，也包括运行时添加的定义）以及**空的词法环境**（看不到调用方
`let`/`lambda` 的局部绑定）中求值。表达式和定义（`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`）都可以求值，定义会立即且
永久地注册到全局环境。

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; 能看到全局的 x
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 返回定义的名字
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 能看到刚才的定义
```

- **返回值**：表达式时以 `Option<Sexpr>` 返回求值结果，定义时返回所定义名字的符号（与 CL 相同）。要使用结果，用 `match`
  （`(int n)`/`(str s)`/…）拆解 `Sexpr`。
- **由静态类型带来的区别（重要）**：CL 返回结果的实际值，但本语言只能把返回类型统一为 `Result<Option<Sexpr>,EvalError>`。另外，
  **静态书写的代码不能前向引用 `eval` 在运行时定义的名字**——直接写在文件中的 `(sq 9)` 会在定义 `sq` 的 `eval` 运行之前被检查，
  成为"未定义"。不过**之后的 `eval` 能看到它**（那个 `eval` 的类型检查在运行时、定义之后进行）。REPL 逐行检查和执行，所以用
  `eval` 定义的名字可以从下一行直接调用。
- **错误的处理**：类型错误、语法错误返回 `Err`（不会 panic）。求值的代码中的**运行时 panic**（除以零等）与直接书写的代码一样原样
  传播。中途的 `unwind-protect` 的 cleanup 会执行（[语法参考第 8 章](../syntax.md#8-非局部退出catch--throw--unwind-protect)）。
- **命名空间**：在 `typl file.typl` 的运行中以及 AOT 可执行文件中，`eval` 在该脚本模块的命名空间中求值（能看到脚本自身的全局
  变量）。REPL 在根命名空间中求值。
- **编译**：`read` 和 `eval` 都可以编译。在 AOT 可执行文件中的处理及其后果（eval 的形式被解释执行）见
  [语法参考 10.2](../syntax.md#102-aot-可执行文件中的-eval)。

## 7. 文档字符串 / `documentation`

`defun`/`defmethod`（包括 `impl` 内的）/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/`deftype`/`deftrait` 可以
拥有文档字符串。位置遵循 CL 各自的规则：

| 形式 | 文档字符串的位置 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 函数体的开头（返回值类型、`where` 子句之后）。仅当其后至少还有一个函数体形式时——单独的字符串仍是返回值 |
| `defvar` / `defconstant` | 初始值的**后面**：`(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 名字的**紧后**，字段／变体列表之前 |
| `deftype` | 名字的**紧后**，类型之前：`(deftype meters "doc" i32)` |
| `deftrait` | 继承列表之后、项目列表之前。整个 trait 一个。**有默认实现的方法**可以在其函数体之前放置自己的文档字符串 |

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `documentation` | `(documentation name)` | （特殊形式。`name` 是裸符号或 `Type::method`）→`Option<string>` | 返回 `name` 的文档字符串 |

`documentation` 与 `quote`/`compile` 一样是特殊形式（不求值 `name`，作为未求值的名字读取）。与 CL 的
`(documentation 'name 'function)` 不同，它不接受类型参数，而是按**变量→函数→类型→trait→宏**的顺序（与作为表达式求值时裸标识符
的优先级相同）解析裸名字。`Type::method` 的形式查找关联方法／静态方法的文档字符串。

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**值在检查时确定**：名字无法解析到任何定义时是检查时的错误（与引用未定义的变量等相同）。能解析但没有文档字符串时为 `Option::none`。

**不在对象之内**：

- 没有 `(setf documentation)`（在运行时改写文档字符串）。
- 不支持带模块限定的自由名（`mod::name`。`Type::method` 支持）。
- `deftrait` 中**没有函数体**的方法声明不能拥有文档字符串。末尾的字符串字面量本身会成为默认实现的函数体（＝返回值），无法区分两者。

语言服务器（`typl-lsp`）的悬停中也会显示文档字符串。

## 8. 宏

宏的定义方法见[语法参考 3.14](../syntax.md#314-defmacro--宏定义)。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | 新的符号。名字是 `" <prefix><n>"`，`n` 是 `*gensym-counter*`。开头的空格在源代码中写不出来，所以生成的绑定不会与写出的名字冲突 |
| `*gensym-counter*` | 变量 | `int` | `gensym` 下次使用的编号。与 CL 一样可以读取和设置 |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 把宏调用展开一层。`none` 表示"不是宏调用" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 反复展开，直到不再是宏 |

`macroexpand-1` 返回的是 `Option`——CL 用第 2 个返回值传达"是否展开了"，但没有多值，所以由 `none` 承担这一点。**展开为自身调用
的宏与非宏不可能被混淆。** 展开的一层与类型检查使用的是同一个，程序看到的与检查看到的不会出现偏差。

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none 作为空列表输出（Option<Sexpr> 是透明的）
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL 中有而这里没有的：`eval-when`（`:compile-toplevel`/`:load-toplevel`/`:execute` 总是一致，没有可选择的区别）、
`define-compiler-macro`、`load-time-value`、`make-symbol`/`copy-symbol`/`gentemp`（未 intern 的符号。绑定按名字查找，所以没有
什么好处）。

## 9. 局部宏绑定（`macrolet` / `symbol-macrolet`）

两者都是对**不是值的名字**进行词法绑定的特殊形式。运行时什么也不留下——被编译的是函数体展开后的形式。

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- `macrolet` 的绑定**只在函数体期间**遮蔽同名的全局宏。lambda 列表与 `defmacro` 相同（`&optional`/`&rest`/`&key`）。
- **同一个 `macrolet` 的兄弟之间，从彼此的*函数体*中看不到对方**（与 CL 相同。这是与 `labels` 的区别）。展开结果在使用处检查，
  所以 `earlier` 展开为 `(later ...)` 是可以的——在那个位置两者都可见。
- `symbol-macrolet` 的名字作为普通绑定进入环境。所以内层的 `let` 会遮蔽同名，外层的变量会被遮蔽——CL 的规则原样体现。
- **`setf` 写到展开目标。** `(setf head 42)` 就是 `(setf (get v 0) 42)`。
- 展开在**使用处的环境**中检查（而不是绑定处）。

## 10. 其他

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | 为假时 panic。省略消息时为 `assertion failed: <测试的原文>`（因为是宏，所以能指出表达式本身）。没有 CL 的 restart |
| `warn` | `(warn control args...)` | `(string,...)→()` | 向 `*error-output*` 写一行带 `WARNING: ` 的内容并**继续**。既不返回 `Result` 也不结束程序的报告手段 |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | 只在 `body` 期间替换全局变量，退出时恢复。CL 把它写成 `let`，但本语言的 `let` 总是词法绑定，所以用别的名字（与 Emacs Lisp 同名宏的作用相同）。无论以正常结束、`throw`、panic、`break`/`return` 中哪种方式退出都会恢复。**不是按任务的绑定** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | 把所有打印控制变量设为标准值、把 `*read-eval*` 设为 `true`，然后执行 `body`（[打印](printing.md#6-控制打印量)） |
| `exit` | `(exit code)` | `int→!` | 结束进程 |
| `dump` | `(dump path)` | `string→bool` | 把当前环境（类型信息＋编译后的函数体）写到一个文件。可以用 `typl --image <path>` 重新启动。解释器专用（[语法参考 10.1](../syntax.md#101-转储)） |
