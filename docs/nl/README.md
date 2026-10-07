<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-documentatie (Nederlands)

typelisp is een statisch getypeerde Lisp. Voor het installeren en bouwen zie de
[README.md](../../README.md) (in het Engels) bovenaan de repository.

## Tutorial

Ben je nieuw met typelisp, lees deze documenten dan in volgorde.

- [Aan de slag](tutorial/intro.md): de REPL, functies, variabelen, voorwaarden, lussen, lijsten en `Vector`
- [Basis van types](tutorial/types.md): statische types, `Option`, `Result`, structs, enums, generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait bounds, `:dyn`
- [Macro's](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Foutafhandeling](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Gelijktijdigheid](tutorial/concurrency.md): taken, kanalen, `select`, `Mutex`, `thread`

## Handleidingen

- [Modules en bestandsindeling](guide/modules.md): `use`, `pub`, hoe bestanden op modules worden afgebeeld
- [Compileren](guide/compile.md): de JIT, uitvoerbare bestanden bouwen met AOT-compilatie, dumps
- [Bestands-I/O, streams en netwerk](guide/io.md): bestanden, padnamen, TCP / TLS / UDP, naamresolutie
- [C-FFI](guide/ffi.md): C-functies aanroepen met `defffi` (inclusief callbacks en C-structs met `def-c-struct`)
- [Editorintegratie](guide/editors.md): `typl-lsp` en het inrichten van VS Code / Emacs
- [Voor Common Lisp-programmeurs](guide/from-common-lisp.md): hoe typelisp van CL verschilt en hoe je CL-code herschrijft

## Referentie

- [Syntaxreferentie](reference/syntax.md): lexicale syntaxis, het schrijven van types, definities, besturingsvormen, compilatie, gelijktijdigheid
- [Ingebouwde functies](reference/functions/README.md): ingebouwde functies, methoden en de standaardbibliotheek
- [Types](reference/types.md): de types en de traits die elk type implementeert
- [Foutmeldingen](reference/errors.md): wat veelvoorkomende fouten betekenen en hoe je ze oplost

## Editorintegratie

Instructies voor het inrichten staan in de [handleiding Editorintegratie](guide/editors.md). De
sneltoetsen en instellingen van elke editor staan in deze documenten:

- [Emacs (typelisp-mode)](../../editor/emacs/README_nl.md)
- [VS Code](../../editor/vscode/README_nl.md)
