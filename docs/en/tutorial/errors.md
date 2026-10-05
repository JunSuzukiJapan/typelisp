<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Error Handling

Error handling in typelisp divides failures into two kinds.

| Kind of failure | Examples | How it is expressed |
|---|---|---|
| Failures that can happen (recoverable) | A file is missing, input is not a number | Return a `Result<T,E>` |
| Mistakes in the program (not recoverable) | An index out of range, `unwrap` of `none`, division by zero | Stop with `panic` |

On top of these there are `catch` / `throw`, which leave many function calls at once, and
`unwind-protect`, which runs cleanup however its body is left. This chapter assumes you have read
the `Result` section of [Type Basics](types.md).

## 1. Return a `Result` and receive it with `match`

Here is a function that reads a port number from a string. It can fail in two ways: the input is not
a number, or it is out of range.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

The caller separates success from failure with `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- The value of a function returning `Result` cannot be used unless `match` handles the `err` case.
  Forgetting to handle failure is a type error.
- The error from `parse-int` is a value of type `ParseIntError`. `(message e)` gives its message
  string.

## 2. Passing a failure up to the caller

There is no shorthand like Rust's `?`. When calling several functions that return `Result` in turn,
write the "return the failure as it is" part with `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

When you know an operation cannot fail, or in a small script where stopping on failure is fine,
`unwrap` takes out the contents. If the value is an `err`, it panics. If a default value will do,
use `unwrap-or`.

## 3. Making your own error type

Expressing errors as a type rather than a string lets the caller branch on the kind of error. An
error type is an ordinary `defenum` or `defstruct` that implements the `Error` trait.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` returns a description of the error.
- `source` returns another error that caused this one. With no cause, it is `none`.

## 4. Combining different kinds of errors

If one function calls both `parse-int` (`ParseIntError`) and `check-workers` (`config-error`), there
are two error types, and they cannot both be the `E` of one `Result<T,E>`. In that case, make `E`
`:dyn Error` (an error of any type that implements `Error`). Convert each error with
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Given `"4"`, `"-1"` and `"abc"`, the results are:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

For `:dyn`, see section 5 of [Traits](traits.md).

## 5. `panic`: mistakes in the program

When the program reaches a state that must never happen, stop it with `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- The type of `panic` is `!` (it does not return), so it can be written wherever any type is
  expected. That is why the two branches of the `if` above fit.
- These operations also panic: `unwrap` of `none` or `err`, `get` with an index out of range, and
  integer division by zero.
- `panic` stops the program. Even when it happens inside a task, the whole program stops.
- In the REPL, a `panic` does not end the REPL; it waits for the next input.
- You can write `(todo)` for "not written yet" and `(unreachable)` for "this point should never be
  reached". Both panic.

`panic` is not a replacement for `Result`. For failures that can happen, such as user input or
whether a file exists, use `Result`.

## 6. `catch` / `throw`: jumping out across functions

`throw` jumps straight out to the enclosing `catch` with the same tag, however many function calls
lie in between.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

If `v` has no negative number, `validate` returns `"all fine"`; if it contains `-7`, control jumps
from inside `check-all` out to the `catch`, which returns `"negative: -7"`.

- Write the tag as a plain symbol, like `'bad-input`.
- **Each tag carries values of exactly one type.** In the example above `'bad-input` carries a
  `string`, so throwing an `int` with the same tag is a type error. The type of the `catch` body
  must also match the tag's type.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- A `throw` with no `catch` of the same tag to reach is an error.

If you only want to return early from within a function, use `return-from` instead of `catch` /
`throw`. `return-from` cannot cross functions, but in exchange you can tell where it returns to by
reading the source.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: always clean up

`(unwind-protect body cleanup)` runs the cleanup however the body is left: when it finishes
normally, when it is left by `throw`, and when it panics.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Use it for things like "always close a file you opened" or "always release a lock you took". The
standard library's `with-open-file` and `with-lock` use `unwind-protect` internally.

## 8. About Common Lisp's condition system

typelisp does not adopt Common Lisp's condition system (`handler-case`, `restart-case` and so on).
It does not show in the types which failures a function can cause, which fits poorly with static
typing. Failures that can happen are written in the types with `Result`, and transfers of control are
done with `catch` / `throw`.

## 9. What to read next

- [Concurrency](concurrency.md): tasks and channels
- [Option, Result and Error Types](../reference/functions/option-result.md): the list of functions
- [Error Messages](../reference/errors.md): what the common errors mean and how to fix them
