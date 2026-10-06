<!-- translated-from: docs/ja/reference/functions/format.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
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
the type of each element can be known from the types. Requirements on elements (an integer for `~d`,
and so on) and missing elements are checked when the values arrive, and are run-time errors (it never
switches to a different representation instead).

CL's lenient rules are not adopted. Passing a non-integer to `~d` and having it printed as `~a`, or
`~:[` treating any value as a boolean, are not reinterpreted that way; they are type errors.

## 2. Output (consuming one argument)

| Directive | Parameters / modifiers | Meaning |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=right-justify | Aesthetic (CL's `princ`; strings without quotes). The argument can be of any type |
| `~s` | Same as above | Standard (CL's `prin1`; a form that can be read back). The argument can be of any type |
| `~w` | — | CL's `write`. Pretty-prints if `*print-pretty*` is true, otherwise the same as `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=digit groups, `@`=always a sign | Decimal/binary/octal/hexadecimal integers. The argument is an integer |
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
| `~/name/` | Method call (chapter 5. The `:`/`@` flags are passed to the method. Takes no parameters) |

The following CL directives are not supported (they are check-time errors).

- `~?` and `~@?`: they take a control string as a run-time argument, so the arguments its directives
  consume cannot be checked. Write those directives directly in the control string.
- `~@[...~]`: it tests whether an argument is not nil, but this language has no nil. Use `~:[false~;true~]`,
  which branches on a `bool`.
- `~{~}` with an empty body: it takes the body from a run-time argument. Write the directives inside
  the braces.

## 5. `~/name/`

**One difference from CL: the name is looked up not as a global function but as a method of the
argument's own type.** The method has the shape `((self Self) (colon bool) (at bool)) → string`, and
the directive's `:`/`@` are passed through as they are.

CL's way of looking it up as a global function cannot be implemented safely in this language. Even
with a literal control string, the type of the elements of a list argument (inside `~{`) is not known
at check time, and looking a function up by name alone could call a function meant for another type.
Choosing by the value's type means the method is type-checked for exactly that type, which is safe
(the same mechanism as `print-object`). It also works for values such as `string`/`bool`/`char`/
`symbol`/lists. Only for integers, whose width cannot be told from the value, is it an error **when
more than one integer type defines a method of that name**.

Which argument it applies to is not known, but which methods it might call is. The checker collects
every `~/name/` from the literal control string and records, among the types of the arguments at that
call site, those that have a method of the shape above. So **if none of the argument types has the
method, it is a check-time error** (not a run-time one), and it works in AOT executables too.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
