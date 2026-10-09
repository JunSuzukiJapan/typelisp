<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Streams en bestanden

Streamtraits en -methoden, concrete streamtypes, bestandsbewerkingen en padnamen. Netwerksockets zijn
ook streams en worden behandeld in [Netwerk](network.md).

## 1. Traitstructuur

Wat CL met een klassenhiërarchie uitdrukt, wordt hier uitgedrukt met een **traithiërarchie**. Zowel de
richting (invoer / uitvoer) als het elementtype worden **statisch** bepaald, dus het is niet nodig om
tijdens runtime te vragen "kan deze stream worden gelezen?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

Een functie die tekens leest accepteert elk streamtype, ingebouwd of door de gebruiker gedefinieerd,
als ze `(where (CharInput S))` of `:dyn CharInput` neemt.

## 2. Methoden

Elke methode van `CharInput` heeft een standaardimplementatie. Een implementatie schrijft alleen
`read-item`.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Het volgende element. `none` aan het einde. **De enige methode die moet worden geïmplementeerd** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Het volgende teken |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Tot de volgende nieuwe regel (de nieuwe regel wordt verbruikt en verwijderd). Een laatste regel die niet op een nieuwe regel eindigt wordt ook teruggegeven |
| `read-all` | `(read-all s)` | `(S)→string` | Alles wat er over is |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Alleen een teken dat al binnen handbereik is. `none` in plaats van te wachten |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Duwt tot `n` tekens op `v` en geeft terug hoeveel er werkelijk zijn gelezen. Minder dan `n` alleen aan het einde |

`listen` zit in `InputStream` (de ouder van `CharInput`):

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Of het volgende lezen zonder wachten kan worden beantwoord. De standaardwaarde is `false`, **de kant die nooit liegt**: `true` zou een gok zijn, en een verkeerde gok zou `read-char-no-hang` laten blokkeren. Alle ingebouwde streams overschrijven het. **Bij door de gebruiker gedefinieerde streams die het niet overschrijven, geeft `read-char-no-hang` altijd `none` terug** |

`PeekInput` (dat van `CharInput` erft) voegt **één teken terugduwen** toe. Alleen de stream zelf heeft
een plek om het teruggeduwde teken te bewaren, dus dit kan geen standaardimplementatie hebben en is een
aparte trait. `file-stream`/`string-input-stream`/`standard-stream` implementeren het, en elke andere
stream krijgt het wanneer hij met `make-peek-stream` wordt omwikkeld (hoofdstuk 4).

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Laat het volgende lezen `c` teruggeven. **De enige methode die moet worden geïmplementeerd**. Zoals in CL wordt maar één teken gegarandeerd |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Kijkt naar het volgende teken zonder het te verbruiken |

Op dezelfde manier schrijft een implementatie voor `CharOutput` alleen `write-item`.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Schrijft één element. **De enige methode die moet worden geïmplementeerd** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Schrijft één teken |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Schrijft een string |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Een string en een nieuwe regel |
| `terpri` | `(terpri s)` | `(S)→()` | Eén nieuwe regel (de naam van CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Eén nieuwe regel tenzij aan het begin van een regel |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Of het volgende teken dat wordt geschreven een regel zal beginnen. De standaardwaarde is `false` (zodat `fresh-line` de nieuwe regel schrijft: bij twijfel is schrijven de veilige kant). Alle ingebouwde streams overschrijven het |
| `finish-output` | `(finish-output s)` | `(S)→()` | Spoelt de buffer door |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Schrijft alle tekens van `v` in volgorde |

`at-line-start` onthoudt **alleen wat via die stream is geschreven**. `print`/`println`/
`(format true ...)` schrijven naar standaarduitvoer zonder via `*standard-output*` te gaan, dus als je
beide mengt, weet `(fresh-line *standard-output*)` niet van de nieuwe regels die `println` schreef.
Houd je aan een van beide.

`Stream` is gemeenschappelijk voor alle streams:

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Of hij nog open is |
| `close` | `(close s)` | `(S)→()` | Sluit hem. **De GC sluit streams niet**, dus doe het expliciet (of met `with-open-file`) |

## 3. Concrete streamtypes

| Type | Hoe je er een maakt | Geïmplementeerde traits |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` is een van de drie constanten `direction-input` / `direction-output` /
`direction-append`. `open-file` geeft `Err(FileError)` terug als het bestand niet kan worden geopend
(een ontbrekend bestand is een gewoon resultaat, geen panic). De bestandsnaam kan een string of een
`pathname` zijn (`Pathish` in hoofdstuk 9).

`(get-output-stream-string s)` geeft terug wat naar een `string-output-stream` is geschreven en maakt
hem leeg. Zoals in CL kan het ook na `close` worden opgehaald.

**Byte-I/O** gebruikt `ByteInput`/`ByteOutput`. Die leggen het `Item` van
`InputStream`/`OutputStream` vast op `int`, op dezelfde manier als `CharInput`/`CharOutput` het op
`char` vastleggen.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | De volgende byte. `none` aan het einde van het bestand |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Schrijft één byte. Een fout buiten 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | De tekenversie, in bytes |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Idem |

CL bepaalt het elementtype in **de aanroep**, zoals in `(open name :element-type '(unsigned-byte 8))`,
maar hier is het elementtype **het type** van de stream, dus wat verschilt is de functie die hem opent.
Bytes lezen uit een tekenstream is een typefout (`string-input-stream` implementeert `ByteInput` niet).
Een byte lezen direct nadat een teken met `unread-char` is teruggeduwd is ook een fout.

## 4. Samengestelde streams

Allemaal `defstruct`s in de standaardbibliotheek en ze kunnen worden genest.

| Naam | Vorm | Beschrijving |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Schrijft naar alle elementen van een `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Leest uit `in` en schrijft naar `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Leest uit `in` en schrijft de gelezen tekens ook naar `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Leest een `Vector<:dyn CharInput>` na elkaar |
| `make-peek-stream` | `(make-peek-stream in)` | Voegt een terugduwbuffer van één teken toe aan elke `:dyn CharInput`, waardoor het een `PeekInput` wordt (voor `read-sexpr`) |

## 5. Macro's

| Naam | Vorm | Beschrijving |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Openen, de body uitvoeren, sluiten. `Result<waarde van de body, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Leest uit een string |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Geeft terug wat is geschreven |

## 6. Generieke functies en bestandsbewerkingen

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Draagt alles over |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Alle resterende regels |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Leest één `Sexpr` (`read` van CL). `Ok(eof)` aan het einde van de invoer, `Ok(datum d)` wanneer er een wordt gelezen, `Err` als het geen data is. Het **verbruikt het ene witruimteteken** dat het datum beëindigde (zoals in CL). `ReadOutcome` is geen `Option<Sexpr>` zodat het lezen van de lege lijst `()` en het einde van de invoer niet dezelfde waarde zijn |
| `read-sexpr-preserving-whitespace` | Idem | Idem | Hetzelfde, maar laat de witruimte staan (`read-preserving-whitespace` van CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Leest tot `ch` en maakt een lijst. `ch` wordt verbruikt. `Err` als de invoer opraakt |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Schrijft één regel per keer |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | De hele inhoud |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Alle regels |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Schrijft het weg |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Of het bestaat |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Verwijderen, hernoemen (argumenten zijn `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Het absolute pad met symbolische koppelingen en `.`/`..` opgelost. `Err` als het niet bestaat |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Het tijdstip van de laatste wijziging. Het is **universele tijd**, dus `decode-universal-time` ([Tijd](system.md#2-datums-decoderen-en-coderen)) kan het lezen |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | De inlognaam van de eigenaar. `Err` als het bestand niet bestaat, `Ok(none)` als de uid van de eigenaar geen invoer in de wachtwoorddatabase heeft: de twee gevallen die CL onderscheidt blijven gescheiden |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Of het een map is. **Ook `false` als het niet bestaat**; gebruik `probe-file` om de twee te onderscheiden |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Toont de inhoud als truename (het absolute pad met opgeloste symbolische koppelingen, zoals bij `truename`). Symbolische koppelingen waarvan het doel ontbreekt blijven weg. `.`/`..` blijven weg. De volgorde is wat het OS geeft |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Maakt het aan samen met zijn ouders. Slaagt als het al bestaat |

Elk argument dat een bestand noemt **kan een string of een `pathname` zijn**. Dit is dezelfde
behandeling als de pathname-designators van CL, opgelost via de trait `Pathish` in plaats van via een
runtime-typetest (hoofdstuk 9).

Het afsluitende teken van `read-delimited-list` **beëindigt ook tokens**. Het werkt alleen op diepte 0:
in `(1 2]` wordt de `]` gelezen als deel van de eigen tekst van de lijst en gemeld als een kapotte lijst.
Er is geen tegenhanger van het derde argument `recursive-p` van CL.

## 7. Je eigen type tot stream maken

Schrijf één `write-item` en de standaardimplementaties brengen de rest mee. Het kan ook in
samengestelde streams.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

Invoer werkt op dezelfde manier: je schrijft alleen `read-item`. Zelfs een type zonder eigen terugduwen
kan worden gelezen met `read` zodra het is omwikkeld, zoals in
`(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Naam | Aanroep | Type | Beschrijving |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` leest het teken `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Geeft terug wat is geregistreerd |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` leest de reeks van twee tekens `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Idem |

`F` is `(fn (string-input-stream char) Option<Sexpr>)`. Hoe je ze gebruikt, wanneer ze van kracht worden
en hoe ze van CL verschillen staat in de
[Syntaxreferentie](../syntax.md#11-readermacros-readtable).

## 9. Padnamen `pathname`

Een in delen gesplitste bestandsnaam. Hij bevat de door `/` gescheiden mapcomponenten, de naam, het type
(extensie) en of hij bij de root begint.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 De pathname-designator-trait `Pathish`

Waar CL een pathname-designator accepteert (een string of een pathname), accepteert deze taal een
`Pathish`. Zowel `string` als `pathname` implementeren hem, en **elke bestandsbewerking neemt hem
generiek**, dus `(open-input "a.txt")` en `(open-input p)` zijn beide gewone aanroepen (er is geen
runtime-typetest). De `namestring` van een string geeft zichzelf terug, dus zolang je een string
doorgeeft, vindt er geen parsing plaats.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | De stringvorm. Moet worden geïmplementeerd |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Converteert naar een `pathname` (de functie `pathname` van CL, hernoemd omdat ze zou botsen met de typenaam). Moet worden geïmplementeerd |

### 9.2 Functies

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Splitst een string in delen. Een afsluitende `/` (of een lege naam) betekent "geen naam", dat wil zeggen een map |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Bouwt er een uit alleen de gegeven componenten (allemaal `&key`). Een weggelaten naam of type blijft "afwezig" en is iets wat `merge-pathnames` invult |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | De mapcomponenten, buitenste eerst |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | De naam zonder het type. `none` voor een map |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Na de laatste punt. Een voorloopstip telt niet mee (heel `.gitignore` is de naam) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Of hij bij de root begint |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | De thuismap. `none` als er geen `$HOME` is (CL staat ook `NIL` toe) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Het deel tot en met de laatste `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Alleen het deel `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Vult de componenten die in `p` ontbreken uit `default`. Een relatieve `p` komt onder de map van `default`; een absolute `p` behoudt zijn eigen map |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | De vorm relatief aan `default`. Heel `p` als het niet onder de basis ligt |

De typeargumenten dragen allemaal `(where (Pathish P))`.

## 10. Verschillen met CL

- **Een traithiërarchie, geen klassenhiërarchie.** Er is geen `input-stream-p` / `output-stream-p`: het
  type draagt de richting, dus het is geen vraag die je tijdens runtime stelt.
- **`read` heeft verschillende namen voor de string- en de streamversie.** `(read "...")` (komt overeen
  met de eerste waarde van `read-from-string` van CL; heb je ook de positie nodig waar het lezen
  eindigde, gebruik dan `read-from-string`) en `(read-sexpr s)` (`read` van CL). Een aanroep wordt
  opgelost naar één ontvangertype, dus dezelfde naam kan niet worden overladen.
- **Terugduwen is een aparte trait** (`PeekInput`), zodat types die alleen `read-char` nodig hebben niet
  worden gedwongen `unread-char` te implementeren.
- **Sluiten is expliciet.** De GC sluit streams niet (de GC draait op onvoorspelbare momenten, dus het
  aan de GC overlaten zou het moment van sluiten ook onvoorspelbaar maken). `with-open-file` gebruiken is
  de veilige manier.
- **Padnamen hebben geen host-, apparaat- of versiecomponenten.** Er zijn geen padnamen met jokertekens
  en geen logische padnamen (`logical-pathname`). Het scheidingsteken is altijd `/`.
- **De functie `pathname` is `to-pathname`**, omdat types, traits en functies één namespace delen.
- **Er is geen matching met jokertekens**, dus `directory` is een functie die "de inhoud van die map
  toont" en niets meer. `directory` van CL matcht tegen een padnaampatroon.
