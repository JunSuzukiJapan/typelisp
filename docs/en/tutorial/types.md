<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Type Basics

typelisp is a statically typed language. This chapter explains what the type checker does for
you, the types you will use most (`Option`, `Result`, structs and enums), and generics. It assumes
you have read [Getting Started](intro.md).

## 1. What static typing means

In typelisp, the type of every expression is settled before the program runs. An expression whose
types do not fit is an error before anything runs.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Running this file stops with a type error without even printing `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

You need to write types for function arguments and return values, global variables, and struct
fields. The type of a `let` variable is taken from its initial value.

The main types:

| Type | Example values |
|---|---|
| `int` | `42`, `-7` (arbitrary-precision integers) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Fixed-width integers |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | The return type of a function that returns no value |

There is no way to ask for the type of a value at run time (no Common Lisp `typep` or `type-of`),
because every type is already known before the program runs.

## 2. `Option<T>`: a value that may be missing

typelisp has no `nil`. "There may be no value" is expressed with the type `Option<T>`. A value of
`Option<T>` is either `some`, holding one value of `T`, or `none`, holding nothing.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` is not `int`, so it cannot be used in arithmetic as it is. `(+ (safe-div 10 2) 1)` is
a type error. To use what is inside, separate `some` from `none` with `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- In the `(some q)` arm, the contents are bound to the variable `q`.
- `match` checks that its arms **cover every case**. Forgetting the `(none)` arm is a type error.

### Why there is no nil

In many languages, `nil` (`null`) can stand in for a value of any type. As a result, forgetting to
handle the "no value" case goes unnoticed until the program runs. In typelisp, a place where a
value may be missing has type `Option<T>`, and the code does not pass the type checker unless
`match` handles the `none` case. A forgotten case is found before the program runs.

Conditions follow the same idea: only a `bool` can be the condition of `if`. There is no rule like
Common Lisp's "anything other than `nil` is true".

### Common operations

| Form | Meaning |
|---|---|
| `(unwrap-or opt default)` | The contents for `some`; the default for `none` |
| `(unwrap opt)` | Takes out the contents. Stops the program on `none` |
| `(is-some opt)` / `(is-none opt)` | Tests which one it is |

Many standard library functions return `Option`. For example, `position` returns the position in
`some` if the element is found and `none` if it is not.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: an operation that may fail

An operation that can fail returns `Result<T,E>`: `ok` holding a value of `T` on success, or `err`
holding an error `E` on failure.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Your own functions can return `Result` too.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Use `Option` when a missing value needs no explanation, and `Result` when you want to say why
something failed. [Error Handling](errors.md) goes into handling errors in detail.

## 4. `defstruct`: structs

A type with named fields is defined with `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

The definition gives you the following:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

To give a struct functions of its own, use `defmethod`. The type of the first argument (`self`)
decides which type the method belongs to.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Writing just the type name instead of a `self` argument makes a function called as
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: one of several shapes

A value that is one of several shapes, such as "a circle, a rectangle or a dot", is defined with
`defenum`. Each shape is called a **variant**. Each variant can hold a different number and type of
values.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

Values are made with the type name in front, as in `shape::circle`. In `match`, they are taken apart
by variant name.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Here too, `match` checks that every case is covered. If you later add a variant to `shape`, every
`match` that does not handle it becomes a type error, so no place that needs fixing is missed.

After `(use shape)`, you can write `(rect 5 6)` without the type name.

`Option` and `Result` are enums built with this same mechanism.

## 6. Generics

A function that works for any type is defined with a **type parameter** `<T>` after its name.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

You do not give the type when calling it. `T` is worked out from the arguments.

```lisp
(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Structs and enums can be generic too. `Vector<T>`, `Option<T>` and `Result<T,E>` are types of this
kind.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Inside a generic function nothing is known about `T`, so you cannot compare or add values of `T`.
To require something like "any type that can be compared", use traits ([Traits](traits.md)).

## 7. Giving a type another name

`deftype` gives a type another name.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` is just another spelling of `int`, not a new type. Passing a plain `int` where `meters` is
expected is not an error. If you want them kept apart, make a struct, as in
`(defstruct meters (value int))`.

## 8. What to read next

- [Traits](traits.md): giving types operations in common
- [Types](../reference/types.md): the built-in types and the traits each one implements
