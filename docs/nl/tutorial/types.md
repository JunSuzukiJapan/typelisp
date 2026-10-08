<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Basis van types

typelisp is een statisch getypeerde taal. Dit hoofdstuk legt uit wat de typechecker voor je doet, de
types die je het meest zult gebruiken (`Option`, `Result`, structs en enums) en generics. Het gaat
ervan uit dat je [Aan de slag](intro.md) hebt gelezen.

## 1. Wat statische typering betekent

In typelisp staat het type van elke expressie vast voordat het programma wordt uitgevoerd. Een
expressie waarvan de types niet passen is een fout voordat er iets draait.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Dit bestand uitvoeren stopt met een typefout zonder zelfs `start` af te drukken.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Je moet types schrijven voor functieargumenten en returnwaarden, globale variabelen en structvelden.
Het type van een `let`-variabele wordt uit haar beginwaarde afgeleid.

De belangrijkste types:

| Type | Voorbeeldwaarden |
|---|---|
| `int` | `42`, `-7` (gehele getallen met willekeurige precisie) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Gehele getallen met vaste breedte |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Het returntype van een functie die geen waarde teruggeeft |

Er is geen manier om tijdens runtime naar het type van een waarde te vragen (geen `typep` of
`type-of` van Common Lisp), omdat elk type al bekend is voordat het programma wordt uitgevoerd.

## 2. `Option<T>`: een waarde die kan ontbreken

typelisp heeft geen `nil`. "Er is misschien geen waarde" wordt uitgedrukt met het type `Option<T>`.
Een waarde van `Option<T>` is ofwel `some`, met één waarde van `T`, ofwel `none`, die niets bevat.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` is niet `int`, dus hij kan niet zoals hij is in rekenkunde worden gebruikt.
`(+ (safe-div 10 2) 1)` is een typefout. Om te gebruiken wat erin zit, scheid je `some` van `none`
met `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- In de `(some q)`-tak wordt de inhoud aan de variabele `q` gebonden.
- `match` controleert dat zijn takken **elk geval dekken**. De `(none)`-tak vergeten is een typefout.

### Waarom er geen nil is

In veel talen kan `nil` (`null`) voor een waarde van elk type staan. Daardoor blijft het vergeten van
het geval "geen waarde" onopgemerkt totdat het programma draait. In typelisp heeft een plek waar een
waarde kan ontbreken het type `Option<T>`, en de code komt niet door de typechecker tenzij `match` het
geval `none` afhandelt. Een vergeten geval wordt gevonden voordat het programma draait.

Voorwaarden volgen hetzelfde idee: alleen een `bool` kan de voorwaarde van `if` zijn. Er is geen regel
zoals die van Common Lisp dat "alles behalve `nil` waar is".

### Veelvoorkomende bewerkingen

| Vorm | Betekenis |
|---|---|
| `(unwrap-or opt default)` | De inhoud bij `some`; de standaardwaarde bij `none` |
| `(unwrap opt)` | Haalt de inhoud eruit. Stopt het programma bij `none` |
| `(is-some opt)` / `(is-none opt)` | Test welke van de twee het is |

Veel functies van de standaardbibliotheek geven `Option` terug. `position` geeft bijvoorbeeld de
positie in `some` terug als het element is gevonden en `none` als dat niet zo is.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: een bewerking die kan mislukken

Een bewerking die kan mislukken geeft `Result<T,E>` terug: `ok` met een waarde van `T` bij succes, of
`err` met een fout `E` bij mislukking.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Je eigen functies kunnen ook `Result` teruggeven.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Gebruik `Option` wanneer een ontbrekende waarde geen uitleg nodig heeft, en `Result` wanneer je wilt
zeggen waarom iets mislukte. [Foutafhandeling](errors.md) behandelt het afhandelen van fouten in
detail.

## 4. `defstruct`: structs

Een type met benoemde velden wordt gedefinieerd met `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

De definitie geeft je het volgende:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Om een struct eigen functies te geven, gebruik je `defmethod`. Het type van het eerste argument
(`self`) bepaalt bij welk type de methode hoort.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Schrijf je in plaats van een `self`-argument alleen de typenaam, dan ontstaat een functie die als
`point::origin` wordt aangeroepen.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: een van meerdere vormen

Een waarde die een van meerdere vormen is, zoals "een cirkel, een rechthoek of een punt", wordt
gedefinieerd met `defenum`. Elke vorm heet een **variant**. Elke variant kan een ander aantal en een
ander type waarden bevatten.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

Waarden worden gemaakt met de typenaam ervoor, zoals in `shape::circle`. In `match` worden ze op
variantnaam uit elkaar gehaald.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Ook hier controleert `match` dat elk geval is gedekt. Als je later een variant aan `shape` toevoegt,
wordt elke `match` die hem niet afhandelt een typefout, zodat geen plek die moet worden aangepast
wordt gemist.

Na `(use shape)` kun je `(rect 5 6)` schrijven zonder de typenaam.

`Option` en `Result` zijn enums die met ditzelfde mechanisme zijn gebouwd.

## 6. Generics

Een functie die voor elk type werkt wordt gedefinieerd met een **typeparameter** `<T>` na haar naam.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Bij het aanroepen geef je het type niet op. `T` wordt uit de argumenten afgeleid.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Structs en enums kunnen ook generiek zijn. `Vector<T>`, `Option<T>` en `Result<T,E>` zijn types van
dit soort.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Binnen een generieke functie is niets over `T` bekend, dus je kunt waarden van `T` niet vergelijken of
optellen. Om iets te eisen als "elk type dat kan worden vergeleken", gebruik je traits
([Traits](traits.md)).

## 7. Een type een andere naam geven

`deftype` geeft een type een andere naam.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` is slechts een andere schrijfwijze van `int`, geen nieuw type. Een gewone `int` doorgeven
waar `meters` wordt verwacht is geen fout. Wil je ze gescheiden houden, maak dan een struct, zoals in
`(defstruct meters (value int))`.

## 8. Wat je hierna kunt lezen

- [Traits](traits.md): types gemeenschappelijke bewerkingen geven
- [Types](../reference/types.md): de ingebouwde types en de traits die elk ervan implementeert
