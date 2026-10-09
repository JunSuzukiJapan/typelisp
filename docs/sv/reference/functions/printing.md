<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Utskrift

`print`/`println`/`format`, utskriftsfunktionerna med ett argument, den snygga skrivaren (pretty printer),
`print-object` och variablerna som styr utskrift. Listan över formatdirektiv finns i
[format.md](format.md). Läsning från och skrivning till strömmar finns i
[Strömmar och filer](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` är alla **specialformer som tolkar formatdirektiv (CL:s `format`-direktiv)**.
Det första argumentet (det andra för `format`) är **kontrollsträngen**, och varje direktiv förbrukar de
följande variadiska argumenten i tur och ordning.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expanderar kontrollsträngen och skriver den till standard ut utan radbyte |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Detsamma, med ett radbyte på slutet |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL:s `format`. Returnerar den expanderade strängen. Om `dest` är `true` (CL:s `t`) skrivs den också till standard ut; om `false` (CL:s `nil`) skrivs den inte utan bara returneras |
| `format` (till en ström) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Om `dest` inte är en `bool` är det CL:s strömmål. Den expanderade strängen skrivs till den strömmen. Returvärdet är `()` (CL:s `nil`), och ingen sträng returneras |

Typen på `dest` delar betydelsen i två (vilken som gäller avgörs statiskt). Strömformen kan skrivas på
samma sätt med en konkret strömtyp, en `:dyn CharOutput` eller en typvariabel bunden av
`(where (CharOutput S))`. Ett `dest` som varken är en `bool` eller en ström är ett typfel.

**Kontrollsträngen måste vara en literal** (samma begränsning som Rusts `format!`). Direktiven i den avgör
hur många argument som tas och av vilka typer, så en sträng som byggs vid körning kan inte läsas vid
kontrolltillfället. Eftersom den måste vara en literal **kontrolleras antalet och typerna på argumenten
vid kontrolltillfället**: `(println "~d" "x")` och `(println "~a ~a" 1)` är fel vid kontrolltillfället.
Ett felstavat direktiv, ett oavslutat `~(` och ett `~/name/` som inget argument kan svara på är också fel
vid kontrolltillfället. Kontrollreglerna finns i
[format.md](format.md#1-hur-direktiv-skrivs). För att skriva ut en sträng du bygger, gör den med
`(format false ...)` och skriv ut den med `(println "~a" s)`.

De variadiska argumenten omsluts i `Sexpr` med sina egna typer innan de skickas: `i32`/`f64`/`int`/`ratio`/
`char`/`bool`/`string`/`Sexpr`, liksom användardefinierade `defstruct`/`defenum`/`Vector<T>`/
`HashTable<K,V>` och liknande, kan alla skickas som de är (`(println "~a" my-struct)` fungerar bara).

Att köra ett skript med `typl file.typl` **skriver inte ut värdena av toppnivåuttryck**, så ett program
skriver till standard ut genom att anropa dessa. `print`/`println`/`format` skickar ut sin utdata vid varje
anrop (så att en prompt syns innan standard in läses, även genom en pipe).

**`Option<Sexpr>` skrivs ut transparent.** Typen för S-uttrycksdata är `Option<Sexpr>`, så omslaget
`(some x)` syns inte i utdata och innehållet skrivs ut som det är. Den tomma listan skrivs ut som `()`.
Andra `Option<T>` skrivs ut som `(some ...)` / `none`. Detsamma gäller `Option<T>`-fält inuti structs, enums
och `Vector`. Ett `Result<Option<Sexpr>,…>` från `(eval ...)` skrivs ut som `(ok 42)`, eller `(ok ())` för
`none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; få bara strängen, utan att skriva ut
  (println "~a" s))                   ; => id=42
```

## 2. Utskriftsfunktioner med ett argument

Utskriftsfunktionerna i CLHS 22.1.3. I stället för att expandera ett format skriver de ut ett enda värde
som det är. Strömmen kan utelämnas (standard är `*standard-output*`).

| Namn | Form | Beskrivning |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Skriver i en form som kan läsas tillbaka (samma som `~s`) och returnerar `x` |
| `princ` | `(princ x [stream])` | Skriver i en form för människor (samma som `~a`) och returnerar `x` |
| `write` | `(write x [stream])` | `prin1` om `*print-escape*` är sant, `princ` om falskt. Returnerar `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Returnerar en sträng i stället för att skriva (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Detsamma (`~a`). Samma som `to-string` |
| `write-to-string` | `(write-to-string x)` | Detsamma, enligt `*print-escape*` |

`print`/`println` ingår **inte** bland dessa. De är genvägar för `format` som tar en kontrollsträng, ett
annat jobb än CL:s `print` (radbyte, sedan `prin1`, sedan ett mellanslag), så var och en behåller sitt eget
namn. Därför **har CL:s `print` med ett argument ingen stavning i det här språket**: skriv `prin1`.

Dessa är makron, eftersom de variadiska argumenten till `format` inte accepterar typvariabler och typen
måste vara känd på anropsstället.

## 3. Standard in och standardströmmarna

**Att läsa standard in** görs inte med dedikerade funktioner utan med `CharInput`-metoderna på
standardströmmen `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([strömmetoder](streams-files.md#2-metoder)). Standard ut och standard fel
har på samma sätt `*standard-output*` / `*error-output*`, och kan skrivas som i
`(write-line *standard-output* s)` (`print`/`println`/`format` är genvägar för när du behöver
formatexpansion, och skriver alltid till standard ut).

## 4. Den snygga skrivaren (pretty printer)

Detta motsvarar CL:s Lisp Pretty Printer (CLHS 22.2). **Den bryter utdata som inte ryms inom radbredden,
enligt logiska block och villkorliga radbyten.**

### 4.1 Kontrollvariabler

Globala variabler som kan tilldelas. När de väl är satta med `setf` påverkar de all senare utskrift. För att
ändra en tillfälligt används `dlet` (6.3).

| Variabel | Typ | Standard | Betydelse |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Om sant tar `~a`/`~s`/`~w` och de snygga direktiven vägen för snygg utskrift |
| `*print-right-margin*` | `int` | `80` | Högermarginalen (i kolumner). 0 betyder "ingen marginal, bryt aldrig". Ett negativt värde är ett utskriftsfel |
| `*print-miser-width*` | `int` | `0` | Bredden där miser-stilen börjar. 0 motsvarar CL:s `nil` (miser-stil av). Ett negativt värde är ett utskriftsfel |

Familjen `pprint` och `pprint-logical-block` skriver alltid snyggt oavsett `*print-pretty*` (enligt
definitionen av CL:s `pprint`).

### 4.2 Färdiga layouter (specialformer)

Liksom `print` är dessa specialformer, så argumentet kan ha vilken typ som helst.

| Namn | Form | Beskrivning |
|---|---|---|
| `pprint` | `(pprint x)` | Skriver snyggt med standardlayouten. Som i CL **skriver den ett radbyte först** och inget på slutet |
| `pprint-fill` | `(pprint-fill x)` | Fyller varje rad med så mycket som ryms. Skriver inget radbyte |
| `pprint-linear` | `(pprint-linear x)` | Om inte alla element ryms på en rad, **ett element per rad**. Skriver inget radbyte |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | En tabell med kolumner `colinc` breda (standard 16). Skriver inget radbyte. Ett negativt `colinc` är ett fel |

Standardlayouten (`pprint`, och `~a` under `*print-pretty*`) följer CL:s standard
`*print-pprint-dispatch*`: den förkortar `(quote x)` som `'x`, och formaterar kodformer som
`defun`/`let`/`if`/`lambda` som "huvudet och det föreskrivna antalet argument på första raden, och resten av
kroppen indragen två kolumner, en form per rad". Andra listor fylls.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Bygga logiska block själv

| Namn | Form | Beskrivning |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | En specialform som öppnar ett logiskt block. `obj` är listan `pprint-pop` går igenom (`()` om ingen går igenom). `:prefix` och `:per-line-prefix` utesluter varandra (som i CL) |
| `pprint-newline` | `(pprint-newline kind)` | Ett villkorligt radbyte. `kind` är `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Indrag. `kind` är `:block` (från blockets början) / `:current` (från aktuell kolumn) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | En tabb. `kind` är `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` och `colinc` är icke-negativa (ett fel om negativa) |
| `pprint-pop` | `(pprint-pop)` | Tar nästa element från blockets lista (`()` om den är uttömd) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Om listan är uttömd |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Om den är uttömd görs `break` ut ur den omslutande `loop` (ett makro) |

Logiska block tar inget strömargument: **ett öppet logiskt block är implicit tillstånd**. Det yttersta
`pprint-logical-block` startar det, och när det stängs formateras det hela och skrivs till standard ut på
en gång. Medan det är öppet går utdata från `print`/`println`/`(format true ...)`/`pprint` alla in i det
blocket, så **du skriver innehållet med vanlig `print` och markerar bara de ställen att bryta på med
`pprint-newline` och liknande**, vilket gör att koden ser nästan likadan ut som i CL.

I CL är `pprint-exit-if-list-exhausted` en icke-lokal utgång från `pprint-logical-block`; här är det **ett
`break` från den omslutande `loop`** (`pprint-logical-block` upprättar inget `block`). CL:s idiom lägger
det ändå alltid inuti en `loop`, så det läses på samma sätt.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Regler för villkorliga radbyten (CLHS `pprint-newline`):

- `:mandatory` bryter alltid.
- `:linear` bryter om det omslutande logiska blocket inte ryms på en rad. Beslutet är per block, så
  **alla `:linear`-radbyten i ett block bryts tillsammans** (det är "allt på en rad eller ett element per
  rad" för `pprint-linear`).
- `:fill` bryter om (a) nästa avsnitt inte ryms på resten av raden, (b) det föregående avsnittet inte
  rymdes på en rad, eller (c) i miser-stil, blocket inte ryms på en rad.
- `:miser` fungerar som `:linear` bara i miser-stil (när blocket börjar inom `*print-miser-width*` från
  högermarginalen).

## 5. `print-object` (utskriftsform per typ)

Att skriva `impl print-object <typ>` gör att `print`/`println`/`format`/`pprint` skriver ut värden av den
typen med den implementationen, **även när de är nästlade inuti listor**. Det motsvarar CL:s generiska
funktion `print-object` (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Betydelse |
|---|---|
| `self` | Värdet som ska skrivas ut |
| `escape` | CL:s `*print-escape*`. `true` för `~s`/`prin1`/`pprint` (en form som kan läsas tillbaka), `false` för `~a`/`princ` (för människor). En implementation som inte bryr sig kan ignorera det |

Den returnerade `string` går rakt in i utdata. Typer utan `impl` skrivs ut i den inbyggda representationen
(av formen `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   fungerar också nästlat
```

Den kombineras också med den snygga skrivaren (kapitel 4). Om `*print-pretty*` är sant bryts en lista som
innehåller strängarna implementationen returnerade vid högermarginalen.

Utskriftsformerna för standardbibliotekets typer. Typer som också finns i CL skrivs ut på samma sätt som i
SBCL. När REPL visar ett resultat använder den samma representation som `~s`.

| Typ | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Samma |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (numret är ett internt serienummer) | Samma |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Ett heltal (värdet av CL:s `get-universal-time` / `get-internal-real-time`) | Samma |
| Feltyper (`ParseIntError`, `SimpleError` och så vidare) | `#<simpleerror "boom">` | Bara meddelandet (`boom`) |
| `complex` | `#C(1.0 2.0)` | Samma |
| `Array<T>` | `#2A((0 0) (0 0))` | Samma |
| Strömmar | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Samma |
| Socketar | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Samma |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` sist under sommartid) | Samma |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Samma |
| `defstruct`-typer | `#<point x: 1 y: 2>` (fältnamn och värden) | Samma (fält med `~a`) |

Regler:

- **Registreringen är statisk.** En `impl` typkontrolleras som en vanlig metoddefinition, så ett
  felstavat typnamn eller en felaktig signatur är ett kompileringsfel.
- **Det fungerar också för generiska typer.** `(impl print-object box<T> (where (print-object T)) ...)`
  går till en separat kropp för varje typargument: ett värde kommer ihåg sin typ inklusive typargumenten
  (`box<i32>`). Inbyggda generiska typer som `Vector<T>` fungerar på samma sätt.
- **Valet görs vid utskrift.** Vilket direktiv som förbrukar vilket argument beror på kontrollsträngens
  innehåll vid körning, så skillnaden mellan `~a` och `~s` (det vill säga `escape`) är känd först i
  utskriftsögonblicket. Det är samma som i CLOS, där `print-object`-metoder "definieras per klass och
  väljs vid utskrift".
- **Återinträde faller tillbaka på den inbyggda representationen.** Om en implementation skriver ut sig
  själv med `(format false "~a" self)` skulle den rekursera för alltid, så när ett värde som skrivs ut
  dyker upp igen används den inbyggda representationen. Detta tittar på värdeidentitet, inte en
  djupgräns, så det kommer inte i vägen för att legitimt skriva ut nästlade självrefererande strukturer.
- **Varje skalär typ implementerar det här traitet.** Detta är **för att det ska kunna användas som en
  gräns**: de variadiska argumenten till `format` kan inte ta typvariabler, så den här gränsen är det enda
  sättet generisk kod kan säga "värden av en okänd typ får renderas" (samma form som Rusts `T: Display`).
  `print-object` för `Array<T>` är ett exempel.
- **Med typargument som inte uppfyller gränsen används den inbyggda representationen tyst.**
  `(impl print-object Array<T> (where (print-object T)))` gäller `Array<i32>`, men inte en `Array` vars
  element är en `defstruct` utan `print-object`. Det vore meningslöst om det att bara skapa en array vore ett
  fel, så det är inte ett fel.
- CL:s andra mekanism, `set-pprint-dispatch` / `*print-pprint-dispatch*` (ett register vid körning med
  typspecificerare som nycklar), **är inte antagen**. Dess registreringar är okontrollerade, vilket
  passar dåligt i ett statiskt typat språk.

## 6. Styra hur mycket som skrivs ut

### 6.1 Djup, längd och delning

Kontrollvariablerna i CLHS 22.1.1 som avgör "hur mycket av ett värde som skrivs ut". Liksom de tre i 4.1
är de tilldelningsbara globaler, och de gäller för alla `print`/`println`/`format`/`pprint`, oavsett om
`*print-pretty*` är sant eller inte.

| Variabel | Typ | Standard | Betydelse |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Objekt nästlade på det här djupet eller djupare ersätts av `#`. Objektet som skrivs ut är på djup 0. 0 betyder obegränsat |
| `*print-length*` | `int` | `0` | Skriver ut listelement (och fälten i `defstruct`/`defenum`-värden) upp till detta antal och ersätter resten med `...`. 0 betyder obegränsat |
| `*print-circle*` | `bool` | `false` | Om sant skannas värdet före utskrift och **objekt som förekommer två gånger eller fler får etiketter**. Den första förekomsten är `#n=…` och senare `#n#` |

CL använder `nil` för "obegränsat", men det här språket har inget `nil`, så som med `*print-right-margin*`
**betyder 0 obegränsat**. Negativa värden har ingen betydelse och är utskriftsfel. Standardvärdena är alla
"ingen gräns / inga etiketter", i enlighet med CL:s startvärden.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Cirkulära strukturer kan skrivas ut bara när `*print-circle*` är sant.** Om du skriver ut ett värde som
pekar på sig självt medan den är falsk (standard) fortsätter skrivaren följa cykeln och processen kraschar.
CL är likadant (CLHS lämnar utskrift av cirkulära strukturer odefinierad när `*print-circle*` är falsk).

En cykel kan bara göras genom att "peka ett `defstruct`-fält på sig självt med `setf`" (`Sexpr`-celler kan
inte ändras efter att de skapats, så en lista som `'(1 2 3)` kan aldrig vara cirkulär):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a pekar på a självt
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Etiketter **börjar om från 1 för varje sak som skrivs ut** (som i CL). Även utan cykel får samma objekt
`#1=`/`#1#` om det förekommer två gånger, vilket bevarar i utdata informationen att "dessa två är samma
objekt", som CL specificerar:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Ett värde utan delning **visar inga etiketter alls**, så att låta den här variabeln vara sann ändrar inte
utdata för vardaglig kod.

### 6.2 Bas, skiftläge och läsbarhet

| Variabel | Typ | Standard | Betydelse |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Basen för utskrift av heltal (med fast bredd och `int`). Utanför 2 till 36 är det ett **utskriftsfel** (CL specificerar också intervallet) |
| `*print-radix*` | `bool` | `false` | Om sant läggs en basmarkör till: `#b`/`#o`/`#x`, `#NNr` för andra baser och en avslutande `.` för bas 10. Markören hamnar **före** tecknet (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Skiftläget för symbolnamn: `:upcase` / `:downcase` / `:capitalize` (samma stavningar som CL). Vilken annan symbol som helst är ett utskriftsfel |
| `*print-readably*` | `bool` | `false` | Om sant skrivs det ut i en form som kan läsas tillbaka. Det tvingar fram escape-tecken och stänger av avskärningarna i `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Antalet rader den snygga skrivaren får använda. Överskottet kapas, med `..` på slutet som i CL. 0 betyder obegränsat. Ett negativt värde är ett utskriftsfel |
| `*print-escape*` | `bool` | `true` | Om `write`/`write-to-string` gör `prin1` eller `princ`. **Bara de två läser den** |
| `*print-array*` | `bool` | `true` | Om `Vector<T>` och `Array<T>` visar sitt innehåll. Om sant, CL:s arraysyntax (`#(1 2 3)` / `#2A((1 2) (3 4))`); om falskt, bara typ och form, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Markörerna som `*print-radix*` lägger till kan läsas tillbaka av läsaren (basnotationen i
[Syntaxreferensen](../syntax.md#1-lexikaliska-element)).

**Varför standardvärdet för `*print-case*` skiljer sig från CL**: CL:s standard är `:upcase` eftersom CL:s
läsare lagrar symbolnamn med versaler, det vill säga det betyder "som lagrat". Den här läsaren lagrar dem
med gemener, så standardvärdet med samma betydelse är `:downcase`.

**Den saknade halvan av `*print-readably*`**: CL signalerar `print-not-readable` för värden som inte kan
läsas tillbaka, men det här språket har inget condition att signalera, och inget sätt att avgöra
läsbarhet för användartyper, som `print-object` får skriva ut hur som helst. Bara det tvingade escapet och
åsidosättandet av avskärningarna finns.

**Varför bara `write` läser `*print-escape*`**: som CLHS specificerar binder `~s`/`prin1`/`pprint` den till
sant, och `~a`/`princ` till falskt, var och en bara under sitt eget anrop. Så de enda läsare som ser den
obunden är `write`/`write-to-string`. En implementation av `print-object` bör läsa sitt eget
`escape`-argument i stället för den här globalen: det argumentet bär det värde direktivet valde.

**Vad CL har och det här språket inte har**: `*print-gensym*` (det finns inga ointernerade symboler).

### 6.3 Tillfälliga åsidosättanden

CL binder dessa med `let`, men `let` i det här språket binder lexikalt, så använd `dlet`
([Övrigt](system.md#10-övrigt)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; gränserna gäller bara den här utskriften
(with-standard-io-syntax (println "~a" x))   ; skriv ut med allt tillbaka på standardvärdena
```

`with-standard-io-syntax` kör sin kropp med alla utskriftskontrollvariabler på sina standardvärden och
`*read-eval*` satt till `true`.
