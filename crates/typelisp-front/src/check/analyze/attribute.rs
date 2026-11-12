//! `#[ATTR...] FORM` — a form with attributes, Rust's spelling. The reader
//! hands it over as `(%internal::attribute (ATTR...) FORM)`
//! (`typelisp_read::ATTRIBUTE_HEAD`); what each attribute does to `FORM` is
//! decided here.
//!
//! There is one attribute, `test`, and it goes on a `defun`: a function that
//! takes nothing and returns `()` or `Result<(), E>`, run by `typl test`. Whether such a
//! function exists at all depends on what the checker is checking for —
//! [`TestForms`] — the way Rust compiles a `#[test]` function only under
//! `--test`.

use super::*;

/// What the checker does with a `#[test]` definition, and with a top-level
/// expression.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TestForms {
    /// A `#[test]` definition is left out without being checked — an
    /// ordinary run, `compile-file`, the REPL.
    #[default]
    Skip,
    /// A `#[test]` definition is checked and defined like any other. The
    /// editor, which has to report the errors in a test too.
    Check,
    /// [`TestForms::Check`], and each test is recorded for
    /// [`Checker::take_tests`]. A top-level expression is checked and then
    /// left out: it is the program's own work, which a test run does not do —
    /// Go's `main` under `go test`. A readtable registration still runs, since
    /// the rest of the file cannot be read without it.
    Collect,
}

/// What a `#[test]` function returns, and so how `typl test` tells (and
/// reports) a failure besides a `panic`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TestReturn {
    /// `()`: only a `panic` or a runtime error fails it.
    Unit,
    /// `Result<(), E>` with `E` an `Error` (a `:dyn Error` too): an `err`
    /// fails it, reported by `describe-error` — the message and its causes.
    ResultError,
    /// `Result<(), E>` with `E` no `Error` but printable: an `err` fails it,
    /// reported by `print-object`.
    ResultPrinted,
}

/// One `#[test]` function, as [`Checker::take_tests`] reports it.
#[derive(Clone, Debug)]
pub struct TestCase {
    /// The function's full name — what `typl test` prints and filters on.
    pub path: Path,
    /// The module it is defined in. A test need not be `pub`, so it is
    /// called from inside that module.
    pub ns: typelisp_mem::NsId,
    /// The name as written, a symbol of that module: calling the test is
    /// checking `(NAME)` there.
    pub name: Value,
    pub returns: TestReturn,
    pub loc: Option<Loc>,
}

impl Checker {
    /// What a `#[test]` definition and a top-level expression are checked
    /// into — see [`TestForms`].
    pub fn set_test_forms(&mut self, forms: TestForms) {
        self.rebuilt.set_test_forms(forms);
    }

    /// The tests recorded under [`TestForms::Collect`] since the last call,
    /// in the order they were defined.
    pub fn take_tests(&mut self) -> Vec<TestCase> {
        self.rebuilt.take_tests()
    }

}
