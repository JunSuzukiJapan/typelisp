//! The primitive value types (`i8`..`u32`, `int`, the floats, `bool`,
//! `char`, `string` ...) and their built-in methods.
//!
//! Every primitive has a method table, so a `defmethod` can target it. The
//! numbers, `string`, `char`, `bool` and `symbol` also get built-in methods
//! (implemented by `eval_builtin_method` in `crate::eval::interp`).
//! Arithmetic and comparison operators are *instance* methods, not free
//! functions, precisely so the same symbol (`+`, `<`, ...) can be overloaded
//! per receiver type — every integer width shares one `+` symbol, and `f64`
//! another, resolved on the first argument's static type exactly like
//! `(get h k)` resolves to `HashTable`'s `get`.

use super::super::registry::AdtKind;
use crate::types::Type;

use super::super::registry::Registry;
use super::{option_of, BuiltinType};

/// Every primitive type, with its methods.
pub(super) fn define(reg: &mut Registry) {
    for ty in crate::types::primitive_types() {
        let path = crate::types::prim_type_path(&ty).expect("primitive_types() are all prim_type_path-mappable");
        let mut def = BuiltinType::new(reg, path, AdtKind::Sum, &[]);
        match ty {
            Type::Str => string(&mut def),
            Type::Char => char(&mut def),
            Type::Int => integer(&mut def),
            Type::Ratio => ratio(&mut def),
            Type::Rational => rational(&mut def),
            Type::Bool => bool(&mut def),
            Type::Symbol => symbol(&mut def),
            // Every integer width and both float widths get the same catalog.
            // The catalog is shared but the *arithmetic* is not: a type name
            // means its width and its signedness (`types::int_width_signed`),
            // so `u8` addition wraps at 8 bits and `f32` division rounds to
            // binary32 — `eval_int_builtin`/`float_at` cut every result back
            // at the receiver's own width.
            //
            // The C words come ahead of the integer arm, which they would
            // not match anyway — spelled out because the difference is the
            // point.
            ref t if t.is_c_word() => c_word(&mut def, t.clone()),
            ref t if t.is_integer() => fixed_width(&mut def, t.clone()),
            ref t if t.is_float() => float(&mut def, t.clone()),
            _ => {}
        }
    }
}

/// Built-in `string` instance methods (`docs/dev/cl-equivalence-catalog.md`
/// §2.2 d). All char/index arguments and `length` count Unicode scalar
/// values (`char`s), not bytes. `ref`/`substring` panic on an out-of-range
/// index — the type system can't express the bound, the same precedent as
/// `car`/`cdr` on a non-`Cons` `Sexpr`. `upcase`/`downcase` are ASCII-only
/// (`str::to_ascii_uppercase`/`lowercase`), avoiding Unicode case mappings
/// that can change a string's length (e.g. German `ß` -> `SS`).
///
/// `eq`/`eql`/`equal`/`equalp` (see `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section for the full rationale): `eq`/`eql` are true
/// CL identity (`Rc::ptr_eq` on a string's underlying `Rc<str>` — see
/// `Heap`'s string table for why `Rc`, not a plain `String`, is what makes
/// identity meaningful here at all); `eql` doesn't add anything beyond `eq`
/// for strings in real CL either (it only extends numbers/characters), so
/// it's a plain alias. Content comparison — what a naive reading of "string
/// equality" usually means — is `equal` (case-sensitive) and `equalp`
/// (case-insensitive, ASCII-only for the same reason `upcase`/`downcase`
/// are) instead; two separately-built equal-content `Str`s are correctly
/// *not* `eq`/`eql`.
fn string(def: &mut BuiltinType) {
    def.instance("upcase", vec![Type::Str], Type::Str);
    def.instance("downcase", vec![Type::Str], Type::Str);
    def.instance("length", vec![Type::Str], Type::Int);
    def.instance("ref", vec![Type::Str, Type::Int], Type::Char);
    def.instance("substring", vec![Type::Str, Type::Int, Type::Int], Type::Str);
    def.instance("append", vec![Type::Str, Type::Str], Type::Str);
    def.instance("lt", vec![Type::Str, Type::Str], Type::Bool);
    // Lexicographic comparison operators, overloaded on `string` the same
    // way the numeric ones are (dispatched by receiver type — see the
    // prelude's `impl Ord string`). `lt` is kept as the older CL-catalog
    // primitive `<` now supersedes.
    for name in ["<", "<=", ">", ">="] {
        def.instance(name, vec![Type::Str, Type::Str], Type::Bool);
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![Type::Str, Type::Str], Type::Bool);
    }
}

/// Built-in `char` instance methods (same catalog section as [`string`]).
/// `upcase`/`downcase` are ASCII-only, for the same reason as `string`'s (a
/// non-ASCII char's case mapping isn't necessarily a single char).
/// `alphap`/`digitp` classify ASCII letters/digits only (CL's
/// `alpha-char-p`/`digit-char-p` without a radix).
///
/// `eq`/`eql`/`equal` all coincide (plain value comparison) — a `char` is an
/// immediate scalar here, so there's no separate identity to diverge from,
/// and CL's `equal` on characters is defined to be the same as `eql` anyway.
/// `equalp` is the one that differs for real: CL requires case-insensitive
/// comparison there (`(equalp #\A #\a)` is true).
fn char(def: &mut BuiltinType) {
    def.instance("upcase", vec![Type::Char], Type::Char);
    def.instance("downcase", vec![Type::Char], Type::Char);
    def.instance("lt", vec![Type::Char, Type::Char], Type::Bool);
    // Code-point comparison operators, overloaded on `char` like the numeric
    // ones (a compiled `char` is a raw `i64` code point, so these lower to
    // the same integer `icmp`s — see `compiler.rs`'s `char-native-method?`).
    // `lt` is kept as the older CL-catalog primitive `<` now supersedes.
    for name in ["<", "<=", ">", ">="] {
        def.instance(name, vec![Type::Char, Type::Char], Type::Bool);
    }
    def.instance("alphap", vec![Type::Char], Type::Bool);
    def.instance("digitp", vec![Type::Char], Type::Bool);
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![Type::Char, Type::Char], Type::Bool);
    }
    // `char->string`: the one-character string. CL reaches this through
    // `string`, which is a designator-taking function this language has no
    // room for; the explicit name says which direction the conversion goes.
    def.instance("char->string", vec![Type::Char], Type::Str);
    // `char->int`: a `char`'s Unicode scalar value (`docs/dev/
    // cl-equivalence-catalog.md` §2.1) — the other half is the integers'
    // `int->char`.
    def.instance("char->int", vec![Type::Char], Type::Int);
}

/// `bool` has exactly two values, both immediate — `eq`/`eql`/`equal`/
/// `equalp` can never diverge for it (no case-folding, no cross-type
/// comparison, no structure to recurse into), so all four are the same
/// value comparison. See `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
/// equalp section for why every one of these four is still defined
/// explicitly rather than leaving `eql`/`equal`/`equalp` undefined.
fn bool(def: &mut BuiltinType) {
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![Type::Bool, Type::Bool], Type::Bool);
    }
}

/// `symbol`'s methods: `eq`/`eql` only. Symbols are always interned, so two
/// `Symbol`s are `eq` iff they are the same interned id (same name) — the
/// runtime routes both to `sexpr_eq`/`sexpr_eql` since a `Symbol` value
/// shares the `Value::Symbol(id)` carrier of a `Sexpr::Sym`. `equal`/`equalp`
/// are not defined (there is no structure to recurse into beyond `eq`'s
/// identity).
fn symbol(def: &mut BuiltinType) {
    for name in ["eq", "eql"] {
        def.instance(name, vec![Type::Symbol, Type::Symbol], Type::Bool);
    }
}

/// Built-in arithmetic/comparison instance methods for a fixed-width integer
/// type: `+ - * / mod` (binary, same-type) and `< <= > >= = /=` (binary,
/// `Bool`-valued). `mod` is floored (CL, sign of the divisor); `/`/`mod`
/// panic on a zero divisor at runtime (`crate::eval::interp::
/// eval_int_builtin`) — the type system can't express "nonzero", the same
/// precedent as `car`/`cdr` on a non-`Cons` `Sexpr`. Shared by every width
/// since the operation set and panic policy are identical; only the
/// receiver/param `Type` differs. The rest of CL's integer catalog
/// (`rem`/`abs`/`signum`/`gcd`/`lcm`) is defined in `prelude.rs` as ordinary
/// typelisp methods (so it compiles via the normal path); only the
/// operations needing a genuinely primitive machine op live here.
fn fixed_width(def: &mut BuiltinType, ty: Type) {
    // `max`/`min` (CL) and the bitwise operators (`logand`/`logior`/
    // `logxor`) are same-type binary ops like `+`/`-`/`*`. Every one of them
    // works in the receiver type's own width and signedness — the value in
    // hand is always the number its type names, sign- or zero-extended into
    // the 64-bit carrier (see `types::normalize_int` and `eval_int_builtin`'s
    // doc comment).
    for op in ["+", "-", "*", "/", "mod", "max", "min", "logand", "logior", "logxor"] {
        def.instance(op, vec![ty.clone(), ty.clone()], ty.clone());
    }
    // `ash` (arithmetic shift, positive = left) is *not* one of them: its
    // second operand is a shift **distance**, not another value of the
    // receiver's type, and typing it as the latter would make right shift
    // unwritable on every unsigned width — `(ash (the u8 x) -3)` would be
    // rejected because `-3` is out of `u8`'s range. The distance is an `int`
    // for the same reason CL's `(ash integer count)` lets `count` be any
    // integer: it measures bits, so the receiver's width and signedness have
    // nothing to say about it. The compiled tier passes the distance as a
    // raw word and `rt_int_ash` reads it as a signed count, as does
    // `eval_int_builtin`.
    def.instance("ash", vec![ty.clone(), Type::Int], ty.clone());
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        def.instance(op, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    // `logtest`: a bitwise `Bool`-valued predicate on two values of the
    // receiver's type (`(logtest a b)`).
    def.instance("logtest", vec![ty.clone(), ty.clone()], Type::Bool);
    // `logbitp` is *not* one of those. Its index is a bit **position**, the
    // same kind of quantity as `ash`'s distance, so it is an `int` at every
    // width. The argument order differs from CL's `(logbitp index integer)`,
    // which would put the *index* in the receiver slot — and since a method
    // is keyed by `(receiver type, name)`, an index could carry only one
    // signature, so the integer could never be more than one type either.
    // The value being asked about comes first here, as it does in every
    // other bit operation (`(logand a b)`, `(ash x count)`, `(lognot x)`);
    // `ldb` and its family follow the same order (`prelude.rs`'s
    // byte-specifier section).
    def.instance("logbitp", vec![ty.clone(), Type::Int], Type::Bool);
    // `lognot` (bitwise complement), `logcount` (population count of a
    // nonnegative integer, or of the zero bits of a negative one — CL
    // §12.10's "infinite precision" reading), `integer-length` (bits needed,
    // excluding sign) are unary, same-type.
    for op in ["lognot", "logcount", "integer-length"] {
        def.instance(op, vec![ty.clone()], ty.clone());
    }
    // `eq`/`eql`/`equal`/`equalp` are all aliases for `=` here — a fixnum has
    // no separate identity to diverge from value, `eql` doesn't add anything
    // for same-type numbers beyond `eq` in real CL either, `equal` on
    // numbers is defined to be the same as `eql`, and `equalp`'s one real
    // difference (cross-*type* numeric comparison, e.g. `i32` vs `f64`) can
    // never be reached here — the checker requires both operands to share
    // this exact `ty`, so there is no other type for it to differ from.
    // Defined under all four names anyway so `case`/`equal`/`equalp` can
    // call any of them uniformly across every type (see
    // `docs/cl-equivalence-catalog.md`'s eq/eql/equal/equalp section).
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    // `int->float`: widening numeric conversion (`docs/dev/
    // cl-equivalence-catalog.md` §2.1), for every integer width. This is also
    // what `equalp`'s `Sexpr` `Int`<->`Float` cross-type comparison
    // (`prelude.rs`) needs.
    def.instance("int->float", vec![ty.clone()], Type::F64);
    // `int->char`: the other half of `char`'s `char->int` — a Unicode scalar
    // value back to `char`. Panics at runtime on a value outside the valid
    // range (surrogates, or past `U+10FFFF`) — the type system can't express
    // "valid scalar value", same precedent as `car`/`cdr` on a non-`Cons`
    // `Sexpr` or division by zero.
    def.instance("int->char", vec![ty.clone()], Type::Char);
    // `try-int->char`: the `Option`-returning counterpart of `int->char`,
    // for `(try-as char n)` — same Unicode-scalar-value validity check,
    // `None` instead of a panic on failure.
    def.instance("try-int->char", vec![ty.clone()], option_of(Type::Char));
    // `int->ratio`: the exact widening into the rational type
    // (`docs/cl-equivalence-catalog.md`'s conversion catalog).
    def.instance("int->ratio", vec![ty.clone()], Type::Ratio);
    // `int->int`: the exact widening into the arbitrary-precision `int` —
    // what `(as int x)` calls. Named by the family-prefix rule every other
    // conversion here follows (`int->float` is "an integer receiver, to
    // float"), which reads oddly for this one target and is kept anyway: a
    // second naming scheme for one method would be the odder thing.
    def.instance("int->int", vec![ty.clone()], Type::Int);
    width_casts(def, &ty);
}

/// `int->i8`/`int->u32`/... and their `try-` counterparts: a cast between
/// two integer *widths*, which is a real conversion because a type name
/// means its width and its signedness (`types::int_width_signed`). `int->W`
/// truncates — that is what a width cast means, the same as Rust's `as` —
/// and `try-int->W` answers `none` instead when the value does not fit,
/// which is the question `try-as` asks. The checker's `as` and `try-as` pick
/// between them.
///
/// One pair per *target*, on every integer receiver, because the target is
/// the only thing a conversion's name can carry: there is no way to spell
/// "narrow to whatever width the context wants" as one method. The C words
/// are targets as well as the six widths: `(as c-ulong n)` is how a size is
/// made for a C function, and without it a `c-ulong` parameter could be
/// declared and never passed anything.
fn width_casts(def: &mut BuiltinType, ty: &Type) {
    for to in crate::types::INT_TYPES.iter().chain(crate::types::C_WORD_TYPES.iter()) {
        let target = to.scalar_name().expect("the widths are scalar types");
        def.instance(&format!("int->{}", target), vec![ty.clone()], to.clone());
        def.instance(&format!("try-int->{}", target), vec![ty.clone()], option_of(to.clone()));
    }
}

/// The conversions a `c-long`/`c-ulong` gets, and nothing else.
///
/// Deliberately not [`fixed_width`]. These are words on the way to or from
/// C, and giving them `+` would invite exactly the thing this language
/// removed its 64-bit integer type to prevent: arithmetic on a value that
/// cannot be stored anywhere, quietly different in width from every other
/// number here. What they do need is a way out — `strlen` answering
/// `c-ulong` is useless if nothing can read it — so every narrowing target
/// is here, plus `int` for the values that do not fit in one.
///
/// `Interp::eval_assoc_builtin` dispatches `int->W` by parsing `W`'s own
/// width out of its name (`types::int_width_signed`), which answers for these
/// two as well.
fn c_word(def: &mut BuiltinType, ty: Type) {
    width_casts(def, &ty);
    // Always exact, which is what makes it the honest way to read a `size_t`
    // that does not fit in an `i32`.
    def.instance("int->int", vec![ty], Type::Int);
}

/// Built-in arithmetic/comparison/transcendental instance methods for a
/// float type: `+ - * /` (IEEE-754 — a zero divisor yields `inf`/`NaN`, no
/// panic), the comparisons, `expt` (binary, `f64::powf`) and the unary
/// rounding/root family `sqrt`/`floor`/`ceiling`/`round`/`truncate`.
/// `mod`/`rem` and `abs`/`signum` are defined in `prelude.rs` as typelisp
/// methods (built from these primitives — `mod`/`rem` via
/// `a - b*floor|truncate(a/b)`), so they compile via the normal path.
fn float(def: &mut BuiltinType, ty: Type) {
    for op in ["+", "-", "*", "/", "max", "min"] {
        def.instance(op, vec![ty.clone(), ty.clone()], ty.clone());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        def.instance(op, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    def.instance("expt", vec![ty.clone(), ty.clone()], ty.clone());
    // The transcendental family (CL §12.10): trig, their inverses and
    // hyperbolic counterparts, natural `exp`/`log`, alongside the root and
    // rounding unaries.
    for op in [
        "sqrt", "floor", "ceiling", "round", "truncate", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh",
        "tanh", "asinh", "acosh", "atanh", "exp", "log",
    ] {
        def.instance(op, vec![ty.clone()], ty.clone());
    }
    // See `fixed_width`'s eq/eql/equal/equalp comment — the same alias for
    // `=` (cross-type numeric `equalp`, e.g. `f64` vs `i32`, is likewise
    // unreachable: the checker requires both operands to be of this type).
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    // `float->int`: truncating toward zero into the arbitrary-precision
    // `int` — exact for every finite float, whatever its magnitude (CL's
    // `truncate`); a non-finite float panics, having no integer to truncate
    // to. A fixed-width target is reached by chaining `int->W`.
    def.instance("float->int", vec![ty.clone()], Type::Int);
    // `float->ratio`: widening and *exact* — every finite `f64` is itself an
    // exact dyadic rational (CL's `rational`, not the lossy-round-trip
    // `rationalize`), via `num_rational::BigRational::from_float`.
    def.instance("float->ratio", vec![ty.clone()], Type::Ratio);
    // `float->f32`/`float->f64` and `try-float->f32`: a cast between the two
    // float *widths*, a real conversion because `f32` is binary32 rather
    // than a label on an `f64` (`docs/ja/reference/functions/numbers.md` §1).
    // `float->f32` rounds to nearest, `float->f64` is exact in both
    // directions (every binary32 value is a binary64 value), and
    // `try-float->f32` answers `none` when the rounding would lose something
    // — the question `try-as` asks.
    def.instance("float->f32", vec![ty.clone()], Type::F32);
    def.instance("float->f64", vec![ty.clone()], Type::F64);
    def.instance("try-float->f32", vec![ty.clone()], option_of(Type::F32));
    // `try-float->f64` can only answer `some` — widening is exact — but it is
    // defined all the same, because the checker names a width cast's method
    // from the target alone and has no "this direction cannot fail" case.
    // The integer side is the same shape: `try-int->i32` from an `i8` is
    // always `some` too.
    def.instance("try-float->f64", vec![ty], option_of(Type::F64));
}

/// Built-in instance methods for `int` (CL's `integer`): the fixed-width
/// catalog's operations without a width — `+ - *` promote instead of
/// wrapping, `/`/`mod` panic only on a zero divisor — plus the bitwise
/// catalog (`ash`/`logbitp` count bits in an `int`, for [`fixed_width`]'s
/// reason), the comparisons, and every conversion out:
/// `int->float`/`int->ratio`/`int->char` (`try-` for the partial one),
/// `int->W`/`try-int->W` for the six widths and the two C words (a cast out
/// of an `int` truncates, as every width cast does; the question form says
/// whether it would), and `int->int` as the identity so that `(as int x)` is
/// one method name on every integer receiver.
fn integer(def: &mut BuiltinType) {
    let ty = Type::Int;
    for op in ["+", "-", "*", "mod", "max", "min", "logand", "logior", "logxor"] {
        def.instance(op, vec![ty.clone(), ty.clone()], ty.clone());
    }
    // CL's `/` on two integers is a `rational`: `(/ 6 3)` is `2`, `(/ 7 2)` is
    // `7/2`. The integer quotient is `truncate` (and `floor`/`ceiling`/
    // `round`, built on it in the prelude), CL's first value.
    def.instance("/", vec![ty.clone(), ty.clone()], Type::Rational);
    def.instance("truncate", vec![ty.clone(), ty.clone()], ty.clone());
    def.instance("ash", vec![ty.clone(), Type::Int], ty.clone());
    def.instance("logbitp", vec![ty.clone(), Type::Int], Type::Bool);
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        def.instance(op, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    def.instance("logtest", vec![ty.clone(), ty.clone()], Type::Bool);
    for op in ["lognot", "logcount", "integer-length"] {
        def.instance(op, vec![ty.clone()], ty.clone());
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    def.instance("int->float", vec![ty.clone()], Type::F64);
    def.instance("int->ratio", vec![ty.clone()], Type::Ratio);
    def.instance("int->int", vec![ty.clone()], Type::Int);
    def.instance("int->char", vec![ty.clone()], Type::Char);
    def.instance("try-int->char", vec![ty.clone()], option_of(Type::Char));
    width_casts(def, &ty);
}

/// Built-in arithmetic/comparison instance methods for `ratio` (CL's ratio:
/// an exact rational, always kept reduced with a positive denominator). `/`
/// panics on a zero divisor like every other numeric type here.
/// `numerator`/`denominator` expose the reduced components as `int` (CL's
/// own accessors of the same names). The rest of CL's rational catalog
/// (`mod`/`rem`/`expt`/`abs`/`signum`) is defined in `prelude.rs` as
/// typelisp methods, so it compiles via the normal path.
fn ratio(def: &mut BuiltinType) {
    let ty = Type::Ratio;
    for op in ["+", "-", "*", "/", "max", "min"] {
        def.instance(op, vec![ty.clone(), ty.clone()], ty.clone());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        def.instance(op, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    def.instance("ratio->int", vec![ty.clone()], Type::Int);
    def.instance("ratio->float", vec![ty.clone()], Type::F64);
    def.instance("numerator", vec![ty.clone()], Type::Int);
    def.instance("denominator", vec![ty.clone()], Type::Int);
    // Widening into `rational`, normalizing: an integer-valued `ratio`
    // becomes the `int`. The checker inserts it wherever a `ratio` reaches a
    // `rational` position.
    def.instance("ratio->rational", vec![ty], Type::Rational);
}

/// Built-in arithmetic/comparison instance methods for `rational` (CL's
/// rational: an `int` or a `ratio`, normalized). Every result is normalized
/// the same way, so a `rational` that is an integer is always the `int`. The
/// rest of CL's rational catalog is written in `prelude.rs` on these.
fn rational(def: &mut BuiltinType) {
    let ty = Type::Rational;
    for op in ["+", "-", "*", "/", "max", "min"] {
        def.instance(op, vec![ty.clone(), ty.clone()], ty.clone());
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        def.instance(op, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    for name in ["eq", "eql", "equal", "equalp"] {
        def.instance(name, vec![ty.clone(), ty.clone()], Type::Bool);
    }
    // `rational->int` truncates toward zero, as `ratio->int` does;
    // `try-rational->int` is the question whether the value is an integer at
    // all (CL's `integerp`), answered with the `int` it is.
    def.instance("rational->int", vec![ty.clone()], Type::Int);
    def.instance("try-rational->int", vec![ty.clone()], option_of(Type::Int));
    def.instance("rational->ratio", vec![ty.clone()], Type::Ratio);
    def.instance("rational->float", vec![ty.clone()], Type::F64);
    def.instance("numerator", vec![ty.clone()], Type::Int);
    def.instance("denominator", vec![ty], Type::Int);
}
