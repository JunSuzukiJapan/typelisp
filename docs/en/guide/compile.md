<!-- translated-from: docs/ja/guide/compile.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Compiling

Unless you do anything else, typelisp programs run in the interpreter. In addition there are two
ways to compile to native code, and one way to save an environment. The details of the
specification are in [Syntax Reference chapter 10](../reference/syntax.md#10-compilation).

| Method | How | Result |
|---|---|---|
| JIT compilation | `(compile name)` | A function in the running session is replaced by native code |
| AOT compilation | `typl -c src.typl` or `(compile-file "src.typl" "out")` | A standalone executable |
| Dump | `(dump "file.typld")` | Saves the definitions; `typl --image` starts again from the same environment |

## 1. Preparation

Compilation uses LLVM 22. If you have built `typl` following [README.md](../../../README.md), no
further preparation is needed.

Executables made by AOT compilation are linked against the static library `libtypelisp_front.a`. A
release build of `typl` (including one installed with `cargo install`) carries this library inside
itself, so no preparation is needed. The first time it compiles, it writes the library out to
`~/.typelisp/lib/<build ID>/` (or `$TYPELISP_HOME/lib/<build ID>/` if the environment variable
`TYPELISP_HOME` is set) and uses that copy from then on. `typl --remove-lib` deletes it (with
`--others`, the ones written by other versions of `typl`; with `--all`, all of them). A debug build of
`typl` uses the library in `target/debug/` of the repository it was built in. To use one placed
somewhere else, give its folder with `--lib-dir` when starting `typl` (section 3.2).
On macOS, linking uses the Xcode Command Line Tools.

## 2. JIT compilation

This turns an already defined function into native code on the spot.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; from here on, calls run the compiled code
```

- `name` is not evaluated. Write the function name as it is (not as a string). For a method, write it
  with the type name, as in `(compile point::norm)`.
- Functions it calls are compiled along with it.
- **Generic functions cannot be compiled.** A copy for each type is made at each place it is used.
  Compile the function that calls it with concrete types instead.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` and `dump` are interpreter operations, so a
  function that calls them cannot be compiled. Trying to compile it gives an error stating why.

To look at the result of compilation, use `disassemble`.

```lisp
(disassemble fib)          ; the host machine code
(disassemble fib true)     ; LLVM IR
```

## 3. Building an executable with AOT compilation

### 3.1 Writing the program

As the entry point, define a **`main` function that takes no arguments**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

The `(main)` at the end of the file is there so that `main` is called when you run
`typl hello.typl`. `compile-file` skips this final `(main)`, so the same file works both in the
interpreter and with AOT compilation.

### 3.2 Compiling

From the command line, use `typl -c` (`typl --compile` is the same).

```sh
$ typl -c hello.typl            # makes hello
$ typl -c hello.typl -o fib     # names the executable fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Without `-o`, the executable is named after the source file with `.typl` removed and is placed in
the same folder as the source file. If the source file name does not end in `.typl`, `-o` is
required. With `-c` (`--compile`), `--image`, `--heap-cells` and `--feature` cannot be given.

You can do the same by calling `compile-file` from the REPL or from a program.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

If you build repeatedly, you can put this one line in a file and run it with `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

File names are resolved from **the current directory where `typl` was started**, not from the
location of `build.typl`.

To link a `libtypelisp_front.a` placed somewhere other than where `typl` looks, give its folder with
`--lib-dir`. It applies to both `typl -c` and `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

If the given folder has no `libtypelisp_front.a`, `typl` stops with an error. The file works only
with the `typl` built together with it. After rebuilding `typl`, copy it again.

### 3.3 What an AOT-compiled file may contain

- The top level of the entry file may contain only definitions (`defun` `defmethod` `defvar`
  `defconstant` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`,
  `(unsafe (def-c-struct ...))`) and `use` `module`. Top-level expressions such as `(println ...)`
  are not allowed, except for the final `(main)`. Put the work inside `main`.
- `defmacro` cannot be written in the entry file. Define macros in another module with
  `(pub defmacro ...)` and `use` them.
- A file containing `defsignature` cannot be AOT-compiled, whether it is the entry file or a `use`d
  module.
- Without a `main` that takes no arguments, compilation fails with an error.
- The files of `use`d modules are compiled too and combined into a single executable.
- Libraries named with `:library` in `defffi` are linked automatically ([C FFI](ffi.md)).
- Every standard library function can be used with AOT compilation. `eval` can be used too, but then
  the type checker and the interpreter go into the executable, which makes it larger and slower to
  start. Programs that do not call `eval` do not include them.

### 3.4 How the executable behaves

- `(command-line-args)` returns a `Vector<string>` of the same shape whether run as
  `typl hello.typl a b` or as `./hello a b`. The first element is the program name.
- Set the exit code with `(exit n)`. If `main` returns normally, it is 0.
- On `panic`, the program prints the message and exits with a nonzero code.

## 4. Dumps

You can save the definitions of the current session to one file and start from it next time.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

It also works for running a file, as in `typl --image session.typld prog.typl`.

- What is saved are the **definitions**. Expressions evaluated in the session are not saved.
- Functions you `compile`d are saved in their compiled form.
- Global variables are restored by **running their initializers again**, not with the values they had
  when the dump was written.
- A dump cannot be loaded by a `typl` of a different version from the one that wrote it (it is an
  error).

If you run a file and `(dump ...)` from it, that file's definitions are in a module named after the
file. A function defined in `dp.typl` is named `dp::sq`, and calling it from another file requires
`pub` ([Modules and File Layout](modules.md)).

## 5. About compiled module files

There is no format, like Common Lisp's `.fasl`, for writing out the compiled result of each module
to a file. `compile-file` builds the executable directly from the sources. No intermediate files are
left behind.
