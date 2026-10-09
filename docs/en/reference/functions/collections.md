<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Strings, Characters and Collections

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` and `BitVector`.

## 1. Strings `string`

Strings are immutable.

| Name | Form | Type | Description |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Converts to upper case (ASCII only). Like CL's `string-upcase`, returns a new string. Strings are immutable, so there is no destructive `nstring-upcase`; this takes its place |
| `downcase` | `(downcase s)` | `string→string` | Converts to lower case (ASCII only). Takes the place of `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Upper-cases the first letter of each word and lower-cases the rest (CL's `string-capitalize`). A word is a maximal run of letters and digits |
| `length` | `(length s)` | `string→int` | Number of characters |
| `ref` | `(ref s i)` | `(string,int)→char` | Character `i`. Panics out of range |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | The substring `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Concatenation. Three or more can be given (the same as `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Lexicographic comparison |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Strict lexicographic less-than (the same as `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Identity comparison (whether they are the same object, not the same contents) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Compares contents (case-sensitive) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Compares contents (case-insensitive, ASCII only) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Whether the contents differ (CL's `string/=`. The variadic form compares adjacent pairs) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Case-insensitive ordering (CL's `string-lessp` and so on). With a common prefix, the shorter is smaller |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | A string of `n` copies of `c` (CL's `make-string`) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | The position where `sub` first appears. **CL's `search` has the arguments the other way round** (`(search pattern sequence)`). The empty string is found at 0. For keywords, see [keyword arguments of sequences](sequences.md#6-keyword-arguments) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | The first position where they differ. `none` only when they are `equal`. If one is a prefix of the other, the end of the shorter one. Keywords as above |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Removes characters contained in `bag` from both ends / the left / the right (CL's `string-trim` and so on). Without `bag`, whitespace `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Splits at `sep`. CL has no counterpart. Consecutive separators produce empty elements. Panics if `sep` is empty |
| `to-string` | `(to-string x)` | `T→string` | Converts to a string as `~a` does. Implemented for `int`/`i32`/`f64`/`bool`/`char`/`string` (CL's `princ-to-string`) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Encodes as UTF-8 (each element 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Decodes. `none` if it is not valid UTF-8 |

## 2. Characters `char`

A `char` is a Unicode scalar value. Case conversion and classification handle only the ASCII range.

| Name | Form | Type | Description |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Converts to upper case (ASCII only) |
| `downcase` | `(downcase c)` | `char→char` | Converts to lower case (ASCII only) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Comparison by code point |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Strict less-than by code point (the same as `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Whether it is an ASCII letter |
| `digitp` | `(digitp c)` | `char→bool` | Whether it is an ASCII digit |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Compares values |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Compares values ignoring case (CL's `char-equal`) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Whether the values differ (CL's `char/=`. **The variadic form compares adjacent pairs**, unlike CL, which asks whether all pairs differ) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Case-insensitive ordering (CL's `char-lessp` and so on) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Upper case / lower case / has case distinctions at all (CL's `upper-case-p` and so on) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | A letter or a digit (same name as in CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Whether it is printable. Includes space, not newline or tab (CL's `graphic-char-p`) |
| `standardp` | `(standardp c)` | `char→bool` | Whether it is one of CL's 96 standard characters, that is `graphicp` plus newline (CL's `standard-char-p`) |
| `char->int` | `(char->int c)` | `char→int` | The Unicode scalar value (the reverse is `int->char`/`try-int->char` in [Numbers](numbers.md#1-fixed-width-integers)). Corresponds to CL's `char-code`/`char-int` |
| `char->string` | `(char->string c)` | `char→string` | A one-character string. CL's `string` function covers this by taking a designator, but this language has no designators, so the direction is in the name |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | The **weight** of the digit in that radix (CL's `digit-char-p`). `digitp` is a separate function returning `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | The character for weight `w`. Upper case for 10 and above (CL's `digit-char`; the radix is at most 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | The character's name. Only the named characters the reader can read have names (CL's `char-name`) |
| `name->char` | `(name->char s)` | `string→Option<char>` | The character for a name. Case-insensitive, and also accepts the reader's aliases (`linefeed`/`null`) (CL's `name-char`) |

There is no constant corresponding to `char-code-limit` (the upper limit of `char` is set by Unicode,
not by the language).

## 3. `Vector<T>`

A growable array.
A value can be written `#(1 2 3)` ([Syntax Reference](../syntax.md#1-lexical-elements); the element
type comes from the context or the first element, and each evaluation makes a new vector). It also
prints as `#(1 2 3)`.

| Name | Form | Type | Description |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Makes an empty vector. The type argument comes from the expected type, so in a bare `let` write `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copies of `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Appends to the end |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Reads element `i`. Panics out of range |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Changes element `i`. Panics out of range. Can also be written `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Number of elements |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Removes the last element and returns it. `None` if empty (unlike `get`/`set`, it does not panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Makes an iterator that implements `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Appends `x` if no equal element exists (CL's `pushnew`. It does not need to rewrite a place, so it is a method rather than a macro) |

`map`/`filter` and friends are [sequence functions](sequences.md#4-sequence-functions-on-iter): pass the
vector through `iter`, as in `(map (iter v) f)`. Destructive operations (`nreverse`, `delete` and so
on) are in [Destructive operations](sequences.md#7-destructive-operations).

## 4. `HashTable<K,V>`

| Name | Form | Type | Description |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Makes an empty table |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Lookup |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Insert or overwrite |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Removes the entry and returns the old value, if any |
| `count` | `(count h)` | `HashTable<K,V>→int` | Number of entries |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Removes everything |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | A snapshot of the keys |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | A snapshot of the values |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | A snapshot of the `(k . v)` pairs |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | An iterator implementing `Iter`. The elements are `(k . v)` `cons-cell`s. Corresponds to CL's `with-hash-table-iterator`; `doiter`/`map`/`filter` and others work on it as they are |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL's `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL's `hash-table-size`. In this table it is the number of occupied entries (equal to `count`) |

**Any type that implements `Hash` can be a key**, including `defstruct`/`defenum` types. `get`/`set`/
`remove` carry `(where (Hash K))`, so a table keyed by a type that does not implement it is a **type
error** (`f64` has no `Hash` because of `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Implemented for: `int` and the six fixed-width integers, `bool`, `char`, `string` and `symbol` (not for
floating-point numbers). For your own types, keep the result non-negative by `logand`-ing it with
`*sxhash-mask*` (2^30-1). To hash a string, you can call `(sxhash-string s)` (32-bit FNV-1a), which the
implementation for `string` uses.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Whether two keys are the same is decided by **the key type itself** (`sxhash`, and `equals` from `Eq`,
the supertrait of `Hash`), not by object identity. That is why, as above, you can look up with a key
that is "a different value but equal".

It is fine for `sxhash` to collide (the contract of `Hash` only goes one way: equal values must have
the same hash). Colliding keys are told apart by `equals`.

## 5. `Array<T>` (multidimensional arrays)

A `defstruct` in the standard library. It is not a built-in type, so everything you can do with a
`defstruct` can be done with it.
A value can be written `#2A((1 2) (3 4))` ([Syntax Reference](../syntax.md#1-lexical-elements)).

| Name | Form | Type | Description |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL's `make-array`. `dims` is copied. `init` is the initial value of every cell (CL's `:initial-element`; this language has no "unbound cell", so it is required). `:fill-pointer` only for one dimension |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL's `aref` / `(setf (aref …))`. Panics if an index is out of range |
| `aref` | `(aref a i j …)` | — | CL's spelling with bare indexes. Expands to `get`/`set` above. `(setf (aref a i j) v)` works too |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL's `row-major-aref`. A flat index |
| `rank` | `(rank a)` | `Array<T>→int` | CL's `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL's `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL's `array-dimensions`. Returns a **copy**, just as CL returns a fresh list |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL's `array-total-size` (the number of allocated cells, unrelated to the fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | CL's `length` on arrays. The fill pointer if there is one, otherwise `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL's `array-in-bounds-p`. False (not an error) even when the **number** of indexes is wrong |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL's `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL's `adjust-array`. The rank cannot change. Elements that stay in range are kept at their indexes, and new cells get `init`. Unlike CL, it does not return the array (every array in this language is adjustable, so there is no second array to return) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL's `vector-push-extend`. Panics without a fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL's `vector-pop`. `none` if empty |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | The fill pointer (`none` if there is none). Can be written with `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | An iterator in row-major order. Stops at the fill pointer if there is one |

- **Indexes are a `Vector<int>`.** A method cannot declare "the same type of argument repeated any
  number of times at the end", and the `aref` sugar bridges that gap.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **do not exist**. The static type of the receiver already answers these questions.
- `Array::new` is the field-order constructor generated by `defstruct` and is not meant for creating
  arrays. Use `Array::make`.
- **Arrays print in CL's array syntax.** Rank 1 is `#(1 2 3)`; other ranks are `#nA` followed by that
  many levels of parentheses (`#2A((1 2 3) (4 5 6))`); rank 0 is `#0A5`. Printing stops at the fill
  pointer if there is one. Setting `*print-array*` ([Printing](printing.md#6-controlling-how-much-is-printed))
  to false prints just the shape, `#<array 2x3>`. Only an array whose elements are a `defstruct`
  without a `print-object` prints in the built-in form `#<array<...> ...>` (it is not an error).

## 6. `BitVector` (bit vectors)

A fixed-length sequence of bits. A `defstruct` in the standard library.

| Name | Form | Type | Description |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Length `n`, all bits 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic out of range |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL's spellings. `(setf (bit v i) b)` works too. CL's `sbit` differs from `bit` only in requiring a simple bit vector, but this language has only one kind of bit vector |
| `len` | `(len v)` | `BitVector→int` | Number of bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Return a new bit vector. Panic if the lengths differ. There is no third argument as in CL (the destination of the result) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complement |

There is no `bit-vector-p` (the static type answers it).
