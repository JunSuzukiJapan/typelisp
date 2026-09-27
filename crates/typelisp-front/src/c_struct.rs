//! The internal builtins `def-c-struct` and typed pointers lower to.
//!
//! The checker turns `(c-alloc T n)`, `(c-ref p i)`, `(c-deref p)`, `p::field`
//! and their `setf`s into calls to these, and wraps the `(unsafe ...)` that
//! owns the allocations in an arena opened and closed around it
//! (`checker::c_struct`). Every name starts with a space, so no program can
//! write one; each maps to one `typelisp_rt::c_mem` shim for compiled code
//! (`compile::externs`) and is answered by [`eval`] in the interpreter. Both
//! run the same `c_mem` functions.

use typelisp_mem::{Heap, Value};
use typelisp_rt::c_mem;

use crate::check::repr::Repr;
use crate::eval::EvalError;

/// `()` → the new arena's id.
pub const ARENA_OPEN: &str = " c-arena-open";
/// `(ARENA)` → `()`: frees what the arena owns.
pub const ARENA_CLOSE: &str = " c-arena-close";
/// `(ARENA COUNT DESC)` → the block's address.
pub const ALLOC: &str = " c-alloc";
/// `(ADDR KEY)` → `ADDR`, if it points at a live `KEY`.
pub const PTR_CHECK: &str = " c-ptr-check";
/// `(ADDR I SIZE KEY)` → the address of the `I`th element.
pub const INDEX: &str = " c-index";
/// `(ADDR OFFSET)` → the address of a field.
pub const OFFSET: &str = " c-offset";
/// `(ADDR OFFSET KIND)` → the value.
pub const LOAD: &str = " c-load";
/// `(ADDR OFFSET KIND VALUE)` → `()`.
pub const STORE: &str = " c-store";

/// The variable the owning `unsafe` binds its arena to. Unwritable for the
/// same reason the builtins' names are.
pub const ARENA_VAR: &str = " c-arena";

/// Each builtin with the `typelisp_rt` shim compiled code calls for it.
pub const BUILTINS: [(&str, &str); 8] = [
    (ARENA_OPEN, "rt_c_arena_open"),
    (ARENA_CLOSE, "rt_c_arena_close"),
    (ALLOC, "rt_c_alloc"),
    (PTR_CHECK, "rt_c_ptr_check"),
    (INDEX, "rt_c_index"),
    (OFFSET, "rt_c_offset"),
    (LOAD, "rt_c_load"),
    (STORE, "rt_c_store"),
];

/// The shim for `name`, if it is one of these builtins.
pub fn rt_symbol(name: &str) -> Option<&'static str> {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// How a value of load/store kind `kind` is represented — what the
/// interpreter converts through around `c_mem::load`/`store`.
pub fn kind_repr(kind: i64) -> Repr {
    match kind {
        c_mem::KIND_C_LONG | c_mem::KIND_C_ULONG | c_mem::KIND_PTR => Repr::RawWord,
        c_mem::KIND_F32 => Repr::F32,
        c_mem::KIND_F64 => Repr::F64,
        c_mem::KIND_BOOL => Repr::Bool,
        _ => Repr::Narrow,
    }
}

/// The interpreter's answer to a call of builtin `name`, or `None` if `name`
/// is not one of these.
pub(crate) fn eval(heap: &mut Heap, name: &str, argv: &[Value]) -> Option<Result<Value, EvalError>> {
    rt_symbol(name)?;
    Some(eval_known(heap, name, argv))
}

fn eval_known(heap: &mut Heap, name: &str, argv: &[Value]) -> Result<Value, EvalError> {
    let word = |i: usize| -> Result<i64, EvalError> {
        match argv.get(i) {
            Some(Value::Int(n)) => Ok(*n),
            other => Err(EvalError::Internal(format!("{}: argument {} is not a word: {:?}", name, i, other))),
        }
    };
    let text = |heap: &Heap, i: usize| -> Result<String, EvalError> {
        match argv.get(i) {
            Some(Value::Str(id)) => Ok(heap.string(*id).to_string()),
            other => Err(EvalError::Internal(format!("{}: argument {} is not a string: {:?}", name, i, other))),
        }
    };
    let raised = EvalError::Panic;
    Ok(match name {
        ARENA_OPEN => Value::Int(c_mem::arena_open() as i64),
        ARENA_CLOSE => {
            c_mem::arena_close(word(0)? as u64);
            Value::Empty
        }
        ALLOC => Value::Int(c_mem::alloc(word(0)? as u64, word(1)?, &text(heap, 2)?).map_err(raised)? as i64),
        PTR_CHECK => {
            let addr = word(0)?;
            c_mem::check(addr as usize, &text(heap, 1)?).map_err(raised)?;
            Value::Int(addr)
        }
        INDEX => Value::Int(
            c_mem::index(word(0)? as usize, word(1)?, word(2)? as usize, &text(heap, 3)?).map_err(raised)? as i64,
        ),
        OFFSET => Value::Int(word(0)?.wrapping_add(word(1)?)),
        LOAD => {
            let kind = word(2)?;
            // The address came from a checked typed pointer plus an offset
            // the checker computed from the struct's layout.
            let raw = unsafe { c_mem::load(word(0)?.wrapping_add(word(1)?) as usize, kind) };
            crate::eval::interp::decode_crossing_return(heap, raw, &kind_repr(kind))?
        }
        STORE => {
            let kind = word(2)?;
            let v = argv.get(3).ok_or_else(|| EvalError::Internal(format!("{}: no value to store", name)))?;
            let raw = crate::eval::interp::encode_crossing_value(heap, v, &kind_repr(kind))?;
            // As `LOAD`.
            unsafe { c_mem::store(word(0)?.wrapping_add(word(1)?) as usize, kind, raw) };
            Value::Empty
        }
        _ => unreachable!("rt_symbol accepted `{}`", name),
    })
}
