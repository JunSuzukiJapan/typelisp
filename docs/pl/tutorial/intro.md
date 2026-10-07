<!-- translated-from: docs/ja/tutorial/intro.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Pierwsze kroki

Zaczynając od obliczania wyrażeń w REPL, ten rozdział omawia po kolei funkcje, zmienne,
instrukcje warunkowe, pętle oraz listy i `Vector`. Informacje o budowaniu `typl` znajdziesz w
[README.md](../../../README.md).

## 1. Uruchamianie REPL

Uruchomiony bez argumentów `typl` wchodzi do REPL (trybu interaktywnego). Wpisz wyrażenie po
`typl>`, a zostanie ono od razu obliczone, po czym wypisana będzie jego wartość. `:quit` kończy REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Od tej pory dane wejściowe i wyniki REPL będą pokazywane w tej postaci.

## 2. Obliczanie wyrażeń

typelisp jest Lispem, więc wyrażenie umieszcza się w nawiasach, a **na pierwszym miejscu stoi
operator lub nazwa funkcji**. Piszesz `(+ 1 2)`, a nie `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Liczby występują w następujących rodzajach:

- **Liczby całkowite** mają typ `int`. Ich wielkość nie ma górnej granicy.
- **Liczby dziesiętne** mają typ `f64`. Zapisuje się je z kropką dziesiętną, jak `1.5` lub `2.0`.
- Nie można mieszać `int` i `f64` w jednym obliczeniu. `(+ 1 2.0)` jest błędem typu. Aby
  dokonać konwersji, napisz `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` na dwóch liczbach całkowitych daje liczbę całkowitą z odrzuconą częścią ułamkową (nie tworzy
ułamka, jak robi to Common Lisp). Do obliczenia reszty użyj `(mod 7 2)`.

Wartościami logicznymi są `true` i `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Definiowanie funkcji

Funkcje definiuje się za pomocą `defun`. **Typy argumentów i typ zwracany zawsze trzeba zapisać.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` oznacza „argument `n` typu `int`". Przy kilku argumentach wypisuje się je po kolei:
  `((a int) (b int))`.
- `int` po liście argumentów to typ zwracany.
- Wartość ostatniego wyrażenia w ciele jest wartością zwracaną funkcji. Nie piszesz
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Wywołanie z niepasującymi typami jest zgłaszane jako błąd typu **zanim zostanie wykonane**. Gdy
uruchamiasz plik, jeden błąd typu w dowolnym miejscu oznacza, że nie zostanie wykonana ani jedna linia programu.

Aby argument uczynić opcjonalnym, użyj `&optional`. Jeśli podasz wartość domyślną, argument
przyjmie ją, gdy zostanie pominięty.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`false` podane jako pierwszy argument `format` oznacza „zwróć wynik jako łańcuch znaków zamiast go
wypisywać". Każde `~a` jest zastępowane kolejnym argumentem.

## 4. Zmienne

Zmienne lokalne tworzy się za pomocą `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Typ zmiennej `let` jest brany z jej wartości początkowej. Nie musisz go zapisywać.
- Zmienne jednego `let` nie mogą się do siebie odwoływać. Aby zbudować jedną zmienną z
  poprzedniej, użyj `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Aby zmienić wartość zmiennej, użyj `setf`. **Przypisanie nie może zmienić typu zmiennej.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Zmienne globalne definiuje się za pomocą `defvar`. Tutaj typ trzeba zapisać.

```lisp
(defvar (counter int) 0)
```

## 5. Instrukcje warunkowe

### if

Pisze się `(if warunek wyrażenie-then wyrażenie-else)`. **Wyrażenia else nie można pominąć.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Warunkiem może być tylko wyrażenie typu `bool`. Zapisanie liczby, jak w `(if 0 ...)`, jest
  błędem typu.
- Wyrażenia then i else muszą mieć ten sam typ.

Gdy w przypadku fałszu nic nie ma się dziać, użyj `when` (a `unless` dla przeciwnego przypadku).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Przy trzech lub więcej warunkach lepiej czyta się `cond`. Końcowe `else` jest wybierane, gdy żaden
z warunków nie jest spełniony.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Aby rozgałęziać się według kształtu wartości, użyj `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` pasuje do każdej wartości. Ponieważ `int` ma niezliczone wartości, pominięcie ramienia `_` jest
błędem informującym, że nie wszystkie przypadki są pokryte. Prawdziwą siłę `match` widać przy
rozbieraniu `Option` i typów definiowanych przez ciebie, które pojawiają się w następnym rozdziale,
[Podstawy typów](types.md).

## 6. Pętle

Funkcja może wywoływać samą siebie.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Dla stałej liczby powtórzeń użyj `dotimes`. `i` przebiega od 0 do `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Są też `while`, `do` oraz rozszerzone `loop` z Common Lisp. Słowa klauzul rozszerzonego
`loop` zapisuje się jako słowa kluczowe (`:for`, `:collect` i tak dalej).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Listy i Vector

### Vector

Aby przechowywać ciąg wartości tego samego typu, użyj `Vector<T>`. `T` to typ elementu.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; wypisuje #<vector<int> 3 1 2>
```

- Samo `(Vector::new)` nie określa typu elementu, więc podaj typ za pomocą
  `(the Vector<int> ...)`.
- `(push v x)` dopisuje na końcu, `(get v i)` odczytuje element `i`, a `(len v)` zwraca długość.
- `get` z indeksem poza zakresem zatrzymuje program z błędem.

### lambda i funkcje wyższego rzędu

Funkcje anonimowe tworzy się za pomocą `lambda`. Podobnie jak w `defun`, zapisujesz typy argumentów
i typ zwracany.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` i pokrewne przyjmują `Vector` zamieniony na **iterator** za pomocą
`(iter v)`. Kolekcja jest pierwsza, a funkcja druga. `v` w następnym przykładzie to `Vector` z
`3 1 2` zbudowany powyżej.

```lisp
(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Aby przetwarzać elementy jeden po drugim, użyj `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Funkcja przyjmująca funkcję jako argument zapisuje typ tego argumentu jako
`(fn (typy-argumentów...) typ-zwracany)`. Funkcję zdefiniowaną za pomocą `defun` można przekazać
jako wartość, podając jej nazwę.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listy (S-wyrażenia)

Listy tworzone za pomocą `'(1 2 3)` lub `(list 1 2 3)` to **dane w postaci S-wyrażeń**. Ich elementy nie
muszą mieć wspólnego typu.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Dane w postaci S-wyrażeń służą głównie do obsługi samych programów, w makrach ([Makra](macros.md)) i
z `read`. Dla danych, których elementy mają znany typ, użyj `Vector<T>`. Listę S-wyrażeń można
przejść za pomocą `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Parę dwóch wartości tworzy się za pomocą `cons`, a rozbiera za pomocą `car` i `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Pisanie programu w pliku

Program można zapisać w pliku (z rozszerzeniem `.typl`) i uruchomić poleceniem `typl nazwa-pliku`.
Do pokazania wyników użyj `println`.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` wypisuje, używając tych samych dyrektyw co `format`, i kończy znakiem nowej linii. `print` nie
  dodaje nowej linii.
- `~a` osadza wartość w postaci czytelnej dla człowieka, a `~s` w postaci, którą można wczytać z powrotem
  (łańcuchy znaków dostają swoje `"`).
- Plik jest czytany od góry do dołu. **Funkcji nie można wywołać przed jej definicją.**

## 9. Co czytać dalej

- [Podstawy typów](types.md): `Option`, `Result`, struktury, enumeracje, typy generyczne
- [Dla programistów Common Lisp](../guide/from-common-lisp.md): lista różnic dla osób, które
  znają Common Lisp
