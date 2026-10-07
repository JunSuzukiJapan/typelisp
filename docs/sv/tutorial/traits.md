<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Traits

Ett trait är ett löfte om att "den här typen stöder dessa operationer". Med traits kan flera typer dela
operationer med samma namn, så att en funktion som använder dem inte behöver skrivas en gång per typ.
De fungerar nästan exakt som Rusts traits. Det här kapitlet förutsätter att du har läst
[Grunderna i typer](types.md).

## 1. Definiera och implementera ett trait

Definiera de operationer som returnerar en figurs area och namn som traitet `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` efter traitets namn är listan över de traits det ärver från (avsnitt 4). Lämna den tom när det
  inte finns några.
- Varje rad deklarerar en metod. `Self` står för "typen som implementerar det här traitet".

För att implementera ett trait för en typ skriver man en `impl`.

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

De implementerade metoderna anropas precis som vanliga funktioner.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Att utelämna även bara en av de metoder traitet deklarerar är ett typfel vid `impl`.

## 2. Trait-gränser: "vilken typ som helst som implementerar det här traitet"

Man kan ställa ett villkor på en generisk funktions typparameter med `where`. Det kallas en
**trait-gräns**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Tack vare `(where (Shape T))` kan kroppen använda `name` och `area` på värden av typen `T`. Utan gränsen
är ingenting känt om `T`, så de skulle inte gå att anropa.

Att skicka en typ som inte implementerar `Shape` är ett typfel vid anropet.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

En generisk funktion får en egen kopia för varje typ den anropas med. Inga typtester eller
förgreningar vid körning ingår.

## 3. Standardimplementationer

Om en trait-metod har en kropp används den kroppen när en `impl` utelämnar metoden.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe är standardvarianten

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; den som skrivs här har företräde

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementera standardtraits

Standardbiblioteket har också traits. Implementerar man ett gör det de standardfunktioner som använder
det tillgängliga för din typ.

| Trait | Metoder att implementera | Vad det möjliggör |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, mönstret `(= expr)` i `match` med flera |
| `Ord` | `less` | `less-equal`, `greater` med flera. `Ord` ärver från `Eq` |
| `print-object` | `print-object` | Hur värden visas av `println` och liknande |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` med flera |
| `Error` | `message`, `source` | Användning som feltyp ([Felhantering](errors.md)) |

Vi implementerar `Eq` och `Ord` för en typ som representerar ett penningbelopp. Eftersom `Ord` ärver
från `Eq` måste `impl` för `Eq` komma först.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (standardimplementationen i Ord)
```

Att implementera `print-object` avgör hur `println` visar värdet. Argumentet `escape` är `true` när en
form som kan läsas tillbaka efterfrågas, som med `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Kombinerat med en trait-gräns kan du skriva en funktion som fungerar för vilken typ som helst som
implementerar `Ord`.

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

Ges en `Vector` med `money`-värdena 300, 900 och 100 i den ordningen returnerar den `(some 900 yen)`.

## 5. `:dyn`: hantera värden av olika typer tillsammans

Alla element i en `Vector<T>` har samma typ, så `circle`- och `rect`-värden kan inte läggas i en och
samma `Vector<circle>`. För att hantera "något som implementerar `Shape`" tillsammans används typen
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

- Ett `circle`- eller `rect`-värde som placeras där en `:dyn Shape` förväntas konverteras automatiskt.
- Vilken typs `area` som anropet `(area s)` kör avgörs vid körning av typen på det `s` håller.
- Att placera ett värde vars typ inte implementerar `Shape` där en `:dyn Shape` förväntas är ett
  typfel.

Valet mellan trait-gränserna i avsnitt 2 och `:dyn`:

| | Trait-gräns (`where`) | `:dyn Trait` |
|---|---|---|
| När den anropade metoden avgörs | Före körning | Vid körning |
| Blanda typer i en `Vector` | Inte möjligt | Möjligt |
| Användbara typer | Ingen begränsning | Structs, enums, `int`, `string`, `f64` med flera (inte `bool`, `char`, `symbol`, `i32` och liknande) |

Den exakta listan över typer som kan användas finns i
[Syntaxreferens 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Vissa traits kan inte användas med `:dyn`: de vars metoder använder `Self` för ett annat argument än
`self` eller för returvärdet (som `equals` i `Eq`). Eftersom typen inte är känd förrän vid körning går
det inte att skapa "ett värde av samma typ".

## 6. Begränsningar

- Håll ett traits definition, `impl`erna för det och koden som använder det via `:dyn` i en modul
  (fil). Ett trait kan ännu inte göras synligt för andra moduler.
- Typer och traits delar en namnrymd. Inom en modul kan en typ och ett trait inte ha samma namn.

## 7. Vad man läser härnäst

- [Makron](macros.md): att definiera egen syntax
- [Syntaxreferens 3.9](../reference/syntax.md#39-deftrait--impl--traits): generella implementationer
  (blanket implementations), associerade typer med mera
- [Standardtraits](../reference/functions/traits.md): listan över traits i standardbiblioteket
