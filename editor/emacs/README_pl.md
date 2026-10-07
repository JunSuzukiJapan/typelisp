<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Główny tryb Emacsa do edycji kodu źródłowego typelisp (`.typl`).
Wersja dla VS Code znajduje się w [../vscode/](../vscode/README_pl.md). Obie dzielą te same tablice słów kluczowych
i te same reguły wcięć, a `cargo test --test editor_keyword_sync_test` sprawdza to
mechanicznie (zobacz koniec tego dokumentu).

## Funkcje

- Podświetlanie składni
  - Formy specjalne i konstrukcje sterujące (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, rodzina `pprint` i tak dalej)
  - Definiowane nazwy (`NAME` w `(defun NAME ...)` jako nazwa funkcji, w `(defstruct NAME ...)`
    jako nazwa typu i w `(defvar (NAME ...))` jako nazwa zmiennej; to samo z `pub`, jak w
    `(pub defun NAME ...)`)
  - Słowa kluczowe przestrzeni nazw i deklaracji (`pub` `module` `use` `load` `impl` `where`) oraz znaczniki
    listy lambda (`&rest` `&optional` `&key`)
  - Funkcje wbudowane (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` i tak dalej)
  - Typy prymitywne (w tym `bignum` / `ratio`), typy wbudowane, wbudowane typy błędów
    (`ParseIntError` i tak dalej), typy użytkownika pisane wielką literą (`Capitalized`) oraz typ obiektu traitu `:dyn Trait`
  - **Użycia typów zdefiniowanych przez użytkownika** (nazwy `defstruct`/`defenum`/`deftrait` są zwykle
    pisane małymi literami (`rect` `todo-item` `board`), więc reguła `Capitalized` ich nie obejmuje).
    Po połączeniu z `typl-lsp` są kolorowane z tokenów semantycznych serwera (działa to także
    z `eglot`; zobacz niżej). Gdy nie ma połączenia, tryb wraca do zbierania nazw typów
    zdefiniowanych w buforze
  - Literały (`true` `false`, literały liczbowe (dziesiętne / `0xff` / `1.5` / `1/3`), literały
    znakowe, takie jak `#\Space`, łańcuchy znaków, słowa kluczowe, takie jak `:name`)
  - Dyrektywy sterujące `format` wewnątrz łańcuchów znaków (`~a` `~5,'0d` `~{...~}` i tak dalej)
  - Zmienne globalne z gwiazdkami w stylu CL (`*print-pretty*` i tak dalej)
- Komentarze
  - Komentarze liniowe `;`
  - **Zagnieżdżalne** komentarze blokowe `#| ... |#`
- Nawigacja po S-wyrażeniach i wcięcia w stylu Lisp
- Indeks definicji przez `imenu` (funkcje / metody / makra / typy / traity / `impl` /
  zmienne / moduły)
- Polecenia uruchamiające CLI `typl` (niżej)

## Skróty klawiszowe

| Klawisz | Polecenie | Co robi |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Zapisuje i uruchamia `typl FILE` (przez `compile`, więc można przeskakiwać do linii z błędami) |
| `C-c C-z` | `typelisp-repl` | Uruchamia REPL `typl` w buforze comint |

Położenie `typl` ustawia się za pomocą `typelisp-program` (domyślnie `"typl"`).
Diagnostyka ma postać `error: FILE:LINE:COL: ...`, którą `compilation-mode` potrafi sparsować, więc
`next-error` / `C-x \`` przeskakuje prosto do miejsca.

## Instalacja

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Pliki `.typl` otwierają się automatycznie w `typelisp-mode` (tryb jest zarejestrowany w `auto-mode-alist`).

Z `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Serwer języka (`typl-lsp`)

Po zbudowaniu `typl-lsp` można go używać z `eglot` (wbudowanego w Emacsa 29+) lub `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Z `eglot`:

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

Obsługiwane: diagnostyka (błędy składni/typów i ostrzeżenia o redefinicji, wysyłane przez
`textDocument/publishDiagnostics`), hover, przejście do definicji, uzupełnianie (`:` jest zarejestrowany jako
znak wyzwalający) oraz tokeny semantyczne. Odwołania między plikami przez `use` są rozstrzygane (serwer
szuka w górę katalogu głównego projektu z `typelisp.toml`; szczegóły w
[Referencji składni 3.11](../../docs/pl/reference/syntax.md#311-pliki-i-moduły-projekty-wieloplikowe)).
Niezapisane zmiany w otwartych buforach edytora są od razu odzwierciedlane w diagnostyce zarówno plików,
od których zależą, jak i plików, które od nich zależą.

### Podświetlanie nazw typów (tokeny semantyczne)

Przez `textDocument/semanticTokens` serwer zgłasza **pozycje, które moduł sprawdzający faktycznie
rozstrzygnął jako nazwy typów**. Ponieważ nie jest to dopasowywanie tekstu:

- Kolorowane są także typy pochodzące z innych plików przez `use` (zakres, do którego rozstrzyganie
  w obrębie bufora z zasady nie sięga)
- Wywołania **funkcji** o tej samej nazwie co typ nie są kolorowane (moduł sprawdzający rozstrzygnął
  je jako funkcje, więc w ogóle nie jest tam zapisywany żaden token)

Po stronie klienta:

- **`eglot` (Emacs 31 i nowszy)**: eglot rysuje tokeny samodzielnie (`eglot-semantic-tokens-mode`).
  `typelisp-mode` nie wchodzi mu w drogę
- **`eglot` (Emacs 30 i starszy)**: ta wersja eglot nie obsługuje semanticTokens. Dlatego
  **`typelisp-mode` sam wysyła żądanie i rysuje wynik za pomocą nakładek (overlays)**
  (`typelisp-semantic-tokens-mode`, włączany automatycznie po połączeniu eglot)
- **`lsp-mode`**: obsługa natywna (ustaw `lsp-semantic-tokens-enable` na `t`). W takim przypadku
  `typelisp-mode` nie wchodzi mu w drogę

`scripts/emacs-semantic-smoke.el` naprawdę łączy się przez eglot i sprawdza stronę, która
rysuje, w używanym Emacsie. Przy dowolnym kliencie rozwiązanie zapasowe w obrębie bufora ustępuje, gdy
serwer odpowiada (aby dwa zestawy reguł nie malowały tego samego bufora).

| Ustawienie | Domyślnie | Co robi |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | W eglot Emacsa 30 i starszego określa, czy kolorować z tokenów semantycznych serwera |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Sekundy bezczynności po edycji przed ponownym żądaniem (utrzymuj większe niż `eglot-send-changes-idle-time`) |

## Uwagi

- typelisp zamienia symbole na małe litery przy wczytywaniu, ale podświetlanie rozróżnia wielkość liter, aby można było odróżnić nazwy typów
  zaczynające się wielką literą.
- Wcięcia są ustalane przez dedykowaną `typelisp-indent-function`, która wyszukuje
  `typelisp-indent-specs` (alist). Tryb utrzymuje własne wpisy nawet dla form, których nazwy
  dzieli z Emacs Lisp (`defun` `let` `if` ...), ponieważ właściwości symboli są **globalne**, a
  ustawienia typelisp tam zmieniłyby wcięcia innych buforów Lisp w tej samej sesji.
  Ponadto formy typelisp różnią się kształtem, nawet gdy dzielą nazwę z Emacs Lisp:
  `(defun NAME (PARAMS) RETTYPE ...)` ma trzy elementy nagłówka, a `if` ma stałe trzy
  elementy z obowiązkowym `else`. Zatem wartości też nie można współdzielić.
  Każdy plik `.typl` w `examples/` został sprawdzony: `indent-region` nie zmienia ani jednego bajta,
  a spłaszczenie wszystkich wcięć i ponowne wcięcie odtwarza oryginał (wersja VS Code spełnia
  ten sam standard na tych samych plikach).

## Wykrywanie rozjazdu definicji edytorów

Tablice słów kluczowych są utrzymywane dwa razy, raz tutaj i raz w wersji VS Code. Aby zapobiec
pozostawaniu definicji edytorów w tyle, gdy implementacja idzie naprzód, istnieje test po stronie
Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Faktycznie ładuje prelude, przechodzi po rejestrze i zgłasza **nazwy, których któryś z edytorów nie
zna**. Formy specjalne nie mają reprezentacji w czasie działania, więc są odczytywane spomiędzy
`// SPECIAL-FORM DISPATCH BEGIN` / `END` w `crates/typelisp-front/src/check/checker.rs` (nie usuwaj
tych komentarzy). Jeśli test zawiedzie, dodaj zgłoszone nazwy do **obu** definicji edytorów.

Ten sam test porównuje także legendę tokenów semantycznych (`SEMANTIC_TOKEN_TYPES` w
`src/bin/lsp.rs` oraz tablice, które przechowują oba edytory, muszą zgadzać się co do nazw i kolejności). Niezgodność nie powoduje
błędu w czasie działania; tylko zamienia kolory każdego tokenu, więc jest przypięta mechanicznie.
