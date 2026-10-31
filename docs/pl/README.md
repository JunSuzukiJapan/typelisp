<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# Dokumentacja typelisp (Polski)

typelisp to statycznie typowany Lisp. Instalacja i budowanie są opisane w pliku [README.md](../../README.md)
(po angielsku) w katalogu głównym repozytorium.

## Cechy

### Zadania i kanały

Uruchamiasz zadania (lekkie wątki), które przekazują sobie wartości przez kanały. Szczegóły w
rozdziale [Współbieżność](tutorial/concurrency.md).

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Typy generyczne

Parametry typów zapisuje się w nawiasach ostrych po nazwie typu lub funkcji. Ograniczenia traitów
umieszcza się w klauzuli `:where`. Szczegóły w rozdziałach [Podstawy typów](tutorial/types.md) i
[Traity](tutorial/traits.md).

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option i Result zamiast nil

Nie ma `nil`. Funkcja, która może nie mieć wartości, zwraca `Option<T>`, a funkcja, która może się
nie powieść, zwraca `Result<T,E>`; zawartość wyjmuje się za pomocą `match`. Użycie `Option<string>`
jako `string` albo pominięcie przypadku `none` w `match` jest błędem typów jeszcze przed
uruchomieniem programu. Szczegóły w rozdziałach [Podstawy typów](tutorial/types.md) i [Obsługa
błędów](tutorial/errors.md).

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Łączenie w łańcuch za pomocą `->`

Makro `->` przekazuje wartość kolejno jako pierwszy argument każdej następnej formy.
`(-> x (f a) (g b))` to to samo co `(g (f x a) b)`. Kroki zwracające `Option` lub `Result` można
tak zapisać od góry do dołu. Szczegóły w rozdziale
[Option, Result i typy błędów](reference/functions/option-result.md).

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### Makra sprawdzane pod względem typów

Makra pisze się tak jak w Common Lisp, za pomocą `defmacro`, quasiquote i `gensym`. Kod po
rozwinięciu jest sprawdzany tak samo jak kod pisany ręcznie, a jeśli nie pasuje do typów, błąd jest
zgłaszany w miejscu wywołania makra. Szczegóły w rozdziale [Makra](tutorial/macros.md).

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Samouczek

Jeśli dopiero zaczynasz z typelisp, przeczytaj te dokumenty po kolei.

- [Pierwsze kroki](tutorial/intro.md): REPL, funkcje, zmienne, instrukcje warunkowe, pętle, listy i `Vector`
- [Podstawy typów](tutorial/types.md): typy statyczne, `Option`, `Result`, struktury, enumeracje, typy generyczne
- [Traity](tutorial/traits.md): `deftrait` / `impl`, ograniczenia traitów, `:dyn`
- [Makra](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Obsługa błędów](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Współbieżność](tutorial/concurrency.md): zadania, kanały, `select`, `Mutex`, `thread`

## Przewodniki

- [Moduły i układ plików](guide/modules.md): `use`, `pub`, jak pliki odpowiadają modułom
- [Kompilacja](guide/compile.md): JIT, budowanie plików wykonywalnych za pomocą kompilacji AOT, zrzuty (dumps)
- [Operacje na plikach, strumienie i sieć](guide/io.md): pliki, nazwy ścieżek, TCP / TLS / UDP, rozwiązywanie nazw
- [FFI do C](guide/ffi.md): wywoływanie funkcji C za pomocą `defffi` (w tym wywołania zwrotne i struktury C z `def-c-struct`)
- [Integracja z edytorami](guide/editors.md): `typl-lsp` oraz konfiguracja VS Code / Emacsa
- [Dla programistów Common Lisp](guide/from-common-lisp.md): czym typelisp różni się od CL i jak przepisać kod CL

## Referencja

- [Referencja składni](reference/syntax.md): składnia leksykalna, zapis typów, definicje, formy sterujące, kompilacja, współbieżność
- [Funkcje wbudowane](reference/functions/README.md): funkcje wbudowane, metody i biblioteka standardowa
- [Typy](reference/types.md): typy oraz traity, które każdy z nich implementuje
- [Komunikaty o błędach](reference/errors.md): co oznaczają typowe błędy i jak je naprawić

## Integracja z edytorami

Instrukcje konfiguracji znajdują się w [przewodniku po integracji z edytorami](guide/editors.md). Skróty klawiszowe
i ustawienia poszczególnych edytorów są opisane w tych dokumentach:

- [Emacs (typelisp-mode)](../../editor/emacs/README_pl.md)
- [VS Code](../../editor/vscode/README_pl.md)
