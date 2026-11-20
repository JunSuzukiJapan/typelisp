<!-- translated-from: docs/ja/guide/compile.md @ eb0e7d31e1d6e61f64d90fcfb4f0dc5e6bfe813f -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Senza `-o`, l'eseguibile prende il nome del file sorgente senza `.typl` e viene collocato nella stessa
cartella del file sorgente. Se il nome del file sorgente non termina con `.typl`, `-o` è obbligatorio.
Con `-c` (`--compile`), non si possono indicare `--image` e `--feature`.

Con `--release`, `typl` produce una **build release**, che rimuove `eprint`/`eprintln` (output su standard error per l'indagine; [Stampa](../reference/functions/printing.md)) insieme ai loro argomenti. Ciò che rimuove viene comunque controllato nei tipi, quindi un errore di tipo lì fa fallire la compilazione. `--release` è un'opzione di `typl` nel suo insieme: vale allo stesso modo non solo per `-c`, ma anche quando si esegue un file, nella REPL e per `typl test`. Anche un `compile-file` chiamato dentro un `typl` avviato con `--release` produce una build release, e `eprint`/`eprintln` vengono rimossi anche dal codice valutato con `eval` dentro l'eseguibile così prodotto.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # uguale se scritto dopo -c
```

`-O0`, `-O1`, `-O2` e `-O3` impostano il **livello di ottimizzazione**. Il valore predefinito è `-O0`, che non ottimizza. Da `-O1` in su il compilatore esegue prima ottimizzazioni semplici, come sostituire una variabile legata solo a un letterale con il suo valore o calcolare in anticipo l'aritmetica intera su costanti, e lascia il resto all'ottimizzazione di LLVM del livello con lo stesso nome. A ogni livello i risultati del calcolo sono gli stessi. Come `--release`, è un'opzione di `typl` nel suo insieme: oltre all'eseguibile creato da `-c`, vale per il codice compilato durante l'esecuzione di un file, nella REPL e in `typl test`, e per un `compile-file` chiamato lì. Il codice eseguito dall'interprete non viene ottimizzato.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # uguale se scritto dopo -c
```

Puoi fare lo stesso chiamando `compile-file` dal REPL o da un programma.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- Il primo livello del file di ingresso può contenere solo definizioni (`defun` `defvar`
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

### 3.5 Risorse usate dall'eseguibile

`typl -c` ha opzioni che indicano quali risorse l'eseguibile può usare durante l'esecuzione.

```sh
$ typl --heap-cells 100000 -c hello.typl    # fissa l'arena cons a 100000 celle
$ typl -c hello.typl --max-memory 64M       # alloca al massimo 64 MiB
$ typl -c hello.typl --no-cons              # non usa celle cons
```

- `--heap-cells N` fissa la capacità dell'arena delle celle cons a `N` celle. L'arena non cresce oltre: se la garbage collection non riesce a liberare una cella, il programma si ferma con un errore di esecuzione. Senza l'opzione, l'arena parte da 65536 celle e cresce quando serve.
- `--max-memory SIZE` limita la memoria totale che l'eseguibile alloca. Conta tutto: l'arena cons, le stringhe, valori come vettori e tabelle hash, e la memoria del runtime stesso. `SIZE` è un numero di byte e può essere seguito da `K`, `M` o `G` (1024, 1024² e 1024³ volte). Un'allocazione che supererebbe il limite viene segnalata e il programma termina con codice di uscita 1. Questa uscita non si può intercettare con `catch`. Senza l'opzione non c'è limite.
- `--no-cons` crea un eseguibile che non usa celle cons. Non ha un'arena cons, quindi un programma che fa qualcosa che richiede celle cons è un **errore di compilazione**, che mostra quell'operazione e la catena di funzioni da `main` che la raggiunge. Le celle cons servono per costruire una lista `Sexpr` (anche una lista quotata o costruita con il quasiquote) o un argomento `&rest`, e per `read`, `eval` e `macroexpand`. Le operazioni su numeri, stringhe, vettori, tabelle hash, struct, canali e simili, e `print`/`println`/`format`, non ne usano. Non si può indicare insieme a `--heap-cells` e si scrive dopo `-c`.

`--max-memory` si scrive dopo `-c`. `--heap-cells` si può scrivere prima o dopo `-c`, e allora è la capacità dell'arena cons dell'eseguibile. L'arena cons conta nel limite di memoria, quindi con un limite in cui l'arena non entra il programma si ferma prima di iniziare.

### 3.6 Compilare per WebAssembly

Con `--target wasm32-wasip1`, `typl -c` crea un modulo WebAssembly (un comando WASI preview 1) invece di un eseguibile. Senza `-o`, prende il nome della sorgente con `.typl` sostituito da `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # crea hello.wasm
$ wasmtime hello.wasm a b                        # eseguirlo con un ambiente che supporta WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- Un `typl` installato con `install.sh` si usa così com'è. Uno installato con `cargo install` richiede una preparazione (che vale anche per la sezione 3.7). Prima eseguite `rustup target add wasm32-wasip1 wasm32-unknown-unknown` con rustup. Poi installatelo con `--features wasm` (`cargo install --locked --path . --features wasm`).
  Un `typl` installato senza non ha il runtime WebAssembly, e `--target` è un errore. Per una build di debug di `typl` nel repository, eseguite invece `scripts/build-wasm-runtime.sh` nel repository.
- Il collegamento usa il linker della toolchain di rustup (`rust-lld`), quindi rustup serve anche durante la compilazione.
- Il modulo si comporta come l'eseguibile: output standard, errore standard e codice di uscita, `(command-line-args)`,
  lettura e scrittura di file, `throw`/`catch`/`panic`, task, canali e `sleep`, ed `eval` funzionano.
  I task vengono eseguiti uno dopo l'altro su un solo thread. I file si possono leggere e scrivere solo nelle
  directory che l'ambiente mostra al modulo (con wasmtime si indicano con `--dir`).
- Un programma che usa qualcosa che WebAssembly non ha è un **errore di compilazione**. L'errore mostra cosa è stato usato
  e la catena di funzioni da `main` che ci arriva. Non si possono usare: socket (TCP, UDP, dominio Unix,
  TLS, risoluzione dei nomi), thread del sistema (`Thread`), funzioni C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  il fuso orario locale ed `ed`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` e `--release` valgono come per un eseguibile.

### 3.7 Eseguire in un browser o in Node.js

Con `--target wasm32-unknown-unknown`, `typl -c` crea un modulo WebAssembly da caricare ed eseguire da JavaScript. Accanto al modulo (`hello.wasm`), nella stessa cartella, scrive il JavaScript che lo carica e lo esegue (`hello.mjs`). La preparazione è la stessa della sezione 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # crea hello.wasm e hello.mjs
$ node hello.mjs                                       # eseguirlo con Node.js
args: #()
fib(25) = 75025
```

Da altro JavaScript, chiamate `run` di `hello.mjs`. `write` riceve l'output come testo (`fd` è 1 per l'output standard, 2 per l'errore standard). Il valore di `run` è il codice di uscita.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* mostrarlo nella pagina, per esempio */ } });
```

- Senza `write`, Node.js scrive sull'output standard e sull'errore standard, un browser sulla console, una riga alla volta.
- In un browser, servite `hello.wasm` via HTTP dallo stesso posto di `hello.mjs` (non si può caricare da `file://`).
- `sleep` attende con `Atomics.wait`. Un browser lo consente solo dentro un Worker di una pagina con isolamento cross-origin, quindi eseguite in un Worker i programmi che usano `sleep`.
- Oltre a ciò che non si può usare nella sezione 3.6, le operazioni su file e directory sono un **errore di compilazione**. L'input standard si legge come vuoto e `(command-line-args)` è un vettore vuoto.

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
