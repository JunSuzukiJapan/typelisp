<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 编辑器集成（typl-lsp）

`typl-lsp` 是 typelisp 的语言服务器。连接到支持 LSP（Language Server Protocol）的编辑器后，可以对正在编辑的文件
使用以下功能：

- 诊断：读取错误、类型错误、重定义警告
- 悬停：括号表达式的类型以及所调用定义的文档字符串（裸变量名不显示）
- 跳转到定义
- 补全（输入 `:` 时显示候选）
- 类型名着色（semantic tokens）：从其他文件 `use` 的类型也会着色

通过 `use` 跨文件的引用也会被解析。在其他文件中尚未保存的编辑，也会立即反映到 `use` 该文件的文件的诊断中。

## 1. 构建

```sh
cargo build --release --bin typl-lsp
```

会生成 `target/release/typl-lsp`。如果按照 [README.md](../../../README.md) 的步骤用 `cargo install` 安装，它会
和 `typl` 一起放在 `~/.cargo/bin/typl-lsp`。

## 2. VS Code

扩展位于仓库的 `editor/vscode`。它没有发布到 Marketplace，需要自行构建安装。

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # 生成 .vsix
```

在扩展视图的"…"菜单中选择"Install from VSIX..."，指定生成的 `.vsix`。

扩展按工作区的 `target/release/typl-lsp`、`target/debug/typl-lsp`、`PATH` 的顺序查找 `typl-lsp`。放在其他位置时，
请在设置 `typelisp.languageServer.path` 中写上路径。

| 设置 | 默认值 | 内容 |
|---|---|---|
| `typelisp.program` | `typl` | `typl` 的路径 |
| `typelisp.languageServer.enable` | `true` | 是否连接 `typl-lsp` |
| `typelisp.languageServer.path` | （空） | `typl-lsp` 的路径 |

`Ctrl+Alt+R` 保存正在编辑的文件并用 `typl` 运行，`Ctrl+Alt+Z` 启动 REPL。详情请参阅
[VS Code 扩展的 README](../../../editor/vscode/README.md)（日文）。

## 3. Emacs

`typelisp-mode` 位于仓库的 `editor/emacs`。

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

用 `eglot`（Emacs 29 及以后版本自带）连接 `typl-lsp` 的设置：

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

eglot 不支持 semantic tokens，所以使用 eglot 时由 `typelisp-mode` 代为给类型名着色。使用 `lsp-mode` 时，请把
`lsp-semantic-tokens-enable` 设为 `t`。

`C-c C-c` 运行正在编辑的文件，`C-c C-z` 启动 REPL。详情请参阅
[typelisp-mode 的 README](../../../editor/emacs/README.md)（日文）。

## 4. 其他编辑器

`typl-lsp` 通过标准输入输出使用 LSP 通信，不接受命令行参数。请在编辑器的 LSP 客户端中设置为对 `.typl` 文件启动
`typl-lsp`。

## 5. 项目的识别

`typl-lsp` 从打开的文件所在目录开始向上查找 `typelisp.toml`，以该位置为源代码根目录解析 `use`。规则与用 `typl` 运行
文件时相同（[模块与文件结构](modules.md#2-创建项目)）。由多个文件构成的项目，请在根目录放置 `typelisp.toml`。

## 6. 语言服务器不会运行程序

`typl-lsp` 只通过读取和类型检查来给出诊断，不会运行正在编辑的程序。每次按键都会进行诊断，不能在那里运行有副作用的代码
或不会结束的代码。唯一的例外是注册 `defmacro`，这是检查其后的宏调用所必需的。

因此，只在用 `typl` 运行时才会发生的错误（panic、文件不存在等）不会出现在语言服务器的诊断中。
