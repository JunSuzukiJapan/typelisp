//! Global variables: `(defvar (name Type) value)`, `defparameter` and
//! `defconstant`.
//!
//! `defvar` and `defparameter` define a variable that may be assigned, and
//! `defconstant` one that may not. Defining one again with `defparameter`
//! sets it to the new value; `defvar` leaves a value it already has (CL).
//! The value is checked before the variable is defined, so it cannot read
//! the variable.

use crate::check::lower::CoreForms;
use crate::check::Checker;
use crate::types::Path;
use crate::{Error, Heap, Loc, Value};

use super::define::{Definer, Kind};
use super::expr::{check_expr, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::{Definition, Registry, ValueDef, VarDef};
use super::scope::TypeScope;
use super::subst::Subst;
use super::type_syntax::parse_type;

const SHAPE: &str = "defvar/defconstant: (defvar (name Type) value) or (defvar (name Type) value \"doc\")";

/// Which of the three forms defines the variable.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Which {
    Defvar,
    Defparameter,
    Defconstant,
}

/// `(defvar ...)` — `parts` without the head — and its core form, unrooted.
pub(crate) fn check_defvar(
    reg: &mut Registry,
    d: &mut Definer,
    heap: &mut Heap,
    requests: &mut Requests,
    which: Which,
    parts: &[(Value, Option<Loc>)],
) -> Result<Value, Error> {
    let (head, value, doc) = match parts {
        [(head, _), value] => (*head, value, None),
        [(head, _), value, (Value::Str(id), _)] => (*head, value, Some(heap.string(*id).to_string())),
        [_, _, _] => return Err(Error::TypeError("defvar/defconstant: third argument must be a docstring".into())),
        _ => return Err(Error::TypeError(SHAPE.into())),
    };
    let (name, ty) = match head {
        Value::Symbol(id) => {
            return Err(Error::TypeError(format!(
                "defvar: a global needs a declared type — write (defvar ({} Type) value)",
                heap.symbol_spelling(id)
            )))
        }
        _ => match heap.list_to_vec(head).ok().as_deref() {
            Some([Value::Symbol(name), ty]) => (*name, *ty),
            Some([_, _]) => return Err(Error::TypeError("defvar: name must be a symbol".into())),
            _ => return Err(Error::TypeError("defvar: name must be (name Type)".into())),
        },
    };
    if name.is_keyword() {
        return Err(Error::TypeError(format!(
            "defvar: `{}` is a keyword — a constant that names itself — so it cannot be bound, assigned or defined",
            name.spelling()
        )));
    }
    let name = d.ownable(heap, name, "defvar")?;
    d.record(reg, Definition::Var(Path::of_symbol(d.site.ns, name)), doc);
    let scope = TypeScope::empty();
    let ty = parse_type(heap, &super::type_syntax::TypeCx { reg, ns: d.site.ns, scope: &scope, self_type: None }, ty)?;
    Checker::reject_raw_word_storage(&ty, "the global", &heap.symbol_spelling(name))?;
    let value = {
        let cx = Cx::new(reg, d.expander, d.site.ns, &scope, None, d.notes);
        let mut value = check_expr(&cx, heap, &Locals::empty(), value.0, Some(&ty), value.1.clone())?;
        cx.settle(heap, std::slice::from_mut(&mut value))?;
        value
    };
    d.value(reg, Kind::Variable, name)?;
    let path = Path::of_symbol(d.site.ns, name);
    let mutable = !matches!(which, Which::Defconstant);
    reg.values.insert(path, ValueDef::Var(VarDef { ty: ty.clone(), public: d.public, mutable }));
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let form = lower_rooted(heap, &mut lw, &value)?;
    reg.defvar_form(heap, &path, &ty, mutable, form, matches!(which, Which::Defparameter))
}
