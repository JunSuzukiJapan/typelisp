<!-- translated-from: docs/ja/tutorial/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Grunderna i typer

typelisp är ett statiskt typat språk. Det här kapitlet förklarar vad typkontrollen gör åt dig, de typer
du använder mest (`Option`, `Result`, structs och enums) samt generiska typer. Det förutsätter att du
har läst [Komma igång](intro.md).

## 1. Vad statisk typning innebär

I typelisp är typen på varje uttryck avgjord innan programmet körs. Ett uttryck vars typer inte passar
ihop är ett fel innan något alls körs.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; typfel

(main)
```

Att köra den här filen stoppar med ett typfel utan att ens skriva ut `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Du behöver skriva typer för funktionsargument och returvärden, globala variabler och fält i structs.
Typen på en `let`-variabel hämtas från dess startvärde.

De viktigaste typerna:

| Typ | Exempelvärden |
|---|---|
| `int` | `42`, `-7` (heltal med godtycklig precision) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Heltal med fast bredd |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Returtypen för en funktion som inte returnerar något värde |

Det går inte att fråga efter ett värdes typ vid körning (inga `typep` eller `type-of` som i Common
Lisp), eftersom varje typ redan är känd innan programmet körs.

## 2. `Option<T>`: ett värde som kan saknas

typelisp har inget `nil`. "Det kanske inte finns något värde" uttrycks med typen `Option<T>`. Ett värde
av typen `Option<T>` är antingen `some`, som håller ett värde av typen `T`, eller `none`, som inte håller
något.

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

`Option<int>` är inte `int`, så det kan inte användas i aritmetik som det är. `(+ (safe-div 10 2) 1)` är
ett typfel. För att använda innehållet skiljer man `some` från `none` med `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- I grenen `(some q)` binds innehållet till variabeln `q`.
- `match` kontrollerar att dess grenar **täcker alla fall**. Att glömma grenen `(none)` är ett typfel.

### Varför det inte finns något nil

I många språk kan `nil` (`null`) stå i stället för ett värde av vilken typ som helst. Följden är att man
inte märker att man glömt hantera fallet "inget värde" förrän programmet körs. I typelisp har en plats
där ett värde kan saknas typen `Option<T>`, och koden passerar inte typkontrollen om inte `match`
hanterar fallet `none`. Ett glömt fall upptäcks innan programmet körs.

Villkor följer samma tanke: bara en `bool` kan vara villkor i `if`. Det finns ingen regel som Common
Lisps "allt utom `nil` är sant".

### Vanliga operationer

| Form | Betydelse |
|---|---|
| `(unwrap-or opt default)` | Innehållet för `some`; standardvärdet för `none` |
| `(unwrap opt)` | Tar ut innehållet. Stoppar programmet vid `none` |
| `(is-some opt)` / `(is-none opt)` | Testar vilket av dem det är |

Många funktioner i standardbiblioteket returnerar `Option`. Till exempel returnerar `position`
positionen i `some` om elementet hittas och `none` om det inte gör det.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: en operation som kan misslyckas

En operation som kan misslyckas returnerar `Result<T,E>`: `ok` med ett värde av typen `T` vid lyckat
resultat, eller `err` med ett fel `E` vid misslyckande.

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

Dina egna funktioner kan också returnera `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Använd `Option` när ett saknat värde inte behöver någon förklaring, och `Result` när du vill säga varför
något misslyckades. [Felhantering](errors.md) går in på hur man hanterar fel i detalj.

## 4. `defstruct`: structs

En typ med namngivna fält definieras med `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

Definitionen ger dig följande:

```lisp
(let ((p (point::new 3 4)))     ; skapa en (argumenten i fältens ordning)
  (println "~a" p::x)           ; läs ett fält; (x p) fungerar också
  (setf p::x 10)                ; ändra det
  (println "~a" p))             ; #<point x: 10 y: 4>
```

För att ge en struct egna funktioner används `defmethod`. Typen på det första argumentet (`self`)
avgör vilken typ metoden tillhör.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Om man skriver bara typnamnet i stället för ett `self`-argument blir det en funktion som anropas som
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: en av flera former

Ett värde som är en av flera former, som "en cirkel, en rektangel eller en punkt", definieras med
`defenum`. Varje form kallas en **variant**. Varje variant kan hålla ett olika antal och olika typer av
värden.

```lisp
(defenum shape
  (circle int)        ; radie
  (rect int int)      ; bredd och höjd
  (dot))              ; håller inget värde
```

Värden skapas med typnamnet framför, som i `shape::circle`. I `match` plockas de isär efter
variantnamn.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Också här kontrollerar `match` att alla fall är täckta. Om du senare lägger till en variant i `shape`
blir varje `match` som inte hanterar den ett typfel, så ingen plats som behöver rättas missas.

Efter `(use shape)` kan du skriva `(rect 5 6)` utan typnamnet.

`Option` och `Result` är enums som byggts med samma mekanism.

## 6. Generiska typer

En funktion som fungerar för vilken typ som helst definieras med en **typparameter** `<T>` efter sitt
namn.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Du anger inte typen när du anropar den. `T` räknas ut från argumenten.

```lisp
(first-or ints 7)          ; T är int
(first-or names "none")    ; T är string
(first-or ints "none")     ; typfel: ints är en Vector<int>, så T är int
```

Structs och enums kan också vara generiska. `Vector<T>`, `Option<T>` och `Result<T,E>` är typer av det
slaget.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Inuti en generisk funktion vet man ingenting om `T`, så man kan inte jämföra eller addera värden av
typen `T`. För att kräva något i stil med "vilken typ som helst som kan jämföras" använder man traits
([Traits](traits.md)).

## 7. Ge en typ ett annat namn

`deftype` ger en typ ett annat namn.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` är bara ett annat sätt att skriva `int`, inte en ny typ. Att skicka ett vanligt `int` där
`meters` förväntas är inget fel. Vill du hålla dem isär gör du en struct, som i
`(defstruct meters (value int))`.

## 8. Vad man läser härnäst

- [Traits](traits.md): att ge typer gemensamma operationer
- [Typer](../reference/types.md): de inbyggda typerna och de traits som var och en implementerar
