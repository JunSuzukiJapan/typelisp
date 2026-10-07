<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tijd, omgeving en implementatie

Functies voor tijd, vragen over de runtime-omgeving, implementatiehulpmiddelen, het parsen en
evalueren van tekst, docstrings en macro's.

## 1. Tijd

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Twee velden: `day` (dagen sinds 1900-01-01) en `second` (de seconde binnen die dag, 0..86399) |
| `internal-time` | — | `defstruct` | Twee velden: `second` en `microsecond` (binnen die seconde, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | De tijd sinds de epoch van CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Verstreken tijd ten opzichte van het proces |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | De **CPU-tijd** die dit proces heeft gebruikt (gebruiker plus systeem) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Als aantal seconden. De vorm om het verschil tussen twee metingen te rapporteren |
| `internal-time-units-per-second` | — | `int` | `1000000` (microseconden), de eenheid van het veld `microsecond`. Zoals in CL is de waarde de keuze van de implementatie |
| `time` | `(time form)` | Macro | Voert `form` uit, drukt de werkelijke tijd en de CPU-tijd af op één regel elk, en geeft de waarde van `form` ongewijzigd terug |

Werkelijke tijd en CPU-tijd vertellen je verschillende dingen. Bij werk dat vooral op I/O wacht
verschillen de twee sterk, en dat verschil is precies wat je wilt weten, dus `time` toont beide.

`sleep`, dat een taak stopt, staat in [Taken en kanalen](concurrency.md#3-yield--sleep--voorrang-geven).

## 2. Datums decoderen en coderen

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Negen velden**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. De negen returnwaarden van CL als één struct (er zijn geen meervoudige waarden) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universele tijd naar kalendercomponenten. `zone` is uren ten westen van Greenwich (dezelfde richting als CL). **Weggelaten is het lokale tijd** (zoals in CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Het omgekeerde. Zonder `zone` worden de argumenten als **lokale tijd** gelezen |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Nu, gedecodeerd in lokale tijd |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | De afwijking van de lokale tijd ten westen van Greenwich, in **seconden**, op die universele tijd |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Of zomertijd van kracht was op die universele tijd |

Zoals in CL is voor `day-of-week` **0 maandag en 6 zondag**.

**Zonder `zone` wordt lokale tijd gebruikt**, zoals in CL. De lokale afwijking wordt aan het OS gevraagd,
dus het resultaat hangt af van waar de machine staat. **Een expliciete zone opgeven maakt het
deterministisch**, en `0` is UTC.

De eenheid van `zone` is, zoals in CL, "uren ten westen van Greenwich", dus UTC+9 wordt gelezen als
`-9`. **Het argument is echter een geheel getal en het veld `zone` van het resultaat is een `f64`.**
Werkelijke afwijkingen zijn niet altijd hele uren (India is +5:30, Nepal +5:45), en de gerapporteerde
waarde afronden zou stilzwijgend liegen. Een zone die je met de hand schrijft is een geheel aantal uren,
dus het argument is `int`.

Wanneer `zone` is gegeven, is `daylight-p` `false` en is `zone` precies de gegeven waarde, zoals CL
voorschrijft (*If a time-zone is supplied, daylight saving time information is ignored*).

Een lokale tijd die binnen een zomertijdovergang valt is om te beginnen niet uniek, en CL zegt niet welke
te nemen. `encode-universal-time` geeft voor zo'n tijd een van de twee antwoorden terug.

## 3. De runtime-omgeving

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | De opdrachtregel. **Element 0 is de programmanaam** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Een omgevingsvariabele. `none` als niet gezet of geen UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. De basis van `user-homedir-pathname` ([Padnamen](streams-files.md#92-functies)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | De versie van de implementatie |
| `machine-type` | `(machine-type)` | `()→string` | De CPU-architectuur (`x86_64` / `aarch64` …). De waarde van het **bouwdoel** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | De hostnaam |
| `machine-version` | `(machine-version)` | `()→Option<string>` | De naam van de hardware die **nu draait** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` waar het niet kan worden bepaald |
| `software-type` | `(software-type)` | `()→string` | Het OS (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | De OS-release (`uname -r`, bijvoorbeeld `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Een korte naam voor de installatieplaats. **Altijd `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Idem, een lange naam. **Altijd `none`** |

Degene die `Option` teruggeven zijn onderdelen waarvoor CL `NIL` toestaat (*or nil if no such name can be
determined*). POSIX heeft geen plek om sitenamen vast te leggen, dus ze zijn altijd `none`; SBCL geeft
hetzelfde terug. Let op het verschil tussen `machine-type` en `machine-version`: de eerste is de
architectuur waarvoor deze binary is **gebouwd**, de tweede is de chip waarop hij nu **draait**.

Element 0 van `command-line-args` is het pad van het script bij `typl script.typl a b`, en het
uitvoerbare bestand zelf bij een AOT-uitvoerbaar bestand dat als `./prog a b` wordt uitgevoerd.
**Beide manieren van uitvoeren lezen dezelfde argumenten op dezelfde indexen** (`typl` verwijdert zijn
eigen naam en opties zoals `--heap-cells` voordat hij ze doorgeeft).

## 4. De gebruiker vragen

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Neemt één `y` / `n`. Vraagt opnieuw totdat hij er een krijgt |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Laat de gebruiker `yes` / `no` voluit schrijven. Voor vragen waarbij een vergissing duur is |

Beide lezen uit `*standard-input*`. Alleen het einde van de invoer stopt het opnieuw vragen, en dan is
het resultaat `false`.

## 5. Implementatiehulpmiddelen (CLHS 25.2)

De laag waarin de implementatie vragen over zichzelf beantwoordt. `heap-info` / `room` / `dribble` zijn
gewone functies; `trace` / `untrace` / `step` / `disassemble` / `ed` zijn **speciale vormen**
(`trace` / `untrace` / `disassemble` / `ed` nemen de *naam* van een definitie, en `step` een *vorm*,
allemaal ongeëvalueerd).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | De huidige toestand van de heap als struct. Dezelfde getallen die `room` afdrukt |
| `room` | `(room &optional verbose)` | `(bool)→()` | Rapporteert `heap-info` aan `*standard-output*`. `(room true)` geeft meer detail |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Begint de uitvoer van de sessie naar `path` op te nemen / stopt met opnemen wanneer zonder argument aangeroepen |
| `trace` | `(trace name...)` | `Sexpr` | Rapporteert aanroepen van de genoemde definities aan `*trace-output*`. Geeft de lijst met namen terug die nu worden getraceerd |
| `untrace` | `(untrace name...)` | `Sexpr` | Stopt met rapporteren. **Zonder argumenten verwijdert het alles** |
| `step` | `(step form)` | Het type van `form` | Evalueert `form` en stopt bij elke aanroep om te vragen |
| `disassemble` | `(disassemble name [llvm])` | `()` | Drukt af wat die definitie wordt. Standaard de machinecode van de host, LLVM IR met `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Start `$VISUAL` / `$EDITOR`. Met een naam opent het de regel waar die definitie is geschreven |

`trace`/`untrace`/`step`/`disassemble` zijn alleen voor de interpreter, en functies die ze aanroepen
kunnen niet worden gecompileerd ([Syntaxreferentie hoofdstuk 10](../syntax.md#10-compilatie)).

### 5.1 Velden van `heap-info`

| Veld | Type | Inhoud |
|---|---|---|
| `capacity` / `live` / `free` | `int` | De hele cons-arena en haar indeling. Altijd `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | De huidige aantallen van de drie andere soorten objecten van de heap |
| `gc-count` | `int` | Het aantal collecties sinds de implementatie startte |
| `growable` | `bool` | Of de arena nog kan groeien |

De velden zijn allemaal `int` (behalve `growable`). De groeilimiet (zie de beschrijving van
`typl --heap-cells`) wordt niet gerapporteerd, omdat een lezer wil weten of hij nog kan groeien
(`growable`).

### 5.2 Wat `trace` / `step` wel en niet kunnen zien

- **Definities met gecompileerde bodies zijn ook zichtbaar, vanaf aanroepplekken die worden
  geïnterpreteerd.**
- **Aanroepplekken *binnen* gecompileerde code zijn niet zichtbaar.** Een naam traceren die een
  gecompileerde body heeft voegt een regel toe die dat zegt. Dezelfde beperking die SBCL voor lokale
  aanroepen beschrijft.
- **Aanroepen via closurewaarden (`funcall`/`apply`) zijn niet zichtbaar.** Closures hebben geen namen.
- **Generieke definities worden niet gedekt.** Op elke plek van gebruik wordt een kopie per type
  gemaakt, dus er is geen enkele body om te noemen (dezelfde reden, en dezelfde formulering, als wanneer
  `compile` weigert).

De commando's van `step` zijn `s` (in deze aanroep stappen; een lege regel doet hetzelfde), `n` (deze
aanroep overslaan), `c` (vanaf hier niet meer vragen) en `q` (afbreken). **Als standaardinvoer geen
terminal is, evalueert `step` gewoon `form`**: een gedegenereerd gedrag dat CLHS expliciet toestaat,
zodat scripts en tests niet blijven hangen op een prompt die niemand kan beantwoorden.

De `$VISUAL` / `$EDITOR` van `ed` wordt bij witruimte gesplitst, dus `EDITOR="code -w"` werkt. Als
geen van beide is gezet, is het resultaat `Err`: hij raadt niet `vi`. Het regelnummer wordt eerst
doorgegeven, in de vorm `+N`.

`dribble` neemt alle drie de manieren op waarop de uitvoer van de sessie het proces verlaat: wat
`print`/`println`/`format` schrijven, wat wordt geschreven naar streams die met standaarduitvoer zijn
verbonden, en de regels die in de REPL worden getypt samen met de waarden die de REPL terugdrukt.

## 6. Parsen en evalueren

Dit alles behandelt tekst en gegevens uit runtime (die het programma zelf niet beheerst), dus bij
mislukking geven ze de `Err` van een `Result` terug in plaats van een panic. De foutentypes zijn concrete
types per bewerking ([Foutentypes](option-result.md#3-foutentypes-en-de-trait-error)).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | `parse-integer` van CL. Slaat voorloop- en volgwitruimte over (dezelfde verzameling als `trim`), leest hoogstens één teken `+`/`-`, dan cijfers in grondtal `radix` (standaard 10, 2 tot en met 36; cijfers boven 10 in beide hoofdlettergebruiken). Er is geen limiet aan het aantal cijfers (`int`). Alle andere overgebleven tekens geven `Err`. Met `:junk-allowed true` stopt het bij het eerste niet-cijfer en negeert de rest, maar geeft `Err` als er niet één cijfer is (komt overeen met `nil` van CL). Het geeft de tweede waarde van CL (de positie waar het lezen eindigde) niet terug. Een `radix` buiten het bereik geeft een panic (een fout van de aanroeper, niet in de tekst) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Een drijvendekommagetal. Accepteert ook `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Leest één `Sexpr` uit `s` (met dezelfde reader die broncode leest). Ongebalanceerde haakjes, niet afgesloten strings en dergelijke geven `Err`. Lezen uit een stream is `read-sexpr` ([Streams](streams-files.md#6-generieke-functies-en-bestandsbewerkingen)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **de positie waar het lezen eindigde**. `(car r)` is de waarde en `(cdr r)` de positie van het volgende te lezen teken. `start` is standaard 0 |
| `read-from-string-preserving-whitespace` | Idem | Idem | Hetzelfde, maar verbruikt de witruimte die het datum beëindigde niet. Het verschil blijkt uit de teruggegeven positie |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Typecheckt `form` tijdens runtime en evalueert hem. Volgt `eval` van CL |

CL geeft **twee waarden** (de waarde en de positie) terug van `read-from-string`, maar deze taal heeft
geen meervoudige waarden, dus ze geeft één `cons-cell` terug. Door de positie te hebben wordt het
datum voor datum lezen van een string een lus in plaats van een herscan:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Het verschil dat `preserving-whitespace` maakt is **één witruimteteken**: `read` van CL verbruikt de
witruimte die het datum beëindigde, en `read-preserving-whitespace` laat hem staan.
`(read-from-string "12 34")` geeft positie 3 terug, en de bewarende versie geeft 2.

De getalsyntaxis die de reader accepteert staat in
[Syntaxreferentie hoofdstuk 1](../syntax.md#1-lexicale-elementen). Wat `*print-radix*`
([Afdrukken](printing.md#62-grondtal-hoofdlettergebruik-en-leesbaarheid)) afdrukt kan zoals het is weer
worden ingelezen. Er is geen `*read-base*` van CL.

### 6.1 Wat `eval` betekent

Het volgt `eval` van CLHS: het evalueert in **de huidige globale omgeving** (globale functies,
variabelen, types en macro's, inclusief definities die tijdens runtime zijn toegevoegd) en in **de lege
lexicale omgeving** (de lokale bindingen van de `let`/`lambda` van de aanroeper zijn niet zichtbaar).
Zowel expressies als definities (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) kunnen worden
geëvalueerd, en definities worden onmiddellijk en permanent in de globale omgeving geregistreerd.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; the global x is visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returns the defined name
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; the definition just made is visible
```

- **Returnwaarde**: voor een expressie het resultaat als `Option<Sexpr>`; voor een definitie het symbool
  van de gedefinieerde naam (zoals in CL). Om het resultaat te gebruiken, haal je de `Sexpr` met `match`
  uit elkaar (`(int n)`/`(str s)`/…).
- **Verschillen door statische types (belangrijk)**: CL geeft de werkelijke waarde van het resultaat
  terug, maar in deze taal kan het returntype alleen uniform `Result<Option<Sexpr>,EvalError>` zijn.
  Bovendien **kan statisch geschreven code niet vooruit verwijzen naar namen die `eval` tijdens runtime
  definieert**: een `(sq 9)` die rechtstreeks in een bestand is geschreven wordt gecontroleerd vóórdat
  de `eval` die `sq` definieert wordt uitgevoerd, en is "ongedefinieerd". **Latere `eval`s kunnen hem
  echter zien** (hun typecontrole draait tijdens runtime, na de definitie). De REPL controleert en
  voert één regel per keer uit, dus een naam die met `eval` is gedefinieerd kan vanaf de volgende regel
  rechtstreeks worden aangeroepen.
- **Fouten**: typefouten en syntaxisfouten geven `Err` terug (ze geven geen panic). **Runtime-panics**
  in de geëvalueerde code (deling door nul enzovoort) planten zich voort zoals bij rechtstreeks
  geschreven code. De opruiming van elke `unwind-protect` daartussen wordt uitgevoerd
  ([Syntaxreferentie hoofdstuk 8](../syntax.md#8-niet-lokale-uitgangen-catch--throw--unwind-protect)).
- **Namespace**: bij uitvoering met `typl file.typl` en binnen een AOT-uitvoerbaar bestand evalueert
  `eval` in de namespace van de module van het script (de eigen globale variabelen van het script zijn
  zichtbaar). De REPL evalueert in de rootnamespace.
- **Compilatie**: zowel `read` als `eval` kunnen worden gecompileerd. Hoe ze in AOT-uitvoerbare
  bestanden worden behandeld, en de gevolgen daarvan (aan eval doorgegeven vormen worden geïnterpreteerd),
  staan in [Syntaxreferentie 10.2](../syntax.md#102-eval-in-aot-uitvoerbare-bestanden).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (ook binnen `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` kunnen docstrings dragen. De positie volgt per vorm de regel van CL:

| Vorm | Positie van de docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | Aan het begin van de body (na het returntype en de `where`-clausule). Alleen wanneer er minstens één bodyvorm volgt; een losse string blijft de returnwaarde |
| `defvar` / `defconstant` | **Na** de beginwaarde: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Direct na** de naam, vóór de velden/varianten |
| `deftype` | **Direct na** de naam, vóór het type: `(deftype meters "doc" i32)` |
| `deftrait` | Direct na de lijst met supertraits, vóór de onderdelen. Eén voor de hele trait. **Methoden met een standaardimplementatie** kunnen hun eigen docstring direct vóór hun body zetten |

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `documentation` | `(documentation name)` | (speciale vorm; `name` is een kaal symbool of `Type::method`)→`Option<string>` | Geeft de docstring van `name` terug |

Net als `quote`/`compile` is `documentation` een speciale vorm (hij leest `name` als ongeëvalueerde
naam). Anders dan `(documentation 'name 'function)` van CL neemt hij geen typeargument; in plaats
daarvan lost hij een kale naam op in de volgorde **variabele → functie → type → trait → macro** (dezelfde
prioriteit als voor een kale identifier die als expressie wordt geëvalueerd). De vorm `Type::method`
zoekt de docstring van een geassocieerde of statische methode op.

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

**De waarde wordt bij het controleren bepaald**: als de naam naar geen enkele definitie oplost, is het
een fout bij het controleren (zoals verwijzen naar een ongedefinieerde variabele). Lost hij op maar is er
geen docstring, dan is het resultaat `Option::none`.

**Niet gedekt**:

- `(setf documentation)` (een docstring tijdens runtime wijzigen) bestaat niet.
- Vrije namen met modulekwalificatie (`mod::name`; `Type::method` wordt ondersteund) worden niet
  ondersteund.
- Een methodedeclaratie in een `deftrait` **zonder body** kan geen docstring hebben. Een afsluitende
  stringliteral zou zelf de body (de returnwaarde) van een standaardimplementatie zijn, dus er is geen
  manier om de twee te onderscheiden.

De hover van de language server (`typl-lsp`) toont ook docstrings.

## 8. Macro's

Hoe je macro's definieert staat in [Syntaxreferentie 3.14](../syntax.md#314-defmacro--macrodefinities).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Een nieuw symbool. De naam is `" <prefix><n>"`, waarbij `n` `*gensym-counter*` is. Een voorloopspatie kan in broncode niet worden geschreven, dus de gegenereerde bindingen botsen nooit met geschreven namen |
| `*gensym-counter*` | Variabele | `int` | Het getal dat `gensym` hierna gebruikt. Zoals in CL kan het worden gelezen en gezet |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expandeert een macro-aanroep één stap. `none` betekent "geen macro-aanroep" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Herhaalt totdat het geen macro meer is |

`macroexpand-1` geeft een `Option` terug. CL meldt "of het expandeerde" als tweede returnwaarde, maar er
zijn geen meervoudige waarden, dus `none` speelt die rol. **Een macro die naar een aanroep van zichzelf
expandeert kan nooit met een niet-macro worden verward.** Eén stap expansie is dezelfde als die de
typechecker gebruikt, dus wat het programma ziet en wat de checker zag lopen nooit uiteen.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none shows as the empty list (Option<Sexpr> is transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Wat CL heeft en deze taal niet: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` vallen altijd
samen, dus er is geen onderscheid om uit te kiezen), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (niet-geïnterneerde symbolen; bindingen worden op naam opgezocht,
dus er zou niets te winnen zijn).

## 9. Lokale macrobindingen (`macrolet` / `symbol-macrolet`)

Beide zijn speciale vormen die **namen die geen waarden zijn** lexicaal binden. Tijdens runtime blijft er
niets over: wat wordt gecompileerd is de geëxpandeerde vorm van de body.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Een `macrolet`-binding verbergt een globale macro met dezelfde naam **alleen tijdens de body**. De
  lambdalijst is dezelfde als bij `defmacro` (`&optional`/`&rest`/`&key`).
- **Broers en zussen van dezelfde `macrolet` kunnen elkaar niet zien vanuit hun *bodies*** (zoals in CL;
  dit is het verschil met `labels`). Expansies worden op de plek van gebruik gecontroleerd, dus
  `earlier` die naar `(later ...)` expandeert werkt: beide zijn op die plek zichtbaar.
- Een `symbol-macrolet`-naam komt als gewone binding in de omgeving. Een binnenste `let` verbergt dus
  dezelfde naam, en een buitenste variabele wordt verborgen: de regels van CL komen er zoals ze zijn uit.
- **`setf` schrijft naar de expansie.** `(setf head 42)` is `(setf (get v 0) 42)`.
- Expansies worden gecontroleerd in **de omgeving van de plek van gebruik** (niet van de plek van
  binding).

## 10. Overig

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Geeft een panic als onwaar. Zonder melding `assertion failed: <de test zoals geschreven>` (het is een macro, dus hij kan de expressie zelf noemen). De restarts van CL bestaan niet in deze taal |
| `warn` | `(warn control args...)` | `(string,...)→()` | Schrijft één regel met het voorvoegsel `WARNING: ` naar `*error-output*` en **gaat door**. Een manier om iets te melden zonder een `Result` terug te geven en zonder het programma te beëindigen |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Vervangt globale variabelen alleen tijdens `body` en herstelt ze bij het verlaten. CL schrijft dit als `let`, maar `let` in deze taal bindt altijd lexicaal, vandaar de aparte naam (dezelfde rol als de Emacs Lisp-macro met dezelfde naam). Herstelt ze hoe de body ook wordt verlaten: normale voltooiing, `throw`, `panic`, `break`/`return`. **Geen binding per taak** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Voert `body` uit met elke besturingsvariabele van de printer op zijn standaardwaarde en `*read-eval*` op `true` ([Afdrukken](printing.md#6-bepalen-hoeveel-wordt-afgedrukt)) |
| `exit` | `(exit code)` | `int→!` | Beëindigt het proces |
| `dump` | `(dump path)` | `string→bool` | Schrijft de huidige omgeving (typeinformatie plus gecompileerde bodies) naar één bestand. `typl --image <path>` start er opnieuw vanuit. Alleen voor de interpreter ([Syntaxreferentie 10.1](../syntax.md#101-dumps)) |
