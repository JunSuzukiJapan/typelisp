//! `#[ATTR...] FORM`, which the reader hands over as
//! `(%internal::attribute (ATTR...) FORM)`, an attribute with arguments
//! (`interrupt(7)`) as the list `(interrupt 7)`. Each goes on a `defun` that
//! takes nothing:
//!
//! - `test`, returning `()` or `Result<(), E>`; what becomes of it depends on
//!   what is checked for (`TestForms`).
//! - `interrupt(N)` and `exception(NAME)`, returning `()`: the function is the
//!   handler of a board's interrupt `N` or of the core exception `NAME`
//!   ([`Attributed::handlers`]). Anywhere else it is an ordinary function.

use std::collections::BTreeMap;

use crate::typeck::registry::TraitBound;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::bounds::settled_meets;
use super::define::Definer;
use super::defun::{check_defun, parse_header};
use super::generic::Requests;
use super::registry::Registry;

/// What the definitions with an attribute are checked into.
#[derive(Debug, Default)]
pub(crate) struct Attributed {
    pub(crate) forms: TestForms,
    /// The `#[test]` functions found.
    pub(crate) found: Vec<TestCase>,
    /// The interrupt handlers, by the number of the vector-table entry each
    /// is at: an exception's number, or 16 + an interrupt's.
    pub(crate) handlers: BTreeMap<u16, Path>,
}

/// How many interrupts a Cortex-M core's vector table has entries for.
pub const INTERRUPTS: i64 = 240;

/// The core exceptions a program may handle, with their vector-table
/// entries. `HardFault` is the runtime's, which ends the program; `Reset` is
/// where the program starts.
pub const EXCEPTIONS: [(&str, u16); 8] = [
    ("NonMaskableInt", 2),
    ("MemoryManagement", 4),
    ("BusFault", 5),
    ("UsageFault", 6),
    ("SVCall", 11),
    ("DebugMonitor", 12),
    ("PendSV", 14),
    ("SysTick", 15),
];

/// An attribute, as [`attribute`] reads it.
enum Attribute {
    Test,
    /// A handler at this vector-table entry; the attribute as written, for
    /// the messages.
    Handler(u16, String),
}

/// The attributes there are, for a message about one that is not.
const ATTRIBUTES: &str = "the attributes are `test`, `interrupt(N)` and `exception(NAME)`";

/// The attribute `a` names.
///
/// `interrupt` and `exception` are told by their spelling: an attribute's
/// name is read in the program's module and defines nothing, and putting the
/// words in the vocabulary would make them reserved, so that no program could
/// name a function `interrupt`. `test` is a vocabulary word already.
fn attribute(heap: &Heap, a: Value) -> Result<Attribute, Error> {
    let name_of = |v: Value| match v {
        Value::Symbol(id) => Some(id),
        _ => None,
    };
    let (name, args) = match a {
        Value::Symbol(id) => (id, None),
        Value::Cons(_) => match name_of(heap.car(a)?) {
            Some(id) => (id, Some(heap.list_to_vec(heap.cdr(a)?)?)),
            None => return Err(Error::TypeError(format!("an attribute is a name, or a name with its arguments: {}", ATTRIBUTES))),
        },
        _ => return Err(Error::TypeError(format!("an attribute is a name, or a name with its arguments: {}", ATTRIBUTES))),
    };
    let spelling = heap.symbol_spelling(name);
    match (&*spelling, args.as_deref()) {
        _ if name.is(wk::TEST) => match args {
            None => Ok(Attribute::Test),
            Some(_) => Err(Error::TypeError("`#[test]` takes no arguments".into())),
        },
        ("interrupt", Some([Value::Int(n)])) if (0..INTERRUPTS).contains(n) => {
            Ok(Attribute::Handler(16 + *n as u16, format!("#[interrupt({})]", n)))
        }
        ("interrupt", Some([Value::Int(n)])) => Err(Error::TypeError(format!(
            "`#[interrupt({})]`: an interrupt is numbered from 0 to {}",
            n,
            INTERRUPTS - 1
        ))),
        ("interrupt", _) => Err(Error::TypeError("an interrupt handler is written `#[interrupt(N)]`, N its number".into())),
        ("exception", Some([Value::Symbol(e)])) => {
            let e = heap.symbol_spelling(*e);
            match EXCEPTIONS.iter().find(|(n, _)| *n == e) {
                Some((_, vector)) => Ok(Attribute::Handler(*vector, format!("#[exception({})]", e))),
                None => Err(Error::TypeError(format!(
                    "`#[exception({})]`: the exceptions a program handles are {}",
                    e,
                    EXCEPTIONS.iter().map(|(n, _)| format!("`{}`", n)).collect::<Vec<_>>().join(", ")
                ))),
            }
        }
        ("exception", _) => Err(Error::TypeError("an exception handler is written `#[exception(NAME)]`, NAME the exception's".into())),
        _ => Err(Error::TypeError(format!("unknown attribute `{}`: {}", spelling, ATTRIBUTES))),
    }
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
    attributed: &mut Attributed,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let [(attrs, _), (form, form_loc)] = parts else {
        return Err(Error::TypeError("an attribute is written `#[ATTR...] FORM`".into()));
    };
    let mut test = false;
    let mut handler: Option<(u16, String)> = None;
    for a in heap.list_to_vec(*attrs)? {
        match attribute(heap, a)? {
            Attribute::Test if test => return Err(Error::TypeError("`#[test]` is given twice".into())),
            Attribute::Test => test = true,
            Attribute::Handler(vector, written) => match &handler {
                Some((_, first)) => {
                    return Err(Error::TypeError(format!("`{}` and `{}`: a function handles one interrupt", first, written)))
                }
                None => handler = Some((vector, written)),
            },
        }
    }
    let what = match &handler {
        Some((_, written)) => written.clone(),
        None => "#[test]".to_string(),
    };
    if test && handler.is_some() {
        return Err(Error::TypeError(format!("`#[test]` and `{}`: a test is not an interrupt handler", what)));
    }
    let elems = match form {
        Value::Cons(_) => heap.list_to_vec_locs(*form)?,
        _ => Vec::new(),
    };
    if !matches!(elems.first(), Some((Value::Symbol(id), _)) if id.is(wk::DEFUN)) {
        return Err(Error::TypeError(format!("`{}` goes on a `defun`", what)));
    }
    if let Some((vector, written)) = handler {
        return check_handler(reg, d, heap, requests, attributed, &elems[1..], vector, &written);
    }
    if attributed.forms == TestForms::Skip {
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
    if attributed.forms == TestForms::Collect {
        let loc = heap.cons_loc(*form).or(form_loc.clone());
        attributed.found.push(TestCase { path, ns: d.site.ns, name: header, returns, loc });
    }
    Ok(checked)
}

/// The `defun` (`parts` without its head) of the handler `written` at the
/// vector-table entry `vector`: a function that takes nothing and returns
/// `()`, defined like any other, and recorded in `attributed`.
#[allow(clippy::too_many_arguments)]
fn check_handler(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    attributed: &mut Attributed,
    parts: &[(Value, Option<Loc>)],
    vector: u16,
    written: &str,
) -> Result<Value, Error> {
    let Some(&(header, _)) = parts.first() else {
        return Err(Error::TypeError("defun: (defun name (params...) ret body...)".into()));
    };
    let (name, type_names) = parse_header(heap, header)?;
    if !type_names.is_empty() {
        return Err(Error::TypeError(format!("`{}` function `{}` cannot take type parameters", written, name.name())));
    }
    if parts.get(1).map(|(v, _)| *v) != Some(Value::Empty) {
        return Err(Error::TypeError(format!("`{}` function `{}` cannot take parameters: nothing calls it with any", written, name.name())));
    }
    let checked = check_defun(reg, d, heap, requests, parts)?;
    let path = Path::of_symbol(d.site.ns, d.ownable(heap, name, "defun")?);
    let ret = &reg.fn_def(&path).expect("check_defun defined the function").ret;
    if *ret != Type::Unit {
        return Err(Error::TypeError(format!("`{}` function `{}` must return `()`, not `{}`: nothing reads its answer", written, name.name(), ret)));
    }
    match attributed.handlers.get(&vector) {
        Some(other) if *other != path => {
            return Err(Error::TypeError(format!("`{}` is handled by `{}` already, so `{}` cannot handle it too", written, other, path)))
        }
        _ => {
            attributed.handlers.retain(|_, p| *p != path);
            attributed.handlers.insert(vector, path);
        }
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
    /// `Result<(), E>` with `E` an `Error` (a `:dyn Error` too): an `Err`
    /// fails it, reported by `describe-error` — the message and its causes.
    ResultError,
    /// `Result<(), E>` with `E` no `Error` but printable: an `Err` fails it,
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
