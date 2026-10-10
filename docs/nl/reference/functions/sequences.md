<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Paren, S-expressies en sequenties

Het generieke paar `cons-cell`, S-expressiedata `Sexpr`, symbolen, de sequentiefuncties die bovenop
`Iter` zijn geschreven, en functies van hogere orde.

## 1. Paren `cons-cell<A,B>`

`cons`/`car`/`cdr` zijn de constructor en de veldaccessors van het **generieke paartype
`cons-cell<A,B>`** (een `defstruct` in de standaardbibliotheek). De velden kunnen worden gelezen als
`variable::car`/`variable::cdr` (de `defstruct`-accessorsyntaxis van de
[Syntaxreferentie](../syntax.md#36-defstruct--structs-door-de-gebruiker-gedefinieerde-types)) of als
`(car variable)`/`(cdr variable)`. Om ze te wijzigen gebruik je
`(setf variable::car v)`/`(setf variable::cdr v)`.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Maakt een paar |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Het eerste element |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | De rest |

`cons-cell` dient ook in plaats van een tuple-syntaxis. CL-functies die meerdere waarden teruggeven
(het quotiënt en de rest van `floor`, de waarde en positie van `read-from-string` enzovoort) geven in
deze taal een `cons-cell` terug.

## 2. S-expressiedata `Sexpr`

Het gegevenstype `Sexpr` dat `read` teruggeeft heeft 19 varianten:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` en `array` zijn data die als `#(..)` en `#nA(..)` geschreven zijn
([syntaxreferentie](../syntax.md#1-lexicale-elementen)) en een `Vector<Option<Sexpr>>`
respectievelijk een `Array<Option<Sexpr>>` bevatten: `len`, `get` enzovoort werken direct op de `v`
die `(vector v)` bindt.
`tuple` zijn data geschreven met `#{..}`, en de `v` die `(tuple v)` bindt is een nieuwe
`Vector<Option<Sexpr>>` van de elementen (zodat een tuple van elke lengte met één type wordt
ontvangen).
S-expressiecellen worden niet met de algemene `cons`/`car`/`cdr` van hoofdstuk 1 behandeld maar met de
`sexpr-*`-functies. Ze worden vooral in `defmacro`-bodies gebruikt om vormen te bouwen en uit elkaar
te halen.

**Het type van S-expressiedata is `Option<Sexpr>`.** De lege lijst is geen variant van `Sexpr` maar de
`none` van `Option`, en `Sexpr` zelf betekent "een niet-lege S-expressie". De `sexpr-*`-functies
nemen en geven dus `Option<Sexpr>`.

- `()` is de lege lijst waar een `Option<Sexpr>` wordt verwacht (het kan ook als `(Option::none)`
  worden geschreven)
- `Sexpr` verbreedt impliciet waar een `Option<Sexpr>` wordt verwacht (zonder runtimeconversie). De
  omgekeerde richting, een `Option<Sexpr>` als `Sexpr` gebruiken, beweert "dit is niet de lege lijst",
  dus dat moet expliciet met `match` of `unwrap` worden gesteld
- In `match` kunnen de 19 varianten van `Sexpr` en `none` **plat in dezelfde lijst takken** worden
  geschreven ([Syntaxreferentie](../syntax.md#43-match--patroonherkenning))

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Maakt een `Sexpr`-cel |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Het eerste element. **De lege lijst voor de lege lijst** (zoals in CL). Geeft een panic bij een atoom dat geen `Cons` is |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | De rest. **De lege lijst voor de lege lijst** (zoals in CL). Geeft een panic bij een atoom dat geen `Cons` is |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Of het een `Cons` is |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Of het de lege lijst is |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Of het geen `Cons` is |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Of het een `Sym` (symbool) is |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | De inhoud van de variant `int` (fixnum of bignum). Geeft een panic bij een ander type |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | De inhoud van de variant met die breedte. Geeft een panic bij een ander type |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | De inhoud van de drijvendekommavarianten. Geeft een panic bij een ander type |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | De inhoud van een `Char`. Geeft een panic bij een ander type |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | De inhoud van een `Bool`. Geeft een panic bij een ander type |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | De inhoud van een `Str`. Geeft een panic bij een ander type |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | De naam van een `Sym`. Geeft een panic bij een ander type |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Identiteitsvergelijking (`Cons`/`Str` vergelijken objectidentiteit, de rest vergelijkt waarden) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Structurele gelijkheid (`Cons` recursief, `Str` op inhoud) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Zoals `equal`, plus niet hoofdlettergevoelige vergelijking en vergelijking van getallen over types heen |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Voegt twee `Sexpr`-lijsten samen (niet-destructief). `,@` expandeert hiernaartoe |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Een nieuwe `Sexpr`-lijst met `f` toegepast op elk element van een `Sexpr`-lijst (de `map` van hoofdstuk 4 is voor `Iter` en kan een `Sexpr`-lijst niet doorlopen) |

Er zijn negen numerieke accessors, een per type, omdat een `Sexpr` "de ene plek is waar het type van
een waarde nergens anders wordt geschreven". Een `u8` die in een `Sexpr` wordt gezet gaat erin als de
variant `u8` en komt er alleen met `(sexpr-u8 s)` uit. Hem aan `(sexpr-int s)` doorgeven geeft een
panic; het verbreedt het antwoord nooit stilzwijgend. De gehele getallen in gelezen data (`'(1 2 3)`,
macroargumenten) zijn van de variant `int` en worden met `(sexpr-int s)` gelezen.

`Sexpr`-lijsten hebben geen destructieve bewerkingen zoals `rplaca`/`nconc`. Een `Sexpr`-cel kan na
het maken niet worden gewijzigd.

## 3. Symbolen

`symbol` is het type van symbolen zelf. Het converteert impliciet waar een `Sexpr` wordt vereist,
maar niet automatisch in de andere richting.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Haalt de naam van het symbool eruit |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Maakt een symbool uit een string (internt het) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Of het een keyword (`:name`) is. De dubbele punt maakt deel uit van de naam, dus de test kijkt naar het eerste teken ([Syntaxreferentie](../syntax.md#1-lexicale-elementen)) |

Voor `gensym` zie [Macro's](system.md#8-macros).

## 4. Sequentiefuncties op `Iter`

De sequentiefuncties zijn **generieke functies over de trait `Iter`**. Haal uit een collectie een
iterator met `(iter coll)` en geef die door (`Vector<T>` / `HashTable<K,V>` / `Array<T>` ondersteunen
dit; een `Sexpr`-lijst implementeert `Iter` niet, dus deze functies zijn er niet op van toepassing).
**Een resulterende collectie wordt als nieuwe `Vector` teruggegeven.** `Iter<A>` in de tabellen
betekent "elke implementatie van `Iter` waarvan `Item` gelijk is aan `A`". Om de teruggegeven `Vector`
opnieuw te doorlopen, geef je `(iter result)` door.

Functies die een predicaat nemen (overeenkomend met de `-if`-familie van CL):

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Afbeelden |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Alleen de elementen die aan het predicaat voldoen |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Verwijdert de elementen die aan het predicaat voldoen |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Het eerste element dat aan het predicaat voldoet |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | De eerste positie die aan het predicaat voldoet |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Hoeveel aan het predicaat voldoen |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Of elk element aan het predicaat voldoet |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Of enig element aan het predicaat voldoet (komt overeen met `some` van CL; een naam die niet botst met de constructor `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Linkse vouwing |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Rechtse vouwing |

Indexeren, lengte en uitsnijden:

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Aantal elementen |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Voegt iterators samen. Er kunnen er drie of meer worden gegeven |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` van CL. Het resultaattype wordt geschreven als **een gequote symboolliteral** (CL gebruikt een runtime-typespecificatie). `'vector` neemt een of meer, `'string` nul of meer (`""` bij nul). `Sexpr`-lijsten worden niet gedekt (gebruik `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Omkeren (niet-destructief) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Element `n` (`None` buiten het bereik) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` met de argumenten andersom |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | De eerste `n` elementen |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` wordt tot de lengte beperkt) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Het laatste **element** (niet "de laatste cel" zoals in CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Alles behalve het laatste element |

Functies die een `Eq`- / `Ord`-bound vereisen (ze vergelijken via een trait in plaats van via een
predicaat; [Standaardtraits](traits.md#2-eq--ord-vergelijking)):

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Of er een element gelijk aan `x` bestaat (anders dan in CL een `bool`, niet de rest van de lijst) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Het eerste element gelijk aan `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | De eerste positie gelijk aan `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Hoeveel elementen gelijk zijn aan `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` van CL. Een stabiele, niet-destructieve sortering. `cmp` is `true` wanneer "het eerste argument strikt vóór het tweede komt" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Het eerste paar waarvan de `car` gelijk is aan `k`. Haal de waarde eruit met `(cdr p)` |

Deze en veel van de functies uit hoofdstuk 5 nemen ook de keyword-argumenten van CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (hoofdstuk 6).

## 5. De rest van de sequentiefuncties van CL

Allemaal generieke functies op `Iter`, zoals in hoofdstuk 4. Resulterende collecties worden als nieuwe
`Vector`s teruggegeven.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | De benoemde indexen van CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Alles behalve het eerste (een nieuwe `Vector`, geen gedeelde staart) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materialiseert een iterator tot een `Vector` (`copy-seq`/`copy-list` van CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` omgekeerd, gevolgd door `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopieën van `x` (`make-list`/`make-sequence` van CL). Net als bij `Vector::new` komt het typeargument uit het verwachte type, dus een kale `let` heeft `the` nodig |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Zoals `member` een **`bool`** (een iterator heeft geen staart om terug te geven) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | De ontkenningen van `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Dezelfde types als de positieve versies | Versies met het predicaat ontkend |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Verwijdert op waarde |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Verwijdert duplicaten. Net als in CL **blijft het laatste voorkomen behouden** (`:from-end true` behoudt het eerste) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Vervangt op waarde / predicaat |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | op `Iter<cons-cell<K,V>>` | De predicaat- en waardezijdeversies van `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Voegt een paar vooraan toe |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Koppelt twee sequenties. Stopt bij de kortste |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` van CL over meerdere sequenties. Stopt bij de kortste |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Afbeelden voor neveneffecten |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Beeldt af en voegt samen |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Beeldt af over opeenvolgende **staarten** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Beeldt af over staarten voor neveneffecten (de `maplist`-tegenhanger van `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Beeldt af over staarten en voegt samen (de `maplist`-tegenhanger van `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | De positie waar `sub` het eerst voorkomt. Is de ontvanger een `string`, dan wordt de `string`-methode gekozen ([Strings](collections.md#1-strings-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | De eerste positie waar ze verschillen. `none` als ze gelijk zijn |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Samenvoegen. CL vereist gesorteerde invoer; dit sorteert de aaneenschakeling |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Voegt `x` **vooraan** toe als het er niet is |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Verzamelingsbewerkingen. CL specificeert de volgorde niet; hier is ze stabiel, **in volgorde van eerste voorkomen** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Insluiting |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Of het een suffix is / het deel vóór de suffix. CL vraagt naar **gedeelde structuur**, maar er is geen structuur om te delen, dus dit vraagt naar een suffix **als waarden** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Elementgewijze gelijkheid. `Vector<T>` zelf implementeert `Eq` niet |
| `caar`…`cddddr` | `(cadr p)` | op geneste paren | De 28 functies van CL. Ze doorlopen **paren, geen lijsten**: `cadr` neemt een `cons-cell<A,cons-cell<B,C>>` |

Wat CL heeft en deze taal niet: `list*` (er is geen begrip van een onechte lijst waarvan de staart is
vervangen), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (geen type kan het doorlopen van een
heterogene boom van willekeurige diepte beschrijven; voor een boom van `Sexpr` komt `equal` overeen
met `tree-equal`), de familie van eigenschappenlijsten
`getf`/`get-properties`/`symbol-plist`/`remprop` (er is geen representatie als ongetypeerde lijst die
sleutels en waarden afwisselt; `assoc` (associatielijsten) of `HashTable` vervullen dezelfde rol), en
functies die tussen `Vector<T>` en `Sexpr`-lijsten converteren (de elementen van een `Sexpr`-lijst
kunnen elk een ander type hebben, dus ze kunnen niet met één elementtype `T` worden geschreven).

## 6. Keyword-argumenten

De functies van hoofdstuk 4 en 5 nemen de sequentiekeywords van CL `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Alle zijn **optioneel**.

| Keyword | Type | Betekenis |
|---|---|---|
| `:key` | `(fn (A) A)` | Een projectie die op elk element wordt toegepast vóór het vergelijken of testen |
| `:test` | `(fn (A A) bool)` | Een gelijkheidstest die wordt gebruikt in plaats van `equals` uit de `Eq`-bound. Het eerste argument is **het item waarnaar wordt gezocht**, het tweede is het element (na `:key`), in dezelfde volgorde als in CL |
| `:test-not` | `(fn (A A) bool)` | De ontkenning van `:test` |
| `:start` `:end` | `int` | Het venster `[start, end)` dat wordt gescand. Indexen zijn relatief aan de hele sequentie |
| `:from-end` | `bool` | Een zoekopdracht antwoordt met de **laatste** match. Gecombineerd met `:count` worden de getroffen elementen van het einde af genomen |
| `:count` | `int` | Het maximale aantal elementen dat de families `remove` / `substitute` treffen |

Welke functie welke neemt volgt CL:

| Functie | Genomen keywords |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Alle bovenstaande (inclusief `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (de `:key` van `assoc` geldt voor de `car`, die van `rassoc` voor de `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; removes only one, from the end
(position 3 (iter v) :start 1)                          ; the index is relative to the whole sequence
```

**Verschillen met CL**:

1. **De projectie van `:key` blijft binnen het elementtype** (`(fn (A) A)`). Ze kan niet naar een ander
   type projecteren zoals in CL: een extra typevariabele zou niet kunnen worden bepaald wanneer het
   argument wordt weggelaten. Waar een projectie naar een ander type nodig is, geef je in plaats daarvan
   een lambda aan de `-if`-familie (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Bij zoekopdrachten op item geldt `:key` alleen voor de elementen** (niet voor het gezochte item).
   Dit is dezelfde regel als bij `find`/`position`/`count`/`member`/`remove`/`substitute` van CL. Bij
   verzamelingsbewerkingen zijn beide zijden elementen, dus geldt hij voor beide.
3. **Alleen de keywords van `search` zijn benoemd in plaats van genummerd.** In CL zijn
   `:start1`/`:end1` voor het **patroon** en `:start2`/`:end2` voor de doorzochte sequentie. In deze
   taal komt de ontvanger eerst, dus dezelfde nummers zouden het tegenovergestelde betekenen, en dat
   nog stilzwijgend ook. `:start`/`:end` zijn voor de ontvanger en `:sub-start`/`:sub-end` voor het
   patroon, dus een verstrooide `:start1` geeft een fout "unknown keyword". `mismatch` en `replace`
   hebben dezelfde argumentvolgorde als CL, dus ze behouden de nummers van CL.

## 7. Destructieve bewerkingen

Methoden van `Vector<T>`. **Ze wijzigen de ontvanger en geven de ontvanger zelf terug**, dus
`(nreverse v)` wordt op dezelfde manier geschreven als `reverse` en `v` zelf wordt ook omgekeerd.

| Naam | Vorm | Beschrijving |
|---|---|---|
| `nreverse` | `(nreverse v)` | Keert ter plekke om |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Ter-plekke-versies van `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Ter-plekke-versies van de familie `substitute` |
| `nbutlast` | `(nbutlast v)` | Laat het laatste element vallen |
| `fill` | `(fill v x)` | Zet elk element op `x`. De lengte verandert niet |
| `replace` | `(replace v src)` | Overschrijft vanaf het begin met de elementen van `src`. `(min (len v) (len src))` elementen |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Zelfde aantal als hierboven |
| `nconc` | `(nconc v w)` | Voegt de elementen van `w` aan `v` toe. Anders dan in CL **herschrijft het geen gedeelde structuur** (`w` wordt niet beïnvloed) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Vervangt de inhoud van `v` door `src` (de lengte verandert ook) |
| `rplaca` `rplacd` | `(rplaca p x)` | Herschrijft de `car`/`cdr` van een `cons-cell` en geeft de cel zelf terug |

Genomen keywords:

| Destructieve versie | Genomen keywords |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (de ontvanger is `sequence-1` van CL) |

`vector-push-extend`/`vector-pop` zijn gewoon `push`/`pop` van `Vector<T>`. Een `Vector<T>` groeit
altijd, dus er is niets dat overeenkomt met het onderscheid van CL tussen "een vector met een fill
pointer" en "een eenvoudige vector".

## 8. Functies van hogere orde

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Geeft zijn argument terug |
| `const` | `(const x y)` | `(A,B)→A` | Geeft het eerste argument terug |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Functiecompositie `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Verwisselt de argumenten van een functie met twee argumenten |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Ontkenning van een predicaat |

Er is geen `constantly` van CL (het type van het genegeerde argument zou alleen in het returntype
voorkomen en niet kunnen worden bepaald). Schrijf `(lambda ((x T)) A v)`.
