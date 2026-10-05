<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Time, Environment and Implementation

Functions for time, queries about the runtime environment, implementation tools, parsing and
evaluating text, docstrings, and macros.

## 1. Time

| Name | Form | Type | Description |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Two fields: `day` (days since 1900-01-01) and `second` (the second within that day, 0..86399) |
| `internal-time` | — | `defstruct` | Two fields: `second` and `microsecond` (within that second, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | The time since CL's epoch (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Elapsed time relative to the process |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | The **CPU time** this process has used (user plus system) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | As a number of seconds. The form for reporting the difference between two readings |
| `internal-time-units-per-second` | — | `int` | `1000000` (microseconds), the unit of the `microsecond` field. As in CL, the value is the implementation's choice |
| `time` | `(time form)` | Macro | Runs `form`, prints the real time and the CPU time on one line each, and returns the value of `form` as it is |

Real time and CPU time tell you different things. For work that mostly waits on I/O, the two differ
widely, and that difference is exactly what you want to know, so `time` shows both.

`sleep`, which stops a task, is in [Tasks and Channels](concurrency.md#3-yield--sleep--giving-way).

## 2. Decoding and encoding dates

| Name | Form | Type | Description |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Nine fields**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. CL's nine return values as one struct (there are no multiple values) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universal time into calendar components. `zone` is hours west of Greenwich (the same direction as CL). **Left out, it is local time** (as in CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | The reverse. Without `zone`, the arguments are read as **local time** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Now, decoded in local time |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | The local time's offset west of Greenwich, in **seconds**, at that universal time |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Whether daylight saving time was in effect at that universal time |

As in CL, for `day-of-week` **0 is Monday and 6 is Sunday**.

**Without `zone`, local time is used**, as in CL. The local offset is asked of the OS, so the result
depends on where the machine is. **Giving an explicit zone makes it deterministic**, and `0` is UTC.

The unit of `zone` is, as in CL, "hours west of Greenwich", so UTC+9 reads as `-9`. However, **the
argument is an integer and the `zone` field of the result is an `f64`**. Real offsets are not always
whole hours (India is +5:30, Nepal +5:45), and rounding the reported value would silently tell a lie.
A zone you write by hand is a whole number of hours, so the argument is `int`.

When `zone` is given, `daylight-p` is `false` and `zone` is exactly the value given, as CL specifies
(*If a time-zone is supplied, daylight saving time information is ignored*).

A local time that falls inside a daylight saving transition is not unique to begin with, and CL does
not say which to take. `encode-universal-time` returns one of the two answers for such a time.

## 3. The runtime environment

| Name | Form | Type | Description |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | The command line. **Element 0 is the program name** |
| `getenv` | `(getenv name)` | `string→Option<string>` | An environment variable. `none` if unset or not UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. The basis of `user-homedir-pathname` ([Pathnames](streams-files.md#92-functions)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | The implementation's version |
| `machine-type` | `(machine-type)` | `()→string` | The CPU architecture (`x86_64` / `aarch64` …). The value of the **build target** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | The host name |
| `machine-version` | `(machine-version)` | `()→Option<string>` | The name of the hardware **running now** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` where it cannot be determined |
| `software-type` | `(software-type)` | `()→string` | The OS (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | The OS release (`uname -r`, for example `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | A short name for the installation site. **Always `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Likewise, a long name. **Always `none`** |

The ones returning `Option` are items for which CL allows `NIL` (*or nil if no such name can be
determined*). POSIX has no place to record site names, so they are always `none`; SBCL returns the
same. Note the difference between `machine-type` and `machine-version`: the former is the architecture
this binary was **built** for, the latter is the chip **running** it now.

Element 0 of `command-line-args` is the script's path for `typl script.typl a b`, and the executable
itself for an AOT executable run as `./prog a b`. **Either way of running it reads the same arguments
at the same indexes** (`typl` removes its own name and options such as `--heap-cells` before passing
them on).

## 4. Asking the user

| Name | Form | Type | Description |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Takes a single `y` / `n`. Asks again until it gets one |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Makes the user spell out `yes` / `no`. For questions where a mistake is costly |

Both read from `*standard-input*`. Only the end of input stops the re-asking, and then the result is
`false`.

## 5. Implementation tools (CLHS 25.2)

The layer where the implementation answers questions about itself. `heap-info` / `room` / `dribble`
are ordinary functions; `trace` / `untrace` / `step` / `disassemble` / `ed` are **special forms**
(`trace` / `untrace` / `disassemble` / `ed` take the *name* of a definition, and `step` a *form*, all
unevaluated).

| Name | Form | Type | Description |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | The current state of the heap as a struct. The same numbers `room` prints |
| `room` | `(room &optional verbose)` | `(bool)→()` | Reports `heap-info` to `*standard-output*`. `(room true)` gives more detail |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Starts recording the session's output to `path` / stops recording when called without an argument |
| `trace` | `(trace name...)` | `Sexpr` | Reports calls of the named definitions to `*trace-output*`. Returns the list of names being traced now |
| `untrace` | `(untrace name...)` | `Sexpr` | Stops reporting. **With no arguments, removes all** |
| `step` | `(step form)` | The type of `form` | Evaluates `form`, stopping at each call to ask |
| `disassemble` | `(disassemble name [llvm])` | `()` | Prints what that definition becomes. The host machine code by default, LLVM IR with `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Starts `$VISUAL` / `$EDITOR`. Given a name, it opens the line where that definition is written |

`trace`/`untrace`/`step`/`disassemble` are interpreter-only, and functions that call them cannot be
compiled ([Syntax Reference chapter 10](../syntax.md#10-compilation)).

### 5.1 Fields of `heap-info`

| Field | Type | Contents |
|---|---|---|
| `capacity` / `live` / `free` | `int` | The whole cons arena and its breakdown. Always `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | The current counts of the heap's three other kinds of object |
| `gc-count` | `int` | The number of collections since the implementation started |
| `growable` | `bool` | Whether the arena can still grow |

The fields are all `int` (except `growable`). The growth limit (see the description of
`typl --heap-cells`) is not reported, because what a reader wants to know is whether it can still grow
(`growable`).

### 5.2 What `trace` / `step` can and cannot see

- **Definitions with compiled bodies are visible too, from call sites being interpreted.**
- **Call sites *inside* compiled code are not visible.** Tracing a name that has a compiled body adds a
  one-line note saying so. The same limitation SBCL describes for local calls.
- **Calls through closure values (`funcall`/`apply`) are not visible.** Closures have no names.
- **Generic definitions are not covered.** A copy for each type is made at each place of use, so there
  is no single body to name (the same reason, and the same wording, as when `compile` refuses).

The commands of `step` are `s` (step into this call; an empty line does the same), `n` (skip this
call), `c` (stop asking from here on) and `q` (abort). **If standard input is not a terminal, `step`
just evaluates `form`**: a degenerate behavior CLHS explicitly allows, so that scripts and tests do not
hang on a prompt no one can answer.

The `$VISUAL` / `$EDITOR` of `ed` is split at whitespace, so `EDITOR="code -w"` works. If neither is
set, the result is `Err`: it does not guess `vi`. The line number is passed first, in the form `+N`.

`dribble` records all three ways the session's output leaves the process: what
`print`/`println`/`format` write, what is written to streams connected to standard output, and the
lines typed into the REPL along with the values the REPL prints back.

## 6. Parsing and evaluation

All of these handle text and data from run time (which the program itself does not control), so on
failure they return the `Err` of a `Result` rather than panicking. The error types are concrete types
per operation ([Error types](option-result.md#3-error-types-and-the-error-trait)).

| Name | Form | Type | Description |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL's `parse-integer`. Skips leading and trailing whitespace (the same set as `trim`), reads at most one sign `+`/`-`, then digits in base `radix` (default 10, 2 to 36; digits above 10 in either case). There is no limit on the number of digits (`int`). Any other characters left over give `Err`. With `:junk-allowed true`, it stops at the first non-digit and ignores the rest, but gives `Err` if there is not a single digit (corresponding to CL's `nil`). It does not return CL's second value (the position where reading ended). A `radix` out of range panics (a mistake by the caller, not in the text) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | A floating-point number. Also accepts `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Reads one `Sexpr` from `s` (with the same reader that reads source code). Unbalanced parentheses, unterminated strings and the like give `Err`. Reading from a stream is `read-sexpr` ([Streams](streams-files.md#6-generic-functions-and-file-operations)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **the position where reading ended**. `(car r)` is the value and `(cdr r)` the position of the next character to read. `start` defaults to 0 |
| `read-from-string-preserving-whitespace` | Same as above | Same as above | The same, but does not consume the whitespace that ended the datum. The difference shows in the returned position |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Type-checks `form` at run time and evaluates it. Follows CL's `eval` |

CL returns **two values** (the value and the position) from `read-from-string`, but this language has no
multiple values, so it returns one `cons-cell`. Having the position makes reading a string one datum at
a time a loop rather than a rescan:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

The difference made by `preserving-whitespace` is **one whitespace character**: CL's `read` consumes
the whitespace that ended the datum, and `read-preserving-whitespace` leaves it. `(read-from-string "12 34")`
returns position 3, and the preserving version returns 2.

The number syntax the reader accepts is in [Syntax Reference chapter 1](../syntax.md#1-lexical-elements).
What `*print-radix*` ([Printing](printing.md#62-base-case-and-readability)) prints can be read back as it
is. There is no CL `*read-base*`.

### 6.1 What `eval` means

It follows CLHS `eval`: it evaluates in **the current global environment** (global functions,
variables, types and macros, including definitions added at run time) and in **the null lexical
environment** (the local bindings of the caller's `let`/`lambda` are not visible). Both expressions and
definitions (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) can be evaluated, and definitions are
registered in the global environment immediately and permanently.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; the global x is visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returns the defined name
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; the definition just made is visible
```

- **Return value**: for an expression, the result as an `Option<Sexpr>`; for a definition, the symbol of
  the defined name (as in CL). To use the result, take the `Sexpr` apart with `match`
  (`(int n)`/`(str s)`/…).
- **Differences due to static types (important)**: CL returns the actual value of the result, but in
  this language the return type can only be uniformly `Result<Option<Sexpr>,EvalError>`. Also, **code
  written statically cannot refer ahead to names that `eval` defines at run time**: a `(sq 9)` written
  directly in a file is checked before the `eval` that defines `sq` runs, and is "undefined". However,
  **later `eval`s can see it** (their type check runs at run time, after the definition). The REPL checks
  and runs one line at a time, so a name defined with `eval` can be called directly from the next line.
- **Errors**: type errors and syntax errors return `Err` (they do not panic). **Run-time panics** in the
  evaluated code (division by zero and so on) propagate as they would from code written directly. The
  cleanup of any `unwind-protect` in between runs
  ([Syntax Reference chapter 8](../syntax.md#8-non-local-exits-catch--throw--unwind-protect)).
- **Namespace**: when run by `typl file.typl` and inside an AOT executable, `eval` evaluates in the
  namespace of the script's module (the script's own globals are visible). The REPL evaluates in the
  root namespace.
- **Compilation**: both `read` and `eval` can be compiled. How they are handled in AOT executables, and
  the consequences (forms passed to eval are interpreted), are in
  [Syntax Reference 10.2](../syntax.md#102-eval-in-aot-executables).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (including inside `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` can carry docstrings. The position follows CL's rule for each:

| Form | Position of the docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | At the start of the body (after the return type and the `where` clause). Only when at least one body form follows; a lone string stays the return value |
| `defvar` / `defconstant` | **After** the initial value: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Right after** the name, before the fields/variants |
| `deftype` | **Right after** the name, before the type: `(deftype meters "doc" i32)` |
| `deftrait` | Right after the supertrait list, before the items. One for the whole trait. **Methods with a default implementation** can put their own docstring right before their body |

| Name | Form | Type | Description |
|---|---|---|---|
| `documentation` | `(documentation name)` | (special form; `name` is a bare symbol or `Type::method`)→`Option<string>` | Returns the docstring of `name` |

Like `quote`/`compile`, `documentation` is a special form (it reads `name` as an unevaluated name).
Unlike CL's `(documentation 'name 'function)`, it takes no type argument; instead it resolves a bare
name in the order **variable → function → type → trait → macro** (the same priority as for a bare
identifier evaluated as an expression). The `Type::method` form looks up the docstring of an associated
or static method.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**The value is decided at check time**: if the name does not resolve to any definition, it is a
check-time error (like referring to an undefined variable). If it resolves but there is no docstring,
the result is `Option::none`.

**Not covered**:

- `(setf documentation)` (changing a docstring at run time) does not exist.
- Module-qualified free names (`mod::name`; `Type::method` is supported) are not supported.
- A method declaration in a `deftrait` **without a body** cannot have a docstring. A trailing string
  literal would itself be the body (the return value) of a default implementation, so there is no way
  to tell the two apart.

The hover of the language server (`typl-lsp`) shows docstrings too.

## 8. Macros

How to define macros is in [Syntax Reference 3.14](../syntax.md#314-defmacro--macro-definitions).

| Name | Form | Type | Description |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | A new symbol. Its name is `" <prefix><n>"`, where `n` is `*gensym-counter*`. A leading space cannot be written in source, so the generated bindings never clash with written names |
| `*gensym-counter*` | Variable | `int` | The number `gensym` uses next. As in CL, it can be read and set |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expands a macro call one step. `none` means "not a macro call" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Repeats until it is no longer a macro |

`macroexpand-1` returns an `Option`. CL reports "whether it expanded" as a second return value, but
there are no multiple values, so `none` plays that role. **A macro that expands into a call of itself
can never be confused with a non-macro.** One step of expansion is the same one the type checker uses,
so what the program sees and what the checker saw never diverge.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none shows as the empty list (Option<Sexpr> is transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

What CL has and this language does not: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute`
always coincide, so there is no distinction to choose), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (uninterned symbols; bindings are looked up by name, so there
would be nothing to gain).

## 9. Local macro bindings (`macrolet` / `symbol-macrolet`)

Both are special forms that bind **names that are not values** lexically. Nothing remains at run time:
what gets compiled is the expanded form of the body.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- A `macrolet` binding hides a global macro of the same name **only during the body**. The lambda list is
  the same as for `defmacro` (`&optional`/`&rest`/`&key`).
- **Siblings of the same `macrolet` cannot see each other from their *bodies*** (as in CL; this is the
  difference from `labels`). Expansions are checked at the place of use, so `earlier` expanding into
  `(later ...)` works: both are visible at that place.
- A `symbol-macrolet` name enters the environment as an ordinary binding. So an inner `let` hides the
  same name, and an outer variable is hidden: CL's rules come out as they are.
- **`setf` writes to the expansion.** `(setf head 42)` is `(setf (get v 0) 42)`.
- Expansions are checked in **the environment of the place of use** (not the place of binding).

## 10. Other

| Name | Form | Type | Description |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panics if false. Without a message, `assertion failed: <the test as written>` (it is a macro, so it can name the expression itself). CL's restarts do not exist in this language |
| `warn` | `(warn control args...)` | `(string,...)→()` | Writes one line prefixed with `WARNING: ` to `*error-output*` and **continues**. A way to report something without returning a `Result` and without ending the program |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Replaces globals only during `body` and restores them on the way out. CL writes this as `let`, but `let` in this language always binds lexically, hence the separate name (the same role as the Emacs Lisp macro of the same name). Restores them however the body is left: normal completion, `throw`, `panic`, `break`/`return`. **Not a per-task binding** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Runs `body` with every printer control variable at its standard value and `*read-eval*` set to `true` ([Printing](printing.md#6-controlling-how-much-is-printed)) |
| `exit` | `(exit code)` | `int→!` | Ends the process |
| `dump` | `(dump path)` | `string→bool` | Writes the current environment (type information plus compiled bodies) to one file. `typl --image <path>` starts again from it. Interpreter-only ([Syntax Reference 10.1](../syntax.md#101-dumps)) |
