<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Вбудовані функції

Перелік вбудованих функцій, методів і стандартної бібліотеки. Про синтаксис (спеціальні форми та способи
визначення) див. [Довідник із синтаксису](../syntax.md), про перелік типів — [Типи](../types.md).

## Форми виклику

Є три форми виклику.

- Вільні функції: `(name args...)`
- Методи екземпляра: `(name receiver args...)` (розв'язуються за статичним типом першого аргументу)
- Статичні методи (асоційовані функції): `(Type::name args...)`

Кожен тип може мати власний метод з тією самою назвою. `(+ a b)` викликає `+` типу `a`.

## Імена, яким потрібен `use`

З імен стандартної бібліотеки в будь-якому модулі без жодного запису доступні такі
([довідник із синтаксису 3.10](../syntax.md#310-module--use--простори-імен)):

- примітивні типи, `Option`/`Result`/`Vector`, ітерація, S-вирази та списки, функції послідовностей,
  керівні макроси, порівняння й арифметичні трейти
- будь-яке ім'я, що є і в пакеті `COMMON-LISP` мови Common Lisp (`length`, `format`, `open` тощо)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, трейти потоків `Stream`,
  `InputStream` і `OutputStream`, а також `Hash`, який реалізують ключі `HashTable`
- типи, які згадано в їхніх функціях і змінних

Будь-яке інше ім'я вводять через `use`, наприклад `(use std::net::tcp-connect)`, або пишуть шляхом,
`std::net::tcp-connect`. У таблиці — ці імена й модуль кожного.

| Модуль | Імена |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Як читати таблиці

Таблиці в кожному розділі мають стовпці «назва, форма, тип, опис». Стовпець типу записується як
`(тип-аргументів,...)→тип-результату`.

- Одна велика літера, як-от `T`, `A` або `B`, — це змінна типу.
- Примітка на кшталт `where Eq A` — це обмеження трейтом, якому має задовольняти змінна типу.
- `Iter<A>` означає «будь-яка реалізація `Iter`, у якої `Item` дорівнює `A`».
- Аргументи, позначені `&optional` / `&key`, можна пропускати.

## Розділи

| Файл | Зміст |
|---|---|
| [numbers.md](numbers.md) | Цілі числа, числа з рухомою комою, раціональні числа, комплексні числа, булеві значення, бітові операції, випадкові числа |
| [sequences.md](sequences.md) | Пара `cons-cell`, S-вирази `Sexpr`, символи, функції над послідовностями, ліниві ітератори `lazy`, функції вищого порядку |
| [collections.md](collections.md) | Рядки, знаки, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, типи помилок і трейт `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, арифметичні трейти, `Push`, `Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, змінні керування друком |
| [format.md](format.md) | Директиви формату |
| [streams-files.md](streams-files.md) | Потоки, файлові операції, шляхи, readtable |
| [concurrency.md](concurrency.md) | Задачі, канали, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, доменні сокети Unix, UDP |
| [system.md](system.md) | Час, середовище виконання, засоби реалізації, `read`/`eval`, рядки документації, функції, пов'язані з макросами |
