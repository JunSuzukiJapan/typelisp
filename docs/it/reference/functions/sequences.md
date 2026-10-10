<!-- translated-from: docs/ja/reference/functions/sequences.md @ eae672c1a271f0b6947f024e81dee8338b2f5ff4 -->
# Coppie, S-expression e sequenze

La coppia generica `cons-cell`, i dati S-expression `Sexpr`, i simboli, le funzioni sulle sequenze
scritte sopra `Iter` e le funzioni di ordine superiore.

## 1. Coppie `cons-cell<A,B>`

`cons`/`car`/`cdr` sono il costruttore e gli accessori dei campi del **tipo coppia generico
`cons-cell<A,B>`** (una `defstruct` della libreria standard). I campi si possono leggere sia come
`variable::car`/`variable::cdr` (la sintassi degli accessori di `defstruct` del
[Riferimento della sintassi](../syntax.md#36-defstruct--strutture-tipi-definiti-dallutente)) sia come
`(car variable)`/`(cdr variable)`. Per modificarli, usa `(setf variable::car v)`/`(setf variable::cdr v)`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Crea una coppia |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Il primo elemento |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Il resto |

`cons-cell` serve anche al posto di una sintassi per le tuple. Le funzioni CL che restituiscono più
valori (il quoziente e il resto di `floor`, il valore e la posizione di `read-from-string` e così via) in
questo linguaggio restituiscono una `cons-cell`.

## 2. Dati S-expression `Sexpr`

Il tipo di dati `Sexpr` restituito da `read` ha 19 varianti:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` e `array` sono dati scritti come `#(..)` e `#nA(..)` ([riferimento della
sintassi](../syntax.md#1-elementi-lessicali)), che contengono rispettivamente un
`Vector<Option<Sexpr>>` e un `Array<Option<Sexpr>>`: `len`, `get` e il resto funzionano direttamente
sulla `v` legata da `(vector v)`.
`tuple` sono dati scritti con `#{..}`, e la `v` legata da `(tuple v)` è un nuovo
`Vector<Option<Sexpr>>` degli elementi (così che una tupla di qualsiasi lunghezza si riceva con un
solo tipo).
Le celle S-expression non vengono trattate dai generali `cons`/`car`/`cdr` del capitolo 1 ma dalle
funzioni `sexpr-*`. Si usano soprattutto nei corpi di `defmacro` per costruire e scomporre le forme.

**Il tipo dei dati S-expression è `Option<Sexpr>`.** La lista vuota non è una variante di `Sexpr` ma il
`none` di `Option`, e `Sexpr` stesso significa "una S-expression non vuota". Quindi le funzioni
`sexpr-*` prendono e restituiscono `Option<Sexpr>`.

- `()` è la lista vuota dove è atteso un `Option<Sexpr>` (si può scrivere anche `(Option::none)`)
- `Sexpr` si allarga implicitamente dove è atteso un `Option<Sexpr>` (senza conversione a runtime). La
  direzione opposta, usare un `Option<Sexpr>` come `Sexpr`, afferma "questa non è la lista vuota",
  quindi va dichiarata esplicitamente con `match` o `unwrap`
- In `match`, le 19 varianti di `Sexpr` e `none` si possono scrivere **piatte nella stessa lista di
  rami** ([Riferimento della sintassi](../syntax.md#43-match--pattern-matching))

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Crea una cella `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Il primo elemento. **La lista vuota per la lista vuota** (come in CL). Va in panic su un atomo che non è un `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Il resto. **La lista vuota per la lista vuota** (come in CL). Va in panic su un atomo che non è un `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Se è un `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Se è la lista vuota |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Se non è un `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Se è un `Sym` (simbolo) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Il contenuto della variante `int` (fixnum o bignum). Va in panic su un altro tipo |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Il contenuto della variante di quella larghezza. Va in panic su un altro tipo |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Il contenuto delle varianti in virgola mobile. Va in panic su un altro tipo |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Il contenuto di un `Char`. Va in panic su un altro tipo |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Il contenuto di un `Bool`. Va in panic su un altro tipo |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Il contenuto di una `Str`. Va in panic su un altro tipo |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Il nome di un `Sym`. Va in panic su un altro tipo |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Confronto di identità (`Cons`/`Str` confrontano l'identità dell'oggetto, il resto confronta i valori) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Uguaglianza strutturale (`Cons` ricorsivamente, `Str` per contenuto) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Come `equal`, più il confronto senza distinzione di maiuscole e il confronto di numeri tra tipi diversi |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Concatena due liste `Sexpr` (in modo non distruttivo). `,@` si espande in questa |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Una nuova lista `Sexpr` con `f` applicata a ogni elemento di una lista `Sexpr` (il `map` del capitolo 4 è per `Iter` e non può percorrere una lista `Sexpr`) |

Ci sono nove accessori numerici, uno per tipo, perché una `Sexpr` è "l'unico posto in cui il tipo di un
valore non è scritto altrove". Un `u8` messo in una `Sexpr` vi entra come variante `u8` e ne esce solo
con `(sexpr-u8 s)`. Passarlo a `(sexpr-int s)` va in panic; non allarga mai in silenzio la risposta. Gli
interi nei dati letti (`'(1 2 3)`, argomenti di macro) sono della variante `int` e si leggono con
`(sexpr-int s)`.

Le liste `Sexpr` non hanno operazioni distruttive come `rplaca`/`nconc`. Una cella `Sexpr` non può essere
modificata dopo la sua creazione.

## 3. Simboli

`symbol` è il tipo dei simboli stessi. Si converte implicitamente dove è richiesta una `Sexpr`, ma non
automaticamente nell'altra direzione.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Estrae il nome del simbolo |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Crea un simbolo da una stringa (lo interna) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Se è una parola chiave (`:name`). I due punti fanno parte del nome, quindi il test guarda il primo carattere ([Riferimento della sintassi](../syntax.md#1-elementi-lessicali)) |

Per `gensym`, si veda [Macro](system.md#8-macro).

## 4. Funzioni sulle sequenze con `Iter`

Le funzioni sulle sequenze sono **funzioni generiche sul trait `Iter`**. Da una collezione, ottieni un
iteratore con `(iter coll)` e passalo (`Vector<T>` / `HashTable<K,V>` / `Array<T>` lo supportano; una
lista `Sexpr` non implementa `Iter`, quindi queste funzioni non si applicano ad essa). **Una collezione
risultante viene restituita come nuovo `Vector`.** `Iter<A>` nelle tabelle significa "qualsiasi
implementazione di `Iter` il cui `Item` sia `A`". Per percorrere di nuovo il `Vector` restituito, passa
`(iter result)`.

Funzioni che prendono un predicato (corrispondenti alla famiglia `-if` di CL):

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Mappatura |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Solo gli elementi che soddisfano il predicato |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Rimuove gli elementi che soddisfano il predicato |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Il primo elemento che soddisfa il predicato |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | La prima posizione che soddisfa il predicato |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Quanti soddisfano il predicato |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Se ogni elemento soddisfa il predicato |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Se un elemento qualsiasi soddisfa il predicato (corrisponde a `some` di CL; un nome che non confligge con il costruttore `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Fold a sinistra |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Fold a destra |
| `collect` | `(collect it)` | `Iter<A>→Vector<A>` | Raccoglie tutti gli elementi rimanenti. Serve a trasformare in `Vector` il risultato di una funzione `lazy` qui sotto |

Indicizzazione, lunghezza e porzioni:

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Numero di elementi |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Concatena iteratori. Se ne possono dare tre o più |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | Il `concatenate` di CL. Il tipo del risultato si scrive come **letterale simbolo con apice** (CL usa uno specificatore di tipo a runtime). `'vector` ne prende uno o più, `'string` zero o più (`""` per zero). Le liste `Sexpr` non sono coperte (usa `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Inversione (non distruttiva) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Elemento `n` (`None` fuori intervallo) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` con gli argomenti nell'ordine opposto |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | I primi `n` elementi |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` viene limitato alla lunghezza) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | L'ultimo **elemento** (non "l'ultima cella" come in CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Tutti tranne l'ultimo elemento |

Funzioni che richiedono un vincolo `Eq` / `Ord` (confrontano attraverso un trait invece che con un
predicato; [Trait standard](traits.md#2-eq--ord-confronto)):

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Se esiste un elemento uguale a `x` (a differenza di CL, un `bool`, non il resto della lista) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Il primo elemento uguale a `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | La prima posizione uguale a `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Quanti elementi sono uguali a `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | Il `(sort sequence predicate)` di CL. Un ordinamento stabile e non distruttivo. `cmp` è `true` quando "il primo argomento viene strettamente prima del secondo" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | La prima coppia il cui `car` è uguale a `k`. Estrai il valore con `(cdr p)` |

Queste e molte delle funzioni del capitolo 5 accettano anche gli argomenti con parola chiave di CL
`:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count` (capitolo 6).

### Iteratori pigri (il modulo `lazy`)

Le funzioni del modulo `lazy` non costruiscono un `Vector`: **restituiscono un iteratore**. Un
elemento viene calcolato solo quando si chiede il successivo, quindi anche un iteratore senza fine
(`iterate`, `repeat`) è utilizzabile purché un `take` o un `take-while` a valle lo fermi. Ogni
risultato implementa `Iter`, perciò le funzioni `lazy` si annidano e le funzioni delle tabelle sopra
le accettano così come sono. `collect` lo trasforma in un `Vector`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `lazy::map` | `(lazy::map it f)` | `(Iter<A>,(fn (A) U))→Iter<U>` | Applica `f` a ogni elemento |
| `lazy::filter` | `(lazy::filter it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Solo gli elementi che soddisfano la condizione |
| `lazy::take` | `(lazy::take it n)` | `(Iter<A>,int)→Iter<A>` | I primi `n` |
| `lazy::take-while` | `(lazy::take-while it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Fino al primo elemento che non soddisfa la condizione, escluso |
| `lazy::skip` | `(lazy::skip it n)` | `(Iter<A>,int)→Iter<A>` | Salta i primi `n` |
| `lazy::enumerate` | `(lazy::enumerate it)` | `Iter<A>→Iter<#{int A}>` | Coppie della posizione, contata da 0, e dell'elemento |
| `lazy::zip` | `(lazy::zip a b)` | `(Iter<A>,Iter<B>)→Iter<#{A B}>` | Coppie prendendo un elemento da ciascun lato. Finisce con il più corto |
| `lazy::chain` | `(lazy::chain a b)` | `(Iter<A>,Iter<A>)→Iter<A>` | Gli elementi di `a`, poi quelli di `b` |
| `lazy::flat-map` | `(lazy::flat-map it f)` | `(Iter<A>,(fn (A) Iter<B>))→Iter<B>` | Trasforma ogni elemento in un iteratore con `f` e li concatena in ordine |
| `lazy::iterate` | `(lazy::iterate x f)` | `(A,(fn (A) A))→Iter<A>` | `x`, `(f x)`, `(f (f x))`, ... senza fine |
| `lazy::repeat` | `(lazy::repeat x)` | `A→Iter<A>` | Ripete `x` senza fine |

`Iter<U>` e simili nella tabella sono in realtà tipi struct il cui nome è quello della funzione con
`-iter` aggiunto (per `lazy::map`, `lazy::map-iter<I,A,U>`, dove `I` è il tipo dell'iteratore di
origine). Il tipo si scrive solo dove nient'altro lo determina, come il tipo di ritorno della lambda
passata a `lazy::flat-map`.

```lisp
(collect (lazy::take (lazy::filter (lazy::iterate 1 (lambda ((n int)) int (+ n 1)))
                                   (lambda ((n int)) bool (= 0 (mod n 3))))
                     4))                                  ; => #(3 6 9 12)

(doiter (#{i s} (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
  (println "~a: ~a" i s))                                 ; 0: a e 1: b

(-> (lazy::iterate 1 (lambda ((n int)) int (* n 2)))
    (lazy::take-while (lambda ((n int)) bool (< n 100)))
    collect)                                              ; => #(1 2 4 8 16 32 64)
```

`->` è la macro che passa un valore come primo argomento di ciascuna forma successiva, in ordine
([Option e Result](option-result.md)).

## 5. Le altre funzioni sulle sequenze di CL

Sono tutte funzioni generiche su `Iter`, come nel capitolo 4. Le collezioni risultanti vengono
restituite come nuovi `Vector`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Gli indici con nome di CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Tutti tranne il primo (un nuovo `Vector`, non una coda condivisa) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materializza un iteratore in un `Vector` (il `copy-seq`/`copy-list` di CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` invertito, seguito da `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copie di `x` (il `make-list`/`make-sequence` di CL). Come con `Vector::new`, l'argomento di tipo viene dal tipo atteso, quindi un `let` semplice richiede `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Come `member`, un **`bool`** (un iteratore non ha una coda da restituire) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Le negazioni di `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Stessi tipi delle versioni positive | Versioni con il predicato negato |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Rimuove per valore |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Rimuove i duplicati. Come in CL, **viene mantenuta l'ultima occorrenza** (`:from-end true` mantiene la prima) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Sostituisce per valore / predicato |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | su `Iter<cons-cell<K,V>>` | Le versioni con predicato e sul lato del valore di `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Aggiunge una coppia in testa |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Accoppia due sequenze. Si ferma alla più corta |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | Il `mapcar` di CL su più sequenze. Si ferma alla più corta |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Mappatura per effetti collaterali |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Mappa e concatena |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Mappa su **code** successive |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Mappa sulle code per effetti collaterali (la controparte di `maplist` per `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Mappa sulle code e concatena (la controparte di `maplist` per `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La posizione in cui `sub` compare per la prima volta. Se il ricevitore è una `string`, viene scelto il metodo di `string` ([Stringhe](collections.md#1-stringhe-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La prima posizione in cui differiscono. `none` se sono uguali |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Fusione. CL richiede ingressi ordinati; questa ordina la concatenazione |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Aggiunge `x` **in testa** se non c'è |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Operazioni su insiemi. CL non specifica l'ordine; qui è stabile, **in ordine di prima comparsa** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inclusione |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Se è un suffisso / la parte prima del suffisso. CL chiede della **struttura condivisa**, ma qui non c'è struttura da condividere, quindi questa chiede di un suffisso **come valori** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Uguaglianza elemento per elemento. `Vector<T>` stesso non implementa `Eq` |
| `caar`…`cddddr` | `(cadr p)` | su coppie annidate | Le 28 funzioni di CL. Percorrono **coppie, non liste**: `cadr` prende una `cons-cell<A,cons-cell<B,C>>` |

Ciò che CL ha e questo linguaggio no: `list*` (non esiste la nozione di lista impropria la cui coda è
sostituita), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (nessun tipo può descrivere il
percorrimento di un albero eterogeneo di profondità arbitraria; per un albero di `Sexpr`, `equal`
corrisponde a `tree-equal`), la famiglia delle liste di proprietà `getf`/`get-properties`/`symbol-plist`/
`remprop` (non c'è una rappresentazione come lista non tipizzata che alterna chiavi e valori; `assoc`
(liste di associazione) o `HashTable` svolgono lo stesso ruolo) e le funzioni che convertono tra
`Vector<T>` e liste `Sexpr` (gli elementi di una lista `Sexpr` possono avere ciascuno un tipo diverso,
quindi non si possono scrivere con un unico tipo di elemento `T`).

## 6. Argomenti con parola chiave

Le funzioni dei capitoli 4 e 5 accettano le parole chiave di sequenza di CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count`. Tutte sono **facoltative**.

| Parola chiave | Tipo | Significato |
|---|---|---|
| `:key` | `(fn (A) A)` | Una proiezione applicata a ogni elemento prima di confrontarlo o verificarlo |
| `:test` | `(fn (A A) bool)` | Un test di uguaglianza usato al posto di `equals` del vincolo `Eq`. Il primo argomento è **l'elemento cercato**, il secondo è l'elemento (dopo `:key`), nello stesso ordine di CL |
| `:test-not` | `(fn (A A) bool)` | La negazione di `:test` |
| `:start` `:end` | `int` | La finestra `[start, end)` da scandire. Gli indici sono relativi all'intera sequenza |
| `:from-end` | `bool` | Una ricerca risponde con l'**ultima** corrispondenza. Combinato con `:count`, gli elementi interessati vengono presi dalla fine |
| `:count` | `int` | Il numero massimo di elementi su cui agiscono le famiglie `remove` / `substitute` |

Quale funzione prende quali segue CL:

| Funzione | Parole chiave accettate |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Tutte quelle sopra (incluso `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (il `:key` di `assoc` si applica al `car`, quello di `rassoc` al `cdr`) |
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

**Differenze rispetto a CL**:

1. **La proiezione di `:key` resta all'interno del tipo dell'elemento** (`(fn (A) A)`). Non può proiettare
   su un altro tipo come in CL: una variabile di tipo aggiuntiva non potrebbe essere determinata quando
   l'argomento viene omesso. Dove serve una proiezione su un tipo diverso, passa invece una lambda alla
   famiglia `-if` (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Nelle ricerche basate sull'elemento, `:key` si applica solo agli elementi** (non all'elemento
   cercato). È la stessa regola di `find`/`position`/`count`/`member`/`remove`/`substitute` di CL. Nelle
   operazioni su insiemi entrambi i lati sono elementi, quindi si applica a entrambi.
3. **Solo le parole chiave di `search` sono nominate anziché numerate.** In CL, `:start1`/`:end1` sono
   per il **pattern** e `:start2`/`:end2` per la sequenza in cui si cerca. In questo linguaggio il
   ricevitore viene per primo, quindi gli stessi numeri significherebbero l'opposto, e per di più in
   silenzio. `:start`/`:end` sono per il ricevitore e `:sub-start`/`:sub-end` per il pattern, quindi un
   `:start1` distratto dà un errore di "parola chiave sconosciuta". `mismatch` e `replace` hanno lo
   stesso ordine degli argomenti di CL, quindi mantengono i numeri di CL.

## 7. Operazioni distruttive

Metodi di `Vector<T>`. **Modificano il ricevitore e restituiscono il ricevitore stesso**, quindi
`(nreverse v)` si scrive come `reverse` e anche `v` stesso viene invertito.

| Nome | Forma | Descrizione |
|---|---|---|
| `nreverse` | `(nreverse v)` | Inverte sul posto |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versioni sul posto di `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versioni sul posto della famiglia `substitute` |
| `nbutlast` | `(nbutlast v)` | Elimina l'ultimo elemento |
| `fill` | `(fill v x)` | Imposta ogni elemento a `x`. La lunghezza non cambia |
| `replace` | `(replace v src)` | Sovrascrive dall'inizio con gli elementi di `src`. `(min (len v) (len src))` elementi |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Stesso numero di cui sopra |
| `nconc` | `(nconc v w)` | Aggiunge a `v` gli elementi di `w`. A differenza di CL, **non riscrive la struttura condivisa** (`w` non viene toccato) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Sostituisce il contenuto di `v` con `src` (cambia anche la lunghezza) |
| `rplaca` `rplacd` | `(rplaca p x)` | Riscrive il `car`/`cdr` di una `cons-cell` e restituisce la cella stessa |

Parole chiave accettate:

| Versione distruttiva | Parole chiave accettate |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (il ricevitore è la `sequence-1` di CL) |

`vector-push-extend`/`vector-pop` sono semplicemente `push`/`pop` di `Vector<T>`. Un `Vector<T>` cresce
sempre, quindi nulla corrisponde alla distinzione di CL tra "un vettore con fill pointer" e "un vettore
semplice".

## 8. Funzioni di ordine superiore

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Restituisce il proprio argomento |
| `const` | `(const x y)` | `(A,B)→A` | Restituisce il primo argomento |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Composizione di funzioni `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Scambia gli argomenti di una funzione a due argomenti |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negazione di un predicato |

Non esiste il `constantly` di CL (il tipo dell'argomento ignorato comparirebbe solo nel tipo di ritorno e
non potrebbe essere determinato). Scrivi `(lambda ((x T)) A v)`.
