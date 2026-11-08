//! Checking one top-level form: the rebuilt checker's entry point.

use crate::types::Type;
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::defun::check_defun;
use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::lower::lower;
use super::registry::Registry;

/// The rebuilt checker, as the current one hands it each top-level form
/// while the switch `TYPELISP_TYPECK` is on (`docs/dev/checker-rebuild-plan.md` 6).
#[derive(Debug, Default)]
pub(crate) struct TypeChecker {
    reg: Registry,
    /// The type of the last form, when it was an expression.
    expr_ty: Option<Type>,
}

impl TypeChecker {
    /// The rebuilt checker when the switch is on, `None` otherwise.
    pub(crate) fn from_switch() -> Option<TypeChecker> {
        std::env::var_os("TYPELISP_TYPECK").is_some_and(|v| !v.is_empty()).then(TypeChecker::default)
    }

    /// `v`, a form written in the module `ns`, checked: its core form,
    /// unrooted.
    ///
    /// What the form defines is kept only if checking it succeeds.
    pub(crate) fn check_form(&mut self, heap: &mut Heap, ns: NsId, v: Value, loc: Option<Loc>) -> Result<Value, Error> {
        self.expr_ty = None;
        // Every value the tree holds is rooted from here until it is lowered
        // into the form, which is then reachable from nothing but the caller.
        // Nothing allocates between releasing them and the caller rooting it.
        let roots_base = heap.root_count();
        let result = self.check_toplevel(heap, ns, v, loc.clone());
        heap.truncate_roots(roots_base);
        match result {
            Ok((form, ty)) => {
                self.reg.commit();
                self.expr_ty = ty;
                Ok(form)
            }
            Err(e) => {
                self.reg.rollback();
                Err(match loc {
                    Some(l) => e.at(l),
                    None => e,
                })
            }
        }
    }

    /// The type of the last form [`Self::check_form`] checked, when it was an
    /// expression.
    pub(crate) fn expr_type(&self) -> Option<&Type> {
        self.expr_ty.as_ref()
    }

    /// `v`'s core form, and its type when it is an expression.
    fn check_toplevel(&mut self, heap: &mut Heap, ns: NsId, v: Value, loc: Option<Loc>) -> Result<(Value, Option<Type>), Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec_locs(v)?;
            if let Value::Symbol(head) = elems[0].0 {
                if head.is(wk::DEFUN) {
                    return Ok((check_defun(&mut self.reg, ns, heap, &elems[1..])?, None));
                }
            }
        }
        let cx = Cx { reg: &self.reg, ns };
        let typed = check_expr(&cx, heap, &Locals::empty(), v, None, loc)?;
        let form = lower(heap, &self.reg, &typed)?;
        heap.push_root(form);
        let form = crate::check::core::tagged(heap, wk::EXPR, &[form])?;
        Ok((form, Some(typed.ty)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Interp, Reader};

    /// `src`'s forms, read, checked by the rebuilt checker and run in order:
    /// the last one's value and type.
    fn run(src: &str) -> (Heap, Value, Type) {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        let mut forms = reader.forms(src);
        let mut last = None;
        while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
            heap.push_root(v);
            let form = checker.check_form(&mut heap, NsId::ROOT, v, Some(loc)).expect("check failed");
            heap.push_root(form);
            if let Some(value) = interp.exec(&mut heap, form).expect("run failed") {
                last = Some((value, checker.expr_type().expect("an expression has a type").clone()));
            }
        }
        let (value, ty) = last.expect("the last form is an expression");
        (heap, value, ty)
    }

    /// `src`'s forms checked in order, the last expected to fail: its message.
    fn check_err(src: &str) -> String {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let mut checker = TypeChecker::default();
        let mut forms = reader.forms(src);
        let mut result = Ok(Value::Empty);
        while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
            heap.push_root(v);
            result = checker.check_form(&mut heap, NsId::ROOT, v, Some(loc));
        }
        match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected the last form to fail"),
        }
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

    #[test]
    fn a_function_with_let_and_if() {
        let src = "(defun pick ((c bool) (a int) (b int)) int (let ((x a)) (if c x b)))\n(pick false 1 2)";
        assert_eq!(run(src).1, Value::Int(2));
    }

    /// A parameter's type decides an integer literal's: `u8` here.
    #[test]
    fn an_argument_is_checked_against_its_parameter() {
        assert_eq!(run("(defun id8 ((x u8)) u8 x)\n(id8 200)").2, Type::U8);
        let msg = check_err("(defun id8 ((x u8)) u8 x)\n(id8 300)");
        assert!(msg.contains("integer literal 300 is out of range for u8"), "{}", msg);
    }

    #[test]
    fn a_function_calls_itself() {
        let src = "(defun last ((n int) (stop bool)) int (if stop n (last 7 true)))\n(last 1 false)";
        assert_eq!(run(src).1, Value::Int(7));
    }

    /// An error is reported where the form that is wrong was written.
    #[test]
    fn an_error_is_where_it_was_written() {
        let msg = check_err("(defun f () int true)");
        assert!(msg.contains(":1:17:"), "{}", msg);
    }

    #[test]
    fn mismatches() {
        let msg = check_err("(defun f () int true)");
        assert!(msg.contains("expected `int`, found `bool`"), "{}", msg);
        let msg = check_err("(if 1 2 3)");
        assert!(msg.contains("expected `bool`, found `int`"), "{}", msg);
        let msg = check_err("(if true 2 \"x\")");
        assert!(msg.contains("expected `int`, found `string`"), "{}", msg);
        let msg = check_err("(defun f ((a int)) int a)\n(f)");
        assert!(msg.contains("expected 1 argument(s), got 0"), "{}", msg);
    }

    /// A definition whose checking fails is not kept (plan 4.4): calling it
    /// afterwards is not a call to anything this checker defined.
    #[test]
    fn a_failed_definition_is_not_kept() {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let mut checker = TypeChecker::default();
        let mut forms = reader.forms("(defun f () int true)");
        let (v, loc) = forms.next_form(&mut heap).unwrap().unwrap();
        heap.push_root(v);
        assert!(checker.check_form(&mut heap, NsId::ROOT, v, Some(loc)).is_err());
        let f = crate::types::Path::of_symbol(NsId::ROOT, typelisp_mem::symbols::intern_in(NsId::ROOT, "f"));
        assert_eq!(checker.reg.fn_def(&f), None);
    }
}
