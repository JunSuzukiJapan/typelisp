//! `def-c-struct` and typed pointers: memory laid out the way C lays it out,
//! allocated inside `(unsafe ...)` and freed when that form is left.
//!
//! ```lisp
//! (unsafe (def-c-struct point (x i32) (y i32)))
//!
//! (unsafe
//!   (let ((ps (c-alloc point 3)))       ; (ptr point), zeroed
//!     (setf (c-ref ps 1)::x 5) ...))    ; not this: see `c-ref` below
//! ```
//!
//! # Who owns an allocation
//!
//! The outermost `(unsafe ...)` of a function body (or of a top-level form)
//! owns every `c-alloc` inside it. That form is lowered to
//!
//! ```text
//! (let ((" c-arena" (" c-arena-open")))
//!   (unwind-protect BODY (" c-arena-close" " c-arena")))
//! ```
//!
//! so leaving it by any path frees the memory. A `lambda` or `labels`
//! function inside starts over: it is a function of its own, may run after
//! the form it was written in is left, and cannot capture a typed pointer.
//!
//! # Keeping a typed pointer inside its owner
//!
//! A typed pointer is a raw word, so the rules for `ptr` already keep it out
//! of storage and type arguments and out of expressions outside `unsafe`.
//! What remains is every way a value can outlive the form it was made in, and
//! each is refused here: being the value of an `unsafe` form, being captured
//! by a closure, being passed to a `task`/`thread`, being thrown.
//!
//! # Pointers from C
//!
//! A typed pointer C hands over — a callback's argument, a `defffi` result,
//! a pointer field read back — is checked against the live allocations at
//! run time (`typelisp_rt::c_mem::check`): memory C allocated is refused.

use super::*;

impl Checker {
    // ---- declarations ------------------------------------------------------

    /// A top-level `(unsafe ...)` whose every form is a `def-c-struct`
    /// (possibly `pub`): `Some(true)`. One that holds none: `Some(false)`, an
    /// ordinary expression. A mixture is refused — the declarations and the
    /// expression would be checked by two different rules.
    pub(crate) fn unsafe_declares_c_structs(heap: &Heap, forms: &[Value]) -> Result<bool, Error> {
        let is_decl = |v: &Value| -> bool {
            let Ok(items) = heap.list_to_vec(*v) else { return false };
            let head = |i: usize, konst: u32| matches!(items.get(i), Some(Value::Symbol(id)) if id.is(konst));
            head(0, wk::DEF_C_STRUCT) || (head(0, wk::PUB) && head(1, wk::DEF_C_STRUCT))
        };
        let n = forms.iter().filter(|v| is_decl(v)).count();
        if n > 0 && n != forms.len() {
            return Err(Error::TypeError(
                "an `(unsafe ...)` that declares C structs holds nothing but `def-c-struct`s — \
                 write the expressions in an `(unsafe ...)` of their own"
                    .into(),
            ));
        }
        Ok(n > 0)
    }

    // ---- types ---------------------------------------------------------------

    // ---- ownership -------------------------------------------------------------

    // ---- expressions -------------------------------------------------------------

}
