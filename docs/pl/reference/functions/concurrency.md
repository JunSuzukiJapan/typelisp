<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Zadania i kanały

Słownictwo zadań (lekkich wątków). `task` i `thread`, które je uruchamiają, oraz `select`,
który czeka na kilka rzeczy, są formami specjalnymi i znajdują się w
[Referencji składni](../syntax.md#12-współbieżność-zadania). Ten rozdział omawia resztę: typy, metody
i funkcje.

Zadania są **kooperacyjne**: zadanie przełącza się tylko w miejscach, które zapiszesz. Zadania działają jednocześnie na
`TYPELISP_THREADS` wątkach systemu operacyjnego (w `typl` na inne wątki wychodzą tylko zadania skompilowane). To, gdzie
się przełączają i czym różni się to od Go, opisano w
[Referencji składni 12.5](../syntax.md#125-gdzie-zadania-się-przełączają) oraz
[12.7](../syntax.md#127-różnice-względem-go).

## 1. `Task<T>` — uchwyty zadań

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Czeka na zakończenie i zwraca jego wartość |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Możesz użyć `wait` dowolną liczbę razy** (wartość jest buforowana). W przeciwieństwie do `JoinHandle::join` z Rust
  nie zużywa uchwytu, więc można na nie czekać z kilku miejsc.
- **Zadanie działa, nawet jeśli nigdy nie użyjesz `wait`.** Porzucenie uchwytu go nie zatrzymuje.
- Jest to zwykła wartość, więc może trafić do `Vector<Task<()>>`.
- **Gdy główne zadanie się kończy, kończy się proces** (jak w Go). Inne działające zadania są ucinane,
  a kod sprzątający `unwind-protect` nie jest wykonywany, ponieważ jest to zakończenie procesu, a nie rozwijanie stosu.

## 2. `Chan<T>` — kanały

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Kanał o pojemności `n`. `0` to rendezvous (bez bufora) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Czeka, aż zrobi się miejsce, a potem przekazuje wartość |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Czeka, aż nadejdzie wartość. `none`, gdy jest zamknięty i pusty |
| `close` | `(close ch)` | `(Chan<T>)→()` | Zamyka go |
| `len` | `(len ch)` | `(Chan<T>)→int` | Ile wartości jest teraz w buforze |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Pojemność |

**Argument typu podaje się za pomocą `the`** (zapisywane tak samo jak `(the Vector<i32> (Vector::new))`).
**Pojemność zawsze trzeba zapisać**: dwa przypadki, które w Go zapisuje się jako `make(chan int)` i
`make(chan int, 16)`, zapisuje się `(Chan::new 0)` i `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; for v := range ch z Go
```

- **`Chan<T>` jest własnym iteratorem** (implementuje `Iter`). `recv` zwraca `Option<T>`, ten sam
  typ co `Iter::next`, więc `doiter` i `map`/`filter`/`foldl` działają na nim bez zmian.
- **`send` na zamkniętym kanale powoduje panic**, a **drugie `close` także powoduje panic** (oba jak w Go). To
  błędy w programie, a nie niepowodzenia odwracalne, więc nie są `Result`.
- **Zamknięcie kanału, na którego `send` czeka zadanie, powoduje panic tego zadania** (reguła Go).
- `recv` na zamkniętym kanale zwraca to, co zostało w buforze, a gdy jest pusty, stale zwraca
  `none`.
- `close` jest rozstrzygane według typu odbiorcy, więc jest czymś innym niż `close` traitu
  `Stream`. `Chan<T>` nie implementuje `Stream`.
- **Ujemna pojemność powoduje panic** (nie jest po cichu zaokrąglana do 0).

## 3. `yield` / `sleep` — ustępowanie

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Rezygnuje z reszty swojej kolejki (`runtime.Gosched` z Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Zatrzymuje **tylko to zadanie**. Pozostałe działają dalej |

`sleep` zatrzymuje zadanie, a nie wątek. Dopiero gdy żadne zadanie nie może działać, przechodzi do
`sleep` systemu operacyjnego do najbliższego terminu. `(sleep 0.0)` to „yield na 0 sekund" z CL.

Tak jak w CL, `sleep` przyjmuje **sekundy**. Liczby całkowite nie są automatycznie konwertowane na liczby zmiennoprzecinkowe,
więc `(sleep 1)` z CL zapisuje się tutaj `(sleep 1.0)`. Wartość ujemna lub NaN powoduje panic.

## 4. `WaitGroup` — oczekiwanie na N zakończeń

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Grupa bez niczego oczekującego |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Zwiększa licznik. Rób to, zanim praca się zacznie |
| `done` | `(done wg)` | `(WaitGroup)→()` | Jedno się zakończyło. Przy 0 każdy czekający zostaje zwolniony |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Czeka, aż osiągnie 0. Z dowolnej liczby zadań |

`(wait wg)` i `(wait task)` są rozstrzygane według typu odbiorcy, więc współistnieją pod jedną nazwą. Jeśli
licznik spadnie poniżej 0, następuje panic (`done` wywołane zbyt wiele razy lub ujemne `add`). Tak jak w Go, grupa,
która wróciła do 0, może być użyta ponownie, zaczynając od `add`. Żadna aktualizacja nie ginie, nawet gdy zadania działają
na osobnych wątkach systemu operacyjnego.

**Gdy dostępne jest `Task<T>`, jest potrzebny rzadziej niż w Go**: `(doiter (t tasks) (wait t))` często
wystarcza. Jest to narzędzie dla pracy, która rośnie dynamicznie, lub gdy nie chcesz przechowywać uchwytów.

```lisp
;; fan-in: uruchom jedno zadanie na wejście i połącz je (ten język nie ma kanałów nil)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — kanał dostarczający po upływie czasu

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Kanał dostarczający jedną wartość po `sec` sekundach |

`time.After` z Go. Można go zapisać bez zmian w ramieniu limitu czasu w `select`
([Referencja składni 12.3](../syntax.md#123-select--oczekiwanie-na-kilka-operacji-na-kanałach-naraz)). Jego
pojemność wynosi 1, więc zadanie wysyłające może się zakończyć, nawet jeśli nikt nie odbiera.

## 6. `Mutex<T>` — wzajemne wykluczanie dla współdzielonych danych

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Odblokowany mutex przechowujący `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Zajmuje zamek (czeka) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Zwalnia go. Powoduje panic, jeśli nie jest zajęty |
| `with-lock` | `(with-lock (x m) body...)` | Makro | Zajmuje zamek, wiąże zawartość z `x`, wykonuje `body` i **zawsze** zwalnia |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` nie jest kopią wartości, lecz „miejscem"** (`symbol-macrolet`). `(setf x 42)` zmienia
  zawartość mutexu.
- `with-lock` zwalnia za pomocą `unwind-protect`, więc zamek jest zwalniany bez względu na to, jak ciało zostanie opuszczone: normalne
  zakończenie, `throw`, `panic` lub `break`/`return`/`return-from`.
- **Ponowne wejście powoduje zakleszczenie** (nie powoduje panic). Planista zgłasza, że „nic nie może posunąć się naprzód"
  dla zadania uwięzionego na własnym zamku.
- **`m::v` sięga do zawartości spoza zamka**, co jest niezdefiniowane w tym sensie, że inne
  zadanie może być w trakcie jej zmieniania. To to samo stanowisko co `sync.Mutex` z Go: w
  języku bez własności i sprawdzania pożyczek nie da się zbudować statycznej gwarancji w rodzaju `MutexGuard`.

## 7. `Thread<T>` — dedykowane wątki systemu operacyjnego

Uchwyt zwracany przez `(thread (f args...))`
([Referencja składni 12.2](../syntax.md#122-thread--uruchamianie-zadania-na-dedykowanym-wątku-systemu-operacyjnego)). Odpowiednik
`Task<T>`.

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Czeka na zakończenie i zwraca wartość (wywołujące **zadanie** się zatrzymuje. Można wywołać dowolną liczbę razy; wartość jest buforowana) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | Wersja funkcyjna `(thread (f))` (`std::thread::spawn` z Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Numer działającego wątku systemu operacyjnego. Unikalny w obrębie procesu, bez znaczenia poza „czy to ten sam wątek" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Liczba wątków, które maszyna może uruchomić naraz (domyślna wartość `TYPELISP_THREADS`). Powoduje panic, jeśli system nie odpowie |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; blokująca funkcja C
(let ((th (thread (sleepy 500000))))
  ...                                            ; w tym czasie inne zadania działają dalej
  (join th))                                     ; => 500000
```

- Wywołanie blokującej funkcji C (`defffi`) zatrzymuje tylko ten wątek.
- `task` wewnątrz `thread` działa jako zwykłe zadanie na innych wątkach.
- Można go używać także w `typl`. Przy interpretowaniu `(thread (f ...))` i `Thread::spawn` kompilują
  funkcję do uruchomienia na miejscu, a potem uruchamiają ją na dedykowanym wątku. `lambda`, która odwołuje się do lokalnych
  zmiennych spoza niej, nie może zostać skompilowana samodzielnie i powoduje panic
  ([Referencja składni 12.2](../syntax.md#122-thread--uruchamianie-zadania-na-dedykowanym-wątku-systemu-operacyjnego)). `lambda` utworzoną wewnątrz skompilowanej
  funkcji można przekazać.

## 8. Czego nie ma

- **`Atomic`**. Wystarczy `Mutex`.
- **Zmiennych lokalnych zadania** (Go też ich nie ma).
- **Kanałów nil**. Powód i alternatywa są opisane w
  [Referencji składni 12.7](../syntax.md#127-różnice-względem-go).
