//! A function handed to C as a callback: an argument of a `defffi` function
//! whose parameter's declared type is `(fn (T...) R)`.
//!
//! C holds nothing but a code address and calls it with exactly the declared
//! arguments, so the function it is given can have no environment: there is
//! nowhere for C to pass one. Three spellings of such a function are
//! accepted, all written directly at the argument:
//!
//! * the name of a top-level function,
//! * a `lambda` that names no local around it,
//! * the name of a `labels` function that, with every sibling it reaches,
//!   names no local around the block: siblings share one environment, so a
//!   sibling's capture is the function's too.
//!
//! A variable holding a function is refused: which function it holds, and so
//! whether it has an environment, is not known until it runs.
//!
//! The last two are lifted to a top-level function of their own when the
//! tree is lowered, placed ahead of the form in its bundle, so the entry C
//! calls always runs a top-level function.

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};

use super::expr::{check_expr, conform_at, Cx};
use super::lambda::check_callback_lambda;
use super::locals::{LabelsCallback, Locals};
use super::tree::{Node, Typed};

/// The argument `arg` to the parameter, of the function type `fn_ty`, of
/// the C function `callee`: the address of the entry C is handed.
pub(crate) fn check_callback_arg(cx: &Cx, heap: &mut Heap, locals: &Locals, (arg, loc): &(Value, Option<Loc>), fn_ty: &Type, callee: &Path) -> Result<Typed, Error> {
    let callback = |target: Path, lifted: Option<Box<Typed>>| Typed { node: Node::Callback { target, fn_ty: fn_ty.clone(), lifted }, ty: Type::Ptr, loc: loc.clone() };
    match *arg {
        Value::Symbol(name) if locals.lookup(name)?.is_some() => {
            let ty = locals.lookup(name)?.expect("checked above").clone();
            let Some(pending) = locals.labels_callbacks(name) else {
                return Err(Error::TypeError(format!(
                    "{}: `{}` is a variable, and which function a variable holds is not known until \
                     the program runs — so neither is whether it has local variables C could not \
                     give it. Pass the function's own name, or a `lambda`.",
                    callee,
                    name.name()
                )));
            };
            conform_at(cx, Typed { node: Node::Local { name }, ty, loc: loc.clone() }, fn_ty)?;
            let lifted = fresh_path(cx);
            pending.borrow_mut().push(LabelsCallback { name, lifted, loc: loc.clone() });
            Ok(callback(lifted, None))
        }
        Value::Symbol(_) => {
            let checked = check_expr(cx, heap, locals, *arg, Some(fn_ty), loc.clone())?;
            match checked.node {
                Node::FnRef { path, .. } if cx.reg.fn_def(&path).is_some_and(|d| d.builtin) => Err(Error::TypeError(format!(
                    "{}: `{}` is a builtin, and a builtin has no body of its own to hand C. \
                     Pass a `lambda` that calls it instead.",
                    callee, path
                ))),
                Node::FnRef { path, ref targs, .. } if !targs.is_empty() => {
                    unimplemented!("typeck: handing C the generic function `{}` at its type arguments", path)
                }
                Node::FnRef { path, .. } => Ok(callback(path, None)),
                _ => Err(written_here(callee)),
            }
        }
        Value::Cons(_) if matches!(heap.car(*arg), Ok(Value::Symbol(h)) if h.is(wk::LAMBDA)) => {
            let elems = heap.list_to_vec_locs(*arg)?;
            let lambda = check_callback_lambda(cx, heap, locals, &elems[1..], loc.clone())?;
            let lambda = conform_at(cx, lambda, fn_ty)?;
            Ok(callback(fresh_path(cx), Some(Box::new(lambda))))
        }
        _ => Err(written_here(callee)),
    }
}

fn written_here(callee: &Path) -> Error {
    Error::TypeError(format!(
        "{}: a C callback must be written where it is passed — the name of a function, \
         or a `lambda` — so that it can be checked for free variables here",
        callee
    ))
}

/// A path in the form's module, for a lifted function, that names nothing
/// yet.
fn fresh_path(cx: &Cx) -> Path {
    loop {
        let path = Path::in_ns(cx.ns, &format!("callback${}", cx.reg.next_callback()));
        if cx.reg.values.get(&path).is_none() {
            return path;
        }
    }
}
