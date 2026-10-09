<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Stampa

`print`/`println`/`format`, i printer a un argomento, il pretty printer, `print-object` e le variabili che
controllano la stampa. L'elenco delle direttive di formato si trova in [format.md](format.md). La
lettura e la scrittura sugli stream si trovano in [Stream e file](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` sono tutte **forme speciali che interpretano le direttive di formato (le
direttive di `format` di CL)**. Il primo argomento (il secondo per `format`) è la **stringa di
controllo**, e ogni direttiva consuma a turno gli argomenti variadici che seguono.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Espande la stringa di controllo e la scrive sullo standard output senza a capo |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Lo stesso, con un a capo alla fine |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | Il `format` di CL. Restituisce la stringa espansa. Se `dest` è `true` (il `t` di CL), viene scritta anche sullo standard output; se è `false` (il `nil` di CL), non viene scritta e viene solo restituita |
| `format` (su uno stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Se `dest` non è un `bool`, è la destinazione stream di CL. La stringa espansa viene scritta su quello stream. Il valore di ritorno è `()` (il `nil` di CL), e non viene restituita alcuna stringa |

Il tipo di `dest` divide il significato in due (quale si applichi viene deciso staticamente). La forma
con stream si può scrivere allo stesso modo con un tipo di stream concreto, un `:dyn CharOutput` o una
variabile di tipo vincolata da `(where (CharOutput S))`. Un `dest` che non è né un `bool` né uno stream
è un errore di tipo.

**La stringa di controllo deve essere un letterale** (la stessa restrizione di `format!` di Rust). Le
direttive al suo interno decidono quanti argomenti vengono presi e di quali tipi, quindi una stringa
costruita a runtime non può essere letta in fase di controllo. Poiché deve essere un letterale, **il
numero e i tipi degli argomenti vengono controllati in fase di controllo**: `(println "~d" "x")` e
`(println "~a ~a" 1)` sono errori in fase di controllo. Anche una direttiva scritta male, un `~(` non
chiuso e un `~/name/` a cui nessun argomento sa rispondere sono errori in fase di controllo. Le regole di
controllo si trovano in [format.md](format.md#1-come-si-scrivono-le-direttive). Per stampare una stringa
che costruisci, creala con `(format false ...)` e stampala con `(println "~a" s)`.

Gli argomenti variadici vengono incapsulati in `Sexpr` con i loro tipi prima di essere passati:
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, così come i tipi definiti dall'utente
`defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` e simili, si possono tutti passare così come sono
(`(println "~a" my-struct)` funziona e basta).

Eseguire uno script con `typl file.typl` **non stampa i valori delle espressioni di primo livello**,
quindi un programma scrive sullo standard output chiamando queste. `print`/`println`/`format` inviano il
loro output a ogni chiamata (in modo che un prompt sia visibile prima che venga letto lo standard input,
anche attraverso una pipe).

**`Option<Sexpr>` viene stampato in modo trasparente.** Il tipo dei dati S-expression è `Option<Sexpr>`,
quindi l'involucro `(some x)` non compare nell'output e il contenuto viene stampato così com'è. La lista
vuota viene stampata come `()`. Gli altri `Option<T>` vengono stampati come `(some ...)` / `none`. Lo
stesso vale per i campi `Option<T>` dentro strutture, enumerazioni e `Vector`. Un
`Result<Option<Sexpr>,…>` da `(eval ...)` viene stampato come `(ok 42)`, oppure `(ok ())` per `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; get only the string, without printing
  (println "~a" s))                   ; => id=42
```

## 2. Printer a un argomento

I printer di CLHS 22.1.3. Invece di espandere un formato, stampano un singolo valore così com'è. Lo
stream può essere omesso (il valore predefinito è `*standard-output*`).

| Nome | Forma | Descrizione |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Scrive in una forma che può essere riletta (come `~s`) e restituisce `x` |
| `princ` | `(princ x [stream])` | Scrive in una forma per le persone (come `~a`) e restituisce `x` |
| `write` | `(write x [stream])` | `prin1` se `*print-escape*` è vero, `princ` se è falso. Restituisce `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Restituisce una stringa invece di scrivere (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Lo stesso (`~a`). Equivale a `to-string` |
| `write-to-string` | `(write-to-string x)` | Lo stesso, secondo `*print-escape*` |

`print`/`println` **non** sono tra questi. Sono abbreviazioni di `format` che prendono una stringa di
controllo, un compito diverso dal `print` di CL (a capo, poi `prin1`, poi uno spazio), quindi ciascuno
mantiene il proprio nome. Di conseguenza, **il `print` a un argomento di CL non ha una grafia in questo
linguaggio**: scrivi `prin1`.

Queste sono macro, perché gli argomenti variadici di `format` non accettano variabili di tipo e il tipo
deve essere noto nel punto di chiamata.

## 3. Standard input e stream standard

**La lettura dello standard input** si fa non con funzioni dedicate ma con i metodi `CharInput` sullo
stream standard `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([metodi degli stream](streams-files.md#2-metodi)). Anche lo standard
output e lo standard error hanno `*standard-output*` / `*error-output*`, e si possono scrivere come in
`(write-line *standard-output* s)` (`print`/`println`/`format` sono scorciatoie per quando serve
l'espansione del formato, e scrivono sempre sullo standard output).

## 4. Il pretty printer

Corrisponde al Lisp Pretty Printer di CL (CLHS 22.2). **Spezza l'output che non sta nella larghezza della
riga, seguendo i blocchi logici e gli a capo condizionali.**

### 4.1 Variabili di controllo

Variabili globali a cui si può assegnare. Una volta fatto `setf`, influenzano tutta la stampa successiva.
Per cambiarne una temporaneamente, usa `dlet` (6.3).

| Variabile | Tipo | Predefinito | Significato |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Se vero, `~a`/`~s`/`~w` e le direttive pretty prendono il percorso di pretty-printing |
| `*print-right-margin*` | `int` | `80` | Il margine destro (in colonne). 0 significa "nessun margine, non spezzare mai". Un valore negativo è un errore di stampa |
| `*print-miser-width*` | `int` | `0` | La larghezza a cui inizia lo stile miser. 0 corrisponde al `nil` di CL (stile miser disattivato). Un valore negativo è un errore di stampa |

La famiglia `pprint` e `pprint-logical-block` fanno sempre pretty-print indipendentemente da
`*print-pretty*` (secondo la definizione del `pprint` di CL).

### 4.2 Layout pronti (forme speciali)

Come `print`, sono forme speciali, quindi l'argomento può essere di qualsiasi tipo.

| Nome | Forma | Descrizione |
|---|---|---|
| `pprint` | `(pprint x)` | Fa pretty-print con il layout predefinito. Come in CL, **scrive prima un a capo** e nessuno alla fine |
| `pprint-fill` | `(pprint-fill x)` | Riempie ogni riga con quanto ci sta. Non scrive a capo |
| `pprint-linear` | `(pprint-linear x)` | Se non tutti gli elementi stanno su una riga, **un elemento per riga**. Non scrive a capo |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Una tabella con colonne larghe `colinc` (predefinito 16). Non scrive a capo. Un `colinc` negativo è un errore |

Il layout predefinito (`pprint`, e `~a` con `*print-pretty*`) segue il `*print-pprint-dispatch*`
predefinito di CL: abbrevia `(quote x)` come `'x`, e formatta le forme di codice come
`defun`/`let`/`if`/`lambda` come "la testa e il numero prescritto di argomenti sulla prima riga, e il
resto del corpo indentato di due colonne, una forma per riga". Le altre liste vengono riempite.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Costruire da sé i blocchi logici

| Nome | Forma | Descrizione |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Una forma speciale che apre un blocco logico. `obj` è la lista che `pprint-pop` percorre (`()` se non se ne percorre nessuna). `:prefix` e `:per-line-prefix` si escludono a vicenda (come in CL) |
| `pprint-newline` | `(pprint-newline kind)` | Un a capo condizionale. `kind` è `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Indentazione. `kind` è `:block` (dall'inizio del blocco) / `:current` (dalla colonna corrente) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Una tabulazione. `kind` è `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` e `colinc` sono non negativi (errore se negativi) |
| `pprint-pop` | `(pprint-pop)` | Prende l'elemento successivo dalla lista del blocco (`()` se è esaurita) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Se la lista è esaurita |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Se è esaurita, esce con `break` dal `loop` che lo racchiude (una macro) |

I blocchi logici non prendono un argomento stream: **un blocco logico aperto è uno stato implicito**.
Il `pprint-logical-block` più esterno lo avvia, e quando si chiude, il tutto viene formattato e scritto
sullo standard output in una volta. Finché è aperto, l'output di `print`/`println`/`(format true ...)`/
`pprint` va tutto in quel blocco, quindi **scrivi il contenuto con il normale `print` e segni solo i
punti in cui spezzare con `pprint-newline` e simili**, il che fa sembrare il codice quasi uguale a quello
in CL.

In CL, `pprint-exit-if-list-exhausted` è un'uscita non locale da `pprint-logical-block`; qui è **un
`break` dal `loop` che lo racchiude** (`pprint-logical-block` non stabilisce un `block`). L'idioma di CL
lo mette comunque sempre dentro un `loop`, quindi si legge allo stesso modo.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Regole per gli a capo condizionali (`pprint-newline` di CLHS):

- `:mandatory` spezza sempre.
- `:linear` spezza se il blocco logico che lo racchiude non sta su una riga. La decisione è per blocco,
  quindi **tutti gli a capo `:linear` di un blocco si spezzano insieme** (è il "tutto su una riga o un
  elemento per riga" di `pprint-linear`).
- `:fill` spezza se (a) la sezione successiva non sta nel resto della riga, (b) la sezione precedente
  non stava su una riga, o (c) in stile miser, il blocco non sta su una riga.
- `:miser` funziona come `:linear` solo in stile miser (quando il blocco inizia entro
  `*print-miser-width*` dal margine destro).

## 5. `print-object` (rappresentazione stampata per tipo)

Scrivere `impl print-object <type>` fa sì che `print`/`println`/`format`/`pprint` stampino i valori di
quel tipo con quell'implementazione, **anche quando sono annidati dentro liste**. Corrisponde alla
funzione generica `print-object` di CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argomento | Significato |
|---|---|
| `self` | Il valore da stampare |
| `escape` | Il `*print-escape*` di CL. `true` per `~s`/`prin1`/`pprint` (una forma che può essere riletta), `false` per `~a`/`princ` (per le persone). Un'implementazione a cui non interessa può ignorarlo |

La `string` restituita va direttamente nell'output. I tipi senza `impl` vengono stampati nella
rappresentazione predefinita (della forma `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   works when nested too
```

Si combina anche con il pretty printer (capitolo 4). Se `*print-pretty*` è vero, una lista che contiene le
stringhe restituite dall'implementazione viene spezzata al margine destro.

Le rappresentazioni stampate dei tipi della libreria standard. I tipi che esistono anche in CL vengono
stampati allo stesso modo di SBCL. Quando il REPL mostra un risultato, usa la stessa rappresentazione di
`~s`.

| Tipo | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Lo stesso |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (il numero è un numero seriale interno) | Lo stesso |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Un intero (il valore di `get-universal-time` / `get-internal-real-time` di CL) | Lo stesso |
| Tipi di errore (`ParseIntError`, `SimpleError` e così via) | `#<simpleerror "boom">` | Solo il messaggio (`boom`) |
| `complex` | `#C(1.0 2.0)` | Lo stesso |
| `Array<T>` | `#2A((0 0) (0 0))` | Lo stesso |
| Stream | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Lo stesso |
| Socket | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Lo stesso |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` alla fine durante l'ora legale) | Lo stesso |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Lo stesso |
| Tipi `defstruct` | `#<point x: 1 y: 2>` (nomi dei campi e valori) | Lo stesso (campi con `~a`) |

Regole:

- **La registrazione è statica.** Un `impl` viene controllato nei tipi come una normale definizione di
  metodo, quindi un nome di tipo scritto male o una firma sbagliata è un errore di compilazione.
- **Funziona anche per i tipi generici.** `(impl print-object box<T> (where (print-object T)) ...)` va a
  un corpo separato per ogni argomento di tipo: un valore ricorda il proprio tipo compresi gli argomenti
  di tipo (`box<i32>`). I tipi generici predefiniti come `Vector<T>` funzionano allo stesso modo.
- **La scelta viene fatta al momento della stampa.** Quale direttiva consuma quale argomento dipende dal
  contenuto a runtime della stringa di controllo, quindi la distinzione tra `~a` e `~s` (cioè `escape`)
  è nota solo nel momento della stampa. È lo stesso di CLOS, dove i metodi `print-object` sono "definiti
  per classe e scelti al momento della stampa".
- **La rientranza ricade sulla rappresentazione predefinita.** Se un'implementazione stampa se stessa con
  `(format false "~a" self)`, ricorrerebbe all'infinito, quindi quando un valore in stampa compare di
  nuovo, viene usata la rappresentazione predefinita. Questo guarda l'identità del valore, non un limite
  di profondità, quindi non ostacola la stampa legittima di strutture autoreferenziali annidate.
- **Ogni tipo scalare implementa questo trait.** Questo **perché possa essere usato come vincolo**: gli
  argomenti variadici di `format` non possono prendere variabili di tipo, quindi questo vincolo è l'unico
  modo in cui il codice generico può dire "valori di un tipo sconosciuto possono essere resi" (la stessa
  forma di `T: Display` di Rust). Il `print-object` di `Array<T>` ne è un esempio.
- **Con argomenti di tipo che non soddisfano il vincolo, viene usata in silenzio la rappresentazione
  predefinita.** `(impl print-object Array<T> (where (print-object T)))` si applica a `Array<i32>`, ma
  non a un `Array` i cui elementi sono una `defstruct` senza `print-object`. Non avrebbe senso che la
  semplice creazione di un array fosse un errore, quindi non lo è.
- L'altro meccanismo di CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (un registro a runtime
  indicizzato da specificatori di tipo), **non è adottato**. Le sue registrazioni non sono controllate, il
  che non si adatta a un linguaggio a tipizzazione statica.

## 6. Controllo della quantità stampata

### 6.1 Profondità, lunghezza e condivisione

Le variabili di controllo di CLHS 22.1.1 che decidono "quanto di un valore viene stampato". Come le tre
in 4.1, sono globali assegnabili, e si applicano a tutti `print`/`println`/`format`/`pprint`, che
`*print-pretty*` sia vero o no.

| Variabile | Tipo | Predefinito | Significato |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Gli oggetti annidati a questa profondità o più vengono sostituiti da `#`. L'oggetto in stampa è a profondità 0. 0 significa illimitato |
| `*print-length*` | `int` | `0` | Stampa gli elementi della lista (e i campi dei valori `defstruct`/`defenum`) fino a questo numero e sostituisce il resto con `...`. 0 significa illimitato |
| `*print-circle*` | `bool` | `false` | Se vero, il valore viene scandito prima della stampa e **gli oggetti che compaiono due o più volte ricevono etichette**. La prima occorrenza è `#n=…` e le successive `#n#` |

CL usa `nil` per "illimitato", ma questo linguaggio non ha `nil`, quindi come per `*print-right-margin*`,
**0 significa illimitato**. I valori negativi non hanno significato e sono errori di stampa. I valori
predefiniti sono tutti "nessun limite / nessuna etichetta", corrispondenti ai valori iniziali di CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Le strutture circolari si possono stampare solo quando `*print-circle*` è vero.** Se stampi un valore che
punta a se stesso mentre è falso (il valore predefinito), il printer continua a seguire il ciclo e il
processo va in crash. CL è uguale (CLHS lascia indefinita la stampa di strutture circolari quando
`*print-circle*` è falso).

Un ciclo si può creare solo "puntando con `setf` un campo di `defstruct` a se stessa" (le celle `Sexpr`
non si possono modificare dopo la creazione, quindi una lista come `'(1 2 3)` non può mai essere
circolare):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a points to a itself
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Le etichette **ricominciano da 1 per ogni cosa stampata** (come in CL). Anche senza un ciclo, se lo stesso
oggetto compare due volte riceve `#1=`/`#1#`, mantenendo nell'output l'informazione che "questi due sono
lo stesso oggetto", come specifica CL:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Un valore senza condivisione **non mostra alcuna etichetta**, quindi lasciare questa variabile vera non
cambia l'output del codice di tutti i giorni.

### 6.2 Base, maiuscole/minuscole e leggibilità

| Variabile | Tipo | Predefinito | Significato |
|---|---|---|---|
| `*print-base*` | `int` | `10` | La base per stampare gli interi (a larghezza fissa e `int`). Fuori da 2-36 è un **errore di stampa** (anche CL specifica l'intervallo) |
| `*print-radix*` | `bool` | `false` | Se vero, aggiunge un marcatore di base: `#b`/`#o`/`#x`, `#NNr` per le altre basi, e un `.` finale per la base 10. Il marcatore va **prima** del segno (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Maiuscole/minuscole dei nomi dei simboli: `:upcase` / `:downcase` / `:capitalize` (le stesse grafie di CL). Qualsiasi altro simbolo è un errore di stampa |
| `*print-readably*` | `bool` | `false` | Se vero, stampa in una forma che può essere riletta. Forza l'escape e disattiva i tagli di `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Il numero di righe che il pretty printer può usare. L'eccesso viene tagliato, con `..` alla fine come in CL. 0 significa illimitato. Un valore negativo è un errore di stampa |
| `*print-escape*` | `bool` | `true` | Se `write`/`write-to-string` fanno `prin1` o `princ`. **Solo quei due lo leggono** |
| `*print-array*` | `bool` | `true` | Se `Vector<T>` e `Array<T>` mostrano il contenuto. Se vero, la sintassi degli array di CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); se falso, solo il tipo e la forma, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

I marcatori che `*print-radix*` aggiunge possono essere riletti dal lettore (la notazione di base nel
[Riferimento della sintassi](../syntax.md#1-elementi-lessicali)).

**Perché il valore predefinito di `*print-case*` differisce da CL**: il valore predefinito di CL è
`:upcase` perché il lettore di CL memorizza i nomi dei simboli in maiuscolo, cioè significa "come
memorizzato". Questo lettore li memorizza in minuscolo, quindi il valore predefinito con lo stesso
significato è `:downcase`.

**La metà mancante di `*print-readably*`**: CL segnala `print-not-readable` per i valori che non si
possono rileggere, ma questo linguaggio non ha una condizione da segnalare, e nessun modo di decidere la
leggibilità per i tipi utente, che `print-object` può stampare in qualsiasi modo. Ci sono solo l'escape
forzato e la sostituzione dei tagli.

**Perché solo `write` legge `*print-escape*`**: come specifica CLHS, `~s`/`prin1`/`pprint` lo associano a
vero, e `~a`/`princ` a falso, ciascuno solo per la durata della propria chiamata. Quindi gli unici lettori
che lo vedono non associato sono `write`/`write-to-string`. Un'implementazione di `print-object` dovrebbe
leggere il proprio argomento `escape` anziché questa globale: quell'argomento porta il valore scelto
dalla direttiva.

**Ciò che CL ha e questo linguaggio no**: `*print-gensym*` (non ci sono simboli non internati).

### 6.3 Sostituzioni temporanee

CL le associa con `let`, ma `let` in questo linguaggio associa in modo lessicale, quindi usa `dlet`
([Altro](system.md#10-altro)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; the limits apply to this one print only
(with-standard-io-syntax (println "~a" x))   ; print with everything back at the standard values
```

`with-standard-io-syntax` esegue il proprio corpo con tutte le variabili di controllo del printer ai
valori standard e `*read-eval*` impostato a `true`.
