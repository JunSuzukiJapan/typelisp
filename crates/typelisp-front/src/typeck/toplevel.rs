//! Checking one top-level form: the rebuilt checker's entry point.

use crate::check::lower::CoreForms;
use crate::check::MONO_BUNDLE_MODULE;
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::NsId;

use super::defun::{check_defun, lower_all};
use super::expr::{check_expr, Cx};
use super::generic::{Request, Requests};
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::Registry;
use super::subst::Subst;

/// The rebuilt checker, as the current one hands it each top-level form
/// while the switch `TYPELISP_TYPECK` is on (`docs/dev/checker-rebuild-plan.md` 6).
#[derive(Debug, Default)]
pub(crate) struct TypeChecker {
    reg: Registry,
    /// Whether the current checker's registry has been copied in yet.
    imported: bool,
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
    ///
    /// `old` is the current checker's registry, which holds the built-in
    /// definitions and the prelude; it is copied in before the first form
    /// (`docs/dev/checker-rebuild-plan.md` 8.3).
    pub(crate) fn check_form(&mut self, heap: &mut Heap, old: &crate::check::Registry, ns: NsId, v: Value, loc: Option<Loc>) -> Result<Value, Error> {
        if !self.imported {
            super::bridge::import(old, &mut self.reg);
            self.reg.commit();
            self.imported = true;
        }
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

    /// `v`'s core form, and its type when it is an expression. The
    /// specializations the form asks for come first, in one bundle with it.
    fn check_toplevel(&mut self, heap: &mut Heap, ns: NsId, v: Value, loc: Option<Loc>) -> Result<(Value, Option<Type>), Error> {
        let mut requests = Requests::default();
        let (form, ty) = self.check_primary(heap, ns, v, loc, &mut requests)?;
        heap.push_root(form);
        let mut forms = Vec::new();
        while let Some(req) = requests.next()? {
            let spec = specialize(heap, &self.reg, &mut requests, &req)?;
            heap.push_root(spec);
            forms.push(spec);
        }
        if forms.is_empty() {
            return Ok((form, ty));
        }
        forms.push(form);
        Ok((crate::check::forms::module_form(heap, &Path::root(MONO_BUNDLE_MODULE), &forms)?, ty))
    }

    fn check_primary(
        &mut self,
        heap: &mut Heap,
        ns: NsId,
        v: Value,
        loc: Option<Loc>,
        requests: &mut Requests,
    ) -> Result<(Value, Option<Type>), Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec_locs(v)?;
            if let Value::Symbol(head) = elems[0].0 {
                if head.is(wk::DEFUN) {
                    return Ok((check_defun(&mut self.reg, ns, heap, requests, &elems[1..])?, None));
                }
            }
        }
        let mut typed = {
            let cx = Cx::new(&self.reg, ns);
            let mut typed = check_expr(&cx, heap, &Locals::empty(), v, None, loc)?;
            typed.map_types(&|t| cx.resolve(t));
            typed
        };
        let subst = Subst::default();
        let mut lw = Lowering { reg: &self.reg, subst: &subst, requests };
        let form = lower_rooted(heap, &mut lw, &typed)?;
        let form = crate::check::core::tagged(heap, wk::EXPR, &[form])?;
        let ty = std::mem::replace(&mut typed.ty, Type::Unit);
        Ok((form, Some(ty)))
    }
}

/// The generic function `req.base` specialized at `req.args`: its body lowered
/// with the type parameters replaced, under the specialization's name.
fn specialize(heap: &mut Heap, reg: &Registry, requests: &mut Requests, req: &Request) -> Result<Value, Error> {
    let def = reg.fn_def(&req.base).expect("a specialization is asked for only of a defined function");
    // The prelude's generic functions were checked by the current checker,
    // which keeps their source, not a checked tree.
    let Some(template) = reg.template(&req.base) else {
        unimplemented!("typeck: specializing {}, a generic function this checker has no checked body of", req.base)
    };
    let subst = Subst::positional(&def.type_params, &req.args);
    let params: Vec<_> = template.params.iter().map(|(n, t)| (*n, subst.apply(t))).collect();
    let ret = subst.apply(&template.ret);
    let mut lw = Lowering { reg, subst: &subst, requests };
    let body = lower_all(heap, &mut lw, &template.body)?;
    reg.defun_form(heap, &req.name, &params, &ret, &body)
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
            let form = checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), NsId::ROOT, v, Some(loc)).expect("check failed");
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
            result = checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), NsId::ROOT, v, Some(loc));
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

    #[test]
    fn a_generic_function_is_specialized_for_each_instantiation() {
        let src = "(defun id<T> ((x T)) T x)\n(defun pick<T> ((c bool) (a T) (b T)) T (if c (id a) b))\n(pick false (id 1) 2)";
        assert_eq!(run(src).1, Value::Int(2));
        let src = "(defun id<T> ((x T)) T x)\n(id true)";
        assert_eq!(run(src).2, Type::Bool);
    }

    /// The names a generic function's parameters are declared with do not
    /// matter to a caller that declares the same names.
    #[test]
    fn the_same_names_in_caller_and_callee() {
        let src = "(defun id<T> ((x T)) T x)\n(defun wrap<T> ((y T)) T (id y))\n(wrap #\\z)";
        assert_eq!(run(src).1, Value::Char('z'));
    }

    #[test]
    fn a_generic_function_calls_itself() {
        let src = "(defun again<T> ((x T) (stop bool)) T (if stop x (again x true)))\n(again 9 false)";
        assert_eq!(run(src).1, Value::Int(9));
    }

    /// The body is checked once, with `T` abstract: a `T` is not an `int`.
    #[test]
    fn a_type_parameter_is_no_particular_type() {
        let msg = check_err("(defun f<T> ((x T)) int x)");
        assert!(msg.contains("expected `int`, found `T`"), "{}", msg);
        let msg = check_err("(defun f<T> ((x T)) T 1)");
        assert!(msg.contains("expected `T`, found `int`"), "{}", msg);
    }

    /// A parameter's type decides the argument's even through a hole: the
    /// first argument makes `T` a `u8`, and the second must be one.
    #[test]
    fn one_argument_decides_anothers_type() {
        let src = "(defun id8 ((x u8)) u8 x)\n(defun same<T> ((a T) (b T)) T b)\n(same (id8 1) 300)";
        let msg = check_err(src);
        assert!(msg.contains("integer literal 300 is out of range for u8"), "{}", msg);
    }

    #[test]
    fn a_type_argument_nothing_decides() {
        let msg = check_err("(defun mk<T> () int 1)\n(mk)");
        assert!(msg.contains("cannot infer type parameter `T`"), "{}", msg);
    }

    #[test]
    fn a_type_parameter_declared_twice() {
        let msg = check_err("(defun f<T, T> ((x T)) T x)");
        assert!(msg.contains("type parameter `T` is declared twice"), "{}", msg);
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
        assert!(checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), NsId::ROOT, v, Some(loc)).is_err());
        let f = crate::types::Path::of_symbol(NsId::ROOT, typelisp_mem::symbols::intern_in(NsId::ROOT, "f"));
        assert!(checker.reg.fn_def(&f).is_none());
    }
}
