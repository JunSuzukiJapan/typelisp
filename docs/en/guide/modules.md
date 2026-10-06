<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Modules and File Layout

This guide explains how to put together a program made of several files. The detailed rules are in
sections 3.10 to 3.13 of the [Syntax Reference](../reference/syntax.md#310-module--use--namespaces).

## 1. One file is one module

In typelisp, **a file is a module by itself**. The file's path relative to the source root is the
module's path.

| File | Module |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

There is no need to write a module declaration at the top of a file.

## 2. Setting up a project

Put a file named `typelisp.toml` at the root of the project. It may be empty.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

To keep the sources under `src/`, write this one line in `typelisp.toml`:

```toml
src = "src"
```

`typl` looks for `typelisp.toml` starting from the directory of the file it runs and moving up, and
uses the place where it finds it as the source root. If none is found, the directory of the file
being run is the root (in the REPL, the current directory).

## 3. Making definitions public and using them

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; a field without pub cannot be read from outside

(defun square ((n i32)) i32 (* n n))   ; a function without pub cannot be called from outside either

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` is loaded at the point where `(use geometry)` is written. There is no need to load it
beforehand.

### What is made public

- Functions, structs, enums, global variables, macros and methods are visible from other modules only
  when they carry `pub`. Put `pub` right before the definition, as in `(pub defun ...)`.
- For structs, **making the type public and making fields public are separate**.
  `(pub defstruct point ...)` makes the type visible, and only the fields written as `(pub x i32)`
  can be read and written from outside.
- Using a name that is not public from outside gives a "cannot resolve" error such as
  `unresolved path: geometry::square`. It is the same message as for a misspelled name, so if the
  spelling is right and the name still does not resolve, suspect a missing `pub`.

The list of definitions that can take `pub` is in
[Syntax Reference 3.13](../reference/syntax.md#313-pub--visibility).

## 4. How to write `use`

```lisp
(use geometry)              ; bring in a module; write geometry::dist2 to use it
(use geometry::dist2)       ; bring in a function; use it by the bare name dist2
(use geometry::point)       ; bring in a type; write point::new, point::origin, and point in type annotations
(use a::f b::g)             ; several can be written together
```

- **`use` takes effect only for the forms after it.** Put it at the top of the file. Writing
  `geometry::dist2` above the `use` gives `unresolved path`.
- Writing the full path `geometry::dist2` without `use`-ing the module does not resolve either. Only
  `use` causes a file to be loaded.
- `use`-ing a name whose bare form is already taken gives a warning. When you mean to bring it in
  anyway, use `shadowing-import`.
- A module inside a directory is written `(use geo::shapes)`, and after that it is referred to by its
  last part (`shapes::...`).

### Calling trait methods

Methods implemented in an `impl` **belong to the type**, not to the module's functions, so they are
called without the module name.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, not core::area
```

Methods inside an `impl` are always public, even without `pub`.

A trait itself cannot be made public to other modules. Keep a trait's definition, the `impl`s for it
and the code that uses it through `:dyn` in a single module.

## 5. Splitting namespaces within a file

To split a namespace further within one file, use `module`. It is nested inside the file's own
module.

```lisp
;; inside main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

To put the whole rest of the file into one namespace, you can write `(in-module util)` instead of
wrapping it in parentheses.

## 6. Dependency restrictions

- **Cycles are not allowed.** If `a.typl` does `(use b)` and `b.typl` does `(use a)`, the result is
  the error `circular module dependency: a -> b -> a`. Move the definitions both need into a third
  module.
- **Neither types nor functions can be referred to before they are defined**, even within the same
  file. For mutually recursive functions, declare one of them first with `defsignature`
  ([Syntax Reference 3.2](../reference/syntax.md#32-defsignature--forward-declarations)).

## 7. Order of execution

Running `typl main.typl` proceeds in this order:

1. `main.typl` and every file `use`d from it are read and type-checked. **If there is a type error
   anywhere, nothing runs.**
2. The top-level expressions of `use`d modules run before those of the modules using them.
3. The top-level expressions of `main.typl` run from top to bottom.

If you gather the program's entry point into a `main` function and call `(main)` at the end of the
file, the same file can also be used for
[AOT compilation](compile.md#3-building-an-executable-with-aot-compilation).

## 8. How this differs from `load`

`(load "path")`, like Common Lisp's `load`, reads the contents of a file **into the current
namespace as they are**. It does not wrap them in a module, and `pub` plays no part. Use it for things
like reading a settings file or reloading a local file in the REPL. To split a program into parts,
use `use`.
