<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Stringhe, caratteri e collezioni

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` e `BitVector`.

## 1. Stringhe `string`

Le stringhe sono immutabili.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Converte in maiuscolo (solo ASCII). Come `string-upcase` di CL, restituisce una nuova stringa. Le stringhe sono immutabili, quindi non esiste un `nstring-upcase` distruttivo; questa ne prende il posto |
| `downcase` | `(downcase s)` | `string→string` | Converte in minuscolo (solo ASCII). Prende il posto di `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Mette in maiuscolo la prima lettera di ogni parola e in minuscolo il resto (il `string-capitalize` di CL). Una parola è una sequenza massimale di lettere e cifre |
| `length` | `(length s)` | `string→int` | Numero di caratteri |
| `ref` | `(ref s i)` | `(string,int)→char` | Il carattere `i`. Va in panic fuori intervallo |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | La sottostringa `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Concatenazione. Se ne possono dare tre o più (equivale a `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Confronto lessicografico |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Minore stretto lessicografico (equivale a `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Confronto di identità (se sono lo stesso oggetto, non lo stesso contenuto) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Confronta i contenuti (distingue maiuscole e minuscole) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Confronta i contenuti (non distingue maiuscole e minuscole, solo ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Se i contenuti differiscono (il `string/=` di CL. La forma variadica confronta coppie adiacenti) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Ordinamento senza distinzione di maiuscole (il `string-lessp` di CL e così via). Con un prefisso comune, la più corta è la minore |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Una stringa di `n` copie di `c` (il `make-string` di CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | La posizione in cui `sub` compare per la prima volta. **Il `search` di CL ha gli argomenti nell'ordine opposto** (`(search pattern sequence)`). La stringa vuota si trova in 0. Per le parole chiave, si vedano gli [argomenti con parola chiave delle sequenze](sequences.md#6-argomenti-con-parola-chiave) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | La prima posizione in cui differiscono. `none` solo quando sono `equal`. Se una è prefisso dell'altra, la fine della più corta. Parole chiave come sopra |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Rimuove i caratteri contenuti in `bag` da entrambe le estremità / dalla sinistra / dalla destra (il `string-trim` di CL e così via). Senza `bag`, gli spazi `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Divide in corrispondenza di `sep`. CL non ha un equivalente. I separatori consecutivi producono elementi vuoti. Va in panic se `sep` è vuoto |
| `to-string` | `(to-string x)` | `T→string` | Converte in stringa come fa `~a`. Implementato per `int`/`i32`/`f64`/`bool`/`char`/`string` (il `princ-to-string` di CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Codifica in UTF-8 (ogni elemento 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Decodifica. `none` se non è UTF-8 valido |

## 2. Caratteri `char`

Un `char` è un valore scalare Unicode. La conversione di maiuscole/minuscole e la classificazione
trattano solo l'intervallo ASCII.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Converte in maiuscolo (solo ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Converte in minuscolo (solo ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Confronto per punto di codice |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Minore stretto per punto di codice (equivale a `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Se è una lettera ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Se è una cifra ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Confronta i valori |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Confronta i valori ignorando maiuscole e minuscole (il `char-equal` di CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Se i valori differiscono (il `char/=` di CL. **La forma variadica confronta coppie adiacenti**, a differenza di CL, che chiede se tutte le coppie differiscono) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Ordinamento senza distinzione di maiuscole (il `char-lessp` di CL e così via) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Maiuscolo / minuscolo / ha distinzione tra maiuscole e minuscole (l'`upper-case-p` di CL e così via) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Una lettera o una cifra (stesso nome che in CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Se è stampabile. Include lo spazio, non l'a capo né la tabulazione (il `graphic-char-p` di CL) |
| `standardp` | `(standardp c)` | `char→bool` | Se è uno dei 96 caratteri standard di CL, cioè `graphicp` più l'a capo (lo `standard-char-p` di CL) |
| `char->int` | `(char->int c)` | `char→int` | Il valore scalare Unicode (l'inverso è `int->char`/`try-int->char` in [Numeri](numbers.md#1-interi-a-larghezza-fissa)). Corrisponde a `char-code`/`char-int` di CL |
| `char->string` | `(char->string c)` | `char→string` | Una stringa di un carattere. La funzione `string` di CL lo copre accettando un designatore, ma questo linguaggio non ha designatori, quindi la direzione è nel nome |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Il **peso** della cifra in quella base (il `digit-char-p` di CL). `digitp` è una funzione separata che restituisce `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Il carattere per il peso `w`. Maiuscolo da 10 in su (il `digit-char` di CL; la base è al massimo 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Il nome del carattere. Hanno nome solo i caratteri con nome che il lettore sa leggere (il `char-name` di CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Il carattere per un nome. Non distingue maiuscole e minuscole, e accetta anche gli alias del lettore (`linefeed`/`null`) (il `name-char` di CL) |

Non c'è una costante corrispondente a `char-code-limit` (il limite superiore di `char` è fissato da
Unicode, non dal linguaggio).

## 3. `Vector<T>`

Un array espandibile.
Un valore si può scrivere `#(1 2 3)` ([riferimento della
sintassi](../syntax.md#1-elementi-lessicali); il tipo degli elementi viene dal contesto o dal primo
elemento, e ogni valutazione crea un nuovo vettore). Si stampa anche come `#(1 2 3)`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Crea un vettore vuoto. L'argomento di tipo viene dal tipo atteso, quindi in un `let` semplice scrivi `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copie di `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Aggiunge in fondo |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Legge l'elemento `i`. Va in panic fuori intervallo |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Modifica l'elemento `i`. Va in panic fuori intervallo. Si può scrivere anche `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Numero di elementi |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Rimuove l'ultimo elemento e lo restituisce. `None` se vuoto (a differenza di `get`/`set`, non va in panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Crea un iteratore che implementa `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Aggiunge `x` se non esiste un elemento uguale (il `pushnew` di CL. Non ha bisogno di riscrivere un place, quindi è un metodo e non una macro) |

`map`/`filter` e simili sono [funzioni sulle sequenze](sequences.md#4-funzioni-sulle-sequenze-con-iter):
passa il vettore attraverso `iter`, come in `(map (iter v) f)`. Le operazioni distruttive (`nreverse`,
`delete` e così via) si trovano in [Operazioni distruttive](sequences.md#7-operazioni-distruttive).

## 4. `HashTable<K,V>`

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Crea una tabella vuota |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Ricerca |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Inserisce o sovrascrive |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Rimuove la voce e restituisce il vecchio valore, se c'è |
| `count` | `(count h)` | `HashTable<K,V>→int` | Numero di voci |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Rimuove tutto |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Un'istantanea delle chiavi |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Un'istantanea dei valori |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Un'istantanea delle coppie `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Un iteratore che implementa `Iter`. Gli elementi sono `cons-cell` `(k . v)`. Corrisponde a `with-hash-table-iterator` di CL; `doiter`/`map`/`filter` e altri funzionano su di esso così come sono |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | Il `maphash` di CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | Il `hash-table-size` di CL. In questa tabella è il numero di voci occupate (uguale a `count`) |

**Qualsiasi tipo che implementi `Hash` può essere una chiave**, compresi i tipi `defstruct`/`defenum`.
`get`/`set`/`remove` portano `(where (Hash K))`, quindi una tabella con chiave di un tipo che non lo
implementa è un **errore di tipo** (`f64` non ha `Hash` a causa di `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Implementato per: `int` e i sei interi a larghezza fissa, `bool`, `char`, `string` e `symbol` (non per i
numeri in virgola mobile). Per i tuoi tipi, mantieni il risultato non negativo facendo `logand` con
`*sxhash-mask*` (2^30-1). Per calcolare l'hash di una stringa, puoi chiamare `(sxhash-string s)` (FNV-1a
a 32 bit), che usa l'implementazione per `string`.

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

Se due chiavi sono la stessa cosa lo decide **il tipo della chiave stesso** (`sxhash`, e `equals` di `Eq`,
il supertrait di `Hash`), non l'identità dell'oggetto. Per questo, come sopra, puoi cercare con una
chiave che è "un valore diverso ma uguale".

Va bene che `sxhash` abbia collisioni (il contratto di `Hash` va in una sola direzione: valori uguali
devono avere lo stesso hash). Le chiavi in collisione vengono distinte tramite `equals`.

## 5. `Array<T>` (array multidimensionali)

Una `defstruct` della libreria standard. Non è un tipo predefinito, quindi tutto ciò che si può fare con
una `defstruct` si può fare con essa.
Un valore si può scrivere `#2A((1 2) (3 4))` ([riferimento della
sintassi](../syntax.md#1-elementi-lessicali)).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | Il `make-array` di CL. `dims` viene copiato. `init` è il valore iniziale di ogni cella (l'`:initial-element` di CL; questo linguaggio non ha "celle non associate", quindi è obbligatorio). `:fill-pointer` solo per una dimensione |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | L'`aref` / `(setf (aref …))` di CL. Va in panic se un indice è fuori intervallo |
| `aref` | `(aref a i j …)` | — | La grafia di CL con indici semplici. Si espande nei `get`/`set` sopra. Funziona anche `(setf (aref a i j) v)` |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | Il `row-major-aref` di CL. Un indice piatto |
| `rank` | `(rank a)` | `Array<T>→int` | L'`array-rank` di CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | L'`array-dimension` di CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | L'`array-dimensions` di CL. Restituisce una **copia**, proprio come CL restituisce una lista nuova |
| `total-size` | `(total-size a)` | `Array<T>→int` | L'`array-total-size` di CL (il numero di celle allocate, indipendente dal fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | Il `length` di CL sugli array. Il fill pointer se c'è, altrimenti `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | L'`array-in-bounds-p` di CL. Falso (non un errore) anche quando il **numero** di indici è sbagliato |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | L'`array-row-major-index` di CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | L'`adjust-array` di CL. Il rango non può cambiare. Gli elementi che restano nell'intervallo sono mantenuti ai loro indici, e le nuove celle ricevono `init`. A differenza di CL, non restituisce l'array (ogni array in questo linguaggio è adattabile, quindi non c'è un secondo array da restituire) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | Il `vector-push-extend` di CL. Va in panic senza fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | Il `vector-pop` di CL. `none` se vuoto |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Il fill pointer (`none` se non c'è). Si può scrivere con `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Un iteratore in ordine row-major. Si ferma al fill pointer se c'è |

- **Gli indici sono un `Vector<int>`.** Un metodo non può dichiarare "lo stesso tipo di argomento ripetuto
  un numero qualsiasi di volte in fondo", e lo zucchero sintattico di `aref` colma questa lacuna.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **non esistono**. Il tipo statico del ricevitore risponde già a queste domande.
- `Array::new` è il costruttore in ordine dei campi generato da `defstruct` e non serve per creare array.
  Usa `Array::make`.
- **Gli array vengono stampati nella sintassi degli array di CL.** Il rango 1 è `#(1 2 3)`; gli altri ranghi
  sono `#nA` seguito da altrettanti livelli di parentesi (`#2A((1 2 3) (4 5 6))`); il rango 0 è `#0A5`. La
  stampa si ferma al fill pointer se c'è. Impostare `*print-array*`
  ([Stampa](printing.md#6-controllo-della-quantità-stampata)) a falso stampa solo la forma, `#<array 2x3>`.
  Solo un array i cui elementi sono una `defstruct` senza `print-object` viene stampato nella forma
  predefinita `#<array<...> ...>` (non è un errore).

## 6. `BitVector` (vettori di bit)

Una sequenza di bit a lunghezza fissa. Una `defstruct` della libreria standard.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Lunghezza `n`, tutti i bit a 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Vanno in panic fuori intervallo |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Le grafie di CL. Funziona anche `(setf (bit v i) b)`. Lo `sbit` di CL differisce da `bit` solo perché richiede un bit vector semplice, ma questo linguaggio ha un solo tipo di bit vector |
| `len` | `(len v)` | `BitVector→int` | Numero di bit |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Restituiscono un nuovo bit vector. Vanno in panic se le lunghezze differiscono. Non c'è un terzo argomento come in CL (la destinazione del risultato) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complemento |

Non esiste `bit-vector-p` (risponde il tipo statico).
