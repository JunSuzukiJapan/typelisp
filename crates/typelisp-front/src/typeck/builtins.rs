//! The built-in definitions: the types, functions and methods implemented in
//! Rust, written into the registry before anything is checked.
//!
//! A type's methods are written in the type's own parameters
//! ([`BuiltinType::params`]); each method gets them as its own when it is
//! defined (`docs/dev/checker-rebuild-plan.md` 7.3). No built-in method has
//! type parameters of its own.

mod llvm;
mod primitive;
mod system;

use crate::typeck::registry::{AdtKind, Variant};
use crate::types::{internal_ns, prelude_ns, std_module, ParamOwner, Path, Type, TypeParam};
use typelisp_mem::symbols::{intern, intern_in, member_symbol, NsId};
use typelisp_mem::{wk, StdModule};

use super::registry::{FnDef, MethodDef, Registry, TypeDef, TypeEntry};
use super::subst::Subst;

/// Write every built-in definition into `reg`, with the modules they are in,
/// and make the library's definitions what `std` and `std::prelude` bring in
/// ([`super::library::reexport`]).
pub(crate) fn define(reg: &mut Registry) {
    let std_ns = NsId::STD;
    let internal = internal_ns();
    let random = std_module("random");
    let time = std_module("time");
    let repl = std_module("repl");
    let env = std_module("env");
    let thread = std_module("thread");
    let sync = std_module("sync");
    let symbol = std_module("symbol");
    let process = std_module("process");
    let cmp = std_module("cmp");
    let reader = std_module("reader");
    let eval = std_module("eval");
    let num = std_module("num");
    let fmt = std_module("fmt");
    let option = StdModule::OPTION.ns();
    let result = StdModule::RESULT.ns();
    let sexpr_ns = StdModule::SEXPR.ns();
    let primitive_ns = StdModule::PRIMITIVE.ns();
    let modules = [
        NsId::ROOT, std_ns, internal, random, time, repl, env, thread, sync, symbol, process, cmp, reader, eval, num, fmt, option, result,
        StdModule::VEC.ns(), sexpr_ns, StdModule::COLLECTIONS.ns(), StdModule::FS.ns(), StdModule::NET.ns(), primitive_ns, NsId::CORE,
        prelude_ns(),
    ];
    for ns in modules {
        reg.modules.insert(ns, ());
    }

    // `Option`'s and `Result`'s constructors are their modules' own bare
    // names too, which `std::prelude` offers every module, as Rust's prelude
    // brings in `Some`/`None`/`Ok`/`Err`.
    let variants = option_type(reg);
    bare_constructors(reg, option, Path::builtin(wk::OPTION), &variants, &[]);
    let variants = result_type(reg);
    bare_constructors(reg, result, Path::builtin(wk::RESULT), &variants, &[]);
    for ty in BUILTIN_ERROR_TYPES {
        error_type(reg, ty);
    }
    // `Sexpr`'s data constructors (`int`/`str`/...) are bare names without a
    // `use`, so `(int 5)` datum literals stay writable, all but `cons`:
    // `(cons a b)` is the free `cons<T,U>` pair function, not the `Sexpr`
    // cons cell. The constructors are `std::primitive`'s while the type is
    // `std::sexpr`'s: several are spelled as the primitive types they hold
    // (`int`, `f64`, `char` ...), and a name `std::prelude` offers is one
    // path, so the constructor and the primitive type have to be one
    // module's two definitions of it.
    let variants = sexpr(reg);
    bare_constructors(reg, primitive_ns, Path::builtin(wk::SEXPR), &variants, &["cons"]);
    hashtable(reg);
    vector(reg);
    task(reg);
    thread_type(reg);
    chan(reg);
    scope(reg);
    llvm::define(reg);
    primitive::define(reg);

    // `random-state` (CL's `random-state`): a mutable PRNG stream, its
    // actual bit-twiddling done in Rust (`interp::eval_random_state_next` —
    // the fixed-width xorshift step isn't expressible in typelisp, which has
    // no bitwise operators). None of these has a natural receiver to
    // dispatch on, so all four stay free functions. `random`/
    // `make-random-state`/`random-state-p` are ordinary `defun`s in the
    // prelude built on top of these.
    function(reg, internal, "make-random-state-fresh", vec![], Type::RandomState);
    function(reg, internal, "random-state-copy", vec![Type::RandomState], Type::RandomState);
    function(reg, internal, "random-state-next", vec![Type::RandomState, Type::Int], Type::Int);
    // `seed-random-state` (SBCL's `sb-ext:seed-random-state`, not in the
    // standard): the stream a given integer names, for a caller who wants a
    // run to be reproducible. CL itself has no portable way to seed —
    // `make-random-state` takes `nil`/`t`/a state and nothing else — so this
    // deliberately does *not* borrow a standard name for a non-standard
    // operation. The integer-to-state map lives in
    // `typelisp_rt::seeded_random_state`, which is where the reason it isn't
    // the identity is written down.
    function(reg, random, "seed-random-state", vec![Type::Int], Type::RandomState);
    // `get-universal-time`/`get-internal-real-time` (CLHS 25.1): wall-clock
    // and monotonic-ish timers, respectively. `get-universal-time` counts
    // seconds since 1900-01-01 UTC (CL's epoch, 2208988800s before the Unix
    // epoch); `get-internal-real-time` counts `internal-time-units-per-second`
    // (a prelude `defvar`, 1_000_000 — i.e. microseconds) since an arbitrary
    // process-start reference point, matching CL's own "units are
    // implementation-defined, only the ratio between two calls means
    // anything" contract.
    //
    // Both return a *struct*, not a number: neither count fits an `i32`
    // (1900-epoch seconds passed 2^31 in 1968; microseconds overflow in
    // about 35 minutes) and this language has no 64-bit-wide integer type —
    // see `Type::is_integer`. Splitting the count the way the domain already
    // splits it costs nothing: `decode-universal-time`'s first step is
    // exactly this division, and `time`'s subtraction is two field
    // subtractions.
    function(reg, time, "get-universal-time", vec![], universal_time());
    function(reg, time, "get-internal-real-time", vec![], internal_time());
    // `get-internal-run-time` (CLHS 25.1): the same unit and the same
    // struct, but CPU time (`getrusage`) instead of elapsed time. The pair is
    // the point — a program blocked on I/O shows a large difference, and that
    // difference is exactly what a single "real time" figure hides.
    function(reg, time, "get-internal-run-time", vec![], internal_time());
    // `heap-info` returns the prelude's `heap-info` struct — `room`'s report,
    // built on the runtime's side of the boundary the way `universal-time`
    // is, because the statistics belong to the `Heap` and not to the
    // checker.
    function(reg, repl, "heap-info", vec![], heap_info());
    // `parse-float`: untrusted-text numeric parsing
    // (`docs/dev/cl-equivalence-catalog.md` §2.1) — `Result`, not a panic,
    // since the input is runtime text the caller doesn't control (unlike a
    // source literal, which the reader and the checker already validate
    // before this code ever runs).
    function(reg, num, "parse-float", vec![Type::Str], result_of(Type::F64, error_ty(wk::PARSE_FLOAT_ERROR)));
    system::define(reg, env, thread, time, internal, reader);
    // `read`: parses one `Sexpr` form out of a string with the same reader
    // `typl`/the REPL use for source text (`crate::read::Reader::read`) —
    // CL's `read-from-string`.
    function(reg, reader, "read", vec![Type::Str], result_of(option_of(sexpr_ty()), error_ty(wk::READ_ERROR)));
    // `read-datum-at`: one datum from a string starting at a character index,
    // paired with where reading stopped — CL's `read-from-string` and its
    // second return value, which this language has no multiple values to
    // carry. The prelude's `read-from-string` /
    // `read-from-string-preserving-whitespace` are this with the two
    // `preserve` settings and a default `start`; the primitive keeps a name
    // of its own because the CL one is the prelude's.
    let datum_and_end = Type::Named(Path::in_ns(sexpr_ns, "cons-cell"), vec![option_of(sexpr_ty()), Type::Int]);
    function(reg, internal, "read-datum-at", vec![Type::Str, Type::Int, Type::Bool], result_of(datum_and_end, error_ty(wk::READ_ERROR)));
    // `eval`: type-checks and runs a runtime `Sexpr` against the current
    // global environment, CL-style (`Interp::eval_form`). Sees all globals
    // but not the caller's lexical locals; a definition form registers
    // immediately. Result is a `Sexpr` (the value, or a definition's name
    // symbol); malformed/ill-typed input is `Err`, not a panic.
    let form_result = result_of(option_of(sexpr_ty()), error_ty(wk::EVAL_ERROR));
    function(reg, eval, "eval", vec![option_of(sexpr_ty())], form_result.clone());
    // `macroexpand-1`/`macroexpand`: what the checker does to a macro call,
    // made available to a program (CL's own, and the only way to see a
    // `defmacro`'s output without reading the checker's mind).
    //
    // CL returns a second value saying whether anything expanded; there are
    // no multiple values here, so `macroexpand-1` answers `Option<Sexpr>` —
    // `none` *is* "not a macro call", and it carries strictly more than CL's
    // boolean, since the caller cannot mistake a macro that expands to itself
    // for a non-macro. `macroexpand` repeats until `none` and answers the
    // final form, as CL's does.
    function(reg, eval, "macroexpand-1", vec![option_of(sexpr_ty())], form_result.clone());
    function(reg, eval, "macroexpand", vec![option_of(sexpr_ty())], form_result);
    // The pretty printer's user-callable layout operators (CLHS 22.2.1),
    // minus the stream argument typelisp has no streams for. They act on the
    // logical block the `pprint-logical-block` special form opened
    // (`Interp`'s `PrettySession`) and are no-ops outside one, exactly as
    // CL's are when the stream is not a pretty stream — which is why they are
    // plain functions here and need no special-form treatment: their
    // arguments are CL's own keywords, i.e. ordinary `Symbol` values.
    //
    // `pprint-pop` returns `()` once the block's list is exhausted;
    // `pprint-list-exhausted` is the predicate to test first (the prelude
    // macro `pprint-exit-if-list-exhausted` is the CL-spelled wrapper).
    function(reg, fmt, "pprint-newline", vec![Type::Symbol], Type::Unit);
    function(reg, fmt, "pprint-indent", vec![Type::Symbol, Type::Int], Type::Unit);
    function(reg, fmt, "pprint-tab", vec![Type::Symbol, Type::Int, Type::Int], Type::Unit);
    function(reg, fmt, "pprint-pop", vec![], option_of(sexpr_ty()));
    function(reg, fmt, "pprint-list-exhausted", vec![], Type::Bool);
    // The `Sexpr` navigation layer: `sexpr-cons`/`sexpr-car`/`sexpr-cdr`/
    // `sexpr-consp`/`sexpr-null`/`sexpr-atom`. `cons`/`car`/`cdr` are the
    // generic `cons<T,U>` pair's (the prelude's free `cons` and
    // `cons-cell<A,B>`'s `car`/`cdr` field accessors), so the self-hosting
    // compiler (`compiler.rs`) and the prelude's macros navigate `Sexpr`
    // structure with these instead, without `match`, which does not take a
    // `Sexpr` apart. The predicates read the runtime tag directly
    // (`Value::is_cons`/`is_empty`).
    let s = option_of(sexpr_ty());
    function(reg, sexpr_ns, "sexpr-cons", vec![s.clone(), s.clone()], s.clone());
    function(reg, sexpr_ns, "sexpr-car", vec![s.clone()], s.clone());
    function(reg, sexpr_ns, "sexpr-cdr", vec![s.clone()], s.clone());
    function(reg, sexpr_ns, "sexpr-consp", vec![s.clone()], Type::Bool);
    function(reg, sexpr_ns, "sexpr-null", vec![s.clone()], Type::Bool);
    function(reg, sexpr_ns, "sexpr-atom", vec![s.clone()], Type::Bool);
    // The `Sexpr` payload extractors (`Interp::eval_builtin`): typed field
    // readers, so the island needs no `match`. Each panics on a tag
    // mismatch. `sexpr-symp` is the tag predicate its non-panicking caller
    // (`form-is-borrowed?`) needs to branch on a `Sym` node — a peer of
    // `sexpr-consp`/`sexpr-null`/`sexpr-atom`.
    function(reg, sexpr_ns, "sexpr-int", vec![s.clone()], Type::Int);
    function(reg, sexpr_ns, "sexpr-f64", vec![s.clone()], Type::F64);
    function(reg, sexpr_ns, "sexpr-f32", vec![s.clone()], Type::F32);
    // One extractor per integer type, not one per machine word: the five
    // narrow widths are separate `Sexpr` variants carrying separate boxes
    // (`BoxedObj::Narrow`), and each of these returns exactly its own.
    for (name, ty) in [
        ("sexpr-i8", Type::I8),
        ("sexpr-i16", Type::I16),
        ("sexpr-u8", Type::U8),
        ("sexpr-u16", Type::U16),
        ("sexpr-u32", Type::U32),
        ("sexpr-i32", Type::I32),
    ] {
        function(reg, sexpr_ns, name, vec![s.clone()], ty);
    }
    function(reg, sexpr_ns, "sexpr-bool", vec![s.clone()], Type::Bool);
    function(reg, sexpr_ns, "sexpr-char", vec![s.clone()], Type::Char);
    function(reg, sexpr_ns, "sexpr-str", vec![s.clone()], Type::Str);
    function(reg, sexpr_ns, "sexpr-sym-name", vec![s.clone()], Type::Str);
    function(reg, sexpr_ns, "sexpr-symp", vec![s], Type::Bool);
    // `equal`/`equalp`: structural equality (CL `equal`/`equalp`), Rust
    // builtins (`Interp::eval_builtin`'s `sexpr_equal`/`sexpr_equalp`). The
    // per-scalar `equal`/`equalp` *methods* (string/char/int/...) are
    // separate and resolved first when the receiver is one of those types —
    // these free ones are for everything else (`case` expands to
    // `(equal ..)`).
    //
    // **Generic, not `Sexpr`-typed.** With concrete `Option<Sexpr>`
    // parameters both arguments would widen into S-expression data
    // independently, so any two values of any two types would compare —
    // `(equalp (pa::new 1) (pb::new "z"))` would type-check and answer
    // `false`, contradicting the rule the language enforces everywhere
    // else: `(equalp 1 "a")` is a type error, because `i32` has its own
    // `equal` method and that method demands both sides be `i32`. One type
    // parameter used twice forces the two arguments to agree; the runtime
    // compares `Value`s structurally whatever they hold.
    for name in ["equal", "equalp"] {
        generic_function(reg, cmp, name, &["T"], |[t]| (vec![t.clone(), t], Type::Bool));
    }
    // `symbol->string`/`intern`: the only bridges between the interned
    // `symbol` handle and its textual name. They are Rust builtins
    // (`Interp::eval_builtin`) rather than typelisp because they touch the
    // symbol intern table directly.
    function(reg, symbol, "symbol->string", vec![Type::Symbol], Type::Str);
    // CL's `intern`: `(intern NAME NAMESPACE)` is the symbol `NAME` reads as
    // in the module `NAMESPACE` names (`"keyword"` for a keyword) — an
    // existing one when the module can see one, a new one of that module
    // otherwise. Written with one argument, it interns in the module the call
    // is written in, which the checker fills in: there is no `*package*` for
    // CL's default to read.
    function(reg, symbol, "intern", vec![Type::Str, Type::Str], Type::Symbol);
    // CL's `keywordp`: owned by the keyword module. A Rust builtin because the
    // owner is not reachable from the language — a keyword's name says
    // nothing about it being one.
    function(reg, symbol, "keywordp", vec![Type::Symbol], Type::Bool);
    // `exit`: process termination (cl-equivalence-catalog.md §1.2). Unlike
    // `panic`/`unreachable`/`todo` (which unwind through `EvalError::Panic`,
    // a typelisp-level signal), this needs an actual OS call
    // (`std::process::exit`, in `Interp::eval_builtin`'s `"exit"` arm), so it
    // stays an ordinary Rust builtin rather than a `defmacro`. `Never` return
    // type, same as `panic`, so it satisfies any expected type.
    function(reg, process, "exit", vec![Type::Int], Type::Never);
    // `compile-file`: AOT-compiles an independent source file to a native
    // executable (`Interp::eval_builtin`'s `"compile-file"` arm /
    // `compile::aot::compile_file`), from the source path to the output
    // path. `compile` is a special form instead: its argument is an
    // unevaluated name.
    function(reg, std_ns, "compile-file", vec![Type::Str, Type::Str], Type::Bool);
    // `dump`: writes this session's whole environment — the units it was
    // loaded from, plus one for what it has defined since — to a file
    // `typl --image` can start from (`compile::dump::dump_image`).
    function(reg, std_ns, "dump", vec![Type::Str], Type::Bool);

    super::library::reexport(reg);
}

/// The built-in types whose values the runtime builds and prints without a
/// definition form ever running: `Option`, `Result` and the concrete error
/// types, whose variants' names a printed value needs, and `Vector`,
/// `Task`, `Thread` and `Chan`, the built-in structs. Written by the same
/// functions [`define`] calls, for the interpreter to seed its own scope tree
/// with (`Interp::new`).
pub(crate) fn runtime_types() -> Vec<(Path, TypeDef)> {
    let mut reg = Registry::default();
    option_type(&mut reg);
    result_type(&mut reg);
    for ty in BUILTIN_ERROR_TYPES {
        error_type(&mut reg, ty);
    }
    vector(&mut reg);
    task(&mut reg);
    thread_type(&mut reg);
    chan(&mut reg);
    let mut types: Vec<(Path, TypeDef)> = reg.types.iter().filter_map(|(p, _)| Some((*p, reg.type_def(p)?.clone()))).collect();
    types.sort_by_key(|(p, _)| p.to_string());
    types
}

/// `Option<T> = Some(T) | None`, its variants returned.
fn option_type(reg: &mut Registry) -> Vec<Variant> {
    sum_type(reg, Path::builtin(wk::OPTION), &["T"], |[t]| {
        vec![Variant { name: member_symbol("some"), fields: vec![t] }, Variant { name: member_symbol("none"), fields: vec![] }]
    })
}

/// `Result<T, E> = Ok(T) | Err(E)`, its variants returned.
fn result_type(reg: &mut Registry) -> Vec<Variant> {
    sum_type(reg, Path::builtin(wk::RESULT), &["T", "E"], |[t, e]| {
        vec![Variant { name: member_symbol("ok"), fields: vec![t] }, Variant { name: member_symbol("err"), fields: vec![e] }]
    })
}

/// The concrete built-in error types, by the vocabulary symbol each is named
/// by — the one list every layer reads: [`define`] registers them, the
/// interpreter builds their values (`crate::eval::interp`'s `result_err`),
/// and both the interpreter and `core_bridge` classify a type as an enum by
/// consulting [`is_builtin_error_type`]. Like `Option`/`Result` they must be
/// recognizable outside the registry, since neither of those two layers
/// holds one to look a definition up in.
///
/// `FileError` is CLHS's `file-error`, covering every failure the
/// stream/file layer can report: `open` on a missing file, `delete-file` on a
/// read-only directory, and the wrong-direction/closed-stream program errors
/// that reach a `Result`-typed entry point. CL splits these across
/// `file-error`/`stream-error`; one concrete type is enough here because the
/// *message* is what a program can act on — dispatching on a condition
/// *class* would need a condition system, which this language deliberately
/// does not have.
///
/// `NetError` is what every `net-*` builtin fails with: a refused connect, an
/// unknown host, a reset connection. Its own type rather than `FileError`
/// because a program handling both — a server that serves files — wants to
/// tell a missing file from a dropped client, and the *type* is how it does
/// that without parsing a message.
pub(crate) const BUILTIN_ERROR_TYPES: [u32; 6] =
    [wk::PARSE_INT_ERROR, wk::PARSE_FLOAT_ERROR, wk::READ_ERROR, wk::EVAL_ERROR, wk::FILE_ERROR, wk::NET_ERROR];

/// Whether `p` names one of [`BUILTIN_ERROR_TYPES`].
pub(crate) fn is_builtin_error_type(p: &Path) -> bool {
    crate::types::path_is_builtin_any(p, &BUILTIN_ERROR_TYPES)
}

/// The concrete error type of every fallible built-in, one per failure
/// source: `ParseIntError` (`parse-int`), `ParseFloatError`
/// (`parse-float`), `ReadError` (`read`), `EvalError` (`eval`), `FileError`
/// (every stream and file operation), `NetError` (every socket operation) —
/// modeled on Rust's std, where `Error` is a *trait* and each operation
/// returns its own concrete error. `Error` is accordingly not a type in this
/// language at all: it is the prelude trait these implement, so code that
/// wants to hold any of them uniformly says `Result<T, :dyn Error>` — the
/// counterpart of Rust's `Box<dyn Error>`.
///
/// Each is a single-variant sum carrying the message string, the same shape
/// (and therefore the same heap representation, `match` destructuring, and
/// trait-object boxing) a user's own `(defenum MyError (my-error string))`
/// would have: nothing about a built-in error type is privileged. Type name
/// and variant name coincide, so `(match e ((ParseIntError m) m))` reads as
/// one name. [`crate::eval::interp`]'s `result_err` builds these values at
/// run time and must agree with the variant order (a single variant, index
/// 0).
fn error_type(reg: &mut Registry, ty: u32) {
    let path = Path::builtin(ty);
    sum_type(reg, path, &[], |[]| vec![Variant { name: member_symbol(path.last_segment()), fields: vec![Type::Str] }]);
}

/// The built-in `Sexpr` sum type (the result type of `read`), its variants
/// returned.
///
/// `Sexpr = i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | Char | Bool | Sym |
/// Str | Cons(Option<Sexpr>, Option<Sexpr>) | Bignum | Ratio | Path`: every
/// S-expression **but** the empty list, which is `Option<Sexpr>`'s `none`.
/// So the type S-expression data is passed around as is `Option<Sexpr>`, not
/// `Sexpr`, and `car`/`cdr` return it.
///
/// Both `cons` fields are `Option<Sexpr>`, the car as much as the cdr: an
/// element position has to hold the empty list too, or `'(a () b)` could
/// not be written.
///
/// The `nil` variant is a **placeholder, not a constructor**. Writing it is
/// refused with a message pointing at `none`; the slot survives only so that
/// the variant numbers (`crate::sexpr_variant`) — which are burned into the
/// island's IR and into compiled code — keep their values. Renumbering them
/// buys nothing and would invalidate every artifact.
///
/// The numbers and names come from `crate::sexpr_variant`; this only says
/// what each variant holds.
fn sexpr(reg: &mut Registry) -> Vec<Variant> {
    use crate::sexpr_variant as sv;
    let variants: Vec<Variant> =
        (0..sv::COUNT).map(|v| Variant { name: member_symbol(sv::NAMES[v]), fields: sexpr_variant_fields(v) }).collect();
    let mut def = BuiltinType::new(reg, Path::builtin(wk::SEXPR), AdtKind::Sum, &[]);
    def.set_variants(variants.clone());
    // `eq`/`eql`: true CL identity on `Sexpr` — comparing the underlying
    // `mem::Value` directly (`crate::eval::interp`'s `eval_builtin_method`)
    // means two `Cons` cells or two `Str`s are equal only if they're the
    // *same* heap cell/slot (real identity), while immediate scalars
    // (`Int`/`Char`/`Bool`/`Sym`/...) compare by value — correct under `eq`
    // for these since there is no separate boxed representation to diverge
    // from (`Sym` in particular is correct since symbols are always
    // interned: same name -> same id). `eql` is a plain alias of `eq` here:
    // CL's `eql` only adds same-type/value number and character comparisons
    // beyond `eq`'s identity, and every immediate scalar in this
    // representation already satisfies that trivially. Two separately built
    // `Str` `Sexpr`s with equal content are correctly *not* `eq`/`eql` —
    // `equal`/`equalp` compare content instead.
    let s = option_of(sexpr_ty());
    for name in ["eq", "eql"] {
        def.instance(name, vec![s.clone(), s.clone()], Type::Bool);
    }
    variants
}

/// The fields of `Sexpr`'s variant number `v`.
fn sexpr_variant_fields(v: usize) -> Vec<Type> {
    use crate::sexpr_variant as sv;
    match v {
        // Never constructible — see `sexpr`'s doc comment. The checker
        // rejects it by name.
        sv::NIL => vec![],
        // `int`, the language's integer — a fixnum or a bignum box, the
        // value's own word, so construction and extraction are both the
        // identity. This slot was `i32` while that was the default integer;
        // the number is burned into the island's IR, the name is what
        // changed, and `i32` went to the end.
        sv::INT => vec![Type::Int],
        sv::F64 => vec![Type::F64],
        sv::CHAR => vec![Type::Char],
        sv::BOOL => vec![Type::Bool],
        sv::SYM => vec![Type::Symbol],
        sv::STR => vec![Type::Str],
        sv::CONS => vec![option_of(sexpr_ty()), option_of(sexpr_ty())],
        // Retired with the `bignum` type: a bignum box is an `int`. Never
        // constructible and never matchable — the checker refuses it by
        // name, as it does `nil` — and kept only so nothing after it
        // renumbers.
        sv::RETIRED_BIGNUM => vec![Type::Int],
        sv::RATIO => vec![Type::Ratio],
        // A `::`-qualified path (e.g. `dep::head`), the reader's
        // `Value::Path` (`crate::mem::Value`) made matchable. Its single
        // field is a proper `Sexpr` list of `sym`s (its segments, in written
        // order) — the same shape a quoted `'(dep head)` list already has —
        // not a re-stringified `"dep::head"`: a segment once split out of the
        // reader's token stays a real `Symbol`, never gets flattened back
        // into text, and the list's own length/`car`/`cdr` give the segment
        // count and per-segment access for free through the ordinary
        // list-processing machinery, instead of needing a second parse.
        sv::PATH => vec![option_of(sexpr_ty())],
        // From here on: the widths that used to be folded into `i32` and
        // `f64` above. Appended rather than inserted, because the variant
        // numbers are burned into the island's IR and into compiled code,
        // and because `Repr::field_kind` reads off the same numbering.
        //
        // Eight numeric variants where there were two. `Sexpr` is the one
        // place a value's type is not written down anywhere else, so it is
        // the one place every width has to be its own variant — folding `u8`
        // and `i32` into one `int` did not merely lose the name, it let
        // `(the u32 4000000000)` come back out as an `i32` holding a number
        // no `i32` can hold.
        sv::F32 => vec![Type::F32],
        sv::I8 => vec![Type::I8],
        sv::I16 => vec![Type::I16],
        sv::U8 => vec![Type::U8],
        sv::U16 => vec![Type::U16],
        sv::U32 => vec![Type::U32],
        // Boxed like the five above it, since `int` took the bare fixnum
        // word. Appended for the usual reason.
        sv::I32 => vec![Type::I32],
        // `#(..)` and `#nA(..)`. The payload is the collection box itself,
        // so construction and extraction are both the identity, like `str`.
        sv::VECTOR => vec![vector_of(option_of(sexpr_ty()))],
        sv::ARRAY => vec![Type::Named(Path::in_ns(std_module("array"), "Array"), vec![option_of(sexpr_ty())])],
        // `#{..}`. A tuple's type depends on its arity, so the payload is a
        // fresh vector of the elements rather than the box itself.
        sv::TUPLE => vec![vector_of(option_of(sexpr_ty()))],
        _ => unreachable!("Sexpr has {} variants, not {}", sv::COUNT, v + 1),
    }
}

/// `HashTable<K, V>`: a builtin (Rust-implemented) mutable hash map, with no
/// constructors of its own (built via the static `new`, not
/// pattern-matched). The runtime implementation lives in
/// `eval_builtin_method` in `crate::eval::interp`.
fn hashtable(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::HASHTABLE), AdtKind::Sum, &["K", "V"]);
    let [k, v] = def.params();
    let table = Type::Named(Path::builtin(wk::HASHTABLE), vec![k.clone(), v.clone()]);
    def.associated("new", vec![], table.clone());
    // `get`/`set`/`remove` are **not here**. They are prelude `defmethod`s
    // (`(:where (Hash K))`), written on the five bucket primitives below —
    // because looking a key up means hashing it and comparing it, and both
    // of those are the key type's own methods, written in typelisp. A
    // builtin that did the whole lookup would have to call back into the
    // interpreter to ask; a prelude method already is the interpreter, and
    // compiles like any other definition.
    //
    // The bucket primitives take the hash the caller computed. `i` is an
    // index the caller got from `bucket-count`; `bucket-put` also accepts
    // the count itself, which appends. Not `public`: these are the layer
    // under `get`/`set`/`remove`, not API.
    def.hidden_instance("bucket-count", vec![table.clone(), Type::Int], Type::Int);
    def.hidden_instance("bucket-key", vec![table.clone(), Type::Int, Type::Int], k.clone());
    def.hidden_instance("bucket-value", vec![table.clone(), Type::Int, Type::Int], v.clone());
    def.hidden_instance("bucket-put", vec![table.clone(), Type::Int, Type::Int, k.clone(), v.clone()], Type::Unit);
    def.hidden_instance("bucket-delete", vec![table.clone(), Type::Int, Type::Int], Type::Unit);
    def.instance("count", vec![table.clone()], Type::Int);
    def.instance("clear", vec![table.clone()], Type::Unit);
    // `keys`/`values`/`entries`: a Rust `HashMap` has no stable, resumable
    // cursor the way `Vector<T>`'s own index-based iterator does, so each
    // call snapshots the table's current contents into a fresh `Vector`
    // rather than exposing a live cursor — `eval_builtin_method`'s
    // `"hashtable"` arm builds these. `entries`' element type is
    // `cons-cell<K,V>` (`prelude.rs`'s generic `car`/`cdr` product), which
    // `hashtable-iter<K,V>` (also `prelude.rs`) walks to give
    // `HashTable<K,V>` an `Iter` impl on top of this snapshot, the same way
    // `vector-iter<T>` walks `Vector<T>`.
    def.instance("keys", vec![table.clone()], vector_of(k.clone()));
    def.instance("values", vec![table.clone()], vector_of(v.clone()));
    let entry = Type::Named(Path::in_ns(StdModule::SEXPR.ns(), "cons-cell"), vec![k, v]);
    def.instance("entries", vec![table], vector_of(entry));
}

/// `Vector<T>`: a builtin growable sequence. Reuses the same boxed-struct
/// representation `defstruct` instances get (`BoxedObj::Struct`) rather than
/// a dedicated one of its own — a `Vector<T>`'s fields are simply treated as
/// variable-length instead of the fixed, name-indexed layout a `defstruct`'s
/// fields have (see `eval_builtin_method`'s `"vector"` arm, where the
/// runtime implementation lives).
fn vector(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::VECTOR), AdtKind::Struct, &["T"]);
    let [t] = def.params();
    let vector = vector_of(t.clone());
    def.associated("new", vec![], vector.clone());
    def.instance("push", vec![vector.clone(), t.clone()], Type::Unit);
    def.instance("get", vec![vector.clone(), Type::Int], t.clone());
    def.instance("set", vec![vector.clone(), Type::Int, t.clone()], Type::Unit);
    def.instance("len", vec![vector.clone()], Type::Int);
    def.instance("pop", vec![vector], option_of(t));
}

/// `Task<T>`, the type `(task (f ...))` yields.
///
/// Spelled once, here, rather than at the checker's `task` — a type's
/// identity is its whole path, and two places writing it is two places to
/// get it wrong.
pub(crate) fn task_of(t: Type) -> Type {
    Type::Named(Path::in_ns(std_module("thread"), "Task"), vec![t])
}

/// `Task<T>`: a handle on a task that is running — what `(task (f ...))`
/// returns.
///
/// A builtin rather than a `defstruct` in the prelude because `task` is a
/// core form: the checker builds this type where it checks `task`, so it has
/// to exist in contexts the prelude was never loaded into (the island's own
/// tests).
///
/// The runtime value is a boxed struct holding the scheduler's id for the
/// task and **no readable fields** — the id is not a number a program may
/// invent or forge, the same reasoning `file-stream`'s opaque handle
/// follows. Nothing constructs one but `task`, so there is no `new` here —
/// only `wait`.
fn task(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::in_ns(std_module("thread"), "Task"), AdtKind::Struct, &["T"]);
    let [t] = def.params();
    // `(wait t)` — the task's result, waiting for it if it has not finished.
    //
    // May be asked as many times as you like: the answer is kept, so a
    // handle passed to several places gives each of them the same value.
    // Unlike Rust's `JoinHandle::join`, waiting does not consume the handle
    // — a `Task<T>` is an ordinary value.
    //
    // The evaluator intercepts this before `eval_builtin_method`, because a
    // builtin answers with a value or an error and has no way to say "not
    // yet".
    def.instance("wait", vec![task_of(t.clone())], t);
}

/// `Thread<T>`, the type `(thread (f ...))` yields — spelled once, here, for
/// [`task_of`]'s reason.
pub(crate) fn thread_of(t: Type) -> Type {
    Type::Named(Path::in_ns(std_module("thread"), "Thread"), vec![t])
}

/// `Thread<T>`: a handle on a task that runs on an OS thread of its own —
/// what `(thread (f ...))` returns. [`task`]'s twin: the same runtime value
/// (a box holding the scheduler's id, under its own key) and no readable
/// fields.
///
/// Two static functions ride on it, about OS threads rather than about one
/// `Thread<T>`. Their signatures never mention `T`, so a call leaves it
/// unconstrained, which a builtin's signature is allowed to.
fn thread_type(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::in_ns(std_module("thread"), "Thread"), AdtKind::Struct, &["T"]);
    let [t] = def.params();
    // `(join th)` — the thread's result, waiting for it. `wait`'s semantics
    // exactly: asked any number of times, it gives each asker the same
    // value, and it stops the asking *task*, not the OS thread under it.
    def.instance("join", vec![thread_of(t.clone())], t);
    // `(Thread::current-id)` — a number naming the OS thread running the
    // caller, unique within the process.
    def.associated("current-id", vec![], Type::Int);
    // `(Thread::available-parallelism)` — how many threads the machine runs
    // at once; what `TYPELISP_THREADS` defaults to.
    def.associated("available-parallelism", vec![], Type::Int);
}

/// `Chan<T>`, the channel type.
///
/// Spelled once, here, for the same reason [`task_of`] is: a type's identity
/// is its whole path.
pub(crate) fn chan_of(t: Type) -> Type {
    Type::Named(Path::in_ns(std_module("sync"), "Chan"), vec![t])
}

/// `Chan<T>`: a channel between tasks — Go's `chan T`.
///
/// Builtin rather than a `defstruct` for the reason [`task`] is: the checker
/// builds this type where it checks `select`, so it has to exist in contexts
/// the prelude was never loaded into.
///
/// The runtime value is a boxed struct holding the scheduler's id for the
/// channel and **no readable fields** — `file-stream`'s opaque handle again.
/// The type argument appears in no field.
///
/// Every one of the six is intercepted by the evaluator before
/// `eval_builtin_method`, and lowered as a *suspension* in compiled code,
/// because all six need the scheduler: four of them only to read its table,
/// and `send`/`recv` to park the task as well.
fn chan(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::in_ns(std_module("sync"), "Chan"), AdtKind::Struct, &["T"]);
    let [t] = def.params();
    let chan = chan_of(t.clone());
    // `(the Chan<i32> (Chan::new 0))` — unbuffered; `(Chan::new 16)` buffers
    // 16. The capacity is **required**: an omitted `&optional` with no
    // default is an `Option<i32>` the callee has to take apart, and no
    // builtin in this language has one. Go spells the same two cases
    // `make(chan int)` and `make(chan int, 16)`; here the second number is
    // always written.
    def.associated("new", vec![Type::Int], chan.clone());
    // Waits for room. A closed channel is a panic, not a `Result`: sending on
    // one is a program bug, which is the line `docs/dev/language-design.md`
    // §7.1 draws (and Go's own).
    def.instance("send", vec![chan.clone(), t.clone()], Type::Unit);
    // `none` once the channel is closed **and** drained — which is what
    // makes this the same shape as `Iter::next`, and so what lets `doiter`
    // run a channel with no new machinery (`impl Iter Chan<T>` in the
    // prelude).
    def.instance("recv", vec![chan.clone()], option_of(t));
    // Closing twice is a panic, again Go's rule and for the same reason.
    // Resolved by receiver type, so it does not collide with the `Stream`
    // trait's `close` — `Chan<T>` deliberately does not implement `Stream`.
    def.instance("close", vec![chan.clone()], Type::Unit);
    // CL's and Go's `len`/`cap`: how many values are buffered right now, and
    // how many fit.
    def.instance("len", vec![chan.clone()], Type::Int);
    def.instance("cap", vec![chan], Type::Int);
}

/// `Scope<V>`: a builtin (Rust-implemented) stack of `String`-keyed frames —
/// the (typelisp-hosted) compiler body's (`src/compiler.rs`) replacement for
/// a bare `HashTable` as `env`/`fn-env`, modeling the "list of scopes" name
/// resolution `labels`/`let` need (see that module's doc comment). No
/// constructors of its own (built via the static `new`); the runtime
/// implementation lives in `eval_builtin_method` in `crate::eval::interp`.
fn scope(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::SCOPE), AdtKind::Sum, &["V"]);
    let [v] = def.params();
    let scope = Type::Named(Path::builtin(wk::SCOPE), vec![v.clone()]);
    def.associated("new", vec![], scope.clone());
    // Shares every existing frame (by reference, not by copying their
    // contents) plus pushes one fresh empty frame on top — the single
    // operation a `labels` def's own new lexical scope needs to start from
    // every enclosing scope's frames (see `compile-labels`'s doc comment for
    // why a *fresh* top frame, not the shared ones themselves, must receive
    // that def's own parameter bindings).
    def.instance("clone-frames", vec![scope.clone()], scope.clone());
    def.instance("push-frame", vec![scope.clone()], Type::Unit);
    def.instance("pop-frame", vec![scope.clone()], Type::Unit);
    def.instance("get", vec![scope.clone(), Type::Str], option_of(v.clone()));
    def.instance("set", vec![scope, Type::Str, v], Type::Unit);
}

/// The type `Option<t>`.
pub(crate) fn option_of(t: Type) -> Type {
    Type::Named(Path::builtin(wk::OPTION), vec![t])
}

/// The type `Result<t, e>`.
fn result_of(t: Type, e: Type) -> Type {
    Type::Named(Path::builtin(wk::RESULT), vec![t, e])
}

/// The type `Vector<t>`.
fn vector_of(t: Type) -> Type {
    Type::Named(Path::builtin(wk::VECTOR), vec![t])
}

/// The type `Sexpr`.
fn sexpr_ty() -> Type {
    Type::Named(Path::builtin(wk::SEXPR), vec![])
}

/// One of the concrete error types ([`BUILTIN_ERROR_TYPES`]), as a `Type` —
/// the `E` in the `Result<T, E>` returned by the fallible builtin that
/// raises it.
fn error_ty(ty: u32) -> Type {
    Type::Named(Path::builtin(ty), vec![])
}

/// `universal-time` (a prelude `defstruct`): a CL universal time split into
/// whole days since 1900-01-01 and seconds within that day. The return type
/// of `get-universal-time`/`file-modified-date` — see the comment at
/// `get-universal-time`'s definition for why a struct and not a number.
fn universal_time() -> Type {
    Type::Named(Path::in_ns(std_module("time"), "universal-time"), vec![])
}

/// `internal-time` (a prelude `defstruct`): `get-internal-real-time`'s
/// reading, as whole seconds plus microseconds within that second.
/// [`universal_time`]'s monotonic counterpart.
fn internal_time() -> Type {
    Type::Named(Path::in_ns(std_module("time"), "internal-time"), vec![])
}

/// `heap-info` (a prelude `defstruct`): what `(heap-info)` reports and what
/// `room` prints — [`universal_time`]'s shape, for the same reason (the
/// runtime builds it, so the checker only has to name it).
fn heap_info() -> Type {
    Type::Named(Path::in_ns(std_module("repl"), "heap-info"), vec![])
}

/// A built-in free function `name` of the module `ns`: public, implemented
/// in Rust, with required parameters only.
fn function(reg: &mut Registry, ns: NsId, name: &str, params: Vec<Type>, ret: Type) {
    reg.define_fn(Path::in_ns(ns, name), builtin_fn(Vec::new(), params, ret));
}

/// [`function`] with the type parameters `names`, which `sig` is given to
/// write the parameters' types and the result's in.
fn generic_function<const N: usize>(reg: &mut Registry, ns: NsId, name: &str, names: &[&str; N], sig: impl FnOnce([Type; N]) -> (Vec<Type>, Type)) {
    let path = Path::in_ns(ns, name);
    let type_params = type_params(ParamOwner::Def(path), names);
    let (params, ret) = sig(as_types(&type_params));
    reg.define_fn(path, builtin_fn(type_params, params, ret));
}

/// A signature implemented in Rust: public, with required parameters only.
fn builtin_fn(type_params: Vec<TypeParam>, params: Vec<Type>, ret: Type) -> FnDef {
    FnDef { public: true, builtin: true, ..FnDef::plain(type_params, params, ret) }
}

/// The type parameters of `owner` named `names`, in order.
fn type_params(owner: ParamOwner, names: &[&str]) -> Vec<TypeParam> {
    names.iter().enumerate().map(|(i, n)| TypeParam { owner, index: i as u32, name: intern(n) }).collect()
}

/// `params` as types, `N` of them.
fn as_types<const N: usize>(params: &[TypeParam]) -> [Type; N] {
    let types: Vec<Type> = params.iter().map(|p| Type::Param(*p)).collect();
    types.try_into().unwrap_or_else(|t: Vec<Type>| panic!("{} type parameters are written as {}", t.len(), N))
}

/// A built-in sum type `path` with the type parameters `names` and the
/// variants `variants` writes in them, which are returned.
fn sum_type<const N: usize>(reg: &mut Registry, path: Path, names: &[&str; N], variants: impl FnOnce([Type; N]) -> Vec<Variant>) -> Vec<Variant> {
    let mut def = BuiltinType::new(reg, path, AdtKind::Sum, names);
    let variants = variants(def.params());
    def.set_variants(variants.clone());
    variants
}

/// Make the variants of the type `ty` but those named in `except` bare names
/// of the module `ns`, each keyed by the name it is read under there.
fn bare_constructors(reg: &mut Registry, ns: NsId, ty: Path, variants: &[Variant], except: &[&str]) {
    for (i, v) in variants.iter().enumerate().filter(|(_, v)| !except.contains(&v.name.name())) {
        reg.ctors.insert((ns, intern_in(ns, v.name.name())), (ty, i));
    }
}

/// A built-in type being written: public, implemented in Rust, and the
/// standard library's.
pub(super) struct BuiltinType<'r> {
    reg: &'r mut Registry,
    path: Path,
    params: Vec<TypeParam>,
}

impl<'r> BuiltinType<'r> {
    /// The type `path` of the kind `kind`, with the type parameters `names`,
    /// no variants and no methods yet.
    fn new(reg: &'r mut Registry, path: Path, kind: AdtKind, names: &[&str]) -> BuiltinType<'r> {
        let params = type_params(ParamOwner::Def(path), names);
        let def = TypeDef { kind, params: params.clone(), public: true, builtin: true, stdlib: true, fields: Vec::new(), variants: Vec::new() };
        reg.types.insert(path, TypeEntry::Type(def));
        BuiltinType { reg, path, params }
    }

    /// The type's parameters, `N` of them: what its variants and its
    /// methods' signatures are written in.
    fn params<const N: usize>(&self) -> [Type; N] {
        as_types(&self.params)
    }

    fn set_variants(&mut self, variants: Vec<Variant>) {
        let Some(TypeEntry::Type(def)) = self.reg.types.get(&self.path) else { unreachable!("`new` defined the type") };
        let def = TypeDef { variants, ..def.clone() };
        self.reg.types.insert(self.path, TypeEntry::Type(def));
    }

    /// A public method whose first parameter is the receiver.
    fn instance(&mut self, name: &str, params: Vec<Type>, ret: Type) {
        self.method(name, true, true, params, ret);
    }

    /// A public method with no receiver, called as `(Type::name ...)`.
    fn associated(&mut self, name: &str, params: Vec<Type>, ret: Type) {
        self.method(name, false, true, params, ret);
    }

    /// An instance method only the standard library calls.
    fn hidden_instance(&mut self, name: &str, params: Vec<Type>, ret: Type) {
        self.method(name, true, false, params, ret);
    }

    /// The method `name`, its signature written in the type's parameters,
    /// which become the method's own.
    fn method(&mut self, name: &str, instance: bool, public: bool, params: Vec<Type>, ret: Type) {
        let name = member_symbol(name);
        let owner = ParamOwner::Method(self.path, name);
        let own: Vec<TypeParam> = self.params.iter().map(|p| TypeParam { owner, ..*p }).collect();
        let reown = Subst::positional(&self.params, &own.iter().map(|p| Type::Param(*p)).collect::<Vec<_>>());
        let params = params.iter().map(|t| reown.apply(t)).collect();
        let sig = FnDef { public, ..builtin_fn(own, params, reown.apply(&ret)) };
        self.reg.define_method(self.path, name, MethodDef { sig, instance, home: None, inst_args: None });
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::types::in_std;

    /// A name has one definition across the library's modules, and a type
    /// name one across the standard library, which [`Path::is_std`] relies
    /// on. The definitions checked later are held to it as they are defined
    /// (`define::std_elsewhere`); the built-in ones are held to it here.
    #[test]
    fn no_two_library_modules_define_one_name() {
        let mut reg = Registry::default();
        define(&mut reg);
        let library = |m: NsId| m == NsId::STD || super::super::library::PRELUDE_MODULES.iter().any(|seg| std_module(seg) == m);
        let defined = reg
            .values
            .iter()
            .map(|(p, _)| *p)
            .chain(reg.types.iter().map(|(p, _)| *p))
            .chain(reg.ctors.iter().map(|((m, s), _)| Path::of_symbol(*m, *s)))
            .filter(|p| library(p.ns()));
        let mut names: HashMap<&str, Path> = HashMap::new();
        for path in defined {
            if let Some(prev) = names.insert(path.name_symbol().name(), path) {
                assert_eq!(prev, path, "the standard library defines `{}` twice", path.name_symbol().name());
            }
        }
        let mut types: HashMap<&str, Path> = HashMap::new();
        for (path, _) in reg.types.iter().filter(|(p, _)| in_std(p.ns())) {
            if let Some(prev) = types.insert(path.name_symbol().name(), *path) {
                panic!("the standard library defines the type `{}` in two modules: `{}` and `{}`", path.name_symbol().name(), prev, path);
            }
        }
    }
}
