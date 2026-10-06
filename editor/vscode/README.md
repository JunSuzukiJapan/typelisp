<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

A VS Code extension for editing typelisp source (`.typl`).
The Emacs version is in [../emacs/](../emacs/README.md). The two share the same keyword tables and
the same indentation rules, and `cargo test --test editor_keyword_sync_test` checks that
mechanically (see below).

## Features

- **Syntax highlighting** (a TextMate grammar; no language server needed)
  - Special forms and control constructs (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, the `pprint` family)
  - Defined names (`(defun NAME ...)` as a function, `(defstruct NAME ...)` as a type,
    `(defvar (NAME ...))` as a variable; the same with `pub`, as in `(pub defun NAME ...)`) and
    both names of `(impl Trait Type)`
  - Namespace and declaration keywords (`pub` `module` `use` `load` `impl` `where`) and lambda
    list markers (`&rest` `&optional` `&key`)
  - Built-in functions, primitive types (including `bignum` / `ratio`), built-in error types,
    `Capitalized` user types, and the trait object type `:dyn Trait` (also inside generic
    arguments)
  - Number literals (decimal / `0xff` / `1.5` / `3.0e10` / `1/3`), character literals such as
    `#\Space`, keywords such as `:name`, and globals with earmuffs such as `*print-pretty*`
  - **`format` control directives inside strings** (`~a` `~5,'0d` `~{...~}` `~^` and so on)
  - Line comments `;` and **nestable** block comments `#| ... |#`
- **Uses of user-defined types** (semantic tokens)
  - The names of `defstruct` / `defenum` / `deftrait` are usually lowercase (`rect` `todo-item`
    `board`), so the `Capitalized` rule does not catch them, and a TextMate grammar works line by
    line and cannot see the whole file. Semantic tokens can, which fixes the state where a
    statically typed language left only its type annotations uncoloured
  - When connected to `typl-lsp`, the extension receives **the positions the checker actually
    resolved as type names**. So types that come from other files through `use` are coloured, and
    calls of a **function** with the same name as a type are not (the checker resolved them as
    functions, so no token is recorded there in the first place)
  - When the server is not connected or not built, the extension falls back to a text scan that
    resolves within the file. That is an approximation: it cannot find types from other files,
    and it cannot tell a function with the same name as a type apart
- **Lisp indentation** (VS Code has no built-in Lisp indentation, so the extension implements it)
  - Format Document, Format Selection, and format on type (Enter and `)`, when
    `editor.formatOnType` is enabled)
- **Outline / breadcrumbs / `Ctrl+Shift+O`** (functions, methods, macros, types, traits, `impl`,
  variables, modules)
- **`typl-lsp` integration** (diagnostics, hover, goto-definition, completion, semantic tokens)
- **`typl` CLI commands** (run, REPL)

Everything except the language server works with the extension alone, so even in a checkout
where `typl-lsp` has not been built, highlighting, indentation, the Outline, and (file-local) type
highlighting are available.

## Installation

The extension is not on the Marketplace, so build it locally and install it.

```sh
cd editor/vscode
npm install
npm run compile
```

Then either:

- **Try it in a development host**: open `editor/vscode` in VS Code and press `F5`
- **Install it permanently**: make a `.vsix` with `npx @vscode/vsce package`, then use
  "..." → "Install from VSIX..." in the Extensions view

`.typl` files open in typelisp mode automatically.

## Key bindings

| Key | Command | What it does |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Save and run `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Start the `typl` REPL |

The command palette also has `typelisp: Restart Language Server`.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `typelisp.program` | `typl` | Path of the `typl` CLI |
| `typelisp.languageServer.enable` | `true` | Whether to connect to `typl-lsp` |
| `typelisp.languageServer.path` | (empty) | Path of `typl-lsp`. When empty, the extension looks for the workspace's `target/release/typl-lsp`, then `target/debug/typl-lsp`, then `PATH` |
| `typelisp.trace.server` | `off` | Log the LSP JSON-RPC traffic |

Build the language server with:

```sh
cargo build --release --bin typl-lsp
```

References across files through `use` are resolved by searching upward for the project root's
`typelisp.toml` (for details see
[Syntax Reference 3.11](../../docs/en/reference/syntax.md#311-files-and-modules-multi-file-projects)).

## The task problem matcher

The extension provides a problem matcher named `typelisp`. `typl` prints diagnostics in the form
`error: FILE:LINE:COL: message`, so they can go straight to the Problems panel:

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

## Development

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

The tests cover only the parts that do not need the `vscode` module. For that, `src/indent.ts` and
`src/symbols.ts` are written as pure functions, and only `src/extension.ts` touches the editor API.

- `src/test/grammar.test.ts` — tokenizes with the grammar for real, using the same engine as VS
  Code (`vscode-textmate` + `vscode-oniguruma`), and checks the result.
  Oniguruma differs from Emacs regular expressions in details (for example, it does not treat a
  `]` at the start of a character class as a literal), and such differences are found only by
  running the real engine.
- `src/test/indent.test.ts` — for every `.typl` file under `examples/`, requires that
  **flattening all indentation and restoring it matches the committed content byte for byte**.
  The Emacs mode meets the same standard on the same files, and that is what makes "the two
  editors agree" a verified claim.
  In addition, `src/test/fixtures/emacs-indent-reference.txt` is reference output collected by
  actually running `indent-region` in an Emacs `typelisp-mode` buffer. The expected side is not a
  restatement of the TS implementation but **what the other editor actually produces**, so the
  faithfulness of the port is checked directly (it includes `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, quote prefixes and more).
- `src/test/symbols.test.ts` — the Outline contents and the fallback's detection of type
  references. The number of definitions must match exactly an independent count of definition
  forms at the start of lines. The boundary rules for type references are deliberately aligned
  with the Emacs version's fallback (VS Code uses a lookbehind; Emacs expresses the same set by
  consuming one preceding character).
- The server's resolution-driven tokens (`crates/typelisp-front/src/check/semantic.rs`) are
  checked by `cargo test --test lsp_semantic_test` and `scripts/lsp-semantic-smoke.py` (which
  drives a real process over stdio). The Emacs client is checked by
  `scripts/emacs-semantic-smoke.el` over a real eglot connection.
- `src/test/manifest.test.ts` — `package.json` is the one part the compiler does not check, so this
  checks that the declared commands and the `registerCommand` calls are the same set, what the key
  bindings refer to, that the settings the code reads are declared, and that the problem matcher
  can parse what `typl` actually prints.

### Detecting drift in the editor definitions

The keyword tables are maintained twice, in the Emacs version and the VS Code version. To prevent
the editor definitions from falling behind while the implementation moves on, there is a test on
the Rust side:

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

## Notes

- typelisp lowercases symbols when reading, but highlighting is case-sensitive so that type names
  starting with an uppercase letter can be told apart.
- Indentation is decided by `INDENT_SPECS` in `src/indent.ts`. It is a port of the Emacs version's
  `typelisp-indent-specs`, with the same values and rules. Places where a form differs in shape
  from the Emacs Lisp form of the same name are carried over as they are: the header of
  `(defun NAME (PARAMS) RETTYPE ...)` has three elements, `if` is fixed at three elements with a
  mandatory `else`, and so on.
- The contents of `#| ... |#` are re-indented when formatting. This matches the behaviour of
  Emacs's `indent-region`.
