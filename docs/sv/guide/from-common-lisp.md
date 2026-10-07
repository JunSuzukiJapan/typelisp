<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# För Common Lisp-programmerare

typelisp ärver syntaxen och många av funktionsnamnen från Common Lisp (CL), men är ett statiskt typat
språk. Därför fungerar CL-kod inte alltid som den står. Den här guiden samlar de punkter där den som är
van vid CL tenderar att snubbla, tillsammans med hur man skriver om koden.

## 1. Det finns inget `nil` eller `t`

De booleska värdena är `true` och `false`. `nil` och `t` är inte definierade.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Bara en `bool` kan vara villkor.** Att skriva `0` eller en tom lista som villkor är ett typfel. Det
  finns ingen regel om att "allt utom nil är sant".
- **Else-grenen i `if` kan inte utelämnas.** `(if c x)` är ett fel. När ingen else-gren behövs används
  `when` / `unless`.
- **"Inget värde" uttrycks med `Option<T>`.** En funktion som i CL returnerade nil för att betyda "hittades
  inte" returnerar här `(some x)` eller `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Den tomma listan `()` är, beroende på sammanhang, antingen värdet av typen Unit (returvärdet från en
  funktion som inte returnerar något) eller den tomma listan av S-uttrycksdata. Det är ett annat värde än
  `false`.

## 2. Att skriva typer

Funktionsargument och returvärden måste ha typer.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; en generisk funktion
  (unwrap-or (first (iter v)) default))
```

- En definition utan typer, som `(defun f (x) x)`, kan inte skrivas.
- Globala variabler som `defvar` behöver också en typ: `(defvar (count int) 0)`.
- `the` är inte en kontroll vid körning utan en annotering åt typkontrollen.
- **Det går inte att inspektera typer vid körning.** Det finns inget `typep` eller `type-of`, eftersom
  typen på varje värde är fast vid kompileringen. För att ta emot en av flera typer gör man en
  summatyp med `defenum` eller använder ett trait.
- `deftype` definierar ett typalias. En typ som beskriver ett värdeintervall, som
  `(deftype small () '(integer 0 9))`, kan inte göras.

Standardtypen för heltal, `int`, har godtycklig precision; liksom CL:s integer finns det ingen övre gräns
för dess storlek. Typerna med fast bredd `i8` till `i32` och `u8` till `u32` finns också. Det finns ingen
64-bitars heltalstyp med fast bredd.

## 3. Funktioner som värden

typelisp skiljer inte på namnrymderna för funktioner och variabler. En funktions namn kan skickas som ett
värde som det är. Det finns inget `#'` och inget `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; anropa den direkt, inte med funcall

(apply-to twice 5)                        ; twice, inte #'twice
```

- Inbyggda funktioner som `+` och `1+` kan också skickas som värden som de är, där argumentets typ är
  fast, som i `(fn (int) int)`. När man skickar en till en generisk funktion som `foldl` eller `map` är
  det inte känt vilken typs `+` som menas, så omslut den i en `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Sekvensfunktioner tar **samlingen först och funktionen sedan**: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. Det är tvärtom mot CL:s `(mapcar f list)`.
- `lambda` kan inte använda `&optional` eller `&key` (`&rest` kan användas).
- **En funktion kan inte anropas före sin definition.** I CL kan du anropa en funktion du definierar
  senare, men här ger det `no such function`. För ömsesidigt rekursiva funktioner deklarerar man en av
  dem med `defsignature` först.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listor och Vector

Det som motsvarar en CL-lista är **S-uttrycksdata**, vars typ är `Option<Sexpr>` (den tomma listan är
`none`). `(list 1 2 3)` och `'(a b c)` har den här typen. S-uttrycksdata är något som makron och `read`
arbetar med; för en vanlig databehållare används **`Vector<T>`**.

| Vad du vill ha | CL | typelisp |
|---|---|---|
| Huvud och rest av ett S-uttryck | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Gå igenom en lista av S-uttryck | `(dolist (x xs) ...)` | Samma |
| En följd av element av en typ | En lista eller en vektor | `Vector<T>` |
| Ett par | `(cons a b)` | `(cons a b)` (dess typ är `cons-cell<A,B>`) |
| Avbildning | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` är accessorerna för paret `cons-cell<A,B>` som skapas med `cons`. De kan inte användas på
listor av S-uttryck.

Att skapa en `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Sekvensfunktioner som `map`, `filter`, `sort` och `find` fungerar på värden som implementerar traitet
`Iter`. Skicka en `Vector` efter att ha gjort den till en iterator med `(iter v)`.

## 5. Det finns inga flera värden

Det finns inget `values` och inget `multiple-value-bind`. Funktioner som i CL returnerar flera värden
returnerar ett par eller en struct.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → en `cons-cell` vars `car` är 3 och `cdr` är 1 |
| `(decode-universal-time t)` → 9 värden | En `decoded-time`-struct |
| `(read-from-string s)` → värde, position | `(read-from-string s)` returnerar en `cons-cell` av värde och position inuti ett `Result`. För bara värdet, `(read s)` |

## 6. Det finns inga speciella variabler (dynamisk bindning)

`let` binder alltid lexikalt. Om du binder om en variabel som definierats med `defvar` med `let` ser
funktioner som anropas därifrån fortfarande det ursprungliga värdet.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 i CL, 1 i typelisp
```

För att tillfälligt ändra en kontrollvariabel som `*print-base*` används `dlet`. Det tilldelar värdet och
återställer det ursprungliga hur kroppen än lämnas.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` skriver om själva den globala variabeln, så det är inte en bindning per tråd.

## 7. Conditionsystemet är inte antaget

Det finns inget `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` eller
`signal`. De passar dåligt med statisk typning. I stället används dessa två för olika syften:

- **Återhämtningsbara misslyckanden returnerar `Result<T,E>`.** Anroparen skiljer `ok` / `err` med
  `match`. Det finns ingen genväg som Rusts `?`.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Icke återhämtningsbara misslyckanden (buggar) är `panic`.** `(panic "message")`, att skicka `none`
  till `unwrap`, division med 0 och ett index utanför intervallet är av det slaget, och programmet
  stoppar. Uppstädningen i `unwind-protect` körs innan det stoppar.

Feltyper förenas av traitet `Error`, och `(message e)` ger meddelandet. Hur man gör en egen feltyp
beskrivs i
[Option, Result och feltyper](../reference/functions/option-result.md#3-feltyper-och-traitet-error).
`assert` och `warn` kan användas som i CL.

`catch` / `throw` / `unwind-protect` finns. Taggen i `catch` är dock begränsad till en onekvaliserad
literal symbol (`'done`), och de värden som kastas med en tagg har en enda typ.

## 8. Det finns inget CLOS

Det finns inget `defclass`, `defgeneric` eller metodkombination.

- Datatyper definieras med `defstruct` (structs) och `defenum` (summatyper).
- `defmethod` definierar metoder vars mål avgörs enbart av **den statiska typen på det första
  argumentet**. Det finns ingen multipel dispatch.
- För att ge operationer gemensamt över typer används traits (`deftrait` / `impl`). För värden vars
  konkreta typ avgörs vid körning används typen `:dyn Trait`
  ([Syntaxreferens 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

Hur `defstruct` skiljer sig:

- Konstruktorn är `TypeName::new`: `(point::new 1 2)`. Om du vill ha ett namn som `make-point` skapar
  flaggan `(:constructor make-point)` ett.
- Förutom `(x p)` kan en accessor skrivas `p::x`. Ändra den med `(setf p::x 5)`.
- Inget predikat (`point-p`) skapas. Det finns inget `:conc-name`, `:type` eller `:named`.
- `:include` ärver bara slots; typen blir inte en subtyp till föräldern.

## 9. Moduler i stället för paket

Det finns inga paket. Namnrymder är moduler, och en fil är en modul i sig. I stället för `pkg:symbol`
skriver man `module::name`, och hämtar in namn med `use` ([Moduler och filuppdelning](modules.md)).

Keywords `:foo` finns och är symboler som utvärderas till sig själva. Eftersom det inte finns paket är
kolonet en del av namnet: `(symbol->string :foo)` returnerar `":foo"`.

## 10. Skillnader i läsning och syntax

- Stora och små bokstäver skiljs inte åt (symboler blir gemena vid läsning). Det är samma som i CL.
- Det finns inget `#'` (avsnitt 3). Literaler för komplexa tal, `#c(...)`, kan inte läsas; gör komplexa
  tal med `(complex 1.0 2.0)`.
- Satserna i den utökade `loop` skrivs med keywords: `(loop :for i :from 1 :to 3 :collect i)`. En
  `loop` som inte börjar med ett keyword är en enkel oändlig slinga, som lämnas med `(break)` eller
  `(return value)`. `return` lämnar den innersta slingan (för att lämna en funktion används
  `return-from`).
- Målet för `format` är `false` (returnera en sträng), `true` (standard ut) eller en ström.
  Formatdirektiven är desamma som i CL.
- Att läsa från en sträng är `(read "...")`, och att läsa från en ström är `(read-sexpr s)`. Båda
  returnerar ett `Result`.
- `eval` typkontrollerar det givna uttrycket innan det utvärderas och returnerar ett `Result`.
  Framåtreferenser är inte möjliga, precis som i källkod.
- Det finns inget `eval-when`.
- Funktionsnamn använder inte ändelserna `?` eller `!`. Predikat namnges med `-p` / `p` som i CL
  (`zerop`, `sexpr-null`), eller med `is-` framför (`is-some`).

## 11. Huvudfunktioner med andra namn

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (från en ström) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versioner med två argument av `floor` och liknande | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (argumentordningen omvänd; avsnitt 4) |
| `length` (för en vektor) | `len` |
| `hash-table-count` | `count` / `size` |

Listan över funktioner finns i [Inbyggda funktioner](../reference/functions/README.md).

## 12. Andra saker som inte finns

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` och `copy-readtable`, `readtable-case` (själva läsarmakron kan definieras med
  `set-macro-character`)
- Logiska pathnames och pathnames med jokertecken
- `input-stream-p` / `output-stream-p` (en ströms riktning avgörs av dess typ)
