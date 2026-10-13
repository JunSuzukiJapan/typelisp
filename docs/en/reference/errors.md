<!-- translated-from: docs/ja/reference/errors.md @ b550ae4 -->
# Error Messages

What the main error messages from `typl` mean and how to fix them.

## 1. Reading an error

Errors are written to standard error in this form:

```text
error: file:line:column: kind: message
```

The `kind` tells you when the error was found.

| Kind | When | Meaning |
|---|---|---|
| `type error` | Before running (at check time) | A mistake in types or names. That form is not run |
| (no kind) | When reading or checking | A syntax mistake such as unbalanced parentheses, or a name that cannot be found |
| `panic` | While running | An unrecoverable failure. The program stops after running the cleanup of `unwind-protect` |

Lines starting with `warning:` are warnings, and processing continues.

`file:line:column` points at the expression with the mistake. For a run-time error that happens
inside a standard library function, it points at the place where the program called that function.
Some errors have no position (such as `error: panic: ...`).

Example:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

This means the expression at line 1, column 24 of `main.typl` was a `string` where an `i32` was
expected.

## 2. Errors at check time

Mistakes found before running. The form is not run until they are fixed.

### 2.1 Types

| Message | Meaning and fix |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | An expression of type `U` is where type `T` is needed. There are no implicit conversions; for numbers, convert with `(as T x)`. `int` and `i32` are different types too |
| ``integer literal 300 is out of range for u8 (0..=255)`` | The literal does not fit the type. If you want it cut down, write `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | There is no type of that name. Define a type before the first form that uses it (types have no forward declaration). If you meant a type variable, write it in a declaring position such as `<foo>` after the function name ([Syntax Reference 3.6](syntax.md#36-defstruct--structs-user-defined-types)) |
| ``cannot infer type argument `t` for `vector::new` `` | A type argument cannot be determined. Write the type with `the`, as in `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | The `match` does not handle every variant. Add arms for the missing variants, or a `_` arm |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | The function requires a trait that the type you passed does not implement. Write `(impl Eq pt ...)` ([Standard Traits](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | A value of a type that does not implement the trait was passed where a `:dyn` is expected. Write the `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | A trait name was written where a type goes. Write `:dyn Error` |
| ``if: (if cond then else)`` | The `if` has the wrong shape. `if` requires an else branch. When you do not need one, use `when` |

### 2.2 Names

| Message | Meaning and fix |
|---|---|
| `no such function: bar` | There is no function or method of that name. Check the spelling |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Methods are selected by the type of the first argument. A method of that name exists, but not for the type of the first argument (`int` here). The end of the message lists the types that have the method (for a method written for one instantiation, the instantiation, such as `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | A method passed as a value is chosen by the type of the first parameter of the function type it is passed as. That type (here `box<string>`) has no method of that name. The end of the message lists the types that have it |
| `unbound variable: y` | There is no variable of that name. Check the spelling and the scope of the binding (is it used outside its `let`?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | Two of a function, a global variable and a macro were given the same name in one module. Rename one of them ([Syntax Reference 3.10](syntax.md#310-module--use--namespaces)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | No `new` is generated for a struct. Make the value with `make-instance`, or write a `new` in `impl` ([Syntax Reference 3.5](syntax.md#35-impl--methods-of-a-type)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | There is no `defmethod`. Write methods in the type's `impl` ([Syntax Reference 3.5](syntax.md#35-impl--methods-of-a-type)) |
| ``defstruct: (:constructor ...) is gone — ...`` | There is no `:constructor` option. Write the constructor in `impl` as a static method |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | A method of one name was written both in an `impl` whose type parameters are variables (`Box<T>`) and in an `impl` of an instantiation (`Box<int>`), in either order. Keep only one of them ([Syntax Reference 3.5](syntax.md#35-impl--methods-of-a-type)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | The same trait was implemented for both `Box<T>` and `Box<int>`. Keep only one of them |
| ``use: unresolved `nosuch` `` | The module named in `use` cannot be found. For how file names map to module paths, see [Syntax Reference 3.11](syntax.md#311-files-and-modules-multi-file-projects) |
| `unresolved path: c::hidden` | The module exists, but the name does not, or it is not visible because it lacks `pub` |
| `circular module dependency: a -> b -> a` | Modules `use` each other. Move the shared part into a separate module |
| ``return-from: no enclosing block named `nope` `` | No `block` with the name given to `return-from` encloses it. A function's block can be used only inside that function |

### 2.3 Calls

| Message | Meaning and fix |
|---|---|
| `f: expected 1 argument(s), got 2` | The number of arguments does not match |
| `f: unknown keyword argument :b` | A keyword argument the function does not have was passed |
| `circle: expected 1 field(s), got 2` | The number of values passed to an enum variant does not match the number of that variant's fields |
| ``make-instance point: slot `y` is not given and has no default`` | A field without a default value is not given to `make-instance` |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | A field the struct does not have was given |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | A field that is not `pub` was given outside the module that defines the type. Have that module provide a `pub` constructor |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | The `impl`s of several instantiations have a static method of that name, and nothing decides which to call. State the expected type, as in `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | A method written per instantiation was given to `compile`, `trace`, `documentation` and the like by the type's name alone. Write the instantiation, as in `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | A name defined with `defconstant` was assigned to. If it needs to change, use `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | A function declared with `defsignature` is not defined |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | The call `(nosuch arg colon at)` written at that place cannot be resolved. Define a method of the argument's type or a function ([Format Directives chapter 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | A `~/name/` was written where it gets the elements of a list, such as inside `~{`. The elements' type cannot settle what to call |

## 3. Read errors

| Message | Meaning and fix |
|---|---|
| `unexpected end of input while reading a list` | A closing parenthesis is missing. The position points at where reading ended (such as the end of the file), so look for the opening parenthesis |

## 4. Errors at run time (panic)

| Message | Meaning and fix |
|---|---|
| `panic: divide by zero` | Division by zero with integers or ratios. Floating-point division by zero does not panic; it gives `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` was applied to `none`. Handle the `none` case with `match` or `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | An index out of range. Check the length with `len`, or use a function that returns `none` when out of range (`nth`, `pop` and so on) |
| `panic: an integer argument does not fit a fixnum` | An `int` that does not fit in 63 bits was passed to an argument taking an index or a count |
| `throw: no enclosing (catch 'oops) for this throw` | A `throw` ran with no enclosing `catch` of the same tag |
| `panic: <message>` | The program called `(panic "<message>")`. A failed `assert` gives `assertion failed: ...` |

A `panic` stops the whole process even when it happens inside a task
([Syntax Reference 12.4](syntax.md#124-interaction-with-other-features)). Express failures you want to
recover from with `Result` ([Syntax Reference chapter 9](syntax.md#9-error-handling-policy)).

## 5. Warnings

| Message | Meaning |
|---|---|
| ``warning: redefining function `f` `` | A function of the same name was defined again. The later definition takes effect. It appears normally when you fix a definition in the REPL |
