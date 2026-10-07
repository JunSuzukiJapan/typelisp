<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Moduły i układ plików

Ten przewodnik wyjaśnia, jak złożyć program składający się z kilku plików. Szczegółowe reguły znajdują się w
sekcjach od 3.10 do 3.13 [Referencji składni](../reference/syntax.md#310-module--use--przestrzenie-nazw).

## 1. Jeden plik to jeden moduł

W typelisp **plik sam w sobie jest modułem**. Ścieżka pliku względem korzenia źródeł jest
ścieżką modułu.

| Plik | Moduł |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Na początku pliku nie trzeba pisać deklaracji modułu.

## 2. Konfigurowanie projektu

Umieść plik o nazwie `typelisp.toml` w korzeniu projektu. Może być pusty.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Aby trzymać źródła w `src/`, wpisz do `typelisp.toml` tę jedną linię:

```toml
src = "src"
```

`typl` szuka `typelisp.toml`, zaczynając od katalogu uruchamianego pliku i idąc w górę, a
miejsce, w którym go znajdzie, przyjmuje jako korzeń źródeł. Jeśli nic nie zostanie znalezione, korzeniem jest katalog
uruchamianego pliku (w REPL bieżący katalog).

## 3. Udostępnianie definicji i ich używanie

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; pola bez pub nie można odczytać z zewnątrz

(defun square ((n i32)) i32 (* n n))   ; funkcji bez pub też nie można wywołać z zewnątrz

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` jest ładowany w miejscu, w którym zapisano `(use geometry)`. Nie trzeba go ładować
wcześniej.

### Co jest udostępniane

- Funkcje, struktury, enumeracje, zmienne globalne, makra i metody są widoczne z innych modułów tylko
  wtedy, gdy mają `pub`. Umieść `pub` tuż przed definicją, jak w `(pub defun ...)`.
- W przypadku struktur **udostępnienie typu i udostępnienie pól to dwie osobne sprawy**.
  `(pub defstruct point ...)` czyni typ widocznym, a z zewnątrz można odczytywać i zapisywać tylko
  pola zapisane jako `(pub x i32)`.
- Użycie z zewnątrz nazwy, która nie jest publiczna, daje błąd „nie można rozwiązać", taki jak
  `unresolved path: geometry::square`. To ten sam komunikat, co przy błędnie zapisanej nazwie, więc jeśli
  pisownia jest poprawna, a nazwa nadal się nie rozwiązuje, podejrzewaj brakujące `pub`.

Lista definicji, które mogą przyjmować `pub`, znajduje się w
[Referencji składni 3.13](../reference/syntax.md#313-pub--widoczność).

## 4. Jak pisać `use`

```lisp
(use geometry)              ; wprowadź moduł; pisz geometry::dist2, aby go użyć
(use geometry::dist2)       ; wprowadź funkcję; używaj jej pod samą nazwą dist2
(use geometry::point)       ; wprowadź typ; pisz point::new, point::origin oraz point w adnotacjach typów
(use a::f b::g)             ; można zapisać kilka naraz
```

- **`use` działa tylko dla form po nim.** Umieść je na początku pliku. Zapisanie
  `geometry::dist2` powyżej `use` daje `unresolved path`.
- Zapisanie pełnej ścieżki `geometry::dist2` bez `use` dla modułu również się nie rozwiązuje. Tylko
  `use` powoduje załadowanie pliku.
- Wprowadzenie przez `use` nazwy, której sama postać jest już zajęta, daje ostrzeżenie. Gdy mimo to chcesz ją
  wprowadzić, użyj `shadowing-import`.
- Moduł wewnątrz katalogu zapisuje się `(use geo::shapes)`, a potem odwołuje się do niego przez jego
  ostatnią część (`shapes::...`).

### Wywoływanie metod traitów

Metody zaimplementowane w `impl` **należą do typu**, a nie do funkcji modułu, więc
wywołuje się je bez nazwy modułu.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, a nie core::area
```

Metody wewnątrz `impl` są zawsze publiczne, nawet bez `pub`.

Samego traitu nie można udostępnić innym modułom. Definicję traitu, jego `impl` oraz kod
używający go przez `:dyn` trzymaj w jednym module.

## 5. Dzielenie przestrzeni nazw w obrębie pliku

Aby dalej podzielić przestrzeń nazw w obrębie jednego pliku, użyj `module`. Jest zagnieżdżony wewnątrz własnego
modułu pliku.

```lisp
;; wewnątrz main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Aby umieścić całą resztę pliku w jednej przestrzeni nazw, możesz napisać `(in-module util)` zamiast
otaczać ją nawiasami.

## 6. Ograniczenia zależności

- **Cykle są niedozwolone.** Jeśli `a.typl` robi `(use b)`, a `b.typl` robi `(use a)`, wynikiem jest
  błąd `circular module dependency: a -> b -> a`. Przenieś definicje potrzebne obu do trzeciego
  modułu.
- **Do typów ani do funkcji nie można odwoływać się przed ich zdefiniowaniem**, nawet w obrębie tego samego
  pliku. Dla funkcji wzajemnie rekurencyjnych zadeklaruj jedną z nich wcześniej za pomocą `defsignature`
  ([Referencja składni 3.2](../reference/syntax.md#32-defsignature--deklaracje-wyprzedzające)).

## 7. Kolejność wykonywania

Uruchomienie `typl main.typl` przebiega w tej kolejności:

1. `main.typl` i każdy plik wprowadzony przez niego za pomocą `use` są wczytywane i sprawdzane pod względem typów. **Jeśli
   gdziekolwiek jest błąd typu, nic się nie uruchamia.**
2. Wyrażenia najwyższego poziomu modułów wprowadzonych przez `use` wykonują się przed wyrażeniami modułów, które ich używają.
3. Wyrażenia najwyższego poziomu `main.typl` wykonują się od góry do dołu.

Jeśli punkt wejścia programu zgromadzisz w funkcji `main` i wywołasz `(main)` na końcu
pliku, ten sam plik można też wykorzystać do
[kompilacji AOT](compile.md#3-budowanie-pliku-wykonywalnego-za-pomocą-kompilacji-aot).

## 8. Czym to się różni od `load`

`(load "ścieżka")`, podobnie jak `load` z Common Lisp, wczytuje zawartość pliku **do bieżącej
przestrzeni nazw bez zmian**. Nie opakowuje jej w moduł, a `pub` nie odgrywa żadnej roli. Używaj go do
takich rzeczy jak wczytanie pliku ustawień czy ponowne załadowanie lokalnego pliku w REPL. Aby podzielić program na części,
użyj `use`.
