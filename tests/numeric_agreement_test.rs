//! Every fixed-width integer operation and every float operation gives the
//! same answer interpreted, compiled, and folded at check time, on the edge
//! values of each type.
//!
//! The three are separate implementations of one meaning
//! (`typelisp_mem::fixed`): the interpreter runs the operation itself,
//! constant folding runs it on literal operands while checking, and compiled
//! code runs the instructions the compiler emitted. A table over the values
//! where widths and signs go wrong (the extremes, `-1`, `0`, `1`, the
//! neighbours of a byte boundary) is what catches one of them drifting.

extern crate typelisp;
mod common;
use common::{eval_string, eval_string_compiled};

/// One fixed-width type and the values its operations are tried on.
struct Width {
    name: &'static str,
    values: &'static [&'static str],
    min: &'static str,
}

const WIDTHS: &[Width] = &[
    Width { name: "i8", values: &["-128", "-127", "-2", "-1", "0", "1", "2", "63", "126", "127"], min: "-128" },
    Width { name: "u8", values: &["0", "1", "2", "127", "128", "254", "255"], min: "0" },
    Width { name: "i16", values: &["-32768", "-32767", "-256", "-1", "0", "1", "255", "32766", "32767"], min: "-32768" },
    Width { name: "u16", values: &["0", "1", "255", "256", "32767", "32768", "65534", "65535"], min: "0" },
    Width {
        name: "i32",
        values: &["-2147483648", "-2147483647", "-65536", "-1", "0", "1", "65535", "2147483646", "2147483647"],
        min: "-2147483648",
    },
    Width { name: "u32", values: &["0", "1", "65535", "65536", "2147483647", "2147483648", "4294967294", "4294967295"], min: "0" },
];

const BINARY: &[&str] = &[
    "(+ a b)", "(- a b)", "(* a b)", "(< a b)", "(<= a b)", "(> a b)", "(>= a b)", "(= a b)", "(/= a b)", "(max a b)", "(min a b)",
    "(logand a b)", "(logior a b)", "(logxor a b)", "(logtest a b)",
];

const UNARY: &[&str] = &["(lognot a)", "(logcount a)", "(integer-length a)", "(as int a)", "(as f64 a)"];

const TARGETS: &[&str] = &["i8", "i16", "i32", "u8", "u16", "u32", "c-long", "c-ulong"];

const COUNTS: &[&str] = &["-70", "-33", "-32", "-31", "-17", "-16", "-9", "-8", "-7", "-1", "0", "1", "7", "8", "9", "15", "16", "31", "32", "33", "70"];

/// `forms` as one `format` over `~a`s, each separated by a space. Inside
/// `unsafe`, where a C word may be a value.
fn formatted(forms: &[String]) -> String {
    format!("(unsafe (format false \"{}\" {}))", vec!["~a"; forms.len()].join(" "), forms.join(" "))
}

/// `(as TO a)` and `(try-as TO a)` for every target. A C word has no `Sexpr`
/// form to print, so one is read back as an `int`; and it has no `try-as`,
/// since an `Option` cannot hold one.
fn casts() -> Vec<String> {
    TARGETS
        .iter()
        .flat_map(|to| {
            if to.starts_with("c-") {
                vec![format!("(as int (as {} a))", to)]
            } else {
                vec![format!("(as {} a)", to), format!("(try-as {} a)", to)]
            }
        })
        .collect()
}

/// `form` with its operands `a` and `b` replaced by the forms `a` and `b`.
fn folded(form: &str, a: &str, b: &str) -> String {
    form.replace(" a", &format!(" {}", a)).replace(" b)", &format!(" {})", b))
}

/// `defs` defines the functions named in `names`; `calls` are `string`
/// forms calling them, and `literal` the same forms with the operands
/// written in. All three ways of running them must give the same lines.
fn agree(what: &str, defs: &str, names: &[&str], calls: &[String], literal: &[String]) {
    let lines = |forms: &[String]| format!("(format false \"~{{~a~%~}}\" (list {}))", forms.join(" "));
    let interpreted = eval_string(&format!("{}\n{}", defs, lines(calls)));
    let compiles: String = names.iter().map(|n| format!("(compile {})\n", n)).collect();
    let compiled = eval_string_compiled(&format!("{}\n{}{}", defs, compiles, lines(calls)));
    let folded = eval_string(&lines(literal));
    let mut disagreements = Vec::new();
    for (i, ((x, y), z)) in interpreted.lines().zip(compiled.lines()).zip(folded.lines()).enumerate() {
        if x != y || x != z {
            disagreements.push(format!("{}\n  interpreted: {}\n  compiled:    {}\n  folded:      {}", calls[i], x, y, z));
        }
    }
    assert!(disagreements.is_empty(), "{}: {} disagreements\n{}", what, disagreements.len(), disagreements.join("\n"));
    assert_eq!(interpreted.lines().count(), calls.len(), "{}: interpreted", what);
    assert_eq!(compiled.lines().count(), calls.len(), "{}: compiled", what);
    assert_eq!(folded.lines().count(), calls.len(), "{}: folded", what);
}

fn integer_width_agrees(w: &Width) {
    let t = w.name;
    let binary: Vec<String> = BINARY.iter().map(|f| f.to_string()).collect();
    let unary: Vec<String> = UNARY.iter().map(|f| f.to_string()).chain(casts()).collect();
    let division = ["(/ a b)".to_string()];
    let modulo = ["(mod a b)".to_string(), "(rem a b)".to_string()];
    let shift = ["(ash a b)".to_string(), "(logbitp a b)".to_string()];
    let defs = format!(
        "(defun bin ((a {t}) (b {t})) string {})\n(defun un ((a {t})) string {})\n(defun dv ((a {t}) (b {t})) string {})\n(defun md ((a {t}) (b {t})) string {})\n(defun sh ((a {t}) (b int)) string {})",
        formatted(&binary),
        formatted(&unary),
        formatted(&division),
        formatted(&modulo),
        formatted(&shift),
    );
    let (mut calls, mut literal) = (Vec::new(), Vec::new());
    let lit = |forms: &[String], a: &str, b: &str, bty: &str| {
        let (a, b) = (format!("(the {} {})", t, a), format!("(the {} {})", bty, b));
        formatted(&forms.iter().map(|f| folded(f, &a, &b)).collect::<Vec<_>>())
    };
    for a in w.values {
        calls.push(format!("(un {})", a));
        literal.push(lit(&unary, a, "", t));
        for b in w.values {
            calls.push(format!("(bin {} {})", a, b));
            literal.push(lit(&binary, a, b, t));
            if *b != "0" {
                calls.push(format!("(md {} {})", a, b));
                literal.push(lit(&modulo, a, b, t));
                if !(*a == w.min && *b == "-1") {
                    calls.push(format!("(dv {} {})", a, b));
                    literal.push(lit(&division, a, b, t));
                }
            }
        }
        for n in COUNTS {
            calls.push(format!("(sh {} {})", a, n));
            literal.push(lit(&shift, a, n, "int"));
        }
    }
    agree(t, &defs, &["bin", "un", "dv", "md", "sh"], &calls, &literal);
}

#[test]
fn i8_agrees() {
    integer_width_agrees(&WIDTHS[0]);
}

#[test]
fn u8_agrees() {
    integer_width_agrees(&WIDTHS[1]);
}

#[test]
fn i16_agrees() {
    integer_width_agrees(&WIDTHS[2]);
}

#[test]
fn u16_agrees() {
    integer_width_agrees(&WIDTHS[3]);
}

#[test]
fn i32_agrees() {
    integer_width_agrees(&WIDTHS[4]);
}

#[test]
fn u32_agrees() {
    integer_width_agrees(&WIDTHS[5]);
}

/// The C words have no arithmetic, only conversions, and their 64 bits are
/// where a signed and an unsigned reading of one word differ.
#[test]
fn the_c_words_convert_alike() {
    let casts: Vec<String> = ["(as int a)".to_string()].into_iter().chain(casts()).collect();
    let defs = format!(
        "(defun cl ((a c-long)) string {})\n(defun cu ((a c-ulong)) string {})\n(defun il ((n int)) string {})",
        formatted(&casts),
        formatted(&casts),
        formatted(&casts.iter().map(|c| c.replace(" a)", " n)")).collect::<Vec<_>>()),
    );
    let (mut calls, mut literal) = (Vec::new(), Vec::new());
    for v in ["-2147483648", "-1", "0", "1", "255", "2147483647"] {
        calls.push(format!("(unsafe (cl (as c-long (the i32 {}))))", v));
        literal.push(formatted(&casts.iter().map(|c| c.replace(" a)", &format!(" (as c-long (the i32 {})))", v))).collect::<Vec<_>>()));
        calls.push(format!("(unsafe (cu (as c-ulong (the i32 {}))))", v));
        literal.push(formatted(&casts.iter().map(|c| c.replace(" a)", &format!(" (as c-ulong (the i32 {})))", v))).collect::<Vec<_>>()));
    }
    for n in ["-18446744073709551616", "-9223372036854775809", "-9223372036854775808", "-4611686018427387905", "-1", "0", "4294967296", "4611686018427387904", "9223372036854775807", "9223372036854775808", "18446744073709551615", "18446744073709551616"] {
        calls.push(format!("(il {})", n));
        literal.push(formatted(&casts.iter().map(|c| c.replace(" a)", &format!(" (the int {}))", n))).collect::<Vec<_>>()));
    }
    agree("c words", &defs, &["cl", "cu", "il"], &calls, &literal);
}

fn float_width_agrees(t: &str) {
    let values = ["0.0", "-0.0", "1.0", "-1.5", "0.1", "2.5", "3.5", "100.25", "1.0e30", "-1.0e-30"];
    let binary: Vec<String> = ["(+ a b)", "(- a b)", "(* a b)", "(/ a b)", "(< a b)", "(<= a b)", "(> a b)", "(>= a b)", "(= a b)", "(/= a b)", "(max a b)", "(min a b)", "(expt a b)"]
        .iter()
        .map(|f| f.to_string())
        .collect();
    let unary: Vec<String> = [
        "(sqrt a)", "(floor a)", "(ceiling a)", "(round a)", "(truncate a)", "(sin a)", "(cos a)", "(tan a)", "(asin a)", "(acos a)", "(atan a)", "(sinh a)",
        "(cosh a)", "(tanh a)", "(asinh a)", "(acosh a)", "(atanh a)", "(exp a)", "(log a)", "(as f32 a)", "(as f64 a)", "(try-as f32 a)",
    ]
    .iter()
    .map(|f| f.to_string())
    .collect();
    let defs = format!("(defun bin ((a {t}) (b {t})) string {})\n(defun un ((a {t})) string {})", formatted(&binary), formatted(&unary));
    let (mut calls, mut literal) = (Vec::new(), Vec::new());
    for a in values {
        calls.push(format!("(un (the {} {}))", t, a));
        literal.push(formatted(&unary.iter().map(|f| folded(f, &format!("(the {} {})", t, a), "")).collect::<Vec<_>>()));
        for b in values {
            calls.push(format!("(bin (the {t} {a}) (the {t} {b}))"));
            literal.push(formatted(&binary.iter().map(|f| folded(f, &format!("(the {} {})", t, a), &format!("(the {} {})", t, b))).collect::<Vec<_>>()));
        }
    }
    agree(t, &defs, &["bin", "un"], &calls, &literal);
}

#[test]
fn f32_agrees() {
    float_width_agrees("f32");
}

#[test]
fn f64_agrees() {
    float_width_agrees("f64");
}
