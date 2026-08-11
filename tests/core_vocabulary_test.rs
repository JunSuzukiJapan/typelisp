//! The core IR's tag vocabulary, written down as executable examples.
//!
//! Two consumers get built against this shape — the evaluator and the bridge
//! that feeds the self-hosted island — before the checker starts producing it.
//! So the vocabulary has to be settled *first*, and settled somewhere that
//! fails loudly if it drifts. That is this file.
//!
//! Three properties, each catching something the others do not.
//!
//! **1. A core form reads back from its own printed text.** Core forms are
//! plain readable s-expressions with no shape the reader cannot express, so a
//! test anywhere can write an expected form as a string instead of building a
//! tree by hand, and `core::print` output can be pasted back into a test. This
//! catches printer/reader disagreement — `core::print` used to emit Scheme's
//! `#t`/`#f` while the reader takes `true`/`false`.
//!
//! On its own this property is weak, and it is worth saying why rather than
//! letting it look stronger than it is: *any* well-formed s-expression round
//! trips. It says nothing about whether these are the right tags. Changing an
//! example from `(int 42)` to `(int 43)` — or to a tag that does not exist —
//! still passes.
//!
//! **2. The vocabulary is closed.** Every tag used above appears in one
//! explicit list, and the list is asserted to be exactly the set used. A typo
//! or a quietly-invented tag fails here, and adding one becomes deliberate.
//!
//! **3. The shared tags are the island's tags.** The expression tags the bridge
//! is meant to pass through unchanged are checked against
//! `compiler::SOURCE`'s own `compile-value` dispatch. This is the one property
//! tied to a real consumer: it is what would catch the vocabulary drifting away
//! from the thing that has to accept it, and it is checked here rather than
//! after the switch — the point of building the consumers first.
//!
//! The top-level forms have no such counterpart to check against: the bridge
//! never expressed one, because `TopLevel` was a Rust enum that never reached
//! the island.
//!
//! # Where the representations go
//!
//! Settling the vocabulary against one consumer and then writing the other is
//! what turned this up: the island tags things the first draft had no way to
//! say. `src/compiler.rs` keeps compiled locals in untagged native registers,
//! so it needs a `Repr` at every point a value crosses between that world and
//! the tagged heap — and an *argument* is such a point (`(kind . form)`, read
//! by `compile-call-args` to decide the GC-root bookkeeping around it), as is
//! a struct field read or write, and as is a `match` scrutinee.
//!
//! The old bridge read all of these off the `Type` hanging on each AST node.
//! Nothing downstream of the checker has a `Type` any more, so each one is
//! written into the form instead — as a repr list beside the argument list
//! (`(call (f) () f (int int) (int 1) (int 2))`), the same parallel-list idiom
//! `defstruct`/`defenum` already use, rather than as a wrapper around every
//! argument.
//!
//! Deliberately *not* one `Repr` on every node: most nodes' own
//! representations are never asked for, and putting one everywhere would
//! recreate `Typed`'s "every node carries a type" shape under a new name. The
//! rule is that a representation appears exactly where a consumer reads one.

extern crate typelisp;
use typelisp::check::core;
use typelisp::{Heap, Reader};

/// Read one form, then assert it prints back to exactly the source text.
///
/// `gc_stress` is on: the reader conses, so every allocation collects, and a
/// form that survives to be printed is one whose intermediates were all rooted.
fn round_trips(src: &str) {
    let mut h = Heap::with_capacity(1 << 14);
    h.set_gc_stress(true);
    let r = Reader::new();
    let mut vs = r.read_all(&mut h, src).expect("read failed");
    assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
    let v = vs.pop().unwrap();
    assert_eq!(core::print(&h, v), src, "core form did not print back to its source");
}

/// Every example in one batch, so a drift in any single tag names itself.
///
/// Also records each example's head tag, so `the_vocabulary_is_closed` can
/// check the whole set against one explicit list rather than trusting that the
/// examples only use tags anyone meant to introduce.
fn all_round_trip(examples: &[&str]) {
    for src in examples {
        round_trips(src);
        record_tags(src);
    }
}

use std::collections::BTreeSet;
use std::sync::Mutex;

static SEEN_TAGS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The tag this example demonstrates: the head of its outermost form, and
/// nothing else.
///
/// Deliberately not a walk over every nested head. A core form contains plenty
/// of lists that are *data*, not nodes — a `home` path, a parameter list, a
/// `let` binding group, anything under `quote` — and their heads are not tags.
/// Telling them apart needs the grammar, which is the very thing this file is
/// trying to pin down, so instead every tag gets its own standalone example and
/// this stays exact.
fn record_tags(src: &str) {
    let tag: String =
        src[1..].chars().take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')').collect();
    assert!(!tag.is_empty(), "example {:?} has no head tag", src);
    SEEN_TAGS.lock().unwrap().insert(tag);
}

// ---- literals -----------------------------------------------------------

/// The self-describing values. `sym` and `quote` have no island counterpart —
/// the bridge resolves a symbol to an interned id and inlines quoted data —
/// but the evaluator needs both, so they are core tags.
#[test]
fn literals() {
    all_round_trip(&[
        "(int 42)",
        "(int -7)",
        "(float 1.5)",
        // Keeps its point, so it is not the same form as `(int 1)`.
        "(float 1.0)",
        "(bignum 123456789012345678901234567890)",
        "(ratio 1/3)",
        r"(char #\a)",
        "(bool true)",
        "(bool false)",
        r#"(str "hi")"#,
        "(sym foo)",
        "(unit)",
        // User data sits under `quote` and nowhere else, which is what keeps
        // it from ever being read as a node.
        "(quote (a b c))",
        "(quote ())",
    ]);
}

// ---- variables and globals ----------------------------------------------

/// A local is referenced by symbol identity. The island's `cellvar`/`cellset`
/// split is not core: whether a name is cell-boxed is a *compilation*
/// decision, so the bridge derives it rather than the checker declaring it.
///
/// A global carries three things because a global reference is three separate
/// facts: the name as written, the module it was written in, and the absolute
/// path it resolved to. The trailing tag is its `Repr`.
#[test]
fn variables_and_globals() {
    all_round_trip(&[
        "(var x)",
        "(set x (int 1))",
        // Written `counter`, in module `m`, resolved to `m::counter`, an int.
        "(global (counter) (m) m::counter int)",
        // A root-module global: `home` is the empty list, because the root has
        // zero segments and so cannot be spelled as a `Path`.
        "(global (n) () n sexpr)",
        "(set-global (counter) (m) m::counter int (int 5))",
    ]);
}

// ---- binding and control ------------------------------------------------

/// `let` carries each binding's `Repr` alongside its name, since that is what
/// the bridge needs to decide the binding's compiled kind. `progn` is not a
/// tag of its own — it is `(let () ...)`, a `let` with no bindings.
#[test]
fn binding_and_control() {
    all_round_trip(&[
        "(let ((x int (int 1))) (var x))",
        "(let ((x int (int 1)) (y sexpr (quote a))) (var y))",
        // progn
        "(let () (int 1) (int 2))",
        "(if (bool true) (int 1) (int 2))",
        "(loop (break))",
        "(break)",
        "(return)",
        "(return (int 3))",
        r#"(panic (str "boom"))"#,
    ]);
}

// ---- calls --------------------------------------------------------------

/// A call carries the same written/home/path triple a global does, and for the
/// same reason: diagnostics need the name as written, visibility needs the
/// module it was written in, and dispatch needs the resolved path. Then the
/// argument representations, then the arguments — one list per argument, in
/// the same order.
///
/// `assoc` carries one more: its own *result* representation, ahead of the
/// argument list. That is what tells the bridge which of the island's builtin
/// operations a call is, in the case where the receiver is absent —
/// `(vector::new)` is a `vector-op` and `(scope::new)` over LLVM handles is a
/// `native-scope` op, and with no receiver to look at, the result is the only
/// place that says so.
///
/// `fnref`/`methodref`/`compile-fn` have no island counterpart — the bridge
/// turns a function reference into a closure construction, and to build one it
/// needs the referenced function's parameter representations, since there is
/// no call site here to read them from. A `&rest` parameter is simply the last
/// entry (always `sexpr`), so no separate flag is needed.
#[test]
fn calls() {
    all_round_trip(&[
        "(call (f) () f (int int) (int 1) (int 2))",
        "(call (helper) (m) m::helper ())",
        // An instance method on `i64`, `true` meaning it takes a receiver,
        // returning an int, over two int arguments.
        "(assoc i64 + true () int (int int) (var a) (var b))",
        // A static associated function: no receiver.
        "(assoc point new false () struct (int int) (int 1) (int 2))",
        // A builtin whose result is what identifies it: an empty vector of
        // ints, which the bridge lowers to a `vector-op` rather than a method
        // call, because there is no compiled body to call.
        "(assoc vector new false () (vector int) ())",
        "(fnref (f) () f (int int))",
        "(methodref point new () (int int))",
        // The callee is a value, so the *return* representation rides on the
        // node — a compiled closure's result has no name to look a signature
        // up by. Then the argument representations, then the arguments.
        "(apply (var g) int (int) (int 1))",
        "(compile-fn (fn (f) () f))",
        "(compile-fn (method point new ()))",
    ]);
    // `compile-fn`'s two payload shapes, standalone for the same reason the
    // pattern tags are.
    all_round_trip(&["(fn (f) () f)", "(method point new ())"]);
}

// ---- data ---------------------------------------------------------------

/// `construct` names the type, the variant index, and whether the value is
/// mutable. Fields are read and written by index — the *name* was resolved at
/// check time and is not needed again.
///
/// It also spells out one representation per field. Those cannot be read back
/// from the type's own `defstruct`/`defenum`, because for a *generic* ADT the
/// definition has no answer: `Option`'s `Some` field is declared `T`, and a
/// type variable has no representation. The instantiation is known only at the
/// site — and a definition-keyed table could not be made to hold it either,
/// since monomorphization erases and `Maybe<i64>`/`Maybe<string>` share the one
/// path `Maybe`. The reprs are the *declared* field types with this site's type
/// arguments substituted in, never the argument expressions' own types, which
/// can be narrower.
///
/// `field-get`/`field-set` spell out the single representation they touch for a
/// related but distinct reason — they name only an *index*, and their object is
/// an arbitrary expression whose type the IR no longer carries.
#[test]
fn data() {
    all_round_trip(&[
        "(construct point 0 false (int int) (int 1) (int 2))",
        "(construct option 1 false (int) (int 9))",
        "(field-get (var p) 0 int)",
        "(field-set (var p) 1 int (int 5))",
    ]);
}

// ---- patterns -----------------------------------------------------------

/// A `match` names its scrutinee's representation, then its arms; an arm is
/// `(pattern body...)`.
///
/// The scrutinee representation is what a whole-value `(pat-bind x)` binds at,
/// since such a pattern names no type of its own. It is also how the island
/// classifies the scrutinee — tagged `sexpr`, enum box, or boxed struct — for
/// the tag test it emits, though there it is redundant: each `pat-ctor` names
/// its own type, and a nested sub-pattern's may differ from its parent's, so
/// the island reads the per-pattern one and ignores this.
///
/// A constructor pattern carries the type, the variant index, whether it is a
/// downcast, and one representation per field — the reading half of what
/// `construct` writes, and there for the same reason (see `data`): a generic
/// ADT's definition declares `T`, so only the site knows. This is the surviving
/// half of the AST's `Pattern::Ctor::field_types`; its companion `sexpr_fields`
/// is not here, being that same fact reduced to a single bit.
///
/// Note what this test does and does not fix: `all_round_trip` compares
/// `core::print` against the source text, so it pins the *tags* and these
/// written examples, not the field layout in general — a builder that emitted
/// its fields in another order would still round-trip. The layout is pinned by
/// the one builder per tag in `Checker` and the field indices its two consumers
/// read.
#[test]
fn patterns() {
    all_round_trip(&[
        "(match (var v) sexpr ((pat-wild) (int 0)))",
        "(match (var v) int ((pat-bind x) (var x)))",
        "(match (var v) sexpr ((pat-lit (int 1)) (int 10)) ((pat-wild) (int 0)))",
        "(match (var v) enum ((pat-ctor option 0 false ()) (int 0)) ((pat-ctor option 1 false (int) (pat-bind x)) (var x)))",
        // A downcast arm, for matching a trait object against a concrete type.
        "(match (var d) sexpr ((pat-ctor point 0 true (int int) (pat-bind p)) (var p)))",
        "(match (var d) sexpr ((pat-typetest point (pat-bind p)) (var p)))",
    ]);
    // Each pattern tag standalone as well: a pattern only ever appears nested
    // inside a `match` arm, and `record_tags` reads the outermost head only.
    all_round_trip(&[
        "(pat-wild)",
        "(pat-bind x)",
        "(pat-lit (int 1))",
        "(pat-ctor option 1 false (int) (pat-bind x))",
        "(pat-typetest point (pat-bind p))",
    ]);
}

// ---- functions ----------------------------------------------------------

/// A `lambda` lists its parameters with their representations, then its return
/// representation, then its body. `labels` is a list of the same shape, each
/// with a name.
#[test]
fn functions() {
    all_round_trip(&[
        "(lambda ((x int)) int (var x))",
        "(lambda () unit (unit))",
        "(labels ((go ((i int)) int (var i))) (call (go) () go (int) (int 1)))",
    ]);
}

// ---- trait objects ------------------------------------------------------

/// `dyn-new` boxes a concrete value together with the vtable for the trait it
/// is being seen through, plus the supertrait tables an upcast may switch to —
/// interned here, where the concrete type is still known.
#[test]
fn trait_objects() {
    all_round_trip(&[
        // The trailing representation is the boxed value's own. It decides
        // whether the island roots the value across the boxing call, and a
        // struct and an enum differ there — so it cannot be derived from the
        // concrete type's *name*, which is all the rest of the node carries.
        r#"(dyn-new "point" shape ((point area)) () struct (var p))"#,
        r#"(dyn-new "point" shape ((point area)) ((drawable ((point draw)))) struct (var p))"#,
        "(dyn-upcast drawable (var d))",
        // The trailing repr list and arguments are an ordinary call's, the
        // receiver included — a dynamic call reaches the island as a call
        // through a vtable slot, so its arguments cross the same boundary.
        "(dyn-call shape area 0 ((point area)) (dyn) (var d))",
        "(dyn-value (var d))",
    ]);
}

// ---- top level ----------------------------------------------------------

/// New vocabulary: `TopLevel` was a Rust enum, so the bridge never had to
/// express one and the island has no counterpart for any of these.
///
/// Note what is *absent*: there is no node for a generic template. The checker
/// keeps those as raw reader forms and emits only monomorphized
/// specializations, so the two "skip a type-erased body" special cases the
/// evaluator carries today have nothing left to skip.
#[test]
fn top_level() {
    all_round_trip(&[
        "(defun m::add ((a int) (b int)) int true (assoc i64 + true () int (int int) (var a) (var b)))",
        "(defun m::nothing () unit false (unit))",
        "(defmethod point area false ((self struct)) int true (field-get (var self) 0 int))",
        // params, then whether it takes `&rest`, then the `&optional`/`&key`
        // structure, then `pub`, then the body.
        "(defmacro m::when (c body) true (1 () ()) false (quote ()))",
        "(defvar m::counter int true true (int 0))",
        // A struct publishes its fields' representations, which is where the
        // bridge reads a pattern's field kinds from.
        "(defstruct point (int int))",
        "(defenum m::color (red green blue) (() () ()))",
        "(defenum option (none some) (() (sexpr)))",
        "(module m (defvar m::x int false false (int 1)))",
        "(use m::helper other::helper)",
        r#"(load "lib.typl")"#,
        "(expr (int 42))",
    ]);
}

// ---- the scaffolding marker ---------------------------------------------

/// Present only while the checker is being converted, and counted as the
/// progress measure. Included here so it is part of the vocabulary rather than
/// something the evaluator meets unannounced.
#[test]
fn the_unlowered_marker() {
    round_trips(r#"(unlowered "CheckIf")"#);
}

// ---- the properties with teeth ------------------------------------------

/// Every core tag, in one place.
///
/// Split by who consumes it, because that is the distinction that matters when
/// the bridge is written: a `SHARED_WITH_ISLAND` tag should pass through
/// unchanged, an `EXPR_ONLY` tag has to be translated into something the island
/// knows, and a `TOP_LEVEL` tag never reaches the island at all.
const SHARED_WITH_ISLAND: &[&str] = &[
    "int", "float", "bignum", "ratio", "char", "bool", "str", "unit", "var", "set", "global",
    "set-global", "let", "lambda", "labels", "call", "assoc", "apply", "if", "loop", "break",
    "return", "panic", "match", "construct", "field-get", "field-set", "dyn-new", "dyn-upcast",
    "dyn-call", "dyn-value",
];

/// Core tags with no island counterpart: the bridge turns each into something
/// else. A symbol becomes an interned id, quoted data is inlined, a function
/// reference becomes a closure construction, and the pattern tags are consumed
/// by `compile-match` rather than by `compile-value`.
const EXPR_ONLY: &[&str] = &[
    "sym",
    "quote",
    "fnref",
    "methodref",
    "compile-fn",
    "fn",
    "method",
    "pat-wild",
    "pat-bind",
    "pat-lit",
    "pat-ctor",
    "pat-typetest",
];

/// New vocabulary — `TopLevel` was a Rust enum, so none of this ever reached
/// the island.
const TOP_LEVEL: &[&str] =
    &["defun", "defmethod", "defmacro", "defvar", "defstruct", "defenum", "module", "use", "load", "expr"];

/// Scaffolding, gone by the end of the conversion.
const SCAFFOLDING: &[&str] = &["unlowered"];

/// The examples above use exactly the declared vocabulary — no more, no less.
///
/// Catches a typo or a quietly-invented tag (an example using a tag nobody
/// declared), and equally an entry declared but never demonstrated. Runs last
/// by name so the example tests have populated `SEEN_TAGS` first; `--test-threads=1`
/// makes that ordering real.
#[test]
fn zz_the_vocabulary_is_closed() {
    let declared: BTreeSet<String> = SHARED_WITH_ISLAND
        .iter()
        .chain(EXPR_ONLY)
        .chain(TOP_LEVEL)
        .chain(SCAFFOLDING)
        .map(|s| s.to_string())
        .collect();

    // Re-run every example so this test does not depend on the others having
    // run, then compare the whole set at once.
    literals();
    variables_and_globals();
    binding_and_control();
    calls();
    data();
    patterns();
    functions();
    trait_objects();
    top_level();
    record_tags(r#"(unlowered "CheckIf")"#);

    let seen = SEEN_TAGS.lock().unwrap().clone();
    let undeclared: Vec<&String> = seen.difference(&declared).collect();
    assert!(undeclared.is_empty(), "examples use tags not in the declared vocabulary: {:?}", undeclared);
    let undemonstrated: Vec<&String> = declared.difference(&seen).collect();
    assert!(
        undemonstrated.is_empty(),
        "declared vocabulary has tags with no example: {:?}",
        undemonstrated
    );
}

/// Every tag declared as shared is one `compile-value` actually dispatches on.
///
/// This is the property tied to a real consumer. The island is the thing that
/// has to accept the bridge's output, and its dispatch is a flat `icond` over
/// `(equal s "TAG")` clauses in `compiler::SOURCE` — so the accepted set can be
/// read straight out of the source it is compiled from, with no guessing.
///
/// Checked now rather than after the checker switches, because that is the
/// whole point of building the consumers first: finding out the island will not
/// take a tag is cheap while the tree is green and expensive once it is not.
#[test]
fn every_shared_tag_is_one_the_island_dispatches_on() {
    let source = typelisp::compiler::SOURCE;
    let accepted: BTreeSet<&str> = source
        .match_indices("(equal s \"")
        .filter_map(|(i, m)| {
            let rest = &source[i + m.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect();

    assert!(
        accepted.len() > 30,
        "expected to find the island's whole tag dispatch, found only {:?}",
        accepted
    );

    let missing: Vec<&&str> = SHARED_WITH_ISLAND.iter().filter(|t| !accepted.contains(**t)).collect();
    assert!(
        missing.is_empty(),
        "declared shared with the island, but `compile-value` does not dispatch on: {:?}",
        missing
    );
}
