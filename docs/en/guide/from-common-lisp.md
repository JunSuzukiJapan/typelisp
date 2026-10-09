<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# For Common Lisp Programmers

typelisp inherits Common Lisp's (CL's) syntax and many of its function names, but it is a
statically typed language. Because of this, CL code does not always work as written. This guide
collects the points where people used to CL tend to stumble, together with how to rewrite the code.

## 1. There is no `nil` or `t`

The boolean values are `true` and `false`. `nil` and `t` are not defined.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Only a `bool` can be a condition.** Writing `0` or an empty list as a condition is a type error.
  There is no rule that "everything other than nil is true".
- **The else branch of `if` cannot be left out.** `(if c x)` is an error. When no else branch is
  needed, use `when` / `unless`.
- **"No value" is expressed with `Option<T>`.** A function that returned nil in CL to mean "not
  found" returns `(some x)` or `none` here.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- The empty list `()` is, depending on context, either the value of the Unit type (the return value
  of a function that returns nothing) or the empty list of S-expression data. It is a different value
  from `false`.

## 2. Writing types

Function arguments and return values must have types.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; a generic function
  (unwrap-or (first (iter v)) default))
```

- A definition without types, such as `(defun f (x) x)`, cannot be written.
- Global variables such as `defvar` also need a type: `(defvar (count int) 0)`.
- `the` is not a run-time check but an annotation for the type checker.
- **There is no way to inspect types at run time.** There is no `typep` or `type-of`, because the
  type of every value is fixed at compile time. To accept one of several types, make a sum type with
  `defenum` or use a trait.
- `deftype` defines a type alias. A type describing a range of values, such as
  `(deftype small () '(integer 0 9))`, cannot be made.

The default integer type `int` has arbitrary precision; like CL's integer, there is no upper limit on
its size. The fixed-width types `i8` to `i32` and `u8` to `u32` also exist. There is no 64-bit
fixed-width integer type.

## 3. Functions as values

typelisp does not separate the namespaces of functions and variables. A function's name can be passed
as a value as it is. There is no `#'` and no `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; call it directly, not with funcall

(apply-to twice 5)                        ; twice, not #'twice
```

- Built-in functions such as `+` and `1+` can also be passed as values as they are, where the
  argument's type is fixed, as in `(fn (int) int)`. When passing one to a generic function such as
  `foldl` or `map`, it is not known which type's `+` is meant, so wrap it in a `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Sequence functions take **the collection first and the function second**: `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. This is the reverse of CL's `(mapcar f list)`.
- `lambda` cannot use `&optional` or `&key` (`&rest` can be used).
- **A function cannot be called before it is defined.** In CL you can call a function you define
  later, but here that gives `no such function`. For mutually recursive functions, declare one of
  them with `defsignature` first.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Lists and Vector

What corresponds to a CL list is **S-expression data**, whose type is `Option<Sexpr>` (the empty list
is `none`). `(list 1 2 3)` and `'(a b c)` have this type. S-expression data is something macros and
`read` work with; for an ordinary data container, use **`Vector<T>`**.

| What you want | CL | typelisp |
|---|---|---|
| Head and rest of an S-expression | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Walk an S-expression list | `(dolist (x xs) ...)` | The same |
| A sequence of elements of one type | A list or a vector | `Vector<T>` |
| A pair | `(cons a b)` | `(cons a b)` (its type is `cons-cell<A,B>`) |
| Mapping | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` are the accessors of the pair `cons-cell<A,B>` made with `cons`. They cannot be used on
S-expression lists.

A `Vector` can be written with `#(..)`, as a vector is in CL, and it prints as `#(..)` too:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

There are three differences from CL. The elements must all have the same type (`#(1 "a")` is a type
error). Each evaluation makes a new vector, so changing it does not affect the next evaluation (in
CL the result of modifying a literal is undefined). An empty `#()` needs its type, as in
`(the Vector<int> #())`. A multidimensional array is written `#2A((1 2) (3 4))`, as in CL.

Sequence functions such as `map`, `filter`, `sort` and `find` work on values that implement the
`Iter` trait. Pass a `Vector` after turning it into an iterator with `(iter v)`.

## 5. There are no multiple values

There is no `values` and no `multiple-value-bind`. Functions that return several values in CL return
a pair or a struct.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → a `cons-cell` whose `car` is 3 and `cdr` is 1 |
| `(decode-universal-time t)` → 9 values | A `decoded-time` struct |
| `(read-from-string s)` → value, position | `(read-from-string s)` returns a `cons-cell` of value and position inside a `Result`. For just the value, `(read s)` |

## 6. There are no special variables (dynamic binding)

`let` always binds lexically. If you rebind a variable defined with `defvar` using `let`, functions
called from there still see the original value.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 in CL, 1 in typelisp
```

To change a control variable such as `*print-base*` temporarily, use `dlet`. It assigns the value
and restores the original however the body is left.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` rewrites the global variable itself, so it is not a per-thread binding.

## 7. The condition system is not adopted

There is no `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` or `signal`.
They fit poorly with static typing. Instead, these two are used for different purposes:

- **Recoverable failures return `Result<T,E>`.** The caller separates `ok` / `err` with `match`.
  There is no shorthand like Rust's `?`.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Unrecoverable failures (bugs) are `panic`.** `(panic "message")`, passing `none` to `unwrap`,
  dividing by 0 and an index out of range are of this kind, and the program stops. The cleanup of
  `unwind-protect` runs before it stops.

Error types are unified by the `Error` trait, and `(message e)` gives the message. How to make your
own error type is in
[Option, Result and Error Types](../reference/functions/option-result.md#3-error-types-and-the-error-trait).
`assert` and `warn` can be used as in CL.

`catch` / `throw` / `unwind-protect` exist. However, the tag of `catch` is limited to an unevaluated
literal symbol (`'done`), and the values thrown with one tag have a single type.

## 8. There is no CLOS

There is no `defclass`, `defgeneric` or method combination.

- Data types are defined with `defstruct` (structs) and `defenum` (sum types).
- `defmethod` defines methods whose target is decided by **the static type of the first argument**
  only. There is no multiple dispatch.
- To give operations in common across types, use traits (`deftrait` / `impl`). For values whose
  concrete type is decided at run time, use the type `:dyn Trait`
  ([Syntax Reference 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

How `defstruct` differs:

- The constructor is `TypeName::new`: `(point::new 1 2)`. If you want a name like `make-point`, the
  `(:constructor make-point)` option creates one.
- Besides `(x p)`, an accessor can be written `p::x`. Change it with `(setf p::x 5)`.
- No predicate (`point-p`) is created. There is no `:conc-name`, `:type` or `:named`.
- `:include` only inherits slots; the type does not become a subtype of the parent.

## 9. Modules instead of packages

There are no packages. Namespaces are modules, and a file is a module by itself. Instead of
`pkg:symbol`, write `module::name`, and bring names in with `use`
([Modules and File Layout](modules.md)).

Keywords `:foo` exist and are symbols that evaluate to themselves. Since there are no packages, the
colon is part of the name: `(symbol->string :foo)` returns `":foo"`.

## 10. Differences in reading and syntax

- Upper and lower case are not distinguished (symbols become lowercase when read). This is the same
  as CL.
- There is no `#'` (section 3). Complex number literals `#c(...)` cannot be read; make complex
  numbers with `(complex 1.0 2.0)`.
- The clauses of the extended `loop` are written with keywords: `(loop :for i :from 1 :to 3 :collect i)`.
  A `loop` that does not start with a keyword is a simple infinite loop, left with `(break)` or
  `(return value)`. `return` leaves the innermost loop (to leave a function, use `return-from`).
- The destination of `format` is `false` (return a string), `true` (standard output) or a stream.
  The format directives are the same as in CL.
- Reading from a string is `(read "...")`, and reading from a stream is `(read-sexpr s)`. Both return
  a `Result`.
- `eval` type-checks the given expression before evaluating it, and returns a `Result`. Forward
  references are not possible, just as in source code.
- There is no `eval-when`.
- Function names do not use `?` or `!` suffixes. Predicates are named with `-p` / `p` as in CL
  (`zerop`, `sexpr-null`), or with `is-` in front (`is-some`).

## 11. Main functions with different names

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (from a stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Two-argument versions of `floor` and friends | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (argument order reversed; section 4) |
| `length` (of a vector) | `len` |
| `hash-table-count` | `count` / `size` |

The list of functions is in [Built-in Functions](../reference/functions/README.md).

## 12. Other things that do not exist

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` and `copy-readtable`, `readtable-case` (reader macros themselves can be defined with
  `set-macro-character`)
- Logical pathnames and wildcard pathnames
- `input-stream-p` / `output-stream-p` (the direction of a stream is decided by its type)
