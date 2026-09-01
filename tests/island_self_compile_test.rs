//! interp-closure removal Stage 2: the self-hosted compiler island
//! (`compiler.rs`'s `SOURCE`) must itself be AOT-compilable now that its
//! `llvm-*`/native-`Scope<V>` builtins have a compiled representation
//! (Stage 1). These tests drive the ordinary `(compile ...)` machinery over
//! the island's own top-level `defun`s — the same mechanism a user program
//! compiles through — and prove (a) every island function compiles without
//! error, and (b) the compiled central entry point `compile-function`,
//! applied to a small function's AST, produces a module whose JIT'd code
//! runs and agrees with the interpreter.
//!
//! This is the in-process rehearsal of what Stage 3's `bootstrap` binary
//! does to a fixed artifact: if `(compile compile-function)` works here, the
//! island can be lifted to native code.

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn fresh() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<Value, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// Every top-level `defun` in `compiler.rs`'s `SOURCE`, in declaration
/// order. Kept in sync with the island by the
/// `island_defun_list_is_exhaustive` guard below (which re-derives the list
/// from the interpreter's own function table and fails if this misses one),
/// so a future island edit that adds/removes a `defun` can't silently
/// desync this test. Island `defmacro`s go in [`ISLAND_MACROS`] instead —
/// see its doc comment for why the two must not be merged.
const ISLAND_DEFUNS: &[&str] = &[
    "compile-tag-bits-test",
    "compile-box-kind-test",
    "compile-ctor-pattern",
    "compile-sexpr-field",
    "compile-tag-struct-field",
    "bind-params",
    "bind-captures",
    "pow2",
    "compute-sexpr-mask",
    "name-is-borrowed?",
    "form-is-borrowed?",
    "retain-bindings",
    "release-bindings",
    "push-sexpr-root",
    "pop-sexpr-root",
    "pop-sexpr-roots",
    "push-permanent-sexpr-root",
    "new-env",
    "new-fn-env",
    "new-acc-table",
    "int-binop-shim-call",
    "int-unary-shim-call",
    "int-native-method?",
    "int-receiver-type?",
    "sexpr-native-method?",
    "string-native-method?",
    "char-native-method?",
    "bool-native-method?",
    "symbol-native-method?",
    "float-native-method?",
    "float-receiver-type?",
    "str-lt-call",
    "bignum-native-method?",
    "ratio-native-method?",
    "bignum-cmp-call",
    "ratio-cmp-call",
    "bignum-binop-call",
    "bignum-unary-call",
    "ratio-binop-call",
    "ratio-unary-call",
    "raising-binop-call",
    "sexpr-list-length",
    "sexpr-list-length-i32",
    "bind-let-values",
    "unroot-let-sexpr-values",
    "compile-sexpr-tag-test",
    "compile-pattern-guard",
    "compile-box-tag-test",
    "compile-box-field",
    "compile-struct-field",
    "compile-option-type-name",
    "compile-value",
    "compile-unit",
    "compile-int",
    "compile-char",
    "compile-bool",
    "compile-float",
    "compile-str",
    "store-str-chars",
    "compile-bignum-literal",
    "compile-ratio-literal",
    "compile-var",
    "compile-cellvar",
    "resolve-value",
    "compile-env-args",
    "compile-escaping-env-args",
    "compile-assoc",
    "compile-assoc-user",
    "compile-llvm-op",
    "compile-dyn-new",
    "compile-dyn-upcast",
    "compile-dyn-value",
    "compile-dyn-call",
    "check-unwind",
    "emit-unwind-onward",
    "emit-static-exit-onward",
    "emit-direct-call",
    "emit-direct-call-with-env",
    "emit-closure-apply",
    "emit-rt-call",
    "compile-call-args",
    "compile-apply",
    "compile-call",
    "compile-apply-indirect",
    "compile-if-branch",
    "compile-if",
    "compile-let-values",
    "compile-let-body",
    "compile-let",
    "compile-lambda",
    "declare-labels-siblings",
    "compile-labels-bodies",
    "compile-labels",
    "compile-loop",
    "compile-loop-body",
    "compile-break",
    "compile-return",
    "compile-set",
    "compile-cellset",
    "compile-pattern-test",
    "compile-sexpr-instance-test",
    "compile-ctor-subpatterns",
    "compile-match",
    "compile-match-arms",
    "compile-construct",
    "compile-construct-sym",
    "compile-construct-sym-args",
    "compile-construct-wk-sym",
    "compile-construct-path",
    "compile-construct-path-segs",
    "compile-construct-boxed-struct",
    "compile-construct-boxed-struct-fields",
    "compile-construct-box",
    "compile-construct-sexpr",
    "compile-field-get",
    "compile-field-set",
    "compile-vector-op",
    "compile-hashtable-op",
    "compile-global",
    "compile-set-global",
    "compile-global-init",
    "compile-catch",
    "compile-throw",
    "compile-unwind-protect",
    "compile-panic",
    "compile-function",
];

/// Every top-level `defmacro` in `compiler.rs`'s `SOURCE`.
///
/// Kept separate from [`ISLAND_DEFUNS`] rather than merged into it, because
/// the two lists are used for opposite things. A macro's body is stored in
/// the very same `fns` table a `defun`'s is (`Interp::exec`'s `Defmacro`
/// arm), so `island_defun_list_is_exhaustive` re-derives it alongside the
/// `defun`s and has to account for it here — but it must stay *out* of
/// `ISLAND_DEFUNS`, which `every_island_defun_compiles` feeds to
/// `(compile ...)`: a macro has no type signature and is rejected outright
/// (`compile: "icond" has no type signature (is it a defmacro?)`). Nothing is
/// lost by not compiling one — a macro body only ever runs interpreted, at
/// expansion time, which for the island is during the check of `SOURCE`
/// itself, before any island bitcode exists to run it compiled.
const ISLAND_MACROS: &[&str] = &[];

#[test]
fn every_island_defun_compiles() {
    // One fresh session per function so an earlier compile's already-
    // compiled state can't mask a later one's independent compilability
    // (and so a failure names exactly one culprit).
    for name in ISLAND_DEFUNS {
        let (mut h, mut chk, mut interp) = fresh();
        let src = format!("(compile {})", name);
        if let Err(e) = eval_in(&mut h, &mut chk, &mut interp, &src) {
            panic!("island defun `{}` failed to compile: {:?}", name, e);
        }
    }
}

#[test]
fn compile_function_compiles_transitively() {
    // The central entry point pulls in every `compile-*` sibling (its own
    // `labels` block) and helper it calls — one `(compile ...)` exercising
    // the whole SCC transitive-compile path over the island at once.
    let (mut h, mut chk, mut interp) = fresh();
    eval_in(&mut h, &mut chk, &mut interp, "(compile compile-function)").expect("compile-function should compile");
}

#[test]
fn island_defun_list_is_exhaustive() {
    // Guards `ISLAND_DEFUNS` against island edits: re-derive the set of
    // top-level island `defun`s the interpreter actually loaded and check
    // this test's hard-coded list still covers exactly them. `load_compiler`
    // runs after `load_prelude`, so subtract the prelude's own functions by
    // diffing two interpreters.
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();

    let mut prelude_only = Interp::new();
    load_prelude(&mut h, &mut chk, &mut prelude_only);
    let prelude_fns = prelude_only.function_names();

    let mut with_island = Interp::new();
    load_prelude(&mut h, &mut chk, &mut with_island);
    load_compiler(&mut h, &mut chk, &mut with_island);
    let all_fns = with_island.function_names();

    let mut island: Vec<String> = all_fns.into_iter().filter(|n| !prelude_fns.contains(n)).collect();
    island.sort();

    let mut expected: Vec<String> =
        ISLAND_DEFUNS.iter().chain(ISLAND_MACROS.iter()).map(|s| s.to_string()).collect();
    expected.sort();

    assert_eq!(island, expected, "ISLAND_DEFUNS/ISLAND_MACROS are out of sync with compiler.rs's SOURCE");
}
