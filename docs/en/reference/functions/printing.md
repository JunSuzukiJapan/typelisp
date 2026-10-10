<!-- translated-from: docs/ja/reference/functions/printing.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Printing

`print`/`println`/`format`, the one-argument printers, the pretty printer, `print-object`, and the
variables that control printing. The list of format directives is in [format.md](format.md). Reading
from and writing to streams is in [Streams and Files](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` are all **special forms that interpret format directives (CL's `format`
directives)**. The first argument (the second for `format`) is the **control string**, and each
directive consumes the following variadic arguments in turn.

| Name | Form | Type | Description |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expands the control string and writes it to standard output without a newline |
| `println` | `(println control args...)` | `(string, ...)→Unit` | The same, with a newline at the end |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL's `format`. Returns the expanded string. If `dest` is `true` (CL's `t`), it is also written to standard output; if `false` (CL's `nil`), it is not written and only returned |
| `format` (to a stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | If `dest` is not a `bool`, it is CL's stream destination. The expanded string is written to that stream. The return value is `()` (CL's `nil`), and no string is returned |

The type of `dest` splits the meaning in two (which one applies is decided statically). The stream
form can be written the same way with a concrete stream type, a `:dyn CharOutput`, or a type variable
bound by `(where (CharOutput S))`. A `dest` that is neither a `bool` nor a stream is a type error.

**The control string must be a literal** (the same restriction as Rust's `format!`). The directives in
it decide how many arguments are taken and of which types, so a string built at run time cannot be
read at check time. Because it must be a literal, **the number and types of the arguments are checked
at check time**: `(println "~d" "x")` and `(println "~a ~a" 1)` are check-time errors. A misspelled
directive, an unclosed `~(`, and a `~/name/` that no argument can answer are check-time errors too.
The checking rules are in [format.md](format.md#1-how-to-write-directives). To print a string you
build, make it with `(format false ...)` and print it with `(println "~a" s)`.

The variadic arguments are wrapped into `Sexpr` with their own types before being passed: `i32`/`f64`/
`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, as well as user-defined `defstruct`/`defenum`/
`Vector<T>`/`HashTable<K,V>` and the like, can all be passed as they are (`(println "~a" my-struct)`
just works).

Running a script with `typl file.typl` **does not print the values of top-level expressions**, so a
program writes to standard output by calling these. `print`/`println`/`format` send their output out
on every call (so that a prompt is visible before standard input is read, even through a pipe).

**`Option<Sexpr>` prints transparently.** The type of S-expression data is `Option<Sexpr>`, so the
`(some x)` wrapper does not appear in the output and the contents are printed as they are. The empty
list prints as `()`. Other `Option<T>` print as `(some ...)` / `none`. The same goes for `Option<T>`
fields inside structs, enums and `Vector`s. A `Result<Option<Sexpr>,…>` from `(eval ...)` prints as
`(ok 42)`, or `(ok ())` for `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; get only the string, without printing
  (println "~a" s))                   ; => id=42
```

## 2. One-argument printers

The printers of CLHS 22.1.3. Instead of expanding a format, they print a single value as it is. The
stream can be left out (it defaults to `*standard-output*`).

| Name | Form | Description |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Writes in a form that can be read back (the same as `~s`) and returns `x` |
| `princ` | `(princ x [stream])` | Writes in a form for people (the same as `~a`) and returns `x` |
| `write` | `(write x [stream])` | `prin1` if `*print-escape*` is true, `princ` if false. Returns `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Returns a string instead of writing (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | The same (`~a`). The same as `to-string` |
| `write-to-string` | `(write-to-string x)` | The same, following `*print-escape*` |

`print`/`println` are **not** among these. They are shorthands for `format` that take a control
string, a different job from CL's `print` (newline, then `prin1`, then a space), so each keeps its own
name. As a result, **CL's one-argument `print` has no spelling in this language**: write `prin1`.

These are macros, because the variadic arguments of `format` do not accept type variables and the
type has to be known at the call site.

## 3. Standard input and the standard streams

**Reading standard input** is done not with dedicated functions but with the `CharInput` methods on
the standard stream `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([stream methods](streams-files.md#2-methods)). Standard output and
standard error likewise have `*standard-output*` / `*error-output*`, and can be written as in
`(write-line *standard-output* s)` (`print`/`println`/`format` are shortcuts for when you need format
expansion, and always write to standard output).

## 4. The pretty printer

This corresponds to CL's Lisp Pretty Printer (CLHS 22.2). **It breaks output that does not fit within
the line width, following logical blocks and conditional newlines.**

### 4.1 Control variables

Global variables that can be assigned. Once `setf`, they affect all later printing. To change one
temporarily, use `dlet` (6.3).

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | If true, `~a`/`~s`/`~w` and the pretty directives take the pretty-printing path |
| `*print-right-margin*` | `int` | `80` | The right margin (in columns). 0 means "no margin, never break". A negative value is a print error |
| `*print-miser-width*` | `int` | `0` | The width at which miser style starts. 0 corresponds to CL's `nil` (miser style off). A negative value is a print error |

The `pprint` family and `pprint-logical-block` always pretty-print regardless of `*print-pretty*`
(following the definition of CL's `pprint`).

### 4.2 Ready-made layouts (special forms)

Like `print`, these are special forms, so the argument can be of any type.

| Name | Form | Description |
|---|---|---|
| `pprint` | `(pprint x)` | Pretty-prints with the default layout. As in CL, it **writes a newline first** and none at the end |
| `pprint-fill` | `(pprint-fill x)` | Fills each line with as much as fits. Writes no newline |
| `pprint-linear` | `(pprint-linear x)` | If not all elements fit on one line, **one element per line**. Writes no newline |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | A table with columns `colinc` wide (default 16). Writes no newline. A negative `colinc` is an error |

The default layout (`pprint`, and `~a` under `*print-pretty*`) follows CL's default
`*print-pprint-dispatch*`: it abbreviates `(quote x)` as `'x`, and formats code forms such as
`defun`/`let`/`if`/`lambda` as "the head and the prescribed number of arguments on the first line,
and the rest of the body indented two columns, one form per line". Other lists are filled.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Building logical blocks yourself

| Name | Form | Description |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | A special form that opens a logical block. `obj` is the list `pprint-pop` walks (`()` if none is walked). `:prefix` and `:per-line-prefix` are mutually exclusive (as in CL) |
| `pprint-newline` | `(pprint-newline kind)` | A conditional newline. `kind` is `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Indentation. `kind` is `:block` (from the start of the block) / `:current` (from the current column) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | A tab. `kind` is `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` and `colinc` are non-negative (an error if negative) |
| `pprint-pop` | `(pprint-pop)` | Takes the next element from the block's list (`()` if it is exhausted) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Whether the list is exhausted |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | If it is exhausted, `break`s out of the enclosing `loop` (a macro) |

Logical blocks do not take a stream argument: **an open logical block is implicit state**. The
outermost `pprint-logical-block` starts it, and when it closes, the whole thing is formatted and
written to standard output at once. While it is open, the output of `print`/`println`/
`(format true ...)`/`pprint` all goes into that block, so **you write the contents with ordinary
`print` and mark only the places to break with `pprint-newline` and friends**, which makes the code
look almost the same as in CL.

In CL, `pprint-exit-if-list-exhausted` is a non-local exit from `pprint-logical-block`; here it is
**a `break` from the enclosing `loop`** (`pprint-logical-block` does not establish a `block`). CL's
idiom always puts it inside a `loop` anyway, so it reads the same.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Rules for conditional newlines (CLHS `pprint-newline`):

- `:mandatory` always breaks.
- `:linear` breaks if the enclosing logical block does not fit on one line. The decision is per block,
  so **all the `:linear` newlines of one block break together** (this is the "all on one line or one
  element per line" of `pprint-linear`).
- `:fill` breaks if (a) the next section does not fit in the rest of the line, (b) the previous
  section did not fit on one line, or (c) in miser style, the block does not fit on one line.
- `:miser` works as `:linear` only in miser style (when the block starts within
  `*print-miser-width*` of the right margin).

## 5. `print-object` (per-type printed representation)

Writing `impl print-object <type>` makes `print`/`println`/`format`/`pprint` print values of that type
with that implementation, **even when they are nested inside lists**. It corresponds to CL's generic
function `print-object` (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Meaning |
|---|---|
| `self` | The value to print |
| `escape` | CL's `*print-escape*`. `true` for `~s`/`prin1`/`pprint` (a form that can be read back), `false` for `~a`/`princ` (for people). An implementation that does not care may ignore it |

The returned `string` goes straight into the output. Types without an `impl` print in the built-in
representation (of the form `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   works when nested too
```

It combines with the pretty printer as well (chapter 4). If `*print-pretty*` is true, a list
containing the strings the implementation returned is broken at the right margin.

The printed representations of the standard library's types. Types that also exist in CL print the
same way as in SBCL. When the REPL shows a result, it uses the same representation as `~s`.

| Type | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| Tuple `#{..}` | `#{1 "a"}` | `#{1 a}` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | The same |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (the number is an internal serial number) | The same |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | An integer (the value of CL's `get-universal-time` / `get-internal-real-time`) | The same |
| Error types (`ParseIntError`, `SimpleError` and so on) | `#<simpleerror "boom">` | The message only (`boom`) |
| `complex` | `#C(1.0 2.0)` | The same |
| `Array<T>` | `#2A((0 0) (0 0))` | The same |
| Streams | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | The same |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | The same |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` at the end during daylight saving time) | The same |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | The same |
| `defstruct` types | `#<point x: 1 y: 2>` (field names and values) | The same (fields with `~a`) |

Rules:

- **Registration is static.** An `impl` is type-checked as an ordinary method definition, so a
  misspelled type name or a wrong signature is a compile error.
- **It works for generic types too.** `(impl print-object box<T> (where (print-object T)) ...)` goes to
  a separate body for each type argument: a value remembers its type including its type arguments
  (`box<i32>`). Built-in generic types such as `Vector<T>` work the same way.
- **The choice is made at print time.** Which directive consumes which argument depends on the
  run-time contents of the control string, so the distinction between `~a` and `~s` (that is,
  `escape`) is known only at the moment of printing. This is the same as CLOS, where `print-object`
  methods are "defined per class and chosen at print time".
- **Re-entry falls back to the built-in representation.** If an implementation prints itself with
  `(format false "~a" self)`, it would recurse forever, so when a value being printed appears again,
  the built-in representation is used. This looks at value identity, not a depth limit, so it does not
  get in the way of legitimately printing nested self-referential structures.
- **Every scalar type implements this trait.** This is **so that it can be used as a bound**: the
  variadic arguments of `format` cannot take type variables, so this bound is the only way generic
  code can say "values of an unknown type may be rendered" (the same shape as Rust's `T: Display`).
  The `print-object` of `Array<T>` is an example.
- **With type arguments that do not meet the bound, the built-in representation is used silently.**
  `(impl print-object Array<T> (where (print-object T)))` applies to `Array<i32>`, but not to an
  `Array` whose elements are a `defstruct` without a `print-object`. It would make no sense for merely
  creating an array to be an error, so it is not an error.
- CL's other mechanism, `set-pprint-dispatch` / `*print-pprint-dispatch*` (a run-time registry keyed by
  type specifiers), **is not adopted**. Its registrations are unchecked, which does not fit a statically
  typed language.

## 6. Controlling how much is printed

### 6.1 Depth, length and sharing

The control variables of CLHS 22.1.1 that decide "how much of a value is printed". Like the three in
4.1, they are assignable globals, and they apply to all of `print`/`println`/`format`/`pprint`,
whether `*print-pretty*` is true or not.

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Objects nested at this depth or deeper are replaced by `#`. The object being printed is at depth 0. 0 means unlimited |
| `*print-length*` | `int` | `0` | Prints list elements (and the fields of `defstruct`/`defenum` values) up to this count and replaces the rest with `...`. 0 means unlimited |
| `*print-circle*` | `bool` | `false` | If true, the value is scanned before printing and **objects that appear twice or more get labels**. The first occurrence is `#n=…` and later ones `#n#` |

CL uses `nil` for "unlimited", but this language has no `nil`, so as with `*print-right-margin*`,
**0 means unlimited**. Negative values have no meaning and are print errors. The defaults are all
"no limit / no labels", matching CL's initial values.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Circular structures can be printed only when `*print-circle*` is true.** If you print a value that
points to itself while it is false (the default), the printer keeps following the cycle and the process
crashes. CL is the same (CLHS leaves printing circular structures undefined when `*print-circle*` is
false).

A cycle can only be made by "pointing a `defstruct` field at itself with `setf`" (`Sexpr` cells cannot
be changed after creation, so a list like `'(1 2 3)` can never be circular):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a points to a itself
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Labels **start again from 1 for each thing printed** (as in CL). Even without a cycle, if the same
object appears twice it gets `#1=`/`#1#`, keeping in the output the information that "these two are
the same object", as CL specifies:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

A value with no sharing **shows no labels at all**, so leaving this variable true does not change the
output of everyday code.

### 6.2 Base, case and readability

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `*print-base*` | `int` | `10` | The base for printing integers (fixed-width and `int`). Outside 2 to 36 it is a **print error** (CL also specifies the range) |
| `*print-radix*` | `bool` | `false` | If true, adds a radix marker: `#b`/`#o`/`#x`, `#NNr` for other bases, and a trailing `.` for base 10. The marker goes **before** the sign (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | The case of symbol names: `:upcase` / `:downcase` / `:capitalize` (the same spellings as CL). Any other symbol is a print error |
| `*print-readably*` | `bool` | `false` | If true, prints in a form that can be read back. It forces escaping and disables the cut-offs of `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | The number of lines the pretty printer may use. The excess is cut, with `..` at the end as in CL. 0 means unlimited. A negative value is a print error |
| `*print-escape*` | `bool` | `true` | Whether `write`/`write-to-string` do `prin1` or `princ`. **Only those two read it** |
| `*print-array*` | `bool` | `true` | Whether `Vector<T>` and `Array<T>` show their contents. If true, CL's array syntax (`#(1 2 3)` / `#2A((1 2) (3 4))`); if false, just the type and shape, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

The markers `*print-radix*` adds can be read back by the reader (the radix notation in the
[Syntax Reference](../syntax.md#1-lexical-elements)).

**Why the default of `*print-case*` differs from CL**: CL's default is `:upcase` because CL's reader
stores symbol names in upper case, that is, it means "as stored". This reader stores them in lower
case, so the default with the same meaning is `:downcase`.

**The missing half of `*print-readably*`**: CL signals `print-not-readable` for values that cannot be
read back, but this language has no condition to signal, and no way to decide readability for user
types, which `print-object` may print in any way. Only the forced escaping and the overriding of the
cut-offs are there.

**Why only `write` reads `*print-escape*`**: as CLHS specifies, `~s`/`prin1`/`pprint` bind it to true,
and `~a`/`princ` to false, each only for the duration of their own call. So the only readers that see
it unbound are `write`/`write-to-string`. An implementation of `print-object` should read its own
`escape` argument rather than this global: that argument carries the value the directive chose.

**What CL has and this language does not**: `*print-gensym*` (there are no uninterned symbols).

### 6.3 Temporary overrides

CL binds these with `let`, but `let` in this language binds lexically, so use `dlet`
([Other](system.md#10-other)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; the limits apply to this one print only
(with-standard-io-syntax (println "~a" x))   ; print with everything back at the standard values
```

`with-standard-io-syntax` runs its body with all the printer control variables at their standard
values and `*read-eval*` set to `true`.
