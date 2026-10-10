<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result i typy błędów

## 1. `Option<T>` / `Result<T,E>`

Konstruktory: `Option<T>` ma `Some(T)` / `None`. `Result<T,E>` ma `Ok(T)` / `Err(E)`. `E` może być dowolnym
typem: wbudowane konkretne typy błędów oraz typy, które piszesz sam za pomocą `defstruct`/`defenum`,
pasują tam tak samo (rozdział 3).

| Nazwa | Forma | Option | Result | Opis |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Wyjmuje wartość. Powoduje panic dla `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Wartość lub wartość domyślna |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Czy jest to `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Czy jest to `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Czy jest to `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Czy jest to `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Wyjmuje wartość. Przy `None`/`Err` panic z `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Wartość albo wynik `f`. `f` jest wywoływana tylko przy `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Stosuje `f` do zawartości `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Stosuje `f` do zawartości `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Przy `Some`/`Ok` przekazuje zawartość do `f` i zwraca jej wynik |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Przy `None`/`Err` zwraca wynik `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Zamienia `Some(v)` na `Ok(v)`, a `None` na `Err(e)` |

Konstruktorami są `Option::some`/`Option::none`/`Result::ok`/`Result::err` (lub, po
`(use option)`/`(use result)`, same nazwy `some`/`none`/`ok`/`err`).

Rozgałęzienie zapisuje się jawnie przez `match` albo łączy w łańcuch przez `map`/`and-then` i
pozostałe powyżej. Nie ma składni odpowiadającej `?` z Rusta.

`map` dla `Option`/`Result` to metoda, odrębna od `map` dla [sekwencji](sequences.md). To ona jest
wywoływana, gdy typ pierwszego argumentu to `Option`/`Result`.

Makro `->` przekazuje wartość kolejno jako pierwszy argument każdej następnej formy (tak jak `->` w
Clojure). `(-> x (f a) (g b))` staje się `(g (f x a) b)`. Nazwa bez nawiasów, `h`, jest traktowana
jak `(h x)`. Pierwszym argumentem metody jest jej odbiorca, więc kombinatory łączą się w łańcuch bez
zmian:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. Reprezentacja `Option<T>` w czasie działania

Tak jak w Rust, **`Option<T>` zwykle nie tworzy pudełka (box)**. `some v` to samo `v`, a `none` to
wartość pustej listy, bez alokacji i bez pośredniości. `Option<Sexpr>` (gdzie pusta lista to
`none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` i `Option<(fn ...)>`
przyjmują tę postać.

Pudełko jest używane tylko wtedy, gdy wartości typu `T` nie da się odróżnić od wartości pustej listy:

| `T` | Reprezentacja | Powód |
|---|---|---|
| `Option<U>` (zagnieżdżone) | Pudełko | Wewnętrzne `none` byłoby tą samą wartością co zewnętrzne `none` |
| `()` | Pudełko | Wartość `()` jest samą wartością pustej listy |
| `ptr` / `c-long` / `c-ulong` | Pudełko | Wszystkie 64 bity to wartość, nie zostaje miejsca na odróżnienie |
| Cokolwiek innego | Bez pudełka | — |

Reprezentacja jest określana wyłącznie przez typ i nie da się jej odczytać z wartości. Przy wypisywaniu
`(some ...)`/`none` jest odtwarzane ze statycznego typu, więc `(format false "~a" opt)` wypisuje
`(some 1)`. Są dwa ograniczenia:

- **Nie można go umieścić w `:dyn Trait`** (przekazanie wartości `Option<int>`, dla której napisałeś
  `(impl Speak Option<int> ...)`, do `:dyn Speak` jest błędem).
- Rzutowanie w dół `(the Option<T> ...)` z `Sexpr` **nazywa konstruktor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Forma wiążąca całą wartość,
  `(the Option<int> o)`, jest błędem.

## 3. Typy błędów i trait `Error`

Idąc za `std::error::Error` z Rust, **`Error` nie jest typem, lecz traitem**. Konkretne typy, które
reprezentują błędy, są osobne dla każdego celu, a każdy implementuje `Error`.

| Typ | Tworzony przez |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operacje na plikach i strumieniach ([Strumienie i pliki](streams-files.md)) |
| `NetError` | Operacje sieciowe ([Sieć](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` z CL: domyślny wybór, gdy chcesz tylko powiedzieć, co się stało |
| `WrappedError` | `(wrap-error msg cause)`. Typ niosący zarówno twój własny komunikat, jak i przyczynę; to dlatego trait `Error` ma `source` |

`ParseIntError` do `NetError` to każdy „enum z pojedynczym wariantem przechowującym jeden komunikat w postaci
łańcucha znaków", a nazwa typu i nazwa wariantu są takie same (`(match e ((ParseIntError m) m))`, tworzone
za pomocą `(ParseIntError::ParseIntError "...")`). Nie ma w nich nic szczególnego: są traktowane
dokładnie tak samo jak twoje własne typy błędów napisane za pomocą `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Komunikat błędu (metoda traitu `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Przyczyna, którą ten błąd opakowuje, lub `None`, jeśli jej nie ma (`Error::source` z Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementuje `Error`) | Rozszerza konkretny typ błędu do obiektu traitu |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementuje `Error`) | Komunikat i łańcuch przyczyn znalezionych przez podążanie za `source`, jedna przyczyna w linii. CL nie ma odpowiednika („caused by" z Rust) |

Jeśli zaimplementujesz `Error` dla własnego typu błędu, można go obsługiwać **tak samo** jak wbudowane
błędy:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; konkretny typ trafia do E bez zmian
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; obsłuż dowolny rodzaj jednolicie
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Aby zebrać kilka typów błędów w jednym `Result`, użyj `Result<T, :dyn Error>` (odpowiednik
`Box<dyn Error>` z Rust) i rozszerzaj konkretne błędy za pomocą `as-dyn-error`. Ponieważ nie ma `?`, ta
konwersja jest zapisywana jawnie:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Typy i traity dzielą jedną przestrzeń nazw** (jak w Rust). W obrębie jednego modułu `defstruct`/`defenum` i
trait nie mogą mieć tej samej nazwy, a zapisanie nazwy traitu w pozycji typu jest zgłaszane jako
„`error` is a trait, not a type — write `:dyn error`".

Niepowodzenia nieodwracalne wyraża się za pomocą `panic`. Informacje o `panic` i `catch`/`throw` znajdziesz w
[Referencji składni](../syntax.md#8-wyjścia-nielokalne-catch--throw--unwind-protect); o zasadach obsługi błędów
w [rozdziale 9 tego samego dokumentu](../syntax.md#9-zasady-obsługi-błędów).
