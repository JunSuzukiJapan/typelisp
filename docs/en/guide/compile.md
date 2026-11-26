<!-- translated-from: docs/ja/guide/compile.md @ 1b5dc6ac828d8e4a95800a91218b5e9dffe83f98 -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Without `-o`, the executable is named after the source file with `.typl` removed and is placed in
the same folder as the source file. If the source file name does not end in `.typl`, `-o` is
required. With `-c` (`--compile`), `--image` and `--feature` cannot be given. `--heap-cells` given with `-c` is the capacity of the executable's cons arena ([3.5](#35-resources-the-executable-uses)).

With `--release`, `typl` makes a **release build**, which removes `eprint`/`eprintln` (standard error output for investigating; [Printing](../reference/functions/printing.md)) together with their arguments. What it removes is still type-checked, so a type error there makes the compilation fail. `--release` is an option of `typl` as a whole: it works the same not only for `-c` but also when running a file, in the REPL, and for `typl test`. A `compile-file` called inside a `typl` given `--release` also makes a release build, and `eprint`/`eprintln` are removed from code `eval`ed inside the executable made that way too.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # the same when written after -c
```

`-O0`, `-O1`, `-O2` and `-O3` set the **optimization level**. The default is `-O0`, which does not optimize. From `-O1` on, the compiler first does simple optimizations, such as replacing a variable bound only to a literal with its value and computing integer arithmetic on constants in advance, and leaves the rest to LLVM's optimization at the level of the same name. At every level the results of the computation are the same. Like `--release`, it is an option of `typl` as a whole: besides the executable `-c` makes, it works on the code compiled while running a file, in the REPL and in `typl test`, and on a `compile-file` called there. Code the interpreter runs is not optimized.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # the same when written after -c
```

You can do the same by calling `compile-file` from the REPL or from a program.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- The top level of the entry file may contain only definitions (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) and `use` `module`. Top-level expressions such as
  `(println ...)` are not allowed, except for the final `(main)`. Put the work inside `main`.
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

### 3.5 Resources the executable uses

`typl -c` has options that say what resources the executable may use while it runs.

```sh
$ typl --heap-cells 100000 -c hello.typl    # fix the cons arena at 100000 cells
$ typl -c hello.typl --max-memory 64M       # allocate at most 64 MiB
$ typl -c hello.typl --no-cons              # use no cons cells
```

- `--heap-cells N` fixes the capacity of the cons cell arena at `N` cells. The arena does not grow
  past it: if garbage collection cannot free a cell, the program stops with a runtime error. Without
  it, the arena starts at 65536 cells and grows as needed.
- `--max-memory SIZE` bounds the total memory the executable allocates. Everything counts: the cons
  arena, strings, values such as vectors and hash tables, and the memory the runtime itself uses.
  `SIZE` is a number of bytes and may be followed by `K`, `M` or `G` (1024, 1024² and 1024³ times).
  An allocation that would go past the bound is reported, and the program exits with exit code 1.
  This exit cannot be caught with `catch`. Without it, there is no bound.
- `--no-cons` builds an executable that uses no cons cells. It has no cons arena, so a program that does something needing cons cells is a **compile error**, which shows that operation and the chain of functions from `main` that reaches it. Cons cells are needed to build a `Sexpr` list (including a quoted list and one built by quasiquote) or a `&rest` argument, and by `read`, `eval` and `macroexpand`. Operations on numbers, strings, vectors, hash tables, structs, channels and the like, and `print`/`println`/`format`, use none. It cannot be given with `--heap-cells`, and is written after `-c`.

`--max-memory` is written after `-c`. `--heap-cells` may be written before or after `-c`. The cons
arena counts toward the memory bound, so a bound the arena does not fit in stops the program before
it starts.

### 3.6 Compiling for WebAssembly

With `--target wasm32-wasip1`, `typl -c` makes a WebAssembly module (a WASI preview 1 command) instead of an executable. Without `-o`, it is named after the source with `.typl` replaced by `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # makes hello.wasm
$ wasmtime hello.wasm a b                        # run it with a runtime that supports WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- A `typl` installed with `install.sh` can be used as it is. One installed with `cargo install` needs preparation (which also covers section 3.7). First run `rustup target add wasm32-wasip1 wasm32-unknown-unknown` with rustup. Then install it with `--features wasm` (`cargo install --locked --path . --features wasm`).
  A `typl` installed without it has no WebAssembly runtime, and `--target` is an error. To use a debug build of `typl` in the repository, run `scripts/build-wasm-runtime.sh` in the repository instead.
- Linking uses the linker of the rustup toolchain (`rust-lld`), so rustup is needed when compiling too.
- The module behaves as the executable does: standard output, standard error and the exit code, `(command-line-args)`,
  reading and writing files, `throw`/`catch`/`panic`, tasks, channels and `sleep`, and `eval` all work.
  Tasks run one after another on a single thread. Files can be read and written only inside the directories
  the runtime exposes to the module (with wasmtime, name them with `--dir`).
- A program that uses something WebAssembly does not have is a **compile error**. The error shows what was used and
  the chain of functions from `main` that reaches it. What cannot be used: sockets (TCP, UDP, Unix domain,
  TLS, name resolution), OS threads (`Thread`), C functions (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  and `ed`.
- WASI has no way to learn the time zone, so local time is unknown. `decode-universal-time` and `encode-universal-time` without `zone`, and `get-decoded-time`, panic at run time. They work when `zone` is given.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` and `--release` work as they do for an executable.

### 3.7 Running in a browser or Node.js

With `--target wasm32-unknown-unknown`, `typl -c` makes a WebAssembly module to be loaded and run from JavaScript. Beside the module (`hello.wasm`), in the same folder, it writes the JavaScript that loads and runs it (`hello.mjs`). The preparation is the same as in section 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # makes hello.wasm and hello.mjs
$ node hello.mjs                                       # run it with Node.js
args: #()
fib(25) = 75025
```

From other JavaScript, call `run` from `hello.mjs`. `write` receives the output as text (`fd` is 1 for standard output, 2 for standard error). The value of `run` is the exit code.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* show it on the page, for example */ } });
```

- Without `write`, Node.js writes to standard output and standard error, and a browser to the console, a line at a time.
- In a browser, serve `hello.wasm` over HTTP from the same place as `hello.mjs` (it cannot be loaded from `file://`).
- `sleep` waits with `Atomics.wait`. A browser allows that only inside a Worker of a cross-origin isolated page, so run a program that uses `sleep` in a Worker.
- Local time (`decode-universal-time` without `zone`, and so on) uses the time zone JavaScript's `Date` reports.
- Besides what section 3.6 cannot use, file and directory operations, environment variables, `(command-line-args)`, the home directory and `dribble` are a **compile error**. Standard input reads as empty.

### 3.8 Compiling for a microcontroller (Cortex-M)

With `--target thumbv7em-none-eabihf`, `typl -c` makes an executable (ELF) for a Cortex-M4F or M7 board without an operating system. Without `-o`, it is named after the source with `.typl` replaced by `.elf`. With `--target thumbv8m.main-none-eabihf`, the executable is for a Cortex-M33 board.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # makes hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # run it on QEMU's Cortex-M4
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # run it on QEMU's Cortex-M33
```

- A `typl` installed with `install.sh` can be used as it is. To install with `cargo install`, run `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf`, then install with `--features embedded` (`--features wasm,embedded` for both). With a debug build of `typl` in the repository, run `scripts/build-embedded-runtime.sh`. Linking uses the linker of the rustup toolchain.
- Give the board's memory with `--linker-script FILE`. It is written as `cortex-m-rt`'s `memory.x` is: the addresses and sizes of FLASH and RAM. A line like `_typelisp_stack_size = 32K;` sets how much of the RAM the stack takes (64KiB if not given); the rest of the RAM is the heap. Without the option, the layout is that of QEMU's `mps2-an386` (`mps2-an505` for a Cortex-M33).

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` chooses who gives output, input, exiting and the clock (the board's functions).
  - `semihosting` (the default): given over semihosting, for running under QEMU (`-semihosting-config enable=on`) or a debugger.
  - `c`: the board gives them as C functions. Pass an object or static library that defines the functions below with `--link FILE` (`--link` can be repeated; a vendor's library that `defffi` calls is passed the same way).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 is standard output, 2 standard error
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; the bytes read, 0 at the end
  void     typelisp_host_exit(int32_t code);                              // does not return
  uint64_t typelisp_host_now_us(void);                                    // microseconds on a clock that never goes back
  int64_t  typelisp_host_unix_us(void);                                   // microseconds since 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // waits us microseconds
  ```

- Read a board's registers (peripherals mapped into memory) with `volatile-read-u8`, `volatile-read-u16` and `volatile-read-u32`, and write them with `volatile-write-u8`, `volatile-write-u16` and `volatile-write-u32`. The address is an `int` or an integer of a fixed width such as `u16`, and the value read has the type of the width (`u8` and so on). Each reads or writes exactly once, where it is written; the compiler neither leaves it out nor moves it. All of them can be used only inside `unsafe`. An address that is negative, or not a multiple of the width, is a runtime error before anything is read or written.

  ```lisp
  ;; UART 0 of QEMU's mps2-an386: data 0x40004000, state 0x40004004, control 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; wait while it is sending
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; enable sending
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" and a newline
  ```

- Write the function called when an interrupt comes as a `defun` with the attribute `#[interrupt(N)]` (the peripheral interrupt `N`) or one like `#[exception(SysTick)]`. Enable an interrupt with `(enable-interrupt N)`. A handler runs at once, and only when it comes in the middle of the runtime's work does it run after that work is done. A handler cannot wait (`sleep` is a compile error). The details are in section 3.17 of the [syntax reference](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; raise interrupt 5 itself (the NVIC's pending register)
  ```

- The runtime starts the program (the vector table and getting RAM ready) and allocates memory. A HardFault or an interrupt nothing handles ends the program with the exit code of a panic (1).
- `throw`/`catch`/`panic`, bignums, ratios and floats, and tasks, channels and `sleep` behave as in the executable. Tasks run in turn on the one core. C functions (`defffi`) can be used.
- A program that uses what the board does not have is a **compile error**: besides what section 3.7 cannot use, `eval` and `macroexpand`. Local time is unknown (as in section 3.6).
- The cons arena starts at 1024 cells and grows when it needs more. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` and `--release` work as they are.

### 3.9 Compiling for an Arduino Uno (AVR)

With `--target avr-atmega328p`, `typl` makes an executable (ELF) for the microcontroller of an Arduino Uno (ATmega328P). The following program blinks the board's LED.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; make it an output
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; turn it over
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # makes blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # run it in QEMU
```

- There is no runtime, so a program that uses the runtime is a **compile error**. The error says what is needed and through which functions it is reached. What can be used: arithmetic on fixed-width integers (`i8` to `u32`), `f32`, `f64`, `bool` and `char`, function calls (recursive ones too), conditionals, loops, `defvar` and `defconstant` holding those values, `volatile-read-u8` and the like, and calls of C functions.
- What cannot be used: values that live in the heap, such as strings, lists and structs; `catch`, and calls of function values; calls of C functions that take a string or a function; mathematical functions of floating-point numbers such as `sin` and `expt` (`sqrt`, and rounding such as `floor`, can be used).
- An interrupt handler is written `#[interrupt(N)]` ([Syntax](../reference/syntax.md), section 3.17). `N` is the ATmega328P's number of the interrupt, 1 to 25 counting reset as 0 (timer 1 overflowing is 13). See the example below.
- Dividing by 0 stops the microcontroller there (no interrupt comes any more). So does `/` of the smallest signed value by `-1`.
- An `int` cannot be used (it is a compile error), because it has no fixed width. Use a type of a fixed width, such as `i16` or `u16`. Addresses can be calculated in integers of a fixed width too. What is worked out at compile time can be used, such as an address written as a number and an `int` of a `defconstant`.
- When a recursion runs out of stack, the microcontroller stops.
- Declare a C function with `defffi`, and pass an object file compiled for AVR with `--link FILE`.
- A loop that does nothing is removed at `-O1` and above. In a loop that waits, read a register, for example.
- When `main` ends, the microcontroller stops.
- `--linker-script FILE` changes where memory is. Without it, memory is 32 KiB of flash less the last 512 bytes, which are the boot loader's, and 2 KiB of RAM. Write a RAM address with `0x800000` added, as in `0x800100`.
- `f32` and `f64` arithmetic is done by helper functions. Those that are used go into flash (those for `f64` are larger).
- Linking uses the linker of the rustup toolchain, as in section 3.8.

An example that blinks the LED by a timer's interrupt. An interrupt is enabled in the device's own register.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; timer 1 overflowing
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; turn it over
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; enable the overflow interrupt
          (volatile-write-u8 TCCR1B 3))           ; count at 1/64 of the clock
  (while true))
```

- A program with a handler lets interrupts come once its global variables are initialized. Set the devices up at the start of `main`.
- `enable-interrupt` and `#[exception(NAME)]` cannot be used (a compile error).
- While a handler runs, other interrupts wait. Like any other function, a handler cannot use the runtime.
- One read or write of a global variable is not broken into by a handler, even of a value of two bytes or more. Interrupts are disabled for this when reading a variable a handler writes and when writing a variable a handler reads or writes, and a warning at compile time names the variable. An interrupt can come between the read and the write of an update (`(setf n (+ n 1))`). To update a variable a handler also writes, read the status register (address `#x5F`) and keep it, write 0 to it to stop interrupts, and write the kept value back afterwards.
- An interrupt with no handler stops the microcontroller.

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
