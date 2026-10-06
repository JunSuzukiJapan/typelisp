<!-- translated-from: docs/ja/README.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp Documentation (English)

typelisp is a statically typed Lisp. For installing and building it, see [README.md](../../README.md)
at the top of the repository.

## Tutorial

If you are new to typelisp, read these in order.

- [Getting Started](tutorial/intro.md): the REPL, functions, variables, conditionals, loops, lists and `Vector`
- [Type Basics](tutorial/types.md): static types, `Option`, `Result`, structs, enums, generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait bounds, `:dyn`
- [Macros](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Error Handling](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concurrency](tutorial/concurrency.md): tasks, channels, `select`, `Mutex`, `thread`

## Guides

- [Modules and File Layout](guide/modules.md): `use`, `pub`, how files map to modules
- [Compiling](guide/compile.md): the JIT, building executables with AOT compilation, dumps
- [File I/O, Streams and Networking](guide/io.md): files, pathnames, TCP / TLS / UDP, name resolution
- [C FFI](guide/ffi.md): calling C functions with `defffi` (including callbacks and C structs with `def-c-struct`)
- [Editor Integration](guide/editors.md): `typl-lsp` and setting up VS Code / Emacs
- [For Common Lisp Programmers](guide/from-common-lisp.md): how typelisp differs from CL and how to rewrite CL code

## Reference

- [Syntax Reference](reference/syntax.md): lexical syntax, writing types, definitions, control forms, compilation, concurrency
- [Built-in Functions](reference/functions/README.md): built-in functions, methods and the standard library
- [Types](reference/types.md): the types and the traits each one implements
- [Error Messages](reference/errors.md): what the common errors mean and how to fix them

## Editor Integration

Setup instructions are in the [Editor Integration guide](guide/editors.md). The key bindings and
settings of each editor are listed in these documents:

- [Emacs (typelisp-mode)](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
