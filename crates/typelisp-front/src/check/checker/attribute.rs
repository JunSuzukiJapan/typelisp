//! `#[ATTR...] FORM` — a form with attributes, Rust's spelling. The reader
//! hands it over as `(%internal::attribute (ATTR...) FORM)`
//! (`typelisp_read::ATTRIBUTE_HEAD`); what each attribute does to `FORM` is
//! decided here.
//!
//! There is one attribute, `test`, and it goes on a `defun`: a function that
//! takes nothing and returns `()`, run by `typl test`. Whether such a
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

/// One `#[test]` function, as [`Checker::take_tests`] reports it.
#[derive(Clone, Debug)]
pub struct TestCase {
    /// The function's full name — what `typl test` prints and filters on.
    pub path: Path,
    /// The module it is defined in. A test need not be `pub`, so it is
    /// called from inside that module.
    pub ns: Vec<String>,
    /// The name as written, a symbol of that module: calling the test is
    /// checking `(NAME)` there.
    pub name: Value,
    pub loc: Option<Loc>,
}

impl Checker {
    /// What a `#[test]` definition and a top-level expression are checked
    /// into — see [`TestForms`].
    pub fn set_test_forms(&mut self, forms: TestForms) {
        self.test_forms = forms;
    }

    /// The tests recorded under [`TestForms::Collect`] since the last call,
    /// in the order they were defined.
    pub fn take_tests(&mut self) -> Vec<TestCase> {
        std::mem::take(&mut self.tests)
    }

    /// Whether the checker leaves out the top-level expression `tl` — see
    /// [`TestForms::Collect`].
    pub(super) fn drops_toplevel_expr(&self, heap: &Heap, tl: TopLevelForm) -> bool {
        self.test_forms == TestForms::Collect
            && core::op_is(heap, tl, wk::EXPR)
            && !crate::project::needs_immediate_exec(heap, tl)
    }

    /// A top-level form that does nothing: the module being checked, with
    /// nothing in it. What a left-out form is checked into.
    pub(super) fn nothing_form(&self, heap: &mut Heap) -> Result<TopLevelForm, Error> {
        forms::module_form(heap, &Path::from_segments(self.ns.clone()), &[])
    }

    /// `(%internal::attribute (ATTR...) FORM)`.
    pub(super) fn check_attribute(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        let [attrs, form] = parts else {
            return Err(Error::TypeError("an attribute is written `#[ATTR...] FORM`".into()));
        };
        let mut test = false;
        for a in heap.list_to_vec(*attrs)? {
            match a {
                Value::Symbol(id) if heap.symbol_name(id) == "test" => {
                    if test {
                        return Err(Error::TypeError("`#[test]` is given twice".into()));
                    }
                    test = true;
                }
                Value::Symbol(id) => {
                    return Err(Error::TypeError(format!(
                        "unknown attribute `{}`: the one attribute is `test`",
                        heap.symbol_name(id)
                    )))
                }
                _ => return Err(Error::TypeError("an attribute is a name: the one attribute is `test`".into())),
            }
        }
        let elems_locs = match form {
            Value::Cons(_) => heap.list_to_vec_locs(*form)?,
            _ => Vec::new(),
        };
        if !matches!(elems_locs.first(), Some((Value::Symbol(id), _)) if id.is(wk::DEFUN)) {
            return Err(Error::TypeError("`#[test]` goes on a `defun`".into()));
        }
        if self.test_forms == TestForms::Skip {
            return self.nothing_form(heap);
        }
        let elems: Vec<Value> = elems_locs.iter().map(|(v, _)| *v).collect();
        let locs: Vec<Option<Loc>> = elems_locs.into_iter().map(|(_, l)| l).collect();
        let loc = heap.cons_loc(*form).or(def_loc);
        let (name, type_params) = match elems.get(1) {
            Some(&n) => self.parse_defun_name(heap, n)?,
            None => return Err(Error::TypeError("defun: (defun name (params...) ret body...)".into())),
        };
        if !type_params.is_empty() {
            return Err(Error::TypeError(format!("`#[test]` function `{}` cannot take type parameters", name)));
        }
        if elems.get(2) != Some(&Value::Empty) {
            return Err(Error::TypeError(format!("`#[test]` function `{}` cannot take parameters", name)));
        }
        let tl = self.check_defun(heap, interp, &elems[1..], &locs[1..], false, loc.clone())?;
        let ret = &self.cur_ns().fns.get(&name).expect("check_defun registered the function").ret;
        if *ret != Type::Unit {
            return Err(Error::TypeError(format!(
                "`#[test]` function `{}` must return `()`, not `{}`",
                name, ret
            )));
        }
        if self.test_forms == TestForms::Collect {
            self.tests.push(TestCase { path: self.fq(&name), ns: self.ns.clone(), name: elems[1], loc });
        }
        Ok(tl)
    }
}
