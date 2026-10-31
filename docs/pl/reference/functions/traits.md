<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Traity standardowe

Traity do iteracji, porównywania, arytmetyki oraz dodawania i wyjmowania elementów. Pozostałe traity standardowe znajdują się we własnych
rozdziałach: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Typy błędów](option-result.md#3-typy-błędów-i-trait-error)), `print-object`
([Wypisywanie](printing.md#5-print-object-reprezentacja-wypisywana-według-typu)) oraz traity strumieni i
`Pathish` ([Strumienie i pliki](streams-files.md)). To, które typy implementują które traity, znajduje się w
[Typach](../types.md). Sposób definiowania traitów opisano w
[Referencji składni](../syntax.md#39-deftrait--impl--traity).

## 1. Trait `Iter` i iteracja

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementują `Iter` odpowiednio przez `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (iterator uzyskuje się za pomocą `(iter collection)`).
`Chan<T>` sam jest `Iter` (`recv` pełni rolę `next`; [Kanały](concurrency.md#2-chant--kanały)).
Listy `Sexpr` nie implementują `Iter` (typy ich elementów nie są jednolite). Jeśli zaimplementujesz `Iter`
dla własnego typu, można go przejść za pomocą `doiter` bez zmian i przekazać do
[funkcji na sekwencjach](sequences.md#4-funkcje-na-sekwencjach-oparte-na-iter).

## 2. `Eq` / `Ord` (porównywanie)

Odpowiadają one `PartialEq`/`PartialOrd` z Rust (nazwane `Eq`/`Ord`). Używa się ich w ograniczeniach `:where`
funkcji generycznych, aby wymagać, by typy elementów dało się porównywać (`sort`/`member`/`assoc`
i tak dalej).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; trzeba zaimplementować
  (not-equals (self (other Self)) bool             ; implementacja domyślna
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; dziedziczy z Eq
  (less (self (other Self)) bool)                  ; trzeba zaimplementować
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
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
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; odległość jest zawsze int (jak w ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; bez metod; połączenie pięciu
```

**Wewnątrz ograniczenia można pisać operatory.** Gdy odbiorcą jest zmienna typowa związana przez `:where`,
operatory są odczytywane jako metody traitu (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Metoda traitu nie nazywa się `+`, ponieważ `+` to nazwa metody wbudowanej, a `impl` odmawia jej
redefinicji (`cannot redefine built-in method`). Nie ma `Neg`: `(- x)` rozwija się do
`(- (- x x) x)`, więc wystarczy `Sub`.

Zaimplementowane dla: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` na wszystkich typach liczbowych (poza `complex`) oraz
`Bits` na wszystkich typach całkowitych i `int`.


**`Div` nazywa swój typ wyniku typem powiązanym `Output`.** Jak w CL, `/` na dwóch liczbach
całkowitych daje `rational`, więc `Output` dla `int` to `rational`, a dla każdego innego typu
liczbowego sam ten typ. Ponieważ `Output` różni się między typami, `Number` nie zawiera `Div` (lista
nadcech musi ustalić każdy typ powiązany). Funkcja generyczna, która dzieli, przyjmuje typ wyniku przez
`(:where (Div T (Output R)))` albo wymaga dzielenia zamkniętego w `T` przez `(:where (Div T (Output T)))`
(wtedy nie można przekazać `int`).

## 4. Trait `Push` (dodawanie elementów)

```lisp
(deftrait Push ()
  (:type Item)
  (push (self (x Item)) ()))
```

Kolekcja, do której można dodawać elementy po jednym. Funkcja generyczna żąda „typu, do którego
można dodać `T`” za pomocą `(:where (Push C (Item T)))` i dodaje do każdej takiej kolekcji tym samym
`(push c x)`.

| Typ | Co robi `push` |
|---|---|
| `Vector<T>` | Dodaje na końcu |
| `Deque<T>` | Dodaje na końcu (`push-back`) |
| `Array<T>` | Dodaje we wskaźniku wypełnienia (`push-extend`). Powoduje panic bez wskaźnika wypełnienia |
| `HashSet<T>` | Wstawia do zbioru (`insert`). Jest `Push` tylko wtedy, gdy typ elementów implementuje `Hash` |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Własny typ, który implementuje `Push`, można przekazać do tych samych funkcji generycznych. Może też
być obiektem traitu, jak w `:dyn Push<int>`.

## 5. Trait `Pop` (wyjmowanie elementów)

```lisp
(deftrait Pop ()
  (:type Item)
  (pop (self) Option<Item>))
```

Kolekcja, z której można z powrotem wyjąć element dodany ostatnio przez `push`. `none`, gdy jest
pusta. Funkcja generyczna wymaga jej przez `(:where (Pop C (Item T)))`.

| Typ | Co robi `pop` |
|---|---|
| `Vector<T>` | Wyjmuje ostatni element |
| `Deque<T>` | Wyjmuje ostatni element (`pop-back`) |
| `Array<T>` | Wyjmuje element tuż przed wskaźnikiem wypełnienia. Panic bez wskaźnika wypełnienia |

`HashSet<T>` nie ma „ostatniego elementu”, więc jest `Push`, ale nie `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Tak jak `Push`, może go implementować własny typ i może być obiektem traitu, jak w `:dyn Pop<int>`.
