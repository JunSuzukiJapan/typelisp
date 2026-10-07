<!-- translated-from: docs/ja/reference/functions/format.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Formatdirektiv

De direktiv som skrivs i kontrollsträngarna för `print`/`println`/`format`. De täcker nästan alla av CL:s
`format`-direktiv. Själva funktionerna beskrivs i [Utskrift](printing.md#1-print--println--format).

## 1. Hur direktiv skrivs

Varje direktiv är `~`, sedan valfria **prefixparametrar** (kommaseparerade: ett heltal / `'c` (ett
tecken) / `v` (tas från nästa argument) / `#` (antalet återstående argument)), sedan de valfria
**modifierarna** `:` och `@`, sedan direktivtecknet, i den ordningen. Direktivtecken skiljer inte på
skiftläge.

Kontrollsträngen måste vara en literal ([Utskrift](printing.md#1-print--println--format)). Utöver det
kontrolleras följande vid kontrolltillfället.

- **Antalet och typerna på argument.** För varje direktiv som förbrukar ett argument: om det finns ett
  argument kvar, och om dess typ accepteras (noterna om "argument" i tabellerna nedan). Där vägen
  beror på värden vid körning, som att flytta med `~*`, vilken sats i `~[` som tas, om `~^` utlöses
  eller hur många gånger `~@{` upprepar, kontrolleras **varje väg**. Överblivna argument är i sin
  ordning (som i CL).
- **Parametrar och modifierare.** En modifierare som inte accepteras, för många parametrar och värden
  utanför intervallet (en negativ bredd, en bas som inte är 2 till 36, ett heltal där ett tecken
  förväntas och så vidare) är fel. De ignoreras eller avrundas aldrig tyst.

**Elementen** i ett listargument (`~{`, `~:{`, `~<...~:>`) är `Sexpr`s, och varken deras antal eller
typen på varje element kan avgöras från typerna. Krav på element (ett heltal för `~d` och så vidare)
och saknade element kontrolleras när värdena kommer, och är fel vid körning (det byter aldrig till en
annan representation i stället).

CL:s tillåtande regler är inte antagna. Att skicka ett icke-heltal till `~d` och få det utskrivet som
`~a`, eller att `~:[` behandlar vilket värde som helst som ett booleskt, tolkas inte om på det sättet;
de är typfel.

## 2. Utdata (förbrukar ett argument)

| Direktiv | Parametrar / modifierare | Betydelse |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=högerjustera | Estetisk (CL:s `princ`; strängar utan citattecken). Argumentet kan ha vilken typ som helst |
| `~s` | Som ovan | Standard (CL:s `prin1`; en form som kan läsas tillbaka). Argumentet kan ha vilken typ som helst |
| `~w` | — | CL:s `write`. Skriver snyggt (pretty) om `*print-pretty*` är sant, annars samma som `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=siffergrupper, `@`=alltid ett tecken | Decimala/binära/oktala/hexadecimala heltal. Argumentet är ett heltal |
| `~r` | `~radix,mincol,padchar,commachar,interval` (med en bas) eller ingen | Med en bas, den basen (2 till 36). Utan: `~r`=engelskt kardinaltal, `~:r`=engelskt ordningstal, `~@r`=romerska siffror, `~:@r`=gamla romerska siffror. Argumentet är ett heltal |
| `~p` | `:`=backa ett, `@`=y/ies | Pluralis (`~p`→"s", `~@p`→"y"/"ies"). Argumentet är ett heltal |
| `~c` | `:`=namn, `@`=syntaxen `#\` | Ett tecken. Argumentet är ett `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=tecken | Fast decimalform. Argumentet är ett tal |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=tecken | Exponentform. Argumentet är ett tal. CL:s parametrar för exponentsiffror, skala och overflowchar stöds inte (att ange dem är ett fel) |
| `~g` | `@`=tecken | Allmän flyttalsform. Argumentet är ett tal. Tar inga parametrar |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Monetär notation. Argumentet är ett tal |

## 3. Utdata (förbrukar inga argument)

| Direktiv | Betydelse |
|---|---|
| `~%` | Radbyte (`~n%` för n stycken) |
| `~&` | fresh-line (ett radbyte om man inte står i början av en rad; `~n&`) |
| `~\|` | Sidbrytning (form feed) |
| `~~` | Ett bokstavligt `~` (`~n~` för n stycken) |
| `~t` | Tabb (`~colnum,colincT`. Om man redan står på kolumn colnum eller längre, flyttar den fram med en multipel av colinc; flyttar inte om colinc är 0. `@`=relativ. `:`=en tabb relativ till början av det logiska blocket, som bara fungerar vid snygg utskrift) |
| `~_` | Villkorligt radbyte (pretty; vanlig=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Indrag (pretty; `~ni`=blockets början + n / `~n:i`=aktuell kolumn + n) |
| `~<newline>` | Ignorerar radbytet (`:`=behåll blanktecknen, `@`=behåll radbytet) |

Som i CL gör direktiven för snygg utskrift (`~_` `~i` `~:t` `~<...~:>`, och vägen för snygg utskrift
i `~a`/`~s`/`~w`) alla ingenting när `*print-pretty*` är falskt. Den är falsk som standard.

## 4. Kontrollstrukturer

| Direktiv | Betydelse |
|---|---|
| `~(...~)` | Skiftlägeskonvertering (`~(` gemener, `~:(` versal på varje ord, `~@(` versal bara på det första ordet, `~:@(` alla versaler) |
| `~[...~;...~]` | Villkorligt val (förgrenar på ett heltalsargument. Med `~n[`, `~v[` eller `~#[` förgrenar den på det värdet och tar inget argument. `~:;`=standardsatsen, bara som sista sats). `~:[false~;true~]` förgrenar på ett `bool`-argument och har exakt två satser |
| `~{...~}` | Iteration (går igenom ett listargument. `~:{`=per underlista, `~@{`=över de återstående argumenten, `~:@{`=över varje lista bland de återstående argumenten, `~^`=avsluta, `~:}`=kör en gång även om tom). En kropp som inte förbrukar något argument i en iteration är ett fel (den skulle aldrig bli klar) |
| `~<...~;...~>` | Justering (sprider segment över `~mincol` kolumner. `:`/`@`=utfyllnad i ändarna) |
| `~<...~;...~:>` | **Logiskt block** (stängs med `~:>`; en annan sak än justeringen ovan). Det första segmentet är prefixet och det sista är suffixet (båda bara literala strängar). Med avgränsaren `~@;` är prefixet ett **prefix per rad**. `~:<` har som standard `(`/`)` som prefix/suffix. Argumentet är en lista (`~@<` använder de återstående argumenten på stället) |
| `~*` | Hoppa över argument (`~n*`=framåt n, `~:*`=bakåt, `~@*`=till en absolut position) |
| `~/name/` | Metodanrop (kapitel 5. Flaggorna `:`/`@` skickas till metoden. Tar inga parametrar) |

Följande CL-direktiv stöds inte (de är fel vid kontrolltillfället).

- `~?` och `~@?`: de tar en kontrollsträng som ett argument vid körning, så de argument dess direktiv
  förbrukar kan inte kontrolleras. Skriv de direktiven direkt i kontrollsträngen.
- `~@[...~]`: det testar om ett argument inte är nil, men det här språket har inget nil. Använd
  `~:[false~;true~]`, som förgrenar på en `bool`.
- `~{~}` med tom kropp: den tar kroppen från ett argument vid körning. Skriv direktiven inuti
  klamrarna.

## 5. `~/name/`

**En skillnad mot CL: namnet slås inte upp som en global funktion utan som en metod på argumentets egen
typ.** Metoden har formen `((self Self) (colon bool) (at bool)) → string`, och direktivets `:`/`@`
skickas vidare som de är.

CL:s sätt att slå upp det som en global funktion kan inte implementeras säkert i det här språket. Även
med en literal kontrollsträng är typen på elementen i ett listargument (inuti `~{`) inte känd vid
kontrolltillfället, och att slå upp en funktion enbart på namn kan anropa en funktion avsedd för en annan
typ. Att välja efter värdets typ innebär att metoden typkontrolleras för precis den typen, vilket är säkert
(samma mekanism som `print-object`). Det fungerar också för värden som `string`/`bool`/`char`/`symbol`/
listor. Bara för heltal, vars bredd inte kan avgöras från värdet, är det ett fel **när fler än en
heltalstyp definierar en metod med det namnet**.

Vilket argument det tillämpas på är inte känt, men vilka metoder det kan anropa är det. Kontrollen samlar
varje `~/name/` från den literala kontrollsträngen och noterar, bland argumenttyperna på det anropsstället,
de som har en metod av formen ovan. Så **om ingen av argumenttyperna har metoden är det ett fel vid
kontrolltillfället** (inte vid körning), och det fungerar i AOT-körbara filer också.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
