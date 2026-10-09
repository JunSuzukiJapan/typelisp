<!-- translated-from: docs/ja/reference/types.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Типы

Типы typelisp и стандартные трейты, которые реализует каждый тип. Как записывать типы, описано в
[главе 2 справочника по синтаксису](syntax.md#2-запись-типов); функции и методы каждого типа — во
[Встроенных функциях](functions/README.md).

## 1. Примитивные типы

| Тип | Содержимое | Подробности |
|---|---|---|
| `int` | Целое произвольной точности. Хранится как непосредственное значение, пока помещается в 63 бита, а сверх этого автоматически становится bignum. Тип по умолчанию для целочисленных литералов без аннотации | [Числа, глава 3](functions/numbers.md#3-целые-произвольной-точности-int) |
| `i8` `i16` `i32` | Знаковые целые фиксированной ширины | [Числа, глава 1](functions/numbers.md#1-целые-фиксированной-ширины) |
| `u8` `u16` `u32` | Беззнаковые целые фиксированной ширины | То же |
| `f32` `f64` | Числа с плавающей точкой IEEE 754. Десятичные литералы по умолчанию `f64` | [Числа, глава 4](functions/numbers.md#4-числа-с-плавающей-точкой-f64--f32) |
| `ratio` | Несократимое рациональное число | [Числа, глава 5](functions/numbers.md#5-рациональные-числа-ratio) |
| `bool` | `true` / `false` | [Числа, глава 7](functions/numbers.md#7-логические-значения) |
| `char` | Скалярное значение Unicode | [Символы](functions/collections.md#2-символы-char) |
| `string` | Неизменяемая строка | [Строки](functions/collections.md#1-строки-string) |
| `symbol` | Символ. Ключевые слова (`:name`) тоже имеют этот тип | [Символы](functions/sequences.md#3-символы-symbol) |
| `()` | Тип Unit. Его значение — тоже `()` | |
| `!` | Тип Never. Тип выражений, не возвращающих управление, например `panic`. Может стоять везде, где ожидается любой тип | |
| `ptr` `c-long` `c-ulong` | Слова, используемые только для передачи значений в C и обратно. Значениями они могут быть только внутри `unsafe`, а места, где они могут встречаться, ограничены | [Числа, глава 2](functions/numbers.md#2-сырые-слова-на-границе-с-c-ptr--c-long--c-ulong) |
| `random-state` | Состояние генератора случайных чисел | [Числа, глава 12](functions/numbers.md#12-случайные-числа) |

64-битного целого типа нет. Для целых, ширина которых не важна, используйте `int`.

## 2. Встроенные обобщённые типы

| Тип | Содержимое | Подробности |
|---|---|---|
| `Option<T>` | Значение, которое есть или которого нет. `some` / `none` | [Option и Result](functions/option-result.md) |
| `Result<T,E>` | Успех или неудача. `ok` / `err` | То же |
| `Vector<T>` | Растущий массив | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Хеш-таблица. Тип ключа должен реализовывать `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Дескриптор задачи | [Задачи](functions/concurrency.md#1-taskt--дескрипторы-задач) |
| `Thread<T>` | Дескриптор задачи, выполняющейся в отдельном потоке ОС | [Thread](functions/concurrency.md#7-threadt--отдельные-потоки-ос) |
| `Chan<T>` | Канал | [Каналы](functions/concurrency.md#2-chant--каналы) |

Типы функций пишутся как `(fn (типы-аргументов...) тип-результата)`, а типажные объекты — как `:dyn Trait`
([глава 2 справочника по синтаксису](syntax.md#2-запись-типов)).

## 3. Данные S-выражений

| Тип | Содержимое | Подробности |
|---|---|---|
| `Sexpr` | Непустое S-выражение. 18 вариантов: `int`, `i8`–`u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array` | [S-выражения](functions/sequences.md#2-s-выражения-sexpr) |
| `Option<Sexpr>` | Данные S-выражений в целом. Пустой список `()` — это `none` | То же |

## 4. Типы стандартной библиотеки

Типы, которые стандартная библиотека (прелюдия) определяет через `defstruct` / `defenum`. Они обрабатываются так же,
как типы, которые вы пишете сами, и с ними можно делать всё, что можно делать с `defstruct`.

| Тип | Содержимое | Подробности |
|---|---|---|
| `cons-cell<A,B>` | Пара. `cons`/`car`/`cdr` | [Пары](functions/sequences.md#1-пары-cons-cellab) |
| `complex` | Комплексное число (компоненты `f64`) | [Числа, глава 6](functions/numbers.md#6-комплексные-числа-complex) |
| `Array<T>` | Многомерный массив | [Array](functions/collections.md#5-arrayt-многомерные-массивы) |
| `BitVector` | Последовательность битов фиксированной длины | [BitVector](functions/collections.md#6-bitvector-битовые-векторы) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Итераторы, возвращаемые `iter` каждой коллекции | [Iter](functions/traits.md#1-трейт-iter-и-итерация) |
| `WaitGroup` | Ожидание завершения N действий | [WaitGroup](functions/concurrency.md#4-waitgroup--ожидание-n-завершений) |
| `Mutex<T>` | Взаимное исключение для общих данных | [Mutex](functions/concurrency.md#6-mutext--взаимное-исключение-для-общих-данных) |
| `pathname` | Имя файла, разбитое на части | [Пути](functions/streams-files.md#9-пути-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Потоки | [Потоки](functions/streams-files.md#3-конкретные-типы-потоков) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Составные потоки | [Составные потоки](functions/streams-files.md#4-составные-потоки) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Сеть | [Сеть](functions/network.md#1-типы) |
| `ReadOutcome` | Результат `read-sexpr`. `datum` / `eof` | [Потоки](functions/streams-files.md#6-обобщённые-функции-и-операции-с-файлами) |
| `universal-time` `internal-time` `decoded-time` | Время | [Время](functions/system.md#1-время) |
| `heap-info` | Текущее состояние кучи | [Инструменты реализации](functions/system.md#51-поля-heap-info) |

## 5. Типы ошибок

`Error` — не тип, а трейт, и его реализуют следующие типы. Чтобы обрабатывать ошибки любого вида, пишите
`:dyn Error`.

| Тип | Кем создаётся |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Операции с файлами и потоками |
| `NetError` | Сетевые операции |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Подробности — в [Типах ошибок и трейте Error](functions/option-result.md#3-типы-ошибок-и-трейт-error).

## 6. Реализации стандартных трейтов

Какие типы реализуют какие трейты. Методы каждого трейта описаны в [Стандартных трейтах](functions/traits.md) и в
главах из правого столбца.

### 6.1 Сравнение, хеширование и печать

| Трейт | Реализующие типы | Подробности |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-сравнение) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | То же |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` и все встроенные типы ошибок | [print-object](functions/printing.md#5-print-object-печатное-представление-по-типу) |

`Eq`/`Ord` для `cons-cell<A,B>` можно использовать, когда типы элементов реализуют `Eq`/`Ord`.

### 6.2 Арифметика

| Трейт | Реализующие типы |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Подробности — в [Арифметических трейтах](functions/traits.md#3-арифметические-трейты-add--sub--mul--div--rem--bits--number).

### 6.3 Итерация

| Трейт | Реализующие типы |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Потоки

| Тип | Реализуемые трейты |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Каждый поток реализует `Stream`; входные потоки дополнительно реализуют `InputStream`, а выходные — `OutputStream`.
`socket-listener` и `udp-socket` реализуют только `Stream` (`close` / `open-stream-p`). Подробности — в
[Потоках](functions/streams-files.md#1-иерархия-трейтов).

### 6.5 Прочее

| Трейт | Реализующие типы | Подробности |
|---|---|---|
| `Error` | Все типы ошибок из главы 5 | [Типы ошибок](functions/option-result.md#3-типы-ошибок-и-трейт-error) |
| `Pathish` | `string` `pathname` | [Пути](functions/streams-files.md#91-трейт-обозначения-пути-pathish) |
