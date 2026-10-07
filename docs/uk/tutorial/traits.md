<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Трейти

Трейт — це обіцянка, що «цей тип підтримує ці операції». Трейти дозволяють кільком типам ділити операції
з однаковою назвою, щоб функцію, яка їх використовує, не доводилося писати окремо для кожного типу.
Вони працюють майже так само, як трейти в Rust. Цей розділ припускає, що ви прочитали
[Основи типів](types.md).

## 1. Визначення та реалізація трейта

Визначимо операції, що повертають площу та назву фігури, як трейт `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` після назви трейта — це список трейтів, які він успадковує (розділ 4). Якщо їх немає, залиште
  порожнім.
- Кожен рядок оголошує метод. `Self` позначає «тип, що реалізує цей трейт».

Щоб реалізувати трейт для типу, напишіть `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Реалізовані методи викликаються так само, як звичайні функції.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Пропуск навіть одного з методів, оголошених трейтом, — це помилка типів на `impl`.

## 2. Обмеження трейтами: «будь-який тип, що реалізує цей трейт»

Параметр типу узагальненої функції можна обмежити умовою через `where`. Це називається
**обмеженням трейтом**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Завдяки `(where (Shape T))` тіло може застосовувати `name` і `area` до значень типу `T`. Без обмеження
про `T` нічого не відомо, тож їх викликати не вдалося б.

Передача типу, що не реалізує `Shape`, — це помилка типів на виклику.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Узагальнена функція отримує власну копію для кожного типу, з яким її викликають. Жодних перевірок
типів чи розгалужень під час виконання не задіяно.

## 3. Типові реалізації

Якщо метод трейта має тіло, це тіло використовується, коли `impl` метод пропускає.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Реалізація стандартних трейтів

Стандартна бібліотека теж має трейти. Реалізація одного з них робить для вашого типу доступними
стандартні функції, що його використовують.

| Трейт | Методи для реалізації | Що вмикає |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, візерунок `(= expr)` у `match` тощо |
| `Ord` | `less` | `less-equal`, `greater` тощо. `Ord` успадковує `Eq` |
| `print-object` | `print-object` | Як значення показують `println` та подібні |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` тощо |
| `Error` | `message`, `source` | Використання як типу помилки ([Обробка помилок](errors.md)) |

Реалізуймо `Eq` і `Ord` для типу, що представляє суму грошей. Оскільки `Ord` успадковує `Eq`, `impl` для
`Eq` має йти першим.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

Реалізація `print-object` визначає, як `println` показує значення. Аргумент `escape` дорівнює `true`,
коли запитано форму, яку можна прочитати назад, як-от із `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

У поєднанні з обмеженням трейтом можна написати функцію, що працює для будь-якого типу, який реалізує
`Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Якщо передати `Vector` зі значеннями `money` 300, 900 і 100 у цьому порядку, вона повертає
`(some 900 yen)`.

## 5. `:dyn`: спільна робота зі значеннями різних типів

Усі елементи `Vector<T>` мають однаковий тип, тож значення `circle` і `rect` не можна покласти в один
`Vector<circle>`. Щоб працювати разом із «чимось, що реалізує `Shape`», використовуйте тип
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Значення `circle` або `rect`, поставлене там, де очікується `:dyn Shape`, перетворюється автоматично.
- Яке саме `area` запускає виклик `(area s)`, визначається під час виконання за типом того, що тримає `s`.
- Розміщення значення, тип якого не реалізує `Shape`, там, де очікується `:dyn Shape`, — це помилка
  типів.

Вибір між обмеженнями трейтами з розділу 2 і `:dyn`:

| | Обмеження трейтом (`where`) | `:dyn Trait` |
|---|---|---|
| Коли визначається викликаний метод | До запуску | Під час виконання |
| Змішування типів в одному `Vector` | Неможливе | Можливе |
| Придатні типи | Без обмежень | Структури, переліки, `int`, `string`, `f64` та інші (не `bool`, `char`, `symbol`, `i32` і подібні) |

Точний перелік типів, які можна використовувати, наведено в
[Довіднику із синтаксису 3.9](../reference/syntax.md#39-deftrait--impl--трейти).

Деякі трейти не можна використовувати з `:dyn`: ті, чиї методи вживають `Self` для аргументу, відмінного
від `self`, або для результату (як-от `equals` в `Eq`). Оскільки тип невідомий до часу виконання,
створити «значення того самого типу» неможливо.

## 6. Обмеження

- Тримайте визначення трейта, його `impl` і код, що використовує його через `:dyn`, в одному
  модулі (файлі). Зробити трейт видимим для інших модулів поки що не можна.
- Типи й трейти ділять один простір імен. В одному модулі тип і трейт не можуть мати однакову
  назву.

## 7. Що читати далі

- [Макроси](macros.md): визначення власного синтаксису
- [Довідник із синтаксису 3.9](../reference/syntax.md#39-deftrait--impl--трейти): загальні реалізації
  (blanket), асоційовані типи тощо
- [Стандартні трейти](../reference/functions/traits.md): перелік трейтів стандартної бібліотеки
