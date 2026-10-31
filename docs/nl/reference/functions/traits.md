<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Standaardtraits

De traits voor iteratie, vergelijking, rekenkunde en het toevoegen en uitnemen van elementen. De overige standaardtraits staan in hun eigen
hoofdstukken: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Foutentypes](option-result.md#3-foutentypes-en-de-trait-error)), `print-object`
([Afdrukken](printing.md#5-print-object-afgedrukte-weergave-per-type)), en de streamtraits en
`Pathish` ([Streams en bestanden](streams-files.md)). Welke types welke implementeren staat in
[Types](../types.md). Hoe je traits definieert staat in de
[Syntaxreferentie](../syntax.md#39-deftrait--impl--traits).

## 1. De trait `Iter` en iteratie

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
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
gebruikt in de `:where`-bounds van generieke functies om te eisen dat elementtypes kunnen worden
vergeleken (`sort`/`member`/`assoc` enzovoort).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; must be implemented
  (not-equals (self (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less (self (other Self)) bool)                  ; must be implemented
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
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
  (shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; geen methoden; een combinatie van vijf
```

**Binnen een bound kun je operatoren schrijven.** Wanneer de ontvanger een typevariabele is die door
`:where` is gebonden, worden operatoren als traitmethoden gelezen (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

De traitmethode heet niet `+` omdat `+` de naam van een ingebouwde methode is en `impl` weigert die
opnieuw te definiëren (`cannot redefine built-in method`). Er is geen `Neg`: `(- x)` expandeert naar
`(- (- x x) x)`, dus `Sub` volstaat.

Geïmplementeerd voor: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` op alle numerieke types (behalve
`complex`), en `Bits` op alle gehele types en `int`.


**`Div` noemt zijn resultaattype met het geassocieerde type `Output`.** Zoals in CL is `/` op twee
gehele getallen een `rational`, dus het `Output` van `int` is `rational` en dat van elk ander
numeriek type het type zelf. Omdat `Output` per type verschilt, bevat `Number` geen `Div` (een
supertraitlijst moet elk geassocieerd type vastleggen). Een generieke functie die deelt, ontvangt het
resultaattype met `(:where (Div T (Output R)))`, of vraagt een in `T` gesloten deling met
`(:where (Div T (Output T)))` (dan kan geen `int` worden doorgegeven).

## 4. De trait `Push` (elementen toevoegen)

```lisp
(deftrait Push ()
  (:type Item)
  (push (self (x Item)) ()))
```

Een collectie waaraan je elementen een voor een kunt toevoegen. Een generieke functie vraagt met
`(:where (Push C (Item T)))` om „een type waaraan `T` kan worden toegevoegd”, en voegt aan elke
zulke collectie toe met dezelfde `(push c x)`.

| Type | Wat `push` doet |
|---|---|
| `Vector<T>` | Voegt achteraan toe |
| `Deque<T>` | Voegt achteraan toe (`push-back`) |
| `Array<T>` | Voegt toe op de fill pointer (`push-extend`). Geeft een panic zonder fill pointer |
| `HashSet<T>` | Neemt het op in de verzameling (`insert`). Alleen een `Push` als het elementtype `Hash` implementeert |

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

Een eigen type dat `Push` implementeert, kun je aan dezelfde generieke functies doorgeven. Het kan
ook een trait-object zijn, zoals in `:dyn Push<int>`.

## 5. De trait `Pop` (elementen uitnemen)

```lisp
(deftrait Pop ()
  (:type Item)
  (pop (self) Option<Item>))
```

Een verzameling waaruit het element dat `push` het laatst toevoegde weer uitgenomen kan worden.
`none` als ze leeg is. Een generieke functie vraagt erom met `(:where (Pop C (Item T)))`.

| Type | Wat `pop` doet |
|---|---|
| `Vector<T>` | Neemt het laatste element uit |
| `Deque<T>` | Neemt het laatste element uit (`pop-back`) |
| `Array<T>` | Neemt het element net voor de fill pointer uit. Panic zonder fill pointer |

Een `HashSet<T>` heeft geen "laatste element", dus is wel een `Push` maar geen `Pop`.

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

Net als bij `Push` kan een eigen type hem implementeren, en hij kan een trait-object zijn, zoals in
`:dyn Pop<int>`.
