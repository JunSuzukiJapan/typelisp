//! Local variables: what a name bound by a parameter list or a `let` means,
//! and the symbol macros `symbol-macrolet` binds the same way.
//!
//! The frames form a chain in which each frame refers to its parent and
//! neither is copied or changed, so entering a `let` costs only its own
//! bindings however deep it is nested.

use crate::types::Type;
use crate::{Error, Value};
use typelisp_mem::SymRef;

/// One frame of local bindings, and the frames around it.
#[derive(Debug)]
pub(crate) struct Locals<'p> {
    parent: Option<&'p Locals<'p>>,
    binds: Vec<(SymRef, Type)>,
    /// Names that stand for a form, written where the name is
    /// (`symbol-macrolet`).
    macros: Vec<(SymRef, Value)>,
}

impl Locals<'static> {
    /// Where no local is bound: a top-level expression.
    pub(crate) fn empty() -> Locals<'static> {
        Locals { parent: None, binds: Vec::new(), macros: Vec::new() }
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
        Ok(Locals { parent: Some(self), binds, macros: Vec::new() })
    }

    /// A frame inside `self` in which each name of `macros` stands for its
    /// form. The forms are the program's, rooted with it.
    pub(crate) fn bind_macros<'s>(&'s self, macros: Vec<(SymRef, Value)>) -> Result<Locals<'s>, Error> {
        if let Some((name, _)) = macros.iter().find(|(n, _)| is_reserved_name(*n)) {
            return Err(Error::TypeError(format!("`{}` is a reserved word and cannot be bound", name.name())));
        }
        Ok(Locals { parent: Some(self), binds: Vec::new(), macros })
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

    /// Whether the type of some local here is one `pred` holds of.
    pub(crate) fn any(&self, pred: impl Fn(&Type) -> bool) -> bool {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if f.binds.iter().any(|(_, t)| pred(t)) {
                return true;
            }
            frame = f.parent;
        }
        false
    }

    /// The type of the local variable `name` here, if it is one.
    pub(crate) fn lookup(&self, name: SymRef) -> Option<&Type> {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if let Some((_, t)) = f.binds.iter().rev().find(|(n, _)| *n == name) {
                return Some(t);
            }
            // A symbol macro hides a variable around it.
            if f.macros.iter().any(|(n, _)| *n == name) {
                return None;
            }
            frame = f.parent;
        }
        None
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
        assert_eq!(inner.lookup(intern("x")), Some(&Type::Str));
        assert_eq!(inner.lookup(intern("y")), Some(&Type::Bool));
        assert_eq!(outer.lookup(intern("x")), Some(&Type::Int));
        assert_eq!(root.lookup(intern("x")), None);
    }
}
