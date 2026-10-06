<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Macros

A macro is a function that takes a program and returns a program. Macros let you create new syntax
that functions cannot express. typelisp macros work the same way as Common Lisp's `defmacro`. This
chapter assumes you have read "Lists (S-expressions)" in [Getting Started](intro.md).

## 1. How macros differ from functions

A function receives its arguments **after they are evaluated**. A macro receives them **as
expressions, before evaluation** (as S-expression data), builds another expression and returns it.
The returned expression replaces the macro call, and only then is it type-checked and run. This
replacement is called **expansion**.

For example, syntax like `unless` cannot be written as a function. As a function, the body would be
evaluated first even when the condition is true.

## 2. `defmacro` and quasiquote

Let us make `my-unless`, which runs its body only when the condition is false.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Macro arguments have no types written. Every argument is S-expression data.
- `&rest body` receives the remaining arguments together as one list.
- An expression starting with `` ` `` (quasiquote) is built as data, as written. Inside it:
  - `,test` inserts the contents of the variable `test` at that position.
  - `,@body` splices the elements of the list `body` in at that position.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

You can check the expansion with `macroexpand-1`. When writing a macro, looking at its expansion
first is the quickest way forward.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Expansions are type-checked too

The expression a macro returns is type-checked like any expression you write by hand.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

The error is reported at the place where the macro was called.

The rules that the else branch of `if` cannot be left out, and that both branches of an `if` must
have the same type, apply to expansions as they are. The `my-unless` above ends with
`(progn ,@body ())` so that, whatever the type of the body's last expression, both branches of the
`if` have type `()`.

## 4. Name clashes and `gensym`

A straightforward macro that swaps the values of two variables looks like this:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

It works most of the time, but breaks when the caller's variable happens to be named `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (not swapped)
```

The expansion is `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, and the `tmp` the macro
made hides the caller's `tmp`.

To avoid this, create the names of variables used inside a macro with `gensym`. `gensym` returns a
fresh symbol that cannot be written anywhere in a program.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

As in Common Lisp, typelisp macros do not prevent name clashes automatically (they are unhygienic).
Remember: **use `gensym` for the bindings a macro creates.**

In the same way, a macro that repeats its body a given number of times can be written like this:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Expanding differently depending on the arguments

A macro body is ordinary typelisp code, so it can inspect its arguments with `if` or `match` and
build a different expansion. The arguments are S-expression data (`Option<Sexpr>`), and the empty
list is `none`.

Let us make `my-and`, which returns `true` if all its conditions are true.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; no arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; just one
         `(if ,f (my-and ,@more) false)))             ; two or more
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` is a pattern that takes the head of a list into `f` and the rest into `more`.
- `sexpr-null` tests whether S-expression data is the empty list.
- The final `_` arm is needed because S-expression data has forms other than lists (numbers,
  strings and so on), and `match` requires those to be covered too. A `&rest` argument is always a
  list, so this arm never actually runs.
- A macro can call itself in its expansion. Expansion repeats until no macro calls remain.

## 6. Optional arguments

`&optional` receives arguments that may be left out. Default values can be given.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` receives keyword arguments
([Syntax Reference 3.14](../reference/syntax.md#314-defmacro--macro-definitions)).

## 7. `macrolet`: macros for one place only

A macro used only inside one expression can be defined with `macrolet`. It is not visible outside.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Things to keep in mind

- **A macro can only be called after its definition.** As with functions, define it near the top of
  the file.
- Make a macro available to other modules with `(pub defmacro ...)`.
- Much of the standard syntax, including `when`, `unless`, `cond`, `and`, `or` and `dotimes`, is
  defined as macros. You can see what is inside with `(macroexpand '(when true 1))`.
- If something can be written as a function, write it as a function. Macros cannot be passed as
  values, and you have to read their expansion to understand what they do.

## 9. What to read next

- [Error Handling](errors.md): `Result`, `panic`, `catch` / `throw`
- [Macro functions](../reference/functions/system.md#8-macros): `gensym`, `macroexpand` and more
