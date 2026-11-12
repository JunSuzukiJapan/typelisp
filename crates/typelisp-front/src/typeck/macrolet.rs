//! `macrolet`: `(macrolet ((name (params) body...)...) body...)`, CL's.
//!
//! Each macro is a `defmacro` whose name is seen only in the body, where a
//! nearer binding of the name hides it. Its body is checked and handed to
//! the evaluator here, before the body is checked, so the calls of it in the
//! body are expanded as a module-level macro's are.

use crate::typeck::defmacro::{parse_macro_lambda_list, MacroShape};
use crate::typeck::registry::MacroDef;
use crate::check::verify::bound_name;
use crate::types::{Path, Type};
use crate::{Error, Heap, Loc, Value};

use super::defmacro::{macro_form, MacroSite};
use super::defun::split_doc;
use super::expr::{check_body, Cx};
use super::generic::Requests;
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(macrolet ((name (params) body...)...) body...)` — `args` without the
/// `macrolet`.
pub(crate) fn check_macrolet(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let Some(((bindings, _), body)) = args.split_first() else {
        return Err(Error::TypeError("macrolet: (macrolet ((name (params) body...)...) body...)".into()));
    };
    let bindings = heap.list_to_vec_locs(*bindings).map_err(|_| Error::TypeError("macrolet: the binding list must be a proper list".into()))?;
    let mut macros = Vec::with_capacity(bindings.len());
    for (binding, _) in bindings {
        let parts = heap.list_to_vec_locs(binding).map_err(|_| Error::TypeError("macrolet: each binding is (name (params) body...)".into()))?;
        let [(name, _), (params, _), rest @ ..] = parts.as_slice() else {
            return Err(Error::TypeError("macrolet: each binding is (name (params) body...)".into()));
        };
        let Value::Symbol(name) = *name else {
            return Err(Error::TypeError("macrolet: the binding name must be a symbol".into()));
        };
        bound_name(name, "macrolet")?;
        let (_, rest) = split_doc(heap, rest);
        let params = parse_macro_lambda_list(heap, &heap.list_to_vec(*params)?)?;
        let shape = MacroShape::of(&MacroDef {
            required: params.0.len(),
            optional: params.1.len(),
            rest: params.2.is_some(),
            keys: params.3.iter().map(|(n, _)| n.name().to_string()).collect(),
            public: false,
            builtin: false,
        });
        // A leading space is unwritable in source, and the number keeps two
        // bindings of one name apart.
        let path = Path::root(&format!(" macrolet {} {}", cx.reg.next_local_macro(), name.name()));
        let site = MacroSite { reg: cx.reg, expander: cx.expander, ns: cx.ns, notes: cx.notes() };
        let mut requests = Requests::default();
        let form = macro_form(&site, heap, &mut requests, &path, params, rest)?;
        let form = super::toplevel::bundled(heap, cx.reg, &mut requests, form)?;
        // The callback entries and `~/name/` targets it reaches are what an
        // executable registers at startup; a macro runs while the program is
        // checked, where they are made as they are first used, and is no
        // part of an executable.
        drop(requests.take_reached());
        cx.expander.install(heap, form).map_err(|e| Error::TypeError(format!("macrolet `{}`: {}", name.name(), e)))?;
        macros.push((name, path, shape));
    }
    let inner = locals.bind_local_macros(macros)?;
    let (body, ty) = check_body(cx, heap, &inner, body, expected)?;
    Ok(Typed { node: Node::Let { binds: Vec::new(), body }, ty, loc })
}
