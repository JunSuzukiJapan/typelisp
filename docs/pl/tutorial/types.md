<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Podstawy typów

typelisp jest językiem typowanym statycznie. Ten rozdział wyjaśnia, co dla ciebie robi sprawdzanie typów,
jakich typów będziesz używać najczęściej (`Option`, `Result`, struktury i enumeracje) oraz czym są typy
generyczne. Zakłada, że przeczytałeś [Pierwsze kroki](intro.md).

## 1. Co oznacza typowanie statyczne

W typelisp typ każdego wyrażenia jest ustalany, zanim program zostanie uruchomiony. Wyrażenie, którego
typy do siebie nie pasują, jest błędem, zanim cokolwiek się wykona.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; błąd typu

(main)
```

Uruchomienie tego pliku kończy się błędem typu, nawet bez wypisania `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Typy trzeba zapisywać dla argumentów funkcji i wartości zwracanych, zmiennych globalnych oraz pól
struktur. Typ zmiennej `let` jest brany z jej wartości początkowej.

Główne typy:

| Typ | Przykładowe wartości |
|---|---|
| `int` | `42`, `-7` (liczby całkowite o dowolnej precyzji) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Liczby całkowite o stałej szerokości |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Typ zwracany przez funkcję, która nie zwraca wartości |

Nie ma sposobu, by w czasie działania programu zapytać o typ wartości (nie ma `typep` ani `type-of` z
Common Lisp), ponieważ każdy typ jest znany, zanim program się uruchomi.

## 2. `Option<T>`: wartość, której może nie być

typelisp nie ma `nil`. „Może nie być wartości" wyraża się typem `Option<T>`. Wartość typu
`Option<T>` to albo `some`, zawierające jedną wartość typu `T`, albo `none`, które nie zawiera niczego.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` to nie `int`, więc nie można go tak po prostu użyć w arytmetyce. `(+ (safe-div 10 2) 1)` jest
błędem typu. Aby użyć tego, co jest w środku, rozdziel `some` od `none` za pomocą `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- W ramieniu `(some q)` zawartość jest wiązana ze zmienną `q`.
- `match` sprawdza, czy jego ramiona **pokrywają wszystkie przypadki**. Pominięcie ramienia `(none)` jest
  błędem typu.

### Dlaczego nie ma nil

W wielu językach `nil` (`null`) może zastępować wartość dowolnego typu. W rezultacie zapomnienie o
obsłużeniu przypadku „brak wartości" pozostaje niezauważone aż do uruchomienia programu. W typelisp
miejsce, w którym wartości może nie być, ma typ `Option<T>`, a kod nie przechodzi sprawdzania typów,
jeśli `match` nie obsługuje przypadku `none`. Zapomniany przypadek zostaje znaleziony, zanim program
się uruchomi.

Warunki opierają się na tej samej idei: tylko `bool` może być warunkiem `if`. Nie ma reguły w rodzaju
„wszystko poza `nil` jest prawdą" z Common Lisp.

### Typowe operacje

| Forma | Znaczenie |
|---|---|
| `(unwrap-or opt default)` | Zawartość dla `some`; wartość domyślna dla `none` |
| `(unwrap opt)` | Wyjmuje zawartość. Zatrzymuje program dla `none` |
| `(is-some opt)` / `(is-none opt)` | Sprawdza, który to przypadek |

Wiele funkcji biblioteki standardowej zwraca `Option`. Na przykład `position` zwraca pozycję w
`some`, jeśli element zostanie znaleziony, a `none`, jeśli nie.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: operacja, która może się nie powieść

Operacja, która może się nie powieść, zwraca `Result<T,E>`: `ok` z wartością typu `T` w razie sukcesu albo `err`
z błędem `E` w razie porażki.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Twoje własne funkcje także mogą zwracać `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Użyj `Option`, gdy brak wartości nie wymaga wyjaśnienia, a `Result`, gdy chcesz powiedzieć, dlaczego
coś się nie powiodło. [Obsługa błędów](errors.md) szczegółowo omawia postępowanie z błędami.

## 4. `defstruct`: struktury

Typ z nazwanymi polami definiuje się za pomocą `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

Definicja daje ci następujące możliwości:

```lisp
(let ((p (point::new 3 4)))     ; utworzenie (argumenty w kolejności pól)
  (println "~a" p::x)           ; odczyt pola; działa też (x p)
  (setf p::x 10)                ; zmiana
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Aby dać strukturze własne funkcje, użyj `defmethod`. Typ pierwszego argumentu (`self`)
decyduje, do którego typu należy metoda.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Zapisanie samej nazwy typu zamiast argumentu `self` tworzy funkcję wywoływaną jako
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: jeden z kilku kształtów

Wartość, która jest jednym z kilku kształtów, jak „koło, prostokąt lub punkt", definiuje się za pomocą
`defenum`. Każdy kształt nazywa się **wariantem**. Każdy wariant może przechowywać inną liczbę i inne typy
wartości.

```lisp
(defenum shape
  (circle int)        ; promień
  (rect int int)      ; szerokość i wysokość
  (dot))              ; nie przechowuje wartości
```

Wartości tworzy się z nazwą typu z przodu, jak w `shape::circle`. W `match` rozbiera się je
według nazwy wariantu.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Również tutaj `match` sprawdza, czy wszystkie przypadki są pokryte. Jeśli później dodasz wariant do
`shape`, każdy `match`, który go nie obsługuje, stanie się błędem typu, więc żadne miejsce wymagające
poprawki nie zostanie pominięte.

Po `(use shape)` możesz pisać `(rect 5 6)` bez nazwy typu.

`Option` i `Result` to enumeracje zbudowane za pomocą tego samego mechanizmu.

## 6. Typy generyczne

Funkcję działającą dla dowolnego typu definiuje się z **parametrem typu** `<T>` po jej nazwie.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Przy wywołaniu nie podajesz typu. `T` jest ustalane na podstawie argumentów.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T to int
(first-or names "none")    ; T to string
(first-or ints "none")     ; błąd typu: ints to Vector<int>, więc T to int
```

Struktury i enumeracje także mogą być generyczne. `Vector<T>`, `Option<T>` i `Result<T,E>` to typy tego
rodzaju.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Wewnątrz funkcji generycznej nic nie wiadomo o `T`, więc nie można porównywać ani dodawać wartości typu `T`.
Aby wymagać czegoś w rodzaju „dowolnego typu, który można porównywać", użyj traitów ([Traity](traits.md)).

## 7. Nadawanie typowi innej nazwy

`deftype` nadaje typowi inną nazwę.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` to tylko inny zapis `int`, a nie nowy typ. Przekazanie zwykłego `int` tam, gdzie oczekiwane jest
`meters`, nie jest błędem. Jeśli chcesz je rozróżniać, utwórz strukturę, jak w
`(defstruct meters (value int))`.

## 8. Co czytać dalej

- [Traity](traits.md): nadawanie typom wspólnych operacji
- [Typy](../reference/types.md): typy wbudowane i traity, które każdy z nich implementuje
