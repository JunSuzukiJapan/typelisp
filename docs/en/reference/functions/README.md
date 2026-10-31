<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# Built-in Functions

The list of built-in functions, methods and the standard library. For syntax (special forms and how
to define things), see the [Syntax Reference](../syntax.md); for the list of types, see
[Types](../types.md).

## Call forms

There are three call forms.

- Free functions: `(name args...)`
- Instance methods: `(name receiver args...)` (resolved from the static type of the first argument)
- Static methods (associated functions): `(Type::name args...)`

Each type may have its own method of the same name. `(+ a b)` calls the `+` of `a`'s type.

## Names that need a `use`

Of the standard library's names, these can be used from every module without writing anything
([syntax reference 3.10](../syntax.md#310-module--use--namespaces)):

- the primitive types, `Option`/`Result`/`Vector`, iteration, S-expressions and lists, the sequence
  functions, the control macros, comparison and the arithmetic traits
- every name that is also a name of Common Lisp's `COMMON-LISP` package (`length`, `format`, `open`
  and so on)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, the stream traits `Stream`,
  `InputStream` and `OutputStream`, and `Hash`, which the keys of a `HashTable` implement
- the types the functions and variables of these mention

Any other name is brought in with a `use`, as in `(use std::net::tcp-connect)`, or written as its
path, `std::net::tcp-connect`. The table lists those names and the module each is in.

| Module | Names |
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

## Reading the tables

The tables in each chapter have the columns "name, form, type, description". The type column is
written as `(argument-type,...)→return-type`.

- A single capital letter such as `T`, `A` or `B` is a type variable.
- A note such as `where Eq A` is a trait bound that the type variable must satisfy.
- `Iter<A>` means "any implementation of `Iter` whose `Item` is `A`".
- Arguments marked `&optional` / `&key` can be left out.

## Chapters

| File | Contents |
|---|---|
| [numbers.md](numbers.md) | Integers, floating-point numbers, rationals, complex numbers, booleans, bit operations, random numbers |
| [sequences.md](sequences.md) | The pair `cons-cell`, S-expression data `Sexpr`, symbols, sequence functions, lazy iterators `lazy`, higher-order functions |
| [collections.md](collections.md) | Strings, characters, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, error types and the `Error` trait |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, arithmetic traits, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, the pretty printer, `print-object`, printer control variables |
| [format.md](format.md) | Format directives |
| [streams-files.md](streams-files.md) | Streams, file operations, pathnames, readtable |
| [concurrency.md](concurrency.md) | Tasks, channels, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix domain sockets, UDP |
| [system.md](system.md) | Time, the runtime environment, implementation tools, `read`/`eval`, docstrings, macro-related functions |
