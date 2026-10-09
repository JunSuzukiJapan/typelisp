<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Stream e file

Trait e metodi degli stream, tipi di stream concreti, operazioni sui file e pathname. Anche i socket di
rete sono stream, e sono trattati in [Rete](network.md).

## 1. La gerarchia dei trait

Ciò che CL esprime con una gerarchia di classi qui viene espresso con una **gerarchia di trait**. Sia la
direzione (input / output) sia il tipo di elemento sono decisi **staticamente**, quindi non c'è bisogno
di chiedersi a runtime "questo stream può essere letto?".

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

Una funzione che legge caratteri accetta qualsiasi tipo di stream, predefinito o definito dall'utente,
se prende `(where (CharInput S))` o `:dyn CharInput`.

## 2. Metodi

Ogni metodo di `CharInput` ha un'implementazione predefinita. Un'implementazione scrive solo
`read-item`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | L'elemento successivo. `none` alla fine. **L'unico metodo che deve essere implementato** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Il carattere successivo |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Fino al successivo a capo (l'a capo viene consumato e rimosso). Viene restituita anche un'ultima riga che non termina con un a capo |
| `read-all` | `(read-all s)` | `(S)→string` | Tutto ciò che resta |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Solo un carattere già disponibile. `none` anziché attendere |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Inserisce in `v` fino a `n` caratteri e restituisce quanti ne sono stati davvero letti. Meno di `n` solo alla fine |

`listen` è in `InputStream` (il genitore di `CharInput`):

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Se la lettura successiva può essere soddisfatta senza attendere. Il valore predefinito è `false`, **il lato che non è mai una bugia**: `true` sarebbe un'ipotesi, e un'ipotesi sbagliata farebbe bloccare `read-char-no-hang`. Tutti gli stream predefiniti lo sovrascrivono. **Per gli stream definiti dall'utente che non lo sovrascrivono, `read-char-no-hang` restituisce sempre `none`** |

`PeekInput` (che eredita da `CharInput`) aggiunge il **rimettere indietro un carattere**. Solo lo stream
stesso ha un posto dove conservare il carattere rimesso, quindi questo non può avere un'implementazione
predefinita ed è un trait separato. `file-stream`/`string-input-stream`/`standard-stream` lo
implementano, e qualsiasi altro stream lo ottiene se avvolto con `make-peek-stream` (capitolo 4).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Fa sì che la lettura successiva restituisca `c`. **L'unico metodo che deve essere implementato**. Come in CL, è garantito un solo carattere |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Guarda il carattere successivo senza consumarlo |

Allo stesso modo, per `CharOutput` un'implementazione scrive solo `write-item`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Scrive un elemento. **L'unico metodo che deve essere implementato** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Scrive un carattere |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Scrive una stringa |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Una stringa e un a capo |
| `terpri` | `(terpri s)` | `(S)→()` | Un a capo (il nome di CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Un a capo a meno che non si sia all'inizio di una riga |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Se il prossimo carattere scritto inizierà una riga. Il valore predefinito è `false` (quindi `fresh-line` scrive l'a capo: nel dubbio, scrivere è il lato sicuro). Tutti gli stream predefiniti lo sovrascrivono |
| `finish-output` | `(finish-output s)` | `(S)→()` | Svuota il buffer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Scrive in ordine tutti i caratteri di `v` |

`at-line-start` ricorda **solo ciò che è stato scritto attraverso quello stream**. `print`/`println`/
`(format true ...)` scrivono sullo standard output senza passare per `*standard-output*`, quindi se
mescoli le due cose, `(fresh-line *standard-output*)` non conosce gli a capo scritti da `println`. Attieniti
a una sola delle due.

`Stream` è comune a tutti gli stream:

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Se è ancora aperto |
| `close` | `(close s)` | `(S)→()` | Lo chiude. **Il GC non chiude gli stream**, quindi fallo esplicitamente (o con `with-open-file`) |

## 3. Tipi di stream concreti

| Tipo | Come crearne uno | Trait implementati |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` è una delle tre costanti `direction-input` / `direction-output` / `direction-append`.
`open-file` restituisce `Err(FileError)` se il file non può essere aperto (un file mancante è un
risultato ordinario, non un panic). Il nome del file può essere una stringa o un `pathname` (`Pathish`
nel capitolo 9).

`(get-output-stream-string s)` restituisce ciò che è stato scritto su uno `string-output-stream` e lo
svuota. Come in CL, può essere estratto anche dopo `close`.

**L'I/O a byte** usa `ByteInput`/`ByteOutput`. Questi fissano l'`Item` di `InputStream`/`OutputStream`
a `int`, allo stesso modo in cui `CharInput`/`CharOutput` lo fissano a `char`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Il byte successivo. `none` alla fine del file |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Scrive un byte. Un errore fuori da 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | La versione a caratteri, in byte |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Come sopra |

CL decide il tipo dell'elemento nella **chiamata**, come in `(open name :element-type '(unsigned-byte 8))`,
ma qui il tipo dell'elemento è **il tipo** dello stream, quindi ciò che differisce è la funzione che lo
apre. Leggere byte da uno stream di caratteri è un errore di tipo (`string-input-stream` non implementa
`ByteInput`). Anche leggere un byte subito dopo aver rimesso indietro un carattere con `unread-char` è un
errore.

## 4. Stream compositi

Sono tutti `defstruct` della libreria standard e possono essere annidati.

| Nome | Forma | Descrizione |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Scrive su tutti gli elementi di un `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Legge da `in` e scrive su `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Legge da `in` e scrive anche su `out` i caratteri letti |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Legge un `Vector<:dyn CharInput>` uno dopo l'altro |
| `make-peek-stream` | `(make-peek-stream in)` | Aggiunge a qualsiasi `:dyn CharInput` il rimettere indietro un carattere, rendendolo un `PeekInput` (per `read-sexpr`) |

## 5. Macro

| Nome | Forma | Descrizione |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Apre, esegue il corpo, chiude. `Result<valore del corpo, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Legge da una stringa |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Restituisce ciò che è stato scritto |

## 6. Funzioni generiche e operazioni sui file

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Trasferisce tutto |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Tutte le righe rimanenti |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Legge una `Sexpr` (il `read` di CL). `Ok(eof)` alla fine dell'input, `Ok(datum d)` quando ne viene letto uno, `Err` se non sono dati. **Consuma l'unico carattere di spazio** che ha terminato il dato (come in CL). `ReadOutcome` non è un `Option<Sexpr>` in modo che leggere la lista vuota `()` e la fine dell'input non siano lo stesso valore |
| `read-sexpr-preserving-whitespace` | Come sopra | Come sopra | Lo stesso, ma lascia lo spazio (il `read-preserving-whitespace` di CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Legge fino a `ch` e crea una lista. `ch` viene consumato. `Err` se l'input finisce |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Scrive una riga alla volta |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | L'intero contenuto |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Tutte le righe |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | La scrive |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Se esiste |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Elimina, rinomina (gli argomenti sono `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Il percorso assoluto con link simbolici e `.`/`..` risolti. `Err` se non esiste |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | L'ora dell'ultima modifica. È **tempo universale**, quindi `decode-universal-time` ([Tempo](system.md#2-decodifica-e-codifica-delle-date)) può leggerlo |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Il nome di login del proprietario. `Err` se il file non esiste, `Ok(none)` se l'uid del proprietario non ha una voce nel database delle password: i due casi che CL distingue restano separati |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Se è una directory. **Anche `false` se non esiste**; usa `probe-file` per distinguere i due casi |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Elenca il contenuto per truename (il percorso assoluto con i link simbolici risolti, come con `truename`). I link simbolici il cui bersaglio manca vengono omessi. `.`/`..` vengono omessi. L'ordine è quello che dà il sistema operativo |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | La crea insieme ai suoi genitori. Riesce se esiste già |

Ogni argomento che indica un file **può essere una stringa o un `pathname`**. È lo stesso trattamento dei
designatori di pathname di CL, risolto attraverso il trait `Pathish` anziché con un test di tipo a
runtime (capitolo 9).

Il carattere terminatore di `read-delimited-list` **termina anche i token**. Ha effetto solo a profondità
0: in `(1 2]` il `]` viene letto come parte del testo della lista stessa e segnalato come lista
malformata. Non c'è un equivalente del terzo argomento `recursive-p` di CL.

## 7. Fare del proprio tipo uno stream

Scrivi un solo `write-item` e le implementazioni predefinite portano con sé il resto. Può anche entrare
negli stream compositi.

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

L'input funziona allo stesso modo: scrivi solo `read-item`. Anche un tipo senza un proprio rimettere
indietro può essere letto con `read` una volta avvolto, come in
`(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nome | Chiamata | Tipo | Descrizione |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` legge il carattere `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Restituisce ciò che è registrato |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` legge la sequenza di due caratteri `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Come sopra |

`F` è `(fn (string-input-stream char) Option<Sexpr>)`. Come usarle, quando hanno effetto e in che cosa
differiscono da CL è spiegato nel [Riferimento della sintassi](../syntax.md#11-reader-macro-readtable).

## 9. Pathname `pathname`

Un nome di file diviso in parti. Contiene i componenti della directory separati da `/`, il nome, il tipo
(estensione) e se parte dalla radice.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Il trait designatore di pathname `Pathish`

Dove CL accetta un designatore di pathname (una stringa o un pathname), questo linguaggio accetta un
`Pathish`. Sia `string` sia `pathname` lo implementano, e **ogni operazione sui file lo prende in modo
generico**, quindi `(open-input "a.txt")` e `(open-input p)` sono entrambe chiamate ordinarie (non c'è
alcun test di tipo a runtime). Il `namestring` di una stringa restituisce semplicemente se stessa, quindi
finché passi una stringa, non avviene alcuna analisi.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | La forma in stringa. Deve essere implementato |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Converte in un `pathname` (la funzione `pathname` di CL, rinominata perché confliggerebbe con il nome del tipo). Deve essere implementato |

### 9.2 Funzioni

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Divide una stringa in parti. Un `/` finale (o un nome vuoto) significa "nessun nome", cioè una directory |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Ne costruisce uno solo con i componenti dati (tutti `&key`). Un nome o tipo omesso resta "assente" ed è qualcosa che `merge-pathnames` riempie |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | I componenti della directory, dal più esterno |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Il nome senza il tipo. `none` per una directory |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Dopo l'ultimo punto. Un punto iniziale non conta (tutto `.gitignore` è il nome) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Se parte dalla radice |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | La directory home. `none` se non c'è `$HOME` (anche CL consente `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | La parte fino all'ultimo `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Solo la parte `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Riempie i componenti mancanti di `p` da `default`. Un `p` relativo va sotto la directory di `default`; un `p` assoluto mantiene la propria directory |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | La forma relativa a `default`. Tutto `p` se non è sotto la base |

Gli argomenti di tipo portano tutti `(where (Pathish P))`.

## 10. Differenze rispetto a CL

- **Una gerarchia di trait, non di classi.** Non esistono `input-stream-p` / `output-stream-p`: il tipo
  porta con sé la direzione, quindi non è una domanda da porre a runtime.
- **`read` ha nomi diversi per le versioni su stringa e su stream.** `(read "...")` (corrisponde al
  primo valore del `read-from-string` di CL; se ti serve anche la posizione in cui la lettura è
  terminata, usa `read-from-string`) e `(read-sexpr s)` (il `read` di CL). Una chiamata si risolve su un
  solo tipo di ricevitore, quindi lo stesso nome non può essere sovraccaricato.
- **Il rimettere indietro è un trait separato** (`PeekInput`), così i tipi che hanno bisogno solo di
  `read-char` non sono costretti a implementare `unread-char`.
- **La chiusura è esplicita.** Il GC non chiude gli stream (il GC viene eseguito in momenti
  imprevedibili, quindi lasciarlo al GC renderebbe imprevedibile anche il momento della chiusura).
  Usare `with-open-file` è il modo sicuro.
- **I pathname non hanno componenti di host, dispositivo o versione.** Non ci sono pathname con caratteri
  jolly né pathname logici (`logical-pathname`). Il separatore è sempre `/`.
- **La funzione `pathname` è `to-pathname`**, perché tipi, trait e funzioni condividono un unico
  namespace.
- **Non c'è corrispondenza con caratteri jolly**, quindi `directory` è una funzione che "elenca il
  contenuto di quella directory" e nulla più. Il `directory` di CL confronta con un pattern di pathname.
