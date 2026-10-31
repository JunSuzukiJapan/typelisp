<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# typelisp Documentation (English)

typelisp is a statically typed Lisp. For installing and building it, see [README.md](../../README.md)
at the top of the repository.

## Features

### Tasks and channels

Start tasks (lightweight threads) and pass values between them through channels. See
[Concurrency](tutorial/concurrency.md) for details.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Generics

Type parameters are written in angle brackets after the name of a type or function. Trait bounds
go in a `:where` clause. See [Type Basics](tutorial/types.md) and [Traits](tutorial/traits.md) for
details.

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option and Result instead of nil

There is no `nil`. A function that may have no value returns `Option<T>`, and one that may fail
returns `Result<T,E>`; `match` takes the contents out. Using an `Option<string>` as a `string`, or
leaving out the `none` case in a `match`, is a type error before the program runs. See [Type
Basics](tutorial/types.md) and [Error Handling](tutorial/errors.md) for details.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Chaining with `->`

The `->` macro passes a value as the first argument of each following form in turn.
`(-> x (f a) (g b))` is the same as `(g (f x a) b)`. Steps that return `Option` or `Result` can be
written from top to bottom. See [Option, Result and Error Types](reference/functions/option-result.md)
for details.

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### Type-checked macros

Macros are written with `defmacro`, quasiquote and `gensym`, as in Common Lisp. The expanded code is
type-checked just like code written by hand, and if it does not fit the types, the error is reported
where the macro was called. See [Macros](tutorial/macros.md) for details.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutorial

If you are new to typelisp, read these in order.

- [Getting Started](tutorial/intro.md): the REPL, functions, variables, conditionals, loops, lists and `Vector`
- [Type Basics](tutorial/types.md): static types, `Option`, `Result`, structs, enums, generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait bounds, `:dyn`
- [Macros](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Error Handling](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concurrency](tutorial/concurrency.md): tasks, channels, `select`, `Mutex`, `thread`

## Guides

- [Modules and File Layout](guide/modules.md): `use`, `pub`, how files map to modules
- [Compiling](guide/compile.md): the JIT, building executables with AOT compilation, dumps
- [File I/O, Streams and Networking](guide/io.md): files, pathnames, TCP / TLS / UDP, name resolution
- [C FFI](guide/ffi.md): calling C functions with `defffi` (including callbacks and C structs with `def-c-struct`)
- [Editor Integration](guide/editors.md): `typl-lsp` and setting up VS Code / Emacs
- [For Common Lisp Programmers](guide/from-common-lisp.md): how typelisp differs from CL and how to rewrite CL code

## Reference

- [Syntax Reference](reference/syntax.md): lexical syntax, writing types, definitions, control forms, compilation, concurrency
- [Built-in Functions](reference/functions/README.md): built-in functions, methods and the standard library
- [Types](reference/types.md): the types and the traits each one implements
- [Error Messages](reference/errors.md): what the common errors mean and how to fix them

## Editor Integration

Setup instructions are in the [Editor Integration guide](guide/editors.md). The key bindings and
settings of each editor are listed in these documents:

- [Emacs (typelisp-mode)](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
