<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Standard Traits

The traits for iteration, comparison, arithmetic, and adding and removing elements. The other standard traits are in their own
chapters: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Error types](option-result.md#3-error-types-and-the-error-trait)), `print-object`
([Printing](printing.md#5-print-object-per-type-printed-representation)), and the stream traits and
`Pathish` ([Streams and Files](streams-files.md)). Which types implement which is in
[Types](../types.md). How to define traits is in the
[Syntax Reference](../syntax.md#39-deftrait--impl--traits).

## 1. The `Iter` trait and iteration

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implement `Iter` through `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectively (get the iterator with `(iter collection)`).
`Chan<T>` is itself an `Iter` (`recv` plays the role of `next`; [Channels](concurrency.md#2-chant--channels)).
`Sexpr` lists do not implement `Iter` (their element types are not uniform). If you implement `Iter`
for your own type, it can be walked with `doiter` as it is, and passed to the
[sequence functions](sequences.md#4-sequence-functions-on-iter).

## 2. `Equal` / `Ord` (comparison)

`Equal` is the trait `equal` uses, and corresponds to Rust's `PartialEq`. `Ord` corresponds to Rust's
`PartialOrd`. They are used in the `:where` bounds of generic functions to require that the element type
can be compared.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; must be implemented
(deftrait Ord (Equal)                                     ; inherits from Equal
  (defmethod less (self (other Self)) bool)                  ; must be implemented
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

To implement `Equal` you write `equal`, and for `Ord` only `less`. The default implementations fill in
the rest of `Ord`. `Ord` inherits from `Equal`, so `impl Equal X` is needed before `impl Ord X`.

`(equal a b)` calls the `Equal` implementation of `a`'s type. Writing it for a type that does not
implement `Equal` is a type error. To compare by identity use `eq`/`eql`, and to compare by structure
use `equalp`. These three can be used on a value of any type.

Each trait method can be called as a function as it is (inside a `where (Equal A)`/`(Ord A)` bound, or on
a concrete type that implements it):

| Name | Form | Type | Description |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Whether they are equal (Rust's `==`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` is implemented for: all numeric types (`i8` to `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` and the tuples (element by element, when
the elements are `Equal`), and `Option<T>` and `Result<T,E>` (when they are the same variant and their
contents are `equal`). For the built-in types, the type's own built-in `equal` method is the `Equal`
implementation as it is. Numbers compare with `=`, and strings compare by content, case-sensitively.
`Ord` is implemented for: all numeric types, `char` `string`, and `cons-cell<A,B>` (lexicographically,
when the elements are `Ord`).

The `equal` of a list (`Sexpr`) is CL's `equal`: conses compare by structure, strings by content, and
every other atom by `eql`. This comparison walks the values at run time and calls no method. So a struct
inside a list is compared by identity, as in CL, whatever `impl Equal` its type has.

The names of `Ord`'s methods do not overlap with the built-in operators (`< <= > >=`) or `lt` because
built-ins cannot be redefined and each implementation delegates to them. The scalar comparison operators
themselves are built-in methods per receiver type ([Numbers](numbers.md), [Strings and characters](collections.md)).
Inside a bound, writing the operator is read as the trait's method (chapter 3). `=` becomes `equal`, and
`/=` becomes `(not (equal a b))`.

## 3. Arithmetic traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

A layer for generic code to require "a type that can be added". **Arithmetic on concrete types uses the
built-in operators** ([Numbers](numbers.md)) and does not go through this layer.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; no methods; a combination of five
```

**Inside a bound, you can write operators.** When the receiver is a type variable bound by `:where`,
operators are read as trait methods (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

The trait method is not named `+` because `+` is the name of a built-in method and `impl` refuses to
redefine it (`cannot redefine built-in method`). There is no `Neg`: `(- x)` expands to
`(- (- x x) x)`, so `Sub` is enough.

Implemented for: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` on all numeric types (except `complex`), and
`Bits` on all integer types and `int`.


**`Div` names its result type with the associated type `Output`.** As in CL, `/` on two integers is a
`rational`, so the `Output` of `int` is `rational`, and that of every other numeric type is the type
itself. Because `Output` differs between the types, `Number` does not include `Div` (a supertrait list
must pin every associated type). A generic function that divides takes the result type with
`(:where (Div T (Output R)))`, or asks for division closed in `T` with `(:where (Div T (Output T)))` (an
`int` cannot be passed then).

## 4. The `Push` trait (adding elements)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

A collection that elements can be added to one at a time. A generic function asks for "a type `T`
can be added to" with `(:where (Push C (Item T)))`, and adds to any such collection with the same
`(push c x)`.

| Type | What `push` does |
|---|---|
| `Vector<T>` | Appends to the end |
| `Deque<T>` | Appends to the end (`push-back`) |
| `Array<T>` | Adds at the fill pointer (`push-extend`). Panics without a fill pointer |
| `HashSet<T>` | Puts it in the set (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

A type of your own that implements `Push` can be passed to the same generic functions. It can also
be a trait object, as in `:dyn Push<int>`.

## 5. The `Pop` trait (taking elements out)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

A collection the element `push` added last can be taken back out of. `none` when it is empty. A
generic function asks for one with `(:where (Pop C (Item T)))`.

| Type | What `pop` does |
|---|---|
| `Vector<T>` | Takes out the last element |
| `Deque<T>` | Takes out the last element (`pop-back`) |
| `Array<T>` | Takes out the element just before the fill pointer. Panics without a fill pointer |

A `HashSet<T>` has no "last element", so it is a `Push` but not a `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

As with `Push`, a type of your own can implement it, and it can be a trait object, as in
`:dyn Pop<int>`.
