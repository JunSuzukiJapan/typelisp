<!-- translated-from: docs/ja/reference/functions/format.md @ 7aae962c61e6f36448edc8bab6b37ff177aefdb3 -->
# Format Directives

The directives written in the control strings of `print`/`println`/`format`. They cover nearly all of
CL's `format` directives. The functions themselves are described in
[Printing](printing.md#1-print--println--format).

## 1. How to write directives

Each directive is `~`, then optional **prefix parameters** (comma-separated: an integer / `'c` (a
character) / `v` (taken from the next argument) / `#` (the number of remaining arguments)), then the
optional **modifiers** `:` and `@`, then the directive character, in that order. Directive characters
are case-insensitive.

The control string must be a literal ([Printing](printing.md#1-print--println--format)). On top of
that, the following are checked at check time.

- **The number and types of arguments.** For each directive that consumes an argument: whether an
  argument is left, and whether its type is accepted (the "argument" notes in the tables below). Where
  the path depends on run-time values, such as moving with `~*`, which clause of `~[` is taken,
  whether `~^` fires, or how many times `~@{` repeats, **every path** is checked. Leftover arguments
  are fine (as in CL).
- **Parameters and modifiers.** A modifier that is not accepted, too many parameters, and out-of-range
  values (a negative width, a base other than 2 to 36, an integer where a character is expected and so
  on) are errors. They are never silently ignored or rounded.

The **elements** of a list argument (`~{`, `~:{`, `~<...~:>`) are `Sexpr`s, and neither their number nor
the type of each element can be known from the types. Requirements on elements (a character for `~c`,
and so on) and missing elements are checked when the values arrive, and are run-time errors (it never
switches to a different representation instead).

`~d` `~b` `~o` `~x` and `~r` with a radix print a non-integer as `~a` would, in decimal, as in CL
(`(format false "~5d" 1/2)` is `"  1/2"`). CL's other lenient rules are not adopted: `~:[` treating
any value as a boolean is not reinterpreted that way; it is a type error.

## 2. Output (consuming one argument)

| Directive | Parameters / modifiers | Meaning |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=right-justify | Aesthetic (CL's `princ`; strings without quotes). The argument can be of any type |
| `~s` | Same as above | Standard (CL's `prin1`; a form that can be read back). The argument can be of any type |
| `~w` | — | CL's `write`. Pretty-prints if `*print-pretty*` is true, otherwise the same as `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=digit groups, `@`=always a sign | Decimal/binary/octal/hexadecimal integers. The argument may be anything (a non-integer prints as with `~a`, in decimal) |
| `~r` | `~radix,mincol,padchar,commachar,interval` (with a radix) or none | With a radix, that base (2 to 36). Without one: `~r`=English cardinal, `~:r`=English ordinal, `~@r`=Roman numerals, `~:@r`=old Roman numerals. The argument is an integer |
| `~p` | `:`=back up one, `@`=y/ies | Plurals (`~p`→"s", `~@p`→"y"/"ies"). The argument is an integer |
| `~c` | `:`=name, `@`=`#\` syntax | A character. The argument is a `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=sign | Fixed-point. The argument is a number |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=sign | Exponential notation. The argument is a number. CL's exponent-digits, scale and overflowchar parameters are not supported (giving them is an error) |
| `~g` | `@`=sign | General floating-point. The argument is a number. Takes no parameters |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Monetary notation. The argument is a number |

## 3. Output (consuming no arguments)

| Directive | Meaning |
|---|---|
| `~%` | Newline (`~n%` for n of them) |
| `~&` | fresh-line (a newline unless at the start of a line; `~n&`) |
| `~\|` | Page break (form feed) |
| `~~` | A literal `~` (`~n~` for n of them) |
| `~t` | Tab (`~colnum,colincT`. If already at column colnum or beyond, moves on by a multiple of colinc; does not move if colinc is 0. `@`=relative. `:`=a tab relative to the start of the logical block, which works only when pretty-printing) |
| `~_` | Conditional newline (pretty; plain=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Indentation (pretty; `~ni`=start of block + n / `~n:i`=current column + n) |
| `~<newline>` | Ignores the newline (`:`=keep the whitespace, `@`=keep the newline) |

As in CL, the pretty-printer directives (`~_` `~i` `~:t` `~<...~:>`, and the pretty-printing path of
`~a`/`~s`/`~w`) all do nothing when `*print-pretty*` is false. It is false by default.

## 4. Control structures

| Directive | Meaning |
|---|---|
| `~(...~)` | Case conversion (`~(` lower case, `~:(` capitalize each word, `~@(` capitalize the first word only, `~:@(` all upper case) |
| `~[...~;...~]` | Conditional selection (branches on an integer argument. With `~n[`, `~v[` or `~#[`, it branches on that value and takes no argument. `~:;`=the default clause, only as the last clause). `~:[false~;true~]` branches on a `bool` argument and has exactly two clauses |
| `~{...~}` | Iteration (walks a list argument. `~:{`=per sublist, `~@{`=over the remaining arguments, `~:@{`=over each list among the remaining arguments, `~^`=exit, `~:}`=run once even if empty). A body that consumes no argument in one iteration is an error (it would never finish) |
| `~<...~;...~>` | Justification (spreads segments over `~mincol` columns. `:`/`@`=padding at the ends) |
| `~<...~;...~:>` | **Logical block** (closed with `~:>`; a different thing from the justification above). The first segment is the prefix and the last is the suffix (both literal strings only). With the `~@;` separator, the prefix is a **per-line prefix**. `~:<` defaults the prefix/suffix to `(`/`)`. The argument is one list (`~@<` uses the remaining arguments in place) |
| `~*` | Skipping arguments (`~n*`=forward n, `~:*`=back, `~@*`=to an absolute position) |
| `~/name/` | Calls a function or method (chapter 5; the `:`/`@` flags are passed to the callee; takes no parameters) |

The following CL directives are not supported (they are check-time errors).

- `~?` and `~@?`: they take a control string as a run-time argument, so the arguments its directives
  consume cannot be checked. Write those directives directly in the control string.
- `~@[...~]`: it tests whether an argument is not nil, but this language has no nil. Use `~:[false~;true~]`,
  which branches on a `bool`.
- `~{~}` with an empty body: it takes the body from a run-time argument. Write the directives inside
  the braces.

## 5. `~/name/`

`~/name/` resolves `name` by the same rules as a call `(name arg colon at)` written at that place, and
outputs the string the call returns. It consumes one argument. `colon` and `at` are `bool`s saying
whether the directive carries `:` and `@`.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- Name resolution is the same as for a call ([Syntax Reference 3.10](../syntax.md#310-module--use--namespaces)). If the argument's type
  has a method `name`, that is called; otherwise the visible function `name`. A function in a module
  can be named by its path (`~/m::shout/`).
- What can be called is a function or method written in typelisp that takes the argument, `colon` and
  `at` as they are and returns a `string`. Built-in functions cannot be called.
- **What to call is settled at check time.** The checker finds every argument that can reach the
  `~/name/`, taking moves by `~*` and `~[` into account, and resolves the name for each one's type. If
  one of them cannot be resolved, it is a check-time error, and so is the case where the arguments that
  can reach it would call two different definitions. No name is looked up at run time, so it works the
  same in AOT executables.
- **It cannot be used inside `~{`, `~:{`, `~:@{` or `~<...~:>`** (a check-time error). The type of a list
  argument's elements is `Sexpr`, which cannot settle what to call. `~@{` goes round the call's
  remaining arguments, whose types are known, so it can be used there.

The difference from CL: CL looks the name up as a global function. Here the resolution is the same as
for a call, so a method of the argument's type is chosen before a function of the same name.
