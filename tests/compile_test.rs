extern crate typelisp;

use std::error;
use typelisp::*;

mod tests {
    use crate::*;

    #[test]
    fn compile_int_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let obj = reader.read("123")?;
        let expr = constructor.make_expr(&obj)?;
        assert_eq!(expr.kind, ExprKind::Int(123, None));

        let result = Compiler::compile_and_run(&expr)?;
        assert_eq!(result, 123);

        Ok(())
    }

    #[test]
    fn compile_add_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let obj = reader.read("(+ 1 2 3)")?;
        let expr = constructor.make_expr(&obj)?;

        // (+ 1 2 3) folds left into nested BinOp::Add nodes.
        let result = Compiler::compile_and_run(&expr)?;
        assert_eq!(result, 6);

        Ok(())
    }

    #[test]
    fn compile_arith_test() -> Result<(), Box<dyn error::Error>> {
        let reader = Reader::new();
        let constructor = ASTConstructor::new();

        let cases = [("(- 10 3)", 7), ("(* 4 5)", 20), ("(/ 20 4)", 5), ("(% 17 5)", 2)];
        for (src, expected) in cases {
            let obj = reader.read(src)?;
            let expr = constructor.make_expr(&obj)?;
            assert_eq!(Compiler::compile_and_run(&expr)?, expected, "src = {}", src);
        }
        Ok(())
    }

    /// Type-check then JIT-run a whole program, returning the last expr's value.
    fn run(src: &str) -> Result<i64, Box<dyn error::Error>> {
        let objs = Reader::new().read_all(src)?;
        let forms = ASTConstructor::new().make_program(&objs)?;
        let checked = Checker::new().check_program(forms)?;
        Ok(Compiler::run_program(&checked.forms)?)
    }

    #[test]
    fn run_factorial() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1))))) (fact 10)";
        assert_eq!(run(prog)?, 3_628_800);
        Ok(())
    }

    #[test]
    fn run_let_and_if() -> Result<(), Box<dyn error::Error>> {
        assert_eq!(run("(let ((x 5) (y 10)) (+ x y))")?, 15);
        assert_eq!(run("(if (< 1 2) 10 20)")?, 10);
        assert_eq!(run("(if (< 5 2) 10 20)")?, 20);
        Ok(())
    }

    #[test]
    fn run_cond() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defun classify ((n i64)) i64 (cond ((< n 0) -1) ((== n 0) 0) (true 1))) \
                    (classify 7)";
        assert_eq!(run(prog)?, 1);
        Ok(())
    }

    #[test]
    fn run_loops() -> Result<(), Box<dyn error::Error>> {
        // sum 0..10 via while
        let while_prog = "(defun sum ((n i64)) i64 \
                            (let ((acc 0) (i 0)) \
                              (progn (while (< i n) (progn (setf acc (+ acc i)) (incf i))) acc))) \
                          (sum 10)";
        assert_eq!(run(while_prog)?, 45);

        // dotimes accumulation
        let do_prog = "(defun tri ((n i64)) i64 \
                          (let ((acc 0)) (progn (dotimes (i n) (setf acc (+ acc i))) acc))) \
                        (tri 5)";
        assert_eq!(run(do_prog)?, 10);
        Ok(())
    }

    #[test]
    fn run_mutual_recursion() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defun is_even ((n i64)) bool (if (== n 0) true (is_odd (- n 1)))) \
                    (defun is_odd ((n i64)) bool (if (== n 0) false (is_even (- n 1)))) \
                    (if (is_even 10) 1 0)";
        assert_eq!(run(prog)?, 1);
        Ok(())
    }

    #[test]
    fn run_struct_construct_and_field() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Point (pub ((x i64) (y i64)))) \
                    (defun getx ((a i64) (b i64)) i64 (let ((p (Point a b))) (.x p))) \
                    (getx 3 4)";
        assert_eq!(run(prog)?, 3);

        let prog2 = "(defstruct Point (pub ((x i64) (y i64)))) \
                     (defun gety ((a i64) (b i64)) i64 (let ((p (Point a b))) (.y p))) \
                     (gety 3 4)";
        assert_eq!(run(prog2)?, 4);
        Ok(())
    }

    #[test]
    fn run_struct_setf_field() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Point (pub ((x i64) (y i64)))) \
                    (defun f () i64 (let ((p (Point 1 2))) (progn (setf (. p x) 10) (.x p)))) \
                    (f)";
        assert_eq!(run(prog)?, 10);
        Ok(())
    }

    #[test]
    fn run_struct_passed_to_fn() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Point (pub ((x i64) (y i64)))) \
                    (defun sum ((p Point)) i64 (+ (.x p) (.y p))) \
                    (defun mk () i64 (let ((p (Point 20 22))) (sum p))) \
                    (mk)";
        assert_eq!(run(prog)?, 42);
        Ok(())
    }

    #[test]
    fn run_instance_method() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Point (pub ((x i64) (y i64)))) \
                    (defmethod manhattan ((self Point)) i64 (+ (.x self) (.y self))) \
                    (defun f () i64 (let ((p (Point 20 22))) (manhattan p))) \
                    (f)";
        assert_eq!(run(prog)?, 42);
        // method-call sugar form
        let prog2 = "(defstruct Point (pub ((x i64) (y i64)))) \
                     (defmethod manhattan ((self Point)) i64 (+ (.x self) (.y self))) \
                     (defun f () i64 (let ((p (Point 1 9))) (.manhattan p))) \
                     (f)";
        assert_eq!(run(prog2)?, 10);
        Ok(())
    }

    #[test]
    fn run_match() -> Result<(), Box<dyn error::Error>> {
        // literal alternatives + wildcard
        let prog = "(defun classify ((b i64)) i64 \
                      (match b (0x20 | 0x09 | 0x0a => (1)) (_ => (0)))) \
                    (classify 9)";
        assert_eq!(run(prog)?, 1);
        let prog0 = "(defun classify ((b i64)) i64 \
                       (match b (0x20 | 0x09 | 0x0a => (1)) (_ => (0)))) \
                     (classify 65)";
        assert_eq!(run(prog0)?, 0);

        // range pattern + bind
        let prog2 = "(defun kind ((n i64)) i64 \
                       (match n (0 => (0)) (1..=9 => (1)) (x => (x)))) \
                     (kind 5)";
        assert_eq!(run(prog2)?, 1);
        let prog3 = "(defun kind ((n i64)) i64 \
                       (match n (0 => (0)) (1..=9 => (1)) (x => (x)))) \
                     (kind 100)";
        assert_eq!(run(prog3)?, 100);
        Ok(())
    }

    #[test]
    fn run_method_on_primitive() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defmethod double ((self i32)) i32 (* self 2)) (double 21)";
        assert_eq!(run(prog)?, 42);
        Ok(())
    }

    #[test]
    fn run_static_method() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Circle (pub ((radius i64)))) \
                    (defmethod make ((Circle) (r i64)) Circle (Circle r)) \
                    (defun f () i64 (let ((c (make 5))) (.radius c))) \
                    (f)";
        assert_eq!(run(prog)?, 5);
        Ok(())
    }

    /// A rooted struct must survive collections triggered by allocating
    /// megabytes of garbage in a loop (validates shadow-stack rooting).
    #[test]
    fn run_struct_survives_gc_pressure() -> Result<(), Box<dyn error::Error>> {
        let prog = "(defstruct Point (pub ((x i64) (y i64)))) \
                    (defun f () i64 \
                      (let ((p (Point 7 9))) \
                        (progn \
                          (dotimes (i 300000) (let ((g (Point i i))) (.x g))) \
                          (.x p)))) \
                    (f)";
        assert_eq!(run(prog)?, 7);
        Ok(())
    }
}
