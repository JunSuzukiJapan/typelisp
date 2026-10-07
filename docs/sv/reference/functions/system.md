<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tid, miljö och implementation

Funktioner för tid, frågor om körmiljön, implementationsverktyg, tolkning och utvärdering av text,
dokumentationssträngar och makron.

## 1. Tid

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Två fält: `day` (dagar sedan 1900-01-01) och `second` (sekunden inom den dagen, 0..86399) |
| `internal-time` | — | `defstruct` | Två fält: `second` och `microsecond` (inom den sekunden, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Tiden sedan CL:s epok (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Förfluten tid relativt processen |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | Den **CPU-tid** den här processen har använt (användare plus system) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Som ett antal sekunder. Formen för att rapportera skillnaden mellan två avläsningar |
| `internal-time-units-per-second` | — | `int` | `1000000` (mikrosekunder), enheten för fältet `microsecond`. Som i CL är värdet implementationens val |
| `time` | `(time form)` | Makro | Kör `form`, skriver ut verklig tid och CPU-tid på en rad vardera, och returnerar värdet av `form` som det är |

Verklig tid och CPU-tid säger olika saker. För arbete som mest väntar på I/O skiljer sig de två mycket,
och den skillnaden är precis vad du vill veta, så `time` visar båda.

`sleep`, som stoppar en task, finns i [Tasks och kanaler](concurrency.md#3-yield--sleep--vika-sig).

## 2. Avkoda och koda datum

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Nio fält**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. CL:s nio returvärden som en struct (det finns inga flera värden) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universal time till kalenderkomponenter. `zone` är timmar väster om Greenwich (samma riktning som CL). **Utelämnad är det lokal tid** (som i CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Det omvända. Utan `zone` läses argumenten som **lokal tid** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Nu, avkodad i lokal tid |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Den lokala tidens avvikelse väster om Greenwich, i **sekunder**, vid den universella tiden |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Om sommartid gällde vid den universella tiden |

Som i CL är för `day-of-week` **0 måndag och 6 söndag**.

**Utan `zone` används lokal tid**, som i CL. Den lokala avvikelsen frågas av OS, så resultatet beror på var
maskinen står. **Att ange en explicit zon gör det deterministiskt**, och `0` är UTC.

Enheten för `zone` är, som i CL, "timmar väster om Greenwich", så UTC+9 läses som `-9`. **Argumentet är
dock ett heltal och fältet `zone` i resultatet är en `f64`.** Verkliga avvikelser är inte alltid hela
timmar (Indien är +5:30, Nepal +5:45), och att avrunda det rapporterade värdet skulle tyst ljuga. En zon du
skriver för hand är ett helt antal timmar, så argumentet är `int`.

När `zone` anges är `daylight-p` `false` och `zone` exakt det angivna värdet, som CL specificerar
(*If a time-zone is supplied, daylight saving time information is ignored*).

En lokal tid som faller inom en övergång till eller från sommartid är från början inte unik, och CL säger
inte vilken som ska tas. `encode-universal-time` returnerar ett av de två svaren för en sådan tid.

## 3. Körmiljön

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Kommandoraden. **Element 0 är programnamnet** |
| `getenv` | `(getenv name)` | `string→Option<string>` | En miljövariabel. `none` om den inte är satt eller inte är UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. Grunden för `user-homedir-pathname` ([Pathnames](streams-files.md#92-funktioner)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Implementationens version |
| `machine-type` | `(machine-type)` | `()→string` | CPU-arkitekturen (`x86_64` / `aarch64` …). Värdet för **bygg-målet** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Värdnamnet |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Namnet på den hårdvara som **körs nu** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` där det inte kan avgöras |
| `software-type` | `(software-type)` | `()→string` | OS (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | OS-utgåvan (`uname -r`, till exempel `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Ett kort namn på installationsplatsen. **Alltid `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | På samma sätt ett långt namn. **Alltid `none`** |

De som returnerar `Option` är poster där CL tillåter `NIL` (*or nil if no such name can be determined*).
POSIX har ingen plats att registrera platsnamn, så de är alltid `none`; SBCL returnerar detsamma. Notera
skillnaden mellan `machine-type` och `machine-version`: det förra är den arkitektur den här binären
**byggdes** för, det senare är det chip som **kör** den nu.

Element 0 i `command-line-args` är skriptets sökväg för `typl script.typl a b`, och själva den körbara
filen för en AOT-körbar fil som körs som `./prog a b`. **Båda sätten att köra läser samma argument på samma
index** (`typl` tar bort sitt eget namn och flaggor som `--heap-cells` innan de skickas vidare).

## 4. Fråga användaren

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Tar ett enda `y` / `n`. Frågar igen tills den får ett |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Låter användaren skriva ut `yes` / `no`. För frågor där ett misstag är kostsamt |

Båda läser från `*standard-input*`. Bara slutet av indata stoppar omfrågandet, och då är resultatet `false`.

## 5. Implementationsverktyg (CLHS 25.2)

Lagret där implementationen svarar på frågor om sig själv. `heap-info` / `room` / `dribble` är vanliga
funktioner; `trace` / `untrace` / `step` / `disassemble` / `ed` är **specialformer** (`trace` / `untrace` /
`disassemble` / `ed` tar *namnet* på en definition, och `step` en *form*, alla onekvaliserade).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Heapens aktuella tillstånd som en struct. Samma siffror som `room` skriver ut |
| `room` | `(room &optional verbose)` | `(bool)→()` | Rapporterar `heap-info` till `*standard-output*`. `(room true)` ger mer detaljer |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Börjar spela in sessionens utdata till `path` / slutar spela in när den anropas utan argument |
| `trace` | `(trace name...)` | `Sexpr` | Rapporterar anrop av de namngivna definitionerna till `*trace-output*`. Returnerar listan över namn som spåras nu |
| `untrace` | `(untrace name...)` | `Sexpr` | Slutar rapportera. **Utan argument tas allt bort** |
| `step` | `(step form)` | Typen på `form` | Utvärderar `form` och stannar vid varje anrop för att fråga |
| `disassemble` | `(disassemble name [llvm])` | `()` | Skriver ut vad den definitionen blir. Värdmaskinens maskinkod som standard, LLVM IR med `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Startar `$VISUAL` / `$EDITOR`. Med ett namn öppnas raden där den definitionen är skriven |

`trace`/`untrace`/`step`/`disassemble` är bara för tolken, och funktioner som anropar dem kan inte
kompileras ([Syntaxreferens kapitel 10](../syntax.md#10-kompilering)).

### 5.1 Fält i `heap-info`

| Fält | Typ | Innehåll |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Hela cons-arenan och dess uppdelning. Alltid `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | De aktuella antalen av heapens tre andra slags objekt |
| `gc-count` | `int` | Antalet insamlingar sedan implementationen startade |
| `growable` | `bool` | Om arenan fortfarande kan växa |

Fälten är alla `int` (utom `growable`). Tillväxtgränsen (se beskrivningen av `typl --heap-cells`)
rapporteras inte, eftersom det en läsare vill veta är om den fortfarande kan växa (`growable`).

### 5.2 Vad `trace` / `step` kan och inte kan se

- **Definitioner med kompilerade kroppar syns också, från anropsställen som tolkas.**
- **Anropsställen *inuti* kompilerad kod syns inte.** Att spåra ett namn som har en kompilerad kropp lägger
  till en rad med en not om det. Samma begränsning som SBCL beskriver för lokala anrop.
- **Anrop genom closure-värden (`funcall`/`apply`) syns inte.** Closures har inga namn.
- **Generiska definitioner omfattas inte.** En kopia för varje typ görs på varje användningsställe, så det
  finns ingen enda kropp att namnge (samma skäl, och samma formulering, som när `compile` vägrar).

Kommandona i `step` är `s` (stega in i det här anropet; en tom rad gör detsamma), `n` (hoppa över det här
anropet), `c` (sluta fråga härifrån) och `q` (avbryt). **Om standard in inte är en terminal utvärderar
`step` bara `form`**: ett urartat beteende som CLHS uttryckligen tillåter, så att skript och tester inte
hänger sig på en prompt som ingen kan svara på.

`$VISUAL` / `$EDITOR` i `ed` delas vid blanktecken, så `EDITOR="code -w"` fungerar. Om ingen av dem är satt
blir resultatet `Err`: den gissar inte `vi`. Radnumret skickas först, i formen `+N`.

`dribble` spelar in alla tre sätt som sessionens utdata lämnar processen: det som
`print`/`println`/`format` skriver, det som skrivs till strömmar kopplade till standard ut, och raderna som
skrivs in i REPL tillsammans med de värden REPL skriver tillbaka.

## 6. Tolkning och utvärdering

Alla dessa hanterar text och data från körning (som programmet självt inte styr), så vid misslyckande
returnerar de `Err` i ett `Result` i stället för att ge panic. Feltyperna är konkreta typer per operation
([Feltyper](option-result.md#3-feltyper-och-traitet-error)).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL:s `parse-integer`. Hoppar över inledande och avslutande blanktecken (samma mängd som `trim`), läser högst ett tecken `+`/`-`, sedan siffror i basen `radix` (standard 10, 2 till 36; siffror över 10 i endera skiftläget). Det finns ingen gräns för antalet siffror (`int`). Alla andra tecken som blir över ger `Err`. Med `:junk-allowed true` stannar den vid den första icke-siffran och ignorerar resten, men ger `Err` om det inte finns en enda siffra (motsvarar CL:s `nil`). Den returnerar inte CL:s andra värde (positionen där läsningen slutade). En `radix` utanför intervallet ger panic (ett fel hos anroparen, inte i texten) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Ett flyttal. Accepterar också `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Läser en `Sexpr` från `s` (med samma läsare som läser källkod). Obalanserade parenteser, oavslutade strängar och liknande ger `Err`. Att läsa från en ström är `read-sexpr` ([Strömmar](streams-files.md#6-generiska-funktioner-och-filoperationer)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **positionen där läsningen slutade**. `(car r)` är värdet och `(cdr r)` positionen för nästa tecken att läsa. `start` är som standard 0 |
| `read-from-string-preserving-whitespace` | Som ovan | Som ovan | Samma, men förbrukar inte det blanktecken som avslutade datumet. Skillnaden syns i den returnerade positionen |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Typkontrollerar `form` vid körning och utvärderar den. Följer CL:s `eval` |

CL returnerar **två värden** (värdet och positionen) från `read-from-string`, men det här språket har inga
flera värden, så det returnerar en `cons-cell`. Att ha positionen gör att läsa en sträng ett datum i taget
blir en slinga i stället för en omskanning:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Skillnaden som `preserving-whitespace` gör är **ett blanktecken**: CL:s `read` förbrukar det blanktecken som
avslutade datumet, och `read-preserving-whitespace` lämnar det. `(read-from-string "12 34")` returnerar
position 3, och den bevarande versionen returnerar 2.

Talsyntaxen som läsaren accepterar finns i [Syntaxreferens kapitel 1](../syntax.md#1-lexikaliska-element). Det
som `*print-radix*` ([Utskrift](printing.md#62-bas-skiftläge-och-läsbarhet)) skriver ut kan läsas tillbaka som
det är. Det finns ingen CL-`*read-base*`.

### 6.1 Vad `eval` innebär

Den följer CLHS `eval`: den utvärderar i **den aktuella globala miljön** (globala funktioner, variabler,
typer och makron, inklusive definitioner som lagts till vid körning) och i **den tomma lexikala miljön**
(de lokala bindningarna i anroparens `let`/`lambda` syns inte). Både uttryck och definitioner
(`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) kan utvärderas, och definitioner registreras i den
globala miljön omedelbart och permanent.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; den globala x syns
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returnerar det definierade namnet
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; definitionen som just gjordes syns
```

- **Returvärde**: för ett uttryck resultatet som en `Option<Sexpr>`; för en definition symbolen för det
  definierade namnet (som i CL). För att använda resultatet plockar man isär `Sexpr` med `match`
  (`(int n)`/`(str s)`/…).
- **Skillnader på grund av statiska typer (viktigt)**: CL returnerar det faktiska värdet av resultatet,
  men i det här språket kan returtypen bara vara enhetligt `Result<Option<Sexpr>,EvalError>`. Dessutom
  **kan kod som skrivs statiskt inte referera framåt till namn som `eval` definierar vid körning**: ett
  `(sq 9)` skrivet direkt i en fil kontrolleras innan den `eval` som definierar `sq` körs, och är
  "odefinierat". **Senare `eval` kan dock se det** (deras typkontroll körs vid körning, efter
  definitionen). REPL kontrollerar och kör en rad i taget, så ett namn som definierats med `eval` kan
  anropas direkt från nästa rad.
- **Fel**: typfel och syntaxfel returnerar `Err` (de ger inte panic). **Panics vid körning** i den
  utvärderade koden (division med noll och så vidare) fortplantar sig som de skulle från kod skriven
  direkt. Uppstädningen i varje `unwind-protect` däremellan körs
  ([Syntaxreferens kapitel 8](../syntax.md#8-icke-lokala-utgångar-catch--throw--unwind-protect)).
- **Namnrymd**: när den körs av `typl file.typl` och inuti en AOT-körbar fil utvärderar `eval` i
  namnrymden för skriptets modul (skriptets egna globaler syns). REPL utvärderar i rotnamnrymden.
- **Kompilering**: både `read` och `eval` kan kompileras. Hur de hanteras i AOT-körbara filer, och
  följderna (former som skickas till eval tolkas), finns i
  [Syntaxreferens 10.2](../syntax.md#102-eval-i-aot-körbara-filer).

## 7. Dokumentationssträngar / `documentation`

`defun`/`defmethod` (även inuti `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` kan bära dokumentationssträngar. Positionen följer CL:s regel för var och en:

| Form | Dokumentationssträngens position |
|---|---|
| `defun` / `defmethod` / `defmacro` | I början av kroppen (efter returtypen och `where`-satsen). Bara när minst en kroppsform följer; en ensam sträng förblir returvärdet |
| `defvar` / `defconstant` | **Efter** startvärdet: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Direkt efter** namnet, före fälten/varianterna |
| `deftype` | **Direkt efter** namnet, före typen: `(deftype meters "doc" i32)` |
| `deftrait` | Direkt efter listan över supertraits, före posterna. En för hela traitet. **Metoder med en standardimplementation** kan sätta sin egen dokumentationssträng direkt före sin kropp |

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `documentation` | `(documentation name)` | (specialform; `name` är en bar symbol eller `Type::method`)→`Option<string>` | Returnerar dokumentationssträngen för `name` |

Liksom `quote`/`compile` är `documentation` en specialform (den läser `name` som ett onekvaliserat namn).
Till skillnad från CL:s `(documentation 'name 'function)` tar den inget typargument; i stället löser den
upp ett bart namn i ordningen **variabel → funktion → typ → trait → makro** (samma prioritet som för en
bar identifierare som utvärderas som ett uttryck). Formen `Type::method` slår upp dokumentationssträngen
för en associerad eller statisk metod.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Värdet avgörs vid kontrolltillfället**: om namnet inte löses upp till någon definition är det ett fel vid
kontrolltillfället (som att referera till en odefinierad variabel). Om det löses upp men det inte finns
någon dokumentationssträng blir resultatet `Option::none`.

**Omfattas inte**:

- `(setf documentation)` (att ändra en dokumentationssträng vid körning) finns inte.
- Modulkvalificerade fria namn (`mod::name`; `Type::method` stöds) stöds inte.
- En metoddeklaration i en `deftrait` **utan kropp** kan inte ha en dokumentationssträng. En avslutande
  strängliteral skulle själv vara kroppen (returvärdet) för en standardimplementation, så det finns inget
  sätt att skilja de två åt.

Språkserverns hovring (`typl-lsp`) visar också dokumentationssträngar.

## 8. Makron

Hur man definierar makron finns i [Syntaxreferens 3.14](../syntax.md#314-defmacro--makrodefinitioner).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | En ny symbol. Dess namn är `" <prefix><n>"`, där `n` är `*gensym-counter*`. Ett inledande mellanslag kan inte skrivas i källkod, så de genererade bindningarna krockar aldrig med skrivna namn |
| `*gensym-counter*` | Variabel | `int` | Numret som `gensym` använder härnäst. Som i CL kan det läsas och sättas |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expanderar ett makroanrop ett steg. `none` betyder "inte ett makroanrop" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Upprepar tills det inte längre är ett makro |

`macroexpand-1` returnerar ett `Option`. CL rapporterar "om det expanderade" som ett andra returvärde, men
det finns inga flera värden, så `none` spelar den rollen. **Ett makro som expanderar till ett anrop av sig
självt kan aldrig förväxlas med ett icke-makro.** Ett expansionssteg är samma som typkontrollen använder, så
det programmet ser och det kontrollen såg divergerar aldrig.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none visas som den tomma listan (Option<Sexpr> är transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Vad CL har och det här språket inte har: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute`
sammanfaller alltid, så det finns ingen skillnad att välja), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (ointernerade symboler; bindningar slås upp efter namn, så det
skulle inte ge något).

## 9. Lokala makrobindningar (`macrolet` / `symbol-macrolet`)

Båda är specialformer som binder **namn som inte är värden** lexikalt. Ingenting finns kvar vid körning:
det som kompileras är den expanderade formen av kroppen.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- En `macrolet`-bindning skymmer ett globalt makro med samma namn **bara under kroppen**. Lambdalistan är
  densamma som för `defmacro` (`&optional`/`&rest`/`&key`).
- **Syskon i samma `macrolet` kan inte se varandra från sina *kroppar*** (som i CL; detta är skillnaden mot
  `labels`). Expansioner kontrolleras på användningsstället, så att `earlier` expanderar till
  `(later ...)` fungerar: båda syns på det stället.
- Ett `symbol-macrolet`-namn går in i miljön som en vanlig bindning. Så ett inre `let` skymmer samma namn,
  och en yttre variabel skyms: CL:s regler kommer ut som de är.
- **`setf` skriver till expansionen.** `(setf head 42)` är `(setf (get v 0) 42)`.
- Expansioner kontrolleras i **användningsställets miljö** (inte bindningsställets).

## 10. Övrigt

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Ger panic om falskt. Utan meddelande `assertion failed: <testet som det skrevs>` (det är ett makro, så det kan namnge själva uttrycket). CL:s restarts finns inte i det här språket |
| `warn` | `(warn control args...)` | `(string,...)→()` | Skriver en rad med prefixet `WARNING: ` till `*error-output*` och **fortsätter**. Ett sätt att rapportera något utan att returnera ett `Result` och utan att avsluta programmet |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Ersätter globaler bara under `body` och återställer dem på vägen ut. CL skriver detta som `let`, men `let` i det här språket binder alltid lexikalt, därav det separata namnet (samma roll som Emacs Lisp-makrot med samma namn). Återställer dem hur kroppen än lämnas: normalt färdigställande, `throw`, `panic`, `break`/`return`. **Inte en bindning per task** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Kör `body` med varje utskriftskontrollvariabel på sitt standardvärde och `*read-eval*` satt till `true` ([Utskrift](printing.md#6-styra-hur-mycket-som-skrivs-ut)) |
| `exit` | `(exit code)` | `int→!` | Avslutar processen |
| `dump` | `(dump path)` | `string→bool` | Skriver den aktuella miljön (typinformation plus kompilerade kroppar) till en fil. `typl --image <path>` startar om från den. Bara för tolken ([Syntaxreferens 10.1](../syntax.md#101-dumpar)) |
