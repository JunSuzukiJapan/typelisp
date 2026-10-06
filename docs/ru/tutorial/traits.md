<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Трейты

Трейт — это обещание: «этот тип поддерживает такие операции». Трейты позволяют нескольким типам иметь общие
операции с одинаковыми именами, так что функцию, которая их использует, не нужно писать заново для каждого типа. Они
работают почти так же, как трейты в Rust. Предполагается, что вы прочитали [Основы типов](types.md).

## 1. Определение и реализация трейта

Определим как трейт `Shape` операции, возвращающие площадь и имя фигуры.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` после имени трейта — список трейтов, от которых он наследуется (раздел 4). Если их нет, оставьте его пустым.
- Каждая строка объявляет метод. `Self` обозначает «тип, реализующий этот трейт».

Чтобы реализовать трейт для типа, напишите `impl`.

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

Реализованные методы вызываются так же, как обычные функции.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Пропуск хотя бы одного из методов, объявленных трейтом, — ошибка типа в `impl`.

## 2. Ограничения трейтами: «любой тип, реализующий этот трейт»

На параметр типа обобщённой функции можно наложить условие с помощью `where`. Это называется **ограничением
трейтом**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Благодаря `(where (Shape T))` тело может использовать `name` и `area` на значениях `T`. Без ограничения о `T` ничего
не известно, и вызвать их было бы нельзя.

Передача типа, не реализующего `Shape`, — ошибка типа в месте вызова.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Обобщённая функция получает собственную копию для каждого типа, с которым её вызывают. Никаких проверок типов и
ветвлений во время выполнения не происходит.

## 3. Реализации по умолчанию

Если у метода трейта есть тело, оно используется, когда `impl` этот метод опускает.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe — реализация по умолчанию

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; написанная здесь имеет приоритет

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Реализация стандартных трейтов

В стандартной библиотеке тоже есть трейты. Реализовав такой трейт, вы делаете доступными для своего типа
стандартные функции, которые его используют.

| Трейт | Методы для реализации | Что это даёт |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, образец `(= expr)` в `match` и т. д. |
| `Ord` | `less` | `less-equal`, `greater` и т. д. `Ord` наследуется от `Eq` |
| `print-object` | `print-object` | То, как значения показывают `println` и подобные |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` и т. д. |
| `Error` | `message`, `source` | Использование в качестве типа ошибки ([Обработка ошибок](errors.md)) |

Реализуем `Eq` и `Ord` для типа, представляющего денежную сумму. Поскольку `Ord` наследуется от `Eq`, `impl` для `Eq`
должен идти первым.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (реализация по умолчанию в Ord)
```

Реализация `print-object` определяет, как `println` показывает значение. Аргумент `escape` равен `true`, когда
требуется форма, которую можно прочитать обратно, как с `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

В сочетании с ограничением трейтом можно написать функцию, работающую для любого типа, реализующего `Ord`.

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

Для `Vector` со значениями `money` 300, 900 и 100 в таком порядке она возвращает `(some 900 yen)`.

## 5. `:dyn`: совместная работа со значениями разных типов

Все элементы `Vector<T>` имеют один тип, поэтому значения `circle` и `rect` не поместить в один `Vector<circle>`.
Чтобы работать вместе с «чем-то, что реализует `Shape`», используйте тип `:dyn Shape`.

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

- Значение `circle` или `rect`, помещённое туда, где ожидается `:dyn Shape`, преобразуется автоматически.
- `area` какого типа выполнит вызов `(area s)`, решается во время выполнения по типу того, что содержит `s`.
- Значение типа, не реализующего `Shape`, там, где ожидается `:dyn Shape`, — ошибка типа.

Выбор между ограничениями трейтами из раздела 2 и `:dyn`:

| | Ограничение трейтом (`where`) | `:dyn Trait` |
|---|---|---|
| Когда определяется вызываемый метод | До выполнения | Во время выполнения |
| Смешение типов в одном `Vector` | Невозможно | Возможно |
| Допустимые типы | Без ограничений | Структуры, перечисления, `int`, `string`, `f64` и другие (не `bool`, `char`, `symbol`, `i32` и подобные) |

Точный список допустимых типов приведён в
[Справочнике по синтаксису 3.9](../reference/syntax.md#39-deftrait--impl--трейты).

Некоторые трейты нельзя использовать с `:dyn`: те, чьи методы используют `Self` для аргумента, отличного от `self`,
или для результата (как `equals` в `Eq`). Поскольку тип становится известен только во время выполнения, создать
«значение того же типа» невозможно.

## 6. Ограничения

- Держите определение трейта, его `impl` и код, использующий его через `:dyn`, в одном модуле (файле). Сделать трейт
  видимым из других модулей пока нельзя.
- Типы и трейты разделяют одно пространство имён. В одном модуле тип и трейт не могут иметь одинаковое имя.

## 7. Что читать дальше

- [Макросы](macros.md): определение собственного синтаксиса
- [Справочник по синтаксису 3.9](../reference/syntax.md#39-deftrait--impl--трейты): обобщённые реализации для всех
  типов, ассоциированные типы и другое
- [Стандартные трейты](../reference/functions/traits.md): список трейтов стандартной библиотеки
