<!-- translated-from: docs/ja/reference/syntax.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-syntaxisreferentie

typelisp is een statisch getypeerde Lisp, geschreven in S-expressies. Voor de lijst met ingebouwde
functies en methoden zie [Ingebouwde functies](functions/README.md); voor de lijst met types
[types.md](types.md); en voor het lezen van foutmeldingen [errors.md](errors.md).

## 1. Lexicale elementen

- **Niet hoofdlettergevoelig.** Symbolen worden bij het lezen allemaal naar kleine letters genormaliseerd.
- **Commentaar**: van `;` tot het einde van de regel (regelcommentaar). `#| ... |#` (blokcommentaar, dat
  kan nesten).
- **Evaluatie tijdens het lezen**: `#.(expr)` **voert de volgende vorm uit tijdens het lezen** en behandelt
  zijn waarde als wat is gelezen. Dit is de enige plek waar de reader meer is dan een functie van de
  tekst. Hoe ver het reikt hangt af van het leespad, zoals in CL:
  - `(load ...)` en de REPL evalueren één vorm per keer, dus het kan **functies aanroepen die eerder in
    dezelfde tekst zijn gedefinieerd** (`load` van CL).
  - Een modulebestand wordt als eenheid gecontroleerd en uitgevoerd door wie het met `use` gebruikt, dus
    `#.` kan alleen de standaardbibliotheek bereiken en wat de sessie al heeft uitgevoerd. Noch de eigen
    definities van het bestand, noch die van de modules die het met `use` gebruikt, **zijn al uitgevoerd**
    (net zoals `compile-file` van CL `eval-when` nodig heeft).
  - `read` / `read-from-string` binnen een programma evalueren `#.` ook (zoals in CL).
  - `*read-eval*` (standaard `true`) op `false` zetten maakt `#.` overal een leesfout: een schakelaar om
    te voorkomen dat als data gelezen tekst code uitvoert (zoals in CL). Hij wordt bij elke `#.`
    geraadpleegd, dus een `setf` werkt vanaf de volgende gelezen vorm. Binnen
    `with-standard-io-syntax` is hij `true`.
- **Booleans**: `true` / `false`.
- **Gehele getallen**: decimaal (`42`, `-7`). Er mag eerst een teken `+`/`-` komen. Andere grondtallen
  worden geschreven met de radixsyntaxis van CL `#b`/`#o`/`#x`/`#NNr` (het teken komt na de markering:
  `#x-ff`). Het voorvoegsel `0x` staat niet in CL en is niet overgenomen: `0xff` wordt als symbool
  gelezen.
  Een geheel getal als literal zonder typeannotatie is standaard `int` (willekeurige precisie,
  [Getallen](functions/numbers.md#3-gehele-getallen-met-willekeurige-precisie-int)), zonder bovengrens aan
  zijn grootte. **Als het verwachte type een geheel type met vaste breedte is, krijgt de literal dat type,
  en wordt gecontroleerd of het type de waarde kan bevatten**: `(the u8 300)` is een typefout (wil je het
  afkappen, schrijf dan `(as u8 300)`). `(the u32 4294967295)` en `(the u32 #xFFFFFFFF)` kunnen dankzij
  deze regel worden geschreven. Of een `int`-waarde in een directe waarde van 63 bits past of een bignum
  wordt, wordt door haar grootte bepaald, zonder speciale syntaxis (zoals in CL).
- **Drijvendekommagetallen**: die welke een decimaalteken of een exponent (`e`/`E`) bevatten (`1.5`,
  `3.0e10`). Standaard `f64` (`f32` als dat het verwachte type is).
- **Ratio's**: `teller/noemer` (alleen decimaal, bijvoorbeeld `1/3`). Bij het lezen vereenvoudigd, zoals
  CL voorschrijft (`2/4` is `1/2`). Die met een geheel getalwaarde (`4/2` enzovoort) worden als `int`
  gelezen, niet als `ratio`. Een noemer van nul (`1/0`) is een leesfout.
- **Tekens**: `#\` gevolgd door één teken of een tekennaam. Bijvoorbeeld `#\a` `#\Space` `#\Newline`
  `#\Tab` `#\Return` `#\Page` `#\Nul` (ook `#\Null`) `#\Backspace`. Namen zijn niet hoofdlettergevoelig.
- **Strings**: `"..."`. De escapes zijn `\n` `\t` `\r` `\0` `\\` `\"` (elke andere `\x` is gewoon `x`).
- **Symbolen**: elk token dat letters, cijfers en symbolen bevat (`+` `<=` `my-func` enzovoort).
- **Keywords**: symbolen die met een dubbele punt beginnen, zoals `:name` (zoals in CL). Ze evalueren naar
  zichzelf: ze zoeken geen binding op en hun waarde is zijzelf, met statisch type `symbol`. Keywords met
  dezelfde naam zijn altijd hetzelfde object (`(eq :foo :FOO)` is waar; net als andere symbolen worden ze
  in kleine letters gezet). De dubbele punt zelf maakt deel uit van de naam, dus `(symbol->string :foo)`
  is `":foo"` (typelisp heeft geen packagesysteem, dus dit verschilt van `symbol-name` van CL). Een
  losse `:` of een met extra dubbele punten zoals `:a:b` is een leesfout. Test met `keywordp`. Die welke
  met `::` beginnen zijn geen keywords maar absolute paden (hieronder).
  Merk op dat `:dyn` een gereserveerd keyword is, alleen voor typeposities; het ergens anders schrijven is
  een fout (zie [hoofdstuk 2](#2-types-schrijven)).
- **Lijsten**: `(a b c)`. Gestippelde paren `(a . b)` kunnen ook worden gelezen.
- **De lege lijst `()`**: afhankelijk van de context de waarde van het type `Unit` of de `none` van
  `Option<Sexpr>`. **`Sexpr` heeft geen variant voor de lege lijst**: `Sexpr` betekent "een niet-lege
  S-expressie", en het type van S-expressiedata is `Option<Sexpr>` (zie "Patronen voor
  `Option<Sexpr>`" in [4.3 match](#43-match--patroonherkenning)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (alleen zinvol binnen een quasiquote)
  - `,@x` → `(unquote-splicing x)` (bij expansie als lijstelementen samengevoegd)
- **Paden `::`**: `foo::bar` wordt gelezen als een pad door modules, types en leden (niet als één
  symboolnaam). Eén die met `::` begint, zoals `::foo`, is een absoluut pad vanaf de root. Een `::` binnen
  generieke argumenten (`Vec<a::b>` en dergelijke) wordt niet als padscheidingsteken behandeld.

## 2. Types schrijven

In de broncode worden types als gewone symbolen of lijsten geschreven.

- **Primitieve types**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` is het gehele type (integer van CL, dat automatisch tussen directe waarden van 63 bits
  en bignums beweegt; [Getallen](functions/numbers.md#3-gehele-getallen-met-willekeurige-precisie-int)),
  en de zes types met vaste breedte zijn genoemd naar hun breedte en tekenstatus (er is geen geheel type
  van 64 bits; zie [Getallen](functions/numbers.md#1-gehele-getallen-met-vaste-breedte)).
- **Het rationale type**: `ratio` (rationale getallen in laagste termen). Op de heap gealloceerd zoals in
  CL, zonder impliciete conversie met `int`/`f64` en dergelijke (converteer expliciet met `as`/`try-as` of
  een conversiemethode; zie [Getallen](functions/numbers.md#5-rationale-getallen-ratio)).
- **Ruwe woorden aan de C-grens**: `ptr` (een ondoorzichtige pointer), `c-long` / `c-ulong`. Alleen voor de
  FFI: er een tot waarde maken vereist `(unsafe ...)`, en de plekken waar ze mogen voorkomen zijn beperkt
  ([3.3 defffi](#ptr--c-long--c-ulong--ruwe-machinewoorden)). Gebruik deze niet waar je een geheel getal van
  64 bits wilt: ze hebben geen rekenkunde.
- **Ondoorzichtige veranderlijke types**: `random-state` (de toestand van een generator voor willekeurige
  getallen). Het kan niet in `Vector<T>`/`HashTable<K,V>`/`Sexpr` (het kan wel in
  `Option<T>`/`Result<T,E>`).
- **Het type Unit**: `()`
- **Het type Never**: `!` (het type van divergerende expressies zoals `panic`/`unreachable`/`todo`/een lus
  die nooit terugkeert. Het past bij elk verwacht type)
- **Functietypes**: `(fn (argumenttypes...) returntype)`. Het type van een functie met variadische
  argumenten is `(fn (argumenttypes... &rest elementtype) returntype)`.
- **Generieke types**: `Name<T1,T2,...>` (gelezen als één token zonder spaties).
  Bijvoorbeeld `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Het eenheidstype `()` kan ook als typeargument worden geschreven (`Result<(), FileError>`). `(`/`)` zijn
  normaal scheidingstekens die een token beëindigen, maar zolang een punthaak open is, wordt dit ene paar
  tekens doorgelaten. `()` kan ook als veldtype of argumenttype worden gebruikt.
- **De toepassingsvorm van generieke types**: `(Name T1 T2 ...)`, een lijstschrijfwijze die hetzelfde type
  noemt als `Name<T1,T2,...>`. Bijvoorbeeld `(vector char)` is hetzelfde als `Vector<char>`.
  De naamvorm is de gebruikelijke manier om het te schrijven; deze vorm **bestaat voor wanneer een
  typeargument niet binnen een naam kan worden gespeld**: een typeargument is zelf een type-expressie,
  maar binnen een naam van één token kunnen alleen namen, `()` en `:dyn` worden geschreven, geen
  functietypes (er is geen schrijfwijze als `Vector<(fn (i32) i32)>`). Het kan ook in deze vorm
  verschijnen wanneer de implementatie een type toont, zoals het resultaat van het substitueren van een
  geassocieerd type van een trait in een signatuur.
- **Gekwalificeerde typenamen**: kunnen met `::` worden gekwalificeerd, zoals in `module::Type`.
- **Trait-objecttypes**: `:dyn Trait` (twee door een spatie gescheiden woorden die één type vormen).
  Stelt een waarde voor waarvan het concrete type tijdens runtime wordt bepaald; aanroepen van
  traitmethoden gaan via een vtable (dynamische dispatch). Voor een trait met geassocieerde types worden
  die positioneel in declaratievolgorde vastgelegd (`:dyn Iter<i32>` legt `Item` vast op `i32`). Het kan
  ook binnen generieke argumenten worden geschreven: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. Concrete waarden worden op verwachte plekken automatisch in een
  box gezet; de expliciete vorm is `(as :dyn Trait expr)`.
  Een waarde van `:dyn Sub` kan zoals ze is worden doorgegeven waar een `:dyn Super` van een van zijn
  supertraits (alles waarvan hij erft, transitief) wordt vereist (upcasting). Hij kan niet aan een niet
  verwante trait worden doorgegeven.
  Voor de voorwaarden waaraan een trait moet voldoen om met `:dyn` te worden gebruikt, zie
  [3.9 deftrait / impl](#39-deftrait--impl--traits). `:dyn` buiten een typepositie schrijven is een fout.
- Ingebouwde generieke types: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, en de gelijktijdigheidstypes `Task<T>` / `Thread<T>` / `Chan<T>`
  ([hoofdstuk 12](#12-gelijktijdigheid-taken)). Er is ook `Sexpr`, het type van S-expressiedata. De
  ingebouwde concrete foutentypes zijn `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, en de standaardbibliotheek heeft de structs `SimpleError` / `WrappedError`
  (`Error` is geen type maar een trait: gebruik het als `:dyn Error`). De lijst staat in
  [types.md](types.md).
- **Types en traits delen één namespace** (zoals in Rust): binnen één module kunnen een type
  (`defstruct`/`defenum`) en een trait (`deftrait`) niet dezelfde naam hebben.

## 3. Definities op het hoogste niveau

### 3.1 defun — functiedefinities

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- De argumenttypes en het returntype zijn verplicht.
- Een generieke functie schrijft haar typeparameters tussen punthaken na haar naam:
  `(defun name<T1,T2...> (params) Ret body...)` (dezelfde punthaaksyntaxis als `Vector<T>` op
  typeposities).
- `defun`/`lambda`/`defmethod` accepteren variadische argumenten wanneer aan het einde `&rest (name Type)`
  wordt geschreven: `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (in de body is `xs` altijd
  gebonden als een `Option<Sexpr>`, een S-expressielijst. Elk werkelijk argument bij de aanroep wordt
  afzonderlijk als `Type2` getypechecked en daarna in een `Sexpr` gewikkeld).
  `defmacro` heeft ook een eigen `&rest`, maar verschilt doordat het altijd een ongetypeerde `Sexpr` is
  (`defun`/`lambda` geven het elementtype op). Een functietype kan ook een variadische functie
  beschrijven, als `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (voor `defun` en `defmethod`; niet voor `lambda`/`labels`, om de reden
  hieronder, en `defmacro` heeft een aparte implementatie, ook hieronder). De volgorde is die van CL:
  `required &optional &rest &key`. Elke parameter wordt geschreven als `(name Type)` of
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; no default
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> in the body

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; with a default
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; the caller writes `:name value`, in any order; omitted ones take their defaults
  ```

  - **Een parameter zonder standaardexpressie heeft het type `Option<Type>`.** Weggelaten is hij `none`;
    doorgegeven wordt de kale waarde die de aanroeper schreef automatisch in `some` gewikkeld. Wat CL met
    een supplied-p-variabele doet ("is hij opgegeven?") verschijnt in plaats daarvan aan de kant van het
    statische type.
  - Met een standaardexpressie blijft het type `Type` zoals gedeclareerd. Wanneer weggelaten, wordt die
    **gecontroleerde expressie** zoals ze is bij de aanroep ingebed (bij elke aanroep geëvalueerd).
  - **`&key` kan niet met `&optional`/`&rest` in één argumentenlijst worden gemengd.** Dit vermijdt een
    dubbelzinnigheid die CL zelf heeft (of een afsluitend werkelijk argument door een positionele
    `&optional` wordt genomen of op label als `&key` wordt gematcht hangt af van de *waarden*) door de
    combinatie te verbieden. `&optional` en `&rest` kunnen samen worden gebruikt.
  - Ze kunnen in generieke functies worden gebruikt, maar **een typeparameter die alleen in weggelaten
    argumenten voorkomt kan niet worden afgeleid en is een fout** (er is geen waarde om mee te matchen).
  - **`defmethod` kan dezelfde drie secties hebben** (zowel voor instantiemethoden als voor statische
    functies). Som `&optional`/`&rest`/`&key` op na de ontvanger:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; static function
    (point::origin :y 7)
    ```

    Ze kunnen ook in methoden van generieke types worden gebruikt, maar **het type van een parameter
    met een standaardexpressie mag de typeparameters van de eigenaar niet noemen** (dezelfde beperking
    die `defun` heeft voor haar eigen typeparameters: wat wordt ingebed wanneer het argument wordt
    weggelaten is een *gecontroleerde* expressie, dus haar type kan niet als abstracte variabele blijven
    staan).
  - **Ze kunnen niet in traitmethoden worden gebruikt.** `deftrait` heeft er geen syntaxis voor, en als
    alleen de `impl`-kant secties kon declareren, zouden aanroepen met een `:dyn`-ontvanger (argumenten
    invullen uit de declaratie van de trait) en aanroepen met een concrete ontvanger (invullen uit de
    declaratie van de `impl`) verschillende dingen worden. De arity van een vtable-slot staat vast.
  - **Ze kunnen niet in `lambda` / `labels` worden gebruikt** (`&rest` wel). Om een weggelaten argument in
    te vullen moet de aanroeper **de gecontroleerde standaardexpressie van de aangeroepene** lezen, die
    alleen beschikbaar is uit een signatuur die op naam is opgelost. Een `lambda` wordt als waarde
    doorgegeven, en het enige dat die waarde beschrijft is haar functietype `(fn ...)`: daarin is geen
    plek voor een expressie, en als die er was, zouden "twee lambda's met dezelfde signatuur maar
    verschillende standaardwaarden" verschillende types worden. `&rest` blijft binnen het domein van
    types, dus het kan in een functietype worden geschreven.
- **Voorwaartse verwijzingen worden met `defsignature` gedeclareerd** (hieronder). Een naam die niet is
  gedeclareerd kan niet vóór zijn definitie worden aangeroepen, omdat het hoogste niveau één vorm per
  keer wordt gecontroleerd en uitgevoerd, in bronvolgorde.
- Om trait bounds te eisen, schrijf je direct vóór de body een `where`-clausule:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (een geassocieerd type vastleggen met `(AssocName ConcreteType)` is optioneel).
- **Docstrings**: een stringliteral aan het begin van de body, direct na de `where`-clausule (als die er
  is), wordt de docstring (zoals in CL). Alleen wanneer er minstens één bodyvorm op volgt: een losse
  string blijft de returnwaarde en wordt niet als docstring genomen: `(defun f () string "doc" "value")`
  heeft een docstring en geeft `"value"` terug, terwijl `(defun f () string "value")` geen docstring heeft
  en `"value"` teruggeeft. Hij kan worden opgehaald met `(documentation name)`
  ([docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — voorwaartse declaraties

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Om een `defun` aan te roepen die **later** dan jezelf is gedefinieerd, declareer je hem eerst zo. Onderling
recursieve functies kunnen alleen zo worden geschreven:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

De argumenten worden **alleen als types** opgesomd; er is geen body, dus er valt niets te benoemen.
`&rest` kan als laatste worden geschreven, als `&rest elementtype`.

Declaraties **worden gecontroleerd**:

- De definitie die volgt moet overeenkomen met de declaratie (het aantal en de types van argumenten, het
  returntype, `&rest` en of hij `pub` is). Een afwijking is een fout bij de definitie.
- Declareren zonder te definiëren is een fout (gemeld wanneer het bestand / de module klaar is met laden).
  De REPL meldt het niet na elke invoer, omdat een declaratie en haar definitie op aparte regels moeten
  kunnen worden getypt.
- Een declaratie die **na** de definitie wordt geplaatst is een fout, aangezien zo'n declaratie niets kon
  doen.

Drie dingen kunnen niet worden gedeclareerd:

- **Generieke functies.** Een kopie per type maken vereist de body, en een declaratie heeft er geen. Een
  voorwaartse aanroep zou kunnen worden opgelost maar de instantiatie zou mislukken, dus de declaratie
  wordt vooraf geweigerd.
- **`&optional`/`&key`.** Hun signatuur bevat de **gecontroleerde** expressie van elke standaardwaarde
  (bij de aanroep ingebed wanneer het argument wordt weggelaten), en een declaratie heeft er geen plek
  voor.
- **Alles behalve `defun`.** Een `defmacro` heeft nodig dat de macrobody **al is uitgevoerd** om te kunnen
  expanderen, wat het registreren van een signatuur niet kan vervangen. Voor types
  (`defstruct`/`defenum`/`deftrait`) is het registreren ervan "wat de code die het type zelf registreert
  nodig heeft", wat niet zelfstandig is zoals een signatuur. Een `defmethod` wordt op het type
  geregistreerd dat hem bezit, dus hij volgt het type.

De tegenhanger in CL is `(declaim (ftype (function (i32) bool) even2))`, maar dat komt met een heel
declaratiesysteem en is slechts **adviserend**. Hier worden declaraties met statische typering
gecontroleerd.

### 3.3 defffi — C-functies declareren (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = the C symbol name
(pub defffi ...)
```

Declareert een C-functie zodat ze kan worden aangeroepen. De vorm is dezelfde als bij `defsignature` (een
naam, argumenttypes, een returntype en geen body), maar geen body hebben betekent iets anders.
`defsignature` is een belofte dat "ik hem later zal definiëren", terwijl `defffi` declareert dat "iemand
anders de body al heeft geschreven en gecompileerd".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

De typelisp-naam en de naam van het C-symbool kunnen apart worden geschreven omdat typelisp-identifiers
meestal `-` bevatten en C-identifiers dat niet kunnen. Als de C-naam wordt weggelaten, wordt de naam zoals
hij is als C-symboolnaam gebruikt.

**Aanroepen vereisen `(unsafe ...)`** (ook voor functies die alleen op scalairen werken). De compiler kan
niet bevestigen dat de gedeclareerde C-signatuur overeenkomt met de echte en kan alleen de declaratie
vertrouwen; `unsafe` is het teken dat je die verantwoordelijkheid op je neemt. De bedoelde manier is om
het één keer te wikkelen en er een veilige wrapper van te maken:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; no unsafe needed from here on
```

De types die kunnen worden geschreven zijn `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, en getypeerde pointers `(ptr T)`
([hieronder](#def-c-struct-en-getypeerde-pointers--c-structs-alloceren)).

`string` is `const char *`. typelisp-strings zijn niet met NUL afgesloten en kunnen zelf NUL bevatten, dus
**ze worden bij het doorgeven naar een C-string gekopieerd** en na de aanroep vrijgegeven. Een NUL in de
string is een fout: C zou er alleen tot kijken, dus er zou stilzwijgend een andere string worden
doorgegeven.

**Teruggegeven strings worden ook gekopieerd**, en niet vrijgegeven: wat C teruggeeft behoort aan C, en
het kan naar een statische tabel wijzen, zoals bij `getenv`. Functies die geheugen teruggeven dat de
aanroeper moet vrijgeven (`strdup` enzovoort) moeten als `ptr` worden genomen en door jezelf worden
vrijgegeven.

Functies waarvan het resultaat binnen een argument wijst (`strchr`, `strstr`) werken ook correct: het
resultaat wordt gekopieerd voordat het argument wordt vrijgegeven.

Als een functie die als `string`-teruggevend is gedeclareerd NULL teruggeeft, is dat een fout, omdat
`string` geen waarde heeft die "er was er geen" betekent. Als NULL mogelijk is, neem het resultaat dan als
`ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Met `:library` wordt die gedeelde bibliotheek geopend en het symbool erin opgezocht. Zonder wordt het
symbool in **het proces zelf** opgezocht (alles wat al is gelinkt, inclusief libc). Een korte naam zoals
`sqlite3` wordt in die volgorde gezocht als `libsqlite3.dylib` / `libsqlite3.so`, en een naam die `/`
bevat wordt als pad behandeld. Geopende bibliotheken worden nooit gesloten: code die naar hun functies
wijst blijft draaien, dus de enige juiste levensduur is die van het proces.

#### ptr / c-long / c-ulong — ruwe machinewoorden

`ptr` is een ondoorzichtige pointer (`void *`, `FILE *`, wat de declaratie ook bedoelde). `c-long` /
`c-ulong` zijn `long` / `unsigned long` van C (ook `size_t`, `int64_t` en `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Ze niet `i64` / `u64` noemen is bewust.** Deze taal heeft geen geheel type van 64 bits, omdat een
getagde directe waarde maar 63 bits heeft ([hoofdstuk 2](#2-types-schrijven)). De naam `c-long` zegt
"dit is een woord dat de grens met C overschrijdt, geen geheel getal van deze taal".

**Ze hebben geen rekenkunde.** `(+ x 1)` kan niet worden geschreven. Het zou kunnen worden geboden maar
is dat niet, zodat er geen berekening draait op een waarde die nergens kan worden opgeslagen en een andere
breedte heeft dan elk ander getal, om dezelfde reden dat het gehele type van 64 bits is weggelaten. Er
zijn **alleen conversies**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; read what came back
(as int (unsafe (c-strlen s)))               ; this one to read it exactly (int does not lose 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; ask whether it fits
(as c-ulong n)                               ; make one from another integer
```

**Literals** van gehele getallen nemen het verwachte type aan, dus er is geen `as` nodig om er zomaar een
door te geven:

```lisp
(unsafe (c-malloc 16))                       ; 16 is read as a c-ulong
```

Literals buiten het bereik worden geweigerd zoals bij andere breedtes (`(c-malloc -1)` past niet in een
`c-ulong`).

**De plekken waar ze mogen voorkomen zijn beperkt**: alleen argumenttypes, returntypes en lokale
variabelen. Elk van het volgende is een fout:

```lisp
(defstruct handle (p ptr))          ; a struct field
(defenum maybe (none) (some ptr))   ; an enum field
(defvar (block ptr) ...)            ; a global
(defffi f ((vector ptr)) i32)       ; inside a type argument
```

Er is voor alle één reden: **de slot tagt wat hij bevat**. Taggen zou de bovenste bits van de pointer
laten vallen, om dezelfde reden dat het gehele type van 64 bits is weggelaten, dus het is ook binnen
`unsafe` niet toegestaan. Dit is geen kwestie van toestemming: die representatie bestaat niet.

Om dezelfde reden kunnen ze geen lokale variabelen zijn die door geneste functies worden **vastgelegd**
(een vastgelegde binding gaat in een cel, en een cel tagt wat hij bevat). Dit is bij het compileren
bekend en wordt door `(compile f)` gemeld.

De GC traceert `ptr` niet. Hij wijst buiten de heap, dus dat is correct.

Vier dingen kunnen niet worden gedeclareerd:

- **Variadische argumenten** (`printf`). Het variadische deel wordt volgens andere regels doorgegeven
  dan de vaste argumenten (op de stack bij AArch64 Darwin), dus het kan niet correct vanuit een vaste
  signatuur worden aangeroepen. `&rest` wordt geweigerd.
- **Structs op waarde doorgeven of teruggeven.** Om dezelfde reden (het hangt af van de
  aanroepconventie van elk platform). De schrijfbare types zijn beperkt tot de bovenstaande lijst, dus
  het kan niet worden gespeld.
- **Generics.** C heeft geen tegenhanger.
- **Dezelfde naam als een ingebouwde functie.** Een gecompileerde aanroep zou die naam naar de
  ingebouwde functie oplossen, dus het wordt geweigerd in plaats van stilletjes mis te gaan.

#### Callbacks — C laten terugroepen

Een functietype `(fn (types...) returntype)` als argumenttype schrijven maakt van dat argument een
functie die C terugroept (een callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; a top-level function
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; a lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; a local function
```

Een C-functiepointer is niets anders dan een codeadres, en C roept hem aan met alleen de gedeclareerde
argumenten. Er is geen plek om vastgelegde variabelen door te geven, dus **alleen functies zonder vrije
variabelen kunnen worden doorgegeven**, en dit wordt tijdens het typechecken gecontroleerd.

- Schrijf een functienaam of een `lambda`-expressie **rechtstreeks** als werkelijk argument. Een
  variabele die een functie bevat kan niet worden doorgegeven: welke functie ze bevat, en dus of ze vrije
  variabelen heeft, is pas tijdens runtime bekend.
- Een `lambda` is een fout als ze naar lokale variabelen daarbuiten verwijst. Globale variabelen en
  functies op het hoogste niveau mogen worden gebruikt.
- Een lokale functie (`labels`) mag geen vrije variabelen hebben, ook niet die van de zusterfuncties die
  ze aanroept. Zusterfuncties delen de plek waar vastgelegde variabelen worden bewaard, dus wat een
  aangeroepen zusterfunctie vastlegt wordt ook door deze functie vastgelegd.
- Een generieke functie krijgt haar types uit het gedeclareerde functietype.
- De types die in het functietype kunnen worden geschreven zijn dezelfde als de bovenstaande lijst.
  `string` kan echter niet het returntype van een callback zijn (het zou C geheugen geven dat niemand
  vrijgeeft). Een `string`-argument kopieert de string die C doorgaf naar een typelisp-string.

C-functieaanroepen kunnen alleen binnen `unsafe` worden geschreven, dus callbacks kunnen alleen binnen
`unsafe` worden doorgegeven.

**De callback kan alleen worden aangeroepen terwijl de C-functie die typelisp aanriep draait.** Wordt hij
ergens anders vandaan aangeroepen (een thread die geen typelisp draait, een signal handler, een functie
die met `atexit` is geregistreerd), dan drukt hij de reden af en stopt het proces.

**Fouten planten zich niet voort door C.** Een `panic` of `throw` binnen de callback kan niet door
C-frames worden afgewikkeld (dat zou ongedefinieerd gedrag zijn), dus er wordt 0 aan C teruggegeven, en de
fout wordt opnieuw aan de aanroeper gegooid wanneer de C-functie terugkeert. Als de callback tussen de
fout en de terugkeer van de C-functie opnieuw wordt aangeroepen, wordt hij niet uitgevoerd en wordt 0
teruggegeven.

Een bewerking die binnen een callback zou moeten wachten (een `recv` op een leeg kanaal enzovoort) is een
fout ([12.6](#126-gecompileerde-code-en-taken)).

Wanneer een functie opnieuw wordt gedefinieerd, wordt de nieuwe definitie aangeroepen vanaf de volgende
keer dat ze aan C wordt doorgegeven.

Met AOT (`compile-file`) werkt het op dezelfde manier. De ingangspunten die C aanroept worden in het
uitvoerbare bestand ingebouwd.

**Ze kunnen niet als waarden worden doorgegeven.** Een FFI-declaratie kan niet zoals ze is worden
geschreven voor de `f` van `(map f xs)`: een functiewaarde is een closure die de body van een definitie
omhult, en deze declaratie heeft geen body om te omhullen. Wikkel haar in een `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` wordt ook geweigerd: wat kon worden getoond is de machinecode van C, die deze
compiler niet heeft geproduceerd. `(compile c-abs)` slaagt (en doet niets, aangezien ze al is
gecompileerd).

**Het werkt ook met AOT (`compile-file`).** De linker lost de C-functies zelf op. Als een declaratie
`:library` heeft, wordt die bibliotheek als `-l` aan de linkregel toegevoegd (duplicaten worden tot één
samengevoegd), dus `compile-file` heeft geen extra argumenten nodig. `compile-file` zelf leest de
broncode, dus het kan ze uit de declaraties verzamelen.

Symbolen worden ook bij het bouwen opgezocht. Als een gedeclareerde functie niet bestaat, noemt de fout
haar vóór enige linkfout.

De standaardbibliotheek (de prelude) gebruikt `defffi` niet. De standaardbibliotheek gaat als geheel in
elk uitvoerbaar bestand, dus een declaratie met `:library` daar zou die bibliotheek zelfs linken in
programma's die de FFI niet gebruiken.

#### def-c-struct en getypeerde pointers — C-structs alloceren

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Declareert een struct met dezelfde indeling als in C. Ze kan alleen binnen een `unsafe` op het hoogste
niveau worden geschreven (die niets anders dan `def-c-struct`s mag bevatten). Direct na de naam kan een
docstring worden gezet.

De types die voor velden kunnen worden geschreven zijn `i8` `i16` `i32` `u8` `u16` `u32` `c-long`
`c-ulong` `f32` `f64` `bool` `ptr`, getypeerde pointers `(ptr T)`, en andere `def-c-struct`s (op waarde
ingebed). De indeling (de offset van elk veld, en de grootte en uitlijning van de struct) wordt volgens de
regels van C berekend (uitgaande van LP64). Een veld dat naar de struct zelf wijst kan worden geschreven,
maar de struct kan zichzelf niet insluiten.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x at 0, y at 8, size 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

De naam van een `def-c-struct` gaat in de typenamespace (geen `defstruct` of dergelijke met dezelfde naam
kan in dezelfde module staan), maar **het is niet het type van een waarde**. Je kunt
`(defun f ((p point)) ...)` niet schrijven; hij verschijnt alleen als datgene waar een getypeerde pointer
naar wijst.

**Een getypeerde pointer `(ptr T)`** is een adres dat naar een `T` wijst. `T` is een van de types die
hierboven voor velden kunnen worden geschreven. Het is een ruw machinewoord zoals `ptr`, met dezelfde
regels voor waar het mag voorkomen (alleen argumenten, returntypes en lokale variabelen; het kan alleen
binnen `unsafe` een waarde zijn).

Alloceren, lezen en schrijven worden in de volgende vormen geschreven. Ze kunnen allemaal alleen binnen
`unsafe` worden gebruikt.

| Vorm | Betekenis |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Alloceert `n` waarden van `T` (1 als weggelaten). De inhoud wordt met 0 gevuld. Geeft een `(ptr T)` terug |
| `(c-ref p i)` | Een pointer naar element `i` vanaf `p`. Een fout als buiten het gealloceerde bereik |
| `(c-deref p)` / `(setf (c-deref p) v)` | Leest / schrijft de scalar waar `p` naar wijst |
| `p::field` / `(setf p::field v)` | Leest / schrijft een veld van een struct. Een veld lezen dat een ingebedde struct is geeft zijn adres (`(ptr inner-type)`) |
| `(as ptr p)` | Vergeet het type en maakt een `ptr` (om door te geven aan iets als de `void *` van `qsort`). Er is geen conversie terug |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Gealloceerd geheugen wordt vrijgegeven wanneer de besturing de `unsafe` verlaat die het alloceerde.**
De eigenaar is de lexicaal buitenste `unsafe` binnen dezelfde functie. Het wordt vrijgegeven of de code
normaal eindigt of via `panic`, `throw` of `return-from` verlaat. `lambda`- en `labels`-functies zijn
aparte functies, dus een `c-alloc` daarin heeft een eigen `unsafe` daarbinnen nodig.

Hierdoor kan een getypeerde pointer de `unsafe` die hem alloceerde niet verlaten. Elk van het volgende is
een typefout:

- Hem de waarde van de `unsafe`-expressie maken (dus hij kan ook niet uit een functie worden
  teruggegeven)
- Hem in een closure vastleggen (`lambda`, `labels`)
- Hem aan `task` / `thread` doorgeven
- Hem met `throw` gooien

Om waarden buiten de `unsafe` te gebruiken, kopieer je ze binnen de `unsafe` naar een `defstruct` of
getallen en geef je die terug.

**Geheugen dat aan de C-kant is gealloceerd wordt niet behandeld.** Waarden die als getypeerde pointers
vanuit C binnenkomen (returnwaarden van `defffi`, callback-argumenten, waarden gelezen uit velden met
pointertype) worden tijdens runtime gecontroleerd of ze binnen een levende `c-alloc`-allocatie naar een
waarde van dat type wijzen, en zijn anders een fout. NULL is ook een fout. Om geheugen te ontvangen dat C
alloceerde, of NULL, gebruik je de ongetypeerde `ptr` (waarvan de inhoud niet kan worden gelezen).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Wanneer het argument van een callback door de controle wordt geweigerd, wordt dat aan de aanroeper
gemeld wanneer de C-functie terugkeert, net als een fout binnen een callback.

### 3.4 defvar / defparameter / defconstant — globale variabelen

```lisp
(defvar (name Type) init-expr)        ; initializes only if not yet bound
(defparameter (name Type) init-expr)  ; assigns every time
(defconstant (name Type) init-expr)

; with a docstring (in the same order as CL's defvar/defparameter/defconstant: after the value)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Het verschil tussen `defvar` en `defparameter` blijkt bij opnieuw laden** (zoals in CL). Als de
globale variabele **al gebonden is, evalueert `defvar` de initializer niet eens**, dus wanneer je een
instellingenbestand bewerkt en opnieuw leest, blijven de waarden die de sessie heeft gewijzigd zoals ze
zijn. `defparameter` wijst elke keer toe, dus opnieuw lezen brengt de waarden terug naar wat er staat.

De typeannotatie is verplicht (ze wordt niet uit de initializer afgeleid). `defvar` kan worden gewijzigd;
`defconstant` niet (`setf` is een fout).

### 3.5 defmethod — methodedefinities

```lisp
; instance method: can be called as (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / associated function: can be called as (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

De aanroeper lost de methode op uit het statische type van `obj` (enkelvoudige, statische dispatch). Een
docstring kan op dezelfde positie en onder dezelfde regels als bij `defun` worden geplaatst (direct na de
`where`-clausule, aan het begin van de body, alleen wanneer er bodyvormen op volgen). Hetzelfde geldt voor
methoden binnen `impl`; ze worden opgehaald met `(documentation Type::method)`.

### 3.6 defstruct — structs (door de gebruiker gedefinieerde types)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (type parameters in angle brackets)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Elk veld is `(name type)` of `(pub name type)` (zichtbaarheid per veld, onafhankelijk van de `pub` van de
  struct zelf). Eén extra expressie aan het einde wordt de **standaardwaarde** van de slot (`(x i32 0)`);
  zie de optielijst hieronder.
- Het volgende wordt automatisch gegenereerd:
  - De constructor `Name::new` (argumenten in veldvolgorde)
  - Getters `(field-name instance)`, met de suiker `instance::field-name`
  - Setters `(set-field-name instance value)`, met de suiker `(setf instance::field-name value)`
- Om de struct zelf `pub` te maken, zet je `pub` ervoor, zoals in `(pub defstruct ...)`.
- **Definieer een type voordat je het noemt.** Het type van een veld kan de struct zelf zijn
  (`(next Option<node>)`), maar niet een later gedefinieerd type: types hebben geen voorwaartse declaratie
  die overeenkomt met `defsignature`. Een nog niet gedefinieerde naam geeft dezelfde fout `unknown type`
  in het argumenttype van een `defun` of in `the`. Twee types die naar elkaar verwijzen kunnen dus niet
  worden geschreven.
- **Typevariabelen zijn alleen die welke op declarerende posities zijn geschreven.** Bij
  `defun`/`defstruct`/`defenum`/`deftype` de `<T>` van de naam; bij `defmethod` het type van de ontvanger
  (`(self box<T>)`, of `box<T>` voor een statische methode); bij `impl` het doeltype en `impl<T>`; bij
  `deftrait` `Self` en de geassocieerde types van `(type Item)`. Een naam die elders (argumenten, de
  returnwaarde, `the`/`lambda` in de body) voor het eerst verschijnt wordt geen typevariabele; het is
  `unknown type`.
- **Docstrings**: een stringliteral direct na de naam, vóór de velden, wordt de docstring
  (`(defstruct Name "doc" (field Type)...)`, dezelfde positie als `defstruct` van CL). Een veld heeft altijd
  de vorm `(name Type ...)` en kan nooit een kale string zijn, dus er is geen dubbelzinnigheid. Haal hem op
  met `(documentation Name)`.

#### Optielijst

Een lijst `(Name option...)` op de naampositie schrijven specificeert opties (dezelfde positie als in CL).

```lisp
(defstruct (point (:constructor make-point)          ; keyword constructor
                  (:constructor at (x &optional y))  ; BOA constructor
                  (:copier copy-point))
  (x i32 0)          ; a third element is that slot's default value
  (y i32 0))

(point::make-point :y 7)   ; x is 0
(point::at 1)              ; y is 0
(point::at 1 2)
(copy-point p)             ; a shallow copy (the same as CL's copier)
```

- **`:constructor`**: wat wordt gegenereerd is een **statische functie** van het type
  (`point::make-point`), waarvan de body altijd `(point::new ...)` is. `new` blijft de ene structurele
  constructor; wat hier wordt gemaakt is een *manier om hem aan te roepen*. Er kunnen er meerdere worden
  gedeclareerd.
  - `(:constructor name)` neemt elke slot als `&key`. **Elke slot heeft een standaardwaarde nodig** (deze
    taal heeft niets dat overeenkomt met de "niet-gebonden slot" van CL).
  - `(:constructor name (slot...))` neemt de genoemde slots als positionele argumenten (in willekeurige
    volgorde). Slots die niet worden genoemd worden met hun standaardwaarden gevuld, dus **ze hebben
    standaardwaarden nodig**. Na `&optional` mogen de rest worden weggelaten (en hebben eveneens
    standaardwaarden nodig).
- **`:copier`**: genereert een **instantiemethode** die een nieuwe waarde met dezelfde slotwaarden
  teruggeeft. Ondiep, zoals de copier van CL.
- **`:include Parent`**: voegt de slots van de ouder vooraan toe (standaardwaarden worden ook geërfd; de
  ouder mag in een ander bestand staan). **Het schept geen typerelatie**: het kind is geen subtype van de
  ouder, de methoden van de ouder zijn niet op het kind van toepassing, en er is geen runtimetest die de
  twee verbindt. Deze taal heeft geen subtypering; gemeenschappelijke interfaces zijn het werk van
  `deftrait`. Alleen de *lijst* met slots wordt samengevoegd.
- **Slotstandaardwaarden worden alleen door gegenereerde constructors gelezen.** Een standaardwaarde
  schrijven zonder enige `:constructor` te declareren is een fout, aangezien ze nooit kan worden gebruikt.
- Weggelaten opties, en waarom:
  - **`:conc-name`**: in CL voegt het een voorvoegsel aan accessors toe om botsingen in één vlakke
    functienamespace te voorkomen. Hier zijn accessors methoden die op het type van de ontvanger worden
    verzonden, dus botsingen komen niet voor, en een voorvoegsel zou `instance::field` breken (dat alleen
    de slotnaam kent).
  - **`:predicate`**: beantwoordt tijdens runtime "is deze waarde een `point`?". Hier zijn types een
    classificatie tijdens het compileren zonder runtimegetuige, en er is geen positie waar "een waarde van
    onbekend type die een point zou kunnen zijn" bestaat (`match` op `Sexpr` is verzegeld, en `:dyn` kan
    niet worden gedowncast), dus een gegenereerd predicaat zou alleen ooit `true` kunnen teruggeven.
  - **`:type` / `:initial-offset` / `:named`**: deze vervangen de representatie van de waarde door een
    lijst of vector. De representatie is van de compiler en kan vanuit de taal niet worden waargenomen.

### 3.7 defenum — enums (somtypes)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; a variant with a payload (positional fields)
  (Variant2)                  ; a variant without a payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Elke variant heeft de vorm `(VariantName FieldType...)`. Velden zijn alleen positioneel (ze hebben geen
  namen). Er is minstens één variant nodig, en namen mogen niet herhaald worden.
- Waarden worden gebouwd, zoals bij de ingebouwde `Option`/`Result`, gekwalificeerd of via `use`:
  `(Name::Variant1 a b)`, of `(Variant1 a b)` na `(use Name)`.
- Ze kunnen met `match` / `if-let` uit elkaar worden gehaald. `match` controleert volledigheid (hij moet
  elke variant dekken of een `_` hebben):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Methoden en geassocieerde functies worden achteraf met `defmethod`/`impl` toegevoegd, net als bij
  `defstruct`.
- Om de enum zelf `pub` te maken, schrijf je `(pub defenum ...)`.
- **Docstrings**: dezelfde positie en regels als bij `defstruct`, direct na de naam, vóór de varianten
  (`(defenum Name "doc" (Variant ...)...)`). Haal hem op met `(documentation Name)`.

### 3.8 deftype — typealiassen

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

`deftype` van CL, teruggebracht tot wat zin heeft in een statisch getypeerde taal: **een schrijfwijze van
een type, geen type**.

- De naampositie is dezelfde als bij `defun`, en generieke argumenten worden als `Name<T,U>` geschreven.
  Op de plek van gebruik is precies het gedeclareerde aantal typeargumenten nodig (te veel of te weinig is
  meteen een fout).
- Expansie gebeurt **binnen de typeparser**. Niets verderop weet dus dat de alias bestaat: de
  monomorfisatiesleutels, dumps, het compileerpad en **foutmeldingen** tonen allemaal de geëxpandeerde
  vorm. Als `(f "x")` faalt tegen een functie die `meters` vereist, zegt de melding `i32`.
- **Het is geen nieuw type.** `(deftype meters i32)` maakt van `meters` en `i32` hetzelfde type, dus het
  door elkaar halen wordt niet gevangen. Wil je ze gescheiden houden, gebruik dan `defstruct`.
- **Het is geen predicaat.** `(deftype small () '(integer 0 9))` van CL beschrijft een *verzameling
  waarden* die `typep` tijdens runtime test, maar hier zijn types een classificatie tijdens het compileren
  zonder runtimegetuige, dus een alias die waarden beperkt zou niets hebben om te beperken.
- **Het kan zichzelf niet bevatten.** Een alias wordt geëxpandeerd waar ze is geschreven, dus er is geen
  plek om naar te recurseren. Recursieve gegevenstypes worden met `defstruct`/`defenum` geschreven.
- Het deelt de namespace met types en traits (binnen één module kan het niet dezelfde naam hebben als een
  `defstruct`/`defenum`/`deftrait`). Maak het openbaar met `(pub deftype ...)` en haal het binnen met
  `(use m::meters)`.
- **Docstrings**: direct na de naam, vóór het type (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; the supertrait list is required; () if none
  (type AssocName)                       ; associated types (any number, optional)
  (method-name ((self Self) params...) RetType)          ; no body = must be implemented
  (method-name ((self Self) params...) RetType body...)) ; with a body = default implementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; bounds applying to the whole impl (optional)
  (type AssocName ConcreteType)          ; makes an associated type concrete
  (method-name (recv params...) RetType body...))
```

Via `impl` wordt elke methode als gewone `defmethod` van `TargetType` geregistreerd. Traits worden als
trait bounds genoemd in de `where`-clausules van generieke functies (zie
[3.1 defun](#31-defun--functiedefinities)). Een traitnaam kan ook een `::`-pad zijn zoals `m::Trait`.

**De supertraitlijst (verplicht)**: altijd direct na de traitnaam geschreven. Elk element is een kale
traitnaam, of, als die trait geassocieerde types heeft, `(Trait (Assoc Type))` met **al zijn geassocieerde
types vastgelegd**.

```lisp
(deftrait Eq () ...)                       ; no supertraits
(deftrait Ord (Eq) ...)                    ; Rust's trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; pinning an associated type
  (rewind ((self Self)) ()))
```

Overerving heeft drie effecten. (1) `impl Ord X` vereist dat `impl Eq X` **eerst** wordt geschreven (een
regel over de volgorde van schrijven: de enige vorm die in de REPL en bij stapsgewijs `load` deterministisch
kan worden bepaald, en strenger dan Rust). (2) `(where (Ord T))` alleen laat je ook de methoden van `Eq`
aanroepen. (3) De methoden van `Eq` kunnen via een `:dyn Ord` worden aangeroepen, en een `:dyn Ord`-waarde
kan zoals ze is worden doorgegeven waar een `:dyn Eq` wordt vereist (upcasting). Een subtrait die een
methode met dezelfde naam als zijn ouder opnieuw declareert, en het erven van methoden met dezelfde naam
van twee ouders, zijn beide fouten (een vtable heeft één slot per naam). Ruitvormige overerving wordt tot
één slot samengevoegd.

**Standaardimplementaties**: een body na de signatuur wordt gebruikt wanneer een `impl` de methode
weglaat. De body wordt opgelost in **de namespace van de module** waar de trait is geschreven, dus ze kan
niet-openbare functies van die module aanroepen. Methoden met bodies kunnen ook `where`-clausules en
docstrings hebben. De body wordt **één keer, op het punt van declaratie**, getypechecked, met `Self` als
typevariabele (begrensd door `Self: de trait zelf`), zoals in Rust: fouten die voor elke `impl` en elk
implementerend type zouden mislukken, zelfs in standaardwaarden die geen enkele `impl` ooit weglaat,
worden daar gevangen. Aanroepen op `self` van methoden van de trait zelf of zijn supertraits gaan door
deze bound, en geassocieerde types staan vast op zichzelf, dus een signatuur die `Item` teruggeeft wordt
tegen de body gematcht zonder het concrete type te kennen.

**Blanket-implementaties**: de target een typevariabele maken implementeert de trait in één keer voor elk
type dat aan de bounds voldoet.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; no body at all; everything is the default
```

**Er wordt geen code gegenereerd totdat een concreet type hem daadwerkelijk gebruikt** (één keer per type,
met hetzelfde mechanisme als gewone monomorfisatie). Een trait kan hoogstens één blanket-implementatie
hebben. Als een type een expliciete `impl` heeft, gaat die voor. Het typechecken van de body staat los van
het genereren: het gebeurt één keer op het punt van declaratie, **met de target als typevariabele** (zoals
in Rust), dus zelfs een implementatie die nooit wordt gebruikt heeft haar fouten daar gevangen als ze voor
elke target onder de gedeclareerde bounds zouden mislukken. Aanroepen die door de bounds worden
gerechtvaardigd (`(less self other)` onder `(where (Ord T))` enzovoort) slagen, zoals in de body van een
generieke `defun`.

**Docstrings**: een `deftrait` kan één docstring voor de hele trait hebben, als stringliteral direct na de
supertraitlijst, vóór de onderdelen (`(deftrait Name () "doc" (type ...) (method ...)...)`). Een signatuur
zonder body kan geen docstring hebben: een afsluitende string zou zelf de returnwaarde van een
standaardimplementatie zijn, dus de twee zouden niet te onderscheiden zijn.

De traits die de standaardbibliotheek biedt: **`Iter`** (`next` / geassocieerd type `Item`; de basis van
`doiter` en de sequentiefuncties), **`Eq`** (`equals`; `not-equals` is een standaardimplementatie),
**`Ord`** (erft `Eq`; alleen `less` moet worden geïmplementeerd, en `less-equal` / `greater` /
`greater-equal` zijn standaardimplementaties), **`Error`** (`message` / `source`; `:dyn Error` om
foutentypes uniform te behandelen), **`print-object`** (een afgedrukte weergave per type), **`Pathish`**
(pathname-designators: een string of een `pathname`), en de streamhiërarchie **`Stream`** →
**`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**.
Welke types welke traits implementeren staat in [types.md](types.md); de methoden van elke trait staan in
[Standaardtraits](functions/traits.md), [Foutentypes](functions/option-result.md#3-foutentypes-en-de-trait-error),
[print-object](functions/printing.md#5-print-object-afgedrukte-weergave-per-type) en
[Streams](functions/streams-files.md). Als je `Iter` voor je eigen collectietype met `impl` implementeert,
werken `doiter` (hoofdstuk 5) en `map` / `filter` / `sort` en dergelijke er direct op.

Traitaanroepen zijn standaard **statisch** (opgelost via het statische type van de ontvanger). Om waarden
te behandelen waarvan het concrete type tijdens runtime wordt bepaald, geeft het trait-objecttype
`:dyn Trait` (hoofdstuk 2) dynamische dispatch via een vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; one call site, an answer per implementation
```

Alleen traits waarvan "elke methode een `self`-ontvanger heeft, `Self` nergens anders dan in de ontvanger
gebruikt, en zelf noch generiek noch variadisch is" kunnen `:dyn` worden gemaakt (geërfde methoden moeten
aan dezelfde voorwaarden voldoen).

Alleen types waarvan de waarden een representatie op de heap hebben kunnen in een `:dyn`-box:

| Kan erin | Kan er niet in |
|---|---|
| `defstruct`- / `defenum`-types (inclusief `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` en de structs van de standaardbibliotheek), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Gehele getallen met vaste breedte (`i8` tot en met `u32`), `f32`, `bool`, `char`, `symbol`, `()`, functietypes, en `Option<T>` zonder box ([de runtime-representatie van Option](functions/option-result.md#2-de-runtime-representatie-van-optiont)) |

Een waarde van een type dat er niet in kan plaatsen waar een `:dyn` wordt verwacht is een typefout. Om
zulke waarden via `:dyn` te behandelen, wikkel je ze in een struct, zoals in `(defstruct flag (v bool))`.

### 3.10 module / use — namespaces

```lisp
(module path body...)      ; path is a sequence of segments such as foo or foo::bar
(in-module path)           ; from here to the end of this unit, inside path (the flat form of module)
(use path...)              ; alias functions, types and modules into the current namespace
(import path...)           ; the same as use (a CL-compatible spelling)
(shadowing-import path...) ; a use that knowingly takes a bare name already in use
```

- `module` maakt een namespace. **Types zijn geen namespaces** (zoals in Rust heeft een type alleen
  geassocieerde functies en methoden).
- Een type met `use` binnenhalen maakt zijn constructors en openbare statische methoden ook op kale naam
  beschikbaar (bijvoorbeeld na `(use option)` kunnen `some`/`none` zonder `option::some`/`option::none`
  worden aangeroepen).
- De oplossingsvolgorde van kale namen (ongekwalificeerde identifiers): speciale vormen → constructors →
  vrije functies (huidige namespace → root) → instantiemethoden (opgelost via het statische type van het
  eerste argument). Het gaat niet omhoog door tussenliggende oudermodules.
- Een gekwalificeerd pad `a::b` lost `a` op in de bovenstaande volgorde; als het een module is, gaat het
  erin, en als het een type is, wordt het laatste segment als geassocieerd onderdeel opgelost.
- **`use` werkt op de vormen erna.** Een bestand wordt één vorm per keer gelezen, en afhankelijkheden
  worden opgelost vlak voordat de vorm wordt gecontroleerd, dus `m::f` **boven** `(use m)` schrijven geeft
  `unresolved path`. Zet `use` bovenaan het bestand.
- **`use` kan meerdere paden nemen** (`(use a::f b::g)`). `import` is een met CL compatibele schrijfwijze
  met hetzelfde gedrag.
- **Een `use` waarvan de kale naam al bezet is, wordt gemeld.** Het oplossen van een kale naam kijkt
  vóór aliassen naar de eigen definities van de module, dus `(use m::twice)` na `(defun twice ...)`
  **doet niets**. Bedoel je het toch, schrijf dan `shadowing-import` (het kan een definitie nog steeds niet
  verslaan, aangezien er geen manier is om er een te verwijderen; het verslaat alleen eerdere aliassen).
- **`in-module` is de platte vorm van `(module path body...)`.** `(in-module geometry)` schrijven zet alles
  vanaf daar tot het einde van de eenheid (het bestand, of de body van de omsluitende `module`) binnen
  `geometry`. Het gaat **binnen** de eigen module van het bestand (`main::geometry` voor `main.typl`). Twee
  achter elkaar nesten in volgorde. Het verschilt van `in-package` van CL en heeft een andere naam: in dit
  systeem is het bestand al een module, dus er valt niets te "selecteren", en alles wat een vorm kan doen
  is nesten.

### 3.11 Bestanden en modules (projecten met meerdere bestanden)

Het bestandspad ten opzichte van de bronroot is het modulepad:
de inhoud van `<root>/geo/point.typl` wordt impliciet in de module `geo::point` gewikkeld
(een map is ook een segment, in de stijl van Rust / Python). Een expliciete `(module bar ...)` in het
bestand nest **daarbinnen** (`geo::point::bar`), dus het afgeleide pad en een expliciete declaratie botsen
nooit.

- **Bronroot**: zet een manifestbestand `typelisp.toml` in de root van het project (het mag leeg zijn;
  optioneel noemt één regel `src = "src"` de bronmap). Het wordt gevonden door vanaf de map van het
  doelbestand omhoog te lopen. Zonder manifest is de map van het invoerbestand (de huidige map voor de
  REPL) de root.
- **Laden op aanvraag**: wanneer `(use geo::point)` naar een nog niet geladen module verwijst, wordt het
  bijbehorende bestand (`geo/point.typl`) automatisch geladen, getypechecked en geregistreerd.
  `use a::b::c` zoekt eerst het langste voorvoegsel: `a/b/c.typl` → `a/b.typl` → `a.typl` (aangezien `c`
  een onderdeel binnen een module kan zijn). Definities die vanuit andere modules zichtbaar zijn hebben
  `pub` nodig ([3.13 pub](#313-pub--zichtbaarheid)).
- **Circulaire verwijzingen zijn fouten**: de keten wordt gemeld in de vorm
  `circular module dependency: a -> b -> a`.
- **Uitvoeren**: `typl <file.typl>` voert een bestand uit (zonder argumenten de REPL). `use` in de REPL
  lost bestanden met dezelfde regels op.
- **Capaciteit van de cons-arena**: `typl --heap-cells N` stelt de **begincapaciteit** van de arena met
  cons-cellen in (standaard 65536; de vorm `--heap-cells=N` werkt ook, zowel bij het uitvoeren van
  bestanden als in de REPL). De arena **groeit door er meer toe te voegen** wanneer ze te klein wordt. De
  groeigrens is 256 keer de begincapaciteit, en een allocatie daarboven geeft `heap exhausted`: de
  begincapaciteit betekent "alloceer zoveel in het begin", en de grens betekent "beschouw alles daarboven
  als een lek".

### 3.12 load — plat laden

```lisp
(load "path")   ; top level only; path is a string literal
```

- **Plat laden** in de stijl van CL: leest de vormen van het doelbestand **zoals ze zijn in de huidige
  namespace** (zonder ze, anders dan `use`, in een module te wikkelen). Alleen op het hoogste niveau
  (binnen een functiebody is het een typefout).
- `path` is relatief aan de map van het ladende bestand (vanuit de REPL aan de cwd van het proces). Heeft
  het geen extensie, dan wordt `.typl` toegevoegd.
- `(load ...)`/`(use ...)` in het geladen bestand worden ook recursief verwerkt.
- **Het leest één vorm per keer en voert hem ter plekke uit** (zoals `load` van CL doet). Vorm *k* is klaar
  met uitvoeren voordat *k+1* wordt gelezen: zelfs als er halverwege een syntaxis- of typefout is, zijn de
  vormen ervoor al uitgevoerd. Modulebestanden die met `use` worden geladen zijn anders: ze worden als één
  eenheid gecontroleerd en het uitvoeren ervan wordt aan wie ze met `use` binnenhaalde overgelaten
  (overeenkomend met `compile-file` van CL).

### 3.13 pub — zichtbaarheid

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` kan alleen op de bovenstaande elf soorten worden gezet (niet op `module`/`use`/`deftrait`/`impl`).
Het wordt geschreven met het definitiekeyword direct na `pub`, niet in de vorm `(pub (defun ...))` waarbij
de definitie tussen haakjes wordt gewikkeld. Eén `pub` maakt precies één definitie openbaar (meerdere
definities kunnen niet in één keer worden gemarkeerd).

### 3.14 defmacro — macrodefinities

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Alle parameters en de returnwaarde zijn altijd `Sexpr`, dus er worden geen typeannotaties geschreven.
- Niet-hygiënische macro's in de stijl van CL (botsingen vermijden met `gensym` is de verantwoordelijkheid
  van de macroauteur).
- De lambdalijst heeft de volgorde van CL `required &optional &rest &key` (elke markering hoogstens één
  keer, en alleen in deze volgorde).
  - `&optional` … optionele argumenten. `name` of `(name default-expr)`. De standaardexpressie wordt bij
    expansie geëvalueerd (ze kan naar eerder gebonden parameters verwijzen) en gebonden wanneer het
    argument wordt weggelaten (zonder standaardwaarde de lege lijst `()`).
  - `&rest name` … ontvangt de overige positionele argumenten samen als één `Sexpr`-lijst.
  - `&key` … keyword-argumenten. `name` of `(name default-expr)`. De aanroeper geeft ze door als
    `:name value` (in willekeurige volgorde). Bij weglating de standaardexpressie (de lege lijst `()` als
    er geen is). Onbekende keywords of een `:key`-reeks van oneven lengte zijn fouten.
- Voorbeelden: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — lokale macrobindingen

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexically scoped macros
(symbol-macrolet ((name expansion) ...) body...)         ; a name stands for a form
```

Beide zijn **expressie**-speciale vormen, en tijdens runtime blijft er niets over (wat wordt gecompileerd
is de geëxpandeerde vorm van de body). De lambdalijst is dezelfde als bij `defmacro`. De gedetailleerde
regels en voorbeelden staan in
[Lokale macrobindingen](functions/system.md#9-lokale-macrobindingen-macrolet--symbol-macrolet).

## 4. Binding en voorwaarden

```lisp
(let ((name val) ...) body...)      ; parallel binding
(let* ((name val) ...) body...)     ; sequential binding (earlier bindings usable in later initializers)

(if cond then else)                 ; else is required (always three elements)
(when cond body...)                 ; an if without else (Unit type). defmacro
(unless cond body...)               ; the negation of when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; a list of keys: matches if any of them does
  (else body...))                   ; expr is evaluated once. keys are compared with equal.
                                     ; keys are "literals" and are not evaluated (as in CL).
                                     ; a bare symbol a means the symbol 'a.
                                     ; writing 'a is an error (use the bare a). defmacro
(ecase expr (key body...) ...)      ; a case requiring a match. panics if nothing matches. defmacro
(ccase expr (key body...) ...)      ; CL's ccase. there are no restarts to offer, so it is the same as ecase. defmacro
(and expr...)                       ; short-circuit evaluation. true with zero arguments. defmacro
(or expr...)                        ; short-circuit evaluation. false with zero arguments. defmacro
(progn body...)                     ; runs in order and returns the last value
(unsafe body...)                    ; the same as progn, plus permission to write FFI calls
                                     ; and raw words. see 3.3 defffi
(prog1 form more...)                ; evaluates everything; the value is that of form. defmacro
(prog2 a b more...)                 ; evaluates everything; the value is that of b. defmacro
(the Type expr)                     ; a type annotation (no run-time effect)
```

### 4.1 unsafe — aannames op je nemen die niet kunnen worden gecontroleerd

```lisp
(unsafe body...)
```

Hetzelfde als `progn`: evalueert de body in volgorde en geeft de laatste waarde terug. Het maakt geen
scope en is geen functiegrens (`break` / `return-from` gaan er rechtstreeks doorheen naar buiten). Het
verschil is dat sommige dingen alleen daarbinnen kunnen worden geschreven.

Drie dingen vereisen momenteel `unsafe`: C-functies aanroepen die met
[defffi](#33-defffi--c-functies-declareren-ffi) zijn gedeclareerd, ruwe machinewoorden
(`ptr` / `c-long` / `c-ulong` / `(ptr T)`) tot waarden maken, en
[`def-c-struct` en `c-alloc`](#def-c-struct-en-getypeerde-pointers--c-structs-alloceren).

Geheugen dat met `c-alloc` is gealloceerd wordt vrijgegeven bij het verlaten van de buitenste `unsafe`
binnen dezelfde functie. Alleen die `unsafe` heeft, anders dan `progn`, werk te doen bij het verlaten: het
vrijgeven.

Wat `unsafe` op zich neemt zijn de volgende aannames die de compiler niet kan verifiëren:

- **Dat de types overeenkomen.** Dat de gedeclareerde C-signatuur overeenkomt met de echte. Zo niet,
  dan gaan argumenten in de verkeerde registers en worden returnwaarden met de verkeerde breedte gelezen.
- **Geheugenveiligheid.** Wat de C-kant doet met wat hij krijgt.
- **Toestand van het hele proces.** Omgevingsvariabelen, signal handlers, `errno`. Bijvoorbeeld,
  `setenv` via de FFI aanroepen breekt de aannames die `decode-universal-time` van deze implementatie
  maakt wanneer hij de lokale tijd uitrekent.
- **Threadveiligheid.**

Het is geen uitweg uit typecontrole. `(unsafe (+ 1 "two"))` slaagt niet. Wat wordt toegestaan is het
schrijven van bepaalde **bewerkingen**, niet het schrijven van onzin.

Het werkt lexicaal. De body van een `lambda` die binnen `unsafe` is geschreven erft de toestemming
(zoals bij closures binnen `unsafe`-blokken van Rust). De waarde kan later van buiten de `unsafe` worden
aangeroepen, maar hem daar schrijven wordt zelf als het aanvaarden van de verantwoordelijkheid gezien.

### 4.2 destructuring-bind — lijsten naar vorm uit elkaar halen

```lisp
(destructuring-bind lambda-list form body...)
```

Haalt de lijst die `form` produceert **naar zijn vorm** uit elkaar en bindt hem. De lambdalijst is die van
`defmacro` (required → `&optional` → `&rest`/`&body` → `&key`, elk met standaardexpressies), om dezelfde
reden dat CL er één tussen de twee deelt: het zijn twee vormen die hetzelfde uit elkaar halen.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Elke gebonden variabele is een `Option<Sexpr>`.** Dit is geen beperking van de implementatie maar de
  aard van wat wordt gebonden: S-expressielijsten zijn de enige lijsten in deze taal, dus er is geen ander
  type om aan de elementen te geven. Terugvallen op `match` waar een scalar nodig is, is hetzelfde als in
  een `defmacro`-body.
- **Een vorm die niet overeenkomt geeft een panic** (overeenkomend met de fout van CL): te weinig of te
  veel elementen, een `&key`-reeks van oneven lengte, of een onbekend keyword. `sexpr-car` is een soepele
  functie die `()` teruggeeft voor `()`, dus zonder de controle zou een korte lijst stilzwijgend aan een
  lege reeks worden gebonden.
- **Geneste lambdalijsten worden niet ondersteund.** `defmacro` neemt ze ook niet, dus er is één regel.
  `(a (b c))` bindt niet stilzwijgend een sublijst aan `b`; het is een fout die dat zegt.
- De standaardexpressies van `&optional` / `&key` worden **alleen geëvalueerd wanneer ze worden gebruikt**
  (zoals in CL).
- Er is niets dat overeenkomt met `&allow-other-keys` van CL (`defmacro` heeft het ook niet).

### 4.3 match — patroonherkenning

```lisp
(match expr
  (pattern body...)
  ...)
```

Soorten patronen:
- `_` — jokerteken
- Een variabelenaam — een bindingspatroon (komt altijd overeen). Als het type van de scrutinee echter een
  variant met die naam heeft, wordt het opgelost als **het onderstaande patroon met kale variantnaam**
- Een kale variantnaam — komt overeen met een variant die geen argumenten neemt
  (`(match c (red 1) (blue 2))`). Een variant met velden op kale naam schrijven is een arityfout, dus
  schrijf hem tussen haakjes, zoals in `(circle r)`
- **Directe literals**: gehele getallen / `true`/`false` / tekens — als woorden vergeleken
- **Waardeliterals**: strings / drijvendekommagetallen / symbolen (`'foo`) / bignum-gehele getallen /
  ratio's — op waarde vergeleken met de `Eq::equals` van dat type
  ([Standaardtraits](functions/traits.md#2-eq--ord-vergelijking)). Strings worden op inhoud vergeleken,
  niet op identiteit
- `(= expr)` — evalueert een willekeurige expressie en vergelijkt met `Eq::equals`. De enige manier om
  types te vergelijken die geen literalsyntaxis hebben (`defstruct`-instanties, globale variabelen,
  berekende resultaten), en een door de gebruiker gedefinieerde `Eq`-implementatie wordt zoals ze is de
  vergelijkingsregel. `expr` kan naar alles verwijzen wat zichtbaar is vanaf de positie van de tak
  (argumenten, buitenste bindingen, globale variabelen)
- `(Ctor sub-pattern...)` — constructorpatronen (`Some x` `None` `Cons a d` `Ok v` enzovoort)

Een type dat `Eq` niet implementeert vergelijken met een waardeliteral / `(= expr)` is een typefout (deze
taal kiest ervoor te zeggen "deze kunnen niet worden vergeleken" in plaats van een tak achter te laten die
stilzwijgend nooit overeenkomt).

**Waardeliterals tegen een `Sexpr`-scrutinee**: de `Eq` van `sexpr` is `eq` (identiteit van CL), dus
directe waarden (`'foo` (geïnterneerd) / gehele getallen / tekens / `true`/`false`) kunnen zoals ze zijn
worden geschreven en komen op inhoud overeen:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Niet-directe literals (strings / drijvendekommagetallen / bignum-gehele getallen / ratio's) **kunnen niet
worden geschreven** tegen een `Sexpr`. Hun `eq` vergelijkt objectidentiteit, wat een "tak die typecheckt
maar nooit overeenkomt" zou opleveren, dus het is een fout die het variantpatroon noemt: schrijf
`(str "hi")` en het wordt in een `string` uit elkaar gehaald en op inhoud vergeleken. `(= expr)` vraagt
expliciet om `equals`, dus deze beperking is er niet op van toepassing.

**De scrutinee hoeft geen ADT te zijn.** `string`/`symbol`/`i32`/`f64` en dergelijke kunnen direct worden
gematcht (daar horen de stringliteralpatronen). Een type zonder varianten kan echter niet door opsomming
worden gedekt, dus `_` (of een bindingspatroon dat als jokerteken fungeert) is vereist:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; a type without variants needs `_`
```

Tegen een `Sexpr`-scrutinee kunnen, naast de bovenstaande 16 ingebouwde variantpatronen,
**downcast-patronen** (het uitnemen van instanties van door de gebruiker gedefinieerde ADT's) worden
geschreven: syntaxis om met `match` een instantie van een `defstruct`/`defenum` (hoofdstuk 3) terug te
krijgen die impliciet naar `Sexpr` is geconverteerd, zoals in `(list p 42)`:

- `(TypeName sub-pattern...)` — veldontleding met de **typenaam** eerst (alleen structs: een `defstruct`
  heeft altijd één variant, dus het wordt met de typenaam geschreven in plaats van een variantnaam).
  Bijvoorbeeld, voor `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Een kale variantnaam `(VariantName sub-pattern...)` — haalt een variant van een `defenum` eruit. Wordt
  opgelost als een kale naam die na `(use EnumType)` zichtbaar is (dezelfde zichtbaarheidsregels als bij het
  aanroepen van de constructor). Bijvoorbeeld, voor `(defenum color (red) (blue))`, `(red)` `(blue)` na
  `(use color)`. Als variantnamen van meerdere zichtbare enums botsen, is het een dubbelzinnigheidsfout,
  dus de gekwalificeerde vorm `(EnumType::VariantName ...)` kan ook worden geschreven (geen `use` nodig).
- `(the Type pattern)` — een downcast van het hele type (het als geheel bindend). Het ontleedt geen velden;
  het geeft de waarde zoals ze is aan `pattern`. De enige manier om een veranderlijke struct eruit te halen
  met behoud van zijn identiteit, en ook de enige manier om een `Vector<T>`/`HashTable<K,V>` uit een
  `Sexpr` te halen (ze hebben geen veldontledingsvorm). Bijvoorbeeld, na `(the point p)` wordt
  `(setf p::x 9)` ook in de oorspronkelijke instantie in de lijst weerspiegeld.

**Patronen voor `Option<Sexpr>`**: het type van S-expressiedata is niet `Sexpr` maar `Option<Sexpr>`, en
de lege lijst is geen variant van `Sexpr` maar de `none` van `Option`. Wanneer je dus een `Option<Sexpr>`
matcht, kunnen de 16 varianten van `Sexpr` en `none` **plat in dezelfde lijst takken** worden geschreven
(er is geen buitenste `match` nodig om de `Option` af te pellen):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; the empty list
    (_          9)))
```

Volledigheid wordt in hetzelfde platte universum gecontroleerd: de 16 varianten van `Sexpr` plus `none`,
17 in totaal. `(none)` vergeten is een fout tenzij er een `_` is. `(some x)` kan ook worden geschreven en
bindt "iets niet-leegs".

Deze suiker is **precies** alleen van toepassing op `Option<Sexpr>`. Bij `Option<Option<Sexpr>>` zou
onduidelijk zijn welke laag `(int n)` afpelde, dus schrijf twee niveaus `match` zoals gebruikelijk.

Dezelfde downcast-patronen kunnen zoals ze zijn worden gebruikt op **een scrutinee van een trait-object
(`:dyn Trait`, hoofdstuk 2)**: `match` pakt het uit en geeft het dan aan de bovenstaande
`Sexpr`-patroonmachinerie, dus er is geen extra syntaxis. De verzameling implementerende types is open,
dus het kan nooit volledig zijn, en `_` is vereist:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; field decomposition with the type name first
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Typeafleiding over takken heen**: alle takken moeten hetzelfde type hebben (behalve takken die
divergeren, zoals met `panic`). In een `match` die is geschreven waar geen type wordt verwacht, vullen de
takken elkaars ontbrekende typeargumenten in: `(result::ok v)` legt alleen `T` vast, en
`(result::err e)` alleen `E`, maar samen leggen ze `Result<T,E>` vast. Een typeargument dat aan het eind
door geen enkele tak kan worden vastgelegd is een fout van die tak (`cannot infer type argument ...`).
Buiten `match` is een typeargument dat niet kan worden vastgelegd meteen een fout.

De volledigheidscontrole van een `match` met downcast-patronen telt ze niet mee voor de dekking van de
eigen varianten van `Sexpr` (een `match` die alleen downcast-patronen opsomt moet met `_` worden
afgesloten). Bij generieke ADT's (`defstruct point<T> ...` enzovoort) kunnen de typeargumenten van een
downcast-patroon niet worden afgeleid, dus de veldontledingsvorm (`(point ...)`) en de kale variantvorm
kunnen niet worden gebruikt; geef ze op met `the`, zoals in `(the point<i32> p)`.

**Downcasts kijken ook naar de instantiatie.** Expliciete typeargumenten worden voor het matchen
gebruikt: `(the point<i32> p)` laat alleen waarden van `point<i32>` door, en een `point<string>` gaat
voorbij naar de volgende tak. Dit komt doordat een waarde zijn type inclusief zijn typeargumenten onthoudt
(hetzelfde mechanisme dat `print-object` kiest).

```lisp
(if-let (pattern val) then els)     ; then (with bindings) if val matches pattern, else els. defmacro
(while-let (pattern val) body...)   ; loops while val (re-evaluated each time) matches pattern. defmacro
```

## 5. Iteratie

```lisp
(loop body...)                      ; an infinite loop. leave with break/return
(while test body...)                ; loops while test is true. defmacro
(until test body...)                ; loops while test is false (the negation of while). defmacro
(dotimes (var count-expr) body...)  ; evaluates count-expr once and runs var over 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL-style iteration with parallel stepping. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; the sequential version of do (let* binding, assigned in order). defmacro
(doiter (var coll-expr) body...)    ; iterates over a value implementing the Iter trait. defmacro

(break)                             ; leaves only the innermost loop. the value is always Unit
(return)                            ; leaves only the innermost loop
(return value)                      ; leaves the innermost loop with a value
```

Zowel `break` als `return` verlaten **alleen de binnenste omsluitende lus** (ze zijn geen vroege terugkeer
uit de functie, en ze kunnen een `lambda`-grens niet overschrijden). Het type van een `loop` is de join van
de waardetypes van de `break`/`return`s die erin staan (`!` als hij nooit wordt verlaten). Gebruik
hieronder `return-from` om een functie te verlaten.

### 5.1 `block` / `return-from` — benoemde uitgangen

```lisp
(block name body...)                ; a named exit target. the value is the last form,
                                    ; or the value passed by return-from
(return-from name)                  ; leaves that block with Unit
(return-from name value)            ; leaves with a value
```

**Elke functie van `defun` / `defmethod` / `labels` stelt impliciet een block in met haar eigen naam**
(zoals in CL). Dus `(return-from f v)` is een vroege terugkeer uit de functie:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` is een **lexicale** uitgang, en de naam wordt **opgelost waar hij is geschreven**: de checker
koppelt een `return-from` aan het omsluitende `block` en voegt het type van zijn waarde samen met het
uitgangstype van het block. Dus:

- Een `return-from` zonder bijpassend `block` is een **typefout** (geen runtimefout).
- Een waarde waarvan het type niet past bij de andere uitgangen of het type van de body is een
  **typefout** (dezelfde regel als voor `match`-takken).
- Als blocks met dezelfde naam genest zijn, **wint de binnenste** (de verbergingsregel van CL).
- **Het kan functiegrenzen niet overschrijden.** Vanuit een `lambda` kun je niet naar een buitenste
  `block` vertrekken (`lambda` stelt geen block in: de impliciete blocks van CL hebben een *naam* nodig, en
  anonieme functies hebben er geen). Wat moet overschrijden is `catch`/`throw` (hoofdstuk 8, dat
  **dynamisch** is).

Net als `break`/`return` (hoofdstuk 5) is het een **statische** uitgang, dus in gecompileerde code is het
een sprong naar een basisblok dat bij het compileren vastligt. Als er een `unwind-protect` tussen zit,
wordt zijn `cleanup` uitgevoerd (hoofdstuk 8).

Als je nooit `return-from` schrijft, kost het impliciete block niets.

### 5.2 Uitgebreide `loop` (LOOP van CL)

**Als het eerste element van `loop` een keyword is**, wordt het als een reeks clausules gelezen. Anders
blijft het de eenvoudige lus hierboven, en verandert de betekenis van bestaande `loop`s niet (dezelfde
regel als de eenvoudige lus van CL zelf).

CL schrijft de clausulewoorden als kale symbolen (`(loop for i from 1 to 3 collect i)`), maar hier zijn
**ze allemaal keywords**: een kale `for` zou gewoon een variabeleverwijzing zijn, en een keyword zijn is
ook wat het van een eenvoudige lus onderscheidt. De uitzondering is `=`, dat een variabele van een waarde
scheidt: zijn positie is ondubbelzinnig, dus het wordt kaal of als keyword (`:=`) gelezen.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<int> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<int> 1 2 4 8>
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Variabeleclausules** (geschreven vóór de bodyclausules. Dit is de regel van CL: erna geschreven zouden ze
kunnen worden gelezen als "itereer alleen vanaf daar", dus het is een fout):

| Clausule | Betekenis |
|---|---|
| `:with v = e` | Bindt één keer. Mag de variabelen van eerdere clausules lezen |
| `:for v :in s` / `:for v :across s` | De elementen van een `Iter` in volgorde. Het onderscheid tussen lijst/vector van CL bestaat hier niet, dus dit zijn twee schrijfwijzen van dezelfde clausule |
| `:for v :on s` | De opeenvolgende **suffixen**. CL geeft de gedeelde staart-cons door, maar een `Iter` heeft geen staart om te delen, dus elk is een nieuwe `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Tellen. `:downfrom`/`:upfrom` werken ook |
| `:for v = e [:then f]` | Begint met `e`, en gebruikt vanaf de tweede keer `f` (zonder `:then` elke keer `e`) |
| `:repeat n` | Itereert zoveel keer |

Met meerdere `:for`s gaan ze **parallel** vooruit, en de lus eindigt zodra er één is uitgeput.

**Bodyclausules** (elke keer uitgevoerd, in de geschreven volgorde):

| Clausule | Betekenis |
|---|---|
| `:do form...` | Voor neveneffecten |
| `:collect e [:into v]` | Verzamelt in een `Vector<T>` |
| `:append e [:into v]` | Voegt de inhoud van een `Iter` toe |
| `:sum e` / `:count e` | De som / het aantal keren dat het waar was |
| `:maximize e` / `:minimize e` | Het maximum / minimum. **`Option<T>`** (net zoals CL nil teruggeeft voor een lege reeks; een willekeurig `Ord`-type heeft geen kleinste element) |
| `:always e` / `:never e` | `true` als ze allemaal gelden; `false` zodra er één faalt |
| `:thereis e` | `e` is een **`Option<T>`**. Geeft de eerste `some` terug, of `none` als die er niet is (dit komt overeen met de "eerste niet-nil waarde" van CL; gebruik `:always`/`:never` om een `bool` te testen) |
| `:while e` / `:until e` | **Eindigt hier normaal** (`:finally` wordt uitgevoerd, en wat is verzameld is het antwoord) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Maakt één clausule voorwaardelijk |
| `:return e` | Vertrekt onmiddellijk met die waarde (`:finally` wordt niet uitgevoerd, zoals in CL) |
| `:initially form...` / `:finally form...` | Vóór de lus / bij normale voltooiing |

**`:named name`** (vóór elke andere clausule, slechts één keer) wikkelt de hele lus in
`(block name …)`. `(return-from name e)` kan meteen vertrekken, zelfs vanuit geneste lussen, en net als
`:return` wordt `:finally` niet uitgevoerd. Zonder naam wordt geen block ingesteld: de naamloze `loop` van
CL stelt `block nil` in, maar er is hier geen `nil`, en `break`/`return` (hoofdstuk 5) bieden al "verlaat
de binnenste lus".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)` weglaten is een **typefout**. Het zijn gewoon de regels van `block` aan het werk
(5.1): het uitgangstype `int` past niet bij de `()` die de lus achterlaat wanneer hij is uitgeput.

**De waarde van de lus** is de accumulatie van de accumulerende clausule als die er is (de eerste, als er
meerdere zijn), `true` voor `:always`/`:never`, `none` voor `:thereis`, en `()` als er geen is. Als het
laatste in `:finally` `(return e)` is, is dat de waarde: het idioom `finally (return …)` van CL, de enige
manier waarop een lus die niet accumuleert zijn eigen antwoord kan noemen.

**Verschillen met CL / wat niet is opgenomen**:

- **Clausulewoorden zijn keywords** (hierboven).
- `:maximize`/`:minimize`/`:thereis` geven `Option<T>` terug (er is geen nil).
- **Alleen `:return` schrijven, zonder accumulatie of `:finally`, is een fout.** CL geeft nil terug
  wanneer uitgeput, maar zoiets is er hier niet, dus de lus moet zeggen wat zijn waarde is wanneer hij is
  uitgeput.
- Parallelle clausules samenvoegen met `:and`, `:being`/speciale iteratie over hashtabellen, `:it` en
  `:nconc` zijn niet opgenomen.
- Het elementtype van `:collect` komt uit het type van de geaccumuleerde expressie. Proberen een type te
  verzamelen dat **niet als typenaam kan worden geschreven**, zoals een functietype, is een fout die dat
  zegt.

## 6. Functiewaarden en aanroepen

```lisp
(lambda (params) RetType body...)   ; makes a first-class function value (a closure)
(labels ((name (params) RetType body...) ...) body...)   ; local function definitions that can be mutually recursive
(apply f arg1 ... argN rest-list)   ; calls f (a variadic function with &rest), spreading rest-list
```

Benoemde functies kunnen ook zoals ze zijn als waarden worden doorgegeven (als argumenten van functies van
hogere orde enzovoort).

## 7. Overige speciale vormen

```lisp
(setq var value ...)                ; CL's variable assignment. just a sequence of (setf var value). defmacro
(psetq var value ...)               ; parallel assignment. evaluates all values first, then assigns. defmacro
(psetf place value ...)             ; psetq generalized to places (the same expansion). defmacro
(setf place value)                  ; assignment to a place. a place is a variable name / var::field /
                                     ; a call of the form (accessor recv key...). valid if the
                                     ; static type of recv has an instance method named
                                     ; set-{accessor} (for the get of Vector<T> and HashTable<K,V>,
                                     ; set corresponds as an exception; otherwise set-accessor-name).
                                     ; the value is the value assigned (as in CL). so
                                     ; in (if c (setf x 1) ()), then and else do not have matching types
(incf place)  (incf place delta)    ; place += delta (delta=1 if omitted). the result is as with setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 if omitted)
(rotatef place1 place2 ... placeN)  ; rotates N places (new place1=old place2, ...,
                                     ; new placeN=old place1). each place's subforms evaluated once
(shiftf place1 ... placeN newvalue) ; shifts the values of place2..N left and puts newvalue in placeN.
                                     ; the return value is the old value of place1
(list e1 e2 ... en)                 ; expands to (cons e1 (cons e2 (... ()))). () with zero arguments.
                                     ; each element is converted to Sexpr implicitly (like CL's cons, it
                                     ; can hold any value). scalars (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) are wrapped in the matching Sexpr variant, and defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> and the like go in as they are
                                     ; (at no conversion cost). the same for &rest/format arguments.
(source-file)                       ; the name of the file this form was read from (string). fixed as a
                                     ; constant at check time. corresponds to CL's *load-pathname*, but is
                                     ; not a variable: module bodies run after checking, so "currently
                                     ; loading" cannot be relied on, while at check time it is always known.
                                     ; for sources that are not files, the reader's name for them (<stdin>/<input>)
(quote datum)                       ; the same as 'datum. returns it as Sexpr data without evaluating
(quasiquote template)               ; the same as `template. embeds expressions in the template with ,/,@
(documentation name)                ; returns the docstring of name (a bare name or Type::method) as Option<string>
(panic message)                     ; message: string. ends abnormally with an unrecoverable error. type !
(unreachable)                       ; expands to (panic "unreachable"). defmacro
(todo)                              ; expands to (panic "todo"). defmacro
(as Type expr)                      ; numeric/character type conversion. conversions that can fail panic on failure
(try-as Type expr)                  ; like as, but returns the result as Option<Type> (None on failure)
(print control args...)             ; expands the format and writes to standard output (no newline)
(println control args...)           ; the same (with a newline at the end)
(format dest control args...)       ; CL's format. returns the expanded string
(pprint x)                          ; pretty-prints. writes a newline first, as in CL
(pprint-fill x)                     ; fill layout
(pprint-linear x)                   ; all on one line or one element per line
(pprint-tabular x [colinc])         ; tabular layout (16 columns by default)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; build a logical block yourself
```

De familie `print`/`println`/`format`/`pprint` zijn speciale vormen, dus hun variadische argumenten (één
enkel object voor de `pprint`-familie) worden met hun eigen types in `Sexpr` gewikkeld voordat ze worden
doorgegeven: daarom werkt `(println "~a" my-struct)` gewoon. De details van formatdirectieven en de pretty
printer staan in [Formatdirectieven](functions/format.md) en
[Afdrukken](functions/printing.md#4-de-pretty-printer).

`as`/`try-as` behandelen alleen de numerieke en tekencatalogus (tussen `int`, de gehele types met vaste
breedte, `f32`/`f64`/`ratio`/`char`). Hetzelfde type is geen conversie. **Conversies tussen
geheeltalbreedtes (inclusief `int`) en tussen `f32`↔`f64` zijn echte conversies**: `as` kapt af / rondt af,
en `try-as` antwoordt of het in die breedte (precisie) past. `(as int x)` is de exacte verbreding vanuit een
vaste breedte, en `(as i32 n)` het afkappen vanuit `int`. Geheel getal → `char` kan buiten het bereik
mislukken, dus `as` geeft een panic en `try-as` geeft `None`. Al het andere (verbreden, en het afkappen van
`float->int`/`ratio->int`) slaagt altijd. `float->int`/`ratio->int`/`char->int` landen op `int`, en als een
smallere breedte wordt gevraagd, wordt daarna `int->W` aangeroepen. Dit is suiker die naar de
overeenkomstige conversiemethoden expandeert (`int->char`/`int->int`/`int->W` enzovoort in
[Getallen](functions/numbers.md)).

`documentation` is, net als `quote`/`compile`, een speciale vorm die `name` zonder evaluatie leest, als een
ongeëvalueerd kaal symbool / `::`-pad. Anders dan `(documentation 'name 'function)` van CL neemt het geen
typeargument: het lost `name` op in de volgorde variabele → functie → type → trait → macro (dezelfde
prioriteit als wanneer een kale identifier als expressie wordt geëvalueerd) en geeft de docstring van de
gevonden definitie terug (`(documentation Type::method)` is voor methoden). Niet kunnen oplossen (geen
definitie met die naam) is een fout bij het controleren; een definitie die bestaat maar geen docstring
heeft geeft `Option::none`. Alles wordt bij het controleren als constante bepaald: er vindt geen
opzoeking tijdens runtime plaats. Vrije namen met modulekwalificatie (`mod::name`, behalve
`Type::method`) worden niet ondersteund.

## 8. Niet-lokale uitgangen (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; runs body. if (throw 'tag v) happens anywhere
                                    ; body reaches, that v becomes the value
(throw 'tag value)                  ; exits to the nearest dynamically enclosing (catch 'tag ...)
(unwind-protect protected cleanup)  ; runs cleanup however protected is left
```

Anders dan `break`/`return` (hoofdstuk 5) is dit een **dynamische** uitgang: `throw` zoekt niet lexicaal
naar de `catch` eromheen, en bereikt een `catch` met dezelfde tag over een willekeurig aantal
functieaanroepen heen.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; if not found, the value at the end as usual
```

- **Tags zijn alleen letterlijke symbolen** (`'done`). Anders dan in CL worden ze niet geëvalueerd.
- **Een tag draagt een type.** Het type wordt bepaald bij het eerste gebruik van `'tag`, en elke latere
  `throw`/`catch` van hetzelfde symbool wordt ertegen gecontroleerd. Het met een ander type gebruiken is
  een typefout.
- Het type van `throw` is `!` (het divergeert). Het type van `(catch 'tag expr)` is de join van het type
  van `expr` en het type van de tag.
- De waarde van `unwind-protect` is de waarde van `protected`. De waarde van `cleanup` wordt weggegooid.
  `cleanup` wordt uitgevoerd hoe `protected` ook wordt verlaten: naast normale voltooiing, `throw` en
  `panic` wordt het ook uitgevoerd wanneer het via `break`/`return`/`return-from` wordt verlaten. Een
  niet-lokale uitgang door `cleanup` zelf wint van de uitgang die gaande is.
- Geneste `unwind-protect`s worden van binnen naar buiten uitgevoerd. Een `break` die een lus **binnen**
  `protected` verlaat heeft `protected` niet verlaten, dus zijn `cleanup` wordt niet uitgevoerd.

De condities van CL (`define-condition`/`handler-bind`/`invoke-restart`) zijn niet overgenomen. Ze passen
niet bij statische typering, dus herstelbare fouten worden met `Result` uitgedrukt (hoofdstuk 9).

## 9. Foutafhandelingsbeleid

- Herstelbare fouten: `Result<T,E>` + `match`. Niet-herstelbare fouten (bugs, gebroken invarianten):
  `panic`.
- Er is geen syntaxis die met `?`/try overeenkomt. Vertakkingen worden expliciet met `match` geschreven.
- Functie- en speciale-vormnamen gebruiken `!` (destructieve bewerkingen) of `?` (predicaten) niet als
  achtervoegsel. Predicaten krijgen een naam met achtervoegsel `-p`/`p` (`zerop`, `consp` enzovoort) of
  voorvoegsel `is-` (`is-some`, `is-ok` enzovoort).

## 10. Compilatie

```lisp
(compile name)                      ; JIT-compiles an already defined defun/method into native code
(compile-file src-path out-path)    ; AOT-compiles a source file into a native executable (skips the final `(main)`)
(dump path)                         ; writes the current environment (type information + compiled bodies) to one file
(disassemble name)                  ; prints what that definition becomes (host machine code by default, LLVM IR with true as the second argument)
```

`compile` is een speciale vorm; `name` wordt niet geëvalueerd en wordt gelezen als een ongeëvalueerd kaal
symbool / `::`-pad (een string is een typefout). Generieke functies kunnen niet het doel zijn: op elke plek
van gebruik wordt een kopie per type gemaakt, dus er bestaat geen enkele gecompileerde body. **Een naam
die niet kan worden opgelost is een fout bij het controleren** en wordt nooit naar runtime doorgeschoven
(er zijn aparte meldingen voor: het type bestaat maar die methode niet / noch het type noch de functie
bestaat / een kale ongedefinieerde naam). Zichtbaarheid wordt hier behandeld als bij elke andere
verwijzing: "bestaat maar is vanaf hier niet zichtbaar" mislukt bij het controleren, net als "wordt niet
opgelost".

Aangeroepenen worden ook transitief gecompileerd, dus **een functie die (zelfs indirect) iets aanroept dat
niet kan worden gecompileerd kan niet worden gecompileerd**. Het proces crasht niet; het wordt geweigerd
met een fout die dat zegt. Elke ingebouwde functie kan worden gecompileerd, dus de enige functies die zo
worden geweigerd zijn die welke de volgende alleen-interpreterbewerkingen aanroepen:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

De alleen-interpreterbewerkingen zijn `compile`/`compile-file`/`dump` en
`trace`/`untrace`/`step`/`disassemble`
([Implementatiehulpmiddelen](functions/system.md#5-implementatiehulpmiddelen-clhs-252)). In plaats van
dingen die niet kunnen worden gecompileerd, zijn dit bewerkingen van de compilerende kant (wat `dump`
wegschrijft is de omgeving van de interpreter zelf, die een AOT-uitvoerbaar bestand niet heeft; wat `trace`
bewaakt en waar `step` stopt zijn de aanroeppaden van de draaiende interpreter; en `disassemble` gebruikt
de compiler zelf). `room`/`dribble`/`ed` horen er niet bij en kunnen normaal worden gecompileerd.

Wat **wel** kan worden gecompileerd: stream- en bestands-I/O, `random`, `gensym`,
`symbol->string`/`string->symbol`, `parse-int`/`parse-float`,
`get-universal-time`/`get-internal-real-time`, `exit`, de transcendente functies, bitbewerkingen,
`catch`/`throw`/`unwind-protect`, alle vier `eq`/`eql`/`equal`/`equalp` (waardoor `case` voor elk type
kan compileren), de hele afdrukfamilie inclusief `print`/`println`/`format`/`pprint` en
`pprint-logical-block`, `read` en `eval`. De standaardbibliotheek wordt al gecompileerd geleverd.

Een AOT-uitvoerbaar bestand bevat alleen de functies die het programma gebruikt. Een programma dat niet
afdrukt krijgt geen formatengine, een dat `read` niet aanroept krijgt geen reader, en een dat `eval` niet
aanroept krijgt geen checker en geen interpreter.

Vanaf de opdrachtregel doet `typl -c src-path [-o out-path]` (`-c` kan ook als `--compile` worden
geschreven) hetzelfde als `compile-file`. Zonder `-o` is de uitvoer `src-path` zonder de extensie `.typl`.
Standaard is de statische bibliotheek `libtypelisp_front.a` die in uitvoerbare bestanden wordt gelinkt,
voor een release-build van `typl`, die welke `typl` in zichzelf draagt, bij de eerste link weggeschreven
naar `$TYPELISP_HOME/lib/<build ID>/` (of `~/.typelisp/lib/<build ID>/` zonder `TYPELISP_HOME`) en
vandaar gebruikt; voor een debug-build die waar `typl` is gebouwd. `typl --remove-lib` verwijdert wat die
`typl` heeft weggeschreven. Met `--others` verwijdert het die van andere build-ID's; met `--all` die van
elke build-ID. Met `typl --lib-dir DIR` wordt die in `DIR` gebruikt (voor zowel `-c` als `compile-file`),
en als hij er niet is, is het een fout bij het opstarten.

### 10.1 Dumps

```lisp
(dump "session.typld")     ; write one out
```
```sh
typl --image session.typld prog.typl   # start from it
typl --image session.typld             # the REPL too
```

Een dump bevat typeinformatie en gecompileerde bodies in één bestand. Wat `(dump path)` schrijft is wat de
huidige sessie laadde (de standaardbibliotheek, of een dump die met `--image` is doorgegeven) plus **wat de
sessie zelf definieerde**. De uitvoer is dus zelfstandig, en `typl --image` brengt dezelfde omgeving op.
Wat de sessie met `(compile f)` heeft gecompileerd wordt in gecompileerde vorm geschreven.

Wat wordt opgeslagen zijn **definities, geen geschiedenis**:

- De expressies op het hoogste niveau van de sessie (`(println ...)` enzovoort) zijn niet inbegrepen. Het
  zou een probleem zijn als laden ze opnieuw uitvoerde.
- Globale variabelen komen terug met **de waarde van hun opnieuw uitgevoerde initializer**, niet de waarde
  op het moment van de dump. Dit is een bewust verschil met `save-lisp-and-die` van SBCL (dat de heap
  zoals hij is wegschrijft), en deze keuze laat een hele familie problemen verdwijnen: "waarden die niet
  kunnen worden opgeslagen", zoals geopende streams, functiepointers van closures en extern geheugen.
- Anders dan bij `save-lisp-and-die` **sterft het proces niet**, aangezien schrijven het image niet
  beschadigt.

Een dump legt de versies vast van de standaardbibliotheek en de compiler van de implementatie die hem
schreef. Hem laden met een `typl` van een andere versie is een fout; hij wordt nooit stilzwijgend
geaccepteerd.

### 10.2 `eval` in AOT-uitvoerbare bestanden

`eval` typecheckt tegen "de huidige globale omgeving" en evalueert daarna
([Parsen en evalueren](functions/system.md#6-parsen-en-evalueren)). Die omgeving (de tabellen met
signaturen, types en macro's die de checker raadpleegt, en de bodies die de interpreter kan uitvoeren) zit
**niet in de machinecode**. Een gecompileerde functie is niets anders dan een symbool op een adres; ze heeft
noch haar argumenttypes noch een tabel om bodies op naam op te zoeken.

Dus alleen voor programma's die `eval` aanroepen **bouwt `compile-file` die omgeving tijdens het
compileren en schrijft haar in het uitvoerbare bestand**. Het formaat is hetzelfde als een dump, met het
deel van de standaardbibliotheek en het eigen deel van het programma. Bij het opstarten gebeurt alleen het
herstellen ervan: de broncode wordt niet opnieuw gelezen, en er wordt niets opnieuw getypechecked. Aan
programma's die `eval` niet aanroepen wordt niets toegevoegd.

Gevolgen:

- **Het opstarten duurt langer en het uitvoerbare bestand is groter**, aangezien de code van de checker en
  de interpreter en een momentopname van de omgeving erin gaan. De heap wordt ook wat groter gemaakt.
- **Aan eval doorgegeven vormen worden geïnterpreteerd.** Ook wanneer de aan eval doorgegeven vorm de eigen
  functies van het programma aanroept, is wat draait de interpreteerbare body die de momentopname bevat. Het
  resultaat is hetzelfde; alleen de snelheid verschilt.

De opslag van globale variabelen wordt **gedeeld** met gecompileerde code (dezelfde slots). Een
`defvar`-initializer wordt één keer door de gecompileerde initialisatie uitgevoerd, en het herstel slaat
hem over, dus een initializer met neveneffecten wordt niet twee keer uitgevoerd.

`compile-file` leest ook de standaardbibliotheek (en bedt haar bodies in het uitvoerbare bestand in), dus
functies van de standaardbibliotheek zoals `abs`/`gcd`, en `(impl print-object ...)` evenals
`(defmethod print-object ...)`, kunnen met AOT worden gebruikt.

`compile-file` accepteert ook `use` (en `import`/`shadowing-import`). De `(use m)` van het invoerbestand
vindt bestanden met dezelfde regels als `typl file.typl`, en de gevonden afhankelijkheidsbestanden worden
ook gecompileerd en in het uitvoerbare bestand gelinkt: een indeling waarin `main.typl` `http.typl` via
`(use http)` leest kan zoals ze is met AOT worden gecompileerd. De eigen definities van het invoerbestand
gaan ook in de module die naar het bestand is genoemd, net als bij `typl file.typl` (`point` in `p.typl`
is `p::point`). De afgedrukte weergave van waarden (`#<p::point x: 1 y: 2>`) is dus hetzelfde, hoe ook
uitgevoerd.

## 11. Readermacro's (readtable)

Wat de reader **doet wanneer hij een bepaald teken tegenkomt** kan vanuit het programma worden vervangen
(CLHS 23.1).

```lisp
(set-macro-character c f)             ; f reads the character c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f reads the two-character sequence d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Het type van `f` is `(fn (string-input-stream char) Option<Sexpr>)`. Het eerste argument is **een stream
over de nog niet gelezen tekst**, en het tweede is **het teken dat het veroorzaakte** (het tweede teken bij
een dispatch). De returnwaarde wordt de data die op dat punt is gelezen. De stream is een concreet type in
plaats van `:dyn PeekInput` omdat de reader altijd deze ene soort doorgeeft:
`read-sexpr` / `read-char` / `peek-char` / `unread-char` / `read-delimited-list` nemen allemaal
`(where (PeekInput S))`, dus ze werken allemaal zoals ze zijn op het concrete type.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => read as (not (equal 1 2)), that is, true
```

De reader **kijkt vóór de ingebouwde syntaxis naar macrotekens**, dus hij kan ook `(` en `'` overnemen.
Op deze manier geregistreerde subtekens van `#` gaan voor de ingebouwde `#b`/`#x`/`#.`. Een ander teken
dan `#` wordt ter plekke een dispatchteken wanneer het aan `set-dispatch-macro-character` wordt
doorgegeven: er is **geen** tegenhanger van `make-dispatch-macro-character` van CL. Registratie doet haar
werk al, dus een aparte stap zou niets te doen hebben.

**Wanneer ze van kracht worden** hangt af van het leespad, hetzelfde als bij `#.` (hoofdstuk 1):

- De REPL en `(load ...)` voeren één vorm per keer uit, dus **functies die in eerdere vormen zijn
  gedefinieerd** kunnen zoals ze zijn worden geregistreerd.
- Modulebestanden worden als eenheid gecontroleerd en later uitgevoerd, dus **alleen de aanroepen van
  `set-macro-character` / `set-dispatch-macro-character` worden onmiddellijk uitgevoerd** (de rol van
  `(eval-when (:compile-toplevel) ...)` van CL). Omdat ze onmiddellijk worden uitgevoerd, **moet de
  doorgegeven functie op dat punt al bestaan**. Een `defun` in hetzelfde bestand is nog niet uitgevoerd,
  dus schrijf een `lambda`, of gebruik de standaardbibliotheek of iets wat al is uitgevoerd. Alleen
  aanroepen op het hoogste niveau worden gedekt; het kijkt niet binnen `progn` of `let`.

De ingebouwde `read` / `read-from-string` raadplegen ook de readtable (zoals in CL).

**Wat er niet is**: `*readtable*` en `copy-readtable`, en `readtable-case`. De eerste twee omdat een
readtable **geen waarde is**: een waarde zou "iets dat aan een reader kan worden gegeven" moeten zijn, maar
de reader die de broncode leest bevindt zich buiten het programma, zonder plek om hem aan te geven.
`readtable-case` omdat hoofdstuk 1 bepaalt dat de reader van deze taal altijd naar kleine letters omzet
(`:downcase` van CL).


## 12. Gelijktijdigheid (taken)

**Een taak is een lichtgewicht thread** (in termen van Go: wat een `go`-statement start) en draait
coöperatief (er is geen preëmptie). Wisselen gaat niet via de kernel, en de uitvoeringstoestand leeft op
de heap in plaats van op een machinestack, dus taken zijn goedkoop in grote aantallen te maken.

**Taken draaien tegelijkertijd op meerdere OS-threads** (parallellisme over meerdere cores). Het aantal
threads is de omgevingsvariabele `TYPELISP_THREADS` (het totaal, inclusief de thread die `main` draait;
de standaardwaarde is het parallellisme van de machine). In `typl` draaien **alleen gecompileerde taken**
op andere threads, en geïnterpreteerde taken draaien op de thread van de interpreter (12.7). Gedeelde data
gaat via `Mutex<T>` of `Chan<T>`; gelijktijdig lezen en schrijven dat daar niet via gaat is ongedefinieerd,
zoals in Go (12.7).

Van de woordenschat zijn **alleen `task` / `thread` / `select` speciale vormen**; de rest zijn gewone
functies, methoden en macro's ([Taken en kanalen](functions/concurrency.md)).

### 12.1 `task` — een taak starten

```lisp
(task (f arg...))                   ; returns Task<T>, where T is the return type of f
```

**Het accepteert alleen de vorm van een aanroep.** `f` en elke `arg` worden geëvalueerd waar de `task`
is geschreven, in de geschreven volgorde, en alleen **de aanroep** gebeurt in de nieuwe taak. Dit is
dezelfde regel als `go f(x)` van Go, en het is ook de reden dat het een aanroepvorm neemt in plaats van
een thunk: een thunk zou zijn argumenten vastleggen zonder ze te evalueren.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i is evaluated on the spot each time; no capture trap

(task ((lambda () ()                ; to run an arbitrary body, call a lambda
         (println "start")
         (send ch 1))))
```

Speciale vormen (`if` / `let` / `progn` …) kunnen niet rechtstreeks onder `task` worden geschreven.

**Waarom het geen functie kan zijn**: `(spawn (lambda () T body...))` schrijven zou vereisen dat `T` wordt
uitgeschreven, aangezien `lambda` een returntypeannotatie vereist, en een macro het returntype van
`(f a b)` niet kent. Alleen de checker kent het.

### 12.2 `thread` — een taak starten op een eigen OS-thread

```lisp
(thread (f arg...))                 ; returns Thread<T>, where T is the return type of f
(join th)                           ; waits for completion and returns its value (any number of times)
```

De vorm en de evaluatieregels zijn dezelfde als bij `task` (het accepteert alleen een aanroepvorm, en `f`
en `arg` worden geëvalueerd waar het is geschreven). Het verschil is waar het draait: **het start een
OS-thread die aan die taak is gewijd en draait alleen daarop**. Het wordt niet met andere taken
gemultiplexed, dus een blokkerende C-functie (`defffi`) daarbinnen aanroepen stopt alleen die thread, en
andere taken boeken vooruitgang. Daarbinnen kunnen `task`, `send`, `recv` en de rest zoals ze zijn worden
gebruikt.

- `Thread<T>` is de tegenhanger van `Task<T>`. Net als `wait` stopt `join` **de aanroepende taak**, en de
  waarde wordt in de cache bewaard. Wanneer de taak eindigt, eindigt ook de thread.
- De panicregels zijn dezelfde als bij `task` (het hele proces gaat neer). Wanneer `main` terugkeert,
  eindigt het proces.
- Om het als functie te schrijven, gebruik je `(Thread::spawn (lambda () T body...))`
  (`std::thread::spawn` van Rust). Er mag ook een benoemde functie worden doorgegeven.
- **Alleen gecompileerde code draait op een eigen thread.** Wanneer `typl` tijdens het interpreteren
  `(thread (f ...))` of `Thread::spawn` evalueert, compileert hij de functie die moet draaien (en wat ze
  aanroept) ter plekke voordat hij haar uitvoert. Wat niet kan worden gecompileerd (een `lambda` die naar
  lokale variabelen daarbuiten verwijst, het construeren van een struct enzovoort) is, voordat de thread
  wordt gestart, een panic die hetzelfde wordt behandeld als een `(panic ...)`. Een `lambda` die naar
  lokale variabelen verwijst kan worden doorgegeven als ze binnen een gecompileerde functie is gemaakt.

### 12.3 `select` — wachten op meerdere kanaalbewerkingen tegelijk

```lisp
(select
  ((v (recv ch1)) body...)          ; a receive arm. v is bound to an Option<T>
  ((send ch2 x) body...)            ; a send arm
  (else body...))                   ; optional. **if written, it goes last**
```

- **Met `else` blokkeert het niet** (`default` van Go). Zonder wacht het totdat er een mogelijk wordt.
- **Als er meerdere tegelijk mogelijk zijn, wordt er willekeurig een gekozen** (in geschreven volgorde
  zouden latere takken uitgehongerd raken).
- De `v` van een ontvangsttak is een **`Option<T>`**. Een gesloten kanaal is "een antwoord", geen reden
  om de tak over te slaan, dus doe `match` erop binnen de tak.
- Het type is **de join van de types van de bodies van alle takken** (dezelfde regel als bij
  `match`-takken).
- `(select)` met nul takken is een typefout (`select{}` van Go, dat eeuwig blokkeert, is niet
  overgenomen). Een `select` met alleen `else` ook, aangezien het hetzelfde is als de body rechtstreeks
  schrijven.

**De kanaalexpressies en de te verzenden waarden worden elk één keer geëvalueerd, van links naar rechts,
welke tak ook wordt gekozen** (dezelfde discipline die `case` voor zijn sleutels heeft).

```lisp
(select                             ; receiving with a timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([een kanaal dat na een tijd aflevert](functions/concurrency.md#5-after--een-kanaal-dat-na-een-tijd-aflevert))
is "een kanaal dat na `sec` seconden één waarde aflevert", overeenkomend met `time.After` van Go.

### 12.4 Samenspel met andere functies

| Functie | Hoe ze zich tot taken verhoudt |
|---|---|
| `catch` / `throw` | **Overschrijden geen taakgrenzen.** Een `throw` die de body van een taak probeert te verlaten is een panic |
| `unwind-protect` | De opruiming wordt uitgevoerd wanneer een taak natuurlijk eindigt. **Ze wordt niet uitgevoerd wanneer het proces eindigt doordat de hoofdtaak eindigde** |
| `block` / `return-from` | Lexicaal, dus ze overschrijden geen `lambda`-grenzen |
| `panic` | Net als in Go gaat het hele proces neer. `wait` neemt een panic niet als waarde waar |
| `dlet` | **Geen binding per taak.** Het "leent en geeft nog steeds een globale variabele terug", dus taken beïnvloeden elkaar |
| Standaarduitvoer | Gedeeld door alle taken. De uitvoer van één `println` wordt nooit midden in een regel met andere gemengd |
| `compile` / `eval` | Geen beperkingen. `(compile f)` binnen een taak werkt |

### 12.5 Waar taken wisselen

Het plannen is coöperatief, dus **taken wisselen alleen waar je het schrijft**: `(yield)`,
`(sleep ...)`, `(wait ...)`, **kanaalbewerkingen die moeten wachten** (`send`/`recv`/`select`), en
**socketbewerkingen die moeten wachten** (`accept` / `tcp-connect` (inclusief naamresolutie) / lezen en
schrijven van sockets / `recv-from`; [Netwerk](functions/network.md)). Alle sockets zijn niet-blokkerend:
als er een niet gereed is, stopt alleen die taak, en ze hervat wanneer het OS zegt dat hij gereed is, in
dezelfde vorm als de netpoller van Go. Alleen wanneer geen enkele taak kan draaien wacht de implementatie
op het OS tot de dichtstbijzijnde deadline van `sleep`.

Kanaalbewerkingen die ter plekke kunnen antwoorden (een `send` met ruimte in de buffer, een `recv` met een
wachtende waarde, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **gebruiken de beurt niet op**.
Dit betekent dat je niet onverwacht door een leesbewerking wordt onderbroken, en het wordt anders behandeld
dan `(sleep 0.0)`, het "yield voor 0 seconden" van CL.

**Er is geen preëmptie.** Een strakke lus die niets aanroept laat andere taken verhongeren. Gecompileerde
lussen geven echter periodiek de besturing aan de scheduler, dus een gecompileerde strakke lus laat ze niet
verhongeren.

### 12.6 Gecompileerde code en taken

Gecompileerde code kan ook taken opschorten. Hetzelfde geldt voor uitvoerbare bestanden die met
`compile-file` zijn gemaakt: `main` draait als de hoofdtaak van de scheduler, en `task`, `sleep`, `wait`,
kanalen en socketwachten werken allemaal met dezelfde betekenis als in `typl`. Wanneer `main` terugkeert,
eindigt het proces en worden de overige taken afgekapt (zoals in Go). De interpreter wordt nooit ter wille
van de scheduler in het uitvoerbare bestand gezet.

De ene uitzondering is "binnen een callback van de C-FFI", waar bewerkingen die **zouden moeten wachten**
fouten zijn (vriendelijker dan stilzwijgend vastlopen): terwijl een functie die met `defffi` is
doorgegeven vanuit C wordt aangeroepen, ligt de stack van C bovenop, en er is geen manier om de taak op te
schorten en later te hervatten.

De volgende plekken zijn ook functies die midden in een taak worden aangeroepen, maar niet kunnen
opschorten: `print-object`-methoden, `~/name/` in `format`, readermacro's, binnen `eval`, en
`defvar`-initializers in AOT-uitvoerbare bestanden. Hier **gaan bewerkingen die zonder wachten antwoorden
erdoor** (`(recv ch)` met een waarde in de buffer, `read-line` op een socket met al ontvangen gegevens,
`(task ...)`, `(yield)` enzovoort), en **bewerkingen die echt zouden moeten wachten zijn fouten** (het
proces wordt niet ter plekke gestopt, maar het is een panic als `` `recv` cannot block: ... ``, die
hetzelfde wordt behandeld als een `(panic ...)`).

### 12.7 Verschillen met Go

- **In `typl` gaan alleen gecompileerde taken naar andere threads.** De toestand van de interpreter kan
  niet tussen threads worden gedeeld, dus taken van een geïnterpreteerde `task` draaien op de thread van de
  interpreter. Ook een gecompileerde taak **gaat naar de thread van de interpreter en blijft daar** (ze
  gaat niet terug) op het punt waar ze een geïnterpreteerde functiewaarde aanroept, een `:dyn`-methode
  aanroept die niemand heeft gecompileerd, of `eval`/`macroexpand`/`read` aanroept. Als een lange
  berekening onderweg zelfs één keer geïnterpreteerde code raakt, draait de rest op de thread van de
  interpreter.
- **In `typl` leven de workers maar voor één evaluatie op het hoogste niveau.** Terwijl de REPL op invoer
  wacht, en tussen vormen op het hoogste niveau, laten andere threads taken niet vooruitgaan (overgebleven
  taken gaan bij de volgende evaluatie verder waar ze waren gebleven). Aan het eind van een evaluatie wacht
  het tot elke thread zijn huidige stap heeft voltooid, dus als een C-functie (`defffi`) binnen een
  `thread` blijft blokkeren, eindigt de evaluatie niet totdat ze terugkeert.
- **Afdrukken op workers**: geïnterpreteerde `print-object`- / `~/name/`-methoden kunnen niet op andere
  threads draaien, dus zulke waarden op een andere thread afdrukken is een panic die hetzelfde wordt
  behandeld als een `(panic ...)` (`(compile T::print-object)`, of druk af vanuit de hoofdtaak).
- **Data races zijn ongedefinieerd** (dezelfde positie als Go). Het resultaat van meerdere taken die
  dezelfde waarde wijzigen zonder via `Mutex<T>` / `Chan<T>` te gaan is niet gegarandeerd.
- **`task` geeft een waarde terug.** Anders dan het `go`-statement van Go geeft het een `Task<T>` terug, en
  `(wait t)` haalt het resultaat op.
- **Er zijn geen nil-kanalen.** Het fan-in-idioom van Go (een gesloten kanaal op `nil` zetten om het uit de
  takken van `select` te halen) kan niet worden geschreven, dus start één taak per invoer en voeg ze samen
  met een `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--wachten-op-n-voltooiingen)). Dat is ook de aanbevolen
  manier in Go, maar het is **het eerste verschil waar mensen die van Go komen tegenaan lopen**.
