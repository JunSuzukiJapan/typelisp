<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Voor Common Lisp-programmeurs

typelisp erft de syntaxis van Common Lisp (CL) en veel van zijn functienamen, maar is een statisch
getypeerde taal. Daardoor werkt CL-code niet altijd zoals ze is geschreven. Deze handleiding verzamelt
de punten waarop mensen die aan CL gewend zijn vaak struikelen, samen met hoe je de code herschrijft.

## 1. Er is geen `nil` of `t`

De booleaanse waarden zijn `true` en `false`. `nil` en `t` zijn niet gedefinieerd.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Alleen een `bool` kan een voorwaarde zijn.** `0` of een lege lijst als voorwaarde schrijven is een
  typefout. Er is geen regel dat "alles behalve nil waar is".
- **De else-tak van `if` mag niet worden weggelaten.** `(if c x)` is een fout. Gebruik `when` /
  `unless` wanneer geen else-tak nodig is.
- **"Geen waarde" wordt uitgedrukt met `Option<T>`.** Een functie die in CL nil teruggaf voor "niet
  gevonden", geeft hier `(some x)` of `none` terug.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- De lege lijst `()` is, afhankelijk van de context, ofwel de waarde van het type Unit (de
  returnwaarde van een functie die niets teruggeeft) ofwel de lege lijst van S-expressiedata. Het is
  een andere waarde dan `false`.

## 2. Types schrijven

Functieargumenten en returnwaarden moeten types hebben.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; a generic function
  (unwrap-or (first (iter v)) default))
```

- Een definitie zonder types, zoals `(defun f (x) x)`, kan niet worden geschreven.
- Globale variabelen zoals `defvar` hebben ook een type nodig: `(defvar (count int) 0)`.
- `the` is geen controle tijdens runtime maar een annotatie voor de typechecker.
- **Er is geen manier om types tijdens runtime te inspecteren.** Er is geen `typep` of `type-of`,
  omdat het type van elke waarde tijdens het compileren vaststaat. Om een van meerdere types te
  accepteren, maak je een somtype met `defenum` of gebruik je een trait.
- `deftype` definieert een typealias. Een type dat een waardenbereik beschrijft, zoals
  `(deftype small () '(integer 0 9))`, kan niet worden gemaakt.

Het standaardtype voor gehele getallen `int` heeft willekeurige precisie; net als het integer van CL
is er geen bovengrens aan zijn grootte. De types met vaste breedte `i8` tot en met `i32` en `u8` tot
en met `u32` bestaan ook. Er is geen geheel type van 64 bits met vaste breedte.

## 3. Functies als waarden

typelisp scheidt de namespaces van functies en variabelen niet. De naam van een functie kan zoals hij
is als waarde worden doorgegeven. Er is geen `#'` en geen `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; call it directly, not with funcall

(apply-to twice 5)                        ; twice, not #'twice
```

- Ingebouwde functies zoals `+` en `1+` kunnen ook zoals ze zijn als waarde worden doorgegeven,
  waar het type van het argument vaststaat, zoals in `(fn (int) int)`. Bij het doorgeven aan een
  generieke functie zoals `foldl` of `map` is niet bekend welk `+` van welk type bedoeld wordt,
  dus wikkel hem in een `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Sequentiefuncties nemen **eerst de collectie en dan de functie**: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. Dit is het omgekeerde van `(mapcar f list)` in CL.
- `lambda` kan `&optional` of `&key` niet gebruiken (`&rest` kan wel).
- **Een functie kan niet worden aangeroepen voordat ze is gedefinieerd.** In CL kun je een functie
  aanroepen die je later definieert, maar hier geeft dat `no such function`. Declareer bij onderling
  recursieve functies eerst een van beide met `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Lijsten en Vector

Wat met een CL-lijst overeenkomt is **S-expressiedata**, waarvan het type `Option<Sexpr>` is (de lege
lijst is `none`). `(list 1 2 3)` en `'(a b c)` hebben dit type. S-expressiedata is iets waarmee macro's
en `read` werken; gebruik voor een gewone gegevenscontainer **`Vector<T>`**.

| Wat je wilt | CL | typelisp |
|---|---|---|
| Kop en rest van een S-expressie | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Door een S-expressielijst lopen | `(dolist (x xs) ...)` | Hetzelfde |
| Een reeks elementen van één type | Een lijst of een vector | `Vector<T>` |
| Een paar | `(cons a b)` | `(cons a b)` (het type is `cons-cell<A,B>`) |
| Afbeelden | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` zijn de accessors van het paar `cons-cell<A,B>` dat met `cons` is gemaakt. Ze kunnen
niet op S-expressielijsten worden gebruikt.

Een `Vector` maken:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Sequentiefuncties zoals `map`, `filter`, `sort` en `find` werken op waarden die de trait `Iter`
implementeren. Geef een `Vector` door nadat je hem met `(iter v)` tot iterator hebt gemaakt.

## 5. Er zijn geen meervoudige waarden

Er is geen `values` en geen `multiple-value-bind`. Functies die in CL meerdere waarden teruggeven,
geven een paar of een struct terug.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → een `cons-cell` waarvan `car` 3 is en `cdr` 1 |
| `(decode-universal-time t)` → 9 waarden | Een struct `decoded-time` |
| `(read-from-string s)` → waarde, positie | `(read-from-string s)` geeft binnen een `Result` een `cons-cell` van waarde en positie terug. Voor alleen de waarde: `(read s)` |

## 6. Er zijn geen speciale variabelen (dynamische binding)

`let` bindt altijd lexicaal. Als je een variabele die met `defvar` is gedefinieerd met `let` opnieuw
bindt, zien functies die vanaf daar worden aangeroepen nog steeds de oorspronkelijke waarde.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 in CL, 1 in typelisp
```

Om een besturingsvariabele zoals `*print-base*` tijdelijk te wijzigen, gebruik je `dlet`. Het wijst de
waarde toe en herstelt de oorspronkelijke waarde, hoe de body ook wordt verlaten.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` herschrijft de globale variabele zelf, dus het is geen binding per thread.

## 7. Het conditiesysteem is niet overgenomen

Er is geen `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` of `signal`.
Ze passen slecht bij statische typering. In plaats daarvan worden deze twee voor verschillende doelen
gebruikt:

- **Herstelbare fouten geven `Result<T,E>` terug.** De aanroeper scheidt `ok` / `err` met `match`.
  Er is geen afkorting zoals `?` in Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Niet-herstelbare fouten (bugs) zijn `panic`.** `(panic "message")`, `none` aan `unwrap` geven,
  delen door 0 en een index buiten het bereik zijn van deze soort, en het programma stopt. De
  opruiming van `unwind-protect` wordt uitgevoerd voordat het stopt.

Foutentypes worden verenigd door de trait `Error`, en `(message e)` geeft de melding. Hoe je een
eigen foutentype maakt staat in
[Option, Result en foutentypes](../reference/functions/option-result.md#3-foutentypes-en-de-trait-error).
`assert` en `warn` kunnen worden gebruikt zoals in CL.

`catch` / `throw` / `unwind-protect` bestaan. De tag van `catch` is echter beperkt tot een
ongeëvalueerd letterlijk symbool (`'done`), en de waarden die met één tag worden gegooid hebben één
type.

## 8. Er is geen CLOS

Er is geen `defclass`, `defgeneric` of methodecombinatie.

- Gegevenstypes worden gedefinieerd met `defstruct` (structs) en `defenum` (somtypes).
- `defmethod` definieert methoden waarvan het doel **alleen door het statische type van het eerste
  argument** wordt bepaald. Er is geen meervoudige dispatch.
- Gebruik traits (`deftrait` / `impl`) om bewerkingen te geven die over types heen gemeenschappelijk
  zijn. Gebruik voor waarden waarvan het concrete type tijdens runtime wordt bepaald het type
  `:dyn Trait`
  ([Syntaxreferentie 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

Hoe `defstruct` verschilt:

- De constructor is `TypeName::new`: `(point::new 1 2)`. Wil je een naam als `make-point`, dan maakt
  de optie `(:constructor make-point)` er een.
- Naast `(x p)` kan een accessor als `p::x` worden geschreven. Wijzig met `(setf p::x 5)`.
- Er wordt geen predicaat (`point-p`) gemaakt. Er is geen `:conc-name`, `:type` of `:named`.
- `:include` erft alleen slots; het type wordt geen subtype van de ouder.

## 9. Modules in plaats van packages

Er zijn geen packages. Namespaces zijn modules, en een bestand is zelf een module. Schrijf in plaats
van `pkg:symbol` `module::name`, en haal namen binnen met `use`
([Modules en bestandsindeling](modules.md)).

Keywords `:foo` bestaan en zijn symbolen die naar zichzelf evalueren. Omdat er geen packages zijn,
maakt de dubbele punt deel uit van de naam: `(symbol->string :foo)` geeft `":foo"` terug.

## 10. Verschillen in lezen en syntaxis

- Hoofdletters en kleine letters worden niet onderscheiden (symbolen worden bij het lezen kleine
  letters). Dit is hetzelfde als in CL.
- Er is geen `#'` (paragraaf 3). Letterlijke complexe getallen `#c(...)` kunnen niet worden gelezen;
  maak complexe getallen met `(complex 1.0 2.0)`.
- De clausules van de uitgebreide `loop` worden met keywords geschreven:
  `(loop :for i :from 1 :to 3 :collect i)`. Een `loop` die niet met een keyword begint is een
  eenvoudige oneindige lus, die met `(break)` of `(return value)` wordt verlaten. `return` verlaat de
  binnenste lus (gebruik `return-from` om een functie te verlaten).
- De bestemming van `format` is `false` (geef een string terug), `true` (standaarduitvoer) of een
  stream. De formatdirectieven zijn dezelfde als in CL.
- Lezen uit een string is `(read "...")`, en lezen uit een stream is `(read-sexpr s)`. Beide geven een
  `Result` terug.
- `eval` typecheckt de gegeven expressie voordat hij haar evalueert, en geeft een `Result` terug.
  Voorwaartse verwijzingen zijn niet mogelijk, net als in broncode.
- Er is geen `eval-when`.
- Functienamen gebruiken geen achtervoegsels `?` of `!`. Predicaten krijgen een naam met `-p` / `p`
  zoals in CL (`zerop`, `sexpr-null`), of met `is-` ervoor (`is-some`).

## 11. Belangrijkste functies met een andere naam

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (uit een stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versies met twee argumenten van `floor` en verwanten | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (omgekeerde argumentvolgorde; paragraaf 4) |
| `length` (van een vector) | `len` |
| `hash-table-count` | `count` / `size` |

De lijst met functies staat in [Ingebouwde functies](../reference/functions/README.md).

## 12. Andere dingen die niet bestaan

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` en `copy-readtable`, `readtable-case` (readermacro's zelf kunnen met
  `set-macro-character` worden gedefinieerd)
- Logische padnamen en padnamen met jokertekens
- `input-stream-p` / `output-stream-p` (de richting van een stream wordt door zijn type bepaald)
