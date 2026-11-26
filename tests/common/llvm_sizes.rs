//! A program that asks `std::dbg::llvm-size-of` how many bytes compiled code
//! holds each kind of value in, for `llvm_size_of_test` and the Uno's test.
//!
//! Every check is `(say (= (llvm-size-of E) N))`, where `N` is worked out
//! here for the machine: the program says `y` or `n` for each, in order, and
//! [`Program::failures`] names the checks that said `n`. The program does not
//! define `say`, which writes where the machine can, nor `main`.

/// How wide the machine's own things are, in bytes, and whether it has the
/// runtime, which the values in the heap and `int` need.
pub struct Machine {
    pub word: u32,
    pub ptr: u32,
    pub c_long: u32,
    pub runtime: bool,
}

/// The machine the tests run on.
pub const HOST: Machine = Machine {
    word: std::mem::size_of::<usize>() as u32,
    ptr: std::mem::size_of::<usize>() as u32,
    c_long: std::mem::size_of::<std::ffi::c_long>() as u32,
    runtime: true,
};

/// A Cortex-M and a 32-bit WebAssembly machine.
pub const THIRTY_TWO: Machine = Machine { word: 4, ptr: 4, c_long: 4, runtime: true };

/// The Uno's ATmega328P, which has no runtime.
pub const AVR: Machine = Machine { word: 2, ptr: 2, c_long: 4, runtime: false };

struct Number {
    name: &'static str,
    bytes: u32,
    float: bool,
}

const NUMBERS: [Number; 8] = [
    Number { name: "i8", bytes: 1, float: false },
    Number { name: "u8", bytes: 1, float: false },
    Number { name: "i16", bytes: 2, float: false },
    Number { name: "u16", bytes: 2, float: false },
    Number { name: "i32", bytes: 4, float: false },
    Number { name: "u32", bytes: 4, float: false },
    Number { name: "f32", bytes: 4, float: true },
    Number { name: "f64", bytes: 8, float: true },
];

impl Number {
    fn literal(&self, n: i32) -> String {
        if self.float { format!("(the {} {}.0)", self.name, n) } else { format!("(the {} {})", self.name, n) }
    }
}

pub struct Program {
    pub source: String,
    checks: Vec<String>,
}

impl Program {
    /// The checks `said` (the `y`s and `n`s the program wrote) failed, and
    /// any difference in how many there are.
    pub fn failures(&self, said: &str) -> Vec<String> {
        let said: Vec<char> = said.chars().filter(|c| *c == 'y' || *c == 'n').collect();
        let mut failed: Vec<String> = self.checks.iter().zip(&said).filter(|(_, c)| **c == 'n').map(|(what, _)| what.clone()).collect();
        if said.len() != self.checks.len() {
            failed.push(format!("{} checks said, of {}", said.len(), self.checks.len()));
        }
        failed
    }

    /// Every function of the program, for `(compile ...)`.
    pub fn functions(&self) -> Vec<String> {
        self.source
            .lines()
            .filter_map(|l| l.strip_prefix("(defun "))
            .filter_map(|l| l.split_whitespace().next())
            .filter(|name| !name.contains('<'))
            .map(String::from)
            .collect()
    }
}

/// The checks of one function, and what it is called.
struct Checks<'a> {
    checks: &'a mut Vec<String>,
    body: String,
}

impl Checks<'_> {
    fn check(&mut self, what: String, form: &str, bytes: u32) {
        self.body.push_str(&format!("  (say (= (llvm-size-of {}) (the u32 {})))\n", form, bytes));
        self.checks.push(format!("{}: (llvm-size-of {}) is {}", what, form, bytes));
    }

    /// A check of a value only `unsafe` may hold.
    fn check_unsafe(&mut self, what: String, form: &str, bytes: u32) {
        self.body.push_str(&format!("  (unsafe (say (= (llvm-size-of {}) (the u32 {}))))\n", form, bytes));
        self.checks.push(format!("{}: (llvm-size-of {}) is {}", what, form, bytes));
    }
}

/// The whole program: `(run-checks)` makes every check.
pub fn program(m: &Machine) -> Program {
    let mut checks = Vec::new();
    let mut out = String::from(
        "(use std::dbg::llvm-size-of)
(defun same<A> ((x A)) A x)
",
    );
    if m.runtime {
        out.push_str("(defenum Held<A> (Holding A) (Nothing))\n");
    }
    let mut calls = Vec::new();
    let mut function = |out: &mut String, checks: &mut Vec<String>, head: &str, call: &str, prologue: &str, epilogue: &str, fill: &dyn Fn(&mut Checks)| {
        let mut c = Checks { checks, body: String::new() };
        fill(&mut c);
        out.push_str(&format!("{}\n{}{}{}  ())\n", head, prologue, c.body, epilogue));
        calls.push(call.to_string());
    };
    for t in &NUMBERS {
        let (n, bytes) = (t.name, t.bytes);
        let zero = t.literal(0);
        let one = t.literal(1);
        out.push_str(&format!("(defvar (*kept-{n}* {n}) {zero})\n(defun handed-{n} ((x {n})) {n} x)\n"));
        if m.runtime {
            out.push_str(&format!(
                "(defstruct Three-{n} (before {n}) (it {n}) (after {n}))
(defun optional-{n} (&optional (x {n} {zero})) {n} x)
(defun thrown-{n} ((x {n})) {n} (throw 'out x))
"
            ));
        }
        // Where a value is kept.
        let prologue = format!(
            "  (setf *kept-{n}* v)\n  (let* ((local v) (changed {zero}) (given {one}))\n  (setf changed v)\n{}",
            if m.runtime {
                format!(
                    "  (let* ((three (make-instance 'Three-{n} :before {one} :it {zero} :after {one})) (many (the Vector<{n}> (Vector::new))) (closed (lambda () {n} v)))\n  (setf three::it v)\n  (push many v)\n"
                )
            } else {
                String::new()
            }
        );
        let epilogue = if m.runtime { "  ())\n  ())\n" } else { "  ())\n" };
        function(&mut out, &mut checks, &format!("(defun places-{n} ((v {n})) ()"), &format!("(places-{n} {one})"), &prologue, epilogue, &|c| {
            let mut places = vec![
                ("parameter", "v".to_string()),
                ("local", "local".to_string()),
                ("assigned local", "changed".to_string()),
                ("local of a literal", "given".to_string()),
                ("literal", one.clone()),
                ("global", format!("*kept-{n}*")),
                ("answer", format!("(handed-{n} v)")),
                ("generic answer", "(same v)".to_string()),
                ("chosen", "(if (= local changed) v local)".to_string()),
                ("from block", "(block out (return-from out v))".to_string()),
                ("from let", format!("(let ((x v)) x)")),
                ("from progn", format!("(progn v)")),
            ];
            if m.runtime {
                places.extend([
                    ("from labels", format!("(labels ((inner ((x {n})) {n} x)) (inner v))")),
                    ("field", "three::it".to_string()),
                    ("element", "(get many 0)".to_string()),
                    ("captured", "(closed)".to_string()),
                    ("optional", format!("(optional-{n} v)")),
                    ("option", "(unwrap (Some v))".to_string()),
                    ("variant", "(match (Held::Holding v) ((Holding x) x) (Nothing v))".to_string()),
                    ("thrown", format!("(catch 'out (thrown-{n} v))")),
                    ("read back by c", format!("(unsafe (let ((p (c-alloc {n}))) (setf (c-deref p) v) (c-deref p)))")),
                ]);
            }
            for (what, form) in places {
                c.check(format!("{} {}", n, what), &form, bytes);
            }
        });
        // What an operation on two of them gives.
        let mut ops = vec!["+", "-", "*", "/", "max", "min"];
        if !t.float {
            ops.extend(["rem", "mod", "logand", "logior", "logxor"]);
        }
        function(&mut out, &mut checks, &format!("(defun ops-{n} ((a {n}) (b {n})) ()"), &format!("(ops-{n} {} {})", t.literal(6), t.literal(4)), "", "", &|c| {
            for op in &ops {
                c.check(format!("{} {}", n, op), &format!("({} a b)", op), bytes);
            }
            c.check(format!("{} with a literal", n), &format!("(+ a {})", t.literal(1)), bytes);
            c.check(format!("{} in three", n), "(+ (* a b) (- a b))", bytes);
            if !t.float {
                c.check(format!("{} lognot", n), "(lognot a)", bytes);
                c.check(format!("{} ash", n), "(ash a 3)", bytes);
                c.check(format!("{} ash right", n), "(ash a -1)", bytes);
            }
            for op in ["<", "<=", ">", ">=", "=", "/="] {
                c.check(format!("{} {}", n, op), &format!("({} a b)", op), 1);
            }
            let updated = if t.float { "(let ((x a)) (setf x (+ x b)) x)" } else { "(let ((x a)) (incf x) (decf x) x)" };
            c.check(format!("{} updated in place", n), updated, bytes);
        });
        // What a cast to each other type gives.
        function(&mut out, &mut checks, &format!("(defun casts-{n} ((v {n})) ()"), &format!("(casts-{n} {one})"), "", "", &|c| {
            for to in &NUMBERS {
                c.check(format!("{} as {}", n, to.name), &format!("(as {} v)", to.name), to.bytes);
                c.check(format!("{} as {} and back", n, to.name), &format!("(as {} (as {} v))", n, to.name), bytes);
                if m.runtime {
                    c.check(format!("{} tried as {}", n, to.name), &format!("(unwrap (try-as {} v))", to.name), to.bytes);
                }
            }
            if !t.float {
                c.check(format!("{} as c-long", n), "(unsafe (as c-long v))", m.c_long);
                c.check(format!("{} as c-ulong", n), "(unsafe (as c-ulong v))", m.c_long);
                c.check(format!("{} through c-long", n), &format!("(unsafe (as {} (as c-long v)))", n), bytes);
            }
            if m.runtime {
                c.check(format!("{} as int", n), "(as int v)", m.word);
                c.check(format!("{} through int", n), &format!("(as {} (as int v))", n), bytes);
            }
        });
        // Two widths meet where one is cast to the other.
        function(&mut out, &mut checks, &format!("(defun mixed-{n} ({}) ()", NUMBERS.iter().map(|u| format!("(x-{} {})", u.name, u.name)).collect::<Vec<_>>().join(" ")), &format!("(mixed-{n} {})", NUMBERS.iter().map(|u| u.literal(3)).collect::<Vec<_>>().join(" ")), "", "", &|c| {
            for u in NUMBERS.iter().filter(|u| u.name != n) {
                let u = u.name;
                let u_bytes = NUMBERS.iter().find(|w| w.name == u).map(|w| w.bytes).unwrap();
                c.check(format!("{} + {} as {}", n, u, n), &format!("(+ x-{n} (as {n} x-{u}))"), bytes);
                c.check(format!("{} * {} as {}", n, u, u), &format!("(* (as {u} x-{n}) x-{u})"), u_bytes);
                c.check(format!("{} + {} as f64", n, u), &format!("(+ (as f64 x-{n}) (as f64 x-{u}))"), 8);
                c.check(format!("{} < {}", n, u), &format!("(< x-{n} (as {n} x-{u}))"), 1);
            }
        });
    }
    // The other values.
    function(&mut out, &mut checks, "(defun others ((b bool) (c char)) ()", "(others true #\\a)", "", "", &|c| {
        c.check("bool".into(), "b", 1);
        c.check("bool literal".into(), "true", 1);
        c.check("not".into(), "(not b)", 1);
        c.check("and".into(), "(and b (not b))", 1);
        c.check("char".into(), "c", 4);
        c.check("char literal".into(), "#\\z", 4);
        c.check("float literal".into(), "1.5", 8);
        c.check("c-long".into(), "(unsafe (as c-long (the i16 1)))", m.c_long);
        c.check("c-ulong".into(), "(unsafe (as c-ulong (the i16 1)))", m.c_long);
        if m.runtime {
            c.check("int literal".into(), "5", m.word);
            c.check("int sum".into(), "(+ 5 6)", m.word);
            c.check("bignum".into(), "(ash 1 200)", m.word);
            c.check("string".into(), "\"text\"", m.ptr);
            c.check("vector".into(), "(the Vector<u8> (Vector::new))", m.ptr);
            c.check("struct".into(), "(make-instance 'Three-u8 :before 1 :it 2 :after 3)", m.ptr);
            c.check_unsafe("pointer".into(), "(c-alloc u16)", m.ptr);
            c.check("value pointed at".into(), "(unsafe (c-deref (c-alloc u16)))", 2);
        }
    });
    // Split, so that no one function is the whole of it.
    for (i, part) in calls.chunks(20).enumerate() {
        out.push_str(&format!("(defun part-{} () ()\n  {}\n  ())\n", i, part.join("\n  ")));
    }
    let parts: Vec<String> = (0..calls.chunks(20).count()).map(|i| format!("(part-{})", i)).collect();
    out.push_str(&format!("(defun run-checks () () {} ())\n", parts.join(" ")));
    Program { source: out, checks }
}
