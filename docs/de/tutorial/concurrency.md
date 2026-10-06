<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Nebenläufigkeit

In typelisp lässt man Arbeit nebenläufig ablaufen, indem man **Tasks** (leichtgewichtige Threads) startet,
und Tasks reichen sich Werte über **Kanäle** weiter. Das Modell ähnelt den Goroutinen und Kanälen von Go.
Dieses Kapitel behandelt der Reihe nach, wie man einen Task startet und sein Ergebnis bekommt, Kanäle,
`select`, den Schutz geteilter Daten und eigene OS-Threads. Es setzt voraus, dass
[Grundlagen der Typen](types.md) gelesen wurde.

## 1. Einen Task starten und auf sein Ergebnis warten

`(task (Funktion Argumente...))` startet einen Funktionsaufruf als neuen Task. Die startende Seite wartet
nicht und macht weiter. Der Wert ist ein Handle vom Typ `Task<T>`; `(wait Handle)` wartet, bis der Task
fertig ist, und gibt sein Ergebnis zurück.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; 0,1 Sekunden warten (nur dieser Task hält an)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` nimmt nur die Form eines Funktionsaufrufs. Die Argumente werden dort ausgewertet, wo das `task`
  steht; im neuen Task läuft nur der Aufruf selbst.
- Um mehrere Ausdrücke auszuführen, erzeugt man ein `lambda` und ruft es sofort auf:
  `(task ((lambda () () (println "start") (work))))`
- `wait` darf beliebig oft aufgerufen werden. Das Ergebnis wird gemerkt.
- Ein Task läuft, auch wenn nie auf ihn mit `wait` gewartet wird.
- **Wenn die Hauptarbeit fertig ist, endet das Programm.** Noch laufende Tasks werden abgebrochen.

## 2. Werte über Kanäle weiterreichen

Ein Kanal `Chan<T>` ist ein Weg, über den Tasks Werte vom Typ `T` weiterreichen. Das Argument von `Chan::new`
ist die Kapazität (wie viele Werte er aufnehmen kann). Bei einem Kanal mit Kapazität 0 warten Sender und
Empfänger jeweils, bis die andere Seite da ist.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; senden
  (close ch))                  ; keine weiteren Sendungen

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; empfangen, bis er geschlossen wird
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` sendet. Ist der Kanal voll, wartet es, bis Platz frei ist.
- `(recv ch)` empfängt. Es wartet, bis ein Wert ankommt. Das Ergebnis ist ein `Option<T>`; sobald der Kanal
  geschlossen und leer ist, gibt es `none` zurück.
- Ein Kanal, der mit `doiter` durchlaufen wird, empfängt so lange Werte, bis er geschlossen wird. Er lässt sich
  auch direkt an `map` oder `filter` übergeben.
- `send` auf einem geschlossenen Kanal löst einen Panic aus.

### Arbeit auf mehrere Tasks verteilen

Ein gängiges Muster ist, einen Kanal einzurichten, der die Arbeit transportiert, und mehrere Worker (Tasks,
die die Arbeit erledigen) daraus Aufträge holen zu lassen. Der Worker, der gerade frei ist, holt den nächsten
Auftrag, sodass sich die Arbeit von selbst verteilt, auch wenn langsame und schnelle Aufträge gemischt sind.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; wie lange dieser Auftrag dauert

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; einen Auftrag nach dem anderen holen, bis jobs geschlossen ist
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; bei der Arbeit; währenddessen holen andere Worker die nächsten Aufträge
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; wie viele Aufträge dieser Worker erledigt hat

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
  (close jobs)                    ; das ist die ganze Arbeit
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

- A, B und C holen sich jeweils einen der ersten drei Aufträge.
- Nach 0,1 Sekunden sind B und C frei und holen sich die übrigen Aufträge. Solange A mit dem langsamen Auftrag 1
  beschäftigt ist, nimmt er keine neue Arbeit an.
- Am Ende hat A einen Auftrag erledigt, B zwei und C drei. Nirgends im Programm steht, welcher Worker welchen
  Auftrag bekommt.
- Das Schließen von `jobs` beendet das `doiter` jedes Workers, die Tasks enden, und jedes `wait` gibt seine
  Anzahl zurück.

`jobs` ist ein Kanal mit Kapazität 0, daher wartet `send`, bis irgendein Worker den Auftrag übernimmt. Mit
größerer Kapazität könnte der Haupt-Task Arbeit in die Warteschlange stellen, ohne auf die Worker zu warten.

## 3. `select`: auf mehrere Kanäle gleichzeitig warten

`select` führt diejenige von mehreren Kanaloperationen aus, die zuerst möglich wird. `(after Sekunden)` ist ein
Kanal, der einen Wert liefert, sobald die angegebene Zeit vergangen ist. Zusammen mit `select` ergibt das ein
Zeitlimit.

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

- `((v (recv ch)) Rumpf...)` ist ein Empfangszweig. `v` bekommt ein `Option<T>`.
- `((send ch x) Rumpf...)` ist ein Sendezweig.
- Wenn mehrere Zweige gleichzeitig fortfahren können, wird einer davon zufällig gewählt.
- Steht `(else Rumpf...)` am Ende, läuft `else`, wenn kein Zweig sofort fortfahren kann, und `select` wartet
  nicht.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Geteilte Daten schützen

Wenn mehrere Tasks denselben Wert ändern, schützt man ihn mit `Mutex<T>`. `with-lock` nimmt die Sperre,
bindet den Inhalt an eine Variable, führt den Rumpf aus und gibt die Sperre immer frei, wie auch immer der
Rumpf verlassen wird. Eine Zuweisung an die Variable mit `setf` im Rumpf ändert den Inhalt des `Mutex`.

`WaitGroup` ist ein Werkzeug, um zu warten, bis eine gegebene Anzahl von Tasks fertig ist. Man erhöht den
Zähler mit `add`, lässt jeden Task beim Ende `done` aufrufen und wartet mit `wait`, bis der Zähler 0 erreicht.

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

Ändern mehrere Tasks denselben Wert gleichzeitig, ohne über einen `Mutex` oder einen Kanal zu gehen, ist das
Ergebnis nicht garantiert. Daten zwischen Tasks reicht man nach Möglichkeit über Kanäle weiter und teilt sie
nur, wenn es nötig ist.

## 5. Wo Tasks wechseln

Tasks wechseln kooperativ. Ein Task gibt anderen Tasks nur an diesen Stellen den Vortritt:

- `(yield)`, `(sleep Sekunden)`, `(wait Handle)`
- Eine Kanaloperation, die warten muss (`send`, `recv`, `select`)
- Eine Socket-Operation, die warten muss (Verbinden, Lesen, Schreiben usw.)

Das Argument von `sleep` ist eine Sekundenzahl vom Typ `f64`. Man schreibt `(sleep 1.0)`, nicht `(sleep 1)`.

Tasks laufen gleichzeitig auf mehreren OS-Threads. Wenn `typl` ein Programm direkt ausführt, gehen jedoch nur
Tasks, die [kompilierte](../guide/compile.md) Funktionen ausführen, auf andere Threads. Die übrigen Tasks
laufen auf einem einzigen Thread und wechseln an den oben genannten Stellen.

## 6. `thread`: auf einem eigenen OS-Thread laufen

Arbeit, die andere Tasks nicht aufhalten darf, wie der Aufruf einer langsamen C-Funktion
([C-FFI](../guide/ffi.md)), startet man mit `thread`. Es wird genauso geschrieben wie `task` und bekommt einen
eigenen OS-Thread. Auf das Ende wartet man mit `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- Das Handle von `thread` hat den Typ `Thread<T>`. Wie `wait` kann `join` beliebig oft aufgerufen werden.
- `thread` kann nur Funktionen ausführen, die sich kompilieren lassen. Beim Ausführen in `typl` wird die
  aufgerufene Funktion vor dem Start an Ort und Stelle kompiliert.

## 7. Tasks und andere Funktionen der Sprache

- Ein `panic` innerhalb eines Tasks hält das ganze Programm an.
- `throw` reicht nicht über einen Task hinaus. Ein `throw`, das den Rumpf des Tasks verlassen würde, wird zu
  einem `panic`.
- Die Ausgabe eines `println` wird nie mitten in einer Zeile mit der Ausgabe anderer Tasks vermischt.

## 8. Wie es weitergeht

- [Tasks und Kanäle](../reference/functions/concurrency.md): die Liste der Funktionen
- [Syntaxreferenz Kapitel 12](../reference/syntax.md#12-nebenläufigkeit-tasks): wo Tasks im Detail wechseln,
  und Unterschiede zu Go
- [Datei-E/A, Streams und Netzwerk](../guide/io.md): einen Server mit Tasks schreiben
