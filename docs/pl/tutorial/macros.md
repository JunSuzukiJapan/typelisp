<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Makra

Makro to funkcja, która przyjmuje program i zwraca program. Makra pozwalają tworzyć nową składnię,
której funkcje nie potrafią wyrazić. Makra typelisp działają tak samo jak `defmacro` w Common Lisp. Ten
rozdział zakłada, że przeczytałeś sekcję „Listy (S-wyrażenia)" w [Pierwszych krokach](intro.md).

## 1. Czym makra różnią się od funkcji

Funkcja otrzymuje swoje argumenty **po ich obliczeniu**. Makro otrzymuje je **jako
wyrażenia, przed obliczeniem** (jako dane w postaci S-wyrażeń), buduje inne wyrażenie i je zwraca.
Zwrócone wyrażenie zastępuje wywołanie makra i dopiero wtedy jest sprawdzane pod względem typów i uruchamiane. To
zastąpienie nazywa się **rozwinięciem**.

Na przykład składni takiej jak `unless` nie da się napisać jako funkcji. Jako funkcja jej ciało
byłoby obliczane w pierwszej kolejności, nawet gdy warunek jest prawdziwy.

## 2. `defmacro` i quasiquote

Zróbmy `my-unless`, które uruchamia swoje ciało tylko wtedy, gdy warunek jest fałszywy.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Argumenty makra nie mają zapisanych typów. Każdy argument to dane w postaci S-wyrażenia.
- `&rest body` odbiera pozostałe argumenty razem, jako jedną listę.
- Wyrażenie zaczynające się od `` ` `` (quasiquote) jest budowane jako dane, dokładnie tak, jak zapisano. Wewnątrz niego:
  - `,test` wstawia w tym miejscu zawartość zmiennej `test`.
  - `,@body` wstawia w tym miejscu elementy listy `body`.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Rozwinięcie można sprawdzić za pomocą `macroexpand-1`. Przy pisaniu makra obejrzenie jego rozwinięcia
jest najszybszą drogą naprzód.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Rozwinięcia są także sprawdzane pod względem typów

Wyrażenie zwracane przez makro jest sprawdzane pod względem typów tak jak każde wyrażenie napisane ręcznie.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Błąd jest zgłaszany w miejscu, w którym wywołano makro.

Reguły, że gałęzi else w `if` nie można pominąć i że obie gałęzie `if` muszą mieć ten sam typ,
obowiązują dla rozwinięć w niezmienionej postaci. Powyższe `my-unless` kończy się
`(progn ,@body ())`, dzięki czemu bez względu na typ ostatniego wyrażenia ciała obie gałęzie
`if` mają typ `()`.

## 4. Kolizje nazw i `gensym`

Prosto napisane makro zamieniające miejscami wartości dwóch zmiennych wygląda tak:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Przez większość czasu działa, ale psuje się, gdy zmienna wywołującego nazywa się akurat `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (nie zamieniono)
```

Rozwinięcie to `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, a `tmp` utworzone przez makro
przesłania `tmp` wywołującego.

Aby tego uniknąć, twórz nazwy zmiennych używanych wewnątrz makra za pomocą `gensym`. `gensym` zwraca
świeży symbol, którego nie da się zapisać nigdzie w programie.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Tak jak w Common Lisp, makra typelisp nie zapobiegają automatycznie kolizjom nazw (są niehigieniczne).
Zapamiętaj: **używaj `gensym` dla wiązań tworzonych przez makro.**

W ten sam sposób można napisać makro powtarzające swoje ciało zadaną liczbę razy:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Rozwijanie na różne sposoby w zależności od argumentów

Ciało makra to zwykły kod typelisp, więc może badać swoje argumenty za pomocą `if` lub `match` i
budować inne rozwinięcie. Argumenty to dane w postaci S-wyrażeń (`Option<Sexpr>`), a pusta
lista to `none`.

Zróbmy `my-and`, które zwraca `true`, jeśli wszystkie jego warunki są prawdziwe.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; brak argumentów
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; tylko jeden
         `(if ,f (my-and ,@more) false)))             ; dwa lub więcej
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` to wzorzec, który bierze głowę listy do `f`, a resztę do `more`.
- `sexpr-null` sprawdza, czy dane w postaci S-wyrażenia są pustą listą.
- Końcowe ramię `_` jest potrzebne, ponieważ dane w postaci S-wyrażeń mają też inne postaci niż listy (liczby,
  łańcuchy znaków i tak dalej), a `match` wymaga pokrycia także ich. Argument `&rest` jest zawsze
  listą, więc to ramię nigdy się faktycznie nie wykonuje.
- Makro może wywoływać samo siebie w swoim rozwinięciu. Rozwijanie powtarza się, aż nie zostanie żadne wywołanie makra.

## 6. Argumenty opcjonalne

`&optional` odbiera argumenty, które można pominąć. Można podać wartości domyślne.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` odbiera argumenty kluczowe
([Referencja składni 3.14](../reference/syntax.md#314-defmacro--definicje-makr)).

## 7. `macrolet`: makra tylko dla jednego miejsca

Makro używane tylko wewnątrz jednego wyrażenia można zdefiniować za pomocą `macrolet`. Nie jest widoczne na zewnątrz.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. O czym pamiętać

- **Makro można wywołać dopiero po jego definicji.** Tak jak w przypadku funkcji, definiuj je blisko początku
  pliku.
- Makro udostępnia się innym modułom za pomocą `(pub defmacro ...)`.
- Duża część składni standardowej, w tym `when`, `unless`, `cond`, `and`, `or` i `dotimes`, jest
  zdefiniowana jako makra. To, co jest w środku, możesz zobaczyć za pomocą `(macroexpand '(when true 1))`.
- Jeśli coś da się napisać jako funkcję, napisz to jako funkcję. Makr nie można przekazywać jako
  wartości, a żeby zrozumieć, co robią, trzeba czytać ich rozwinięcie.

## 9. Co czytać dalej

- [Obsługa błędów](errors.md): `Result`, `panic`, `catch` / `throw`
- [Funkcje makr](../reference/functions/system.md#8-makra): `gensym`, `macroexpand` i inne
