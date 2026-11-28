//! A tree-walking interpreter over the checker's typed AST.
//!
//! Top-level definitions (`defun`/`defmethod`) are registered at [`Interp::root`],
//! a runtime mirror of the checker's own module tree (see `eval::scope`'s module
//! doc comment) — each at the node for its own defining module, under its own
//! unqualified name, never in one flat table spanning the whole program.
//! Expressions are evaluated against that tree plus a lexical environment.
//! Functions are not closures — a body sees only its parameters and the global
//! definitions, matching top-level `defun`/`defmethod` semantics.
//!
//! Values live in the GC-managed [`Heap`] shared with the reader, so `eval`
//! threads a `&mut Heap` throughout. Every mutable [`Slot`] *is* a heap cell,
//! which the collector finds through its own `cell_registry` on every
//! collection — so the interpreter pushes no roots for bindings at all. What
//! it does still have to root by hand is anything held only on the Rust stack
//! across an allocation: partially-built structures inside a builder, and
//! arguments staged for a crossing into compiled code.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use num_rational::BigRational;

use crate::type_key::alloc_typed_enum;

use crate::check::core;
use crate::check::repr::Repr;
use crate::{wk, Heap, MacroLambda, Path, SymRef, Value};
use typelisp_mem::{spec, StdModule, TypeSpec};

use super::scope;
pub use super::value::EvalError;
use super::value::Slot;

/// The evaluator over core forms, the cons-cell program representation the
/// checker produces: the tag vocabulary, the leaves, the field readers and the
/// top level. A child module rather than a sibling of `interp`, so it can
/// reach this module's private items (the builtin dispatch, the module tree).
mod core_eval;

/// The loop that drives it: an explicit state machine over a continuation
/// stack, so the depth of a Lisp recursion is not the depth of the Rust stack
/// and the continuation is data a task can be suspended on —
/// `docs/dev/cps-evaluator-design.md`.
mod core_cps;

/// What `core_eval`'s and `core_cps`'s tests both read and evaluate core forms
/// with.
#[cfg(test)]
mod core_test_support;

/// The scheduler's task body over `core_cps`'s task and the compiled one,
/// and the worker threads that step the compiled kind.
mod task_body;

/// What those worker threads print with.
#[cfg(not(target_family = "wasm"))]
mod worker_print;

mod compiled;
mod compiled_tables;
mod macros;
mod builtins;
mod printing;
mod read_hooks;
mod numeric;
mod random_time;
mod backend;
mod collections;
mod strings;
mod equality;
mod xref;
pub(crate) use compiled::*;
pub(crate) use builtins::*;
// What the worker threads' printer snapshot reads (`worker_print`).
#[cfg(not(target_family = "wasm"))]
pub(crate) use printing::*;
use read_hooks::*;
pub use numeric::*;
use random_time::*;
pub use backend::*;
pub use collections::*;
pub use strings::*;
pub(crate) use equality::*;

pub use core_eval::read_ffi_decl;

/// A registered function or method body with its parameter names. Lives at
/// exactly one [`scope::ModuleScope`] tree node — its own defining module —
/// rather than in a flat program-wide table; see that module's doc comment.
pub struct FnDef {
    /// The definition this is.
    ///
    /// The scope tree keys these by their *last* segment inside a namespace,
    /// which is enough to find one but not enough to *name* one: `enter` has
    /// an `Rc<FnDef>` in hand and nothing else. `trace` needs both — a name
    /// to look up in its table, and a name to print.
    pub target: crate::CompileTarget,
    /// Parameter names, including the receiver name first for instance methods.
    pub params: Vec<SymRef>,
    /// The body's core forms, in order.
    ///
    /// These are heap cells, which the old `Vec<Typed>` was not — so the
    /// module tree is now a *source of GC roots*. `Interp::exec` permanently
    /// roots the whole top-level form it registers (one root per definition,
    /// covering the body, the parameter list and every representation
    /// reachable from it); see `Heap::push_permanent_root`, which is
    /// LIFO-independent for exactly this.
    pub body: Vec<Value>,
    /// Only ever set for a `defmacro` with a trailing `&rest` parameter (see
    /// [`Interp::expand_macro`]); always `false` for `defun`/`defmethod`,
    /// which `apply` calls 1:1 regardless.
    pub rest: bool,
    /// The `&optional`/`&key` structure of a `defmacro`'s lambda list —
    /// `Some` only for a macro (its default-value bodies and keyword names,
    /// consumed by [`Interp::bind_macro_args`]), `None` for `defun`/
    /// `defmethod`, which bind their arguments 1:1 with no defaults. `params`
    /// still lists every binding name in order; this only adds how the
    /// non-required regions are filled.
    pub lambda: Option<MacroLambda>,
    /// `(parameter representations, return representation)`, parallel to
    /// `params` — `None` for a `defmacro` (every parameter and the implicit
    /// return are always `Sexpr`, see `typeck::registry::MacroDef`'s doc
    /// comment) since a macro is never a `compile` target.
    ///
    /// Representations, not `Type`s: the type is gone from the IR by the time
    /// a definition is registered, and what the two consumers actually need is
    /// exactly what a `Repr` says. `compile::driver::compile_function` needs an LLVM
    /// signature, and `encode_crossing_args` must be driven by the *declared*
    /// shape rather than the runtime value's — see
    /// `docs/dev/` and the crossing rule: whether a word is raw or tagged is
    /// in the declaration only.
    pub sig: Option<(Vec<Repr>, Repr)>,
    /// This definition's JIT/AOT-compiled form, if `(compile name)` (or an
    /// AOT pass) has ever produced one — the direct replacement for the old
    /// standalone `Interp::compiled`/`compiled_methods` side tables (`Path`-
    /// keyed, "must never drift" from `fns`/`methods` by discipline alone).
    /// Living on the node `scope::ModuleScope::get_fn`/`get_method` finds
    /// eliminates that duplication structurally: the compiled-fast-path check in `Call`/`Assoc` eval is
    /// just "does the `FnDef` I already have in hand carry one?". Purely a
    /// machine-code-linkage implementation detail — never consulted by this
    /// module's own visibility/resolution logic.
    ///
    /// Its own `RefCell`, separate from the rest of this struct (the tree
    /// stores `Rc<FnDef>`, not `Rc<RefCell<FnDef>>`): `Self::apply` holds a
    /// plain `&FnDef` borrowed out of that `Rc` for the *entire* recursive
    /// evaluation of `body` (needed since the body's form list is too large
    /// to clone per call) — if `compiled` needed the same outer `RefCell`,
    /// an inner `(compile name)` call reached from within that body (e.g. a
    /// self-referential or mutually-recursive JIT request) that tried to
    /// populate this very same `FnDef`'s `compiled` slot would panic against
    /// `apply`'s already-live borrow. Scoping the `RefCell` down to just this
    /// field keeps that borrow-and-mutate pair independent, exactly like the
    /// old design's two separate top-level fields (`fns`/`compiled`) did.
    pub compiled: RefCell<Option<Rc<dyn CompiledBody>>>,
    /// Declared by `(defffi ...)`: `body` is empty and `compiled` is the thunk
    /// that reaches the C function — the runtime twin of `FnDef::ffi`.
    ///
    /// The interpreter itself never consults this: `enter` prefers a compiled
    /// body whatever the definition is, which is exactly right here. It exists
    /// so the tools can tell the two kinds of empty body apart —
    /// `disassemble` would otherwise build a module from no forms and show a
    /// function that is not the one the name reaches.
    pub ffi: bool,
}

/// What the interpreter needs from a compiled function body: where it is.
///
/// A trait rather than the backend's own `crate::compile::CompiledFn`, so
/// nothing here names a type that owns JIT-compiled code. That struct is,
/// for the evaluator, two things — the code, held only to keep it alive, and
/// this address — and every use of it from the evaluator is `.address()`. Naming
/// only the half the evaluator actually reads is what lets the front end stop
/// depending on the backend.
///
/// The implementor's `Drop` is what retires the code, so holding
/// `Rc<dyn CompiledBody>` here keeps the machine code alive exactly as
/// holding the concrete type did.
pub trait CompiledBody {
    /// The entry address of this body, under the shared compiled-function ABI
    /// ([`typelisp_abi::ClassicFn`]).
    fn address(&self) -> usize;

    /// Which ABI that address answers to — `typelisp_abi::BODY_ABI_CLASSIC`
    /// (`f(args, argc)`, runs to completion) or `BODY_ABI_COROUTINE`
    /// (`f(frame)`, returns a status word).
    ///
    /// **Not inferable from the address.** Calling one as the other is not a
    /// type error anywhere — it is a wrong answer or a crash — so the artifact
    /// that carried the body has to say, and this is where that answer arrives.
    ///
    /// Required, with no default. It defaulted to classic while that was
    /// "what every producer says until the island itself changes over"; the
    /// island changed over in C2d, and a default that no implementor uses is
    /// a wrong answer waiting for the next one. Both implementors already
    /// override it, so nothing is lost by making the question unanswerable
    /// by omission.
    fn body_abi(&self) -> u8;

    /// Calls it with `args` already in the compiled representation.
    ///
    /// Provided, not required: the address is the only thing that varies per
    /// implementor. A `(panic ...)` in the body comes back as a raise waiting
    /// on the thread, which the caller's `catch_compiled_panic` takes.
    fn call(&self, args: &[typelisp_mem::Word]) -> i64 {
        call_body(self.address(), self.body_abi(), args)
    }
}


/// A user `defenum`'s variants (each one's declared field types), as
/// `TopLevel::Defenum` baked them in at check time — the entry
/// [`Interp::enum_defs`] keeps per enum type; see that field's doc comment for
/// why the interpreter needs this at all (compiled-global box decoding) and
/// why `Option`/`Result` are not represented this way.
///
/// The type *parameters* used to sit here too, read only by
/// `enum_fields_representable` to substitute `args` into the field types
/// before asking whether each was storable. Deleting `RtValue::Data` made that
/// question moot — every enum value is a heap box — and the parameters went
/// unread with it.
#[derive(Clone)]
pub struct EnumDef {
    /// Each variant's name and its fields' representations, in declaration
    /// order — everything the two consumers need: the printer wants the name
    /// (`eval::format`), and the compiled-global boundary wants each field's
    /// representation to decode a box back into an enum value. The core
    /// `(defenum PATH (SYM...) ((REPR...)...) ((TEMPLATE...)...))` form
    /// carries exactly this, plus the templates below.
    pub(crate) variants: Vec<(String, Vec<Repr>)>,
    /// Each variant's fields' types as templates (`type_key::field_template`),
    /// parallel to `variants` — what the printer instantiates to learn that a
    /// field holds a niche-represented `Option` (`scope::FieldTemplates`).
    pub(crate) templates: Vec<Vec<crate::TypeId>>,
}

/// The interpreter state: a runtime mirror of the checker's module tree
/// ([`scope::ModuleScope`], rooted here), holding every free function/
/// macro/method/global/struct/enum by its own defining module and
/// unqualified name — never by a single flat [`Path`]-keyed table spanning
/// the whole program. See `eval::scope`'s module doc comment for why, and
/// for how a reference re-resolves against this tree instead of trusting a
/// checker-baked [`Path`] as a lookup key.
/// One call that was open when an evaluation failed: what a backtrace line
/// says about it. Rendered when the failure passed it, while the values were
/// still rooted, so it holds no heap value.
#[derive(Clone, Debug)]
pub struct BacktraceFrame {
    /// The function, by the name `trace` gives it.
    pub name: String,
    /// Each parameter and its value as `~s` prints it, rendered by the
    /// parameter's declared representation.
    pub args: Vec<(String, String)>,
    /// Where the call was made; `None` for a call in a form with no location.
    pub called_at: Option<crate::Loc>,
}

impl BacktraceFrame {
    /// The call as `trace` writes it: `(fact 3)`.
    pub fn call_text(&self) -> String {
        let mut text = format!("({}", self.name);
        for (_, v) in &self.args {
            text.push(' ');
            text.push_str(v);
        }
        text.push(')');
        text
    }
}

pub struct Interp {
    /// The runtime module tree. Behind a `RefCell` so the `eval` builtin
    /// (`Self::eval_form`, reached through the `&self` evaluation path) can
    /// register a definition it just evaluated — `(eval '(defun ...))` — into
    /// the same tree that ordinary top-level `exec` writes to, giving
    /// CL-faithful "definitions become visible immediately" semantics without
    /// a `&mut self` the deep `&self` eval chain cannot produce. Every read
    /// (`resolve_fn`/`get_fn`/`find_type`/...) borrows shared; the handful of
    /// registration sites in [`Self::exec`] borrow mutably.
    pub root: RefCell<scope::ModuleScope>,
    /// The tasks that exist and the order they run in — the runtime's
    /// scheduler (`typelisp_rt::sched`), over this interpreter's task body
    /// ([`task_body::TyplBody`]). Shared with the worker threads that step
    /// its compiled tasks, and locked for every question — each one short: a
    /// task is taken *out* to be stepped.
    pub(crate) scheduler: std::sync::Arc<typelisp_rt::sched::SchedShared<task_body::TyplBody>>,
    /// Whether a drive of [`Self::scheduler`] is on this thread's stack. A
    /// *nested* evaluation — a compiled callee re-entering the interpreter,
    /// or `Interp::apply` — must not switch tasks: there is a Rust frame
    /// waiting on its result, and no continuation stack underneath it to
    /// come back to.
    pub(crate) driving: std::cell::Cell<bool>,
    /// Whether the scheduler has its crew of worker threads yet — see
    /// `Interp::install_crew`.
    pub(crate) crew_installed: std::cell::Cell<bool>,
    /// What the crew's threads print with — see `worker_print`.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) worker_print: worker_print::SharedSnapshot,
    /// How many times what [`Self::worker_print`] copies has changed, and
    /// which of those the published snapshot reflects — see
    /// `Interp::printer_tables_changed`.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) print_generation: std::cell::Cell<u64>,
    #[cfg(not(target_family = "wasm"))]
    pub(crate) published_generation: std::cell::Cell<u64>,
    /// The compiled bodies the published snapshot names, kept alive while a
    /// worker may be calling one — see `worker_print`.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) printer_bodies: RefCell<Vec<Rc<dyn CompiledBody>>>,
    /// Whether this is an `eval`-carrying executable's interpreter, whose
    /// program prints through the executable's own registration tables
    /// (`typelisp_print::aot`) — its compiled `print-object`s are there, not
    /// in this interpreter's module tree — on a worker as on the main thread
    /// before the interpreter joined. Set by `shim::rt_eval_init`.
    pub(crate) executable_printer: std::cell::Cell<bool>,
    /// The C callback entries this session has made, by callback
    /// (`crate::ffi_callback`), each with the compiled body it runs — kept so
    /// the code stays alive while C may hold the entry, and compared on every
    /// lookup so a redefined function gets an entry of its own. Per session,
    /// not per process: an entry runs code this session compiled.
    callback_entries: RefCell<HashMap<crate::ffi_callback::CallbackSig, crate::ffi_callback::CallbackEntry>>,
    /// The bodies of entries [`Self::callback_entries`] has replaced: C may
    /// still call an old entry, so what it runs must not be freed.
    retired_callback_bodies: RefCell<Vec<Rc<dyn CompiledBody>>>,
    /// A shared handle to the live `Checker`, set by [`Self::set_checker`] on
    /// the drivers that support runtime `eval` (the CLI's `run_file`/`repl`
    /// and the LSP). `None` in throwaway/AOT/bootstrap/test
    /// contexts, where `eval` returns an error rather than type-checking at
    /// runtime. The `Rc<RefCell<..>>` is the same cell the driver checks
    /// top-level forms through; the borrow discipline (never hold a checker
    /// borrow across an `exec` that could `eval`) keeps the two from
    /// double-borrowing — see `Self::eval_form` and the drivers' own comments.
    pub(crate) checker: Option<Rc<RefCell<crate::typeck::checker::Checker>>>,
    /// The macro calls being expanded now, innermost last: the number the
    /// expansion's `Environment` value carries, and where the call is. An
    /// entry lives exactly as long as its [`MacroExpander::expand_macro`]
    /// call, which is what makes the pointer good. It is how a
    /// `macroexpand` the macro's body makes expands — the checker is
    /// borrowed by the check that is expanding, so it cannot be asked.
    expansions: RefCell<Vec<(i64, *const (dyn crate::typeck::checker::ExpansionEnv + 'static))>>,
    /// The number the next expansion is given. Never reused, so an
    /// `Environment` kept after its expansion ended is told from a live one.
    next_expansion: Cell<i64>,
    /// Every `defvar`/`defconstant` global some compiled function has
    /// referenced, promoted to a compiled-global slot (a permanent GC root
    /// — `typelisp_rt::global_new`) and mapped to the id that slot got.
    /// Populated lazily by [`Self::add_compiled_function`], the single
    /// choke point both the JIT (`Self::compile_function`) and AOT
    /// (`crate::compile::aot::compile_file`) paths compile a `defun`
    /// through — so this table, and the promotion it drives, is shared by
    /// both with no separate AOT-specific logic. A global *never*
    /// referenced from compiled code has no entry here and keeps using its
    /// ordinary [`Slot`] in [`Self::globals`] untouched — see that
    /// method's doc comment for why only a promoted global's storage moves.
    pub compiled_globals: RefCell<HashMap<Path, usize>>,
    /// The values currently being rendered by their own `print-object`
    /// method, innermost last — [`Self::print_object`]'s re-entry guard.
    pub(crate) printing: RefCell<Vec<Value>>,
    /// The definitions `trace` is currently watching.
    ///
    /// Here rather than a flag on each [`FnDef`] so that tracing survives
    /// **redefinition**: redefining a function replaces the `Rc<FnDef>` in
    /// the scope tree, and a flag on the old one would go with it. CL traces
    /// a *name*, and so does this.
    pub(crate) traced: RefCell<HashSet<crate::CompileTarget>>,
    /// `!traced.is_empty()`, kept as a `Cell` so the check on the hot path in
    /// [`Self::enter`] is a load and not a `RefCell` borrow. Every write to
    /// `traced` updates it.
    pub(crate) trace_armed: Cell<bool>,
    /// The calls that were open when the last drive's main task failed,
    /// innermost first — what the REPL's `:bt` prints. Replaced by each
    /// failure; see [`Self::take_backtrace`].
    pub(crate) last_backtrace: RefCell<Vec<BacktraceFrame>>,
    /// How many traced (or stepped) frames are open — the indentation depth
    /// in a trace report, and CL's own `0:`/`1:`/`2:` prefix.
    pub(crate) trace_depth: Cell<usize>,
    /// Whether a `(step form)` is in progress: every call reached from inside
    /// `form` stops and asks, using the same [`Self::enter`] hook `trace`
    /// uses. Restored by `step_core` when the form finishes, however it
    /// finishes.
    pub(crate) stepping: Cell<bool>,
    /// The depth a `next` command was given at — while the current depth is
    /// *below* it, the stepper does not ask. `usize::MAX` means "ask at every
    /// depth", which is where a step starts and what returning to the
    /// commanding frame restores.
    pub(crate) step_quiet_depth: Cell<usize>,
    /// The dumps whose units this environment was built from, in the order
    /// they were applied — the bytes `(dump ...)` re-emits ahead of the
    /// session's own unit, so that what it writes is self-contained.
    ///
    /// `Cow` rather than `Vec<u8>`: the prelude's and island's are
    /// `include_bytes!` statics (4MB between them, borrowed for free), while a
    /// `typl --image` load owns what it read.
    dump_sources: RefCell<Vec<std::borrow::Cow<'static, [u8]>>>,
    /// What this session has defined since [`Self::start_recording`], and the
    /// checker signature it started from — the two halves `(dump ...)` needs
    /// to write the session's own unit.
    ///
    /// `None` until a driver turns it on, which is what keeps the definitions
    /// arriving from the loaded units themselves out of it.
    recording: RefCell<Option<Recording>>,
    /// Trait-object vtables, interpreter tier: `vtable_id` -> the call
    /// targets for the trait's methods, in slot order (`dyn-new`'s
    /// `slots`, which the checker laid out from `TraitDef::method_order`).
    /// Entries are `(owning type path, method name)` — resolved through the
    /// scope tree at call time, so a method that gets `compile`d later is
    /// picked up automatically, exactly as for a static `assoc`.
    ///
    /// The compiled tier keeps a parallel table of raw function pointers in
    /// `typelisp_rt`, indexed by the same ids (which [`Self::vtable_id_for`]
    /// is the sole source of). Neither table holds heap values, so a vtable
    /// needs no GC root — `BoxedObj::Dyn`'s own `value` is all the collector
    /// ever has to trace.
    pub(crate) vtables: RefCell<Vec<Vec<(Path, SymRef)>>>,
    /// `(concrete type, trait path)` -> `vtable_id`, so the same pair
    /// interns to one id however many times it is boxed.
    pub vtable_ids: RefCell<HashMap<(crate::TypeId, Path), u32>>,
    /// `(vtable id, trait id)` -> the vtable id the *same concrete type* uses
    /// for that trait: the supertrait upcast table `dyn-upcast` reads.
    ///
    /// Keyed by the runtime vtable id because that is all an upcast site has
    /// — the concrete type, the other half of a vtable's identity, is gone by
    /// then. Filled in at every boxing site from `dyn-new`'s `SUPERS`
    /// ([`Self::register_dyn_box`]), which is where both halves are still in
    /// hand. The compiled tier keeps the same map under the same ids in
    /// `typelisp_rt` (`upcast_define`).
    pub(crate) dyn_upcasts: RefCell<HashMap<(u32, u32), u32>>,
    /// Trait path -> a small integer, so an upcast site in *compiled* code
    /// can name its target trait with a baked-in constant the way a boxing
    /// site names its vtable ([`Self::vtable_id_for`]). Interpreted upcasts
    /// go through the same ids, so both tiers agree by construction.
    pub trait_ids: RefCell<HashMap<Path, u32>>,
    /// Traits some *compiled* body dispatches on (an `dyn-call` in a
    /// function that has been through `translate_and_compile`).
    ///
    /// A compiled `:dyn` call site reads a raw function pointer out of the
    /// vtable, so every implementation it could reach has to be compiled
    /// too. `dyn-call`'s `IMPL_TARGETS` covers the ones that existed when
    /// the call site was checked; this set covers the rest, by telling
    /// `dyn-new`'s evaluation that boxing for *this* trait now has a
    /// native consumer, so a slot method still lacking a compiled form must
    /// be compiled before the box escapes. Empty for any program that never
    /// compiles a dynamic dispatch — which is why boxing does not simply
    /// compile its slots unconditionally.
    pub dyn_dispatch_compiled: RefCell<HashSet<Path>>,
    /// The definitions an interpreted closure was given so that a `thread`
    /// could run it, keyed by the closure's body forms.
    ///
    /// A thread runs only compiled code, and what the compile driver compiles
    /// is a registered definition. A closure is not one — a named function
    /// used as a value is reified into a closure over a copy of its body list
    /// ([`Interp::reify`]) — so each body gets a definition of its own under
    /// [`crate::INTERNAL_MODULE`], compiled once however often it is spawned.
    /// The key is the identity of the body's forms, which the same function
    /// or `lambda` shares between evaluations; the body is rooted
    /// permanently when its definition is made, so its cells are never
    /// reused for another form and a key cannot come to name something else.
    pub thread_entries: RefCell<HashMap<Vec<typelisp_mem::Word>, Path>>,
}

/// Why `compile::driver::precheck_compilable` says no.
///
/// The split is between a cause worth *recording* and one worth *fixing*: a
/// missing target names a specific gap in what the compile path lowers, stays
/// stable while that gap does, and is the same for every definition that
/// reaches it; anything else is a one-off and gets no such treatment.
#[derive(Debug)]
pub enum Uncompilable {
    /// A call target with no compilable body — a Rust builtin free function or
    /// builtin method the compile path has no lowering for. Carries that
    /// target's name, not the caller's.
    MissingTarget(String),
    /// Anything else the compile path rejected.
    Other(EvalError),
}

impl Interp {
    pub fn new() -> Interp {
        // A fresh `Interp` always pairs with a fresh `Heap` (every caller in
        // this codebase constructs them together) — clear the global-id
        // table typelisp-rt keeps for compiled global-variable access
        // (`compiled_globals`'s eventual runtime counterpart) so a stale
        // entry from an earlier `Interp`/`Heap` pair that happened to share
        // this OS thread (`cargo test`'s worker pool) can't resolve to a
        // permanent-root position in a `Heap` that no longer exists. See
        // `typelisp_rt::reset_global_table`'s doc comment.
        typelisp_rt::reset_global_table();
        // Same reasoning for the compiled tier's vtable table: ids are
        // per-`Interp`, so a stale entry from an earlier pair on this thread
        // must not survive into this one.
        typelisp_rt::reset_vtable_table();
        // The built-in types no definition form ever runs for
        // (`typeck::builtins::runtime_types`) are seeded here, in the
        // standard library's modules where the checker defines them, so the
        // printer finds them.
        let mut root = scope::ModuleScope::default();
        use crate::typeck::builtins::{chan_of, environment_type, task_of, thread_of};
        let handles: Vec<Path> = [task_of(crate::Type::Int), thread_of(crate::Type::Int), chan_of(crate::Type::Int), environment_type()]
            .into_iter()
            .map(|handle| match handle {
                crate::Type::Named(p, _) => p,
                _ => unreachable!("a handle type is named"),
            })
            .collect();
        for (path, def) in crate::typeck::builtins::runtime_types() {
            match def.kind {
                // `Vector<T>`'s storage is internal (`rt_struct_*`), it has no
                // constructor-pattern shape, and nothing ever names one of
                // its fields — so there is nothing for a `construct` or a
                // pattern to read here. Its elements do print, though, and
                // they are all of the one type argument: a uniform template
                // says so.
                crate::typeck::registry::AdtKind::Struct if path == Path::builtin(typelisp_mem::wk::VECTOR) => {
                    let t = crate::Type::Param(def.params[0]);
                    root.insert_type(
                        &path,
                        scope::TypeEntry::Struct { reprs: Vec::new(), templates: scope::FieldTemplates::Uniform(crate::type_key::type_id(&t)), names: Vec::new() },
                    );
                }
                // `Task<T>`/`Thread<T>`/`Chan<T>`: a handle box holding the
                // scheduler's id and nothing else, built by the runtime
                // (`typelisp_rt::sched::task_handle`). Nothing constructs or
                // matches one either, but it prints as its type and the id
                // (`#<Chan<int> 3>`), so the printer has to find it here —
                // positionally, with no names.
                crate::typeck::registry::AdtKind::Struct => {
                    assert!(handles.contains(&path), "`{}` is a built-in struct the interpreter does not know", path);
                    root.insert_type(
                        &path,
                        scope::TypeEntry::Struct {
                            reprs: Vec::new(),
                            templates: scope::FieldTemplates::Positional(vec![crate::type_key::type_id(&crate::Type::Int)]),
                            names: Vec::new(),
                        },
                    );
                }
                // `Option`/`Result`/the concrete error types: seeded through
                // the same `register_enum` a real `defenum` uses, so a printed
                // value's variant has its name. The field types are the
                // builtins' own declarations, so their representations come
                // from the same classification a `defenum`'s do — `Repr::of_by`
                // with no ADT lookup, since none of these mention a user type.
                crate::typeck::registry::AdtKind::Sum => {
                    let templates = def.variants.iter().map(|v| v.fields.iter().map(crate::type_key::type_id).collect()).collect();
                    let variants = def
                        .variants
                        .into_iter()
                        .map(|v| {
                            let reprs = v.fields.iter().map(|t| Repr::of_by(t, &|_| None)).collect();
                            (v.name.name().to_string(), reprs)
                        })
                        .collect();
                    root.register_enum(&path, EnumDef { variants, templates });
                }
            }
        }
        Interp {
            callback_entries: RefCell::new(HashMap::new()),
            retired_callback_bodies: RefCell::new(Vec::new()),
            root: RefCell::new(root),
            checker: None,
            expansions: RefCell::new(Vec::new()),
            next_expansion: Cell::new(1),
            compiled_globals: RefCell::new(HashMap::new()),
            vtables: RefCell::new(Vec::new()),
            vtable_ids: RefCell::new(HashMap::new()),
            dyn_upcasts: RefCell::new(HashMap::new()),
            trait_ids: RefCell::new(HashMap::new()),
            dyn_dispatch_compiled: RefCell::new(HashSet::new()),
            thread_entries: RefCell::new(HashMap::new()),
            printing: RefCell::new(Vec::new()),
            traced: RefCell::new(HashSet::new()),
            trace_armed: Cell::new(false),
            last_backtrace: RefCell::new(Vec::new()),
            trace_depth: Cell::new(0),
            stepping: Cell::new(false),
            step_quiet_depth: Cell::new(usize::MAX),
            dump_sources: RefCell::new(Vec::new()),
            recording: RefCell::new(None),
            scheduler: std::sync::Arc::new(typelisp_rt::sched::SchedShared::new()),
            driving: std::cell::Cell::new(false),
            crew_installed: std::cell::Cell::new(false),
            #[cfg(not(target_family = "wasm"))]
            worker_print: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            print_generation: std::cell::Cell::new(1),
            #[cfg(not(target_family = "wasm"))]
            published_generation: std::cell::Cell::new(0),
            #[cfg(not(target_family = "wasm"))]
            printer_bodies: RefCell::new(Vec::new()),
            executable_printer: std::cell::Cell::new(false),
        }
    }

    /// A shared handle to the live `Checker`, for the paths that need to check
    /// or capture against this environment (`(dump ...)`). `None` where no
    /// driver wired one — see the `checker` field.
    pub fn checker_handle(&self) -> Option<Rc<RefCell<crate::typeck::checker::Checker>>> {
        self.checker.clone()
    }

    /// Records that this environment came from `bytes` — a dump whose units
    /// have just been applied.
    pub fn push_dump_source(&self, bytes: std::borrow::Cow<'static, [u8]>) {
        self.dump_sources.borrow_mut().push(bytes);
    }

    /// Runs `f` over the dumps this environment was built from.
    pub fn with_dump_sources<R>(&self, f: impl FnOnce(&[std::borrow::Cow<'static, [u8]>]) -> R) -> R {
        f(&self.dump_sources.borrow())
    }

    /// Starts recording what this session defines, from a checker signature
    /// taken now.
    ///
    /// Called by a driver once its environment is fully loaded and before it
    /// runs a line of user code: everything after this point is the session's,
    /// and everything before it belongs to a unit that is already written down.
    pub fn start_recording(&self, baseline: crate::dump::RegistrySignature) {
        *self.recording.borrow_mut() =
            Some(Recording { baseline, forms: Vec::new(), globals_before: self.compiled_globals.borrow().len() });
    }

    /// Runs `f` over this session's recording, or fails if there is none.
    pub fn with_recording<R>(
        &self,
        f: impl FnOnce(&crate::dump::RegistrySignature, &[Value], usize) -> R,
    ) -> Option<R> {
        let r = self.recording.borrow();
        let r = r.as_ref()?;
        Some(f(&r.baseline, &r.forms, r.globals_before))
    }

    /// Adds `tl`'s definitions to the recording, if there is one.
    ///
    /// Called by [`Self::exec`] after a successful top-level form. Each
    /// recorded node is rooted for the rest of the session: `exec` permanently
    /// roots a definition's *body*, not the top-level node, and `(dump ...)`
    /// reads these nodes long afterwards. Permanently, not on the root stack:
    /// the form may be an `eval` inside a running task, whose stack is cut
    /// back when its frames are left.
    pub(crate) fn note_definitions(&self, heap: &mut Heap, tl: Value) {
        let mut recording = self.recording.borrow_mut();
        let Some(recording) = recording.as_mut() else { return };
        let mut found = Vec::new();
        crate::dump::record_definitions(heap, tl, &mut found);
        for v in found {
            heap.push_permanent_root(v);
            recording.forms.push(v);
        }
    }

    /// Wire this interpreter to the live `Checker` so a runtime `eval` can
    /// type-check the form it is handed against the program's current global
    /// environment. Called by the `eval`-supporting drivers (the CLI and LSP)
    /// right after construction; contexts that never run user `eval` (AOT
    /// compilation, the compiler-island bootstrap, most tests) skip it and
    /// leave the handle `None`. See the `checker` field's doc comment.
    /// Binds the global `path`, which the checker has defined, to `v`: the
    /// runtime half of a `defvar` whose value is already in hand.
    pub fn define_global_value(&self, heap: &mut Heap, path: &Path, v: Value) {
        heap.push_root(v);
        let slot = self.slot(heap, v);
        heap.pop_root();
        self.root.borrow_mut().insert_global(path, slot);
    }

    /// Removes the global `path`, whose definition the checker removed.
    pub fn forget_global(&self, path: &Path) {
        self.root.borrow_mut().remove_global(path);
    }

    /// The calls that were open when the last drive's main task failed,
    /// innermost first, leaving none behind. Empty after a drive that did
    /// not fail, or one whose failure left from no call.
    pub fn take_backtrace(&self) -> Vec<BacktraceFrame> {
        std::mem::take(&mut *self.last_backtrace.borrow_mut())
    }

    pub fn set_checker(&mut self, checker: Rc<RefCell<crate::typeck::checker::Checker>>) {
        self.checker = Some(checker);
    }

    /// Wrap `v` in a fresh GC heap cell. Every binding site (`let`,
    /// parameters, `match` bindings, `labels` placeholders, globals,
    /// `eval_args`'s anchors) goes through here, and they all get the same
    /// thing — there is no longer a kind to choose.
    ///
    /// That uniformity is the point, and it is a correctness property rather
    /// than a simplification. A slot used to come in three kinds, routed by a
    /// declared type through `is_heap_repr_ty` and its checker-side twin
    /// `check::repr`, because the interpreter had a second value
    /// world (`RtValue`) whose variants were not heap values. But a heap cell
    /// is rooted by the collector's own `cell_registry` walk on *every*
    /// collection, whereas a native slot was only rooted when `sync_roots`
    /// next ran — so any type whose values were collectible heap values *had*
    /// to be classified heap-repr, and misclassifying one meant the value
    /// silently vanished mid-collection. Five types were found that way, each
    /// after it went missing under `gc_stress`: `random-state`, `bignum`,
    /// `ratio`, `f64`, and `string` (the last as an outright dangling
    /// `StrId`; see `tests/random_state_time_test.rs`,
    /// `tests/bignum_ratio_gc_test.rs`, `tests/float_gc_test.rs`,
    /// `tests/string_gc_test.rs`). With one value world every value is a heap
    /// value, so the requirement is satisfied by construction and the two
    /// predicates that had to agree with each other are gone.
    ///
    /// (`Heap::alloc_cell` cannot trigger a collection — only `Heap::cons`
    /// can — so `v`'s payload needs no rooting across this call.)
    fn slot(&self, heap: &mut Heap, v: Value) -> Slot {
        Slot::new(heap.alloc_cell(v))
    }


    /// Every currently-registered top-level function's *local* name, in no
    /// particular order — for tooling and tests that need to enumerate what
    /// a given load produced (e.g. deriving the compiler island's own
    /// `defun` set by diffing a prelude-only interpreter against one that
    /// also ran `load_compiler`; interp-closure removal Stage 2 onward).
    pub fn function_names(&self) -> Vec<String> {
        self.root.borrow().all_fn_names()
    }


    /// A global's current value, wherever it actually lives.
    ///
    /// A *promoted* global (one compiled code can reach) is stored in a
    /// permanent GC root, and `set-global` writes only there — its
    /// interpreter-side cell keeps whatever it was initialized with. So Rust
    /// code that reads a prelude global by name must ask here rather than
    /// through `root.get_global(..).get(heap)`, which is the cell.
    ///
    /// The `eval_core` reader (`global_core`) has always made this
    /// distinction; the printer-settings readers did not, and once the
    /// precompiled prelude started promoting *every* prelude `defvar` eagerly,
    /// `(setf *print-pretty* true)` stopped reaching the printer —
    /// pretty printing silently never happened.
    pub(crate) fn global_value(&self, heap: &Heap, path: &Path) -> Option<Value> {
        if let Some(&id) = self.compiled_globals.borrow().get(path) {
            // See `global_core`: the id is not the permanent-root position,
            // so it is resolved the same way `rt_global_get` resolves it.
            return typelisp_rt::global_perm_idx(id).map(|i| heap.permanent_root(i));
        }
        self.root.borrow().get_global(path).map(|slot| slot.get(heap))
    }

    /// Record that `path`'s storage is the compiled-global slot `id`, without
    /// creating one — [`Self::promote_global`]'s half for an environment that
    /// is *joining* a running compiled program rather than preparing one.
    ///
    /// The AOT `eval` environment is the only caller: a standalone executable
    /// created its globals at startup with ids `compile-file` baked into its
    /// machine code, and the interpreter the executable builds for `eval` has
    /// to address those same slots rather than allocate a second set nobody
    /// else can see. `global_core`/`set_global_core` consult this map before
    /// the module tree, so binding it is the whole of sharing the storage.
    pub fn bind_compiled_global(&self, path: Path, id: usize) {
        self.compiled_globals.borrow_mut().insert(path, id);
        self.printer_tables_changed();
    }

    /// Whether `path` already has a compiled-global slot — "is this variable
    /// already initialized by somebody else?", asked by the AOT `eval`
    /// environment before it replays a `defvar` (see
    /// [`Self::bind_compiled_global`]).
    pub fn has_compiled_global(&self, path: &Path) -> bool {
        self.compiled_globals.borrow().contains_key(path)
    }

    /// Whether `path` already names a global with a value — either in the
    /// module tree or, for a program whose globals the compiler took over, in
    /// `compiled_globals`.
    ///
    /// This is what makes `defvar` mean CL's `defvar` ("initialize only if
    /// unbound") rather than its `defparameter`. Both tables have to be
    /// consulted for the same reason `Interp::promote_global` moves entries
    /// between them: a global that has been promoted is bound, and is no
    /// longer in the tree.
    pub fn global_is_bound(&self, path: &Path) -> bool {
        if self.has_compiled_global(path) {
            return true;
        }
        self.root.borrow().get_global(path).is_some()
    }

    /// The compiled-global slot id [`Self::promote_global`] assigned `path`,
    /// if it has one.
    pub fn compiled_global_id(&self, path: &Path) -> Option<usize> {
        self.compiled_globals.borrow().get(path).copied()
    }

    /// Ensures `path` (a global some compiled function's body references)
    /// has a compiled-global slot, promoting it from its ordinary
    /// interpreter [`Slot`] on first reference; returns the slot's id
    /// either way. See [`Self::compiled_globals`]'s doc comment for why
    /// promotion — not moving every global's storage — is the shape this
    /// takes: reads the global's *current* value (whatever the interpreter
    /// last set it to) via the existing `Slot` and hands it to
    /// `typelisp_rt::global_new`, which roots it permanently and returns its
    /// id.
    ///
    /// Nothing is converted on the way. There used to be a crossing here — a
    /// global's storage has the shape of a single always-live boxed-struct
    /// field, so it went through the same `RtValue` -> `mem::Value` mapping a
    /// field write did, with a raw-box/shift-tagged special case for an enum
    /// global on top. The value worlds were unified and that mapping became the
    /// identity, then went; the enum special case had already been retired by
    /// the enum-representation unification's compiler flip.
    ///
    /// `pub`: `compile::aot::compile_file` calls this directly too,
    /// once per `defvar`, *before* compiling any `defun` — eagerly, in file
    /// declaration order, rather than waiting for some `defun` body to
    /// reference it. That ordering matters only for AOT: the ids this
    /// assigns are baked into compiled IR at this (the *compiling*)
    /// process's `Heap`, but AOT's actual runtime storage is established by
    /// a *different* `Heap` — the standalone executable's own, via a
    /// generated startup sequence (`compile::driver::add_compiled_global_init`) that
    /// must call `rt_global_new` in this exact same order for the two
    /// numberings to agree. Running that sequence in file-declaration order
    /// is what keeps it correct even when one `defvar`'s initializer
    /// references an earlier one — the checker's forward-reference
    /// restriction guarantees "earlier in the file" for any such reference,
    /// so eager, in-order promotion here guarantees "already promoted,
    /// lower id" for it too. The JIT path (`compile::driver::prepare_targets`)
    /// has no such concern — compiling and running happen in the same
    /// `Heap` there, so lazy, reference-driven promotion (its own call
    /// here) is simpler and just as correct.
    pub fn promote_global(&self, heap: &mut Heap, path: &Path) -> Result<usize, EvalError> {
        if let Some(&id) = self.compiled_globals.borrow().get(path) {
            return Ok(id);
        }
        let slot = self
            .root
            .borrow()
            .get_global(path)
            .ok_or_else(|| EvalError::Internal(format!("compile: global \"{}\" is not defined", path)))?;
        let v = slot.get(heap);
        let id = typelisp_rt::global_new(heap, v);
        self.compiled_globals.borrow_mut().insert(*path, id);
        self.printer_tables_changed();
        Ok(id)
    }


    pub fn eval_form(&self, heap: &mut Heap, arg: &Value) -> Result<Value, EvalError> {
        let checker = match &self.checker {
            Some(c) => Rc::clone(c),
            None => return Ok(result_err(heap, &EVAL_RESULT_TYPE, wk::EVAL_ERROR, "eval: unavailable in this context (no checker handle)".to_string())),
        };
        let form = *arg;
        // Root the form across type-checking (which allocates), then pop back
        // to the entry mark before exec — never hold a manual root across the
        // exec below.
        let mark = heap.root_count();
        heap.push_root(form);
        let checked = checker.borrow_mut().check_form_at(heap, self, form, None);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        let tl = match checked {
            Ok(tl) => tl,
            Err(e) => return Ok(result_err(heap, &EVAL_RESULT_TYPE, wk::EVAL_ERROR, e.to_string())),
        };
        // Decide the CL-style return before `exec` consumes `tl`: a definition
        // returns its own name symbol; an expression returns its value below.
        let def_name: Option<String> = match core::op_sym(heap, tl).map(|s| s.well_known()) {
            // Every definition form but `defmethod` names itself in field 0;
            // `defmethod` names its owner there and the method in field 1.
            Some(wk::DEFUN) | Some(wk::DEFVAR) | Some(wk::DEFMACRO) | Some(wk::DEFSTRUCT)
            | Some(wk::DEFENUM) => {
                core::definition_field(heap, tl, 0).map(|p| p.last_segment().to_string())
            }
            Some(wk::DEFMETHOD) => match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => Some(heap.symbol_spelling(id).to_string()),
                _ => None,
            },
            _ => None,
        };
        let out = self.exec(heap, tl)?;
        let result: Value = match def_name {
            Some(n) => heap.intern_symbol(&n),
            None => match out {
                Some(rt) => rt,
                // `use`/`module`, or an empty body — nothing to hand back but `()`.
                None => Value::Empty,
            },
        };
        Ok(result_ok(heap, &EVAL_RESULT_TYPE, result))
    }
}


impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

/// Whether `type_name` is the built-in `Sexpr` type, whose values are
/// represented as a heap box rather than a Rust-side value.
fn is_sexpr_type(type_name: &Path) -> bool {
    crate::types::path_is_builtin(type_name, typelisp_mem::wk::SEXPR)
}

pub(crate) fn rt_i64(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(n) => Ok(*n),
        // An `int` argument in a machine-word position (an index, a count, a
        // shift distance): the type admits a bignum, the operation does not.
        // A language-level error, not an internal one — `(substring s 0
        // (expt 2 70))` is a program's mistake, reachable from source.
        Value::Boxed(_) => Err(EvalError::Panic("an integer argument does not fit a fixnum".into())),
        _ => Err(EvalError::Internal("sexpr: expected an i64 field".into())),
    }
}

pub(crate) fn rt_f64(heap: &Heap, v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_f64(*id) => Ok(heap.f64_value(*id)),
        _ => Err(EvalError::Internal("sexpr: expected an f64 field".into())),
    }
}

/// [`rt_f64`] for an `f32` field. An `f64` box is refused rather than
/// narrowed: the field's declared type said `f32`, so a value of the other
/// width arriving here is a bug, not something to round.
pub(crate) fn rt_f32(heap: &Heap, v: &Value) -> Result<f32, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_f32(*id) => Ok(heap.f32_value(*id)),
        _ => Err(EvalError::Internal("sexpr: expected an f32 field".into())),
    }
}

fn rt_char(v: &Value) -> Result<char, EvalError> {
    match v {
        Value::Char(c) => Ok(*c),
        _ => Err(EvalError::Internal("sexpr: expected a char field".into())),
    }
}

fn rt_bool(v: &Value) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(EvalError::Internal("sexpr: expected a bool field".into())),
    }
}

fn rt_str(heap: &Heap, v: &Value) -> Result<String, EvalError> {
    match v {
        Value::Str(id) => Ok(heap.string(*id).to_string()),
        _ => Err(EvalError::Internal("sexpr: expected a str field".into())),
    }
}

fn rt_ratio(heap: &Heap, v: &Value) -> Result<BigRational, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_ratio(*id) => Ok(heap.ratio_value(*id).clone()),
        _ => Err(EvalError::Internal("sexpr: expected a ratio field".into())),
    }
}

/// Walks a proper `Sexpr` list of `sym`s into a `Vec<SymRef>` —
/// `construct_sexpr`'s `SEXPR_PATH` arm's own reverse of
/// `match_sexpr_ctor`'s path-segments-to-list direction. Read-only (no
/// allocation): every `car` must already be a `Value::Symbol` and the `cdr`
/// chain must terminate in `Value::Empty`, or the path being constructed
/// isn't well-formed.
fn sexpr_list_to_symbols(heap: &Heap, mut v: Value) -> Result<Vec<SymRef>, EvalError> {
    let mut ids = Vec::new();
    loop {
        match v {
            Value::Empty => return Ok(ids),
            Value::Cons(_) => {
                match heap.car(v) {
                    Ok(Value::Symbol(id)) => ids.push(id),
                    _ => return Err(EvalError::Internal("sexpr: path segment is not a sym".into())),
                }
                v = heap.cdr(v).map_err(|e| EvalError::Internal(e.to_string()))?;
            }
            _ => return Err(EvalError::Internal("sexpr: path segments are not a proper list".into())),
        }
    }
}

/// Variant indices of `Sexpr`'s constructors, under the names this module
/// and `core_eval` match on. The numbers are `crate::sexpr_variant`'s.
use crate::sexpr_variant::{
    BOOL as SEXPR_BOOL, CHAR as SEXPR_CHAR, CONS as SEXPR_CONS, F32 as SEXPR_F32, F64 as SEXPR_F64, I16 as SEXPR_I16,
    I32 as SEXPR_I32, I8 as SEXPR_I8, INT as SEXPR_INT, NIL as SEXPR_NIL, PATH as SEXPR_PATH, RATIO as SEXPR_RATIO,
    RETIRED_BIGNUM as SEXPR_RETIRED_BIGNUM, STR as SEXPR_STR, SYM as SEXPR_SYM, U16 as SEXPR_U16, U32 as SEXPR_U32,
    U8 as SEXPR_U8, VECTOR as SEXPR_VECTOR, ARRAY as SEXPR_ARRAY, TUPLE as SEXPR_TUPLE,
};

/// The `(width, signed)` pair each narrow-integer `Sexpr` variant names, or
/// `None` for every other variant — the one table mapping between the two,
/// read by `match_sexpr_core`, `construct_sexpr_core` and the accessors.
pub(crate) fn narrow_variant(variant: usize) -> Option<typelisp_mem::fixed::FixedType> {
    use typelisp_mem::fixed::FixedType;
    Some(match variant {
        SEXPR_I8 => FixedType::I8,
        SEXPR_I16 => FixedType::I16,
        SEXPR_U8 => FixedType::U8,
        SEXPR_U16 => FixedType::U16,
        SEXPR_U32 => FixedType::U32,
        SEXPR_I32 => FixedType::I32,
        _ => return None,
    })
}


/// What a session has defined since it started recording.
struct Recording {
    /// The checker signature as of the moment recording started — the "before"
    /// side of the delta `(dump ...)` writes.
    baseline: crate::dump::RegistrySignature,
    /// The definitions, in the order they were executed, each rooted.
    forms: Vec<Value>,
    /// How many globals had compiled slots before the session began; the ones
    /// past this are the session's own.
    globals_before: usize,
}


/// `macroexpand`/`macroexpand-1`'s result type. It has a fixed signature, so
/// unlike a container method's there is nothing for a call site to say — but
/// the type still has to be the one the checker gives it, which
/// `tests/type_identity_guard_test.rs` checks against the registry. Its inner
/// `Option<Sexpr>` is [`TypeId::OPTION_SEXPR`].
pub const MACROEXPAND_RESULT_TYPE: TypeSpec = TypeSpec::Named(
    StdModule::RESULT,
    "Result",
    &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::SEXPR]), TypeSpec::Named(StdModule::EVAL, "EvalError", &[])],
);

/// `eval`'s result type — the same as `macroexpand`'s, and the same fixed
/// signature (`registry`'s `eval`: `Result<Option<Sexpr>, EvalError>`).
pub const EVAL_RESULT_TYPE: TypeSpec = MACROEXPAND_RESULT_TYPE;

/// `Ok(v)` of the result type `ty`, matching `result_def`'s variant order
/// (`Ok` = 0, `Err` = 1).
fn result_ok(heap: &mut Heap, ty: &TypeSpec, v: Value) -> Value {
    let ty = typelisp_mem::intern_spec(ty);
    crate::type_key::alloc_enum_of(heap, ty, 0, vec![v])
}


/// `Err(<ErrType>(msg))` — wraps `msg` in the concrete error type of the
/// failing built-in (`registry::builtin_error_defs`: `parseinterror`,
/// `parsefloaterror`, `readerror`, `evalerror`), whose single variant carries
/// exactly that string, before wrapping *that* in `Result`'s `Err` variant.
/// `err_type` must name one of those four — its variant index is 0, the only
/// one each has. `result` is the *whole* result type
/// (`Result<Option<sexpr>,evalerror>`), which is what the caller's patterns
/// test against.
fn result_err(heap: &mut Heap, result: &TypeSpec, err_type: u32, msg: String) -> Value {
    let msg_val = str_rt(heap, msg);
    let err_val = alloc_typed_enum(heap, &Path::builtin(err_type), 0, vec![msg_val]);
    // The `Result`'s own type, not the bare path: a value's identity carries
    // its instantiation, and `result` matches no `Result<Option<sexpr>,
    // evalerror>` pattern. `err_type` needs no such treatment — a concrete
    // error type has no type arguments — but the wrapper does, which is why
    // this takes two rather than one.
    let result = typelisp_mem::intern_spec(result);
    crate::type_key::alloc_enum_of(heap, result, 1, vec![err_val])
}


/// Runs `f` against this thread's registered `Interp`, or `None` when there
/// is none.
///
/// # Safety of the raw pointer
///
/// The slot is written immediately before every use (see
/// [`Interp::install_print_hooks`] and [`Interp::enter_compiled`]) by an
/// `&self` method, so the `Interp` it names is on the stack below this call
/// for the whole of `f`.
pub fn with_active_interp<T>(f: impl FnOnce(&Interp) -> T) -> Option<T> {
    let ptr = ACTIVE_INTERP.with(|cell| cell.get());
    if ptr.is_null() {
        return None;
    }
    Some(f(unsafe { &*ptr }))
}


thread_local! {
    /// The interpreter [`rt_apply_interpreted`] re-enters — registered by
    /// [`Interp::enter_compiled`] alongside the active heap, and thread-local
    /// for the identical reason (`cargo test` drives one `Interp` per
    /// thread, many per process).
    static ACTIVE_INTERP: std::cell::Cell<*const Interp> = const { std::cell::Cell::new(std::ptr::null()) };
}

/// No worker thread prints on WebAssembly, which has none, so there is no
/// snapshot of the printer's tables to keep (`worker_print`).
#[cfg(target_family = "wasm")]
impl Interp {
    pub fn printer_tables_changed(&self) {}

    fn refresh_print_snapshot(&self) {}
}
