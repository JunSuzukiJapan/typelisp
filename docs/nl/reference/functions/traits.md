<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Standaardtraits

De traits voor iteratie, vergelijking en rekenkunde. De overige standaardtraits staan in hun eigen
hoofdstukken: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Foutentypes](option-result.md#3-foutentypes-en-de-trait-error)), `print-object`
([Afdrukken](printing.md#5-print-object-afgedrukte-weergave-per-type)), en de streamtraits en
`Pathish` ([Streams en bestanden](streams-files.md)). Welke types welke implementeren staat in
[Types](../types.md). Hoe je traits definieert staat in de
[Syntaxreferentie](../syntax.md#39-deftrait--impl--traits).

## 1. De trait `Iter` en iteratie

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementeren `Iter` via respectievelijk
`vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` (haal de iterator op met
`(iter collection)`). `Chan<T>` is zelf een `Iter` (`recv` speelt de rol van `next`;
[Kanalen](concurrency.md#2-chant--kanalen)). `Sexpr`-lijsten implementeren `Iter` niet (hun
elementtypes zijn niet uniform). Als je `Iter` voor je eigen type implementeert, kan het zoals het is
met `doiter` worden doorlopen en aan de [sequentiefuncties](sequences.md#4-sequentiefuncties-op-iter)
worden doorgegeven.

## 2. `Eq` / `Ord` (vergelijking)

Deze komen overeen met `PartialEq`/`PartialOrd` van Rust (hier `Eq`/`Ord` genoemd). Ze worden
gebruikt in de `where`-bounds van generieke functies om te eisen dat elementtypes kunnen worden
vergeleken (`sort`/`member`/`assoc` enzovoort).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Om `Eq` te implementeren schrijf je alleen `equals`, en voor `Ord` alleen `less`. De
standaardimplementaties vullen de rest in. `Ord` erft van `Eq`, dus `impl Eq X` is nodig vóór
`impl Ord X`.

Elke traitmethode kan zoals ze is als functie worden aangeroepen (binnen een
`where (Eq A)`/`(Ord A)`-bound, of op een concreet type dat ze implementeert):

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Of ze gelijk zijn (`==` van Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Of ze niet gelijk zijn (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` is geïmplementeerd voor: alle numerieke types (`i8` tot en met `u32` / `f32` / `f64` / `int` /
`ratio`), `bool` `char` `string` `symbol` `complex`, `Sexpr` (`eq`, dat wil zeggen identiteit; gebruikt
door de waardepatronen van `match`), en `cons-cell<A,B>` (recursief, wanneer de elementen `Eq` zijn).
`Ord` is geïmplementeerd voor: alle numerieke types, `char` `string`, en `cons-cell<A,B>`
(lexicografisch, wanneer de elementen `Ord` zijn).

De methodenamen overlappen niet met de ingebouwde operatoren (`= /= < <= > >=`) of `eq`/`lt`, omdat
de ingebouwde niet opnieuw kunnen worden gedefinieerd, en elke implementatie aan hen delegeert. De
scalaire vergelijkingsoperatoren zelf zijn ingebouwde methoden van elk ontvangertype
([Getallen](numbers.md), [Strings en tekens](collections.md)). Binnen een bound worden geschreven
operatoren als de traitmethoden gelezen (hoofdstuk 3).

## 3. Rekenkundige traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Een laag waarmee generieke code "een type dat kan worden opgeteld" kan eisen. **Rekenkunde op
concrete types gebruikt de ingebouwde operatoren** ([Getallen](numbers.md)) en gaat niet via deze
laag.

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
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**Binnen een bound kun je operatoren schrijven.** Wanneer de ontvanger een typevariabele is die door
`where` is gebonden, worden operatoren als traitmethoden gelezen (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

De traitmethode heet niet `+` omdat `+` de naam van een ingebouwde methode is en `impl` weigert die
opnieuw te definiëren (`cannot redefine built-in method`). Er is geen `Neg`: `(- x)` expandeert naar
`(- (- x x) x)`, dus `Sub` volstaat.

Geïmplementeerd voor: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` op alle numerieke types (behalve
`complex`), en `Bits` op alle gehele types en `int`.
