<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Traits

Een trait is een belofte dat "dit type deze bewerkingen ondersteunt". Met traits kunnen meerdere
types bewerkingen met dezelfde naam delen, zodat een functie die ze gebruikt niet per type opnieuw
hoeft te worden geschreven. Ze werken vrijwel precies zoals de traits van Rust. Dit hoofdstuk gaat
ervan uit dat je [Basis van types](types.md) hebt gelezen.

## 1. Een trait definiëren en implementeren

Definieer de bewerkingen die de oppervlakte en de naam van een vorm teruggeven als de trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- De `()` na de traitnaam is de lijst met traits waarvan hij erft (paragraaf 4). Laat hem leeg als
  die er niet zijn.
- Elke regel declareert een methode. `Self` staat voor "het type dat deze trait implementeert".

Om een trait voor een type te implementeren, schrijf je een `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

De geïmplementeerde methoden worden net als gewone functies aangeroepen.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Ook maar één van de methoden die de trait declareert weglaten is een typefout bij de `impl`.

## 2. Trait bounds: "elk type dat deze trait implementeert"

Met `where` kun je een voorwaarde stellen aan de typeparameter van een generieke functie. Dit heet
een **trait bound**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Dankzij `(where (Shape T))` kan de body `name` en `area` op waarden van `T` gebruiken. Zonder de
bound is niets over `T` bekend, dus ze konden niet worden aangeroepen.

Een type doorgeven dat `Shape` niet implementeert is een typefout bij de aanroep.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Een generieke functie krijgt een eigen kopie voor elk type waarmee ze wordt aangeroepen. Er zijn geen
typetests of vertakkingen tijdens runtime bij betrokken.

## 3. Standaardimplementaties

Als een traitmethode een body heeft, wordt die body gebruikt wanneer een `impl` de methode weglaat.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Standaardtraits implementeren

De standaardbibliotheek heeft ook traits. Er een implementeren maakt de standaardfuncties die hem
gebruiken beschikbaar voor je type.

| Trait | Te implementeren methoden | Wat het mogelijk maakt |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, het patroon `(= expr)` van `match`, enzovoort |
| `Ord` | `less` | `less-equal`, `greater` enzovoort. `Ord` erft van `Eq` |
| `print-object` | `print-object` | Hoe waarden door `println` en verwanten worden getoond |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` enzovoort |
| `Error` | `message`, `source` | Gebruik als foutentype ([Foutafhandeling](errors.md)) |

Laten we `Eq` en `Ord` implementeren voor een type dat een geldbedrag voorstelt. Omdat `Ord` van `Eq`
erft, moet de `impl` van `Eq` eerst komen.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

`print-object` implementeren bepaalt hoe `println` de waarde toont. Het argument `escape` is `true`
wanneer om een vorm wordt gevraagd die weer kan worden ingelezen, zoals bij `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Gecombineerd met een trait bound kun je een functie schrijven die werkt voor elk type dat `Ord`
implementeert.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Voor een `Vector` met `money`-waarden van 300, 900 en 100 in die volgorde geeft ze `(some 900 yen)`
terug.

## 5. `:dyn`: waarden van verschillende types samen behandelen

Alle elementen van een `Vector<T>` hebben hetzelfde type, dus `circle`- en `rect`-waarden kunnen niet
in één `Vector<circle>`. Om "iets dat `Shape` implementeert" samen te behandelen, gebruik je het type
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Een `circle`- of `rect`-waarde die wordt geplaatst waar een `:dyn Shape` wordt verwacht, wordt
  automatisch geconverteerd.
- Welke `area` van welk type de aanroep `(area s)` uitvoert, wordt tijdens runtime bepaald door het
  type van wat `s` bevat.
- Een waarde plaatsen waarvan het type `Shape` niet implementeert waar een `:dyn Shape` wordt
  verwacht, is een typefout.

Kiezen tussen de trait bounds van paragraaf 2 en `:dyn`:

| | Trait bound (`where`) | `:dyn Trait` |
|---|---|---|
| Wanneer de aangeroepen methode wordt bepaald | Voor het uitvoeren | Tijdens runtime |
| Types mengen in één `Vector` | Niet mogelijk | Mogelijk |
| Bruikbare types | Geen beperking | Structs, enums, `int`, `string`, `f64` en andere (niet `bool`, `char`, `symbol`, `i32` en dergelijke) |

De exacte lijst met types die kunnen worden gebruikt staat in
[Syntaxreferentie 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Sommige traits kunnen niet met `:dyn` worden gebruikt: die waarvan de methoden `Self` gebruiken voor
een ander argument dan `self` of voor de returnwaarde (zoals `equals` in `Eq`). Omdat het type pas
tijdens runtime bekend is, is er geen manier om "een waarde van hetzelfde type" te produceren.

## 6. Beperkingen

- Houd de definitie van een trait, de `impl`s ervoor en de code die hem via `:dyn` gebruikt in één
  module (bestand). Een trait kan nog niet voor andere modules zichtbaar worden gemaakt.
- Types en traits delen één namespace. Binnen één module kunnen een type en een trait niet dezelfde
  naam hebben.

## 7. Wat je hierna kunt lezen

- [Macro's](macros.md): je eigen syntaxis definiëren
- [Syntaxreferentie 3.9](../reference/syntax.md#39-deftrait--impl--traits): blanket-implementaties,
  geassocieerde types en meer
- [Standaardtraits](../reference/functions/traits.md): de lijst met traits in de standaardbibliotheek
