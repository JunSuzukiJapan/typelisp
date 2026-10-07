<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Task e canali

Il vocabolario dei task (thread leggeri). `task` e `thread`, che li avviano, e `select`, che attende su
più cose, sono forme speciali e si trovano nel [Riferimento della sintassi](../syntax.md#12-concorrenza-task).
Questo capitolo tratta il resto: tipi, metodi e funzioni.

I task sono **cooperativi**: un task si alterna solo nei punti che scrivi. I task vengono eseguiti
contemporaneamente su `TYPELISP_THREADS` thread del sistema operativo (in `typl`, solo i task compilati
passano ad altri thread). Dove si alternano e in che cosa differiscono da Go è spiegato nel
[Riferimento della sintassi 12.5](../syntax.md#125-dove-i-task-si-alternano) e in
[12.7](../syntax.md#127-differenze-rispetto-a-go).

## 1. `Task<T>` — handle dei task

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Attende il completamento e ne restituisce il valore |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Puoi fare `wait` un numero qualsiasi di volte** (il valore viene messo in cache). A differenza di
  `JoinHandle::join` di Rust, non consuma l'handle, quindi si può attendere da più punti.
- **Un task viene eseguito anche se non fai mai `wait`.** Scartare l'handle non lo ferma.
- È un valore ordinario, quindi può stare in un `Vector<Task<()>>`.
- **Quando il task principale termina, il processo termina** (come in Go). Gli altri task in esecuzione
  vengono interrotti, e la pulizia di `unwind-protect` non viene eseguita, perché si tratta della
  terminazione del processo, non dello svolgimento dello stack.

## 2. `Chan<T>` — canali

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Un canale di capacità `n`. `0` è un rendezvous (nessun buffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Attende finché c'è spazio, poi consegna il valore |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Attende finché non arriva un valore. `none` una volta che è chiuso e vuoto |
| `close` | `(close ch)` | `(Chan<T>)→()` | Lo chiude |
| `len` | `(len ch)` | `(Chan<T>)→int` | Quanti valori ci sono ora nel buffer |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | La capacità |

**L'argomento di tipo si indica con `the`** (scritto come `(the Vector<i32> (Vector::new))`). **La
capacità va sempre scritta**: i due casi che Go scrive `make(chan int)` e `make(chan int, 16)` si
scrivono `(Chan::new 0)` e `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **Un `Chan<T>` è il proprio iteratore** (implementa `Iter`). `recv` restituisce un `Option<T>`, lo stesso
  tipo di `Iter::next`, quindi `doiter` e `map`/`filter`/`foldl` funzionano tutti su di esso così come
  sono.
- **`send` su un canale chiuso va in panic**, e **anche un secondo `close` va in panic** (entrambi come
  in Go). Sono bug del programma, non fallimenti recuperabili, quindi non sono `Result`.
- **Chiudere un canale su cui un task sta attendendo per fare `send` fa andare in panic quel task** (la
  regola di Go).
- `recv` su un canale chiuso restituisce ciò che resta nel buffer, e una volta vuoto continua a
  restituire `none`.
- `close` viene risolto in base al tipo del ricevitore, quindi è una cosa diversa dal `close` del trait
  `Stream`. `Chan<T>` non implementa `Stream`.
- **Una capacità negativa va in panic** (non viene arrotondata in silenzio a 0).

## 3. `yield` / `sleep` — cedere il passo

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Rinuncia al resto del proprio turno (il `runtime.Gosched` di Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Ferma **solo quel task**. Gli altri continuano |

`sleep` ferma un task, non un thread. Solo quando nessun task può essere eseguito entra in un `sleep` del
sistema operativo fino alla scadenza più vicina. `(sleep 0.0)` è il "cedi il passo per 0 secondi" di CL.

Come in CL, `sleep` prende **secondi**. Gli interi non vengono convertiti automaticamente in numeri in
virgola mobile, quindi il `(sleep 1)` di CL qui si scrive `(sleep 1.0)`. Un valore negativo o NaN va in
panic.

## 4. `WaitGroup` — attendere N completamenti

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Un gruppo senza nulla in sospeso |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Aggiunge al contatore. Fallo prima che il lavoro inizi |
| `done` | `(done wg)` | `(WaitGroup)→()` | Uno ha terminato. A 0, ogni chi attende viene rilasciato |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Attende finché non raggiunge 0. Da un numero qualsiasi di task |

`(wait wg)` e `(wait task)` vengono risolti in base al tipo del ricevitore, quindi coesistono con un solo
nome. Se il contatore scende sotto 0, va in panic (`done` chiamato troppe volte, oppure un `add`
negativo). Come in Go, un gruppo tornato a 0 può essere riusato a partire da `add`. Nessun aggiornamento
va perso nemmeno quando i task vengono eseguiti su thread del sistema operativo distinti.

**Avendo a disposizione `Task<T>`, serve meno che in Go**: spesso basta `(doiter (t tasks) (wait t))`. È
uno strumento per il lavoro che cresce dinamicamente, o per quando non vuoi conservare gli handle.

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — un canale che consegna dopo un certo tempo

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Un canale che consegna un valore dopo `sec` secondi |

Il `time.After` di Go. Si può scrivere così com'è nel ramo di timeout di `select`
([Riferimento della sintassi 12.3](../syntax.md#123-select--attesa-di-più-operazioni-su-canali-contemporaneamente)). La
sua capacità è 1, quindi il task che invia può terminare anche se nessuno riceve.

## 6. `Mutex<T>` — mutua esclusione per dati condivisi

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Un mutex sbloccato che contiene `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Acquisisce il lock (attende) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Lo rilascia. Va in panic se non è bloccato |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Blocca, associa il contenuto a `x`, esegue `body` e rilascia **sempre** |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` non è una copia del valore ma un "place"** (`symbol-macrolet`). `(setf x 42)` cambia il contenuto
  del mutex.
- `with-lock` rilascia con `unwind-protect`, quindi il lock viene rilasciato in qualunque modo si esca
  dal corpo: completamento normale, `throw`, `panic`, oppure `break`/`return`/`return-from`.
- **Rientrare produce un deadlock** (non va in panic). Lo scheduler segnala che "nulla può fare
  progressi" per un task bloccato sul proprio lock.
- **`m::v` tocca il contenuto fuori dal lock**, il che è indefinito nel senso che un altro task potrebbe
  essere nel mezzo di una modifica. È la stessa posizione di `sync.Mutex` di Go: in un linguaggio senza
  controllo di ownership o di borrow, non si può costruire una garanzia statica come `MutexGuard`.

## 7. `Thread<T>` — thread del sistema operativo dedicati

L'handle restituito da `(thread (f args...))`
([Riferimento della sintassi 12.2](../syntax.md#122-thread--avvio-di-un-task-su-un-thread-os-dedicato)). La
controparte di `Task<T>`.

| Nome | Uso | Tipo | Significato |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Attende il completamento e restituisce il valore (si ferma il **task** chiamante. Può essere chiamato un numero qualsiasi di volte; il valore viene messo in cache) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | La versione funzione di `(thread (f))` (il `std::thread::spawn` di Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Il numero del thread del sistema operativo in esecuzione. Unico all'interno del processo, senza altro significato che "è lo stesso thread" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Il numero di thread che la macchina può eseguire contemporaneamente (il valore predefinito di `TYPELISP_THREADS`). Va in panic se il sistema operativo non risponde |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- Chiamare una funzione C bloccante (`defffi`) ferma solo quel thread.
- Un `task` dentro un `thread` viene eseguito come task ordinario su altri thread.
- Si può usare anche in `typl`. Quando interpreta, `(thread (f ...))` e `Thread::spawn` compilano al volo
  la funzione da eseguire e poi la eseguono sul thread dedicato. Una `lambda` che fa riferimento a
  variabili locali esterne non può essere compilata da sola e va in panic
  ([Riferimento della sintassi 12.2](../syntax.md#122-thread--avvio-di-un-task-su-un-thread-os-dedicato)). Una `lambda` creata dentro una
  funzione compilata può essere passata.

## 8. Che cosa non c'è

- **`Atomic`**. `Mutex` basta.
- **Variabili locali ai task** (nemmeno Go le ha).
- **Canali nil**. Il motivo e l'alternativa si trovano nel
  [Riferimento della sintassi 12.7](../syntax.md#127-differenze-rispetto-a-go).
