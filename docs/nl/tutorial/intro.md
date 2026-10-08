<!-- translated-from: docs/ja/tutorial/intro.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Aan de slag

Beginnend met het evalueren van expressies in de REPL behandelt dit hoofdstuk achtereenvolgens
functies, variabelen, voorwaarden, lussen, lijsten en `Vector`. Hoe je `typl` bouwt staat in
[README.md](../../../README.md).

## 1. De REPL starten

Zonder argumenten gestart gaat `typl` de REPL (interactieve modus) in. Typ een expressie na
`typl>` en ze wordt ter plekke geëvalueerd en de waarde wordt afgedrukt. `:quit` verlaat de REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Vanaf hier worden REPL-invoer en -resultaten in deze vorm getoond.

## 2. Expressies evalueren

typelisp is een Lisp, dus een expressie staat tussen haakjes met **eerst de operator of functienaam**.
Je schrijft `(+ 1 2)`, niet `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Getallen zijn er in deze soorten:

- **Gehele getallen** hebben het type `int`. Er is geen bovengrens aan hun grootte.
- **Decimale getallen** hebben het type `f64`. Schrijf ze met een decimaalteken, zoals `1.5` of `2.0`.
- Je kunt `int` en `f64` niet in één berekening mengen. `(+ 1 2.0)` is een typefout. Schrijf
  `(as f64 1)` om te converteren.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` op twee gehele getallen geeft een geheel getal waarvan het gebroken deel is weggelaten (het
levert geen breuk op zoals in Common Lisp). Gebruik `(mod 7 2)` voor de rest.

De booleaanse waarden zijn `true` en `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Functies definiëren

Functies worden gedefinieerd met `defun`. **De argumenttypes en het returntype worden altijd
geschreven.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` betekent "een argument `n` van het type `int`". Bij meerdere argumenten som je ze op:
  `((a int) (b int))`.
- De `int` na de argumentenlijst is het returntype.
- De waarde van de laatste expressie in de body is de returnwaarde van de functie. Je schrijft geen
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Een aanroep waarvan de types niet overeenkomen wordt **voordat ze wordt uitgevoerd** als typefout
gemeld. Voer je een bestand uit, dan betekent één typefout ergens dat geen enkele regel van het
programma wordt uitgevoerd.

Om een argument optioneel te maken, gebruik je `&optional`. Geef je een standaardwaarde op, dan
neemt het argument die waarde aan als het wordt weggelaten.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

De `false` die als eerste argument van `format` wordt gegeven betekent "geef het resultaat als string
terug in plaats van het af te drukken". Elke `~a` wordt vervangen door het volgende argument.

## 4. Variabelen

Lokale variabelen worden gemaakt met `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Het type van een `let`-variabele wordt uit haar beginwaarde afgeleid. Je hoeft het niet te
  schrijven.
- De variabelen van één `let` kunnen niet naar elkaar verwijzen. Gebruik `let*` om een variabele uit
  de voorgaande op te bouwen.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Om de waarde van een variabele te wijzigen, gebruik je `setf`. **Toewijzing kan het type van de
variabele niet veranderen.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Globale variabelen worden gedefinieerd met `defvar`. Hier schrijf je het type wel.

```lisp
(defvar (counter int) 0)
```

## 5. Voorwaarden

### if

Schrijf `(if voorwaarde then-expressie else-expressie)`. **De else-expressie mag niet worden
weggelaten.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Alleen een expressie van het type `bool` kan een voorwaarde zijn. Een getal schrijven, zoals in
  `(if 0 ...)`, is een typefout.
- De then- en else-expressies moeten hetzelfde type hebben.

Als er in het onware geval niets moet gebeuren, gebruik je `when` (en `unless` voor het omgekeerde).

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

Bij drie of meer voorwaarden leest `cond` beter. De laatste `else` wordt genomen als geen van de
voorwaarden geldt.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Om op de vorm van een waarde te vertakken, gebruik je `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` komt met elke waarde overeen. Omdat `int` talloze waarden heeft, is het weglaten van de
`_`-tak een fout die zegt dat niet alle gevallen zijn gedekt. Waar `match` echt uitblinkt is het
uit elkaar halen van `Option` en types die je zelf definieert, wat in het volgende hoofdstuk aan bod
komt, [Basis van types](types.md).

## 6. Lussen

Een functie kan zichzelf aanroepen.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Gebruik `dotimes` voor een vast aantal herhalingen. `i` loopt van 0 tot `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Er zijn ook `while`, `do` en de uitgebreide `loop` uit Common Lisp. De clausulewoorden van de
uitgebreide `loop` worden als keywords geschreven (`:for`, `:collect` enzovoort).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Lijsten en Vector

### Vector

Om een reeks waarden van hetzelfde type te bewaren, gebruik je `Vector<T>`. `T` is het
elementtype.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; prints #<vector<int> 3 1 2>
```

- `(Vector::new)` alleen bepaalt het elementtype niet, dus geef het type op met
  `(the Vector<int> ...)`.
- `(push v x)` voegt aan het einde toe, `(get v i)` leest element `i` en `(len v)` geeft de lengte.
- Een `get` met een index buiten het bereik stopt het programma met een fout.

### lambda en functies van hogere orde

Anonieme functies worden gemaakt met `lambda`. Net als bij `defun` schrijf je de argument- en
returntypes.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` en verwanten nemen een `Vector` die met `(iter v)` tot
**iterator** is gemaakt. De collectie komt eerst en de functie daarna. De `v` hierboven is met
`let` gebonden en is daarom buiten die `let` niet te gebruiken. Het volgende voorbeeld
definieert `v` eerst met `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Om de elementen één voor één te verwerken, gebruik je `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Een functie die een functie als argument neemt, schrijft het type van dat argument als
`(fn (argumenttypes...) returntype)`. Een functie die met `defun` is gedefinieerd kan met haar naam
als waarde worden doorgegeven.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Lijsten (S-expressies)

Lijsten die met `'(1 2 3)` of `(list 1 2 3)` zijn gemaakt, zijn **S-expressiedata**. Hun elementen
hoeven geen type te delen.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S-expressiedata is vooral bedoeld om programma's zelf te verwerken, in macro's ([Macro's](macros.md))
en met `read`. Gebruik voor gegevens waarvan de elementen een bekend type hebben `Vector<T>`. Een
S-expressielijst kun je doorlopen met `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Een paar van twee waarden maak je met `cons` en haal je uit elkaar met `car` en `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Een programma in een bestand schrijven

Een programma kan in een bestand (met de extensie `.typl`) worden geschreven en met
`typl bestandsnaam` worden uitgevoerd. Gebruik `println` om resultaten te tonen.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` drukt af met dezelfde directieven als `format` en eindigt met een nieuwe regel. `print`
  voegt de nieuwe regel niet toe.
- `~a` voegt een waarde in in leesbare vorm, en `~s` in een vorm die weer kan worden ingelezen
  (strings krijgen hun `"`).
- Een bestand wordt van boven naar beneden gelezen. **Een functie kan niet worden aangeroepen vóór
  haar definitie.**

## 9. Wat je hierna kunt lezen

- [Basis van types](types.md): `Option`, `Result`, structs, enums, generics
- [Voor Common Lisp-programmeurs](../guide/from-common-lisp.md): een lijst met verschillen voor
  mensen die Common Lisp kennen
