<!-- translated-from: docs/ja/reference/types.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# Типи

Типи, які є в typelisp, і стандартні трейти, що їх реалізує кожен тип. Як записувати типи, описано в
[Довіднику із синтаксису, розділ 2](syntax.md#2-запис-типів); функції та методи кожного типу наведено
у розділі [Вбудовані функції](functions/README.md).

## 1. Примітивні типи

| Тип | Зміст | Подробиці |
|---|---|---|
| `int` | Ціле число довільної точності. Зберігається як безпосереднє значення, поки вміщується в 63 біти, а далі автоматично стає bignum. Типовий тип цілих літералів без анотацій | [Числа, розділ 3](functions/numbers.md#3-цілі-числа-довільної-точності-int) |
| `i8` `i16` `i32` | Знакові цілі числа фіксованої ширини | [Числа, розділ 1](functions/numbers.md#1-цілі-числа-фіксованої-ширини) |
| `u8` `u16` `u32` | Беззнакові цілі числа фіксованої ширини | Те саме |
| `f32` `f64` | Числа з рухомою комою IEEE-754. Десяткові літерали типово мають тип `f64` | [Числа, розділ 4](functions/numbers.md#4-числа-з-рухомою-комою-f64--f32) |
| `ratio` | Раціональне число в нескоротному вигляді | [Числа, розділ 5](functions/numbers.md#5-раціональні-числа-ratio) |
| `bool` | `true` / `false` | [Числа, розділ 7](functions/numbers.md#7-булеві-значення) |
| `char` | Скалярне значення Unicode | [Знаки](functions/collections.md#2-знаки-char) |
| `string` | Незмінний рядок | [Рядки](functions/collections.md#1-рядки-string) |
| `symbol` | Символ. Ключові слова (`:name`) теж мають цей тип | [Символи](functions/sequences.md#3-символи) |
| `()` | Тип Unit. Його значення теж `()` | |
| `!` | Тип Never. Тип виразів, що не повертаються, як-от `panic`. Може стояти там, де очікується будь-який тип | |
| `ptr` `c-long` `c-ulong` | Слова, що використовуються лише для передавання значень до C і з C. Вони можуть бути значеннями лише всередині `unsafe`, а місця, де вони можуть траплятися, обмежені | [Числа, розділ 2](functions/numbers.md#2-сирі-слова-на-межі-з-c-ptr--c-long--c-ulong) |
| `random-state` | Стан генератора випадкових чисел | [Числа, розділ 12](functions/numbers.md#12-випадкові-числа) |

64-бітного цілого типу немає. Для цілих чисел, ширина яких не важлива, використовуйте `int`.

## 2. Вбудовані узагальнені типи

| Тип | Зміст | Подробиці |
|---|---|---|
| `Option<T>` | Значення, яке є або якого немає. `some` / `none` | [Option і Result](functions/option-result.md) |
| `Result<T,E>` | Успіх або невдача. `ok` / `err` | Те саме |
| `Vector<T>` | Масив, що росте | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Хеш-таблиця. Тип ключа має реалізовувати `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | Кортеж (від 1 до 12 елементів). Елементи читаються як `t::0` | [Синтаксис, розділ 2](syntax.md#2-запис-типів) |
| `Task<T>` | Дескриптор задачі | [Задачі](functions/concurrency.md#1-taskt--дескриптори-задач) |
| `Thread<T>` | Дескриптор задачі, що виконується в окремому потоці ОС | [Thread](functions/concurrency.md#7-threadt--окремі-потоки-ос) |
| `Chan<T>` | Канал | [Канали](functions/concurrency.md#2-chant--канали) |

Типи функцій записуються як `(fn (типи-аргументів...) тип-результату)`, а трейт-об'єкти — як `:dyn Trait`
([Довідник із синтаксису, розділ 2](syntax.md#2-запис-типів)).

## 3. Дані у вигляді S-виразів

| Тип | Зміст | Подробиці |
|---|---|---|
| `Sexpr` | Непорожній S-вираз. 19 варіантів: `int`, від `i8` до `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [Дані у вигляді S-виразів](functions/sequences.md#2-дані-у-вигляді-s-виразів-sexpr) |
| `Option<Sexpr>` | Дані у вигляді S-виразів загалом. Порожній список `()` — це `none` | Те саме |

## 4. Типи у стандартній бібліотеці

Типи, які стандартна бібліотека (prelude) визначає через `defstruct` / `defenum`. Вони трактуються так
само, як типи, що ви пишете самі, і з ними можна робити все, що можна робити з `defstruct`.

| Тип | Зміст | Подробиці |
|---|---|---|
| `cons-cell<A,B>` | Пара. `cons`/`car`/`cdr` | [Пари](functions/sequences.md#1-пари-cons-cellab) |
| `complex` | Комплексне число (компоненти `f64`) | [Числа, розділ 6](functions/numbers.md#6-комплексні-числа-complex) |
| `Array<T>` | Багатовимірний масив | [Array](functions/collections.md#5-arrayt-багатовимірні-масиви) |
| `BitVector` | Послідовність бітів фіксованої довжини | [BitVector](functions/collections.md#6-bitvector-бітові-вектори) |
| `HashSet<T>` | Набір елементів без повторів | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | Таблиця в порядку ключів | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | Послідовність, до якої додають і з якої беруть з обох кінців | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Ітератори, що повертає `iter` кожної колекції | [Iter](functions/traits.md#1-трейт-iter-та-ітерація) |
| `lazy::map-iter<I,A,U>` тощо | Ітератори, які повертають функції модуля `lazy` | [Ліниві ітератори](functions/sequences.md#ліниві-ітератори-модуль-lazy) |
| `WaitGroup` | Очікування завершення N речей | [WaitGroup](functions/concurrency.md#4-waitgroup--очікування-n-завершень) |
| `Mutex<T>` | Взаємне виключення для спільних даних | [Mutex](functions/concurrency.md#6-mutext--взаємне-виключення-для-спільних-даних) |
| `pathname` | Ім'я файлу, розбите на частини | [Шляхи](functions/streams-files.md#9-шляхи-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Потоки | [Потоки](functions/streams-files.md#3-конкретні-типи-потоків) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Складені потоки | [Складені потоки](functions/streams-files.md#4-складені-потоки) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Мережа | [Мережа](functions/network.md#1-типи) |
| `ReadOutcome` | Результат `read-sexpr`. `datum` / `eof` | [Потоки](functions/streams-files.md#6-узагальнені-функції-та-файлові-операції) |
| `universal-time` `internal-time` `decoded-time` | Час | [Час](functions/system.md#1-час) |
| `heap-info` | Поточний стан купи | [Засоби реалізації](functions/system.md#51-поля-heap-info) |

## 5. Типи помилок

`Error` — це не тип, а трейт, і його реалізують такі типи. Щоб обробляти помилки будь-якого виду,
пишіть `:dyn Error`.

| Тип | Чим створюється |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Операції з файлами та потоками |
| `NetError` | Мережеві операції |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Подробиці наведено в розділі [Типи помилок і трейт Error](functions/option-result.md#3-типи-помилок-і-трейт-error).

## 6. Реалізації стандартних трейтів

Які типи реалізують які трейти. Методи кожного трейта наведено в розділі
[Стандартні трейти](functions/traits.md) і в розділах, указаних у крайньому правому стовпці.

### 6.1 Порівняння, хешування та друк

| Трейт | Типи, що реалізують | Подробиці |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-порівняння) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | Те саме |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `pathname` `universal-time` `internal-time` і всі вбудовані типи помилок | [print-object](functions/printing.md#5-print-object-представлення-для-друку-для-кожного-типу) |

Трейти `cons-cell<A,B>` і кортежів `#{..}`, а також `print-object` колекцій доступні, коли типи
елементів реалізують цей трейт.

### 6.2 Арифметика

| Трейт | Типи, що реалізують |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Подробиці наведено в розділі [Арифметичні трейти](functions/traits.md#3-арифметичні-трейти-add--sub--mul--div--rem--bits--number).

### 6.3 Ітерація

| Трейт | Типи, що реалізують |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` типи модуля `lazy` (`lazy::map-iter<I,A,U>` тощо) |

### 6.4 Потоки

| Тип | Реалізовані трейти |
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

Кожен потік реалізує `Stream`; потоки вводу реалізують також `InputStream`, а потоки виводу —
`OutputStream`. `socket-listener` і `udp-socket` реалізують лише `Stream` (`close` /
`open-stream-p`). Подробиці наведено в розділі [Потоки](functions/streams-files.md#1-ієрархія-трейтів).

### 6.5 Інше

| Трейт | Типи, що реалізують | Подробиці |
|---|---|---|
| `Error` | Усі типи помилок з розділу 5 | [Типи помилок](functions/option-result.md#3-типи-помилок-і-трейт-error) |
| `Pathish` | `string` `pathname` | [Шляхи](functions/streams-files.md#91-трейт-designator-а-шляху-pathish) |
