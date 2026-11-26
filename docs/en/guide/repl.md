<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# The REPL

Starting `typl` with no arguments gives you the REPL (interactive mode). Beyond evaluating
expressions, this document covers what else the REPL can do: its commands, help while typing,
referring to previous results, stopping a computation, and investigating after an error.

## Starting and quitting

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Quit with `:quit` (short: `:q`) or `Ctrl+D`. Input history is kept in `~/.typl_history`, so the
next time you start you can bring it back with `Ctrl+P` / `Ctrl+N` or `Ctrl+R` (history search).

On startup, before the first prompt, the following run in order. None of them print the values
they produce.

1. The contents of `~/.typlrc`, if it exists. `--no-init` skips it.
2. `--eval FORM` and `--load FILE` from the command line, in the order written. Both can be given
   any number of times.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` and `--no-init` are only for starting the REPL. Giving them when running a file
is an error.

The REPL starts in the module `user`. When you move to another module with `(in-module foo)`, the
prompt shows that module.

```
typl> (in-module foo)
typl user::foo>
```

## REPL commands

A `:name` written at the start of a line is a REPL command. `:help` lists them. A `:name` that is
not in the list is not a command; it is evaluated as an ordinary keyword.

| Command | Meaning |
|---|---|
| `:help` (`:h`, `:?`) | list the commands |
| `:type FORM` (`:t`) | print the type of FORM without evaluating it |
| `:describe NAME` (`:d`) | print what kind of definition NAME is, its type, fields or variants, methods, the traits it implements, where it was defined, and its documentation string |
| `:apropos TEXT` | find the definitions in every module whose name contains TEXT (case is ignored) |
| `:src NAME` | print the source of a definition |
| `:who-calls NAME` | print the functions and methods that call the function or method NAME |
| `:inspect FORM` (`:i`) | evaluate FORM and open its value according to its type |
| `:backtrace` (`:bt`) | print the calls that were in progress when the previous line failed, innermost first |
| `:locals [N]` | print the arguments of call N of `:backtrace` (call 0 if omitted) |
| `:cd [DIR]` | change the current folder to DIR (the home folder if omitted) |
| `:pwd` | print the current folder |
| `:quit` (`:exit`, `:q`) | quit the REPL |

NAME can be a name such as `inner`, or a path such as `std::seq::reverse` or `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Adds 1 to x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Adds 1 to x.
typl> :src inner
(defun inner ((x int) (s string)) int "Adds 1 to x." (+ x 1))
```

`:src` shows a definition loaded from a file from that file, and a definition typed at the REPL
from the text that was typed. Definitions in the standard library have no recorded source
location, so their source cannot be shown.

`:who-calls` searches the bodies of the functions in the current session. For a generic function,
only the uses that have been instantiated at concrete types are found.

`:cd` changes the folder `(load ...)` looks in, but `use` keeps looking for files in the folder
the REPL was started in.

## Opening a value (`:inspect`)

`:inspect` opens a value according to its type. A struct opens into its fields, an enum (including
`Option`) into the fields of its variant, a `Vector` into its elements, and a `HashTable` into its
keys and values. Type a number to go into that part, `u` to go back up one level, and `q` to stop.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Lists and `Sexpr` values are not opened; they are printed as they are.

## Help while typing

When typing at a terminal, the following are available.

- `Tab` completes names (definitions, and syntax such as `if` and `defun`), paths containing `::`,
  REPL commands at the start of a line, and file names inside a string.
- After you type a function's name followed by a space, the function's type is shown dimmed at
  the end of the line.
- The parenthesis matching the one next to the cursor is highlighted.
- If you press Enter before a form is finished, the next line starts indented by the number of
  open parentheses.

## Previous results (`:*`, `:**`, `:***`)

In the REPL, `:*` is the value printed last, `:**` the one before it, and `:***` the one before
that. Each has the type of its value.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- An input that printed no value (such as a definition) or that failed does not move the results
  along.
- Writing `:**` or `:***` before there have been that many results is an error.
- A quoted `':*` is an ordinary keyword.
- A `:*` written in a function body keeps referring to the result from when the function was
  defined. Common Lisp's `*` reads the value at the time the function is called, but typelisp
  fixes it at definition time so that it has a type.
- Outside the REPL (running a file, or in `~/.typlrc`, `--load` and `--eval`), `:*` is an ordinary
  keyword.

## Stopping a computation (`Ctrl+C`)

Pressing `Ctrl+C` during an evaluation stops that computation and returns to the prompt. Whatever
you defined before stays defined. This works whether the computation is a loop in the interpreter,
a loop in compiled code, or waiting in `sleep` or `wait`. The cleanups of any `unwind-protect` in
progress are run.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` pressed while typing only discards the line being typed.

## Investigating after an error (`:backtrace`, `:locals`)

After an evaluation stops with an error, `:backtrace` (`:bt`) prints the function calls that were
in progress at that point, innermost first. `:locals N` prints the arguments of call N according
to their declared types.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- For a call made from the last expression of a function (a tail call), the calling function is
  not shown. This is what lets a tail-recursive loop use the space of a single call however many
  times it goes round.
- Calls inside compiled functions and anonymous functions (`lambda`) are not shown.
- `:locals` shows only the arguments, not variables bound with `let`.
- Evaluating the next line discards the record of the previous error.
