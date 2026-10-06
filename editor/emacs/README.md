<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

An Emacs major mode for editing typelisp source (`.typl`).
The VS Code version is in [../vscode/](../vscode/README.md). The two share the same keyword tables
and the same indentation rules, and `cargo test --test editor_keyword_sync_test` checks that
mechanically (see the end of this document).

## Features

- Syntax highlighting
  - Special forms and control constructs (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, the `pprint` family, and so on)
  - Defined names (the `NAME` of `(defun NAME ...)` as a function name, of `(defstruct NAME ...)`
    as a type name, and of `(defvar (NAME ...))` as a variable name; the same with `pub`, as in
    `(pub defun NAME ...)`)
  - Namespace and declaration keywords (`pub` `module` `use` `load` `impl` `where`) and lambda
    list markers (`&rest` `&optional` `&key`)
  - Built-in functions (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` and so on)
  - Primitive types (including `bignum` / `ratio`), built-in types, built-in error types
    (`ParseIntError` and so on), `Capitalized` user types, and the trait object type `:dyn Trait`
  - **Uses of user-defined types** (the names of `defstruct`/`defenum`/`deftrait` are usually
    lowercase (`rect` `todo-item` `board`), so the `Capitalized` rule does not catch them).
    When connected to `typl-lsp`, they are coloured from the server's semantic tokens (this also
    works with `eglot`; see below). When not connected, the mode falls back to collecting the type
    names defined in the buffer
  - Literals (`true` `false`, number literals (decimal / `0xff` / `1.5` / `1/3`), character
    literals such as `#\Space`, strings, keywords such as `:name`)
  - `format` control directives inside strings (`~a` `~5,'0d` `~{...~}` and so on)
  - CL-style globals with earmuffs (`*print-pretty*` and so on)
- Comments
  - Line comments `;`
  - **Nestable** block comments `#| ... |#`
- S-expression navigation and Lisp-style indentation
- A definition index through `imenu` (functions / methods / macros / types / traits / `impl` /
  variables / modules)
- Commands that run the `typl` CLI (below)

## Key bindings

| Key | Command | What it does |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Save and run `typl FILE` (through `compile`, so you can jump to error lines) |
| `C-c C-z` | `typelisp-repl` | Start the `typl` REPL in a comint buffer |

Set the location of `typl` with `typelisp-program` (default `"typl"`).
Diagnostics have the form `error: FILE:LINE:COL: ...`, which `compilation-mode` can parse, so
`next-error` / `C-x \`` jumps straight to the place.

## Installation

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` files open in `typelisp-mode` automatically (the mode is registered in `auto-mode-alist`).

With `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

Once `typl-lsp` is built, it can be used from `eglot` (built into Emacs 29+) or `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

With `eglot`:

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

Supported: diagnostics (syntax/type errors and redefinition warnings, sent through
`textDocument/publishDiagnostics`), hover, goto-definition, completion (`:` is registered as a
trigger character), and semantic tokens. References across files through `use` are resolved (the
server searches upward for the project root's `typelisp.toml`; for details see
[Syntax Reference 3.11](../../docs/en/reference/syntax.md#311-files-and-modules-multi-file-projects)).
Unsaved edits in open editor buffers are reflected at once in the diagnostics of both the files
they depend on and the files that depend on them.

### Type name highlighting (semantic tokens)

Through `textDocument/semanticTokens`, the server reports **the positions the checker actually
resolved as type names**. Because this is not text matching:

- Types that come from other files through `use` are coloured too (a range that resolution inside
  the buffer cannot reach in principle)
- Calls of a **function** with the same name as a type are not coloured (the checker resolved
  them as functions, so no token is recorded there in the first place)

On the client side:

- **`eglot` (Emacs 31 and later)**: eglot draws the tokens itself (`eglot-semantic-tokens-mode`).
  `typelisp-mode` stays out of the way
- **`eglot` (Emacs 30 and earlier)**: this version of eglot does not handle semanticTokens. So
  **`typelisp-mode` sends the request itself and draws the result with overlays**
  (`typelisp-semantic-tokens-mode`, turned on automatically when eglot connects)
- **`lsp-mode`**: native support (set `lsp-semantic-tokens-enable` to `t`). In that case
  `typelisp-mode` stays out of the way

`scripts/emacs-semantic-smoke.el` connects through eglot for real and checks the side that does the
drawing in the Emacs being used. With any client, the in-buffer fallback steps back while the
server is answering (so that two sets of rules do not paint the same buffer).

| Setting | Default | What it does |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | With the eglot of Emacs 30 and earlier, whether to colour from the server's semantic tokens |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Idle seconds after an edit before requesting again (keep it larger than `eglot-send-changes-idle-time`) |

## Notes

- typelisp lowercases symbols when reading, but highlighting is case-sensitive so that type names
  starting with an uppercase letter can be told apart.
- Indentation is decided by the dedicated `typelisp-indent-function`, which looks up
  `typelisp-indent-specs` (an alist). The mode keeps its own entries even for forms whose names it
  shares with Emacs Lisp (`defun` `let` `if` ...) because symbol properties are **global**, and
  typelisp settings there would change the indentation of other Lisp buffers in the same session.
  And typelisp's forms differ in shape even when they share a name with Emacs Lisp:
  `(defun NAME (PARAMS) RETTYPE ...)` has three header elements, and `if` is fixed at three
  elements with a mandatory `else`. So the values cannot be shared either.
  Every `.typl` file under `examples/` has been checked: `indent-region` changes not a single byte,
  and flattening all indentation and re-indenting restores the original (the VS Code version meets
  the same standard on the same files).

## Detecting drift in the editor definitions

The keyword tables are maintained twice, once here and once in the VS Code version. To prevent the
editor definitions from falling behind while the implementation moves on, there is a test on the
Rust side:

```sh
cargo test --test editor_keyword_sync_test
```

It actually loads the prelude, walks the registry, and reports **names that either editor does not
know**. Special forms have no runtime representation, so they are read from between
`// SPECIAL-FORM DISPATCH BEGIN` / `END` in `crates/typelisp-front/src/check/checker.rs` (do not
delete these comments). If it fails, add the reported names to **both** editor definitions.

The same test also compares the semantic tokens legend (`SEMANTIC_TOKEN_TYPES` in
`src/bin/lsp.rs` and the tables both editors hold must agree in names and order). A mismatch causes
no runtime error; it only swaps the colours of every token, so it is pinned down mechanically.
