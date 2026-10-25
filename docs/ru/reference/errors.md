<!-- translated-from: docs/ja/reference/errors.md @ d179fbc5112a7c56e11d7cee68e15bbebfafb631 -->
# Сообщения об ошибках

Что означают основные сообщения об ошибках `typl` и как их исправить.

## 1. Как читать ошибку

Ошибки выводятся в стандартный поток ошибок в таком виде:

```text
error: файл:строка:столбец: вид: сообщение
```

`вид` говорит о том, когда ошибка была найдена.

| Вид | Когда | Смысл |
|---|---|---|
| `type error` | До выполнения (при проверке) | Ошибка в типах или именах. Эта форма не выполняется |
| (без вида) | При чтении или проверке | Синтаксическая ошибка, например несбалансированные скобки, или имя, которое не удаётся найти |
| `panic` | Во время выполнения | Невосстановимая неудача. Программа останавливается после выполнения очистки `unwind-protect` |

Строки, начинающиеся с `warning:`, — предупреждения, и обработка продолжается.

`файл:строка:столбец` указывает на ошибочное выражение. Для ошибки времени выполнения внутри функции стандартной
библиотеки оно указывает на место, где программа вызвала эту функцию. У некоторых ошибок нет позиции (например,
`error: panic: ...`).

Пример:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Это значит, что выражение в строке 1, столбце 24 файла `main.typl` было `string` там, где ожидался `i32`.

## 2. Ошибки при проверке

Ошибки, найденные до выполнения. Форма не выполняется, пока они не исправлены.

### 2.1 Типы

| Сообщение | Смысл и исправление |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Выражение типа `U` стоит там, где нужен тип `T`. Неявных преобразований нет; числа преобразуйте через `(as T x)`. `int` и `i32` — тоже разные типы |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Литерал не помещается в тип. Если нужно усечь, пишите `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Типа с таким именем нет. Определяйте тип до первой использующей его формы (у типов нет опережающих объявлений). Если имелась в виду переменная типа, напишите её в объявляющей позиции, например `<foo>` после имени функции ([Справочник по синтаксису 3.6](syntax.md#36-defstruct--структуры-пользовательские-типы)) |
| ``cannot infer type argument `t` for `Vector::new` `` | Аргумент типа не удаётся определить. Укажите тип через `the`, как в `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` обрабатывает не все варианты. Добавьте ветви для недостающих вариантов или ветвь `_` |
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | Функция требует трейт, который переданный тип не реализует. Напишите `(impl Eq pt ...)` ([Стандартные трейты](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Значение типа, не реализующего трейт, передано туда, где ожидается `:dyn`. Напишите `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Имя трейта написано там, где должен быть тип. Пишите `:dyn Error` |
| ``if: (if cond then else)`` | У `if` неверная форма. `if` требует ветви «иначе». Если она не нужна, используйте `when` |

### 2.2 Имена

| Сообщение | Смысл и исправление |
|---|---|
| `no such function: bar` | Нет ни функции, ни метода с таким именем. Проверьте написание |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Методы выбираются по типу первого аргумента. Метод с таким именем есть, но не для типа первого аргумента (здесь `int`). В конце сообщения перечислены типы, у которых этот метод есть (для метода, написанного для одной конкретизации, — сама конкретизация, например `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | Метод, переданный как значение, выбирается по типу первого параметра того функционального типа, в качестве которого он передаётся. У этого типа (здесь `box<string>`) нет метода с таким именем. В конце сообщения перечислены типы, у которых он есть |
| `unbound variable: y` | Переменной с таким именем нет. Проверьте написание и область видимости привязки (не используется ли она вне своего `let`?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | В одном модуле двум из функции, глобальной переменной и макроса дано одно имя. Переименуйте одно из них ([Справочник по синтаксису 3.10](syntax.md#310-module--use--пространства-имён)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | Для структуры `new` не генерируется. Создайте значение через `make-instance` или напишите `new` в `impl` ([Справочник по синтаксису 3.5](syntax.md#35-impl--методы-типа)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | `defmethod` нет. Методы пишутся в `impl` типа ([Справочник по синтаксису 3.5](syntax.md#35-impl--методы-типа)) |
| ``defstruct: (:constructor ...) is gone — ...`` | Опции `:constructor` нет. Конструктор пишется в `impl` как статический метод |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | Метод с одним именем написан и в `impl`, где параметры типа — переменные (`Box<T>`), и в `impl` конкретизации (`Box<int>`), в любом порядке. Оставьте только один ([Справочник по синтаксису 3.5](syntax.md#35-impl--методы-типа)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | Один и тот же трейт реализован и для `Box<T>`, и для `Box<int>`. Оставьте только одну реализацию |
| ``use: unresolved `nosuch` `` | Модуль, указанный в `use`, не найден. О соответствии имён файлов путям модулей см. [Справочник по синтаксису 3.11](syntax.md#311-файлы-и-модули-многофайловые-проекты) |
| `unresolved path: c::hidden` | Модуль существует, но имени нет, или оно не видно из-за отсутствия `pub` |
| `circular module dependency: a -> b -> a` | Модули подключают друг друга через `use`. Вынесите общую часть в отдельный модуль |
| ``return-from: no enclosing block named `nope` `` | Нет объемлющего `block` с именем, переданным в `return-from`. Блок функции можно использовать только внутри этой функции |

### 2.3 Вызовы

| Сообщение | Смысл и исправление |
|---|---|
| `f: expected 1 argument(s), got 2` | Число аргументов не совпадает |
| `f: unknown keyword argument :b` | Передан ключевой аргумент, которого у функции нет |
| `circle: expected 1 field(s), got 2` | Число значений, переданных варианту перечисления, не совпадает с числом его полей |
| ``make-instance point: slot `y` is not given and has no default`` | В `make-instance` не задано поле без значения по умолчанию |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | Задано поле, которого нет у структуры |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | Поле без `pub` задано вне модуля, где определён тип. Попросите этот модуль предоставить конструктор с `pub` |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | В `impl` нескольких конкретизаций есть статический метод с таким именем, и ничто не определяет, какой вызвать. Укажите ожидаемый тип, как в `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | Метод, написанный для конкретизации, передан в `compile`, `trace`, `documentation` и т. п. только по имени типа. Пишите конкретизацию, как в `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | Выполнено присваивание имени, определённому через `defconstant`. Если оно должно меняться, используйте `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Функция, объявленная через `defsignature`, не определена |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | Вызов `(nosuch arg colon at)`, записанный в этом месте, не разрешается. Определите метод типа аргумента или функцию ([Директивы формата, глава 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | `~/name/` записан там, где он получает элементы списка, например внутри `~{`. По типу элементов нельзя определить, что вызывать |

## 3. Ошибки чтения

| Сообщение | Смысл и исправление |
|---|---|
| `unexpected end of input while reading a list` | Не хватает закрывающей скобки. Позиция указывает, где закончилось чтение (например, конец файла), так что ищите открывающую скобку |

## 4. Ошибки во время выполнения (panic)

| Сообщение | Смысл и исправление |
|---|---|
| `panic: divide by zero` | Деление на ноль для целых или рациональных чисел. Деление на ноль с плавающей точкой panic не вызывает; оно даёт `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` применён к `none`. Обрабатывайте случай `none` через `match` или `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Индекс вне диапазона. Проверяйте длину через `len` или используйте функцию, возвращающую `none` вне диапазона (`nth`, `pop` и т. п.) |
| `panic: an integer argument does not fit a fixnum` | `int`, не помещающийся в 63 бита, передан аргументу, принимающему индекс или количество |
| `throw: no enclosing (catch 'oops) for this throw` | `throw` выполнен без объемлющего `catch` с той же меткой |
| `panic: <message>` | Программа вызвала `(panic "<message>")`. Неудачный `assert` даёт `assertion failed: ...` |

`panic` останавливает весь процесс, даже если происходит внутри задачи
([Справочник по синтаксису 12.4](syntax.md#124-взаимодействие-с-другими-возможностями)). Неудачи, от которых нужно
восстанавливаться, выражайте через `Result` ([глава 9 справочника по синтаксису](syntax.md#9-принципы-обработки-ошибок)).

## 5. Предупреждения

| Сообщение | Смысл |
|---|---|
| ``warning: redefining function `f` `` | Функция с тем же именем определена снова. Действует более позднее определение. Обычно появляется, когда вы исправляете определение в REPL |
