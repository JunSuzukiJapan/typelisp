<!-- translated-from: docs/ja/README.md @ 7d830ae3902dd3c581c7cf6fa5365b91b34106da -->
# Документация typelisp (русский)

typelisp — это Лисп со статической типизацией. Об установке и сборке см.
[README.md](../../README.md) (на английском) в корне репозитория.

## Особенности

### Задачи и каналы

Запускайте задачи (лёгкие потоки) и передавайте между ними значения через каналы. Подробнее — в
разделе [Конкурентность](tutorial/concurrency.md).

```lisp
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

### Option и Result вместо nil

`nil` нет. Функция, у которой может не быть значения, возвращает `Option<T>`, а функция, которая
может завершиться неудачей, — `Result<T,E>`; содержимое извлекается с помощью `match`. Если
использовать `Option<string>` как `string` или пропустить случай `none` в `match`, будет ошибка
типов ещё до запуска программы. Подробнее — в разделах [Основы типов](tutorial/types.md) и
[Обработка ошибок](tutorial/errors.md).

```lisp
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (else (Option::none))))

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

### Макросы с проверкой типов

Макросы пишутся так же, как в Common Lisp: с `defmacro`, квазицитированием и `gensym`. Развёрнутый
код проверяется так же, как написанный вручную, и если он не соответствует типам, ошибка указывается
в месте вызова макроса. Подробнее — в разделе [Макросы](tutorial/macros.md).

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

## Учебник

Если вы впервые знакомитесь с typelisp, читайте в таком порядке.

- [Первые шаги](tutorial/intro.md): REPL, функции, переменные, ветвления, циклы, списки и `Vector`
- [Основы типов](tutorial/types.md): статические типы, `Option`, `Result`, структуры, перечисления, обобщённые типы
- [Трейты](tutorial/traits.md): `deftrait` / `impl`, ограничения трейтами, `:dyn`
- [Макросы](tutorial/macros.md): `defmacro`, квазицитирование, `gensym`, `macrolet`
- [Обработка ошибок](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Конкурентность](tutorial/concurrency.md): задачи, каналы, `select`, `Mutex`, `thread`

## Руководства

- [Модули и разбиение на файлы](guide/modules.md): `use`, `pub`, соответствие файлов и модулей
- [Компиляция](guide/compile.md): JIT, сборка исполняемых файлов AOT-компиляцией, дампы
- [Файловый ввод-вывод, потоки и сеть](guide/io.md): файлы, пути, TCP / TLS / UDP, разрешение имён
- [FFI для C](guide/ffi.md): вызов функций C через `defffi` (включая обратные вызовы и структуры C с `def-c-struct`)
- [Интеграция с редакторами](guide/editors.md): `typl-lsp` и настройка VS Code / Emacs
- [Для программистов на Common Lisp](guide/from-common-lisp.md): чем typelisp отличается от CL и как переписывать код на CL

## Справочник

- [Справочник по синтаксису](reference/syntax.md): лексика, запись типов, определения, управляющие формы, компиляция, конкурентность
- [Встроенные функции](reference/functions/README.md): встроенные функции, методы и стандартная библиотека
- [Типы](reference/types.md): типы и трейты, которые реализует каждый из них
- [Сообщения об ошибках](reference/errors.md): что означают распространённые ошибки и как их исправить

## Интеграция с редакторами

Порядок настройки описан в [руководстве по интеграции с редакторами](guide/editors.md). Привязки клавиш и
настройки каждого редактора перечислены в этих документах:

- [Emacs (typelisp-mode)](../../editor/emacs/README_ru.md)
- [VS Code](../../editor/vscode/README_ru.md)
