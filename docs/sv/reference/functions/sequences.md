<!-- translated-from: docs/ja/reference/functions/sequences.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Par, S-uttryck och sekvenser

Det generiska paret `cons-cell`, S-uttrycksdata `Sexpr`, symboler, sekvensfunktionerna som skrivs ovanpå
`Iter`, och högre ordningens funktioner.

## 1. Par `cons-cell<A,B>`

`cons`/`car`/`cdr` är konstruktorn och fältaccessorerna för den **generiska partypen
`cons-cell<A,B>`** (en `defstruct` i standardbiblioteket). Fälten kan läsas antingen som
`variable::car`/`variable::cdr` (accessorsyntaxen för `defstruct` i
[Syntaxreferensen](../syntax.md#36-defstruct--structs-användardefinierade-typer)) eller som
`(car variable)`/`(cdr variable)`. För att ändra dem används `(setf variable::car v)`/`(setf variable::cdr v)`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Skapar ett par |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Det första elementet |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Resten |

`cons-cell` fungerar också i stället för en tupelsyntax. CL-funktioner som returnerar flera värden
(kvoten och resten från `floor`, värdet och positionen från `read-from-string` och så vidare) returnerar en
`cons-cell` i det här språket.

## 2. S-uttrycksdata `Sexpr`

Datatypen `Sexpr` som `read` returnerar har 18 varianter:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array`.
`vector` och `array` är data skrivna som `#(..)` och `#nA(..)` (se
[syntaxreferensen](../syntax.md#1-lexikaliska-element)) som innehåller en `Vector<Option<Sexpr>>`
respektive en `Array<Option<Sexpr>>`: `len`, `get` och resten fungerar direkt på den `v` som
`(vector v)` binder.
S-uttrycksceller hanteras inte med de allmänna `cons`/`car`/`cdr` i kapitel 1 utan med
`sexpr-*`-funktionerna. De används främst i `defmacro`-kroppar för att bygga och plocka isär former.

**Typen för S-uttrycksdata är `Option<Sexpr>`.** Den tomma listan är inte en variant av `Sexpr` utan
`none` i `Option`, och `Sexpr` självt betyder "ett icke-tomt S-uttryck". Så `sexpr-*`-funktionerna tar
och returnerar `Option<Sexpr>`.

- `()` är den tomma listan där en `Option<Sexpr>` förväntas (den kan också skrivas
  `(Option::none)`)
- `Sexpr` vidgas implicit där en `Option<Sexpr>` förväntas (utan konvertering vid körning). Den motsatta
  riktningen, att använda en `Option<Sexpr>` som en `Sexpr`, påstår "det här är inte den tomma listan",
  så det måste anges explicit med `match` eller `unwrap`
- I `match` kan de 18 varianterna av `Sexpr` och `none` skrivas **platt i samma lista av grenar**
  ([Syntaxreferens](../syntax.md#43-match--mönstermatchning))

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Skapar en `Sexpr`-cell |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Det första elementet. **Den tomma listan för den tomma listan** (som i CL). Ger panic på en atom som inte är en `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Resten. **Den tomma listan för den tomma listan** (som i CL). Ger panic på en atom som inte är en `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Om det är en `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Om det är den tomma listan |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Om det inte är en `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Om det är en `Sym` (symbol) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Innehållet i varianten `int` (fixnum eller bignum). Ger panic på en annan typ |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Innehållet i varianten med den bredden. Ger panic på en annan typ |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Innehållet i flyttalsvarianterna. Ger panic på en annan typ |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Innehållet i en `Char`. Ger panic på en annan typ |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Innehållet i en `Bool`. Ger panic på en annan typ |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Innehållet i en `Str`. Ger panic på en annan typ |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Namnet på en `Sym`. Ger panic på en annan typ |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Identitetsjämförelse (`Cons`/`Str` jämför objektidentitet, resten jämför värden) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Strukturell likhet (`Cons` rekursivt, `Str` efter innehåll) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Som `equal`, plus jämförelse utan hänsyn till skiftläge och jämförelse av tal över typer |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Konkatenerar två `Sexpr`-listor (icke-destruktivt). `,@` expanderar till detta |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | En ny `Sexpr`-lista med `f` tillämpad på varje element i en `Sexpr`-lista (`map` i kapitel 4 är för `Iter` och kan inte gå igenom en `Sexpr`-lista) |

Det finns nio numeriska accessorer, en per typ, eftersom en `Sexpr` är "det enda ställe där ett värdes typ
inte skrivs någon annanstans". En `u8` som läggs i en `Sexpr` går in som varianten `u8` och kommer bara ut
med `(sexpr-u8 s)`. Att skicka den till `(sexpr-int s)` ger panic; det vidgar aldrig svaret tyst. Heltalen i
data som lästs (`'(1 2 3)`, makroargument) är av varianten `int` och läses med `(sexpr-int s)`.

`Sexpr`-listor har inga destruktiva operationer som `rplaca`/`nconc`. En `Sexpr`-cell kan inte ändras
efter att den skapats.

## 3. Symboler

`symbol` är typen för själva symbolerna. Den konverteras implicit där en `Sexpr` krävs, men inte
automatiskt i den andra riktningen.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Tar ut symbolens namn |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Skapar en symbol från en sträng (internerar den) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Om det är ett keyword (`:name`). Kolonet är en del av namnet, så testet tittar på det första tecknet ([Syntaxreferens](../syntax.md#1-lexikaliska-element)) |

För `gensym`, se [Makron](system.md#8-makron).

## 4. Sekvensfunktioner på `Iter`

Sekvensfunktionerna är **generiska funktioner över traitet `Iter`**. Från en samling får man en iterator
med `(iter coll)` och skickar den (`Vector<T>` / `HashTable<K,V>` / `Array<T>` stöder detta; en
`Sexpr`-lista implementerar inte `Iter`, så dessa funktioner gäller inte den). **En resulterande samling
returneras som en ny `Vector`.** `Iter<A>` i tabellerna betyder "vilken implementation av `Iter` som helst
vars `Item` är `A`". För att gå igenom den returnerade `Vector` igen skickar man `(iter result)`.

Funktioner som tar ett predikat (motsvarar CL:s `-if`-familj):

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Avbildning |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Bara de element som uppfyller predikatet |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Tar bort de element som uppfyller predikatet |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Det första element som uppfyller predikatet |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Den första position som uppfyller predikatet |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Hur många som uppfyller predikatet |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Om varje element uppfyller predikatet |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Om något element uppfyller predikatet (motsvarar CL:s `some`; ett namn som inte krockar med konstruktorn `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Vänstervikning |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Högervikning |

Indexering, längd och delning:

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Antal element |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Konkatenerar iteratorer. Tre eller fler kan anges |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL:s `concatenate`. Resultattypen skrivs som **en citerad symbolliteral** (CL använder en typspecificerare vid körning). `'vector` tar en eller fler, `'string` noll eller fler (`""` för noll). `Sexpr`-listor omfattas inte (använd `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Omvändning (icke-destruktiv) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Element `n` (`None` utanför intervallet) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` med argumenten tvärtom |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | De första `n` elementen |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` begränsas till längden) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Det sista **elementet** (inte "den sista cellen" som i CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Allt utom det sista elementet |

Funktioner som kräver en gräns `Eq` / `Ord` (de jämför genom ett trait i stället för ett predikat;
[Standardtraits](traits.md#2-eq--ord-jämförelse)):

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Om ett element lika med `x` finns (till skillnad från CL en `bool`, inte resten av listan) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Det första element som är lika med `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Den första position som är lika med `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Hur många element som är lika med `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL:s `(sort sequence predicate)`. En stabil, icke-destruktiv sortering. `cmp` är `true` när "det första argumentet kommer strikt före det andra" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Det första par vars `car` är lika med `k`. Ta ut värdet med `(cdr p)` |

Dessa och många av funktionerna i kapitel 5 tar också CL:s nyckelordsargument `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (kapitel 6).

## 5. Resten av CL:s sekvensfunktioner

Alla är generiska funktioner på `Iter`, som i kapitel 4. Resulterande samlingar returneras som nya
`Vector`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL:s namngivna index |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Allt utom det första (en ny `Vector`, inte en delad svans) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materialiserar en iterator till en `Vector` (CL:s `copy-seq`/`copy-list`) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` omvänd, följd av `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopior av `x` (CL:s `make-list`/`make-sequence`). Som med `Vector::new` kommer typargumentet från den förväntade typen, så ett bart `let` behöver `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Som `member`, en **`bool`** (en iterator har ingen svans att returnera) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Negationerna av `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Samma typer som de positiva versionerna | Versioner med negerat predikat |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Tar bort efter värde |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Tar bort dubbletter. Som i CL **behålls den sista förekomsten** (`:from-end true` behåller den första) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Ersätter efter värde / predikat |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | på `Iter<cons-cell<K,V>>` | Predikatversionen och värdesidans versioner av `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Lägger till ett par först |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Parar ihop två sekvenser. Stannar vid den kortare |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL:s `mapcar` över flera sekvenser. Stannar vid den kortare |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Avbildning för sidoeffekter |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Avbildar och konkatenerar |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Avbildar över på varandra följande **svansar** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Avbildar över svansar för sidoeffekter (`maplist`-motsvarigheten till `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Avbildar över svansar och konkatenerar (`maplist`-motsvarigheten till `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Positionen där `sub` först förekommer. Om mottagaren är en `string` väljs `string`-metoden ([Strängar](collections.md#1-strängar-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Den första position där de skiljer sig. `none` om de är lika |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Sammanslagning. CL kräver sorterade indata; den här sorterar konkateneringen |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Lägger till `x` **först** om det inte finns där |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Mängdoperationer. CL specificerar inte ordningen; här är den stabil, **i ordning efter första förekomst** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inklusion |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Om det är ett suffix / delen före suffixet. CL frågar om **delad struktur**, men det finns ingen struktur att dela, så den här frågar om ett suffix **som värden** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Likhet element för element. `Vector<T>` implementerar inte själv `Eq` |
| `caar`…`cddddr` | `(cadr p)` | på nästlade par | CL:s 28 funktioner. De går igenom **par, inte listor**: `cadr` tar en `cons-cell<A,cons-cell<B,C>>` |

Vad CL har och det här språket inte har: `list*` (det finns ingen föreställning om en oäkta lista vars
svans ersatts), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (ingen typ kan beskriva att gå igenom
ett heterogent träd av godtyckligt djup; för ett träd av `Sexpr` motsvarar `equal` `tree-equal`),
egenskapslistefamiljen `getf`/`get-properties`/`symbol-plist`/`remprop` (det finns ingen representation som
en otypad lista som växlar mellan nycklar och värden; `assoc` (associationslistor) eller `HashTable` fyller
samma roll), och funktioner som konverterar mellan `Vector<T>` och `Sexpr`-listor (elementen i en
`Sexpr`-lista kan ha olika typ var och en, så de kan inte skrivas med en enda elementtyp `T`).

## 6. Nyckelordsargument

Funktionerna i kapitel 4 och 5 tar CL:s sekvensnyckelord `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Alla är **valfria**.

| Nyckelord | Typ | Betydelse |
|---|---|---|
| `:key` | `(fn (A) A)` | En projektion som tillämpas på varje element före jämförelse eller test |
| `:test` | `(fn (A A) bool)` | Ett likhetstest som används i stället för `equals` från gränsen `Eq`. Det första argumentet är **det som söks**, det andra är elementet (efter `:key`), i samma ordning som CL |
| `:test-not` | `(fn (A A) bool)` | Negationen av `:test` |
| `:start` `:end` | `int` | Fönstret `[start, end)` att genomsöka. Index är relativa till hela sekvensen |
| `:from-end` | `bool` | En sökning svarar med den **sista** träffen. Kombinerat med `:count` tas de berörda elementen från slutet |
| `:count` | `int` | Det högsta antal element som familjerna `remove` / `substitute` påverkar |

Vilken funktion som tar vilka följer CL:

| Funktion | Nyckelord som tas |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Alla ovanstående (inklusive `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` för `assoc` tillämpas på `car`, den för `rassoc` på `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; tar bara bort ett, från slutet
(position 3 (iter v) :start 1)                          ; indexet är relativt hela sekvensen
```

**Skillnader mot CL**:

1. **Projektionen i `:key` stannar inom elementtypen** (`(fn (A) A)`). Den kan inte projicera till en
   annan typ som i CL: en extra typvariabel kunde inte avgöras när argumentet utelämnas. Där en projektion
   till en annan typ behövs, skicka i stället en lambda till `-if`-familjen
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Vid sökningar efter ett föremål gäller `:key` bara elementen** (inte det som söks). Det är samma
   regel som CL:s `find`/`position`/`count`/`member`/`remove`/`substitute`. Vid mängdoperationer är båda
   sidor element, så den gäller båda.
3. **Bara nyckelorden för `search` är namngivna i stället för numrerade.** I CL är `:start1`/`:end1` för
   **mönstret** och `:start2`/`:end2` för sekvensen som genomsöks. I det här språket kommer mottagaren
   först, så samma nummer skulle betyda tvärtom, och dessutom tyst. `:start`/`:end` är för mottagaren och
   `:sub-start`/`:sub-end` för mönstret, så ett tankspritt `:start1` ger felet "unknown keyword".
   `mismatch` och `replace` har samma argumentordning som CL, så de behåller CL:s nummer.

## 7. Destruktiva operationer

Metoder för `Vector<T>`. **De ändrar mottagaren och returnerar mottagaren själv**, så `(nreverse v)`
skrivs på samma sätt som `reverse` och `v` självt blir också omvänd.

| Namn | Form | Beskrivning |
|---|---|---|
| `nreverse` | `(nreverse v)` | Vänder på plats |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versioner på plats av `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versioner på plats av familjen `substitute` |
| `nbutlast` | `(nbutlast v)` | Tar bort det sista elementet |
| `fill` | `(fill v x)` | Sätter varje element till `x`. Längden ändras inte |
| `replace` | `(replace v src)` | Skriver över från början med elementen i `src`. `(min (len v) (len src))` element |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Samma antal som ovan |
| `nconc` | `(nconc v w)` | Lägger elementen i `w` sist i `v`. Till skillnad från CL **skriver den inte om delad struktur** (`w` påverkas inte) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Ersätter innehållet i `v` med `src` (längden ändras också) |
| `rplaca` `rplacd` | `(rplaca p x)` | Skriver om `car`/`cdr` i en `cons-cell` och returnerar cellen själv |

Nyckelord som tas:

| Destruktiv version | Nyckelord som tas |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (mottagaren är CL:s `sequence-1`) |

`vector-push-extend`/`vector-pop` är helt enkelt `push`/`pop` för `Vector<T>`. En `Vector<T>` växer
alltid, så ingenting motsvarar CL:s skillnad mellan "en vektor med fyllpekare" och "en enkel vektor".

## 8. Högre ordningens funktioner

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Returnerar sitt argument |
| `const` | `(const x y)` | `(A,B)→A` | Returnerar det första argumentet |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Funktionskomposition `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Byter plats på argumenten i en funktion med två argument |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negation av ett predikat |

Det finns ingen CL-`constantly` (typen på det ignorerade argumentet skulle bara förekomma i returtypen och
kunde inte avgöras). Skriv `(lambda ((x T)) A v)`.
