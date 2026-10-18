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
    interp: std::cell::RefCell<&'a mut crate::Interp>,
    /// What loads the file behind a `use`, when this driver loads files.
    files: Option<std::cell::RefCell<Files<'a>>>,
}

/// A project loader, lent for one read: what [`DriverReadEval::bring_in`]
/// loads a `use`'s file with.
struct Files<'a> {
    loader: &'a mut crate::project::Loader,
    reader: &'a Reader,
    /// The module path of the file being read — what a sibling file is
    /// found relative to.
    file_module: &'a [String],
}

impl<'a> DriverReadEval<'a> {
    /// A driver that reads one text and loads no other: a `use` is resolved
    /// against what the session has.
    pub fn new(checker: &'a mut crate::Checker, interp: &'a mut crate::Interp) -> Self {
        DriverReadEval { checker: std::cell::RefCell::new(checker), interp: std::cell::RefCell::new(interp), files: None }
    }

    /// A driver that loads the file behind a `use` with `loader`, the file
    /// being read being the module `file_module`.
    pub fn with_loader(
        checker: &'a mut crate::Checker,
        interp: &'a mut crate::Interp,
        loader: &'a mut crate::project::Loader,
        reader: &'a Reader,
        file_module: &'a [String],
    ) -> Self {
        DriverReadEval {
            checker: std::cell::RefCell::new(checker),
            interp: std::cell::RefCell::new(interp),
            files: Some(std::cell::RefCell::new(Files { loader, reader, file_module })),
        }
    }
}

impl reader::ReadEval for DriverReadEval<'_> {
    fn read_eval(
        &self,
        heap: &mut crate::Heap,
        form: crate::Value,
    ) -> Result<crate::Value, String> {
        let interp = self.interp.borrow();
        interp.read_eval_allowed(heap)?;
        // The same discipline `Interp::read_eval_form` follows: root the form
        // across checking (which allocates), release before `exec`.
        let mark = heap.root_count();
        heap.push_root(form);
        let checked = self.checker.borrow_mut().check_form_at(heap, &**interp, form, None);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let tl = checked.map_err(|e| e.to_string())?;
        match interp.exec(heap, tl) {
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
        self.interp.borrow().call_reader_macro_fn(heap, f, ch, rest)
    }

    /// The file behind the path, loaded if this driver loads files and
    /// nothing has loaded it yet; then the checker registers what the `use`
    /// brings in.
    fn bring_in(&self, heap: &mut crate::Heap, ns: crate::NsId, path: &[String]) -> Result<(), String> {
        if let Some(files) = &self.files {
            let mut files = files.borrow_mut();
            let Files { loader, reader, file_module } = &mut *files;
            loader
                .ensure_loaded(heap, reader, &mut self.checker.borrow_mut(), &mut self.interp.borrow_mut(), path, file_module)
                .map_err(|e| e.to_string())?;
        }
        self.checker.borrow().register_use_for_reading(heap, ns, path);
        Ok(())
    }
}
