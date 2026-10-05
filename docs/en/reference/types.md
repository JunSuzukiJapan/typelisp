<!-- translated-from: docs/ja/reference/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Types

The types typelisp has, and the standard traits each type implements. How to write types is in
[Syntax Reference chapter 2](syntax.md#2-writing-types); the functions and methods of each type are in
[Built-in Functions](functions/README.md).

## 1. Primitive types

| Type | Contents | Details |
|---|---|---|
| `int` | An arbitrary-precision integer. Held as an immediate value while it fits in 63 bits, and becomes a bignum automatically beyond that. The default type of unannotated integer literals | [Numbers chapter 3](functions/numbers.md#3-arbitrary-precision-integers-int) |
| `i8` `i16` `i32` | Signed fixed-width integers | [Numbers chapter 1](functions/numbers.md#1-fixed-width-integers) |
| `u8` `u16` `u32` | Unsigned fixed-width integers | Same as above |
| `f32` `f64` | IEEE-754 floating-point numbers. Decimal literals default to `f64` | [Numbers chapter 4](functions/numbers.md#4-floating-point-numbers-f64--f32) |
| `ratio` | A rational number in lowest terms | [Numbers chapter 5](functions/numbers.md#5-rationals-ratio) |
| `bool` | `true` / `false` | [Numbers chapter 7](functions/numbers.md#7-booleans) |
| `char` | A Unicode scalar value | [Characters](functions/collections.md#2-characters-char) |
| `string` | An immutable string | [Strings](functions/collections.md#1-strings-string) |
| `symbol` | A symbol. Keywords (`:name`) have this type too | [Symbols](functions/sequences.md#3-symbols) |
| `()` | The Unit type. Its value is `()` too | |
| `!` | The Never type. The type of expressions that do not return, such as `panic`. Can be placed where any type is expected | |
| `ptr` `c-long` `c-ulong` | Words used only for passing values to and from C. They can be values only inside `unsafe`, and the places they can appear are limited | [Numbers chapter 2](functions/numbers.md#2-raw-words-at-the-c-boundary-ptr--c-long--c-ulong) |
| `random-state` | The state of a random number generator | [Numbers chapter 12](functions/numbers.md#12-random-numbers) |

There is no 64-bit integer type. For integers whose width does not matter, use `int`.

## 2. Built-in generic types

| Type | Contents | Details |
|---|---|---|
| `Option<T>` | A value that is there or not. `some` / `none` | [Option and Result](functions/option-result.md) |
| `Result<T,E>` | Success or failure. `ok` / `err` | Same as above |
| `Vector<T>` | A growable array | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | A hash table. The key type must implement `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | A handle to a task | [Tasks](functions/concurrency.md#1-taskt--handles-to-tasks) |
| `Thread<T>` | A handle to a task running on a dedicated OS thread | [Thread](functions/concurrency.md#7-threadt--dedicated-os-threads) |
| `Chan<T>` | A channel | [Channels](functions/concurrency.md#2-chant--channels) |

Function types are written `(fn (argument-types...) return-type)`, and trait objects `:dyn Trait`
([Syntax Reference chapter 2](syntax.md#2-writing-types)).

## 3. S-expression data

| Type | Contents | Details |
|---|---|---|
| `Sexpr` | A non-empty S-expression. 16 variants: `int`, `i8` to `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [S-expression data](functions/sequences.md#2-s-expression-data-sexpr) |
| `Option<Sexpr>` | S-expression data in general. The empty list `()` is `none` | Same as above |

## 4. Types in the standard library

Types the standard library (the prelude) defines with `defstruct` / `defenum`. They are treated the
same as types you write yourself, and everything you can do with a `defstruct` can be done with them.

| Type | Contents | Details |
|---|---|---|
| `cons-cell<A,B>` | A pair. `cons`/`car`/`cdr` | [Pairs](functions/sequences.md#1-pairs-cons-cellab) |
| `complex` | A complex number (`f64` components) | [Numbers chapter 6](functions/numbers.md#6-complex-numbers-complex) |
| `Array<T>` | A multidimensional array | [Array](functions/collections.md#5-arrayt-multidimensional-arrays) |
| `BitVector` | A fixed-length sequence of bits | [BitVector](functions/collections.md#6-bitvector-bit-vectors) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | The iterators returned by `iter` of each collection | [Iter](functions/traits.md#1-the-iter-trait-and-iteration) |
| `WaitGroup` | Waiting for N things to finish | [WaitGroup](functions/concurrency.md#4-waitgroup--waiting-for-n-completions) |
| `Mutex<T>` | Mutual exclusion for shared data | [Mutex](functions/concurrency.md#6-mutext--mutual-exclusion-for-shared-data) |
| `pathname` | A file name split into parts | [Pathnames](functions/streams-files.md#9-pathnames-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Streams | [Streams](functions/streams-files.md#3-concrete-stream-types) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Composite streams | [Composite streams](functions/streams-files.md#4-composite-streams) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Networking | [Networking](functions/network.md#1-types) |
| `ReadOutcome` | The result of `read-sexpr`. `datum` / `eof` | [Streams](functions/streams-files.md#6-generic-functions-and-file-operations) |
| `universal-time` `internal-time` `decoded-time` | Time | [Time](functions/system.md#1-time) |
| `heap-info` | The current state of the heap | [Implementation tools](functions/system.md#51-fields-of-heap-info) |

## 5. Error types

`Error` is not a type but a trait, and the following types implement it. To handle errors of any
kind, write `:dyn Error`.

| Type | Produced by |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | File and stream operations |
| `NetError` | Network operations |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Details are in [Error Types and the Error Trait](functions/option-result.md#3-error-types-and-the-error-trait).

## 6. Implementations of standard traits

Which types implement which traits. The methods of each trait are in
[Standard Traits](functions/traits.md) and in the chapters listed in the rightmost column.

### 6.1 Comparison, hashing and printing

| Trait | Implementing types | Details |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-comparison) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Same as above |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` and all the built-in error types | [print-object](functions/printing.md#5-print-object-per-type-printed-representation) |

`Eq`/`Ord` of `cons-cell<A,B>` can be used when the element types implement `Eq`/`Ord`.

### 6.2 Arithmetic

| Trait | Implementing types |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Details are in [Arithmetic traits](functions/traits.md#3-arithmetic-traits-add--sub--mul--div--rem--bits--number).

### 6.3 Iteration

| Trait | Implementing types |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Streams

| Type | Implemented traits |
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

Every stream implements `Stream`; input streams also implement `InputStream`, and output streams
`OutputStream`. `socket-listener` and `udp-socket` implement only `Stream` (`close` /
`open-stream-p`). Details are in [Streams](functions/streams-files.md#1-the-trait-hierarchy).

### 6.5 Others

| Trait | Implementing types | Details |
|---|---|---|
| `Error` | All the error types of chapter 5 | [Error types](functions/option-result.md#3-error-types-and-the-error-trait) |
| `Pathish` | `string` `pathname` | [Pathnames](functions/streams-files.md#91-the-pathname-designator-trait-pathish) |
