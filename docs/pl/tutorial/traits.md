<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Traity

Trait to obietnica, że „ten typ obsługuje te operacje". Traity pozwalają kilku typom dzielić operacje o
tej samej nazwie, dzięki czemu funkcja z nich korzystająca nie musi być pisana osobno dla każdego
typu. Działają niemal dokładnie tak jak traity w Rust. Ten rozdział zakłada, że przeczytałeś
[Podstawy typów](types.md).

## 1. Definiowanie i implementowanie traitu

Zdefiniuj operacje zwracające pole i nazwę figury jako trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` po nazwie traitu to lista traitów, które dziedziczy (sekcja 4). Zostaw ją pustą, gdy
  nie ma żadnych.
- Każda linia deklaruje metodę. `Self` oznacza „typ implementujący ten trait".

Aby zaimplementować trait dla typu, napisz `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Zaimplementowane metody wywołuje się tak samo jak zwykłe funkcje.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Pominięcie choćby jednej z metod zadeklarowanych przez trait jest błędem typu w `impl`.

## 2. Ograniczenia traitów: „dowolny typ implementujący ten trait"

Na parametr typu funkcji generycznej można nałożyć warunek za pomocą `where`. Nazywa się to
**ograniczeniem traitu**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Dzięki `(where (Shape T))` ciało funkcji może używać `name` i `area` na wartościach typu `T`. Bez
ograniczenia nic nie wiadomo o `T`, więc nie można by ich wywołać.

Przekazanie typu, który nie implementuje `Shape`, jest błędem typu w miejscu wywołania.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Funkcja generyczna dostaje własną kopię dla każdego typu, z którym jest wywoływana. Nie są
potrzebne żadne testy typów ani rozgałęzienia w czasie działania.

## 3. Implementacje domyślne

Jeśli metoda traitu ma ciało, jest ono używane, gdy `impl` pomija tę metodę.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe to wersja domyślna

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; wersja zapisana tutaj ma pierwszeństwo

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementowanie traitów standardowych

Biblioteka standardowa także ma traity. Zaimplementowanie jednego z nich udostępnia dla twojego typu
funkcje standardowe, które z niego korzystają.

| Trait | Metody do zaimplementowania | Co umożliwia |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, wzorzec `(= expr)` w `match` i tak dalej |
| `Ord` | `less` | `less-equal`, `greater` i tak dalej. `Ord` dziedziczy z `Eq` |
| `print-object` | `print-object` | Sposób pokazywania wartości przez `println` i pokrewne |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` i tak dalej |
| `Error` | `message`, `source` | Użycie jako typ błędu ([Obsługa błędów](errors.md)) |

Zaimplementujmy `Eq` i `Ord` dla typu reprezentującego kwotę pieniędzy. Ponieważ `Ord` dziedziczy
z `Eq`, `impl` dla `Eq` musi być pierwszy.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (domyślna implementacja w Ord)
```

Zaimplementowanie `print-object` decyduje o tym, jak `println` pokazuje wartość. Argument `escape` ma wartość `true`,
gdy żądana jest postać, którą można wczytać z powrotem, jak przy `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

W połączeniu z ograniczeniem traitu możesz napisać funkcję działającą dla dowolnego typu implementującego `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Dla `Vector` z wartościami `money` 300, 900 i 100 (w tej kolejności) zwraca `(some 900 yen)`.

## 5. `:dyn`: wspólna obsługa wartości różnych typów

Wszystkie elementy `Vector<T>` mają ten sam typ, więc wartości `circle` i `rect` nie mogą trafić do jednego
`Vector<circle>`. Aby wspólnie obsługiwać „coś, co implementuje `Shape`", użyj typu
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Wartość `circle` lub `rect` umieszczona tam, gdzie oczekiwane jest `:dyn Shape`, jest konwertowana automatycznie.
- To, które `area` (z którego typu) uruchomi wywołanie `(area s)`, jest rozstrzygane w czasie działania na podstawie typu tego, co przechowuje `s`.
- Umieszczenie wartości, której typ nie implementuje `Shape`, tam, gdzie oczekiwane jest `:dyn Shape`, jest błędem
  typu.

Wybór między ograniczeniami traitów z sekcji 2 a `:dyn`:

| | Ograniczenie traitu (`where`) | `:dyn Trait` |
|---|---|---|
| Kiedy rozstrzygana jest wywoływana metoda | Przed uruchomieniem | W czasie działania |
| Mieszanie typów w jednym `Vector` | Niemożliwe | Możliwe |
| Typy, których można użyć | Bez ograniczeń | Struktury, enumeracje, `int`, `string`, `f64` i inne (nie `bool`, `char`, `symbol`, `i32` i podobne) |

Dokładna lista typów, których można użyć, znajduje się w
[Referencji składni 3.9](../reference/syntax.md#39-deftrait--impl--traity).

Niektórych traitów nie można używać z `:dyn`: tych, których metody używają `Self` dla argumentu innego niż
`self` lub dla wartości zwracanej (jak `equals` w `Eq`). Ponieważ typ nie jest znany aż do czasu działania,
nie ma sposobu na wytworzenie „wartości tego samego typu".

## 6. Ograniczenia

- Definicję traitu, jego `impl` oraz kod używający go przez `:dyn` trzymaj w jednym
  module (pliku). Trait nie może jeszcze zostać udostępniony innym modułom.
- Typy i traity dzielą jedną przestrzeń nazw. W obrębie jednego modułu typ i trait nie mogą mieć tej samej
  nazwy.

## 7. Co czytać dalej

- [Makra](macros.md): definiowanie własnej składni
- [Referencja składni 3.9](../reference/syntax.md#39-deftrait--impl--traity): implementacje blankietowe (blanket),
  typy powiązane i więcej
- [Traity standardowe](../reference/functions/traits.md): lista traitów w bibliotece standardowej
