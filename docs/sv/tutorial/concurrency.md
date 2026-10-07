<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Samtidighet

I typelisp kör man arbete samtidigt genom att starta **tasks** (lättviktstrådar), och tasks skickar
värden till varandra genom **kanaler**. Modellen ligger nära Gos goroutines och kanaler. Det här
kapitlet går i tur och ordning igenom att starta en task och få dess resultat, kanaler, `select`, att
skydda delad data och dedikerade OS-trådar. Det förutsätter att du har läst
[Grunderna i typer](types.md).

## 1. Starta en task och vänta på dess resultat

`(task (funktion argument...))` startar ett funktionsanrop som en ny task. Den startande sidan väntar
inte utan går vidare. Värdet är ett handtag av typen `Task<T>`; `(wait handle)` väntar tills tasken är
klar och returnerar dess resultat.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; vänta 0,1 sekunder (bara den här tasken stannar)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` tar bara formen av ett funktionsanrop. Argumenten utvärderas där `task` är skriven; bara
  själva anropet körs i den nya tasken.
- För att köra flera uttryck gör man en `lambda` och anropar den direkt:
  `(task ((lambda () () (println "start") (work))))`
- Du får anropa `wait` hur många gånger du vill. Resultatet kommer ihåg.
- En task körs även om du aldrig gör `wait` på den.
- **När huvudarbetet är klart tar programmet slut.** Tasks som fortfarande körs avbryts.

## 2. Skicka värden genom kanaler

En kanal `Chan<T>` är en väg som tasks skickar värden av typen `T` genom. Argumentet till `Chan::new` är
kapaciteten (hur många värden den kan hålla). På en kanal med kapacitet 0 väntar både sändaren och
mottagaren tills den andra sidan finns på plats.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; skicka
  (close ch))                  ; inga fler sändningar

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; ta emot tills den stängs
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` skickar. Om kanalen är full väntar den tills det finns plats.
- `(recv ch)` tar emot. Den väntar tills ett värde kommer. Resultatet är ett `Option<T>`; när kanalen är
  stängd och tom returnerar den `none`.
- Att iterera över en kanal med `doiter` fortsätter ta emot värden tills den stängs. Den kan också
  skickas direkt till `map` eller `filter`.
- `send` på en stängd kanal ger panic.

### Fördela arbete mellan flera tasks

Ett vanligt mönster är att sätta upp en kanal som bär arbetet och låta flera arbetare (tasks som
behandlar arbetet) ta jobb från den. Den arbetare som är ledig tar nästa jobb, så även när långsamma
och snabba jobb blandas fördelas arbetet naturligt.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; hur lång tid det här jobbet tar

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; ta ett jobb i taget tills jobs stängs
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; arbetar; under tiden tar andra arbetare nästa jobb
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; hur många jobb den här arbetaren gjorde

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
  (close jobs)                    ; det var allt arbete
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

- A, B och C tar var sitt av de tre första jobben.
- Efter 0,1 sekunder är B och C lediga och tar de återstående jobben. Medan A är upptagen med det
  långsamma jobb 1 tar den inget nytt arbete.
- Till slut hanterade A ett jobb, B två och C tre. Ingenting i programmet säger vilken arbetare som
  tar vilket jobb.
- Att stänga `jobs` avslutar varje arbetares `doiter`, tasks blir klara och varje `wait` returnerar sitt
  antal.

`jobs` är en kanal med kapacitet 0, så `send` väntar tills någon arbetare tar jobbet. Med större
kapacitet kunde huvudtasken köa arbete utan att vänta på arbetarna.

## 3. `select`: vänta på flera kanaler samtidigt

`select` utför den av flera kanaloperationer som blir möjlig först. `(after seconds)` är en kanal som
levererar ett värde när den angivna tiden har gått. I kombination med `select` ger den en timeout.

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

- `((v (recv ch)) body...)` är en mottagningsgren. `v` får ett `Option<T>`.
- `((send ch x) body...)` är en sändningsgren.
- När flera grenar kan gå vidare samtidigt väljs en av dem slumpmässigt.
- Med `(else body...)` sist körs `else` när ingen gren kan gå vidare direkt, och `select` väntar inte.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Skydda delad data

När flera tasks ändrar samma värde skyddar man det med `Mutex<T>`. `with-lock` tar låset, binder
innehållet till en variabel, kör kroppen och släpper alltid låset hur kroppen än lämnas. Att tilldela
variabeln med `setf` inuti kroppen ändrar innehållet i `Mutex`.

`WaitGroup` är ett verktyg för att vänta tills ett givet antal tasks har blivit klara. Höj räknaren
med `add`, låt varje task anropa `done` när den är klar och `wait` tills räknaren når 0.

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

Om flera tasks ändrar samma värde samtidigt utan att gå via en `Mutex` eller en kanal är resultatet inte
garanterat. Skicka data mellan tasks genom kanaler där du kan, och dela data bara när du måste.

## 5. Var tasks byter

Tasks byter kooperativt. En task viker sig för andra tasks bara på dessa ställen:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- En kanaloperation som måste vänta (`send`, `recv`, `select`)
- En socketoperation som måste vänta (anslutning, läsning, skrivning och så vidare)

Argumentet till `sleep` är ett antal sekunder som `f64`. Skriv `(sleep 1.0)`, inte `(sleep 1)`.

Tasks körs samtidigt på flera OS-trådar. När `typl` kör ett program direkt är det dock bara tasks som
kör [kompilerade](../guide/compile.md) funktioner som går ut på andra trådar. De övriga tasks körs på
en enda tråd och byter på ställena ovan.

## 6. `thread`: köra på en dedikerad OS-tråd

Arbete som inte ska hålla upp andra tasks, som att anropa en långsam C-funktion ([C FFI](../guide/ffi.md)),
startas med `thread`. Det skrivs på samma sätt som `task` och får en egen OS-tråd. Vänta på att den blir
klar med `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- Handtaget från `thread` har typen `Thread<T>`. Liksom `wait` kan `join` anropas hur många gånger som
  helst.
- `thread` kan bara köra funktioner som kan kompileras. När det körs i `typl` kompileras funktionen den
  anropar på stället innan den körs.

## 7. Tasks och andra funktioner

- En `panic` inuti en task stoppar hela programmet.
- `throw` når inte utanför en task. Ett `throw` som skulle lämna taskens kropp blir en `panic`.
- Utdata från en `println` blandas aldrig in mitt i en rad med andra tasks utdata.

## 8. Vad man läser härnäst

- [Tasks och kanaler](../reference/functions/concurrency.md): listan över funktioner
- [Syntaxreferens kapitel 12](../reference/syntax.md#12-samtidighet-tasks): var tasks byter i detalj,
  och skillnader mot Go
- [Fil-I/O, strömmar och nätverk](../guide/io.md): att skriva en server med tasks
