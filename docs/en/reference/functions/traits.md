<!-- translated-from: docs/ja/reference/functions/traits.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# Standard Traits

The traits for iteration, comparison, arithmetic and adding elements. The other standard traits are in their own
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
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implement `Iter` through `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` respectively (get the iterator with `(iter collection)`).
`Chan<T>` is itself an `Iter` (`recv` plays the role of `next`; [Channels](concurrency.md#2-chant--channels)).
`Sexpr` lists do not implement `Iter` (their element types are not uniform). If you implement `Iter`
for your own type, it can be walked with `doiter` as it is, and passed to the
[sequence functions](sequences.md#4-sequence-functions-on-iter).

## 2. `Eq` / `Ord` (comparison)

These correspond to Rust's `PartialEq`/`PartialOrd` (named `Eq`/`Ord`). They are used in the `:where`
bounds of generic functions to require that element types can be compared (`sort`/`member`/`assoc`
and so on).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

To implement `Eq` you only write `equals`, and for `Ord` only `less`. The default implementations
fill in the rest. `Ord` inherits from `Eq`, so `impl Eq X` is needed before `impl Ord X`.

Each trait method can be called as a function as it is (inside a `where (Eq A)`/`(Ord A)` bound, or on
a concrete type that implements it):

| Name | Form | Type | Description |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Whether they are equal (Rust's `==`) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Whether they are not equal (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` is implemented for: all numeric types (`i8` to `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, that is identity; used by the value patterns of
`match`), and `cons-cell<A,B>` (recursively, when the elements are `Eq`). `Ord` is implemented for:
all numeric types, `char` `string`, and `cons-cell<A,B>` (lexicographically, when the elements are
`Ord`).

The method names do not overlap the built-in operators (`= /= < <= > >=`) or `eq`/`lt` because the
built-ins cannot be redefined, and each implementation delegates to them. The scalar comparison
operators themselves are built-in methods of each receiver type ([Numbers](numbers.md),
[Strings and Characters](collections.md)). Inside a bound, writing the operators reads them as the
trait methods (chapter 3).

## 3. Arithmetic traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

A layer for generic code to require "a type that can be added". **Arithmetic on concrete types uses the
built-in operators** ([Numbers](numbers.md)) and does not go through this layer.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (:type Output) (div ((self Self) (other Self)) Output))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; no methods; a combination of five
```

**Inside a bound, you can write operators.** When the receiver is a type variable bound by `:where`,
operators are read as trait methods (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

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
  (push ((self Self) (x Item)) ()))
```

A collection that elements can be added to one at a time. A generic function asks for "a type `T`
can be added to" with `(:where (Push C (Item T)))`, and adds to any such collection with the same
`(push c x)`.

| Type | What `push` does |
|---|---|
| `Vector<T>` | Appends to the end |
| `Deque<T>` | Appends to the end (`push-back`) |
| `Array<T>` | Adds at the fill pointer (`push-extend`). Panics without a fill pointer |
| `HashSet<T>` | Puts it in the set (`insert`). A `Push` only when the element type implements `Hash` |

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
