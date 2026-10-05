<!-- translated-from: docs/ja/reference/functions/sequences.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Pairs, S-Expressions and Sequences

The generic pair `cons-cell`, S-expression data `Sexpr`, symbols, the sequence functions written on
top of `Iter`, and higher-order functions.

## 1. Pairs `cons-cell<A,B>`

`cons`/`car`/`cdr` are the constructor and field accessors of the **generic pair type
`cons-cell<A,B>`** (a `defstruct` in the standard library). The fields can be read either as
`variable::car`/`variable::cdr` (the `defstruct` accessor syntax of the
[Syntax Reference](../syntax.md#36-defstruct--structs-user-defined-types)) or as
`(car variable)`/`(cdr variable)`. To change them, use `(setf variable::car v)`/`(setf variable::cdr v)`.

| Name | Form | Type | Description |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Makes a pair |
| `car` | `(car p)` | `cons-cell<A,B>→A` | The first element |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | The rest |

`cons-cell` also serves in place of a tuple syntax. CL functions that return multiple values (the
quotient and remainder of `floor`, the value and position of `read-from-string` and so on) return a
`cons-cell` in this language.

## 2. S-expression data `Sexpr`

The data type `Sexpr` returned by `read` has 16 variants:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path`.
S-expression cells are handled not by the general `cons`/`car`/`cdr` of chapter 1 but by the
`sexpr-*` functions. They are used mainly in `defmacro` bodies to build and take apart forms.

**The type of S-expression data is `Option<Sexpr>`.** The empty list is not a variant of `Sexpr` but
the `none` of `Option`, and `Sexpr` itself means "a non-empty S-expression". So the `sexpr-*`
functions take and return `Option<Sexpr>`.

- `()` is the empty list where an `Option<Sexpr>` is expected (it can also be written
  `(Option::none)`)
- `Sexpr` widens implicitly where an `Option<Sexpr>` is expected (with no run-time conversion). The
  opposite direction, using an `Option<Sexpr>` as an `Sexpr`, claims "this is not the empty list",
  so it has to be stated explicitly with `match` or `unwrap`
- In `match`, the 16 variants of `Sexpr` and `none` can be written **flat in the same list of arms**
  ([Syntax Reference](../syntax.md#43-match--pattern-matching))

| Name | Form | Type | Description |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Makes an `Sexpr` cell |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | The first element. **The empty list for the empty list** (as in CL). Panics on an atom that is not a `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | The rest. **The empty list for the empty list** (as in CL). Panics on an atom that is not a `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Whether it is a `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Whether it is the empty list |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Whether it is not a `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Whether it is a `Sym` (symbol) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | The contents of the `int` variant (fixnum or bignum). Panics on another type |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | The contents of the variant of that width. Panics on another type |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | The contents of the floating-point variants. Panics on another type |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | The contents of a `Char`. Panics on another type |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | The contents of a `Bool`. Panics on another type |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | The contents of a `Str`. Panics on another type |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | The name of a `Sym`. Panics on another type |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Identity comparison (`Cons`/`Str` compare object identity, the rest compare values) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Structural equality (`Cons` recursively, `Str` by contents) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Like `equal`, plus case-insensitive comparison and comparison of numbers across types |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Concatenates two `Sexpr` lists (non-destructively). `,@` expands into this |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | A new `Sexpr` list with `f` applied to each element of an `Sexpr` list (the `map` of chapter 4 is for `Iter` and cannot walk an `Sexpr` list) |

There are nine numeric accessors, one per type, because an `Sexpr` is "the one place where the type
of a value is written nowhere else". A `u8` put into an `Sexpr` goes in as the `u8` variant and comes
out only with `(sexpr-u8 s)`. Passing it to `(sexpr-int s)` panics; it never silently widens the
answer. The integers in data that was read (`'(1 2 3)`, macro arguments) are of the `int` variant and
are read with `(sexpr-int s)`.

`Sexpr` lists have no destructive operations like `rplaca`/`nconc`. An `Sexpr` cell cannot be changed
after it is made.

## 3. Symbols

`symbol` is the type of symbols themselves. It converts implicitly where an `Sexpr` is required, but
not automatically in the other direction.

| Name | Form | Type | Description |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Takes out the symbol's name |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Makes a symbol from a string (interns it) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Whether it is a keyword (`:name`). The colon is part of the name, so the test looks at the first character ([Syntax Reference](../syntax.md#1-lexical-elements)) |

For `gensym`, see [Macros](system.md#8-macros).

## 4. Sequence functions on `Iter`

The sequence functions are **generic functions over the `Iter` trait**. From a collection, get an
iterator with `(iter coll)` and pass it (`Vector<T>` / `HashTable<K,V>` / `Array<T>` support this; an
`Sexpr` list does not implement `Iter`, so these functions do not apply to it). **A resulting
collection is returned as a new `Vector`.** `Iter<A>` in the tables means "any implementation of
`Iter` whose `Item` is `A`". To walk the returned `Vector` again, pass `(iter result)`.

Functions taking a predicate (corresponding to CL's `-if` family):

| Name | Form | Type | Description |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Mapping |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Only the elements that satisfy the predicate |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Removes the elements that satisfy the predicate |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | The first element that satisfies the predicate |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | The first position that satisfies the predicate |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | How many satisfy the predicate |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Whether every element satisfies the predicate |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Whether any element satisfies the predicate (corresponds to CL's `some`; a name that does not clash with the `Some` constructor) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Left fold |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Right fold |

Indexing, length and slicing:

| Name | Form | Type | Description |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Number of elements |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Concatenates iterators. Three or more can be given |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL's `concatenate`. The result type is written as **a quoted symbol literal** (CL uses a run-time type specifier). `'vector` takes one or more, `'string` zero or more (`""` for zero). `Sexpr` lists are not covered (use `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Reversal (non-destructive) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Element `n` (`None` out of range) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` with the arguments the other way round |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | The first `n` elements |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` is clamped to the length) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | The last **element** (not "the last cell" as in CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | All but the last element |

Functions requiring an `Eq` / `Ord` bound (they compare through a trait instead of a predicate;
[Standard Traits](traits.md#2-eq--ord-comparison)):

| Name | Form | Type | Description |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Whether an element equal to `x` exists (unlike CL, a `bool`, not the rest of the list) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | The first element equal to `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | The first position equal to `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | How many elements equal `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL's `(sort sequence predicate)`. A stable, non-destructive sort. `cmp` is `true` when "the first argument comes strictly before the second" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | The first pair whose `car` equals `k`. Take the value out with `(cdr p)` |

These and many of the functions of chapter 5 also take CL's keyword arguments `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (chapter 6).

## 5. The rest of CL's sequence functions

All are generic functions on `Iter`, as in chapter 4. Resulting collections are returned as new
`Vector`s.

| Name | Form | Type | Description |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL's named indexes |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | All but the first (a new `Vector`, not a shared tail) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Materializes an iterator into a `Vector` (CL's `copy-seq`/`copy-list`) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` reversed, followed by `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copies of `x` (CL's `make-list`/`make-sequence`). As with `Vector::new`, the type argument comes from the expected type, so a bare `let` needs `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Like `member`, a **`bool`** (an iterator has no tail to return) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | The negations of `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Same types as the positive versions | Versions with the predicate negated |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Removes by value |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Removes duplicates. As in CL, **the last occurrence is kept** (`:from-end true` keeps the first) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Replaces by value / predicate |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | on `Iter<cons-cell<K,V>>` | The predicate and value-side versions of `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Adds a pair at the front |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Pairs up two sequences. Stops at the shorter one |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL's `mapcar` over several sequences. Stops at the shorter one |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Mapping for side effects |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Maps and concatenates |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Maps over successive **tails** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Maps over tails for side effects (the `maplist` counterpart of `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Maps over tails and concatenates (the `maplist` counterpart of `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | The position where `sub` first appears. If the receiver is a `string`, the `string` method is chosen ([Strings](collections.md#1-strings-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | The first position where they differ. `none` if they are equal |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Merging. CL requires sorted inputs; this sorts the concatenation |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Adds `x` **at the front** if it is not there |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Set operations. CL does not specify the order; here it is stable, **in order of first appearance** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inclusion |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Whether it is a suffix / the part before the suffix. CL asks about **shared structure**, but there is no structure to share, so this asks about a suffix **as values** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Element-wise equality. `Vector<T>` itself does not implement `Eq` |
| `caar`…`cddddr` | `(cadr p)` | on nested pairs | CL's 28 functions. They walk **pairs, not lists**: `cadr` takes a `cons-cell<A,cons-cell<B,C>>` |

What CL has and this language does not: `list*` (there is no notion of an improper list whose tail is
replaced), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (no type can describe walking a
heterogeneous tree of arbitrary depth; for a tree of `Sexpr`, `equal` corresponds to `tree-equal`),
the property list family `getf`/`get-properties`/`symbol-plist`/`remprop` (there is no
representation as an untyped list alternating keys and values; `assoc` (association lists) or
`HashTable` fill the same role), and functions converting between `Vector<T>` and `Sexpr` lists (the
elements of an `Sexpr` list can each have a different type, so they cannot be written with a single
element type `T`).

## 6. Keyword arguments

The functions of chapters 4 and 5 take CL's sequence keywords `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. All are **optional**.

| Keyword | Type | Meaning |
|---|---|---|
| `:key` | `(fn (A) A)` | A projection applied to each element before comparing or testing |
| `:test` | `(fn (A A) bool)` | An equality test used instead of `equals` from the `Eq` bound. The first argument is **the item being searched for**, the second is the element (after `:key`), in the same order as CL |
| `:test-not` | `(fn (A A) bool)` | The negation of `:test` |
| `:start` `:end` | `int` | The window `[start, end)` to scan. Indexes are relative to the whole sequence |
| `:from-end` | `bool` | A search answers with the **last** match. Combined with `:count`, the elements affected are taken from the end |
| `:count` | `int` | The maximum number of elements the `remove` / `substitute` families affect |

Which function takes which follows CL:

| Function | Keywords taken |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | All of the above (including `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (the `:key` of `assoc` applies to the `car`, that of `rassoc` to the `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; removes only one, from the end
(position 3 (iter v) :start 1)                          ; the index is relative to the whole sequence
```

**Differences from CL**:

1. **The projection of `:key` stays within the element type** (`(fn (A) A)`). It cannot project to
   another type as in CL: an extra type variable could not be determined when the argument is left
   out. Where a projection to a different type is needed, pass a lambda to the `-if` family instead
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **In item-based searches, `:key` applies only to the elements** (not to the item searched for).
   This is the same rule as CL's `find`/`position`/`count`/`member`/`remove`/`substitute`. In set
   operations both sides are elements, so it applies to both.
3. **Only the keywords of `search` are named rather than numbered.** In CL, `:start1`/`:end1` are for
   the **pattern** and `:start2`/`:end2` for the sequence searched. In this language the receiver
   comes first, so the same numbers would mean the opposite, and silently at that. `:start`/`:end`
   are for the receiver and `:sub-start`/`:sub-end` for the pattern, so an absent-minded `:start1`
   gives an "unknown keyword" error. `mismatch` and `replace` have the same argument order as CL, so
   they keep CL's numbers.

## 7. Destructive operations

Methods of `Vector<T>`. **They modify the receiver and return the receiver itself**, so `(nreverse v)`
is written the same way as `reverse` and `v` itself is reversed too.

| Name | Form | Description |
|---|---|---|
| `nreverse` | `(nreverse v)` | Reverses in place |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | In-place versions of `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | In-place versions of the `substitute` family |
| `nbutlast` | `(nbutlast v)` | Drops the last element |
| `fill` | `(fill v x)` | Sets every element to `x`. The length does not change |
| `replace` | `(replace v src)` | Overwrites from the front with the elements of `src`. `(min (len v) (len src))` elements |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Same count as above |
| `nconc` | `(nconc v w)` | Appends the elements of `w` to `v`. Unlike CL, **it does not rewrite shared structure** (`w` is not affected) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Replaces the contents of `v` with `src` (the length changes too) |
| `rplaca` `rplacd` | `(rplaca p x)` | Rewrites the `car`/`cdr` of a `cons-cell` and returns the cell itself |

Keywords taken:

| Destructive version | Keywords taken |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (the receiver is CL's `sequence-1`) |

`vector-push-extend`/`vector-pop` are simply `push`/`pop` of `Vector<T>`. A `Vector<T>` always grows,
so nothing corresponds to CL's distinction between "a vector with a fill pointer" and "a simple
vector".

## 8. Higher-order functions

| Name | Form | Type | Description |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Returns its argument |
| `const` | `(const x y)` | `(A,B)→A` | Returns the first argument |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Function composition `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Swaps the arguments of a two-argument function |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negation of a predicate |

There is no CL `constantly` (the type of the ignored argument would appear only in the return type
and could not be determined). Write `(lambda ((x T)) A v)`.
