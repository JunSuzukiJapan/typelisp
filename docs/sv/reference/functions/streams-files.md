<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Strömmar och filer

Strömtraits och metoder, konkreta strömtyper, filoperationer och pathnames. Nätverkssocketar är också
strömmar och tas upp i [Nätverk](network.md).

## 1. Trait-hierarkin

Det CL uttrycker med en klasshierarki uttrycks här med en **trait-hierarki**. Både riktningen (in / ut)
och elementtypen avgörs **statiskt**, så det behövs ingen fråga vid körning om "kan den här strömmen
läsas?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; teckeninmatning
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; teckenutmatning
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; inmatning som kan skjuta tillbaka ett tecken
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byteinmatning
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byteutmatning
```

En funktion som läser tecken accepterar vilken strömtyp som helst, inbyggd eller användardefinierad, om
den tar `(where (CharInput S))` eller `:dyn CharInput`.

## 2. Metoder

Varje metod i `CharInput` har en standardimplementation. En implementation skriver bara `read-item`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Nästa element. `none` vid slutet. **Den enda metod som måste implementeras** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Nästa tecken |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Fram till nästa radbyte (radbytet förbrukas och tas bort). En sista rad som inte slutar med ett radbyte returneras också |
| `read-all` | `(read-all s)` | `(S)→string` | Allt som är kvar |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Bara ett tecken som redan finns till hands. `none` i stället för att vänta |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Lägger upp till `n` tecken på `v` och returnerar hur många som faktiskt lästes. Färre än `n` bara vid slutet |

`listen` finns i `InputStream` (föräldern till `CharInput`):

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Om nästa läsning kan besvaras utan att vänta. Standardvärdet är `false`, **den sida som aldrig ljuger**: `true` vore en gissning, och en felaktig gissning skulle få `read-char-no-hang` att blockera. Alla inbyggda strömmar åsidosätter den. **För användardefinierade strömmar som inte åsidosätter den returnerar `read-char-no-hang` alltid `none`** |

`PeekInput` (som ärver från `CharInput`) lägger till att **skjuta tillbaka ett tecken**. Bara strömmen
själv har en plats att behålla det tillbakaskjutna tecknet, så detta kan inte ha en standardimplementation
och är ett separat trait. `file-stream`/`string-input-stream`/`standard-stream` implementerar det, och
varje annan ström får det när den omsluts med `make-peek-stream` (kapitel 4).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Får nästa läsning att returnera `c`. **Den enda metod som måste implementeras**. Som i CL garanteras bara ett tecken |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Tittar på nästa tecken utan att förbruka det |

På samma sätt skriver en implementation för `CharOutput` bara `write-item`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Skriver ett element. **Den enda metod som måste implementeras** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Skriver ett tecken |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Skriver en sträng |
| `write-line` | `(write-line s str)` | `(S,string)→()` | En sträng och ett radbyte |
| `terpri` | `(terpri s)` | `(S)→()` | Ett radbyte (CL:s namn) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Ett radbyte om man inte står i början av en rad |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Om nästa tecken som skrivs börjar en rad. Standardvärdet är `false` (så `fresh-line` skriver radbytet: vid tvekan är det säkra att skriva). Alla inbyggda strömmar åsidosätter den |
| `finish-output` | `(finish-output s)` | `(S)→()` | Tömmer bufferten |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Skriver alla tecken i `v` i ordning |

`at-line-start` kommer ihåg **bara det som skrivits genom just den strömmen**. `print`/`println`/
`(format true ...)` skriver till standard ut utan att gå genom `*standard-output*`, så om du blandar
de två känner `(fresh-line *standard-output*)` inte till de radbyten `println` skrev. Håll dig till en av
dem.

`Stream` är gemensamt för alla strömmar:

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Om den fortfarande är öppen |
| `close` | `(close s)` | `(S)→()` | Stänger den. **GC stänger inte strömmar**, så gör det explicit (eller med `with-open-file`) |

## 3. Konkreta strömtyper

| Typ | Hur man skapar en | Implementerade traits |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` är en av de tre konstanterna `direction-input` / `direction-output` /
`direction-append`. `open-file` returnerar `Err(FileError)` om filen inte kan öppnas (en saknad fil är ett
vanligt resultat, inte en panic). Filnamnet kan vara en sträng eller ett `pathname` (`Pathish` i
kapitel 9).

`(get-output-stream-string s)` returnerar det som skrivits till en `string-output-stream` och tömmer den.
Som i CL kan den tas ut även efter `close`.

**Byte-I/O** använder `ByteInput`/`ByteOutput`. Dessa fastställer `Item` för
`InputStream`/`OutputStream` till `int`, på samma sätt som `CharInput`/`CharOutput` fastställer det till
`char`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Nästa byte. `none` vid filslut |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Skriver ett byte. Ett fel utanför 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Teckenversionen, i byte |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Som ovan |

CL avgör elementtypen i **anropet**, som i `(open name :element-type '(unsigned-byte 8))`, men här är
elementtypen **strömmens typ**, så det som skiljer sig är funktionen som öppnar den. Att läsa byte från en
teckenström är ett typfel (`string-input-stream` implementerar inte `ByteInput`). Att läsa ett byte direkt
efter att ha skjutit tillbaka ett tecken med `unread-char` är också ett fel.

## 4. Sammansatta strömmar

Alla är `defstruct`s i standardbiblioteket och kan nästlas.

| Namn | Form | Beskrivning |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Skriver till alla i en `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Läser från `in` och skriver till `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Läser från `in` och skriver också de lästa tecknen till `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Läser en `Vector<:dyn CharInput>` efter varandra |
| `make-peek-stream` | `(make-peek-stream in)` | Lägger till en tillbakaskjutning av ett tecken till vilken `:dyn CharInput` som helst och gör den till en `PeekInput` (för `read-sexpr`) |

## 5. Makron

| Namn | Form | Beskrivning |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Öppna, kör kroppen, stäng. `Result<kroppens värde, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Läser från en sträng |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Returnerar det som skrevs |

## 6. Generiska funktioner och filoperationer

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Överför allt |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Alla återstående rader |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Läser en `Sexpr` (CL:s `read`). `Ok(eof)` vid slutet av indata, `Ok(datum d)` när en läses, `Err` om det inte är data. Den **förbrukar det ena blanktecken** som avslutade datumet (som i CL). `ReadOutcome` är inte en `Option<Sexpr>` så att läsning av den tomma listan `()` och slutet av indata inte blir samma värde |
| `read-sexpr-preserving-whitespace` | Som ovan | Som ovan | Samma, men lämnar blanktecknet (CL:s `read-preserving-whitespace`) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Läser fram till `ch` och gör en lista. `ch` förbrukas. `Err` om indata tar slut |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Skriver en rad i taget |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Hela innehållet |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Alla rader |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Skriver ut det |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Om den finns |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Ta bort, byt namn (argumenten är `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Den absoluta sökvägen med symboliska länkar och `.`/`..` upplösta. `Err` om den inte finns |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Tidpunkten för senaste ändring. Det är **universal time**, så `decode-universal-time` ([Tid](system.md#2-avkoda-och-koda-datum)) kan läsa den |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Ägarens inloggningsnamn. `Err` om filen inte finns, `Ok(none)` om ägarens uid inte har någon post i lösenordsdatabasen: de två fall CL skiljer åt hålls isär |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Om det är en katalog. **Också `false` om den inte finns**; använd `probe-file` för att skilja de två åt |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Listar innehållet som truenames (den absoluta sökvägen med symboliska länkar upplösta, som med `truename`). Symboliska länkar vars mål saknas utelämnas. `.`/`..` utelämnas. Ordningen är den OS ger |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Skapar den tillsammans med sina föräldrar. Lyckas om den redan finns |

Varje argument som namnger en fil **kan vara en sträng eller ett `pathname`**. Det är samma behandling som
CL:s pathname-designatorer, upplöst genom traitet `Pathish` i stället för ett typtest vid körning
(kapitel 9).

Avslutningstecknet i `read-delimited-list` **avslutar också token**. Det gäller bara på djup 0: i `(1 2]`
läses `]` som en del av listans egen text och rapporteras som en trasig lista. Det finns ingen motsvarighet
till CL:s tredje argument `recursive-p`.

## 7. Göra din egen typ till en ström

Skriv ett enda `write-item` så tar standardimplementationerna med resten. Den kan också användas i
sammansatta strömmar.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; varje återstående metod är standardimplementationen

(write-line (counter::new 0) "four")   ; write-line, terpri och fresh-line fungerar alla
```

Inmatning fungerar på samma sätt: du skriver bara `read-item`. Även en typ utan egen tillbakaskjutning kan
`read` när den omsluts, som i `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Namn | Anrop | Typ | Beskrivning |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` läser tecknet `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Returnerar det som är registrerat |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` läser teckenföljden `d s` med två tecken |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Som ovan |

`F` är `(fn (string-input-stream char) Option<Sexpr>)`. Hur de används, när de får effekt och hur de skiljer
sig från CL finns i [Syntaxreferensen](../syntax.md#11-läsarmakron-readtable).

## 9. Pathnames `pathname`

Ett filnamn uppdelat i delar. Det håller de `/`-separerade katalogkomponenterna, namnet, typen
(filändelsen) och om det börjar vid roten.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   delat vid den sista punkten
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Pathname-designator-traitet `Pathish`

Där CL accepterar en pathname-designator (en sträng eller ett pathname) accepterar det här språket en
`Pathish`. Både `string` och `pathname` implementerar det, och **varje filoperation tar det generiskt**, så
`(open-input "a.txt")` och `(open-input p)` är båda vanliga anrop (det finns inget typtest vid körning).
`namestring` för en sträng returnerar bara sig själv, så så länge du skickar en sträng sker ingen
tolkning.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Strängformen. Måste implementeras |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Konverterar till ett `pathname` (CL:s funktion `pathname`, omdöpt eftersom den skulle krocka med typnamnet). Måste implementeras |

### 9.2 Funktioner

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Delar upp en sträng i delar. Ett avslutande `/` (eller ett tomt namn) betyder "inget namn", det vill säga en katalog |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Bygger ett av bara de komponenter som anges (alla `&key`). Ett namn eller en typ som utelämnas förblir "frånvarande" och är något `merge-pathnames` fyller i |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Katalogkomponenterna, yttersta först |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Namnet utan typen. `none` för en katalog |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Efter den sista punkten. En inledande punkt räknas inte (hela `.gitignore` är namnet) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Om det börjar vid roten |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Hemkatalogen. `none` om det inte finns någon `$HOME` (CL tillåter också `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Delen fram till det sista `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Bara delen `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Fyller de komponenter som saknas i `p` från `default`. Ett relativt `p` hamnar under katalogen i `default`; ett absolut `p` behåller sin egen katalog |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Formen relativt `default`. Hela `p` om det inte ligger under basen |

Typargumenten har alla `(where (Pathish P))`.

## 10. Skillnader mot CL

- **En trait-hierarki, inte en klasshierarki.** Det finns inget `input-stream-p` / `output-stream-p`:
  typen bär riktningen, så det är ingen fråga att ställa vid körning.
- **`read` har olika namn för sträng- och strömversionerna.** `(read "...")` (motsvarar det första
  värdet från CL:s `read-from-string`; om du också behöver positionen där läsningen slutade, använd
  `read-from-string`) och `(read-sexpr s)` (CL:s `read`). Ett anrop löses upp till en mottagartyp, så
  samma namn kan inte överlagras.
- **Tillbakaskjutning är ett separat trait** (`PeekInput`), så typer som bara behöver `read-char` inte
  tvingas implementera `unread-char`.
- **Att stänga är explicit.** GC stänger inte strömmar (GC körs vid oförutsägbara tidpunkter, så att
  överlåta det åt GC skulle göra tidpunkten för stängning oförutsägbar också). Att använda
  `with-open-file` är det säkra sättet.
- **Pathnames har inga värd-, enhets- eller versionskomponenter.** Det finns inga pathnames med
  jokertecken och inga logiska pathnames (`logical-pathname`). Avgränsaren är alltid `/`.
- **Funktionen `pathname` heter `to-pathname`**, eftersom typer, traits och funktioner delar en
  namnrymd.
- **Det finns ingen matchning med jokertecken**, så `directory` är en funktion som "listar innehållet i
  den katalogen" och inget mer. CL:s `directory` matchar mot ett pathname-mönster.
