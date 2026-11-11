//! `#[ATTR...] FORM`, which the reader hands over as
//! `(%internal::attribute (ATTR...) FORM)`. The one attribute is `test`, on
//! a `defun` that takes nothing and returns `()` or `Result<(), E>`; what
//! becomes of it depends on what is checked for (`TestForms`).

use std::collections::BTreeMap;

use crate::check::{TestCase, TestForms, TestReturn};
use crate::check::registry::TraitBound;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::bounds::settled_meets;
use super::define::Definer;
use super::defun::{check_defun, parse_header};
use super::generic::Requests;
use super::registry::Registry;

/// What a `#[test]` definition is checked into, and the tests found.
#[derive(Debug, Default)]
pub(crate) struct Tests {
    pub(crate) forms: TestForms,
    pub(crate) found: Vec<TestCase>,
}

/// Whether `head`, a form's head, is the reader's `%internal::attribute`.
pub(crate) fn is_attribute(heap: &Heap, head: Value) -> bool {
    match head {
        Value::Path(pid) => heap.path_segments(pid).iter().map(|s| s.well_known()).eq(typelisp_read::ATTRIBUTE_HEAD),
        _ => false,
    }
}

/// `(%internal::attribute (ATTR...) FORM)`, `parts` being what follows the
/// head: its core form, unrooted.
pub(crate) fn check_attribute(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    tests: &mut Tests,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(attrs, _), (form, form_loc)] = parts else {
        return Err(Error::TypeError("an attribute is written `#[ATTR...] FORM`".into()));
    };
    let mut test = false;
    for a in heap.list_to_vec(*attrs)? {
        match a {
            Value::Symbol(id) if id.is(wk::TEST) => {
                if test {
                    return Err(Error::TypeError("`#[test]` is given twice".into()));
                }
                test = true;
            }
            Value::Symbol(id) => {
                return Err(Error::TypeError(format!(
                    "unknown attribute `{}`: the one attribute is `test`",
                    heap.symbol_spelling(id)
                )))
            }
            _ => return Err(Error::TypeError("an attribute is a name: the one attribute is `test`".into())),
        }
    }
    let elems = match form {
        Value::Cons(_) => heap.list_to_vec_locs(*form)?,
        _ => Vec::new(),
    };
    if !matches!(elems.first(), Some((Value::Symbol(id), _)) if id.is(wk::DEFUN)) {
        return Err(Error::TypeError("`#[test]` goes on a `defun`".into()));
    }
    if tests.forms == TestForms::Skip {
        return crate::check::forms::module_form(heap, &Path::in_ns(d.site.ns, "nothing"), &[]);
    }
    let Some(&(header, _)) = elems.get(1) else {
        return Err(Error::TypeError("defun: (defun name (params...) ret body...)".into()));
    };
    let (name, type_names) = parse_header(heap, header)?;
    if !type_names.is_empty() {
        return Err(Error::TypeError(format!("`#[test]` function `{}` cannot take type parameters", name.name())));
    }
    if elems.get(2).map(|(v, _)| *v) != Some(Value::Empty) {
        return Err(Error::TypeError(format!("`#[test]` function `{}` cannot take parameters", name.name())));
    }
    let checked = check_defun(reg, d, heap, requests, &elems[1..])?;
    let path = Path::of_symbol(d.site.ns, d.ownable(heap, name, "defun")?);
    let ret = &reg.fn_def(&path).expect("check_defun defined the function").ret;
    let returns = test_return(reg, name.name(), ret)?;
    if tests.forms == TestForms::Collect {
        let loc = heap.cons_loc(*form).or(form_loc.clone());
        tests.found.push(TestCase { path, ns: d.site.ns, name: header, returns, loc });
    }
    Ok(checked)
}

/// How `typl test` tells that a test returning `ret` failed.
fn test_return(reg: &Registry, name: &str, ret: &Type) -> Result<TestReturn, Error> {
    let Type::Named(p, args) = ret else {
        return match ret {
            Type::Unit => Ok(TestReturn::Unit),
            _ => Err(not_a_test_return(name, ret)),
        };
    };
    if !(crate::types::path_is_builtin(p, typelisp_mem::wk::RESULT) && args.len() == 2 && args[0] == Type::Unit) {
        return Err(not_a_test_return(name, ret));
    }
    // A failing test prints its error: in an `Error`'s own words, which is
    // all a `:dyn Error` can say, or else as `print-object` writes it.
    let bound = |module: &str, name: &str| TraitBound { trait_path: Path::in_ns(crate::types::std_module(module), name), assoc: BTreeMap::new() };
    if settled_meets(reg, &args[1], &bound("error", "Error")) {
        Ok(TestReturn::ResultError)
    } else if settled_meets(reg, &args[1], &bound("fmt", "print-object")) {
        Ok(TestReturn::ResultPrinted)
    } else {
        Err(Error::TypeError(format!(
            "`#[test]` function `{}` returns `{}`, and `{}` implements neither `Error` nor `print-object`: a failing test prints its error",
            name, ret, args[1]
        )))
    }
}

fn not_a_test_return(name: &str, ret: &Type) -> Error {
    Error::TypeError(format!("`#[test]` function `{}` must return `()` or `Result<(), E>`, not `{}`", name, ret))
}
