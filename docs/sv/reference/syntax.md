<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# Syntaxreferens för typelisp

typelisp är ett statiskt typat Lisp, skrivet med S-uttryck. För listan över inbyggda funktioner och
metoder, se [Inbyggda funktioner](functions/README.md); för listan över typer, [types.md](types.md);
och för att läsa felmeddelanden, [errors.md](errors.md).

## 1. Lexikaliska element

- **Skiljer inte på stora och små bokstäver.** Symboler normaliseras alla till gemener vid läsning.
- **Kommentarer**: från `;` till radens slut (radkommentarer). `#| ... |#` (blockkommentarer, som kan
  nästlas).
- **Utvärdering vid läsning**: `#.(expr)` **kör den följande formen under läsningen** och behandlar dess
  värde som det som lästes. Det är det enda stället där läsaren är mer än en funktion av texten. Hur långt
  det når beror på läsvägen, som i CL:
  - `(load ...)` och REPL utvärderar en form i taget, så det kan anropa **funktioner som definierats
    tidigare i samma text** (CL:s `load`).
  - En modulfil kontrolleras som en enhet och körs av den som gör `use` på den, så `#.` når bara
    standardbiblioteket och det sessionen redan har kört. Varken filens egna definitioner eller de i
    modulerna den gör `use` på **har körts än** (precis som CL:s `compile-file` behöver `eval-when`).
  - `read` / `read-from-string` inuti ett program utvärderar också `#.` (som i CL).
  - Att sätta `*read-eval*` (standard `true`) till `false` gör `#.` till ett läsfel överallt: en
    brytare för att hindra text som läses som data från att köra kod (som i CL). Den konsulteras vid varje
    `#.`, så en `setf` får effekt från nästa form som läses. Inuti `with-standard-io-syntax` är den `true`.
- **Booleska värden**: `true` / `false`.
- **Heltal**: decimala (`42`, `-7`). Ett tecken `+`/`-` får komma först. Andra baser skrivs med CL:s
  basnotation `#b`/`#o`/`#x`/`#NNr` (tecknet kommer efter markören: `#x-ff`). Prefixet `0x` finns inte i
  CL och är inte antaget: `0xff` läses som en symbol.
  En heltalsliteral utan typannotering är som standard `int` (godtycklig precision,
  [Tal](functions/numbers.md#3-heltal-med-godtycklig-precision-int)), utan övre gräns för dess storlek.
  **Om den förväntade typen är en heltalstyp med fast bredd får literalen den typen, och det kontrolleras
  att typen kan hålla värdet**: `(the u8 300)` är ett typfel (om du vill att den ska kapas, skriv
  `(as u8 300)`). `(the u32 4294967295)` och `(the u32 #xFFFFFFFF)` kan skrivas tack vare den här regeln.
  Om ett `int`-värde ryms i ett omedelbart 63-bitarsvärde eller blir ett bignum avgörs av dess storlek,
  utan särskild syntax (som i CL).
- **Flyttal**: de som innehåller en decimalpunkt eller en exponent (`e`/`E`) (`1.5`, `3.0e10`). `f64` som
  standard (`f32` om det är den förväntade typen).
- **Kvoter (ratio)**: `täljare/nämnare` (bara decimala, till exempel `1/3`). Förkortas vid läsning, som CL
  specificerar (`2/4` är `1/2`). De med heltalsvärde (`4/2` och så vidare) läses som `int`, inte
  `ratio`. En nämnare noll (`1/0`) är ett läsfel.
- **Tecken**: `#\` följt av ett tecken eller ett teckennamn. Till exempel `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (även `#\Null`) `#\Backspace`. Namn skiljer inte på
  stora och små bokstäver.
- **Strängar**: `"..."`. Escape-sekvenserna är `\n` `\t` `\r` `\0` `\\` `\"` (varje annat `\x` är bara `x`).
- **Symboler**: vilken token som helst som innehåller bokstäver, siffror och symboler (`+` `<=` `my-func`
  och så vidare).
  `]` och `}` avslutar en token och kan därför inte stå i en symbol; står en sådan i början av ett
  datum är det ett läsfel. `[` och `{` kan stå i en symbol: precis som i CL lämnas de fria så att
  programmeraren kan använda dem i [läsmakron](#11-läsarmakron-readtable).
- **Keywords**: symboler som börjar med ett kolon, som `:name` (som i CL). De utvärderas till sig själva:
  de slår inte upp någon bindning och deras värde är de själva, med statisk typ `symbol`. Keywords med
  samma namn är alltid samma objekt (`(eq :foo :FOO)` är sant; liksom andra symboler görs de till
  gemener). Själva kolonet är en del av namnet, så `(symbol->string :foo)` är `":foo"` (typelisp har
  inget paketsystem, så detta skiljer sig från CL:s `symbol-name`). Ett ensamt `:` eller ett med ytterligare
  kolon som `:a:b` är ett läsfel. Testa med `keywordp`. De som börjar med `::` är inte keywords utan
  absoluta sökvägar (nedan).
  Notera att `:dyn` är ett reserverat keyword bara för typpositioner; att skriva det någon annanstans är
  ett fel (se [kapitel 2](#2-hur-typer-skrivs)).
- **Listor**: `(a b c)`. Punktpar `(a . b)` kan också läsas.
- **Vektorer**: `#(1 2 3)` (som i CL). Innehållet består bara av literaler och evalueras inte: `a` i
  `#(a b)` är en symbol, inte en variabel. Elementtypen kommer från sammanhanget
  (`(the Vector<i32> #(1 2))`), eller från det första elementet när sammanhang saknas (`#(1 2 3)` är
  en `Vector<int>`). Alla element måste ha samma typ: `#(1 "a")` är ett typfel, liksom `#()` utan
  element och utan sammanhang. Varje evaluering skapar en ny vektor. Där S-uttrycksdata förväntas
  (`(the Option<Sexpr> #(1 x))`, `'#(..)`, det `read` returnerar) är det en `Vector<Option<Sexpr>>`
  vars element alla är data: varianten `vector` av `Sexpr`.
- **Arrayer**: `#2A((1 2) (3 4))` (som i CL). Talet mellan `#` och `A` är rangen, och lika många
  första nivåer av listnästling i innehållet är dimensionerna. `#0A x` är en nolldimensionell array
  med ett element. Listor på samma nivå med olika längd är ett läsfel. Typen bestäms som för
  vektorer och är en `Array<T>` (utan element måste sammanhanget ange den, som i
  `(the Array<f64> #2A(()))`). Som S-uttrycksdata är det en `Array<Option<Sexpr>>`: varianten
  `array` av `Sexpr`.
- **Den tomma listan `()`**: beroende på sammanhang värdet av typen `Unit` eller `none` i
  `Option<Sexpr>`. **`Sexpr` har ingen variant för den tomma listan**: `Sexpr` betyder "ett icke-tomt
  S-uttryck", och typen för S-uttrycksdata är `Option<Sexpr>` (se "Mönster för `Option<Sexpr>`" i
  [4.3 match](#43-match--mönstermatchning)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (meningsfullt bara inuti en quasiquote)
  - `,@x` → `(unquote-splicing x)` (fogas in som listelement vid expansion)
- **Sökvägar `::`**: `foo::bar` läses som en sökväg genom moduler, typer och medlemmar (inte som ett enda
  symbolnamn). En som börjar med `::`, som `::foo`, är en absolut sökväg från roten. Ett `::` inuti
  generiska argument (`Vec<a::b>` och liknande) behandlas inte som sökvägsavgränsare.

## 2. Hur typer skrivs

I källkod skrivs typer som vanliga symboler eller listor.

- **Primitiva typer**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` är heltalstypen (CL:s integer, som automatiskt växlar mellan omedelbara 63-bitarsvärden
  och bignums; [Tal](functions/numbers.md#3-heltal-med-godtycklig-precision-int)), och de sex typerna med
  fast bredd är uppkallade efter sin bredd och teckenhantering (det finns ingen 64-bitars heltalstyp; se
  [Tal](functions/numbers.md#1-heltal-med-fast-bredd)).
- **Den rationella typen**: `ratio` (rationella tal i enklaste form). Heapallokerade som i CL, utan
  implicit konvertering med `int`/`f64` och liknande (konvertera explicit med `as`/`try-as` eller en
  konverteringsmetod; se [Tal](functions/numbers.md#5-kvoter-ratio)).
- **Råa ord vid C-gränsen**: `ptr` (en ogenomskinlig pekare), `c-long` / `c-ulong`. Bara för FFI: att
  göra en till ett värde kräver `(unsafe ...)`, och de platser där de kan förekomma är begränsade
  ([3.3 defffi](#ptr--c-long--c-ulong--råa-maskinord)). Använd inte dessa där du vill ha ett
  64-bitars heltal: de har ingen aritmetik.
- **Ogenomskinliga föränderliga typer**: `random-state` (tillståndet hos en slumptalsgenerator). Den kan
  inte läggas i `Vector<T>`/`HashTable<K,V>`/`Sexpr` (den kan läggas i `Option<T>`/`Result<T,E>`).
- **Typen Unit**: `()`
- **Typen Never**: `!` (typen på divergerande uttryck som `panic`/`unreachable`/`todo`/en slinga som aldrig
  returnerar. Den passar vilken förväntad typ som helst)
- **Funktionstyper**: `(fn (argumenttyper...) returtyp)`. Typen på en funktion med variadiska argument är
  `(fn (argumenttyper... &rest elementtyp) returtyp)`.
- **Generiska typer**: `Name<T1,T2,...>` (läses som en enda token utan mellanslag).
  Till exempel `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Enhetstypen `()` kan också skrivas som typargument (`Result<(), FileError>`). `(`/`)` är normalt
  avgränsare som avslutar en token, men medan en vinkelparentes är öppen släpps just det här teckenparet
  igenom. `()` kan också användas som fälttyp eller argumenttyp.
- **Tillämpningsformen för generiska typer**: `(Name T1 T2 ...)`, en listskrivning som namnger samma typ som
  `Name<T1,T2,...>`. Till exempel är `(vector char)` samma som `Vector<char>`.
  Namnformen är det vanliga sättet att skriva; den här formen **finns för när ett typargument inte kan
  stavas inuti ett namn**: ett typargument är självt ett typuttryck, men inuti ett namn med en enda token
  kan bara namn, `()` och `:dyn` skrivas, inte funktionstyper (det finns ingen stavning som
  `Vector<(fn (i32) i32)>`). Den kan också förekomma i den här formen när implementationen visar en typ,
  som resultatet av att sätta in ett traits associerade typ i en signatur.
- **Kvalificerade typnamn**: kan kvalificeras med `::`, som i `module::Type`.
- **Trait-objekttyper**: `:dyn Trait` (två ord åtskilda av mellanslag som bildar en typ). Representerar ett
  värde vars konkreta typ avgörs vid körning; trait-metodanrop går genom en vtable (dynamisk dispatch).
  För ett trait med associerade typer fastställs de positionellt i deklarationsordning (`:dyn Iter<i32>`
  fastställer `Item` till `i32`). Det kan också skrivas inuti generiska argument: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. Konkreta värden boxas automatiskt på förväntade positioner;
  den explicita formen är `(as :dyn Trait expr)`.
  Ett värde av `:dyn Sub` kan skickas som det är där en `:dyn Super` krävs för något av dess supertraits
  (allt det ärver, transitivt) (uppkastning). Det kan inte skickas till ett orelaterat trait.
  För villkoren ett trait måste uppfylla för att kunna användas med `:dyn`, se
  [3.9 deftrait / impl](#39-deftrait--impl--traits). Att skriva `:dyn` utanför en typposition är ett fel.
- Inbyggda generiska typer: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, och samtidighetstyperna `Task<T>` / `Thread<T>` / `Chan<T>`
  ([kapitel 12](#12-samtidighet-tasks)). Det finns också `Sexpr`, typen för S-uttrycksdata. De
  inbyggda konkreta feltyperna är `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, och standardbiblioteket har structs `SimpleError` / `WrappedError`
  (`Error` är inte en typ utan ett trait: använd det som `:dyn Error`). Listan finns i [types.md](types.md).
- **Typer och traits delar en namnrymd** (som i Rust): inom en modul kan en typ
  (`defstruct`/`defenum`) och ett trait (`deftrait`) inte ha samma namn.

## 3. Definitioner på toppnivå

### 3.1 defun — funktionsdefinitioner

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Argumenttyperna och returtypen är obligatoriska.
- En generisk funktion skriver sina typparametrar inom vinkelparenteser efter sitt namn:
  `(defun name<T1,T2...> (params) Ret body...)` (samma vinkelparentessyntax som `Vector<T>` på
  typpositioner).
- `defun`/`lambda`/`defmethod` accepterar variadiska argument när `&rest (name Type)` skrivs sist:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (i kroppen är `xs` alltid bunden som en
  `Option<Sexpr>`, en lista av S-uttryck. Varje faktiskt argument vid anropet typkontrolleras som `Type2`
  individuellt och omsluts sedan i en `Sexpr`).
  `defmacro` har också sitt eget `&rest`, men skiljer sig genom att det alltid är en otypad `Sexpr`
  (`defun`/`lambda` anger elementtypen). En funktionstyp kan också beskriva en variadisk funktion, som
  `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (för `defun` och `defmethod`; inte för `lambda`/`labels`, av skälet nedan, och
  `defmacro` har en separat implementation, också nedan). Ordningen är CL:s:
  `required &optional &rest &key`. Varje parameter skrivs `(name Type)` eller
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; inget standardvärde
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> i kroppen

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; med ett standardvärde
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; anroparen skriver `:name value`, i vilken ordning som helst; utelämnade får sina standardvärden
  ```

  - **En parameter utan standarduttryck har typen `Option<Type>`.** Utelämnad är den `none`; skickad
    omsluts det bara värde anroparen skrev automatiskt i `some`. Det CL gör med en supplied-p-variabel
    ("angavs den?") visar sig i stället på den statiska typens sida.
  - Med ett standarduttryck förblir typen `Type` som deklarerad. När den utelämnas bäddas det
    **kontrollerade uttrycket** in vid anropet som det är (utvärderas vid varje anrop).
  - **`&key` kan inte blandas med `&optional`/`&rest` i en argumentlista.** Detta undviker en tvetydighet
    som CL självt har (om ett avslutande faktiskt argument tas av ett positionellt `&optional` eller
    matchas med etikett som ett `&key` beror på *värdena*) genom att förbjuda kombinationen. `&optional`
    och `&rest` kan användas tillsammans.
  - De kan användas i generiska funktioner, men **en typparameter som bara förekommer i utelämnade
    argument kan inte härledas och är ett fel** (det finns inget värde att matcha mot).
  - **`defmethod` kan ha samma tre avsnitt** (både för instansmetoder och statiska funktioner).
    Lista `&optional`/`&rest`/`&key` efter mottagaren:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; statisk funktion
    (point::origin :y 7)
    ```

    De kan användas också i metoder för generiska typer, men **typen på en parameter med ett
    standarduttryck kan inte nämna ägarens typparametrar** (samma begränsning som `defun` har för sina
    egna typparametrar: det som bäddas in när argumentet utelämnas är ett *kontrollerat* uttryck, så dess
    typ kan inte lämnas som en abstrakt variabel).
  - **De kan inte användas i trait-metoder.** `deftrait` har ingen syntax för dem, och om bara
    `impl`-sidan kunde deklarera avsnitt skulle anrop med en `:dyn`-mottagare (som fyller argument från
    traitets deklaration) och anrop med en konkret mottagare (som fyller dem från `impl`-deklarationen)
    bli olika saker. Aritet för en vtable-plats är fast.
  - **De kan inte användas i `lambda` / `labels`** (`&rest` kan). För att fylla i ett utelämnat argument
    måste anroparen läsa **den anropades kontrollerade standarduttryck**, vilket bara finns tillgängligt
    från en signatur som löses upp efter namn. En `lambda` skickas runt som ett värde, och det enda som
    beskriver det värdet är dess funktionstyp `(fn ...)`: det finns ingen plats i den för ett uttryck, och
    om det fanns skulle "två lambdas med samma signatur men olika standardvärden" bli olika typer.
    `&rest` stannar inom typfrågor, så det kan skrivas i en funktionstyp.
- **Framåtreferenser deklareras med `defsignature`** (nedan). Ett namn som inte har deklarerats kan inte
  anropas före sin definition, eftersom toppnivån kontrolleras och körs en form i taget, i
  källordning.
- För att kräva trait-gränser skriver man en `where`-sats precis före kroppen:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (att fastställa en associerad typ med `(AssocName ConcreteType)` är valfritt).
- **Dokumentationssträngar**: en strängliteral i början av kroppen, direkt efter `where`-satsen (om den
  finns), blir dokumentationssträngen (som i CL). Dock bara när minst en kroppsform följer efter den: en
  ensam sträng förblir returvärdet och tas inte som dokumentationssträng: `(defun f () string "doc" "value")`
  har en dokumentationssträng och returnerar `"value"`, medan `(defun f () string "value")` inte har någon
  dokumentationssträng och returnerar `"value"`. Den kan hämtas med `(documentation name)`
  ([dokumentationssträngar](functions/system.md#7-dokumentationssträngar--documentation)).

### 3.2 defsignature — framåtdeklarationer

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

För att anropa en `defun` som är definierad **senare** än du själv deklarerar du den först så här.
Ömsesidig rekursion kan bara skrivas på det här sättet:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Argumenten listas som **bara typer**; det finns ingen kropp, så det finns inget att ge namn åt. `&rest`
kan skrivas sist, som `&rest elementtyp`.

Deklarationer **kontrolleras**:

- Definitionen som följer måste stämma med deklarationen (antalet och typerna på argument, returtypen,
  `&rest` och om den är `pub`). En avvikelse är ett fel vid definitionen.
- Att deklarera utan att definiera är ett fel (rapporteras när filen / modulen har lästs in klart). REPL
  rapporterar det inte efter varje indata, eftersom en deklaration och dess definition ska kunna skrivas på
  separata rader.
- En deklaration som placeras **efter** definitionen är ett fel, eftersom en sådan deklaration inte kunde
  göra något.

Tre saker kan inte deklareras:

- **Generiska funktioner.** Att göra en kopia för varje typ kräver kroppen, och en deklaration har ingen.
  Ett framåtanrop kunde lösas upp men instansieringen skulle misslyckas, så deklarationen avvisas på
  förhand.
- **`&optional`/`&key`.** Deras signatur innehåller det **kontrollerade** uttrycket för varje standardvärde
  (inbäddat vid anropet när argumentet utelämnas), och en deklaration har ingen plats för det.
- **Allt annat än `defun`.** Ett `defmacro` kräver att makrokroppen **redan har körts** för att kunna
  expandera, vilket inte kan ersättas av att registrera en signatur. För typer
  (`defstruct`/`defenum`/`deftrait`) är registreringen "vad koden som registrerar själva typen behöver",
  vilket inte är fristående på det sätt en signatur är. En `defmethod` registreras på typen som äger den,
  så den följer typen.

CL:s motsvarighet är `(declaim (ftype (function (i32) bool) even2))`, men det kommer med ett helt
deklarationssystem och är bara **rådgivande**. Här, med statisk typning, kontrolleras deklarationer.

### 3.3 defffi — deklarera C-funktioner (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = C-symbolens namn
(pub defffi ...)
```

Deklarerar en C-funktion så att den kan anropas. Formen är densamma som för `defsignature` (ett namn,
argumenttyper, en returtyp och ingen kropp), men att sakna kropp betyder något annat. `defsignature` är ett
löfte om att "jag definierar den senare", medan `defffi` deklarerar att "någon annan redan har skrivit och
kompilerat kroppen".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

typelisp-namnet och C-symbolens namn kan skrivas separat eftersom typelisp-identifierare vanligtvis
innehåller `-` och C-identifierare inte kan det. Om C-namnet utelämnas används namnet som C-symbolens namn
som det är.

**Anrop kräver `(unsafe ...)`** (även för funktioner som bara tar skalärer). Kompilatorn har inget sätt att
bekräfta att den deklarerade C-signaturen stämmer med den verkliga och kan bara lita på deklarationen;
`unsafe` är märket för att du tar på dig det ansvaret. Det avsedda sättet är att omsluta det en gång och
göra ett säkert omslag:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; inget unsafe behövs härifrån
```

De typer som kan skrivas är `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, och typade pekare `(ptr T)`
([nedan](#def-c-struct-och-typade-pekare--allokera-c-structs)).

`string` är `const char *`. typelisp-strängar är inte NUL-terminerade och kan själva innehålla NUL, så
**de kopieras till en C-sträng när de skickas**, och frigörs efter anropet. En NUL i strängen är ett fel:
C skulle bara titta fram till den, så en annan sträng skulle tyst skickas.

**Returnerade strängar kopieras också**, och frigörs inte: det C returnerar tillhör C, och det kan peka in
i en statisk tabell, som med `getenv`. Funktioner som returnerar minne som anroparen måste frigöra
(`strdup` och så vidare) bör tas emot som `ptr` och frigöras av dig själv.

Funktioner vars resultat pekar inuti ett argument (`strchr`, `strstr`) fungerar också korrekt: resultatet
kopieras innan argumentet frigörs.

Om en funktion som deklarerats returnera `string` returnerar NULL är det ett fel, eftersom `string` inte
har något värde som betyder "det fanns ingen". Om NULL är möjligt, ta emot resultatet som `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Med `:library` öppnas det delade biblioteket och symbolen slås upp i det. Utan det slås symbolen upp i
**själva processen** (allt som redan är länkat, inklusive libc). Ett kort namn som `sqlite3` slås upp som
`libsqlite3.dylib` / `libsqlite3.so` i den ordningen, och ett namn som innehåller `/` behandlas som en
sökväg. Öppnade bibliotek stängs aldrig: kod som pekar på deras funktioner fortsätter köra, så den enda
korrekta livslängden är processens.

#### ptr / c-long / c-ulong — råa maskinord

`ptr` är en ogenomskinlig pekare (`void *`, `FILE *`, vad deklarationen än avsåg). `c-long` / `c-ulong` är
C:s `long` / `unsigned long` (även `size_t`, `int64_t` och `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Att inte kalla dem `i64` / `u64` är avsiktligt.** Det här språket har ingen 64-bitars heltalstyp, eftersom
ett taggat omedelbart värde bara har 63 bitar ([kapitel 2](#2-hur-typer-skrivs)). Namnet `c-long` säger "det
här är ett ord som korsar gränsen mot C, inte ett heltal i det här språket".

**De har ingen aritmetik.** `(+ x 1)` kan inte skrivas. Det kunde tillhandahållas men gör det inte, så att
ingen beräkning körs på ett värde som inte kan lagras någonstans och har en annan bredd än alla andra tal,
av samma skäl som 64-bitars heltalstypen utelämnades. Det finns **bara konverteringar**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; läs det som kom tillbaka
(as int (unsafe (c-strlen s)))               ; den här för att läsa det exakt (int förlorar inte 64 bitar)
(try-as i32 (unsafe (c-strlen s)))           ; fråga om det ryms
(as c-ulong n)                               ; gör ett av ett annat heltal
```

Heltals-**literaler** tar den förväntade typen, så inget `as` behövs bara för att skicka en:

```lisp
(unsafe (c-malloc 16))                       ; 16 läses som en c-ulong
```

Literaler utanför intervallet avvisas som med andra bredder (`(c-malloc -1)` ryms inte i en `c-ulong`).

**De platser där de kan förekomma är begränsade**: bara argumenttyper, returtyper och lokala variabler.
Vart och ett av följande är ett fel:

```lisp
(defstruct handle (p ptr))          ; ett struct-fält
(defenum maybe (none) (some ptr))   ; ett enum-fält
(defvar (block ptr) ...)            ; en global
(defffi f ((vector ptr)) i32)       ; inuti ett typargument
```

Det finns ett skäl för alla: **platsen taggar det den håller**. Att tagga skulle tappa pekarens översta
bitar, samma skäl som 64-bitars heltalstypen utelämnades, så det är inte tillåtet ens i `unsafe`. Det är
inte en fråga om tillstånd: den representationen finns inte.

Av samma skäl kan de inte vara lokala variabler som **fångas** av nästlade funktioner (en fångad bindning
hamnar i en cell, och en cell taggar det den håller). Detta är känt vid kompilering och rapporteras av
`(compile f)`.

GC spårar inte `ptr`. Den pekar utanför heapen, så det är korrekt.

Fyra saker kan inte deklareras:

- **Variadiska argument** (`printf`). Den variadiska delen skickas enligt andra regler än de fasta
  argumenten (på stacken på AArch64 Darwin), så den kan inte anropas korrekt från en fast signatur.
  `&rest` avvisas.
- **Att skicka eller returnera structs per värde.** Av samma skäl (det beror på varje plattforms
  anropskonvention). De typer som kan skrivas är begränsade till listan ovan, så det kan inte stavas.
- **Generiska typer.** C har ingen motsvarighet.
- **Samma namn som en inbyggd funktion.** Ett kompilerat anrop skulle lösa upp det namnet till den
  inbyggda, så det avvisas i stället för att tyst bli fel.

#### Callbacks — låta C anropa tillbaka

Att skriva en funktionstyp `(fn (types...) return-type)` som argumenttyp gör det argumentet till en
funktion som C anropar tillbaka (en callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; en toppnivåfunktion
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; en lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; en lokal funktion
```

En C-funktionspekare är inget annat än en kodadress, och C anropar den och skickar bara de deklarerade
argumenten. Det finns ingen plats att skicka fångade variabler, så **bara funktioner utan fria variabler
kan skickas**, och detta kontrolleras vid typkontrollen.

- Skriv ett funktionsnamn eller ett `lambda`-uttryck **direkt** som faktiskt argument. En variabel som
  håller en funktion kan inte skickas: vilken funktion den håller, och därmed om den har fria variabler,
  är inte känt förrän vid körning.
- En `lambda` är ett fel om den refererar till lokala variabler utanför sig. Globala variabler och
  toppnivåfunktioner får refereras.
- En lokal funktion (`labels`) får inte ha några fria variabler, inklusive de i syskonfunktionerna den
  anropar. Syskonfunktioner delar platsen där fångade variabler förvaras, så det en anropad syskonfunktion
  fångar fångas också av den här funktionen.
- En generisk funktion får sina typer från den deklarerade funktionstypen.
- De typer som kan skrivas i funktionstypen är desamma som listan ovan. Dock kan `string` inte vara
  returtyp för en callback (det skulle ge C minne som ingen frigör). Ett `string`-argument kopierar
  strängen C skickade till en typelisp-sträng.

C-funktionsanrop kan bara skrivas inuti `unsafe`, så callbacks kan bara skickas inuti `unsafe`.

**Callbacken kan bara anropas medan den C-funktion som typelisp anropade körs.** Om den anropas från någon
annanstans (en tråd som inte kör typelisp, en signalhanterare, en funktion registrerad med `atexit`)
skriver den ut skälet och stoppar processen.

**Misslyckanden fortplantar sig inte genom C.** En `panic` eller `throw` inuti callbacken kan inte vinda
av genom C-ramar (det vore odefinierat beteende), så 0 returneras till C, och misslyckandet kastas om till
anroparen när C-funktionen returnerar. Om callbacken anropas igen mellan misslyckandet och C-funktionens
retur körs den inte och 0 returneras.

En operation som skulle behöva vänta inuti en callback (en `recv` på en tom kanal och så vidare) är ett
fel ([12.6](#126-kompilerad-kod-och-tasks)).

När en funktion omdefinieras anropas den nya definitionen från nästa gång den skickas till C.

Det fungerar på samma sätt med AOT (`compile-file`). De ingångspunkter C anropar byggs in i den körbara
filen.

**De kan inte skickas som värden.** En FFI-deklaration kan inte skrivas som den är för `f` i
`(map f xs)`: ett funktionsvärde är en closure som omsluter kroppen av en definition, och den här
deklarationen har ingen kropp att omsluta. Omslut den i en `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` avvisas också: det som kunde visas är C:s maskinkod, som den här kompilatorn inte
producerade. `(compile c-abs)` lyckas (och gör ingenting, eftersom den redan är kompilerad).

**Det fungerar också med AOT (`compile-file`).** Länkaren löser upp själva C-funktionerna. Om en
deklaration har `:library` läggs det biblioteket till på länkraden som `-l` (dubbletter slås ihop till
en), så `compile-file` behöver inga extra argument. `compile-file` läser själv källkoden, så den kan
samla dem från deklarationerna.

Symboler slås upp också vid bygget. Om en deklarerad funktion inte finns nämner felet den vid namn före
något länkfel.

Standardbiblioteket (prelude) använder inte `defffi`. Standardbiblioteket går in hela i varje körbar fil,
så en deklaration med `:library` där skulle länka det biblioteket även in i program som inte använder FFI.

#### def-c-struct och typade pekare — allokera C-structs

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Deklarerar en struct med samma layout som i C. Den kan bara skrivas inuti ett `unsafe` på toppnivå (som
inte kan innehålla något annat än `def-c-struct`). En dokumentationssträng kan sättas direkt efter namnet.

De typer som kan skrivas för fält är `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, typade pekare `(ptr T)` och andra `def-c-struct` (inbäddade per värde). Layouten
(varje fälts offset, samt structens storlek och justering) beräknas enligt C:s regler (under antagande av
LP64). Ett fält som pekar på structen själv kan skrivas, men structen kan inte bädda in sig själv.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x vid 0, y vid 8, storlek 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Namnet på en `def-c-struct` går in i typnamnrymden (ingen `defstruct` eller liknande med samma namn kan
finnas i samma modul), men **det är inte typen på ett värde**. Du kan inte skriva
`(defun f ((p point)) ...)`; det förekommer bara som det en typad pekare pekar på.

**En typad pekare `(ptr T)`** är en adress som pekar på ett `T`. `T` är en av de typer som kan skrivas för
fält ovan. Det är ett rått maskinord som `ptr`, med samma regler för var det kan förekomma (bara
argument, returtyper och lokala variabler; det kan vara ett värde bara inuti `unsafe`).

Allokering, läsning och skrivning skrivs i följande former. Alla kan användas bara inuti `unsafe`.

| Form | Betydelse |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Allokerar `n` värden av `T` (1 om utelämnat). Innehållet fylls med 0. Returnerar en `(ptr T)` |
| `(c-ref p i)` | En pekare till element `i` från `p`. Ett fel om utanför det allokerade intervallet |
| `(c-deref p)` / `(setf (c-deref p) v)` | Läser / skriver den skalär `p` pekar på |
| `p::field` / `(setf p::field v)` | Läser / skriver ett fält i en struct. Att läsa ett fält som är en inbäddad struct ger dess adress (`(ptr inner-type)`) |
| `(as ptr p)` | Glömmer typen och gör en `ptr` (för att skicka till något som `void *` i `qsort`). Det finns ingen konvertering tillbaka |

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

**Allokerat minne frigörs när kontrollen lämnar det `unsafe` som allokerade det.** Ägaren är det lexikaliskt
yttersta `unsafe` inom samma funktion. Det frigörs oavsett om koden slutar normalt eller lämnas med
`panic`, `throw` eller `return-from`. `lambda` och `labels`-funktioner är separata funktioner, så en
`c-alloc` i dem behöver ett eget `unsafe` inuti dem.

Därför kan en typad pekare inte lämna det `unsafe` som allokerade den. Vart och ett av följande är ett
typfel:

- Att göra den till värdet av `unsafe`-uttrycket (så den kan inte heller returneras från en funktion)
- Att fånga den i en closure (`lambda`, `labels`)
- Att skicka den till `task` / `thread`
- Att kasta den med `throw`

För att använda värden utanför `unsafe` kopierar man dem till en `defstruct` eller tal inuti `unsafe` och
returnerar de.

**Minne som allokerats på C-sidan hanteras inte.** Värden som kommer in från C som typade pekare
(`defffi`-returvärden, callback-argument, värden lästa från pekartypade fält) kontrolleras vid körning för
att se om de pekar på ett värde av den typen inom en levande `c-alloc`-allokering, och är ett fel om de
inte gör det. NULL är också ett fel. För att ta emot minne som C allokerat, eller NULL, använd den otypade
`ptr` (vars innehåll inte kan läsas).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

När en callbacks argument avvisas av kontrollen rapporteras det till anroparen när C-funktionen
returnerar, precis som ett misslyckande inuti en callback.

### 3.4 defvar / defparameter / defconstant — globala variabler

```lisp
(defvar (name Type) init-expr)        ; initierar bara om den inte redan är bunden
(defparameter (name Type) init-expr)  ; tilldelar varje gång
(defconstant (name Type) init-expr)

; med en dokumentationssträng (i samma ordning som CL:s defvar/defparameter/defconstant: efter värdet)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Skillnaden mellan `defvar` och `defparameter` syns vid omladdning** (som i CL). Om globalen **redan är
bunden utvärderar `defvar` inte ens initieraren**, så när du redigerar en inställningsfil och läser den
igen förblir de värden sessionen ändrade som de är. `defparameter` tilldelar varje gång, så att läsa den
igen för tillbaka värdena till det som står skrivet.

Typannoteringen är obligatorisk (den härleds inte från initieraren). `defvar` kan ändras; `defconstant`
kan inte (`setf` är ett fel).

### 3.5 defmethod — metoddefinitioner

```lisp
; instansmetod: kan anropas som (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; statisk / associerad funktion: kan anropas som (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Anroparen löser upp metoden från den statiska typen på `obj` (enkel, statisk dispatch). En
dokumentationssträng kan placeras på samma position och under samma regler som för `defun` (direkt efter
`where`-satsen, i början av kroppen, bara när kroppsformer följer). Detsamma gäller metoder inuti `impl`;
de hämtas med `(documentation Type::method)`.

En metods egna typparametrar skrivs i namnet med `<...>`, som för `defun`. Mottagartypens
typparametrar (`T` nedan) bestäms av mottagaren; metodens egna (`U`) härleds från argumenten i varje
anrop.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Metodens egna typparametrar ska ha andra namn än både typparametrarna som mottagartypen deklarerar
  (`T` i `(defstruct Box<T> ...)`) och namnen som står i mottagaren.
- Om mottagartypen är generisk skriver man i mottagaren antingen alla dess typparametrar som
  variabler (`Box<T>`) eller alla som konkreta typer (`Box<int>`).
- En metod i `impl` kan inte lägga till typparametrar: dess signatur följer den som traitet
  deklarerar.

### 3.6 defstruct — structs (användardefinierade typer)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generisk (typparametrar inom vinkelparenteser)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Varje fält är `(name type)` eller `(pub name type)` (synlighet per fält, oberoende av structens eget
  `pub`). Ytterligare ett uttryck på slutet blir platsens **standardvärde** (`(x i32 0)`); se
  alternativlistan nedan.
- Följande genereras automatiskt:
  - Konstruktorn `Name::new` (argument i fältens ordning)
  - Getters `(field-name instance)`, med sockret `instance::field-name`
  - Setters `(set-field-name instance value)`, med sockret `(setf instance::field-name value)`
- För att göra själva structen `pub` sätter man `pub` framför, som i `(pub defstruct ...)`.
- **Definiera en typ innan du namnger den.** Ett fälts typ kan vara själva structen (`(next Option<node>)`),
  men inte en typ som definieras senare: typer har ingen framåtdeklaration som motsvarar `defsignature`.
  Ett namn som ännu inte är definierat ger samma `unknown type`-fel i en `defun`-argumenttyp eller i
  `the`. Så två typer som refererar till varandra kan inte skrivas.
- **Typvariabler är bara de som skrivs på deklarerande positioner.** För
  `defun`/`defstruct`/`defenum`/`deftype` är det `<T>` i namnet; för `defmethod` mottagarens typ
  (`(self box<T>)`, eller `box<T>` för en statisk metod) och `<U>` i metodens namn; för `impl`
  måltypen och `impl<T>`; för `deftrait` `Self` och de associerade typerna i `(type Item)`. Ett namn
  som förekommer för första gången någon annanstans (argument, returvärdet, `the`/`lambda` i
  kroppen) blir inte en typvariabel; det är `unknown type`.
- **Dokumentationssträngar**: en strängliteral direkt efter namnet, före fälten, blir dokumentationssträngen
  (`(defstruct Name "doc" (field Type)...)`, samma position som CL:s `defstruct`). Ett fält har alltid
  formen `(name Type ...)` och kan aldrig vara en bar sträng, så det finns ingen tvetydighet. Hämta den med
  `(documentation Name)`.

#### Alternativlista

Att skriva en lista `(Name option...)` på namnpositionen anger alternativ (samma position som i CL).

```lisp
(defstruct (point (:constructor make-point)          ; nyckelordskonstruktor
                  (:constructor at (x &optional y))  ; BOA-konstruktor
                  (:copier copy-point))
  (x i32 0)          ; ett tredje element är den platsens standardvärde
  (y i32 0))

(point::make-point :y 7)   ; x är 0
(point::at 1)              ; y är 0
(point::at 1 2)
(copy-point p)             ; en ytlig kopia (samma som CL:s copier)
```

- **`:constructor`**: det som genereras är en **statisk funktion** för typen (`point::make-point`), vars
  kropp alltid är `(point::new ...)`. `new` förblir den enda strukturella konstruktorn; det som görs här
  är ett *sätt att anropa* den. Flera kan deklareras.
  - `(:constructor name)` tar varje plats som `&key`. **Varje plats behöver ett standardvärde** (det här
    språket har inget som motsvarar CL:s "obunden plats").
  - `(:constructor name (slot...))` tar de namngivna platserna som positionella argument (i vilken ordning
    som helst). Platser som inte nämns fylls med sina standardvärden, så **de behöver standardvärden**.
    Efter `&optional` får resten utelämnas (och behöver likaså standardvärden).
- **`:copier`**: genererar en **instansmetod** som returnerar ett nytt värde med samma platsvärden. Ytlig,
  som CL:s copier.
- **`:include Parent`**: lägger föräldrens platser först (standardvärden ärvs också; föräldern får finnas i
  en annan fil). **Det skapar ingen typrelation**: barnet är inte en subtyp av föräldern, förälderns
  metoder gäller inte barnet, och det finns inget test vid körning som länkar de två. Det här språket har
  ingen subtypning; gemensamma gränssnitt är `deftrait`s uppgift. Bara *listan* över platser förenas.
- **Platsernas standardvärden läses bara av genererade konstruktorer.** Att skriva ett standardvärde utan
  att deklarera någon `:constructor` är ett fel, eftersom det aldrig kunde användas.
- Alternativ som utelämnats, och varför:
  - **`:conc-name`**: i CL sätter det ett prefix på accessorer för att undvika krockar i en enda platt
    funktionsnamnrymd. Här är accessorer metoder som dispatchas på mottagarens typ, så krockar uppstår inte,
    och ett prefix skulle bryta `instance::field` (som bara känner till platsnamnet).
  - **`:predicate`**: svarar vid körning på "är det här värdet ett `point`?". Här är typer en klassificering
    vid kompilering utan vittne vid körning, och det finns ingen position där "ett värde av okänd typ som
    kan vara ett point" existerar (`match` på `Sexpr` är förseglad, och `:dyn` kan inte nedkastas), så ett
    genererat predikat kunde bara någonsin returnera `true`.
  - **`:type` / `:initial-offset` / `:named`**: dessa ersätter värdets representation med en lista eller
    vektor. Representationen tillhör kompilatorn och kan inte observeras från språket.

### 3.7 defenum — enums (summatyper)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; en variant med nyttolast (positionella fält)
  (Variant2)                  ; en variant utan nyttolast
  ...)

; generisk
(defenum Option<T>
  (Some T)
  (None))
```

- Varje variant har formen `(VariantName FieldType...)`. Fält är bara positionella (de har inga namn).
  Minst en variant behövs, och namn kan inte upprepas.
- Värden byggs, som med de inbyggda `Option`/`Result`, kvalificerade eller genom `use`:
  `(Name::Variant1 a b)`, eller `(Variant1 a b)` efter `(use Name)`.
- De kan plockas isär med `match` / `if-let`. `match` kontrollerar uttömmande täckning (den måste täcka
  varje variant eller ha en `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Metoder och associerade funktioner läggs till efteråt med `defmethod`/`impl`, som med `defstruct`.
- För att göra själva enumen `pub` skriver man `(pub defenum ...)`.
- **Dokumentationssträngar**: samma position och regler som `defstruct`, direkt efter namnet, före
  varianterna (`(defenum Name "doc" (Variant ...)...)`). Hämta den med `(documentation Name)`.

### 3.8 deftype — typalias

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CL:s `deftype`, avgränsad till det som är meningsfullt i ett statiskt typat språk: **ett sätt att stava en
typ, inte en typ**.

- Namnpositionen är densamma som för `defun`, och generiska argument skrivs `Name<T,U>`. På
  användningsstället behövs exakt det deklarerade antalet typargument (för många eller för få är ett fel
  på stället).
- Expansionen sker **inuti typtolkaren**. Så ingenting längre ned känner till att aliaset finns:
  monomorfiseringsnycklarna, dumpar, kompileringsvägen och **felmeddelanden** visar alla den expanderade
  formen. Om `(f "x")` misslyckas mot en funktion som kräver `meters` säger meddelandet `i32`.
- **Det är inte en ny typ.** `(deftype meters i32)` gör `meters` och `i32` till samma typ, så att blanda
  ihop dem upptäcks inte. Om du vill hålla dem isär, använd `defstruct`.
- **Det är inte ett predikat.** CL:s `(deftype small () '(integer 0 9))` beskriver en *mängd värden* som
  `typep` testar vid körning, men här är typer en klassificering vid kompilering utan vittne vid körning,
  så ett alias som begränsar värden skulle inte ha något att begränsa.
- **Det kan inte innehålla sig självt.** Ett alias expanderas där det är skrivet, så det finns ingenstans
  för det att rekursera till. Rekursiva datatyper skrivs med `defstruct`/`defenum`.
- Det delar namnrymd med typer och traits (inom en modul kan det inte ha samma namn som en
  `defstruct`/`defenum`/`deftrait`). Gör det publikt med `(pub deftype ...)` och hämta in det med
  `(use m::meters)`.
- **Dokumentationssträngar**: direkt efter namnet, före typen (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; listan över supertraits är obligatorisk; () om inga
  (type AssocName)                       ; associerade typer (hur många som helst, valfritt)
  (method-name ((self Self) params...) RetType)          ; ingen kropp = måste implementeras
  (method-name ((self Self) params...) RetType body...)) ; med kropp = standardimplementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; gränser som gäller hela impl (valfritt)
  (type AssocName ConcreteType)          ; gör en associerad typ konkret
  (method-name (recv params...) RetType body...))
```

Genom `impl` registreras varje metod som en vanlig `defmethod` för `TargetType`. Traits refereras som
trait-gränser i `where`-satserna för generiska funktioner (se [3.1 defun](#31-defun--funktionsdefinitioner)).
Ett traitnamn kan också vara en `::`-sökväg som `m::Trait`.

**Listan över supertraits (obligatorisk)**: skrivs alltid direkt efter traitnamnet. Varje element är ett
bart traitnamn, eller, om det traitet har associerade typer, `(Trait (Assoc Type))` med **alla dess
associerade typer fastlagda**.

```lisp
(deftrait Eq () ...)                       ; inga supertraits
(deftrait Ord (Eq) ...)                    ; Rusts trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; fastlägga en associerad typ
  (rewind ((self Self)) ()))
```

Arv har tre effekter. (1) `impl Ord X` kräver att `impl Eq X` skrivs **först** (en regel om
skrivordning: den enda form som kan avgöras deterministiskt i REPL och med stegvis `load`, och strängare
än Rust). (2) `(where (Ord T))` ensamt låter dig anropa också `Eq`s metoder. (3) `Eq`s metoder kan
anropas genom en `:dyn Ord`, och ett värde av `:dyn Ord` kan skickas som det är där en `:dyn Eq` krävs
(uppkastning). Att ett subtrait omdeklarerar en metod med samma namn som sin förälder, och att ärva metoder
med samma namn från två föräldrar, är båda fel (en vtable har en plats per namn). Diamantarv slås ihop till
en plats.

**Standardimplementationer**: en kropp efter signaturen används när en `impl` utelämnar metoden. Kroppen
löses upp i **namnrymden för den modul** där traitet är skrivet, så den kan anropa icke-publika funktioner
i den modulen. Metoder med kroppar kan också ha `where`-satser och dokumentationssträngar. Kroppen
typkontrolleras **en gång, vid deklarationsstället**, med `Self` kvar som en typvariabel (begränsad av
`Self: själva traitet`), som i Rust: fel som skulle misslyckas för varje `impl` och varje implementerande
typ, även i standardvärden som ingen `impl` någonsin utelämnar, fångas där. Anrop på `self` till metoder i
själva traitet eller dess supertraits passerar genom den här gränsen, och associerade typer är fastlagda
till sig själva, så en signatur som returnerar `Item` matchas mot kroppen utan att känna till den konkreta
typen.

**Generella implementationer (blanket implementations)**: att göra målet till en typvariabel
implementerar traitet på en gång för varje typ som uppfyller gränserna.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; ingen kropp alls; allt är standardimplementationen
```

**Ingen kod genereras förrän en konkret typ faktiskt använder det** (en gång per typ, med samma mekanism
som vanlig monomorfisering). Ett trait kan ha högst en generell implementation. Om en typ har en
explicit `impl` har den företräde. Typkontrollen av kroppen är skild från genereringen: den görs en gång på
deklarationsstället, **med målet kvar som en typvariabel** (som i Rust), så även en implementation som
aldrig används får sina fel fångade där om de skulle misslyckas för varje mål under de deklarerade
gränserna. Anrop som motiveras av gränserna (`(less self other)` under `(where (Ord T))` och så vidare)
passerar, som i kroppen av en generisk `defun`.

**Dokumentationssträngar**: ett `deftrait` kan ha en dokumentationssträng för hela traitet, som en
strängliteral direkt efter listan över supertraits, före posterna
(`(deftrait Name () "doc" (type ...) (method ...)...)`). En signatur utan kropp kan inte ha en
dokumentationssträng: en avslutande sträng skulle själv vara returvärdet för en standardimplementation, så
de två kunde inte skiljas åt.

De traits standardbiblioteket tillhandahåller: **`Iter`** (`next` / associerad typ `Item`; grunden för
`doiter` och sekvensfunktionerna), **`Eq`** (`equals`; `not-equals` är en standardimplementation),
**`Ord`** (ärver `Eq`; bara `less` måste implementeras, och `less-equal` / `greater` /
`greater-equal` är standardimplementationer), **`Error`** (`message` / `source`; `:dyn Error` för att
hantera feltyper enhetligt), **`print-object`** (en utskriftsform per typ), **`Pathish`**
(pathname-designatorer: en sträng eller ett `pathname`), och strömhierarkin **`Stream`** →
**`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**.
Vilka typer som implementerar vilka traits finns i [types.md](types.md); metoderna för varje trait finns i
[Standardtraits](functions/traits.md), [Feltyper](functions/option-result.md#3-feltyper-och-traitet-error),
[print-object](functions/printing.md#5-print-object-utskriftsform-per-typ) och
[Strömmar](functions/streams-files.md). Om du gör `impl` av `Iter` för din egen samlingstyp fungerar
`doiter` (kapitel 5) och `map` / `filter` / `sort` och liknande på den som de är.

Trait-anrop är **statiska** som standard (upplösta efter mottagarens statiska typ). För att hantera värden
vars konkreta typ avgörs vid körning ger trait-objekttypen `:dyn Trait` (kapitel 2) dynamisk dispatch
genom en vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; ett anropsställe, ett svar per implementation
```

Bara traits där "varje metod har en `self`-mottagare, inte använder `Self` någon annanstans än på
mottagaren, och själv varken är generisk eller variadisk" kan göras till `:dyn` (ärvda metoder måste uppfylla
samma villkor).

Bara typer vars värden har en representation på heapen kan läggas i en `:dyn`-ruta:

| Kan läggas in | Kan inte läggas in |
|---|---|
| `defstruct` / `defenum`-typer (inklusive `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` och standardbibliotekets structs), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Heltal med fast bredd (`i8` till `u32`), `f32`, `bool`, `char`, `symbol`, `()`, funktionstyper, och `Option<T>` utan ruta ([körtidsrepresentationen av Option](functions/option-result.md#2-körtidsrepresentationen-av-optiont)) |

Att placera ett värde av en typ som inte kan läggas in där en `:dyn` förväntas är ett typfel. För att
hantera sådana värden genom `:dyn` omsluter man dem i en struct, som i `(defstruct flag (v bool))`.

### 3.10 module / use — namnrymder

```lisp
(module path body...)      ; path är en följd av segment som foo eller foo::bar
(in-module path)           ; härifrån till slutet av den här enheten, inuti path (den platta formen av module)
(use path...)              ; ge funktioner, typer och moduler alias i den aktuella namnrymden
(import path...)           ; samma som use (en CL-kompatibel stavning)
(shadowing-import path...) ; ett use som medvetet tar ett bart namn som redan används
```

- `module` skapar en namnrymd. **Typer är inte namnrymder** (som i Rust har en typ bara associerade
  funktioner och metoder).
- Att göra `use` på en typ gör dess konstruktorer och publika statiska metoder tillgängliga med bara namnet
  också (till exempel kan efter `(use option)` `some`/`none` anropas utan `option::some`/`option::none`).
- Upplösningsordningen för bara namn (okvalificerade identifierare): specialformer → konstruktorer → fria
  funktioner (aktuell namnrymd → rot) → instansmetoder (upplösta efter den statiska typen på det första
  argumentet). Den går inte upp genom mellanliggande föräldramoduler.
- En kvalificerad sökväg `a::b` löser upp `a` i ordningen ovan; om det är en modul går den in i den, och om
  det är en typ löses det sista segmentet upp som en associerad post.
- **`use` påverkar formerna efter det.** En fil läses en form i taget, och beroenden löses precis före
  formen kontrolleras, så att skriva `m::f` **ovanför** `(use m)` ger `unresolved path`. Sätt `use` överst i
  filen.
- **`use` kan ta flera sökvägar** (`(use a::f b::g)`). `import` är en CL-kompatibel stavning med samma
  beteende.
- **Ett `use` vars bara namn redan är upptaget rapporteras.** Upplösning av ett bart namn tittar på modulens
  egna definitioner före alias, så `(use m::twice)` efter `(defun twice ...)` **gör ingenting**. Om du
  menar det, skriv `shadowing-import` (det kan fortfarande inte slå en definition, eftersom det inte finns
  något sätt att ta bort en; det slår bara tidigare alias).
- **`in-module` är den platta formen av `(module path body...)`.** Att skriva `(in-module geometry)` lägger
  allt därifrån till slutet av enheten (filen, eller kroppen i den omslutande `module`) inuti `geometry`.
  Det hamnar **inuti** filens egen modul (`main::geometry` för `main.typl`). Två i rad nästlas i ordning. Det
  skiljer sig från CL:s `in-package`, och har ett annat namn: i det här systemet är filen redan en modul,
  så det finns inget att "välja", och allt en form kan göra är att nästla.

### 3.11 Filer och moduler (projekt med flera filer)

Filens sökväg relativt källroten är modulens sökväg:
innehållet i `<root>/geo/point.typl` omsluts implicit i modulen `geo::point`
(en katalog är också ett segment, i Rust- / Python-stil). Ett explicit `(module bar ...)` i filen
nästlas **inuti** den (`geo::point::bar`), så den härledda sökvägen och en explicit deklaration krockar
aldrig.

- **Källrot**: lägg en manifestfil `typelisp.toml` i projektets rot (den får vara tom; valfritt kan en rad
  `src = "src"` namnge källkatalogen). Den hittas genom att gå uppåt från katalogen för målfilen. Utan
  manifest är katalogen för ingångsfilen (den aktuella katalogen för REPL) roten.
- **Inläsning vid behov**: när `(use geo::point)` refererar till en modul som inte lästs in än läses den
  matchande filen (`geo/point.typl`) in, typkontrolleras och registreras automatiskt. `use a::b::c` söker
  det längsta prefixet först: `a/b/c.typl` → `a/b.typl` → `a.typl` (eftersom `c` kan vara en post inuti en
  modul). Definitioner som syns från andra moduler behöver `pub` ([3.13 pub](#313-pub--synlighet)).
- **Cirkulära referenser är fel**: kedjan rapporteras i formen
  `circular module dependency: a -> b -> a`.
- **Körning**: `typl <file.typl>` kör en fil (utan argument, REPL). `use` i REPL löser upp filer enligt
  samma regler.
- **Kapacitet för cons-arenan**: `typl --heap-cells N` sätter **startkapaciteten** för cons-cellsarenan
  (standard 65536; formen `--heap-cells=N` fungerar också, både för att köra filer och för REPL). Arenan
  **växer genom att lägga till mer** när den tar slut. Tillväxtgränsen är 256 gånger startkapaciteten,
  och en allokering bortom den ger `heap exhausted`: startkapaciteten betyder "allokera så här mycket från
  början", och gränsen betyder "bortom detta, behandla det som en läcka".

### 3.12 load — platt inläsning

```lisp
(load "path")   ; bara på toppnivå; path är en strängliteral
```

- **Platt inläsning** i CL-stil: läser formerna i målfilen **in i den aktuella namnrymden** som de är (utan
  att omsluta dem i en modul, till skillnad från `use`). Bara på toppnivå (inuti en funktionskropp är det
  ett typfel).
- `path` är relativ till katalogen för den inläsande filen (från REPL, till processens cwd). Om den inte har
  någon filändelse läggs `.typl` till.
- `(load ...)`/`(use ...)` i den inlästa filen bearbetas också rekursivt.
- **Den läser en form i taget och kör den på stället** (som CL:s `load` gör). Form *k* har körts klart
  innan *k+1* läses: även om det finns ett syntax- eller typfel mitt i har formerna före det redan körts.
  Modulfiler som läses in med `use` är annorlunda: de kontrolleras som en enhet och att köra dem överlåts åt
  den som gjorde `use` på dem (motsvarar CL:s `compile-file`).

### 3.13 pub — synlighet

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

`pub` kan sättas bara på de elva slagen ovan (inte på `module`/`use`/`deftrait`/`impl`). Det skrivs med
definitionsnyckelordet direkt efter `pub`, inte i formen `(pub (defun ...))` där definitionen omsluts av
parenteser. Ett `pub` gör exakt en definition publik (flera definitioner kan inte markeras på en gång).

### 3.14 defmacro — makrodefinitioner

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Alla parametrar och returvärdet är alltid `Sexpr`, så inga typannoteringar skrivs.
- Ohygieniska makron i CL-stil (att undvika krockar med `gensym` är makroförfattarens ansvar).
- Lambdalistan följer CL:s ordning `required &optional &rest &key` (varje markör högst en gång, och bara i
  den här ordningen).
  - `&optional` … valfria argument. `name` eller `(name default-expr)`. Standarduttrycket utvärderas vid
    expansionstillfället (det kan referera till parametrar bundna tidigare) och binds när argumentet
    utelämnas (utan standardvärde, den tomma listan `()`).
  - `&rest name` … tar emot de återstående positionella argumenten tillsammans som en `Sexpr`-lista.
  - `&key` … nyckelordsargument. `name` eller `(name default-expr)`. Anroparen skickar dem som
    `:name value` (i vilken ordning som helst). När de utelämnas används standarduttrycket (den tomma
    listan `()` om det saknas). Okända nyckelord eller en `:key`-följd av udda längd är fel.
- Exempel: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — lokala makrobindningar

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; makron med lexikal räckvidd
(symbol-macrolet ((name expansion) ...) body...)         ; ett namn står för en form
```

Båda är **uttrycks**-specialformer, och ingenting finns kvar vid körning (det som kompileras är den
expanderade formen av kroppen). Lambdalistan är densamma som för `defmacro`. De detaljerade reglerna och
exemplen finns i
[Lokala makrobindningar](functions/system.md#9-lokala-makrobindningar-macrolet--symbol-macrolet).

## 4. Bindning och villkor

```lisp
(let ((name val) ...) body...)      ; parallell bindning
(let* ((name val) ...) body...)     ; sekventiell bindning (tidigare bindningar kan användas i senare initierare)

(if cond then else)                 ; else är obligatoriskt (alltid tre element)
(when cond body...)                 ; ett if utan else (typen Unit). defmacro
(unless cond body...)               ; negationen av when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; en lista av nycklar: matchar om någon av dem gör det
  (else body...))                   ; expr utvärderas en gång. nycklar jämförs med equal.
                                     ; nycklar är "literaler" och utvärderas inte (som i CL).
                                     ; en bar symbol a betyder symbolen 'a.
                                     ; att skriva 'a är ett fel (använd den bara a). defmacro
(ecase expr (key body...) ...)      ; ett case som kräver en träff. ger panic om inget matchar. defmacro
(ccase expr (key body...) ...)      ; CL:s ccase. det finns inga restarts att erbjuda, så det är samma som ecase. defmacro
(and expr...)                       ; kortslutningsutvärdering. true med noll argument. defmacro
(or expr...)                        ; kortslutningsutvärdering. false med noll argument. defmacro
(progn body...)                     ; kör i ordning och returnerar det sista värdet
(unsafe body...)                    ; samma som progn, plus tillstånd att skriva FFI-anrop
                                     ; och råa ord. se 3.3 defffi
(prog1 form more...)                ; utvärderar allt; värdet är det av form. defmacro
(prog2 a b more...)                 ; utvärderar allt; värdet är det av b. defmacro
(the Type expr)                     ; en typannotering (ingen effekt vid körning)
```

### 4.1 unsafe — ta på sig antaganden som inte kan kontrolleras

```lisp
(unsafe body...)
```

Samma som `progn`: utvärderar kroppen i ordning och returnerar det sista värdet. Det skapar ingen räckvidd
och är ingen funktionsgräns (`break` / `return-from` går rakt igenom till utsidan). Skillnaden är att vissa
saker bara kan skrivas inuti det.

Tre saker kräver för närvarande `unsafe`: att anropa C-funktioner som deklarerats med
[defffi](#33-defffi--deklarera-c-funktioner-ffi), att göra råa maskinord (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) till värden, och [`def-c-struct` och `c-alloc`](#def-c-struct-och-typade-pekare--allokera-c-structs).

Minne som allokerats med `c-alloc` frigörs när det yttersta `unsafe` inom samma funktion lämnas. Bara det
`unsafe` har, till skillnad från `progn`, något att göra på vägen ut: frigörandet.

Det `unsafe` tar på sig är följande antaganden som kompilatorn inte kan verifiera:

- **Att typerna stämmer.** Att den deklarerade C-signaturen stämmer med den verkliga. Annars hamnar
  argument i fel register och returvärden läses med fel bredd.
- **Minnessäkerhet.** Vad C-sidan gör med det den får.
- **Processövergripande tillstånd.** Miljövariabler, signalhanterare, `errno`. Till exempel bryter ett
  anrop av `setenv` genom FFI de antaganden den här implementationens `decode-universal-time` gör när den
  räknar ut lokal tid.
- **Trådsäkerhet.**

Det är ingen väg runt typkontrollen. `(unsafe (+ 1 "two"))` passerar inte. Det som tillåts är att skriva
vissa **operationer**, inte att skriva nonsens.

Det fungerar lexikalt. Kroppen i en `lambda` som skrivs inuti `unsafe` ärver tillståndet (som med closures
inuti Rusts `unsafe`-block). Värdet kan senare anropas utifrån `unsafe`, men att skriva det där tas i sig
som att man tar ansvaret.

### 4.2 destructuring-bind — plocka isär listor efter form

```lisp
(destructuring-bind lambda-list form body...)
```

Plockar isär listan som `form` producerar **efter dess form** och binder den. Lambdalistan är den för
`defmacro` (required → `&optional` → `&rest`/`&body` → `&key`, var och en med standarduttryck), av samma
skäl som CL delar en mellan de två: de är två former som plockar isär samma sak.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Varje bunden variabel är en `Option<Sexpr>`.** Det här är inte en begränsning i implementationen utan
  naturen hos det som binds: S-uttryckslistor är de enda listorna i det här språket, så det finns ingen
  annan typ att ge elementen. Att falla tillbaka på `match` där en skalär behövs är samma som i en
  `defmacro`-kropp.
- **En form som inte stämmer ger panic** (motsvarar CL:s fel): för få eller för många element, en
  `&key`-följd av udda längd eller ett okänt nyckelord. `sexpr-car` är en tillåtande funktion som returnerar
  `()` för `()`, så utan kontrollen skulle en kort lista tyst bindas till en tom följd.
- **Nästlade lambdalistor stöds inte.** `defmacro` tar dem inte heller, så det finns en regel.
  `(a (b c))` binder inte tyst en underlista till `b`; det är ett fel som säger det.
- Standarduttrycken för `&optional` / `&key` **utvärderas bara när de används** (som i CL).
- Det finns inget som motsvarar CL:s `&allow-other-keys` (`defmacro` har inget heller).

### 4.3 match — mönstermatchning

```lisp
(match expr
  (pattern body...)
  ...)
```

Slag av mönster:
- `_` — jokertecken
- Ett variabelnamn — ett bindningsmönster (matchar alltid). Men om scrutineens typ har en variant med det
  namnet löses det upp som **mönstret för bart variantnamn nedan**
- Ett bart variantnamn — matchar en variant som inte tar några argument (`(match c (red 1) (blue 2))`). Att
  skriva en variant med fält med dess bara namn är ett aritetsfel, så skriv den inom parenteser, som i
  `(circle r)`
- **Omedelbara literaler**: heltal / `true`/`false` / tecken — jämförs som ord
- **Värdeliteraler**: strängar / flyttal / symboler (`'foo`) / bignum-heltal / kvoter — jämförs efter värde
  med den typens `Eq::equals` ([Standardtraits](functions/traits.md#2-eq--ord-jämförelse)). Strängar jämförs
  efter innehåll, inte efter identitet
- `(= expr)` — utvärderar vilket uttryck som helst och jämför med `Eq::equals`. Det enda sättet att jämföra
  typer som saknar literalsyntax (`defstruct`-instanser, globaler, beräknade resultat), och en
  användardefinierad `Eq`-implementation blir jämförelseregeln som den är. `expr` kan referera till allt
  som syns från grenens position (argument, yttre bindningar, globaler)
- `(Ctor sub-pattern...)` — konstruktormönster (`Some x` `None` `Cons a d` `Ok v` och så vidare)

Att jämföra en typ som inte implementerar `Eq` med en värdeliteral / `(= expr)` är ett typfel (det här
språket väljer att säga "dessa kan inte jämföras" i stället för att lämna en gren som tyst aldrig matchar).

**Värdeliteraler mot en `Sexpr`-scrutinee**: `Eq` för `sexpr` är `eq` (CL:s identitet), så omedelbara
värden (`'foo` (internerad) / heltal / tecken / `true`/`false`) kan skrivas som de är och matchar efter
innehåll:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Icke-omedelbara literaler (strängar / flyttal / bignum-heltal / kvoter) **kan inte skrivas** mot en
`Sexpr`. Deras `eq` jämför objektidentitet, vilket skulle ge en "gren som typkontrolleras men aldrig
matchar", så det är ett fel som namnger variantmönstret: skriv `(str "hi")` så plockas den isär till en
`string` och jämförs efter innehåll. `(= expr)` ber uttryckligen om `equals`, så den här begränsningen
gäller inte det.

**Scrutineen behöver inte vara en ADT.** `string`/`symbol`/`i32`/`f64` och liknande kan matchas direkt
(det är där strängliteralmönster hör hemma). En typ utan varianter kan dock inte täckas genom uppräkning,
så `_` (eller ett bindningsmönster som fungerar som jokertecken) krävs:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; en typ utan varianter behöver `_`
```

Mot en `Sexpr`-scrutinee kan man, förutom de 18 inbyggda variantmönstren ovan, skriva
**nedkastningsmönster** (för att ta ut instanser av användardefinierade ADT:er): syntax för att få tillbaka,
med `match`, en instans av en `defstruct`/`defenum` (kapitel 3) som implicit konverterats till `Sexpr`,
som i `(list p 42)`:

- `(TypeName sub-pattern...)` — fältuppdelning med **typnamnet** först (bara structs: en `defstruct` har
  alltid en variant, så den skrivs med typnamnet i stället för ett variantnamn). Till exempel för
  `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Ett bart variantnamn `(VariantName sub-pattern...)` — tar ut en variant av en `defenum`. Löses upp som
  ett bart namn som syns efter `(use EnumType)` (samma synlighetsregler som när konstruktorn anropas).
  Till exempel för `(defenum color (red) (blue))`, `(red)` `(blue)` efter `(use color)`. Om variantnamn i
  flera synliga enums krockar är det ett tvetydighetsfel, så den kvalificerade formen
  `(EnumType::VariantName ...)` kan också skrivas (inget `use` behövs).
- `(the Type pattern)` — en nedkastning av hela typen (att binda den som helhet). Den delar inte upp
  fält; den skickar värdet till `pattern` som det är. Det enda sättet att ta ut en föränderlig struct
  samtidigt som dess identitet behålls, och också det enda sättet att ta ut en
  `Vector<T>`/`HashTable<K,V>` ur en `Sexpr` (de har ingen form för fältuppdelning). Till exempel efter
  `(the point p)` återspeglas `(setf p::x 9)` också i originalinstansen i listan.

**Mönster för `Option<Sexpr>`**: typen för S-uttrycksdata är inte `Sexpr` utan `Option<Sexpr>`, och den
tomma listan är inte en variant av `Sexpr` utan `none` i `Option`. Så när man matchar en `Option<Sexpr>`
kan de 18 varianterna av `Sexpr` och `none` skrivas **platt i samma lista av grenar** (ingen yttre `match`
för att skala bort `Option` behövs):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; den tomma listan
    (_          9)))
```

Uttömmande täckning kontrolleras i samma platta universum: de 18 varianterna av `Sexpr` plus `none`, 19 i
allt. Att glömma `(none)` är ett fel om det inte finns en `_`. `(some x)` kan också skrivas och binder
"något icke-tomt".

Det här sockret gäller **exakt** bara `Option<Sexpr>`. För `Option<Option<Sexpr>>` skulle det vara oklart
vilket lager `(int n)` skalade bort, så skriv två nivåer av `match` som vanligt.

Samma nedkastningsmönster kan användas som de är på **en trait-objekt-scrutinee (`:dyn Trait`, kapitel 2)**:
`match` packar upp den och lämnar den sedan till `Sexpr`-mönstermaskineriet ovan, så ingen ytterligare
syntax behövs. Mängden implementerande typer är öppen, så den kan aldrig vara uttömmande, och `_` krävs:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; fältuppdelning med typnamnet först
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Typinferens över grenar**: alla grenar måste ha samma typ (utom grenar som divergerar, som med `panic`).
I en `match` skriven där ingen typ förväntas fyller grenarna i varandras saknade typargument:
`(result::ok v)` fastställer bara `T`, och `(result::err e)` bara `E`, men tillsammans fastställer de
`Result<T,E>`. Ett typargument som ingen gren kan fastställa vid slutet är ett fel i den grenen
(`cannot infer type argument ...`). Utanför `match` är ett typargument som inte kan fastställas ett fel på
stället.

Kontrollen av uttömmande täckning för en `match` som använder nedkastningsmönster räknar inte dem som
täckning av `Sexpr`s egna varianter (en `match` som bara listar nedkastningsmönster måste avslutas med
`_`). För generiska ADT:er (`defstruct point<T> ...` och så vidare) kan typargumenten i ett
nedkastningsmönster inte härledas, så formen för fältuppdelning (`(point ...)`) och formen med bart
variantnamn kan inte användas; ange dem med `the`, som i `(the point<i32> p)`.

**Nedkastningar tittar också på instansieringen.** Explicita typargument används för matchningen:
`(the point<i32> p)` släpper bara igenom värden av `point<i32>`, och ett `point<string>` passerar vidare
till nästa gren. Det beror på att ett värde kommer ihåg sin typ inklusive sina typargument (samma mekanism
som väljer `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (med bindningar) om val matchar pattern, annars els. defmacro
(while-let (pattern val) body...)   ; slingrar medan val (omvärderas varje gång) matchar pattern. defmacro
```

## 5. Iteration

```lisp
(loop body...)                      ; en oändlig slinga. lämna med break/return
(while test body...)                ; slingrar medan test är sant. defmacro
(until test body...)                ; slingrar medan test är falskt (negationen av while). defmacro
(dotimes (var count-expr) body...)  ; utvärderar count-expr en gång och låter var gå över 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; iteration i CL-stil med parallell stegning. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; den sekventiella versionen av do (let*-bindning, tilldelas i ordning). defmacro
(doiter (var coll-expr) body...)    ; itererar över ett värde som implementerar traitet Iter. defmacro

(break)                             ; lämnar bara den innersta slingan. värdet är alltid Unit
(return)                            ; lämnar bara den innersta slingan
(return value)                      ; lämnar den innersta slingan med ett värde
```

Både `break`/`return` lämnar **bara den innersta omslutande slingan** (de är inte en tidig retur från
funktionen, och de kan inte korsa en `lambda`-gräns). Typen på en `loop` är föreningen av värdetyperna för
de `break`/`return` som finns inuti den (`!` om den aldrig lämnas). För att lämna en funktion används
`return-from`, nedan.

### 5.1 `block` / `return-from` — namngivna utgångar

```lisp
(block name body...)                ; ett namngivet utgångsmål. värdet är den sista formen,
                                    ; eller värdet som skickas av return-from
(return-from name)                  ; lämnar det blocket med Unit
(return-from name value)            ; lämnar med ett värde
```

**Varje funktion i `defun` / `defmethod` / `labels` upprättar implicit ett block med sitt eget namn** (som i
CL). Så `(return-from f v)` är en tidig retur från funktionen:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` är en **lexikal** utgång, och namnet **löses upp där det skrivs**: kontrollen associerar ett
`return-from` med det omslutande `block` och för in typen på dess värde i blockets utgångstyp. Så:

- Ett `return-from` utan matchande `block` är ett **typfel** (inte ett fel vid körning).
- Ett värde vars typ inte passar de andra utgångarna eller kroppens typ är ett **typfel** (samma regel som
  för `match`-grenar).
- Om block med samma namn är nästlade **vinner det inre** (CL:s skuggningsregel).
- **Det kan inte korsa funktionsgränser.** Inifrån en `lambda` kan man inte lämna till ett yttre `block`
  (`lambda` upprättar inget block: CL:s implicita block behöver ett *namn*, och anonyma funktioner har
  inget). Det som behöver korsa är `catch`/`throw` (kapitel 8, som är **dynamiskt**).

Liksom `break`/`return` (kapitel 5) är det en **statisk** utgång, så i kompilerad kod är det en förgrening
till ett basblock som är fast vid kompileringen. Om det finns ett `unwind-protect` däremellan körs dess
`cleanup` (kapitel 8).

Om du aldrig skriver `return-from` kostar det implicita blocket ingenting.

### 5.2 Utökad `loop` (CL:s LOOP)

**Om det första elementet i `loop` är ett keyword** läses den som en följd av satser. Annars förblir det
den enkla slingan ovan, och betydelsen av befintliga `loop` ändras inte (samma som CL:s egen regel för
enkel loop).

CL skriver satsorden som bara symboler (`(loop for i from 1 to 3 collect i)`), men här är **alla
keywords**: ett bart `for` vore bara en variabelreferens, och att vara ett keyword är också det som skiljer
det från en enkel slinga. Undantaget är `=`, som skiljer en variabel från ett värde: dess position är
entydig, så det läses antingen bart eller som ett keyword (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Variabelsatser** (skrivs före kroppssatser. Det är CL:s regel: skrivna efter kunde de läsas som "iterera
bara härifrån och framåt", så det är ett fel):

| Sats | Betydelse |
|---|---|
| `:with v = e` | Binder en gång. Får läsa variablerna från tidigare satser |
| `:for v :in s` / `:for v :across s` | Elementen i en `Iter` i ordning. CL:s skillnad mellan lista/vektor finns inte här, så dessa är två stavningar av samma sats |
| `:for v :on s` | De på varandra följande **suffixen**. CL skickar den delade svans-cons, men en `Iter` har ingen svans att dela, så var och en är en ny `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Räkning. `:downfrom`/`:upfrom` fungerar också |
| `:for v = e [:then f]` | Börjar med `e`, och från andra gången används `f` (utan `:then`, `e` varje gång) |
| `:repeat n` | Itererar så många gånger |

Med flera `:for` stegar de **parallellt**, och slingan slutar så fort någon av dem är uttömd.

**Kroppssatser** (körs varje gång, i den ordning de skrivs):

| Sats | Betydelse |
|---|---|
| `:do form...` | För sidoeffekter |
| `:collect e [:into v]` | Samlar i en `Vector<T>` |
| `:append e [:into v]` | Lägger till innehållet i en `Iter` |
| `:sum e` / `:count e` | Summan / antalet gånger den var sann |
| `:maximize e` / `:minimize e` | Maximum / minimum. **`Option<T>`** (precis som CL returnerar nil för en tom sekvens; en godtycklig `Ord`-typ har inget minsta element) |
| `:always e` / `:never e` | `true` om alla gäller; `false` direkt när en misslyckas |
| `:thereis e` | `e` är ett **`Option<T>`**. Returnerar det första `some`, eller `none` om det inte finns något (det är det som motsvarar CL:s "första värde som inte är nil"; för att testa en `bool`, använd `:always`/`:never`) |
| `:while e` / `:until e` | **Avslutar normalt** här (`:finally` körs, och det som samlats är svaret) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Gör en sats villkorlig |
| `:return e` | Lämnar direkt med det värdet (`:finally` körs inte, som i CL) |
| `:initially form...` / `:finally form...` | Före slingan / vid normalt avslut |

**`:named name`** (före alla andra satser, bara en gång) omsluter hela slingan i `(block name …)`.
`(return-from name e)` kan lämna direkt även inifrån nästlade slingor, och liksom `:return` körs inte
`:finally`. Utan namn upprättas inget block: CL:s onamnade `loop` upprättar `block nil`, men det finns
inget `nil` här, och `break`/`return` (kapitel 5) ger redan "lämna den innersta slingan".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Att utelämna `:finally (return 0)` är ett **typfel**. Det är bara reglerna för `block` i arbete (5.1):
utgångens typ `int` passar inte `()` som slingan lämnar när den är uttömd.

**Slingans värde** är ackumuleringen i den ackumulerande satsen om det finns en (den första, om det finns
flera), `true` för `:always`/`:never`, `none` för `:thereis`, och `()` om det inte finns någon. Om det sista
i `:finally` är `(return e)` är det värdet: CL:s idiom `finally (return …)`, det enda sättet en slinga som
inte ackumulerar kan namnge sitt eget svar.

**Skillnader mot CL / vad som inte ingår**:

- **Satsorden är keywords** (ovan).
- `:maximize`/`:minimize`/`:thereis` returnerar `Option<T>` (det finns inget nil).
- **Att bara skriva `:return`, utan vare sig en ackumulering eller `:finally`, är ett fel.** CL returnerar
  nil när den är uttömd, men det finns inget sådant här, så slingan måste säga vad dess värde är när den
  är uttömd.
- Att foga ihop parallella satser med `:and`, `:being`/dedikerad iteration över hashtabeller, `:it` och
  `:nconc` ingår inte.
- Elementtypen för `:collect` kommer från typen på det ackumulerade uttrycket. Att försöka samla en typ som
  **inte kan skrivas som ett typnamn**, som en funktionstyp, är ett fel som säger det.

## 6. Funktionsvärden och anrop

```lisp
(lambda (params) RetType body...)   ; skapar ett förstklassigt funktionsvärde (en closure)
(labels ((name (params) RetType body...) ...) body...)   ; lokala funktionsdefinitioner som kan vara ömsesidigt rekursiva
(apply f arg1 ... argN rest-list)   ; anropar f (en variadisk funktion med &rest), och sprider rest-list
```

Namngivna funktioner kan också skickas som värden som de är (som argument till högre ordningens funktioner
och så vidare).

## 7. Övriga specialformer

```lisp
(setq var value ...)                ; CL:s variabeltilldelning. bara en följd av (setf var value). defmacro
(psetq var value ...)               ; parallell tilldelning. utvärderar alla värden först, tilldelar sedan. defmacro
(psetf place value ...)             ; psetq generaliserad till platser (samma expansion). defmacro
(setf place value)                  ; tilldelning till en plats. en plats är ett variabelnamn / var::field /
                                     ; ett anrop av formen (accessor recv key...). giltigt om den
                                     ; statiska typen på recv har en instansmetod som heter
                                     ; set-{accessor} (för get i Vector<T> och HashTable<K,V>
                                     ; motsvarar set som ett undantag; annars set-accessornamn).
                                     ; värdet är det tilldelade värdet (som i CL). så
                                     ; i (if c (setf x 1) ()) har then och else inte matchande typer
(incf place)  (incf place delta)    ; place += delta (delta=1 om utelämnat). resultatet är som med setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 om utelämnat)
(rotatef place1 place2 ... placeN)  ; roterar N platser (ny place1=gammal place2, ...,
                                     ; ny placeN=gammal place1). varje placs delformer utvärderas en gång
(shiftf place1 ... placeN newvalue) ; skiftar värdena för place2..N åt vänster och lägger newvalue i placeN.
                                     ; returvärdet är det gamla värdet av place1
(list e1 e2 ... en)                 ; expanderar till (cons e1 (cons e2 (... ()))). () med noll argument.
                                     ; varje element konverteras implicit till Sexpr (som CL:s cons kan
                                     ; det hålla vilket värde som helst). skalärer (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) omsluts i den matchande Sexpr-varianten, och defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> och liknande går in som de är
                                     ; (utan konverteringskostnad). samma för &rest/format-argument.
(source-file)                       ; namnet på filen den här formen lästes från (sträng). fast som en
                                     ; konstant vid kontrolltillfället. motsvarar CL:s *load-pathname*, men är
                                     ; inte en variabel: modulkroppar körs efter kontrollen, så "läses in just nu"
                                     ; kan inte förlitas på, medan det vid kontrolltillfället alltid är känt.
                                     ; för källor som inte är filer, läsarens namn för dem (<stdin>/<input>)
(quote datum)                       ; samma som 'datum. returnerar det som Sexpr-data utan att utvärdera
(quasiquote template)               ; samma som `template. bäddar in uttryck i mallen med ,/,@
(documentation name)                ; returnerar dokumentationssträngen för name (ett bart namn eller Type::method) som Option<string>
(panic message)                     ; message: string. avslutar onormalt med ett icke återhämtningsbart fel. typ !
(unreachable)                       ; expanderar till (panic "unreachable"). defmacro
(todo)                              ; expanderar till (panic "todo"). defmacro
(as Type expr)                      ; tal-/teckentypkonvertering. konverteringar som kan misslyckas ger panic vid misslyckande
(try-as Type expr)                  ; som as, men returnerar resultatet som Option<Type> (None vid misslyckande)
(print control args...)             ; expanderar formatet och skriver till standard ut (inget radbyte)
(println control args...)           ; samma (med ett radbyte på slutet)
(format dest control args...)       ; CL:s format. returnerar den expanderade strängen
(pprint x)                          ; skriver snyggt. skriver ett radbyte först, som i CL
(pprint-fill x)                     ; fyllande layout
(pprint-linear x)                   ; allt på en rad eller ett element per rad
(pprint-tabular x [colinc])         ; tabelllayout (16 kolumner som standard)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; bygg ett logiskt block själv
```

Familjen `print`/`println`/`format`/`pprint` är specialformer, så deras variadiska argument (ett enda
objekt för `pprint`-familjen) omsluts i `Sexpr` med sina egna typer innan de skickas: det är därför
`(println "~a" my-struct)` bara fungerar. Detaljerna om formatdirektiv och den snygga skrivaren finns i
[Formatdirektiv](functions/format.md) och [Utskrift](functions/printing.md#4-den-snygga-skrivaren-pretty-printer).

`as`/`try-as` hanterar bara det numeriska katalogen och teckenkatalogen (mellan `int`, heltalstyperna med
fast bredd, `f32`/`f64`/`ratio`/`char`). Samma typ är ingen konvertering. **Konverteringar mellan
heltalsbredder (inklusive `int`) och mellan `f32`↔`f64` är riktiga konverteringar**: `as` trunkerar /
avrundar, och `try-as` svarar om det ryms i den bredden (precisionen). `(as int x)` är den exakta
vidgningen från en fast bredd, och `(as i32 n)` trunkeringen från `int`. Heltal → `char` kan misslyckas
utanför intervallet, så `as` ger panic och `try-as` ger `None`. Allt annat (vidgning, och trunkeringen i
`float->int`/`ratio->int`) lyckas alltid. `float->int`/`ratio->int`/`char->int` landar på `int`, och om en
smalare bredd efterfrågas anropas `int->W` efter dem. Detta är socker som expanderar till motsvarande
konverteringsmetoder (`int->char`/`int->int`/`int->W` och så vidare i [Tal](functions/numbers.md)).

`documentation` är, liksom `quote`/`compile`, en specialform som läser `name` utan att utvärdera den, som
en onekvaliserad bar symbol / `::`-sökväg. Till skillnad från CL:s `(documentation 'name 'function)` tar den
inget typargument: den löser upp `name` i ordningen variabel → funktion → typ → trait → makro (samma
prioritet som när en bar identifierare utvärderas som ett uttryck) och returnerar dokumentationssträngen
för den definition som hittas (`(documentation Type::method)` är för metoder). Att inte kunna lösa upp (ingen
definition med det namnet) är ett fel vid kontrolltillfället; en definition som finns men saknar
dokumentationssträng ger `Option::none`. Allt avgörs som en konstant vid kontrolltillfället: ingen
uppslagning sker vid körning. Modulkvalificerade fria namn (`mod::name`, utom `Type::method`) stöds inte.

## 8. Icke-lokala utgångar (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; kör body. om (throw 'tag v) sker någonstans
                                    ; body når blir det v värdet
(throw 'tag value)                  ; lämnar till närmaste dynamiskt omslutande (catch 'tag ...)
(unwind-protect protected cleanup)  ; kör cleanup hur protected än lämnas
```

Till skillnad från `break`/`return` (kapitel 5) är detta en **dynamisk** utgång: `throw` letar inte
lexikalt efter `catch` runt sig, och når ett `catch` med samma tagg över hur många funktionsanrop som helst.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; om det inte hittas, värdet på slutet som vanligt
```

- **Taggar är bara literala symboler** (`'done`). Till skillnad från CL utvärderas de inte.
- **En tagg bär en typ.** Typen avgörs första gången `'tag` används, och varje senare `throw`/`catch` av
  samma symbol kontrolleras mot den. Att använda den med en annan typ är ett typfel.
- Typen på `throw` är `!` (den divergerar). Typen på `(catch 'tag expr)` är föreningen av typen på `expr` och
  typen på taggen.
- Värdet av `unwind-protect` är värdet av `protected`. Värdet av `cleanup` kastas. `cleanup` körs hur
  `protected` än lämnas: förutom normalt färdigställande, `throw` och `panic` körs den också när den lämnas
  med `break`/`return`/`return-from`. En icke-lokal utgång från `cleanup` självt vinner över utgången som
  pågår.
- Nästlade `unwind-protect` körs inifrån och ut. Ett `break` som lämnar en slinga **inuti** `protected` har
  inte lämnat `protected`, så dess `cleanup` körs inte.

CL:s conditions (`define-condition`/`handler-bind`/`invoke-restart`) är inte antagna. De passar dåligt med
statisk typning, så återhämtningsbara misslyckanden uttrycks med `Result` (kapitel 9).

## 9. Policy för felhantering

- Återhämtningsbara misslyckanden: `Result<T,E>` + `match`. Icke återhämtningsbara misslyckanden (buggar,
  brutna invarianter): `panic`.
- Det finns ingen syntax som motsvarar `?`/try. Förgreningar skrivs explicit med `match`.
- Funktions- och specialformsnamn använder inte `!` (destruktiva operationer) eller `?` (predikat) som
  ändelser. Predikat namnges med ändelsen `-p`/`p` (`zerop`, `consp` och så vidare) eller prefixet `is-`
  (`is-some`, `is-ok` och så vidare).

## 10. Kompilering

```lisp
(compile name)                      ; JIT-kompilerar en redan definierad defun/metod till nativ kod
(compile-file src-path out-path)    ; AOT-kompilerar en källfil till en nativ körbar fil (hoppar över det avslutande `(main)`)
(dump path)                         ; skriver den aktuella miljön (typinformation + kompilerade kroppar) till en fil
(disassemble name)                  ; skriver ut vad den definitionen blir (värdmaskinens maskinkod som standard, LLVM IR med true som andra argument)
```

`compile` är en specialform; `name` utvärderas inte och läses som en onekvaliserad bar symbol /
`::`-sökväg (en sträng är ett typfel). Generiska funktioner kan inte vara mål: en kopia för varje typ görs
på varje användningsställe, så ingen enskild kompilerad kropp finns. **Ett namn som inte kan lösas upp är ett
fel vid kontrolltillfället** och förs aldrig över till körning (det finns separata meddelanden för: typen
finns men inte den metoden / varken typen eller funktionen finns / ett bart odefinierat namn). Synlighet
behandlas här som för varje annan referens: "finns men syns inte härifrån" misslyckas vid kontrolltillfället,
precis som "löses inte upp".

Anropade funktioner kompileras också transitivt, så **en funktion som (även indirekt) anropar något som
inte kan kompileras kan inte kompileras**. Processen kraschar inte; den avvisas med ett fel som säger det.
Varje inbyggd funktion kan kompileras, så de enda funktioner som avvisas på det här sättet är de som anropar
följande operationer som bara finns i tolken:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

De som bara finns i tolken är `compile`/`compile-file`/`dump` och `trace`/`untrace`/`step`/`disassemble`
([Implementationsverktyg](functions/system.md#5-implementationsverktyg-clhs-252)). I stället för att vara
saker som inte kan kompileras är dessa operationer på den kompilerande sidan (det `dump` skriver ut är
själva tolkens miljö, som en AOT-körbar fil inte har; det `trace` bevakar och var `step` stannar är
anropsvägarna i den tolk som körs; och `disassemble` använder själva kompilatorn).
`room`/`dribble`/`ed` ingår inte bland dem och kan kompileras normalt.

Det som **kan** kompileras: ström- och fil-I/O, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, de transcendenta
funktionerna, bitoperationer, `catch`/`throw`/`unwind-protect`, alla fyra av `eq`/`eql`/`equal`/`equalp`
(vilket låter `case` kompileras för varje typ), hela utskriftsfamiljen inklusive `print`/`println`/
`format`/`pprint` och `pprint-logical-block`, `read` och `eval`. Standardbiblioteket levereras redan
kompilerat.

En AOT-körbar fil innehåller bara de funktioner programmet använder. Ett program som inte skriver ut får
ingen formatmotor, ett som inte anropar `read` får ingen läsare, och ett som inte anropar `eval` får ingen
kontroll och ingen tolk.

Från kommandoraden gör `typl -c src-path [-o out-path]` (`-c` kan också skrivas `--compile`) samma sak som
`compile-file`. Utan `-o` är utdata `src-path` med filändelsen `.typl` borttagen. Som standard är det
statiska biblioteket `libtypelisp_front.a` som länkas in i körbara filer, för ett release-bygge av `typl`,
det som `typl` bär inuti sig självt, utskrivet vid den första länkningen till
`$TYPELISP_HOME/lib/<bygg-ID>/` (eller `~/.typelisp/lib/<bygg-ID>/` utan `TYPELISP_HOME`) och använt
därifrån; för ett debug-bygge det där `typl` byggdes. `typl --remove-lib` tar bort det som den `typl`
skrev ut. Med `--others` tar den bort de med andra bygg-ID; med `--all` de för varje bygg-ID. Med
`typl --lib-dir DIR` används det i `DIR` (för både `-c` och `compile-file`), och om det inte finns där är
det ett fel vid start.

### 10.1 Dumpar

```lisp
(dump "session.typld")     ; skriv ut en
```
```sh
typl --image session.typld prog.typl   # starta från den
typl --image session.typld             # REPL också
```

En dump håller typinformation och kompilerade kroppar i en fil. Det `(dump path)` skriver är det den
aktuella sessionen läste in (standardbiblioteket, eller en dump som skickats med `--image`) plus **det
sessionen själv definierade**. Så utdata är fristående, och `typl --image` startar upp samma miljö. Det
sessionen har gjort `(compile f)` på skrivs i kompilerad form.

Det som sparas är **definitioner, inte historik**:

- Sessionens toppnivåuttryck (`(println ...)` och så vidare) ingår inte. Det vore ett problem om
  inläsningen körde om dem.
- Globala variabler kommer tillbaka med **värdet av att deras initierare körs igen**, inte värdet vid
  dumptillfället. Det är en medveten skillnad mot SBCL:s `save-lisp-and-die` (som skriver ut heapen som
  den är), och det här valet får en hel familj av problem att försvinna: "värden som inte kan sparas", som
  öppna strömmar, funktionspekare för closures och externt minne.
- Till skillnad från `save-lisp-and-die` **dör inte processen**, eftersom skrivandet inte skadar avbilden.

En dump registrerar versionerna av standardbiblioteket och kompilatorn i den implementation som skrev
den. Att läsa in den med en `typl` av en annan version är ett fel; den accepteras aldrig tyst.

### 10.2 `eval` i AOT-körbara filer

`eval` typkontrollerar mot "den aktuella globala miljön" och utvärderar sedan
([Tolkning och utvärdering](functions/system.md#6-tolkning-och-utvärdering)). Den miljön (tabellerna över
signaturer, typer och makron som kontrollen konsulterar, och de kroppar tolken kan köra) finns **inte i
maskinkoden**. En kompilerad funktion är inget annat än en symbol placerad på en adress; den har varken sina
argumenttyper eller en tabell för att slå upp kroppar efter namn.

Så, bara för program som anropar `eval`, **bygger `compile-file` den miljön vid kompilering och skriver in
den i den körbara filen**. Formatet är samma som en dump, och innehåller standardbibliotekets del och
programmets egen del. Allt som sker vid start är att återställa den: källan läses inte om, och inget
typkontrolleras igen. Ingenting läggs till i program som inte anropar `eval`.

Följder:

- **Starten tar längre tid och den körbara filen blir större**, eftersom koden för kontrollen och tolken
  samt en ögonblicksbild av miljön går in. Heapen görs också något större.
- **Former som skickas till eval tolkas.** Även när formen som skickas till eval anropar programmets egna
  funktioner är det den tolkningsbara kroppen ögonblicksbilden håller som körs. Resultatet är detsamma;
  bara hastigheten skiljer sig.

Lagringen av globala variabler är **delad** med kompilerad kod (samma platser). En `defvar`-initierare
körs en gång av den kompilerade initieringen, och återställningen hoppar över den, så en initierare med
sidoeffekter körs inte två gånger.

`compile-file` läser också standardbiblioteket (och bäddar in dess kroppar i den körbara filen), så
standardbiblioteksfunktioner som `abs`/`gcd`, och `(impl print-object ...)` såväl som
`(defmethod print-object ...)`, kan användas med AOT.

`compile-file` accepterar också `use` (och `import`/`shadowing-import`). Ingångsfilens `(use m)` hittar
filer enligt samma regler som `typl file.typl`, och de beroendefiler som hittas kompileras och länkas också
in i den körbara filen: en uppläggning där `main.typl` läser `http.typl` genom `(use http)` kan
AOT-kompileras som den är. Ingångsfilens egna definitioner hamnar också i modulen uppkallad efter filen,
som med `typl file.typl` (`point` i `p.typl` är `p::point`). Så värdens utskriftsform
(`#<p::point x: 1 y: 2>`) är densamma oavsett hur det körs.

## 11. Läsarmakron (readtable)

Det läsaren **gör när den möter ett visst tecken** kan ersättas från programmet (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f läser tecknet c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f läser teckenföljden d s med två tecken
(get-dispatch-macro-character d s)    ; Option<f>
```

Typen på `f` är `(fn (string-input-stream char) Option<Sexpr>)`. Det första argumentet är **en ström över
den text som ännu inte lästs**, och det andra är **tecknet som utlöste det** (det andra tecknet för en
dispatch). Returvärdet blir den data som lästes på det stället. Strömmen är en konkret typ i stället för
`:dyn PeekInput` eftersom läsaren alltid skickar just det här ena slaget: `read-sexpr` / `read-char` /
`peek-char` / `unread-char` / `read-delimited-list` tar alla `(where (PeekInput S))`, så alla fungerar på
den konkreta typen som den är.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => läses som (not (equal 1 2)), det vill säga true
```

Läsaren **tittar på makrotecken före den inbyggda syntaxen**, så den kan ta över även `(` och `'`.
Undertecken till `#` som registreras på det här sättet har företräde framför de inbyggda `#b`/`#x`/`#.`. Ett
annat tecken än `#` blir ett dispatch-tecken på stället när det skickas till
`set-dispatch-macro-character`: det finns **ingen** motsvarighet till CL:s `make-dispatch-macro-character`.
Registreringen gör redan sitt jobb, så ett separat steg skulle inte ha något att göra.

**När de får effekt** beror på läsvägen, samma som för `#.` (kapitel 1):

- REPL och `(load ...)` kör en form i taget, så **funktioner som definierats i tidigare former** kan
  registreras som de är.
- Modulfiler kontrolleras som en enhet och körs senare, så **bara anropen till `set-macro-character` /
  `set-dispatch-macro-character` körs omedelbart** (rollen för CL:s `(eval-when (:compile-toplevel) ...)`).
  Eftersom de körs omedelbart **måste funktionen som skickas redan finnas vid den punkten**. En `defun` i
  samma fil har inte körts än, så skriv en `lambda`, eller använd standardbiblioteket eller något som
  redan har körts. Bara anrop på toppnivå omfattas; den tittar inte inuti `progn` eller `let`.

De inbyggda `read` / `read-from-string` konsulterar också readtable (som i CL).

**Vad som inte finns**: `*readtable*` och `copy-readtable`, samt `readtable-case`. De två första eftersom
en readtable **inte är ett värde**: ett värde skulle behöva vara "något som kan lämnas till en läsare", men
läsaren som läser källkoden ligger utanför programmet, utan någonstans att lämna den till. `readtable-case`
eftersom kapitel 1 avgör att det här språkets läsare alltid gör om till gemener (CL:s `:downcase`).


## 12. Samtidighet (tasks)

**En task är en lättviktstråd** (i Go-termer, det en `go`-sats startar) och körs kooperativt (det finns
ingen preemption). Växlingen går inte genom kärnan, och körtillståndet ligger på heapen i stället för på en
maskinstack, så tasks är billiga att skapa i stort antal.

**Tasks körs samtidigt på flera OS-trådar** (flerkärnig parallellism). Antalet trådar är miljövariabeln
`TYPELISP_THREADS` (totalen, inklusive tråden som kör `main`; standard är maskinens parallellism). I `typl`
körs **bara kompilerade tasks** på andra trådar, och tolkade tasks körs på tolkens tråd (12.7). Delad data
går genom `Mutex<T>` eller `Chan<T>`; samtidiga läsningar och skrivningar som inte gör det är odefinierade,
som i Go (12.7).

Av vokabulären är **bara `task` / `thread` / `select` specialformer**; resten är vanliga funktioner,
metoder och makron ([Tasks och kanaler](functions/concurrency.md)).

### 12.1 `task` — starta en task

```lisp
(task (f arg...))                   ; returnerar Task<T>, där T är returtypen för f
```

**Den tar bara formen av ett anrop.** `f` och varje `arg` utvärderas där `task` är skrivet, i den ordning de
skrivs, och bara **själva anropet** sker i den nya tasken. Det är samma regel som Gos `go f(x)`, och det är
också därför den tar en anropsform i stället för en thunk: en thunk skulle fånga sina argument utan att
utvärdera dem.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i utvärderas på stället varje gång; ingen fångstfälla

(task ((lambda () ()                ; för att köra en godtycklig kropp, anropa en lambda
         (println "start")
         (send ch 1))))
```

Specialformer (`if` / `let` / `progn` …) kan inte skrivas direkt under `task`.

**Varför det inte kan vara en funktion**: att skriva `(spawn (lambda () T body...))` skulle kräva att `T`
stavas ut, eftersom `lambda` kräver en returtypsannotering, och ett makro känner inte till returtypen för
`(f a b)`. Bara kontrollen känner till den.

### 12.2 `thread` — starta en task på en dedikerad OS-tråd

```lisp
(thread (f arg...))                 ; returnerar Thread<T>, där T är returtypen för f
(join th)                           ; väntar på färdigställande och returnerar dess värde (hur många gånger som helst)
```

Formen och utvärderingsreglerna är desamma som för `task` (den tar bara en anropsform, och `f` och `arg`
utvärderas där den skrivs). Skillnaden är var den körs: **den startar en OS-tråd dedikerad åt den tasken och
körs bara på den**. Den multiplexas inte med andra tasks, så att anropa en blockerande C-funktion
(`defffi`) inuti stoppar bara den tråden, och andra tasks gör framsteg. Inuti den kan `task`, `send`, `recv`
och resten användas som de är.

- `Thread<T>` är motsvarigheten till `Task<T>`. Liksom `wait` stoppar `join` **den anropande tasken**, och
  värdet cachas. När tasken tar slut tar tråden också slut.
- Reglerna för panic är desamma som för `task` (hela processen går ner). När `main` returnerar tar
  processen slut.
- För att skriva det som en funktion används `(Thread::spawn (lambda () T body...))` (Rusts
  `std::thread::spawn`). En namngiven funktion får också skickas.
- **Bara kompilerad kod körs på en dedikerad tråd.** När `typl` utvärderar `(thread (f ...))` eller
  `Thread::spawn` under tolkning kompilerar den funktionen som ska köras (och det den anropar) på stället
  innan den körs. Det som inte kan kompileras (en `lambda` som refererar till lokala variabler utanför sig,
  att konstruera en struct och så vidare) är, innan tråden startas, en panic som behandlas på samma sätt som
  en `(panic ...)`. En `lambda` som refererar till lokala variabler kan skickas om den skapas inuti en
  kompilerad funktion.

### 12.3 `select` — vänta på flera kanaloperationer samtidigt

```lisp
(select
  ((v (recv ch1)) body...)          ; en mottagningsgren. v binds till en Option<T>
  ((send ch2 x) body...)            ; en sändningsgren
  (else body...))                   ; valfritt. **om det skrivs kommer det sist**
```

- **Med `else` blockerar den inte** (Gos `default`). Utan det väntar den tills en blir möjlig.
- **Om flera är möjliga samtidigt väljs en slumpmässigt** (i skriven ordning skulle senare grenar svälta).
- `v` i en mottagningsgren är ett **`Option<T>`**. En stängd kanal är "ett svar", inte ett skäl att hoppa
  över grenen, så gör `match` på den inuti grenen.
- Typen är **föreningen av typerna för alla grenars kroppar** (samma regel som för `match`-grenar).
- `(select)` med noll grenar är ett typfel (Gos `select{}`, som blockerar för alltid, är inte antaget). En
  `select` med bara `else` är det också, eftersom det är samma som att skriva kroppen direkt.

**Kanaluttrycken och värdena som ska skickas utvärderas en gång vardera, från vänster till höger, oavsett
vilken gren som väljs** (samma disciplin som `case` har för sina nycklar).

```lisp
(select                             ; ta emot med en timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([en kanal som levererar efter en tid](functions/concurrency.md#5-after--en-kanal-som-levererar-efter-en-tid))
är "en kanal som levererar ett värde efter `sec` sekunder", och motsvarar Gos `time.After`.

### 12.4 Samspel med andra funktioner

| Funktion | Hur den förhåller sig till tasks |
|---|---|
| `catch` / `throw` | **Korsar inte taskgränser.** Ett `throw` som försöker lämna en tasks kropp är en panic |
| `unwind-protect` | Uppstädningen körs när en task tar slut naturligt. **Den körs inte när processen tar slut för att huvudtasken tog slut** |
| `block` / `return-from` | Lexikala, så de korsar inte `lambda`-gränser |
| `panic` | Som i Go går hela processen ner. `wait` observerar inte en panic som ett värde |
| `dlet` | **Inte en bindning per task.** Det "lånar och lämnar tillbaka en global" ändå, så tasks stör varandra |
| Standard ut | Delas av alla tasks. Utdata från en `println` blandas aldrig med andras mitt i en rad |
| `compile` / `eval` | Inga begränsningar. `(compile f)` inuti en task fungerar |

### 12.5 Var tasks byter

Schemaläggningen är kooperativ, så **tasks byter bara där du skriver det**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **kanaloperationer som måste vänta** (`send`/`recv`/`select`) och **socketoperationer som
måste vänta** (`accept` / `tcp-connect` (inklusive namnuppslagning) / läsning och skrivning av socketar /
`recv-from`; [Nätverk](functions/network.md)). Alla socketar är icke-blockerande: om en inte är redo
stoppar bara den tasken, och den återupptas när OS säger att den är redo, samma form som Gos netpoller.
Först när ingen task kan köra väntar implementationen på OS till närmaste `sleep`-tidsgräns.

Kanaloperationer som kan svara på stället (ett `send` med plats i bufferten, ett `recv` med ett värde som
väntar, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **förbrukar inte turen**. Det innebär att du inte
avbryts oväntat av en läsning, och det behandlas annorlunda än `(sleep 0.0)`, som är CL:s "yield i 0
sekunder".

**Det finns ingen preemption.** En tät slinga som inte anropar något svälter andra tasks. Kompilerade
slingor lämnar dock periodiskt över kontrollen till schemaläggaren, så en kompilerad tät slinga svälter dem
inte.

### 12.6 Kompilerad kod och tasks

Kompilerad kod kan också avbryta tasks. Detsamma gäller körbara filer gjorda med `compile-file`: `main` körs
som schemaläggarens huvudtask, och `task`, `sleep`, `wait`, kanaler och socketväntan fungerar alla med
samma betydelse som i `typl`. När `main` returnerar tar processen slut och de återstående tasks avbryts
(som i Go). Tolken läggs aldrig in i den körbara filen för schemaläggarens skull.

Det enda undantaget är "inuti en C FFI-callback", där operationer som **skulle behöva vänta** är fel
(vänligare än att tyst hamna i baklås): medan en funktion som skickats med `defffi` anropas från C ligger
C:s stack överst, och det finns inget sätt att avbryta tasken och återuppta den senare.

Följande platser är också funktioner som anropas mitt i en task, men kan inte avbryta:
`print-object`-metoder, `~/name/` i `format`, läsarmakron, inuti `eval` och `defvar`-initierare i
AOT-körbara filer. Här **går operationer som svarar utan att vänta igenom** (`(recv ch)` med ett värde i
bufferten, `read-line` på en socket med data som redan tagits emot, `(task ...)`, `(yield)` och så vidare),
och **operationer som verkligen skulle behöva vänta är fel** (inte att processen stoppas på stället, utan en
panic som `` `recv` cannot block: ... ``, behandlad på samma sätt som en `(panic ...)`).

### 12.7 Skillnader mot Go

- **I `typl` går bara kompilerade tasks ut på andra trådar.** Tolkens tillstånd kan inte delas mellan
  trådar, så tasks från en tolkad `task` körs på tolkens tråd. En kompilerad task **flyttas också till
  tolkens tråd och stannar där** (den går inte tillbaka) vid den punkt där den anropar ett tolkat
  funktionsvärde, anropar en `:dyn`-metod som ingen har kompilerat, eller anropar
  `eval`/`macroexpand`/`read`. Om en lång beräkning rör tolkad kod ens en gång på vägen körs resten på
  tolkens tråd.
- **I `typl` lever arbetarna bara under en toppnivåutvärdering.** Medan REPL väntar på indata, och mellan
  toppnivåformer, för andra trådar inte tasks framåt (återstående tasks fortsätter från där de slutade i
  nästa utvärdering). Vid slutet av en utvärdering väntar den på att varje tråd ska bli klar med sitt
  aktuella steg, så om en C-funktion (`defffi`) fortsätter blockera inuti en `thread` tar utvärderingen inte
  slut förrän den returnerar.
- **Utskrift på arbetare**: tolkade `print-object`- / `~/name/`-metoder kan inte köras på andra trådar, så
  att skriva ut sådana värden på en annan tråd är en panic som behandlas på samma sätt som en `(panic ...)`
  (`(compile T::print-object)`, eller skriv ut från huvudtasken).
- **Dataracer är odefinierade** (samma ståndpunkt som Go). Resultatet av att flera tasks ändrar samma värde
  utan att gå genom `Mutex<T>` / `Chan<T>` är inte garanterat.
- **`task` returnerar ett värde.** Till skillnad från Gos `go`-sats returnerar den en `Task<T>`, och
  `(wait t)` får resultatet.
- **Det finns inga nil-kanaler.** Gos fan-in-idiom (att sätta en stängd kanal till `nil` för att ta bort den
  från grenarna i `select`) kan inte skrivas, så starta en task per indata och slå ihop dem med en
  `WaitGroup` ([WaitGroup](functions/concurrency.md#4-waitgroup--vänta-på-n-färdigställanden)). Det är också
  det rekommenderade sättet i Go, men det är **den första skillnaden folk som kommer från Go stöter på**.
