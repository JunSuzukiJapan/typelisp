<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Getallen

Bewerkingen op gehele getallen, drijvendekommagetallen, rationale getallen, complexe getallen en
booleans, en andere getalgerelateerde functies. Hoe je de aanroepvormen leest staat in
[Ingebouwde functies](README.md).

## 1. Gehele getallen met vaste breedte

Er zijn zeven gehele types: **`int`** (`integer` van CL: willekeurige precisie, en het standaardtype
van gehele literals zonder annotatie; hoofdstuk 3), en de types met vaste breedte `i8` `i16` `i32`
`u8` `u16` `u32`. Voor welk type een bewerking wordt opgelost, bepaalt het type van het eerste
argument (ze zijn onafhankelijk van elkaar, zonder impliciete conversies). **Er is geen geheel type
van 64 bits.** Een runtimewaarde is één woord waarvan de lage bits een tag zijn, dus er blijven maar 63
bits over voor een directe geheel getalwaarde, en een type dat 64 bits claimt zou ergens de bovenste
bit moeten laten vallen. `int` wordt een bignum zodra het die 63 bits passeert, dus gebruik `int` als
de breedte er niet toe doet. De onderstaande tabel is voor de zes types met vaste breedte (de tabel
voor `int` staat in hoofdstuk 3).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | De vier hoofdbewerkingen. `/` kapt af richting nul en geeft een panic bij deling door nul |
| `mod` | `(mod a b)` | `(T,T)→T` | Rest (`mod` van CL, **delen met naar beneden afronden**: het teken volgt de deler. `(mod -7 3)`→`2`). Geeft een panic bij deling door nul |
| `rem` | `(rem a b)` | `(T,T)→T` | Rest (`rem` van CL, **delen met afkappen**: het teken volgt het deeltal. `(rem -7 3)`→`-1`). Geeft een panic bij deling door nul |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Komen overeen met `floor`/`ceiling`/`round`/`truncate` van CL met twee argumenten (`(floor 7 2)`→quotiënt 3, rest 1). In plaats van meerdere waarden geven ze het quotiënt en de rest in een `cons-cell` terug (`car`=quotiënt, `cdr`=rest). `round-div` rondt halve waarden af naar even, zoals CL |
| `abs` | `(abs x)` | `T→T` | Absolute waarde |
| `signum` | `(signum x)` | `T→T` | Teken (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Grootste gemene deler |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Kleinste gemene veelvoud (0 als een van beide 0 is) |
| `max` `min` | `(op a b)` | `(T,T)→T` | De grootste / kleinste (drie of meer argumenten worden door de variadische suiker van hoofdstuk 8 uitgebreid) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Vergelijking |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Allemaal hetzelfde als `=` (voor getallen van hetzelfde type is er geen verschil) |
| `int->float` | `(int->float x)` | `T→f64` | Verbredende conversie naar `f64` |
| `int->int` | `(int->int x)` | `T→int` | Verbredende conversie naar `int` (altijd exact). Wat `(as int x)` doet |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Verbredende conversie naar `ratio` (altijd exact) |
| `int->char` | `(int->char x)` | `T→char` | Interpreteert de waarde als een Unicode-scalarwaarde. Geeft een panic bij een ongeldige waarde |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Een versie van `int->char` die bij mislukking `None` teruggeeft |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Breedteconversie. Waarden die niet passen worden afgekapt (zoals `as` van Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Dezelfde conversie als vraag. `None` als de waarde niet in die breedte past |

Deze conversies zijn ook wat de speciale vormen `(as Type x)`/`(try-as Type x)`
([Syntaxreferentie](../syntax.md#7-overige-speciale-vormen)) doen. Bitbewerkingen
(`logand`/`ash`/`ldb` enzovoort) en predicaten (`zerop`/`evenp` enzovoort) hebben over types heen
dezelfde vorm, dus ze zijn samengebracht in hoofdstuk 11 en 9.

`i8` `i16` `u8` `u16` `u32` hebben precies de tabel van dit hoofdstuk, en `f32` heeft precies de
`f64`-tabel van hoofdstuk 4.

**Een typenaam betekent zijn breedte en tekenstatus, niets meer.** `i32` betekent "behandel 32 bits als
met teken" en `u32` betekent "behandel 32 bits als zonder teken". `(+ (the u8 200) (the u8 100))` is
`44`, `(+ 2147483647 1)` (als `i32`) is `-2147483648`, en `(lognot (the u32 0))` is `4294967295`. `f32`
is hetzelfde: een echte binary32. `(/ (the f32 1.0) (the f32 3.0))` wordt afgedrukt als `0.33333334`, een
andere waarde dan het `f64`-resultaat `0.3333333333333333`.

De afgeleide CL-catalogus (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` en de predicaten van hoofdstuk 9)
bestaat voor `int`/`i32`/`f64`/`ratio`. Als je hem voor een andere breedte nodig hebt, ga dan over met
`(as int x)` / `(as i32 x)` (breedteconversies bestaan voor elk paar).

## 2. Ruwe woorden aan de C-grens (`ptr` / `c-long` / `c-ulong`)

Drie types die alleen worden gebruikt om waarden van en naar C-functies door te geven die met
[`defffi`](../syntax.md#33-defffi--c-functies-declareren-ffi) zijn gedeclareerd. `ptr` is een ondoorzichtige
pointer, en `c-long` / `c-ulong` zijn `long` / `unsigned long` van C. Er een tot waarde maken vereist
dat je binnen `(unsafe ...)` bent.

**Er is geen rekenkunde.** Niets uit de tabel van hoofdstuk 1 is van toepassing: noch `(+ p 1)` noch
`(< n m)` kan worden geschreven. Dit zijn woorden om aan C te geven, geen types om mee te rekenen, dus
ga om te rekenen over naar een type met een breedte. `c-long` / `c-ulong` hebben alleen conversies:

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Dezelfde breedteconversies als hoofdstuk 1. Waarden die niet passen worden afgekapt |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Dezelfde conversie als vraag |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | De weg naar binnen, vanuit het andere ruwe woord en vanuit de gehele types van hoofdstuk 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Idem |
| `int->int` | `(int->int x)` | `T→int` | **Altijd exact**. De eerlijke manier om een `size_t` te lezen die niet in een `i32` past |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` zijn wat deze doen, en de
conversies bestaan voor elk paar met de gehele types van hoofdstuk 1. `ptr` heeft deze tabel niet
eens: er is geen manier om een pointer als getal te lezen. Het is een waarde die alleen wordt
doorgegeven, ontvangen en aan een andere C-functie overgedragen.

**Ze kunnen ook niet worden afgedrukt.** `(println "~a" x)` accepteert geen ruw woord (het heeft geen
`Sexpr`-representatie), dus ga eerst over naar een type met een breedte, zoals in
`(println "~a" (as int n))`.

"Er is geen geheel type van 64 bits" uit het begin van hoofdstuk 1 geldt ook voor deze drie. Het geldt
**omdat ze niet kunnen worden opgeslagen**: ze kunnen geen `defstruct`-veld, een `defvar`, binnen een
typeargument of binnen een `Sexpr` zijn, dus het zijn woorden die alleen als argumenten, returnwaarden
en lokale variabelen door een functie heen gaan. Voor details zie de
[Syntaxreferentie](../syntax.md#ptr--c-long--c-ulong--ruwe-machinewoorden).

## 3. Gehele getallen met willekeurige precisie `int`

`integer` van CL, en het **gehele getal** van deze taal: gehele literals zonder annotatie hebben dit
type, en ingebouwde functies die een getal teruggeven, zoals `length` en `char->int`, geven dit type
terug. Een waarde wordt als directe waarde van 63 bits (fixnum) bewaard zolang ze past, wordt
automatisch tot bignum gepromoveerd wanneer het resultaat van een bewerking niet meer past, en gaat
terug naar een directe waarde wanneer ze weer past. `eq` is binnen het fixnum-bereik altijd
waarde-identiteit, en `eql`/`=` zijn numerieke identiteit over het hele bereik. Het is een ander type
dan de gehele types met vaste breedte (hoofdstuk 1), zonder impliciete conversie: `(as int x)` is de
exacte verbreding vanuit een vaste breedte, en `(as i32 n)` / `(try-as i32 n)` zijn het afkappen /
controleren vanuit `int` (dezelfde betekenis als `int->W` / `try-int->W` in hoofdstuk 1).

De gehele variant van `Sexpr` is ook gewoon `int` (`(int n)` accepteert zowel fixnums als bignums).

Ingebouwde functies die een index of een aantal nemen (`substring`, `get` van `Vector`, het
verschuivingsaantal van `ash` enzovoort) accepteren `int`, maar een waarde doorgeven die niet in een
fixnum past is een runtimefout ("an integer argument does not fit a fixnum").

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Lopen nooit over (ze promoveren) |
| `/` | `(/ a b)` | `(int,int)→int` | Kapt af richting nul. Geeft een panic bij deling door nul |
| `mod` | `(mod a b)` | `(int,int)→int` | Rest van deling met naar beneden afronden (het teken volgt de deler) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Allemaal `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Hetzelfde als hoofdstuk 11 (twee-complement met oneindig veel bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Hetzelfde als hoofdstuk 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Afkappen / controleren. `W` is een van de zes breedtes of `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identiteit (aan de kant van vaste breedte en C-woorden verbreedt `int->int`; hoofdstuk 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Dezelfde vorm als hoofdstuk 1. `expt` accepteert alleen niet-negatieve exponenten |

## 4. Drijvendekommagetallen (`f64` / `f32`)

`f32` heeft dezelfde tabel.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Deling door nul geeft geen panic; het geeft `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Rest van deling met naar beneden afronden (zoals in CL; het teken volgt de deler. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Rest van deling met afkappen (zoals in CL; het teken volgt het deeltal. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Vergelijking |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Allemaal hetzelfde als `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Macht |
| `abs` | `(abs x)` | `f64→f64` | Absolute waarde |
| `signum` | `(signum x)` | `f64→f64` | Teken (`1.0`/`-1.0`; `±0.0`/`NaN` worden ongewijzigd teruggegeven. Zoals in CL, anders dan `signum` van Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | De grootste / kleinste (drie of meer argumenten worden door de variadische suiker van hoofdstuk 8 uitgebreid) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Unaire bewerkingen |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Transcendente functies. `log` is de natuurlijke logaritme |
| `log` (twee argumenten) | `(log x base)` | `(f64,f64)→f64` | Logaritme met een gegeven grondtal. Wordt uitgebreid tot `(/ (log x) (log base))` (hoofdstuk 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Komen overeen met de versies met twee argumenten van CL (`(floor 7.0 2.0)`→quotiënt 3, rest 1). Hetzelfde ontwerp als de functies met dezelfde naam in hoofdstuk 1 (`car`=quotiënt, `cdr`=rest) |
| `float->int` | `(float->int x)` | `f64→int` | Converteert naar `int` door af te kappen richting nul (`truncate` van CL; exact voor eindige waarden van elke grootte). Geeft een panic bij oneindig en NaN. Gebruik voor een vaste breedte `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Converteert naar een `ratio` als het exacte binaire rationale getal (`rational` van CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Converteert tussen drijvendekommabreedtes. `float->f32` rondt af naar het dichtstbijzijnde, `float->f64` is altijd exact. Wat `(as f32 x)` doet |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Dezelfde conversie als vraag. `none` als afronden de waarde verandert (verbreden naar `f64` is altijd `some`). Wat `(try-as f32 x)` doet |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Functies met dezelfde naam in CL. Aliassen van de bovenstaande `floor`/`ceiling`/`round`/`truncate`: in CL geven die zonder voorvoegsel gehele getallen terug, dus die met voorvoegsel `f` komen overeen met het gedrag van deze taal |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Respectievelijk 2 / 53 / 53 (alleen de precisie van `0.0` is 0). `f64` is altijd IEEE-754 binary64, dus dit zijn constanten |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` of `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | De mantisse (in `[1/2,1)`, zonder teken) en de exponent. CL geeft drie waarden terug, maar er zijn geen meervoudige waarden, dus het teken wordt aan `float-sign` overgelaten |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Dezelfde ontbinding met een exacte geheel getalmantisse van 53 bits. `mantissa * 2^exponent` is precies de oorspronkelijke waarde |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Het eenvoudigste rationale getal dat als die float terug wordt gelezen** (`(rationalize 0.1)` is `1/10`). Gebruik `float->ratio` voor de exacte binaire waarde |

**Verschil met CL: hoe `round` afrondt.** `round` (en dus `fround`/`round-div`) rondt **van nul weg**
af (`(round 2.5)` = `3.0`). CL rondt **naar even** af en geeft `2`.

## 5. Rationale getallen `ratio`

Met CL compatibele rationale getallen met willekeurige precisie. Ze worden altijd in laagste termen met
een positieve noemer bewaard en op de heap gealloceerd. Er is geen impliciete conversie met de gehele
types of `f64` (gebruik een expliciete conversiemethode of `as`/`try-as`). Voor de syntaxis van
ratio-literals zie de [Syntaxreferentie](../syntax.md#1-lexicale-elementen).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | De vier bewerkingen (resultaten altijd in laagste termen). `/` geeft een panic bij deling door nul |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Rest van deling met naar beneden afronden (zoals in CL; het teken volgt de deler) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Rest van deling met afkappen (zoals in CL; het teken volgt het deeltal) |
| `abs` | `(abs x)` | `ratio→ratio` | Absolute waarde |
| `signum` | `(signum x)` | `ratio→ratio` | Teken (geeft `1`/`-1`/`0` als `ratio` terug) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Macht. De exponent moet een `ratio` met een geheel getalwaarde zijn (anders een panic). Een negatieve exponent geeft het omgekeerde |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | De grootste / kleinste |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` heeft geen bitbewerkingen (in CL zijn die alleen voor gehele getallen) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Vergelijking |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Allemaal hetzelfde als `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Teller in laagste termen (dezelfde naam als in CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Noemer in laagste termen (altijd positief) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Geheel deel (afgekapt richting nul) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Converteert naar `f64` |

De wegen naar binnen vanuit gehele getallen met vaste breedte en `f64` zijn `int->int`/`int->ratio`
(hoofdstuk 1) en `float->int`/`float->ratio` (hoofdstuk 4). `int`/`ratio` zijn aparte types die
onafhankelijk zijn van `i32` en de andere, en gemengde rekenkunde vereist expliciete conversies.

## 6. Complexe getallen `complex`

Een struct (`defstruct`) in de standaardbibliotheek.

**Twee verschillen met CL** (beide volgen uit statische typering):

1. **De componenten zijn altijd `f64`.** Een complex getal in CL kan ook rationale getallen bevatten, en
   `(complex 1 2)` en `(complex 1.0 2.0)` zijn verschillende types. Een statisch type moet er een
   kiezen, en de transcendente functies geven de drijvendekommasoort terug.
2. **`(sqrt -1.0)` is de reële `sqrt` (NaN).** In CL kan `sqrt` uit een reëel getal een complex getal
   geven, maar de `sqrt` van `f64` moet een `f64` teruggeven. Een complex resultaat komt uit een
   complex argument: `(sqrt (complex -1.0 0.0))` is `i`.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Constructie. De componenten kunnen direct als `z::re`/`z::im` worden gelezen |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Reëel deel en imaginair deel. **Ze werken ook op reële getallen** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), zoals in CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Geconjugeerde (werkt ook op reële getallen) |
| `phase` | `(phase z)` | `complex→f64` | Argument in (-pi,pi] (werkt ook op reële getallen) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Absolute waarde. **De enige `abs` die niet het type van de ontvanger teruggeeft** (zoals in CL is de absolute waarde van een complex getal reëel) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Complexe rekenkunde |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Componentgewijze gelijkheid. `Eq` is ook geïmplementeerd (er is geen `Ord`: complexe getallen hebben geen ordening, en `<` van CL weigert ze ook) |
| `zerop` | `(zerop z)` | `complex→bool` | Of beide componenten 0 zijn |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` geven hoofdwaarden |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | De hoek van de vector `(x,y)`. **`(atan y x)` van CL met twee argumenten is hiervoor suiker** (het vertakt op het aantal argumenten, zoals de `log` met twee argumenten) |

Het implementeert `print-object`, dus `~a`/`~s` drukken het af als `#C(re im)`, zoals CL doet (de
reader van deze taal heeft geen `#C`-syntaxis om het terug te lezen).

## 7. Booleans

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Ontkenning |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Allemaal vergelijken ze waarden op gelijkheid |

`and`/`or` hebben kortsluitende evaluatie nodig, dus het zijn speciale vormen
([Syntaxreferentie](../syntax.md#4-binding-en-voorwaarden)).

## 8. Numerieke hulpfuncties en aanroepsuiker

`abs`/`signum` (alle numerieke types), `gcd`/`lcm` (alleen gehele types), `rem` (alle reële types
inclusief `f64`) en `expt` (`int`/`f64`/`ratio`) zijn gedefinieerd als methoden van elk numeriek type
(opgelost via het type van de ontvanger: `(abs x)` is de methode voor het type van `x`). De details per
type staan in hoofdstuk 1, 3, 4 en 5. Gehele getallen met vaste breedte hebben geen `expt` (ze hebben
geen promotie en zouden overlopen; ga met `(as int x)` over naar `int` en gebruik zijn `expt`).

### 8.1 Variadische vormen en vormen met 0/1 argument

De rekenkunde en vergelijking van CL zijn variadisch, maar methoden worden alleen via het type van de
ontvanger opgelost, niet via het aantal argumenten. Daarom **breidt de checker de volgende vormen uit
tot aanroepen met twee argumenten**.

| Vorm die je kunt schrijven | Expansie | Van toepassing op |
|---|---|---|
| `(op a b c ...)` | De linkse vouwing `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` met elke term aan een tijdelijke variabele gebonden | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Die van het bovenstaande die een neutraal element hebben |
| `(op x)` | Voor `+ * max min logand logior logxor` `x` zelf. `(- x)` negeert, `(/ x)` geeft het omgekeerde, `(gcd x)`/`(lcm x)` geven `(abs x)` (zoals in CL) | Idem |
| `(cmp x)` | Evalueert `x` en geeft `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Elke term wordt precies één keer geëvalueerd, van links naar rechts (daarom gaan de variadische
vergelijkingen via tijdelijke variabelen). De variadische vorm van `/=` vergelijkt **aangrenzende
paren**, anders dan CL, dat vraagt of alle paren verschillen.

### 8.2 `isqrt` en gehele `expt`

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Het grootste gehele getal dat de vierkantswortel niet overschrijdt. Geeft een panic bij een negatieve waarde |
| `expt` | `(expt n e)` | `(T,T)→T` | Macht (door kwadrateren). CL geeft bij een negatieve exponent een rationaal getal terug, maar een geheel type kan dat niet voorstellen, dus het geeft een panic; converteer eerst naar `ratio` |

## 9. Predicaten

| Naam | Vorm | Type | Types |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (alleen gehele types, zoals in CL) |

Er zijn **geen typepredicaten** zoals `numberp`/`integerp`/`floatp` van CL. Met statische typering
staat het type van een waarde al vast zonder dat tijdens runtime te vragen.

## 10. Constanten

| Naam | Type | Waarde |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Bewerkingscodes die aan `boole` worden doorgegeven (in plaats van de keywords van CL) |

Constanten voor numerieke grenzen (CLHS 12.1.4.2 / 12.1.3):

| Naam | Type | Beschrijving |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | De boven- / ondergrens van een directe waarde van 63 bits (2^62-1 / -2^62). Een `int` daarbuiten wordt een bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | De grootste / kleinste eindige waarden |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | De kleinste grootte ongelijk aan nul, inclusief subnormale getallen |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Hetzelfde, beperkt tot genormaliseerde getallen |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Ze volgen de definitie van CL (de kleinste positieve `e` met `(/= (+ 1 e) 1)`), dus ze zijn **één ULP groter dan** 2^-53: 2^-53 zelf rondt onder round-to-nearest-even terug naar `1.0` |

## 11. Bitbewerkingen

Gedefinieerd op twee-complement met oneindig veel bits (CL 12.10). Ze zijn geïmplementeerd voor de
gehele types met vaste breedte en `int`, niet voor `ratio` (CL heeft ook alleen bitbewerkingen voor
gehele getallen).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bitsgewijze en, of, exclusieve of (variadische vormen en vormen zonder argumenten in 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Bitsgewijs complement |
| `ash` | `(ash x count)` | `(T,int)→T` | Rekenkundige verschuiving. Naar links als `count` positief is, naar rechts als negatief |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Of bit `index` gezet is (**de argumentvolgorde is het omgekeerde van CL**; zie hieronder) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Het aantal gezette bits (bij een negatief getal het aantal 0-bits) |
| `integer-length` | `(integer-length x)` | `T→T` | Het aantal bits dat nodig is om het voor te stellen, zonder het teken mee te tellen |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | De overige zeven, samengesteld uit het bovenstaande |

**Alleen het tweede argument van `ash` is `int` in plaats van `T`.** Het is een **afstand** in bits, geen
waarde van het type van de ontvanger, dus de breedte en tekenstatus van de ontvanger zeggen niets over
de afstand (om dezelfde reden dat `count` in `(ash integer count)` van CL een willekeurig geheel getal
is). Een waarde zonder teken naar rechts verschuiven is een logische verschuiving
(`(ash (the u8 200) -3)` = `25`), en een waarde met teken is een rekenkundige verschuiving die naar min
oneindig afrondt (`(ash (the i32 -100) -4)` = `-7`). De `index` van `logbitp` is om dezelfde reden `int`.

**Bytespecificaties.** In plaats van het ondoorzichtige object dat `byte` van CL teruggeeft, wordt een
`cons-cell<int,int>` gebruikt (`car`=grootte, `cdr`=positie). Zowel grootte als positie zijn aantallen
bits, dus ze zijn `int`, wat de breedte van het gehele getal dat wordt ontleed ook is.

**Het gehele getal is het eerste argument, in een andere volgorde dan in CL.** CL schrijft
`(ldb bytespec integer)`, maar deze taal kiest een methode op het type van de ontvanger (het eerste
argument), en met de specificatie eerst zou ze niet op het type van het gehele getal kunnen kiezen.
Alle andere bitbewerkingen hebben de vorm `(op integer ...)` (`(logand a b)`, `(ash x count)`,
`(lognot x)`), en alleen de `ldb`-familie en `logbitp` stonden andersom, dus die zijn gelijkgetrokken.
De overige argumenten behouden de relatieve volgorde van CL, dus `(dpb newbyte spec n)` wordt
`(dpb n newbyte spec)`.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Maakt een bytespecificatie |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Haalt een component eruit |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Haalt de gespecificeerde byte uit `x`, rechts uitgelijnd |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Of enige bit in de gespecificeerde byte gezet is |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Wist alles buiten de gespecificeerde byte (posities blijven behouden) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Deponeert de rechts uitgelijnde `newbyte` in de gespecificeerde byte van `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | De positiebehoudende versie van `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Een van de 16 logische bewerkingen met twee operanden, gekozen door `op` (een `boole-*`-constante uit hoofdstuk 10) |

`T` is een type dat de trait `Bits` implementeert, namelijk `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`.
Alleen `boole` houdt `op` vooraan, omdat er daar geen reden is om de volgorde van CL te wijzigen.

## 12. Willekeurige getallen

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Een willekeurig getal van `0` tot maar niet inclusief `n`. Als de toestand wordt weggelaten, trekt het uit `*random-state*` en schuift die op |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Zonder argument een nieuwe toestand; met een argument een kopie ervan (de kopie speelt dezelfde reeks opnieuw af) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Altijd `true` (het statische type sluit andere types al uit; het bestaat alleen om met CL overeen te komen) |
| `*random-state*` | — | `random-state` | De standaardtoestand van `random`. Een globale variabele waaraan kan worden toegewezen (vervang hem met `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | De toestand die het gehele getal noemt. Dezelfde seed speelt altijd dezelfde reeks af |

De generator is xorshift64 en geeft dezelfde reeks, of geïnterpreteerd of gecompileerd.

Een nieuwe toestand van `make-random-state` wordt uit de wandklok gezaaid, dus hij is niet
reproduceerbaar tussen uitvoeringen. Gebruik `seed-random-state` om te reproduceren:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; prints the same three numbers on every run
```

**CL heeft geen draagbare manier om een seed te geven** (`make-random-state` neemt alleen
`nil`/`t`/een toestand), dus deze naam volgt `sb-ext:seed-random-state` van SBCL en niet CL.

Verschillende seeds geven verschillende reeksen. `(seed-random-state 0)` en `(seed-random-state 1)`
geven verschillende reeksen, en `-7` en `7` ook.
