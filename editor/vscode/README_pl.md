<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Rozszerzenie VS Code do edycji kodu źródłowego typelisp (`.typl`).
Wersja dla Emacsa znajduje się w [../emacs/](../emacs/README_pl.md). Obie dzielą te same tablice słów kluczowych i
te same reguły wcięć, a `cargo test --test editor_keyword_sync_test` sprawdza to
mechanicznie (zobacz niżej).

## Funkcje

- **Podświetlanie składni** (gramatyka TextMate; serwer języka nie jest potrzebny)
  - Formy specjalne i konstrukcje sterujące (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, rodzina `pprint`)
  - Definiowane nazwy (`(defun NAME ...)` jako funkcja, `(defstruct NAME ...)` jako typ,
    `(defvar (NAME ...))` jako zmienna; to samo z `pub`, jak w `(pub defun NAME ...)`) oraz
    obie nazwy w `(impl Trait Type)`
  - Słowa kluczowe przestrzeni nazw i deklaracji (`pub` `module` `use` `load` `impl` `where`) oraz znaczniki
    listy lambda (`&rest` `&optional` `&key`)
  - Funkcje wbudowane, typy prymitywne (w tym `bignum` / `ratio`), wbudowane typy błędów,
    typy użytkownika pisane wielką literą (`Capitalized`) oraz typ obiektu traitu `:dyn Trait` (także wewnątrz argumentów
    generycznych)
  - Literały liczbowe (dziesiętne / `0xff` / `1.5` / `3.0e10` / `1/3`), literały znakowe, takie jak
    `#\Space`, słowa kluczowe, takie jak `:name`, oraz zmienne globalne z gwiazdkami, takie jak `*print-pretty*`
  - **Dyrektywy sterujące `format` wewnątrz łańcuchów znaków** (`~a` `~5,'0d` `~{...~}` `~^` i tak dalej)
  - Komentarze liniowe `;` i **zagnieżdżalne** komentarze blokowe `#| ... |#`
- **Użycia typów zdefiniowanych przez użytkownika** (tokeny semantyczne)
  - Nazwy `defstruct` / `defenum` / `deftrait` są zwykle pisane małymi literami (`rect` `todo-item`
    `board`), więc reguła `Capitalized` ich nie obejmuje, a gramatyka TextMate działa
    linia po linii i nie widzi całego pliku. Tokeny semantyczne to potrafią, co naprawia sytuację, w której
    język typowany statycznie zostawiał niepokolorowane tylko adnotacje typów
  - Po połączeniu z `typl-lsp` rozszerzenie otrzymuje **pozycje, które moduł sprawdzający faktycznie
    rozstrzygnął jako nazwy typów**. Dzięki temu kolorowane są typy pochodzące z innych plików przez `use`, a
    wywołania **funkcji** o tej samej nazwie co typ nie są (moduł sprawdzający rozstrzygnął je jako
    funkcje, więc w ogóle nie jest tam zapisywany żaden token)
  - Gdy serwer nie jest połączony lub nie został zbudowany, rozszerzenie wraca do skanowania tekstu, które
    rozstrzyga w obrębie pliku. Jest to przybliżenie: nie znajduje typów z innych plików
    i nie odróżni funkcji o tej samej nazwie co typ
- **Wcięcia Lisp** (VS Code nie ma wbudowanych wcięć dla Lisp, więc rozszerzenie je implementuje)
  - Formatowanie dokumentu, formatowanie zaznaczenia i formatowanie podczas pisania (Enter i `)`, gdy
    włączone jest `editor.formatOnType`)
- **Konspekt (Outline) / ścieżka okruszków / `Ctrl+Shift+O`** (funkcje, metody, makra, typy, traity, `impl`,
  zmienne, moduły)
- **Integracja z `typl-lsp`** (diagnostyka, hover, przejście do definicji, uzupełnianie, tokeny semantyczne)
- **Polecenia CLI `typl`** (uruchamianie, REPL)

Wszystko poza serwerem języka działa samo z rozszerzeniem, więc nawet w kopii repozytorium,
w której nie zbudowano `typl-lsp`, dostępne są podświetlanie, wcięcia, konspekt i (w obrębie pliku) podświetlanie
typów.

## Instalacja

Rozszerzenia nie ma w Marketplace, więc zbuduj je lokalnie i zainstaluj.

```sh
cd editor/vscode
npm install
npm run compile
```

Następnie:

- **Wypróbuj w hoście deweloperskim**: otwórz `editor/vscode` w VS Code i naciśnij `F5`
- **Zainstaluj na stałe**: utwórz `.vsix` poleceniem `npx @vscode/vsce package`, a potem użyj
  „..." → „Install from VSIX..." w widoku Extensions

Pliki `.typl` otwierają się automatycznie w trybie typelisp.

## Skróty klawiszowe

| Klawisz | Polecenie | Co robi |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Zapisuje i uruchamia `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Uruchamia REPL `typl` |

Paleta poleceń zawiera także `typelisp: Restart Language Server`.

## Ustawienia

| Ustawienie | Domyślnie | Co robi |
|---|---|---|
| `typelisp.program` | `typl` | Ścieżka do CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Czy łączyć się z `typl-lsp` |
| `typelisp.languageServer.path` | (puste) | Ścieżka do `typl-lsp`. Gdy puste, rozszerzenie szuka `target/release/typl-lsp` w obszarze roboczym, potem `target/debug/typl-lsp`, a następnie w `PATH` |
| `typelisp.trace.server` | `off` | Zapisuje w dzienniku ruch LSP JSON-RPC |

Serwer języka buduje się poleceniem:

```sh
cargo build --release --bin typl-lsp
```

Odwołania między plikami przez `use` są rozstrzygane przez wyszukiwanie w górę katalogu głównego projektu z
`typelisp.toml` (szczegóły w
[Referencji składni 3.11](../../docs/pl/reference/syntax.md#311-pliki-i-moduły-projekty-wieloplikowe)).

## Dopasowywacz problemów zadania

Rozszerzenie udostępnia dopasowywacz problemów (problem matcher) o nazwie `typelisp`. `typl` wypisuje diagnostykę w postaci
`error: FILE:LINE:COL: message`, więc mogą one trafiać prosto do panelu Problems:

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

## Rozwój

```sh
npm run compile   # tsc
npm run watch     # buduj przy zmianie
npm test          # node --test (gramatyka, wcięcia, symbole, odwołania do typów, manifest)
```

Testy obejmują tylko te części, które nie potrzebują modułu `vscode`. Dlatego `src/indent.ts` i
`src/symbols.ts` są napisane jako czyste funkcje, a tylko `src/extension.ts` dotyka API edytora.

- `src/test/grammar.test.ts` — naprawdę tokenizuje za pomocą gramatyki, tym samym silnikiem co VS
  Code (`vscode-textmate` + `vscode-oniguruma`), i sprawdza wynik.
  Oniguruma różni się od wyrażeń regularnych Emacsa w szczegółach (na przykład nie traktuje
  `]` na początku klasy znaków jako literału), a takie różnice można znaleźć tylko
  uruchamiając prawdziwy silnik.
- `src/test/indent.test.ts` — dla każdego pliku `.typl` w `examples/` wymaga, aby
  **spłaszczenie wszystkich wcięć i ich odtworzenie zgadzało się z zatwierdzoną zawartością co do bajta**.
  Tryb Emacsa spełnia ten sam standard na tych samych plikach i to sprawia, że „oba
  edytory się zgadzają" jest twierdzeniem zweryfikowanym.
  Ponadto `src/test/fixtures/emacs-indent-reference.txt` to wyjście referencyjne zebrane przez
  faktyczne uruchomienie `indent-region` w buforze `typelisp-mode` Emacsa. Strona oczekiwana nie jest
  powtórzeniem implementacji TS, lecz **tym, co faktycznie wytwarza drugi edytor**, więc
  wierność przeniesienia jest sprawdzana bezpośrednio (obejmuje `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, prefiksy quote i więcej).
- `src/test/symbols.test.ts` — zawartość konspektu i wykrywanie odwołań do typów przez rozwiązanie zapasowe. Liczba definicji musi dokładnie zgadzać się z niezależnym zliczeniem form
  definicji na początku linii. Reguły granic odwołań do typów są celowo wyrównane
  z rozwiązaniem zapasowym wersji Emacsa (VS Code używa lookbehind; Emacs wyraża ten sam zbiór
  przez zużycie jednego poprzedzającego znaku).
- Tokeny serwera napędzane rozstrzyganiem (`crates/typelisp-front/src/check/semantic.rs`) są
  sprawdzane przez `cargo test --test lsp_semantic_test` oraz `scripts/lsp-semantic-smoke.py` (który
  steruje prawdziwym procesem przez stdio). Klient Emacsa jest sprawdzany przez
  `scripts/emacs-semantic-smoke.el` przez prawdziwe połączenie eglot.
- `src/test/manifest.test.ts` — `package.json` to jedyna część, której kompilator nie sprawdza, więc ten test
  sprawdza, że zadeklarowane polecenia i wywołania `registerCommand` to ten sam zbiór, do czego odnoszą się skróty
  klawiszowe, że ustawienia czytane przez kod są zadeklarowane oraz że dopasowywacz problemów
  potrafi sparsować to, co `typl` faktycznie wypisuje.

### Wykrywanie rozjazdu definicji edytorów

Tablice słów kluczowych są utrzymywane dwa razy, w wersji Emacsa i w wersji VS Code. Aby zapobiec
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

## Uwagi

- typelisp zamienia symbole na małe litery przy wczytywaniu, ale podświetlanie rozróżnia wielkość liter, aby można było odróżnić nazwy typów
  zaczynające się wielką literą.
- Wcięcia są ustalane przez `INDENT_SPECS` w `src/indent.ts`. Jest to przeniesienie
  `typelisp-indent-specs` z wersji Emacsa, z tymi samymi wartościami i regułami. Miejsca, w których forma różni się kształtem
  od formy Emacs Lisp o tej samej nazwie, są przenoszone bez zmian: nagłówek
  `(defun NAME (PARAMS) RETTYPE ...)` ma trzy elementy, `if` ma stałe trzy elementy z
  obowiązkowym `else` i tak dalej.
- Zawartość `#| ... |#` jest ponownie wcinana przy formatowaniu. Zgadza się to z zachowaniem
  `indent-region` w Emacsie.
