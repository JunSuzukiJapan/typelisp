<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Обработка ошибок

Обработка ошибок в typelisp делит неудачи на два вида.

| Вид неудачи | Примеры | Как выражается |
|---|---|---|
| Неудачи, которые могут случиться (восстановимые) | Нет файла, ввод — не число | Вернуть `Result<T,E>` |
| Ошибки в программе (невосстановимые) | Индекс вне диапазона, `unwrap` от `none`, деление на ноль | Остановиться через `panic` |

Помимо этого есть `catch` / `throw`, которые выходят сразу из многих вызовов функций, и `unwind-protect`, который
выполняет очистку, как бы ни было покинуто тело. Предполагается, что вы прочитали раздел о `Result` в
[Основах типов](types.md).

## 1. Вернуть `Result` и принять его через `match`

Вот функция, читающая номер порта из строки. Она может не удаться двумя способами: ввод не число или выходит за
диапазон.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Вызывающий разделяет успех и неудачу с помощью `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Значение функции, возвращающей `Result`, нельзя использовать, пока `match` не обработает случай `err`. Забытая
  обработка неудачи — ошибка типа.
- Ошибка от `parse-int` — значение типа `ParseIntError`. `(message e)` даёт строку сообщения.

## 2. Передача неудачи вызывающему

Сокращения вроде `?` из Rust нет. Когда по очереди вызываются несколько функций, возвращающих `Result`, часть
«вернуть неудачу как есть» пишется через `match`.

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

Когда известно, что операция не может не удаться, или в маленьком скрипте, где остановка при неудаче допустима,
`unwrap` извлекает содержимое. Если значение — `err`, происходит panic. Если подходит значение по умолчанию,
используйте `unwrap-or`.

## 3. Собственный тип ошибки

Если выражать ошибки типом, а не строкой, вызывающий сможет ветвиться по виду ошибки. Тип ошибки — обычный `defenum`
или `defstruct`, реализующий трейт `Error`.

```lisp
(defenum config-error
  (missing string)          ; не хватает настройки
  (invalid string int))     ; неверное значение

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

- `message` возвращает описание ошибки.
- `source` возвращает другую ошибку, ставшую причиной этой. Без причины — `none`.

## 4. Объединение разных видов ошибок

Если одна функция вызывает и `parse-int` (`ParseIntError`), и `check-workers` (`config-error`), получаются два типа
ошибок, и оба они не могут быть `E` одного `Result<T,E>`. В этом случае сделайте `E` типом `:dyn Error` (ошибка
любого типа, реализующего `Error`). Каждую ошибку преобразуйте через `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Для `"4"`, `"-1"` и `"abc"` результаты таковы:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

О `:dyn` см. раздел 5 главы [Трейты](traits.md).

## 5. `panic`: ошибки в программе

Когда программа доходит до состояния, которого никогда не должно быть, остановите её через `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Тип `panic` — `!` (он не возвращает управление), поэтому его можно писать везде, где ожидается любой тип. Вот
  почему две ветви `if` выше совместимы.
- Эти операции тоже вызывают panic: `unwrap` от `none` или `err`, `get` с индексом вне диапазона и целочисленное
  деление на ноль.
- `panic` останавливает программу. Даже если он случается внутри задачи, останавливается вся программа.
- В REPL `panic` не завершает REPL; она ждёт следующего ввода.
- Можно писать `(todo)` для «ещё не написано» и `(unreachable)` для «сюда никогда не должно дойти». Оба вызывают
  panic.

`panic` не заменяет `Result`. Для неудач, которые могут случиться, например при вводе пользователя или проверке
существования файла, используйте `Result`.

## 6. `catch` / `throw`: выход сквозь функции

`throw` сразу выпрыгивает к объемлющему `catch` с той же меткой, сколько бы вызовов функций ни было между ними.

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

Если в `v` нет отрицательных чисел, `validate` возвращает `"all fine"`; если там есть `-7`, управление выпрыгивает
изнутри `check-all` к `catch`, который возвращает `"negative: -7"`.

- Метку пишите простым символом, например `'bad-input`.
- **Каждая метка переносит значения ровно одного типа.** В примере выше `'bad-input` переносит `string`, поэтому
  бросить `int` с той же меткой — ошибка типа. Тип тела `catch` тоже должен совпадать с типом метки.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw`, у которого нет `catch` с той же меткой, — ошибка.

Если нужен лишь досрочный выход из функции, используйте `return-from` вместо `catch` / `throw`. `return-from` не может
пересекать функции, зато по исходному тексту видно, куда он возвращается.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: очистка всегда

`(unwind-protect body cleanup)` выполняет очистку, как бы ни было покинуто тело: при нормальном завершении, при
выходе через `throw` и при panic.

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

Используйте его для вещей вроде «всегда закрыть открытый файл» или «всегда освободить взятую блокировку».
`with-open-file` и `with-lock` из стандартной библиотеки используют `unwind-protect` внутри.

## 8. О системе условий Common Lisp

typelisp не перенимает систему условий Common Lisp (`handler-case`, `restart-case` и т. д.). Она не показывает в типах,
какие неудачи может вызвать функция, что плохо сочетается со статической типизацией. Неудачи, которые могут
случиться, записываются в типах через `Result`, а передача управления делается через `catch` / `throw`.

## 9. Что читать дальше

- [Конкурентность](concurrency.md): задачи и каналы
- [Option, Result и типы ошибок](../reference/functions/option-result.md): список функций
- [Сообщения об ошибках](../reference/errors.md): что означают распространённые ошибки и как их исправить
