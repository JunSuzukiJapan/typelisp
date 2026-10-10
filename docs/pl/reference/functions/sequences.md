<!-- translated-from: docs/ja/reference/functions/sequences.md @ eae672c1a271f0b6947f024e81dee8338b2f5ff4 -->
# Pary, S-wyrażenia i sekwencje

Generyczna para `cons-cell`, dane w postaci S-wyrażeń `Sexpr`, symbole, funkcje na sekwencjach napisane
na bazie `Iter` oraz funkcje wyższego rzędu.

## 1. Pary `cons-cell<A,B>`

`cons`/`car`/`cdr` to konstruktor i akcesory pól **generycznego typu pary
`cons-cell<A,B>`** (`defstruct` w bibliotece standardowej). Pola można odczytywać jako
`zmienna::car`/`zmienna::cdr` (składnia akcesorów `defstruct` z
[Referencji składni](../syntax.md#36-defstruct--struktury-typy-definiowane-przez-użytkownika)) lub jako
`(car zmienna)`/`(cdr zmienna)`. Aby je zmienić, użyj `(setf zmienna::car v)`/`(setf zmienna::cdr v)`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Tworzy parę |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Pierwszy element |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Reszta |

`cons-cell` pełni także rolę składni krotek. Funkcje CL zwracające wiele wartości (iloraz i reszta z `floor`,
wartość i pozycja z `read-from-string` i tak dalej) zwracają w tym języku `cons-cell`.

## 2. Dane w postaci S-wyrażeń `Sexpr`

Typ danych `Sexpr` zwracany przez `read` ma 19 wariantów:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` i `array` to dane zapisane jako `#(..)` i `#nA(..)` (zob. [referencję
składni](../syntax.md#1-elementy-leksykalne)), zawierające odpowiednio `Vector<Option<Sexpr>>` i
`Array<Option<Sexpr>>`: `len`, `get` i pozostałe działają bezpośrednio na `v` związanym przez
`(vector v)`.
`tuple` to dane zapisane przez `#{..}`, a `v` wiązane przez `(tuple v)` to nowy
`Vector<Option<Sexpr>>` elementów (by krotkę dowolnej długości odbierać jednym typem).
Komórkami S-wyrażeń nie zajmują się ogólne `cons`/`car`/`cdr` z rozdziału 1, lecz
funkcje `sexpr-*`. Używa się ich głównie w ciałach `defmacro` do budowania i rozbierania form.

**Typem danych w postaci S-wyrażeń jest `Option<Sexpr>`.** Pusta lista nie jest wariantem `Sexpr`, lecz
`none` z `Option`, a samo `Sexpr` oznacza „niepuste S-wyrażenie". Dlatego funkcje `sexpr-*`
przyjmują i zwracają `Option<Sexpr>`.

- `()` jest pustą listą tam, gdzie oczekiwany jest `Option<Sexpr>` (można też napisać
  `(Option::none)`)
- `Sexpr` jest niejawnie rozszerzane tam, gdzie oczekiwany jest `Option<Sexpr>` (bez konwersji w czasie działania).
  Przeciwny kierunek, użycie `Option<Sexpr>` jako `Sexpr`, twierdzi, że „to nie jest pusta lista",
  więc trzeba to jawnie stwierdzić za pomocą `match` lub `unwrap`
- W `match` 19 wariantów `Sexpr` i `none` można zapisać **płasko w tej samej liście ramion**
  ([Referencja składni](../syntax.md#43-match--dopasowywanie-wzorców))

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Tworzy komórkę `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Pierwszy element. **Pusta lista dla pustej listy** (jak w CL). Powoduje panic dla atomu, który nie jest `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Reszta. **Pusta lista dla pustej listy** (jak w CL). Powoduje panic dla atomu, który nie jest `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Czy jest to `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Czy jest to pusta lista |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Czy nie jest to `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Czy jest to `Sym` (symbol) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Zawartość wariantu `int` (fixnum lub bignum). Powoduje panic dla innego typu |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Zawartość wariantu o tej szerokości. Powoduje panic dla innego typu |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Zawartość wariantów zmiennoprzecinkowych. Powoduje panic dla innego typu |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Zawartość `Char`. Powoduje panic dla innego typu |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Zawartość `Bool`. Powoduje panic dla innego typu |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Zawartość `Str`. Powoduje panic dla innego typu |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Nazwa `Sym`. Powoduje panic dla innego typu |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Porównanie tożsamości (`Cons`/`Str` porównują tożsamość obiektu, reszta porównuje wartości) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Równość strukturalna (`Cons` rekurencyjnie, `Str` według zawartości) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Jak `equal`, plus porównanie bez rozróżniania wielkości liter i porównanie liczb różnych typów |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Łączy dwie listy `Sexpr` (niedestrukcyjnie). `,@` rozwija się do tej funkcji |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Nowa lista `Sexpr` z `f` zastosowanym do każdego elementu listy `Sexpr` (`map` z rozdziału 4 dotyczy `Iter` i nie potrafi przejść po liście `Sexpr`) |

Istnieje dziewięć akcesorów liczbowych, po jednym na typ, ponieważ `Sexpr` to „jedyne miejsce, w którym typ
wartości nie jest zapisany nigdzie indziej". `u8` umieszczone w `Sexpr` wchodzi jako wariant `u8` i wychodzi
tylko za pomocą `(sexpr-u8 s)`. Przekazanie go do `(sexpr-int s)` powoduje panic; nigdy po cichu nie rozszerza
odpowiedzi. Liczby całkowite w wczytanych danych (`'(1 2 3)`, argumenty makr) są wariantu `int` i
odczytuje się je za pomocą `(sexpr-int s)`.

Listy `Sexpr` nie mają operacji destrukcyjnych w rodzaju `rplaca`/`nconc`. Komórki `Sexpr` nie można zmienić
po jej utworzeniu.

## 3. Symbole

`symbol` to typ samych symboli. Konwertuje się niejawnie tam, gdzie wymagane jest `Sexpr`, ale
nie automatycznie w drugą stronę.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Wyjmuje nazwę symbolu |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Tworzy symbol z łańcucha znaków (internuje go) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Czy jest słowem kluczowym (`:name`). Dwukropek jest częścią nazwy, więc test patrzy na pierwszy znak ([Referencja składni](../syntax.md#1-elementy-leksykalne)) |

Informacje o `gensym` znajdziesz w [Makrach](system.md#8-makra).

## 4. Funkcje na sekwencjach oparte na `Iter`

Funkcje na sekwencjach to **funkcje generyczne nad traitem `Iter`**. Z kolekcji uzyskaj
iterator za pomocą `(iter coll)` i przekaż go (`Vector<T>` / `HashTable<K,V>` / `Array<T>` to obsługują;
lista `Sexpr` nie implementuje `Iter`, więc te funkcje jej nie dotyczą). **Wynikowa
kolekcja jest zwracana jako nowy `Vector`.** `Iter<A>` w tabelach oznacza „dowolną implementację
`Iter`, której `Item` to `A`". Aby ponownie przejść po zwróconym `Vector`, przekaż `(iter result)`.

Funkcje przyjmujące predykat (odpowiadające rodzinie `-if` z CL):

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Odwzorowanie |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Tylko elementy spełniające predykat |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Usuwa elementy spełniające predykat |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Pierwszy element spełniający predykat |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Pierwsza pozycja spełniająca predykat |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Ile spełnia predykat |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Czy każdy element spełnia predykat |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Czy jakikolwiek element spełnia predykat (odpowiada `some` z CL; nazwa, która nie koliduje z konstruktorem `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Lewostronne złożenie |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Prawostronne złożenie |
| `collect` | `(collect it)` | `Iter<A>→Vector<A>` | Zbiera wszystkie pozostałe elementy. Służy do zamiany wyniku funkcji `lazy` poniżej na `Vector` |

Indeksowanie, długość i wycinanie:

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Liczba elementów |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Łączy iteratory. Można podać trzy lub więcej |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` z CL. Typ wyniku zapisuje się jako **cytowany literał symbolu** (CL używa specyfikatora typu z czasu działania). `'vector` przyjmuje jeden lub więcej, `'string` zero lub więcej (`""` dla zera). Listy `Sexpr` nie są objęte (użyj `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Odwrócenie (niedestrukcyjne) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Element `n` (`None` poza zakresem) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` z argumentami w odwrotnej kolejności |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Pierwsze `n` elementów |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` jest ograniczane do długości) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Ostatni **element** (a nie „ostatnia komórka" jak w CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Wszystko poza ostatnim elementem |

Funkcje wymagające ograniczenia `Eq` / `Ord` (porównują za pomocą traitu zamiast predykatu;
[Traity standardowe](traits.md#2-eq--ord-porównywanie)):

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Czy istnieje element równy `x` (w przeciwieństwie do CL, `bool`, a nie reszta listy) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Pierwszy element równy `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Pierwsza pozycja równa `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Ile elementów jest równych `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` z CL. Stabilne, niedestrukcyjne sortowanie. `cmp` to `true`, gdy „pierwszy argument znajduje się ściśle przed drugim" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Pierwsza para, której `car` jest równe `k`. Wartość wyjmuje się za pomocą `(cdr p)` |

Te funkcje i wiele funkcji z rozdziału 5 przyjmują także argumenty kluczowe z CL: `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (rozdział 6).

### Leniwe iteratory (moduł `lazy`)

Funkcje modułu `lazy` nie budują `Vector`, **zwracają iterator**. Element jest obliczany dopiero
wtedy, gdy zażąda się następnego, więc nawet iterator bez końca (`iterate`, `repeat`) da się użyć, o
ile dalej zatrzyma go `take` lub `take-while`. Każdy wynik implementuje `Iter`, więc funkcje `lazy`
można zagnieżdżać, a funkcje z tabel powyżej przyjmują je bez zmian. `collect` zamienia go na
`Vector`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `lazy::map` | `(lazy::map it f)` | `(Iter<A>,(fn (A) U))→Iter<U>` | Stosuje `f` do każdego elementu |
| `lazy::filter` | `(lazy::filter it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Tylko elementy spełniające warunek |
| `lazy::take` | `(lazy::take it n)` | `(Iter<A>,int)→Iter<A>` | Pierwsze `n` |
| `lazy::take-while` | `(lazy::take-while it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Aż do elementu tuż przed pierwszym, który nie spełnia warunku |
| `lazy::skip` | `(lazy::skip it n)` | `(Iter<A>,int)→Iter<A>` | Pomija pierwsze `n` |
| `lazy::enumerate` | `(lazy::enumerate it)` | `Iter<A>→Iter<#{int A}>` | Pary pozycji liczonej od 0 i elementu |
| `lazy::zip` | `(lazy::zip a b)` | `(Iter<A>,Iter<B>)→Iter<#{A B}>` | Pary złożone z elementu z każdej strony. Kończy się wraz z krótszą |
| `lazy::chain` | `(lazy::chain a b)` | `(Iter<A>,Iter<A>)→Iter<A>` | Elementy `a`, a potem `b` |
| `lazy::flat-map` | `(lazy::flat-map it f)` | `(Iter<A>,(fn (A) Iter<B>))→Iter<B>` | Zamienia każdy element w iterator za pomocą `f` i łączy je po kolei |
| `lazy::iterate` | `(lazy::iterate x f)` | `(A,(fn (A) A))→Iter<A>` | `x`, `(f x)`, `(f (f x))`, ... bez końca |
| `lazy::repeat` | `(lazy::repeat x)` | `A→Iter<A>` | Powtarza `x` bez końca |

`Iter<U>` i podobne w tabeli to w rzeczywistości typy struktur nazwane jak funkcja z dopisanym
`-iter` (dla `lazy::map` jest to `lazy::map-iter<I,A,U>`, gdzie `I` to typ iteratora źródłowego).
Typ pisze się tylko tam, gdzie nic innego go nie ustala, np. jako typ zwracany lambdy przekazywanej
do `lazy::flat-map`.

```lisp
(collect (lazy::take (lazy::filter (lazy::iterate 1 (lambda ((n int)) int (+ n 1)))
                                   (lambda ((n int)) bool (= 0 (mod n 3))))
                     4))                                  ; => #(3 6 9 12)

(doiter (#{i s} (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
  (println "~a: ~a" i s))                                 ; 0: a i 1: b

(-> (lazy::iterate 1 (lambda ((n int)) int (* n 2)))
    (lazy::take-while (lambda ((n int)) bool (< n 100)))
    collect)                                              ; => #(1 2 4 8 16 32 64)
```

`->` to makro, które przekazuje wartość kolejno jako pierwszy argument każdej następnej formy
([Option i Result](option-result.md)).

## 5. Pozostałe funkcje na sekwencjach z CL

Wszystkie są funkcjami generycznymi na `Iter`, jak w rozdziale 4. Wynikowe kolekcje są zwracane jako nowe
`Vector`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Nazwane indeksy z CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Wszystko poza pierwszym (nowy `Vector`, a nie współdzielony ogon) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materializuje iterator w `Vector` (`copy-seq`/`copy-list` z CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` odwrócone, a po nim `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopii `x` (`make-list`/`make-sequence` z CL). Tak jak w `Vector::new`, argument typu pochodzi z oczekiwanego typu, więc w samym `let` potrzebne jest `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Jak `member`, **`bool`** (iterator nie ma ogona do zwrócenia) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Negacje `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Te same typy co wersje pozytywne | Wersje z zanegowanym predykatem |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Usuwa według wartości |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Usuwa duplikaty. Jak w CL, **zachowywane jest ostatnie wystąpienie** (`:from-end true` zachowuje pierwsze) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Zastępuje według wartości / predykatu |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | na `Iter<cons-cell<K,V>>` | Wersje `assoc` z predykatem i po stronie wartości |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Dodaje parę z przodu |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Łączy dwie sekwencje w pary. Zatrzymuje się na krótszej |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` z CL na kilku sekwencjach. Zatrzymuje się na krótszej |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Odwzorowanie dla efektów ubocznych |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Odwzorowuje i łączy |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Odwzorowuje po kolejnych **ogonach** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Odwzorowuje po ogonach dla efektów ubocznych (odpowiednik `maplist` dla `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Odwzorowuje po ogonach i łączy (odpowiednik `maplist` dla `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Pozycja, w której `sub` pojawia się po raz pierwszy. Jeśli odbiorcą jest `string`, wybierana jest metoda `string` ([Łańcuchy znaków](collections.md#1-łańcuchy-znaków-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Pierwsza pozycja, w której się różnią. `none`, jeśli są równe |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Scalanie. CL wymaga posortowanych wejść; ta funkcja sortuje konkatenację |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Dodaje `x` **z przodu**, jeśli go nie ma |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Operacje na zbiorach. CL nie określa kolejności; tutaj jest stabilna, **w kolejności pierwszego wystąpienia** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Zawieranie |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Czy jest sufiksem / część przed sufiksem. CL pyta o **współdzieloną strukturę**, ale nie ma struktury do współdzielenia, więc ta funkcja pyta o sufiks **jako wartości** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Równość element po elemencie. Sam `Vector<T>` nie implementuje `Eq` |
| `caar`…`cddddr` | `(cadr p)` | na zagnieżdżonych parach | 28 funkcji z CL. Przechodzą po **parach, a nie listach**: `cadr` przyjmuje `cons-cell<A,cons-cell<B,C>>` |

Czego CL ma, a ten język nie: `list*` (nie ma pojęcia listy niewłaściwej z zastąpionym ogonem),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (żaden typ nie może opisać przechodzenia po
heterogenicznym drzewie dowolnej głębokości; dla drzewa `Sexpr` odpowiednikiem `tree-equal` jest `equal`),
rodzina list własności `getf`/`get-properties`/`symbol-plist`/`remprop` (nie ma
reprezentacji jako nietypowanej listy naprzemiennie kluczy i wartości; tę samą rolę pełnią `assoc` (listy asocjacyjne) lub
`HashTable`) oraz funkcje konwertujące między `Vector<T>` a listami `Sexpr` (elementy
listy `Sexpr` mogą mieć różne typy, więc nie da się ich zapisać jednym
typem elementu `T`).

## 6. Argumenty kluczowe

Funkcje z rozdziałów 4 i 5 przyjmują słowa kluczowe sekwencji z CL: `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Wszystkie są **opcjonalne**.

| Słowo kluczowe | Typ | Znaczenie |
|---|---|---|
| `:key` | `(fn (A) A)` | Projekcja stosowana do każdego elementu przed porównaniem lub testem |
| `:test` | `(fn (A A) bool)` | Test równości używany zamiast `equals` z ograniczenia `Eq`. Pierwszym argumentem jest **poszukiwany element**, drugim element (po `:key`), w tej samej kolejności co w CL |
| `:test-not` | `(fn (A A) bool)` | Negacja `:test` |
| `:start` `:end` | `int` | Okno `[start, end)` do przeszukania. Indeksy są względne wobec całej sekwencji |
| `:from-end` | `bool` | Wyszukiwanie odpowiada **ostatnim** dopasowaniem. W połączeniu z `:count` elementy, których dotyczy działanie, są brane od końca |
| `:count` | `int` | Maksymalna liczba elementów, na które działają rodziny `remove` / `substitute` |

To, która funkcja przyjmuje które, jest zgodne z CL:

| Funkcja | Przyjmowane słowa kluczowe |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Wszystkie powyższe (w tym `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` w `assoc` dotyczy `car`, a w `rassoc` `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; usuwa tylko jeden, od końca
(position 3 (iter v) :start 1)                          ; indeks jest względny wobec całej sekwencji
```

**Różnice względem CL**:

1. **Projekcja `:key` pozostaje w obrębie typu elementu** (`(fn (A) A)`). Nie może rzutować na
   inny typ jak w CL: dodatkowej zmiennej typowej nie dałoby się ustalić, gdy argument
   jest pominięty. Tam, gdzie potrzebna jest projekcja na inny typ, przekaż lambdę do rodziny `-if`
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **W wyszukiwaniach opartych na elemencie `:key` dotyczy tylko elementów sekwencji** (a nie poszukiwanego elementu).
   Jest to ta sama reguła co w `find`/`position`/`count`/`member`/`remove`/`substitute` z CL. W
   operacjach na zbiorach obie strony są elementami, więc dotyczy obu.
3. **Tylko słowa kluczowe `search` są nazwane, a nie numerowane.** W CL `:start1`/`:end1` dotyczą
   **wzorca**, a `:start2`/`:end2` przeszukiwanej sekwencji. W tym języku odbiorca
   jest pierwszy, więc te same numery znaczyłyby coś przeciwnego, i to po cichu. `:start`/`:end`
   dotyczą odbiorcy, a `:sub-start`/`:sub-end` wzorca, więc roztargnione `:start1`
   daje błąd „unknown keyword". `mismatch` i `replace` mają tę samą kolejność argumentów co CL, więc
   zachowują numery z CL.

## 7. Operacje destrukcyjne

Metody `Vector<T>`. **Modyfikują odbiorcę i zwracają samego odbiorcę**, więc `(nreverse v)`
zapisuje się tak samo jak `reverse`, a samo `v` także zostaje odwrócone.

| Nazwa | Forma | Opis |
|---|---|---|
| `nreverse` | `(nreverse v)` | Odwraca w miejscu |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Wersje w miejscu dla `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Wersje w miejscu dla rodziny `substitute` |
| `nbutlast` | `(nbutlast v)` | Odrzuca ostatni element |
| `fill` | `(fill v x)` | Ustawia każdy element na `x`. Długość się nie zmienia |
| `replace` | `(replace v src)` | Nadpisuje od przodu elementami `src`. `(min (len v) (len src))` elementów |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Ta sama liczba co wyżej |
| `nconc` | `(nconc v w)` | Dopisuje elementy `w` do `v`. W przeciwieństwie do CL **nie przepisuje współdzielonej struktury** (`w` nie jest dotknięte) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Zastępuje zawartość `v` zawartością `src` (długość też się zmienia) |
| `rplaca` `rplacd` | `(rplaca p x)` | Przepisuje `car`/`cdr` w `cons-cell` i zwraca samą komórkę |

Przyjmowane słowa kluczowe:

| Wersja destrukcyjna | Przyjmowane słowa kluczowe |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (odbiorca to `sequence-1` z CL) |

`vector-push-extend`/`vector-pop` to po prostu `push`/`pop` z `Vector<T>`. `Vector<T>` zawsze rośnie,
więc nic nie odpowiada rozróżnieniu CL na „wektor ze wskaźnikiem wypełnienia" i „prosty
wektor".

## 8. Funkcje wyższego rzędu

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Zwraca swój argument |
| `const` | `(const x y)` | `(A,B)→A` | Zwraca pierwszy argument |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Składanie funkcji `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Zamienia argumenty funkcji dwuargumentowej |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negacja predykatu |

Nie ma `constantly` z CL (typ ignorowanego argumentu pojawiałby się tylko w typie zwracanym
i nie dałoby się go ustalić). Napisz `(lambda ((x T)) A v)`.
