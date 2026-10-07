<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Gelijktijdigheid

In typelisp voer je werk gelijktijdig uit door **taken** (lichtgewicht threads) te starten, en taken
geven waarden aan elkaar door via **kanalen**. Het model lijkt op de goroutines en kanalen van Go.
Dit hoofdstuk behandelt achtereenvolgens het starten van een taak en het ophalen van zijn resultaat,
kanalen, `select`, het beschermen van gedeelde gegevens en eigen OS-threads. Het gaat ervan uit dat je
[Basis van types](types.md) hebt gelezen.

## 1. Een taak starten en op zijn resultaat wachten

`(task (functie argumenten...))` start een functieaanroep als nieuwe taak. De startende kant wacht
niet en gaat door. De waarde is een handle van het type `Task<T>`; `(wait handle)` wacht tot de taak
klaar is en geeft zijn resultaat terug.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` accepteert alleen de vorm van een functieaanroep. De argumenten worden geëvalueerd waar de
  `task` is geschreven; alleen de aanroep zelf draait in de nieuwe taak.
- Om meerdere expressies uit te voeren, maak je een `lambda` en roep je die ter plekke aan:
  `(task ((lambda () () (println "start") (work))))`
- Je mag `wait` zo vaak aanroepen als je wilt. Het resultaat wordt onthouden.
- Een taak draait ook als je er nooit op `wait`.
- **Wanneer het hoofdwerk klaar is, eindigt het programma.** Taken die nog draaien worden afgekapt.

## 2. Waarden doorgeven via kanalen

Een kanaal `Chan<T>` is een pad waarlangs taken waarden van het type `T` doorgeven. Het argument van
`Chan::new` is de capaciteit (hoeveel waarden het kan bevatten). Bij een kanaal met capaciteit 0
wachten de zender en de ontvanger allebei totdat de andere kant er is.

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

- `(send ch v)` verzendt. Als het kanaal vol is, wacht het tot er ruimte is.
- `(recv ch)` ontvangt. Het wacht tot er een waarde binnenkomt. Het resultaat is een `Option<T>`;
  zodra het kanaal gesloten en leeg is, geeft het `none` terug.
- Een kanaal met `doiter` doorlopen blijft waarden ontvangen totdat het wordt gesloten. Het kan ook
  rechtstreeks aan `map` of `filter` worden doorgegeven.
- `send` op een gesloten kanaal geeft een panic.

### Werk verdelen over meerdere taken

Een veelvoorkomend patroon is om één kanaal op te zetten dat het werk draagt en meerdere workers
(taken die het werk verwerken) er opdrachten van te laten nemen. De worker die vrij is neemt de
volgende opdracht, dus zelfs wanneer trage en snelle opdrachten door elkaar lopen, verspreidt het
werk zich vanzelf.

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

- A, B en C nemen elk een van de eerste drie opdrachten.
- Na 0,1 seconde zijn B en C vrij en nemen ze de resterende opdrachten. Terwijl A bezig is met de
  trage opdracht 1, neemt hij geen nieuw werk.
- Uiteindelijk verwerkte A één opdracht, B twee en C drie. Niets in het programma bepaalt welke
  worker welke opdracht neemt.
- Het sluiten van `jobs` beëindigt de `doiter` van elke worker, de taken eindigen en elke `wait`
  geeft zijn aantal terug.

`jobs` is een kanaal met capaciteit 0, dus `send` wacht totdat een worker de opdracht neemt. Met een
grotere capaciteit zou de hoofdtaak werk in de wachtrij kunnen zetten zonder op de workers te wachten.

## 3. `select`: op meerdere kanalen tegelijk wachten

`select` voert die van meerdere kanaalbewerkingen uit die het eerst mogelijk wordt. `(after seconds)`
is een kanaal dat één waarde aflevert zodra de opgegeven tijd is verstreken. Gecombineerd met
`select` geeft het je een timeout.

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

- `((v (recv ch)) body...)` is een ontvangsttak. `v` krijgt een `Option<T>`.
- `((send ch x) body...)` is een verzendtak.
- Wanneer meerdere takken tegelijk kunnen doorgaan, wordt er willekeurig een gekozen.
- Met `(else body...)` aan het einde wordt `else` uitgevoerd wanneer geen tak direct kan doorgaan, en
  wacht `select` niet.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Gedeelde gegevens beschermen

Wanneer meerdere taken dezelfde waarde wijzigen, bescherm je die met `Mutex<T>`. `with-lock` neemt
het slot, bindt de inhoud aan een variabele, voert de body uit en geeft het slot altijd vrij hoe de
body ook wordt verlaten. Aan de variabele toewijzen met `setf` binnen de body wijzigt de inhoud van
de `Mutex`.

`WaitGroup` is een hulpmiddel om te wachten tot een bepaald aantal taken klaar is. Verhoog de teller
met `add`, laat elke taak `done` aanroepen wanneer ze klaar is, en `wait` tot de teller 0 bereikt.

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

Als meerdere taken dezelfde waarde tegelijk wijzigen zonder via een `Mutex` of een kanaal te gaan, is
het resultaat niet gegarandeerd. Geef gegevens tussen taken waar mogelijk door via kanalen, en deel
gegevens alleen wanneer het nodig is.

## 5. Waar taken wisselen

Taken wisselen coöperatief. Een taak laat andere taken alleen op deze punten voorgaan:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- Een kanaalbewerking die moet wachten (`send`, `recv`, `select`)
- Een socketbewerking die moet wachten (verbinden, lezen, schrijven enzovoort)

Het argument van `sleep` is een aantal seconden als `f64`. Schrijf `(sleep 1.0)`, niet `(sleep 1)`.

Taken draaien tegelijkertijd op meerdere OS-threads. Wanneer `typl` een programma echter direct
uitvoert, gaan alleen taken die [gecompileerde](../guide/compile.md) functies uitvoeren naar andere
threads. De overige taken draaien op één thread en wisselen op de bovenstaande punten.

## 6. `thread`: op een eigen OS-thread draaien

Werk dat andere taken niet mag ophouden, zoals het aanroepen van een trage C-functie
([C-FFI](../guide/ffi.md)), wordt met `thread` gestart. Het wordt op dezelfde manier geschreven als
`task` en krijgt een eigen OS-thread. Wacht tot het klaar is met `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- De handle van `thread` heeft het type `Thread<T>`. Net als `wait` kan `join` willekeurig vaak
  worden aangeroepen.
- `thread` kan alleen functies uitvoeren die kunnen worden gecompileerd. Bij uitvoering in `typl`
  wordt de functie die hij aanroept ter plekke gecompileerd voordat hij draait.

## 7. Taken en andere functies

- Een `panic` binnen een taak stopt het hele programma.
- `throw` reikt niet buiten een taak. Een `throw` die de body van de taak zou verlaten wordt een
  `panic`.
- De uitvoer van één `println` wordt nooit midden in een regel gemengd met de uitvoer van andere
  taken.

## 8. Wat je hierna kunt lezen

- [Taken en kanalen](../reference/functions/concurrency.md): de lijst met functies
- [Syntaxreferentie hoofdstuk 12](../reference/syntax.md#12-gelijktijdigheid-taken): waar taken
  wisselen in detail, en verschillen met Go
- [Bestands-I/O, streams en netwerk](../guide/io.md): een server met taken schrijven
