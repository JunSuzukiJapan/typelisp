<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# Документація typelisp (українська)

typelisp — це Лісп зі статичною типізацією. Про встановлення та збирання див.
[README.md](../../README.md) (англійською) у корені репозиторію.

## Особливості

### Задачі та канали

Запускайте задачі (легкі потоки) і передавайте між ними значення через канали. Докладніше — у
розділі [Конкурентність](tutorial/concurrency.md).

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

### Узагальнені типи

Параметри типів записують у кутових дужках після імені типу або функції. Обмеження трейтами
записують у реченні `:where`. Докладніше — у розділах [Основи типів](tutorial/types.md) і
[Трейти](tutorial/traits.md).

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

### Option і Result замість nil

`nil` немає. Функція, яка може не мати значення, повертає `Option<T>`, а функція, яка може
завершитися невдачею, — `Result<T,E>`; вміст дістають за допомогою `match`. Якщо використати
`Option<string>` як `string` або пропустити випадок `none` у `match`, буде помилка типів ще до
запуску програми. Докладніше — у розділах [Основи типів](tutorial/types.md) і [Обробка
помилок](tutorial/errors.md).

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

### Ланцюжки з `->`

Макрос `->` передає значення по черзі першим аргументом у кожну наступну форму.
`(-> x (f a) (g b))` — те саме, що `(g (f x a) b)`. Кроки, що повертають `Option` або `Result`,
так записують згори донизу. Докладніше — у розділі
[Option, Result і типи помилок](reference/functions/option-result.md).

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

### Макроси з перевіркою типів

Макроси пишуть так само, як у Common Lisp: з `defmacro`, квазіцитуванням і `gensym`. Розгорнутий код
перевіряється так само, як написаний вручну, і якщо він не відповідає типам, помилку вказують у
місці виклику макросу. Докладніше — у розділі [Макроси](tutorial/macros.md).

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

## Підручник

Якщо ви вперше знайомитеся з typelisp, читайте в такому порядку.

- [Перші кроки](tutorial/intro.md): REPL, функції, змінні, умови, цикли, списки та `Vector`
- [Основи типів](tutorial/types.md): статичні типи, `Option`, `Result`, структури, переліки, узагальнені типи
- [Трейти](tutorial/traits.md): `deftrait` / `impl`, обмеження трейтами, `:dyn`
- [Макроси](tutorial/macros.md): `defmacro`, квазіцитування, `gensym`, `macrolet`
- [Обробка помилок](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Конкурентність](tutorial/concurrency.md): задачі, канали, `select`, `Mutex`, `thread`

## Посібники

- [Модулі та розкладання по файлах](guide/modules.md): `use`, `pub`, відповідність файлів і модулів
- [Компіляція](guide/compile.md): JIT, збирання виконуваних файлів за допомогою AOT-компіляції, дампи
- [Файловий ввід-вивід, потоки та мережа](guide/io.md): файли, шляхи, TCP / TLS / UDP, розв'язання імен
- [FFI для C](guide/ffi.md): виклик функцій C через `defffi` (разом зі зворотними викликами та структурами C через `def-c-struct`)
- [Інтеграція з редакторами](guide/editors.md): `typl-lsp` і налаштування VS Code / Emacs
- [Для програмістів на Common Lisp](guide/from-common-lisp.md): чим typelisp відрізняється від CL і як переписувати код на CL

## Довідник

- [Довідник із синтаксису](reference/syntax.md): лексика, запис типів, визначення, керівні форми, компіляція, конкурентність
- [Вбудовані функції](reference/functions/README.md): вбудовані функції, методи та стандартна бібліотека
- [Типи](reference/types.md): типи та трейти, які реалізує кожен із них
- [Повідомлення про помилки](reference/errors.md): що означають поширені помилки і як їх виправити

## Інтеграція з редакторами

Порядок налаштування описано в [посібнику з інтеграції з редакторами](guide/editors.md). Прив'язки клавіш і
налаштування кожного редактора перелічено в цих документах:

- [Emacs (typelisp-mode)](../../editor/emacs/README_uk.md)
- [VS Code](../../editor/vscode/README_uk.md)
