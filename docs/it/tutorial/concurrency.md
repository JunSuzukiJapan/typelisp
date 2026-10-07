<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Concorrenza

In typelisp si esegue il lavoro in modo concorrente avviando **task** (thread leggeri), e i task si
passano valori attraverso **canali**. Il modello è vicino alle goroutine e ai canali di Go. Questo
capitolo tratta, nell'ordine, l'avvio di un task e l'ottenimento del suo risultato, i canali, `select`,
la protezione dei dati condivisi e i thread del sistema operativo dedicati. Presume che tu abbia letto
[Fondamenti dei tipi](types.md).

## 1. Avviare un task e attendere il suo risultato

`(task (funzione argomenti...))` avvia una chiamata di funzione come nuovo task. Il lato che avvia non
attende e prosegue. Il valore è un handle di tipo `Task<T>`; `(wait handle)` attende che il task
termini e ne restituisce il risultato.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` accetta solo la forma di una chiamata di funzione. Gli argomenti vengono valutati dove è scritto
  il `task`; solo la chiamata in sé viene eseguita nel nuovo task.
- Per eseguire più espressioni, crea una `lambda` e chiamala subito:
  `(task ((lambda () () (println "start") (work))))`
- Puoi chiamare `wait` tutte le volte che vuoi. Il risultato viene ricordato.
- Un task viene eseguito anche se non fai mai `wait` su di esso.
- **Quando il lavoro principale termina, il programma finisce.** I task ancora in esecuzione vengono
  interrotti.

## 2. Passare valori attraverso i canali

Un canale `Chan<T>` è un percorso attraverso il quale i task si passano valori di tipo `T`. L'argomento
di `Chan::new` è la capacità (quanti valori può contenere). Su un canale di capacità 0, sia chi invia
sia chi riceve attendono finché l'altro lato non c'è.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; send
  (close ch))                  ; no more sends

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; receive until it is closed
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` invia. Se il canale è pieno, attende finché non c'è spazio.
- `(recv ch)` riceve. Attende finché non arriva un valore. Il risultato è un `Option<T>`; una volta che
  il canale è chiuso e vuoto, restituisce `none`.
- Iterare un canale con `doiter` continua a ricevere valori finché non viene chiuso. Può anche essere
  passato direttamente a `map` o `filter`.
- `send` su un canale chiuso va in panic.

### Dividere il lavoro tra più task

Un modello comune è predisporre un canale che trasporta il lavoro e far sì che più worker (task che
elaborano il lavoro) prendano da esso i job. Il worker libero prende il job successivo, quindi anche
quando job lenti e veloci sono mescolati, il lavoro si distribuisce in modo naturale.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; how long this job takes

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; take one job at a time until jobs is closed
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; working; meanwhile other workers take the next jobs
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; how many jobs this worker did

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; that is all the work
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- A, B e C prendono ciascuno uno dei primi tre job.
- Dopo 0,1 secondi B e C sono liberi e prendono i job rimanenti. Mentre A è occupato con il lento job 1,
  non prende nuovo lavoro.
- Alla fine A ha gestito un job, B due e C tre. Nulla nel programma dice quale worker prende quale job.
- Chiudere `jobs` termina il `doiter` di ciascun worker, i task finiscono e ogni `wait` restituisce il
  suo conteggio.

`jobs` è un canale di capacità 0, quindi `send` attende finché un worker non prende il job. Con una
capacità maggiore, il task principale potrebbe mettere in coda il lavoro senza attendere i worker.

## 3. `select`: attendere su più canali contemporaneamente

`select` esegue quella tra più operazioni su canali che diventa possibile per prima. `(after seconds)` è
un canale che consegna un valore una volta trascorso il tempo indicato. Combinato con `select`, fornisce
un timeout.

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) body...)` è un ramo di ricezione. `v` riceve un `Option<T>`.
- `((send ch x) body...)` è un ramo di invio.
- Quando più rami possono procedere nello stesso momento, ne viene scelto uno a caso.
- Con `(else body...)` alla fine, `else` viene eseguito quando nessun ramo può procedere subito, e
  `select` non attende.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Proteggere i dati condivisi

Quando più task modificano lo stesso valore, proteggilo con `Mutex<T>`. `with-lock` acquisisce il lock,
associa il contenuto a una variabile, esegue il corpo e rilascia sempre il lock in qualunque modo si
esca dal corpo. Assegnare alla variabile con `setf` dentro il corpo cambia il contenuto del `Mutex`.

`WaitGroup` è uno strumento per attendere che un dato numero di task abbia terminato. Aumenta il
contatore con `add`, fai chiamare `done` a ciascun task quando finisce, e fai `wait` finché il contatore
non raggiunge 0.

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

Se più task modificano lo stesso valore nello stesso momento senza passare per un `Mutex` o un canale,
il risultato non è garantito. Passa i dati tra i task attraverso i canali dove puoi, e condividi i dati
solo quando ne hai bisogno.

## 5. Dove i task si alternano

I task si alternano in modo cooperativo. Un task cede il passo ad altri task solo in questi punti:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- Un'operazione su un canale che deve attendere (`send`, `recv`, `select`)
- Un'operazione su un socket che deve attendere (connessione, lettura, scrittura e così via)

L'argomento di `sleep` è un numero di secondi `f64`. Scrivi `(sleep 1.0)`, non `(sleep 1)`.

I task vengono eseguiti contemporaneamente su più thread del sistema operativo. Tuttavia, quando `typl`
esegue direttamente un programma, solo i task che eseguono funzioni [compilate](../guide/compile.md)
passano ad altri thread. Gli altri task vengono eseguiti su un unico thread, alternandosi nei punti
sopra indicati.

## 6. `thread`: eseguire su un thread del sistema operativo dedicato

Il lavoro che non deve bloccare gli altri task, come la chiamata di una lenta funzione C ([FFI C](../guide/ffi.md)),
si avvia con `thread`. Si scrive come `task` e ottiene un thread del sistema operativo tutto suo.
Attendi la sua terminazione con `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- L'handle restituito da `thread` ha tipo `Thread<T>`. Come `wait`, `join` può essere chiamato un numero
  qualsiasi di volte.
- `thread` può eseguire solo funzioni che possono essere compilate. Quando viene eseguito in `typl`, la
  funzione che chiama viene compilata al volo prima dell'esecuzione.

## 7. Task e altre funzionalità

- Un `panic` dentro un task ferma l'intero programma.
- `throw` non raggiunge l'esterno di un task. Un `throw` che uscirebbe dal corpo del task diventa un
  `panic`.
- L'output di un singolo `println` non viene mai mescolato in mezzo a una riga con l'output di altri
  task.

## 8. Che cosa leggere dopo

- [Task e canali](../reference/functions/concurrency.md): l'elenco delle funzioni
- [Capitolo 12 del Riferimento della sintassi](../reference/syntax.md#12-concorrenza-task): i punti in cui i task si
  alternano nel dettaglio e le differenze da Go
- [I/O su file, stream e rete](../guide/io.md): scrivere un server con i task
