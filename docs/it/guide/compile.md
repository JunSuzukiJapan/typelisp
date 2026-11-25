<!-- translated-from: docs/ja/guide/compile.md @ be54d7f19b5f87846f94e73219d59bce4604fa51 -->
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
  TLS, risoluzione dei nomi), thread del sistema (`Thread`), funzioni C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  ed `ed`.
- WASI non ha modo di conoscere il fuso orario, quindi l'ora locale è sconosciuta. `decode-universal-time` ed `encode-universal-time` senza `zone`, e `get-decoded-time`, vanno in panic durante l'esecuzione. Funzionano se si passa `zone`.
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
- L'ora locale (`decode-universal-time` senza `zone`, ecc.) si calcola con il fuso orario che riporta `Date` di JavaScript.
- Oltre a ciò che non si può usare nella sezione 3.6, le operazioni su file e directory, le variabili d'ambiente, `(command-line-args)`, la directory home e `dribble` sono un **errore di compilazione**. L'input standard viene letto come vuoto.

### 3.8 Compilare per un microcontrollore (Cortex-M)

Con `--target thumbv7em-none-eabihf`, `typl -c` crea un eseguibile (ELF) per una scheda Cortex-M4F o M7 senza sistema operativo. Senza `-o`, prende il nome del sorgente con `.typl` sostituito da `.elf`. Con `--target thumbv8m.main-none-eabihf`, è per una scheda Cortex-M33.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # crea hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # eseguilo sul Cortex-M4 di QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # eseguirlo sul Cortex-M33 di QEMU
```

- Un `typl` installato con `install.sh` si usa così com'è. Per installare con `cargo install`, esegui `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf` e poi installa con `--features embedded` (`--features wasm,embedded` per entrambi). Con una build di debug di `typl` nel repository, esegui `scripts/build-embedded-runtime.sh`. Il collegamento usa il linker della toolchain di rustup.
- Indica la memoria della scheda con `--linker-script FILE`. Si scrive come il `memory.x` di `cortex-m-rt`: indirizzi e dimensioni di FLASH e RAM. Una riga come `_typelisp_stack_size = 32K;` stabilisce quanta RAM occupa lo stack (64KiB se non indicato); il resto della RAM è l'heap. Senza l'opzione, la disposizione è quella del `mps2-an386` (`mps2-an505` per un Cortex-M33) di QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` sceglie chi fornisce output, input, terminazione e orologio (le funzioni della scheda).
  - `semihosting` (predefinito): tramite semihosting, per eseguire sotto QEMU (`-semihosting-config enable=on`) o un debugger.
  - `c`: la scheda le fornisce come funzioni C. Passa con `--link FILE` un oggetto o una libreria statica che definisce le funzioni seguenti (`--link` si può ripetere; una libreria del produttore chiamata da `defffi` si passa allo stesso modo).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 è l'output standard, 2 l'errore standard
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; byte letti, 0 alla fine
  void     typelisp_host_exit(int32_t code);                              // non ritorna
  uint64_t typelisp_host_now_us(void);                                    // microsecondi di un orologio che non torna mai indietro
  int64_t  typelisp_host_unix_us(void);                                   // microsecondi dal 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // attende us microsecondi
  ```

- I registri di una scheda (periferiche mappate in memoria) si leggono con `volatile-read-u8`, `volatile-read-u16` e `volatile-read-u32`, e si scrivono con `volatile-write-u8`, `volatile-write-u16` e `volatile-write-u32`. L'indirizzo è un `int` e il valore letto ha il tipo della larghezza (`u8` e così via). Ognuno legge o scrive esattamente una volta, dove è scritto; il compilatore non lo omette né lo sposta. Tutti si possono usare solo dentro `unsafe`. Un indirizzo negativo, o non multiplo della larghezza, è un errore a runtime prima di leggere o scrivere qualsiasi cosa.

  ```lisp
  ;; UART 0 del mps2-an386 di QEMU: dati 0x40004000, stato 0x40004004, controllo 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; attendere mentre trasmette
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; abilitare la trasmissione
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" e un a capo
  ```

- La funzione chiamata quando arriva un interrupt si scrive come un `defun` con l'attributo `#[interrupt(N)]` (l'interrupt di periferica `N`) o uno come `#[exception(SysTick)]`. Un interrupt si abilita con `(enable-interrupt N)`. Un gestore si esegue subito; solo se arriva a metà del lavoro del runtime si esegue dopo che quel lavoro è finito. Un gestore non può attendere (`sleep` è un errore di compilazione). I dettagli sono nella sezione 3.17 del [riferimento della sintassi](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; sollevare da sé l'interrupt 5 (il registro dei pendenti del NVIC)
  ```

- Il runtime avvia il programma (la tabella dei vettori e la preparazione della RAM) e alloca la memoria. Un HardFault o un interrupt non gestito termina il programma con il codice di uscita di un panic (1).
- `throw`/`catch`/`panic`, bignum, frazioni e numeri in virgola mobile, e task, canali e `sleep` si comportano come nell'eseguibile. I task vengono eseguiti a turno sull'unico core. Si possono usare funzioni C (`defffi`).
- Un programma che usa ciò che la scheda non ha è un **errore di compilazione**: oltre a ciò che non si può usare nella sezione 3.7, `eval` e `macroexpand`. L'ora locale è sconosciuta (come nella sezione 3.6).
- L'arena dei cons parte da 1024 celle e cresce quando serve. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` e `--release` funzionano così come sono.

### 3.9 Compilare per Arduino Uno (AVR)

Con `--target avr-atmega328p` si crea un eseguibile (ELF) per il microcontrollore di Arduino Uno (ATmega328P). Il programma seguente fa lampeggiare il LED della scheda.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; imposta come uscita
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; inverte
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # crea blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # esegue in QEMU
```

- Non c'è un runtime, quindi un programma che lo usa è un **errore di compilazione**. L'errore indica che cosa serve e attraverso quali funzioni ci si arriva. Si possono usare: i calcoli sugli interi a larghezza fissa (da `i8` a `u32`), `bool` e `char`, le chiamate di funzione, le condizioni, i cicli, `defvar` e `defconstant` con questi valori, e `volatile-read-u8` e simili con l'indirizzo scritto come numero o costante.
- Non si possono usare: i valori che stanno nello heap, come stringhe, liste e strutture; i calcoli su `int`; la ricorsione, `catch` e le chiamate di valori funzione; le chiamate di funzioni C; gli accessi `volatile` con indirizzo calcolato.
- Un gestore di interrupt si scrive `#[interrupt(N)]` ([Sintassi](../reference/syntax.md), sezione 3.17). `N` è il numero dell'interrupt dell'ATmega328P, da 1 a 25 contando il reset come 0 (l'overflow del timer 1 è 13). Vedi l'esempio sotto.
- Una divisione per 0 ferma il microcontrollore in quel punto (non arrivano più nemmeno interrupt). Lo stesso vale per `/` del più piccolo valore con segno per `-1`.
- Un ciclo che non fa nulla scompare da `-O1` in su. In un ciclo di attesa, per esempio, si legga un registro.
- Quando `main` termina, il microcontrollore si ferma.
- La disposizione della memoria si cambia con `--linker-script FILE`. Se omesso, sono 32 KiB di flash meno gli ultimi 512 byte del bootloader, e 2 KiB di RAM. Gli indirizzi di RAM si scrivono sommando `0x800000`, come `0x800100`.
- I calcoli su `f32` e `f64` sono un errore di link, perché non ci sono funzioni ausiliarie per AVR.
- Il link usa il linker della toolchain di rustup, come nella sezione 3.8.

Un esempio che fa lampeggiare il LED con l'interrupt di un timer. Un interrupt si abilita nel registro della periferica.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; overflow del timer 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; inverte
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; abilita l'interrupt di overflow
          (volatile-write-u8 TCCR1B 3))           ; conta a 1/64 del clock
  (while true))
```

- Un programma con un gestore accetta gli interrupt appena le variabili globali sono inizializzate. Configurare le periferiche all'inizio di `main`.
- `enable-interrupt` e `#[exception(NAME)]` non si possono usare (errore di compilazione).
- Mentre un gestore è in esecuzione, gli altri interrupt attendono. Come ogni altra funzione, un gestore non può usare il runtime.
- Una singola lettura o scrittura di una variabile globale non viene interrotta da un gestore, anche per valori di due byte o più. Tra la lettura e la scrittura di un aggiornamento (`(setf n (+ n 1))`) può arrivare un interrupt. Per aggiornare una variabile scritta anche da un gestore, leggere e conservare il registro di stato (indirizzo `#x5F`), scriverci 0 per fermare gli interrupt e poi riscrivere il valore conservato.
- Un interrupt senza gestore ferma il microcontrollore.

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
