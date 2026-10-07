<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Traity standardowe

Traity do iteracji, porównywania i arytmetyki. Pozostałe traity standardowe znajdują się we własnych
rozdziałach: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Typy błędów](option-result.md#3-typy-błędów-i-trait-error)), `print-object`
([Wypisywanie](printing.md#5-print-object-reprezentacja-wypisywana-według-typu)) oraz traity strumieni i
`Pathish` ([Strumienie i pliki](streams-files.md)). To, które typy implementują które traity, znajduje się w
[Typach](../types.md). Sposób definiowania traitów opisano w
[Referencji składni](../syntax.md#39-deftrait--impl--traity).

## 1. Trait `Iter` i iteracja

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementują `Iter` odpowiednio przez `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (iterator uzyskuje się za pomocą `(iter collection)`).
`Chan<T>` sam jest `Iter` (`recv` pełni rolę `next`; [Kanały](concurrency.md#2-chant--kanały)).
Listy `Sexpr` nie implementują `Iter` (typy ich elementów nie są jednolite). Jeśli zaimplementujesz `Iter`
dla własnego typu, można go przejść za pomocą `doiter` bez zmian i przekazać do
[funkcji na sekwencjach](sequences.md#4-funkcje-na-sekwencjach-oparte-na-iter).

## 2. `Eq` / `Ord` (porównywanie)

Odpowiadają one `PartialEq`/`PartialOrd` z Rust (nazwane `Eq`/`Ord`). Używa się ich w ograniczeniach `where`
funkcji generycznych, aby wymagać, by typy elementów dało się porównywać (`sort`/`member`/`assoc`
i tak dalej).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; trzeba zaimplementować
  (not-equals ((self Self) (other Self)) bool             ; implementacja domyślna
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; dziedziczy z Eq
  (less ((self Self) (other Self)) bool)                  ; trzeba zaimplementować
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Aby zaimplementować `Eq`, piszesz tylko `equals`, a dla `Ord` tylko `less`. Implementacje domyślne
uzupełniają resztę. `Ord` dziedziczy z `Eq`, więc `impl Eq X` jest potrzebne przed `impl Ord X`.

Każdą metodę traitu można wywołać jako funkcję bez zmian (wewnątrz ograniczenia `where (Eq A)`/`(Ord A)` lub na
konkretnym typie, który ją implementuje):

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Czy są równe (`==` z Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Czy nie są równe (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` jest zaimplementowane dla: wszystkich typów liczbowych (od `i8` do `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, czyli tożsamość; używane przez wzorce wartości w
`match`) oraz `cons-cell<A,B>` (rekurencyjnie, gdy elementy są `Eq`). `Ord` jest zaimplementowane dla:
wszystkich typów liczbowych, `char` `string` oraz `cons-cell<A,B>` (leksykograficznie, gdy elementy są
`Ord`).

Nazwy metod nie nakładają się na wbudowane operatory (`= /= < <= > >=`) ani na `eq`/`lt`, ponieważ
wbudowanych nie można redefiniować, a każda implementacja deleguje do nich. Same skalarne operatory
porównania to wbudowane metody każdego typu odbiorcy ([Liczby](numbers.md),
[Łańcuchy znaków i znaki](collections.md)). Wewnątrz ograniczenia zapisanie operatorów odczytuje je jako
metody traitu (rozdział 3).

## 3. Traity arytmetyczne (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Warstwa pozwalająca kodowi generycznemu wymagać „typu, który można dodawać". **Arytmetyka na konkretnych typach używa
wbudowanych operatorów** ([Liczby](numbers.md)) i nie przechodzi przez tę warstwę.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; odległość jest zawsze int (jak w ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; bez metod; połączenie sześciu
```

**Wewnątrz ograniczenia można pisać operatory.** Gdy odbiorcą jest zmienna typowa związana przez `where`,
operatory są odczytywane jako metody traitu (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Metoda traitu nie nazywa się `+`, ponieważ `+` to nazwa metody wbudowanej, a `impl` odmawia jej
redefinicji (`cannot redefine built-in method`). Nie ma `Neg`: `(- x)` rozwija się do
`(- (- x x) x)`, więc wystarczy `Sub`.

Zaimplementowane dla: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` na wszystkich typach liczbowych (poza `complex`) oraz
`Bits` na wszystkich typach całkowitych i `int`.
