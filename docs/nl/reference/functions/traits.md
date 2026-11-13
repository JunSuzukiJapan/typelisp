<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
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
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementeren `Iter` via respectievelijk
`vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` (haal de iterator op met
`(iter collection)`). `Chan<T>` is zelf een `Iter` (`recv` speelt de rol van `next`;
[Kanalen](concurrency.md#2-chant--kanalen)). `Sexpr`-lijsten implementeren `Iter` niet (hun
elementtypes zijn niet uniform). Als je `Iter` voor je eigen type implementeert, kan het zoals het is
met `doiter` worden doorlopen en aan de [sequentiefuncties](sequences.md#4-sequentiefuncties-op-iter)
worden doorgegeven.

## 2. `Equal` / `Ord` (vergelijking)

`Equal` is de trait die `equal` gebruikt en komt overeen met Rusts `PartialEq`. `Ord` komt overeen met
Rusts `PartialOrd`. Ze worden gebruikt in de `:where`-grenzen van generieke functies om te eisen dat het
elementtype vergeleken kan worden.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; moet geïmplementeerd worden
(deftrait Ord (Equal)                                     ; erft van Equal
  (defmethod less (self (other Self)) bool)                  ; moet geïmplementeerd worden
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Om `Equal` te implementeren schrijf je `equal`, en voor `Ord` alleen `less`. De standaardimplementaties
vullen de rest van `Ord` in. `Ord` erft van `Equal`, dus `impl Equal X` is nodig vóór `impl Ord X`.

`(equal a b)` roept de `Equal`-implementatie van het type van `a` aan. Het schrijven voor een type dat
`Equal` niet implementeert is een typefout. Vergelijken op identiteit doe je met `eq`/`eql`, op structuur
met `equalp`. Deze drie kunnen op een waarde van elk type gebruikt worden.

Elke traitmethode kan rechtstreeks als functie worden aangeroepen (binnen een grens `where (Equal A)`/
`(Ord A)`, of op een concreet type dat haar implementeert):

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Of ze gelijk zijn (Rusts `==`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` is geïmplementeerd voor: alle numerieke typen (`i8` t/m `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` en de tupels (element voor element,
als de elementen `Equal` zijn), en `Option<T>` en `Result<T,E>` (als ze dezelfde variant zijn en hun
inhoud `equal` is). Bij de ingebouwde typen is de eigen ingebouwde methode `equal` van het type zonder meer
de `Equal`-implementatie. Getallen vergelijken met `=`, strings op inhoud, hoofdlettergevoelig. `Ord` is
geïmplementeerd voor: alle numerieke typen, `char` `string` en `cons-cell<A,B>` (lexicografisch, als de
elementen `Ord` zijn).

De `equal` van een lijst (`Sexpr`) is CLs `equal`: conses vergelijken op structuur, strings op inhoud en
elk ander atoom met `eql`. Deze vergelijking doorloopt de waarden tijdens het uitvoeren en roept geen
methode aan. Een struct in een lijst wordt dus, zoals in CL, op identiteit vergeleken, welke `impl Equal`
zijn type ook heeft.

De namen van de methoden van `Ord` overlappen niet met de ingebouwde operatoren (`< <= > >=`) of `lt`,
omdat ingebouwde dingen niet opnieuw gedefinieerd kunnen worden en elke implementatie naar hen delegeert.
De scalaire vergelijkingsoperatoren zelf zijn ingebouwde methoden per ontvangertype ([Getallen](numbers.md),
[Strings en tekens](collections.md)). Binnen een grens wordt de geschreven operator gelezen als de methode
van de trait (hoofdstuk 3). `=` wordt `equal`, en `/=` wordt `(not (equal a b))`.

## 3. Rekenkundige traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Een laag waarmee generieke code "een type dat kan worden opgeteld" kan eisen. **Rekenkunde op
concrete types gebruikt de ingebouwde operatoren** ([Getallen](numbers.md)) en gaat niet via deze
laag.

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
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; geen methoden; een combinatie van vijf
```

**Binnen een bound kun je operatoren schrijven.** Wanneer de ontvanger een typevariabele is die door
`:where` is gebonden, worden operatoren als traitmethoden gelezen (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

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
  (defmethod push (self (x Item)) ()))
```

Een collectie waaraan je elementen een voor een kunt toevoegen. Een generieke functie vraagt met
`(:where (Push C (Item T)))` om „een type waaraan `T` kan worden toegevoegd”, en voegt aan elke
zulke collectie toe met dezelfde `(push c x)`.

| Type | Wat `push` doet |
|---|---|
| `Vector<T>` | Voegt achteraan toe |
| `Deque<T>` | Voegt achteraan toe (`push-back`) |
| `Array<T>` | Voegt toe op de fill pointer (`push-extend`). Geeft een panic zonder fill pointer |
| `HashSet<T>` | Stopt het in de verzameling (`insert`) |

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
  (defmethod pop (self) Option<Item>))
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
