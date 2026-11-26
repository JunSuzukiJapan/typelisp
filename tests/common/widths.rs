//! A program that finds out, at run time, how wide the numbers are, for
//! `size_of_test` and the tests of the targets.
//!
//! What a width should be comes from `size-of`; what it is comes from what
//! the values do — where they wrap, what a cast keeps, whether a write reaches
//! its neighbour — worked out again in `int`, which has no width.

pub struct Fixed {
    pub name: &'static str,
    pub signed: bool,
    pub bits: u32,
}

pub const FIXED: [Fixed; 6] = [
    Fixed { name: "i8", signed: true, bits: 8 },
    Fixed { name: "u8", signed: false, bits: 8 },
    Fixed { name: "i16", signed: true, bits: 16 },
    Fixed { name: "u16", signed: false, bits: 16 },
    Fixed { name: "i32", signed: true, bits: 32 },
    Fixed { name: "u32", signed: false, bits: 32 },
];

impl Fixed {
    pub fn min(&self) -> i64 {
        if self.signed { -(1 << (self.bits - 1)) } else { 0 }
    }

    pub fn max(&self) -> i64 {
        (1 << (self.bits - u32::from(self.signed))) - 1
    }

    /// Where a width or a sign goes wrong: the ends, around zero, around a
    /// byte boundary, and a pattern with every byte different.
    pub fn values(&self) -> Vec<i64> {
        let mut all = vec![self.min(), self.min() + 1, -2, -1, 0, 1, 2, 127, 128, 255, 256, 32767, 32768, 65535, 65536, 0x1234_5678, self.max() - 1, self.max()];
        all.retain(|n| (self.min()..=self.max()).contains(n));
        all.sort();
        all.dedup();
        all
    }
}

/// `int`s a cast has to cut.
const INTS: [&str; 16] = [
    "0", "-1", "128", "-129", "256", "32768", "65536", "2147483647", "2147483648", "-2147483649", "4294967295", "4294967296", "81985529216486895",
    "9223372036854775808", "-9223372036854775809", "18446744073709551621",
];

const HELPERS: &str = r#"
(defvar (*bad* string) "")
(defvar (*made* int) 0)
(defun check ((what string) (got int) (want int)) ()
  (setf *made* (+ *made* 1))
  (unless (= got want) (setf *bad* (format false "~a~a: ~a and not ~a~%" *bad* what got want)))
  ())
(defun wrapped ((n int) (bytes int) (signed bool)) int
  (let* ((m (ash 1 (* 8 bytes))) (u (mod n m)))
    (if (and signed (>= u (ash m -1))) (- u m) u)))
(defun truth ((b bool)) int (if b 1 0))
(defun same<A> ((x A)) A x)
(defun size-in-generic<A> ((x A)) int (size-of A))
(defun size-of-generic-val<A> ((x A)) int (size-of-val x))
(defenum Held<A> (Holding A) (Nothing))
"#;

fn each(call: &str, values: &[String]) -> String {
    values.iter().map(|v| format!("  ({} {})\n", call, v)).collect()
}

/// The whole program: its `(run-checks)` answers the failures, one to a line,
/// and the number of checks made.
pub fn program() -> String {
    let mut out = String::from(HELPERS);
    let mut main = String::new();
    for t in &FIXED {
        out.push_str(&locations(t.name));
        out.push_str(&operations(t));
        for to in &FIXED {
            out.push_str(&cast(t.name, to));
        }
        let values: Vec<String> = t.values().iter().map(|n| n.to_string()).collect();
        main.push_str(&format!("  (check \"size of {0}\" (size-of {0}) {1})\n  (widths-{0})\n", t.name, t.bits / 8));
        main.push_str(&each(&format!("places-{}", t.name), &values));
        for to in &FIXED {
            main.push_str(&each(&format!("cast-{}-{}", t.name, to.name), &values));
        }
        main.push_str(&each(&format!("c-words-{}", t.name), &values));
        for a in &values {
            main.push_str(&each(&format!("ops-{} {}", t.name, a), &values));
        }
        out.push_str(&mixed(t));
        for u in FIXED.iter().filter(|u| u.name != t.name) {
            for a in [t.min(), 1, t.max()] {
                for b in [u.min(), 1, u.max()] {
                    main.push_str(&format!("  (mixed-{}-{} {} {})\n", t.name, u.name, a, b));
                }
            }
        }
    }
    for to in &FIXED {
        out.push_str(&cast("int", to));
        main.push_str(&each(&format!("cast-int-{}", to.name), &INTS.map(String::from)));
        out.push_str(&format!(
            "(defun float-{0} ((x f64)) () (check \"f64 to {0}\" (as int (as {0} x)) (wrapped (as int x) (size-of {0}) {1})) (check \"f32 to {0}\" (as int (as {0} (as f32 x))) (wrapped (as int (as f32 x)) (size-of {0}) {1})) (check \"size\" (size-of-val (as {0} x)) (size-of {0})))\n",
            to.name, to.signed
        ));
        main.push_str(&each(&format!("float-{}", to.name), &["0.0", "1.75", "-1.75", "127.0", "200.5", "-200.5", "70000.0", "-70000.0", "16777216.0"].map(String::from)));
    }
    out.push_str(FLOATS);
    for f in ["f32", "f64"] {
        out.push_str(&locations(f));
        main.push_str(&each(&format!("places-{}", f), &["0.0", "-1.5", "16777216.0"].map(String::from)));
    }
    out.push_str(OTHERS);
    main.push_str("  (check-floats)\n  (check-others)\n");
    // Split, so that no one function is the whole of it.
    let lines: Vec<&str> = main.lines().collect();
    let mut calls = String::new();
    for (i, part) in lines.chunks(40).enumerate() {
        out.push_str(&format!("(defun part-{} ()  ()\n{}\n  ())\n", i, part.join("\n")));
        calls.push_str(&format!(" (part-{})", i));
    }
    out.push_str(&format!("(defun run-checks () string {} (format false \"~a~a checks\" *bad* *made*))\n", calls));
    out
}

/// Every function of [`program`], for `(compile ...)`.
pub fn functions(program: &str) -> Vec<String> {
    program
        .lines()
        .filter_map(|l| l.strip_prefix("(defun "))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|name| !name.contains('<'))
        .map(String::from)
        .collect()
}

/// A `t` kept everywhere a value can be kept comes back whole, is said to be
/// as wide as a `t`, and leaves what is beside it alone.
fn locations(t: &str) -> String {
    let equal = |what: &str, form: &str| {
        format!("  (check \"{t} {what}\" (truth (= {form} v)) 1)\n  (check \"size of {t} {what}\" (size-of-val {form}) (size-of {t}))\n")
    };
    let one = if t.starts_with('f') { "1.0" } else { "1" };
    let zero = if t.starts_with('f') { "0.0" } else { "0" };
    let mut s = format!(
        "(defstruct Three-{t} (before {t}) (it {t}) (after {t}))
(defvar (*kept-{t}* {t}) {zero})
(unsafe (def-c-struct In-c-{t} (before u8) (it {t}) (after {t})))
(defun handed-{t} ((x {t})) {t} x)
(defun optional-{t} (&optional (x {t} {zero})) {t} x)
(defun thrown-{t} ((x {t})) {t} (throw 'out x))
(defun places-{t} ((v {t})) ()
  (setf *kept-{t}* v)
  (let* ((local v) (changed (the {t} {zero})) (three (make-instance 'Three-{t} :before {one} :it {zero} :after {one}))
         (many (the Vector<{t}> (Vector::new))) (closed (lambda () {t} v)) (cell (cons v {one})))
    (setf changed v)
    (setf three::it v)
    (push many {one}) (push many v) (push many {one})
"
    );
    for (what, form) in [
        ("parameter", "v".to_string()),
        ("local", "local".to_string()),
        ("assigned local", "changed".to_string()),
        ("global", format!("*kept-{t}*")),
        ("field", "three::it".to_string()),
        ("element", "(get many 1)".to_string()),
        ("captured", "(closed)".to_string()),
        ("answer", format!("(handed-{t} v)")),
        ("generic answer", "(same v)".to_string()),
        ("optional", format!("(optional-{t} v)")),
        ("option", "(unwrap (Some v))".to_string()),
        ("variant", "(match (Held::Holding v) ((Holding x) x) (Nothing (panic \"nothing\")))".to_string()),
        ("thrown", format!("(catch 'out (thrown-{t} v))")),
        ("chosen", "(if (= local changed) v local)".to_string()),
        ("quoted through int", if t.starts_with('f') { "v".to_string() } else { format!("(as {t} (as int v))") }),
        ("from labels", format!("(labels ((inner ((x {t})) {t} x)) (inner v))")),
        ("from block", "(block out (return-from out v))".to_string()),
        ("read back by c", format!("(unsafe (let ((p (c-alloc {t}))) (setf (c-deref p) v) (c-deref p)))")),
    ] {
        s.push_str(&equal(what, &form));
    }
    s.push_str(&format!(
        "  (check \"{t} beside the field\" (truth (and (= three::before {one}) (= three::after {one}))) 1)
  (check \"{t} beside the element\" (truth (and (= (get many 0) {one}) (= (get many 2) {one}))) 1)
  (check \"{t} beside the cell\" (truth (= (car cell) v)) 1)
  (check \"{t} in a generic\" (size-in-generic v) (size-of {t}))
  (check \"{t} generic val\" (size-of-generic-val v) (size-of {t}))
  (unsafe
    (let ((row (c-alloc {t} 3)) (in-c (c-alloc In-c-{t})))
      (setf (c-deref (c-ref row 0)) {one}) (setf (c-deref (c-ref row 2)) {one}) (setf (c-deref (c-ref row 1)) v)
      (check \"{t} beside in c\" (truth (and (= (c-deref (c-ref row 0)) {one}) (= (c-deref (c-ref row 2)) {one}))) 1)
      (setf in-c::before 170) (setf in-c::after {one}) (setf in-c::it v)
      (check \"{t} in a c struct\" (truth (and (= in-c::it v) (= in-c::before 170) (= in-c::after {one}))) 1)
      (check \"size of a c struct of {t}\" (size-of In-c-{t}) (* 3 (size-of {t}))))))
  ())
"
    ));
    s
}

/// Where a `t` runs out of bits, and what each operation on two of them gives.
fn operations(t: &Fixed) -> String {
    let (n, signed) = (t.name, t.signed);
    let mut s = format!(
        "(defun widths-{n} () ()
  (let ((x (the {n} 1)) (bits 0))
    (while (/= x 0) (setf x (ash x 1)) (setf bits (+ bits 1)))
    (check \"bits of {n}\" bits (* 8 (size-of {n}))))
  (let ((x (the {n} {max})))
    (setf x (+ x 1))
    (check \"{n} past its end\" (as int x) {min})
    (decf x)
    (check \"{n} before its start\" (as int x) {max})
    (incf x)
    (check \"{n} past its end again\" (as int x) {min}))
  ())
(defun ops-{n} ((a {n}) (b {n})) ()
",
        max = t.max(),
        min = t.min()
    );
    for op in ["+", "-", "*", "logand", "logior", "logxor", "max", "min"] {
        s.push_str(&format!(
            "  (check \"{n} {op}\" (as int ({op} a b)) (wrapped ({op} (as int a) (as int b)) (size-of {n}) {signed}))\n  (check \"size of {n} {op}\" (size-of-val ({op} a b)) (size-of {n}))\n"
        ));
    }
    for op in ["<", "<=", ">", ">=", "=", "/="] {
        s.push_str(&format!(
            "  (check \"{n} {op}\" (truth ({op} a b)) (truth ({op} (as int a) (as int b))))\n  (check \"size of {n} {op}\" (size-of-val ({op} a b)) (size-of bool))\n"
        ));
    }
    s.push_str("  (unless (= b 0)\n");
    for (op, whole) in [("mod", "mod"), ("rem", "rem"), ("/", "truncate")] {
        // The smallest over -1 is one past the largest: an error, as dividing
        // by zero is.
        let guard = if op == "/" && signed { format!("(unless (and (= (as int a) {}) (= (as int b) -1))", t.min()) } else { "(progn".into() };
        s.push_str(&format!(
            "    {guard} (check \"{n} {op}\" (as int ({op} a b)) ({whole} (as int a) (as int b))) (check \"size of {n} {op}\" (size-of-val ({op} a b)) (size-of {n})))\n"
        ));
    }
    s.push_str("    ())\n");
    s.push_str(&format!(
        "  (check \"{n} lognot\" (as int (lognot a)) (wrapped (lognot (as int a)) (size-of {n}) {signed}))
  (check \"size of {n} lognot\" (size-of-val (lognot a)) (size-of {n}))
  (check \"{n} literal\" (as int (+ a 1)) (wrapped (+ (as int a) 1) (size-of {n}) {signed}))
  (check \"size of {n} literal\" (size-of-val (+ a 1)) (size-of {n}))
  (check \"{n} in three\" (as int (+ (* a b) (- a b))) (wrapped (+ (* (as int a) (as int b)) (- (as int a) (as int b))) (size-of {n}) {signed}))
"
    ));
    for count in [-33, -32, -8, -1, 0, 1, 7, 8, 15, 16, 31, 32, 33] {
        s.push_str(&format!("  (check \"{n} ash {count}\" (as int (ash a {count})) (wrapped (ash (as int a) {count}) (size-of {n}) {signed}))\n"));
    }
    s.push_str(&format!("  (check \"size of {n} ash\" (size-of-val (ash a 3)) (size-of {n}))\n  ())\n"));
    s.push_str(&format!(
        "(defun c-words-{n} ((a {n})) ()
  (unsafe
    (check \"{n} to c-long\" (as int (as c-long a)) (wrapped (as int a) (size-of c-long) true))
    (check \"{n} to c-ulong\" (as int (as c-ulong a)) (wrapped (as int a) (size-of c-ulong) false))
    (check \"{n} there and back\" (as int (as {n} (as c-long a))) (as int a))
    (check \"{n} there and back, unsigned\" (as int (as {n} (as c-ulong a))) (as int a))
    (check \"size of {n} as c-long\" (size-of-val (as c-long a)) (size-of c-long)))
  ())\n"
    ));
    s
}

fn cast(from: &str, to: &Fixed) -> String {
    let (t, signed) = (to.name, to.signed);
    format!(
        "(defun cast-{from}-{t} ((v {from})) ()
  (check \"{from} to {t}\" (as int (as {t} v)) (wrapped (as int v) (size-of {t}) {signed}))
  (check \"size of {from} to {t}\" (size-of-val (as {t} v)) (size-of {t}))
  (check \"{from} fits {t}\" (truth (is-some (try-as {t} v))) (truth (= (as int v) (wrapped (as int v) (size-of {t}) {signed}))))
  (match (try-as {t} v)
    ((Some x) (check \"{from} tried to {t}\" (as int x) (as int v)) (check \"size of {from} tried to {t}\" (size-of-val x) (size-of {t})))
    (None ()))
  ())\n"
    )
}

/// Numbers of two widths meet only where one is cast to the other, or both to
/// something wider.
fn mixed(t: &Fixed) -> String {
    let (a, signed) = (t.name, t.signed);
    let mut s = String::new();
    for u in FIXED.iter().filter(|u| u.name != a) {
        let b = u.name;
        s.push_str(&format!("(defun mixed-{a}-{b} ((x {a}) (y {b})) ()\n"));
        for op in ["+", "-", "*", "logand", "logior", "logxor"] {
            s.push_str(&format!(
                "  (check \"{a} {op} {b} as {a}\" (as int ({op} x (as {a} y))) (wrapped ({op} (as int x) (as int y)) (size-of {a}) {signed}))
  (check \"size of {a} {op} {b} as {a}\" (size-of-val ({op} x (as {a} y))) (size-of {a}))
  (check \"size of {a} {op} {b} as int\" (size-of-val ({op} (as int x) (as int y))) (size-of int))\n"
            ));
        }
        s.push_str(&format!(
            "  (check \"{a} + {b} as f64\" (as int (+ (as f64 x) (as f64 y))) (+ (as int x) (as int y)))
  (check \"size of {a} + {b} as f64\" (size-of-val (+ (as f64 x) (as f64 y))) (size-of f64))
  ())\n"
        ));
    }
    s
}

const FLOATS: &str = r#"
(defun digits-32 () int
  (let ((e (the f32 1.0)) (n 0))
    (while (/= (+ (the f32 1.0) e) (the f32 1.0)) (setf e (/ e (the f32 2.0))) (setf n (+ n 1)))
    n))
(defun digits-64 () int
  (let ((e 1.0) (n 0))
    (while (/= (+ 1.0 e) 1.0) (setf e (/ e 2.0)) (setf n (+ n 1)))
    n))
(defun ops-f32 ((a f32) (b f32)) ()
  (check "size of f32 +" (size-of-val (+ a b)) (size-of f32))
  (check "size of f32 *" (size-of-val (* a b)) (size-of f32))
  (check "size of f32 /" (size-of-val (/ a b)) (size-of f32))
  (check "f32 rounds" (truth (= (as f64 (/ a b)) (/ (as f64 a) (as f64 b)))) 0)
  (check "f32 to f64 and back" (truth (= (as f32 (as f64 (/ a b))) (/ a b))) 1)
  (check "size of f32 as f64" (size-of-val (as f64 a)) (size-of f64))
  (check "size of f32 < f32" (size-of-val (< a b)) (size-of bool))
  ())
(defun ops-f64 ((a f64) (b f64)) ()
  (check "size of f64 +" (size-of-val (+ a b)) (size-of f64))
  (check "size of f64 as f32" (size-of-val (as f32 a)) (size-of f32))
  (check "f64 to f32 rounds" (truth (= (as f64 (as f32 (/ a b))) (/ a b))) 0)
  ())
(defun check-floats () ()
  (check "size of f32" (size-of f32) 4)
  (check "size of f64" (size-of f64) 8)
  (check "digits of f32" (digits-32) 24)
  (check "digits of f64" (digits-64) 53)
  (ops-f32 1.0 3.0)
  (ops-f64 1.0 3.0)
  (check "an f32 holds 2^24" (as int (as f32 16777216)) 16777216)
  (check "and not 2^24 + 1" (as int (as f32 16777217)) 16777216)
  (check "an f64 holds 2^53" (as int (as f64 9007199254740992)) 9007199254740992)
  (check "and not 2^53 + 1" (as int (as f64 9007199254740993)) 9007199254740992)
  ())
"#;

const OTHERS: &str = r#"
(defun char-through ((c char)) char c)
(defun c-long-through ((n int)) ()
  (unsafe
    (check "int to c-long" (as int (as c-long n)) (wrapped n (size-of c-long) true))
    (check "int to c-ulong" (as int (as c-ulong n)) (wrapped n (size-of c-ulong) false))
    (check "c-long to c-ulong" (as int (as c-ulong (as c-long n))) (wrapped n (size-of c-ulong) false))
    (let ((row (c-alloc c-long 2)))
      (setf (c-deref (c-ref row 1)) (as c-long n))
      (check "c-long in c" (as int (c-deref (c-ref row 1))) (wrapped n (size-of c-long) true))
      (check "c-long beside in c" (as int (c-deref (c-ref row 0))) 0)))
  ())
(unsafe (def-c-struct Mixed-c (a u8) (b u32) (c u8) (d u16) (e f64) (f bool)))
(defun check-others () ()
  (check "size of char" (size-of char) 4)
  (check "char" (as int (char->int (char-through (int->char 1114111)))) 1114111)
  (check "size of a char" (size-of-val #\a) (size-of char))
  (check "size of bool" (size-of bool) 1)
  (check "size of ()" (size-of ()) 0)
  (check "size of c-long and ptr" (size-of c-long) (size-of ptr))
  (check "size of c-ulong" (size-of c-ulong) (size-of c-long))
  (check "size of int" (size-of int) (size-of ptr))
  (check "size of an int literal" (size-of-val 5) (size-of int))
  (check "size of a float literal" (size-of-val 5.0) (size-of f64))
  (check "size of a sum of literals" (size-of-val (+ 1 2)) (size-of int))
  (check "an int has no width" (- (+ (ash 1 200) 1) (ash 1 200)) 1)
  (c-long-through 0) (c-long-through -1) (c-long-through 2147483647) (c-long-through 2147483648)
  (c-long-through -2147483649) (c-long-through 4294967301) (c-long-through 9223372036854775807)
  (c-long-through 9223372036854775808) (c-long-through 18446744073709551621)
  (unsafe
    (let ((m (c-alloc Mixed-c)))
      (setf m::a 255) (setf m::b 4294967295) (setf m::c 255) (setf m::d 65535) (setf m::e -0.5) (setf m::f true)
      (setf m::b 0)
      (check "mixed a" (as int m::a) 255) (check "mixed b" (as int m::b) 0) (check "mixed c" (as int m::c) 255)
      (check "mixed d" (as int m::d) 65535) (check "mixed e" (truth (= m::e -0.5)) 1) (check "mixed f" (truth m::f) 1)
      (setf m::c 0) (setf m::e 1.0)
      (check "mixed a again" (as int m::a) 255) (check "mixed d again" (as int m::d) 65535)
      (check "mixed f again" (truth m::f) 1)
      (check "size of a mixed c struct" (size-of Mixed-c) 32)
      (check "size of a value of one" (size-of-val (c-deref (c-alloc u16))) 2)
      (check "size of a pointer" (size-of-val m) (size-of ptr))))
  ())
"#;
