<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Dokumentacja typelisp (Polski)

typelisp to statycznie typowany Lisp. Instalacja i budowanie są opisane w pliku [README.md](../../README.md)
(po angielsku) w katalogu głównym repozytorium.

## Samouczek

Jeśli dopiero zaczynasz z typelisp, przeczytaj te dokumenty po kolei.

- [Pierwsze kroki](tutorial/intro.md): REPL, funkcje, zmienne, instrukcje warunkowe, pętle, listy i `Vector`
- [Podstawy typów](tutorial/types.md): typy statyczne, `Option`, `Result`, struktury, enumeracje, typy generyczne
- [Traity](tutorial/traits.md): `deftrait` / `impl`, ograniczenia traitów, `:dyn`
- [Makra](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Obsługa błędów](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Współbieżność](tutorial/concurrency.md): zadania, kanały, `select`, `Mutex`, `thread`

## Przewodniki

- [Moduły i układ plików](guide/modules.md): `use`, `pub`, jak pliki odpowiadają modułom
- [Kompilacja](guide/compile.md): JIT, budowanie plików wykonywalnych za pomocą kompilacji AOT, zrzuty (dumps)
- [Operacje na plikach, strumienie i sieć](guide/io.md): pliki, nazwy ścieżek, TCP / TLS / UDP, rozwiązywanie nazw
- [FFI do C](guide/ffi.md): wywoływanie funkcji C za pomocą `defffi` (w tym wywołania zwrotne i struktury C z `def-c-struct`)
- [Integracja z edytorami](guide/editors.md): `typl-lsp` oraz konfiguracja VS Code / Emacsa
- [Dla programistów Common Lisp](guide/from-common-lisp.md): czym typelisp różni się od CL i jak przepisać kod CL

## Referencja

- [Referencja składni](reference/syntax.md): składnia leksykalna, zapis typów, definicje, formy sterujące, kompilacja, współbieżność
- [Funkcje wbudowane](reference/functions/README.md): funkcje wbudowane, metody i biblioteka standardowa
- [Typy](reference/types.md): typy oraz traity, które każdy z nich implementuje
- [Komunikaty o błędach](reference/errors.md): co oznaczają typowe błędy i jak je naprawić

## Integracja z edytorami

Instrukcje konfiguracji znajdują się w [przewodniku po integracji z edytorami](guide/editors.md). Skróty klawiszowe
i ustawienia poszczególnych edytorów są opisane w tych dokumentach:

- [Emacs (typelisp-mode)](../../editor/emacs/README_pl.md)
- [VS Code](../../editor/vscode/README_pl.md)
