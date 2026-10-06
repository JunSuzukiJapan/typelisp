<!-- translated-from: docs/ja/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# typelisp-Dokumentation (Deutsch)

typelisp ist ein statisch typisiertes Lisp. Installation und Bau sind in der
[README.md](../../README.md) (Englisch) im Wurzelverzeichnis des Repositorys beschrieben.

## Tutorial

Wer typelisp zum ersten Mal begegnet, liest am besten in dieser Reihenfolge.

- [Erste Schritte](tutorial/intro.md): die REPL, Funktionen, Variablen, Verzweigungen, Schleifen, Listen und `Vector`
- [Grundlagen der Typen](tutorial/types.md): statische Typen, `Option`, `Result`, Strukturen, Aufzählungen, Generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, Trait-Schranken, `:dyn`
- [Makros](tutorial/macros.md): `defmacro`, Quasiquote, `gensym`, `macrolet`
- [Fehlerbehandlung](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Nebenläufigkeit](tutorial/concurrency.md): Tasks, Kanäle, `select`, `Mutex`, `thread`

## Leitfäden

- [Module und Dateiaufteilung](guide/modules.md): `use`, `pub`, wie Dateien den Modulen entsprechen
- [Kompilierung](guide/compile.md): der JIT, Programme mit AOT-Kompilierung erstellen, Dumps
- [Datei-E/A, Streams und Netzwerk](guide/io.md): Dateien, Pfadnamen, TCP / TLS / UDP, Namensauflösung
- [C-FFI](guide/ffi.md): C-Funktionen mit `defffi` aufrufen (einschließlich Callbacks und C-Structs mit `def-c-struct`)
- [Editor-Anbindung](guide/editors.md): `typl-lsp` und die Einrichtung von VS Code / Emacs
- [Für Common-Lisp-Programmierer](guide/from-common-lisp.md): wie sich typelisp von CL unterscheidet und wie man CL-Code umschreibt

## Referenz

- [Syntaxreferenz](reference/syntax.md): Lexik, Typschreibweise, Definitionen, Steuerformen, Kompilierung, Nebenläufigkeit
- [Eingebaute Funktionen](reference/functions/README.md): eingebaute Funktionen, Methoden und die Standardbibliothek
- [Typen](reference/types.md): die Typen und die Traits, die jeder von ihnen implementiert
- [Fehlermeldungen](reference/errors.md): was die häufigen Fehler bedeuten und wie man sie behebt

## Editor-Anbindung

Die Schritte zur Einrichtung stehen im [Leitfaden zur Editor-Anbindung](guide/editors.md). Die
Tastenbelegungen und Einstellungen der einzelnen Editoren sind in diesen Dokumenten aufgeführt (auf Japanisch):

- [Emacs (typelisp-mode)](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
