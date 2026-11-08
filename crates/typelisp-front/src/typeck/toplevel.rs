//! Checking one top-level form: the rebuilt checker's entry point.

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};

use super::expr::check_expr;
use super::lower::lower;

/// The rebuilt checker, as the current one hands it each top-level form
/// while the switch `TYPELISP_TYPECK` is on (`docs/dev/checker-rebuild-plan.md` 6).
#[derive(Debug, Default)]
pub(crate) struct TypeChecker {
    /// The type of the last form, when it was an expression.
    expr_ty: Option<Type>,
}

impl TypeChecker {
    /// The rebuilt checker when the switch is on, `None` otherwise.
    pub(crate) fn from_switch() -> Option<TypeChecker> {
        std::env::var_os("TYPELISP_TYPECK").is_some_and(|v| !v.is_empty()).then(TypeChecker::default)
    }

    /// `v`'s core form, unrooted.
    pub(crate) fn check_form(&mut self, heap: &mut Heap, v: Value, loc: Option<Loc>) -> Result<Value, Error> {
        self.expr_ty = None;
        // Every value the tree holds is rooted from here until it is lowered
        // into the form, which is then reachable from nothing but the caller.
        // Nothing allocates between releasing them and the caller rooting it.
        let roots_base = heap.root_count();
        let result = check_toplevel_expr(heap, v, loc);
        heap.truncate_roots(roots_base);
        let (form, ty) = result?;
        self.expr_ty = Some(ty);
        Ok(form)
    }

    /// The type of the last form [`Self::check_form`] checked, when it was an
    /// expression.
    pub(crate) fn expr_type(&self) -> Option<&Type> {
        self.expr_ty.as_ref()
    }
}

/// A top-level expression `v`: `(expr FORM)` and `v`'s type.
fn check_toplevel_expr(heap: &mut Heap, v: Value, loc: Option<Loc>) -> Result<(Value, Type), Error> {
    let typed = check_expr(heap, v, None, loc).map_err(|e| match heap.cons_loc(v) {
        Some(l) => e.at(l),
        None => e,
    })?;
    let form = lower(heap, &typed)?;
    heap.push_root(form);
    let form = crate::check::core::tagged(heap, wk::EXPR, &[form])?;
    Ok((form, typed.ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Interp, Reader};

    /// `src`'s one form, read, checked by the rebuilt checker and run: its
    /// value and type.
    fn run(src: &str) -> (Heap, Value, Type) {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let mut forms = reader.forms(src);
        let (v, loc) = forms.next_form(&mut heap).expect("read failed").expect("one form");
        heap.push_root(v);
        let mut checker = TypeChecker::default();
        let form = checker.check_form(&mut heap, v, Some(loc)).expect("check failed");
        heap.push_root(form);
        let value = Interp::new().exec(&mut heap, form).expect("run failed").expect("an expression has a value");
        let ty = checker.expr_type().expect("an expression has a type").clone();
        (heap, value, ty)
    }

    #[test]
    fn integer_literals() {
        let (_, v, ty) = run("42");
        assert_eq!((v, ty), (Value::Int(42), Type::Int));
        let (heap, v, ty) = run("123456789012345678901234567890");
        assert_eq!(ty, Type::Int);
        let Value::Boxed(id) = v else { panic!("a bignum is boxed: {:?}", v) };
        assert_eq!(heap.bignum_value(id).to_string(), "123456789012345678901234567890");
    }

    #[test]
    fn other_literals() {
        assert_eq!(run("true").1, Value::Bool(true));
        assert_eq!(run("#\\a").1, Value::Char('a'));
        assert_eq!(run("()").2, Type::Unit);
        assert_eq!(run(":key").2, Type::Symbol);
        let (heap, v, ty) = run("2.5");
        let Value::Boxed(id) = v else { panic!("a float is boxed: {:?}", v) };
        assert_eq!((heap.f64_value(id), ty), (2.5, Type::F64));
        let (heap, v, ty) = run("\"abc\"");
        let Value::Str(id) = v else { panic!("a string: {:?}", v) };
        assert_eq!((heap.string(id).to_string(), ty), ("abc".to_string(), Type::Str));
        assert_eq!(run("1/3").2, Type::Ratio);
    }
}
