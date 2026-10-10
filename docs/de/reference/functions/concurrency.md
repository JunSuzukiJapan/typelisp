<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Tasks und Kanäle

Das Vokabular der Tasks (leichtgewichtigen Threads). `task` und `thread`, die sie starten, und `select`, das
auf mehreres wartet, sind Spezialformen und stehen in der
[Syntaxreferenz](../syntax.md#12-nebenläufigkeit-tasks). Dieses Kapitel behandelt den Rest: Typen, Methoden
und Funktionen.

Tasks sind **kooperativ**: Ein Task wechselt nur an den Stellen, die man schreibt. Tasks laufen gleichzeitig
auf `TYPELISP_THREADS` OS-Threads (in `typl` gehen nur kompilierte Tasks auf andere Threads). Wo sie wechseln
und wie sich das von Go unterscheidet, steht in
[Syntaxreferenz 12.5](../syntax.md#125-wo-tasks-wechseln) und
[12.7](../syntax.md#127-unterschiede-zu-go).

## 1. `Task<T>` — Handles auf Tasks

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Wartet auf den Abschluss und gibt seinen Wert zurück |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Man darf beliebig oft `wait` aufrufen** (der Wert wird zwischengespeichert). Anders als Rusts
  `JoinHandle::join` verbraucht es das Handle nicht, daher kann man von mehreren Stellen aus warten.
- **Ein Task läuft, auch wenn man nie `wait` aufruft.** Das Handle fallen zu lassen hält ihn nicht an.
- Es ist ein gewöhnlicher Wert, daher kann es in einen `Vector<Task<()>>`.
- **Wenn der Haupttask endet, endet der Prozess** (wie in Go). Andere laufende Tasks werden abgeschnitten, und
  die Aufräumarbeit von `unwind-protect` läuft nicht, denn das ist ein Prozessende, kein Abwickeln des Stacks.

## 2. `Chan<T>` — Kanäle

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Ein Kanal mit Kapazität `n`. `0` ist ein Rendezvous (ohne Puffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Wartet, bis Platz ist, und übergibt dann den Wert |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Wartet, bis ein Wert ankommt. `none`, sobald er geschlossen und leer ist |
| `close` | `(close ch)` | `(Chan<T>)→()` | Schließt ihn |
| `len` | `(len ch)` | `(Chan<T>)→int` | Wie viele Werte gerade im Puffer sind |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Die Kapazität |

**Das Typargument wird mit `the` angegeben** (genauso geschrieben wie `(the Vector<i32> (Vector::new))`).
**Die Kapazität muss immer geschrieben werden**: Die beiden Fälle, die Go als `make(chan int)` und
`make(chan int, 16)` schreibt, schreibt man `(Chan::new 0)` und `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Gos for v := range ch
```

- **Ein `Chan<T>` ist sein eigener Iterator** (er implementiert `Iter`). `recv` gibt ein `Option<T>` zurück,
  denselben Typ wie `Iter::next`, daher funktionieren `doiter` und `map`/`filter`/`foldl` unverändert darauf.
- **`send` auf einem geschlossenen Kanal löst einen Panic aus**, und **ein zweites `close` ebenfalls** (beides
  wie in Go). Das sind Fehler im Programm, keine behebbaren Fehlschläge, daher sind es keine `Result`s.
- **Einen Kanal zu schließen, auf dem ein Task mit `send` wartet, lässt diesen Task einen Panic auslösen**
  (Gos Regel).
- `recv` auf einem geschlossenen Kanal gibt zurück, was noch im Puffer ist, und gibt danach, sobald er leer
  ist, fortwährend `none` zurück.
- `close` wird über den Typ des Empfängers aufgelöst und ist daher etwas anderes als das `close` des Traits
  `Stream`. `Chan<T>` implementiert `Stream` nicht.
- **Eine negative Kapazität löst einen Panic aus** (sie wird nicht stillschweigend auf 0 gerundet).

## 3. `yield` / `sleep` — den Vortritt lassen

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Gibt den Rest seines Zuges ab (Gos `runtime.Gosched`) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Hält **nur diesen Task** an. Die anderen laufen weiter |

`sleep` hält einen Task an, keinen Thread. Nur wenn überhaupt kein Task laufen kann, geht es bis zur nächsten
Frist in ein `sleep` des BS. `(sleep 0.0)` ist CLs „für 0 Sekunden den Vortritt lassen“.

Wie in CL nimmt `sleep` **Sekunden**. Ganzzahlen werden nicht automatisch in Gleitkommazahlen umgewandelt,
daher schreibt man CLs `(sleep 1)` hier `(sleep 1.0)`. Ein negativer Wert oder NaN löst einen Panic aus.

## 4. `WaitGroup` — auf N Abschlüsse warten

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Eine Gruppe ohne Ausstehendes |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Erhöht den Zähler. Vor Beginn der Arbeit aufrufen |
| `done` | `(done wg)` | `(WaitGroup)→()` | Eines ist fertig. Bei 0 werden alle Wartenden freigegeben |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Wartet, bis er 0 erreicht. Von beliebig vielen Tasks aus |

`(wait wg)` und `(wait task)` werden über den Typ des Empfängers aufgelöst und bestehen daher unter einem Namen
nebeneinander. Fällt der Zähler unter 0, gibt es einen Panic (`done` zu oft aufgerufen oder ein negatives
`add`). Wie in Go lässt sich eine Gruppe, die wieder bei 0 ist, mit `add` erneut verwenden. Es geht keine
Aktualisierung verloren, auch wenn die Tasks auf verschiedenen OS-Threads laufen.

**Da es `Task<T>` gibt, braucht man es seltener als in Go**: `(doiter (t tasks) (wait t))` genügt oft. Es ist
ein Werkzeug für dynamisch wachsende Arbeit oder für den Fall, dass man die Handles nicht aufbewahren will.

```lisp
;; Fan-in: pro Eingabe einen Task starten und sie zusammenführen (diese Sprache hat keine nil-Kanäle)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — ein Kanal, der nach einer Zeit liefert

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Ein Kanal, der nach `sec` Sekunden einen Wert liefert |

Gos `time.After`. Lässt sich unverändert im Timeout-Zweig von `select` schreiben
([Syntaxreferenz 12.3](../syntax.md#123-select--auf-mehrere-kanaloperationen-gleichzeitig-warten)). Seine
Kapazität ist 1, daher kann der sendende Task enden, auch wenn niemand empfängt.

## 6. `Mutex<T>` — gegenseitiger Ausschluss für geteilte Daten

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Ein nicht gesperrter Mutex, der `v` enthält |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Nimmt die Sperre (wartet) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Gibt sie frei. Panic, wenn er nicht gesperrt ist |
| `with-lock` | `(with-lock (x m) body...)` | Makro | Sperrt, bindet den Inhalt an `x`, führt `body` aus und gibt **immer** frei |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` ist keine Kopie des Wertes, sondern ein „Ort“** (`symbol-macrolet`). `(setf x 42)` ändert den Inhalt
  des Mutex.
- `with-lock` gibt mit `unwind-protect` frei, daher wird die Sperre freigegeben, wie auch immer der Rumpf
  verlassen wird: normaler Abschluss, `throw`, `panic` oder `break`/`return`/`return-from`.
- **Erneutes Betreten führt zu einem Deadlock** (kein Panic). Der Scheduler meldet für einen Task, der an seiner
  eigenen Sperre hängt, dass „nichts vorankommen kann“.
- **`m::v` greift von außerhalb der Sperre auf den Inhalt zu**, was in dem Sinne undefiniert ist, dass ein
  anderer Task ihn gerade ändern könnte. Das ist dieselbe Position wie bei Gos `sync.Mutex`: In einer Sprache
  ohne Besitz- und Ausleihprüfung lässt sich eine statische Garantie wie `MutexGuard` nicht bauen.

## 7. `Thread<T>` — eigene OS-Threads

Das von `(thread (f args...))` zurückgegebene Handle
([Syntaxreferenz 12.2](../syntax.md#122-thread--einen-task-auf-einem-eigenen-os-thread-starten)). Das
Gegenstück zu `Task<T>`.

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Wartet auf den Abschluss und gibt den Wert zurück (der aufrufende **Task** hält an. Kann beliebig oft aufgerufen werden; der Wert wird zwischengespeichert) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | Die Funktionsfassung von `(thread (f))` (Rusts `std::thread::spawn`) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Die Nummer des laufenden OS-Threads. Innerhalb des Prozesses eindeutig, ohne Bedeutung über „ist das derselbe Thread“ hinaus |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Die Anzahl der Threads, die die Maschine gleichzeitig ausführen kann (der Standard von `TYPELISP_THREADS`). Panic, wenn das BS nicht antwortet |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; eine blockierende C-Funktion
(let ((th (thread (sleepy 500000))))
  ...                                            ; andere Tasks laufen derweil weiter
  (join th))                                     ; => 500000
```

- Der Aufruf einer blockierenden C-Funktion (`defffi`) hält nur diesen Thread an.
- Ein `task` innerhalb eines `thread` läuft als gewöhnlicher Task auf anderen Threads.
- Auch in `typl` verwendbar. Beim Interpretieren kompilieren `(thread (f ...))` und `Thread::spawn` die
  auszuführende Funktion an Ort und Stelle und führen sie dann auf dem eigenen Thread aus. Ein `lambda`, das auf
  lokale Variablen außerhalb verweist, lässt sich nicht für sich allein kompilieren und löst einen Panic aus
  ([Syntaxreferenz 12.2](../syntax.md#122-thread--einen-task-auf-einem-eigenen-os-thread-starten)). Ein
  innerhalb einer kompilierten Funktion erzeugtes `lambda` kann übergeben werden.

## 8. `Context` — kooperativer Abbruch

Gos `context.Context`. Man gibt ihn Arbeit mit, die man von außen anhalten können will. Das Anhalten
ist **kooperativ**: `cancel` unterbricht nichts; eine Task oder ein Thread merkt es, indem sie
selbst `is-cancelled` prüft oder auf `done` empfängt.

| Name | Verwendung | Typ | Bedeutung |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | Ein neuer Kontext als Wurzel |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | Erzeugt ein Kind von `parent` |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | Erzeugt ein Kind von `parent`, das sich nach `sec` Sekunden selbst abbricht |
| `cancel` | `(cancel ctx)` | `(Context)→()` | Bricht ab. Darf beliebig oft aufgerufen werden |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | Ein Kanal, der beim Abbruch geschlossen wird |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | Ob abgebrochen wurde |

```lisp
(defun worker ((ctx Context) (jobs Chan<int>)) ()
  (loop
    (select
      ((v (recv (done ctx))) (println "stopped") (break))
      ((j (recv jobs)) (match j
                         ((some n) (println "job ~a" n))
                         ((none) (break)))))))

(let* ((ctx (Context::with-timeout (Context::background) 1.0))
       (jobs (the Chan<int> (Chan::new 0))))
  (task (worker ctx jobs))
  (send jobs 1)
  (send jobs 2)
  (cancel ctx)                          ; job 1, job 2, dann stopped
  (sleep 0.1))
```

- **Der Abbruch erreicht die Kinder.** Ein mit `with-cancel`/`with-timeout` erzeugter Kontext wird
  zusammen mit seinem Elternkontext abgebrochen. In die andere Richtung (vom Kind zum Elternteil)
  geht es nicht.
- Ein Kind eines bereits abgebrochenen Kontexts ist von Anfang an abgebrochen.
- `done` wird nur geschlossen; es wird kein Wert gesendet. Ein Empfang liefert `none`.
- Jeder Aufruf von `(Context::background)` erzeugt eine eigene Wurzel. Gos `Background()` gibt es
  nur einmal und es ist nicht abbrechbar; hier lässt sich auch eine Wurzel abbrechen, und das
  betrifft nur, was aus ihr erzeugt wurde.
- Ein Kontext kann zwischen Tasks und zwischen Threads weitergegeben werden.

## 9. Was es nicht gibt

- **`Atomic`**. `Mutex` genügt.
- **Task-lokale Variablen** (Go hat sie auch nicht).
- **nil-Kanäle**. Der Grund und die Alternative stehen in
  [Syntaxreferenz 12.7](../syntax.md#127-unterschiede-zu-go).
