<!-- translated-from: docs/ja/reference/functions/collections.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# Strängar, tecken och samlingar

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>` och `Deque<T>`.

## 1. Strängar `string`

Strängar är oföränderliga.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Konverterar till versaler (bara ASCII). Som CL:s `string-upcase` returnerar den en ny sträng. Strängar är oföränderliga, så det finns ingen destruktiv `nstring-upcase`; den här tar dess plats |
| `downcase` | `(downcase s)` | `string→string` | Konverterar till gemener (bara ASCII). Tar `nstring-downcase`s plats |
| `capitalize` | `(capitalize s)` | `string→string` | Gör första bokstaven i varje ord versal och resten gemena (CL:s `string-capitalize`). Ett ord är en maximal följd av bokstäver och siffror |
| `length` | `(length s)` | `string→int` | Antal tecken |
| `ref` | `(ref s i)` | `(string,int)→char` | Tecken `i`. Ger panic utanför intervallet |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Delsträngen `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Konkatenering. Tre eller fler kan anges (samma som `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Lexikografisk jämförelse |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Strikt lexikografiskt mindre än (samma som `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Identitetsjämförelse (om de är samma objekt, inte samma innehåll) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Jämför innehåll (skiljer på stora och små bokstäver) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Jämför innehåll (skiljer inte på stora och små bokstäver, bara ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Om innehållet skiljer sig (CL:s `string/=`. Den variadiska formen jämför intilliggande par) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Ordning utan hänsyn till skiftläge (CL:s `string-lessp` och så vidare). Vid gemensamt prefix är den kortare mindre |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | En sträng med `n` kopior av `c` (CL:s `make-string`) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Positionen där `sub` först förekommer. **CL:s `search` har argumenten tvärtom** (`(search pattern sequence)`). Den tomma strängen hittas vid 0. För nyckelord, se [nyckelordsargument för sekvenser](sequences.md#6-nyckelordsargument) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Den första positionen där de skiljer sig. `none` bara när de är `equal`. Om den ena är ett prefix av den andra, slutet av den kortare. Nyckelord som ovan |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Tar bort tecken som ingår i `bag` från båda ändar / vänster / höger (CL:s `string-trim` och så vidare). Utan `bag` blanktecken `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Delar vid `sep`. CL har ingen motsvarighet. På varandra följande avgränsare ger tomma element. Ger panic om `sep` är tom |
| `to-string` | `(to-string x)` | `T→string` | Konverterar till en sträng som `~a` gör. Implementerad för `int`/`i32`/`f64`/`bool`/`char`/`string` (CL:s `princ-to-string`) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Kodar som UTF-8 (varje element 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Avkodar. `none` om det inte är giltig UTF-8 |

## 2. Tecken `char`

Ett `char` är ett skalärt Unicode-värde. Skiftlägeskonvertering och klassificering hanterar bara
ASCII-området.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Konverterar till versal (bara ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Konverterar till gemen (bara ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Jämförelse efter kodpunkt |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Strikt mindre än efter kodpunkt (samma som `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Om det är en ASCII-bokstav |
| `digitp` | `(digitp c)` | `char→bool` | Om det är en ASCII-siffra |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Jämför värden |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Jämför värden utan hänsyn till skiftläge (CL:s `char-equal`) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Om värdena skiljer sig (CL:s `char/=`. **Den variadiska formen jämför intilliggande par**, till skillnad från CL, som frågar om alla par skiljer sig) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Ordning utan hänsyn till skiftläge (CL:s `char-lessp` och så vidare) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Versal / gemen / har över huvud taget skiftläge (CL:s `upper-case-p` och så vidare) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | En bokstav eller en siffra (samma namn som i CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Om det är utskrivbart. Inkluderar mellanslag, inte radbyte eller tabb (CL:s `graphic-char-p`) |
| `standardp` | `(standardp c)` | `char→bool` | Om det är ett av CL:s 96 standardtecken, det vill säga `graphicp` plus radbyte (CL:s `standard-char-p`) |
| `char->int` | `(char->int c)` | `char→int` | Det skalära Unicode-värdet (det omvända är `int->char`/`try-int->char` i [Tal](numbers.md#1-heltal-med-fast-bredd)). Motsvarar CL:s `char-code`/`char-int` |
| `char->string` | `(char->string c)` | `char→string` | En sträng med ett tecken. CL:s funktion `string` täcker detta genom att ta en designator, men det här språket har inga designatorer, så riktningen står i namnet |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Siffrans **vikt** i den basen (CL:s `digit-char-p`). `digitp` är en separat funktion som returnerar `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Tecknet för vikten `w`. Versal för 10 och uppåt (CL:s `digit-char`; basen är högst 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Tecknets namn. Bara de namngivna tecken som läsaren kan läsa har namn (CL:s `char-name`) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Tecknet för ett namn. Skiljer inte på skiftläge, och accepterar också läsarens alias (`linefeed`/`null`) (CL:s `name-char`) |

Det finns ingen konstant som motsvarar `char-code-limit` (övre gränsen för `char` sätts av Unicode, inte
av språket).

## 3. `Vector<T>`

En växande array.
Ett värde kan skrivas `#(1 2 3)` (se [syntaxreferensen](../syntax.md#1-lexikaliska-element);
elementtypen kommer från sammanhanget eller det första elementet, och varje evaluering skapar en ny
vektor). Det skrivs också ut som `#(1 2 3)`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Skapar en tom vektor. Typargumentet kommer från den förväntade typen, så i ett bart `let` skriver man `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopior av `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Lägger till sist |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Läser element `i`. Ger panic utanför intervallet |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Ändrar element `i`. Ger panic utanför intervallet. Kan också skrivas `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Antal element |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Tar bort det sista elementet och returnerar det. `None` om tom (till skillnad från `get`/`set` ger den inte panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Skapar en iterator som implementerar `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Lägger till `x` om inget lika element finns (CL:s `pushnew`. Den behöver inte skriva om en plats, så den är en metod i stället för ett makro) |

`map`/`filter` och liknande är [sekvensfunktioner](sequences.md#4-sekvensfunktioner-på-iter): skicka
vektorn genom `iter`, som i `(map (iter v) f)`. Destruktiva operationer (`nreverse`, `delete` och så
vidare) finns i [Destruktiva operationer](sequences.md#7-destruktiva-operationer).

## 4. `HashTable<K,V>`

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Skapar en tom tabell |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Uppslagning |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Infogar eller skriver över |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Tar bort posten och returnerar det gamla värdet, om det finns |
| `count` | `(count h)` | `HashTable<K,V>→int` | Antal poster |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Tar bort allt |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | En ögonblicksbild av nycklarna |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | En ögonblicksbild av värdena |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | En ögonblicksbild av paren `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | En iterator som implementerar `Iter`. Elementen är `cons-cell`s av typen `(k . v)`. Motsvarar CL:s `with-hash-table-iterator`; `doiter`/`map`/`filter` och andra fungerar på den som den är |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL:s `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL:s `hash-table-size`. I den här tabellen är det antalet upptagna poster (lika med `count`) |

**Vilken typ som helst som implementerar `Hash` kan vara nyckel**, inklusive `defstruct`/`defenum`-typer.
`get`/`set`/`remove` har `(where (Hash K))`, så en tabell med en nyckeltyp som inte implementerar det är
ett **typfel** (`f64` har inget `Hash` på grund av `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returnerar ett icke-negativt värde som ryms i ett fixnum
```

Implementerat för: `int` och de sex heltalstyperna med fast bredd, `bool`, `char`, `string` och `symbol`
(inte för flyttal). För dina egna typer håller du resultatet icke-negativt genom att `logand`a det med
`*sxhash-mask*` (2^30-1). För att hasha en sträng kan du anropa `(sxhash-string s)` (32-bitars FNV-1a),
som implementationen för `string` använder.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Om två nycklar är samma avgörs av **nyckeltypen själv** (`sxhash`, och `equals` från `Eq`, supertraitet
till `Hash`), inte av objektidentitet. Därför kan man, som ovan, slå upp med en nyckel som är "ett annat
värde men lika".

Det är i sin ordning att `sxhash` kolliderar (kontraktet för `Hash` går bara åt ett håll: lika värden
måste ha samma hash). Nycklar som kolliderar skiljs åt med `equals`.

## 5. `Array<T>` (flerdimensionella arrayer)

En `defstruct` i standardbiblioteket. Den är inte en inbyggd typ, så allt du kan göra med en `defstruct`
kan göras med den.
Ett värde kan skrivas `#2A((1 2) (3 4))` (se
[syntaxreferensen](../syntax.md#1-lexikaliska-element)).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL:s `make-array`. `dims` kopieras. `init` är startvärdet för varje cell (CL:s `:initial-element`; det här språket har ingen "obunden cell", så det är obligatoriskt). `:fill-pointer` bara för en dimension |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL:s `aref` / `(setf (aref …))`. Ger panic om ett index är utanför intervallet |
| `aref` | `(aref a i j …)` | — | CL:s stavning med bara index. Expanderar till `get`/`set` ovan. `(setf (aref a i j) v)` fungerar också |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL:s `row-major-aref`. Ett platt index |
| `rank` | `(rank a)` | `Array<T>→int` | CL:s `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL:s `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL:s `array-dimensions`. Returnerar en **kopia**, precis som CL returnerar en ny lista |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL:s `array-total-size` (antalet allokerade celler, oberoende av fyllpekaren) |
| `len` | `(len a)` | `Array<T>→int` | CL:s `length` på arrayer. Fyllpekaren om det finns en, annars `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL:s `array-in-bounds-p`. Falskt (inte ett fel) även när **antalet** index är fel |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL:s `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL:s `adjust-array`. Rangen kan inte ändras. Element som stannar inom intervallet behålls på sina index, och nya celler får `init`. Till skillnad från CL returnerar den inte arrayen (varje array i det här språket är justerbar, så det finns ingen andra array att returnera) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL:s `vector-push-extend`. Ger panic utan fyllpekare |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL:s `vector-pop`. `none` om tom |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Fyllpekaren (`none` om det inte finns någon). Kan skrivas med `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | En iterator i radvis ordning (row-major). Stannar vid fyllpekaren om det finns en |

- **Index är en `Vector<int>`.** En metod kan inte deklarera "samma typ av argument upprepat godtyckligt
  många gånger på slutet", och sockret `aref` överbryggar det glappet.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **finns inte**. Mottagarens statiska typ besvarar redan dessa frågor.
- `Array::new` är den konstruktor i fältordning som `defstruct` genererar och är inte avsedd för att
  skapa arrayer. Använd `Array::make`.
- **Arrayer skrivs ut i CL:s arraysyntax.** Rang 1 är `#(1 2 3)`; andra ranger är `#nA` följt av lika
  många nivåer parenteser (`#2A((1 2 3) (4 5 6))`); rang 0 är `#0A5`. Utskriften stannar vid
  fyllpekaren om det finns en. Att sätta `*print-array*` ([Utskrift](printing.md#6-styra-hur-mycket-som-skrivs-ut))
  till falskt skriver bara ut formen, `#<array 2x3>`. Bara en array vars element är en `defstruct` utan
  `print-object` skrivs ut i den inbyggda formen `#<array<...> ...>` (det är inte ett fel).

## 6. `BitVector` (bitvektorer)

En följd av bitar med fast längd. En `defstruct` i standardbiblioteket.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Längd `n`, alla bitar 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Ger panic utanför intervallet |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL:s stavningar. `(setf (bit v i) b)` fungerar också. CL:s `sbit` skiljer sig från `bit` bara genom att kräva en enkel bitvektor, men det här språket har bara en sorts bitvektor |
| `len` | `(len v)` | `BitVector→int` | Antal bitar |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Returnerar en ny bitvektor. Ger panic om längderna skiljer sig. Det finns inget tredje argument som i CL (resultatets mål) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Komplement |

Det finns ingen `bit-vector-p` (den statiska typen besvarar det).

## 7. `HashSet<T>`

En samling element utan dubbletter (Rusts `HashSet`). En `defstruct` i standardbiblioteket vars
innehåll är en `HashTable<T,()>`. Elementtypen måste implementera `Hash`, liksom en nyckel i en
`HashTable`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `make` | `(HashSet::make)` | `()→HashSet<T>` | Skapar en tom mängd. Typargumentet kommer från den förväntade typen |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | Lägger till `x`. `true` om det inte fanns, `false` om det redan fanns |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | Om `x` finns |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | Tar bort `x`. `true` om det fanns |
| `count` | `(count s)` | `HashSet<T>→int` | Antalet element |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | Tar bort allt |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | En iterator över elementen. Ordningen är inte bestämd |

```lisp
(let ((seen (the HashSet<string> (HashSet::make))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

Gemensamt för de tre typerna i kapitel 7 till 9:

- Skapa dem med `make`. `new` är konstruktorn i fältordning som `defstruct` genererar, inte den man
  skapar dem med (som med `Array::make`).
- `iter` går igenom en kopia tagen vid anropet. Ändrar man samma samling inuti en `doiter` ser den
  loopen det inte.
- Om elementtyperna implementerar `print-object` skrivs elementen ut, i formen `#<hashset "a" "b">`
  `#<sortedtable 1 "a">` `#<deque 1 2>`.

## 8. `SortedTable<K,V>`

En tabell i stigande nyckelordning (Rusts `BTreeMap`). Nyckeltypen måste implementera `Ord`. Nycklar
och värden hålls i två `Vector` i nyckelordning, och sökning sker binärt. `set` av en ny nyckel och
`remove` flyttar elementen efter dess position.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `make` | `(SortedTable::make)` | `()→SortedTable<K,V>` | Skapar en tom tabell |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | Uppslag |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | Infogar eller skriver över |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | Tar bort och returnerar det gamla värdet om det fanns |
| `count` | `(count t)` | `SortedTable<K,V>→int` | Antalet element |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | Tar bort allt |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | Nycklarna, minsta först |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | Värdena i nyckelordning |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | `#{nyckel värde}`-tupler i nyckelordning |

```lisp
(let ((t (the SortedTable<string,int> (SortedTable::make))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 och pear 3
```

## 9. `Deque<T>`

En följd som man kan lägga till i och ta ut ur i båda ändar (Rusts `VecDeque`).

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `make` | `(Deque::make)` | `()→Deque<T>` | Skapar en tom följd |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | Lägger till först / sist |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | Tar ut det första / sista elementet och returnerar det. `none` om tom |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | Tittar på det första / sista elementet (utan att ta ut det) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | Det `i`:te från början. `none` utanför intervallet |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | Skriver över det `i`:te. Panic utanför intervallet |
| `count` | `(count d)` | `Deque<T>→int` | Antalet element |
| `clear` | `(clear d)` | `Deque<T>→Unit` | Tar bort allt |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | Från början, i ordning |

```lisp
(let ((q (the Deque<int> (Deque::make))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```
