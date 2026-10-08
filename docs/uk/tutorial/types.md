<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Основи типів

typelisp — це мова зі статичною типізацією. Цей розділ пояснює, що для вас робить перевіряльник типів,
які типи ви використовуватимете найчастіше (`Option`, `Result`, структури та переліки) і що таке
узагальнені типи. Він припускає, що ви прочитали [Перші кроки](intro.md).

## 1. Що означає статична типізація

У typelisp тип кожного виразу визначається до запуску програми. Вираз із невідповідними типами —
це помилка ще до того, як щось виконається.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Запуск цього файлу зупиняється з помилкою типів, навіть не надрукувавши `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Типи потрібно писати для аргументів і результатів функцій, глобальних змінних і полів структур. Тип
змінної `let` береться з її початкового значення.

Основні типи:

| Тип | Приклади значень |
|---|---|
| `int` | `42`, `-7` (цілі числа довільної точності) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Цілі числа фіксованої ширини |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Тип результату функції, що не повертає значення |

Спитати про тип значення під час виконання не можна (немає `typep` чи `type-of` з Common Lisp), бо
кожен тип відомий ще до запуску програми.

## 2. `Option<T>`: значення, якого може не бути

У typelisp немає `nil`. «Значення може бути відсутнє» виражається типом `Option<T>`. Значення
`Option<T>` — це або `some`, що містить одне значення типу `T`, або `none`, що не містить нічого.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` — це не `int`, тож у арифметиці його використати як є не можна. `(+ (safe-div 10 2) 1)` —
це помилка типів. Щоб скористатися вмістом, розділіть `some` і `none` через `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- У гілці `(some q)` вміст зв'язується зі змінною `q`.
- `match` перевіряє, що його гілки **охоплюють усі випадки**. Забута гілка `(none)` — це помилка типів.

### Чому немає nil

У багатьох мовах `nil` (`null`) може підставлятися замість значення будь-якого типу. Через це забутий
випадок «значення немає» лишається непоміченим, доки програма не запуститься. У typelisp місце, де
значення може бути відсутнє, має тип `Option<T>`, і код не проходить перевірку типів, поки `match` не
обробить випадок `none`. Забутий випадок знаходиться до запуску програми.

Умови підкоряються тій самій ідеї: умовою `if` може бути лише `bool`. Правила на кшталт «усе, крім
`nil`, — істина» з Common Lisp немає.

### Поширені операції

| Форма | Значення |
|---|---|
| `(unwrap-or opt default)` | Вміст для `some`; типове значення для `none` |
| `(unwrap opt)` | Виймає вміст. Зупиняє програму на `none` |
| `(is-some opt)` / `(is-none opt)` | Перевіряє, який це випадок |

Багато функцій стандартної бібліотеки повертають `Option`. Наприклад, `position` повертає позицію в
`some`, якщо елемент знайдено, і `none`, якщо ні.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: операція, яка може завершитися невдачею

Операція, що може зазнати невдачі, повертає `Result<T,E>`: `ok` зі значенням типу `T` у разі успіху або
`err` з помилкою `E` у разі невдачі.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Ваші власні функції теж можуть повертати `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Використовуйте `Option`, коли відсутність значення не потребує пояснення, і `Result`, коли хочете
сказати, чому щось не вдалося. Докладніше про обробку помилок — у розділі [Обробка помилок](errors.md).

## 4. `defstruct`: структури

Тип з іменованими полями визначається через `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

Визначення дає вам таке:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Щоб надати структурі власні функції, використовуйте `defmethod`. Тип першого аргументу (`self`)
визначає, якому типові належить метод.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Якщо замість аргументу `self` написати лише назву типу, вийде функція, що викликається як
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: одна з кількох форм

Значення, що є однією з кількох форм, як-от «коло, прямокутник або точка», визначається через
`defenum`. Кожна форма називається **варіантом**. Кожен варіант може містити різну кількість і різні
типи значень.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

Значення створюються з назвою типу попереду, як-от `shape::circle`. У `match` їх розбирають за назвою
варіанта.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

І тут `match` перевіряє, що всі випадки охоплено. Якщо згодом додати варіант до `shape`, кожен `match`,
що його не обробляє, стане помилкою типів, тож жодне місце, яке потребує виправлення, не буде
пропущене.

Після `(use shape)` можна писати `(rect 5 6)` без назви типу.

`Option` і `Result` — це переліки, побудовані тим самим механізмом.

## 6. Узагальнені типи

Функція, що працює для будь-якого типу, визначається з **параметром типу** `<T>` після назви.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Під час виклику тип не задають. `T` виводиться з аргументів.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Структури та переліки теж можуть бути узагальненими. `Vector<T>`, `Option<T>` і `Result<T,E>` — це типи
такого роду.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Усередині узагальненої функції про `T` нічого не відомо, тож порівнювати чи додавати значення типу `T`
не можна. Щоб вимагати щось на кшталт «будь-який тип, який можна порівнювати», використовуйте трейти
([Трейти](traits.md)).

## 7. Інша назва для типу

`deftype` дає типові іншу назву.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` — це лише інший запис `int`, а не новий тип. Передати звичайний `int` там, де очікується
`meters`, не помилка. Якщо ви хочете їх розрізняти, створіть структуру, як-от
`(defstruct meters (value int))`.

## 8. Що читати далі

- [Трейти](traits.md): як дати типам спільні операції
- [Типи](../reference/types.md): вбудовані типи та трейти, які реалізує кожен із них
