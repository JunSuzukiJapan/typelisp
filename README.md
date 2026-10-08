<!-- translated-from: README_JP.md @ 0569107727efd1d0e43f519b1568a9af2cb1890d -->
# TypeLisp

English | [日本語](README_JP.md) | [简体中文](docs/zh-CN/README.md) | [繁體中文](docs/zh-TW/README.md) | [한국어](docs/ko/README.md) | [Español](docs/es/README.md) | [Português (Brasil)](docs/pt-BR/README.md) | [Deutsch](docs/de/README.md) | [Français](docs/fr/README.md) | [Русский](docs/ru/README.md) | [العربية](docs/ar/README.md) | [Italiano](docs/it/README.md) | [हिन्दी](docs/hi/README.md) | [Türkçe](docs/tr/README.md) | [Tiếng Việt](docs/vi/README.md) | [Bahasa Indonesia](docs/id/README.md) | [Polski](docs/pl/README.md) | [ภาษาไทย](docs/th/README.md) | [Українська](docs/uk/README.md) | [Nederlands](docs/nl/README.md) | [Svenska](docs/sv/README.md)

TypeLisp is a statically typed Lisp.
Its syntax is modeled mainly on Common Lisp.

## Features

### Defining functions

A function definition states the type of each argument and the return type.
It looks like this:

```
(defun fun-name ((arg1 type1) (arg2 type2) ...) return-type
    body ...)
```

### Methods

```
(defstruct Point (x i32) (y i32))

(defmethod show ((p Point)) ()
  (println "Point: (x ~d y ~d)" p::x p::y))

(show (Point::new 1 2))
```

## Examples

### factorial

```
(defun factorial ((n int)) int
  (if (<= n 1)
    1
    (* n (factorial (- n 1))) ))

(let ((num (factorial 10)))
  (println "10! = ~d" num) )
```

### wc

```
(defun is-whitespace ((b int)) bool
  (case b
    ((#x20 #x09 #x0a #x0b #x0c #x0d) true)
    (else false)))

(defun main () ()
  (let ((args (command-line-args)))
    (when (< (len args) 2)
      (println "usage: wc file")
      (exit 0))

    (let ((path (get args 1))
          (char-count 0)
          (word-count 0)
          (line-count 0)
          (in-word false))
      (match (open-binary-input path)
        ((ok file)
         (progn
           (loop (match (read-byte file)
                   ((some b)
                    (progn
                      (incf char-count)
                      (when (= b #x0a)
                        (incf line-count))
                      (if in-word
                        (when (is-whitespace b)
                          (setf in-word false))
                        (unless (is-whitespace b)
                          (setf in-word true)
                          (incf word-count)))))
                   ((none) (break))))
           (close file)
           (println "~10d ~10d ~10d ~a" line-count word-count char-count path)))
        ((err e)
         (progn
           (println "wc: ~a" (message e))
           (exit 1)))))))

(main)
```

### Traits

```
(deftrait Animal ()
  (name ((self Self)) string)
  (sound ((self Self)) string)
  (speak ((self Self)) string
    (format false "~a says ~a" (name self) (sound self))))

(defstruct Dog (nick string))
(defstruct Cat (nick string))

(impl Animal Dog
  (name ((self Self)) string self::nick)
  (sound ((self Self)) string "Woof"))

(impl Animal Cat
  (name ((self Self)) string self::nick)
  (sound ((self Self)) string "Meow"))

(defun main () ()
  (let ((animals (the Vector<:dyn Animal> (Vector::new))))
    (push animals (Dog::new "Pochi"))
    (push animals (Cat::new "Tama"))
    (doiter (a (iter animals))
      (println "~a" (speak a)))))

(main)
```

### More examples

The programs in [examples/](examples/) can be run by passing the file name to `typl`. The wc and
trait examples above are also there, as [examples/wc.typl](examples/wc.typl) and
[examples/animals.typl](examples/animals.typl).

```sh
typl examples/fizzbuzz.typl
typl examples/wc.typl README.md
```

| File | Contents |
|---|---|
| [bst.typl](examples/bst.typl) | Binary search tree (`defenum` and `match`) |
| [factorial.typl](examples/factorial.typl) | Factorial (switches to arbitrary precision automatically past 63 bits) |
| [fibonacci.typl](examples/fibonacci.typl) | Fibonacci sequence (with a loop) |
| [fizzbuzz.typl](examples/fizzbuzz.typl) | FizzBuzz |
| [game_of_life.typl](examples/game_of_life.typl) | Conway's Game of Life |
| [maze_bfs.typl](examples/maze_bfs.typl) | Shortest path through a maze (breadth-first search) |
| [primes.typl](examples/primes.typl) | Sieve of Eratosthenes |

[examples/projects/](examples/projects/) holds programs split across several files.
Run one by passing its `src/main.typl` (for example, `typl examples/projects/todo-cli/src/main.typl`).

| Project | Contents |
|---|---|
| [echo-server](examples/projects/echo-server/) | An echo server that starts one task per connection |
| [expr-eval](examples/projects/expr-eval/) | An interactive arithmetic calculator (lexer, parser, evaluator) |
| [http](examples/projects/http/) | An HTTP/1.1 server and client (with TLS) |
| [mini-lisp](examples/projects/mini-lisp/) | A REPL for a small Lisp |
| [shape-canvas](examples/projects/shape-canvas/) | Draws shapes on a character canvas (`:dyn` and error types) |
| [todo-cli](examples/projects/todo-cli/) | A command-line to-do manager |

## Installation

On macOS (Intel or Apple Silicon), you can install prebuilt `typl` and `typl-lsp` binaries with
this command:

```sh
curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | sh
```

They are installed into `~/.typelisp/bin`; add it to your `PATH` as the installer tells you. To
build executables with `typl -c`, you also need the Xcode Command Line Tools
(`xcode-select --install`). The prebuilt `typl` runs on macOS 26 or later on Apple Silicon, and on
macOS 15 or later on Intel. On Apple Silicon with a macOS older than 26, `typl` prints a one-line
warning at startup. You can keep using it, but code that `typl` compiles while running may crash if
it uses `panic`, `throw` or `unwind-protect`.

The environment variable `TYPELISP_HOME` sets the install location, and `TYPELISP_VERSION` sets the
version (the latest release if not given).

```sh
curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | TYPELISP_VERSION=0.2.1 sh
```

To uninstall, first delete the libraries `typl` has written out, then delete the install directory.

```sh
typl --remove-lib --all
rm -rf ~/.typelisp
```

To build from source, see "Building" below.

## Building

### Requirements

- Rust (cargo)
- LLVM 22
- On macOS, the Xcode Command Line Tools; on Linux, a C compiler (both are used for linking)

On macOS, LLVM 22 can be installed with Homebrew.

```sh
brew install llvm@22
```

On Linux, install the LLVM 22 development files from your distribution's packages. Building and
testing are checked on Ubuntu 24.04, Debian 13 and Fedora 44.

- Ubuntu / Debian: add the [apt.llvm.org](https://apt.llvm.org/) repository and install
  `llvm-22-dev` and `libpolly-22-dev`. You also need `build-essential`, `pkg-config`,
  `zlib1g-dev`, `libzstd-dev` and `libxml2-dev`.
- Fedora: install `llvm-devel`, `llvm-static`, `gcc`, `gcc-c++`, `zlib-devel`, `libzstd-devel`,
  `libxml2-devel` and `libffi-devel`.

### First-time setup

After cloning, run this script once:

```sh
scripts/setup-cargo-env.sh
```

The script finds LLVM 22 (with `brew --prefix llvm@22` on macOS; on Linux with `llvm-config-22`,
or with `llvm-config` if it belongs to LLVM 22) and generates `.cargo/config.toml`. The file differs
from machine to machine, so it is not tracked by Git. On macOS the script also writes the minimum OS
version used for the build (`MACOSX_DEPLOYMENT_TARGET`). Run it again whenever you update the Rust
toolchain.

If the script cannot determine the minimum OS version, it says so and stops. In that case, give the
value yourself:

```sh
scripts/setup-cargo-env.sh --deployment-target 15.0
```

If you would rather not generate the file, run cargo commands through `scripts/with-llvm-env.sh`.
It sets the same values as environment variables on every run.

```sh
scripts/with-llvm-env.sh cargo build
```

If you do not use Homebrew, set `LLVM_SYS_221_PREFIX` to where LLVM 22 is installed. You can build
without setting the minimum macOS version (`MACOSX_DEPLOYMENT_TARGET`); if it is not set, the
version that Rust's standard library targets is used.

### Build

```sh
cargo build            # debug build (target/debug/)
cargo build --release  # release build (target/release/)
```

This produces two executables:

| Executable | Role |
|---|---|
| `typl` | The language implementation itself (see "Running" below) |
| `typl-lsp` | The language server (for editor integration) |

### Installing with cargo install

To install `typl` and `typl-lsp` into `~/.cargo/bin`, run this in your clone of the repository:

```sh
cargo install --locked --path .
```

`cargo install --path` reads the `.cargo/config.toml` generated in "First-time setup". If you have
not generated it, run `scripts/with-llvm-env.sh cargo install --locked --path .` instead. If you do
not use Homebrew, set `LLVM_SYS_221_PREFIX` and then run `cargo install --locked --path .`.

The installed `typl` carries the static library it needs for compiling, so `-c` and `compile-file`
keep working even if you move or delete the repository afterwards (see "Compiling a file" below).

## Running

There are three ways to start `typl`. The examples below assume `typl` is on your `PATH` (because
you installed it with `install.sh` or `cargo install` and added it to `PATH`, or because you added
`target/debug/` or `target/release/` of your build to `PATH`).

### Interactive mode (REPL)

Started without arguments, `typl` enters interactive mode (the REPL). Each expression you type is
evaluated on the spot and its result is printed. Type `:quit` or `:exit` to leave.

```sh
typl
```

### Running a file

Given just a file name, `typl` runs that file. Arguments after the file name are passed to the
program (it receives them with `(command-line-args)`).

```sh
typl foo.typl
typl foo.typl a b c
```

### Compiling a file

With `-c` or `--compile`, `typl` compiles the file into an executable. Without `-o`, the executable
is named after the file with `.typl` removed (`foo` in this example).

```sh
typl -c foo.typl
typl --compile foo.typl -o bar
```

The executables are linked against the static library `libtypelisp_front.a`. A release build of
`typl` (including one installed with `cargo install`) carries this library inside itself and writes
it out to `~/.typelisp/lib/<build ID>/` the first time it compiles; from then on it uses the copy it
wrote. The environment variable `TYPELISP_HOME` changes where it is written (with `TYPELISP_HOME`
set, the location is `$TYPELISP_HOME/lib/<build ID>/`). The build ID is derived from the contents of
the library, so after you install a different version of `typl`, the next compile writes into a new
directory.

These commands delete the libraries that have been written out:

```sh
typl --remove-lib           # delete the one this typl wrote
typl --remove-lib --others  # delete the ones other versions of typl wrote
typl --remove-lib --all     # delete all of them
```

Running `typl --remove-lib` before uninstalling `typl`, and `typl --remove-lib --others` after
reinstalling it, leaves no unused libraries behind. Anything deleted is written out again the next
time `typl` compiles. Only the build ID directories under `lib/` are deleted; no other files are
touched.

A debug build of `typl` uses the library in `target/debug/` of the repository it was built in, and
writes nothing out.

To use a library placed somewhere else, pass its folder with `--lib-dir`.

```sh
cp target/debug/libtypelisp_front.a ~/lib/typelisp/
typl --lib-dir ~/lib/typelisp -c foo.typl
```

`libtypelisp_front.a` works only with the `typl` that was built together with it. After rebuilding
`typl`, copy the library again.

### Other options

```sh
typl --help        # list the options
typl --version     # print the version
typl --remove-lib  # delete written-out libraries (--others / --all also work)
```

## Documentation

The full list of English documents is in [docs/en/README.md](docs/en/README.md).

### Tutorial

- [Getting Started](docs/en/tutorial/intro.md)
- [Type Basics](docs/en/tutorial/types.md)
- [Traits](docs/en/tutorial/traits.md)
- [Macros](docs/en/tutorial/macros.md)
- [Error Handling](docs/en/tutorial/errors.md)
- [Concurrency](docs/en/tutorial/concurrency.md)

### Guides

- [Modules and File Layout](docs/en/guide/modules.md)
- [Compiling](docs/en/guide/compile.md)
- [File I/O, Streams and Networking](docs/en/guide/io.md)
- [C FFI](docs/en/guide/ffi.md)
- [Editor Integration](docs/en/guide/editors.md)
- [For Common Lisp Programmers](docs/en/guide/from-common-lisp.md)

### Reference

- [Syntax Reference](docs/en/reference/syntax.md)
- [Built-in Functions](docs/en/reference/functions/README.md)
- [Types](docs/en/reference/types.md)
- [Error Messages](docs/en/reference/errors.md)

### Editor integration

- [Emacs (typelisp-mode)](editor/emacs/README.md)
- [VS Code](editor/vscode/README.md)

### Other languages

- [日本語](docs/ja/README.md)
- [简体中文](docs/zh-CN/README.md)
- [繁體中文](docs/zh-TW/README.md)
- [한국어](docs/ko/README.md)
- [Español](docs/es/README.md)
- [Português (Brasil)](docs/pt-BR/README.md)
- [Deutsch](docs/de/README.md)
- [Français](docs/fr/README.md)
- [Русский](docs/ru/README.md)
- [العربية](docs/ar/README.md)
- [Italiano](docs/it/README.md)
- [हिन्दी](docs/hi/README.md)
- [Türkçe](docs/tr/README.md)
- [Tiếng Việt](docs/vi/README.md)
- [Bahasa Indonesia](docs/id/README.md)
- [Polski](docs/pl/README.md)
- [ภาษาไทย](docs/th/README.md)
- [Українська](docs/uk/README.md)
- [Nederlands](docs/nl/README.md)
- [Svenska](docs/sv/README.md)

## License

typelisp is available under your choice of either of these two licenses:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

Either way, the exception in [LICENSE-EXCEPTION](LICENSE-EXCEPTION) applies as well. Executables
built with `typl -c` or `compile-file` contain parts of typelisp, but you may distribute them
without including typelisp's copyright notice or license text for those parts.

Unless you explicitly state otherwise, any contribution to typelisp is treated as offered under the
same terms as above (either of the two licenses, plus the exception).
