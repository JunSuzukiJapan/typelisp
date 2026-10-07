<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Обробка помилок

Обробка помилок у typelisp ділить збої на два види.

| Вид збою | Приклади | Як виражається |
|---|---|---|
| Збої, що можуть статися (відновлювані) | Файлу немає, ввід не є числом | Повернути `Result<T,E>` |
| Помилки в програмі (невідновлювані) | Індекс поза межами, `unwrap` для `none`, ділення на нуль | Зупинитися через `panic` |

Крім цього, є `catch` / `throw`, що залишають одразу багато викликів функцій, і `unwind-protect`, що
виконує очищення, як би не залишили його тіло. Цей розділ припускає, що ви прочитали розділ про `Result`
в [Основи типів](types.md).

## 1. Повернути `Result` і прийняти його через `match`

Ось функція, що читає номер порту з рядка. Вона може зазнати невдачі двома способами: ввід не є числом
або виходить за межі діапазону.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Викликач відокремлює успіх від невдачі через `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Значення функції, що повертає `Result`, не можна використати, якщо `match` не обробляє випадок `err`.
  Забути обробити невдачу — це помилка типів.
- Помилка від `parse-int` — це значення типу `ParseIntError`. `(message e)` дає її рядок повідомлення.

## 2. Передача невдачі вгору викликачеві

Скорочення на кшталт `?` з Rust немає. Коли по черзі викликаєте кілька функцій, що повертають `Result`,
частину «повернути невдачу як є» записуйте через `match`.

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

Коли ви знаєте, що операція не може зазнати невдачі, або в невеликому скрипті, де зупинка при невдачі
припустима, `unwrap` виймає вміст. Якщо значення — `err`, він завершується через panic. Якщо
достатньо типового значення, використовуйте `unwrap-or`.

## 3. Створення власного типу помилки

Вираження помилок типом, а не рядком, дає викликачеві змогу розгалужуватися за видом помилки. Тип
помилки — це звичайний `defenum` або `defstruct`, що реалізує трейт `Error`.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

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

- `message` повертає опис помилки.
- `source` повертає іншу помилку, яка спричинила цю. Якщо причини немає, це `none`.

## 4. Поєднання різних видів помилок

Якщо одна функція викликає і `parse-int` (`ParseIntError`), і `check-workers` (`config-error`), є два
типи помилок, і обидва не можуть бути `E` одного `Result<T,E>`. У такому разі зробіть `E`
рівним `:dyn Error` (помилка будь-якого типу, що реалізує `Error`). Перетворюйте кожну помилку через
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Для `"4"`, `"-1"` і `"abc"` результати такі:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Про `:dyn` див. розділ 5 в [Трейти](traits.md).

## 5. `panic`: помилки в програмі

Коли програма досягає стану, якого ніколи не повинно бути, зупиніть її через `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Тип `panic` — це `!` (він не повертається), тож його можна писати всюди, де очікується будь-який тип.
  Тому дві гілки наведеного вище `if` узгоджуються.
- Також завершуються через panic такі операції: `unwrap` для `none` або `err`, `get` з індексом поза
  межами та цілочисельне ділення на нуль.
- `panic` зупиняє програму. Навіть коли це відбувається всередині задачі, зупиняється вся програма.
- У REPL `panic` не завершує REPL; він чекає наступного вводу.
- Можна писати `(todo)` для «ще не написано» і `(unreachable)` для «до цієї точки ніколи не мало б
  дійти». Обидва завершуються через panic.

`panic` не замінює `Result`. Для збоїв, що можуть статися, як-от ввід користувача чи наявність файлу,
використовуйте `Result`.

## 6. `catch` / `throw`: вихід через кілька функцій

`throw` стрибає просто до охоплювального `catch` з тим самим тегом, скільки б викликів функцій не
лежало між ними.

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

Якщо в `v` немає від'ємних чисел, `validate` повертає `"all fine"`; якщо він містить `-7`, керування
стрибає з-усередини `check-all` до `catch`, який повертає `"negative: -7"`.

- Пишіть тег як простий символ, як-от `'bad-input`.
- **Кожен тег несе значення рівно одного типу.** У наведеному прикладі `'bad-input` несе `string`, тож
  кинути `int` із тим самим тегом — це помилка типів. Тип тіла `catch` також має збігатися з типом
  тега.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw`, для якого немає `catch` з тим самим тегом, — це помилка.

Якщо ви хочете лише достроково вийти з функції, використовуйте `return-from` замість `catch` /
`throw`. `return-from` не може перетинати функції, але натомість куди він повертається, видно з
читання коду.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: завжди очищати

`(unwind-protect body cleanup)` виконує очищення, як би не залишили тіло: коли воно завершується
нормально, коли його залишають через `throw` і коли воно завершується через panic.

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

Використовуйте його для таких речей, як «завжди закривати відкритий файл» або «завжди звільняти взяте
блокування». `with-open-file` і `with-lock` зі стандартної бібліотеки всередині використовують
`unwind-protect`.

## 8. Про систему умов Common Lisp

typelisp не приймає систему умов Common Lisp (`handler-case`, `restart-case` тощо). Вона не показує в
типах, які збої може спричинити функція, що погано поєднується зі статичною типізацією. Збої, що можуть
статися, записуються в типах через `Result`, а передачі керування виконуються через `catch` / `throw`.

## 9. Що читати далі

- [Конкурентність](concurrency.md): задачі та канали
- [Option, Result і типи помилок](../reference/functions/option-result.md): перелік функцій
- [Повідомлення про помилки](../reference/errors.md): що означають поширені помилки і як їх виправити
