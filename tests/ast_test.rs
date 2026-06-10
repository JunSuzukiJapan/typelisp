extern crate typelisp;

use typelisp::*;

mod tests {
    use crate::*;

    fn form(src: &str) -> Result<Form, Error> {
        let reader = Reader::new();
        let c = ASTConstructor::new();
        c.make_form(&reader.read(src)?)
    }

    fn expr(src: &str) -> Result<TypedExpr, Error> {
        let reader = Reader::new();
        let c = ASTConstructor::new();
        c.make_expr(&reader.read(src)?)
    }

    // ---- defun ----------------------------------------------------------

    #[test]
    fn defun_factorial() -> Result<(), Error> {
        let f = form("(defun factorial ((n i32)) i32 (if (<= n 1) 1 (* n (factorial (- n 1)))))")?;
        match f {
            Form::Defun(d) => {
                assert_eq!(d.name, "factorial");
                assert_eq!(d.params, vec![("n".to_string(), Type::I32)]);
                assert_eq!(d.ret, Type::I32);
                assert_eq!(d.body.len(), 1);
                assert!(!d.pub_);
            }
            other => panic!("expected Defun, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn defun_pub_and_unit() -> Result<(), Error> {
        let f = form("(pub defun main () () (foo))")?;
        match f {
            Form::Defun(d) => {
                assert!(d.pub_);
                assert_eq!(d.params, vec![]);
                assert_eq!(d.ret, Type::Unit);
            }
            other => panic!("expected Defun, got {:?}", other),
        }
        Ok(())
    }

    // ---- defstruct ------------------------------------------------------

    #[test]
    fn defstruct_groups_and_generics() -> Result<(), Error> {
        let f = form("(defstruct Point (pub ((x f64) (y f64))) (((label String))))")?;
        match f {
            Form::DefStruct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.pub_fields, vec![("x".into(), Type::F64), ("y".into(), Type::F64)]);
                assert_eq!(s.priv_fields, vec![("label".into(), Type::Str)]);
            }
            other => panic!("expected DefStruct, got {:?}", other),
        }

        let f = form("(defstruct Pair<K,V> (pub ((key K) (value V))))")?;
        match f {
            Form::DefStruct(s) => {
                assert_eq!(s.name, "Pair");
                assert_eq!(s.generics, vec!["K".to_string(), "V".to_string()]);
                assert_eq!(
                    s.pub_fields,
                    vec![
                        ("key".into(), Type::Named("K".into(), vec![])),
                        ("value".into(), Type::Named("V".into(), vec![])),
                    ]
                );
            }
            other => panic!("expected DefStruct, got {:?}", other),
        }
        Ok(())
    }

    // ---- defmethod (receiver dispatch) ----------------------------------

    #[test]
    fn defmethod_instance() -> Result<(), Error> {
        let f = form("(defmethod area ((self Circle)) f64 (* 3.0 (.radius self)))")?;
        match f {
            Form::DefMethod(m) => {
                assert_eq!(m.name, "area");
                assert_eq!(m.recv, Receiver::Instance(Type::Named("Circle".into(), vec![])));
            }
            other => panic!("expected DefMethod, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn defmethod_static() -> Result<(), Error> {
        let f = form("(defmethod make ((Circle) (r f64)) Circle (Circle r))")?;
        match f {
            Form::DefMethod(m) => {
                assert_eq!(m.recv, Receiver::Static(Type::Named("Circle".into(), vec![])));
                assert_eq!(m.params, vec![("r".to_string(), Type::F64)]);
            }
            other => panic!("expected DefMethod, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn defmethod_receiver_errors() {
        // 2-element receiver whose name isn't `self`
        assert!(form("(defmethod bad ((x Circle)) f64 0.0)").is_err());
        // `self` alone is not a type
        assert!(form("(defmethod bad ((self)) f64 0.0)").is_err());
    }

    // ---- mandatory-typed vs optional-typed ------------------------------

    #[test]
    fn typed_param_is_mandatory() {
        // a param without a type is an error
        assert!(form("(defun f ((x)) i32 0)").is_err());
    }

    #[test]
    fn let_binding_optional_type() -> Result<(), Error> {
        let e = expr("(let ((x 5) ((y i64) 10) (name \"lisp\")) (+ x y))")?;
        match e.kind {
            ExprKind::Let { bindings, .. } => {
                assert_eq!(bindings[0].name, "x");
                assert_eq!(bindings[0].ty, None); // inferred
                assert_eq!(bindings[1].name, "y");
                assert_eq!(bindings[1].ty, Some(Type::I64)); // explicit
                assert_eq!(bindings[2].ty, None);
            }
            other => panic!("expected Let, got {:?}", other),
        }
        Ok(())
    }

    // ---- lambda ---------------------------------------------------------

    #[test]
    fn lambda_plain_and_move() -> Result<(), Error> {
        match expr("(lambda ((x i32)) (* x x))")?.kind {
            ExprKind::Lambda { move_, params, .. } => {
                assert!(!move_);
                assert_eq!(params, vec![("x".to_string(), Type::I32)]);
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
        match expr("(lambda move ((x i32)) (+ x captured))")?.kind {
            ExprKind::Lambda { move_, .. } => assert!(move_),
            other => panic!("expected Lambda, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn immediately_applied_lambda() -> Result<(), Error> {
        match expr("((lambda ((x i32)) (* x x)) 5)")?.kind {
            ExprKind::Call { target, args } => {
                assert!(matches!(target, CallTarget::Lambda(_)));
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected Call, got {:?}", other),
        }
        Ok(())
    }

    // ---- control flow & operators --------------------------------------

    #[test]
    fn control_forms() -> Result<(), Error> {
        assert!(matches!(expr("(if a b c)")?.kind, ExprKind::If { els: Some(_), .. }));
        assert!(matches!(expr("(if a b)")?.kind, ExprKind::If { els: None, .. }));
        assert!(matches!(expr("(when a b)")?.kind, ExprKind::When { .. }));
        assert!(matches!(expr("(unless a b)")?.kind, ExprKind::Unless { .. }));
        assert!(matches!(expr("(cond ((< n 0) -1) (true 1))")?.kind, ExprKind::Cond { .. }));
        assert!(matches!(expr("(while (< n 10) (incf n))")?.kind, ExprKind::While { .. }));
        assert!(matches!(expr("(loop (break))")?.kind, ExprKind::Loop { .. }));
        assert!(matches!(expr("(dotimes (i 100) (foo i))")?.kind, ExprKind::Dotimes { .. }));
        assert!(matches!(expr("(progn a b c)")?.kind, ExprKind::Progn { .. }));
        Ok(())
    }

    #[test]
    fn operators() -> Result<(), Error> {
        assert!(matches!(expr("(+ 1 2)")?.kind, ExprKind::BinOp { op: BinOp::Add, .. }));
        assert!(matches!(expr("(<= a b)")?.kind, ExprKind::BinOp { op: BinOp::Le, .. }));
        assert!(matches!(expr("(1+ x)")?.kind, ExprKind::UnOp { op: UnOp::Inc1, .. }));
        assert!(matches!(expr("(! x)")?.kind, ExprKind::UnOp { op: UnOp::Not, .. }));
        Ok(())
    }

    #[test]
    fn method_and_field() -> Result<(), Error> {
        assert!(matches!(expr("(.area c)")?.kind, ExprKind::MethodCall { .. }));
        assert!(matches!(expr("(. obj field)")?.kind, ExprKind::FieldAccess { .. }));
        Ok(())
    }

    #[test]
    fn setf_and_call_path() -> Result<(), Error> {
        assert!(matches!(expr("(setf x 5)")?.kind, ExprKind::Setf { .. }));
        match expr("(std::process::exit 0)")?.kind {
            ExprKind::Call { target: CallTarget::Path(p), .. } => {
                assert_eq!(p, vec!["std".to_string(), "process".to_string(), "exit".to_string()]);
            }
            other => panic!("expected path Call, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn match_form() -> Result<(), Error> {
        match expr("(match b (0x20 | 0x09 => (true)) (_ => (false)))")?.kind {
            ExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].patterns, vec![Pattern::Int(0x20), Pattern::Int(0x09)]);
                assert_eq!(arms[1].patterns, vec![Pattern::Wildcard]);
            }
            other => panic!("expected Match, got {:?}", other),
        }
        Ok(())
    }

    // ---- defvar / defconstant ------------------------------------------

    #[test]
    fn defvar_forms() -> Result<(), Error> {
        match form("(defvar x 5)")? {
            Form::DefVar(d) => {
                assert_eq!(d.name, "x");
                assert_eq!(d.ty, None);
                assert!(d.mutable);
            }
            other => panic!("expected DefVar, got {:?}", other),
        }
        match form("(defconstant (n i32) 5)")? {
            Form::DefConstant(d) => {
                assert_eq!(d.ty, Some(Type::I32));
                assert!(!d.mutable);
            }
            other => panic!("expected DefConstant, got {:?}", other),
        }
        Ok(())
    }
}
