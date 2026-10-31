<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# Встроенные функции

Список встроенных функций, методов и стандартной библиотеки. О синтаксисе (специальных формах и способах
определения) см. [Справочник по синтаксису](../syntax.md); список типов — в [Типах](../types.md).

## Формы вызова

Есть три формы вызова.

- Свободные функции: `(name args...)`
- Методы экземпляра: `(name receiver args...)` (разрешаются по статическому типу первого аргумента)
- Статические методы (ассоциированные функции): `(Type::name args...)`

У каждого типа может быть свой метод с тем же именем. `(+ a b)` вызывает `+` типа `a`.

## Имена, которым нужен `use`

Из имён стандартной библиотеки в любом модуле без всякой записи доступны такие
([справочник по синтаксису 3.10](../syntax.md#310-module--use--пространства-имён)):

- примитивные типы, `Option`/`Result`/`Vector`, итерация, S-выражения и списки, функции
  последовательностей, управляющие макросы, сравнение и арифметические трейты
- всякое имя, которое есть и в пакете `COMMON-LISP` языка Common Lisp (`length`, `format`, `open` и
  т. д.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, трейты потоков `Stream`,
  `InputStream` и `OutputStream`, а также `Hash`, который реализуют ключи `HashTable`
- типы, которые упоминаются в их функциях и переменных

Любое другое имя вводят через `use`, например `(use std::net::tcp-connect)`, или пишут путём,
`std::net::tcp-connect`. В таблице — эти имена и модуль каждого.

| Модуль | Имена |
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

## Как читать таблицы

У таблиц в каждой главе столбцы «имя, форма, тип, описание». Столбец типа записывается как
`(тип-аргумента,...)→тип-результата`.

- Одиночная заглавная буква, например `T`, `A` или `B`, — переменная типа.
- Пометка вроде `where Eq A` — ограничение трейтом, которому должна удовлетворять переменная типа.
- `Iter<A>` означает «любая реализация `Iter`, у которой `Item` — это `A`».
- Аргументы с пометками `&optional` / `&key` можно опускать.

## Главы

| Файл | Содержание |
|---|---|
| [numbers.md](numbers.md) | Целые, числа с плавающей точкой, рациональные, комплексные, логические значения, битовые операции, случайные числа |
| [sequences.md](sequences.md) | Пара `cons-cell`, данные S-выражений `Sexpr`, символы, функции последовательностей, ленивые итераторы `lazy`, функции высшего порядка |
| [collections.md](collections.md) | Строки, символы, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, типы ошибок и трейт `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, арифметические трейты, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, красивая печать, `print-object`, управляющие переменные печати |
| [format.md](format.md) | Директивы формата |
| [streams-files.md](streams-files.md) | Потоки, операции с файлами, пути, readtable |
| [concurrency.md](concurrency.md) | Задачи, каналы, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix-сокеты, UDP |
| [system.md](system.md) | Время, среда выполнения, инструменты реализации, `read`/`eval`, строки документации, функции для макросов |
