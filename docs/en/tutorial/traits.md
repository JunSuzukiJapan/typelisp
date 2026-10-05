<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits

A trait is a promise that "this type supports these operations". Traits let several types share
operations of the same name, so that a function using them does not have to be written once per
type. They work almost exactly like Rust's traits. This chapter assumes you have read
[Type Basics](types.md).

## 1. Defining and implementing a trait

Define the operations that return a shape's area and name as the trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- The `()` after the trait name is the list of traits it inherits from (section 4). Leave it empty
  when there are none.
- Each line declares a method. `Self` stands for "the type implementing this trait".

To implement a trait for a type, write an `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

The implemented methods are called just like ordinary functions.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Leaving out even one of the methods the trait declares is a type error at the `impl`.

## 2. Trait bounds: "any type that implements this trait"

You can put a condition on a generic function's type parameter with `where`. This is called a
**trait bound**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Because of `(where (Shape T))`, the body can use `name` and `area` on values of `T`. Without the
bound nothing is known about `T`, so they could not be called.

Passing a type that does not implement `Shape` is a type error at the call.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

A generic function gets its own copy for each type it is called with. No run-time type tests or
branches are involved.

## 3. Default implementations

If a trait method has a body, that body is used when an `impl` leaves the method out.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementing standard traits

The standard library has traits too. Implementing one makes the standard functions that use it
available for your type.

| Trait | Methods to implement | What it enables |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, the `(= expr)` pattern of `match`, and so on |
| `Ord` | `less` | `less-equal`, `greater` and so on. `Ord` inherits from `Eq` |
| `print-object` | `print-object` | How values are shown by `println` and friends |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` and so on |
| `Error` | `message`, `source` | Use as an error type ([Error Handling](errors.md)) |

Let us implement `Eq` and `Ord` for a type representing an amount of money. Since `Ord` inherits
from `Eq`, the `Eq` `impl` has to come first.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

Implementing `print-object` decides how `println` shows the value. The `escape` argument is `true`
when a form that can be read back is asked for, as with `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Combined with a trait bound, you can write a function that works for any type implementing `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Given a `Vector` with `money` values of 300, 900 and 100 in that order, it returns `(some 900 yen)`.

## 5. `:dyn`: handling values of different types together

All elements of a `Vector<T>` have the same type, so `circle` and `rect` values cannot go into one
`Vector<circle>`. To handle "something that implements `Shape`" together, use the type
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- A `circle` or `rect` value placed where a `:dyn Shape` is expected is converted automatically.
- Which type's `area` the call `(area s)` runs is decided at run time by the type of what `s` holds.
- Placing a value whose type does not implement `Shape` where a `:dyn Shape` is expected is a type
  error.

Choosing between the trait bounds of section 2 and `:dyn`:

| | Trait bound (`where`) | `:dyn Trait` |
|---|---|---|
| When the called method is decided | Before running | At run time |
| Mixing types in one `Vector` | Not possible | Possible |
| Usable types | No restriction | Structs, enums, `int`, `string`, `f64` and others (not `bool`, `char`, `symbol`, `i32` and similar) |

The exact list of types that can be used is in
[Syntax Reference 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Some traits cannot be used with `:dyn`: those whose methods use `Self` for an argument other than
`self` or for the return value (such as `equals` in `Eq`). Since the type is not known until run
time, there is no way to produce "a value of the same type".

## 6. Restrictions

- Keep a trait's definition, the `impl`s for it and the code that uses it through `:dyn` in one
  module (file). A trait cannot yet be made visible to other modules.
- Types and traits share one namespace. Within one module, a type and a trait cannot have the same
  name.

## 7. What to read next

- [Macros](macros.md): defining syntax of your own
- [Syntax Reference 3.9](../reference/syntax.md#39-deftrait--impl--traits): blanket implementations,
  associated types and more
- [Standard Traits](../reference/functions/traits.md): the list of traits in the standard library
