<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Taken en kanalen

De woordenschat van taken (lichtgewicht threads). `task` en `thread`, die ze starten, en `select`, dat
op meerdere dingen wacht, zijn speciale vormen en staan in de
[Syntaxreferentie](../syntax.md#12-gelijktijdigheid-taken). Dit hoofdstuk behandelt de rest: types,
methoden en functies.

Taken zijn **coöperatief**: een taak wisselt alleen op de punten die je schrijft. Taken draaien
tegelijkertijd op `TYPELISP_THREADS` OS-threads (in `typl` gaan alleen gecompileerde taken naar andere
threads). Waar ze wisselen en hoe dit van Go verschilt staat in
[Syntaxreferentie 12.5](../syntax.md#125-waar-taken-wisselen) en
[12.7](../syntax.md#127-verschillen-met-go).

## 1. `Task<T>` — handles voor taken

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Wacht op voltooiing en geeft de waarde terug |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Je mag willekeurig vaak `wait` aanroepen** (de waarde wordt in de cache bewaard). Anders dan
  `JoinHandle::join` van Rust verbruikt het de handle niet, dus er kan vanaf meerdere plekken op
  worden gewacht.
- **Een taak draait ook als je er nooit op `wait`.** De handle laten vallen stopt haar niet.
- Het is een gewone waarde, dus hij kan in een `Vector<Task<()>>`.
- **Wanneer de hoofdtaak eindigt, eindigt het proces** (zoals in Go). Andere draaiende taken worden
  afgekapt, en de opruiming van `unwind-protect` wordt niet uitgevoerd, omdat dit het beëindigen van
  het proces is en geen het afwikkelen van de stack.

## 2. `Chan<T>` — kanalen

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Een kanaal met capaciteit `n`. `0` is een rendez-vous (geen buffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Wacht tot er ruimte is en draagt dan de waarde over |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Wacht tot er een waarde binnenkomt. `none` zodra het gesloten en leeg is |
| `close` | `(close ch)` | `(Chan<T>)→()` | Sluit het |
| `len` | `(len ch)` | `(Chan<T>)→int` | Hoeveel waarden er nu in de buffer zitten |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | De capaciteit |

**Het typeargument wordt met `the` gegeven** (op dezelfde manier geschreven als
`(the Vector<i32> (Vector::new))`). **De capaciteit moet altijd worden geschreven**: de twee gevallen
die Go als `make(chan int)` en `make(chan int, 16)` schrijft, worden hier `(Chan::new 0)` en
`(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **Een `Chan<T>` is zijn eigen iterator** (hij implementeert `Iter`). `recv` geeft een `Option<T>`
  terug, hetzelfde type als `Iter::next`, dus `doiter` en `map`/`filter`/`foldl` werken er allemaal
  direct op.
- **`send` op een gesloten kanaal geeft een panic**, en **een tweede `close` geeft ook een panic**
  (beide zoals in Go). Dit zijn bugs in het programma, geen herstelbare fouten, dus het zijn geen
  `Result`s.
- **Een kanaal sluiten waarop een taak wacht om te `send`en laat die taak een panic geven** (de regel
  van Go).
- `recv` op een gesloten kanaal geeft terug wat er in de buffer over is, en zodra die leeg is blijft
  het `none` teruggeven.
- `close` wordt opgelost via het type van de ontvanger, dus het is iets anders dan de `close` van de
  trait `Stream`. `Chan<T>` implementeert `Stream` niet.
- **Een negatieve capaciteit geeft een panic** (ze wordt niet stilzwijgend op 0 afgerond).

## 3. `yield` / `sleep` — voorrang geven

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Geeft de rest van zijn beurt op (`runtime.Gosched` van Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Stopt **alleen die taak**. De andere blijven draaien |

`sleep` stopt een taak, geen thread. Alleen wanneer helemaal geen taak kan draaien gaat het tot de
dichtstbijzijnde deadline in een OS-`sleep`. `(sleep 0.0)` is het "yield voor 0 seconden" van CL.

Net als in CL neemt `sleep` **seconden**. Gehele getallen worden niet automatisch naar
drijvendekommagetallen geconverteerd, dus het `(sleep 1)` van CL wordt hier `(sleep 1.0)` geschreven.
Een negatieve waarde of NaN geeft een panic.

## 4. `WaitGroup` — wachten op N voltooiingen

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Een groep met niets openstaands |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Telt bij de teller op. Doe het voordat het werk begint |
| `done` | `(done wg)` | `(WaitGroup)→()` | Eén is klaar. Bij 0 worden alle wachtenden vrijgegeven |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Wacht tot de teller 0 bereikt. Vanuit een willekeurig aantal taken |

`(wait wg)` en `(wait task)` worden opgelost via het type van de ontvanger, dus ze bestaan naast
elkaar onder één naam. Als de teller onder 0 komt, geeft het een panic (`done` te vaak aangeroepen,
of een negatieve `add`). Net als in Go kan een groep die weer op 0 staat opnieuw worden gebruikt,
beginnend met `add`. Er gaat geen update verloren, ook niet als de taken op afzonderlijke OS-threads
draaien.

**Doordat `Task<T>` beschikbaar is, is het minder nodig dan in Go**: `(doiter (t tasks) (wait t))`
volstaat vaak. Het is een hulpmiddel voor werk dat dynamisch groeit, of voor wanneer je de handles
niet wilt bewaren.

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — een kanaal dat na een tijd aflevert

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Een kanaal dat na `sec` seconden één waarde aflevert |

`time.After` van Go. Het kan zoals het is worden geschreven in de timeout-tak van `select`
([Syntaxreferentie 12.3](../syntax.md#123-select--wachten-op-meerdere-kanaalbewerkingen-tegelijk)).
De capaciteit is 1, dus de verzendende taak kan eindigen ook als niemand ontvangt.

## 6. `Mutex<T>` — wederzijdse uitsluiting voor gedeelde gegevens

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Een niet-vergrendelde mutex die `v` bevat |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Neemt het slot (wacht) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Geeft het vrij. Geeft een panic als het niet is vergrendeld |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Vergrendelt, bindt de inhoud aan `x`, voert `body` uit en geeft **altijd** vrij |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` is geen kopie van de waarde maar een "plaats"** (`symbol-macrolet`). `(setf x 42)` wijzigt de
  inhoud van de mutex.
- `with-lock` geeft vrij met `unwind-protect`, dus het slot wordt vrijgegeven hoe de body ook wordt
  verlaten: normale voltooiing, `throw`, `panic`, of `break`/`return`/`return-from`.
- **Opnieuw binnengaan geeft een deadlock** (geen panic). De scheduler meldt dat "niets vooruitgang
  kan boeken" voor een taak die op zijn eigen slot vastzit.
- **`m::v` raakt de inhoud buiten het slot aan**, wat ongedefinieerd is in de zin dat een andere taak
  er midden in een wijziging kan zitten. Dit is dezelfde positie als `sync.Mutex` van Go: in een
  taal zonder eigendoms- of leencontrole kan geen statische garantie als `MutexGuard` worden gebouwd.

## 7. `Thread<T>` — eigen OS-threads

De handle die `(thread (f args...))` teruggeeft
([Syntaxreferentie 12.2](../syntax.md#122-thread--een-taak-starten-op-een-eigen-os-thread)). De
tegenhanger van `Task<T>`.

| Naam | Gebruik | Type | Betekenis |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Wacht op voltooiing en geeft de waarde terug (de aanroepende **taak** stopt. Kan willekeurig vaak worden aangeroepen; de waarde wordt in de cache bewaard) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | De functieversie van `(thread (f))` (`std::thread::spawn` van Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Het nummer van de OS-thread die draait. Uniek binnen het proces, zonder betekenis buiten "is dit dezelfde thread" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Het aantal threads dat de machine tegelijk kan uitvoeren (de standaardwaarde van `TYPELISP_THREADS`). Geeft een panic als het OS niet antwoordt |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- Een blokkerende C-functie (`defffi`) aanroepen stopt alleen die thread.
- Een `task` binnen een `thread` draait als gewone taak op andere threads.
- Het kan ook in `typl` worden gebruikt. Bij interpreteren compileren `(thread (f ...))` en
  `Thread::spawn` de functie ter plekke om uit te voeren en voeren haar dan op de eigen thread uit.
  Een `lambda` die naar lokale variabelen daarbuiten verwijst kan niet op zichzelf worden
  gecompileerd en geeft een panic
  ([Syntaxreferentie 12.2](../syntax.md#122-thread--een-taak-starten-op-een-eigen-os-thread)). Een
  `lambda` die binnen een gecompileerde functie is gemaakt kan worden doorgegeven.

## 8. Wat er niet is

- **`Atomic`**. `Mutex` volstaat.
- **Taaklokale variabelen** (Go heeft ze ook niet).
- **nil-kanalen**. De reden en het alternatief staan in
  [Syntaxreferentie 12.7](../syntax.md#127-verschillen-met-go).
