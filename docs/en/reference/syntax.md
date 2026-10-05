<!-- translated-from: docs/ja/reference/syntax.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# typelisp Syntax Reference

typelisp is a statically typed Lisp, written in S-expressions. For the list of built-in functions and
methods, see [Built-in Functions](functions/README.md); for the list of types, [types.md](types.md);
and for reading error messages, [errors.md](errors.md).

## 1. Lexical elements

- **Case-insensitive.** Symbols are all normalized to lower case when read.
- **Comments**: from `;` to the end of the line (line comments). `#| ... |#` (block comments, which can
  nest).
- **Read-time evaluation**: `#.(expr)` **runs the following form while reading** and treats its value as
  what was read. This is the only place where the reader is more than a function of the text. How far it
  can reach depends on the reading path, as in CL:
  - `(load ...)` and the REPL evaluate one form at a time, so it can call **functions defined earlier in
    the same text** (CL's `load`).
  - A module file is checked as a unit and run by whoever `use`s it, so `#.` can reach only the standard
    library and what the session has already run. Neither the file's own definitions nor those of the
    modules it `use`s **have run yet** (just as CL's `compile-file` needs `eval-when`).
  - `read` / `read-from-string` inside a program evaluate `#.` too (as in CL).
  - Setting `*read-eval*` (default `true`) to `false` makes `#.` a read error everywhere: a switch to keep
    text read as data from running code (as in CL). It is consulted at every `#.`, so a `setf` takes
    effect from the next form read. Inside `with-standard-io-syntax` it is `true`.
- **Booleans**: `true` / `false`.
- **Integers**: decimal (`42`, `-7`). A sign `+`/`-` may come first. Other bases are written with CL's
  radix syntax `#b`/`#o`/`#x`/`#NNr` (the sign goes after the marker: `#x-ff`). The `0x` prefix is not
  in CL and is not adopted: `0xff` reads as a symbol.
  An integer literal without a type annotation is `int` by default (arbitrary precision,
  [Numbers](functions/numbers.md#3-arbitrary-precision-integers-int)), with no upper limit on its size.
  **If the expected type is a fixed-width integer type, the literal takes that type, and it is checked
  that the type can hold the value**: `(the u8 300)` is a type error (if you want it cut down, write
  `(as u8 300)`). `(the u32 4294967295)` and `(the u32 #xFFFFFFFF)` can be written thanks to this rule.
  Whether an `int` value fits in a 63-bit immediate or becomes a bignum is decided by its size, with no
  special syntax (as in CL).
- **Floating-point numbers**: those containing a decimal point or an exponent (`e`/`E`) (`1.5`,
  `3.0e10`). `f64` by default (`f32` if that is the expected type).
- **Ratios**: `numerator/denominator` (decimal only, for example `1/3`). Reduced when read, as CL
  specifies (`2/4` is `1/2`). Those with an integer value (`4/2` and so on) are read as `int`, not
  `ratio`. A zero denominator (`1/0`) is a read error.
- **Characters**: `#\` followed by one character or a character name. For example `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (also `#\Null`) `#\Backspace`. Names are
  case-insensitive.
- **Strings**: `"..."`. The escapes are `\n` `\t` `\r` `\0` `\\` `\"` (any other `\x` is just `x`).
- **Symbols**: any token containing letters, digits and symbols (`+` `<=` `my-func` and so on).
- **Keywords**: symbols starting with a colon, such as `:name` (as in CL). They are self-evaluating:
  they look up no binding and their value is themselves, with static type `symbol`. Keywords with the
  same name are always the same object (`(eq :foo :FOO)` is true; like other symbols they are
  lower-cased). The colon itself is part of the name, so `(symbol->string :foo)` is `":foo"` (typelisp
  has no package system, so this differs from CL's `symbol-name`). A lone `:` or one with additional
  colons such as `:a:b` is a read error. Test with `keywordp`. Ones starting with `::` are not keywords
  but absolute paths (below).
  Note that `:dyn` is a reserved keyword for type positions only; writing it anywhere else is an error
  (see [chapter 2](#2-writing-types)).
- **Lists**: `(a b c)`. Dotted pairs `(a . b)` can be read too.
- **The empty list `()`**: depending on context, the value of the `Unit` type or the `none` of
  `Option<Sexpr>`. **`Sexpr` has no empty-list variant**: `Sexpr` means "a non-empty S-expression",
  and the type of S-expression data is `Option<Sexpr>` (see "Patterns for `Option<Sexpr>`" in
  [4.3 match](#43-match--pattern-matching)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (meaningful only inside a quasiquote)
  - `,@x` → `(unquote-splicing x)` (spliced in as list elements at expansion)
- **Paths `::`**: `foo::bar` is read as a path through modules, types and members (not as a single
  symbol name). One starting with `::`, as in `::foo`, is an absolute path from the root. A `::` inside
  generic arguments (`Vec<a::b>` and the like) is not treated as a path separator.

## 2. Writing types

In source, types are written as ordinary symbols or lists.

- **Primitive types**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` is the integer type (CL's integer, moving automatically between 63-bit immediates and
  bignums; [Numbers](functions/numbers.md#3-arbitrary-precision-integers-int)), and the six fixed-width
  types are named for their width and signedness (there is no 64-bit integer type; see
  [Numbers](functions/numbers.md#1-fixed-width-integers)).
- **The rational type**: `ratio` (rationals in lowest terms). Heap-allocated as in CL, with no implicit
  conversion with `int`/`f64` and the like (convert explicitly with `as`/`try-as` or a conversion
  method; see [Numbers](functions/numbers.md#5-rationals-ratio)).
- **Raw words at the C boundary**: `ptr` (an opaque pointer), `c-long` / `c-ulong`. For the FFI only:
  making one a value requires `(unsafe ...)`, and the places they can appear are limited
  ([3.3 defffi](#ptr--c-long--c-ulong--raw-machine-words)). Do not use these where you want a 64-bit
  integer: they have no arithmetic.
- **Opaque mutable types**: `random-state` (the state of a random number generator). It cannot go into
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (it can go into `Option<T>`/`Result<T,E>`).
- **The Unit type**: `()`
- **The Never type**: `!` (the type of diverging expressions such as `panic`/`unreachable`/`todo`/a loop
  that never returns. It fits any expected type)
- **Function types**: `(fn (argument-types...) return-type)`. The type of a function with variadic
  arguments is `(fn (argument-types... &rest element-type) return-type)`.
- **Generic types**: `Name<T1,T2,...>` (read as a single token without spaces).
  For example `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  The unit type `()` can also be written as a type argument (`Result<(), FileError>`). `(`/`)` are
  normally delimiters that end a token, but while an angle bracket is open, this one pair of characters
  is allowed through. `()` can also be used as a field type or an argument type.
- **The application form of generic types**: `(Name T1 T2 ...)`, a list spelling that names the same type
  as `Name<T1,T2,...>`. For example `(vector char)` is the same as `Vector<char>`.
  The name form is the usual way to write it; this form **exists for when a type argument cannot be
  spelled inside a name**: a type argument is itself a type expression, but inside a single-token name
  only names, `()` and `:dyn` can be written, not function types (there is no such spelling as
  `Vector<(fn (i32) i32)>`). It may also appear in this form when the implementation shows a type, such
  as the result of substituting a trait's associated type into a signature.
- **Qualified type names**: can be qualified with `::`, as in `module::Type`.
- **Trait object types**: `:dyn Trait` (two space-separated words forming one type). Represents a value
  whose concrete type is decided at run time; trait method calls go through a vtable (dynamic dispatch).
  For a trait with associated types, they are fixed positionally in declaration order (`:dyn Iter<i32>`
  fixes `Item` to `i32`). It can also be written inside generic arguments: `Vector<:dyn Drawable>`
  `HashTable<string, :dyn Drawable>`. Concrete values are boxed automatically in expected positions;
  the explicit form is `(as :dyn Trait expr)`.
  A value of `:dyn Sub` can be passed as it is where a `:dyn Super` of any of its supertraits (everything
  it inherits, transitively) is required (upcasting). It cannot be passed to an unrelated trait.
  For the conditions a trait must meet to be used with `:dyn`, see
  [3.9 deftrait / impl](#39-deftrait--impl--traits). Writing `:dyn` outside a type position is an error.
- Built-in generic types: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, and the concurrency types `Task<T>` / `Thread<T>` / `Chan<T>`
  ([chapter 12](#12-concurrency-tasks)). There is also `Sexpr`, the type of S-expression data. The
  built-in concrete error types are `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, and the standard library has the structs `SimpleError` / `WrappedError`
  (`Error` is not a type but a trait: use it as `:dyn Error`). The list is in [types.md](types.md).
- **Types and traits share one namespace** (as in Rust): within one module, a type
  (`defstruct`/`defenum`) and a trait (`deftrait`) cannot have the same name.

## 3. Top-level definitions

### 3.1 defun — function definitions

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- The argument types and the return type are required.
- A generic function writes its type parameters in angle brackets after its name:
  `(defun name<T1,T2...> (params) Ret body...)` (the same angle-bracket syntax as `Vector<T>` in type
  positions).
- `defun`/`lambda`/`defmethod` accept variadic arguments when `&rest (name Type)` is written at the end:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (in the body, `xs` is always bound as an
  `Option<Sexpr>`, an S-expression list. Each actual argument at the call is type-checked as `Type2`
  individually and then wrapped into an `Sexpr`).
  `defmacro` has its own `&rest` too, but differs in that it is always an untyped `Sexpr` (`defun`/
  `lambda` state the element type). A function type can describe a variadic function too, as
  `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (for `defun` and `defmethod`; not for `lambda`/`labels`, for the reason
  below, and `defmacro` has a separate implementation, also below). The order is CL's:
  `required &optional &rest &key`. Each parameter is written `(name Type)` or
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; no default
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> in the body

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; with a default
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; the caller writes `:name value`, in any order; omitted ones take their defaults
  ```

  - **A parameter without a default expression has type `Option<Type>`.** Omitted, it is `none`; passed,
    the bare value the caller wrote is wrapped in `some` automatically. What CL does with a supplied-p
    variable ("was it supplied?") shows up on the side of the static type instead.
  - With a default expression, the type stays `Type` as declared. When omitted, that **checked
    expression** is embedded at the call as it is (evaluated on every call).
  - **`&key` cannot be mixed with `&optional`/`&rest` in one argument list.** This avoids an ambiguity CL
    itself has (whether a trailing actual argument is taken by a positional `&optional` or matched by
    label as a `&key` depends on the *values*) by forbidding the combination. `&optional` and `&rest`
    can be used together.
  - They can be used in generic functions, but **a type parameter that appears only in omitted
    arguments cannot be inferred and is an error** (there is no value to match against).
  - **`defmethod` can have the same three sections** (for both instance methods and static functions).
    List `&optional`/`&rest`/`&key` after the receiver:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; static function
    (point::origin :y 7)
    ```

    They can be used in methods of generic types too, but **the type of a parameter with a default
    expression cannot mention the owner's type parameters** (the same restriction `defun` has for its
    own type parameters: what gets embedded when the argument is omitted is a *checked* expression, so
    its type cannot be left as an abstract variable).
  - **They cannot be used in trait methods.** `deftrait` has no syntax for them, and if only the `impl`
    side could declare sections, calls with a `:dyn` receiver (filling arguments from the trait's
    declaration) and calls with a concrete receiver (filling them from the `impl`'s declaration) would
    become different things. The arity of a vtable slot is fixed.
  - **They cannot be used in `lambda` / `labels`** (`&rest` can). To fill in an omitted argument, the
    caller has to read **the callee's checked default expression**, which is available only from a
    signature resolved by name. A `lambda` is passed around as a value, and the only thing describing
    that value is its function type `(fn ...)`: there is no place in it for an expression, and if there
    were, "two lambdas with the same signature but different defaults" would become different types.
    `&rest` stays within the matter of types, so it can be written in a function type.
- **Forward references are declared with `defsignature`** (below). A name that has not been declared
  cannot be called before its definition, because the top level is checked and run one form at a time,
  in source order.
- To require trait bounds, write a `where` clause right before the body:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (fixing an associated type with `(AssocName ConcreteType)` is optional).
- **Docstrings**: a string literal at the start of the body, right after the `where` clause (if any),
  becomes the docstring (as in CL). Only when at least one body form follows it, though: a lone string
  stays the return value and is not taken as a docstring: `(defun f () string "doc" "value")` has a
  docstring and returns `"value"`, while `(defun f () string "value")` has no docstring and returns
  `"value"`. It can be retrieved with `(documentation name)`
  ([docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — forward declarations

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

To call a `defun` defined **later** than yourself, declare it first like this. Mutual recursion can be
written only this way:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

The arguments are listed as **types only**; there is no body, so there is nothing to give names to.
`&rest` can be written last, as `&rest element-type`.

Declarations **are checked**:

- The definition that follows must match the declaration (the number and types of arguments, the
  return type, `&rest`, and whether it is `pub`). A mismatch is an error at the definition.
- Declaring without defining is an error (reported when the file / module finishes loading). The REPL
  does not report it after each input, because a declaration and its definition should be able to be
  typed on separate lines.
- A declaration placed **after** the definition is an error, since such a declaration could do nothing.

Three things cannot be declared:

- **Generic functions.** Making a copy for each type needs the body, and a declaration has none. A
  forward call could be resolved but instantiation would fail, so the declaration is refused up front.
- **`&optional`/`&key`.** Their signature includes the **checked** expression of each default value
  (embedded at the call when the argument is omitted), and a declaration has no place for it.
- **Anything other than `defun`.** A `defmacro` needs the macro body to have **already run** in order to
  expand, which registering a signature cannot replace. For types (`defstruct`/`defenum`/`deftrait`),
  registering them is "what the code registering the type itself needs", which is not self-contained
  the way a signature is. A `defmethod` is registered on the type that owns it, so it follows the type.

CL's counterpart is `(declaim (ftype (function (i32) bool) even2))`, but that comes with a whole
declaration system and is only **advisory**. Here, with static typing, declarations are checked.

### 3.3 defffi — declaring C functions (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = the C symbol name
(pub defffi ...)
```

Declares a C function so it can be called. The form is the same as `defsignature` (a name, argument
types, a return type and no body), but having no body means something different. `defsignature` is a
promise that "I will define it later", while `defffi` declares that "someone else has already written
and compiled the body".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

The typelisp name and the C symbol name can be written separately because typelisp identifiers usually
contain `-` and C identifiers cannot. If the C name is left out, the name is used as the C symbol name
as it is.

**Calls require `(unsafe ...)`** (even for functions on scalars only). The compiler has no way to
confirm that the declared C signature matches the real one and can only trust the declaration;
`unsafe` is the mark that you take on that responsibility. The intended way is to wrap it once and
make a safe wrapper:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; no unsafe needed from here on
```

The types that can be written are `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, and typed pointers `(ptr T)`
([below](#def-c-struct-and-typed-pointers--allocating-c-structs)).

`string` is `const char *`. typelisp strings are not NUL-terminated and may themselves contain NUL, so
**they are copied into a C string when passed**, and freed after the call. A NUL in the string is an
error: C would only look up to it, so a different string would be passed silently.

**Returned strings are copied too**, and not freed: what C returns belongs to C, and it may point into
a static table, as with `getenv`. Functions that return memory the caller must free (`strdup` and so on)
should be taken as `ptr` and freed yourself.

Functions whose result points inside an argument (`strchr`, `strstr`) also work correctly: the result
is copied before the argument is freed.

If a function declared to return `string` returns NULL, it is an error, because `string` has no value
meaning "there was none". If NULL is possible, take the result as `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

With `:library`, that shared library is opened and the symbol looked up in it. Without it, the symbol is
looked up in **the process itself** (everything already linked, including libc). A short name such as
`sqlite3` is looked up as `libsqlite3.dylib` / `libsqlite3.so` in that order, and a name containing `/`
is treated as a path. Opened libraries are never closed: code pointing at their functions keeps
running, so the only correct lifetime is that of the process.

#### ptr / c-long / c-ulong — raw machine words

`ptr` is an opaque pointer (`void *`, `FILE *`, whatever the declaration meant). `c-long` / `c-ulong` are
C's `long` / `unsigned long` (also `size_t`, `int64_t` and `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Not calling them `i64` / `u64` is deliberate.** This language has no 64-bit integer type, because a
tagged immediate has only 63 bits ([chapter 2](#2-writing-types)). The name `c-long` says "this is a
word crossing the boundary with C, not an integer of this language".

**They have no arithmetic.** `(+ x 1)` cannot be written. It could be provided but is not, so that no
computation runs on a value that cannot be stored anywhere and has a different width from every other
number, for the same reason the 64-bit integer type was left out. There are **only conversions**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; read what came back
(as int (unsafe (c-strlen s)))               ; this one to read it exactly (int does not lose 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; ask whether it fits
(as c-ulong n)                               ; make one from another integer
```

Integer **literals** take the expected type, so no `as` is needed just to pass one:

```lisp
(unsafe (c-malloc 16))                       ; 16 is read as a c-ulong
```

Out-of-range literals are rejected as with other widths (`(c-malloc -1)` does not fit a `c-ulong`).

**The places they can appear are limited**: argument types, return types and local variables only.
Each of the following is an error:

```lisp
(defstruct handle (p ptr))          ; a struct field
(defenum maybe (none) (some ptr))   ; an enum field
(defvar (block ptr) ...)            ; a global
(defffi f ((vector ptr)) i32)       ; inside a type argument
```

There is one reason for all of them: **the slot tags what it holds**. Tagging would drop the top bits of
the pointer, the same reason the 64-bit integer type was left out, so it is not allowed even in
`unsafe`. This is not a matter of permission: that representation does not exist.

For the same reason, they cannot be local variables **captured** by nested functions (a captured
binding goes into a cell, and a cell tags what it holds). This is known at compile time and is reported
by `(compile f)`.

The GC does not trace `ptr`. It points outside the heap, so that is correct.

Four things cannot be declared:

- **Variadic arguments** (`printf`). The variadic part is passed by different rules from the fixed
  arguments (on the stack on AArch64 Darwin), so it cannot be called correctly from a fixed signature.
  `&rest` is rejected.
- **Passing or returning structs by value.** For the same reason (it depends on each platform's
  calling convention). The writable types are limited to the list above, so it cannot be spelled.
- **Generics.** C has no counterpart.
- **The same name as a built-in.** A compiled call would resolve that name to the built-in, so it is
  refused rather than going quietly wrong.

#### Callbacks — having C call back

Writing a function type `(fn (types...) return-type)` as an argument type makes that argument a function
C calls back (a callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; a top-level function
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; a lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; a local function
```

A C function pointer is nothing but a code address, and C calls it passing only the declared arguments.
There is no place to pass captured variables, so **only functions with no free variables can be
passed**, and this is checked at type-checking time.

- Write a function name or a `lambda` expression **directly** as the actual argument. A variable holding
  a function cannot be passed: which function it holds, and so whether it has free variables, is not
  known until run time.
- A `lambda` is an error if it refers to local variables outside it. Global variables and top-level
  functions may be referred to.
- A local function (`labels`) must have no free variables, including those of the sibling functions it
  calls. Sibling functions share the place where captured variables are kept, so what a called sibling
  captures is captured by this function too.
- A generic function gets its types from the declared function type.
- The types that can be written in the function type are the same as the list above. However, `string`
  cannot be the return type of a callback (it would hand C memory no one frees). A `string` argument
  copies the string C passed into a typelisp string.

C function calls can only be written inside `unsafe`, so callbacks can only be passed inside `unsafe`.

**The callback can be called only while the C function that typelisp called is running.** If it is
called from anywhere else (a thread not running typelisp, a signal handler, a function registered with
`atexit`), it prints the reason and stops the process.

**Failures do not propagate through C.** A `panic` or `throw` inside the callback cannot unwind through C
frames (that would be undefined behavior), so 0 is returned to C, and the failure is rethrown to the
caller when the C function returns. If the callback is called again between the failure and the return
of the C function, it is not run and 0 is returned.

An operation that would have to wait inside a callback (a `recv` on an empty channel and so on) is an
error ([12.6](#126-compiled-code-and-tasks)).

When a function is redefined, the new definition is called from the next time it is passed to C.

It works the same way with AOT (`compile-file`). The entry points C calls are built into the
executable.

**They cannot be passed as values.** An FFI declaration cannot be written as it is for the `f` of
`(map f xs)`: a function value is a closure wrapping the body of a definition, and this declaration has
no body to wrap. Wrap it in a `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` is refused too: what could be shown is C's machine code, which this compiler did
not produce. `(compile c-abs)` succeeds (and does nothing, since it is already compiled).

**It works with AOT (`compile-file`) too.** The linker resolves the C functions themselves. If a
declaration has `:library`, that library is added to the link line as `-l` (duplicates are merged into
one), so `compile-file` needs no extra arguments. `compile-file` itself reads the source, so it can
collect them from the declarations.

Symbols are looked up at build time too. If a declared function does not exist, the error names it
before any link error.

The standard library (the prelude) does not use `defffi`. The standard library goes into every
executable whole, so a declaration with `:library` there would link that library even into programs
that do not use the FFI.

#### def-c-struct and typed pointers — allocating C structs

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Declares a struct with the same layout as in C. It can be written only inside a top-level `unsafe`
(which can contain nothing but `def-c-struct`s). A docstring can be put right after the name.

The types that can be written for fields are `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, typed pointers `(ptr T)`, and other `def-c-struct`s (embedded by value). The layout
(the offset of each field, and the size and alignment of the struct) is computed by C's rules (assuming
LP64). A field pointing to the struct itself can be written, but the struct cannot embed itself.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x at 0, y at 8, size 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

The name of a `def-c-struct` goes into the type namespace (no `defstruct` or the like of the same name
can be in the same module), but **it is not the type of a value**. You cannot write
`(defun f ((p point)) ...)`; it appears only as what a typed pointer points to.

**A typed pointer `(ptr T)`** is an address pointing to a `T`. `T` is one of the types that can be
written for fields above. It is a raw machine word like `ptr`, with the same rules for where it can
appear (arguments, return types and local variables only; it can be a value only inside `unsafe`).

Allocation, reading and writing are written in the following forms. All of them can be used only inside
`unsafe`.

| Form | Meaning |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Allocates `n` values of `T` (1 if omitted). The contents are filled with 0. Returns a `(ptr T)` |
| `(c-ref p i)` | A pointer to element `i` from `p`. An error if outside the allocated range |
| `(c-deref p)` / `(setf (c-deref p) v)` | Reads / writes the scalar `p` points to |
| `p::field` / `(setf p::field v)` | Reads / writes a field of a struct. Reading a field that is an embedded struct gives its address (`(ptr inner-type)`) |
| `(as ptr p)` | Forgets the type, making a `ptr` (to pass to something like the `void *` of `qsort`). There is no conversion back |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Allocated memory is freed when control leaves the `unsafe` that allocated it.** The owner is the
lexically outermost `unsafe` within the same function. It is freed whether the code finishes normally
or leaves by `panic`, `throw` or `return-from`. `lambda` and `labels` functions are separate functions,
so a `c-alloc` in them needs an `unsafe` of its own inside them.

Because of this, a typed pointer cannot leave the `unsafe` that allocated it. Each of the following is a
type error:

- Making it the value of the `unsafe` expression (so it cannot be returned from a function either)
- Capturing it in a closure (`lambda`, `labels`)
- Passing it to `task` / `thread`
- Throwing it with `throw`

To use values outside the `unsafe`, copy them into a `defstruct` or numbers inside the `unsafe` and
return those.

**Memory allocated on the C side is not handled.** Values that come in from C as typed pointers (`defffi`
return values, callback arguments, values read from pointer-typed fields) are checked at run time to
see whether they point at a value of that type within a live `c-alloc` allocation, and are an error if
not. NULL is an error too. To receive memory C allocated, or NULL, use the untyped `ptr` (whose contents
cannot be read).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

When the argument of a callback is refused by the check, it is reported to the caller when the C
function returns, just like a failure inside a callback.

### 3.4 defvar / defparameter / defconstant — global variables

```lisp
(defvar (name Type) init-expr)        ; initializes only if not yet bound
(defparameter (name Type) init-expr)  ; assigns every time
(defconstant (name Type) init-expr)

; with a docstring (in the same order as CL's defvar/defparameter/defconstant: after the value)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**The difference between `defvar` and `defparameter` shows on reload** (as in CL). If the global is
**already bound, `defvar` does not even evaluate the initializer**, so when you edit a settings file and
read it again, the values the session changed stay as they are. `defparameter` assigns every time, so
reading it again brings the values back to what is written.

The type annotation is required (it is not inferred from the initializer). `defvar` can be changed;
`defconstant` cannot (`setf` is an error).

### 3.5 defmethod — method definitions

```lisp
; instance method: can be called as (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / associated function: can be called as (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

The caller resolves the method from the static type of `obj` (single, static dispatch). A docstring can
be placed in the same position and under the same rules as for `defun` (right after the `where` clause,
at the start of the body, only when body forms follow). The same goes for methods inside `impl`; they
are retrieved with `(documentation Type::method)`.

### 3.6 defstruct — structs (user-defined types)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (type parameters in angle brackets)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Each field is `(name type)` or `(pub name type)` (visibility per field, independent of the `pub` of the
  struct itself). One more expression at the end becomes the slot's **default value** (`(x i32 0)`); see
  the option list below.
- The following are generated automatically:
  - The constructor `Name::new` (arguments in field order)
  - Getters `(field-name instance)`, with the sugar `instance::field-name`
  - Setters `(set-field-name instance value)`, with the sugar `(setf instance::field-name value)`
- To make the struct itself `pub`, put `pub` in front, as in `(pub defstruct ...)`.
- **Define a type before naming it.** A field's type can be the struct itself (`(next Option<node>)`),
  but not a type defined later: types have no forward declaration corresponding to `defsignature`. A
  name not yet defined gives the same `unknown type` error in a `defun` argument type or in `the`. So two
  types that refer to each other cannot be written.
- **Type variables are only those written in declaring positions.** For `defun`/`defstruct`/`defenum`/
  `deftype`, the `<T>` of the name; for `defmethod`, the receiver's type (`(self box<T>)`, or `box<T>`
  for a static method); for `impl`, the target type and `impl<T>`; for `deftrait`, `Self` and the
  associated types of `(type Item)`. A name appearing for the first time anywhere else (arguments, the
  return value, `the`/`lambda` in the body) does not become a type variable; it is `unknown type`.
- **Docstrings**: a string literal right after the name, before the fields, becomes the docstring
  (`(defstruct Name "doc" (field Type)...)`, the same position as CL's `defstruct`). A field is always of
  the form `(name Type ...)` and can never be a bare string, so there is no ambiguity. Retrieve it with
  `(documentation Name)`.

#### Option list

Writing a list `(Name option...)` in the name position specifies options (the same position as CL).

```lisp
(defstruct (point (:constructor make-point)          ; keyword constructor
                  (:constructor at (x &optional y))  ; BOA constructor
                  (:copier copy-point))
  (x i32 0)          ; a third element is that slot's default value
  (y i32 0))

(point::make-point :y 7)   ; x is 0
(point::at 1)              ; y is 0
(point::at 1 2)
(copy-point p)             ; a shallow copy (the same as CL's copier)
```

- **`:constructor`**: what is generated is a **static function** of the type (`point::make-point`), whose
  body is always `(point::new ...)`. `new` remains the one structural constructor; what is made here is
  a *way to call* it. Several can be declared.
  - `(:constructor name)` takes every slot as `&key`. **Every slot needs a default** (this language has
    nothing corresponding to CL's "unbound slot").
  - `(:constructor name (slot...))` takes the named slots as positional arguments (in any order). Slots
    not named are filled with their defaults, so **they need defaults**. After `&optional`, the rest may
    be omitted (and likewise need defaults).
- **`:copier`**: generates an **instance method** returning a new value with the same slot values.
  Shallow, like CL's copier.
- **`:include Parent`**: prepends the parent's slots (defaults are inherited too; the parent may be in
  another file). **It creates no type relationship**: the child is not a subtype of the parent, the
  parent's methods do not apply to the child, and there is no run-time test linking the two. This
  language has no subtyping; common interfaces are the job of `deftrait`. Only the *list* of slots is
  joined.
- **Slot defaults are read only by generated constructors.** Writing a default without declaring any
  `:constructor` is an error, since it could never be used.
- Options left out, and why:
  - **`:conc-name`**: in CL it prefixes accessors to avoid clashes in one flat function namespace. Here,
    accessors are methods dispatched on the receiver's type, so clashes do not occur, and a prefix would
    break `instance::field` (which knows only the slot name).
  - **`:predicate`**: answers at run time "is this value a `point`?". Here types are a compile-time
    classification with no run-time witness, and there is no position where "a value of unknown type
    that might be a point" exists (`match` on `Sexpr` is sealed, and `:dyn` cannot be downcast), so a
    generated predicate could only ever return `true`.
  - **`:type` / `:initial-offset` / `:named`**: these replace the value's representation with a list or
    vector. The representation belongs to the compiler and cannot be observed from the language.

### 3.7 defenum — enums (sum types)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; a variant with a payload (positional fields)
  (Variant2)                  ; a variant without a payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Each variant has the form `(VariantName FieldType...)`. Fields are positional only (they have no
  names). At least one variant is needed, and names cannot repeat.
- Values are built, as with the built-in `Option`/`Result`, qualified or through `use`:
  `(Name::Variant1 a b)`, or `(Variant1 a b)` after `(use Name)`.
- They can be taken apart with `match` / `if-let`. `match` checks exhaustiveness (it must cover every
  variant or have a `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Methods and associated functions are added afterwards with `defmethod`/`impl`, as with `defstruct`.
- To make the enum itself `pub`, write `(pub defenum ...)`.
- **Docstrings**: the same position and rules as `defstruct`, right after the name, before the variants
  (`(defenum Name "doc" (Variant ...)...)`). Retrieve it with `(documentation Name)`.

### 3.8 deftype — type aliases

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CL's `deftype`, narrowed to what makes sense in a statically typed language: **a spelling of a type, not
a type**.

- The name position is the same as `defun`, and generic arguments are written `Name<T,U>`. At the place
  of use, exactly the declared number of type arguments is needed (too many or too few is an error on
  the spot).
- Expansion happens **inside the type parser**. So nothing downstream knows the alias exists: the
  monomorphization keys, dumps, the compile path and **error messages** all show the expanded form. If
  `(f "x")` fails against a function requiring `meters`, the message says `i32`.
- **It is not a new type.** `(deftype meters i32)` makes `meters` and `i32` the same type, so mixing them
  up is not caught. If you want them kept apart, use `defstruct`.
- **It is not a predicate.** CL's `(deftype small () '(integer 0 9))` describes a *set of values* that
  `typep` tests at run time, but here types are a compile-time classification with no run-time witness,
  so an alias restricting values would have nothing to restrict.
- **It cannot contain itself.** An alias is expanded where it is written, so there is nowhere for it to
  recurse to. Recursive data types are written with `defstruct`/`defenum`.
- It shares the namespace with types and traits (within one module it cannot have the same name as a
  `defstruct`/`defenum`/`deftrait`). Make it public with `(pub deftype ...)` and bring it in with
  `(use m::meters)`.
- **Docstrings**: right after the name, before the type (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; the supertrait list is required; () if none
  (type AssocName)                       ; associated types (any number, optional)
  (method-name ((self Self) params...) RetType)          ; no body = must be implemented
  (method-name ((self Self) params...) RetType body...)) ; with a body = default implementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; bounds applying to the whole impl (optional)
  (type AssocName ConcreteType)          ; makes an associated type concrete
  (method-name (recv params...) RetType body...))
```

Through `impl`, each method is registered as an ordinary `defmethod` of `TargetType`. Traits are
referred to as trait bounds in the `where` clauses of generic functions (see
[3.1 defun](#31-defun--function-definitions)). A trait name can also be a `::` path such as `m::Trait`.

**The supertrait list (required)**: always written right after the trait name. Each element is a bare
trait name, or, if that trait has associated types, `(Trait (Assoc Type))` with **all of its associated
types pinned**.

```lisp
(deftrait Eq () ...)                       ; no supertraits
(deftrait Ord (Eq) ...)                    ; Rust's trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; pinning an associated type
  (rewind ((self Self)) ()))
```

Inheritance has three effects. (1) `impl Ord X` requires `impl Eq X` to be written **first** (a rule
about order of writing: the only form that can be decided deterministically in the REPL and with
step-by-step `load`, and stricter than Rust). (2) `(where (Ord T))` alone lets you call the methods of
`Eq` too. (3) The methods of `Eq` can be called through a `:dyn Ord`, and a `:dyn Ord` value can be passed
as it is where a `:dyn Eq` is required (upcasting). A subtrait redeclaring a method of the same name as
its parent, and inheriting methods of the same name from two parents, are both errors (a vtable has one
slot per name). Diamond inheritance merges into one slot.

**Default implementations**: a body after the signature is used when an `impl` leaves the method out. The
body is resolved in **the namespace of the module** where the trait is written, so it can call
non-public functions of that module. Methods with bodies can also have `where` clauses and docstrings.
The body is type-checked **once, at the point of declaration**, with `Self` left as a type variable
(bounded by `Self: the trait itself`), as in Rust: mistakes that would fail for every `impl` and every
implementing type, even in defaults no `impl` ever omits, are caught there. Calls on `self` to methods of
the trait itself or its supertraits pass through this bound, and associated types are fixed to
themselves, so a signature returning `Item` is matched against the body without knowing the concrete
type.

**Blanket implementations**: making the target a type variable implements the trait at once for every
type meeting the bounds.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; no body at all; everything is the default
```

**No code is generated until a concrete type actually uses it** (once per type, by the same mechanism as
ordinary monomorphization). A trait can have at most one blanket implementation. If a type has an
explicit `impl`, that one takes priority. Type-checking the body is separate from generation: it is done
once at the point of declaration, **with the target left as a type variable** (as in Rust), so even an
implementation that is never used has its mistakes caught there if they would fail for every target
under the declared bounds. Calls justified by the bounds (`(less self other)` under `(where (Ord T))`
and so on) pass, as in the body of a generic `defun`.

**Docstrings**: a `deftrait` can have one docstring for the whole trait, as a string literal right after
the supertrait list, before the items (`(deftrait Name () "doc" (type ...) (method ...)...)`). A signature
without a body cannot have a docstring: a trailing string would itself be the return value of a default
implementation, so the two could not be told apart.

The traits the standard library provides: **`Iter`** (`next` / associated type `Item`; the basis of
`doiter` and the sequence functions), **`Eq`** (`equals`; `not-equals` is a default implementation),
**`Ord`** (inherits `Eq`; only `less` must be implemented, and `less-equal` / `greater` /
`greater-equal` are default implementations), **`Error`** (`message` / `source`; `:dyn Error` for
handling error types uniformly), **`print-object`** (a per-type printed representation), **`Pathish`**
(pathname designators: a string or a `pathname`), and the stream hierarchy **`Stream`** →
**`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**.
Which types implement which traits is in [types.md](types.md); the methods of each trait are in
[Standard Traits](functions/traits.md), [Error types](functions/option-result.md#3-error-types-and-the-error-trait),
[print-object](functions/printing.md#5-print-object-per-type-printed-representation) and
[Streams](functions/streams-files.md). If you `impl` `Iter` for your own collection type, `doiter`
(chapter 5) and `map` / `filter` / `sort` and the like work on it as they are.

Trait calls are **static** by default (resolved by the receiver's static type). To handle values whose
concrete type is decided at run time, the trait object type `:dyn Trait` (chapter 2) gives dynamic
dispatch through a vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; one call site, an answer per implementation
```

Only traits where "every method has a `self` receiver, does not use `Self` anywhere other than the
receiver, and is itself neither generic nor variadic" can be made `:dyn` (inherited methods must meet
the same conditions).

Only types whose values have a representation on the heap can go into a `:dyn` box:

| Can go in | Cannot go in |
|---|---|
| `defstruct` / `defenum` types (including `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` and the standard library's structs), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Fixed-width integers (`i8` to `u32`), `f32`, `bool`, `char`, `symbol`, `()`, function types, and `Option<T>` without a box ([the run-time representation of Option](functions/option-result.md#2-the-run-time-representation-of-optiont)) |

Placing a value of a type that cannot go in where a `:dyn` is expected is a type error. To handle such
values through `:dyn`, wrap them in a struct, as in `(defstruct flag (v bool))`.

### 3.10 module / use — namespaces

```lisp
(module path body...)      ; path is a sequence of segments such as foo or foo::bar
(in-module path)           ; from here to the end of this unit, inside path (the flat form of module)
(use path...)              ; alias functions, types and modules into the current namespace
(import path...)           ; the same as use (a CL-compatible spelling)
(shadowing-import path...) ; a use that knowingly takes a bare name already in use
```

- `module` creates a namespace. **Types are not namespaces** (as in Rust, a type only has associated
  functions and methods).
- `use`-ing a type makes its constructors and public static methods available by bare name too (for
  example, after `(use option)`, `some`/`none` can be called without `option::some`/`option::none`).
- The resolution order of bare names (unqualified identifiers): special forms → constructors → free
  functions (current namespace → root) → instance methods (resolved by the static type of the first
  argument). It does not go up through intermediate parent modules.
- A qualified path `a::b` resolves `a` in the order above; if it is a module it goes inside, and if it is
  a type, the last segment is resolved as an associated item.
- **`use` affects the forms after it.** A file is read one form at a time, and dependencies are resolved
  just before the form is checked, so writing `m::f` **above** `(use m)` gives `unresolved path`. Put
  `use` at the top of the file.
- **`use` can take several paths** (`(use a::f b::g)`). `import` is a CL-compatible spelling with the
  same behavior.
- **A `use` whose bare name is already taken is reported.** Resolving a bare name looks at the module's
  own definitions before aliases, so `(use m::twice)` after `(defun twice ...)` **does nothing**. If you
  mean it, write `shadowing-import` (it still cannot beat a definition, since there is no way to remove
  one; it only beats earlier aliases).
- **`in-module` is the flat form of `(module path body...)`.** Writing `(in-module geometry)` puts
  everything from there to the end of the unit (the file, or the body of the enclosing `module`) inside
  `geometry`. It goes **inside** the file's own module (`main::geometry` for `main.typl`). Two in a row
  nest in order. It is different from CL's `in-package`, and is named differently: in this system the
  file is already a module, so there is nothing to "select", and all a form can do is nest.

### 3.11 Files and modules (multi-file projects)

The file path relative to the source root is the module path:
the contents of `<root>/geo/point.typl` are implicitly wrapped in the module `geo::point`
(a directory is one segment too, in the Rust / Python style). An explicit `(module bar ...)` in the file
nests **inside** it (`geo::point::bar`), so the derived path and an explicit declaration never collide.

- **Source root**: put a manifest file `typelisp.toml` at the project root (it can be empty; optionally
  one line `src = "src"` names the source directory). It is found by walking up from the directory of
  the target file. Without a manifest, the directory of the entry file (the current directory for the
  REPL) is the root.
- **On-demand loading**: when `(use geo::point)` refers to a module not loaded yet, the matching file
  (`geo/point.typl`) is loaded, type-checked and registered automatically. `use a::b::c` searches the
  longest prefix first: `a/b/c.typl` → `a/b.typl` → `a.typl` (since `c` may be an item inside a module).
  Definitions visible from other modules need `pub` ([3.13 pub](#313-pub--visibility)).
- **Circular references are errors**: the chain is reported in the form
  `circular module dependency: a -> b -> a`.
- **Running**: `typl <file.typl>` runs a file (without arguments, the REPL). `use` in the REPL resolves
  files by the same rules.
- **Cons arena capacity**: `typl --heap-cells N` sets the **initial capacity** of the cons cell arena
  (default 65536; the form `--heap-cells=N` works too, for both running files and the REPL). The arena
  **grows by adding more** when it runs short. The limit of growth is 256 times the initial capacity,
  and an allocation beyond it gives `heap exhausted`: the initial capacity means "allocate this much at
  first", and the limit means "beyond this, treat it as a leak".

### 3.12 load — flat loading

```lisp
(load "path")   ; top level only; path is a string literal
```

- CL-style **flat loading**: reads the forms of the target file **into the current namespace** as they
  are (without wrapping them in a module, unlike `use`). Top level only (inside a function body it is a
  type error).
- `path` is relative to the directory of the loading file (from the REPL, to the process's cwd). If it
  has no extension, `.typl` is added.
- `(load ...)`/`(use ...)` in the loaded file are processed recursively too.
- **It reads one form at a time and runs it on the spot** (as CL's `load` does). Form *k* has finished
  running before *k+1* is read: even if there is a syntax or type error partway, the forms before it
  have already run. Module files loaded by `use` are different: they are checked as one unit and running
  them is left to whoever `use`d them (corresponding to CL's `compile-file`).

### 3.13 pub — visibility

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` can be put only on the eleven kinds above (not on `module`/`use`/`deftrait`/`impl`). It is written
with the definition keyword right after `pub`, not in the form `(pub (defun ...))` wrapping the definition
in parentheses. One `pub` makes exactly one definition public (several definitions cannot be marked at
once).

### 3.14 defmacro — macro definitions

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- All parameters and the return value are always `Sexpr`, so no type annotations are written.
- CL-style unhygienic macros (avoiding clashes with `gensym` is the macro author's responsibility).
- The lambda list is in CL's order `required &optional &rest &key` (each marker at most once, and only in
  this order).
  - `&optional` … optional arguments. `name` or `(name default-expr)`. The default expression is
    evaluated at expansion time (it can refer to parameters bound earlier) and bound when the argument
    is omitted (without a default, the empty list `()`).
  - `&rest name` … receives the remaining positional arguments together as one `Sexpr` list.
  - `&key` … keyword arguments. `name` or `(name default-expr)`. The caller passes them as `:name value`
    (in any order). When omitted, the default expression (the empty list `()` if none). Unknown keywords
    or an odd-length `:key` sequence are errors.
- Examples: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — local macro bindings

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexically scoped macros
(symbol-macrolet ((name expansion) ...) body...)         ; a name stands for a form
```

Both are **expression** special forms, and nothing remains at run time (what gets compiled is the
expanded form of the body). The lambda list is the same as for `defmacro`. The detailed rules and
examples are in
[Local macro bindings](functions/system.md#9-local-macro-bindings-macrolet--symbol-macrolet).

## 4. Binding and conditionals

```lisp
(let ((name val) ...) body...)      ; parallel binding
(let* ((name val) ...) body...)     ; sequential binding (earlier bindings usable in later initializers)

(if cond then else)                 ; else is required (always three elements)
(when cond body...)                 ; an if without else (Unit type). defmacro
(unless cond body...)               ; the negation of when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; a list of keys: matches if any of them does
  (else body...))                   ; expr is evaluated once. keys are compared with equal.
                                     ; keys are "literals" and are not evaluated (as in CL).
                                     ; a bare symbol a means the symbol 'a.
                                     ; writing 'a is an error (use the bare a). defmacro
(ecase expr (key body...) ...)      ; a case requiring a match. panics if nothing matches. defmacro
(ccase expr (key body...) ...)      ; CL's ccase. there are no restarts to offer, so it is the same as ecase. defmacro
(and expr...)                       ; short-circuit evaluation. true with zero arguments. defmacro
(or expr...)                        ; short-circuit evaluation. false with zero arguments. defmacro
(progn body...)                     ; runs in order and returns the last value
(unsafe body...)                    ; the same as progn, plus permission to write FFI calls
                                     ; and raw words. see 3.3 defffi
(prog1 form more...)                ; evaluates everything; the value is that of form. defmacro
(prog2 a b more...)                 ; evaluates everything; the value is that of b. defmacro
(the Type expr)                     ; a type annotation (no run-time effect)
```

### 4.1 unsafe — taking on assumptions that cannot be checked

```lisp
(unsafe body...)
```

The same as `progn`: evaluates the body in order and returns the last value. It creates no scope and is
not a function boundary (`break` / `return-from` pass straight through to the outside). The difference
is that some things can be written only inside it.

Three things currently require `unsafe`: calling C functions declared with
[defffi](#33-defffi--declaring-c-functions-ffi), making raw machine words (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) into values, and [`def-c-struct` and `c-alloc`](#def-c-struct-and-typed-pointers--allocating-c-structs).

Memory allocated with `c-alloc` is freed on leaving the outermost `unsafe` within the same function.
Only that `unsafe`, unlike `progn`, has work to do on the way out: the freeing.

What `unsafe` takes on are the following assumptions the compiler cannot verify:

- **That the types match.** That the declared C signature matches the real one. If not, arguments go into
  the wrong registers and return values are read at the wrong width.
- **Memory safety.** What the C side does with what it is given.
- **Process-wide state.** Environment variables, signal handlers, `errno`. For example, calling `setenv`
  through the FFI breaks the assumptions this implementation's `decode-universal-time` makes when it
  works out local time.
- **Thread safety.**

It is not a way out of type checking. `(unsafe (+ 1 "two"))` does not pass. What is permitted is writing
certain **operations**, not writing nonsense.

It works lexically. The body of a `lambda` written inside `unsafe` inherits the permission (as with
closures inside Rust's `unsafe` blocks). The value may later be called from outside the `unsafe`, but
writing it there is itself taken as accepting the responsibility.

### 4.2 destructuring-bind — taking lists apart by shape

```lisp
(destructuring-bind lambda-list form body...)
```

Takes apart the list `form` produces **by its shape** and binds it. The lambda list is that of `defmacro`
(required → `&optional` → `&rest`/`&body` → `&key`, each with default expressions), for the same reason
CL shares one between the two: they are two forms that take apart the same thing.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Every bound variable is an `Option<Sexpr>`.** This is not a limitation of the implementation but the
  nature of what is bound: S-expression lists are the only lists in this language, so there is no other
  type to give the elements. Falling back to `match` where a scalar is needed is the same as in a
  `defmacro` body.
- **A shape that does not match panics** (corresponding to CL's error): too few or too many elements, an
  odd-length `&key` sequence, or an unknown keyword. `sexpr-car` is a lenient function that returns `()`
  for `()`, so without the check, a short list would silently be bound to an empty sequence.
- **Nested lambda lists are not supported.** `defmacro` does not take them either, so there is one rule.
  `(a (b c))` does not silently bind a sublist to `b`; it is an error saying so.
- The default expressions of `&optional` / `&key` are **evaluated only when used** (as in CL).
- There is nothing corresponding to CL's `&allow-other-keys` (`defmacro` has none either).

### 4.3 match — pattern matching

```lisp
(match expr
  (pattern body...)
  ...)
```

Kinds of patterns:
- `_` — wildcard
- A variable name — a binding pattern (always matches). However, if the scrutinee's type has a variant of
  that name, it is resolved as **the bare variant name pattern below**
- A bare variant name — matches a variant that takes no arguments (`(match c (red 1) (blue 2))`). Writing a
  variant with fields by its bare name is an arity error, so write it in parentheses, as in `(circle r)`
- **Immediate literals**: integers / `true`/`false` / characters — compared as words
- **Value literals**: strings / floating-point numbers / symbols (`'foo`) / bignum integers / ratios —
  compared by value with that type's `Eq::equals` ([Standard Traits](functions/traits.md#2-eq--ord-comparison)).
  Strings compare by contents, not by identity
- `(= expr)` — evaluates any expression and compares with `Eq::equals`. The only way to compare types
  that have no literal syntax (`defstruct` instances, globals, computed results), and a user-defined `Eq`
  implementation becomes the comparison rule as it is. `expr` can refer to anything visible from the
  arm's position (arguments, outer bindings, globals)
- `(Ctor sub-pattern...)` — constructor patterns (`Some x` `None` `Cons a d` `Ok v` and so on)

Comparing a type that does not implement `Eq` with a value literal / `(= expr)` is a type error (this
language chooses to say "these cannot be compared" rather than leave an arm that silently never matches).

**Value literals against an `Sexpr` scrutinee**: the `Eq` of `sexpr` is `eq` (CL's identity), so
immediates (`'foo` (interned) / integers / characters / `true`/`false`) can be written as they are and
match by content:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Non-immediate literals (strings / floating-point numbers / bignum integers / ratios) **cannot be written**
against an `Sexpr`. Their `eq` compares object identity, which would make an "arm that type-checks but
never matches", so it is an error naming the variant pattern: write `(str "hi")` and it is taken apart
into a `string` and compared by content. `(= expr)` explicitly asks for `equals`, so this restriction does
not apply to it.

**The scrutinee does not have to be an ADT.** `string`/`symbol`/`i32`/`f64` and the like can be matched
directly (that is where string literal patterns go). However, a type without variants cannot be covered
by enumeration, so `_` (or a binding pattern acting as a wildcard) is required:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; a type without variants needs `_`
```

Against an `Sexpr` scrutinee, besides the 16 built-in variant patterns above, **downcast patterns**
(taking out instances of user-defined ADTs) can be written: syntax for getting back, with `match`, an
instance of a `defstruct`/`defenum` (chapter 3) that was implicitly converted to `Sexpr`, as in
`(list p 42)`:

- `(TypeName sub-pattern...)` — field decomposition with the **type name** first (structs only: a
  `defstruct` always has one variant, so it is written with the type name rather than a variant name).
  For example, for `(defstruct point (x f64) (y f64))`, `(point x y)`.
- A bare variant name `(VariantName sub-pattern...)` — extracts a variant of a `defenum`. Resolved as a
  bare name visible after `(use EnumType)` (the same visibility rules as when calling the constructor).
  For example, for `(defenum color (red) (blue))`, `(red)` `(blue)` after `(use color)`. If variant names
  of several visible enums clash, it is an ambiguity error, so the qualified form
  `(EnumType::VariantName ...)` can be written too (no `use` needed).
- `(the Type pattern)` — a downcast of the whole type (binding it as a whole). It does not decompose
  fields; it passes the value to `pattern` as it is. The only way to take out a mutable struct while
  keeping its identity, and also the only way to take a `Vector<T>`/`HashTable<K,V>` out of an `Sexpr`
  (they have no field-decomposition form). For example, after `(the point p)`, `(setf p::x 9)` is
  reflected in the original instance in the list too.

**Patterns for `Option<Sexpr>`**: the type of S-expression data is not `Sexpr` but `Option<Sexpr>`, and the
empty list is not a variant of `Sexpr` but the `none` of `Option`. So when matching an `Option<Sexpr>`,
the 16 variants of `Sexpr` and `none` can be written **flat in the same list of arms** (no outer `match`
to peel off the `Option` is needed):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; the empty list
    (_          9)))
```

Exhaustiveness is checked in the same flat universe: the 16 variants of `Sexpr` plus `none`, 17 in all.
Forgetting `(none)` is an error unless there is a `_`. `(some x)` can also be written and binds "something
non-empty".

This sugar applies **exactly** to `Option<Sexpr>` only. For `Option<Option<Sexpr>>`, it would be unclear
which layer `(int n)` peeled, so write two levels of `match` as usual.

The same downcast patterns can be used as they are on **a trait object (`:dyn Trait`, chapter 2)
scrutinee**: `match` unboxes it and then hands it to the `Sexpr` pattern machinery above, so there is no
additional syntax. The set of implementing types is open, so it can never be exhaustive, and `_` is
required:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; field decomposition with the type name first
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Type inference across arms**: all arms must have the same type (except arms that diverge, such as with
`panic`). In a `match` written where no type is expected, the arms fill in each other's missing type
arguments: `(result::ok v)` fixes only `T`, and `(result::err e)` only `E`, but together they fix
`Result<T,E>`. A type argument that no arm can fix by the end is an error of that arm
(`cannot infer type argument ...`). Outside `match`, a type argument that cannot be fixed is an error on
the spot.

The exhaustiveness check of a `match` using downcast patterns does not count them toward the coverage of
`Sexpr`'s own variants (a `match` listing only downcast patterns must be closed with `_`). For generic ADTs
(`defstruct point<T> ...` and so on), the type arguments of a downcast pattern cannot be inferred, so the
field-decomposition form (`(point ...)`) and the bare variant form cannot be used; state them with `the`,
as in `(the point<i32> p)`.

**Downcasts look at the instantiation too.** Explicit type arguments are used for matching:
`(the point<i32> p)` lets through only values of `point<i32>`, and a `point<string>` passes by to the next
arm. This is because a value remembers its type including its type arguments (the same mechanism that
chooses `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (with bindings) if val matches pattern, else els. defmacro
(while-let (pattern val) body...)   ; loops while val (re-evaluated each time) matches pattern. defmacro
```

## 5. Iteration

```lisp
(loop body...)                      ; an infinite loop. leave with break/return
(while test body...)                ; loops while test is true. defmacro
(until test body...)                ; loops while test is false (the negation of while). defmacro
(dotimes (var count-expr) body...)  ; evaluates count-expr once and runs var over 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL-style iteration with parallel stepping. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; the sequential version of do (let* binding, assigned in order). defmacro
(doiter (var coll-expr) body...)    ; iterates over a value implementing the Iter trait. defmacro

(break)                             ; leaves only the innermost loop. the value is always Unit
(return)                            ; leaves only the innermost loop
(return value)                      ; leaves the innermost loop with a value
```

Both `break`/`return` leave **only the innermost enclosing loop** (they are not an early return from the
function, and they cannot cross a `lambda` boundary). The type of a `loop` is the join of the value types
of the `break`/`return`s found inside it (`!` if it is never left). To leave a function, use
`return-from`, below.

### 5.1 `block` / `return-from` — named exits

```lisp
(block name body...)                ; a named exit target. the value is the last form,
                                    ; or the value passed by return-from
(return-from name)                  ; leaves that block with Unit
(return-from name value)            ; leaves with a value
```

**Each function of `defun` / `defmethod` / `labels` implicitly establishes a block with its own name**
(as in CL). So `(return-from f v)` is an early return from the function:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` is a **lexical** exit, and the name is **resolved where it is written**: the checker associates a
`return-from` with the enclosing `block` and joins the type of its value into the block's exit type. So:

- A `return-from` with no matching `block` is a **type error** (not a run-time error).
- A value whose type does not fit the other exits or the body's type is a **type error** (the same rule
  as for `match` arms).
- If blocks of the same name are nested, **the inner one wins** (CL's shadowing rule).
- **It cannot cross function boundaries.** From inside a `lambda`, you cannot leave to an outer `block`
  (`lambda` establishes no block: CL's implicit blocks need a *name*, and anonymous functions have none).
  What needs to cross is `catch`/`throw` (chapter 8, which is **dynamic**).

Like `break`/`return` (chapter 5), it is a **static** exit, so in compiled code it is a branch to a basic
block fixed at compile time. If there is an `unwind-protect` in between, its `cleanup` runs (chapter 8).

If you never write `return-from`, the implicit block costs nothing.

### 5.2 Extended `loop` (CL's LOOP)

**If the first element of `loop` is a keyword**, it is read as a sequence of clauses. Otherwise it stays
the simple loop above, and the meaning of existing `loop`s does not change (the same as CL's own simple
loop rule).

CL writes the clause words as bare symbols (`(loop for i from 1 to 3 collect i)`), but here **all of them
are keywords**: a bare `for` would be just a variable reference, and being a keyword is also what tells it
apart from a simple loop. The exception is `=`, which separates a variable from a value: its position is
unambiguous, so it is read either bare or as a keyword (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<int> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<int> 1 2 4 8>
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Variable clauses** (written before body clauses. This is CL's rule: written after, they could be read as
"iterate only from there on", so it is an error):

| Clause | Meaning |
|---|---|
| `:with v = e` | Binds once. May read the variables of earlier clauses |
| `:for v :in s` / `:for v :across s` | The elements of an `Iter` in order. CL's list/vector distinction does not exist here, so these are two spellings of the same clause |
| `:for v :on s` | The successive **suffixes**. CL passes the shared tail cons, but an `Iter` has no tail to share, so each is a new `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Counting. `:downfrom`/`:upfrom` also work |
| `:for v = e [:then f]` | Starts with `e`, and from the second time on uses `f` (without `:then`, `e` every time) |
| `:repeat n` | Iterates that many times |

With several `:for`s, they advance **in parallel**, and the loop ends as soon as any one is exhausted.

**Body clauses** (run every time, in the order written):

| Clause | Meaning |
|---|---|
| `:do form...` | For side effects |
| `:collect e [:into v]` | Collects into a `Vector<T>` |
| `:append e [:into v]` | Appends the contents of an `Iter` |
| `:sum e` / `:count e` | The sum / the number of times it was true |
| `:maximize e` / `:minimize e` | The maximum / minimum. **`Option<T>`** (just as CL returns nil for an empty sequence; an arbitrary `Ord` type has no least element) |
| `:always e` / `:never e` | `true` if all of them hold; `false` immediately once one fails |
| `:thereis e` | `e` is an **`Option<T>`**. Returns the first `some`, or `none` if there is none (this is what corresponds to CL's "first non-nil value"; to test a `bool`, use `:always`/`:never`) |
| `:while e` / `:until e` | **Ends normally** here (`:finally` runs, and what was collected is the answer) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Makes one clause conditional |
| `:return e` | Leaves immediately with that value (`:finally` does not run, as in CL) |
| `:initially form...` / `:finally form...` | Before the loop / on normal completion |

**`:named name`** (before any other clause, only once) wraps the whole loop in `(block name …)`.
`(return-from name e)` can leave at once even from inside nested loops, and like `:return`, `:finally`
does not run. Without a name, no block is established: CL's unnamed `loop` establishes `block nil`, but
there is no `nil` here, and `break`/`return` (chapter 5) already provide "leave the innermost loop".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Leaving out `:finally (return 0)` is a **type error**. It is just the rules of `block` at work (5.1): the
exit's type `int` does not fit the `()` the loop leaves when it is exhausted.

**The value of the loop** is the accumulation of the accumulating clause if there is one (the first, if
there are several), `true` for `:always`/`:never`, `none` for `:thereis`, and `()` if there is none. If
the last thing in `:finally` is `(return e)`, that is the value: CL's `finally (return …)` idiom, the only
way a loop that does not accumulate can name its own answer.

**Differences from CL / what is not included**:

- **Clause words are keywords** (above).
- `:maximize`/`:minimize`/`:thereis` return `Option<T>` (there is no nil).
- **Writing only `:return`, with neither an accumulation nor `:finally`, is an error.** CL returns nil when
  exhausted, but there is no such thing here, so the loop has to say what its value is when exhausted.
- Joining parallel clauses with `:and`, `:being`/dedicated iteration over hash tables, `:it` and `:nconc`
  are not included.
- The element type of `:collect` comes from the type of the accumulated expression. Trying to collect a
  type that **cannot be written as a type name**, such as a function type, is an error saying so.

## 6. Function values and calls

```lisp
(lambda (params) RetType body...)   ; makes a first-class function value (a closure)
(labels ((name (params) RetType body...) ...) body...)   ; local function definitions that can be mutually recursive
(apply f arg1 ... argN rest-list)   ; calls f (a variadic function with &rest), spreading rest-list
```

Named functions can be passed as values as they are too (as arguments to higher-order functions and so
on).

## 7. Other special forms

```lisp
(setq var value ...)                ; CL's variable assignment. just a sequence of (setf var value). defmacro
(psetq var value ...)               ; parallel assignment. evaluates all values first, then assigns. defmacro
(psetf place value ...)             ; psetq generalized to places (the same expansion). defmacro
(setf place value)                  ; assignment to a place. a place is a variable name / var::field /
                                     ; a call of the form (accessor recv key...). valid if the
                                     ; static type of recv has an instance method named
                                     ; set-{accessor} (for the get of Vector<T> and HashTable<K,V>,
                                     ; set corresponds as an exception; otherwise set-accessor-name).
                                     ; the value is the value assigned (as in CL). so
                                     ; in (if c (setf x 1) ()), then and else do not have matching types
(incf place)  (incf place delta)    ; place += delta (delta=1 if omitted). the result is as with setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 if omitted)
(rotatef place1 place2 ... placeN)  ; rotates N places (new place1=old place2, ...,
                                     ; new placeN=old place1). each place's subforms evaluated once
(shiftf place1 ... placeN newvalue) ; shifts the values of place2..N left and puts newvalue in placeN.
                                     ; the return value is the old value of place1
(list e1 e2 ... en)                 ; expands to (cons e1 (cons e2 (... ()))). () with zero arguments.
                                     ; each element is converted to Sexpr implicitly (like CL's cons, it
                                     ; can hold any value). scalars (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) are wrapped in the matching Sexpr variant, and defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> and the like go in as they are
                                     ; (at no conversion cost). the same for &rest/format arguments.
(source-file)                       ; the name of the file this form was read from (string). fixed as a
                                     ; constant at check time. corresponds to CL's *load-pathname*, but is
                                     ; not a variable: module bodies run after checking, so "currently
                                     ; loading" cannot be relied on, while at check time it is always known.
                                     ; for sources that are not files, the reader's name for them (<stdin>/<input>)
(quote datum)                       ; the same as 'datum. returns it as Sexpr data without evaluating
(quasiquote template)               ; the same as `template. embeds expressions in the template with ,/,@
(documentation name)                ; returns the docstring of name (a bare name or Type::method) as Option<string>
(panic message)                     ; message: string. ends abnormally with an unrecoverable error. type !
(unreachable)                       ; expands to (panic "unreachable"). defmacro
(todo)                              ; expands to (panic "todo"). defmacro
(as Type expr)                      ; numeric/character type conversion. conversions that can fail panic on failure
(try-as Type expr)                  ; like as, but returns the result as Option<Type> (None on failure)
(print control args...)             ; expands the format and writes to standard output (no newline)
(println control args...)           ; the same (with a newline at the end)
(format dest control args...)       ; CL's format. returns the expanded string
(pprint x)                          ; pretty-prints. writes a newline first, as in CL
(pprint-fill x)                     ; fill layout
(pprint-linear x)                   ; all on one line or one element per line
(pprint-tabular x [colinc])         ; tabular layout (16 columns by default)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; build a logical block yourself
```

The `print`/`println`/`format`/`pprint` family are special forms, so their variadic arguments (a single
object for the `pprint` family) are wrapped into `Sexpr` with their own types before being passed: this is
why `(println "~a" my-struct)` just works. The details of format directives and the pretty printer are in
[Format Directives](functions/format.md) and [Printing](functions/printing.md#4-the-pretty-printer).

`as`/`try-as` handle only the numeric and character catalog (between `int`, the fixed-width integer
types, `f32`/`f64`/`ratio`/`char`). The same type is no conversion. **Conversions between integer widths
(including `int`) and between `f32`↔`f64` are real conversions**: `as` truncates / rounds, and `try-as`
answers whether it fits that width (precision). `(as int x)` is the exact widening from a fixed width,
and `(as i32 n)` the truncation from `int`. Integer → `char` can fail out of range, so `as` panics and
`try-as` gives `None`. Everything else (widening, and the truncation of `float->int`/`ratio->int`) always
succeeds. `float->int`/`ratio->int`/`char->int` land on `int`, and if a narrower width is asked for,
`int->W` is called after them. This is sugar that expands to the corresponding conversion methods
(`int->char`/`int->int`/`int->W` and so on in [Numbers](functions/numbers.md)).

`documentation`, like `quote`/`compile`, is a special form that reads `name` without evaluating it, as an
unevaluated bare symbol / `::` path. Unlike CL's `(documentation 'name 'function)`, it takes no type
argument: it resolves `name` in the order variable → function → type → trait → macro (the same priority
as when a bare identifier is evaluated as an expression) and returns the docstring of the definition found
(`(documentation Type::method)` is for methods). Failing to resolve (no definition of that name) is a
check-time error; a definition that exists but has no docstring gives `Option::none`. Everything is
decided as a constant at check time: no run-time lookup happens. Module-qualified free names (`mod::name`,
except `Type::method`) are not supported.

## 8. Non-local exits (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; runs body. if (throw 'tag v) happens anywhere
                                    ; body reaches, that v becomes the value
(throw 'tag value)                  ; exits to the nearest dynamically enclosing (catch 'tag ...)
(unwind-protect protected cleanup)  ; runs cleanup however protected is left
```

Unlike `break`/`return` (chapter 5), this is a **dynamic** exit: `throw` does not look lexically for the
`catch` around it, and reaches a `catch` of the same tag across any number of function calls.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; if not found, the value at the end as usual
```

- **Tags are literal symbols only** (`'done`). Unlike CL, they are not evaluated.
- **A tag carries a type.** The type is decided the first time `'tag` is used, and every later
  `throw`/`catch` of the same symbol is checked against it. Using it with another type is a type error.
- The type of `throw` is `!` (it diverges). The type of `(catch 'tag expr)` is the join of the type of
  `expr` and the type of the tag.
- The value of `unwind-protect` is the value of `protected`. The value of `cleanup` is discarded.
  `cleanup` runs however `protected` is left: in addition to normal completion, `throw` and `panic`, it
  also runs when it is left by `break`/`return`/`return-from`. A non-local exit by `cleanup` itself wins
  over the exit in flight.
- Nested `unwind-protect`s run from the inside out. A `break` leaving a loop **inside** `protected` has
  not left `protected`, so its `cleanup` does not run.

CL's conditions (`define-condition`/`handler-bind`/`invoke-restart`) are not adopted. They do not fit
static typing, so recoverable failures are expressed with `Result` (chapter 9).

## 9. Error handling policy

- Recoverable failures: `Result<T,E>` + `match`. Unrecoverable failures (bugs, broken invariants):
  `panic`.
- There is no syntax corresponding to `?`/try. Branches are written explicitly with `match`.
- Function and special form names do not use `!` (destructive operations) or `?` (predicates) as
  suffixes. Predicates are named with a `-p`/`p` suffix (`zerop`, `consp` and so on) or an `is-` prefix
  (`is-some`, `is-ok` and so on).

## 10. Compilation

```lisp
(compile name)                      ; JIT-compiles an already defined defun/method into native code
(compile-file src-path out-path)    ; AOT-compiles a source file into a native executable (skips the final `(main)`)
(dump path)                         ; writes the current environment (type information + compiled bodies) to one file
(disassemble name)                  ; prints what that definition becomes (host machine code by default, LLVM IR with true as the second argument)
```

`compile` is a special form; `name` is not evaluated and is read as an unevaluated bare symbol / `::` path
(a string is a type error). Generic functions cannot be targeted: a copy for each type is made at each
place of use, so no single compiled body exists. **A name that cannot be resolved is a check-time
error** and is never carried over to run time (there are separate messages for: the type exists but not
that method / neither the type nor the function exists / a bare undefined name). Visibility here is
treated as for any other reference: "exists but is not visible from here" fails at check time, just like
"does not resolve".

Callees are compiled transitively too, so **a function that (even indirectly) calls something that cannot
be compiled cannot be compiled**. The process does not crash; it is refused with an error saying so.
Every built-in function can be compiled, so the only functions refused this way are those calling the
following interpreter-only operations:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

The interpreter-only ones are `compile`/`compile-file`/`dump` and `trace`/`untrace`/`step`/`disassemble`
([Implementation tools](functions/system.md#5-implementation-tools-clhs-252)). Rather than being things
that cannot be compiled, these are operations of the compiling side (what `dump` writes out is the
interpreter's environment itself, which an AOT executable does not have; what `trace` watches and where
`step` stops are the call paths of the running interpreter; and `disassemble` uses the compiler itself).
`room`/`dribble`/`ed` are not among them and can be compiled normally.

What **can** be compiled: stream and file I/O, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, the transcendental
functions, bit operations, `catch`/`throw`/`unwind-protect`, all four of `eq`/`eql`/`equal`/`equalp`
(which lets `case` compile for every type), the whole printing family including `print`/`println`/
`format`/`pprint` and `pprint-logical-block`, `read`, and `eval`. The standard library ships already
compiled.

An AOT executable contains only the features the program uses. A program that does not print gets no
formatting engine, one that does not call `read` gets no reader, and one that does not call `eval` gets
no checker and no interpreter.

From the command line, `typl -c src-path [-o out-path]` (`-c` can also be written `--compile`) does the
same as `compile-file`. Without `-o`, the output is `src-path` with the `.typl` extension removed. By
default, the static library `libtypelisp_front.a` linked into executables is, for a release build of
`typl`, the one `typl` carries inside itself, written out at the first link to
`$TYPELISP_HOME/lib/<build ID>/` (or `~/.typelisp/lib/<build ID>/` without `TYPELISP_HOME`) and used from
there; for a debug build, the one where `typl` was built. `typl --remove-lib` deletes what that `typl`
wrote out. With `--others`, it deletes those of other build IDs; with `--all`, those of every build ID.
With `typl --lib-dir DIR`, the one in `DIR` is used (for both `-c` and `compile-file`), and if it is not
there, it is an error at startup.

### 10.1 Dumps

```lisp
(dump "session.typld")     ; write one out
```
```sh
typl --image session.typld prog.typl   # start from it
typl --image session.typld             # the REPL too
```

A dump holds type information and compiled bodies in one file. What `(dump path)` writes is what the
current session loaded (the standard library, or a dump passed with `--image`) plus **what the session
itself defined**. So the output is self-contained, and `typl --image` brings up the same environment.
What the session `(compile f)`d is written in its compiled form.

What is saved is **definitions, not history**:

- The session's top-level expressions (`(println ...)` and so on) are not included. It would be a problem
  if loading re-ran them.
- Global variables come back with **the value of their initializer run again**, not the value at dump
  time. This is a deliberate difference from SBCL's `save-lisp-and-die` (which writes the heap out as it
  is), and this choice makes a whole family of problems disappear: "values that cannot be saved", such
  as open streams, function pointers of closures and external memory.
- Unlike `save-lisp-and-die`, **the process does not die**, since writing does not damage the image.

A dump records the versions of the standard library and the compiler of the implementation that wrote
it. Loading it with a `typl` of a different version is an error; it is never silently accepted.

### 10.2 `eval` in AOT executables

`eval` type-checks against "the current global environment" and then evaluates
([Parsing and evaluation](functions/system.md#6-parsing-and-evaluation)). That environment (the tables of
signatures, types and macros the checker consults, and the bodies the interpreter can run) is **not in
the machine code**. A compiled function is nothing but a symbol placed at an address; it has neither its
argument types nor a table for looking up bodies by name.

So, only for programs that call `eval`, `compile-file` **builds that environment at compile time and
writes it into the executable**. The format is the same as a dump, containing the standard library's part
and the program's own part. All that happens at startup is restoring it: the source is not reread, and
nothing is type-checked again. Nothing is added to programs that do not call `eval`.

Consequences:

- **Startup takes longer and the executable is larger**, since the code of the checker and the
  interpreter and a snapshot of the environment go in. The heap is also made somewhat larger.
- **Forms passed to eval are interpreted.** Even when the form passed to eval calls the program's own
  functions, what runs is the interpretable body the snapshot holds. The result is the same; only the
  speed differs.

The storage of global variables is **shared** with compiled code (the same slots). A `defvar` initializer
is run once by the compiled initialization, and the restore skips it, so an initializer with side effects
does not run twice.

`compile-file` reads the standard library too (and embeds its bodies into the executable), so standard
library functions such as `abs`/`gcd`, and `(impl print-object ...)` as well as
`(defmethod print-object ...)`, can be used with AOT.

`compile-file` also accepts `use` (and `import`/`shadowing-import`). The entry file's `(use m)` finds files
by the same rules as `typl file.typl`, and the dependency files found are compiled and linked into the
executable too: a layout where `main.typl` reads `http.typl` through `(use http)` can be AOT-compiled as it
is. The entry file's own definitions also go into the module named after the file, as with
`typl file.typl` (`point` in `p.typl` is `p::point`). So the printed representation of values
(`#<p::point x: 1 y: 2>`) is the same whichever way it is run.

## 11. Reader macros (readtable)

What the reader **does when it meets a certain character** can be replaced from the program (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f reads the character c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f reads the two-character sequence d s
(get-dispatch-macro-character d s)    ; Option<f>
```

The type of `f` is `(fn (string-input-stream char) Option<Sexpr>)`. The first argument is **a stream over
the text not yet read**, and the second is **the character that triggered it** (the second character for a
dispatch). The return value becomes the data read at that point. The stream is a concrete type rather than
`:dyn PeekInput` because the reader always passes this one kind: `read-sexpr` / `read-char` / `peek-char` /
`unread-char` / `read-delimited-list` all take `(where (PeekInput S))`, so all of them work on the concrete
type as it is.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => read as (not (equal 1 2)), that is, true
```

The reader **looks at macro characters before the built-in syntax**, so it can take over `(` and `'` too.
Sub-characters of `#` registered this way take priority over the built-in `#b`/`#x`/`#.`. A character other
than `#` becomes a dispatching character on the spot when passed to `set-dispatch-macro-character`: there
is **no** counterpart to CL's `make-dispatch-macro-character`. Registration already does its job, so a
separate step would have nothing to do.

**When they take effect** depends on the reading path, the same as for `#.` (chapter 1):

- The REPL and `(load ...)` run one form at a time, so **functions defined in earlier forms** can be
  registered as they are.
- Module files are checked as a unit and run later, so **only the calls to `set-macro-character` /
  `set-dispatch-macro-character` run immediately** (the role of CL's `(eval-when (:compile-toplevel) ...)`).
  Since they run immediately, **the function passed must already exist at that point**. A `defun` in the
  same file has not run yet, so write a `lambda`, or use the standard library or something that has
  already run. Only top-level calls are covered; it does not look inside `progn` or `let`.

The built-in `read` / `read-from-string` also consult the readtable (as in CL).

**What is not there**: `*readtable*` and `copy-readtable`, and `readtable-case`. The first two because a
readtable is **not a value**: a value would have to be "something that can be handed to a reader", but the
reader that reads the source is outside the program, with nowhere to hand it to. `readtable-case` because
chapter 1 decides that this language's reader always lower-cases (CL's `:downcase`).


## 12. Concurrency (tasks)

**A task is a lightweight thread** (in Go terms, what a `go` statement starts) and runs cooperatively
(there is no preemption). Switching does not go through the kernel, and the execution state lives on the
heap rather than on a machine stack, so tasks are cheap to create in large numbers.

**Tasks run at the same time on several OS threads** (multi-core parallelism). The number of threads is
the environment variable `TYPELISP_THREADS` (the total, including the thread running `main`; the default
is the machine's parallelism). In `typl`, **only compiled tasks** run on other threads, and interpreted
tasks run on the interpreter's thread (12.7). Shared data goes through `Mutex<T>` or `Chan<T>`;
simultaneous reads and writes that do not are undefined, as in Go (12.7).

Of the vocabulary, **only `task` / `thread` / `select` are special forms**; the rest are ordinary
functions, methods and macros ([Tasks and Channels](functions/concurrency.md)).

### 12.1 `task` — starting a task

```lisp
(task (f arg...))                   ; returns Task<T>, where T is the return type of f
```

**It takes only the form of a call.** `f` and each `arg` are evaluated where the `task` is written, in
the order written, and only **the call** happens in the new task. This is the same rule as Go's
`go f(x)`, and it is also why it takes a call form rather than a thunk: a thunk would capture its arguments
without evaluating them.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i is evaluated on the spot each time; no capture trap

(task ((lambda () ()                ; to run an arbitrary body, call a lambda
         (println "start")
         (send ch 1))))
```

Special forms (`if` / `let` / `progn` …) cannot be written directly under `task`.

**Why it cannot be a function**: writing `(spawn (lambda () T body...))` would require spelling out `T`,
since `lambda` requires a return type annotation, and a macro does not know the return type of `(f a b)`.
Only the checker knows it.

### 12.2 `thread` — starting a task on a dedicated OS thread

```lisp
(thread (f arg...))                 ; returns Thread<T>, where T is the return type of f
(join th)                           ; waits for completion and returns its value (any number of times)
```

The form and evaluation rules are the same as `task` (it takes only a call form, and `f` and `arg` are
evaluated where it is written). The difference is where it runs: **it starts an OS thread dedicated to
that task and runs only on it**. It is not multiplexed with other tasks, so calling a blocking C function
(`defffi`) inside stops only that thread, and other tasks make progress. Inside it, `task`, `send`, `recv`
and the rest can be used as they are.

- `Thread<T>` is the counterpart of `Task<T>`. Like `wait`, `join` stops **the calling task**, and the value
  is cached. When the task ends, the thread ends too.
- The panic rules are the same as for `task` (the whole process goes down). When `main` returns, the
  process ends.
- To write it as a function, use `(Thread::spawn (lambda () T body...))` (Rust's `std::thread::spawn`).
  The function value passed must be compiled. Called from a top level that `typl` is interpreting, it
  panics before starting the thread, the same as a `(panic ...)`, whether given a `lambda` or a named
  function. It can be used from inside compiled functions.
- **Only compiled code runs on a dedicated thread.** When `typl` evaluates `(thread (f ...))` while
  interpreting, it compiles `f` (and what it calls) on the spot before running it. A call that cannot be
  compiled (the value of an interpreted `lambda`, constructing a struct and so on) is, before the thread is
  started, a panic treated the same as a `(panic ...)`.

### 12.3 `select` — waiting on several channel operations at once

```lisp
(select
  ((v (recv ch1)) body...)          ; a receive arm. v is bound to an Option<T>
  ((send ch2 x) body...)            ; a send arm
  (else body...))                   ; optional. **if written, it goes last**
```

- **With `else`, it does not block** (Go's `default`). Without it, it waits until one becomes possible.
- **If several are possible at once, one is chosen at random** (in written order, later arms would
  starve).
- The `v` of a receive arm is an **`Option<T>`**. A closed channel is "an answer", not a reason to skip the
  arm, so `match` on it inside the arm.
- The type is **the join of the types of all the arms' bodies** (the same rule as `match` arms).
- `(select)` with zero arms is a type error (Go's `select{}`, blocking forever, is not adopted). A `select`
  with only `else` is too, since it is the same as writing the body directly.

**The channel expressions and the values to send are evaluated once each, from left to right, whichever
arm is chosen** (the same discipline `case` has for its keys).

```lisp
(select                             ; receiving with a timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([a channel that delivers after a time](functions/concurrency.md#5-after--a-channel-that-delivers-after-a-time))
is "a channel that delivers one value after `sec` seconds", corresponding to Go's `time.After`.

### 12.4 Interaction with other features

| Feature | How it relates to tasks |
|---|---|
| `catch` / `throw` | **Do not cross task boundaries.** A `throw` trying to leave a task's body is a panic |
| `unwind-protect` | The cleanup runs when a task ends naturally. **It does not run when the process ends because the main task ended** |
| `block` / `return-from` | Lexical, so they do not cross `lambda` boundaries |
| `panic` | As in Go, the whole process goes down. `wait` does not observe a panic as a value |
| `dlet` | **Not a per-task binding.** It still "borrows and returns a global", so tasks interfere with each other |
| Standard output | Shared by all tasks. The output of one `println` is never mixed with others in the middle of a line |
| `compile` / `eval` | No restrictions. `(compile f)` inside a task works |

### 12.5 Where tasks switch

Scheduling is cooperative, so **tasks switch only where you write it**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **channel operations that have to wait** (`send`/`recv`/`select`), and **socket operations
that have to wait** (`accept` / `tcp-connect` (including name resolution) / reading and writing sockets /
`recv-from`; [Networking](functions/network.md)). All sockets are non-blocking: if one is not ready, only
that task stops, and it resumes when the OS says it is ready, the same shape as Go's netpoller. Only when no
task can run does the implementation wait on the OS until the nearest `sleep` deadline.

Channel operations that can answer on the spot (a `send` with room in the buffer, a `recv` with a value
waiting, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **do not use up the turn**. This means you are
not interrupted unexpectedly by a read, and it is treated differently from `(sleep 0.0)`, which is CL's
"yield for 0 seconds".

**There is no preemption.** A tight loop that calls nothing starves other tasks. However, compiled loops
hand control to the scheduler periodically, so a compiled tight loop does not starve them.

### 12.6 Compiled code and tasks

Compiled code can suspend tasks too. The same goes for executables made with `compile-file`: `main` runs as
the scheduler's main task, and `task`, `sleep`, `wait`, channels and socket waits all work with the same
meaning as in `typl`. When `main` returns, the process ends and the remaining tasks are cut off (as in Go).
The interpreter is never put into the executable for the scheduler's sake.

The one exception is "inside a C FFI callback", where operations that **would have to wait** are errors
(friendlier than deadlocking silently): while a function passed with `defffi` is being called from C, C's
stack is on top, and there is no way to suspend the task and resume it later.

The following places are also functions called in the middle of a task, yet cannot suspend:
`print-object` methods, `~/name/` in `format`, reader macros, inside `eval`, and `defvar` initializers in
AOT executables. Here, **operations that answer without waiting go through** (`(recv ch)` with a value in
the buffer, `read-line` on a socket with data already received, `(task ...)`, `(yield)` and so on), and
**operations that would really have to wait are errors** (not stopping the process on the spot, but a
panic like `` `recv` cannot block: ... ``, treated the same as a `(panic ...)`).

### 12.7 Differences from Go

- **In `typl`, only compiled tasks go out to other threads.** The interpreter's state cannot be shared
  between threads, so tasks from an interpreted `task` run on the interpreter's thread. A compiled task
  too **moves to the interpreter's thread and stays there** (it does not go back) at the point where it
  calls an interpreted function value, calls a `:dyn` method no one has compiled, or calls
  `eval`/`macroexpand`/`read`. If a long computation touches interpreted code even once along the way, the
  rest runs on the interpreter's thread.
- **In `typl`, the workers live only for one top-level evaluation.** While the REPL waits for input, and
  between top-level forms, other threads do not advance tasks (remaining tasks continue from where they
  left off in the next evaluation). At the end of an evaluation, it waits for each thread to finish its
  current step, so if a C function (`defffi`) keeps blocking inside a `thread`, the evaluation does not end
  until it returns.
- **Printing on workers**: interpreted `print-object` / `~/name/` methods cannot run on other threads, so
  printing such values on another thread is a panic treated the same as a `(panic ...)` (`(compile
  T::print-object)`, or print from the main task).
- **Data races are undefined** (the same position as Go). The result of several tasks changing the same
  value without going through `Mutex<T>` / `Chan<T>` is not guaranteed.
- **`task` returns a value.** Unlike Go's `go` statement, it returns a `Task<T>`, and `(wait t)` gets the
  result.
- **There are no nil channels.** Go's fan-in idiom (setting a closed channel to `nil` to drop it from the
  arms of `select`) cannot be written, so start one task per input and join them with a `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--waiting-for-n-completions)). That is also the
  recommended way in Go, but it is **the first difference people coming from Go run into**.
