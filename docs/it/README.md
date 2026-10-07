<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Documentazione di typelisp (Italiano)

typelisp è un Lisp a tipizzazione statica. Per l'installazione e la compilazione si veda il
[README.md](../../README.md) (in inglese) nella radice del repository.

## Tutorial

Se è la prima volta che usi typelisp, leggi questi documenti nell'ordine indicato.

- [Primi passi](tutorial/intro.md): il REPL, le funzioni, le variabili, i condizionali, i cicli, le liste e `Vector`
- [Fondamenti dei tipi](tutorial/types.md): tipi statici, `Option`, `Result`, strutture, enumerazioni, generici
- [Trait](tutorial/traits.md): `deftrait` / `impl`, vincoli di trait, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Gestione degli errori](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concorrenza](tutorial/concurrency.md): task, canali, `select`, `Mutex`, `thread`

## Guide

- [Moduli e organizzazione dei file](guide/modules.md): `use`, `pub`, come i file corrispondono ai moduli
- [Compilazione](guide/compile.md): il JIT, la creazione di eseguibili con la compilazione AOT, i dump
- [I/O su file, stream e rete](guide/io.md): file, pathname, TCP / TLS / UDP, risoluzione dei nomi
- [FFI C](guide/ffi.md): chiamare funzioni C con `defffi` (inclusi i callback e le struct C con `def-c-struct`)
- [Integrazione con gli editor](guide/editors.md): `typl-lsp` e la configurazione di VS Code / Emacs
- [Per i programmatori Common Lisp](guide/from-common-lisp.md): in cosa typelisp differisce da CL e come riscrivere il codice CL

## Riferimento

- [Riferimento della sintassi](reference/syntax.md): sintassi lessicale, scrittura dei tipi, definizioni, forme di controllo, compilazione, concorrenza
- [Funzioni predefinite](reference/functions/README.md): funzioni predefinite, metodi e libreria standard
- [Tipi](reference/types.md): i tipi e i trait implementati da ciascuno
- [Messaggi di errore](reference/errors.md): il significato degli errori più comuni e come correggerli

## Integrazione con gli editor

Le istruzioni di configurazione si trovano nella [guida all'integrazione con gli editor](guide/editors.md).
Le scorciatoie da tastiera e le impostazioni di ciascun editor sono elencate in questi documenti:

- [Emacs (typelisp-mode)](../../editor/emacs/README_it.md)
- [VS Code](../../editor/vscode/README_it.md)
