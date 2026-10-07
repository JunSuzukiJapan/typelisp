<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-dokumentation (Svenska)

typelisp är ett statiskt typat Lisp. Installation och bygge beskrivs i [README.md](../../README.md)
(på engelska) högst upp i repositoriet.

## Handledning

Är du ny med typelisp, läs dessa i ordning.

- [Komma igång](tutorial/intro.md): REPL, funktioner, variabler, villkor, slingor, listor och `Vector`
- [Grunderna i typer](tutorial/types.md): statiska typer, `Option`, `Result`, structs, enums, generiska typer
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait-gränser, `:dyn`
- [Makron](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Felhantering](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Samtidighet](tutorial/concurrency.md): tasks, kanaler, `select`, `Mutex`, `thread`

## Guider

- [Moduler och filuppdelning](guide/modules.md): `use`, `pub`, hur filer motsvarar moduler
- [Kompilering](guide/compile.md): JIT, att bygga körbara filer med AOT-kompilering, dumpar
- [Fil-I/O, strömmar och nätverk](guide/io.md): filer, pathnames, TCP / TLS / UDP, namnuppslagning
- [C FFI](guide/ffi.md): att anropa C-funktioner med `defffi` (inklusive callbacks och C-structs med `def-c-struct`)
- [Editorintegration](guide/editors.md): `typl-lsp` och hur man ställer in VS Code / Emacs
- [För Common Lisp-programmerare](guide/from-common-lisp.md): hur typelisp skiljer sig från CL och hur man skriver om CL-kod

## Referens

- [Syntaxreferens](reference/syntax.md): lexikalisk syntax, hur typer skrivs, definitioner, kontrollformer, kompilering, samtidighet
- [Inbyggda funktioner](reference/functions/README.md): inbyggda funktioner, metoder och standardbiblioteket
- [Typer](reference/types.md): typerna och de traits som var och en implementerar
- [Felmeddelanden](reference/errors.md): vad de vanliga felen betyder och hur man åtgärdar dem

## Editorintegration

Installationsanvisningar finns i [guiden för editorintegration](guide/editors.md). Tangentbindningar och
inställningar för varje editor står i dessa dokument:

- [Emacs (typelisp-mode)](../../editor/emacs/README_sv.md)
- [VS Code](../../editor/vscode/README_sv.md)
