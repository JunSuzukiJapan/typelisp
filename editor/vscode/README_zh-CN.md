<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp（VS Code）

用于编辑 typelisp 源代码（`.typl`）的 VS Code 扩展。
Emacs 版见 [../emacs/](../emacs/README_zh-CN.md)。两者使用相同的关键字表和相同的缩进规则，
这一点由 `cargo test --test editor_keyword_sync_test` 机械地检查（见下文）。

## 功能

- **语法高亮**（TextMate 语法，不需要语言服务器）
  - 特殊形式和控制结构（`defun` `let` `if` `match` `loop` `lambda` `setf` `as` `apply`、
    `print`/`println`/`format`、`pprint` 系列）
  - 定义名（`(defun NAME ...)` 为函数，`(defstruct NAME ...)` 为类型，`(defvar (NAME ...))` 为变量。
    带 `pub` 的 `(pub defun NAME ...)` 也一样）以及 `(impl Trait Type)` 的两个名字
  - 命名空间和声明关键字（`pub` `module` `use` `load` `impl` `where`）、
    lambda 列表标记（`&rest` `&optional` `&key`）
  - 内置函数、基本类型（包括 `bignum` / `ratio`）、内置错误类型、
    `Capitalized` 的用户类型、trait 对象类型 `:dyn Trait`（泛型参数内部也是）
  - 数字字面量（十进制 / `0xff` / `1.5` / `3.0e10` / `1/3`）、字符字面量 `#\Space`、
    关键字 `:name`、带耳罩的全局变量 `*print-pretty*`
  - **字符串中的 `format` 控制指令**（`~a` `~5,'0d` `~{...~}` `~^` 等）
  - 行注释 `;` 和**可嵌套的**块注释 `#| ... |#`
- **用户定义类型的使用处**（semantic tokens）
  - `defstruct` / `defenum` / `deftrait` 的名字通常是小写（`rect` `todo-item` `board`），
    `Capitalized` 规则捕捉不到，而 TextMate 语法按行工作，看不到整个文件。
    semantic tokens 可以看到，从而解决了静态类型语言却只有类型注解不着色的状况
  - 连接 `typl-lsp` 时，返回的是**检查器实际解析为类型名的位置**。因此通过 `use` 从其他文件引入的
    类型也会着色，而与类型同名的**函数**的调用处不会着色
    （检查器在那里将其解析为函数，因此根本不会记录 token）
  - 未连接或未构建时，扩展会退回到在文件内部解析的文本扫描。这只是近似：
    找不到来自其他文件的类型，也无法区分与类型同名的函数
- **Lisp 缩进**（VS Code 本身没有 Lisp 缩进，因此由扩展实现）
  - 格式化文档、格式化选定内容、键入时格式化（Enter 和 `)`，启用 `editor.formatOnType` 时）
- **Outline / breadcrumbs / `Ctrl+Shift+O`**（函数、方法、宏、类型、trait、
  `impl`、变量、模块）
- **`typl-lsp` 集成**（诊断、hover、跳转到定义、补全、semantic tokens）
- **`typl` CLI 命令**（运行、REPL）

除语言服务器以外的功能都只靠扩展本身就能工作，因此即使在没有构建 `typl-lsp` 的检出中，
高亮、缩进、Outline 和（限于文件内的）类型高亮也都可以使用。

## 安装

扩展没有发布到 Marketplace，需要在本地构建后安装。

```sh
cd editor/vscode
npm install
npm run compile
```

然后选择其一：

- **在开发宿主中试用**：用 VS Code 打开 `editor/vscode` 并按 `F5`
- **永久安装**：用 `npx @vscode/vsce package` 生成 `.vsix`，
  然后在扩展视图中选择“…”→“Install from VSIX...”

`.typl` 文件会自动以 typelisp 模式打开。

## 按键绑定

| 按键 | 命令 | 作用 |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | 保存并用 `typl FILE` 运行 |
| `Ctrl+Alt+Z` | `typelisp.repl` | 启动 `typl` 的 REPL |

命令面板中还有 `typelisp: Restart Language Server`。

## 设置

| 设置 | 默认值 | 作用 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` CLI 的路径 |
| `typelisp.languageServer.enable` | `true` | 是否连接 `typl-lsp` |
| `typelisp.languageServer.path` | （空） | `typl-lsp` 的路径。为空时按工作区的 `target/release/typl-lsp` → `target/debug/typl-lsp` → `PATH` 的顺序查找 |
| `typelisp.trace.server` | `off` | 记录 LSP 的 JSON-RPC |

用下面的命令构建语言服务器：

```sh
cargo build --release --bin typl-lsp
```

通过 `use` 跨文件的引用，会向上查找项目根目录的 `typelisp.toml` 来解析
（详见 [语法参考 3.11](../../docs/zh-CN/reference/syntax.md#311-文件与模块的对应多文件项目)）。

## 任务的 problem matcher

扩展提供名为 `typelisp` 的 problem matcher。`typl` 的诊断格式是
`error: FILE:LINE:COL: message`，因此可以直接显示在“问题”面板中：

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## 开发

```sh
npm run compile   # tsc
npm run watch     # 监视构建
npm test          # node --test（语法、缩进、符号、类型引用、清单）
```

测试只覆盖不需要 `vscode` 模块的部分。为此，`src/indent.ts` 和 `src/symbols.ts` 写成纯函数，
只有 `src/extension.ts` 接触编辑器 API。

- `src/test/grammar.test.ts` —— 用与 VS Code 相同的引擎（`vscode-textmate` +
  `vscode-oniguruma`）实际对语法进行分词并检查。
  Oniguruma 与 Emacs 正则表达式在细节上不同（例如不把字符类开头的 `]` 当作字面量），
  这种差异只有真正运行引擎才能发现。
- `src/test/indent.test.ts` —— 对 `examples/` 下的所有 `.typl` 文件，
  要求**把缩进全部去掉后恢复的结果与已提交的内容逐字节一致**。
  Emacs 模式在同一批文件上也满足同样的标准，这使“两个编辑器一致”成为经过验证的说法。
  此外，`src/test/fixtures/emacs-indent-reference.txt` 是在 Emacs 的 `typelisp-mode` 缓冲区中
  实际运行 `indent-region` 采集的参考输出。期望值不是对 TS 实现的照抄，而是
  **另一个编辑器实际产生的结果**，因此移植的忠实度可以直接得到检验
  （包括 `let*` `do` `doiter` `labels` `impl` `pprint-logical-block`、quote 前缀等）。
- `src/test/symbols.test.ts` —— Outline 的内容以及后备方式对类型引用的检测。定义数必须与
  统计行首定义形式的另一种独立方法完全一致。类型引用的边界规则有意与 Emacs 版的后备方式对齐
  （VS Code 用 lookbehind，Emacs 用消耗一个前导字符的形式表达同一集合）。
- 服务器端由解析驱动的 token（`crates/typelisp-front/src/check/semantic.rs`）由
  `cargo test --test lsp_semantic_test` 和 `scripts/lsp-semantic-smoke.py`
  （通过 stdio 驱动真实进程）检查。Emacs 端的客户端由
  `scripts/emacs-semantic-smoke.el` 通过真实的 eglot 连接检查。
- `src/test/manifest.test.ts` —— `package.json` 是编译器不检查的唯一部分，因此这里检查：
  已声明的命令与 `registerCommand` 的集合一致、按键绑定的引用目标、代码读取的设置是否已声明、
  problem matcher 能否解析 `typl` 实际的输出。

### 检测编辑器定义的偏差

关键字表在 Emacs 版和 VS Code 版中双重维护。为防止实现前进了而编辑器定义却落后，
Rust 一侧有测试：

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

## 备注

- typelisp 在读取时把符号转为小写，但高亮区分大小写，以便区分以大写字母开头的类型名。
- 缩进由 `src/indent.ts` 的 `INDENT_SPECS` 决定。它移植自 Emacs 版的
  `typelisp-indent-specs`，值和规则都相同。`(defun NAME (PARAMS) RETTYPE ...)` 的头部有 3 个元素、
  `if` 固定为必须有 `else` 的 3 个元素等，与 Emacs Lisp 同名但形状不同的地方都照样反映出来。
- `#| ... |#` 的内容在格式化时会重新缩进。这与 Emacs 的 `indent-region` 行为一致。
