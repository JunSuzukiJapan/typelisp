<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Getting Started

Starting from evaluating expressions in the REPL, this chapter covers functions, variables,
conditionals, loops, and lists and `Vector`, in that order. For how to build `typl`, see
[README.md](../../../README.md).

## 1. Starting the REPL

Started without arguments, `typl` enters the REPL (interactive mode). Type an expression after
`typl>` and it is evaluated on the spot and its value is printed. `:quit` leaves the REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

From here on, REPL input and results are shown in this form.

## 2. Evaluating expressions

typelisp is a Lisp, so an expression is wrapped in parentheses with **the operator or function name
first**. You write `(+ 1 2)`, not `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Numbers come in these kinds:

- **Integers** have type `int`. There is no upper limit on their size.
- **Decimals** have type `f64`. Write them with a decimal point, as in `1.5` or `2.0`.
- You cannot mix `int` and `f64` in a calculation. `(+ 1 2.0)` is a type error. To convert, write
  `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` on two integers gives an integer with the fractional part dropped (it does not produce a
fraction as Common Lisp does). Use `(mod 7 2)` for the remainder.

The boolean values are `true` and `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Defining functions

Functions are defined with `defun`. **The argument types and the return type are always written.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` means "an argument `n` of type `int`". With several arguments, list them:
  `((a int) (b int))`.
- The `int` after the argument list is the return type.
- The value of the last expression in the body is the function's return value. You do not write
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

A call whose types do not match is reported as a type error **before it runs**. When you run a
file, a single type error anywhere means not one line of the program runs.

To make an argument optional, use `&optional`. If you give a default value, the argument takes
that value when it is left out.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

The `false` given as the first argument of `format` means "return the result as a string instead of
printing it". Each `~a` is replaced by the next argument.

## 4. Variables

Local variables are created with `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- The type of a `let` variable is taken from its initial value. You do not need to write it.
- The variables of one `let` cannot refer to each other. To build one variable from the one
  before it, use `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

To change the value of a variable, use `setf`. **Assignment cannot change the variable's type.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Global variables are defined with `defvar`. Here you do write the type.

```lisp
(defvar (counter int) 0)
```

## 5. Conditionals

### if

Write `(if condition then-expression else-expression)`. **The else expression cannot be left out.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Only an expression of type `bool` can be a condition. Writing a number, as in `(if 0 ...)`, is a
  type error.
- The then and else expressions must have the same type.

When nothing should happen in the false case, use `when` (and `unless` for the opposite).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

With three or more conditions, `cond` reads better. The final `else` is taken when none of the
conditions hold.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

To branch on the shape of a value, use `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` matches any value. Since `int` has countless values, leaving out the `_` arm is an error saying
that not all cases are covered. Where `match` really shines is taking apart `Option` and types you
define yourself, which come up in the next chapter, [Type Basics](types.md).

## 6. Loops

A function can call itself.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

For a fixed number of repetitions, use `dotimes`. `i` goes from 0 to `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

There are also `while`, `do`, and the extended `loop` from Common Lisp. The clause words of the
extended `loop` are written as keywords (`:for`, `:collect` and so on).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Lists and Vector

### Vector

To hold a sequence of values of the same type, use `Vector<T>`. `T` is the element type.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; prints #<vector<int> 3 1 2>
```

- `(Vector::new)` alone does not determine the element type, so give the type with
  `(the Vector<int> ...)`.
- `(push v x)` appends to the end, `(get v i)` reads element `i`, and `(len v)` gives the length.
- A `get` with an index out of range stops the program with an error.

### lambda and higher-order functions

Anonymous functions are made with `lambda`. As with `defun`, you write the argument and return
types.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` and friends take a `Vector` turned into an **iterator** with
`(iter v)`. The collection comes first and the function second. The `v` above was bound with `let`,
so it cannot be used outside that `let`. The next example first defines `v` with `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

To process the elements one by one, use `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

A function that takes a function as an argument writes that argument's type as
`(fn (argument-types...) return-type)`. A function defined with `defun` can be passed by its name
as a value.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Lists (S-expressions)

Lists made with `'(1 2 3)` or `(list 1 2 3)` are **S-expression data**. Their elements do not have
to share a type.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S-expression data is mainly for handling programs themselves, in macros ([Macros](macros.md)) and
with `read`. For data whose elements have a known type, use `Vector<T>`. An S-expression list can
be walked with `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

A pair of two values is made with `cons` and taken apart with `car` and `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Writing a program in a file

A program can be written in a file (with the extension `.typl`) and run with `typl file-name`.
Use `println` to show results.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` prints using the same directives as `format` and ends with a newline. `print` does
  not add the newline.
- `~a` embeds a value in human-readable form, and `~s` in a form that can be read back (strings get
  their `"`).
- A file is read from top to bottom. **A function cannot be called before its definition.**

## 9. What to read next

- [Type Basics](types.md): `Option`, `Result`, structs, enums, generics
- [For Common Lisp Programmers](../guide/from-common-lisp.md): a list of differences for people who
  know Common Lisp
