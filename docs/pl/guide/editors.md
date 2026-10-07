<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Integracja z edytorami (typl-lsp)

`typl-lsp` to serwer języka typelisp. Podłączony do edytora obsługującego LSP (Language
Server Protocol), udostępnia dla edytowanego pliku następujące funkcje:

- Diagnostyka: błędy odczytu, błędy typów i ostrzeżenia o redefinicji
- Najechanie (hover): typ wyrażenia w nawiasach oraz docstring definicji, którą ono wywołuje (nie
  jest pokazywany dla samych nazw zmiennych)
- Przejście do definicji
- Uzupełnianie (kandydaci pojawiają się po wpisaniu `:`)
- Kolorowanie nazw typów (tokeny semantyczne), w tym typów wprowadzonych przez `use` z innych plików

Odwołania między plikami przez `use` są rozwiązywane. Niezapisane zmiany w innym otwartym pliku są odzwierciedlane
od razu w diagnostyce plików, które go wprowadzają przez `use`.

## 1. Budowanie

```sh
cargo build --release --bin typl-lsp
```

Powstaje `target/release/typl-lsp`. Jeśli zainstalowałeś za pomocą `cargo install`, jak opisano w
[README.md](../../../README.md), znajduje się w `~/.cargo/bin/typl-lsp` razem z `typl`.

## 2. VS Code

Rozszerzenie znajduje się w `editor/vscode` w repozytorium. Nie jest opublikowane w Marketplace, więc
zbuduj je i zainstaluj samodzielnie.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # tworzy plik .vsix
```

Wybierz „Install from VSIX..." z menu „..." widoku Extensions i wskaż zbudowany plik `.vsix`.

Rozszerzenie szuka `typl-lsp` w `target/release/typl-lsp` obszaru roboczego, potem w
`target/debug/typl-lsp`, a następnie w `PATH`. Jeśli umieściłeś go gdzie indziej, wpisz jego ścieżkę do
ustawienia `typelisp.languageServer.path`.

| Ustawienie | Domyślnie | Znaczenie |
|---|---|---|
| `typelisp.program` | `typl` | Ścieżka do `typl` |
| `typelisp.languageServer.enable` | `true` | Czy łączyć się z `typl-lsp` |
| `typelisp.languageServer.path` | (puste) | Ścieżka do `typl-lsp` |

`Ctrl+Alt+R` zapisuje edytowany plik i uruchamia go za pomocą `typl`, a `Ctrl+Alt+Z` uruchamia
REPL. Więcej informacji znajdziesz w [README rozszerzenia VS Code](../../../editor/vscode/README_pl.md).

## 3. Emacs

`typelisp-mode` znajduje się w `editor/emacs` w repozytorium.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Ustawienia połączenia z `typl-lsp` za pomocą `eglot` (dołączonego do Emacsa 29 i nowszych):

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Z `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Eglot w Emacsie 31 i nowszych sam koloruje nazwy typów (tokeny semantyczne). Eglot w Emacsie 30 i
starszych ich nie obsługuje, więc nazwy typów koloruje zamiast niego `typelisp-mode`. W `lsp-mode` ustaw
`lsp-semantic-tokens-enable` na `t`.

`C-c C-c` uruchamia edytowany plik, a `C-c C-z` uruchamia REPL. Więcej informacji znajdziesz w
[README typelisp-mode](../../../editor/emacs/README_pl.md).

## 4. Inne edytory

`typl-lsp` komunikuje się przez LSP na standardowym wejściu i wyjściu i nie przyjmuje argumentów wiersza poleceń. Skonfiguruj
klienta LSP swojego edytora tak, aby uruchamiał `typl-lsp` dla plików `.typl`.

## 5. Jak rozpoznawane są projekty

`typl-lsp` szuka `typelisp.toml`, zaczynając od katalogu otwartego pliku i idąc w górę,
i rozwiązuje `use`, przyjmując to miejsce za korzeń źródeł. Są to te same reguły, co przy uruchamianiu przez `typl`
pliku ([Moduły i układ plików](modules.md#2-konfigurowanie-projektu)). W projekcie złożonym z kilku
plików umieść `typelisp.toml` w jego korzeniu.

## 6. Serwer języka nie uruchamia twojego programu

`typl-lsp` tworzy diagnostykę wyłącznie przez wczytanie i sprawdzenie typów. Nigdy nie uruchamia edytowanego
programu. Diagnostyka działa przy każdym naciśnięciu klawisza, więc nie może sobie pozwolić na uruchamianie kodu z efektami ubocznymi ani
kodu, który nigdy się nie kończy. Jedynym wyjątkiem jest rejestrowanie `defmacro`, które jest potrzebne do
sprawdzenia wywołań makr występujących po nich.

Z tego powodu błędy, które pojawiają się dopiero, gdy `typl` uruchamia program (`panic`, brakujący plik i tak
dalej), nie pojawiają się w diagnostyce serwera języka.
