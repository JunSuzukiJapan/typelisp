<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tal

Operationer på heltal, flyttal, kvoter, komplexa tal och booleska värden, samt andra talrelaterade
funktioner. För hur anropsformerna läses, se [Inbyggda funktioner](README.md).

## 1. Heltal med fast bredd

Det finns sju heltalstyper: **`int`** (CL:s `integer`: godtycklig precision, och standardtypen för
heltalsliteraler utan annotering; kapitel 3), och de med fast bredd `i8` `i16` `i32` `u8` `u16` `u32`.
Vilken en operation löses upp för avgörs av typen på det första argumentet (de är oberoende av varandra,
utan implicita konverteringar). **Det finns ingen 64-bitars heltalstyp.** Ett värde vid körning är ett ord
vars låga bitar är en tagg, så bara 63 bitar återstår för ett omedelbart heltal, och en typ som påstod sig
ha 64 bitar skulle behöva tappa den översta biten någonstans. `int` blir ett bignum när det passerar de
63 bitarna, så om bredden inte spelar någon roll, använd `int`. Tabellen nedan gäller de sex typerna med
fast bredd (tabellen för `int` finns i kapitel 3).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | De fyra räknesätten. `/` trunkerar mot noll och ger panic vid division med noll |
| `mod` | `(mod a b)` | `(T,T)→T` | Rest (CL:s `mod`, **golvdivision**: tecknet följer divisorn. `(mod -7 3)`→`2`). Ger panic vid division med noll |
| `rem` | `(rem a b)` | `(T,T)→T` | Rest (CL:s `rem`, **trunkerande division**: tecknet följer dividenden. `(rem -7 3)`→`-1`). Ger panic vid division med noll |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Motsvarar CL:s `floor`/`ceiling`/`round`/`truncate` med två argument (`(floor 7 2)`→kvot 3, rest 1). I stället för flera värden returnerar de kvoten och resten i en `cons-cell` (`car`=kvot, `cdr`=rest). `round-div` avrundar jämna fall till jämnt, som CL gör |
| `abs` | `(abs x)` | `T→T` | Absolutbelopp |
| `signum` | `(signum x)` | `T→T` | Tecken (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Största gemensamma delare |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Minsta gemensamma multipel (0 om någon är 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Det större / mindre (tre eller fler argument expanderas av det variadiska sockret i kapitel 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Jämförelse |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Alla samma som `=` (det finns ingen skillnad för tal av samma typ) |
| `int->float` | `(int->float x)` | `T→f64` | Vidgande konvertering till `f64` |
| `int->int` | `(int->int x)` | `T→int` | Vidgande konvertering till `int` (alltid exakt). Det `(as int x)` gör |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Vidgande konvertering till `ratio` (alltid exakt) |
| `int->char` | `(int->char x)` | `T→char` | Tolkar värdet som ett skalärt Unicode-värde. Ger panic vid ogiltigt värde |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | En version av `int->char` som returnerar `None` vid misslyckande |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Breddkonvertering. Värden som inte ryms trunkeras (som Rusts `as`) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Samma konvertering som en fråga. `None` om värdet inte ryms i den bredden |

Dessa konverteringar är också vad specialformerna `(as Type x)`/`(try-as Type x)`
([Syntaxreferens](../syntax.md#7-övriga-specialformer)) gör. Bitoperationer (`logand`/`ash`/`ldb` och så
vidare) och predikat (`zerop`/`evenp` och så vidare) har samma form över typer, så de är samlade i
kapitel 11 och 9.

`i8` `i16` `u8` `u16` `u32` har exakt tabellen i det här kapitlet, och `f32` har exakt `f64`-tabellen i
kapitel 4.

**Ett typnamn betyder dess bredd och teckenhantering, inget mer.** `i32` betyder "behandla 32 bitar som
med tecken" och `u32` betyder "behandla 32 bitar som utan tecken". `(+ (the u8 200) (the u8 100))` är
`44`, `(+ 2147483647 1)` (som `i32`) är `-2147483648`, och `(lognot (the u32 0))` är `4294967295`. `f32`
är likadant: ett riktigt binary32. `(/ (the f32 1.0) (the f32 3.0))` skrivs ut som `0.33333334`, ett annat
värde än `f64`-resultatet `0.3333333333333333`.

Den härledda CL-katalogen (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` och predikaten i kapitel 9) finns för
`int`/`i32`/`f64`/`ratio`. Om du behöver den för en annan bredd, flytta över med `(as int x)` /
`(as i32 x)` (breddkonverteringar finns för varje par).

## 2. Råa ord vid C-gränsen (`ptr` / `c-long` / `c-ulong`)

Tre typer som bara används för att skicka värden till och från C-funktioner som deklarerats med
[`defffi`](../syntax.md#33-defffi--deklarera-c-funktioner-ffi). `ptr` är en ogenomskinlig pekare, och
`c-long` / `c-ulong` är C:s `long` / `unsigned long`. Att göra en till ett värde kräver att man är inuti
`(unsafe ...)`.

**Det finns ingen aritmetik.** Ingenting i tabellen i kapitel 1 gäller: varken `(+ p 1)` eller `(< n m)`
kan skrivas. Det här är ord att lämna till C, inte typer att räkna med, så för att räkna flyttar man till
en typ med bredd. `c-long` / `c-ulong` har bara konverteringar:

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Samma breddkonverteringar som i kapitel 1. Värden som inte ryms trunkeras |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Samma konvertering som en fråga |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Vägen in, från det andra råa ordet och från heltalstyperna i kapitel 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Som ovan |
| `int->int` | `(int->int x)` | `T→int` | **Alltid exakt**. Det ärliga sättet att läsa en `size_t` som inte ryms i en `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` är vad dessa gör, och konverteringarna
finns för varje par med heltalstyperna i kapitel 1. `ptr` har inte ens den här tabellen: inget sätt
tillhandahålls att läsa en pekare som ett tal. Det är ett värde som bara skickas, tas emot och lämnas
vidare till en annan C-funktion.

**De kan inte heller skrivas ut.** `(println "~a" x)` accepterar inte ett rått ord (det har ingen
`Sexpr`-representation), så flytta det först till en typ med bredd, som i
`(println "~a" (as int n))`.

"Det finns ingen 64-bitars heltalstyp" från början av kapitel 1 gäller även dessa tre. Det gäller
**eftersom de inte kan lagras**: de kan inte vara ett `defstruct`-fält, en `defvar`, inuti ett typargument
eller inuti en `Sexpr`, så de är ord som bara passerar genom en funktion som argument, returvärden och
lokala variabler. För detaljer, se
[Syntaxreferensen](../syntax.md#ptr--c-long--c-ulong--råa-maskinord).

## 3. Heltal med godtycklig precision `int`

CL:s `integer`, och det här språkets **heltal**: heltalsliteraler utan annotering har den här typen, och
inbyggda funktioner som returnerar ett tal, som `length` och `char->int`, returnerar den här typen. Ett
värde hålls som ett omedelbart 63-bitarsvärde (fixnum) så länge det ryms, befordras automatiskt till ett
bignum när resultatet av en operation inte längre ryms, och går tillbaka till ett omedelbart värde när det
ryms igen. `eq` är alltid värdeidentitet inom fixnum-intervallet, och `eql`/`=` är numerisk identitet över
hela intervallet. Det är en annan typ än heltalstyperna med fast bredd (kapitel 1), utan implicit
konvertering: `(as int x)` är den exakta vidgningen från en fast bredd, och `(as i32 n)` /
`(try-as i32 n)` är trunkeringen / kontrollen från `int` (samma betydelse som `int->W` / `try-int->W` i
kapitel 1).

Heltalsvarianten av `Sexpr` är också bara `int` (`(int n)` accepterar både fixnums och bignums).

Inbyggda funktioner som tar ett index eller ett antal (`substring`, `get` för `Vector`, skiftantalet i
`ash` och så vidare) accepterar `int`, men att skicka ett värde som inte ryms i ett fixnum är ett fel vid
körning ("an integer argument does not fit a fixnum").

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Svämmar aldrig över (de befordrar) |
| `/` | `(/ a b)` | `(int,int)→int` | Trunkerar mot noll. Ger panic vid division med noll |
| `mod` | `(mod a b)` | `(int,int)→int` | Rest av golvdivision (tecknet följer divisorn) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Alla `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Samma som kapitel 11 (tvåkomplement med oändligt många bitar) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Samma som kapitel 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Trunkering / kontroll. `W` är en av de sex bredderna eller `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identitet (på sidan för fast bredd och C-ord vidgar `int->int`; kapitel 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Samma form som kapitel 1. `expt` accepterar bara icke-negativa exponenter |

## 4. Flyttal (`f64` / `f32`)

`f32` har samma tabell.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Division med noll ger inte panic; den ger `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Rest av golvdivision (som i CL; tecknet följer divisorn. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Rest av trunkerande division (som i CL; tecknet följer dividenden. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Jämförelse |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Alla samma som `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potens |
| `abs` | `(abs x)` | `f64→f64` | Absolutbelopp |
| `signum` | `(signum x)` | `f64→f64` | Tecken (`1.0`/`-1.0`; `±0.0`/`NaN` returneras som de är. Som i CL, till skillnad från Rusts `signum`) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Det större / mindre (tre eller fler argument expanderas av det variadiska sockret i kapitel 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Unära operationer |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Transcendenta funktioner. `log` är den naturliga logaritmen |
| `log` (två argument) | `(log x base)` | `(f64,f64)→f64` | Logaritm med given bas. Expanderas till `(/ (log x) (log base))` (kapitel 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Motsvarar CL:s versioner med två argument (`(floor 7.0 2.0)`→kvot 3, rest 1). Samma design som funktionerna med samma namn i kapitel 1 (`car`=kvot, `cdr`=rest) |
| `float->int` | `(float->int x)` | `f64→int` | Konverterar till `int` genom att trunkera mot noll (CL:s `truncate`; exakt för ändliga värden av vilken storlek som helst). Ger panic vid oändlighet och NaN. För en fast bredd, använd `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Konverterar till en `ratio` som det exakta binära rationella talet (CL:s `rational`) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Konverterar mellan flyttalsbredder. `float->f32` avrundar till närmaste, `float->f64` är alltid exakt. Det `(as f32 x)` gör |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Samma konvertering som en fråga. `none` om avrundningen ändrar värdet (vidgning till `f64` är alltid `some`). Det `(try-as f32 x)` gör |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL:s funktioner med samma namn. Alias för `floor`/`ceiling`/`round`/`truncate` ovan: i CL returnerar de utan prefix heltal, så de med prefixet `f` stämmer med det här språkets beteende |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 2 / 53 / 53 respektive (bara precisionen för `0.0` är 0). `f64` är alltid IEEE-754 binary64, så dessa är konstanter |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` eller `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Mantissan (i `[1/2,1)`, utan tecken) och exponenten. CL returnerar tre värden, men det finns inga flera värden, så tecknet lämnas åt `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Samma uppdelning med en exakt 53-bitars heltalsmantissa. `mantissa * 2^exponent` är exakt det ursprungliga värdet |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Det enklaste rationella tal som läses tillbaka som det flyttalet** (`(rationalize 0.1)` är `1/10`). För det exakta binära värdet, använd `float->ratio` |

**Skillnad mot CL: hur `round` avrundar.** `round` (och därmed `fround`/`round-div`) avrundar **bort från
noll** (`(round 2.5)` = `3.0`). CL avrundar **till jämnt** och ger `2`.

## 5. Kvoter `ratio`

CL-kompatibla rationella tal med godtycklig precision. De hålls alltid i enklaste form med positiv
nämnare och är heapallokerade. Det finns ingen implicit konvertering med heltalstyperna eller `f64`
(använd en explicit konverteringsmetod eller `as`/`try-as`). För syntaxen för kvotliteraler, se
[Syntaxreferensen](../syntax.md#1-lexikaliska-element).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | De fyra räknesätten (resultat alltid i enklaste form). `/` ger panic vid division med noll |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Rest av golvdivision (som i CL; tecknet följer divisorn) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Rest av trunkerande division (som i CL; tecknet följer dividenden) |
| `abs` | `(abs x)` | `ratio→ratio` | Absolutbelopp |
| `signum` | `(signum x)` | `ratio→ratio` | Tecken (returnerar `1`/`-1`/`0` som en `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potens. Exponenten måste vara en `ratio` med heltalsvärde (annars panic). En negativ exponent ger inversen |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Det större / mindre |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` har inga bitoperationer (i CL är de bara för heltal) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Jämförelse |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Alla samma som `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Täljare i enklaste form (samma namn som i CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Nämnare i enklaste form (alltid positiv) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Heltalsdel (trunkerad mot noll) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Konverterar till `f64` |

Vägarna in från heltal med fast bredd och `f64` är `int->int`/`int->ratio` (kapitel 1) och
`float->int`/`float->ratio` (kapitel 4). `int`/`ratio` är separata typer oberoende av `i32` och de andra,
och blandad aritmetik kräver explicita konverteringar.

## 6. Komplexa tal `complex`

En struct (`defstruct`) i standardbiblioteket.

**Två skillnader mot CL** (båda följer av statisk typning):

1. **Komponenterna är alltid `f64`.** Ett CL-komplext tal kan också hålla kvoter, och `(complex 1 2)` och
   `(complex 1.0 2.0)` är olika typer. En statisk typ måste välja en, och de transcendenta funktionerna
   returnerar flyttalsslaget.
2. **`(sqrt -1.0)` är den reella `sqrt` (NaN).** I CL kan `sqrt` returnera ett komplext tal från ett
   reellt, men `sqrt` för `f64` måste returnera en `f64`. Ett komplext resultat kommer från ett komplext
   argument: `(sqrt (complex -1.0 0.0))` är `i`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Konstruktion. Komponenterna kan läsas direkt som `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Realdel och imaginärdel. **De fungerar också på reella tal** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), som i CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Konjugat (fungerar också på reella tal) |
| `phase` | `(phase z)` | `complex→f64` | Argument i (-pi,pi] (fungerar också på reella tal) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Absolutbelopp. **Den enda `abs` som inte returnerar mottagarens typ** (som i CL är absolutbeloppet av ett komplext tal reellt) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Komplex aritmetik |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Likhet komponent för komponent. `Eq` är också implementerat (det finns ingen `Ord`: komplexa tal har ingen ordning, och CL:s `<` avvisar dem också) |
| `zerop` | `(zerop z)` | `complex→bool` | Om båda komponenterna är 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` ger principalvärden |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | Vinkeln för vektorn `(x,y)`. **CL:s `(atan y x)` med två argument är socker för detta** (den förgrenar på antalet argument, som `log` med två argument) |

Den implementerar `print-object`, så `~a`/`~s` skriver ut den som `#C(re im)`, som CL gör (det här
språkets läsare har ingen `#C`-syntax för att läsa tillbaka den).

## 7. Booleska värden

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negation |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Alla jämför värden för likhet |

`and`/`or` behöver kortslutningsutvärdering, så de är specialformer
([Syntaxreferens](../syntax.md#4-bindning-och-villkor)).

## 8. Numeriska hjälpfunktioner och anropssocker

`abs`/`signum` (alla numeriska typer), `gcd`/`lcm` (bara heltalstyper), `rem` (alla reella typer inklusive
`f64`) och `expt` (`int`/`f64`/`ratio`) är definierade som metoder för varje numerisk typ (upplöses efter
mottagarens typ: `(abs x)` är metoden för typen på `x`). Detaljerna för varje typ finns i kapitel 1, 3, 4
och 5. Heltal med fast bredd har ingen `expt` (de har ingen befordran och skulle svämma över; flytta till
`int` med `(as int x)` och använd dess `expt`).

### 8.1 Variadiska former och former med 0/1 argument

CL:s aritmetik och jämförelser är variadiska, men metoder löses bara upp efter mottagarens typ, inte efter
antalet argument. Så **kontrollen expanderar följande former till anrop med två argument**.

| Form du kan skriva | Expansion | Gäller för |
|---|---|---|
| `(op a b c ...)` | Den vänstra vikningen `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` med varje term bunden till en temporär | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | De av ovanstående som har ett identitetselement |
| `(op x)` | För `+ * max min logand logior logxor`, `x` självt. `(- x)` negerar, `(/ x)` ger inversen, `(gcd x)`/`(lcm x)` ger `(abs x)` (som i CL) | Samma som ovan |
| `(cmp x)` | Utvärderar `x` och ger `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Varje term utvärderas exakt en gång, från vänster till höger (det är därför de variadiska jämförelserna går
genom temporärer). Den variadiska formen av `/=` jämför **intilliggande par**, till skillnad från CL, som
frågar om alla par skiljer sig.

### 8.2 `isqrt` och heltals-`expt`

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Det största heltal som inte överstiger kvadratroten. Ger panic vid ett negativt värde |
| `expt` | `(expt n e)` | `(T,T)→T` | Potens (genom kvadrering). CL returnerar en kvot för en negativ exponent, men en heltalstyp kan inte representera den, så den ger panic; konvertera till `ratio` först |

## 9. Predikat

| Namn | Form | Typ | Typer |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (bara heltalstyper, som i CL) |

Det finns **inga typpredikat** som CL:s `numberp`/`integerp`/`floatp`. Med statisk typning är ett värdes typ
redan avgjord utan att man frågar vid körning.

## 10. Konstanter

| Namn | Typ | Värde |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Operationskoder som skickas till `boole` (i stället för CL:s keywords) |

Konstanter för numeriska gränser (CLHS 12.1.4.2 / 12.1.3):

| Namn | Typ | Beskrivning |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Övre / nedre gränsen för ett omedelbart 63-bitarsvärde (2^62-1 / -2^62). Ett `int` bortom dem blir ett bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | De största / minsta ändliga värdena |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Den minsta storleken skild från noll, inklusive subnormala tal |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Detsamma, begränsat till normaliserade tal |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | De följer CL:s definition (det minsta positiva `e` med `(/= (+ 1 e) 1)`), så de är **en ULP större än** 2^-53: 2^-53 självt avrundas tillbaka till `1.0` vid avrundning till närmaste jämna |

## 11. Bitoperationer

Definierade på tvåkomplement med oändligt många bitar (CL 12.10). De är implementerade för heltalstyperna
med fast bredd och `int`, inte för `ratio` (CL har också bitoperationer bara för heltal).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bitvis och, eller, exklusivt eller (variadiska versioner och versioner utan argument i 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Bitvis komplement |
| `ash` | `(ash x count)` | `(T,int)→T` | Aritmetiskt skift. Vänster om `count` är positivt, höger om negativt |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Om bit `index` är satt (**argumentordningen är omvänd mot CL**; se nedan) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Antalet satta bitar (för ett negativt tal, antalet 0-bitar) |
| `integer-length` | `(integer-length x)` | `T→T` | Antalet bitar som behövs för att representera det, utan att räkna tecknet |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | De återstående sju, sammansatta av ovanstående |

**Bara det andra argumentet till `ash` är `int` i stället för `T`.** Det är ett **avstånd** i bitar, inte
ett värde av mottagarens typ, så mottagarens bredd och teckenhantering säger ingenting om avståndet (av
samma skäl som `count` i CL:s `(ash integer count)` är vilket heltal som helst). Att skifta ett värde utan
tecken åt höger är ett logiskt skift (`(ash (the u8 200) -3)` = `25`), och ett med tecken är ett
aritmetiskt skift som avrundar mot minus oändligheten (`(ash (the i32 -100) -4)` = `-7`). `index` i
`logbitp` är `int` av samma skäl.

**Bytespecificerare.** I stället för det ogenomskinliga objekt som CL:s `byte` returnerar används en
`cons-cell<int,int>` (`car`=storlek, `cdr`=position). Både storlek och position är antal bitar, så de är
`int` oavsett bredden på heltalet som plockas isär.

**Heltalet är det första argumentet, i en annan ordning än i CL.** CL skriver `(ldb bytespec integer)`,
men det här språket väljer en metod efter typen på mottagaren (det första argumentet), och med
specificeraren först kunde det inte välja efter heltalets typ. Alla andra bitoperationer har formen
`(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), och bara `ldb`-familjen och `logbitp`
var tvärtom, så de ändrades för att stämma. De återstående argumenten behåller CL:s inbördes ordning, så
`(dpb newbyte spec n)` blir `(dpb n newbyte spec)`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Skapar en bytespecificerare |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Tar ut en komponent |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extraherar den angivna byten ur `x`, högerjusterad |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Om någon bit i den angivna byten är satt |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Nollställer allt utanför den angivna byten (behåller positionerna) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Placerar den högerjusterade `newbyte` i den angivna byten i `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Den positionsbevarande versionen av `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | En av de 16 logiska operationerna med två operander, vald av `op` (en `boole-*`-konstant från kapitel 10) |

`T` är en typ som implementerar traitet `Bits`, nämligen `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Bara
`boole` behåller `op` först, eftersom det inte finns någon anledning att ändra CL:s ordning där.

## 12. Slumptal

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Ett slumptal från `0` upp till men inte inklusive `n`. Om tillståndet utelämnas dras det från `*random-state*` och det flyttas fram |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Utan argument ett nytt tillstånd; med ett argument en kopia av det (kopian spelar upp samma följd) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Alltid `true` (den statiska typen utesluter redan andra typer; den finns bara för att motsvara CL) |
| `*random-state*` | — | `random-state` | Standardtillståndet för `random`. En global som kan tilldelas (ersätt den med `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | Tillståndet som heltalet namnger. Samma frö spelar alltid upp samma följd |

Generatorn är xorshift64 och returnerar samma följd oavsett om den tolkas eller kompileras.

Ett nytt tillstånd från `make-random-state` får sitt frö från väggklockan, så det kan inte återskapas
mellan körningar. För att återskapa, använd `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; skriver ut samma tre tal vid varje körning
```

**CL har inget portabelt sätt att ange ett frö** (`make-random-state` tar bara `nil`/`t`/ett tillstånd),
så det här namnet följer SBCL:s `sb-ext:seed-random-state` snarare än CL.

Olika frön ger olika följder. `(seed-random-state 0)` och `(seed-random-state 1)` ger olika följder, och
det gör även `-7` och `7`.
