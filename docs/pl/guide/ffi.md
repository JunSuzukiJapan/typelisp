<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# FFI do C (defffi)

Ten przewodnik wyjaśnia, jak wywoływać funkcje C z typelisp. Lista typów, które można deklarować, oraz
ograniczenia znajdują się w [Referencji składni 3.3](../reference/syntax.md#33-defffi--deklarowanie-funkcji-c-ffi).

## 1. Deklarowanie i wywoływanie funkcji

`defffi` deklaruje nazwę i typy funkcji C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; nazwa w typelisp i nazwa symbolu C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; szukaj w libm
```

Wywołania opakowuje się w `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` jest potrzebne, ponieważ kompilator nie może sprawdzić, czy zadeklarowane typy zgadzają się z rzeczywistymi typami po
stronie C. Zapisanie `unsafe` oznacza, że to ty, piszący, bierzesz odpowiedzialność za to sprawdzenie. Zapomnienie o nim
daje błąd, który to wyjaśnia.

## 2. Pisanie bezpiecznego opakowania

Zamierzone użycie polega na ograniczeniu `unsafe` do jednego miejsca i przedstawieniu na zewnątrz zwykłej funkcji.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; wywołujący nie potrzebuje unsafe
(str-len "hello")  ; => 5
```

## 3. Jak odpowiadają sobie typy

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Liczby całkowite o tej samej szerokości |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (także `size_t`, `int64_t` i tak dalej) |
| `ptr` | Dowolny wskaźnik (`void *`, `FILE *` i tak dalej) |
| `(ptr T)` | Wskaźnik na `T` ([sekcja 7](#7-struktury-c)) |

### Łańcuchy znaków

- Przekazywany `string` jest kopiowany do łańcucha C zakończonego znakiem NUL, który jest zwalniany po powrocie
  z wywołania. NUL w środku łańcucha jest błędem.
- Wynik funkcji zwracającej `string` jest również kopiowany. Pamięć po stronie C nie jest zwalniana.
  W przypadku funkcji zwracających łańcuch, który wywołujący musi zwolnić (jak `strdup`), przyjmij wynik jako
  `ptr` i sam go zwolnij za pomocą `free`.
- Jeśli funkcja zadeklarowana jako zwracająca `string` zwróci NULL, jest to błąd. Wynik
  funkcji, które mogą zwrócić NULL (jak `getenv`), przyjmuj jako `ptr`.

### `c-long` / `c-ulong` / `ptr`

Te typy istnieją wyłącznie do przekazywania wartości przez granicę z C i **nie obsługują arytmetyki**. Aby
użyć któregoś jako liczby całkowitej typelisp, przekonwertuj go za pomocą `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int nie traci żadnej części 64-bitowej wartości
(try-as i32 (unsafe (c-strlen s)))  ; none, jeśli nie mieści się w i32
(unsafe (c-malloc 16))              ; literały całkowite można przekazywać wprost
```

`ptr` to wartość, którą należy przekazać z powrotem do funkcji C. Nie ma sposobu, by odczytać po stronie typelisp,
na co wskazuje.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Te typy mogą występować tylko jako argumenty funkcji, wartości zwracane i zmienne lokalne. Nie mogą być
polami struktur, zmiennymi globalnymi ani argumentami typu `Vector` i podobnych.

## 4. Nazywanie biblioteki

Bez `:library` symbol jest wyszukiwany w tym, co jest już zlinkowane z procesem (libc i tak
dalej). Funkcje z innych bibliotek wymagają `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Krótka nazwa, taka jak `"sqlite3"`, jest wyszukiwana jako `libsqlite3.dylib`, a potem `libsqlite3.so`.
- Nazwa zawierająca `/` jest traktowana jako ścieżka.
- Jeśli zadeklarowany symbol nie zostanie znaleziony, błąd podaje jego nazwę.

## 5. Kompilacja AOT

Programy używające `defffi` można zamienić w pliki wykonywalne za pomocą
[`compile-file`](compile.md#3-budowanie-pliku-wykonywalnego-za-pomocą-kompilacji-aot) bez zmian. Biblioteki wskazane
za pomocą `:library` są dodawane przy linkowaniu automatycznie, więc `compile-file` nie potrzebuje dodatkowych argumentów.

## 6. Wywołania zwrotne (callbacki)

Możesz przekazać funkcję typelisp do funkcji C i pozwolić jej ją wywołać zwrotnie. Zapisz typ funkcji
wśród typów argumentów `defffi`, a przy wywołaniu umieść w tym miejscu nazwę funkcji lub wyrażenie `lambda`.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") zwraca p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Można przekazywać tylko funkcje **bez zmiennych wolnych**. Funkcje najwyższego poziomu, `lambda` i lokalne
  funkcje `labels` działają, ale odwołanie do zmiennej lokalnej otaczającego zakresu jest błędem
  w czasie sprawdzania typów. C przekazuje tylko zadeklarowane argumenty, więc nie ma sposobu na dostarczenie przechwyconych
  zmiennych. Aby zachować stan, użyj zmiennych globalnych.
- Zmiennej przechowującej funkcję nie można przekazać. Zapisz w tym miejscu nazwę funkcji lub wyrażenie `lambda`.
- `panic` lub `throw` wewnątrz wywołania zwrotnego dociera do wywołującego po powrocie funkcji C.
- Wywołanie zwrotne może być wywołane tylko wtedy, gdy działa funkcja C wywołana przez typelisp. Nie można go
  używać z takich miejsc jak `atexit` czy obsługa sygnałów.

## 7. Struktury C

Aby przekazać do funkcji C coś w rodzaju tablicy struktur, zadeklaruj strukturę o takim samym układzie jak
w C za pomocą `def-c-struct` i zaalokuj ją wewnątrz `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; zadeklarowane wewnątrz unsafe najwyższego poziomu

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; cztery elementy, wszystkie wyzerowane
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` alokuje `n` wartości typu `T` i zwraca `(ptr T)`. `(c-ref p i)` to wskaźnik na
  `i`-ty element, `p::field` to pole, a `(c-deref p)` to to, na co wskazuje wskaźnik na skalar, taki jak `i32`.
  Wszystkie te formy można zapisać z `setf`.
- `(as ptr p)` zamienia go na nietypowany `ptr`, aby przekazać do funkcji C przyjmujących `void *`.
- Rozmiar `item` (tutaj 8) i położenie każdego pola są określane według tych samych reguł co
  w C.

### Czas życia zaalokowanej pamięci

Zaalokowana pamięć jest zwalniana, gdy sterowanie opuszcza najbardziej zewnętrzne `unsafe` w tej funkcji. To samo
dzieje się, gdy zostaje opuszczone przez `panic` lub `throw`. Z tego powodu wartości `(ptr T)` nie można wynieść
poza `unsafe`. Uczynienie jej wartością `unsafe`, przechwycenie jej w domknięciu, przekazanie jej do
`task` oraz rzucenie jej za pomocą `throw` są błędami typu. Wartości, których chcesz używać na zewnątrz, skopiuj do
liczb lub do `defstruct` wewnątrz `unsafe`.

Przy alokowaniu wewnątrz `lambda` lub funkcji `labels` zapisz `unsafe` wewnątrz tej funkcji.

### Pamięć zaalokowana przez C

Wskaźnik otrzymany z C jako `(ptr T)` (wartość zwracana przez `defffi`, argument wywołania zwrotnego i tak dalej) jest
błędem, o ile nie wskazuje wewnątrz pamięci zaalokowanej za pomocą `c-alloc`. Funkcje, które otrzymują
pamięć zaalokowaną przez C za pomocą `malloc`, lub NULL, deklaruj z nietypowanym `ptr`.

## 8. Czego nie można zrobić

- **Funkcji o zmiennej liczbie argumentów** (`printf` i podobnych) nie można deklarować. Część zmienna jest przekazywana
  według innych reguł niż argumenty stałe. Zadeklaruj osobną nazwę dla każdej używanej liczby
  argumentów.
- **Przekazywanie lub zwracanie struktur przez wartość** nie jest możliwe. Używaj funkcji przekazujących wskaźniki.
- **Deklaracje generyczne** nie są możliwe.
- **Tej samej nazwy co funkcja wbudowana** nie można używać.
- **Nie można ich przekazywać jako wartości funkcyjnych.** Nie można przekazać jednej jak w `(map xs c-abs)`; opakuj ją
  w `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
