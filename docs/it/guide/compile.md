<!-- translated-from: docs/ja/guide/compile.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Compilazione

Se non fai altro, i programmi typelisp vengono eseguiti nell'interprete. Inoltre esistono due modi per
compilare in codice nativo e un modo per salvare un ambiente. I dettagli della specifica si trovano nel
[capitolo 10 del Riferimento della sintassi](../reference/syntax.md#10-compilazione).

| Metodo | Come | Risultato |
|---|---|---|
| Compilazione JIT | `(compile name)` | Una funzione della sessione in esecuzione viene sostituita da codice nativo |
| Compilazione AOT | `typl -c src.typl` o `(compile-file "src.typl" "out")` | Un eseguibile autonomo |
| Dump | `(dump "file.typld")` | Salva le definizioni; `typl --image` riparte dallo stesso ambiente |

## 1. Preparazione

La compilazione usa LLVM 22. Se hai compilato `typl` seguendo il [README.md](../../../README.md), non è
necessaria nessun'altra preparazione.

Gli eseguibili creati con la compilazione AOT vengono collegati alla libreria statica
`libtypelisp_front.a`. Una build di release di `typl` (inclusa quella installata con `cargo install`)
porta questa libreria al proprio interno, quindi non serve alcuna preparazione. La prima volta che
compila, scrive la libreria in `~/.typelisp/lib/<ID della build>/` (oppure in
`$TYPELISP_HOME/lib/<ID della build>/` se è impostata la variabile d'ambiente `TYPELISP_HOME`) e da
quel momento usa quella copia. `typl --remove-lib` la elimina (con `--others`, quelle scritte da altre
versioni di `typl`; con `--all`, tutte). Una build di debug di `typl` usa la libreria in
`target/debug/` del repository in cui è stata compilata. Per usarne una collocata altrove, indica la
sua cartella con `--lib-dir` quando avvii `typl` (sezione 3.2).
Su macOS, il collegamento usa gli Xcode Command Line Tools.

## 2. Compilazione JIT

Questa trasforma al volo in codice nativo una funzione già definita.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; from here on, calls run the compiled code
```

- `name` non viene valutato. Scrivi il nome della funzione così com'è (non come stringa). Per un
  metodo, scrivilo con il nome del tipo, come in `(compile point::norm)`.
- Le funzioni che essa chiama vengono compilate insieme ad essa.
- **Le funzioni generiche non possono essere compilate.** Una copia per ciascun tipo viene creata in
  ogni punto in cui sono usate. Compila invece la funzione che le chiama con tipi concreti.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` e `dump` sono operazioni
  dell'interprete, quindi una funzione che le chiama non può essere compilata. Tentare di compilarla
  produce un errore che ne indica il motivo.

Per vedere il risultato della compilazione, usa `disassemble`.

```lisp
(disassemble fib)          ; the host machine code
(disassemble fib true)     ; LLVM IR
```

## 3. Creazione di un eseguibile con la compilazione AOT

### 3.1 Scrivere il programma

Come punto di ingresso, definisci una **funzione `main` che non accetta argomenti**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

Il `(main)` alla fine del file serve a far chiamare `main` quando esegui `typl hello.typl`.
`compile-file` salta questo `(main)` finale, quindi lo stesso file funziona sia nell'interprete sia
con la compilazione AOT.

### 3.2 Compilare

Dalla riga di comando, usa `typl -c` (`typl --compile` è equivalente).

```sh
$ typl -c hello.typl            # makes hello
$ typl -c hello.typl -o fib     # names the executable fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Senza `-o`, l'eseguibile prende il nome del file sorgente senza `.typl` e viene collocato nella stessa
cartella del file sorgente. Se il nome del file sorgente non termina con `.typl`, `-o` è obbligatorio.
Con `-c` (`--compile`), non si possono indicare `--image`, `--heap-cells` e `--feature`.

Puoi fare lo stesso chiamando `compile-file` dal REPL o da un programma.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Se compili ripetutamente, puoi mettere questa riga in un file ed eseguirla con `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

I nomi dei file vengono risolti a partire dalla **directory corrente in cui è stato avviato `typl`**,
non dalla posizione di `build.typl`.

Per collegare una `libtypelisp_front.a` collocata in un posto diverso da quello in cui `typl` cerca,
indica la sua cartella con `--lib-dir`. Vale sia per `typl -c` sia per `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Se la cartella indicata non contiene `libtypelisp_front.a`, `typl` si ferma con un errore. Il file
funziona solo con il `typl` compilato insieme ad esso. Dopo aver ricompilato `typl`, copialo di nuovo.

### 3.3 Che cosa può contenere un file compilato con AOT

- Il primo livello del file di ingresso può contenere solo definizioni (`defun` `defmethod` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) e `use` `module`. Le espressioni di primo livello
  come `(println ...)` non sono ammesse, tranne il `(main)` finale. Metti il lavoro dentro `main`.
- Senza una `main` che non accetta argomenti, la compilazione fallisce con un errore.
- Anche i file dei moduli importati con `use` vengono compilati e combinati in un unico eseguibile.
- Le librerie indicate con `:library` in `defffi` vengono collegate automaticamente ([FFI C](ffi.md)).
- Ogni funzione della libreria standard può essere usata con la compilazione AOT. Si può usare anche
  `eval`, ma in tal caso il type checker e l'interprete entrano nell'eseguibile, che diventa più
  grande e più lento ad avviarsi. I programmi che non chiamano `eval` non li includono.

### 3.4 Come si comporta l'eseguibile

- `(command-line-args)` restituisce un `Vector<string>` della stessa forma sia che il programma venga
  eseguito come `typl hello.typl a b` sia come `./hello a b`. Il primo elemento è il nome del
  programma.
- Imposta il codice di uscita con `(exit n)`. Se `main` ritorna normalmente, è 0.
- In caso di `panic`, il programma stampa il messaggio ed esce con un codice diverso da zero.

## 4. Dump

Puoi salvare in un unico file le definizioni della sessione corrente e ripartire da esso la volta
successiva.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Funziona anche per eseguire un file, come in `typl --image session.typld prog.typl`.

- Ciò che viene salvato sono le **definizioni**. Le espressioni valutate nella sessione non vengono
  salvate.
- Le funzioni che hai `compile`ato vengono salvate nella loro forma compilata.
- Le variabili globali vengono ripristinate **eseguendo di nuovo i loro inizializzatori**, non con i
  valori che avevano quando il dump è stato scritto.
- Un dump non può essere caricato da un `typl` di versione diversa da quella che lo ha scritto (è un
  errore).

Se esegui un file e da esso fai `(dump ...)`, le definizioni di quel file si trovano in un modulo che
porta il nome del file. Una funzione definita in `dp.typl` si chiama `dp::sq`, e chiamarla da un altro
file richiede `pub` ([Moduli e organizzazione dei file](modules.md)).

## 5. I file di modulo compilati

Non esiste un formato, come il `.fasl` di Common Lisp, per scrivere su file il risultato compilato di
ciascun modulo. `compile-file` costruisce l'eseguibile direttamente dai sorgenti. Non vengono lasciati
file intermedi.
