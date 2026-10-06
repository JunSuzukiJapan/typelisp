<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode（Emacs）

用于编辑 typelisp 源代码（`.typl`）的 Emacs 主模式。
VS Code 版见 [../vscode/](../vscode/README_zh-CN.md)。两者使用相同的关键字表和相同的缩进规则，
这一点由 `cargo test --test editor_keyword_sync_test` 机械地检查（见本文末尾）。

## 功能

- 语法高亮
  - 特殊形式和控制结构（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系列等）
  - 定义名（`(defun NAME ...)` 的 `NAME` 作为函数名，`(defstruct NAME ...)` 作为类型名，
    `(defvar (NAME ...))` 作为变量名。带 `pub` 的 `(pub defun NAME ...)` 也一样）
  - 命名空间和声明关键字（`pub` `module` `use` `load` `impl` `where`）以及 lambda 列表标记
    （`&rest` `&optional` `&key`）
  - 内置函数（`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` 等）
  - 基本类型（包括 `bignum` / `ratio`）、内置类型、内置错误类型（`ParseIntError` 等）、
    `Capitalized` 的用户类型、trait 对象类型 `:dyn Trait`
  - **用户定义类型的使用处**（`defstruct`/`defenum`/`deftrait` 的名字通常是小写
    （`rect` `todo-item` `board`），`Capitalized` 规则捕捉不到）。
    连接 `typl-lsp` 时按服务器的 semantic tokens 着色（`eglot` 也可用，见下文）。
    未连接时退回到收集缓冲区内类型名的方式
  - 字面量（`true` `false`、数字字面量（十进制 / `0xff` / `1.5` / `1/3`）、
    字符字面量 `#\Space`、字符串、关键字 `:name`）
  - 字符串中的 `format` 控制指令（`~a` `~5,'0d` `~{...~}` 等）
  - CL 风格带耳罩的全局变量（`*print-pretty*` 等）
- 注释
  - 行注释 `;`
  - **可嵌套的**块注释 `#| ... |#`
- S 表达式导航和 Lisp 风格缩进
- 通过 `imenu` 提供定义列表（函数 / 方法 / 宏 / 类型 / trait / `impl` / 变量 / 模块）
- 调用 `typl` CLI 的命令（见下文）

## 按键绑定

| 按键 | 命令 | 作用 |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | 保存并用 `typl FILE` 运行（经由 `compile`，可跳转到错误行） |
| `C-c C-z` | `typelisp-repl` | 在 comint 缓冲区中启动 `typl` 的 REPL |

`typl` 的位置用 `typelisp-program`（默认 `"typl"`）指定。
诊断的格式是 `error: FILE:LINE:COL: ...`，`compilation-mode` 可以解析，
用 `next-error` / `C-x \`` 可以直接跳到对应位置。

## 安装

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` 文件会自动以 `typelisp-mode` 打开（已注册到 `auto-mode-alist`）。

使用 `use-package` 时：

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## 语言服务器（`typl-lsp`）

构建 `typl-lsp` 后，可以从 `eglot`（Emacs 29+ 内置）或 `lsp-mode` 使用。

```sh
cargo build --release --bin typl-lsp
```

使用 `eglot` 时：

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

使用 `lsp-mode` 时：

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

支持诊断（语法/类型错误和重定义警告，通过 `textDocument/publishDiagnostics` 通知）、hover、
跳转到定义（goto-definition）、补全（`:` 已注册为触发字符）以及 semantic tokens。
通过 `use` 跨文件的引用会被解析（向上查找项目根目录的 `typelisp.toml`，详见
[语法参考 3.11](../../docs/zh-CN/reference/syntax.md#311-文件与模块的对应多文件项目)）。
已打开的编辑器缓冲区中未保存的编辑，会立即反映到依赖文件和被依赖文件两方的诊断中。

### 类型名高亮（semantic tokens）

服务器通过 `textDocument/semanticTokens` 报告**检查器实际解析为类型名的位置**。
由于这不是文本匹配：

- 通过 `use` 从其他文件引入的类型也会着色（这是缓冲区内解析原理上达不到的范围）
- 与类型同名的**函数**的调用处不会着色（检查器在那里将其解析为函数，因此根本不会记录 token）

客户端方面：

- **`eglot`（Emacs 31 及以后）**：eglot 自己绘制（`eglot-semantic-tokens-mode`）。
  `typelisp-mode` 不介入
- **`eglot`（Emacs 30 及以前）**：这个版本的 eglot 不处理 semanticTokens。因此
  **`typelisp-mode` 自行发送请求并用 overlay 绘制**
  （`typelisp-semantic-tokens-mode`，eglot 连接时自动启用）
- **`lsp-mode`**：原生支持（把 `lsp-semantic-tokens-enable` 设为 `t`）。此时
  `typelisp-mode` 不介入

`scripts/emacs-semantic-smoke.el` 会真正通过 eglot 连接，检查在所用 Emacs 中负责绘制的一方。
无论哪种客户端，在服务器应答期间，缓冲区内解析的后备方式都会退让（以免两套规则给同一个缓冲区着色）。

| 设置 | 默认值 | 作用 |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | 使用 Emacs 30 及以前的 eglot 时，是否按服务器的 semantic tokens 着色 |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | 编辑后再次请求前的空闲秒数（应大于 `eglot-send-changes-idle-time`） |

## 备注

- typelisp 在读取时把符号转为小写，但高亮区分大小写，以便区分以大写字母开头的类型名。
- 缩进由专用的 `typelisp-indent-function` 查询 `typelisp-indent-specs`（关联列表）来决定。
  与 Emacs Lisp 同名的形式（`defun` `let` `if` …）也由本模式自己保存，是因为符号属性是
  **全局的**，为 typelisp 所做的设置会改变同一会话中其他 Lisp 缓冲区的缩进。
  而且 typelisp 的形式即使与 Emacs Lisp 同名，形状也不同——`(defun NAME (PARAMS) RETTYPE ...)`
  有 3 个头部元素，`if` 固定为必须有 `else` 的 3 个元素——所以值也无法共享。
  已确认 `examples/` 下的所有 `.typl` 文件用 `indent-region` 后一个字节也不变，并且把缩进全部
  去掉后重新缩进会恢复原样（VS Code 版在同一批文件上也满足同样的标准）。

## 检测编辑器定义的偏差

关键字表与 VS Code 版是双重维护的。为防止实现前进了而编辑器定义却落后，Rust 一侧有测试：

```sh
cargo test --test editor_keyword_sync_test
```

它实际加载 prelude，遍历注册表，报告**任一编辑器不认识的名字**。
特殊形式没有运行时表示，因此从 `crates/typelisp-front/src/check/checker.rs` 中
`// SPECIAL-FORM DISPATCH BEGIN` / `END` 之间读取（不要删除这些注释）。
失败时，把报告的名字加到**两个**编辑器定义中。

同一个测试还会核对 semantic tokens 的 legend（`src/bin/lsp.rs` 的 `SEMANTIC_TOKEN_TYPES` 与
两个编辑器持有的对应表，名字和顺序都必须一致）。不一致不会造成运行时错误，只会让所有 token 的
颜色互换，因此用机械方式固定下来。
