<!-- translated-from: docs/ja/reference/syntax.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Referencja składni typelisp

typelisp to statycznie typowany Lisp zapisywany w S-wyrażeniach. Listę funkcji wbudowanych i
metod znajdziesz w [Funkcjach wbudowanych](functions/README.md), listę typów w [types.md](types.md),
a informacje o czytaniu komunikatów o błędach w [errors.md](errors.md).

## 1. Elementy leksykalne

- **Bez rozróżniania wielkości liter.** Symbole są przy wczytaniu normalizowane do małych liter.
- **Komentarze**: od `;` do końca linii (komentarze liniowe). `#| ... |#` (komentarze blokowe, które można
  zagnieżdżać).
- **Obliczanie w czasie czytania**: `#.(expr)` **wykonuje następującą po nim formę w trakcie czytania** i traktuje jej wartość jako
  to, co zostało wczytane. To jedyne miejsce, w którym czytnik jest czymś więcej niż funkcją tekstu. Jak daleko
  może sięgnąć, zależy od ścieżki czytania, jak w CL:
  - `(load ...)` i REPL obliczają po jednej formie naraz, więc może wywoływać **funkcje zdefiniowane wcześniej w
    tym samym tekście** (`load` z CL).
  - Plik modułu jest sprawdzany jako całość i uruchamiany przez tego, kto go wprowadza przez `use`, więc `#.` sięga tylko do biblioteki
    standardowej i tego, co sesja już uruchomiła. Ani własne definicje pliku, ani definicje
    modułów, które wprowadza przez `use`, **jeszcze się nie wykonały** (tak jak `compile-file` z CL potrzebuje
    `eval-when`).
  - `read` / `read-from-string` wewnątrz programu także obliczają `#.` (jak w CL).
  - Ustawienie `*read-eval*` (domyślnie `true`) na `false` czyni `#.` błędem odczytu wszędzie: przełącznik, który nie pozwala
    tekstowi czytanemu jako dane uruchamiać kodu (jak w CL). Jest sprawdzany przy każdym `#.`, więc `setf` zaczyna
    działać od następnej wczytanej formy. Wewnątrz `with-standard-io-syntax` ma wartość `true`.
- **Wartości logiczne**: `true` / `false`.
- **Liczby całkowite**: dziesiętne (`42`, `-7`). Najpierw może stać znak `+`/`-`. Inne podstawy zapisuje się
  składnią podstaw z CL `#b`/`#o`/`#x`/`#NNr` (znak stoi po znaczniku: `#x-ff`). Prefiks `0x` nie występuje
  w CL i nie został przyjęty: `0xff` jest czytane jako symbol.
  Literał całkowity bez adnotacji typu ma domyślnie typ `int` (dowolna precyzja,
  [Liczby](functions/numbers.md#3-liczby-całkowite-o-dowolnej-precyzji-int)), bez górnej granicy wielkości.
  **Jeśli oczekiwanym typem jest typ całkowity o stałej szerokości, literał przyjmuje ten typ i sprawdzane jest,
  czy typ może pomieścić wartość**: `(the u8 300)` jest błędem typu (jeśli chcesz ją obciąć, napisz
  `(as u8 300)`). Dzięki tej regule można zapisać `(the u32 4294967295)` i `(the u32 #xFFFFFFFF)`.
  To, czy wartość `int` mieści się w 63-bitowej wartości natychmiastowej, czy staje się bignum, jest rozstrzygane przez jej wielkość, bez
  specjalnej składni (jak w CL).
- **Liczby zmiennoprzecinkowe**: te zawierające kropkę dziesiętną lub wykładnik (`e`/`E`) (`1.5`,
  `3.0e10`). Domyślnie `f64` (`f32`, jeśli to oczekiwany typ).
- **Ułamki**: `licznik/mianownik` (tylko dziesiętne, na przykład `1/3`). Skracane przy wczytaniu, jak określa CL
  (`2/4` to `1/2`). Te o wartości całkowitej (`4/2` i tak dalej) są wczytywane jako `int`, a nie
  `ratio`. Zerowy mianownik (`1/0`) jest błędem odczytu.
- **Znaki**: `#\` po którym następuje jeden znak lub nazwa znaku. Na przykład `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (także `#\Null`) `#\Backspace`. Nazwy nie
  rozróżniają wielkości liter.
- **Łańcuchy znaków**: `"..."`. Sekwencje ucieczki to `\n` `\t` `\r` `\0` `\\` `\"` (każde inne `\x` to po prostu `x`).
- **Symbole**: dowolny token zawierający litery, cyfry i symbole (`+` `<=` `my-func` i tak dalej).
  `]` i `}` kończą token, więc nie mogą wystąpić wewnątrz symbolu, a napotkanie któregoś na początku
  danej jest błędem odczytu. `[` i `{` mogą wystąpić wewnątrz symbolu: tak jak w CL pozostają wolne,
  aby programista mógł ich użyć w [makrach czytnika](#11-makra-czytnika-readtable).
- **Słowa kluczowe**: symbole zaczynające się od dwukropka, takie jak `:name` (jak w CL). Obliczają się do samych siebie:
  nie wyszukują żadnego wiązania, a ich wartością są one same, ze statycznym typem `symbol`. Słowa kluczowe o
  tej samej nazwie są zawsze tym samym obiektem (`(eq :foo :FOO)` to prawda; jak inne symbole są
  zamieniane na małe litery). Sam dwukropek jest częścią nazwy, więc `(symbol->string :foo)` to `":foo"` (typelisp
  nie ma systemu pakietów, więc różni się to od `symbol-name` z CL). Samotny `:` lub z dodatkowymi
  dwukropkami, jak `:a:b`, jest błędem odczytu. Test za pomocą `keywordp`. Te zaczynające się od `::` nie są słowami kluczowymi,
  lecz ścieżkami bezwzględnymi (niżej).
  Zwróć uwagę, że `:dyn` jest słowem zastrzeżonym tylko dla pozycji typów; zapisanie go gdziekolwiek indziej jest błędem
  (zobacz [rozdział 2](#2-zapis-typów)).
- **Listy**: `(a b c)`. Pary z kropką `(a . b)` także można wczytać.
- **Wektory**: `#(1 2 3)` (tak jak w CL). Zawartość to wyłącznie literały i nie jest obliczana: `a`
  w `#(a b)` to symbol, nie zmienna. Typ elementów wynika z kontekstu (`(the Vector<i32> #(1 2))`),
  a bez kontekstu z pierwszego elementu (`#(1 2 3)` to `Vector<int>`). Wszystkie elementy muszą mieć
  ten sam typ: `#(1 "a")` jest błędem typu, podobnie jak `#()` bez elementów i bez kontekstu. Każde
  obliczenie tworzy nowy wektor. Tam, gdzie oczekiwane są dane w postaci S-wyrażeń
  (`(the Option<Sexpr> #(1 x))`, `'#(..)`, to, co zwraca `read`), jest to `Vector<Option<Sexpr>>`,
  którego wszystkie elementy są danymi: wariant `vector` typu `Sexpr`.
- **Tablice**: `#2A((1 2) (3 4))` (tak jak w CL). Liczba między `#` a `A` to ranga, a tyle samo
  pierwszych poziomów zagnieżdżenia list w zawartości to wymiary. `#0A x` to tablica zerowymiarowa z
  jednym elementem. Listy na tym samym poziomie o różnych długościach są błędem odczytu. Typ ustala
  się tak jak dla wektorów i jest to `Array<T>` (bez elementów musi go podać kontekst, jak w
  `(the Array<f64> #2A(()))`). Jako dane w postaci S-wyrażeń jest to `Array<Option<Sexpr>>`: wariant
  `array` typu `Sexpr`.
- **Pusta lista `()`**: zależnie od kontekstu wartość typu `Unit` lub `none` z
  `Option<Sexpr>`. **`Sexpr` nie ma wariantu pustej listy**: `Sexpr` oznacza „niepuste S-wyrażenie",
  a typem danych w postaci S-wyrażeń jest `Option<Sexpr>` (zobacz „Wzorce dla `Option<Sexpr>`" w
  [4.3 match](#43-match--dopasowywanie-wzorców)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (ma sens tylko wewnątrz quasiquote)
  - `,@x` → `(unquote-splicing x)` (wstawiane jako elementy listy przy rozwinięciu)
- **Ścieżki `::`**: `foo::bar` jest czytane jako ścieżka przez moduły, typy i składowe (a nie jako pojedyncza
  nazwa symbolu). Ta zaczynająca się od `::`, jak `::foo`, jest ścieżką bezwzględną od korzenia. `::` wewnątrz
  argumentów generycznych (`Vec<a::b>` i podobne) nie jest traktowane jako separator ścieżki.

## 2. Zapis typów

W kodzie źródłowym typy zapisuje się jako zwykłe symbole lub listy.

- **Typy prymitywne**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` to typ całkowity (integer z CL, przechodzący automatycznie między 63-bitowymi wartościami natychmiastowymi a
  bignum; [Liczby](functions/numbers.md#3-liczby-całkowite-o-dowolnej-precyzji-int)), a sześć typów o stałej szerokości
  nazwano od ich szerokości i znakowości (nie ma 64-bitowego typu całkowitego; zobacz
  [Liczby](functions/numbers.md#1-liczby-całkowite-o-stałej-szerokości)).
- **Typ wymierny**: `ratio` (liczby wymierne w postaci nieskracalnej). Alokowany na stercie jak w CL, bez niejawnej
  konwersji z `int`/`f64` i podobnymi (konwertuj jawnie za pomocą `as`/`try-as` lub metody konwersji; zobacz
  [Liczby](functions/numbers.md#5-liczby-wymierne-ratio)).
- **Surowe słowa na granicy z C**: `ptr` (nieprzezroczysty wskaźnik), `c-long` / `c-ulong`. Tylko dla FFI:
  uczynienie któregoś wartością wymaga `(unsafe ...)`, a miejsca ich występowania są ograniczone
  ([3.3 defffi](#ptr--c-long--c-ulong--surowe-słowa-maszynowe)). Nie używaj ich tam, gdzie chcesz 64-bitową
  liczbę całkowitą: nie mają arytmetyki.
- **Nieprzezroczyste typy zmienne**: `random-state` (stan generatora liczb losowych). Nie może trafić do
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (może trafić do `Option<T>`/`Result<T,E>`).
- **Typ Unit**: `()`
- **Typ Never**: `!` (typ wyrażeń rozbieżnych, takich jak `panic`/`unreachable`/`todo`/pętla,
  która nigdy nie wraca. Pasuje do każdego oczekiwanego typu)
- **Typy funkcyjne**: `(fn (typy-argumentów...) typ-zwracany)`. Typ funkcji o zmiennej liczbie
  argumentów to `(fn (typy-argumentów... &rest typ-elementu) typ-zwracany)`.
- **Typy generyczne**: `Name<T1,T2,...>` (czytane jako pojedynczy token bez spacji).
  Na przykład `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Typ jednostkowy `()` można też zapisać jako argument typu (`Result<(), FileError>`). `(`/`)` to
  zwykle ograniczniki kończące token, ale gdy nawias ostrokątny jest otwarty, ta jedna para znaków
  jest przepuszczana. `()` można też użyć jako typu pola lub typu argumentu.
- **Forma aplikacji typów generycznych**: `(Name T1 T2 ...)`, zapis listowy nazywający ten sam typ co
  `Name<T1,T2,...>`. Na przykład `(vector char)` to to samo co `Vector<char>`.
  Forma z nazwą jest zwykłym sposobem zapisu; ta forma **istnieje na wypadek, gdy argumentu typu nie da się
  zapisać wewnątrz nazwy**: argument typu sam jest wyrażeniem typu, ale wewnątrz jednotokenowej nazwy
  można zapisać tylko nazwy, `()` i `:dyn`, a nie typy funkcyjne (nie ma takiego zapisu jak
  `Vector<(fn (i32) i32)>`). Może też pojawić się w tej formie, gdy implementacja pokazuje typ, na przykład
  wynik podstawienia typu powiązanego traitu do sygnatury.
- **Kwalifikowane nazwy typów**: można je kwalifikować za pomocą `::`, jak w `module::Type`.
- **Typy obiektów traitów**: `:dyn Trait` (dwa słowa rozdzielone spacją tworzące jeden typ). Reprezentuje wartość,
  której konkretny typ jest ustalany w czasie działania; wywołania metod traitu idą przez vtable (dyspozycja dynamiczna).
  Dla traitu z typami powiązanymi są one ustalane pozycyjnie w kolejności deklaracji (`:dyn Iter<i32>`
  ustala `Item` na `i32`). Można go też zapisać wewnątrz argumentów generycznych: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. Konkretne wartości są automatycznie pakowane w oczekiwanych miejscach;
  forma jawna to `(as :dyn Trait expr)`.
  Wartość `:dyn Sub` można przekazać bez zmian tam, gdzie wymagane jest `:dyn Super` dowolnego z jego supertraitów (wszystkiego,
  co dziedziczy, przechodnio) (rzutowanie w górę). Nie można jej przekazać do niepowiązanego traitu.
  Warunki, które trait musi spełniać, by używać go z `:dyn`, opisano w
  [3.9 deftrait / impl](#39-deftrait--impl--traity). Zapisanie `:dyn` poza pozycją typu jest błędem.
- Wbudowane typy generyczne: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>` oraz typy współbieżności `Task<T>` / `Thread<T>` / `Chan<T>`
  ([rozdział 12](#12-współbieżność-zadania)). Jest też `Sexpr`, typ danych w postaci S-wyrażeń.
  Wbudowane konkretne typy błędów to `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, a biblioteka standardowa ma struktury `SimpleError` / `WrappedError`
  (`Error` nie jest typem, lecz traitem: używaj go jako `:dyn Error`). Lista znajduje się w [types.md](types.md).
- **Typy i traity dzielą jedną przestrzeń nazw** (jak w Rust): w obrębie jednego modułu typ
  (`defstruct`/`defenum`) i trait (`deftrait`) nie mogą mieć tej samej nazwy.

## 3. Definicje najwyższego poziomu

### 3.1 defun — definicje funkcji

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Typy argumentów i typ zwracany są wymagane.
- Funkcja generyczna zapisuje swoje parametry typu w nawiasach ostrokątnych po nazwie:
  `(defun name<T1,T2...> (params) Ret body...)` (ta sama składnia z nawiasami ostrokątnymi co `Vector<T>` w pozycjach
  typów).
- `defun`/`lambda`/`defmethod` akceptują argumenty o zmiennej liczbie, gdy na końcu zapisano `&rest (name Type)`:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (w ciele `xs` jest zawsze związane jako
  `Option<Sexpr>`, lista S-wyrażeń. Każdy rzeczywisty argument w wywołaniu jest sprawdzany pod względem typu jako `Type2`
  osobno, a potem opakowywany w `Sexpr`).
  `defmacro` ma własne `&rest`, ale różni się tym, że jest zawsze nietypowanym `Sexpr` (`defun`/
  `lambda` podają typ elementu). Typ funkcyjny też może opisywać funkcję o zmiennej liczbie argumentów, jako
  `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (dla `defun` i `defmethod`; nie dla `lambda`/`labels`, z powodu opisanego
  niżej, a `defmacro` ma osobną implementację, również opisaną niżej). Kolejność jest jak w CL:
  `required &optional &rest &key`. Każdy parametr zapisuje się `(name Type)` lub
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; bez wartości domyślnej
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> w ciele

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; z wartością domyślną
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; wywołujący pisze `:name value`, w dowolnej kolejności; pominięte przyjmują wartości domyślne
  ```

  - **Parametr bez wyrażenia domyślnego ma typ `Option<Type>`.** Pominięty jest `none`; przekazany,
    goła wartość zapisana przez wywołującego jest automatycznie opakowywana w `some`. To, co CL robi ze zmienną
    supplied-p („czy podano?"), pojawia się tutaj po stronie typu statycznego.
  - Z wyrażeniem domyślnym typ pozostaje `Type`, jak zadeklarowano. Gdy parametr jest pominięty, to **sprawdzone
    wyrażenie** jest osadzane w wywołaniu bez zmian (obliczane przy każdym wywołaniu).
  - **`&key` nie można mieszać z `&optional`/`&rest` w jednej liście argumentów.** Pozwala to uniknąć niejednoznaczności, którą ma
    sam CL (czy końcowy rzeczywisty argument jest brany przez pozycyjne `&optional`, czy dopasowywany
    po etykiecie jako `&key`, zależy od *wartości*), przez zakazanie tego połączenia. `&optional` i `&rest`
    można używać razem.
  - Można ich używać w funkcjach generycznych, ale **parametr typu, który występuje tylko w pominiętych
    argumentach, nie może być wywnioskowany i jest błędem** (nie ma wartości do dopasowania).
  - **`defmethod` może mieć te same trzy sekcje** (zarówno dla metod instancji, jak i funkcji statycznych).
    Wypisz `&optional`/`&rest`/`&key` po odbiorcy:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; funkcja statyczna
    (point::origin :y 7)
    ```

    Można ich używać także w metodach typów generycznych, ale **typ parametru z wyrażeniem
    domyślnym nie może wspominać parametrów typu właściciela** (to samo ograniczenie, które `defun` ma dla własnych
    parametrów typu: to, co jest osadzane po pominięciu argumentu, jest *sprawdzonym* wyrażeniem, więc
    jego typ nie może pozostać abstrakcyjną zmienną).
  - **Nie można ich używać w metodach traitów.** `deftrait` nie ma dla nich składni, a gdyby tylko strona `impl`
    mogła deklarować sekcje, wywołania z odbiorcą `:dyn` (uzupełnianie argumentów z deklaracji traitu) i
    wywołania z konkretnym odbiorcą (uzupełnianie z deklaracji `impl`) stałyby się różnymi rzeczami. Arność
    slotu vtable jest stała.
  - **Nie można ich używać w `lambda` / `labels`** (`&rest` można). Aby uzupełnić pominięty argument,
    wywołujący musi odczytać **sprawdzone wyrażenie domyślne wywoływanego**, które jest dostępne tylko z
    sygnatury rozstrzygniętej po nazwie. `lambda` jest przekazywana jako wartość, a jedyną rzeczą opisującą
    tę wartość jest jej typ funkcyjny `(fn ...)`: nie ma w nim miejsca na wyrażenie, a gdyby było,
    „dwie lambdy o tej samej sygnaturze, ale różnych wartościach domyślnych" stałyby się różnymi typami.
    `&rest` pozostaje w obrębie spraw typów, więc można go zapisać w typie funkcyjnym.
- **Odwołania wyprzedzające deklaruje się za pomocą `defsignature`** (niżej). Nazwy, która nie została zadeklarowana,
  nie można wywołać przed jej definicją, ponieważ najwyższy poziom jest sprawdzany i uruchamiany po jednej formie naraz,
  w kolejności kodu źródłowego.
- Aby wymagać ograniczeń traitów, napisz klauzulę `where` tuż przed ciałem:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (ustalenie typu powiązanego za pomocą `(AssocName ConcreteType)` jest opcjonalne).
- **Docstringi**: literał łańcuchowy na początku ciała, zaraz po klauzuli `where` (jeśli jest),
  staje się docstringiem (jak w CL). Jednak tylko wtedy, gdy po nim następuje co najmniej jedna forma ciała: sam
  łańcuch pozostaje wartością zwracaną i nie jest brany za docstring: `(defun f () string "doc" "value")` ma
  docstring i zwraca `"value"`, podczas gdy `(defun f () string "value")` nie ma docstringu i zwraca
  `"value"`. Można go pobrać za pomocą `(documentation name)`
  ([docstringi](functions/system.md#7-docstringi--documentation)).

### 3.2 defsignature — deklaracje wyprzedzające

```lisp
(defsignature name (typy-argumentów...) typ-zwracany)
(pub defsignature name (typy-argumentów...) typ-zwracany)
```

Aby wywołać `defun` zdefiniowane **później** niż ty sam, zadeklaruj je najpierw w ten sposób. Wzajemną rekurencję
można zapisać tylko tak:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Argumenty wypisuje się **tylko jako typy**; nie ma ciała, więc nie ma czemu nadawać nazw.
`&rest` można zapisać na końcu, jako `&rest typ-elementu`.

Deklaracje **są sprawdzane**:

- Następująca po nich definicja musi pasować do deklaracji (liczba i typy argumentów, typ
  zwracany, `&rest` i to, czy jest `pub`). Niezgodność jest błędem w definicji.
- Zadeklarowanie bez zdefiniowania jest błędem (zgłaszanym, gdy plik / moduł skończy się ładować). REPL
  nie zgłasza tego po każdym wejściu, ponieważ deklaracja i jej definicja powinny dać się
  wpisać w osobnych liniach.
- Deklaracja umieszczona **po** definicji jest błędem, ponieważ taka deklaracja nic by nie mogła zrobić.

Trzech rzeczy nie można zadeklarować:

- **Funkcji generycznych.** Tworzenie kopii dla każdego typu wymaga ciała, a deklaracja go nie ma. Wywołanie
  wyprzedzające dałoby się rozstrzygnąć, ale konkretyzacja by się nie powiodła, więc deklaracja jest odrzucana z góry.
- **`&optional`/`&key`.** Ich sygnatura zawiera **sprawdzone** wyrażenie każdej wartości domyślnej
  (osadzane w wywołaniu po pominięciu argumentu), a deklaracja nie ma na nie miejsca.
- **Czegokolwiek innego niż `defun`.** `defmacro` potrzebuje, aby ciało makra **już się wykonało**, by
  się rozwinąć, czego rejestracja sygnatury nie zastąpi. W przypadku typów (`defstruct`/`defenum`/`deftrait`)
  ich rejestracja jest „tym, czego potrzebuje kod rejestrujący sam typ", co nie jest samodzielne
  tak jak sygnatura. `defmethod` jest rejestrowane w typie, który je posiada, więc idzie za typem.

Odpowiednikiem w CL jest `(declaim (ftype (function (i32) bool) even2))`, ale wiąże się to z całym
systemem deklaracji i jest tylko **doradcze**. Tutaj, przy typowaniu statycznym, deklaracje są sprawdzane.

### 3.3 defffi — deklarowanie funkcji C (FFI)

```lisp
(defffi (name "c_symbol") (typy-argumentów...) typ-zwracany)
(defffi (name "c_symbol") (typy-argumentów...) typ-zwracany :library "name")
(defffi name (typy-argumentów...) typ-zwracany)              ; name = nazwa symbolu C
(pub defffi ...)
```

Deklaruje funkcję C tak, aby można ją było wywołać. Forma jest taka sama jak `defsignature` (nazwa, typy argumentów,
typ zwracany i brak ciała), ale brak ciała oznacza coś innego. `defsignature` to
obietnica, że „zdefiniuję to później", natomiast `defffi` deklaruje, że „ktoś inny już napisał
i skompilował ciało".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Nazwę w typelisp i nazwę symbolu C można zapisać osobno, ponieważ identyfikatory typelisp zwykle
zawierają `-`, a identyfikatory C nie mogą. Jeśli nazwę C pominięto, nazwa jest używana jako nazwa symbolu C
bez zmian.

**Wywołania wymagają `(unsafe ...)`** (nawet dla funkcji operujących wyłącznie na skalarach). Kompilator nie ma jak
potwierdzić, że zadeklarowana sygnatura C zgadza się z rzeczywistą, i może tylko zaufać deklaracji;
`unsafe` to znak, że bierzesz na siebie tę odpowiedzialność. Zamierzony sposób to opakowanie raz i
stworzenie bezpiecznego opakowania:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; od tej pory unsafe nie jest potrzebne
```

Typy, które można zapisać, to `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong` oraz typowane wskaźniki `(ptr T)`
([niżej](#def-c-struct-i-typowane-wskaźniki--alokowanie-struktur-c)).

`string` to `const char *`. Łańcuchy typelisp nie są zakończone znakiem NUL i mogą same zawierać NUL, więc
**są kopiowane do łańcucha C przy przekazywaniu** i zwalniane po wywołaniu. NUL w łańcuchu jest
błędem: C patrzyłoby tylko do niego, więc po cichu przekazany zostałby inny łańcuch.

**Zwracane łańcuchy też są kopiowane** i nie są zwalniane: to, co zwraca C, należy do C i może wskazywać na
statyczną tablicę, jak w `getenv`. Funkcje zwracające pamięć, którą wywołujący musi zwolnić (`strdup` i tak dalej),
należy przyjmować jako `ptr` i samemu zwalniać.

Funkcje, których wynik wskazuje wewnątrz argumentu (`strchr`, `strstr`), także działają poprawnie: wynik
jest kopiowany, zanim argument zostanie zwolniony.

Jeśli funkcja zadeklarowana jako zwracająca `string` zwróci NULL, jest to błąd, ponieważ `string` nie ma wartości
oznaczającej „nic nie było". Jeśli NULL jest możliwy, przyjmij wynik jako `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Z `:library` ta biblioteka współdzielona jest otwierana i w niej wyszukiwany jest symbol. Bez niego symbol jest
wyszukiwany w **samym procesie** (wszystko, co już zlinkowane, w tym libc). Krótka nazwa, taka jak
`sqlite3`, jest wyszukiwana jako `libsqlite3.dylib` / `libsqlite3.so` w tej kolejności, a nazwa zawierająca `/`
jest traktowana jako ścieżka. Otwarte biblioteki nigdy nie są zamykane: kod wskazujący na ich funkcje działa
dalej, więc jedynym poprawnym czasem życia jest czas życia procesu.

#### ptr / c-long / c-ulong — surowe słowa maszynowe

`ptr` to nieprzezroczysty wskaźnik (`void *`, `FILE *`, cokolwiek oznaczała deklaracja). `c-long` / `c-ulong` to
`long` / `unsigned long` z C (także `size_t`, `int64_t` i `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Nienazywanie ich `i64` / `u64` jest zamierzone.** Ten język nie ma 64-bitowego typu całkowitego, ponieważ
otagowana wartość natychmiastowa ma tylko 63 bity ([rozdział 2](#2-zapis-typów)). Nazwa `c-long` mówi „to jest
słowo przekraczające granicę z C, a nie liczba całkowita tego języka".

**Nie mają arytmetyki.** `(+ x 1)` nie da się napisać. Można by to udostępnić, ale nie jest, aby żadne
obliczenie nie działało na wartości, której nie można nigdzie przechować i która ma inną szerokość niż każda inna
liczba, z tego samego powodu, dla którego pominięto 64-bitowy typ całkowity. Są **tylko konwersje**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; odczytaj to, co wróciło
(as int (unsafe (c-strlen s)))               ; ta, aby odczytać dokładnie (int nie gubi 64 bitów)
(try-as i32 (unsafe (c-strlen s)))           ; zapytaj, czy się mieści
(as c-ulong n)                               ; utwórz z innej liczby całkowitej
```

**Literały** całkowite przyjmują oczekiwany typ, więc `as` nie jest potrzebne tylko po to, by je przekazać:

```lisp
(unsafe (c-malloc 16))                       ; 16 jest czytane jako c-ulong
```

Literały spoza zakresu są odrzucane jak przy innych szerokościach (`(c-malloc -1)` nie mieści się w `c-ulong`).

**Miejsca, w których mogą występować, są ograniczone**: tylko typy argumentów, typy zwracane i zmienne lokalne.
Każde z poniższych jest błędem:

```lisp
(defstruct handle (p ptr))          ; pole struktury
(defenum maybe (none) (some ptr))   ; pole enumeracji
(defvar (block ptr) ...)            ; zmienna globalna
(defffi f ((vector ptr)) i32)       ; wewnątrz argumentu typu
```

Jest jeden powód dla wszystkich: **slot taguje to, co przechowuje**. Tagowanie odrzuciłoby najwyższe bity
wskaźnika, z tego samego powodu, dla którego pominięto 64-bitowy typ całkowity, więc nie jest dozwolone nawet w
`unsafe`. Nie jest to kwestia pozwolenia: ta reprezentacja nie istnieje.

Z tego samego powodu nie mogą być zmiennymi lokalnymi **przechwytywanymi** przez zagnieżdżone funkcje (przechwycone
wiązanie trafia do komórki, a komórka taguje to, co przechowuje). Wiadomo to w czasie kompilacji i jest zgłaszane
przez `(compile f)`.

GC nie śledzi `ptr`. Wskazuje poza stertę, więc to poprawne.

Czterech rzeczy nie można zadeklarować:

- **Argumentów o zmiennej liczbie** (`printf`). Część zmienna jest przekazywana według innych reguł niż argumenty
  stałe (na stosie w AArch64 Darwin), więc nie można jej poprawnie wywołać ze stałej sygnatury.
  `&rest` jest odrzucane.
- **Przekazywania lub zwracania struktur przez wartość.** Z tego samego powodu (zależy od konwencji
  wywołań każdej platformy). Typy, które można zapisać, są ograniczone do powyższej listy, więc nie da się tego zapisać.
- **Generyków.** C nie ma odpowiednika.
- **Tej samej nazwy co funkcja wbudowana.** Skompilowane wywołanie rozstrzygnęłoby tę nazwę do funkcji wbudowanej, więc jest
  odrzucana, zamiast po cichu działać błędnie.

#### Wywołania zwrotne — wywoływanie z powrotem z C

Zapisanie typu funkcyjnego `(fn (typy...) typ-zwracany)` jako typu argumentu czyni ten argument funkcją,
którą C wywołuje z powrotem (callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; funkcja najwyższego poziomu
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; funkcja lokalna
```

Wskaźnik funkcji C to nic innego jak adres kodu, a C wywołuje go, przekazując tylko zadeklarowane argumenty.
Nie ma miejsca na przekazanie przechwyconych zmiennych, więc **można przekazywać tylko funkcje bez zmiennych wolnych**,
co jest sprawdzane w czasie sprawdzania typów.

- Zapisz nazwę funkcji lub wyrażenie `lambda` **bezpośrednio** jako rzeczywisty argument. Zmiennej przechowującej
  funkcję nie można przekazać: to, którą funkcję przechowuje, a więc czy ma zmienne wolne, nie jest
  znane aż do czasu działania.
- `lambda` jest błędem, jeśli odwołuje się do zmiennych lokalnych spoza siebie. Można odwoływać się do zmiennych globalnych i funkcji
  najwyższego poziomu.
- Funkcja lokalna (`labels`) nie może mieć zmiennych wolnych, w tym tych z funkcji rodzeństwa, które
  wywołuje. Funkcje rodzeństwa dzielą miejsce, w którym przechowywane są przechwycone zmienne, więc to, co wywołane rodzeństwo
  przechwytuje, jest przechwytywane także przez tę funkcję.
- Funkcja generyczna bierze swoje typy z zadeklarowanego typu funkcyjnego.
- Typy, które można zapisać w typie funkcyjnym, są takie same jak na powyższej liście. Jednak `string`
  nie może być typem zwracanym wywołania zwrotnego (przekazałby C pamięć, której nikt nie zwalnia). Argument `string`
  kopiuje łańcuch przekazany przez C do łańcucha typelisp.

Wywołania funkcji C można zapisywać tylko wewnątrz `unsafe`, więc wywołania zwrotne można przekazywać tylko wewnątrz `unsafe`.

**Wywołanie zwrotne może być wywołane tylko wtedy, gdy działa funkcja C wywołana przez typelisp.** Jeśli jest
wywołane skądkolwiek indziej (z wątku niewykonującego typelisp, z obsługi sygnału, z funkcji zarejestrowanej przez
`atexit`), wypisuje przyczynę i zatrzymuje proces.

**Niepowodzenia nie propagują się przez C.** `panic` lub `throw` wewnątrz wywołania zwrotnego nie mogą rozwinąć stosu przez ramki C
(byłoby to zachowanie niezdefiniowane), więc do C zwracane jest 0, a niepowodzenie jest rzucane ponownie do
wywołującego, gdy funkcja C wróci. Jeśli wywołanie zwrotne zostanie wywołane ponownie między niepowodzeniem a powrotem
funkcji C, nie jest uruchamiane i zwracane jest 0.

Operacja, która musiałaby czekać wewnątrz wywołania zwrotnego (`recv` na pustym kanale i tak dalej), jest
błędem ([12.6](#126-skompilowany-kod-i-zadania)).

Gdy funkcja zostanie zdefiniowana ponownie, nowa definicja jest wywoływana od następnego przekazania jej do C.

Z AOT (`compile-file`) działa to tak samo. Punkty wejścia, które wywołuje C, są wbudowywane w
plik wykonywalny.

**Nie można ich przekazywać jako wartości.** Deklaracji FFI nie można zapisać bez zmian jako `f` w
`(map f xs)`: wartość funkcyjna to domknięcie opakowujące ciało definicji, a ta deklaracja
nie ma ciała do opakowania. Opakuj ją w `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` także jest odrzucane: to, co dałoby się pokazać, to kod maszynowy C, którego ten kompilator
nie wytworzył. `(compile c-abs)` się powiedzie (i nic nie zrobi, ponieważ jest już skompilowane).

**Działa to także z AOT (`compile-file`).** Linker rozwiązuje same funkcje C. Jeśli
deklaracja ma `:library`, ta biblioteka jest dodawana do linii linkowania jako `-l` (duplikaty są scalane w
jeden), więc `compile-file` nie potrzebuje dodatkowych argumentów. Samo `compile-file` czyta kod źródłowy, więc może
je zebrać z deklaracji.

Symbole są wyszukiwane także w czasie budowania. Jeśli zadeklarowana funkcja nie istnieje, błąd podaje jej nazwę
przed jakimkolwiek błędem linkowania.

Biblioteka standardowa (prelude) nie używa `defffi`. Biblioteka standardowa trafia do każdego
pliku wykonywalnego w całości, więc deklaracja z `:library` zlinkowałaby tę bibliotekę nawet do programów,
które nie używają FFI.

#### def-c-struct i typowane wskaźniki — alokowanie struktur C

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Deklaruje strukturę o takim samym układzie jak w C. Można ją zapisać tylko wewnątrz `unsafe` najwyższego poziomu
(które może zawierać wyłącznie `def-c-struct`). Docstring można umieścić zaraz po nazwie.

Typy, które można zapisać dla pól, to `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, typowane wskaźniki `(ptr T)` oraz inne `def-c-struct` (osadzane przez wartość). Układ
(przesunięcie każdego pola oraz rozmiar i wyrównanie struktury) jest obliczany według reguł C (przy założeniu
LP64). Można zapisać pole wskazujące na samą strukturę, ale struktura nie może osadzać samej siebie.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x przy 0, y przy 8, rozmiar 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Nazwa `def-c-struct` trafia do przestrzeni nazw typów (żaden `defstruct` ani podobny o tej samej nazwie nie może być
w tym samym module), ale **nie jest typem wartości**. Nie można napisać
`(defun f ((p point)) ...)`; pojawia się tylko jako to, na co wskazuje typowany wskaźnik.

**Typowany wskaźnik `(ptr T)`** to adres wskazujący na `T`. `T` to jeden z typów, które można
zapisać dla pól powyżej. Jest to surowe słowo maszynowe jak `ptr`, z tymi samymi regułami dotyczącymi miejsc, w których może
występować (tylko argumenty, typy zwracane i zmienne lokalne; może być wartością tylko wewnątrz `unsafe`).

Alokację, odczyt i zapis zapisuje się w następujących formach. Wszystkie można używać tylko wewnątrz
`unsafe`.

| Forma | Znaczenie |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Alokuje `n` wartości typu `T` (1, jeśli pominięto). Zawartość jest wypełniona zerami. Zwraca `(ptr T)` |
| `(c-ref p i)` | Wskaźnik na element `i` od `p`. Błąd, jeśli poza zaalokowanym zakresem |
| `(c-deref p)` / `(setf (c-deref p) v)` | Odczytuje / zapisuje skalar, na który wskazuje `p` |
| `p::field` / `(setf p::field v)` | Odczytuje / zapisuje pole struktury. Odczyt pola będącego osadzoną strukturą daje jej adres (`(ptr inner-type)`) |
| `(as ptr p)` | Zapomina typ, tworząc `ptr` (do przekazania do czegoś takiego jak `void *` w `qsort`). Nie ma konwersji z powrotem |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Zaalokowana pamięć jest zwalniana, gdy sterowanie opuszcza `unsafe`, które ją zaalokowało.** Właścicielem jest
leksykalnie najbardziej zewnętrzne `unsafe` w obrębie tej samej funkcji. Jest zwalniana bez względu na to, czy kod kończy się normalnie,
czy opuszcza go `panic`, `throw` lub `return-from`. Funkcje `lambda` i `labels` to osobne funkcje,
więc `c-alloc` w nich potrzebuje własnego `unsafe` wewnątrz nich.

Z tego powodu typowany wskaźnik nie może opuścić `unsafe`, które go zaalokowało. Każde z poniższych jest
błędem typu:

- Uczynienie go wartością wyrażenia `unsafe` (więc nie można go też zwrócić z funkcji)
- Przechwycenie go w domknięciu (`lambda`, `labels`)
- Przekazanie go do `task` / `thread`
- Rzucenie go za pomocą `throw`

Aby użyć wartości na zewnątrz `unsafe`, skopiuj je do `defstruct` lub liczb wewnątrz `unsafe` i
zwróć je.

**Pamięć zaalokowana po stronie C nie jest obsługiwana.** Wartości, które przychodzą z C jako typowane wskaźniki (wartości
zwracane przez `defffi`, argumenty wywołań zwrotnych, wartości odczytane z pól typu wskaźnikowego), są sprawdzane w czasie działania,
czy wskazują na wartość tego typu wewnątrz żywej alokacji `c-alloc`, a jeśli nie, jest to błąd.
NULL też jest błędem. Aby otrzymać pamięć zaalokowaną przez C lub NULL, użyj nietypowanego `ptr` (którego zawartości
nie można odczytać).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Gdy argument wywołania zwrotnego zostanie odrzucony przez sprawdzenie, jest to zgłaszane wywołującemu, gdy funkcja C
wróci, tak samo jak niepowodzenie wewnątrz wywołania zwrotnego.

### 3.4 defvar / defparameter / defconstant — zmienne globalne

```lisp
(defvar (name Type) init-expr)        ; inicjuje tylko, jeśli jeszcze niezwiązana
(defparameter (name Type) init-expr)  ; przypisuje za każdym razem
(defconstant (name Type) init-expr)

; z docstringiem (w tej samej kolejności co defvar/defparameter/defconstant z CL: po wartości)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Różnica między `defvar` a `defparameter` ujawnia się przy ponownym wczytaniu** (jak w CL). Jeśli zmienna globalna jest
**już związana, `defvar` nawet nie oblicza inicjalizatora**, więc gdy edytujesz plik ustawień i wczytujesz go
ponownie, wartości zmienione przez sesję pozostają bez zmian. `defparameter` przypisuje za każdym razem, więc
ponowne wczytanie przywraca wartości zapisane w pliku.

Adnotacja typu jest wymagana (nie jest wnioskowana z inicjalizatora). `defvar` można zmieniać;
`defconstant` nie (`setf` jest błędem).

### 3.5 defmethod — definicje metod

```lisp
; metoda instancji: można wywołać jako (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; funkcja statyczna / powiązana: można wywołać jako (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Wywołujący rozstrzyga metodę na podstawie statycznego typu `obj` (pojedyncza, statyczna dyspozycja). Docstring można
umieścić w tej samej pozycji i według tych samych reguł co dla `defun` (zaraz po klauzuli `where`,
na początku ciała, tylko gdy następują formy ciała). To samo dotyczy metod wewnątrz `impl`; są
pobierane za pomocą `(documentation Type::method)`.

### 3.6 defstruct — struktury (typy definiowane przez użytkownika)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generyczna (parametry typu w nawiasach ostrokątnych)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Każde pole to `(name type)` lub `(pub name type)` (widoczność dla każdego pola, niezależna od `pub`
  samej struktury). Jedno dodatkowe wyrażenie na końcu staje się **wartością domyślną** slotu (`(x i32 0)`); zobacz
  listę opcji niżej.
- Automatycznie generowane są:
  - Konstruktor `Name::new` (argumenty w kolejności pól)
  - Gettery `(field-name instance)`, z lukrem `instance::field-name`
  - Settery `(set-field-name instance value)`, z lukrem `(setf instance::field-name value)`
- Aby samą strukturę uczynić `pub`, umieść `pub` z przodu, jak w `(pub defstruct ...)`.
- **Zdefiniuj typ, zanim go nazwiesz.** Typem pola może być sama struktura (`(next Option<node>)`),
  ale nie typ zdefiniowany później: typy nie mają deklaracji wyprzedzającej odpowiadającej `defsignature`. Nazwa
  jeszcze niezdefiniowana daje ten sam błąd `unknown type` w typie argumentu `defun` lub w `the`. Zatem dwa
  typy odwołujące się do siebie nawzajem nie mogą być zapisane.
- **Zmiennymi typowymi są tylko te zapisane w pozycjach deklarujących.** Dla `defun`/`defstruct`/`defenum`/
  `deftype` jest to `<T>` w nazwie; dla `defmethod` typ odbiorcy (`(self box<T>)` lub `box<T>`
  dla metody statycznej); dla `impl` typ docelowy i `impl<T>`; dla `deftrait` `Self` i typy
  powiązane z `(type Item)`. Nazwa pojawiająca się po raz pierwszy gdziekolwiek indziej (argumenty, wartość
  zwracana, `the`/`lambda` w ciele) nie staje się zmienną typową; jest `unknown type`.
- **Docstringi**: literał łańcuchowy zaraz po nazwie, przed polami, staje się docstringiem
  (`(defstruct Name "doc" (field Type)...)`, ta sama pozycja co w `defstruct` z CL). Pole ma zawsze
  postać `(name Type ...)` i nigdy nie może być gołym łańcuchem, więc nie ma niejednoznaczności. Pobierz go za pomocą
  `(documentation Name)`.

#### Lista opcji

Zapisanie listy `(Name option...)` w pozycji nazwy określa opcje (ta sama pozycja co w CL).

```lisp
(defstruct (point (:constructor make-point)          ; konstruktor ze słowami kluczowymi
                  (:constructor at (x &optional y))  ; konstruktor BOA
                  (:copier copy-point))
  (x i32 0)          ; trzeci element to wartość domyślna tego slotu
  (y i32 0))

(point::make-point :y 7)   ; x wynosi 0
(point::at 1)              ; y wynosi 0
(point::at 1 2)
(copy-point p)             ; płytka kopia (tak samo jak copier z CL)
```

- **`:constructor`**: generowana jest **funkcja statyczna** typu (`point::make-point`), której
  ciałem jest zawsze `(point::new ...)`. `new` pozostaje jedynym konstruktorem strukturalnym; to, co tu powstaje, to
  *sposób wywołania* go. Można zadeklarować kilka.
  - `(:constructor name)` przyjmuje każdy slot jako `&key`. **Każdy slot potrzebuje wartości domyślnej** (ten język nie ma
    niczego odpowiadającego „niezwiązanemu slotowi" z CL).
  - `(:constructor name (slot...))` przyjmuje wymienione sloty jako argumenty pozycyjne (w dowolnej kolejności). Sloty
    niewymienione są wypełniane wartościami domyślnymi, więc **potrzebują wartości domyślnych**. Po `&optional` pozostałe mogą
    być pominięte (i także potrzebują wartości domyślnych).
- **`:copier`**: generuje **metodę instancji** zwracającą nową wartość z tymi samymi wartościami slotów.
  Płytką, jak copier z CL.
- **`:include Parent`**: dopisuje z przodu sloty rodzica (wartości domyślne też są dziedziczone; rodzic może być w
  innym pliku). **Nie tworzy relacji typów**: dziecko nie jest podtypem rodzica, metody
  rodzica nie dotyczą dziecka i nie ma testu czasu działania łączącego oba. Ten
  język nie ma podtypowania; wspólne interfejsy to zadanie `deftrait`. Łączona jest tylko *lista* slotów.
- **Wartości domyślne slotów są odczytywane tylko przez generowane konstruktory.** Zapisanie wartości domyślnej bez zadeklarowania
  żadnego `:constructor` jest błędem, ponieważ nigdy nie mogłaby być użyta.
- Pominięte opcje i dlaczego:
  - **`:conc-name`**: w CL poprzedza akcesory prefiksem, aby uniknąć kolizji w jednej płaskiej przestrzeni nazw funkcji. Tutaj
    akcesory to metody rozstrzygane według typu odbiorcy, więc kolizje nie występują, a prefiks
    zepsułby `instance::field` (które zna tylko nazwę slotu).
  - **`:predicate`**: odpowiada w czasie działania na pytanie „czy ta wartość jest `point`?". Tutaj typy to klasyfikacja
    czasu kompilacji bez świadka w czasie działania i nie ma pozycji, w której istniałaby „wartość nieznanego typu,
    która może być point" (`match` na `Sexpr` jest zamknięty, a `:dyn` nie można rzutować w dół), więc
    wygenerowany predykat mógłby zawsze zwracać tylko `true`.
  - **`:type` / `:initial-offset` / `:named`**: zastępują reprezentację wartości listą lub
    wektorem. Reprezentacja należy do kompilatora i nie może być obserwowana z poziomu języka.

### 3.7 defenum — enumeracje (typy sum)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; wariant z ładunkiem (pola pozycyjne)
  (Variant2)                  ; wariant bez ładunku
  ...)

; generyczna
(defenum Option<T>
  (Some T)
  (None))
```

- Każdy wariant ma postać `(VariantName FieldType...)`. Pola są tylko pozycyjne (nie mają
  nazw). Potrzebny jest co najmniej jeden wariant, a nazwy nie mogą się powtarzać.
- Wartości buduje się, tak jak z wbudowanymi `Option`/`Result`, kwalifikowanie lub przez `use`:
  `(Name::Variant1 a b)` albo `(Variant1 a b)` po `(use Name)`.
- Można je rozbierać za pomocą `match` / `if-let`. `match` sprawdza wyczerpywalność (musi pokrywać każdy
  wariant lub mieć `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Metody i funkcje powiązane dodaje się później za pomocą `defmethod`/`impl`, tak jak z `defstruct`.
- Aby samą enumerację uczynić `pub`, napisz `(pub defenum ...)`.
- **Docstringi**: ta sama pozycja i reguły co w `defstruct`, zaraz po nazwie, przed wariantami
  (`(defenum Name "doc" (Variant ...)...)`). Pobierz go za pomocą `(documentation Name)`.

### 3.8 deftype — aliasy typów

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

`deftype` z CL, zawężone do tego, co ma sens w języku typowanym statycznie: **zapis typu, a nie
typ**.

- Pozycja nazwy jest taka sama jak w `defun`, a argumenty generyczne zapisuje się `Name<T,U>`. W miejscu
  użycia potrzebna jest dokładnie zadeklarowana liczba argumentów typu (zbyt wiele lub zbyt mało jest błędem
  na miejscu).
- Rozwinięcie następuje **wewnątrz parsera typów**. Zatem nic dalej nie wie, że alias istnieje: klucze
  jednomorfizacji, zrzuty, ścieżka kompilacji i **komunikaty o błędach** pokazują postać rozwiniętą. Jeśli
  `(f "x")` zawiedzie wobec funkcji wymagającej `meters`, komunikat mówi `i32`.
- **Nie jest to nowy typ.** `(deftype meters i32)` czyni `meters` i `i32` tym samym typem, więc pomylenie ich
  nie zostanie wykryte. Jeśli chcesz je rozróżniać, użyj `defstruct`.
- **Nie jest to predykat.** `(deftype small () '(integer 0 9))` z CL opisuje *zbiór wartości*, który
  `typep` testuje w czasie działania, ale tutaj typy to klasyfikacja czasu kompilacji bez świadka w czasie działania,
  więc alias ograniczający wartości nie miałby czego ograniczać.
- **Nie może zawierać samego siebie.** Alias jest rozwijany tam, gdzie jest zapisany, więc nie ma miejsca, do którego mógłby
  rekurencyjnie sięgnąć. Rekurencyjne typy danych zapisuje się za pomocą `defstruct`/`defenum`.
- Dzieli przestrzeń nazw z typami i traitami (w obrębie jednego modułu nie może mieć tej samej nazwy co
  `defstruct`/`defenum`/`deftrait`). Udostępnij go za pomocą `(pub deftype ...)` i wprowadź za pomocą
  `(use m::meters)`.
- **Docstringi**: zaraz po nazwie, przed typem (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traity

```lisp
(deftrait TraitName (SuperTrait...)      ; lista supertraitów jest wymagana; () jeśli brak
  (type AssocName)                       ; typy powiązane (dowolna liczba, opcjonalne)
  (method-name ((self Self) params...) RetType)          ; bez ciała = trzeba zaimplementować
  (method-name ((self Self) params...) RetType body...)) ; z ciałem = implementacja domyślna

(impl TraitName TargetType
  (where (Trait A)...)                   ; ograniczenia dotyczące całego impl (opcjonalne)
  (type AssocName ConcreteType)          ; konkretyzuje typ powiązany
  (method-name (recv params...) RetType body...))
```

Przez `impl` każda metoda jest rejestrowana jako zwykłe `defmethod` typu `TargetType`. Do traitów
odwołuje się jako do ograniczeń traitów w klauzulach `where` funkcji generycznych (zobacz
[3.1 defun](#31-defun--definicje-funkcji)). Nazwa traitu może być także ścieżką `::`, taką jak `m::Trait`.

**Lista supertraitów (wymagana)**: zawsze zapisywana zaraz po nazwie traitu. Każdy element to goła
nazwa traitu lub, jeśli ten trait ma typy powiązane, `(Trait (Assoc Type))` ze **wszystkimi jego typami
powiązanymi ustalonymi**.

```lisp
(deftrait Eq () ...)                       ; bez supertraitów
(deftrait Ord (Eq) ...)                    ; trait Ord: Eq z Rust
(deftrait CharSource ((Iter (Item char)))  ; ustalenie typu powiązanego
  (rewind ((self Self)) ()))
```

Dziedziczenie ma trzy skutki. (1) `impl Ord X` wymaga, aby `impl Eq X` zostało zapisane **najpierw** (reguła
kolejności zapisu: jedyna forma, którą da się rozstrzygnąć deterministycznie w REPL i przy
`load` krok po kroku, i surowsza niż w Rust). (2) Samo `(where (Ord T))` pozwala wywoływać także metody
`Eq`. (3) Metody `Eq` można wywoływać przez `:dyn Ord`, a wartość `:dyn Ord` można przekazać
bez zmian tam, gdzie wymagane jest `:dyn Eq` (rzutowanie w górę). Subtrait ponownie deklarujący metodę o tej samej nazwie co
jego rodzic oraz dziedziczący metody o tej samej nazwie od dwóch rodziców są błędami (vtable ma jeden
slot na nazwę). Dziedziczenie rombowe scala się do jednego slotu.

**Implementacje domyślne**: ciało po sygnaturze jest używane, gdy `impl` pomija metodę. Ciało
jest rozstrzygane w **przestrzeni nazw modułu**, w którym zapisano trait, więc może wywoływać
niepubliczne funkcje tego modułu. Metody z ciałami mogą też mieć klauzule `where` i docstringi.
Ciało jest sprawdzane pod względem typów **raz, w miejscu deklaracji**, z `Self` pozostawionym jako zmienna typowa
(ograniczona przez `Self: sam trait`), jak w Rust: błędy, które zawiodłyby dla każdego `impl` i każdego
implementującego typu, nawet w wartościach domyślnych, które żadne `impl` nigdy nie pomija, są tam wychwytywane. Wywołania na `self` metod
samego traitu lub jego supertraitów przechodzą przez to ograniczenie, a typy powiązane są ustalone na
siebie, więc sygnatura zwracająca `Item` jest dopasowywana do ciała bez znajomości konkretnego
typu.

**Implementacje blankietowe (blanket)**: uczynienie celu zmienną typową implementuje trait naraz dla każdego
typu spełniającego ograniczenia.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; w ogóle bez ciała; wszystko jest domyślne
```

**Żaden kod nie jest generowany, dopóki konkretny typ faktycznie go nie użyje** (raz dla typu, tym samym mechanizmem co
zwykła jednomorfizacja). Trait może mieć co najwyżej jedną implementację blankietową. Jeśli typ ma
jawne `impl`, ma ono pierwszeństwo. Sprawdzanie typów ciała jest oddzielone od generowania: wykonywane jest
raz w miejscu deklaracji, **z celem pozostawionym jako zmienna typowa** (jak w Rust), więc nawet
implementacja, która nigdy nie jest używana, ma tam wychwytywane błędy, jeśli zawiodłyby dla każdego celu
przy zadeklarowanych ograniczeniach. Wywołania uzasadnione ograniczeniami (`(less self other)` przy `(where (Ord T))`
i tak dalej) przechodzą, jak w ciele funkcji generycznej `defun`.

**Docstringi**: `deftrait` może mieć jeden docstring dla całego traitu, jako literał łańcuchowy zaraz po
liście supertraitów, przed pozycjami (`(deftrait Name () "doc" (type ...) (method ...)...)`). Sygnatura
bez ciała nie może mieć docstringu: końcowy łańcuch sam byłby wartością zwracaną implementacji domyślnej, więc
nie dałoby się ich rozróżnić.

Traity dostarczane przez bibliotekę standardową: **`Iter`** (`next` / typ powiązany `Item`; podstawa
`doiter` i funkcji na sekwencjach), **`Eq`** (`equals`; `not-equals` to implementacja domyślna),
**`Ord`** (dziedziczy `Eq`; trzeba zaimplementować tylko `less`, a `less-equal` / `greater` /
`greater-equal` to implementacje domyślne), **`Error`** (`message` / `source`; `:dyn Error` do
jednolitej obsługi typów błędów), **`print-object`** (reprezentacja wypisywana według typu), **`Pathish`**
(desygnatory nazw ścieżek: łańcuch znaków lub `pathname`) oraz hierarchia strumieni **`Stream`** →
**`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**.
To, które typy implementują które traity, znajduje się w [types.md](types.md); metody każdego traitu znajdują się w
[Traitach standardowych](functions/traits.md), [Typach błędów](functions/option-result.md#3-typy-błędów-i-trait-error),
[print-object](functions/printing.md#5-print-object-reprezentacja-wypisywana-według-typu) i
[Strumieniach](functions/streams-files.md). Jeśli zaimplementujesz `Iter` dla własnego typu kolekcji, `doiter`
(rozdział 5) oraz `map` / `filter` / `sort` i podobne działają na nim bez zmian.

Wywołania traitów są domyślnie **statyczne** (rozstrzygane według statycznego typu odbiorcy). Aby obsługiwać wartości, których
konkretny typ jest ustalany w czasie działania, typ obiektu traitu `:dyn Trait` (rozdział 2) daje dynamiczną
dyspozycję przez vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; jedno miejsce wywołania, odpowiedź na implementację
```

Tylko traity, w których „każda metoda ma odbiorcę `self`, nie używa `Self` nigdzie poza
odbiorcą i sama nie jest ani generyczna, ani o zmiennej liczbie argumentów", mogą być używane z `:dyn` (odziedziczone metody muszą spełniać
te same warunki).

Tylko typy, których wartości mają reprezentację na stercie, mogą trafić do pudełka `:dyn`:

| Mogą trafić | Nie mogą trafić |
|---|---|
| Typy `defstruct` / `defenum` (w tym `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` i struktury biblioteki standardowej), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Liczby całkowite o stałej szerokości (od `i8` do `u32`), `f32`, `bool`, `char`, `symbol`, `()`, typy funkcyjne oraz `Option<T>` bez pudełka ([reprezentacja Option w czasie działania](functions/option-result.md#2-reprezentacja-optiont-w-czasie-działania)) |

Umieszczenie wartości typu, który nie może trafić, tam, gdzie oczekiwane jest `:dyn`, jest błędem typu. Aby obsługiwać takie
wartości przez `:dyn`, opakuj je w strukturę, jak w `(defstruct flag (v bool))`.

### 3.10 module / use — przestrzenie nazw

```lisp
(module path body...)      ; path to ciąg segmentów, takich jak foo lub foo::bar
(in-module path)           ; od tego miejsca do końca tej jednostki, wewnątrz path (płaska forma module)
(use path...)              ; aliasuje funkcje, typy i moduły w bieżącej przestrzeni nazw
(import path...)           ; to samo co use (zapis zgodny z CL)
(shadowing-import path...) ; use, które świadomie przejmuje nazwę już używaną
```

- `module` tworzy przestrzeń nazw. **Typy nie są przestrzeniami nazw** (jak w Rust, typ ma tylko powiązane
  funkcje i metody).
- Wprowadzenie typu przez `use` udostępnia jego konstruktory i publiczne metody statyczne także pod gołą nazwą (na
  przykład po `(use option)` można wywoływać `some`/`none` bez `option::some`/`option::none`).
- Kolejność rozstrzygania gołych nazw (niekwalifikowanych identyfikatorów): formy specjalne → konstruktory → funkcje
  wolne (bieżąca przestrzeń nazw → korzeń) → metody instancji (rozstrzygane według statycznego typu pierwszego
  argumentu). Nie wspina się przez pośrednie moduły nadrzędne.
- Kwalifikowana ścieżka `a::b` rozstrzyga `a` w powyższej kolejności; jeśli jest modułem, wchodzi do środka, a jeśli
  jest typem, ostatni segment jest rozstrzygany jako element powiązany.
- **`use` wpływa na formy po nim.** Plik jest czytany po jednej formie naraz, a zależności są rozstrzygane
  tuż przed sprawdzeniem formy, więc zapisanie `m::f` **powyżej** `(use m)` daje `unresolved path`. Umieszczaj
  `use` na początku pliku.
- **`use` może przyjmować kilka ścieżek** (`(use a::f b::g)`). `import` to zapis zgodny z CL o tym samym
  zachowaniu.
- **`use`, którego goła nazwa jest już zajęta, jest zgłaszane.** Rozstrzyganie gołej nazwy patrzy na własne definicje
  modułu przed aliasami, więc `(use m::twice)` po `(defun twice ...)` **nic nie robi**. Jeśli
  to zamierzone, napisz `shadowing-import` (nadal nie może pokonać definicji, ponieważ nie ma sposobu na usunięcie
  jednej; pokonuje tylko wcześniejsze aliasy).
- **`in-module` to płaska forma `(module path body...)`.** Zapisanie `(in-module geometry)` umieszcza
  wszystko od tego miejsca do końca jednostki (pliku lub ciała otaczającego `module`) wewnątrz
  `geometry`. Trafia **do wnętrza** własnego modułu pliku (`main::geometry` dla `main.typl`). Dwa z rzędu
  zagnieżdżają się po kolei. Różni się od `in-package` z CL i ma inną nazwę: w tym systemie
  plik jest już modułem, więc nie ma czego „wybierać", a jedyne, co forma może zrobić, to zagnieździć.

### 3.11 Pliki i moduły (projekty wieloplikowe)

Ścieżka pliku względem korzenia źródeł jest ścieżką modułu:
zawartość `<root>/geo/point.typl` jest niejawnie opakowana w moduł `geo::point`
(katalog to także jeden segment, w stylu Rust / Python). Jawne `(module bar ...)` w pliku
zagnieżdża się **wewnątrz** niego (`geo::point::bar`), więc wyprowadzona ścieżka i jawna deklaracja nigdy nie kolidują.

- **Korzeń źródeł**: umieść plik manifestu `typelisp.toml` w korzeniu projektu (może być pusty; opcjonalnie
  jedna linia `src = "src"` wskazuje katalog źródeł). Jest znajdowany przez wspinanie się od katalogu
  pliku docelowego. Bez manifestu korzeniem jest katalog pliku wejściowego (bieżący katalog dla
  REPL).
- **Ładowanie na żądanie**: gdy `(use geo::point)` odwołuje się do modułu jeszcze niezaładowanego, odpowiedni plik
  (`geo/point.typl`) jest ładowany, sprawdzany pod względem typów i rejestrowany automatycznie. `use a::b::c` przeszukuje
  najpierw najdłuższy prefiks: `a/b/c.typl` → `a/b.typl` → `a.typl` (ponieważ `c` może być elementem wewnątrz modułu).
  Definicje widoczne z innych modułów wymagają `pub` ([3.13 pub](#313-pub--widoczność)).
- **Odwołania cykliczne są błędami**: łańcuch jest zgłaszany w postaci
  `circular module dependency: a -> b -> a`.
- **Uruchamianie**: `typl <file.typl>` uruchamia plik (bez argumentów REPL). `use` w REPL rozstrzyga
  pliki według tych samych reguł.
- **Pojemność areny cons**: `typl --heap-cells N` ustawia **początkową pojemność** areny komórek cons
  (domyślnie 65536; forma `--heap-cells=N` też działa, zarówno przy uruchamianiu plików, jak i REPL). Arena
  **rośnie przez dodawanie kolejnych**, gdy brakuje miejsca. Granicą wzrostu jest 256-krotność początkowej pojemności,
  a alokacja poza nią daje `heap exhausted`: początkowa pojemność oznacza „zaalokuj tyle na początku",
  a limit oznacza „poza tym traktuj to jako wyciek".

### 3.12 load — płaskie ładowanie

```lisp
(load "path")   ; tylko najwyższy poziom; path to literał łańcuchowy
```

- **Płaskie ładowanie** w stylu CL: wczytuje formy pliku docelowego **do bieżącej przestrzeni nazw** bez zmian
  (bez opakowywania ich w moduł, w przeciwieństwie do `use`). Tylko najwyższy poziom (wewnątrz ciała funkcji jest to
  błąd typu).
- `path` jest względna wobec katalogu ładującego pliku (z REPL wobec cwd procesu). Jeśli
  nie ma rozszerzenia, dodawane jest `.typl`.
- `(load ...)`/`(use ...)` w ładowanym pliku są także przetwarzane rekurencyjnie.
- **Czyta po jednej formie naraz i wykonuje ją na miejscu** (jak `load` z CL). Forma *k* kończy
  się wykonywać, zanim zostanie wczytana *k+1*: nawet jeśli w połowie jest błąd składni lub typu, formy
  przed nim już się wykonały. Pliki modułów ładowane przez `use` są inne: są sprawdzane jako jedna jednostka, a ich
  uruchomienie pozostawiono temu, kto je wprowadził przez `use` (odpowiada `compile-file` z CL).

### 3.13 pub — widoczność

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` można umieścić tylko na powyższych jedenastu rodzajach (nie na `module`/`use`/`deftrait`/`impl`). Zapisuje się je
ze słowem kluczowym definicji zaraz po `pub`, a nie w formie `(pub (defun ...))` opakowującej definicję
w nawiasy. Jedno `pub` czyni publiczną dokładnie jedną definicję (nie można oznaczyć kilku definicji
naraz).

### 3.14 defmacro — definicje makr

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Wszystkie parametry i wartość zwracana są zawsze `Sexpr`, więc nie zapisuje się adnotacji typów.
- Makra niehigieniczne w stylu CL (unikanie kolizji za pomocą `gensym` jest odpowiedzialnością autora makra).
- Lista lambda ma kolejność z CL `required &optional &rest &key` (każdy znacznik co najwyżej raz i tylko w
  tej kolejności).
  - `&optional` … argumenty opcjonalne. `name` lub `(name default-expr)`. Wyrażenie domyślne jest
    obliczane w czasie rozwijania (może odwoływać się do wcześniej związanych parametrów) i wiązane, gdy argument
    jest pominięty (bez wartości domyślnej pusta lista `()`).
  - `&rest name` … odbiera pozostałe argumenty pozycyjne razem jako jedną listę `Sexpr`.
  - `&key` … argumenty kluczowe. `name` lub `(name default-expr)`. Wywołujący przekazuje je jako `:name value`
    (w dowolnej kolejności). Po pominięciu wyrażenie domyślne (pusta lista `()`, jeśli go nie ma). Nieznane słowa kluczowe
    lub ciąg `:key` o nieparzystej długości są błędami.
- Przykłady: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — lokalne wiązania makr

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; makra o zasięgu leksykalnym
(symbol-macrolet ((name expansion) ...) body...)         ; nazwa oznacza formę
```

Obie to **wyrażeniowe** formy specjalne i nic nie zostaje w czasie działania (kompilowana jest rozwinięta postać
ciała). Lista lambda jest taka sama jak w `defmacro`. Szczegółowe reguły i przykłady znajdują się w
[Lokalnych wiązaniach makr](functions/system.md#9-lokalne-wiązania-makr-macrolet--symbol-macrolet).

## 4. Wiązanie i instrukcje warunkowe

```lisp
(let ((name val) ...) body...)      ; wiązanie równoległe
(let* ((name val) ...) body...)     ; wiązanie sekwencyjne (wcześniejsze wiązania dostępne w późniejszych inicjalizatorach)

(if cond then else)                 ; else jest wymagane (zawsze trzy elementy)
(when cond body...)                 ; if bez else (typ Unit). defmacro
(unless cond body...)               ; negacja when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; lista kluczy: pasuje, jeśli pasuje którykolwiek
  (else body...))                   ; expr jest obliczane raz. klucze są porównywane za pomocą equal.
                                     ; klucze to „literały" i nie są obliczane (jak w CL).
                                     ; goły symbol a oznacza symbol 'a.
                                     ; zapisanie 'a jest błędem (użyj gołego a).
                                     ; defmacro
(ecase expr (key body...) ...)      ; case wymagające dopasowania. powoduje panic, jeśli nic nie pasuje. defmacro
(ccase expr (key body...) ...)      ; ccase z CL. nie ma restartów do zaoferowania, więc to samo co ecase. defmacro
(and expr...)                       ; obliczanie skrócone. true dla zera argumentów. defmacro
(or expr...)                        ; obliczanie skrócone. false dla zera argumentów. defmacro
(progn body...)                     ; wykonuje po kolei i zwraca ostatnią wartość
(unsafe body...)                    ; to samo co progn, plus pozwolenie na zapisywanie wywołań FFI
                                     ; i surowych słów. zobacz 3.3 defffi
(prog1 form more...)                ; oblicza wszystko; wartością jest wartość form. defmacro
(prog2 a b more...)                 ; oblicza wszystko; wartością jest wartość b. defmacro
(the Type expr)                     ; adnotacja typu (bez skutku w czasie działania)
```

### 4.1 unsafe — przyjmowanie założeń, których nie można sprawdzić

```lisp
(unsafe body...)
```

To samo co `progn`: oblicza ciało po kolei i zwraca ostatnią wartość. Nie tworzy zakresu i nie jest
granicą funkcji (`break` / `return-from` przechodzą prosto na zewnątrz). Różnica polega
na tym, że niektóre rzeczy można zapisać tylko wewnątrz niego.

Obecnie `unsafe` wymagają trzy rzeczy: wywoływanie funkcji C zadeklarowanych za pomocą
[defffi](#33-defffi--deklarowanie-funkcji-c-ffi), tworzenie wartości z surowych słów maszynowych (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) oraz [`def-c-struct` i `c-alloc`](#def-c-struct-i-typowane-wskaźniki--alokowanie-struktur-c).

Pamięć zaalokowana za pomocą `c-alloc` jest zwalniana przy opuszczeniu najbardziej zewnętrznego `unsafe` w obrębie tej samej funkcji.
Tylko to `unsafe`, w przeciwieństwie do `progn`, ma przy wyjściu pracę do wykonania: zwolnienie pamięci.

To, co przyjmuje na siebie `unsafe`, to następujące założenia, których kompilator nie może zweryfikować:

- **Że typy się zgadzają.** Że zadeklarowana sygnatura C zgadza się z rzeczywistą. Jeśli nie, argumenty trafiają do
  niewłaściwych rejestrów, a wartości zwracane są odczytywane z niewłaściwą szerokością.
- **Bezpieczeństwo pamięci.** To, co strona C robi z tym, co dostaje.
- **Stan obejmujący cały proces.** Zmienne środowiskowe, obsługa sygnałów, `errno`. Na przykład wywołanie `setenv`
  przez FFI łamie założenia, które `decode-universal-time` tej implementacji przyjmuje przy
  obliczaniu czasu lokalnego.
- **Bezpieczeństwo wątków.**

Nie jest to sposób na ominięcie sprawdzania typów. `(unsafe (+ 1 "two"))` nie przejdzie. Dozwolone jest zapisywanie
pewnych **operacji**, a nie pisanie bzdur.

Działa leksykalnie. Ciało `lambda` zapisanej wewnątrz `unsafe` dziedziczy pozwolenie (tak jak domknięcia wewnątrz
bloków `unsafe` w Rust). Wartość może być później wywołana spoza `unsafe`, ale
zapisanie jej tam jest samo w sobie przyjęciem odpowiedzialności.

### 4.2 destructuring-bind — rozbieranie list według kształtu

```lisp
(destructuring-bind lambda-list form body...)
```

Rozbiera listę, którą tworzy `form`, **według jej kształtu** i wiąże ją. Lista lambda to ta z `defmacro`
(wymagane → `&optional` → `&rest`/`&body` → `&key`, każde z wyrażeniami domyślnymi), z tego samego powodu,
z którego CL dzieli jedną między oba: są to dwie formy rozbierające to samo.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Każda związana zmienna to `Option<Sexpr>`.** Nie jest to ograniczenie implementacji, lecz
  natura tego, co jest wiązane: listy S-wyrażeń to jedyne listy w tym języku, więc nie ma innego
  typu, który można by nadać elementom. Wracanie do `match` tam, gdzie potrzebny jest skalar, jest tym samym co w
  ciele `defmacro`.
- **Niepasujący kształt powoduje panic** (odpowiada błędowi z CL): zbyt mało lub zbyt wiele elementów, ciąg
  `&key` o nieparzystej długości lub nieznane słowo kluczowe. `sexpr-car` to łagodna funkcja zwracająca `()`
  dla `()`, więc bez tego sprawdzenia krótka lista byłaby po cichu związana z pustą sekwencją.
- **Zagnieżdżone listy lambda nie są obsługiwane.** `defmacro` też ich nie przyjmuje, więc jest jedna reguła.
  `(a (b c))` nie wiąże po cichu podlisty z `b`; jest to błąd, który to mówi.
- Wyrażenia domyślne `&optional` / `&key` są **obliczane tylko wtedy, gdy są używane** (jak w CL).
- Nie ma niczego odpowiadającego `&allow-other-keys` z CL (`defmacro` też go nie ma).

### 4.3 match — dopasowywanie wzorców

```lisp
(match expr
  (pattern body...)
  ...)
```

Rodzaje wzorców:
- `_` — wieloznacznik (wildcard)
- Nazwa zmiennej — wzorzec wiążący (zawsze pasuje). Jednak jeśli typ badanej wartości ma wariant o
  tej nazwie, jest rozstrzygana jako **poniższy wzorzec gołej nazwy wariantu**
- Goła nazwa wariantu — pasuje do wariantu niemającego argumentów (`(match c (red 1) (blue 2))`). Zapisanie
  wariantu z polami jego gołą nazwą jest błędem arności, więc zapisz go w nawiasach, jak w `(circle r)`
- **Literały natychmiastowe**: liczby całkowite / `true`/`false` / znaki — porównywane jako słowa
- **Literały wartościowe**: łańcuchy znaków / liczby zmiennoprzecinkowe / symbole (`'foo`) / liczby całkowite bignum / ułamki —
  porównywane według wartości za pomocą `Eq::equals` tego typu ([Traity standardowe](functions/traits.md#2-eq--ord-porównywanie)).
  Łańcuchy znaków porównują się według zawartości, a nie tożsamości
- `(= expr)` — oblicza dowolne wyrażenie i porównuje za pomocą `Eq::equals`. Jedyny sposób porównania typów,
  które nie mają składni literałów (instancje `defstruct`, zmienne globalne, obliczone wyniki), a zdefiniowana przez użytkownika
  implementacja `Eq` staje się regułą porównania bez zmian. `expr` może odwoływać się do wszystkiego, co jest widoczne z
  pozycji ramienia (argumenty, zewnętrzne wiązania, zmienne globalne)
- `(Ctor sub-pattern...)` — wzorce konstruktorów (`Some x` `None` `Cons a d` `Ok v` i tak dalej)

Porównanie typu, który nie implementuje `Eq`, z literałem wartościowym / `(= expr)` jest błędem typu (ten
język woli powiedzieć „tych nie da się porównać", niż zostawić ramię, które po cichu nigdy nie pasuje).

**Literały wartościowe względem badanej wartości `Sexpr`**: `Eq` dla `sexpr` to `eq` (tożsamość z CL), więc
wartości natychmiastowe (`'foo` (internowane) / liczby całkowite / znaki / `true`/`false`) można zapisać bez zmian i
dopasowują się według zawartości:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Literałów nienatychmiastowych (łańcuchy znaków / liczby zmiennoprzecinkowe / liczby całkowite bignum / ułamki) **nie można zapisać**
względem `Sexpr`. Ich `eq` porównuje tożsamość obiektu, co stworzyłoby „ramię, które przechodzi sprawdzanie typów, ale
nigdy nie pasuje", więc jest to błąd wskazujący wzorzec wariantu: zapisz `(str "hi")`, a zostanie rozebrany
na `string` i porównany według zawartości. `(= expr)` jawnie prosi o `equals`, więc to ograniczenie
go nie dotyczy.

**Badana wartość nie musi być ADT.** `string`/`symbol`/`i32`/`f64` i podobne można dopasowywać
bezpośrednio (tam trafiają wzorce z literałami łańcuchowymi). Jednak typu bez wariantów nie da się pokryć
wyliczeniem, więc wymagane jest `_` (lub wzorzec wiążący pełniący rolę wieloznacznika):

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; typ bez wariantów potrzebuje `_`
```

Względem badanej wartości `Sexpr`, poza 18 wbudowanymi wzorcami wariantów powyżej, można zapisać **wzorce rzutowania w dół**
(wyjmowanie instancji zdefiniowanych przez użytkownika ADT): składnię do odzyskiwania za pomocą `match`
instancji `defstruct`/`defenum` (rozdział 3), która została niejawnie skonwertowana do `Sexpr`, jak w
`(list p 42)`:

- `(TypeName sub-pattern...)` — rozkład pól z **nazwą typu** na początku (tylko struktury: `defstruct`
  ma zawsze jeden wariant, więc zapisuje się go nazwą typu, a nie nazwą wariantu).
  Na przykład dla `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Goła nazwa wariantu `(VariantName sub-pattern...)` — wyjmuje wariant `defenum`. Rozstrzygana jako goła
  nazwa widoczna po `(use EnumType)` (te same reguły widoczności co przy wywoływaniu konstruktora).
  Na przykład dla `(defenum color (red) (blue))`, `(red)` `(blue)` po `(use color)`. Jeśli nazwy wariantów
  kilku widocznych enumeracji kolidują, jest to błąd niejednoznaczności, więc można też zapisać formę kwalifikowaną
  `(EnumType::VariantName ...)` (bez potrzeby `use`).
- `(the Type pattern)` — rzutowanie w dół całego typu (wiążące go jako całość). Nie rozkłada
  pól; przekazuje wartość do `pattern` bez zmian. Jedyny sposób wyjęcia zmiennej struktury z zachowaniem jej
  tożsamości, a także jedyny sposób wyjęcia `Vector<T>`/`HashTable<K,V>` z `Sexpr`
  (nie mają formy rozkładu pól). Na przykład po `(the point p)` zmiana `(setf p::x 9)` jest
  odzwierciedlona także w oryginalnej instancji na liście.

**Wzorce dla `Option<Sexpr>`**: typem danych w postaci S-wyrażeń nie jest `Sexpr`, lecz `Option<Sexpr>`, a
pusta lista nie jest wariantem `Sexpr`, lecz `none` z `Option`. Zatem przy dopasowywaniu `Option<Sexpr>`
18 wariantów `Sexpr` i `none` można zapisać **płasko w tej samej liście ramion** (nie jest potrzebny zewnętrzny
`match` zdejmujący `Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; pusta lista
    (_          9)))
```

Wyczerpywalność jest sprawdzana w tym samym płaskim uniwersum: 18 wariantów `Sexpr` plus `none`, razem 19.
Zapomnienie `(none)` jest błędem, chyba że jest `_`. Można też zapisać `(some x)`, które wiąże „coś
niepustego".

Ten lukier dotyczy **dokładnie** tylko `Option<Sexpr>`. Dla `Option<Option<Sexpr>>` nie byłoby jasne,
którą warstwę zdjęło `(int n)`, więc zapisz dwa poziomy `match` jak zwykle.

Te same wzorce rzutowania w dół można stosować bez zmian względem **badanej wartości będącej obiektem traitu (`:dyn Trait`, rozdział 2)**:
`match` ją rozpakowuje, a potem przekazuje do powyższego mechanizmu wzorców `Sexpr`, więc nie ma
dodatkowej składni. Zbiór typów implementujących jest otwarty, więc nigdy nie może być wyczerpujący, a `_` jest
wymagane:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; rozkład pól z nazwą typu na początku
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Wnioskowanie typów między ramionami**: wszystkie ramiona muszą mieć ten sam typ (poza ramionami, które nie wracają, jak
z `panic`). W `match` zapisanym tam, gdzie nie oczekuje się typu, ramiona uzupełniają nawzajem brakujące argumenty
typu: `(result::ok v)` ustala tylko `T`, a `(result::err e)` tylko `E`, ale razem ustalają
`Result<T,E>`. Argument typu, którego żadne ramię nie potrafi ustalić do końca, jest błędem tego ramienia
(`cannot infer type argument ...`). Poza `match` argument typu, którego nie da się ustalić, jest błędem
na miejscu.

Sprawdzanie wyczerpywalności `match` używającego wzorców rzutowania w dół nie zalicza ich do pokrycia
własnych wariantów `Sexpr` (`match` wymieniający tylko wzorce rzutowania w dół musi być zamknięty za pomocą `_`). Dla generycznych ADT
(`defstruct point<T> ...` i tak dalej) argumentów typu wzorca rzutowania w dół nie da się wywnioskować, więc
forma rozkładu pól (`(point ...)`) i forma gołego wariantu nie mogą być użyte; podaj je za pomocą `the`,
jak w `(the point<i32> p)`.

**Rzutowania w dół patrzą także na konkretyzację.** Jawne argumenty typu są używane przy dopasowywaniu:
`(the point<i32> p)` przepuszcza tylko wartości `point<i32>`, a `point<string>` przechodzi do następnego
ramienia. Dzieje się tak, ponieważ wartość pamięta swój typ łącznie z argumentami typu (ten sam mechanizm, który
wybiera `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (z wiązaniami), jeśli val pasuje do pattern, w przeciwnym razie els. defmacro
(while-let (pattern val) body...)   ; pętla, dopóki val (obliczane za każdym razem na nowo) pasuje do pattern. defmacro
```

## 5. Iteracja

```lisp
(loop body...)                      ; pętla nieskończona. opuszcza się ją przez break/return
(while test body...)                ; pętla, dopóki test jest prawdziwy. defmacro
(until test body...)                ; pętla, dopóki test jest fałszywy (negacja while). defmacro
(dotimes (var count-expr) body...)  ; oblicza count-expr raz i przebiega var po 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; iteracja w stylu CL z krokiem równoległym. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; wersja sekwencyjna do (wiązanie jak let*, przypisywane po kolei). defmacro
(doiter (var coll-expr) body...)    ; iteruje po wartości implementującej trait Iter. defmacro

(break)                             ; opuszcza tylko najbardziej wewnętrzną pętlę. wartością jest zawsze Unit
(return)                            ; opuszcza tylko najbardziej wewnętrzną pętlę
(return value)                      ; opuszcza najbardziej wewnętrzną pętlę z wartością
```

Zarówno `break`, jak i `return` opuszczają **tylko najbardziej wewnętrzną otaczającą pętlę** (nie są wczesnym powrotem z
funkcji i nie mogą przekraczać granicy `lambda`). Typem `loop` jest złączenie typów wartości
z `break`/`return` znalezionych wewnątrz (`!`, jeśli nigdy nie jest opuszczana). Aby opuścić funkcję, użyj
`return-from`, niżej.

### 5.1 `block` / `return-from` — nazwane wyjścia

```lisp
(block name body...)                ; nazwany cel wyjścia. wartością jest ostatnia forma
                                    ; lub wartość przekazana przez return-from
(return-from name)                  ; opuszcza ten blok z Unit
(return-from name value)            ; opuszcza z wartością
```

**Każda funkcja `defun` / `defmethod` / `labels` niejawnie ustanawia blok o własnej nazwie**
(jak w CL). Zatem `(return-from f v)` to wczesny powrót z funkcji:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` to wyjście **leksykalne**, a nazwa jest **rozstrzygana tam, gdzie jest zapisana**: moduł sprawdzający łączy
`return-from` z otaczającym `block` i dołącza typ jego wartości do typu wyjścia bloku. Zatem:

- `return-from` bez pasującego `block` jest **błędem typu** (a nie błędem czasu działania).
- Wartość, której typ nie pasuje do innych wyjść lub typu ciała, jest **błędem typu** (ta sama reguła
  co dla ramion `match`).
- Jeśli bloki o tej samej nazwie są zagnieżdżone, **wygrywa wewnętrzny** (reguła przesłaniania z CL).
- **Nie może przekraczać granic funkcji.** Z wnętrza `lambda` nie można wyjść do zewnętrznego `block`
  (`lambda` nie ustanawia bloku: niejawne bloki CL potrzebują *nazwy*, a funkcje anonimowe jej nie mają).
  To, co musi przekraczać granice, to `catch`/`throw` (rozdział 8, które są **dynamiczne**).

Tak jak `break`/`return` (rozdział 5), jest to wyjście **statyczne**, więc w skompilowanym kodzie jest skokiem do bloku podstawowego
ustalonego w czasie kompilacji. Jeśli pomiędzy jest `unwind-protect`, jego `cleanup` jest wykonywane (rozdział 8).

Jeśli nigdy nie zapiszesz `return-from`, niejawny blok nic nie kosztuje.

### 5.2 Rozszerzone `loop` (LOOP z CL)

**Jeśli pierwszym elementem `loop` jest słowo kluczowe**, jest ono czytane jako ciąg klauzul. W przeciwnym razie pozostaje
prostą pętlą opisaną wyżej, a znaczenie istniejących `loop` się nie zmienia (ta sama reguła co dla prostej
pętli w samym CL).

CL zapisuje słowa klauzul jako gołe symbole (`(loop for i from 1 to 3 collect i)`), ale tutaj **wszystkie są
słowami kluczowymi**: goła `for` byłaby tylko odwołaniem do zmiennej, a bycie słowem kluczowym odróżnia też
od prostej pętli. Wyjątkiem jest `=`, które oddziela zmienną od wartości: jego pozycja jest
jednoznaczna, więc jest czytane zarówno jako gołe, jak i jako słowo kluczowe (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Klauzule zmiennych** (zapisywane przed klauzulami ciała. To reguła CL: zapisane po nich mogłyby zostać odczytane jako
„iteruj tylko od tego miejsca", więc jest to błąd):

| Klauzula | Znaczenie |
|---|---|
| `:with v = e` | Wiąże raz. Może czytać zmienne z wcześniejszych klauzul |
| `:for v :in s` / `:for v :across s` | Elementy `Iter` po kolei. Rozróżnienie CL na listę/wektor tu nie istnieje, więc są to dwa zapisy tej samej klauzuli |
| `:for v :on s` | Kolejne **sufiksy**. CL przekazuje współdzielony ogon cons, ale `Iter` nie ma ogona do współdzielenia, więc każdy jest nowym `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Liczenie. Działają też `:downfrom`/`:upfrom` |
| `:for v = e [:then f]` | Zaczyna od `e`, a od drugiego razu używa `f` (bez `:then` za każdym razem `e`) |
| `:repeat n` | Iteruje tyle razy |

Przy kilku `:for` posuwają się **równolegle**, a pętla kończy się, gdy tylko któryś się wyczerpie.

**Klauzule ciała** (wykonywane za każdym razem, w kolejności zapisu):

| Klauzula | Znaczenie |
|---|---|
| `:do form...` | Dla efektów ubocznych |
| `:collect e [:into v]` | Zbiera do `Vector<T>` |
| `:append e [:into v]` | Dopisuje zawartość `Iter` |
| `:sum e` / `:count e` | Suma / liczba razy, gdy było prawdą |
| `:maximize e` / `:minimize e` | Maksimum / minimum. **`Option<T>`** (tak jak CL zwraca nil dla pustej sekwencji; dowolny typ `Ord` nie ma elementu najmniejszego) |
| `:always e` / `:never e` | `true`, jeśli wszystkie zachodzą; `false` natychmiast, gdy któryś zawiedzie |
| `:thereis e` | `e` to **`Option<T>`**. Zwraca pierwsze `some` lub `none`, jeśli go nie ma (to odpowiada „pierwszej wartości niebędącej nil" z CL; aby testować `bool`, użyj `:always`/`:never`) |
| `:while e` / `:until e` | **Kończą normalnie** tutaj (`:finally` się wykonuje, a to, co zebrano, jest odpowiedzią) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Czynią jedną klauzulę warunkową |
| `:return e` | Opuszcza natychmiast z tą wartością (`:finally` się nie wykonuje, jak w CL) |
| `:initially form...` / `:finally form...` | Przed pętlą / przy normalnym zakończeniu |

**`:named name`** (przed jakąkolwiek inną klauzulą, tylko raz) opakowuje całą pętlę w `(block name …)`.
`(return-from name e)` może od razu wyjść nawet z wnętrza zagnieżdżonych pętli i, jak `:return`, `:finally`
się nie wykonuje. Bez nazwy żaden blok nie jest ustanawiany: nienazwany `loop` z CL ustanawia `block nil`, ale
tutaj nie ma `nil`, a `break`/`return` (rozdział 5) już zapewniają „opuść najbardziej wewnętrzną pętlę".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Pominięcie `:finally (return 0)` jest **błędem typu**. To po prostu działanie reguł `block` (5.1): typ
wyjścia `int` nie pasuje do `()`, które pętla zwraca po wyczerpaniu.

**Wartością pętli** jest akumulacja klauzuli akumulującej, jeśli jest (pierwszej, jeśli
jest ich kilka), `true` dla `:always`/`:never`, `none` dla `:thereis` i `()`, jeśli jej nie ma. Jeśli
ostatnią rzeczą w `:finally` jest `(return e)`, to jest to wartość: idiom `finally (return …)` z CL, jedyny
sposób, w jaki pętla niezbierająca może nazwać własną odpowiedź.

**Różnice względem CL / co nie jest uwzględnione**:

- **Słowa klauzul są słowami kluczowymi** (wyżej).
- `:maximize`/`:minimize`/`:thereis` zwracają `Option<T>` (nie ma nil).
- **Zapisanie samego `:return`, bez akumulacji ani `:finally`, jest błędem.** CL zwraca nil po wyczerpaniu, ale
  tutaj nie ma takiej rzeczy, więc pętla musi powiedzieć, jaka jest jej wartość po wyczerpaniu.
- Łączenie klauzul równoległych za pomocą `:and`, `:being`/dedykowana iteracja po tablicach mieszających, `:it` i `:nconc`
  nie są uwzględnione.
- Typ elementu `:collect` pochodzi z typu zbieranego wyrażenia. Próba zbierania
  typu, którego **nie da się zapisać jako nazwy typu**, takiego jak typ funkcyjny, jest błędem, który to mówi.

## 6. Wartości funkcyjne i wywołania

```lisp
(lambda (params) RetType body...)   ; tworzy wartość funkcyjną pierwszej klasy (domknięcie)
(labels ((name (params) RetType body...) ...) body...)   ; lokalne definicje funkcji, które mogą być wzajemnie rekurencyjne
(apply f arg1 ... argN rest-list)   ; wywołuje f (funkcję o zmiennej liczbie argumentów z &rest), rozkładając rest-list
```

Funkcje nazwane można także przekazywać jako wartości bez zmian (jako argumenty funkcji wyższego rzędu i tak
dalej).

## 7. Pozostałe formy specjalne

```lisp
(setq var value ...)                ; przypisanie zmiennej z CL. to po prostu ciąg (setf var value). defmacro
(psetq var value ...)               ; przypisanie równoległe. najpierw oblicza wszystkie wartości, potem przypisuje. defmacro
(psetf place value ...)             ; psetq uogólnione na miejsca (to samo rozwinięcie). defmacro
(setf place value)                  ; przypisanie do miejsca. miejsce to nazwa zmiennej / var::field /
                                     ; wywołanie postaci (accessor recv key...). poprawne, jeśli
                                     ; statyczny typ recv ma metodę instancji o nazwie
                                     ; set-{accessor} (dla get z Vector<T> i HashTable<K,V>
                                     ; wyjątkowo odpowiada set; w pozostałych przypadkach set-accessor-name).
                                     ; wartością jest przypisana wartość (jak w CL). dlatego
                                     ; w (if c (setf x 1) ()) then i else nie mają pasujących typów
(incf place)  (incf place delta)    ; place += delta (delta=1, jeśli pominięto). wynik jak przy setf
(decf place)  (decf place delta)    ; place -= delta (delta=1, jeśli pominięto)
(rotatef place1 place2 ... placeN)  ; obraca N miejsc (nowe place1=stare place2, ...,
                                     ; nowe placeN=stare place1). podformy każdego miejsca obliczane raz
(shiftf place1 ... placeN newvalue) ; przesuwa wartości place2..N w lewo i wstawia newvalue do placeN.
                                     ; wartością zwracaną jest stara wartość place1
(list e1 e2 ... en)                 ; rozwija się do (cons e1 (cons e2 (... ()))). () dla zera argumentów.
                                     ; każdy element jest niejawnie konwertowany do Sexpr (jak cons z CL
                                     ; może przechowywać dowolną wartość). skalary (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) są opakowywane w odpowiedni wariant Sexpr, a defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> i podobne wchodzą bez zmian
                                     ; (bez kosztu konwersji). to samo dla argumentów &rest/format.
(source-file)                       ; nazwa pliku, z którego odczytano tę formę (łańcuch znaków). ustalana jako
                                     ; stała w czasie sprawdzania. odpowiada *load-pathname* z CL, ale nie
                                     ; jest zmienną: ciała modułów działają po sprawdzeniu, więc na „aktualnie
                                     ; ładowane" nie można polegać, podczas gdy w czasie sprawdzania zawsze jest znane.
                                     ; dla źródeł niebędących plikami nazwa nadana przez czytnik (<stdin>/<input>)
(quote datum)                       ; to samo co 'datum. zwraca je jako dane Sexpr bez obliczania
(quasiquote template)               ; to samo co `template. osadza wyrażenia w szablonie za pomocą ,/,@
(documentation name)                ; zwraca docstring name (gołą nazwę lub Type::method) jako Option<string>
(panic message)                     ; message: string. kończy się nieprawidłowo nieodwracalnym błędem. typ !
(unreachable)                       ; rozwija się do (panic "unreachable"). defmacro
(todo)                              ; rozwija się do (panic "todo"). defmacro
(as Type expr)                      ; konwersja typu liczbowego/znakowego. konwersje, które mogą się nie powieść, powodują panic przy porażce
(try-as Type expr)                  ; jak as, ale zwraca wynik jako Option<Type> (None przy porażce)
(print control args...)             ; rozwija format i zapisuje na standardowe wyjście (bez nowej linii)
(println control args...)           ; to samo (z nową linią na końcu)
(format dest control args...)       ; format z CL. zwraca rozwinięty łańcuch
(pprint x)                          ; wypisuje ozdobnie. najpierw zapisuje nową linię, jak w CL
(pprint-fill x)                     ; układ wypełniający
(pprint-linear x)                   ; wszystko w jednej linii albo jeden element na linię
(pprint-tabular x [colinc])         ; układ tabelaryczny (domyślnie 16 kolumn)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; zbuduj blok logiczny samodzielnie
```

Rodzina `print`/`println`/`format`/`pprint` to formy specjalne, więc ich argumenty o zmiennej liczbie (pojedynczy
obiekt dla rodziny `pprint`) są opakowywane w `Sexpr` z własnymi typami, zanim zostaną przekazane: dlatego
`(println "~a" my-struct)` po prostu działa. Szczegóły dyrektyw formatu i pretty printera znajdują się w
[Dyrektywach formatu](functions/format.md) i [Wypisywaniu](functions/printing.md#4-pretty-printer).

`as`/`try-as` obsługują tylko katalog liczbowy i znakowy (między `int`, typami całkowitymi o stałej szerokości,
`f32`/`f64`/`ratio`/`char`). Ten sam typ nie jest konwersją. **Konwersje między szerokościami całkowitymi
(w tym `int`) oraz między `f32`↔`f64` są prawdziwymi konwersjami**: `as` obcina / zaokrągla, a `try-as`
odpowiada, czy mieści się w tej szerokości (precyzji). `(as int x)` to dokładne rozszerzenie ze stałej szerokości,
a `(as i32 n)` to obcięcie z `int`. Liczba całkowita → `char` może się nie powieść poza zakresem, więc `as` powoduje panic, a
`try-as` daje `None`. Wszystko inne (rozszerzanie oraz obcięcie z `float->int`/`ratio->int`) zawsze
się udaje. `float->int`/`ratio->int`/`char->int` lądują na `int`, a jeśli żądana jest węższa szerokość,
po nich wywoływane jest `int->W`. Jest to lukier rozwijający się do odpowiednich metod konwersji
(`int->char`/`int->int`/`int->W` i tak dalej w [Liczbach](functions/numbers.md)).

`documentation`, jak `quote`/`compile`, jest formą specjalną, która czyta `name` bez obliczania, jako
nieobliczony goły symbol / ścieżkę `::`. W przeciwieństwie do `(documentation 'name 'function)` z CL nie przyjmuje argumentu
typu: rozstrzyga `name` w kolejności zmienna → funkcja → typ → trait → makro (ten sam priorytet
co przy obliczaniu gołego identyfikatora jako wyrażenia) i zwraca docstring znalezionej definicji
(`(documentation Type::method)` jest dla metod). Brak rozstrzygnięcia (brak definicji o tej nazwie) jest
błędem czasu sprawdzania; definicja, która istnieje, ale nie ma docstringu, daje `Option::none`. Wszystko jest
ustalane jako stała w czasie sprawdzania: nie następuje żadne wyszukiwanie w czasie działania. Wolne nazwy kwalifikowane modułem (`mod::name`,
poza `Type::method`) nie są obsługiwane.

## 8. Wyjścia nielokalne (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; wykonuje body. jeśli (throw 'tag v) nastąpi gdziekolwiek,
                                    ; dokąd dociera body, to v staje się wartością
(throw 'tag value)                  ; wychodzi do najbliższego dynamicznie otaczającego (catch 'tag ...)
(unwind-protect protected cleanup)  ; wykonuje cleanup bez względu na to, jak opuszczono protected
```

W przeciwieństwie do `break`/`return` (rozdział 5) jest to wyjście **dynamiczne**: `throw` nie szuka leksykalnie
otaczającego `catch` i sięga do `catch` o tym samym znaczniku przez dowolną liczbę wywołań funkcji.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; jeśli nie znaleziono, wartość na końcu jak zwykle
```

- **Znaczniki to wyłącznie literały symboli** (`'done`). W przeciwieństwie do CL nie są obliczane.
- **Znacznik niesie typ.** Typ jest ustalany przy pierwszym użyciu `'tag`, a każde późniejsze
  `throw`/`catch` tego samego symbolu jest sprawdzane względem niego. Użycie z innym typem jest błędem typu.
- Typem `throw` jest `!` (nie wraca). Typem `(catch 'tag expr)` jest złączenie typu
  `expr` i typu znacznika.
- Wartością `unwind-protect` jest wartość `protected`. Wartość `cleanup` jest odrzucana.
  `cleanup` jest wykonywane bez względu na to, jak opuszczono `protected`: poza normalnym zakończeniem, `throw` i `panic`
  jest wykonywane także, gdy opuszczono je przez `break`/`return`/`return-from`. Wyjście nielokalne wykonane przez samo `cleanup` wygrywa
  z trwającym wyjściem.
- Zagnieżdżone `unwind-protect` wykonują się od środka na zewnątrz. `break` opuszczający pętlę **wewnątrz** `protected` nie
  opuścił `protected`, więc jego `cleanup` się nie wykonuje.

Kondycje z CL (`define-condition`/`handler-bind`/`invoke-restart`) nie zostały przyjęte. Nie pasują do
typowania statycznego, więc niepowodzenia odwracalne wyraża się za pomocą `Result` (rozdział 9).

## 9. Zasady obsługi błędów

- Niepowodzenia odwracalne: `Result<T,E>` + `match`. Niepowodzenia nieodwracalne (błędy, złamane niezmienniki):
  `panic`.
- Nie ma składni odpowiadającej `?`/try. Rozgałęzienia zapisuje się jawnie za pomocą `match`.
- Nazwy funkcji i form specjalnych nie używają `!` (operacje destrukcyjne) ani `?` (predykaty) jako
  przyrostków. Predykaty nazywa się z przyrostkiem `-p`/`p` (`zerop`, `consp` i tak dalej) lub przedrostkiem `is-`
  (`is-some`, `is-ok` i tak dalej).

## 10. Kompilacja

```lisp
(compile name)                      ; kompiluje JIT już zdefiniowane defun/metodę do kodu natywnego
(compile-file src-path out-path)    ; kompiluje AOT plik źródłowy do natywnego pliku wykonywalnego (pomija końcowe `(main)`)
(dump path)                         ; zapisuje bieżące środowisko (informacje o typach + skompilowane ciała) do jednego pliku
(disassemble name)                  ; wypisuje to, czym staje się ta definicja (domyślnie kod maszynowy hosta, LLVM IR z true jako drugim argumentem)
```

`compile` jest formą specjalną; `name` nie jest obliczane i jest czytane jako nieobliczony goły symbol / ścieżka `::`
(łańcuch znaków jest błędem typu). Funkcji generycznych nie można wskazać: kopia dla każdego typu jest tworzona w każdym
miejscu użycia, więc nie istnieje jedno skompilowane ciało. **Nazwa, której nie da się rozstrzygnąć, jest błędem
czasu sprawdzania** i nigdy nie jest przenoszona do czasu działania (są osobne komunikaty dla: typ istnieje, ale nie
ta metoda / nie istnieje ani typ, ani funkcja / goła niezdefiniowana nazwa). Widoczność jest tu
traktowana jak w każdym innym odwołaniu: „istnieje, ale nie jest stąd widoczne" kończy się błędem w czasie sprawdzania, tak jak
„nie rozstrzyga się".

Wywoływane funkcje są kompilowane przechodnio, więc **funkcji, która (nawet pośrednio) wywołuje coś, czego nie da się
skompilować, nie można skompilować**. Proces się nie zawiesza; jest odrzucana z błędem, który to mówi.
Każdą funkcję wbudowaną można skompilować, więc jedynymi funkcjami odrzucanymi w ten sposób są te wywołujące
następujące operacje działające tylko w interpreterze:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Operacjami tylko interpretera są `compile`/`compile-file`/`dump` oraz `trace`/`untrace`/`step`/`disassemble`
([Narzędzia implementacji](functions/system.md#5-narzędzia-implementacji-clhs-252)). Zamiast być rzeczami,
których nie można skompilować, są to operacje strony kompilującej (to, co zapisuje `dump`, to samo środowisko
interpretera, którego plik wykonywalny AOT nie ma; to, co obserwuje `trace` i gdzie zatrzymuje się
`step`, to ścieżki wywołań działającego interpretera; a `disassemble` używa samego kompilatora).
`room`/`dribble`/`ed` do nich nie należą i można je kompilować normalnie.

To, co **można** skompilować: wejście/wyjście strumieni i plików, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, funkcje przestępne,
operacje bitowe, `catch`/`throw`/`unwind-protect`, wszystkie cztery `eq`/`eql`/`equal`/`equalp`
(dzięki czemu `case` kompiluje się dla każdego typu), całą rodzinę wypisywania, w tym `print`/`println`/
`format`/`pprint` i `pprint-logical-block`, `read` oraz `eval`. Biblioteka standardowa jest dostarczana już
skompilowana.

Plik wykonywalny AOT zawiera tylko te funkcje, których używa program. Program, który nie wypisuje, nie dostaje
silnika formatowania, ten, który nie wywołuje `read`, nie dostaje czytnika, a ten, który nie wywołuje `eval`, nie dostaje
modułu sprawdzającego ani interpretera.

Z wiersza poleceń `typl -c src-path [-o out-path]` (`-c` można też zapisać `--compile`) robi to samo co
`compile-file`. Bez `-o` wyjściem jest `src-path` z usuniętym rozszerzeniem `.typl`. Domyślnie
biblioteka statyczna `libtypelisp_front.a` linkowana do plików wykonywalnych jest, dla wydaniowej wersji
`typl`, tą, którą `typl` nosi w sobie, zapisywaną przy pierwszym linkowaniu do
`$TYPELISP_HOME/lib/<identyfikator kompilacji>/` (lub `~/.typelisp/lib/<identyfikator kompilacji>/` bez `TYPELISP_HOME`) i używaną
stamtąd; dla wersji debugowej tą, w której zbudowano `typl`. `typl --remove-lib` usuwa to, co ten `typl`
zapisał. Z `--others` usuwa te z innych identyfikatorów kompilacji; z `--all` te z każdego identyfikatora kompilacji.
Z `typl --lib-dir DIR` używana jest ta z `DIR` (zarówno dla `-c`, jak i `compile-file`), a jeśli jej tam
nie ma, jest to błąd przy uruchomieniu.

### 10.1 Zrzuty (dumps)

```lisp
(dump "session.typld")     ; zapisz jeden
```
```sh
typl --image session.typld prog.typl   # zacznij od niego
typl --image session.typld             # REPL też
```

Zrzut przechowuje informacje o typach i skompilowane ciała w jednym pliku. To, co zapisuje `(dump path)`, to to,
co załadowała bieżąca sesja (biblioteka standardowa lub zrzut przekazany za pomocą `--image`) plus **to, co sesja
sama zdefiniowała**. Zatem wyjście jest samodzielne, a `typl --image` podnosi to samo środowisko.
To, co sesja `(compile f)`-owała, jest zapisywane w postaci skompilowanej.

Zapisywane są **definicje, a nie historia**:

- Wyrażenia najwyższego poziomu sesji (`(println ...)` i tak dalej) nie są uwzględniane. Byłoby problemem,
  gdyby wczytanie uruchamiało je ponownie.
- Zmienne globalne wracają z **wartością ponownie uruchomionego inicjalizatora**, a nie wartością z chwili
  zrzutu. To zamierzona różnica względem `save-lisp-and-die` z SBCL (które zapisuje stertę bez zmian),
  a ten wybór sprawia, że znika cała rodzina problemów: „wartości, których nie da się zapisać", takie
  jak otwarte strumienie, wskaźniki funkcji domknięć i pamięć zewnętrzna.
- W przeciwieństwie do `save-lisp-and-die` **proces nie umiera**, ponieważ zapis nie uszkadza obrazu.

Zrzut zapisuje wersje biblioteki standardowej i kompilatora implementacji, która go zapisała.
Wczytanie go za pomocą `typl` w innej wersji jest błędem; nigdy nie jest po cichu akceptowane.

### 10.2 `eval` w plikach wykonywalnych AOT

`eval` sprawdza typy względem „bieżącego środowiska globalnego", a potem oblicza
([Parsowanie i obliczanie](functions/system.md#6-parsowanie-i-obliczanie)). To środowisko (tablice
sygnatur, typów i makr, do których odwołuje się moduł sprawdzający, oraz ciała, które interpreter może uruchomić) **nie znajduje się w
kodzie maszynowym**. Skompilowana funkcja to nic innego jak symbol umieszczony pod adresem; nie ma ani
typów argumentów, ani tablicy do wyszukiwania ciał po nazwie.

Dlatego tylko dla programów wywołujących `eval` `compile-file` **buduje to środowisko w czasie kompilacji
i zapisuje je do pliku wykonywalnego**. Format jest taki sam jak zrzutu, zawierający część biblioteki standardowej
i część własną programu. Przy uruchomieniu następuje tylko jego odtworzenie: kod źródłowy nie jest czytany ponownie, a
nic nie jest ponownie sprawdzane pod względem typów. Do programów niewywołujących `eval` nic nie jest dodawane.

Konsekwencje:

- **Uruchamianie trwa dłużej, a plik wykonywalny jest większy**, ponieważ trafia do niego kod modułu sprawdzającego i
  interpretera oraz migawka środowiska. Sterta jest także nieco większa.
- **Formy przekazane do eval są interpretowane.** Nawet gdy forma przekazana do eval wywołuje własne
  funkcje programu, uruchamiane jest interpretowalne ciało przechowywane w migawce. Wynik jest taki sam; różni się tylko
  szybkość.

Pamięć zmiennych globalnych jest **współdzielona** ze skompilowanym kodem (te same sloty). Inicjalizator `defvar`
jest uruchamiany raz przez skompilowaną inicjalizację, a odtworzenie go pomija, więc inicjalizator z efektami ubocznymi
nie uruchamia się dwa razy.

`compile-file` czyta także bibliotekę standardową (i osadza jej ciała w pliku wykonywalnym), więc
funkcje biblioteki standardowej, takie jak `abs`/`gcd`, oraz `(impl print-object ...)`, jak i
`(defmethod print-object ...)`, można używać z AOT.

`compile-file` akceptuje też `use` (oraz `import`/`shadowing-import`). `(use m)` pliku wejściowego znajduje pliki
według tych samych reguł co `typl file.typl`, a znalezione pliki zależności są także kompilowane i linkowane do
pliku wykonywalnego: układ, w którym `main.typl` czyta `http.typl` przez `(use http)`, można skompilować AOT bez
zmian. Własne definicje pliku wejściowego także trafiają do modułu nazwanego od pliku, tak jak przy
`typl file.typl` (`point` w `p.typl` to `p::point`). Zatem reprezentacja wypisywana wartości
(`#<p::point x: 1 y: 2>`) jest taka sama bez względu na sposób uruchomienia.

## 11. Makra czytnika (readtable)

To, co czytnik **robi po napotkaniu danego znaku**, można zastąpić z poziomu programu (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f wczytuje znak c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f wczytuje dwuznakową sekwencję d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Typem `f` jest `(fn (string-input-stream char) Option<Sexpr>)`. Pierwszy argument to **strumień nad
jeszcze niewczytanym tekstem**, a drugi to **znak, który go wywołał** (drugi znak w przypadku
dyspozycji). Wartość zwracana staje się danymi wczytanymi w tym miejscu. Strumień jest konkretnym typem, a nie
`:dyn PeekInput`, ponieważ czytnik zawsze przekazuje ten jeden rodzaj: `read-sexpr` / `read-char` / `peek-char` /
`unread-char` / `read-delimited-list` wszystkie przyjmują `(where (PeekInput S))`, więc wszystkie działają na konkretnym
typie bez zmian.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => wczytane jako (not (equal 1 2)), czyli true
```

Czytnik **patrzy na znaki makr przed składnią wbudowaną**, więc może przejąć także `(` i `'`.
Podznaki `#` zarejestrowane w ten sposób mają pierwszeństwo przed wbudowanymi `#b`/`#x`/`#.`. Znak inny
niż `#` staje się znakiem dyspozytorskim od razu po przekazaniu do `set-dispatch-macro-character`: **nie ma**
odpowiednika `make-dispatch-macro-character` z CL. Rejestracja już wykonuje swoją pracę, więc osobny
krok nie miałby nic do zrobienia.

**Kiedy wchodzą w życie**, zależy od ścieżki czytania, tak samo jak dla `#.` (rozdział 1):

- REPL i `(load ...)` wykonują po jednej formie naraz, więc **funkcje zdefiniowane we wcześniejszych formach** można
  rejestrować bez zmian.
- Pliki modułów są sprawdzane jako całość i wykonywane później, więc **natychmiast wykonują się tylko wywołania `set-macro-character` /
  `set-dispatch-macro-character`** (rola `(eval-when (:compile-toplevel) ...)` z CL).
  Ponieważ wykonują się natychmiast, **przekazana funkcja musi już istnieć w tym momencie**. `defun` w
  tym samym pliku jeszcze się nie wykonało, więc zapisz `lambda` albo użyj biblioteki standardowej lub czegoś, co
  już się wykonało. Objęte są tylko wywołania najwyższego poziomu; nie zagląda do wnętrza `progn` ani `let`.

Wbudowane `read` / `read-from-string` także korzystają z readtable (jak w CL).

**Czego nie ma**: `*readtable*` oraz `copy-readtable` i `readtable-case`. Dwa pierwsze, ponieważ readtable
**nie jest wartością**: wartość musiałaby być „czymś, co można przekazać czytnikowi", ale
czytnik czytający kod źródłowy jest poza programem i nie ma gdzie go przekazać. `readtable-case`, ponieważ
rozdział 1 ustala, że czytnik tego języka zawsze zamienia na małe litery (`:downcase` z CL).


## 12. Współbieżność (zadania)

**Zadanie to lekki wątek** (w terminach Go to, co uruchamia instrukcja `go`) i działa kooperacyjnie
(nie ma wywłaszczania). Przełączanie nie przechodzi przez jądro, a stan wykonania znajduje się na
stercie, a nie na stosie maszynowym, więc tworzenie dużej liczby zadań jest tanie.

**Zadania działają jednocześnie na kilku wątkach systemu operacyjnego** (równoległość wielordzeniowa). Liczbę wątków określa
zmienna środowiskowa `TYPELISP_THREADS` (łącznie, wraz z wątkiem uruchamiającym `main`; wartość domyślna
to równoległość maszyny). W `typl` **tylko zadania skompilowane** działają na innych wątkach, a zadania
interpretowane działają na wątku interpretera (12.7). Dane współdzielone przechodzą przez `Mutex<T>` lub `Chan<T>`;
jednoczesny odczyt i zapis, które tego nie robią, są niezdefiniowane, jak w Go (12.7).

Ze słownictwa **tylko `task` / `thread` / `select` są formami specjalnymi**; reszta to zwykłe
funkcje, metody i makra ([Zadania i kanały](functions/concurrency.md)).

### 12.1 `task` — uruchamianie zadania

```lisp
(task (f arg...))                   ; zwraca Task<T>, gdzie T to typ zwracany przez f
```

**Przyjmuje tylko formę wywołania.** `f` i każde `arg` są obliczane tam, gdzie zapisano `task`, w
zapisanej kolejności, a w nowym zadaniu odbywa się tylko **samo wywołanie**. Jest to ta sama reguła co `go f(x)` w Go,
i dlatego przyjmuje formę wywołania, a nie thunk: thunk przechwyciłby swoje argumenty
bez ich obliczenia.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i jest obliczane na miejscu za każdym razem; brak pułapki przechwytywania

(task ((lambda () ()                ; aby uruchomić dowolne ciało, wywołaj lambdę
         (println "start")
         (send ch 1))))
```

Form specjalnych (`if` / `let` / `progn` …) nie można zapisać bezpośrednio pod `task`.

**Dlaczego nie może to być funkcja**: zapisanie `(spawn (lambda () T body...))` wymagałoby podania `T`,
ponieważ `lambda` wymaga adnotacji typu zwracanego, a makro nie zna typu zwracanego przez `(f a b)`.
Zna go tylko moduł sprawdzający.

### 12.2 `thread` — uruchamianie zadania na dedykowanym wątku systemu operacyjnego

```lisp
(thread (f arg...))                 ; zwraca Thread<T>, gdzie T to typ zwracany przez f
(join th)                           ; czeka na zakończenie i zwraca jego wartość (dowolną liczbę razy)
```

Forma i reguły obliczania są takie same jak w `task` (przyjmuje tylko formę wywołania, a `f` i `arg` są
obliczane tam, gdzie zapisano). Różnica polega na miejscu działania: **uruchamia wątek systemu operacyjnego dedykowany
temu zadaniu i działa tylko na nim**. Nie jest multipleksowane z innymi zadaniami, więc wywołanie blokującej funkcji C
(`defffi`) wewnątrz zatrzymuje tylko ten wątek, a inne zadania robią postępy. Wewnątrz można używać `task`, `send`, `recv`
i reszty bez zmian.

- `Thread<T>` to odpowiednik `Task<T>`. Tak jak `wait`, `join` zatrzymuje **wywołujące zadanie**, a wartość
  jest buforowana. Gdy zadanie się kończy, kończy się także wątek.
- Reguły panic są takie same jak dla `task` (cały proces się zatrzymuje). Gdy `main` wraca, proces
  się kończy.
- Aby zapisać to jako funkcję, użyj `(Thread::spawn (lambda () T body...))` (`std::thread::spawn` z Rust).
  Można też przekazać funkcję nazwaną.
- **Na dedykowanym wątku działa tylko kod skompilowany.** Gdy `typl` oblicza `(thread (f ...))` lub
  `Thread::spawn` w trakcie interpretowania, kompiluje funkcję do uruchomienia (i to, co wywołuje) na miejscu,
  zanim ją uruchomi. To, czego nie da się skompilować (`lambda` odwołująca się do lokalnych zmiennych spoza niej,
  tworzenie struktury i tak dalej), jest, zanim wątek zostanie uruchomiony, panic traktowanym tak samo jak
  `(panic ...)`. `lambdę` odwołującą się do zmiennych lokalnych można przekazać, jeśli została utworzona wewnątrz
  skompilowanej funkcji.

### 12.3 `select` — oczekiwanie na kilka operacji na kanałach naraz

```lisp
(select
  ((v (recv ch1)) body...)          ; ramię odbierające. v jest związane z Option<T>
  ((send ch2 x) body...)            ; ramię wysyłające
  (else body...))                   ; opcjonalne. **jeśli zapisane, idzie na końcu**
```

- **Z `else` nie blokuje** (`default` z Go). Bez niego czeka, aż któraś stanie się możliwa.
- **Jeśli kilka jest możliwych naraz, jedna jest wybierana losowo** (w kolejności zapisu późniejsze ramiona
  byłyby głodzone).
- `v` w ramieniu odbierającym to **`Option<T>`**. Zamknięty kanał to „odpowiedź", a nie powód do pominięcia
  ramienia, więc wykonaj na nim `match` wewnątrz ramienia.
- Typem jest **złączenie typów ciał wszystkich ramion** (ta sama reguła co dla ramion `match`).
- `(select)` z zerem ramion jest błędem typu (`select{}` z Go, blokujące na zawsze, nie zostało przyjęte). `select`
  z samym `else` także, ponieważ to to samo, co zapisanie ciała bezpośrednio.

**Wyrażenia kanałów i wartości do wysłania są obliczane po jednym razie, od lewej do prawej, bez względu na to, które
ramię zostanie wybrane** (ta sama dyscyplina, którą `case` ma dla swoich kluczy).

```lisp
(select                             ; odbiór z limitem czasu
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([kanał dostarczający po upływie czasu](functions/concurrency.md#5-after--kanał-dostarczający-po-upływie-czasu))
to „kanał dostarczający jedną wartość po `sec` sekundach", odpowiadający `time.After` z Go.

### 12.4 Współdziałanie z innymi funkcjami

| Funkcja | Jak odnosi się do zadań |
|---|---|
| `catch` / `throw` | **Nie przekraczają granic zadań.** `throw` próbujący opuścić ciało zadania jest panic |
| `unwind-protect` | Kod sprzątający jest wykonywany, gdy zadanie kończy się naturalnie. **Nie jest wykonywany, gdy proces kończy się, bo skończyło się główne zadanie** |
| `block` / `return-from` | Leksykalne, więc nie przekraczają granic `lambda` |
| `panic` | Jak w Go, zatrzymuje cały proces. `wait` nie obserwuje panic jako wartości |
| `dlet` | **Nie jest wiązaniem na zadanie.** Nadal „pożycza i oddaje zmienną globalną", więc zadania sobie przeszkadzają |
| Standardowe wyjście | Współdzielone przez wszystkie zadania. Wyjście jednego `println` nigdy nie jest mieszane z innymi w środku linii |
| `compile` / `eval` | Bez ograniczeń. `(compile f)` wewnątrz zadania działa |

### 12.5 Gdzie zadania się przełączają

Planowanie jest kooperacyjne, więc **zadania przełączają się tylko tam, gdzie to zapiszesz**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **operacje na kanałach, które muszą czekać** (`send`/`recv`/`select`) oraz **operacje na gniazdach,
które muszą czekać** (`accept` / `tcp-connect` (w tym rozwiązywanie nazw) / odczyt i zapis gniazd /
`recv-from`; [Sieć](functions/network.md)). Wszystkie gniazda są nieblokujące: jeśli któreś nie jest gotowe, zatrzymuje się tylko
to zadanie i wznawia się, gdy system powie, że jest gotowe, w tym samym kształcie co netpoller z Go. Dopiero gdy żadne
zadanie nie może działać, implementacja czeka na system do najbliższego terminu `sleep`.

Operacje na kanałach, które mogą odpowiedzieć od razu (`send` z miejscem w buforze, `recv` z czekającą wartością,
`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`), **nie zużywają kolejki**. Oznacza to, że nie jesteś
nieoczekiwanie przerywany przez odczyt, i jest to traktowane inaczej niż `(sleep 0.0)`, które jest „yield na 0 sekund"
z CL.

**Nie ma wywłaszczania.** Ciasna pętla, która niczego nie wywołuje, głodzi inne zadania. Jednak skompilowane pętle
okresowo oddają sterowanie planiście, więc skompilowana ciasna pętla ich nie głodzi.

### 12.6 Skompilowany kod i zadania

Skompilowany kod także może zawieszać zadania. To samo dotyczy plików wykonywalnych tworzonych za pomocą `compile-file`: `main` działa jako
główne zadanie planisty, a `task`, `sleep`, `wait`, kanały i oczekiwanie na gniazda działają z tym samym
znaczeniem co w `typl`. Gdy `main` wraca, proces się kończy, a pozostałe zadania są ucinane (jak w Go).
Interpreter nigdy nie trafia do pliku wykonywalnego ze względu na planistę.

Jedynym wyjątkiem jest „wewnątrz wywołania zwrotnego C FFI", gdzie operacje, które **musiałyby czekać**, są błędami
(przyjaźniejszymi niż ciche zakleszczenie): gdy funkcja przekazana za pomocą `defffi` jest wywoływana z C, stos C
jest na wierzchu i nie ma sposobu, by zawiesić zadanie i wznowić je później.

Poniższe miejsca to także funkcje wywoływane w środku zadania, które jednak nie mogą się zawiesić:
metody `print-object`, `~/name/` w `format`, makra czytnika, wnętrze `eval` i inicjalizatory `defvar` w
plikach wykonywalnych AOT. Tutaj **operacje odpowiadające bez czekania przechodzą** (`(recv ch)` z wartością w
buforze, `read-line` na gnieździe z już odebranymi danymi, `(task ...)`, `(yield)` i tak dalej), a
**operacje, które naprawdę musiałyby czekać, są błędami** (nie zatrzymują procesu na miejscu, lecz powodują
panic w rodzaju `` `recv` cannot block: ... ``, traktowany tak samo jak `(panic ...)`).

### 12.7 Różnice względem Go

- **W `typl` tylko zadania skompilowane wychodzą na inne wątki.** Stanu interpretera nie można współdzielić
  między wątkami, więc zadania z interpretowanego `task` działają na wątku interpretera. Zadanie skompilowane
  także **przenosi się na wątek interpretera i tam zostaje** (nie wraca) w momencie, gdy
  wywołuje interpretowaną wartość funkcyjną, wywołuje metodę `:dyn`, której nikt nie skompilował, lub wywołuje
  `eval`/`macroexpand`/`read`. Jeśli długie obliczenie choć raz po drodze dotknie kodu interpretowanego,
  reszta działa na wątku interpretera.
- **W `typl` pracownicy żyją tylko przez jedno obliczenie najwyższego poziomu.** Gdy REPL czeka na dane wejściowe oraz
  między formami najwyższego poziomu, inne wątki nie posuwają zadań naprzód (pozostałe zadania kontynuują od miejsca, w którym
  skończyły, w następnym obliczeniu). Na końcu obliczenia czeka się, aż każdy wątek skończy swój
  bieżący krok, więc jeśli funkcja C (`defffi`) stale blokuje wewnątrz `thread`, obliczenie się nie kończy,
  dopóki nie wróci.
- **Wypisywanie na pracownikach**: interpretowane metody `print-object` / `~/name/` nie mogą działać na innych wątkach,
  więc wypisanie takich wartości na innym wątku jest panic traktowanym tak samo jak `(panic ...)` (`(compile
  T::print-object)` lub wypisz z głównego zadania).
- **Wyścigi danych są niezdefiniowane** (to samo stanowisko co w Go). Wynik zmiany tej samej wartości przez kilka zadań
  bez użycia `Mutex<T>` / `Chan<T>` nie jest gwarantowany.
- **`task` zwraca wartość.** W przeciwieństwie do instrukcji `go` z Go zwraca `Task<T>`, a `(wait t)` pobiera
  wynik.
- **Nie ma kanałów nil.** Idiomu fan-in z Go (ustawienie zamkniętego kanału na `nil`, aby usunąć go z
  ramion `select`) nie da się zapisać, więc uruchom jedno zadanie na wejście i połącz je za pomocą `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--oczekiwanie-na-n-zakończeń)). Jest to także
  zalecany sposób w Go, ale to **pierwsza różnica, na którą natrafiają osoby przychodzące z Go**.
