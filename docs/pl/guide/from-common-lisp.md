<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Dla programistów Common Lisp

typelisp dziedziczy składnię Common Lisp (CL) i wiele jego nazw funkcji, ale jest językiem
typowanym statycznie. Z tego powodu kod CL nie zawsze działa bez zmian. Ten przewodnik
zbiera miejsca, w których osoby przyzwyczajone do CL zwykle się potykają, wraz ze sposobami przepisania kodu.

## 1. Nie ma `nil` ani `t`

Wartościami logicznymi są `true` i `false`. `nil` i `t` nie są zdefiniowane.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Warunkiem może być tylko `bool`.** Zapisanie `0` lub pustej listy jako warunku jest błędem typu.
  Nie ma reguły, że „wszystko poza nil jest prawdą".
- **Gałęzi else w `if` nie można pominąć.** `(if c x)` jest błędem. Gdy gałąź else nie jest
  potrzebna, użyj `when` / `unless`.
- **„Brak wartości" wyraża się za pomocą `Option<T>`.** Funkcja, która w CL zwracała nil na znak „nie
  znaleziono", zwraca tutaj `(some x)` lub `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Pusta lista `()` jest, zależnie od kontekstu, albo wartością typu Unit (wartością zwracaną
  przez funkcję niezwracającą niczego), albo pustą listą danych w postaci S-wyrażeń. Jest to inna wartość
  niż `false`.

## 2. Zapisywanie typów

Argumenty funkcji i wartości zwracane muszą mieć typy.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; funkcja generyczna
  (unwrap-or (first (iter v)) default))
```

- Definicji bez typów, takiej jak `(defun f (x) x)`, nie można napisać.
- Zmienne globalne, takie jak `defvar`, też potrzebują typu: `(defvar (count int) 0)`.
- `the` nie jest sprawdzeniem w czasie działania, lecz adnotacją dla modułu sprawdzającego typy.
- **Nie ma sposobu na badanie typów w czasie działania.** Nie ma `typep` ani `type-of`, ponieważ
  typ każdej wartości jest ustalony w czasie kompilacji. Aby przyjmować jeden z kilku typów, zrób typ sumy za pomocą
  `defenum` lub użyj traitu.
- `deftype` definiuje alias typu. Typu opisującego zakres wartości, takiego jak
  `(deftype small () '(integer 0 9))`, nie można utworzyć.

Domyślny typ całkowity `int` ma dowolną precyzję; podobnie jak integer w CL, nie ma górnej granicy jego
wielkości. Istnieją także typy o stałej szerokości od `i8` do `i32` oraz od `u8` do `u32`. Nie ma 64-bitowego
typu całkowitego o stałej szerokości.

## 3. Funkcje jako wartości

typelisp nie rozdziela przestrzeni nazw funkcji i zmiennych. Nazwę funkcji można przekazać
jako wartość bez zmian. Nie ma `#'` ani `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; wywołaj bezpośrednio, nie przez funcall

(apply-to twice 5)                        ; twice, a nie #'twice
```

- Funkcje wbudowane, takie jak `+` i `1+`, też można przekazywać jako wartości bez zmian, tam gdzie typ
  argumentu jest ustalony, jak w `(fn (int) int)`. Przy przekazywaniu do funkcji generycznej, takiej jak
  `foldl` lub `map`, nie wiadomo, o `+` którego typu chodzi, więc opakuj je w `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Funkcje na sekwencjach przyjmują **kolekcję jako pierwszy argument, a funkcję jako drugi**: `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. Jest to odwrotność `(mapcar f list)` z CL.
- `lambda` nie może używać `&optional` ani `&key` (można używać `&rest`).
- **Funkcji nie można wywołać przed jej zdefiniowaniem.** W CL możesz wywołać funkcję definiowaną
  później, ale tutaj daje to `no such function`. Dla funkcji wzajemnie rekurencyjnych zadeklaruj najpierw jedną
  z nich za pomocą `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listy i Vector

Odpowiednikiem listy z CL są **dane w postaci S-wyrażeń**, których typem jest `Option<Sexpr>` (pusta lista
to `none`). `(list 1 2 3)` i `'(a b c)` mają ten typ. Dane w postaci S-wyrażeń to coś, z czym pracują makra i
`read`; jako zwykły kontener danych użyj **`Vector<T>`**.

| Czego chcesz | CL | typelisp |
|---|---|---|
| Głowa i reszta S-wyrażenia | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Przejście po liście S-wyrażeń | `(dolist (x xs) ...)` | To samo |
| Ciąg elementów jednego typu | Lista lub wektor | `Vector<T>` |
| Para | `(cons a b)` | `(cons a b)` (jej typem jest `cons-cell<A,B>`) |
| Odwzorowanie | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` to akcesory pary `cons-cell<A,B>` tworzonej za pomocą `cons`. Nie można ich używać na
listach S-wyrażeń.

`Vector` zapisuje się za pomocą `#(..)`, tak jak wektor w CL, i jest też drukowany jako `#(..)`:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

Są trzy różnice względem CL. Wszystkie elementy muszą mieć ten sam typ (`#(1 "a")` jest błędem
typu). Każde obliczenie tworzy nowy wektor, więc jego zmiana nie wpływa na następne obliczenie (w CL
wynik modyfikacji literału jest niezdefiniowany). Pusty `#()` wymaga podania typu, jak w
`(the Vector<int> #())`. Tablicę wielowymiarową zapisuje się jako `#2A((1 2) (3 4))`, tak jak w CL.

Funkcje na sekwencjach, takie jak `map`, `filter`, `sort` i `find`, działają na wartościach implementujących
trait `Iter`. Przekaż `Vector` po zamianie go na iterator za pomocą `(iter v)`.

## 5. Nie ma wielokrotnych wartości

Nie ma `values` ani `multiple-value-bind`. Funkcje, które w CL zwracają kilka wartości, zwracają
parę lub strukturę.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `cons-cell`, którego `car` to 3, a `cdr` to 1 |
| `(decode-universal-time t)` → 9 wartości | Struktura `decoded-time` |
| `(read-from-string s)` → wartość, pozycja | `(read-from-string s)` zwraca `cons-cell` z wartością i pozycją wewnątrz `Result`. Dla samej wartości `(read s)` |

## 6. Nie ma zmiennych specjalnych (wiązania dynamicznego)

`let` zawsze wiąże leksykalnie. Jeśli przewiążesz zmienną zdefiniowaną za pomocą `defvar` przy użyciu `let`,
funkcje wywoływane stamtąd nadal widzą pierwotną wartość.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 w CL, 1 w typelisp
```

Aby tymczasowo zmienić zmienną sterującą, taką jak `*print-base*`, użyj `dlet`. Przypisuje ono wartość
i przywraca pierwotną bez względu na to, jak ciało zostanie opuszczone.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` przepisuje samą zmienną globalną, więc nie jest to wiązanie na poziomie wątku.

## 7. System kondycji nie jest przyjęty

Nie ma `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` ani `signal`.
Słabo pasują do typowania statycznego. Zamiast tego te dwa mechanizmy służą różnym celom:

- **Niepowodzenia odwracalne zwracają `Result<T,E>`.** Wywołujący rozdziela `ok` / `err` za pomocą `match`.
  Nie ma skrótu w rodzaju `?` z Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Niepowodzenia nieodwracalne (błędy) to `panic`.** `(panic "message")`, przekazanie `none` do `unwrap`,
  dzielenie przez 0 i indeks poza zakresem należą do tego rodzaju, a program się zatrzymuje. Kod sprzątający
  `unwind-protect` wykonuje się przed zatrzymaniem.

Typy błędów są ujednolicone przez trait `Error`, a `(message e)` daje komunikat. Jak zrobić własny
typ błędu, opisano w
[Option, Result i typy błędów](../reference/functions/option-result.md#3-typy-błędów-i-trait-error).
`assert` i `warn` można używać tak jak w CL.

`catch` / `throw` / `unwind-protect` istnieją. Jednak znacznik `catch` jest ograniczony do niewartościowanego
literału symbolu (`'done`), a wartości rzucane z jednym znacznikiem mają jeden typ.

## 8. Nie ma CLOS

Nie ma `defclass`, `defgeneric` ani kombinacji metod.

- Typy danych definiuje się za pomocą `defstruct` (struktury) i `defenum` (typy sum).
- `defmethod` definiuje metody, których cel jest wybierany wyłącznie według **statycznego typu pierwszego argumentu**.
  Nie ma wielokrotnej dyspozycji.
- Aby nadać różnym typom wspólne operacje, użyj traitów (`deftrait` / `impl`). Dla wartości, których
  konkretny typ jest ustalany w czasie działania, użyj typu `:dyn Trait`
  ([Referencja składni 3.9](../reference/syntax.md#39-deftrait--impl--traity)).

Czym różni się `defstruct`:

- Konstruktorem jest `TypeName::new`: `(point::new 1 2)`. Jeśli chcesz nazwy w rodzaju `make-point`,
  opcja `(:constructor make-point)` ją tworzy.
- Poza `(x p)` akcesor można zapisać jako `p::x`. Zmienia się go za pomocą `(setf p::x 5)`.
- Nie jest tworzony żaden predykat (`point-p`). Nie ma `:conc-name`, `:type` ani `:named`.
- `:include` jedynie dziedziczy sloty; typ nie staje się podtypem rodzica.

## 9. Moduły zamiast pakietów

Nie ma pakietów. Przestrzeniami nazw są moduły, a plik sam jest modułem. Zamiast
`pkg:symbol` pisz `module::name`, a nazwy wprowadzaj za pomocą `use`
([Moduły i układ plików](modules.md)).

Słowa kluczowe `:foo` istnieją i są symbolami, które obliczają się do samych siebie. Ponieważ nie ma pakietów,
dwukropek jest częścią nazwy: `(symbol->string :foo)` zwraca `":foo"`.

## 10. Różnice w czytaniu i składni

- Wielkie i małe litery nie są rozróżniane (symbole po wczytaniu stają się małymi literami). Jest to to samo,
  co w CL.
- Nie ma `#'` (sekcja 3). Literałów liczb zespolonych `#c(...)` nie można wczytać; liczby zespolone
  tworzy się za pomocą `(complex 1.0 2.0)`.
- Klauzule rozszerzonego `loop` zapisuje się za pomocą słów kluczowych: `(loop :for i :from 1 :to 3 :collect i)`.
  `loop`, który nie zaczyna się od słowa kluczowego, to prosta pętla nieskończona, opuszczana za pomocą `(break)` lub
  `(return value)`. `return` opuszcza najbardziej wewnętrzną pętlę (aby opuścić funkcję, użyj `return-from`).
- Celem `format` jest `false` (zwróć łańcuch znaków), `true` (standardowe wyjście) lub strumień.
  Dyrektywy formatu są takie same jak w CL.
- Wczytywanie z łańcucha znaków to `(read "...")`, a ze strumienia `(read-sexpr s)`. Oba zwracają
  `Result`.
- `eval` sprawdza typy podanego wyrażenia przed jego obliczeniem i zwraca `Result`. Odwołania wyprzedzające
  nie są możliwe, tak jak w kodzie źródłowym.
- Nie ma `eval-when`.
- Nazwy funkcji nie używają przyrostków `?` ani `!`. Predykaty nazywa się z `-p` / `p` jak w CL
  (`zerop`, `sexpr-null`) lub z `is-` na początku (`is-some`).

## 11. Główne funkcje o innych nazwach

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (ze strumienia) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Dwuargumentowe wersje `floor` i pokrewnych | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (odwrócona kolejność argumentów; sekcja 4) |
| `length` (wektora) | `len` |
| `hash-table-count` | `count` / `size` |

Lista funkcji znajduje się w [Funkcjach wbudowanych](../reference/functions/README.md).

## 12. Inne rzeczy, które nie istnieją

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` oraz `copy-readtable`, `readtable-case` (same makra czytnika można definiować za pomocą
  `set-macro-character`)
- Logiczne nazwy ścieżek i nazwy ścieżek z wieloznacznikami (wildcard)
- `input-stream-p` / `output-stream-p` (kierunek strumienia jest określany przez jego typ)
