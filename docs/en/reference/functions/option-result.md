<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option, Result and Error Types

## 1. `Option<T>` / `Result<T,E>`

Constructors: `Option<T>` has `Some(T)` / `None`. `Result<T,E>` has `Ok(T)` / `Err(E)`. `E` can be any
type: the built-in concrete error types, and types you write yourself with `defstruct`/`defenum`,
fit there just the same (chapter 3).

| Name | Form | Option | Result | Description |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Takes out the value. Panics on `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | The value, or the default |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Whether it is `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Whether it is `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Whether it is `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Whether it is `Err` |

The constructors are `Option::some`/`Option::none`/`Result::ok`/`Result::err` (or, after
`(use option)`/`(use result)`, the bare names `some`/`none`/`ok`/`err`).

Branching is written explicitly with `match`. There is no syntax corresponding to Rust's `?`.

## 2. The run-time representation of `Option<T>`

As in Rust, **`Option<T>` usually makes no box**. `some v` is `v` itself and `none` is the
empty-list value, with no allocation and no indirection. `Option<Sexpr>` (where the empty list is
`none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` and `Option<(fn ...)>`
all take this form.

A box is used only when a value of `T` cannot be told apart from the empty-list value:

| `T` | Representation | Reason |
|---|---|---|
| `Option<U>` (nested) | Box | The inner `none` would be the same value as the outer `none` |
| `()` | Box | The value of `()` is the empty-list value itself |
| `ptr` / `c-long` / `c-ulong` | Box | All 64 bits are value, leaving no room to tell them apart |
| Anything else | No box | — |

The representation is decided by the type alone and cannot be read from a value. When printing,
`(some ...)`/`none` is reconstructed from the static type, so `(format false "~a" opt)` prints
`(some 1)`. There are two restrictions:

- **It cannot be put into a `:dyn Trait`** (passing a value of `Option<int>` for which you wrote
  `(impl Speak Option<int> ...)` to a `:dyn Speak` is an error).
- A `(the Option<T> ...)` downcast from an `Sexpr` **names a constructor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. The form that binds the whole value,
  `(the Option<int> o)`, is an error.

## 3. Error types and the `Error` trait

Following Rust's `std::error::Error`, **`Error` is not a type but a trait**. The concrete types that
represent errors are separate for each purpose, and each implements `Error`.

| Type | Produced by |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | File and stream operations ([Streams and Files](streams-files.md)) |
| `NetError` | Network operations ([Networking](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. CL's `simple-error`: the default choice when you just want to say what happened |
| `WrappedError` | `(wrap-error msg cause)`. A type carrying both your own message and the cause; it is why the `Error` trait has `source` |

`ParseIntError` through `NetError` are each "an enum with a single variant holding one message
string", and the type name and variant name are the same (`(match e ((ParseIntError m) m))`, built
with `(ParseIntError::ParseIntError "...")`). There is nothing special about them: they are treated
exactly as your own error types written with `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Name | Form | Type | Description |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | The error message (a method of the `Error` trait) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | The cause this error wraps, or `None` if there is none (Rust's `Error::source`) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implements `Error`) | Widens a concrete error type to the trait object |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implements `Error`) | The message and the chain of causes found by following `source`, one cause per line. CL has no counterpart (Rust's "caused by") |

If you implement `Error` for your own error type, it can be handled **the same way** as the built-in
errors:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

To collect several error types into one `Result`, use `Result<T, :dyn Error>` (corresponding to
Rust's `Box<dyn Error>`), and widen concrete errors with `as-dyn-error`. Since there is no `?`, this
conversion is written explicitly:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Types and traits share one namespace** (as in Rust). Within one module, a `defstruct`/`defenum` and
a trait cannot have the same name, and writing a trait name in a type position is reported as
"`error` is a trait, not a type — write `:dyn error`".

Unrecoverable failures are expressed with `panic`. For `panic` and `catch`/`throw`, see the
[Syntax Reference](../syntax.md#8-non-local-exits-catch--throw--unwind-protect); for the error handling
policy, see [chapter 9 of the same](../syntax.md#9-error-handling-policy).
