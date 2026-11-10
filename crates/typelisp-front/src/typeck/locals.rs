//! Local variables: what a name bound by a parameter list or a `let` means,
//! and the symbol macros `symbol-macrolet` binds the same way.
//!
//! The frames form a chain in which each frame refers to its parent and
//! neither is copied or changed, so entering a `let` costs only its own
//! bindings however deep it is nested.

use std::cell::RefCell;

use crate::check::analyze::MacroShape;
use crate::types::{Path, Type};
use crate::{Error, Loc, Value};
use typelisp_mem::SymRef;

/// One frame of local bindings, and the frames around it.
#[derive(Debug)]
pub(crate) struct Locals<'p> {
    parent: Option<&'p Locals<'p>>,
    binds: Vec<(SymRef, Type)>,
    /// Names that stand for a form, written where the name is
    /// (`symbol-macrolet`).
    macros: Vec<(SymRef, Value)>,
    /// Names of macros, which the evaluator holds under the path
    /// (`macrolet`).
    local_macros: Vec<(SymRef, Path, MacroShape)>,
    /// Set on a function's parameters: the frames around it are what the
    /// function captures.
    fence: Option<Fence>,
    /// Set on a `labels` block's names: the callbacks C is to be handed of
    /// its functions, lifted once every body of the block is checked.
    callbacks: Option<RefCell<Vec<LabelsCallback>>>,
}

/// Where a function's own frames end and those it captures begin.
#[derive(Debug)]
struct Fence {
    /// The function, as a message names it ("a `lambda`").
    what: String,
    /// Whether the function is handed to C, which can give it nothing but
    /// its arguments: then it may capture nothing at all.
    callback: bool,
    /// The names the function's body reached across the fence.
    crossed: RefCell<Vec<SymRef>>,
}

/// A `labels` function passed to C: the function, the path its lifted copy
/// is given, and where it was passed.
#[derive(Debug)]
pub(crate) struct LabelsCallback {
    pub(crate) name: SymRef,
    pub(crate) lifted: Path,
    pub(crate) loc: Option<Loc>,
}

impl Locals<'static> {
    /// Where no local is bound: a top-level expression.
    pub(crate) fn empty() -> Locals<'static> {
        Locals { parent: None, binds: Vec::new(), macros: Vec::new(), local_macros: Vec::new(), fence: None, callbacks: None }
    }
}

impl<'p> Locals<'p> {
    /// A frame inside `self` binding `binds`; a later binding of a name in
    /// the same frame hides an earlier one. Every local binding is made
    /// here, which makes this the one place a reserved word is refused.
    pub(crate) fn bind<'s>(&'s self, binds: Vec<(SymRef, Type)>) -> Result<Locals<'s>, Error> {
        if let Some((name, _)) = binds.iter().find(|(n, _)| is_reserved_name(*n)) {
            return Err(Error::TypeError(format!("`{}` is a reserved word and cannot be bound", name.name())));
        }
        Ok(Locals { parent: Some(self), binds, macros: Vec::new(), local_macros: Vec::new(), fence: None, callbacks: None })
    }

    /// The parameters `binds` of the function `what` ("a `lambda`"), whose
    /// body is checked inside `self`: a local of `self` its body names is
    /// one the function captures. `callback`: the function is handed to C,
    /// and may capture nothing.
    pub(crate) fn bind_function<'s>(&'s self, binds: Vec<(SymRef, Type)>, what: String, callback: bool) -> Result<Locals<'s>, Error> {
        let mut frame = self.bind(binds)?;
        frame.fence = Some(Fence { what, callback, crossed: RefCell::new(Vec::new()) });
        Ok(frame)
    }

    /// The names of a `labels` block, `binds`, bound inside `self`.
    pub(crate) fn bind_labels<'s>(&'s self, binds: Vec<(SymRef, Type)>) -> Result<Locals<'s>, Error> {
        let mut frame = self.bind(binds)?;
        frame.callbacks = Some(RefCell::new(Vec::new()));
        Ok(frame)
    }

    /// The names this frame's function reached outside its own frames, in
    /// the order first reached; none for a frame [`Self::bind_function`]
    /// did not make.
    pub(crate) fn captured(&self) -> Vec<SymRef> {
        self.fence.as_ref().map(|f| f.crossed.borrow().clone()).unwrap_or_default()
    }

    /// Where a callback C is handed of the local `name` is noted, when
    /// `name` is a function of a `labels` block here; `None` when it is a
    /// variable.
    pub(crate) fn labels_callbacks(&self, name: SymRef) -> Option<&RefCell<Vec<LabelsCallback>>> {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if f.binds.iter().any(|(n, _)| *n == name) {
                return f.callbacks.as_ref();
            }
            frame = f.parent;
        }
        None
    }

    /// The callbacks noted of this block's functions.
    pub(crate) fn take_labels_callbacks(&self) -> Vec<LabelsCallback> {
        self.callbacks.as_ref().map(|c| c.take()).unwrap_or_default()
    }

    /// A frame inside `self` in which each name of `macros` stands for its
    /// form. The forms are the program's, rooted with it.
    pub(crate) fn bind_macros<'s>(&'s self, macros: Vec<(SymRef, Value)>) -> Result<Locals<'s>, Error> {
        if let Some((name, _)) = macros.iter().find(|(n, _)| is_reserved_name(*n)) {
            return Err(Error::TypeError(format!("`{}` is a reserved word and cannot be bound", name.name())));
        }
        Ok(Locals { parent: Some(self), binds: Vec::new(), macros, local_macros: Vec::new(), fence: None, callbacks: None })
    }

    /// A frame inside `self` in which each name of `macros` is the macro the
    /// evaluator holds under its path.
    pub(crate) fn bind_local_macros<'s>(&'s self, macros: Vec<(SymRef, Path, MacroShape)>) -> Result<Locals<'s>, Error> {
        if let Some((name, _, _)) = macros.iter().find(|(n, _, _)| is_reserved_name(*n)) {
            return Err(Error::TypeError(format!("`{}` is a reserved word and cannot be bound", name.name())));
        }
        Ok(Locals { parent: Some(self), binds: Vec::new(), macros: Vec::new(), local_macros: macros, fence: None, callbacks: None })
    }

    /// The macro `macrolet` bound to `name` here, when the nearest binding
    /// of `name` is one: its path and how many arguments it takes.
    pub(crate) fn local_macro(&self, name: SymRef) -> Option<(Path, MacroShape)> {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if f.binds.iter().any(|(n, _)| *n == name) || f.macros.iter().any(|(n, _)| *n == name) {
                return None;
            }
            if let Some((_, path, shape)) = f.local_macros.iter().rev().find(|(n, _, _)| *n == name) {
                return Some((*path, *shape));
            }
            frame = f.parent;
        }
        None
    }

    /// The form the symbol macro `name` stands for here, when the nearest
    /// binding of `name` is one.
    pub(crate) fn symbol_macro(&self, name: SymRef) -> Option<Value> {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if f.binds.iter().any(|(n, _)| *n == name) {
                return None;
            }
            if let Some((_, form)) = f.macros.iter().rev().find(|(n, _)| *n == name) {
                return Some(*form);
            }
            frame = f.parent;
        }
        None
    }

    /// The type of the local variable `name` here, if it is one. A capture
    /// is refused when the capturing function is handed to C, and when it
    /// is of a typed pointer: the function can be called after the `unsafe`
    /// that allocated the memory is left.
    pub(crate) fn lookup(&self, name: SymRef) -> Result<Option<&Type>, Error> {
        let mut frame = Some(self);
        let mut crossed: Vec<&Fence> = Vec::new();
        while let Some(f) = frame {
            if let Some((_, t)) = f.binds.iter().rev().find(|(n, _)| *n == name) {
                for fence in &crossed {
                    if fence.callback {
                        return Err(Error::TypeError(format!(
                            "{} passed to C as a callback cannot refer to local variables of the \
                             code around it — C calls it with nothing but its arguments, so there is \
                             nowhere to keep them. This one refers to `{}`.",
                            fence.what,
                            name.name()
                        )));
                    }
                    if super::c_memory::mentions_typed_ptr(t) {
                        return Err(Error::TypeError(format!(
                            "{} captures `{}`, a `{}`: the closure can run after the `unsafe` that \
                             allocated the memory is left and has freed it. Pass it as an argument instead.",
                            fence.what,
                            name.name(),
                            t
                        )));
                    }
                    let mut names = fence.crossed.borrow_mut();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                return Ok(Some(t));
            }
            // A symbol macro hides a variable around it.
            if f.macros.iter().any(|(n, _)| *n == name) {
                return Ok(None);
            }
            crossed.extend(f.fence.as_ref());
            frame = f.parent;
        }
        Ok(None)
    }
}

/// Names no binding may take.
///
/// `task` is always a special form in head position, so a variable called
/// `task` could be bound but never called; `thread` is its twin. The other
/// special forms may be bound: programs use names like `list` and `format` as
/// ordinary words.
pub(crate) fn is_reserved_name(name: SymRef) -> bool {
    name.is(typelisp_mem::wk::TASK) || name.is(typelisp_mem::wk::THREAD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typelisp_mem::symbols::intern;

    #[test]
    fn an_inner_binding_hides_an_outer_one() {
        let root = Locals::empty();
        let outer = root.bind(vec![(intern("x"), Type::Int), (intern("y"), Type::Bool)]).unwrap();
        let inner = outer.bind(vec![(intern("x"), Type::Str)]).unwrap();
        assert_eq!(inner.lookup(intern("x")).unwrap(), Some(&Type::Str));
        assert_eq!(inner.lookup(intern("y")).unwrap(), Some(&Type::Bool));
        assert_eq!(outer.lookup(intern("x")).unwrap(), Some(&Type::Int));
        assert_eq!(root.lookup(intern("x")).unwrap(), None);
    }
}
