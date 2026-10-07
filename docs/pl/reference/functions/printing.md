<!-- translated-from: docs/ja/reference/functions/printing.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Wypisywanie

`print`/`println`/`format`, drukarki jednoargumentowe, pretty printer, `print-object` oraz
zmienne sterujące wypisywaniem. Lista dyrektyw formatu znajduje się w [format.md](format.md). Odczyt ze
strumieni i zapis do nich opisano w [Strumieniach i plikach](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` to wszystko **formy specjalne interpretujące dyrektywy formatu (dyrektywy `format`
z CL)**. Pierwszy argument (drugi dla `format`) to **łańcuch sterujący**, a każda
dyrektywa zużywa kolejne argumenty o zmiennej liczbie po kolei.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Rozwija łańcuch sterujący i zapisuje go na standardowe wyjście bez nowej linii |
| `println` | `(println control args...)` | `(string, ...)→Unit` | To samo, z nową linią na końcu |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | `format` z CL. Zwraca rozwinięty łańcuch. Jeśli `dest` to `true` (`t` z CL), jest on także zapisywany na standardowe wyjście; jeśli `false` (`nil` z CL), nie jest zapisywany i jest tylko zwracany |
| `format` (do strumienia) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Jeśli `dest` nie jest `bool`, jest to cel będący strumieniem z CL. Rozwinięty łańcuch jest zapisywany do tego strumienia. Wartością zwracaną jest `()` (`nil` z CL) i żaden łańcuch nie jest zwracany |

Typ `dest` rozdziela znaczenie na dwa (to, które obowiązuje, jest ustalane statycznie). Formę strumieniową
można zapisać tak samo z konkretnym typem strumienia, z `:dyn CharOutput` lub ze zmienną typową
związaną przez `(where (CharOutput S))`. `dest`, które nie jest ani `bool`, ani strumieniem, jest błędem typu.

**Łańcuch sterujący musi być literałem** (to samo ograniczenie co w `format!` z Rust). Dyrektywy
w nim zawarte decydują o tym, ile argumentów jest branych i jakich typów, więc łańcuch zbudowany w czasie działania nie może być
odczytany w czasie sprawdzania. Ponieważ musi być literałem, **liczba i typy argumentów są sprawdzane
w czasie sprawdzania**: `(println "~d" "x")` i `(println "~a ~a" 1)` są błędami czasu sprawdzania. Błędnie zapisana
dyrektywa, niezamknięte `~(` oraz `~/name/`, na które nie potrafi odpowiedzieć żaden argument, też są błędami czasu sprawdzania.
Reguły sprawdzania opisano w [format.md](format.md#1-jak-zapisywać-dyrektywy). Aby wypisać łańcuch, który
budujesz, utwórz go za pomocą `(format false ...)` i wypisz za pomocą `(println "~a" s)`.

Argumenty o zmiennej liczbie są opakowywane w `Sexpr` z własnymi typami, zanim zostaną przekazane: `i32`/`f64`/
`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, a także zdefiniowane przez użytkownika `defstruct`/`defenum`/
`Vector<T>`/`HashTable<K,V>` i podobne, można przekazywać bez zmian (`(println "~a" my-struct)`
po prostu działa).

Uruchomienie skryptu za pomocą `typl file.typl` **nie wypisuje wartości wyrażeń najwyższego poziomu**, więc
program zapisuje na standardowe wyjście, wywołując te funkcje. `print`/`println`/`format` wysyłają swoje wyjście
przy każdym wywołaniu (aby zachęta była widoczna przed odczytem standardowego wejścia, nawet przez potok).

**`Option<Sexpr>` wypisuje się przezroczyście.** Typem danych w postaci S-wyrażeń jest `Option<Sexpr>`, więc
opakowanie `(some x)` nie pojawia się na wyjściu, a zawartość jest wypisywana bez zmian. Pusta
lista wypisuje się jako `()`. Inne `Option<T>` wypisują się jako `(some ...)` / `none`. To samo dotyczy pól `Option<T>`
wewnątrz struktur, enumeracji i `Vector`. `Result<Option<Sexpr>,…>` z `(eval ...)` wypisuje się jako
`(ok 42)`, albo `(ok ())` dla `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; uzyskaj tylko łańcuch, bez wypisywania
  (println "~a" s))                   ; => id=42
```

## 2. Drukarki jednoargumentowe

Drukarki z CLHS 22.1.3. Zamiast rozwijać format, wypisują pojedynczą wartość bez zmian.
Strumień można pominąć (domyślnie `*standard-output*`).

| Nazwa | Forma | Opis |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Zapisuje w postaci możliwej do wczytania z powrotem (tak samo jak `~s`) i zwraca `x` |
| `princ` | `(princ x [stream])` | Zapisuje w postaci dla ludzi (tak samo jak `~a`) i zwraca `x` |
| `write` | `(write x [stream])` | `prin1`, jeśli `*print-escape*` jest prawdą, `princ`, jeśli fałszem. Zwraca `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Zwraca łańcuch zamiast zapisywać (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | To samo (`~a`). Tak samo jak `to-string` |
| `write-to-string` | `(write-to-string x)` | To samo, zgodnie z `*print-escape*` |

`print`/`println` **nie** należą do tej grupy. Są skrótami dla `format` przyjmującymi łańcuch sterujący,
co jest innym zadaniem niż `print` z CL (nowa linia, potem `prin1`, potem spacja), więc każde zachowuje własną
nazwę. W rezultacie **jednoargumentowe `print` z CL nie ma zapisu w tym języku**: napisz `prin1`.

Są to makra, ponieważ argumenty o zmiennej liczbie w `format` nie przyjmują zmiennych typowych, a
typ musi być znany w miejscu wywołania.

## 3. Standardowe wejście i strumienie standardowe

**Odczyt standardowego wejścia** wykonuje się nie za pomocą dedykowanych funkcji, lecz metod `CharInput` na
strumieniu standardowym `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([metody strumieni](streams-files.md#2-metody)). Standardowe wyjście i
standardowe wyjście błędów mają podobnie `*standard-output*` / `*error-output*` i można pisać na nie jak w
`(write-line *standard-output* s)` (`print`/`println`/`format` to skróty na wypadek, gdy potrzebujesz rozwinięcia
formatu, i zawsze zapisują na standardowe wyjście).

## 4. Pretty printer

Odpowiada Lisp Pretty Printer z CL (CLHS 22.2). **Łamie wyjście, które nie mieści się w
szerokości linii, według bloków logicznych i warunkowych nowych linii.**

### 4.1 Zmienne sterujące

Zmienne globalne, do których można przypisywać. Po `setf` wpływają na całe późniejsze wypisywanie. Aby zmienić jedną
tymczasowo, użyj `dlet` (6.3).

| Zmienna | Typ | Domyślnie | Znaczenie |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Jeśli prawda, `~a`/`~s`/`~w` i dyrektywy ozdobne idą ścieżką ozdobnego wypisywania |
| `*print-right-margin*` | `int` | `80` | Prawy margines (w kolumnach). 0 oznacza „bez marginesu, nigdy nie łam". Wartość ujemna jest błędem wypisywania |
| `*print-miser-width*` | `int` | `0` | Szerokość, od której zaczyna się styl miser. 0 odpowiada `nil` z CL (styl miser wyłączony). Wartość ujemna jest błędem wypisywania |

Rodzina `pprint` i `pprint-logical-block` zawsze wypisują ozdobnie, niezależnie od `*print-pretty*`
(zgodnie z definicją `pprint` z CL).

### 4.2 Gotowe układy (formy specjalne)

Tak jak `print`, są to formy specjalne, więc argument może być dowolnego typu.

| Nazwa | Forma | Opis |
|---|---|---|
| `pprint` | `(pprint x)` | Wypisuje ozdobnie z układem domyślnym. Jak w CL, **najpierw zapisuje nową linię**, a na końcu żadnej |
| `pprint-fill` | `(pprint-fill x)` | Wypełnia każdą linię tym, co się zmieści. Nie zapisuje nowej linii |
| `pprint-linear` | `(pprint-linear x)` | Jeśli nie wszystkie elementy mieszczą się w jednej linii, **jeden element na linię**. Nie zapisuje nowej linii |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Tabela z kolumnami o szerokości `colinc` (domyślnie 16). Nie zapisuje nowej linii. Ujemne `colinc` jest błędem |

Układ domyślny (`pprint` oraz `~a` przy `*print-pretty*`) idzie za domyślnym
`*print-pprint-dispatch*` z CL: skraca `(quote x)` do `'x` i formatuje formy kodu, takie jak
`defun`/`let`/`if`/`lambda`, jako „głowa i przepisana liczba argumentów w pierwszej linii,
a reszta ciała wcięta o dwie kolumny, jedna forma na linię". Inne listy są wypełniane.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Samodzielne budowanie bloków logicznych

| Nazwa | Forma | Opis |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Forma specjalna otwierająca blok logiczny. `obj` to lista, po której przechodzi `pprint-pop` (`()`, jeśli żadna nie jest przechodzona). `:prefix` i `:per-line-prefix` wzajemnie się wykluczają (jak w CL) |
| `pprint-newline` | `(pprint-newline kind)` | Warunkowa nowa linia. `kind` to `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Wcięcie. `kind` to `:block` (od początku bloku) / `:current` (od bieżącej kolumny) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Tabulacja. `kind` to `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` i `colinc` są nieujemne (błąd, jeśli ujemne) |
| `pprint-pop` | `(pprint-pop)` | Bierze następny element z listy bloku (`()`, jeśli się wyczerpała) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Czy lista się wyczerpała |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Jeśli się wyczerpała, wykonuje `break` z otaczającego `loop` (makro) |

Bloki logiczne nie przyjmują argumentu strumienia: **otwarty blok logiczny jest stanem niejawnym**.
Najbardziej zewnętrzny `pprint-logical-block` go rozpoczyna, a gdy się zamyka, całość jest formatowana
i zapisywana na standardowe wyjście naraz. Dopóki jest otwarty, wyjście z `print`/`println`/
`(format true ...)`/`pprint` trafia do tego bloku, więc **zawartość zapisujesz zwykłym
`print` i zaznaczasz tylko miejsca łamania za pomocą `pprint-newline` i pokrewnych**, dzięki czemu kod
wygląda niemal tak samo jak w CL.

W CL `pprint-exit-if-list-exhausted` to nielokalne wyjście z `pprint-logical-block`; tutaj jest to
**`break` z otaczającego `loop`** (`pprint-logical-block` nie ustanawia `block`). Idiom z CL i tak zawsze
umieszcza go wewnątrz `loop`, więc czyta się to tak samo.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Reguły warunkowych nowych linii (CLHS `pprint-newline`):

- `:mandatory` zawsze łamie.
- `:linear` łamie, jeśli otaczający blok logiczny nie mieści się w jednej linii. Decyzja jest podejmowana dla bloku,
  więc **wszystkie nowe linie `:linear` jednego bloku łamią się razem** (to „wszystko w jednej linii albo jeden
  element na linię" z `pprint-linear`).
- `:fill` łamie, jeśli (a) następna sekcja nie mieści się w reszcie linii, (b) poprzednia
  sekcja nie mieściła się w jednej linii lub (c) w stylu miser blok nie mieści się w jednej linii.
- `:miser` działa jak `:linear` tylko w stylu miser (gdy blok zaczyna się w odległości
  `*print-miser-width*` od prawego marginesu).

## 5. `print-object` (reprezentacja wypisywana według typu)

Napisanie `impl print-object <type>` sprawia, że `print`/`println`/`format`/`pprint` wypisują wartości tego typu
za pomocą tej implementacji, **nawet gdy są zagnieżdżone w listach**. Odpowiada funkcji generycznej
`print-object` z CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Znaczenie |
|---|---|
| `self` | Wartość do wypisania |
| `escape` | `*print-escape*` z CL. `true` dla `~s`/`prin1`/`pprint` (postać możliwa do wczytania z powrotem), `false` dla `~a`/`princ` (dla ludzi). Implementacja, której to nie obchodzi, może go zignorować |

Zwrócony `string` trafia wprost na wyjście. Typy bez `impl` wypisują się w reprezentacji wbudowanej
(w postaci `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   działa także przy zagnieżdżeniu
```

Łączy się także z pretty printerem (rozdział 4). Jeśli `*print-pretty*` jest prawdą, lista
zawierająca łańcuchy zwrócone przez implementację jest łamana na prawym marginesie.

Reprezentacje wypisywane dla typów biblioteki standardowej. Typy, które istnieją także w CL, wypisują się tak samo
jak w SBCL. Gdy REPL pokazuje wynik, używa tej samej reprezentacji co `~s`.

| Typ | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#<vector<int> 1 2 3>` | To samo (elementy z `~a`) |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | To samo |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (liczba to wewnętrzny numer seryjny) | To samo |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Liczba całkowita (wartość `get-universal-time` / `get-internal-real-time` z CL) | To samo |
| Typy błędów (`ParseIntError`, `SimpleError` i tak dalej) | `#<simpleerror "boom">` | Tylko komunikat (`boom`) |
| `complex` | `#C(1.0 2.0)` | To samo |
| `Array<T>` | `#2A((0 0) (0 0))` | To samo |
| Strumienie | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | To samo |
| Gniazda | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | To samo |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` na końcu w czasie letnim) | To samo |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | To samo |
| Typy `defstruct` | `#<point x: 1 y: 2>` (nazwy pól i wartości) | To samo (pola z `~a`) |

Reguły:

- **Rejestracja jest statyczna.** `impl` jest sprawdzane pod względem typów jak zwykła definicja metody, więc
  błędnie zapisana nazwa typu lub zły podpis są błędem kompilacji.
- **Działa także dla typów generycznych.** `(impl print-object box<T> (where (print-object T)) ...)` trafia do
  osobnego ciała dla każdego argumentu typu: wartość pamięta swój typ łącznie z argumentami typu
  (`box<i32>`). Wbudowane typy generyczne, takie jak `Vector<T>`, działają tak samo.
- **Wybór jest dokonywany w czasie wypisywania.** To, która dyrektywa zużywa który argument, zależy od
  zawartości łańcucha sterującego w czasie działania, więc rozróżnienie między `~a` a `~s` (czyli
  `escape`) jest znane dopiero w chwili wypisywania. Jest to to samo co w CLOS, gdzie metody `print-object`
  są „definiowane dla klasy i wybierane w czasie wypisywania".
- **Ponowne wejście wraca do reprezentacji wbudowanej.** Jeśli implementacja wypisuje samą siebie za pomocą
  `(format false "~a" self)`, rekurencja trwałaby w nieskończoność, więc gdy wypisywana wartość pojawia się ponownie,
  używana jest reprezentacja wbudowana. Sprawdzana jest tożsamość wartości, a nie limit głębokości, więc nie
  przeszkadza to w uprawnionym wypisywaniu zagnieżdżonych struktur odwołujących się do siebie.
- **Każdy typ skalarny implementuje ten trait.** Dzieje się tak, **aby można go było używać jako ograniczenia**: argumenty
  o zmiennej liczbie w `format` nie przyjmują zmiennych typowych, więc to ograniczenie jest jedynym sposobem, w jaki kod generyczny
  może powiedzieć „wartości nieznanego typu można wyrenderować" (ten sam kształt co `T: Display` w Rust).
  Przykładem jest `print-object` dla `Array<T>`.
- **Przy argumentach typu niespełniających ograniczenia po cichu używana jest reprezentacja wbudowana.**
  `(impl print-object Array<T> (where (print-object T)))` ma zastosowanie do `Array<i32>`, ale nie do
  `Array`, której elementami są `defstruct` bez `print-object`. Nie miałoby sensu, żeby samo
  utworzenie tablicy było błędem, więc nie jest.
- Inny mechanizm CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (rejestr czasu działania indeksowany
  specyfikatorami typów), **nie został przyjęty**. Jego rejestracje nie są sprawdzane, co nie pasuje do języka typowanego
  statycznie.

## 6. Kontrolowanie, ile jest wypisywane

### 6.1 Głębokość, długość i współdzielenie

Zmienne sterujące z CLHS 22.1.1, które decydują o tym, „ile z wartości jest wypisywane". Tak jak trzy z
4.1, są to zmienne globalne, do których można przypisywać, i dotyczą wszystkich `print`/`println`/`format`/`pprint`,
niezależnie od tego, czy `*print-pretty*` jest prawdą.

| Zmienna | Typ | Domyślnie | Znaczenie |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Obiekty zagnieżdżone na tej głębokości lub głębiej są zastępowane przez `#`. Wypisywany obiekt jest na głębokości 0. 0 oznacza bez limitu |
| `*print-length*` | `int` | `0` | Wypisuje elementy listy (oraz pola wartości `defstruct`/`defenum`) do tej liczby, a resztę zastępuje przez `...`. 0 oznacza bez limitu |
| `*print-circle*` | `bool` | `false` | Jeśli prawda, wartość jest skanowana przed wypisaniem, a **obiekty występujące dwa razy lub więcej dostają etykiety**. Pierwsze wystąpienie to `#n=…`, a kolejne `#n#` |

CL używa `nil` dla „bez limitu", ale ten język nie ma `nil`, więc tak jak przy `*print-right-margin*`
**0 oznacza bez limitu**. Wartości ujemne nie mają znaczenia i są błędami wypisywania. Wartości domyślne to wszędzie
„bez limitu / bez etykiet", zgodnie z wartościami początkowymi w CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Struktury cykliczne można wypisywać tylko wtedy, gdy `*print-circle*` jest prawdą.** Jeśli wypiszesz wartość, która
wskazuje na samą siebie, gdy jest fałszem (domyślnie), drukarka podąża za cyklem w nieskończoność, a proces
ulega awarii. CL jest takie samo (CLHS pozostawia wypisywanie struktur cyklicznych niezdefiniowane, gdy `*print-circle*` jest
fałszem).

Cykl można utworzyć tylko przez „skierowanie pola `defstruct` na siebie samo za pomocą `setf`" (komórek `Sexpr` nie można
zmienić po utworzeniu, więc lista taka jak `'(1 2 3)` nigdy nie może być cykliczna):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a wskazuje na samo a
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Etykiety **zaczynają się od 1 dla każdej wypisywanej rzeczy** (jak w CL). Nawet bez cyklu, jeśli ten sam
obiekt pojawia się dwa razy, dostaje `#1=`/`#1#`, zachowując na wyjściu informację, że „te dwa to
ten sam obiekt", jak określa CL:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Wartość bez współdzielenia **nie pokazuje żadnych etykiet**, więc pozostawienie tej zmiennej jako prawdy nie zmienia
wyjścia codziennego kodu.

### 6.2 Podstawa, wielkość liter i czytelność

| Zmienna | Typ | Domyślnie | Znaczenie |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Podstawa do wypisywania liczb całkowitych (o stałej szerokości i `int`). Poza zakresem od 2 do 36 jest to **błąd wypisywania** (CL też określa ten zakres) |
| `*print-radix*` | `bool` | `false` | Jeśli prawda, dodaje znacznik podstawy: `#b`/`#o`/`#x`, `#NNr` dla innych podstaw i końcową `.` dla podstawy 10. Znacznik stoi **przed** znakiem (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Wielkość liter nazw symboli: `:upcase` / `:downcase` / `:capitalize` (te same zapisy co w CL). Każdy inny symbol jest błędem wypisywania |
| `*print-readably*` | `bool` | `false` | Jeśli prawda, wypisuje w postaci możliwej do wczytania z powrotem. Wymusza cytowanie i wyłącza odcinanie z `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Liczba linii, których może użyć pretty printer. Nadmiar jest obcinany, z `..` na końcu jak w CL. 0 oznacza bez limitu. Wartość ujemna jest błędem wypisywania |
| `*print-escape*` | `bool` | `true` | Czy `write`/`write-to-string` robią `prin1`, czy `princ`. **Tylko te dwie go odczytują** |
| `*print-array*` | `bool` | `true` | Czy `Array<T>` pokazuje zawartość. Jeśli prawda, składnia tablic z CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); jeśli fałsz, tylko kształt, `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Znaczniki dodawane przez `*print-radix*` czytnik może wczytać z powrotem (notacja podstawy w
[Referencji składni](../syntax.md#1-elementy-leksykalne)).

**Dlaczego wartość domyślna `*print-case*` różni się od CL**: domyślne w CL jest `:upcase`, ponieważ czytnik CL
przechowuje nazwy symboli wielkimi literami, czyli oznacza „tak jak przechowano". Ten czytnik przechowuje je
małymi literami, więc wartością domyślną o tym samym znaczeniu jest `:downcase`.

**Brakująca połowa `*print-readably*`**: CL sygnalizuje `print-not-readable` dla wartości, których nie można
wczytać z powrotem, ale ten język nie ma kondycji do sygnalizowania ani sposobu rozstrzygania czytelności dla typów
użytkownika, które `print-object` może wypisywać w dowolny sposób. Jest tylko wymuszone cytowanie i nadpisanie
odcinania.

**Dlaczego tylko `write` czyta `*print-escape*`**: zgodnie z CLHS `~s`/`prin1`/`pprint` wiążą go z prawdą,
a `~a`/`princ` z fałszem, każde tylko na czas własnego wywołania. Zatem jedynymi czytelnikami, którzy widzą go
niezwiązanego, są `write`/`write-to-string`. Implementacja `print-object` powinna czytać własny
argument `escape`, a nie tę zmienną globalną: ten argument niesie wartość wybraną przez dyrektywę.

**Czego CL ma, a ten język nie**: `*print-gensym*` (nie ma nieinternowanych symboli).

### 6.3 Tymczasowe nadpisania

CL wiąże je za pomocą `let`, ale `let` w tym języku wiąże leksykalnie, więc użyj `dlet`
([Pozostałe](system.md#10-pozostałe)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; limity dotyczą tylko tego jednego wypisania
(with-standard-io-syntax (println "~a" x))   ; wypisz ze wszystkim przywróconym do wartości standardowych
```

`with-standard-io-syntax` wykonuje swoje ciało ze wszystkimi zmiennymi sterującymi drukarki ustawionymi na ich standardowe
wartości i `*read-eval*` ustawionym na `true`.
