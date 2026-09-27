//! `def-c-struct` and typed pointers: C-laid-out memory allocated inside
//! `(unsafe ...)`, freed when that form is left, and refused when it comes
//! from C.
//!
//! libc is the only C used: `qsort` and `bsearch` call back with pointers into
//! the caller's array, `memset` with a length of 0 hands a pointer straight
//! back, and `malloc` is memory C owns.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Heap, Interp, Reader, Value};

fn eval(src: &str) -> Result<(Heap, Value), String> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| e.to_string())?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| e.to_string())?;
        match interp.exec(&mut h, tl) {
            Ok(Some(val)) => last = val,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((h, last))
}

fn int(src: &str) -> i64 {
    match eval(src).expect("evaluation failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn float(src: &str) -> f64 {
    let (h, v) = eval(src).expect("evaluation failed");
    match v {
        Value::Boxed(id) if h.is_f64(id) => h.f64_value(id),
        other => panic!("expected a float, got {:?}", other),
    }
}

fn err(src: &str) -> String {
    match eval(src) {
        Err(e) => e,
        Ok(_) => panic!("expected a failure, but the program ran"),
    }
}

// ------------------------------------------------------------ fields

const POINTS: &str = r#"
(unsafe (def-c-struct point (x i32) (y f64) (flag bool)))

(defun sum-points ((n int)) f64
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))
          (setf p::y (* 1.5 (int->float i)))
          (setf p::flag (= i 2))))
      (let ((total 0.0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (when p::flag
              (setf total (+ total p::y (int->float (as int p::x)))))))
        total))))
"#;

#[test]
fn fields_are_written_and_read_back() {
    assert_eq!(float(&format!("{}(sum-points 4)", POINTS)), 5.0);
}

#[test]
fn a_compiled_function_reads_the_same_fields() {
    assert_eq!(float(&format!("{}(compile sum-points)(sum-points 4)", POINTS)), 5.0);
}

#[test]
fn a_scalar_is_read_and_written_through_c_deref() {
    let src = "(defun f () int (unsafe (let ((n (c-alloc i32))) (setf (c-deref n) -42) (as int (c-deref n)))))";
    assert_eq!(int(&format!("{}(f)", src)), -42);
    assert_eq!(int(&format!("{}(compile f)(f)", src)), -42);
}

#[test]
fn memory_starts_zeroed() {
    let src = "(unsafe (def-c-struct pair (a i32) (b c-long)))
               (defun f () int (unsafe (let ((p (c-alloc pair 2))) (let ((q (c-ref p 1))) (+ (as int q::a) (as int q::b))))))";
    assert_eq!(int(&format!("{}(f)", src)), 0);
}

#[test]
fn setf_answers_what_it_stored() {
    let src = "(unsafe (def-c-struct cell (v i32)))
               (defun f () i32 (unsafe (let ((c (c-alloc cell))) (setf c::v 9))))";
    assert_eq!(int(&format!("{}(f)", src)), 9);
}

/// C's layout: `y` after `x` at the next multiple of 8, the struct padded to
/// 24. An embedded struct is its address, not a copy.
#[test]
fn embedded_structs_and_pointer_fields() {
    let src = "(unsafe
                 (def-c-struct point (x i32) (y f64))
                 (def-c-struct seg (a point) (b point) (next (ptr seg))))
               (defun f () i32
                 (unsafe
                   (let ((s (c-alloc seg 2)))
                     (let ((s1 (c-ref s 1)))
                       (setf s::next s1)
                       (let ((b s1::b)) (setf b::x 7))
                       (let ((n s::next)) (let ((b n::b)) b::x))))))";
    assert_eq!(int(&format!("{}(f)", src)), 7);
    assert_eq!(int(&format!("{}(compile f)(f)", src)), 7);
}

#[test]
fn an_unset_pointer_field_is_a_null_pointer() {
    let src = "(unsafe (def-c-struct node (v i32) (next (ptr node))))
               (defun f () i32 (unsafe (let ((n (c-alloc node))) (let ((m n::next)) m::v))))
               (f)";
    let e = err(src);
    assert!(e.contains("null pointer"), "unexpected error: {}", e);
}

#[test]
fn c_ref_stays_inside_the_allocation() {
    let e = err("(unsafe (let ((xs (c-alloc i32 3))) (c-deref (c-ref xs 3))))");
    assert!(e.contains("outside the allocation"), "unexpected error: {}", e);
}

// ------------------------------------------------------------ callbacks

const SORT: &str = r#"
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))
(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1)) (* 10 (key-at xs 2)) (key-at xs 3)))))
"#;

#[test]
fn qsort_sorts_an_array_of_c_structs() {
    assert_eq!(int(&format!("{}(sorted-keys)", SORT)), 1234);
}

#[test]
fn a_compiled_caller_sorts_an_array_of_c_structs() {
    assert_eq!(int(&format!("{}(compile sorted-keys)(sorted-keys)", SORT)), 1234);
}

const BSEARCH: &str = r#"
(defffi (c-bsearch "bsearch")
  ((ptr i32) (ptr i32) c-ulong c-ulong (fn ((ptr i32) (ptr i32)) i32)) (ptr i32))
(defffi (c-malloc "malloc") (c-ulong) (ptr i32))
(defffi (c-memset "memset") (ptr i32 c-ulong) (ptr i32))
"#;

/// `bsearch` answers a pointer into the caller's own array.
#[test]
fn a_pointer_c_returns_into_our_memory_is_accepted() {
    let src = format!(
        "{}(defun f () i32
             (unsafe
               (let ((xs (c-alloc i32 3)) (k (c-alloc i32)))
                 (setf (c-deref xs) 10)
                 (setf (c-deref (c-ref xs 1)) 20)
                 (setf (c-deref (c-ref xs 2)) 30)
                 (setf (c-deref k) 20)
                 (let ((hit (c-bsearch k xs 3 4 (lambda ((a (ptr i32)) (b (ptr i32))) i32 (- (c-deref a) (c-deref b))))))
                   (setf (c-deref hit) 21)
                   (c-deref (c-ref xs 1))))))
           (f)",
        BSEARCH
    );
    assert_eq!(int(&src), 21);
}

#[test]
fn a_pointer_to_memory_c_allocated_is_refused() {
    let e = err(&format!("{}(unsafe (c-deref (c-malloc 4)))", BSEARCH));
    assert!(e.contains("does not point into memory an `unsafe` allocated"), "unexpected error: {}", e);
}

/// The memory is gone once the owning `unsafe` is left: an address kept as
/// an untyped `ptr` is refused when C hands it back typed.
#[test]
fn memory_is_freed_when_the_unsafe_is_left() {
    let src = format!(
        "{}(defun stale () ptr (unsafe (let ((n (c-alloc i32))) (as ptr n))))
           (unsafe (c-deref (c-memset (stale) 0 0)))",
        BSEARCH
    );
    let e = err(&src);
    assert!(e.contains("does not point into memory an `unsafe` allocated"), "unexpected error: {}", e);
}

#[test]
fn memory_is_freed_when_the_unsafe_is_left_by_throw() {
    let src = format!(
        "{}(defun thrower () ptr (unsafe (let ((n (c-alloc i32))) (throw 'k (as ptr n)))))
           (defun escape () ptr (unsafe (catch 'k (thrower))))
           (unsafe (c-deref (c-memset (escape) 0 0)))",
        BSEARCH
    );
    let e = err(&src);
    assert!(e.contains("does not point into memory an `unsafe` allocated"), "unexpected error: {}", e);
}

#[test]
fn a_live_pointer_handed_back_by_c_is_accepted() {
    let src = format!(
        "{}(unsafe (let ((n (c-alloc i32))) (setf (c-deref n) 7) (as int (c-deref (c-memset (as ptr n) 0 0)))))",
        BSEARCH
    );
    assert_eq!(int(&src), 7);
}

/// A callback's typed-pointer argument is checked too; the failure comes
/// back after `qsort` returns.
#[test]
fn a_callback_argument_into_c_memory_is_refused() {
    let src = r#"
        (defffi (c-malloc "malloc") (c-ulong) ptr)
        (defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr i32) (ptr i32)) i32)) ())
        (unsafe (c-qsort (c-malloc 8) 2 4 (lambda ((a (ptr i32)) (b (ptr i32))) i32 (- (c-deref a) (c-deref b)))))
    "#;
    let e = err(src);
    assert!(e.contains("does not point into memory an `unsafe` allocated"), "unexpected error: {}", e);
}

// ------------------------------------------------------------ refused statically

#[test]
fn an_unsafe_cannot_answer_a_typed_pointer() {
    let e = err("(defun f () i32 (let ((p (unsafe (c-alloc i32)))) 0))");
    assert!(e.contains("cannot answer"), "unexpected error: {}", e);
}

#[test]
fn a_closure_cannot_capture_a_typed_pointer() {
    let e = err("(unsafe (let ((p (c-alloc i32))) (let ((f (lambda () i32 (c-deref p)))) (f))))");
    assert!(e.contains("captures `p`"), "unexpected error: {}", e);
    let e = err("(unsafe (let ((p (c-alloc i32))) (labels ((g () i32 (c-deref p))) (g))))");
    assert!(e.contains("captures `p`"), "unexpected error: {}", e);
}

#[test]
fn a_task_cannot_be_handed_a_typed_pointer() {
    let e = err("(defun peek ((p (ptr i32))) i32 (unsafe (c-deref p)))
                 (unsafe (let ((p (c-alloc i32))) (task (peek p)) 0))");
    assert!(e.contains("cannot be passed to `task`"), "unexpected error: {}", e);
}

#[test]
fn a_typed_pointer_cannot_be_thrown() {
    let e = err("(unsafe (catch 'k (let ((p (c-alloc i32))) (throw 'k p))))");
    assert!(e.contains("typed pointer cannot leave"), "unexpected error: {}", e);
}

#[test]
fn a_typed_pointer_cannot_be_stored() {
    let e = err("(defstruct holder (p (ptr i32)))");
    assert!(e.contains("cannot be stored"), "unexpected error: {}", e);
}

#[test]
fn c_alloc_needs_an_unsafe_in_its_own_function() {
    let e = err("(unsafe (let ((f (lambda () i32 (c-deref (c-alloc i32))))) (f)))");
    assert!(e.contains("c-alloc: the memory belongs"), "unexpected error: {}", e);
}

#[test]
fn def_c_struct_is_written_inside_a_top_level_unsafe() {
    let e = err("(def-c-struct p (x i32))");
    assert!(e.contains("inside `(unsafe ...)`"), "unexpected error: {}", e);
    let e = err("(pub def-c-struct p (x i32))");
    assert!(e.contains("inside `(unsafe ...)`"), "unexpected error: {}", e);
    let e = err("(unsafe (def-c-struct p (x i32)) (+ 1 2))");
    assert!(e.contains("nothing but `def-c-struct`s"), "unexpected error: {}", e);
}

#[test]
fn a_c_struct_field_is_a_c_type() {
    let e = err("(unsafe (def-c-struct p (name string)))");
    assert!(e.contains("cannot be `string`"), "unexpected error: {}", e);
    let e = err("(unsafe (def-c-struct p (self p)))");
    assert!(e.contains("infinitely"), "unexpected error: {}", e);
}

#[test]
fn a_c_struct_is_not_a_value_type() {
    let e = err("(unsafe (def-c-struct p (x i32))) (defun f ((v p)) i32 1)");
    assert!(e.contains("unknown type `p`"), "unexpected error: {}", e);
}

#[test]
fn a_typed_pointer_points_at_a_c_type() {
    let e = err("(defun f ((p (ptr string))) i32 1)");
    assert!(e.contains("a typed pointer points at"), "unexpected error: {}", e);
}

#[test]
fn a_c_struct_shares_the_type_name_space() {
    let e = err("(unsafe (def-c-struct p (x i32))) (defstruct p (x i32))");
    assert!(e.contains("a `def-c-struct` of that name already exists"), "unexpected error: {}", e);
    let e = err("(defstruct p (x i32)) (unsafe (def-c-struct p (x i32)))");
    assert!(e.contains("a type of that name already exists"), "unexpected error: {}", e);
}
