<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tasks och kanaler

Vokabulären för tasks (lättviktstrådar). `task` och `thread`, som startar dem, och `select`, som väntar
på flera saker, är specialformer och finns i [Syntaxreferensen](../syntax.md#12-samtidighet-tasks). Det
här kapitlet täcker resten: typer, metoder och funktioner.

Tasks är **kooperativa**: en task byter bara på de ställen du skriver. Tasks körs samtidigt på
`TYPELISP_THREADS` OS-trådar (i `typl` går bara kompilerade tasks ut på andra trådar). Var de byter och
hur detta skiljer sig från Go finns i [Syntaxreferens 12.5](../syntax.md#125-var-tasks-byter) och
[12.7](../syntax.md#127-skillnader-mot-go).

## 1. `Task<T>` — handtag till tasks

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Väntar på färdigställande och returnerar dess värde |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Du får göra `wait` hur många gånger som helst** (värdet cachas). Till skillnad från Rusts
  `JoinHandle::join` förbrukar det inte handtaget, så det kan väntas på från flera ställen.
- **En task körs även om du aldrig gör `wait`.** Att släppa handtaget stoppar den inte.
- Det är ett vanligt värde, så det kan läggas i en `Vector<Task<()>>`.
- **När huvudtasken tar slut tar processen slut** (som i Go). Andra tasks som körs avbryts, och
  uppstädningen i `unwind-protect` körs inte, eftersom detta är processavslut, inte avvecklande av
  stacken.

## 2. `Chan<T>` — kanaler

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | En kanal med kapacitet `n`. `0` är ett möte (rendezvous, ingen buffert) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Väntar tills det finns plats och lämnar sedan över värdet |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Väntar tills ett värde kommer. `none` när den är stängd och tom |
| `close` | `(close ch)` | `(Chan<T>)→()` | Stänger den |
| `len` | `(len ch)` | `(Chan<T>)→int` | Hur många värden som finns i bufferten nu |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Kapaciteten |

**Typargumentet anges med `the`** (skrivs på samma sätt som `(the Vector<i32> (Vector::new))`).
**Kapaciteten måste alltid skrivas**: de två fall som Go skriver `make(chan int)` och
`make(chan int, 16)` skrivs `(Chan::new 0)` och `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Gos for v := range ch
```

- **En `Chan<T>` är sin egen iterator** (den implementerar `Iter`). `recv` returnerar ett `Option<T>`,
  samma typ som `Iter::next`, så `doiter` och `map`/`filter`/`foldl` fungerar alla på den som den är.
- **`send` på en stängd kanal ger panic**, och **ett andra `close` ger också panic** (båda som i Go).
  Det här är buggar i programmet, inte återhämtningsbara misslyckanden, så de är inte `Result`.
- **Att stänga en kanal som en task väntar på att göra `send` på får den tasken att ge panic** (Gos
  regel).
- `recv` på en stängd kanal returnerar det som finns kvar i bufferten, och när den är tom fortsätter den
  returnera `none`.
- `close` löses upp efter mottagarens typ, så det är en annan sak än `close` i traitet `Stream`.
  `Chan<T>` implementerar inte `Stream`.
- **En negativ kapacitet ger panic** (den avrundas inte tyst till 0).

## 3. `yield` / `sleep` — vika sig

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Ger upp resten av sin tur (Gos `runtime.Gosched`) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Stoppar **bara den tasken**. De andra fortsätter köra |

`sleep` stoppar en task, inte en tråd. Först när ingen task alls kan köra går den in i ett OS-`sleep` till
närmaste tidsgräns. `(sleep 0.0)` är CL:s "yield i 0 sekunder".

Liksom i CL tar `sleep` **sekunder**. Heltal konverteras inte automatiskt till flyttal, så CL:s
`(sleep 1)` skrivs `(sleep 1.0)` här. Ett negativt värde eller NaN ger panic.

## 4. `WaitGroup` — vänta på N färdigställanden

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | En grupp utan något utestående |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Lägger till räknaren. Gör det innan arbetet startar |
| `done` | `(done wg)` | `(WaitGroup)→()` | En har blivit klar. Vid 0 släpps alla som väntar |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Väntar tills den når 0. Från hur många tasks som helst |

`(wait wg)` och `(wait task)` löses upp efter mottagarens typ, så de samexisterar under ett namn. Om
räknaren går under 0 ger det panic (`done` anropad för många gånger, eller ett negativt `add`). Som i Go
kan en grupp som är tillbaka på 0 användas igen med början i `add`. Ingen uppdatering går förlorad ens
när tasks körs på separata OS-trådar.

**Med `Task<T>` tillgänglig behövs den mindre än i Go**: `(doiter (t tasks) (wait t))` räcker ofta. Det
är ett verktyg för arbete som växer dynamiskt, eller för när du inte vill behålla handtagen.

```lisp
;; fan-in: starta en task per indata och slå ihop dem (det här språket har inga nil-kanaler)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — en kanal som levererar efter en tid

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | En kanal som levererar ett värde efter `sec` sekunder |

Gos `time.After`. Den kan skrivas som den är i timeout-grenen i `select`
([Syntaxreferens 12.3](../syntax.md#123-select--vänta-på-flera-kanaloperationer-samtidigt)). Dess
kapacitet är 1, så den sändande tasken kan bli klar även om ingen tar emot.

## 6. `Mutex<T>` — ömsesidig uteslutning för delad data

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | En olåst mutex som håller `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Tar låset (väntar) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Släpper det. Ger panic om det inte är låst |
| `with-lock` | `(with-lock (x m) body...)` | Makro | Låser, binder innehållet till `x`, kör `body` och släpper **alltid** |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` är inte en kopia av värdet utan en "plats"** (`symbol-macrolet`). `(setf x 42)` ändrar
  mutexens innehåll.
- `with-lock` släpper med `unwind-protect`, så låset släpps hur kroppen än lämnas: normalt
  färdigställande, `throw`, `panic` eller `break`/`return`/`return-from`.
- **Att gå in igen ger baklås (deadlock)** (det ger inte panic). Schemaläggaren rapporterar att "inget
  kan göra framsteg" för en task som sitter fast på sitt eget lås.
- **`m::v` rör innehållet utanför låset**, vilket är odefinierat i meningen att en annan task kan vara
  mitt i att ändra det. Det här är samma ståndpunkt som Gos `sync.Mutex`: i ett språk utan ägande eller
  lånekontroll kan en statisk garanti som `MutexGuard` inte byggas.

## 7. `Thread<T>` — dedikerade OS-trådar

Handtaget som `(thread (f args...))` returnerar
([Syntaxreferens 12.2](../syntax.md#122-thread--starta-en-task-på-en-dedikerad-os-tråd)).
Motsvarigheten till `Task<T>`.

| Namn | Användning | Typ | Betydelse |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Väntar på färdigställande och returnerar värdet (den anropande **tasken** stoppar. Kan anropas hur många gånger som helst; värdet cachas) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | Funktionsversionen av `(thread (f))` (Rusts `std::thread::spawn`) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Numret på den OS-tråd som körs. Unikt inom processen, utan betydelse utöver "är det samma tråd" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Antalet trådar maskinen kan köra samtidigt (standardvärdet för `TYPELISP_THREADS`). Ger panic om OS inte svarar |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; en blockerande C-funktion
(let ((th (thread (sleepy 500000))))
  ...                                            ; andra tasks fortsätter under tiden
  (join th))                                     ; => 500000
```

- Att anropa en blockerande C-funktion (`defffi`) stoppar bara den tråden.
- En `task` inuti en `thread` körs som en vanlig task på andra trådar.
- Den kan användas i `typl` också. Vid tolkning kompilerar `(thread (f ...))` och `Thread::spawn`
  funktionen som ska köras på stället och kör den sedan på den dedikerade tråden. En `lambda` som
  refererar till lokala variabler utanför sig kan inte kompileras på egen hand och ger panic
  ([Syntaxreferens 12.2](../syntax.md#122-thread--starta-en-task-på-en-dedikerad-os-tråd)). En
  `lambda` som skapas inuti en kompilerad funktion kan skickas.

## 8. Vad som inte finns

- **`Atomic`**. `Mutex` räcker.
- **Task-lokala variabler** (inte heller Go har dem).
- **nil-kanaler**. Skälet och alternativet finns i
  [Syntaxreferens 12.7](../syntax.md#127-skillnader-mot-go).
