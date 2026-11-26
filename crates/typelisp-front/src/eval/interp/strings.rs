//! The `string` and `char` built-in methods.

use crate::{wk, Heap, SymRef, Value};
use crate::eval::value::EvalError;
use super::{rt_i64, str_rt};

/// Resolve an `i32` index against a sequence's current length: out of range
/// (including negative) is `None`, the caller turns that into a panic — the
/// type system can't express the bound, the same "runtime panic for what
/// types can't catch" precedent as `car`/`cdr` on a non-`Cons` `Sexpr`. Used
/// by `string::ref`'s bounds check.
fn checked_index(i: i64, len: usize) -> Option<usize> {
    if i >= 0 && (i as usize) < len { Some(i as usize) } else { None }
}

/// The string behind a `string` value, borrowed from the heap it lives in.
///
/// Borrowed rather than owned (unlike [`expect_bignum`]) because most callers
/// only read it; the ones that go on to build a new string end the borrow
/// with an explicit `.to_string()` first, so the copy is visible at the site
/// that needs it rather than paid by every caller. A mapped read-locked
/// guard rather than plain `&'h str` since `Heap::string` moved behind a
/// lock (`docs/dev/os-threads-design.md` §3) — `Deref<Target = str>` keeps
/// every read-only caller unchanged.
pub fn expect_str<'h>(heap: &'h Heap, v: &Value) -> Result<parking_lot::MappedRwLockReadGuard<'h, str>, EvalError> {
    match v {
        Value::Str(id) => Ok(heap.string(*id)),
        other => Err(EvalError::Internal(format!("expected a Str, got {:?}", other))),
    }
}

pub(super) fn expect_char(v: &Value) -> Result<char, EvalError> {
    match v {
        Value::Char(c) => Ok(*c),
        other => Err(EvalError::Internal(format!("expected a Char, got {:?}", other))),
    }
}

/// `char->int` (`registry::char_assoc`): a `char`'s Unicode scalar value as
/// `i32` (uniformly `Value::Int(i64)` at runtime — see
/// `eval_fixed_builtin`'s doc comment). Always succeeds — every `char` is
/// already a valid scalar value, unlike `int->char`'s reverse direction.
pub(super) fn char_to_int(args: &[Value]) -> Result<Value, EvalError> {
    expect_char(&args[0]).map(|c| Value::Int(c as i64))
}

pub(super) fn string_length(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Int(expect_str(heap, &args[0])?.chars().count() as i64))
}

pub(super) fn string_ref(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let chars: Vec<char> = expect_str(heap, &args[0])?.chars().collect();
    let i = rt_i64(&args[1])?;
    match checked_index(i, chars.len()) {
        Some(idx) => Ok(Value::Char(chars[idx])),
        None => Err(EvalError::Panic(format!("ref: index {} out of range (length {})", i, chars.len()))),
    }
}

pub(super) fn string_substring(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let chars: Vec<char> = expect_str(heap, &args[0])?.chars().collect();
    let start = rt_i64(&args[1])?;
    let end = rt_i64(&args[2])?;
    let len = chars.len() as i64;
    if start < 0 || end > len || start > end {
        return Err(EvalError::Panic(format!("substring: invalid range {}..{} (length {})", start, end, len)));
    }
    let s: String = chars[start as usize..end as usize].iter().collect();
    Ok(str_rt(heap, s))
}

pub(super) fn string_append(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let joined = format!("{}{}", expect_str(heap, &args[0])?, expect_str(heap, &args[1])?);
    Ok(str_rt(heap, joined))
}

/// True CL identity for `string` — the same heap string, not merely equal
/// content (that's [`string_content_eq`]/[`string_content_eqp`] instead — CL's
/// `eq`/`eql` never do structural string comparison, only `equal`/`equalp`
/// do).
///
/// `StrId` equality is what makes this meaningful, and it is meaningful only
/// because `Heap::alloc_string` does not dedupe: two separately evaluated
/// literals with equal content get distinct ids, so `(eq "abc" "abc")` stays
/// false while a value threaded through a binding, a call, a `cons` cell, or
/// a struct field stays `eq` to itself. This replaces an `Rc::ptr_eq` on a
/// Rust-side `Rc<str>`, which had the same identity semantics for the first
/// three but *lost* it through a `cons` cell or a struct field, where the
/// old encoding copied the text onto the heap.
pub(super) fn string_identity_eq(args: &[Value]) -> Result<Value, EvalError> {
    match (&args[0], &args[1]) {
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a == b)),
        (other0, other1) => Err(EvalError::Internal(format!("string::eq: expected two Str arguments, got {:?}/{:?}", other0, other1))),
    }
}

/// Content equality (case-sensitive) — CL's `equal`/`string=`, *not* `eq`
/// (identity — see the string table's doc comment and `docs/cl-equivalence-catalog.md`'s
/// eq/eql/equal/equalp section). Named for what it computes, not for which
/// builtin method currently calls it.
pub(super) fn string_content_eq(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(*expect_str(heap, &args[0])? == *expect_str(heap, &args[1])?))
}

/// Content equality ignoring ASCII case — CL's `equalp` for strings.
pub(super) fn string_content_eqp(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_str(heap, &args[0])?.eq_ignore_ascii_case(&expect_str(heap, &args[1])?)))
}

pub(super) fn string_lt(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(*expect_str(heap, &args[0])? < *expect_str(heap, &args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `string`, lexicographic (byte
/// order) — the counterpart of `eval_fixed_builtin`'s numeric comparisons, kept
/// in one function for the same reason (one match over the operator symbol).
pub(super) fn string_compare(heap: &Heap, method: SymRef, args: &[Value]) -> Result<Value, EvalError> {
    let a = expect_str(heap, &args[0])?;
    let b = expect_str(heap, &args[1])?;
    Ok(Value::Bool(match method.well_known() {
        wk::M_OP_LT => *a < *b,
        wk::M_OP_LE => *a <= *b,
        wk::M_OP_GT => *a > *b,
        wk::M_OP_GE => *a >= *b,
        other => return Err(EvalError::Internal(format!("string_compare: not a comparison operator: {}", other))),
    }))
}

pub(super) fn char_eq(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])? == expect_char(&args[1])?))
}

pub(super) fn char_lt(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])? < expect_char(&args[1])?))
}

/// The `<`/`<=`/`>`/`>=` comparison operators on `char`, by Unicode scalar
/// value (a compiled `char` is a raw `i64` code point, so this matches the
/// integer `icmp`s `compiler.rs` emits for the same operators).
pub(super) fn char_compare(method: SymRef, args: &[Value]) -> Result<Value, EvalError> {
    let a = expect_char(&args[0])?;
    let b = expect_char(&args[1])?;
    Ok(Value::Bool(match method.well_known() {
        wk::M_OP_LT => a < b,
        wk::M_OP_LE => a <= b,
        wk::M_OP_GT => a > b,
        wk::M_OP_GE => a >= b,
        other => return Err(EvalError::Internal(format!("char_compare: not a comparison operator: {}", other))),
    }))
}

/// CL's `equalp` for `char` — case-insensitive (`(equalp #\A #\a)` is true).
pub(super) fn char_eqp(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_char(&args[0])?.eq_ignore_ascii_case(&expect_char(&args[1])?)))
}
