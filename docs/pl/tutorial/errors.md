<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Obsługa błędów

Obsługa błędów w typelisp dzieli niepowodzenia na dwa rodzaje.

| Rodzaj niepowodzenia | Przykłady | Jak jest wyrażane |
|---|---|---|
| Niepowodzenia, które mogą się zdarzyć (odwracalne) | Brakuje pliku, dane wejściowe nie są liczbą | Zwrócenie `Result<T,E>` |
| Błędy w programie (nieodwracalne) | Indeks poza zakresem, `unwrap` na `none`, dzielenie przez zero | Zatrzymanie za pomocą `panic` |

Poza tym istnieją `catch` / `throw`, które opuszczają naraz wiele wywołań funkcji, oraz
`unwind-protect`, które uruchamia kod sprzątający bez względu na to, jak opuszczono jego ciało. Ten rozdział zakłada, że przeczytałeś
sekcję o `Result` w [Podstawach typów](types.md).

## 1. Zwróć `Result` i odbierz go za pomocą `match`

Oto funkcja odczytująca numer portu z łańcucha znaków. Może zawieść na dwa sposoby: dane wejściowe nie są
liczbą albo liczba jest poza zakresem.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Wywołujący oddziela sukces od porażki za pomocą `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Wartości zwróconej przez funkcję zwracającą `Result` nie można użyć, jeśli `match` nie obsługuje przypadku `err`.
  Zapomnienie o obsłudze porażki jest błędem typu.
- Błąd z `parse-int` jest wartością typu `ParseIntError`. `(message e)` daje jego komunikat
  w postaci łańcucha znaków.

## 2. Przekazywanie porażki wywołującemu

Nie ma skrótu w rodzaju `?` z Rust. Przy kolejnym wywoływaniu kilku funkcji zwracających `Result`
część „zwróć porażkę bez zmian" zapisuje się za pomocą `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Gdy wiesz, że operacja nie może zawieść, albo w małym skrypcie, w którym zatrzymanie przy porażce jest w porządku,
`unwrap` wyjmuje zawartość. Jeśli wartością jest `err`, następuje panic. Jeśli wystarczy wartość domyślna,
użyj `unwrap-or`.

## 3. Tworzenie własnego typu błędu

Wyrażenie błędów jako typu, a nie łańcucha znaków, pozwala wywołującemu rozgałęziać się według rodzaju błędu. Typ
błędu to zwykłe `defenum` lub `defstruct` implementujące trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; brakuje ustawienia
  (invalid string int))     ; wartość jest błędna

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` zwraca opis błędu.
- `source` zwraca inny błąd, który spowodował ten. Gdy nie ma przyczyny, jest to `none`.

## 4. Łączenie różnych rodzajów błędów

Jeśli jedna funkcja wywołuje zarówno `parse-int` (`ParseIntError`), jak i `check-workers` (`config-error`), są
dwa typy błędów i nie mogą one być jednocześnie `E` w jednym `Result<T,E>`. W takim przypadku ustaw `E` na
`:dyn Error` (błąd dowolnego typu implementującego `Error`). Każdy błąd konwertuje się za pomocą
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Dla `"4"`, `"-1"` i `"abc"` wyniki są następujące:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Informacje o `:dyn` znajdziesz w sekcji 5 rozdziału [Traity](traits.md).

## 5. `panic`: błędy w programie

Gdy program dojdzie do stanu, który nigdy nie powinien się zdarzyć, zatrzymaj go za pomocą `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Typem `panic` jest `!` (nie wraca), więc można go zapisać wszędzie tam, gdzie oczekiwany jest dowolny typ. Dlatego
  obie gałęzie powyższego `if` do siebie pasują.
- Również te operacje powodują panic: `unwrap` na `none` lub `err`, `get` z indeksem poza zakresem oraz
  dzielenie liczb całkowitych przez zero.
- `panic` zatrzymuje program. Nawet gdy zdarzy się wewnątrz zadania, zatrzymany zostaje cały program.
- W REPL `panic` nie kończy REPL; ten czeka na następne dane wejściowe.
- Możesz napisać `(todo)` dla „jeszcze nie napisane" i `(unreachable)` dla „do tego punktu nigdy nie
  powinno się dojść". Oba powodują panic.

`panic` nie zastępuje `Result`. Dla niepowodzeń, które mogą się zdarzyć, takich jak dane wejściowe użytkownika lub to,
czy plik istnieje, użyj `Result`.

## 6. `catch` / `throw`: wyskakiwanie ponad funkcjami

`throw` wyskakuje prosto do otaczającego `catch` z tym samym znacznikiem (tag), bez względu na to, ile wywołań funkcji
leży pomiędzy.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Jeśli `v` nie zawiera liczby ujemnej, `validate` zwraca `"all fine"`; jeśli zawiera `-7`, sterowanie przeskakuje
z wnętrza `check-all` do `catch`, które zwraca `"negative: -7"`.

- Znacznik zapisuj jako zwykły symbol, jak `'bad-input`.
- **Każdy znacznik niesie wartości dokładnie jednego typu.** W powyższym przykładzie `'bad-input` niesie
  `string`, więc rzucenie `int` z tym samym znacznikiem jest błędem typu. Typ ciała `catch`
  musi także zgadzać się z typem znacznika.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw`, do którego nie ma `catch` z tym samym znacznikiem, jest błędem.

Jeśli chcesz tylko wcześniej wrócić z wnętrza funkcji, użyj `return-from` zamiast `catch` /
`throw`. `return-from` nie może przekraczać granic funkcji, ale w zamian miejsce, do którego wraca, można rozpoznać
czytając kod źródłowy.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: zawsze posprzątaj

`(unwind-protect body cleanup)` uruchamia kod sprzątający bez względu na to, jak opuszczono ciało: gdy kończy
się normalnie, gdy jest opuszczone przez `throw` i gdy następuje panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Używaj go do rzeczy w rodzaju „zawsze zamknij otwarty plik" lub „zawsze zwolnij zajęty zamek". Standardowe
`with-open-file` i `with-lock` używają wewnętrznie `unwind-protect`.

## 8. O systemie kondycji z Common Lisp

typelisp nie przyjmuje systemu kondycji z Common Lisp (`handler-case`, `restart-case` i tak dalej).
Nie pokazuje on w typach, jakie niepowodzenia może spowodować funkcja, co słabo pasuje do typowania
statycznego. Niepowodzenia, które mogą się zdarzyć, zapisuje się w typach za pomocą `Result`, a przekazania sterowania
realizuje się za pomocą `catch` / `throw`.

## 9. Co czytać dalej

- [Współbieżność](concurrency.md): zadania i kanały
- [Option, Result i typy błędów](../reference/functions/option-result.md): lista funkcji
- [Komunikaty o błędach](../reference/errors.md): co oznaczają typowe błędy i jak je naprawić
