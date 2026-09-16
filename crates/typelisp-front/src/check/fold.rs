//! Constant folding — done while lowering, on the core form.
//!
//! A call whose operator is a Rust-implemented scalar builtin and whose every
//! argument is a literal node has one possible result, and this computes it
//! here so that `(+ 1 2)` lowers to `(int 3)` rather than to an `assoc` node
//! that adds at every evaluation. The same goes for `(if (bool true) T E)`,
//! which lowers to `T`.
//!
//! # Why here and not in the island
//!
//! Lowering is the one stage every execution path shares: the interpreter
//! walks the lowered form, the bridge hands it to the self-hosted island, and
//! the fasl serializes it. A fold done in the island's codegen would reach
//! compiled code only, and LLVM's own builder already folds the *raw-word*
//! arithmetic it is handed — what it cannot fold is the tagged-`int` path,
//! which calls into the runtime for the overflow check. Folding the form
//! covers both, once.
//!
//! # Why the evaluator's own builtins compute the value
//!
//! The result of `(+ 200 100)` depends on the receiver type: it is `300` on an
//! `int`, `44` on a `u8`, a bignum past the fixnum range. That arithmetic is
//! already written, once, in `eval::interp::eval_builtin_method` — the very
//! dispatch the interpreter runs at evaluation time. Calling it here is what
//! makes the folded value *the* value: there is no second table of what `+`
//! means on a `u8` to drift from the first.
//!
//! # What is deliberately not folded
//!
//! * A builtin that raises: `(/ 1 0)` is a runtime panic, and folding it into
//!   a check-time error would change *when* a program fails. A fold that
//!   raises is simply not made, and the call stays.
//! * Anything whose result is not a scalar literal: `print` (`Unit`, and a
//!   side effect), `try-int->char` (an `Option`, which has no literal node),
//!   `char->string` (a heap string with an identity `eq` can observe).
//! * A local bound to a literal — `(let ((x 5)) (+ x 1))` — which would need
//!   constant propagation, not folding.

use typelisp_mem::{Error, Heap, Value};

use crate::check::core::{self, Checked};
use crate::eval::interp::eval_builtin_method;
use crate::eval::EvalError;
use crate::types::{Path, Type};

/// The literal value a checked form is, if it is a literal node.
///
/// The value returned is exactly what evaluating the node would produce: the
/// tag's single field, which for `int`/`int-any-width` is the `Value::Int`
/// the interpreter yields and for the boxed literals is the box itself.
fn literal_value(heap: &Heap, form: Value) -> Option<Value> {
    match core::op(heap, form)? {
        "int" | "int-any-width" | "float-any-width" | "bignum" | "ratio" | "char" | "bool" => {
            core::field(heap, form, 0)
        }
        _ => None,
    }
}

/// The `bool` a checked form is, if it is a `(bool B)` node.
pub(super) fn literal_bool(heap: &Heap, form: Value) -> Option<bool> {
    match (core::op(heap, form)?, core::field(heap, form, 0)?) {
        ("bool", Value::Bool(b)) => Some(b),
        _ => None,
    }
}

/// Whether `ty` is a type a folded result can be spelled as a literal node
/// of: the integer family, the floats, `ratio`, `char` and `bool`. Everything
/// else — `Unit`, `Str`, an `Option`, the C words — has either no literal
/// node or an identity the fold would erase.
fn has_literal_node(ty: &Type) -> bool {
    ty.is_int_family() || ty.is_float() || matches!(ty, Type::Ratio | Type::Char | Type::Bool)
}

/// Whether `type_name` is a receiver whose builtin methods are pure functions
/// of their arguments' values: the same set as [`has_literal_node`], read off
/// the registry's spelling of the type.
fn foldable_receiver(type_name: &Path) -> bool {
    crate::types::INT_TYPE_NAMES
        .iter()
        .chain(crate::types::FLOAT_TYPE_NAMES.iter())
        .chain(["int", "ratio", "char", "bool"].iter())
        .any(|n| *type_name == Path::root(n))
}

/// Fold a builtin method call whose arguments are all literals.
///
/// `Ok(None)` when the call is not foldable — a receiver or result type
/// outside the scalar set, an argument that is not a literal, or a builtin
/// that raised on these arguments (the call is kept so that it raises at
/// runtime, where the program said it would). `Ok(Some(node))` is the
/// literal node the call lowers to instead.
///
/// The caller has established that `method` is a *builtin* of `type_name`
/// (`AssocFn::builtin`), so the evaluator's dispatch is guaranteed to have an
/// arm for it: reaching `None` from it here is a hole between the registry
/// and the evaluator, not a call that merely cannot be folded.
pub(super) fn fold_builtin_method(
    heap: &mut Heap,
    type_name: &Path,
    method: &str,
    args: &[Checked],
    ret: &Type,
) -> Result<Option<Value>, Error> {
    if !foldable_receiver(type_name) || !has_literal_node(ret) {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(args.len());
    for a in args {
        match literal_value(heap, a.form) {
            Some(v) => values.push(v),
            None => return Ok(None),
        }
    }
    let ret_key = crate::type_key::type_key_of_type(ret);
    let value = match eval_builtin_method(heap, type_name, method, &values, &ret_key) {
        Some(Ok(v)) => v,
        // A language-level error: the program raises here at runtime.
        Some(Err(EvalError::Panic(_))) => return Ok(None),
        Some(Err(e)) => {
            unreachable!("constant folding `{}::{}` on literal arguments hit an internal error: {}", type_name, method, e)
        }
        None => unreachable!("`{}::{}` is registered as a builtin but the evaluator has no arm for it", type_name, method),
    };
    // `value` may be a box the builtin just allocated, reachable from nothing
    // yet; `core::tagged` roots it before it conses. Nothing allocates between
    // here and there.
    literal_node(heap, value, ret).map(Some)
}

/// The literal node `value` is, as a `ret`-typed result — the inverse of
/// [`literal_value`], and the same spelling `Checker::check_inner` gives a
/// literal read from source: an `int` is `(int N)` when it fits a fixnum and
/// `(bignum B)` otherwise, a fixed width is `(int-any-width N)`, a float is
/// its box under `float-any-width`.
///
/// A value whose shape does not match `ret` is the evaluator disagreeing with
/// the registry about a builtin's result type, which is a bug rather than a
/// call to leave unfolded.
fn literal_node(heap: &mut Heap, value: Value, ret: &Type) -> Result<Value, Error> {
    let tag = match (ret, value) {
        (Type::Int, Value::Int(_)) => "int",
        (Type::Int, Value::Boxed(id)) if heap.is_bignum(id) => "bignum",
        (t, Value::Int(_)) if t.is_integer() => "int-any-width",
        (Type::F32, Value::Boxed(id)) if heap.is_f32(id) => "float-any-width",
        (Type::F64, Value::Boxed(id)) if heap.is_f64(id) => "float-any-width",
        (Type::Ratio, Value::Boxed(id)) if heap.is_ratio(id) => "ratio",
        (Type::Char, Value::Char(_)) => "char",
        (Type::Bool, Value::Bool(_)) => "bool",
        (t, v) => unreachable!("constant folding produced {:?} for a `{:?}`-typed builtin", v, t),
    };
    core::tagged(heap, tag, &[value])
}
