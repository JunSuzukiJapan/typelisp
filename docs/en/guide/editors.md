<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Editor Integration (typl-lsp)

`typl-lsp` is the typelisp language server. Connected to an editor that supports LSP (the Language
Server Protocol), it provides these features for the file you are editing:

- Diagnostics: read errors, type errors and redefinition warnings
- Hover: the type of a parenthesized expression and the docstring of the definition it calls (not
  shown for bare variable names)
- Go to definition
- Completion (candidates appear when you type `:`)
- Coloring of type names (semantic tokens), including types `use`d from other files

References across files through `use` are resolved. Unsaved edits to another open file are reflected
right away in the diagnostics of the files that `use` it.

## 1. Building

```sh
cargo build --release --bin typl-lsp
```

This produces `target/release/typl-lsp`. If you installed with `cargo install` as described in
[README.md](../../../README.md), it is in `~/.cargo/bin/typl-lsp` along with `typl`.

## 2. VS Code

The extension is in `editor/vscode` in the repository. It is not published on the Marketplace, so
build it and install it yourself.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produces a .vsix
```

Choose "Install from VSIX..." from the "..." menu of the Extensions view and select the `.vsix` you
built.

The extension looks for `typl-lsp` in the workspace's `target/release/typl-lsp`, then
`target/debug/typl-lsp`, then on `PATH`. If you put it somewhere else, write its path in the
setting `typelisp.languageServer.path`.

| Setting | Default | Meaning |
|---|---|---|
| `typelisp.program` | `typl` | Path of `typl` |
| `typelisp.languageServer.enable` | `true` | Whether to connect to `typl-lsp` |
| `typelisp.languageServer.path` | (empty) | Path of `typl-lsp` |

`Ctrl+Alt+R` saves the file you are editing and runs it with `typl`, and `Ctrl+Alt+Z` starts the
REPL. For more, see the [README of the VS Code extension](../../../editor/vscode/README.md) (in
Japanese).

## 3. Emacs

`typelisp-mode` is in `editor/emacs` in the repository.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Settings for connecting to `typl-lsp` with `eglot` (bundled with Emacs 29 and later):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

With `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot does not support semantic tokens, so when you use eglot, `typelisp-mode` colors type names
itself instead. With `lsp-mode`, set `lsp-semantic-tokens-enable` to `t`.

`C-c C-c` runs the file you are editing, and `C-c C-z` starts the REPL. For more, see the
[README of typelisp-mode](../../../editor/emacs/README.md) (in Japanese).

## 4. Other editors

`typl-lsp` speaks LSP over standard input and output and takes no command-line arguments. Configure
your editor's LSP client to start `typl-lsp` for `.typl` files.

## 5. How projects are recognized

`typl-lsp` looks for `typelisp.toml` starting from the directory of the opened file and moving up,
and resolves `use` with that place as the source root. These are the same rules as when `typl` runs a
file ([Modules and File Layout](modules.md#2-setting-up-a-project)). For a project made of several
files, put `typelisp.toml` at its root.

## 6. The language server does not run your program

`typl-lsp` produces diagnostics by reading and type-checking only. It never runs the program you are
editing. Diagnostics run on every keystroke, so it cannot afford to run code with side effects or
code that never finishes there. The one exception is registering `defmacro`s, which is needed to
check the macro calls that come after them.

Because of this, errors that happen only when `typl` runs the program (`panic`, a missing file and so
on) do not appear in the language server's diagnostics.
