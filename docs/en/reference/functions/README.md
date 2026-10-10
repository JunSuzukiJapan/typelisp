<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, arithmetic traits |
| [printing.md](printing.md) | `print`/`println`/`format`, the pretty printer, `print-object`, printer control variables |
| [format.md](format.md) | Format directives |
| [streams-files.md](streams-files.md) | Streams, file operations, pathnames, readtable |
| [concurrency.md](concurrency.md) | Tasks, channels, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix domain sockets, UDP |
| [system.md](system.md) | Time, the runtime environment, implementation tools, `read`/`eval`, docstrings, macro-related functions |
