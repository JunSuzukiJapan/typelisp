<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Współbieżność

W typelisp uruchamiasz pracę współbieżnie, startując **zadania** (lekkie wątki), a zadania przekazują
sobie wartości przez **kanały**. Model jest bliski gorutynom i kanałom z Go.
Ten rozdział omawia po kolei uruchamianie zadania i pobieranie jego wyniku, kanały, `select`,
ochronę współdzielonych danych oraz dedykowane wątki systemu operacyjnego. Zakłada, że przeczytałeś
[Podstawy typów](types.md).

## 1. Uruchamianie zadania i oczekiwanie na jego wynik

`(task (funkcja argumenty...))` uruchamia wywołanie funkcji jako nowe zadanie. Strona uruchamiająca nie
czeka i idzie dalej. Wartością jest uchwyt typu `Task<T>`; `(wait uchwyt)` czeka, aż zadanie
się zakończy, i zwraca jego wynik.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; czekaj 0,1 sekundy (zatrzymuje się tylko to zadanie)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` przyjmuje wyłącznie formę wywołania funkcji. Argumenty są obliczane tam, gdzie zapisano `task`;
  w nowym zadaniu działa tylko samo wywołanie.
- Aby uruchomić kilka wyrażeń, zrób `lambda` i wywołaj ją od razu:
  `(task ((lambda () () (println "start") (work))))`
- Możesz wywołać `wait` dowolną liczbę razy. Wynik jest zapamiętywany.
- Zadanie działa, nawet jeśli nigdy na nie nie zaczekasz za pomocą `wait`.
- **Gdy główna praca się kończy, program się kończy.** Zadania, które jeszcze działają, są ucinane.

## 2. Przekazywanie wartości przez kanały

Kanał `Chan<T>` to droga, którą zadania przekazują sobie wartości typu `T`. Argumentem
`Chan::new` jest pojemność (ile wartości kanał może pomieścić). W kanale o pojemności 0 nadawca
i odbiorca czekają oboje, aż druga strona będzie na miejscu.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; wyślij
  (close ch))                  ; koniec wysyłania

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; odbieraj do zamknięcia
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` wysyła. Jeśli kanał jest pełny, czeka, aż zrobi się miejsce.
- `(recv ch)` odbiera. Czeka, aż nadejdzie wartość. Wynikiem jest `Option<T>`; gdy
  kanał jest zamknięty i pusty, zwraca `none`.
- Iterowanie po kanale za pomocą `doiter` odbiera kolejne wartości aż do zamknięcia kanału. Można go też
  przekazać wprost do `map` lub `filter`.
- `send` na zamkniętym kanale powoduje panic.

### Dzielenie pracy między kilka zadań

Częsty wzorzec to jeden kanał niosący prace i kilku pracowników (zadań przetwarzających prace) pobierających z niego
zlecenia. Zlecenie bierze ten pracownik, który jest akurat wolny, więc nawet gdy wolne
i szybkie zlecenia są wymieszane, praca rozkłada się naturalnie.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; ile trwa to zlecenie

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; bierz po jednym zleceniu, aż jobs zostanie zamknięty
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; praca; w tym czasie inni pracownicy biorą kolejne zlecenia
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; ile zleceń wykonał ten pracownik

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
  (close jobs)                    ; to cała praca
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

- A, B i C biorą po jednym z pierwszych trzech zleceń.
- Po 0,1 sekundy B i C są wolne i biorą pozostałe zlecenia. Dopóki A jest zajęty wolnym zleceniem
  1, nie bierze nowej pracy.
- Na końcu A obsłużył jedno zlecenie, B dwa, a C trzy. Nic w programie nie mówi, który pracownik bierze
  które zlecenie.
- Zamknięcie `jobs` kończy `doiter` każdego pracownika, zadania się kończą, a każde `wait` zwraca swoją liczbę.

`jobs` to kanał o pojemności 0, więc `send` czeka, aż któryś pracownik weźmie zlecenie. Przy większej
pojemności główne zadanie mogłoby kolejkować pracę, nie czekając na pracowników.

## 3. `select`: oczekiwanie na kilka kanałów naraz

`select` wykonuje tę z kilku operacji na kanałach, która stanie się możliwa jako pierwsza. `(after sekundy)`
to kanał, który dostarcza jedną wartość po upływie zadanego czasu. W połączeniu z `select`
daje limit czasu (timeout).

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

- `((v (recv ch)) body...)` to ramię odbierające. `v` dostaje `Option<T>`.
- `((send ch x) body...)` to ramię wysyłające.
- Gdy kilka ramion może przejść jednocześnie, jedno z nich jest wybierane losowo.
- Z `(else body...)` na końcu `else` jest wykonywane, gdy żadne ramię nie może przejść od razu, a `select`
  nie czeka.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Ochrona współdzielonych danych

Gdy kilka zadań modyfikuje tę samą wartość, chroń ją za pomocą `Mutex<T>`. `with-lock` zajmuje zamek,
wiąże zawartość ze zmienną, wykonuje ciało i zawsze zwalnia zamek, bez względu na to, jak ciało
zostanie opuszczone. Przypisanie do zmiennej za pomocą `setf` wewnątrz ciała zmienia zawartość `Mutex`.

`WaitGroup` to narzędzie do czekania, aż zakończy się zadana liczba zadań. Zwiększ licznik za pomocą
`add`, niech każde zadanie wywoła `done`, gdy się skończy, i czekaj za pomocą `wait`, aż licznik osiągnie 0.

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

Jeśli kilka zadań modyfikuje tę samą wartość w tym samym czasie, nie korzystając z `Mutex` ani z
kanału, wynik nie jest gwarantowany. Dane między zadaniami przekazuj, gdzie tylko się da, przez kanały, a
współdziel je tylko wtedy, gdy jest to potrzebne.

## 5. Gdzie zadania się przełączają

Zadania przełączają się kooperacyjnie. Zadanie ustępuje miejsca innym zadaniom tylko w tych punktach:

- `(yield)`, `(sleep sekundy)`, `(wait uchwyt)`
- Operacja na kanale, która musi czekać (`send`, `recv`, `select`)
- Operacja na gnieździe (socket), która musi czekać (łączenie, odczyt, zapis i tak dalej)

Argumentem `sleep` jest liczba sekund typu `f64`. Pisz `(sleep 1.0)`, a nie `(sleep 1)`.

Zadania działają jednocześnie na kilku wątkach systemu operacyjnego. Jednak gdy `typl` uruchamia program bezpośrednio,
na inne wątki wychodzą tylko zadania uruchamiające funkcje [skompilowane](../guide/compile.md). Pozostałe
zadania działają na jednym wątku, przełączając się w powyższych punktach.

## 6. `thread`: uruchamianie na dedykowanym wątku systemu operacyjnego

Pracę, która nie powinna blokować innych zadań, jak wywołanie powolnej funkcji C ([FFI do C](../guide/ffi.md)),
uruchamia się za pomocą `thread`. Zapisuje się go tak samo jak `task`, a dostaje on własny wątek systemu operacyjnego.
Na jego zakończenie czeka się za pomocą `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- Uchwyt z `thread` ma typ `Thread<T>`. Podobnie jak `wait`, `join` można wywołać dowolną
  liczbę razy.
- `thread` może uruchamiać tylko funkcje, które da się skompilować. Przy działaniu w `typl` wywoływana
  funkcja jest kompilowana na miejscu, zanim zostanie uruchomiona.

## 7. Zadania i inne funkcje

- `panic` wewnątrz zadania zatrzymuje cały program.
- `throw` nie sięga poza zadanie. `throw`, który opuściłby ciało zadania, staje się
  `panic`.
- Wyjście jednego `println` nigdy nie jest mieszane w środku linii z wyjściem innych zadań.

## 8. Co czytać dalej

- [Zadania i kanały](../reference/functions/concurrency.md): lista funkcji
- [Referencja składni, rozdział 12](../reference/syntax.md#12-współbieżność-zadania): szczegóły dotyczące tego, gdzie zadania się przełączają,
  oraz różnice względem Go
- [Operacje na plikach, strumienie i sieć](../guide/io.md): pisanie serwera z użyciem zadań
