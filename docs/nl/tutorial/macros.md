<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Macro's

Een macro is een functie die een programma neemt en een programma teruggeeft. Met macro's kun je
nieuwe syntaxis maken die functies niet kunnen uitdrukken. De macro's van typelisp werken op dezelfde
manier als `defmacro` in Common Lisp. Dit hoofdstuk gaat ervan uit dat je "Lijsten (S-expressies)" in
[Aan de slag](intro.md) hebt gelezen.

## 1. Hoe macro's van functies verschillen

Een functie ontvangt haar argumenten **nadat ze zijn geëvalueerd**. Een macro ontvangt ze **als
expressies, vóór de evaluatie** (als S-expressiedata), bouwt een andere expressie en geeft die terug.
De teruggegeven expressie vervangt de macro-aanroep, en pas dan wordt ze getypechecked en uitgevoerd.
Deze vervanging heet **expansie**.

Syntaxis zoals `unless` kan bijvoorbeeld niet als functie worden geschreven. Als functie zou de body
eerst worden geëvalueerd, zelfs wanneer de voorwaarde waar is.

## 2. `defmacro` en quasiquote

Laten we `my-unless` maken, dat zijn body alleen uitvoert wanneer de voorwaarde onwaar is.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Macroargumenten hebben geen geschreven types. Elk argument is S-expressiedata.
- `&rest body` ontvangt de overige argumenten samen als één lijst.
- Een expressie die met `` ` `` (quasiquote) begint wordt als data opgebouwd, zoals ze is geschreven.
  Daarbinnen:
  - `,test` voegt de inhoud van de variabele `test` op die plek in.
  - `,@body` voegt de elementen van de lijst `body` op die plek samen in.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Je kunt de expansie controleren met `macroexpand-1`. Bij het schrijven van een macro is eerst naar
zijn expansie kijken de snelste weg vooruit.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Expansies worden ook getypechecked

De expressie die een macro teruggeeft wordt getypechecked zoals elke expressie die je met de hand
schrijft.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

De fout wordt gemeld op de plek waar de macro werd aangeroepen.

De regels dat de else-tak van `if` niet mag worden weggelaten en dat beide takken van een `if`
hetzelfde type moeten hebben, gelden onverkort voor expansies. De bovenstaande `my-unless` eindigt
met `(progn ,@body ())` zodat, wat het type van de laatste expressie van de body ook is, beide takken
van de `if` het type `()` hebben.

## 4. Naamconflicten en `gensym`

Een eenvoudige macro die de waarden van twee variabelen verwisselt ziet er zo uit:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Meestal werkt dat, maar het gaat mis wanneer de variabele van de aanroeper toevallig `tmp` heet.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (not swapped)
```

De expansie is `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, en de `tmp` die de macro
maakte verbergt de `tmp` van de aanroeper.

Om dit te voorkomen, maak je de namen van variabelen die binnen een macro worden gebruikt met
`gensym`. `gensym` geeft een nieuw symbool terug dat nergens in een programma kan worden geschreven.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Net als in Common Lisp voorkomen de macro's van typelisp naamconflicten niet automatisch (ze zijn niet
hygiënisch). Onthoud: **gebruik `gensym` voor de bindingen die een macro maakt.**

Op dezelfde manier kan een macro die zijn body een opgegeven aantal keren herhaalt zo worden
geschreven:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Verschillend expanderen afhankelijk van de argumenten

Een macrobody is gewone typelisp-code, dus ze kan haar argumenten met `if` of `match` inspecteren en
een andere expansie bouwen. De argumenten zijn S-expressiedata (`Option<Sexpr>`), en de lege lijst is
`none`.

Laten we `my-and` maken, dat `true` teruggeeft als al zijn voorwaarden waar zijn.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; no arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; just one
         `(if ,f (my-and ,@more) false)))             ; two or more
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` is een patroon dat de kop van een lijst in `f` en de rest in `more` neemt.
- `sexpr-null` test of S-expressiedata de lege lijst is.
- De laatste `_`-tak is nodig omdat S-expressiedata andere vormen heeft dan lijsten (getallen,
  strings enzovoort), en `match` eist dat ook die zijn gedekt. Een `&rest`-argument is altijd een
  lijst, dus deze tak wordt in werkelijkheid nooit uitgevoerd.
- Een macro kan zichzelf in zijn expansie aanroepen. De expansie wordt herhaald totdat er geen
  macro-aanroepen meer over zijn.

## 6. Optionele argumenten

`&optional` ontvangt argumenten die mogen worden weggelaten. Er kunnen standaardwaarden worden
opgegeven.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` ontvangt keyword-argumenten
([Syntaxreferentie 3.14](../reference/syntax.md#314-defmacro--macrodefinities)).

## 7. `macrolet`: macro's voor slechts één plek

Een macro die alleen binnen één expressie wordt gebruikt kan met `macrolet` worden gedefinieerd. Hij
is daarbuiten niet zichtbaar.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Dingen om in gedachten te houden

- **Een macro kan pas na zijn definitie worden aangeroepen.** Net als bij functies definieer je hem
  bovenaan het bestand.
- Maak een macro beschikbaar voor andere modules met `(pub defmacro ...)`.
- Veel van de standaardsyntaxis, waaronder `when`, `unless`, `cond`, `and`, `or` en `dotimes`, is als
  macro gedefinieerd. Je kunt zien wat erin zit met `(macroexpand '(when true 1))`.
- Als iets als functie kan worden geschreven, schrijf het dan als functie. Macro's kunnen niet als
  waarden worden doorgegeven, en je moet hun expansie lezen om te begrijpen wat ze doen.

## 9. Wat je hierna kunt lezen

- [Foutafhandeling](errors.md): `Result`, `panic`, `catch` / `throw`
- [Macrofuncties](../reference/functions/system.md#8-macros): `gensym`, `macroexpand` en meer
