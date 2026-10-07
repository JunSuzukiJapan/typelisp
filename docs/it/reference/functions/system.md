<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tempo, ambiente e implementazione

Funzioni per il tempo, interrogazioni sull'ambiente di esecuzione, strumenti di implementazione,
analisi e valutazione di testo, docstring e macro.

## 1. Tempo

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Due campi: `day` (giorni dal 1900-01-01) e `second` (il secondo all'interno di quel giorno, 0..86399) |
| `internal-time` | — | `defstruct` | Due campi: `second` e `microsecond` (all'interno di quel secondo, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Il tempo dall'epoca di CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Il tempo trascorso relativo al processo |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | Il **tempo di CPU** usato da questo processo (utente più sistema) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Come numero di secondi. La forma per riportare la differenza tra due letture |
| `internal-time-units-per-second` | — | `int` | `1000000` (microsecondi), l'unità del campo `microsecond`. Come in CL, il valore è una scelta dell'implementazione |
| `time` | `(time form)` | Macro | Esegue `form`, stampa il tempo reale e il tempo di CPU su una riga ciascuno, e restituisce il valore di `form` così com'è |

Il tempo reale e il tempo di CPU dicono cose diverse. Per un lavoro che per lo più attende l'I/O, i due
differiscono molto, e quella differenza è esattamente ciò che si vuole sapere, quindi `time` mostra
entrambi.

`sleep`, che ferma un task, si trova in [Task e canali](concurrency.md#3-yield--sleep--cedere-il-passo).

## 2. Decodifica e codifica delle date

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Nove campi**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. I nove valori di ritorno di CL come un'unica struttura (non esistono valori multipli) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Il tempo universale in componenti di calendario. `zone` è in ore a ovest di Greenwich (la stessa direzione di CL). **Se omesso, è l'ora locale** (come in CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | L'inverso. Senza `zone`, gli argomenti sono letti come **ora locale** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Adesso, decodificato in ora locale |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Lo scarto dell'ora locale a ovest di Greenwich, in **secondi**, a quel tempo universale |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Se a quel tempo universale era in vigore l'ora legale |

Come in CL, per `day-of-week` **0 è lunedì e 6 è domenica**.

**Senza `zone`, si usa l'ora locale**, come in CL. Lo scarto locale viene chiesto al sistema operativo,
quindi il risultato dipende da dove si trova la macchina. **Indicare una zona esplicita lo rende
deterministico**, e `0` è UTC.

L'unità di `zone` è, come in CL, "ore a ovest di Greenwich", quindi UTC+9 si legge come `-9`. Tuttavia,
**l'argomento è un intero e il campo `zone` del risultato è un `f64`**. Gli scarti reali non sono sempre
ore intere (l'India è +5:30, il Nepal +5:45), e arrotondare il valore riportato direbbe in silenzio una
bugia. Una zona che scrivi a mano è un numero intero di ore, quindi l'argomento è `int`.

Quando `zone` è indicato, `daylight-p` è `false` e `zone` è esattamente il valore indicato, come specifica
CL (*If a time-zone is supplied, daylight saving time information is ignored*).

Un'ora locale che cade dentro una transizione dell'ora legale non è comunque unica, e CL non dice quale
scegliere. `encode-universal-time` restituisce una delle due risposte per un'ora del genere.

## 3. L'ambiente di esecuzione

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | La riga di comando. **L'elemento 0 è il nome del programma** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Una variabile d'ambiente. `none` se non impostata o non UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. La base di `user-homedir-pathname` ([Pathname](streams-files.md#92-funzioni)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | La versione dell'implementazione |
| `machine-type` | `(machine-type)` | `()→string` | L'architettura della CPU (`x86_64` / `aarch64` …). Il valore del **target di compilazione** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Il nome dell'host |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Il nome dell'hardware **in esecuzione ora** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` dove non si può determinare |
| `software-type` | `(software-type)` | `()→string` | Il sistema operativo (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | La release del sistema operativo (`uname -r`, per esempio `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Un nome breve per il sito di installazione. **Sempre `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Allo stesso modo, un nome lungo. **Sempre `none`** |

Quelli che restituiscono `Option` sono voci per le quali CL consente `NIL` (*or nil if no such name can be
determined*). POSIX non ha un posto dove registrare i nomi dei siti, quindi sono sempre `none`; SBCL
restituisce lo stesso. Nota la differenza tra `machine-type` e `machine-version`: il primo è
l'architettura per cui questo binario è stato **compilato**, il secondo è il chip che lo **esegue** ora.

L'elemento 0 di `command-line-args` è il percorso dello script per `typl script.typl a b`, e
l'eseguibile stesso per un eseguibile AOT avviato come `./prog a b`. **In entrambi i modi di
esecuzione si leggono gli stessi argomenti agli stessi indici** (`typl` rimuove il proprio nome e le
opzioni come `--heap-cells` prima di passarli).

## 4. Chiedere all'utente

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Accetta un singolo `y` / `n`. Chiede di nuovo finché non ne ottiene uno |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Fa scrivere per esteso `yes` / `no`. Per domande in cui un errore costa caro |

Entrambe leggono da `*standard-input*`. Solo la fine dell'input interrompe la richiesta ripetuta, e allora
il risultato è `false`.

## 5. Strumenti di implementazione (CLHS 25.2)

Lo strato in cui l'implementazione risponde a domande su se stessa. `heap-info` / `room` / `dribble` sono
funzioni ordinarie; `trace` / `untrace` / `step` / `disassemble` / `ed` sono **forme speciali** (`trace` /
`untrace` / `disassemble` / `ed` prendono il *nome* di una definizione, e `step` una *forma*, tutti non
valutati).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Lo stato corrente dello heap come struttura. Gli stessi numeri che stampa `room` |
| `room` | `(room &optional verbose)` | `(bool)→()` | Riporta `heap-info` su `*standard-output*`. `(room true)` dà più dettaglio |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Avvia la registrazione dell'output della sessione su `path` / interrompe la registrazione quando chiamato senza argomento |
| `trace` | `(trace name...)` | `Sexpr` | Riporta su `*trace-output*` le chiamate delle definizioni nominate. Restituisce la lista dei nomi tracciati ora |
| `untrace` | `(untrace name...)` | `Sexpr` | Smette di riportare. **Senza argomenti, rimuove tutto** |
| `step` | `(step form)` | Il tipo di `form` | Valuta `form`, fermandosi a ogni chiamata per chiedere |
| `disassemble` | `(disassemble name [llvm])` | `()` | Stampa in che cosa diventa quella definizione. Il codice macchina dell'host per impostazione predefinita, LLVM IR con `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Avvia `$VISUAL` / `$EDITOR`. Dato un nome, apre la riga in cui è scritta quella definizione |

`trace`/`untrace`/`step`/`disassemble` sono solo dell'interprete, e le funzioni che le chiamano non possono
essere compilate ([capitolo 10 del Riferimento della sintassi](../syntax.md#10-compilazione)).

### 5.1 Campi di `heap-info`

| Campo | Tipo | Contenuto |
|---|---|---|
| `capacity` / `live` / `free` | `int` | L'intera arena dei cons e la sua suddivisione. Sempre `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | I conteggi correnti degli altri tre tipi di oggetto dello heap |
| `gc-count` | `int` | Il numero di raccolte dall'avvio dell'implementazione |
| `growable` | `bool` | Se l'arena può ancora crescere |

I campi sono tutti `int` (tranne `growable`). Il limite di crescita (si veda la descrizione di
`typl --heap-cells`) non viene riportato, perché ciò che chi legge vuole sapere è se può ancora crescere
(`growable`).

### 5.2 Che cosa `trace` / `step` possono e non possono vedere

- **Anche le definizioni con corpo compilato sono visibili, dai punti di chiamata interpretati.**
- **I punti di chiamata *dentro* il codice compilato non sono visibili.** Tracciare un nome che ha un
  corpo compilato aggiunge una nota di una riga che lo dice. La stessa limitazione che SBCL descrive per
  le chiamate locali.
- **Le chiamate attraverso valori di chiusura (`funcall`/`apply`) non sono visibili.** Le chiusure non
  hanno nomi.
- **Le definizioni generiche non sono coperte.** Una copia per ciascun tipo viene creata in ogni punto
  d'uso, quindi non c'è un unico corpo da nominare (lo stesso motivo, e le stesse parole, di quando
  `compile` rifiuta).

I comandi di `step` sono `s` (entra in questa chiamata; anche una riga vuota fa lo stesso), `n` (salta
questa chiamata), `c` (smetti di chiedere da qui in poi) e `q` (interrompi). **Se lo standard input non è
un terminale, `step` si limita a valutare `form`**: un comportamento degenere che CLHS consente
esplicitamente, in modo che script e test non si blocchino su un prompt a cui nessuno può rispondere.

`$VISUAL` / `$EDITOR` di `ed` viene diviso agli spazi, quindi `EDITOR="code -w"` funziona. Se nessuno dei
due è impostato, il risultato è `Err`: non ipotizza `vi`. Il numero di riga viene passato per primo, nella
forma `+N`.

`dribble` registra tutti e tre i modi in cui l'output della sessione esce dal processo: ciò che scrivono
`print`/`println`/`format`, ciò che viene scritto sugli stream collegati allo standard output, e le righe
digitate nel REPL insieme ai valori che il REPL stampa in risposta.

## 6. Analisi e valutazione

Tutte queste trattano testo e dati a runtime (che il programma stesso non controlla), quindi in caso di
fallimento restituiscono l'`Err` di un `Result` anziché andare in panic. I tipi di errore sono tipi
concreti per ciascuna operazione ([Tipi di errore](option-result.md#3-tipi-di-errore-e-il-trait-error)).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | Il `parse-integer` di CL. Salta gli spazi iniziali e finali (lo stesso insieme di `trim`), legge al più un segno `+`/`-`, poi cifre in base `radix` (predefinita 10, da 2 a 36; le cifre oltre 10 in maiuscolo o minuscolo). Non c'è limite al numero di cifre (`int`). Qualsiasi altro carattere rimasto dà `Err`. Con `:junk-allowed true`, si ferma alla prima non cifra e ignora il resto, ma dà `Err` se non c'è nemmeno una cifra (corrispondente al `nil` di CL). Non restituisce il secondo valore di CL (la posizione in cui la lettura è terminata). Una `radix` fuori intervallo va in panic (un errore di chi chiama, non nel testo) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Un numero in virgola mobile. Accetta anche `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Legge una `Sexpr` da `s` (con lo stesso lettore che legge il codice sorgente). Parentesi non bilanciate, stringhe non terminate e simili danno `Err`. La lettura da uno stream è `read-sexpr` ([Stream](streams-files.md#6-funzioni-generiche-e-operazioni-sui-file)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` più **la posizione in cui la lettura è terminata**. `(car r)` è il valore e `(cdr r)` la posizione del carattere successivo da leggere. `start` è 0 per impostazione predefinita |
| `read-from-string-preserving-whitespace` | Come sopra | Come sopra | Lo stesso, ma non consuma lo spazio che ha terminato il dato. La differenza si vede nella posizione restituita |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Controlla i tipi di `form` a runtime e la valuta. Segue l'`eval` di CL |

CL restituisce **due valori** (il valore e la posizione) da `read-from-string`, ma questo linguaggio non
ha valori multipli, quindi restituisce una `cons-cell`. Avere la posizione rende la lettura di una
stringa un dato alla volta un ciclo anziché una nuova scansione:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

La differenza introdotta da `preserving-whitespace` è **un carattere di spazio**: il `read` di CL consuma
lo spazio che ha terminato il dato, e `read-preserving-whitespace` lo lascia. `(read-from-string "12 34")`
restituisce la posizione 3, e la versione preserving restituisce 2.

La sintassi dei numeri che il lettore accetta si trova nel
[capitolo 1 del Riferimento della sintassi](../syntax.md#1-elementi-lessicali). Ciò che stampa
`*print-radix*` ([Stampa](printing.md#62-base-maiuscoleminuscole-e-leggibilità)) può essere riletto così
com'è. Non esiste `*read-base*` di CL.

### 6.1 Che cosa significa `eval`

Segue l'`eval` di CLHS: valuta nell'**ambiente globale corrente** (funzioni globali, variabili, tipi e
macro, comprese le definizioni aggiunte a runtime) e nell'**ambiente lessicale nullo** (i binding locali
del `let`/`lambda` del chiamante non sono visibili). Si possono valutare sia espressioni sia definizioni
(`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`), e le definizioni vengono registrate nell'ambiente
globale immediatamente e in modo permanente.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; the global x is visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returns the defined name
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; the definition just made is visible
```

- **Valore di ritorno**: per un'espressione, il risultato come `Option<Sexpr>`; per una definizione, il
  simbolo del nome definito (come in CL). Per usare il risultato, scomponi la `Sexpr` con `match`
  (`(int n)`/`(str s)`/…).
- **Differenze dovute ai tipi statici (importante)**: CL restituisce il valore reale del risultato, ma
  in questo linguaggio il tipo di ritorno può essere solo uniformemente
  `Result<Option<Sexpr>,EvalError>`. Inoltre, **il codice scritto staticamente non può fare riferimento
  in avanti a nomi che `eval` definisce a runtime**: un `(sq 9)` scritto direttamente in un file viene
  controllato prima che venga eseguito l'`eval` che definisce `sq`, ed è "non definito". Tuttavia, **gli
  `eval` successivi possono vederlo** (il loro controllo dei tipi avviene a runtime, dopo la
  definizione). Il REPL controlla ed esegue una riga alla volta, quindi un nome definito con `eval` può
  essere chiamato direttamente dalla riga successiva.
- **Errori**: gli errori di tipo e di sintassi restituiscono `Err` (non vanno in panic). I **panic a
  runtime** nel codice valutato (divisione per zero e così via) si propagano come farebbero dal codice
  scritto direttamente. La pulizia di qualsiasi `unwind-protect` intermedio viene eseguita
  ([capitolo 8 del Riferimento della sintassi](../syntax.md#8-uscite-non-locali-catch--throw--unwind-protect)).
- **Namespace**: quando viene eseguito da `typl file.typl` e dentro un eseguibile AOT, `eval` valuta nel
  namespace del modulo dello script (le globali dello script stesso sono visibili). Il REPL valuta nel
  namespace radice.
- **Compilazione**: sia `read` sia `eval` possono essere compilati. Come vengono trattati negli
  eseguibili AOT, e le conseguenze (le forme passate a eval vengono interpretate), si trovano nel
  [Riferimento della sintassi 10.2](../syntax.md#102-eval-negli-eseguibili-aot).

## 7. Docstring / `documentation`

`defun`/`defmethod` (anche dentro `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` possono avere docstring. La posizione segue la regola di CL per ciascuno:

| Forma | Posizione della docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | All'inizio del corpo (dopo il tipo di ritorno e la clausola `where`). Solo quando segue almeno una forma del corpo; una stringa da sola resta il valore di ritorno |
| `defvar` / `defconstant` | **Dopo** il valore iniziale: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Subito dopo** il nome, prima dei campi/varianti |
| `deftype` | **Subito dopo** il nome, prima del tipo: `(deftype meters "doc" i32)` |
| `deftrait` | Subito dopo l'elenco dei supertrait, prima delle voci. Una per l'intero trait. I **metodi con un'implementazione predefinita** possono mettere la propria docstring subito prima del loro corpo |

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `documentation` | `(documentation name)` | (forma speciale; `name` è un simbolo semplice o `Type::method`)→`Option<string>` | Restituisce la docstring di `name` |

Come `quote`/`compile`, `documentation` è una forma speciale (legge `name` come nome non valutato). A
differenza del `(documentation 'name 'function)` di CL, non prende un argomento di tipo; risolve invece un
nome semplice nell'ordine **variabile → funzione → tipo → trait → macro** (la stessa priorità di un
identificatore semplice valutato come espressione). La forma `Type::method` cerca la docstring di un
metodo associato o statico.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Il valore viene deciso in fase di controllo**: se il nome non si risolve in alcuna definizione, è un
errore in fase di controllo (come fare riferimento a una variabile non definita). Se si risolve ma non c'è
una docstring, il risultato è `Option::none`.

**Non coperto**:

- `(setf documentation)` (cambiare una docstring a runtime) non esiste.
- I nomi liberi qualificati dal modulo (`mod::name`; `Type::method` è supportato) non sono supportati.
- Una dichiarazione di metodo in un `deftrait` **senza corpo** non può avere una docstring. Un letterale
  stringa finale sarebbe esso stesso il corpo (il valore di ritorno) di un'implementazione predefinita,
  quindi non c'è modo di distinguere i due casi.

Anche l'hover del language server (`typl-lsp`) mostra le docstring.

## 8. Macro

Come definire le macro è spiegato nel [Riferimento della sintassi 3.14](../syntax.md#314-defmacro--definizioni-di-macro).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Un nuovo simbolo. Il suo nome è `" <prefix><n>"`, dove `n` è `*gensym-counter*`. Uno spazio iniziale non si può scrivere nel sorgente, quindi i binding generati non confliggono mai con i nomi scritti |
| `*gensym-counter*` | Variabile | `int` | Il numero che `gensym` usa dopo. Come in CL, si può leggere e impostare |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Espande una chiamata di macro di un passo. `none` significa "non è una chiamata di macro" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Ripete finché non è più una macro |

`macroexpand-1` restituisce un `Option`. CL riporta "se è stata espansa" come secondo valore di ritorno, ma
non esistono valori multipli, quindi `none` svolge quel ruolo. **Una macro che si espande in una chiamata
di se stessa non può mai essere confusa con una non-macro.** Un passo di espansione è lo stesso che usa il
type checker, quindi ciò che il programma vede e ciò che il checker ha visto non divergono mai.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none shows as the empty list (Option<Sexpr> is transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Ciò che CL ha e questo linguaggio no: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute`
coincidono sempre, quindi non c'è distinzione da scegliere), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (simboli non internati; i binding vengono cercati per nome, quindi
non ci sarebbe nulla da guadagnare).

## 9. Binding locali di macro (`macrolet` / `symbol-macrolet`)

Entrambe sono forme speciali che associano in modo lessicale **nomi che non sono valori**. A runtime non
resta nulla: ciò che viene compilato è la forma espansa del corpo.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Un binding `macrolet` nasconde una macro globale con lo stesso nome **solo durante il corpo**. La lista
  dei parametri è la stessa di `defmacro` (`&optional`/`&rest`/`&key`).
- **I fratelli dello stesso `macrolet` non possono vedersi a vicenda dai loro *corpi*** (come in CL; è la
  differenza rispetto a `labels`). Le espansioni vengono controllate nel punto d'uso, quindi funziona
  `earlier` che si espande in `(later ...)`: entrambi sono visibili in quel punto.
- Un nome `symbol-macrolet` entra nell'ambiente come un binding ordinario. Quindi un `let` interno
  nasconde lo stesso nome, e una variabile esterna viene nascosta: le regole di CL risultano così come
  sono.
- **`setf` scrive sull'espansione.** `(setf head 42)` è `(setf (get v 0) 42)`.
- Le espansioni vengono controllate nell'**ambiente del punto d'uso** (non del punto di binding).

## 10. Altro

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Va in panic se falso. Senza messaggio, `assertion failed: <il test come scritto>` (è una macro, quindi può nominare l'espressione stessa). I restart di CL non esistono in questo linguaggio |
| `warn` | `(warn control args...)` | `(string,...)→()` | Scrive su `*error-output*` una riga con prefisso `WARNING: ` e **continua**. Un modo di segnalare qualcosa senza restituire un `Result` e senza terminare il programma |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Sostituisce le globali solo durante `body` e le ripristina all'uscita. CL lo scrive come `let`, ma `let` in questo linguaggio associa sempre in modo lessicale, da cui il nome separato (lo stesso ruolo della macro omonima di Emacs Lisp). Le ripristina in qualunque modo si esca dal corpo: completamento normale, `throw`, `panic`, `break`/`return`. **Non è un binding per task** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Esegue `body` con ogni variabile di controllo del printer al suo valore standard e `*read-eval*` impostato a `true` ([Stampa](printing.md#6-controllo-della-quantità-stampata)) |
| `exit` | `(exit code)` | `int→!` | Termina il processo |
| `dump` | `(dump path)` | `string→bool` | Scrive l'ambiente corrente (informazioni di tipo più corpi compilati) in un unico file. `typl --image <path>` riparte da esso. Solo dell'interprete ([Riferimento della sintassi 10.1](../syntax.md#101-dump)) |
