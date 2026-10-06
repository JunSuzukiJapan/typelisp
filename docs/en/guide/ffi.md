<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C FFI (defffi)

This guide explains how to call C functions from typelisp. The list of types that can be declared and
the restrictions are in [Syntax Reference 3.3](../reference/syntax.md#33-defffi--declaring-c-functions-ffi).

## 1. Declaring and calling a function

`defffi` declares the name and types of a C function.

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

Calls are wrapped in `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` is needed because the compiler cannot check that the declared types match the real types on
the C side. Writing `unsafe` means you, the writer, take responsibility for that check. Forgetting it
gives an error that explains this.

## 2. Writing a safe wrapper

The intended use is to confine `unsafe` to one place and present an ordinary function to the outside.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. How types correspond

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Integers of the same width |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (also `size_t`, `int64_t` and so on) |
| `ptr` | Any pointer (`void *`, `FILE *` and so on) |
| `(ptr T)` | A pointer to `T` ([section 7](#7-c-structs)) |

### Strings

- A `string` you pass is copied into a NUL-terminated C string, which is freed after the call
  returns. A NUL in the middle of the string is an error.
- The result of a function that returns `string` is copied as well. The C-side memory is not freed.
  For functions that return a string the caller must free (such as `strdup`), take the result as
  `ptr` and `free` it yourself.
- If a function declared to return `string` returns NULL, it is an error. Take the result of
  functions that may return NULL (such as `getenv`) as `ptr`.

### `c-long` / `c-ulong` / `ptr`

These types exist only to pass values across the boundary with C, and **support no arithmetic**. To
use one as a typelisp integer, convert it with `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

A `ptr` is a value to be handed back to C functions. There is no way to read what it points to from
the typelisp side.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

These types can appear only as function arguments, return values and local variables. They cannot be
struct fields, global variables, or type arguments of `Vector` and the like.

## 4. Naming a library

Without `:library`, the symbol is looked up in what is already linked into the process (libc and so
on). Functions from other libraries need `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- A short name such as `"sqlite3"` is looked up as `libsqlite3.dylib`, then `libsqlite3.so`.
- A name containing `/` is treated as a path.
- If the declared symbol is not found, the error names it.

## 5. AOT compilation

Programs using `defffi` can be turned into executables with
[`compile-file`](compile.md#3-building-an-executable-with-aot-compilation) as they are. Libraries named
with `:library` are added at link time automatically, so `compile-file` needs no extra arguments.

## 6. Callbacks

You can pass a typelisp function to a C function and have it called back. Write a function type
among the argument types of `defffi`, and at the call put a function name or a `lambda` expression in
that position.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Only functions **with no free variables** can be passed. Top-level functions, `lambda`s and local
  `labels` functions all work, but referring to a local variable of an enclosing scope is an error
  at type-checking time. C passes only the declared arguments, so there is no way to deliver captured
  variables. To keep state, use global variables.
- A variable holding a function cannot be passed. Write a function name or a `lambda` expression in
  place.
- A `panic` or `throw` inside the callback reaches the caller after the C function returns.
- The callback can be called only while the C function that typelisp called is running. It cannot be
  used from things like `atexit` or signal handlers.

## 7. C structs

To pass something like an array of structs to a C function, declare a struct with the same layout as
in C using `def-c-struct`, and allocate it inside `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` allocates `n` values of `T` and returns a `(ptr T)`. `(c-ref p i)` is a pointer to
  the `i`th one, `p::field` is a field, and `(c-deref p)` is what a pointer to a scalar such as `i32`
  points to. All of them can be written with `setf`.
- `(as ptr p)` turns it into an untyped `ptr` for passing to C functions that take a `void *`.
- The size of `item` (8 here) and the position of each field are determined by the same rules as in
  C.

### Lifetime of allocated memory

Allocated memory is freed when control leaves the outermost `unsafe` in that function. The same
happens when it is left by `panic` or `throw`. Because of this, a `(ptr T)` value cannot be taken
outside the `unsafe`. Making it the value of the `unsafe`, capturing it in a closure, passing it to a
`task` and throwing it with `throw` are all type errors. Copy the values you want to use outside into
numbers or a `defstruct` inside the `unsafe`.

When allocating inside a `lambda` or a `labels` function, write an `unsafe` inside that function.

### Memory allocated by C

A pointer received from C as a `(ptr T)` (a `defffi` return value, a callback argument and so on) is
an error unless it points inside memory allocated with `c-alloc`. Declare functions that receive
memory C allocated with `malloc`, or NULL, with the untyped `ptr`.

## 8. What cannot be done

- **Variadic functions** (`printf` and the like) cannot be declared. The variadic part is passed by
  different rules from the fixed arguments. Declare a separate name for each number of arguments you
  use.
- **Passing or returning structs by value** is not possible. Use functions that pass pointers.
- **Generic declarations** are not possible.
- **The same name as a built-in function** cannot be used.
- **They cannot be passed as function values.** You cannot pass one as in `(map xs c-abs)`; wrap it in
  a `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
