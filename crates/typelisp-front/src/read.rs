//! The reader now lives in the `typelisp-read` crate (see its doc comment for
//! why) — re-exported here unchanged so every existing `crate::read::…` /
//! `crate::Reader` reference elsewhere in this crate keeps working.
pub use typelisp_read::reader;

pub use reader::*;

/// The reader's `#.` hook, built where a driver already holds both halves of
/// what running a form takes.
///
/// [`Interp`](crate::Interp) implements [`reader::ReadEval`] on its own, but
/// only for a session that has been handed a shared checker
/// (`Interp::set_checker` — the REPL does this up front, for `(eval ...)`).
/// A file load has no such handle: the loader holds the checker by `&mut` for
/// the whole file, which is exactly the thing an `Rc<RefCell<Checker>>` is
/// not. So the driver lends both for the duration of one read.
///
/// The `RefCell` is this struct's own, over a reborrow. Nothing else can
/// touch the checker while a read is in flight — the reborrow says so — and
/// `read_eval` takes `&self` because that is what the reader can offer.
pub struct DriverReadEval<'a> {
    checker: std::cell::RefCell<&'a mut crate::Checker>,
    interp: &'a crate::Interp,
}

impl<'a> DriverReadEval<'a> {
    pub fn new(checker: &'a mut crate::Checker, interp: &'a crate::Interp) -> Self {
        DriverReadEval { checker: std::cell::RefCell::new(checker), interp }
    }
}

impl reader::ReadEval for DriverReadEval<'_> {
    fn read_eval(
        &self,
        heap: &mut crate::Heap,
        form: crate::Value,
    ) -> Result<crate::Value, String> {
        self.interp.read_eval_allowed(heap)?;
        // The same discipline `Interp::read_eval_form` follows: root the form
        // across checking (which allocates), release before `exec`.
        let mark = heap.root_count();
        heap.push_root(form);
        let checked = self.checker.borrow_mut().check_form_at(heap, self.interp, form, None);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let tl = checked.map_err(|e| e.to_string())?;
        match self.interp.exec(heap, tl) {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Ok(crate::Value::Empty),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Straight to the interpreter: calling a reader macro needs no checker
    /// at all. The function was type-checked where it was written, and the
    /// glue it goes through (`Interp::call_reader_macro_fn`) was checked with
    /// the prelude.
    fn call_reader_macro(
        &self,
        heap: &mut crate::Heap,
        f: crate::Value,
        ch: char,
        rest: &str,
    ) -> Result<(crate::Value, usize), String> {
        self.interp.call_reader_macro_fn(heap, f, ch, rest)
    }
}
