<!-- translated-from: docs/ja/reference/functions/traits.md @ b228b9f6a6a6786e814804c166bce6b79db55522 -->
# Standard-Traits

Die Traits für Iteration, Vergleich und Arithmetik. Die anderen Standard-Traits stehen in ihren eigenen
Kapiteln: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Fehlertypen](option-result.md#3-fehlertypen-und-der-trait-error)), `print-object`
([Ausgabe](printing.md#5-print-object-typabhängige-druckdarstellung)) sowie die Stream-Traits und
`Pathish` ([Streams und Dateien](streams-files.md)). Welche Typen welche implementieren, steht unter
[Typen](../types.md). Wie man Traits definiert, steht in der
[Syntaxreferenz](../syntax.md#39-deftrait--impl--traits).

## 1. Der Trait `Iter` und die Iteration

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementieren `Iter` jeweils über `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (den Iterator bekommt man mit `(iter Sammlung)`). `Chan<T>` ist selbst
ein `Iter` (`recv` übernimmt die Rolle von `next`; [Kanäle](concurrency.md#2-chant--kanäle)). `Sexpr`-Listen
implementieren `Iter` nicht (ihre Elementtypen sind nicht einheitlich). Implementiert man `Iter` für einen
eigenen Typ, lässt er sich unverändert mit `doiter` durchlaufen und an die
[Sequenzfunktionen](sequences.md#4-sequenzfunktionen-auf-iter) übergeben.

## 2. `Eq` / `Ord` (Vergleich)

Diese entsprechen Rusts `PartialEq`/`PartialOrd` (benannt `Eq`/`Ord`). Sie werden in den `where`-Schranken
generischer Funktionen verwendet, um zu verlangen, dass Elementtypen vergleichbar sind
(`sort`/`member`/`assoc` usw.).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; muss implementiert werden
  (not-equals ((self Self) (other Self)) bool             ; Standardimplementierung
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; erbt von Eq
  (less ((self Self) (other Self)) bool)                  ; muss implementiert werden
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Um `Eq` zu implementieren, schreibt man nur `equals`, für `Ord` nur `less`. Die Standardimplementierungen
ergänzen den Rest. `Ord` erbt von `Eq`, daher ist `impl Eq X` vor `impl Ord X` nötig.

Jede Trait-Methode lässt sich unverändert als Funktion aufrufen (innerhalb einer Schranke `where (Eq A)`/
`(Ord A)` oder auf einem konkreten Typ, der sie implementiert):

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Ob sie gleich sind (Rusts `==`) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Ob sie ungleich sind (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` ist implementiert für: alle numerischen Typen (`i8` bis `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, also Identität; verwendet von den Wertmustern von `match`)
und `cons-cell<A,B>` (rekursiv, wenn die Elemente `Eq` sind). `Ord` ist implementiert für: alle numerischen
Typen, `char` `string` und `cons-cell<A,B>` (lexikografisch, wenn die Elemente `Ord` sind).

Die Methodennamen überschneiden sich nicht mit den eingebauten Operatoren (`= /= < <= > >=`) oder `eq`/`lt`,
weil sich die eingebauten nicht neu definieren lassen, und jede Implementierung delegiert an sie. Die
skalaren Vergleichsoperatoren selbst sind eingebaute Methoden des jeweiligen Empfängertyps
([Zahlen](numbers.md), [Zeichenketten und Zeichen](collections.md)). Innerhalb einer Schranke werden
geschriebene Operatoren als Trait-Methoden gelesen (Kapitel 3).

## 3. Arithmetische Traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Eine Schicht, mit der generischer Code „einen Typ, der sich addieren lässt“ verlangen kann. **Arithmetik auf
konkreten Typen verwendet die eingebauten Operatoren** ([Zahlen](numbers.md)) und geht nicht durch diese
Schicht.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (type Output) (div ((self Self) (other Self)) Output))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; die Distanz ist immer int (wie bei ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; keine Methoden; eine Kombination aus fünf
```

**Innerhalb einer Schranke kann man Operatoren schreiben.** Ist der Empfänger eine durch `where` gebundene
Typvariable, werden Operatoren als Trait-Methoden gelesen (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Die Trait-Methode heißt nicht `+`, weil `+` der Name einer eingebauten Methode ist und `impl` sich weigert, sie
neu zu definieren (`cannot redefine built-in method`). Es gibt kein `Neg`: `(- x)` expandiert zu
`(- (- x x) x)`, daher genügt `Sub`.

Implementiert für: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` auf allen numerischen Typen (außer `complex`) und
`Bits` auf allen Ganzzahltypen und `int`.


**`Div` benennt seinen Ergebnistyp mit dem assoziierten Typ `Output`.** Wie in CL ist `/` auf zwei
ganzen Zahlen ein `rational`, daher ist das `Output` von `int` `rational` und das jedes anderen
Zahlentyps der Typ selbst. Weil `Output` je nach Typ verschieden ist, enthält `Number` kein `Div` (eine
Supertrait-Liste muss jeden assoziierten Typ festlegen). Eine generische Funktion, die dividiert, nimmt
den Ergebnistyp mit `(where (Div T (Output R)))` entgegen oder verlangt mit `(where (Div T (Output T)))`
eine in `T` geschlossene Division (dann kann kein `int` übergeben werden).