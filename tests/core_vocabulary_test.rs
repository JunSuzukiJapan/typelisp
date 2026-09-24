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
//! example from `(int-any-width 42)` to `(int-any-width 43)` — or to a tag that
//! does not exist —
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
//! (`(call (f) () f (int-any-width int-any-width) (int-any-width 1)
//! (int-any-width 2))`), the same parallel-list idiom
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
        "(int-any-width 42)",
        "(int-any-width -7)",
        // An `int` literal: the same number under the tag that says "tagged
        // word", which the island cannot read off the number.
        "(int 42)",
        "(int -4611686018427387904)",
        "(float-any-width 1.5)",
        // Keeps its point, so it is not the same form as `(int-any-width 1)`.
        "(float-any-width 1.0)",
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
        "(set x (int-any-width 1))",
        // Written `counter`, in module `m`, resolved to `m::counter`, an int.
        "(global (counter) (m) m::counter int-any-width)",
        // A root-module global: `home` is the empty list, because the root has
        // zero segments and so cannot be spelled as a `Path`.
        "(global (n) () n sexpr)",
        "(set-global (counter) (m) m::counter int-any-width (int-any-width 5))",
    ]);
}

// ---- binding and control ------------------------------------------------

/// `let` carries each binding's `Repr` alongside its name, since that is what
/// the bridge needs to decide the binding's compiled kind. `progn` is not a
/// tag of its own — it is `(let () ...)`, a `let` with no bindings.
#[test]
fn binding_and_control() {
    all_round_trip(&[
        "(let ((x int-any-width (int-any-width 1))) (var x))",
        "(let ((x int-any-width (int-any-width 1)) (y sexpr (quote a))) (var y))",
        // progn
        "(let () (int-any-width 1) (int-any-width 2))",
        "(if (bool true) (int-any-width 1) (int-any-width 2))",
        // `loop` leads with its own `Repr` — the type a `break`/`return`
        // carries out — where every other node in this vocabulary trails one.
        // The body is variadic, so a trailing field could not be told from one
        // more statement. `unit` here: the only exit is a value-less `break`.
        "(loop unit (break))",
        "(break)",
        "(return)",
        "(return (int-any-width 3))",
        // The named escape. The name is a `(str ...)` node and the body is a
        // single form — the checker wraps its sequence in a binding-less
        // `let` first, so nothing downstream re-implements sequencing. The
        // trailing `Repr` is the block's own value, for `loop`'s reason.
        r#"(block (str "b") (let () (int-any-width 1)) int-any-width)"#,
        r#"(return-from (str "b"))"#,
        r#"(return-from (str "b") (int-any-width 3))"#,
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
        "(call (f) () f (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))",
        "(call (helper) (m) m::helper ())",
        // An instance method on `int`, `true` meaning it takes a receiver,
        // returning an int, over two int arguments.
        "(assoc int + true () int-any-width (int-any-width int-any-width) (var a) (var b))",
        // A static associated function: no receiver.
        "(assoc point new false () struct (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))",
        // A builtin whose result is what identifies it: an empty vector of
        // ints, which the bridge lowers to a `vector-op` rather than a method
        // call, because there is no compiled body to call.
        "(assoc vector new false () (vector int-any-width) ())",
        "(fnref (f) () f (int-any-width int-any-width))",
        "(methodref point new () (int-any-width int-any-width))",
        // The callee is a value, so the *return* representation rides on the
        // node — a compiled closure's result has no name to look a signature
        // up by. Then the argument representations, then the arguments.
        "(apply (var g) int-any-width (int-any-width) (int-any-width 1))",
        "(compile-fn (fn (f) () f))",
        "(compile-fn (method point new ()))",
        // `(go CALL)` — the call is made in a task rather than here. It wraps
        // a whole call node, so its own shape is the thinnest in the
        // vocabulary; the bridge is what takes the wrapped node apart.
        "(go (call (f) () f (int-any-width) (int-any-width 1)))",
        // The callee can be a value, in which case the wrapped node is an
        // `apply` and its callee is a form like any argument.
        "(go (apply (var g) int-any-width (int-any-width) (int-any-width 1)))",
        // `(thread CALL)` — `go`'s shape, for a task on an OS thread of its
        // own.
        "(thread (call (f) () f (int-any-width) (int-any-width 1)))",
        // `(select ARM...)`. Each arm is a plain list rather than a node —
        // `("recv" VAR KEY CHAN BODY)`, `("send" KIND CHAN VALUE BODY)`,
        // `("else" BODY)` — because the fields are metadata (a tag, a bound
        // name, a type key, a field kind) with one form each. The channel
        // expressions are `var`s: the checker bound every operand in a `let`
        // around this node, so nothing here evaluates anything that can stop.
        "(select (\"recv\" v \"option<int>\" (var c) (var v)) (\"else\" (int-any-width 0)))",
        "(select (\"send\" 1 (var c) (var x) (int-any-width 0)))",
    ]);
    // `compile-fn`'s two payload shapes, standalone for the same reason the
    // pattern tags are.
    all_round_trip(&["(fn (f) () f)", "(method point new ())"]);
}

/// The REPL tool forms that name a definition: `(trace ...)` and
/// `(untrace ...)`.
///
/// They carry the *same* `(fn ..)`/`(method ..)` payloads `compile-fn` does,
/// because they ask a name the same question — which single body is this? —
/// and share the checker's resolution for it. Unlike `compile-fn` they take
/// any number, including none: `(trace)` is CL's "tell me what is traced" and
/// `(untrace)` its untrace-everything.
///
/// No island counterpart, for the reason `compile-fn` has none: these act on
/// the interpreter's own environment, so a compiled program has nothing for
/// them to act on.
#[test]
fn repl_tools() {
    all_round_trip(&[
        "(trace (fn (f) () f))",
        "(trace (fn (f) () f) (method point new ()))",
        "(trace)",
        "(untrace (fn (f) () f))",
        "(untrace)",
        // `step` is the odd one: it names no definition, it wraps a *form*.
        // The form keeps its own type, so the node carries nothing else.
        "(step (call (f) () f ()))",
        // `disassemble-fn` carries the same payload plus one flag: `true`
        // asks for LLVM IR rather than host assembly. A literal, because the
        // answer decides what to emit.
        "(disassemble-fn (fn (f) () f) false)",
        "(disassemble-fn (method point new ()) true)",
    ]);
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
/// since monomorphization erases and `Maybe<int>`/`Maybe<string>` share the one
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
        "(construct point 0 false (int-any-width int-any-width) (int-any-width 1) (int-any-width 2))",
        "(construct option 1 false (int-any-width) (int-any-width 9))",
        "(field-get (var p) 0 int-any-width)",
        "(field-set (var p) 1 int-any-width (int-any-width 5))",
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
        "(match (var v) sexpr ((pat-wild) (int-any-width 0)))",
        // The empty list. Its own node rather than `Sexpr`'s variant 0:
        // the empty list outlives `nil` (docs/dev/null-elimination-plan.md).
        "(match (var v) sexpr ((pat-empty) (int-any-width 0)) ((pat-wild) (int-any-width 1)))",
        // `Option<Sexpr>`'s `(some P)`: reject the empty list, then match
        // `P` against the same word (the niche makes them one value).
        "(match (var v) sexpr ((pat-nonempty (pat-bind x)) (var x)) ((pat-empty) (int-any-width 0)))",
        "(match (var v) int-any-width ((pat-bind x) (var x)))",
        "(match (var v) sexpr ((pat-lit (int-any-width 1)) (int-any-width 10)) ((pat-wild) (int-any-width 0)))",
        // A value pattern: the test is an ordinary expression — here the
        // `equals` call a `"a"` literal pattern lowers to — and the symbol is
        // the name that expression reads the value under test through.
        r#"(match (var s) str ((pat-guard $match-scrut (assoc string equals true () bool (str str) (var $match-scrut) (str "a"))) (int-any-width 1)) ((pat-wild) (int-any-width 0)))"#,
        "(match (var v) enum ((pat-ctor option 0 false ()) (int-any-width 0)) ((pat-ctor option 1 false (int-any-width) (pat-bind x)) (var x)))",
        // A downcast arm, for matching a trait object against a concrete type.
        "(match (var d) sexpr ((pat-ctor point 0 true (int-any-width int-any-width) (pat-bind p)) (var p)))",
        "(match (var d) sexpr ((pat-typetest point (pat-bind p)) (var p)))",
    ]);
    // Each pattern tag standalone as well: a pattern only ever appears nested
    // inside a `match` arm, and `record_tags` reads the outermost head only.
    all_round_trip(&[
        "(pat-wild)",
        "(pat-empty)",
        "(pat-nonempty (pat-bind x))",
        "(pat-bind x)",
        "(pat-lit (int-any-width 1))",
        "(pat-guard $match-scrut (bool true))",
        "(pat-ctor option 1 false (int-any-width) (pat-bind x))",
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
        "(lambda ((x int-any-width)) int-any-width (var x))",
        "(lambda () unit (unit))",
        "(labels ((rec ((i int-any-width)) int-any-width (var i))) (call (rec) () rec (int-any-width) (int-any-width 1)))",
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
        "(defun m::add ((a int-any-width) (b int-any-width)) int-any-width true (assoc int + true () int-any-width (int-any-width int-any-width) (var a) (var b)))",
        "(defun m::nothing () unit false (unit))",
        "(defmethod point area false ((self struct)) int-any-width true (field-get (var self) 0 int-any-width))",
        // params, then whether it takes `&rest`, then the `&optional`/`&key`
        // structure, then `pub`, then the body.
        "(defmacro m::when (c body) true (1 () ()) false (quote ()))",
        "(defvar m::counter int-any-width true true (int-any-width 0))",
        // A struct publishes its fields' representations, which is where the
        // bridge reads a pattern's field kinds from.
        "(defstruct point (int-any-width int-any-width))",
        "(defenum m::color (red green blue) (() () ()))",
        "(defenum option (none some) (() (sexpr)))",
        "(module m (defvar m::x int-any-width false false (int-any-width 1)))",
        // A C function, declared. Fields 3-5 are `defun`'s own; field 2 is the
        // library to look the symbol up in (`()` for "the running process"),
        // and the last two spell the C types, which the `REPR`s beside them
        // cannot — `int-any-width` is all six widths at once.
        r#"(defffi m::c-abs "abs" () ((a0 int-any-width)) int-any-width true (i32) i32)"#,
        r#"(defffi c-sqrt "sqrt" "m" ((a0 f64)) f64 true (f64) f64)"#,
        "(use m::helper other::helper)",
        r#"(load "lib.typl")"#,
        "(expr (int-any-width 42))",
    ]);
}

// ---- the properties with teeth ------------------------------------------

/// Every core tag, in one place.
///
/// Split by who consumes it, because that is the distinction that matters when
/// the bridge is written: a `SHARED_WITH_ISLAND` tag should pass through
/// unchanged, an `EXPR_ONLY` tag has to be translated into something the island
/// knows, and a `TOP_LEVEL` tag never reaches the island at all.
const SHARED_WITH_ISLAND: &[&str] = &[
    "int-any-width", "int", "float-any-width", "bignum", "ratio", "char", "bool", "str", "unit", "var", "set", "global",
    "set-global", "let", "lambda", "labels", "call", "assoc", "apply", "if", "loop", "break",
    "return", "block", "return-from", "panic", "match", "construct", "field-get", "field-set",
    "dyn-new", "dyn-upcast", "dyn-call", "dyn-value", "select",
];

/// Core tags with no island counterpart: the bridge turns each into something
/// else. A symbol becomes an interned id, quoted data is inlined, a function
/// reference becomes a closure construction, `go` becomes a `let` of its
/// operands around a `spawn` of a thunk (the island only ever sees the
/// `suspend` the thunk is handed to) and `thread` the same around a
/// `spawn-thread`, and the pattern tags are consumed by
/// `compile-match` rather than by `compile-value`.
const EXPR_ONLY: &[&str] = &[
    "sym",
    "go",
    "thread",
    "quote",
    "fnref",
    "methodref",
    "compile-fn",
    "trace",
    "untrace",
    "step",
    "disassemble-fn",
    "fn",
    "method",
    "pat-wild",
    "pat-empty",
    "pat-nonempty",
    "pat-bind",
    "pat-lit",
    "pat-guard",
    "pat-ctor",
    "pat-typetest",
];

/// New vocabulary — `TopLevel` was a Rust enum, so none of this ever reached
/// the island.
const TOP_LEVEL: &[&str] = &[
    "defun", "defmethod", "defmacro", "defvar", "defstruct", "defenum", "defffi", "module", "use", "load", "expr",
];

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
        .map(|s| s.to_string())
        .collect();

    // Re-run every example so this test does not depend on the others having
    // run, then compare the whole set at once.
    literals();
    variables_and_globals();
    binding_and_control();
    calls();
    repl_tools();
    data();
    patterns();
    functions();
    trait_objects();
    top_level();

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
/// has to accept the bridge's output, and its dispatch is a flat `case` over
/// the tag in `compiler::SOURCE` — so the accepted set can be read straight out
/// of the source it is compiled from, with no guessing.
///
/// The keys are bare symbols in clause-head position (`(int-any-width ...)`) since the
/// dispatch compares tag *symbols* rather than their names, so the scan walks
/// the `case` form by paren depth and takes the first token of each clause.
/// It has now been through two shapes before this one — `(equal s "TAG")`
/// across the whole SOURCE, then `("int-any-width" ...)` string keys — and both times it
/// was the `accepted.len() > 30` assertion below that caught the change rather
/// than the test quietly passing on an empty set. Keep that assertion.
///
/// Checked now rather than after the checker switches, because that is the
/// whole point of building the consumers first: finding out the island will not
/// take a tag is cheap while the tree is green and expensive once it is not.
#[test]
fn every_shared_tag_is_one_the_island_dispatches_on() {
    let source = typelisp::compiler::SOURCE;
    let head = "(defun compile-value ";
    let start = source.find(head).expect("`compile-value` not found in the island SOURCE") + head.len();
    let rest = &source[start..];
    let body = match rest.find("\n(defun ") {
        Some(end) => &rest[..end],
        None => rest,
    };
    let case_head = "(case (sexpr-car e)";
    let case_at = body.find(case_head).expect("`compile-value`'s tag dispatch is not a `case` on (sexpr-car e)");
    // Walk the `case` form. Depth 1 is the form itself; every group opening at
    // depth 2 is a clause, and its first token is the key. The key expression
    // `(sexpr-car e)` opens at depth 2 as well and is simply the first one, so
    // it is skipped by position rather than by matching its text.
    let mut accepted: BTreeSet<&str> = BTreeSet::new();
    let mut depth = 0usize;
    let mut groups_at_clause_depth = 0usize;
    let mut chars = body[case_at..].char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            ';' => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            // A `;` inside a string does not start a comment, and a paren
            // inside one is not structure.
            '"' => {
                for (_, c) in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                }
            }
            '(' => {
                depth += 1;
                if depth == 2 {
                    groups_at_clause_depth += 1;
                    let from = i + 1;
                    let mut to = from;
                    while let Some((j, c)) = chars.peek() {
                        if c.is_whitespace() || *c == '(' || *c == ')' {
                            to = *j;
                            break;
                        }
                        to = *j + c.len_utf8();
                        chars.next();
                    }
                    if groups_at_clause_depth > 1 {
                        accepted.insert(&body[case_at + from..case_at + to]);
                    }
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    accepted.remove("else");

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
