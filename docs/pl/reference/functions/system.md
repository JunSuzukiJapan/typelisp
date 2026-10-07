<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Czas, środowisko i implementacja

Funkcje do obsługi czasu, zapytania o środowisko uruchomieniowe, narzędzia implementacji, parsowanie i
obliczanie tekstu, docstringi oraz makra.

## 1. Czas

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Dwa pola: `day` (dni od 1900-01-01) i `second` (sekunda w obrębie tego dnia, 0..86399) |
| `internal-time` | — | `defstruct` | Dwa pola: `second` i `microsecond` (w obrębie tej sekundy, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Czas od epoki CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Czas, który upłynął, względem procesu |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | **Czas CPU** zużyty przez ten proces (użytkownika plus systemu) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Jako liczba sekund. Forma do raportowania różnicy między dwoma odczytami |
| `internal-time-units-per-second` | — | `int` | `1000000` (mikrosekundy), jednostka pola `microsecond`. Jak w CL, wartość jest wyborem implementacji |
| `time` | `(time form)` | Makro | Wykonuje `form`, wypisuje czas rzeczywisty i czas CPU po jednej linii i zwraca wartość `form` bez zmian |

Czas rzeczywisty i czas CPU mówią o różnych rzeczach. Dla pracy, która głównie czeka na I/O, oba mocno się
różnią, a ta różnica jest dokładnie tym, co chcesz wiedzieć, więc `time` pokazuje oba.

`sleep`, które zatrzymuje zadanie, znajduje się w [Zadaniach i kanałach](concurrency.md#3-yield--sleep--ustępowanie).

## 2. Dekodowanie i kodowanie dat

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Dziewięć pól**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Dziewięć wartości zwracanych przez CL jako jedna struktura (nie ma wielu wartości) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Czas uniwersalny na składniki kalendarza. `zone` to godziny na zachód od Greenwich (ten sam kierunek co w CL). **Pominięte oznacza czas lokalny** (jak w CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Odwrotność. Bez `zone` argumenty są odczytywane jako **czas lokalny** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Teraz, zdekodowane w czasie lokalnym |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Przesunięcie czasu lokalnego na zachód od Greenwich, w **sekundach**, dla tego czasu uniwersalnego |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Czy w tym czasie uniwersalnym obowiązywał czas letni |

Jak w CL, dla `day-of-week` **0 to poniedziałek, a 6 to niedziela**.

**Bez `zone` używany jest czas lokalny**, jak w CL. Lokalne przesunięcie jest pobierane od systemu, więc wynik
zależy od tego, gdzie znajduje się maszyna. **Podanie jawnej strefy czyni wynik deterministycznym**, a `0` to UTC.

Jednostką `zone` jest, jak w CL, „godziny na zachód od Greenwich", więc UTC+9 zapisuje się jako `-9`. Jednak **argument
jest liczbą całkowitą, a pole `zone` wyniku jest `f64`**. Rzeczywiste przesunięcia nie zawsze są
całymi godzinami (Indie to +5:30, Nepal +5:45), a zaokrąglenie zgłaszanej wartości po cichu by kłamało.
Strefa zapisywana ręcznie to całkowita liczba godzin, więc argument jest typu `int`.

Gdy podano `zone`, `daylight-p` jest `false`, a `zone` to dokładnie podana wartość, jak określa CL
(*If a time-zone is supplied, daylight saving time information is ignored*).

Czas lokalny wypadający w przejściu na czas letni nie jest w ogóle jednoznaczny, a CL
nie mówi, który wybrać. `encode-universal-time` zwraca jedną z dwóch odpowiedzi dla takiego czasu.

## 3. Środowisko uruchomieniowe

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Wiersz poleceń. **Element 0 to nazwa programu** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Zmienna środowiskowa. `none`, jeśli nieustawiona lub nie jest UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. Podstawa `user-homedir-pathname` ([Nazwy ścieżek](streams-files.md#92-funkcje)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Wersja implementacji |
| `machine-type` | `(machine-type)` | `()→string` | Architektura CPU (`x86_64` / `aarch64` …). Wartość **celu kompilacji** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Nazwa hosta |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Nazwa sprzętu, **na którym teraz działa** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` tam, gdzie nie da się ustalić |
| `software-type` | `(software-type)` | `()→string` | System operacyjny (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | Wydanie systemu operacyjnego (`uname -r`, na przykład `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Krótka nazwa miejsca instalacji. **Zawsze `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Podobnie, długa nazwa. **Zawsze `none`** |

Te zwracające `Option` to pozycje, dla których CL dopuszcza `NIL` (*or nil if no such name can be
determined*). POSIX nie ma miejsca na zapisanie nazw lokalizacji, więc są zawsze `none`; SBCL zwraca to
samo. Zwróć uwagę na różnicę między `machine-type` a `machine-version`: pierwsze to architektura,
dla której ten plik binarny został **zbudowany**, drugie to układ, na którym teraz **działa**.

Element 0 z `command-line-args` to ścieżka skryptu dla `typl script.typl a b` oraz sam
plik wykonywalny dla pliku wykonywalnego AOT uruchomionego jako `./prog a b`. **Każdy ze sposobów uruchomienia odczytuje te same argumenty
pod tymi samymi indeksami** (`typl` usuwa własną nazwę i opcje, takie jak `--heap-cells`, zanim przekaże
je dalej).

## 4. Pytanie użytkownika

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Przyjmuje pojedyncze `y` / `n`. Pyta ponownie, aż je dostanie |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Zmusza użytkownika do wpisania `yes` / `no`. Dla pytań, w których pomyłka jest kosztowna |

Obie czytają z `*standard-input*`. Ponowne pytanie zatrzymuje tylko koniec wejścia, a wtedy wynik to
`false`.

## 5. Narzędzia implementacji (CLHS 25.2)

Warstwa, w której implementacja odpowiada na pytania o samą siebie. `heap-info` / `room` / `dribble`
to zwykłe funkcje; `trace` / `untrace` / `step` / `disassemble` / `ed` to **formy specjalne**
(`trace` / `untrace` / `disassemble` / `ed` przyjmują *nazwę* definicji, a `step` *formę*, wszystko
nieobliczone).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Bieżący stan sterty jako struktura. Te same liczby, które wypisuje `room` |
| `room` | `(room &optional verbose)` | `(bool)→()` | Raportuje `heap-info` do `*standard-output*`. `(room true)` daje więcej szczegółów |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Rozpoczyna zapisywanie wyjścia sesji do `path` / przerywa zapisywanie, gdy wywołane bez argumentu |
| `trace` | `(trace name...)` | `Sexpr` | Raportuje wywołania wskazanych definicji do `*trace-output*`. Zwraca listę nazw, które są teraz śledzone |
| `untrace` | `(untrace name...)` | `Sexpr` | Przestaje raportować. **Bez argumentów usuwa wszystkie** |
| `step` | `(step form)` | Typ `form` | Oblicza `form`, zatrzymując się przy każdym wywołaniu, aby zapytać |
| `disassemble` | `(disassemble name [llvm])` | `()` | Wypisuje to, czym staje się ta definicja. Domyślnie kod maszynowy hosta, z `true` LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Uruchamia `$VISUAL` / `$EDITOR`. Po podaniu nazwy otwiera linię, w której ta definicja jest zapisana |

`trace`/`untrace`/`step`/`disassemble` działają tylko w interpreterze, a funkcji, które je wywołują, nie można
skompilować ([Referencja składni, rozdział 10](../syntax.md#10-kompilacja)).

### 5.1 Pola `heap-info`

| Pole | Typ | Zawartość |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Cała arena cons i jej podział. Zawsze `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Bieżące liczby trzech pozostałych rodzajów obiektów sterty |
| `gc-count` | `int` | Liczba odśmiecań od startu implementacji |
| `growable` | `bool` | Czy arena nadal może rosnąć |

Wszystkie pola są typu `int` (poza `growable`). Limit wzrostu (zobacz opis
`typl --heap-cells`) nie jest raportowany, ponieważ czytelnik chce wiedzieć, czy może jeszcze rosnąć
(`growable`).

### 5.2 Co `trace` / `step` widzą, a czego nie

- **Definicje ze skompilowanymi ciałami są także widoczne, z miejsc wywołań, które są interpretowane.**
- **Miejsca wywołań *wewnątrz* skompilowanego kodu nie są widoczne.** Śledzenie nazwy, która ma skompilowane ciało, dodaje
  jednoliniową uwagę o tym. To samo ograniczenie opisuje SBCL dla wywołań lokalnych.
- **Wywołania przez wartości domknięć (`funcall`/`apply`) nie są widoczne.** Domknięcia nie mają nazw.
- **Definicje generyczne nie są objęte.** Kopia dla każdego typu jest tworzona w każdym miejscu użycia, więc
  nie ma jednego ciała, które można by nazwać (ten sam powód i to samo sformułowanie, co gdy `compile` odmawia).

Poleceniami `step` są `s` (wejdź w to wywołanie; pusta linia robi to samo), `n` (pomiń to
wywołanie), `c` (przestań pytać od tego miejsca) i `q` (przerwij). **Jeśli standardowe wejście nie jest terminalem, `step`
po prostu oblicza `form`**: zdegenerowane zachowanie, które CLHS wyraźnie dopuszcza, aby skrypty i testy nie
zawieszały się na zachęcie, na którą nikt nie może odpowiedzieć.

`$VISUAL` / `$EDITOR` w `ed` jest dzielone na białych znakach, więc `EDITOR="code -w"` działa. Jeśli żadna
nie jest ustawiona, wynikiem jest `Err`: nie zgaduje `vi`. Numer linii jest przekazywany jako pierwszy, w postaci `+N`.

`dribble` zapisuje wszystkie trzy drogi, którymi wyjście sesji opuszcza proces: to, co zapisują
`print`/`println`/`format`, to, co jest zapisywane do strumieni połączonych ze standardowym wyjściem, oraz
linie wpisane do REPL razem z wartościami, które REPL wypisuje z powrotem.

## 6. Parsowanie i obliczanie

Wszystkie z nich obsługują tekst i dane z czasu działania (nad którymi sam program nie ma kontroli), więc w razie niepowodzenia
zwracają `Err` z `Result`, a nie powodują panic. Typy błędów to konkretne typy
dla każdej operacji ([Typy błędów](option-result.md#3-typy-błędów-i-trait-error)).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | `parse-integer` z CL. Pomija początkowe i końcowe białe znaki (ten sam zbiór co w `trim`), czyta co najwyżej jeden znak `+`/`-`, a potem cyfry w podstawie `radix` (domyślnie 10, od 2 do 36; cyfry powyżej 10 w obu wielkościach liter). Nie ma limitu liczby cyfr (`int`). Wszelkie inne pozostałe znaki dają `Err`. Z `:junk-allowed true` zatrzymuje się na pierwszej nie-cyfrze i ignoruje resztę, ale daje `Err`, jeśli nie ma ani jednej cyfry (odpowiada `nil` z CL). Nie zwraca drugiej wartości z CL (pozycji, na której zakończył się odczyt). `radix` spoza zakresu powoduje panic (błąd wywołującego, a nie tekstu) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Liczba zmiennoprzecinkowa. Akceptuje także `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Wczytuje jedno `Sexpr` z `s` (tym samym czytnikiem, który czyta kod źródłowy). Niezbalansowane nawiasy, niezakończone łańcuchy znaków i tym podobne dają `Err`. Odczyt ze strumienia to `read-sexpr` ([Strumienie](streams-files.md#6-funkcje-generyczne-i-operacje-na-plikach)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **pozycja, na której zakończył się odczyt**. `(car r)` to wartość, a `(cdr r)` pozycja następnego znaku do odczytania. `start` domyślnie wynosi 0 |
| `read-from-string-preserving-whitespace` | Jak wyżej | Jak wyżej | To samo, ale nie zużywa białego znaku, który zakończył daną. Różnica widoczna jest w zwróconej pozycji |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Sprawdza typy `form` w czasie działania i je oblicza. Idzie za `eval` z CL |

CL zwraca **dwie wartości** (wartość i pozycję) z `read-from-string`, ale ten język nie ma
wielu wartości, więc zwraca jeden `cons-cell`. Posiadanie pozycji sprawia, że czytanie łańcucha znaków po jednej danej
naraz jest pętlą, a nie ponownym skanowaniem:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Różnica powodowana przez `preserving-whitespace` to **jeden biały znak**: `read` z CL zużywa
biały znak, który zakończył daną, a `read-preserving-whitespace` go zostawia. `(read-from-string "12 34")`
zwraca pozycję 3, a wersja zachowująca zwraca 2.

Składnia liczb akceptowana przez czytnik znajduje się w [Referencji składni, rozdział 1](../syntax.md#1-elementy-leksykalne).
To, co wypisuje `*print-radix*` ([Wypisywanie](printing.md#62-podstawa-wielkość-liter-i-czytelność)), można wczytać z powrotem bez zmian.
Nie ma `*read-base*` z CL.

### 6.1 Co oznacza `eval`

Idzie za CLHS `eval`: oblicza w **bieżącym środowisku globalnym** (funkcje globalne,
zmienne, typy i makra, w tym definicje dodane w czasie działania) oraz w **zerowym środowisku leksykalnym**
(lokalne wiązania `let`/`lambda` wywołującego nie są widoczne). Można obliczać zarówno wyrażenia, jak i
definicje (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`), a definicje są
rejestrowane w środowisku globalnym natychmiast i na stałe.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; globalne x jest widoczne
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; zwraca zdefiniowaną nazwę
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; dopiero co utworzona definicja jest widoczna
```

- **Wartość zwracana**: dla wyrażenia wynik jako `Option<Sexpr>`; dla definicji symbol
  zdefiniowanej nazwy (jak w CL). Aby użyć wyniku, rozbierz `Sexpr` za pomocą `match`
  (`(int n)`/`(str s)`/…).
- **Różnice wynikające z typów statycznych (ważne)**: CL zwraca faktyczną wartość wyniku, ale w
  tym języku typ zwracany może być tylko jednolicie `Result<Option<Sexpr>,EvalError>`. Ponadto **kod
  napisany statycznie nie może odwoływać się wprzód do nazw, które `eval` definiuje w czasie działania**: `(sq 9)` zapisane
  bezpośrednio w pliku jest sprawdzane przed wykonaniem `eval` definiującego `sq` i jest „niezdefiniowane". Jednak
  **późniejsze `eval` je widzą** (ich sprawdzanie typów działa w czasie działania, po definicji). REPL sprawdza
  i wykonuje po jednej linii, więc nazwę zdefiniowaną za pomocą `eval` można wywołać bezpośrednio z następnej linii.
- **Błędy**: błędy typów i błędy składni zwracają `Err` (nie powodują panic). **Panic w czasie działania** w
  obliczanym kodzie (dzielenie przez zero i tak dalej) propagują się tak, jak z kodu napisanego bezpośrednio. Kod sprzątający
  każdego `unwind-protect` pomiędzy jest wykonywany
  ([Referencja składni, rozdział 8](../syntax.md#8-wyjścia-nielokalne-catch--throw--unwind-protect)).
- **Przestrzeń nazw**: gdy uruchamia go `typl file.typl` oraz wewnątrz pliku wykonywalnego AOT, `eval` oblicza w
  przestrzeni nazw modułu skryptu (własne zmienne globalne skryptu są widoczne). REPL oblicza w
  przestrzeni nazw korzenia.
- **Kompilacja**: zarówno `read`, jak i `eval` można skompilować. Sposób ich obsługi w plikach wykonywalnych AOT oraz
  konsekwencje (formy przekazane do eval są interpretowane) opisano w
  [Referencji składni 10.2](../syntax.md#102-eval-w-plikach-wykonywalnych-aot).

## 7. Docstringi / `documentation`

`defun`/`defmethod` (także wewnątrz `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` mogą mieć docstringi. Pozycja idzie za regułą CL dla każdej z nich:

| Forma | Pozycja docstringu |
|---|---|
| `defun` / `defmethod` / `defmacro` | Na początku ciała (po typie zwracanym i klauzuli `where`). Tylko gdy następuje co najmniej jedna forma ciała; sam łańcuch pozostaje wartością zwracaną |
| `defvar` / `defconstant` | **Po** wartości początkowej: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Zaraz po** nazwie, przed polami/wariantami |
| `deftype` | **Zaraz po** nazwie, przed typem: `(deftype meters "doc" i32)` |
| `deftrait` | Zaraz po liście supertraitów, przed pozycjami. Jeden dla całego traitu. **Metody z implementacją domyślną** mogą umieścić własny docstring tuż przed swoim ciałem |

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `documentation` | `(documentation name)` | (forma specjalna; `name` to goły symbol lub `Type::method`)→`Option<string>` | Zwraca docstring `name` |

Tak jak `quote`/`compile`, `documentation` jest formą specjalną (czyta `name` jako nieobliczoną nazwę).
W przeciwieństwie do `(documentation 'name 'function)` z CL nie przyjmuje argumentu typu; zamiast tego rozstrzyga gołą
nazwę w kolejności **zmienna → funkcja → typ → trait → makro** (ten sam priorytet co dla gołego
identyfikatora obliczanego jako wyrażenie). Forma `Type::method` wyszukuje docstring metody powiązanej
lub statycznej.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Wartość jest ustalana w czasie sprawdzania**: jeśli nazwa nie rozwiązuje się do żadnej definicji, jest to
błąd czasu sprawdzania (jak odwołanie do niezdefiniowanej zmiennej). Jeśli się rozwiązuje, ale nie ma docstringu,
wynikiem jest `Option::none`.

**Nieobjęte**:

- `(setf documentation)` (zmiana docstringu w czasie działania) nie istnieje.
- Wolne nazwy kwalifikowane modułem (`mod::name`; `Type::method` jest obsługiwane) nie są obsługiwane.
- Deklaracja metody w `deftrait` **bez ciała** nie może mieć docstringu. Końcowy literał łańcuchowy
  sam byłby ciałem (wartością zwracaną) implementacji domyślnej, więc nie ma sposobu
  na rozróżnienie tych dwóch.

Hover serwera języka (`typl-lsp`) także pokazuje docstringi.

## 8. Makra

Sposób definiowania makr opisano w [Referencji składni 3.14](../syntax.md#314-defmacro--definicje-makr).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Nowy symbol. Jego nazwa to `" <prefix><n>"`, gdzie `n` to `*gensym-counter*`. Początkowej spacji nie da się zapisać w kodzie źródłowym, więc wygenerowane wiązania nigdy nie kolidują z zapisanymi nazwami |
| `*gensym-counter*` | Zmienna | `int` | Liczba, której `gensym` użyje następnym razem. Jak w CL, można ją odczytać i ustawić |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Rozwija wywołanie makra o jeden krok. `none` oznacza „to nie jest wywołanie makra" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Powtarza, aż to już nie będzie makro |

`macroexpand-1` zwraca `Option`. CL zgłasza „czy rozwinięto" jako drugą wartość zwracaną, ale
nie ma wielu wartości, więc tę rolę pełni `none`. **Makra, które rozwija się do wywołania samego siebie,
nigdy nie da się pomylić z nie-makrem.** Jeden krok rozwinięcia jest tym samym, którego używa moduł sprawdzający typy,
więc to, co widzi program, i to, co widział moduł sprawdzający, nigdy się nie rozchodzą.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none pokazuje się jako pusta lista (Option<Sexpr> jest przezroczysty)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Czego CL ma, a ten język nie: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute`
zawsze się pokrywają, więc nie ma rozróżnienia do wyboru), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (symbole nieinternowane; wiązania są wyszukiwane po nazwie, więc
nic by się nie zyskało).

## 9. Lokalne wiązania makr (`macrolet` / `symbol-macrolet`)

Obie to formy specjalne wiążące leksykalnie **nazwy, które nie są wartościami**. W czasie działania nic nie zostaje:
kompilowana jest rozwinięta postać ciała.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Wiązanie `macrolet` przesłania makro globalne o tej samej nazwie **tylko na czas ciała**. Lista lambda jest
  taka sama jak w `defmacro` (`&optional`/`&rest`/`&key`).
- **Rodzeństwo tego samego `macrolet` nie widzi się nawzajem ze swoich *ciał*** (jak w CL; to
  różnica względem `labels`). Rozwinięcia są sprawdzane w miejscu użycia, więc `earlier` rozwijające się do
  `(later ...)` działa: oba są widoczne w tym miejscu.
- Nazwa `symbol-macrolet` wchodzi do środowiska jako zwykłe wiązanie. Zatem wewnętrzne `let` przesłania
  tę samą nazwę, a zewnętrzna zmienna zostaje przesłonięta: reguły CL wychodzą bez zmian.
- **`setf` zapisuje do rozwinięcia.** `(setf head 42)` to `(setf (get v 0) 42)`.
- Rozwinięcia są sprawdzane w **środowisku miejsca użycia** (a nie miejsca wiązania).

## 10. Pozostałe

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Powoduje panic, jeśli fałsz. Bez komunikatu `assertion failed: <test jak zapisano>` (jest to makro, więc może nazwać samo wyrażenie). Restarty z CL nie istnieją w tym języku |
| `warn` | `(warn control args...)` | `(string,...)→()` | Zapisuje jedną linię z prefiksem `WARNING: ` do `*error-output*` i **kontynuuje**. Sposób zgłoszenia czegoś bez zwracania `Result` i bez kończenia programu |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Zastępuje zmienne globalne tylko w czasie `body` i przywraca je przy wyjściu. CL zapisuje to jako `let`, ale `let` w tym języku zawsze wiąże leksykalnie, stąd osobna nazwa (ta sama rola co makro o tej samej nazwie w Emacs Lisp). Przywraca je bez względu na to, jak ciało zostanie opuszczone: normalne zakończenie, `throw`, `panic`, `break`/`return`. **Nie jest wiązaniem na zadanie** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Wykonuje `body` z każdą zmienną sterującą drukarki ustawioną na wartość standardową i `*read-eval*` ustawionym na `true` ([Wypisywanie](printing.md#6-kontrolowanie-ile-jest-wypisywane)) |
| `exit` | `(exit code)` | `int→!` | Kończy proces |
| `dump` | `(dump path)` | `string→bool` | Zapisuje bieżące środowisko (informacje o typach plus skompilowane ciała) do jednego pliku. `typl --image <path>` startuje od niego ponownie. Tylko interpreter ([Referencja składni 10.1](../syntax.md#101-zrzuty-dumps)) |
