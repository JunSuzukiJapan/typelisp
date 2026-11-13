<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Standard-Traits

Die Traits für Iteration, Vergleich, Arithmetik und das Hinzufügen und Entnehmen von Elementen. Die anderen Standard-Traits stehen in ihren eigenen
Kapiteln: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Fehlertypen](option-result.md#3-fehlertypen-und-der-trait-error)), `print-object`
([Ausgabe](printing.md#5-print-object-typabhängige-druckdarstellung)) sowie die Stream-Traits und
`Pathish` ([Streams und Dateien](streams-files.md)). Welche Typen welche implementieren, steht unter
[Typen](../types.md). Wie man Traits definiert, steht in der
[Syntaxreferenz](../syntax.md#39-deftrait--impl--traits).

## 1. Der Trait `Iter` und die Iteration

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementieren `Iter` jeweils über `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (den Iterator bekommt man mit `(iter Sammlung)`). `Chan<T>` ist selbst
ein `Iter` (`recv` übernimmt die Rolle von `next`; [Kanäle](concurrency.md#2-chant--kanäle)). `Sexpr`-Listen
implementieren `Iter` nicht (ihre Elementtypen sind nicht einheitlich). Implementiert man `Iter` für einen
eigenen Typ, lässt er sich unverändert mit `doiter` durchlaufen und an die
[Sequenzfunktionen](sequences.md#4-sequenzfunktionen-auf-iter) übergeben.

## 2. `Equal` / `Ord` (Vergleich)

`Equal` ist der Trait, den `equal` verwendet, und entspricht Rusts `PartialEq`. `Ord` entspricht Rusts
`PartialOrd`. Sie werden in den `:where`-Schranken generischer Funktionen verwendet, um zu verlangen,
dass der Elementtyp verglichen werden kann.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; muss implementiert werden
(deftrait Ord (Equal)                                     ; erbt von Equal
  (defmethod less (self (other Self)) bool)                  ; muss implementiert werden
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Um `Equal` zu implementieren, schreibt man `equal`, für `Ord` nur `less`. Den Rest von `Ord` füllen
die Standardimplementierungen. `Ord` erbt von `Equal`, daher ist vor `impl Ord X` ein `impl Equal X` nötig.

`(equal a b)` ruft die `Equal`-Implementierung des Typs von `a` auf. Für einen Typ, der `Equal` nicht
implementiert, ist es ein Typfehler. Nach Identität vergleicht man mit `eq`/`eql`, nach Struktur mit
`equalp`. Diese drei lassen sich auf Werte jedes Typs anwenden.

Jede Trait-Methode kann direkt als Funktion aufgerufen werden (innerhalb einer Schranke `where (Equal A)`/
`(Ord A)` oder auf einem konkreten Typ, der sie implementiert):

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Ob sie gleich sind (Rusts `==`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` ist implementiert für: alle numerischen Typen (`i8` bis `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` und die Tupel (Element für Element,
wenn die Elemente `Equal` sind) sowie `Option<T>` und `Result<T,E>` (wenn sie dieselbe Variante sind und
ihr Inhalt `equal` ist). Bei den eingebauten Typen ist die eigene eingebaute Methode `equal` des Typs
unverändert die `Equal`-Implementierung. Zahlen vergleichen mit `=`, Strings nach Inhalt unter Beachtung
der Groß- und Kleinschreibung. `Ord` ist implementiert für: alle numerischen Typen, `char` `string` und
`cons-cell<A,B>` (lexikographisch, wenn die Elemente `Ord` sind).

Das `equal` einer Liste (`Sexpr`) ist CLs `equal`: Conses werden nach Struktur verglichen, Strings nach
Inhalt und jedes andere Atom mit `eql`. Dieser Vergleich durchläuft die Werte zur Laufzeit und ruft keine
Methode auf. Ein Struct in einer Liste wird daher wie in CL nach Identität verglichen, unabhängig davon,
welches `impl Equal` sein Typ hat.

Die Methodennamen von `Ord` überschneiden sich nicht mit den eingebauten Operatoren (`< <= > >=`) oder
`lt`, weil Eingebautes nicht neu definiert werden kann und jede Implementierung an sie delegiert. Die
skalaren Vergleichsoperatoren selbst sind eingebaute Methoden je Empfängertyp ([Zahlen](numbers.md),
[Strings und Zeichen](collections.md)). Innerhalb einer Schranke wird der geschriebene Operator als
Methode des Traits gelesen (Kapitel 3). `=` wird zu `equal`, `/=` zu `(not (equal a b))`.

## 3. Arithmetische Traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Eine Schicht, mit der generischer Code „einen Typ, der sich addieren lässt“ verlangen kann. **Arithmetik auf
konkreten Typen verwendet die eingebauten Operatoren** ([Zahlen](numbers.md)) und geht nicht durch diese
Schicht.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; die Distanz ist immer int (wie bei ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; keine Methoden; eine Kombination aus fünf
```

**Innerhalb einer Schranke kann man Operatoren schreiben.** Ist der Empfänger eine durch `:where` gebundene
Typvariable, werden Operatoren als Trait-Methoden gelesen (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
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
den Ergebnistyp mit `(:where (Div T (Output R)))` entgegen oder verlangt mit `(:where (Div T (Output T)))`
eine in `T` geschlossene Division (dann kann kein `int` übergeben werden).

## 4. Der Trait `Push` (Elemente hinzufügen)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Eine Sammlung, der Elemente einzeln hinzugefügt werden können. Eine generische Funktion verlangt mit
`(:where (Push C (Item T)))` „einen Typ, dem `T` hinzugefügt werden kann“, und fügt jeder solchen
Sammlung mit demselben `(push c x)` hinzu.

| Typ | Was `push` tut |
|---|---|
| `Vector<T>` | Hängt ans Ende an |
| `Deque<T>` | Hängt ans Ende an (`push-back`) |
| `Array<T>` | Fügt am Füllzeiger hinzu (`push-extend`). Panic ohne Füllzeiger |
| `HashSet<T>` | Legt es in die Menge (`insert`) |

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

Ein eigener Typ, der `Push` implementiert, kann denselben generischen Funktionen übergeben werden.
Er kann auch ein Trait-Objekt sein, wie in `:dyn Push<int>`.

## 5. Der Trait `Pop` (Elemente entnehmen)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Eine Sammlung, aus der sich das zuletzt mit `push` hinzugefügte Element wieder entnehmen lässt.
`none`, wenn sie leer ist. Eine generische Funktion verlangt sie mit `(:where (Pop C (Item T)))`.

| Typ | Was `pop` tut |
|---|---|
| `Vector<T>` | Entnimmt das letzte Element |
| `Deque<T>` | Entnimmt das letzte Element (`pop-back`) |
| `Array<T>` | Entnimmt das Element direkt vor dem Füllzeiger. Panic ohne Füllzeiger |

Ein `HashSet<T>` hat kein „letztes Element“, ist also ein `Push`, aber kein `Pop`.

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

Wie bei `Push` kann ein eigener Typ ihn implementieren, und er kann ein Trait-Objekt sein, wie in
`:dyn Pop<int>`.
