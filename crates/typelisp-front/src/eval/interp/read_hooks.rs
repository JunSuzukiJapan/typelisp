//! The reader's hooks into the running program: `#.`, reader macros, and the
//! `read` built-ins.

use std::rc::Rc;
use crate::{Heap, Path, Value};
use crate::eval::value::EvalError;
use super::{expect_str, with_active_interp, Interp};

impl Interp {
    /// CL's `*read-eval*` (the prelude's global): `Err` when it is `false`,
    /// which turns a `#.` into a read error. Asked by both implementors of
    /// the reader's `#.` hook — this one and `read::DriverReadEval` — before
    /// anything runs. A session without the prelude has no such global, and
    /// `#.` is allowed, CL's default.
    pub(crate) fn read_eval_allowed(&self, heap: &Heap) -> Result<(), String> {
        match self.global_value(heap, &crate::Path::in_ns(crate::types::std_module("reader"), "*read-eval*")) {
            Some(Value::Bool(false)) => Err("`#.` is switched off: `*read-eval*` is false".to_string()),
            _ => Ok(()),
        }
    }

    /// [`typelisp_read::reader::ReadEval`] for the reader's `#.`.
    ///
    /// The same act as [`Self::eval_form`] — check this datum, run it — with
    /// the CL-conformant `eval` wrapping left off: `eval` answers a
    /// `Result<Sexpr, Error>` because a program calling it wants to handle a
    /// failure, while a `#.` that does not check is a *read* error and the
    /// reader reports it with the position, like any other.
    ///
    /// A definition placed in a `#.` still defines; what the reader splices in
    /// is whatever the form produced, which for a definition is nothing
    /// (`()`).
    pub fn read_eval_form(&self, heap: &mut Heap, form: Value) -> Result<Value, String> {
        self.read_eval_allowed(heap)?;
        let Some(checker) = self.checker.as_ref().map(Rc::clone) else {
            return Err("`#.` needs a checker handle, and this session has none".to_string());
        };
        let mark = heap.root_count();
        heap.push_root(form);
        let checked = checker.borrow_mut().check_form_at(heap, self, form, None);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let tl = checked.map_err(|e| e.to_string())?;
        match self.exec(heap, tl) {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Ok(Value::Empty),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The reader's hook for a macro character: run `f` on `ch` and the text
    /// the reader has not consumed, answering the datum and how many
    /// characters of `rest` went into it.
    ///
    /// The call goes through the prelude's `call-reader-macro`, not straight
    /// through [`Self::enter`]. A reader macro takes a *stream*, and making
    /// one out of `rest` — a `string-input-stream` upcast to
    /// `:dyn PeekInput` — is work with a type on it, which is to say the
    /// checker's; doing it from Rust would mean assembling a trait object by
    /// hand. Going through typelisp also means `f` is called the ordinary
    /// way, so a *compiled* reader macro works with nothing added here.
    pub fn call_reader_macro_fn(
        &self,
        heap: &mut Heap,
        f: Value,
        ch: char,
        rest: &str,
    ) -> Result<(Value, usize), String> {
        let def = self
            .root
            .borrow()
            .get_fn(&Path::internal(READER_MACRO_GLUE))
            .ok_or_else(|| format!("{}: not defined in {}", READER_MACRO_GLUE, crate::INTERNAL_MODULE))?;
        let mark = heap.root_count();
        // `f` is rooted first: allocating the string can collect, and the
        // readtable's permanent root is not the only thing that has to be
        // true here — a `f` built on the fly and passed straight in has no
        // other holder.
        heap.push_root(f);
        let text = heap.alloc_string(rest.to_string());
        heap.push_root(text);
        // `enter`, not `apply` — see `print_object_method`'s call for why.
        // The glue is prelude code and the prelude's dump generator compiles
        // it, so here the difference is not hypothetical.
        let out = self.enter(heap, &def, vec![f, Value::Char(ch), text]);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let out = out.map_err(|e| e.to_string())?;
        // `cons-cell<Option<Sexpr>, i32>` — a `defstruct`, so a two-field
        // struct box. Anything else means the glue was redefined out from
        // under this, which is a broken image rather than a user error.
        let Value::Boxed(id) = out else {
            return Err(format!("{}: expected a cons-cell, got {:?}", READER_MACRO_GLUE, out));
        };
        let datum = heap.struct_field(id, 0);
        let used = match heap.struct_field(id, 1) {
            Value::Int(n) if n >= 0 => n as usize,
            other => return Err(format!("{}: consumed count is not a count: {:?}", READER_MACRO_GLUE, other)),
        };
        Ok((datum, used))
    }
}

/// `read` (`registry.rs`'s free-function entry): parses exactly one `Sexpr`
/// form from `s` via the ordinary reader (`crate::read::Reader::read`) —
/// the same pipeline `typl`/the REPL use for source text, just callable at
/// runtime on a string value instead of a file/stdin. `Err` (not a panic)
/// on malformed input, e.g. an unterminated list or string — this reads
/// data the running program doesn't control.
impl Interp {
    pub(super) fn eval_read(&self, heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
        let s = expect_str(heap, &args[0])?.to_string();
        // `typelisp_read::shim`, the same implementation `rt_read` calls: one
        // reader, one `Result` shape, whichever side of the compile boundary the
        // caller is on.
        Ok(typelisp_read::shim::read_builtin_with(heap, &s, Some(self)))
    }

    pub(super) fn eval_read_datum_at(&self, heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
        let s = expect_str(heap, &args[0])?.to_string();
        let start = match args[1] {
            Value::Int(n) => n,
            ref other => return Err(EvalError::Internal(format!("read-datum-at: start is not an integer: {:?}", other))),
        };
        let preserve = matches!(args[2], Value::Bool(true));
        Ok(typelisp_read::shim::read_datum_at_builtin_with(heap, &s, start, preserve, Some(self)))
    }
}

/// The prelude function the reader's macro-character hook goes through — see
/// [`Interp::call_reader_macro_fn`]. It lives in [`crate::INTERNAL_MODULE`].
const READER_MACRO_GLUE: &str = "call-reader-macro";

/// The reader's [`typelisp_read::runtime::ReadHooks`] pointing at the
/// interpreter — what makes the `read`/`read-datum-at` *builtins* honour `#.`
/// and macro characters, which the drivers' own reads already do by being
/// handed a `ReadEval` directly.
///
/// Same shape and same reason as [`INTERP_PRINT_HOOKS`]: plain `fn` pointers
/// reaching the running interpreter through [`ACTIVE_INTERP`], refreshed at
/// the point of use so a nested `Interp` cannot leave a finished one behind.
pub(super) const INTERP_READ_HOOKS: typelisp_read::runtime::ReadHooks = typelisp_read::runtime::ReadHooks {
    read_eval: |heap, form| match with_active_interp(|i| i.read_eval_form(heap, form)) {
        Some(r) => r,
        None => Err("`#.` needs a program to run the form in".to_string()),
    },
    call_reader_macro: |heap, f, ch, rest| match with_active_interp(|i| i.call_reader_macro_fn(heap, f, ch, rest)) {
        Some(r) => r,
        None => Err(format!("`{}` is a reader macro, and there is no program to run it in", ch)),
    },
};

/// The reader's hook into the evaluator, for `#.` — see
/// [`Interp::read_eval_form`].
impl typelisp_read::reader::ReadEval for Interp {
    fn read_eval(&self, heap: &mut Heap, form: Value) -> Result<Value, String> {
        self.read_eval_form(heap, form)
    }

    fn call_reader_macro(
        &self,
        heap: &mut Heap,
        f: Value,
        ch: char,
        rest: &str,
    ) -> Result<(Value, usize), String> {
        self.call_reader_macro_fn(heap, f, ch, rest)
    }

    /// The session's checker registers what the `use` names; a session
    /// driven this way (the REPL) loads no files.
    fn bring_in(&self, heap: &mut Heap, ns: typelisp_mem::NsId, path: &[typelisp_mem::SymRef]) -> Result<(), String> {
        let Some(checker) = self.checker.as_ref() else {
            let spelled: Vec<&str> = path.iter().map(|s| s.name()).collect();
            return Err(format!("`(use {})` needs a checker handle, and this session has none", spelled.join("::")));
        };
        checker.borrow().register_use_for_reading(heap, ns, path);
        Ok(())
    }
}
