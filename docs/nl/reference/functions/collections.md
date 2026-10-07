<!-- translated-from: docs/ja/reference/functions/collections.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Strings, tekens en collecties

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` en `BitVector`.

## 1. Strings `string`

Strings zijn onveranderlijk.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Zet om naar hoofdletters (alleen ASCII). Net als `string-upcase` van CL geeft het een nieuwe string terug. Strings zijn onveranderlijk, dus er is geen destructieve `nstring-upcase`; dit neemt zijn plaats in |
| `downcase` | `(downcase s)` | `string→string` | Zet om naar kleine letters (alleen ASCII). Neemt de plaats van `nstring-downcase` in |
| `capitalize` | `(capitalize s)` | `string→string` | Zet de eerste letter van elk woord in hoofdletter en de rest in kleine letters (`string-capitalize` van CL). Een woord is een maximale reeks letters en cijfers |
| `length` | `(length s)` | `string→int` | Aantal tekens |
| `ref` | `(ref s i)` | `(string,int)→char` | Teken `i`. Geeft een panic buiten het bereik |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | De substring `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Aaneenschakeling. Er kunnen er drie of meer worden gegeven (hetzelfde als `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Lexicografische vergelijking |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Strikt lexicografisch kleiner dan (hetzelfde als `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Identiteitsvergelijking (of ze hetzelfde object zijn, niet dezelfde inhoud) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Vergelijkt inhoud (hoofdlettergevoelig) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Vergelijkt inhoud (niet hoofdlettergevoelig, alleen ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Of de inhoud verschilt (`string/=` van CL. De variadische vorm vergelijkt aangrenzende paren) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Niet hoofdlettergevoelige ordening (`string-lessp` van CL enzovoort). Bij een gemeenschappelijk voorvoegsel is de kortste kleiner |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Een string van `n` kopieën van `c` (`make-string` van CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | De positie waar `sub` het eerst voorkomt. **`search` van CL heeft de argumenten andersom** (`(search pattern sequence)`). De lege string wordt op 0 gevonden. Voor keywords zie [keyword-argumenten van sequenties](sequences.md#6-keyword-argumenten) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | De eerste positie waar ze verschillen. `none` alleen wanneer ze `equal` zijn. Is de een een voorvoegsel van de ander, dan het einde van de kortste. Keywords als hierboven |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Verwijdert tekens die in `bag` zitten van beide uiteinden / links / rechts (`string-trim` van CL enzovoort). Zonder `bag` witruimte `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Splitst bij `sep`. CL heeft geen tegenhanger. Opeenvolgende scheidingstekens geven lege elementen. Geeft een panic als `sep` leeg is |
| `to-string` | `(to-string x)` | `T→string` | Converteert naar een string zoals `~a` doet. Geïmplementeerd voor `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` van CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Codeert als UTF-8 (elk element 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Decodeert. `none` als het geen geldige UTF-8 is |

## 2. Tekens `char`

Een `char` is een Unicode-scalarwaarde. Hoofdletterconversie en classificatie behandelen alleen het
ASCII-bereik.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Zet om naar hoofdletter (alleen ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Zet om naar kleine letter (alleen ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Vergelijking op codepoint |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Strikt kleiner dan op codepoint (hetzelfde als `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Of het een ASCII-letter is |
| `digitp` | `(digitp c)` | `char→bool` | Of het een ASCII-cijfer is |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Vergelijkt waarden |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Vergelijkt waarden zonder op hoofdletters te letten (`char-equal` van CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Of de waarden verschillen (`char/=` van CL. **De variadische vorm vergelijkt aangrenzende paren**, anders dan CL, dat vraagt of alle paren verschillen) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Niet hoofdlettergevoelige ordening (`char-lessp` van CL enzovoort) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Hoofdletter / kleine letter / heeft hoofdlettergebruik überhaupt (`upper-case-p` van CL enzovoort) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Een letter of een cijfer (dezelfde naam als in CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Of het afdrukbaar is. Omvat spatie, geen nieuwe regel of tab (`graphic-char-p` van CL) |
| `standardp` | `(standardp c)` | `char→bool` | Of het een van de 96 standaardtekens van CL is, dat wil zeggen `graphicp` plus nieuwe regel (`standard-char-p` van CL) |
| `char->int` | `(char->int c)` | `char→int` | De Unicode-scalarwaarde (het omgekeerde is `int->char`/`try-int->char` in [Getallen](numbers.md#1-gehele-getallen-met-vaste-breedte)). Komt overeen met `char-code`/`char-int` van CL |
| `char->string` | `(char->string c)` | `char→string` | Een string van één teken. De functie `string` van CL dekt dit door een designator te nemen, maar deze taal heeft geen designators, dus de richting zit in de naam |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Het **gewicht** van het cijfer in dat grondtal (`digit-char-p` van CL). `digitp` is een aparte functie die `bool` teruggeeft |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Het teken voor gewicht `w`. Hoofdletter vanaf 10 (`digit-char` van CL; het grondtal is hoogstens 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | De naam van het teken. Alleen de benoemde tekens die de reader kan lezen hebben namen (`char-name` van CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Het teken voor een naam. Niet hoofdlettergevoelig, en accepteert ook de aliassen van de reader (`linefeed`/`null`) (`name-char` van CL) |

Er is geen constante die overeenkomt met `char-code-limit` (de bovengrens van `char` wordt door
Unicode bepaald, niet door de taal).

## 3. `Vector<T>`

Een groeibare array.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Maakt een lege vector. Het typeargument komt uit het verwachte type, dus schrijf in een kale `let` `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopieën van `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Voegt aan het einde toe |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Leest element `i`. Geeft een panic buiten het bereik |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Wijzigt element `i`. Geeft een panic buiten het bereik. Kan ook als `(setf (get v i) x)` worden geschreven |
| `len` | `(len v)` | `Vector<T>→int` | Aantal elementen |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Verwijdert het laatste element en geeft het terug. `None` als leeg (anders dan `get`/`set` geeft het geen panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Maakt een iterator die `Iter` implementeert |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Voegt `x` toe als er geen gelijk element bestaat (`pushnew` van CL. Het hoeft geen plaats te herschrijven, dus het is een methode en geen macro) |

`map`/`filter` en verwanten zijn [sequentiefuncties](sequences.md#4-sequentiefuncties-op-iter): geef
de vector door via `iter`, zoals in `(map (iter v) f)`. Destructieve bewerkingen (`nreverse`, `delete`
enzovoort) staan in [Destructieve bewerkingen](sequences.md#7-destructieve-bewerkingen).

## 4. `HashTable<K,V>`

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Maakt een lege tabel |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Opzoeken |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Invoegen of overschrijven |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Verwijdert de invoer en geeft de oude waarde terug, als die er was |
| `count` | `(count h)` | `HashTable<K,V>→int` | Aantal invoeren |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Verwijdert alles |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Een momentopname van de sleutels |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Een momentopname van de waarden |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Een momentopname van de paren `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Een iterator die `Iter` implementeert. De elementen zijn `cons-cell`s `(k . v)`. Komt overeen met `with-hash-table-iterator` van CL; `doiter`/`map`/`filter` en andere werken er direct op |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` van CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` van CL. In deze tabel is het het aantal bezette invoeren (gelijk aan `count`) |

**Elk type dat `Hash` implementeert kan een sleutel zijn**, ook `defstruct`/`defenum`-types.
`get`/`set`/`remove` dragen `(where (Hash K))`, dus een tabel met een sleutel van een type dat het
niet implementeert is een **typefout** (`f64` heeft geen `Hash` vanwege `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Geïmplementeerd voor: `int` en de zes gehele types met vaste breedte, `bool`, `char`, `string` en
`symbol` (niet voor drijvendekommagetallen). Houd bij je eigen types het resultaat niet-negatief door
het met `logand` te combineren met `*sxhash-mask*` (2^30-1). Om een string te hashen kun je
`(sxhash-string s)` (32-bits FNV-1a) aanroepen, dat de implementatie voor `string` gebruikt.

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

Of twee sleutels hetzelfde zijn wordt bepaald door **het sleuteltype zelf** (`sxhash`, en `equals` van
`Eq`, de supertrait van `Hash`), niet door objectidentiteit. Daarom kun je, zoals hierboven, opzoeken
met een sleutel die "een andere waarde maar gelijk" is.

Het is prima dat `sxhash` botst (het contract van `Hash` geldt maar in één richting: gelijke waarden
moeten dezelfde hash hebben). Botsende sleutels worden met `equals` uit elkaar gehouden.

## 5. `Array<T>` (meerdimensionale arrays)

Een `defstruct` in de standaardbibliotheek. Het is geen ingebouwd type, dus alles wat met een
`defstruct` kan, kan ermee.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` van CL. `dims` wordt gekopieerd. `init` is de beginwaarde van elke cel (`:initial-element` van CL; deze taal heeft geen "ongebonden cel", dus het is verplicht). `:fill-pointer` alleen voor één dimensie |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` van CL. Geeft een panic als een index buiten het bereik valt |
| `aref` | `(aref a i j …)` | — | De schrijfwijze van CL met kale indexen. Expandeert naar de bovenstaande `get`/`set`. `(setf (aref a i j) v)` werkt ook |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` van CL. Een platte index |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` van CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` van CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` van CL. Geeft een **kopie** terug, net zoals CL een verse lijst teruggeeft |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` van CL (het aantal gealloceerde cellen, los van de fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | `length` van CL op arrays. De fill pointer als die er is, anders `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` van CL. Onwaar (geen fout) ook wanneer het **aantal** indexen verkeerd is |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` van CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` van CL. De rang kan niet veranderen. Elementen die binnen het bereik blijven behouden hun indexen, en nieuwe cellen krijgen `init`. Anders dan in CL geeft het de array niet terug (elke array in deze taal is aanpasbaar, dus er is geen tweede array om terug te geven) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` van CL. Geeft een panic zonder fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` van CL. `none` als leeg |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | De fill pointer (`none` als er geen is). Kan met `(setf a::fill-pointer …)` worden geschreven |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Een iterator in row-major-volgorde. Stopt bij de fill pointer als die er is |

- **Indexen zijn een `Vector<int>`.** Een methode kan niet declareren "hetzelfde type argument
  willekeurig vaak herhaald aan het einde", en de suiker van `aref` overbrugt die kloof.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **bestaan niet**. Het statische type van de ontvanger beantwoordt deze vragen al.
- `Array::new` is de door `defstruct` gegenereerde constructor in veldvolgorde en is niet bedoeld
  om arrays te maken. Gebruik `Array::make`.
- **Arrays worden in de arraysyntaxis van CL afgedrukt.** Rang 1 is `#(1 2 3)`; andere rangen zijn
  `#nA` gevolgd door zoveel niveaus haakjes (`#2A((1 2 3) (4 5 6))`); rang 0 is `#0A5`. Het afdrukken
  stopt bij de fill pointer als die er is. `*print-array*`
  ([Afdrukken](printing.md#6-bepalen-hoeveel-wordt-afgedrukt)) op onwaar zetten drukt alleen de vorm
  af, `#<array 2x3>`. Alleen een array waarvan de elementen een `defstruct` zonder `print-object` zijn,
  wordt in de ingebouwde vorm `#<array<...> ...>` afgedrukt (het is geen fout).

## 6. `BitVector` (bitvectoren)

Een bitreeks met vaste lengte. Een `defstruct` in de standaardbibliotheek.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Lengte `n`, alle bits 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic buiten het bereik |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Schrijfwijzen van CL. `(setf (bit v i) b)` werkt ook. `sbit` van CL verschilt van `bit` alleen doordat het een eenvoudige bitvector vereist, maar deze taal heeft maar één soort bitvector |
| `len` | `(len v)` | `BitVector→int` | Aantal bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Geven een nieuwe bitvector terug. Panic als de lengtes verschillen. Er is geen derde argument zoals in CL (de bestemming van het resultaat) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complement |

Er is geen `bit-vector-p` (het statische type beantwoordt dat).
