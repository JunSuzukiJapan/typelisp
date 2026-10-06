<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Numbers

Operations on integers, floating-point numbers, rationals, complex numbers and booleans, and other
number-related functions. For how to read the call forms, see [Built-in Functions](README.md).

## 1. Fixed-width integers

There are seven integer types: **`int`** (CL's `integer`: arbitrary precision, and the default type of
unannotated integer literals; chapter 3), and the fixed-width `i8` `i16` `i32` `u8` `u16` `u32`.
Which one an operation resolves for is decided by the type of the first argument (they are
independent of each other, with no implicit conversions). **There is no 64-bit integer type.** A
run-time value is one word whose low bits are a tag, so only 63 bits are left for an immediate
integer, and a type claiming 64 bits would have to drop the top bit somewhere. `int` becomes a bignum
once it passes those 63 bits, so if the width does not matter, use `int`. The table below is for the
six fixed-width types (the table for `int` is in chapter 3).

| Name | Form | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | The four arithmetic operations. `/` truncates toward zero and panics on division by zero |
| `mod` | `(mod a b)` | `(T,T)→T` | Remainder (CL's `mod`, **floor division**: the sign follows the divisor. `(mod -7 3)`→`2`). Panics on division by zero |
| `rem` | `(rem a b)` | `(T,T)→T` | Remainder (CL's `rem`, **truncating division**: the sign follows the dividend. `(rem -7 3)`→`-1`). Panics on division by zero |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Correspond to CL's two-argument `floor`/`ceiling`/`round`/`truncate` (`(floor 7 2)`→quotient 3, remainder 1). Instead of multiple values, they return the quotient and remainder in a `cons-cell` (`car`=quotient, `cdr`=remainder). `round-div` rounds ties to even, as CL does |
| `abs` | `(abs x)` | `T→T` | Absolute value |
| `signum` | `(signum x)` | `T→T` | Sign (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Greatest common divisor |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Least common multiple (0 if either is 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | The larger / smaller (three or more arguments are expanded by the variadic sugar of chapter 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Comparison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | All the same as `=` (there is no difference for numbers of the same type) |
| `int->float` | `(int->float x)` | `T→f64` | Widening conversion to `f64` |
| `int->int` | `(int->int x)` | `T→int` | Widening conversion to `int` (always exact). What `(as int x)` does |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Widening conversion to `ratio` (always exact) |
| `int->char` | `(int->char x)` | `T→char` | Interprets the value as a Unicode scalar value. Panics on an invalid value |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | A version of `int->char` that returns `None` on failure |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Width conversion. Values that do not fit are truncated (like Rust's `as`) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | The same conversion as a question. `None` if the value does not fit that width |

These conversions are also what the special forms `(as Type x)`/`(try-as Type x)`
([Syntax Reference](../syntax.md#7-other-special-forms)) do. Bit operations (`logand`/`ash`/`ldb` and so
on) and predicates (`zerop`/`evenp` and so on) have the same shape across types, so they are gathered
in chapters 11 and 9.

`i8` `i16` `u8` `u16` `u32` have exactly the table of this chapter, and `f32` has exactly the `f64`
table of chapter 4.

**A type name means its width and signedness, nothing more.** `i32` means "treat 32 bits as signed"
and `u32` means "treat 32 bits as unsigned". `(+ (the u8 200) (the u8 100))` is `44`,
`(+ 2147483647 1)` (as `i32`) is `-2147483648`, and `(lognot (the u32 0))` is `4294967295`. `f32` is
the same: a real binary32. `(/ (the f32 1.0) (the f32 3.0))` prints as `0.33333334`, a different
value from the `f64` result `0.3333333333333333`.

The derived CL catalog (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` and the predicates of chapter 9)
exists for `int`/`i32`/`f64`/`ratio`. If you need it for another width, move over with `(as int x)` /
`(as i32 x)` (width conversions exist for every pair).

## 2. Raw words at the C boundary (`ptr` / `c-long` / `c-ulong`)

Three types used only for passing values to and from C functions declared with
[`defffi`](../syntax.md#33-defffi--declaring-c-functions-ffi). `ptr` is an opaque pointer, and
`c-long` / `c-ulong` are C's `long` / `unsigned long`. Making one a value requires being inside
`(unsafe ...)`.

**There is no arithmetic.** None of the table of chapter 1 applies: neither `(+ p 1)` nor `(< n m)`
can be written. These are words to hand to C, not types to compute with, so to compute, move to a
type with a width. `c-long` / `c-ulong` have only conversions:

| Name | Form | Type | Description |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | The same width conversions as chapter 1. Values that do not fit are truncated |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | The same conversion as a question |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | The way in, from the other raw word and from the integer types of chapter 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Same as above |
| `int->int` | `(int->int x)` | `T→int` | **Always exact**. The honest way to read a `size_t` that does not fit in an `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` are what these do, and the
conversions exist for every pair with the integer types of chapter 1. `ptr` does not even have this
table: no way is provided to read a pointer as a number. It is a value that is only passed, received
and handed on to another C function.

**They cannot be printed either.** `(println "~a" x)` does not accept a raw word (it has no `Sexpr`
representation), so move it to a type with a width first, as in `(println "~a" (as int n))`.

"There is no 64-bit integer type" from the start of chapter 1 holds for these three too. It holds
**because they cannot be stored**: they cannot be a `defstruct` field, a `defvar`, inside a type
argument or inside an `Sexpr`, so they are words that only pass through a function as arguments,
return values and local variables. For details, see the
[Syntax Reference](../syntax.md#ptr--c-long--c-ulong--raw-machine-words).

## 3. Arbitrary-precision integers `int`

CL's `integer`, and this language's **integer**: unannotated integer literals have this type, and
built-ins that return a number, such as `length` and `char->int`, return this type. A value is held
as a 63-bit immediate value (fixnum) while it fits, is promoted to a bignum automatically when the
result of an operation no longer fits, and goes back to an immediate value when it fits again. `eq`
is always value identity within the fixnum range, and `eql`/`=` are numeric identity over the whole
range. It is a different type from the fixed-width integer types (chapter 1), with no implicit
conversion: `(as int x)` is the exact widening from a fixed width, and `(as i32 n)` /
`(try-as i32 n)` are the truncation / check from `int` (the same meaning as `int->W` / `try-int->W`
in chapter 1).

The integer variant of `Sexpr` is also just `int` (`(int n)` accepts both fixnums and bignums).

Built-ins that take an index or a count (`substring`, `get` of `Vector`, the shift count of `ash` and
so on) accept `int`, but passing a value that does not fit a fixnum is a run-time error ("an integer
argument does not fit a fixnum").

| Name | Form | Type | Description |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Never overflow (they promote) |
| `/` | `(/ a b)` | `(int,int)→int` | Truncates toward zero. Panics on division by zero |
| `mod` | `(mod a b)` | `(int,int)→int` | Remainder of floor division (the sign follows the divisor) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | All `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | The same as chapter 11 (two's complement with infinitely many bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | The same as chapter 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Truncation / check. `W` is one of the six widths or `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identity (on the fixed-width and C-word side, `int->int` widens; chapter 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | The same shape as chapter 1. `expt` accepts only non-negative exponents |

## 4. Floating-point numbers (`f64` / `f32`)

`f32` has the same table.

| Name | Form | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Division by zero does not panic; it gives `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Remainder of floor division (as in CL; the sign follows the divisor. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Remainder of truncating division (as in CL; the sign follows the dividend. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Comparison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | All the same as `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Power |
| `abs` | `(abs x)` | `f64→f64` | Absolute value |
| `signum` | `(signum x)` | `f64→f64` | Sign (`1.0`/`-1.0`; `±0.0`/`NaN` are returned as they are. As in CL, unlike Rust's `signum`) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | The larger / smaller (three or more arguments are expanded by the variadic sugar of chapter 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Unary operations |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Transcendental functions. `log` is the natural logarithm |
| `log` (two arguments) | `(log x base)` | `(f64,f64)→f64` | Logarithm to a given base. Expanded to `(/ (log x) (log base))` (chapter 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Correspond to CL's two-argument versions (`(floor 7.0 2.0)`→quotient 3, remainder 1). The same design as the functions of the same name in chapter 1 (`car`=quotient, `cdr`=remainder) |
| `float->int` | `(float->int x)` | `f64→int` | Converts to `int` by truncating toward zero (CL's `truncate`; exact for finite values of any size). Panics on infinity and NaN. For a fixed width, use `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Converts to a `ratio` as the exact binary rational (CL's `rational`) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Converts between floating-point widths. `float->f32` rounds to nearest, `float->f64` is always exact. What `(as f32 x)` does |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | The same conversion as a question. `none` if rounding changes the value (widening to `f64` is always `some`). What `(try-as f32 x)` does |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL's functions of the same name. Aliases of `floor`/`ceiling`/`round`/`truncate` above: in CL the unprefixed ones return integers, so the `f`-prefixed ones match this language's behavior |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 2 / 53 / 53 respectively (only the precision of `0.0` is 0). `f64` is always IEEE-754 binary64, so these are constants |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` or `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | The mantissa (in `[1/2,1)`, without sign) and the exponent. CL returns three values, but there are no multiple values, so the sign is left to `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | The same decomposition with an exact 53-bit integer mantissa. `mantissa * 2^exponent` is exactly the original value |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **The simplest rational that reads back as that float** (`(rationalize 0.1)` is `1/10`). For the exact binary value, use `float->ratio` |

**Difference from CL: how `round` rounds.** `round` (and so `fround`/`round-div`) rounds **away from
zero** (`(round 2.5)` = `3.0`). CL rounds **to even**, giving `2`.

## 5. Rationals `ratio`

CL-compatible arbitrary-precision rationals. They are always kept in lowest terms with a positive
denominator, and are heap-allocated. There is no implicit conversion with the integer types or
`f64` (use an explicit conversion method or `as`/`try-as`). For the ratio literal syntax, see the
[Syntax Reference](../syntax.md#1-lexical-elements).

| Name | Form | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | The four operations (results always in lowest terms). `/` panics on division by zero |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Remainder of floor division (as in CL; the sign follows the divisor) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Remainder of truncating division (as in CL; the sign follows the dividend) |
| `abs` | `(abs x)` | `ratio→ratio` | Absolute value |
| `signum` | `(signum x)` | `ratio→ratio` | Sign (returns `1`/`-1`/`0` as a `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Power. The exponent must be an integer-valued `ratio` (panics otherwise). A negative exponent gives the reciprocal |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | The larger / smaller |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` has no bit operations (in CL they are for integers only) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Comparison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | All the same as `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Numerator in lowest terms (same name as in CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Denominator in lowest terms (always positive) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Integer part (truncated toward zero) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Converts to `f64` |

The ways in from fixed-width integers and `f64` are `int->int`/`int->ratio` (chapter 1) and
`float->int`/`float->ratio` (chapter 4). `int`/`ratio` are separate types independent of `i32` and the
others, and mixed arithmetic needs explicit conversions.

## 6. Complex numbers `complex`

A struct (`defstruct`) in the standard library.

**Two differences from CL** (both follow from static typing):

1. **The components are always `f64`.** A CL complex can also hold rationals, and `(complex 1 2)` and
   `(complex 1.0 2.0)` are different types. A static type has to pick one, and the transcendental
   functions return the floating-point kind.
2. **`(sqrt -1.0)` is the real `sqrt` (NaN).** In CL, `sqrt` can return a complex number from a real
   one, but the `sqrt` of `f64` has to return an `f64`. A complex result comes from a complex
   argument: `(sqrt (complex -1.0 0.0))` is `i`.

| Name | Form | Type | Description |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Construction. The components can be read directly as `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Real part and imaginary part. **They also work on real numbers** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), as in CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Conjugate (also works on real numbers) |
| `phase` | `(phase z)` | `complex→f64` | Argument in (-pi,pi] (also works on real numbers) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Absolute value. **The only `abs` that does not return the receiver's type** (as in CL, the absolute value of a complex number is real) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Complex arithmetic |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Component-wise equality. `Eq` is implemented too (there is no `Ord`: complex numbers have no order, and CL's `<` rejects them too) |
| `zerop` | `(zerop z)` | `complex→bool` | Whether both components are 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` give principal values |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | The angle of the vector `(x,y)`. **CL's two-argument `(atan y x)` is sugar for this** (it branches on the number of arguments, like the two-argument `log`) |

It implements `print-object`, so `~a`/`~s` print it as `#C(re im)`, as CL does (this language's
reader has no `#C` syntax to read it back).

## 7. Booleans

| Name | Form | Type | Description |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negation |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | All compare values for equality |

`and`/`or` need short-circuit evaluation, so they are special forms
([Syntax Reference](../syntax.md#4-binding-and-conditionals)).

## 8. Numeric helpers and call sugar

`abs`/`signum` (all numeric types), `gcd`/`lcm` (integer types only), `rem` (all real types including
`f64`) and `expt` (`int`/`f64`/`ratio`) are defined as methods of each numeric type (resolved by the
receiver's type: `(abs x)` is the method for the type of `x`). The details for each type are in
chapters 1, 3, 4 and 5. Fixed-width integers have no `expt` (they have no promotion and would
overflow; move to `int` with `(as int x)` and use its `expt`).

### 8.1 Variadic and 0/1-argument forms

CL's arithmetic and comparison are variadic, but methods are resolved only by the receiver's type,
not by the number of arguments. So **the checker expands the following forms into two-argument
calls**.

| Form you can write | Expansion | Applies to |
|---|---|---|
| `(op a b c ...)` | The left fold `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` with each term bound to a temporary | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Those of the above that have an identity element |
| `(op x)` | For `+ * max min logand logior logxor`, `x` itself. `(- x)` negates, `(/ x)` gives the reciprocal, `(gcd x)`/`(lcm x)` give `(abs x)` (as in CL) | Same as above |
| `(cmp x)` | Evaluates `x` and gives `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Each term is evaluated exactly once, from left to right (this is why the variadic comparisons go
through temporaries). The variadic form of `/=` compares **adjacent pairs**, unlike CL, which asks
whether all pairs differ.

### 8.2 `isqrt` and integer `expt`

| Name | Form | Type | Description |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | The largest integer not exceeding the square root. Panics on a negative value |
| `expt` | `(expt n e)` | `(T,T)→T` | Power (by squaring). CL returns a rational for a negative exponent, but an integer type cannot represent it, so it panics; convert to `ratio` first |

## 9. Predicates

| Name | Form | Type | Types |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (integer types only, as in CL) |

There are **no type predicates** like CL's `numberp`/`integerp`/`floatp`. With static typing, the type
of a value is already settled without asking at run time.

## 10. Constants

| Name | Type | Value |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Operation codes passed to `boole` (in place of CL's keywords) |

Numeric limit constants (CLHS 12.1.4.2 / 12.1.3):

| Name | Type | Description |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | The upper / lower limit of a 63-bit immediate value (2^62-1 / -2^62). An `int` beyond them becomes a bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | The largest / smallest finite values |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | The smallest nonzero magnitude, including subnormals |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | The same, limited to normalized numbers |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | They follow CL's definition (the smallest positive `e` with `(/= (+ 1 e) 1)`), so they are **one ULP larger than** 2^-53: 2^-53 itself rounds back to `1.0` under round-to-nearest-even |

## 11. Bit operations

Defined on two's complement with infinitely many bits (CL 12.10). They are implemented for the
fixed-width integer types and `int`, not for `ratio` (CL also has bit operations for integers only).

| Name | Form | Type | Description |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bitwise and, or, exclusive or (variadic and zero-argument versions in 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Bitwise complement |
| `ash` | `(ash x count)` | `(T,int)→T` | Arithmetic shift. Left if `count` is positive, right if negative |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Whether bit `index` is set (**the argument order is the reverse of CL**; see below) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | The number of set bits (for a negative number, the number of 0 bits) |
| `integer-length` | `(integer-length x)` | `T→T` | The number of bits needed to represent it, not counting the sign |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | The remaining seven, composed from the above |

**Only the second argument of `ash` is `int` rather than `T`.** It is a **distance** in bits, not a
value of the receiver's type, so the receiver's width and signedness say nothing about the distance
(for the same reason that `count` in CL's `(ash integer count)` is any integer). Shifting an unsigned
value right is a logical shift (`(ash (the u8 200) -3)` = `25`), and a signed one is an arithmetic
shift rounding toward negative infinity (`(ash (the i32 -100) -4)` = `-7`). The `index` of `logbitp`
is `int` for the same reason.

**Byte specifiers.** Instead of the opaque object CL's `byte` returns, a `cons-cell<int,int>`
(`car`=size, `cdr`=position) is used. Both size and position are numbers of bits, so they are `int`
whatever the width of the integer being taken apart.

**The integer is the first argument, in a different order from CL.** CL writes
`(ldb bytespec integer)`, but this language chooses a method by the type of the receiver (the first
argument), and with the specifier first it could not choose by the type of the integer. All the other
bit operations have the form `(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), and
only the `ldb` family and `logbitp` were the other way round, so those were brought in line. The
remaining arguments keep CL's relative order, so `(dpb newbyte spec n)` becomes `(dpb n newbyte spec)`.

| Name | Form | Type | Description |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Makes a byte specifier |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Takes out a component |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extracts the specified byte from `x`, right-justified |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Whether any bit in the specified byte is set |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Clears everything outside the specified byte (keeping positions) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Deposits the right-justified `newbyte` into the specified byte of `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | The position-preserving version of `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | One of the 16 two-operand logical operations, chosen by `op` (a `boole-*` constant from chapter 10) |

`T` is a type implementing the `Bits` trait, namely `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Only
`boole` keeps `op` first, since there is no reason to change CL's order there.

## 12. Random numbers

| Name | Form | Type | Description |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | A random number from `0` up to but not including `n`. If the state is left out, draws from `*random-state*` and advances it |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Without an argument, a new state; given one, a copy of it (the copy replays the same sequence) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Always `true` (the static type already rules out other types; it exists only to correspond to CL) |
| `*random-state*` | — | `random-state` | The default state of `random`. A global that can be assigned (replace it with `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | The state that the integer names. The same seed always replays the same sequence |

The generator is xorshift64 and returns the same sequence whether interpreted or compiled.

A new state from `make-random-state` is seeded from the wall clock, so it cannot be reproduced across
runs. To reproduce, use `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; prints the same three numbers on every run
```

**CL has no portable way to give a seed** (`make-random-state` takes only `nil`/`t`/a state), so this
name follows SBCL's `sb-ext:seed-random-state` rather than CL.

Different seeds give different sequences. `(seed-random-state 0)` and `(seed-random-state 1)` give
different sequences, and so do `-7` and `7`.
