//! The type checker: lowers read `Sexpr` values into a typed AST while checking.
//!
//! Checking is bidirectional in a small way: literal and constructor nodes may
//! be checked *against* an expected type (so integer literals adopt the expected
//! integer type and a nullary `None` learns its type argument), while everything
//! else synthesizes its own type and is reconciled against the expectation.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::{parse_type, prim_type_path, Error, Heap, Loc, Path, Type, Value};
use crate::name_lexer::{NameLexer, NameTok};

use super::ast::{Arm, CompileTarget, Expr, MacroLambda, Pattern, QuotedSexpr, Ref, Typed};
use super::registry::{AdtDef, AdtKind, AssocFn, FnSig, MacroDef, Namespace, Registry, TraitBound, TraitDef, VarInfo, Variant};

/// Expands a macro call *during* type-checking: `path` names a `defmacro`,
/// `raw_args` are the call's unevaluated argument forms (exactly as written —
/// this is what makes macros unhygienic/CL-style: no implicit quoting
/// conversion is needed since `Sexpr`'s runtime representation already *is*
/// the reader's raw `Value`). Returns the expansion to check in place of the
/// original call.
///
/// Defined here (in `check`, not `eval`) and implemented by `Interp` in
/// `eval/interp.rs`, so this module — pure static analysis otherwise — never
/// has to name the concrete `Interp` type. `eval` already depends on
/// `check`'s typed AST (`Expr`/`TopLevel`/`Typed`), so a direct `use
/// crate::eval::Interp` here would make the two modules mutually dependent;
/// this trait keeps that dependency one-directional.
/// Every Common Lisp comparison operator whose printed name contains a `<` or
/// `>`. The `char<`/`char<=`/`string<`/`string<=` family is syntactically
/// indistinguishable from an `ident<...>` generic header (an identifier
/// immediately followed by `<`), so it would otherwise be mis-parsed as a
/// malformed generic; the number family (`<`/`<=`/...) is already safe because
/// it *starts* with punctuation, but is listed here too for completeness. A
/// header whose whole name is one of these is always taken verbatim, with no
/// type parameters. (Reader case-folding means these are the lowercase forms.)
const CL_COMPARISON_OPERATORS: &[&str] = &[
    "<", ">", "<=", ">=",
    "char<", "char>", "char<=", "char>=",
    "string<", "string>", "string<=", "string>=",
];

/// Parse a definition header symbol into a name and its type-parameter names.
/// Accepts `name` (no params) or `name<T1,T2,...>` — the angle-bracket generic
/// syntax also used in type positions. A symbol that does not begin with an
/// identifier immediately followed by `<` (an operator name like `<=`, `->`,
/// or any name whose second token isn't `<`), or that is one of the known
/// [`CL_COMPARISON_OPERATORS`] (e.g. `char<`, `string<=`), is taken verbatim as
/// the name with no type parameters, so operator-named `defun`s keep working.
/// A name that *does* start `ident<` and isn't a known operator must be a
/// well-formed `ident<Ident,Ident...>`.
fn parse_generic_name_header(raw: &str) -> Result<(String, Vec<String>), Error> {
    // A known comparison operator (`char<`, `string<=`, ...) whose `<`/`>`
    // would otherwise read as the start of a generic-parameter list.
    if CL_COMPARISON_OPERATORS.contains(&raw) {
        return Ok((raw.to_string(), Vec::new()));
    }
    let mut toks = NameLexer::new(raw).peekable();
    let name = match toks.peek() {
        Some(NameTok::Ident(s)) => s.to_string(),
        // Punctuation-led name (`<=`, `<`, ...): verbatim, no generics.
        _ => return Ok((raw.to_string(), Vec::new())),
    };
    toks.next(); // consume the leading identifier
    match toks.peek() {
        None => return Ok((name, Vec::new())), // plain, non-generic name
        Some(NameTok::Lt) => {
            toks.next(); // consume '<'
        }
        // Second token isn't `<` (e.g. `->`, `foo::bar`, `a>b`): take verbatim.
        _ => return Ok((raw.to_string(), Vec::new())),
    }
    // From here a `<` was seen, so this must be a well-formed generic header.
    let mut params = Vec::new();
    loop {
        match toks.next() {
            Some(NameTok::Ident(s)) => params.push(s.to_string()),
            _ => {
                return Err(Error::TypeError(format!(
                    "definition name `{}`: type parameter must be a simple identifier",
                    raw
                )))
            }
        }
        match toks.next() {
            Some(NameTok::Comma) => continue,
            Some(NameTok::Gt) => break,
            _ => {
                return Err(Error::TypeError(format!(
                    "definition name `{}`: malformed `<...>` type parameter list",
                    raw
                )))
            }
        }
    }
    if toks.next().is_some() {
        return Err(Error::TypeError(format!(
            "definition name `{}`: unexpected tokens after `>`",
            raw
        )));
    }
    Ok((name, params))
}

pub trait MacroExpander {
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String>;
}

/// A checked top-level form.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TopLevel {
    /// A `defun`: fully-qualified [`Path`], typed parameters, return type, body.
    /// `type_params` are the names declared by `(defun (name T1 T2...) ...)`
    /// (empty for an ordinary, non-generic function) — the evaluator ignores
    /// them (type information is erased before execution; see
    /// `Checker::check_call`).
    /// `params` includes a trailing `&rest` parameter's name last (bound to
    /// a plain `Sexpr` list), exactly like `Defmacro::params` — see
    /// `Checker::check_defun`/`parse_params_rest`.
    Defun {
        name: Path,
        type_params: Vec<String>,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
        /// Whether this `defun` is `pub` — the runtime twin of `FnSig::public`
        /// (`check::registry`), baked in here so `eval::scope::ModuleScope`'s
        /// qualified-path resolution can enforce visibility without the
        /// checker's `Registry` at runtime (see `eval::scope`'s module doc
        /// comment).
        public: bool,
    },
    /// A `defmethod`: instance or static associated function of a type.
    Defmethod {
        type_name: Path,
        method: String,
        instance: bool,
        /// The receiver's variable name (e.g. `self`) for instance methods.
        self_name: Option<String>,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Vec<Typed>,
        /// The owner type's parameters as the receiver spelled them
        /// (`(self Option<U>)` -> `["u"]`) when this is a *generic-owner*
        /// method — the erased, diagnostics-only artifact `Interp::exec`
        /// must skip, exactly like `Defun::type_params`; empty for a
        /// concrete method or a generated specialization.
        type_params: Vec<String>,
        /// Whether this `defmethod` is `pub` — see `Defun::public`'s doc
        /// comment; the runtime twin of `AssocFn.sig.public`.
        public: bool,
    },
    /// A `defmacro`: a compile-time, `Sexpr`-to-`Sexpr` code transformer.
    /// Stored as an ordinary callable body — calling it (at macro-expansion
    /// time, via [`MacroExpander`]) is identical to calling a `defun`. `params`
    /// lists every binding name in order: required, then `&optional`, then the
    /// `&rest` name (when `rest` is true), then `&key` names — the `&optional`/
    /// `&rest`/`&key` markers themselves dropped. `lambda` describes how the
    /// non-required regions are filled at expansion time (default-value bodies,
    /// keyword matching); see [`Checker::check_defmacro`] and [`MacroLambda`].
    Defmacro { name: Path, params: Vec<String>, body: Vec<Typed>, rest: bool, lambda: MacroLambda, public: bool },
    /// A `defvar`/`defconstant`: a global variable (`mutable`) or constant.
    Defvar { name: Path, ty: Type, value: Typed, mutable: bool, public: bool },
    /// A `module`: a namespace and its checked body forms.
    Module { path: Path, body: Vec<TopLevel> },
    /// A `use`: a name brought into the current scope (alias -> target path).
    Use { alias: Path, target: Path },
    /// A `defstruct`: registers a single-variant `AdtKind::Struct` type
    /// (`Checker::check_defstruct`). Carries only the name — unlike
    /// `Defun`/`Defmethod`, there's no body to run; the registry mutation
    /// already happened at check time, and the constructor/field accessors
    /// are ordinary `Type::ctor`/`defmethod` machinery reached through
    /// `Expr::Construct`/`Assoc` at use sites, not through this node.
    /// `Interp::exec` treats it as a no-op, the same as `Option`/`Result`
    /// needing no runtime registration of their own.
    Defstruct { name: Path },
    /// A `defenum`: registers a multi-variant `AdtKind::Sum` type
    /// (`Checker::check_defenum`). Variant construction/`match` are the same
    /// `Expr::Construct`/`Expr::Match` machinery `Option`/`Result` already
    /// use, and unlike `Defstruct` the name is *not* added to the
    /// interpreter's `struct_types` set: an enum instance is an immutable
    /// `RtValue::Data`, never a boxed struct. `params`/`variants` are the
    /// type's declared parameters and each variant's field types, baked in
    /// at check time (the checker always knows the types — the same
    /// bake-into-the-AST principle `Expr::Construct`'s resolved fields
    /// follow) so `Interp::exec` can record them in `Interp::enum_defs`:
    /// the compiled-global boundary's box -> `RtValue::Data` decode
    /// (`decode_data_value`) needs each variant's field types, and the
    /// checker's `Registry` no longer exists by then.
    Defenum { name: Path, params: Vec<String>, variants: Vec<Variant> },
    /// A bare top-level expression.
    Expr(Typed),
    /// `(load "path")` — a CL-style flat load of another file's forms into
    /// the *current* namespace (unlike `use`, which wraps a file in its own
    /// module). The driver resolves `path` to a `.fasl` (compiled) or `.typl`
    /// (source) file and loads it before checking subsequent forms; the
    /// checker only records the request (it can't do file I/O itself). See
    /// `crate::project::load_file_flat`.
    Load { path: String },
}

/// A `&rest` parameter's `(name, elem-type, name's source position)`, as
/// returned by [`Checker::parse_params_rest`] alongside the fixed params.
type RestParam = (String, Type, Option<Loc>);

/// The call-site arity shape of a macro, derived from its [`MacroDef`] by
/// [`Checker::resolve_macro`]/[`resolve_macro_path`](Checker::resolve_macro_path)
/// and consumed by [`Checker::check_macro_arity`]. Carries only what a raw
/// argument *count* can be judged against — the `&key` names themselves aren't
/// needed here (keyword validation happens in `Interp::expand_macro`).
#[derive(Clone, Copy)]
struct MacroShape {
    required: usize,
    optional: usize,
    rest: bool,
    has_keys: bool,
}

impl MacroShape {
    fn of(def: &MacroDef) -> MacroShape {
        MacroShape { required: def.required, optional: def.optional, rest: def.rest, has_keys: !def.keys.is_empty() }
    }
}

/// The variables a pattern binds: each `(name, type, name's source position)`,
/// as returned by [`Checker::check_pattern`]/[`Checker::check_ctor_pattern`].
/// The `Loc` lets a later reference in a `match` arm body resolve
/// goto-definition back to the binding site.
type PatternBindings = Vec<(String, Type, Option<Loc>)>;

/// The generic templates [`Checker::export_templates`] hands to the fasl
/// serializer: free functions and (type-name-qualified) methods, each paired
/// with its heap-independent [`crate::fasl`] representation.
type ExportedTemplates = (
    Vec<(Path, crate::fasl::FnTemplateRepr)>,
    Vec<(Path, String, crate::fasl::MethodTemplateRepr)>,
);

/// A lexical environment mapping variable names to their types.
#[derive(Clone)]
struct Env {
    /// Each binding's name, type, and — where known — the source position of
    /// the name in its binding form (a `let`/`let*` binding, a `lambda`/
    /// `labels`/`defun`/`defmethod` parameter or receiver name, a `labels`
    /// function name, or a `match`-pattern binding). `None` for a binding
    /// this checker doesn't bother tracking a position for
    /// (monomorphization's synthesized re-checks) — consulted by
    /// `Checker::check_at` to populate `DefLocs::local_refs` when a `Var`
    /// reference resolves here.
    vars: Vec<(String, Type, Option<Loc>)>,
    /// Trait bounds on this function's own generic type parameters, declared
    /// by a `(where (Trait T (Assoc Concrete)...)...)` clause
    /// (`Checker::check_defun`) — e.g. `{"t": [TraitBound { trait_path:
    /// Path::root("iter"), assoc: {} }]}`. Fixed for the lifetime of one
    /// function body (a nested `let`/`lambda` never introduces new type
    /// parameters), so `extended` just carries the same `Rc` forward instead
    /// of letting it vary per-binding the way `vars` does. Consulted only by
    /// `Checker::check_instance_method`'s type-variable-receiver branch — see
    /// its doc comment. The same bounds are also stored on the registered
    /// `FnSig` (`Checker::check_defun`), for `Checker::check_call`'s
    /// call-site validation — this copy exists only because body-checking
    /// needs it readily available in `Env`, not threaded through `Registry`
    /// lookups on every method call.
    bounds: std::rc::Rc<HashMap<String, Vec<TraitBound>>>,
}

impl Env {
    fn new() -> Env {
        Env { vars: Vec::new(), bounds: std::rc::Rc::new(HashMap::new()) }
    }

    fn get(&self, name: &str) -> Option<&Type> {
        self.vars.iter().rev().find(|(n, _, _)| n == name).map(|(_, t, _)| t)
    }

    /// The nearest binding of `name`'s own recorded source position, if any —
    /// see [`Self::vars`]'s doc comment. Consulted by `Checker::check_at`
    /// alongside [`Self::get`] when a `Var` reference resolves to a local.
    fn get_loc(&self, name: &str) -> Option<Loc> {
        self.vars.iter().rev().find(|(n, _, _)| n == name).and_then(|(_, _, l)| l.clone())
    }

    /// A child environment with `binds` added (later bindings shadow earlier),
    /// with no recorded position for any of them — used by callers that don't
    /// track individual binding positions (monomorphization's synthesized
    /// re-checks). See [`Self::extended_with_locs`] for the counterpart that
    /// does.
    fn extended(&self, binds: Vec<(String, Type)>) -> Env {
        self.extended_with_locs(binds.into_iter().map(|(n, t)| (n, t, None)).collect())
    }

    /// Like [`Self::extended`], but each binding also carries the source
    /// position of its name in the binding form — used by `let`/`let*`,
    /// `lambda`/`labels` parameters, and `defun`/`defmethod` parameters and
    /// receivers, so a later reference to the bound name can resolve back to
    /// where it was bound (`Checker::check_at`, `DefLocs::local_refs`).
    fn extended_with_locs(&self, binds: Vec<(String, Type, Option<Loc>)>) -> Env {
        let mut vars = self.vars.clone();
        vars.extend(binds);
        Env { vars, bounds: self.bounds.clone() }
    }

    /// A child environment that additionally declares `bounds` (a function's
    /// own `where`-clause trait bounds) — see the field's doc comment.
    fn with_bounds(&self, bounds: HashMap<String, Vec<TraitBound>>) -> Env {
        Env { vars: self.vars.clone(), bounds: std::rc::Rc::new(bounds) }
    }
}

/// Bundles [`Checker::check_assoc_call`]'s arguments to keep its arity down.
struct AssocCall<'a> {
    type_fq: &'a Path,
    method: &'a str,
    /// `Some` for an instance call (already-checked receiver, fills the
    /// method's first parameter slot); `None` for a static call.
    receiver: Option<Typed>,
    /// The call site's expected type. Only consulted for a static call with
    /// no receiver to read concrete type arguments from (e.g. `HashTable::new`
    /// learning `K`/`V` the same way a field-less constructor like `None`
    /// learns its type argument from `expected`).
    expected: Option<&'a Type>,
}

/// What happens when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
/// reuses a name already defined by *non-builtin* code (the same module, in
/// the same namespace). Redefining a builtin (`with_builtins`-registered) is
/// always an error, regardless of this setting — see [`Checker::check_redef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RedefPolicy {
    /// Allow the redefinition, but record a warning (`Checker::take_warnings`).
    #[default]
    Warn,
    /// Reject the redefinition with a `TypeError`.
    Error,
    /// Allow the redefinition silently.
    Silent,
}

/// Anything that can occupy a name in one of a [`Namespace`]'s tables (or a
/// type's `assoc` table), so [`Checker::check_redef`] can be written once and
/// shared by every `check_def*` instead of duplicating the warn/error/silent
/// decision at each call site.
trait Definable {
    fn builtin(&self) -> bool;
}
impl Definable for FnSig {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for MacroDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for VarInfo {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for AdtDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for AssocFn {
    fn builtin(&self) -> bool {
        self.builtin
    }
}
impl Definable for TraitDef {
    fn builtin(&self) -> bool {
        self.builtin
    }
}

/// A generic `defun`'s retained raw source form, for re-checking at each
/// concrete instantiation — see [`Checker::specialize_defun`]. The checked
/// `Typed` body can't serve this purpose: it was lowered once with the type
/// variables still abstract, and everything the checker *derives* from types
/// (`Expr::Construct::mutable`, `Pattern::Ctor::sexpr_fields`, method
/// resolution, `Expr::TraitCall` vs `Expr::Assoc`) would have to be re-derived
/// anyway — re-running the checker on the source with the type variables
/// bound (see `Checker::type_var_bindings`) gets all of that for free.
#[derive(Clone)]
struct FnTemplate {
    /// The `defun` form's parts (everything after the `defun` symbol),
    /// exactly as `check_defun` received them. Each `Value` is permanently
    /// rooted in the heap (`Heap::push_permanent_root`) — a template must
    /// outlive any number of GC cycles between its definition and its last
    /// instantiation.
    parts: Vec<Value>,
    /// The namespace the `defun` was defined in — specialization re-checks
    /// the body under the *defining* module's name resolution, not the
    /// caller's.
    ns: Vec<String>,
    type_params: Vec<String>,
}

/// One queued "generate this instantiation" work item — see
/// [`Checker::request_fn_specialization`]/[`Checker::request_method_specialization`]
/// (producers, called from the `&self` expression-checking context) and
/// [`Checker::drain_specializations`] (consumer, run by `check_form` with
/// `&mut self`). The two-phase split exists because expression checking
/// (`check_call`/`check_assoc_call`) cannot re-enter top-level definition
/// checking itself.
enum SpecRequest {
    /// A generic free function (`FnTemplate`) at concrete `args`.
    Fn { base: Path, args: Vec<Type>, mangled: Path },
    /// A generic-owner associated method (`MethodTemplate`, keyed
    /// `(type_fq, base)`) at the owner's concrete type arguments `args`.
    Method { type_fq: Path, base: String, args: Vec<Type>, mangled: String },
}

/// A generic *type*'s associated method, retained for per-instantiation
/// re-generation — the method-side counterpart of [`FnTemplate`]. A method
/// is generic through its *owner* (`Option<T>`'s `unwrap`, a generic
/// `defstruct`'s accessors), never through parameters of its own, so the
/// instantiation arguments are always the owner's type arguments.
#[derive(Clone)]
enum MethodTemplate {
    /// A written `defmethod` whose receiver names the owner's type
    /// parameters (directly, or synthesized by `check_impl`'s
    /// `Self`-substitution). `written_vars` are the type-variable names *as
    /// the receiver spelled them* (`(self Option<U>)` -> `["u"]`), zipped
    /// positionally with the owner's concrete type arguments at
    /// specialization time — they need not match the `AdtDef::params`
    /// names the call site's substitution is keyed by.
    Form { parts: Vec<Value>, ns: Vec<String>, written_vars: Vec<String> },
    /// A generic `defstruct` field getter — there is no raw form to
    /// re-check (`check_defstruct` synthesizes accessor ASTs directly), so
    /// specialization re-synthesizes the `FieldGet` with the field type
    /// substituted from the owner's `AdtDef`.
    Getter { index: usize },
    /// The matching `set-<field>` setter (`FieldSet`).
    Setter { index: usize },
}

/// [`Checker::parse_defmethod_sig`]'s output — a `defmethod` form's parsed
/// header, shared between definition checking and template specialization.
struct MethodSig {
    method: String,
    instance: bool,
    self_name: Option<String>,
    /// The receiver name's own source position (`(self Type)`'s `self`), for
    /// the checked body's `Env` binding — see `Checker::check_defmethod`'s
    /// use of `Env::extended_with_locs`.
    self_name_loc: Option<Loc>,
    recv_ty: Type,
    type_fq: Path,
    params: Vec<(String, Type)>,
    /// Each of `params`'s name's own source position, parallel to `params` —
    /// see `Checker::parse_param_pairs`.
    param_locs: Vec<Option<Loc>>,
    ret: Type,
    /// `(where (Trait TypeVar ...))` bounds after the return type — same
    /// syntax and parse as a free `defun`'s (`Checker::parse_defun_sig`).
    /// Lets an `impl` method on a generic owner require trait bounds on the
    /// owner's own type variables (e.g. `cons-cell<A,B>`'s `equals` needing
    /// `(where (Eq A) (Eq B))` for its recursive field comparisons).
    bounds: HashMap<String, Vec<TraitBound>>,
    /// Index of the first body form in the `defmethod` parts: 4 when a
    /// `where` clause is present, 3 otherwise.
    body_start: usize,
}

/// The synthetic `TopLevel::Module` path under which `check_form` bundles
/// monomorphized specializations together with the form that requested them.
/// Contains a space, which [`crate::read::Reader`] treats as a delimiter — so
/// no user-written `module` form can ever collide with it (see
/// `mangled_fn_path` for the same trick on function names).
pub const MONO_BUNDLE_MODULE: &str = "<monomorph specializations>";

/// Upper bound on specializations generated while draining one top-level
/// form's requests — the convergence guard for polymorphic recursion (a
/// generic function calling itself at an ever-growing type, e.g. `(f T)`
/// recursing as `(f Option<T>)`), which memoization alone cannot stop since
/// every instantiation is new. Ordinary programs instantiate a handful of
/// generics per form; hitting this is a `TypeError`, not a hang.
const SPECIALIZATION_BUDGET: usize = 512;

/// Opaque saved namespace context — see [`Checker::suspend_ns_context`].
pub struct NsContext {
    ns: Vec<String>,
    file_ns: Vec<Vec<String>>,
}

/// The type checker, holding the data-type and function registries plus the
/// current namespace (module) path.
pub struct Checker {
    reg: Registry,
    ns: Vec<String>,
    /// Stack of enclosing *files'* own module paths, innermost last — set by
    /// [`Self::enter_file_module`]/[`Self::exit_file_module`] (`project.rs`'s
    /// file-level wrapping only, never [`Self::enter_module`]'s nested
    /// `(module ...)` forms), so it stays fixed at "the loading file's own
    /// path" even while `self.ns` grows deeper through a nested `module`
    /// block inside that file. [`Self::find_module`]'s sibling-file
    /// resolution tier reads the top of this stack (never `self.ns` itself)
    /// so `(use foo)` inside a nested `module` block still resolves `foo`
    /// relative to the *file's* directory, not that inner block's own
    /// namespace (which has no filesystem counterpart to be a sibling of).
    /// A recursively-loaded dependency pushes and fully pops its own frame
    /// before the file that `use`s it resumes checking (`project.rs`'s
    /// scan-before-check ordering), so this never needs more than simple
    /// stack discipline.
    file_ns: Vec<Vec<String>>,
    /// Stack of enclosing loops' accumulated exit type, innermost last.
    /// `break`/`return` unify their (optional) value's type into the top
    /// frame; `while`/`dotimes`/`dolist` seed it with `Unit` (their fixed
    /// result type), `loop` seeds it with `Never` (refined by any exit found).
    /// `lambda` bodies see an empty stack — `break`/`return` cannot cross a
    /// function boundary (there is no labelled non-local exit in this
    /// language, only "nearest enclosing loop").
    loop_stack: RefCell<Vec<Type>>,
    /// What to do when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
    /// reuses a non-builtin name — see [`RedefPolicy`]. Defaults to `Warn`;
    /// override with [`Self::set_redef_policy`] (e.g. from a CLI flag).
    redef_policy: RedefPolicy,
    /// Non-fatal diagnostics accumulated by [`Self::check_redef`] (currently
    /// just `RedefPolicy::Warn` redefinitions); drained by [`Self::take_warnings`].
    warnings: RefCell<Vec<String>>,
    /// When true, the checker runs in *error-recovery* mode: at the handful of
    /// recovery boundaries (`check_form`, `drain_specializations`, `check_seq`,
    /// `check_let`, `check_match`) an `Err` from a sub-check is recorded in
    /// [`Self::errors`] and replaced by a `Never`-typed hole node
    /// ([`Self::hole`]) so checking of the rest of the form/file continues and
    /// a partial [`Typed`] tree still comes back. Defaults to `false`, which
    /// reproduces the strict "abort on first error" behaviour the CLI, REPL,
    /// prelude, and the test suite depend on — recovery is used only by the
    /// LSP (`diagnostics_for`/`candidates_for`), which needs a best-effort tree
    /// for hover/goto-definition/completion even in the presence of type errors.
    recover: bool,
    /// Recoverable errors accumulated while `recover` is set — mirrors
    /// `warnings`. Each carries its own innermost source location (via the
    /// `check_at`/`check_form` `Error::at` tagging that already runs before the
    /// error reaches a boundary). Drained by [`Self::take_errors`]. Always
    /// empty when `recover` is false.
    errors: RefCell<Vec<Error>>,
    /// Scratch accumulator for `DefLocs::local_refs`, written by
    /// [`Self::check_at`] whenever a `Var` reference resolves against an
    /// `Env` binding with a recorded position. A `RefCell` for the same
    /// reason as `loop_stack`/`warnings`: `check_at`/`check_inner` recurse
    /// through `&self`, not `&mut self`. Flushed into `self.reg.def_locs.
    /// local_refs` at the end of every [`Self::check_form`] (a `&mut self`
    /// boundary), so the registry's copy — the one the LSP snapshots — stays
    /// current without every recursive `check_*` helper needing `&mut self`.
    local_refs: RefCell<HashMap<(u32, u32), Loc>>,
    /// Every generic `defun`'s retained source form, keyed by its
    /// fully-qualified path — see [`FnTemplate`].
    generic_fn_templates: HashMap<Path, FnTemplate>,
    /// Every generic-owner method's retained template, keyed by
    /// `(owner type path, method name)` — see [`MethodTemplate`].
    generic_method_templates: HashMap<(Path, String), MethodTemplate>,
    /// Instantiations already requested *during the current top-level form* —
    /// a free function as `(mangled path, None)`, a method as
    /// `(owner type path, Some(mangled method))` — so one form's many calls
    /// to the same instantiation — including a specialization's own
    /// recursive call to itself — produce exactly one definition.
    /// Deliberately cleared at the end of every `check_form` rather than
    /// kept for the checker's lifetime: each form's output bundle is then
    /// self-contained, so a caller that checks several forms but discards
    /// some without executing them (the REPL does exactly this when a later
    /// form in the same paste fails to check) can never leave a later form
    /// referring to a specialization whose defining bundle was thrown away.
    /// The cost — a later form re-generating an instantiation an earlier
    /// form already produced, and `Interp::exec` silently overwriting the
    /// identical earlier registration — is compile-time-only.
    spec_memo: RefCell<HashSet<(Path, Option<String>)>>,
    /// Instantiations requested but not yet generated — see [`SpecRequest`].
    /// Always empty outside `check_form`.
    spec_pending: RefCell<Vec<SpecRequest>>,
    /// The current specialization's type-variable bindings (`T` -> concrete
    /// type), consulted by [`Self::canon`] — the one chokepoint every parsed
    /// type annotation passes through, which is what makes "re-check the
    /// template with its type variables bound" work without touching the
    /// type parser: `Option<T>`'s `T` arrives here already split out as a
    /// structured `Type::Named` argument. Non-empty only while
    /// [`Self::specialize_defun`] runs.
    type_var_bindings: HashMap<String, Type>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            reg: Registry::with_builtins(),
            ns: Vec::new(),
            file_ns: Vec::new(),
            loop_stack: RefCell::new(Vec::new()),
            redef_policy: RedefPolicy::default(),
            warnings: RefCell::new(Vec::new()),
            recover: false,
            errors: RefCell::new(Vec::new()),
            local_refs: RefCell::new(HashMap::new()),
            generic_fn_templates: HashMap::new(),
            generic_method_templates: HashMap::new(),
            spec_memo: RefCell::new(HashSet::new()),
            spec_pending: RefCell::new(Vec::new()),
            type_var_bindings: HashMap::new(),
        }
    }

    /// Overrides the default [`RedefPolicy`] (`Warn`) for non-builtin
    /// redefinitions. Redefining a builtin is always an error regardless.
    pub fn set_redef_policy(&mut self, policy: RedefPolicy) {
        self.redef_policy = policy;
    }

    /// Drains and returns every warning recorded so far (e.g. by a
    /// `RedefPolicy::Warn` redefinition) — call after each `check_form` to
    /// surface them (see `main.rs`/`prelude.rs`).
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
    }

    /// Turns error-recovery mode on or off (see [`Self::recover`]). The LSP
    /// enables it before a check so a file with type errors still yields a
    /// partial tree and a full list of errors; everything else leaves it off
    /// and keeps the strict "abort on first error" behaviour.
    pub fn set_recover(&mut self, on: bool) {
        self.recover = on;
    }

    /// Set the namespace that subsequent `check_form`/`check_form_at` calls
    /// treat as "current". Used by the CLI's `run_file` to place a runtime
    /// `(eval ...)` in the script's own file-derived module (so the script's
    /// module-scoped globals/functions resolve from an eval'd form, and
    /// eval-defined names register there too), matching how those forms were
    /// checked at load time. The REPL leaves this at the root (`[]`), where it
    /// checks its own input, so REPL `eval` resolves against the root
    /// environment unchanged.
    pub fn set_current_ns(&mut self, ns: Vec<String>) {
        self.ns = ns;
    }

    /// Drains and returns every error accumulated at a recovery boundary while
    /// `recover` was set (see [`Self::errors`]). Each already carries its own
    /// source location. Empty unless `recover` is enabled.
    pub fn take_errors(&self) -> Vec<Error> {
        std::mem::take(&mut *self.errors.borrow_mut())
    }

    /// A `Never`-typed placeholder for a sub-expression that failed to check.
    /// Reuses [`Expr::Panic`] (already `Never`-typed, already handled by every
    /// downstream consumer — the interpreter, `ast_bridge`, `locate.rs`) rather
    /// than adding a new `Expr` variant, so recovery needs no changes outside
    /// the checker. `Type::Never` unifies with any expected type, so a hole
    /// flowing into a typed position never produces a cascade of follow-on
    /// errors. `loc` is the failing form's position, so `locate_node` can still
    /// land the cursor on it for completion/hover.
    fn hole(loc: Option<Loc>) -> Typed {
        Typed {
            expr: Expr::Panic(Box::new(Typed::new(Expr::Str("<check-error>".into()), Type::Str))),
            ty: Type::Never,
            loc,
        }
    }

    /// The single catch helper used at expression-level recovery boundaries: a
    /// pass-through in strict mode, and in `recover` mode it records a failing
    /// sub-check's error (tagged with `loc` if it lacks a more specific one)
    /// and substitutes a [`Self::hole`] so checking continues.
    fn recovered(&self, r: Result<Typed, Error>, loc: Option<Loc>) -> Result<Typed, Error> {
        match r {
            Err(e) if self.recover => {
                self.push_recovered(e, loc.clone());
                Ok(Self::hole(loc))
            }
            other => other,
        }
    }

    /// Records a recoverable error at a boundary that has no `Typed` slot to
    /// fill with a hole (a `match` arm that gets skipped, or the exhaustiveness
    /// check). Tags with `loc` (a no-op if the error already carries a more
    /// specific location) and pushes into [`Self::errors`]. Callers must have
    /// already checked `self.recover`.
    fn push_recovered(&self, e: Error, loc: Option<Loc>) {
        let e = match loc {
            Some(l) => e.at(l),
            None => e,
        };
        self.errors.borrow_mut().push(e);
    }

    /// The one place the warn/error/silent (and "never touch a builtin")
    /// decision actually lives, given only whether the name already in use
    /// is a builtin. [`Self::check_redef`] (the `Definable`-generic path used
    /// by `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`) and
    /// `check_use`'s constructor/static-method snapshot (whose tables,
    /// `Namespace::ctors`/`static_uses`, are plain `(Path, ..)` tuples with
    /// no `Definable` impl of their own — "builtin" there means "owned by a
    /// builtin type") both reduce to calling this.
    fn check_redef_outcome(&self, kind: &str, name: &str, is_builtin: bool) -> Result<(), Error> {
        if is_builtin {
            return Err(Error::TypeError(format!("cannot redefine built-in {} `{}`", kind, name)));
        }
        match self.redef_policy {
            RedefPolicy::Error => Err(Error::TypeError(format!("{} `{}` is already defined", kind, name))),
            RedefPolicy::Warn => {
                self.warnings.borrow_mut().push(format!("warning: redefining {} `{}`", kind, name));
                Ok(())
            }
            RedefPolicy::Silent => Ok(()),
        }
    }

    /// The one place that decides whether a `defun`/`defmethod`/`defmacro`/
    /// `defvar`/`defstruct` may reuse `name` (already a `kind`, e.g.
    /// `"function"`/`"type"`), given whatever already occupies that name in
    /// the table it's about to be inserted into (`None` if the name is free).
    /// A builtin is never redefinable; anything else follows `redef_policy`.
    fn check_redef<T: Definable>(&self, kind: &str, name: &str, existing: Option<&T>) -> Result<(), Error> {
        match existing {
            None => Ok(()),
            Some(e) => self.check_redef_outcome(kind, name, e.builtin()),
        }
    }

    /// Check one top-level form. Definition forms (`defun`/`module`/
    /// `defmethod`/`use`) register into the current namespace; anything else is
    /// checked as an expression.
    ///
    /// If checking the form instantiated any generic function at concrete
    /// types (see [`Self::request_fn_specialization`]), the generated
    /// specializations are bundled *with* the form into a single synthetic
    /// `TopLevel::Module` (path [`MONO_BUNDLE_MODULE`], specializations
    /// first) — `Interp::exec`'s `Module` arm runs the body in order, so the
    /// caller needs no new handling, and the bundle is self-contained: a
    /// form's specializations can never be separated from the form that
    /// needs them (see `spec_memo`'s doc comment for why that matters).
    pub fn check_form(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value) -> Result<TopLevel, Error> {
        self.check_form_at(heap, interp, v, None)
    }

    /// [`Self::check_form`] with a caller-supplied location for `v` itself.
    /// A list form carries its own recorded location (`heap.cons_loc`) and
    /// ignores the hint, but a *bare atom* at top level (e.g. a lone `42`)
    /// has no heap identity to key a location on — the reader hands its span
    /// alongside the value (`Reader::read_all_in_spanned`) and this is where
    /// it enters the checker. Mirrors [`Self::check_at`] vs [`Self::check`].
    pub fn check_form_at(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value, loc_hint: Option<Loc>) -> Result<TopLevel, Error> {
        debug_assert!(
            self.spec_pending.borrow().is_empty(),
            "specialization requests must never leak across check_form calls"
        );
        // Location of this top-level form, attached to any error that lacks a
        // more specific one (a body-expression error already carries the
        // deeper location from `check`, and `Error::at` keeps that innermost
        // one — see its doc comment).
        let loc = heap.cons_loc(v).or(loc_hint);
        let primary = self.check_form_dispatch(heap, interp, v, loc.clone());
        // Specialization-drain recovery boundary (B6): the primary form checked
        // fine, but instantiating a generic it calls failed. In `recover` mode
        // keep the primary form's typed tree (it's what completion/hover want)
        // and drop only the failed bundle, rather than losing the whole form.
        // Monomorphization itself stays strict — a template re-check error's
        // location points into the (possibly foreign) template, so it is
        // recorded as-is rather than turned into an in-body hole.
        let bundled = match primary {
            Err(e) => Err(e),
            Ok(tl) => match self.drain_specializations(heap, interp) {
                Ok(specs) => Ok((specs, tl)),
                Err(e) if self.recover => {
                    self.spec_pending.borrow_mut().clear();
                    let e = match loc.clone() {
                        Some(l) => e.at(l),
                        None => e,
                    };
                    self.errors.borrow_mut().push(e);
                    Ok((Vec::new(), tl))
                }
                Err(e) => Err(e),
            },
        };
        // Both maps reset per form regardless of outcome — see `spec_memo`'s
        // doc comment for why the memo must not outlive the form.
        self.spec_memo.borrow_mut().clear();
        // Publish this form's local-variable reference resolutions (see
        // `local_refs`'s doc comment) into the registry's copy — even on a
        // form that ultimately errors, whatever resolved before the failure
        // point is still valid and worth keeping (mirrors how a partially-
        // checked form's earlier registry mutations, e.g. `def_locs.fns`,
        // aren't rolled back either).
        self.reg.def_locs.local_refs.extend(self.local_refs.borrow_mut().drain());
        let result = match bundled {
            Ok((specs, tl)) if specs.is_empty() => Ok(tl),
            Ok((mut specs, tl)) => {
                specs.push(tl);
                Ok(TopLevel::Module { path: Path::root(MONO_BUNDLE_MODULE), body: specs })
            }
            Err(e) => {
                self.spec_pending.borrow_mut().clear();
                Err(e)
            }
        };
        let result = match loc.clone() {
            Some(loc) => result.map_err(|e| e.at(loc)),
            None => result,
        };
        // Top-level recovery boundary (B5): in `recover` mode a form that fails
        // to check must not abort the whole file — record its (already
        // location-tagged) error and hand back a hole so `load_source_inner`
        // keeps checking the remaining forms and still pushes this file's
        // `TopLevel::Module`, which is what gives the LSP a partial tree. The
        // `spec_pending` clear mirrors the `Err(e)` arm above: without it the
        // `debug_assert!` at the top of the next `check_form` would fire on a
        // request that leaked out of a failed specialization drain.
        match result {
            Err(e) if self.recover => {
                self.spec_pending.borrow_mut().clear();
                self.errors.borrow_mut().push(e);
                Ok(TopLevel::Expr(Self::hole(loc)))
            }
            other => other,
        }
    }

    /// [`Self::check_form`]'s dispatch body (the pre-monomorphization
    /// `check_form`, unchanged) — split out so the public entry point can
    /// wrap it with specialization draining. `def_loc` is `v`'s own source
    /// location (already computed by the caller via `heap.cons_loc(v)`),
    /// threaded down into whichever `check_def*` this form dispatches to so
    /// it can record where the name it registers was defined (`Registry::
    /// def_locs`, consulted by the LSP's goto-definition).
    fn check_form_dispatch(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value, def_loc: Option<Loc>) -> Result<TopLevel, Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec(v)?;
            // Per-element locations parallel to `elems`, so a `def*` body form
            // that is a bare atom (e.g. `(defun id ((x i32)) i32 x)`, whose
            // body is a lone parameter reference) keeps its own `Loc` for
            // hover — threaded to the body-checking helpers as `parts_locs`.
            let elem_locs: Vec<Option<Loc>> = heap.list_to_vec_locs(v)?.into_iter().map(|(_, l)| l).collect();
            let parts_locs = &elem_locs[1..];
            if let Some(Value::Symbol(id)) = elems.first() {
                match heap.symbol_name(*id) {
                    "pub" => return self.check_pub(heap, interp, &elems[1..], parts_locs, def_loc),
                    "defun" => return self.check_defun(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    "defvar" => return self.check_defvar(heap, interp, &elems[1..], true, false, def_loc),
                    "defconstant" => return self.check_defvar(heap, interp, &elems[1..], false, false, def_loc),
                    "defmacro" => return self.check_defmacro(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    "module" => return self.check_module(heap, interp, &elems[1..]),
                    "defmethod" => return self.check_defmethod(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    "defstruct" => return self.check_defstruct(heap, &elems[1..], false, def_loc),
                    "defenum" => return self.check_defenum(heap, &elems[1..], false, def_loc),
                    "deftrait" => return self.check_deftrait(heap, &elems[1..], false, def_loc),
                    "impl" => return self.check_impl(heap, interp, &elems[1..], false),
                    "use" => return self.check_use(heap, &elems[1..]),
                    "load" => return self.check_load(heap, &elems[1..]),
                    _ => {}
                }
            }
            // A top-level macro call: expand and re-dispatch the expansion as
            // a top-level form — a macro can produce `use`/`defun`/`module`/
            // any definition form, not just an expression (recursion covers
            // macro→macro chains). Expression-position macro calls are
            // unaffected: those expand inside `check_list` as before.
            if let Some(expanded) = self.try_expand_toplevel_macro(heap, interp, v)? {
                heap.push_root(expanded);
                let result = self.check_form_dispatch(heap, interp, expanded, def_loc);
                heap.pop_root();
                return result;
            }
        }
        let env = Env::new();
        // `def_loc` doubles as the expression's own location hint: for a list
        // form `check_at` prefers `cons_loc` (the same location) anyway, and
        // for a bare top-level atom it is the only location there is.
        let t = self.check_at(heap, interp, &env, v, None, def_loc)?;
        Ok(TopLevel::Expr(t))
    }

    /// The type/constructor registry — exposed so a caller (e.g. the REPL)
    /// can resolve an ADT variant's constructor name when printing a
    /// `RtValue::Data` result.
    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    /// Mutable registry access for the fasl loader (`crate::fasl`) — the one
    /// caller that legitimately writes registry entries without going
    /// through `check_form` (it replays entries a previous check already
    /// produced and serialized).
    pub(crate) fn registry_mut(&mut self) -> &mut Registry {
        &mut self.reg
    }

    /// Exports the retained generic templates in the heap-independent
    /// [`crate::fasl`] representation — the serialize-side half of the fasl
    /// round trip (templates are the only checker state whose data lives on
    /// the GC heap; see [`FnTemplate`]/[`MethodTemplate`]).
    pub fn export_templates(&self, heap: &Heap) -> Result<ExportedTemplates, Error> {
        let mut fns = Vec::new();
        for (path, t) in &self.generic_fn_templates {
            let parts = t.parts.iter().map(|v| crate::fasl::value_to_owned(heap, *v)).collect::<Result<_, _>>()?;
            fns.push((
                path.clone(),
                crate::fasl::FnTemplateRepr { parts, ns: t.ns.clone(), type_params: t.type_params.clone() },
            ));
        }
        let mut methods = Vec::new();
        for ((owner, name), t) in &self.generic_method_templates {
            let repr = match t {
                MethodTemplate::Form { parts, ns, written_vars } => crate::fasl::MethodTemplateRepr::Form {
                    parts: parts.iter().map(|v| crate::fasl::value_to_owned(heap, *v)).collect::<Result<_, _>>()?,
                    ns: ns.clone(),
                    written_vars: written_vars.clone(),
                },
                MethodTemplate::Getter { index } => crate::fasl::MethodTemplateRepr::Getter { index: *index },
                MethodTemplate::Setter { index } => crate::fasl::MethodTemplateRepr::Setter { index: *index },
            };
            methods.push((owner.clone(), name.clone(), repr));
        }
        Ok((fns, methods))
    }

    /// Installs fasl-carried generic templates, rebuilding each raw form in
    /// `heap` through its allocation APIs ([`crate::fasl::owned_to_value`])
    /// and permanently rooting it — exactly the protection a source-checked
    /// template's parts get (see the `push_permanent_root` calls in
    /// [`Self::check_defun`]/[`Self::check_defmethod`]).
    pub fn install_templates(
        &mut self,
        heap: &mut Heap,
        fns: &[(Path, crate::fasl::FnTemplateRepr)],
        methods: &[(Path, String, crate::fasl::MethodTemplateRepr)],
    ) -> Result<(), Error> {
        for (path, repr) in fns {
            let mut parts = Vec::with_capacity(repr.parts.len());
            for f in &repr.parts {
                let v = crate::fasl::owned_to_value(heap, f)?;
                heap.push_permanent_root(v);
                parts.push(v);
            }
            self.generic_fn_templates.insert(
                path.clone(),
                FnTemplate { parts, ns: repr.ns.clone(), type_params: repr.type_params.clone() },
            );
        }
        for (owner, name, repr) in methods {
            let t = match repr {
                crate::fasl::MethodTemplateRepr::Form { parts, ns, written_vars } => {
                    let mut vs = Vec::with_capacity(parts.len());
                    for f in parts {
                        let v = crate::fasl::owned_to_value(heap, f)?;
                        heap.push_permanent_root(v);
                        vs.push(v);
                    }
                    MethodTemplate::Form { parts: vs, ns: ns.clone(), written_vars: written_vars.clone() }
                }
                crate::fasl::MethodTemplateRepr::Getter { index } => MethodTemplate::Getter { index: *index },
                crate::fasl::MethodTemplateRepr::Setter { index } => MethodTemplate::Setter { index: *index },
            };
            self.generic_method_templates.insert((owner.clone(), name.clone()), t);
        }
        Ok(())
    }

    /// `(pub defun ...)` / `(pub defmethod ...)` etc. — mark the next definition public.
    fn check_pub(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value], parts_locs: &[Option<Loc>], def_loc: Option<Loc>) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("pub: expected a definition form".into()));
        }
        let inner_locs: &[Option<Loc>] = if parts_locs.len() > 1 { &parts_locs[1..] } else { &[] };
        if let Value::Symbol(id) = parts[0] {
            match heap.symbol_name(id) {
                "defun" => return self.check_defun(heap, interp, &parts[1..], inner_locs, true, def_loc),
                "defvar" => return self.check_defvar(heap, interp, &parts[1..], true, true, def_loc),
                "defconstant" => return self.check_defvar(heap, interp, &parts[1..], false, true, def_loc),
                "defmacro" => return self.check_defmacro(heap, interp, &parts[1..], inner_locs, true, def_loc),
                "defmethod" => return self.check_defmethod(heap, interp, &parts[1..], inner_locs, true, def_loc),
                "defstruct" => return self.check_defstruct(heap, &parts[1..], true, def_loc),
                "defenum" => return self.check_defenum(heap, &parts[1..], true, def_loc),
                _ => {}
            }
        }
        Err(Error::TypeError("pub: expected defun/defmacro/defmethod/defstruct/defenum/defvar/defconstant".into()))
    }

    // ---- namespace navigation & resolution --------------------------------
    //
    // Identities are structured `Path`s, never joined `"a::b"` strings: the only
    // `::`-string decoding lives in the reader/parser. Resolution navigates the
    // namespace tree, current namespace first then root.

    /// Qualify a bare name with the current namespace path.
    fn fq(&self, name: &str) -> Path {
        let mut segments = self.ns.clone();
        segments.push(name.to_string());
        Path::from_segments(segments)
    }

    /// The current namespace.
    fn cur_ns(&self) -> &Namespace {
        self.reg.root.module(&self.ns).expect("current namespace exists")
    }

    /// If `path` starts with an empty segment (reader-encoded `::foo` absolute
    /// path), strip it and return `(true, rest)`.  Otherwise `(false, path)`.
    fn split_abs<'a>(&self, path: &'a [String]) -> (bool, &'a [String]) {
        if path.first().map(|s| s.is_empty()).unwrap_or(false) {
            (true, &path[1..])
        } else {
            (false, path)
        }
    }

    /// The current namespace's ancestor chain, nearest first, ending at root:
    /// `self.ns`, `self.ns[..len-1]`, ..., `[]`. The module-tree analogue of
    /// walking parent lexical-scope frames — every bare-name resolver walks
    /// this instead of only checking "current module or literal root", so a
    /// submodule sees not just its own definitions but every enclosing
    /// module's too (including root's, however deeply nested the caller is).
    fn ns_ancestors(&self) -> impl Iterator<Item = &[String]> {
        (0..=self.ns.len()).rev().map(move |k| &self.ns[..k])
    }

    /// Whether `item_ns` (a definition's owning module) is visible from the
    /// current namespace without needing `pub` — Rust-style module privacy:
    /// the current module IS `item_ns`, or is nested inside it (a
    /// descendant). Replaces the old exact-equality `same_module` check;
    /// root (`item_ns == []`) is always an ancestor, so this also subsumes
    /// the former "literal root caller" special case.
    fn in_scope(&self, item_ns: &[String]) -> bool {
        self.ns.len() >= item_ns.len() && self.ns[..item_ns.len()] == *item_ns
    }

    /// Build a [`Ref`] for a `Call`/`Global`/`FnRef`/`SetGlobal` node: pairs
    /// the name exactly as written at the reference site with the current
    /// namespace (the walk's starting point for `Interp`'s own independent
    /// re-resolution, see `eval::scope`'s module doc comment) and the
    /// already-resolved target (for compile-time-only consumers). Ordinary
    /// call sites use this; a synthesized reference with no real "as
    /// written" form (a mangled specialization, a compiler-internal
    /// rewrite) uses [`Ref::synthetic`] instead.
    fn mk_ref(&self, written: Vec<String>, resolved: Path) -> Ref {
        Ref { written, home: self.ns.clone(), resolved }
    }

    /// Whether an associated function/method `af` of `type_fq` is reachable
    /// from the current namespace: public, or in scope (declared in the
    /// same module or an ancestor of it) — mirrors `resolve_fn_path`'s
    /// cross-module `public` check. Used at every call site that looks up
    /// `def.assoc` directly (`try_field_access`, `check_field_set`,
    /// `check_path_call`'s static-member branch, `try_instance_method`,
    /// `check_instance_method`) so a private member — including a
    /// `defstruct` field whose accessor wasn't declared `pub` — is treated
    /// as absent rather than merely forbidden, matching how a private free
    /// function or constructor "doesn't resolve" instead of erroring with a
    /// privacy-specific message.
    fn assoc_visible(&self, type_fq: &Path, af: &AssocFn) -> bool {
        af.sig.public || self.in_scope(type_fq.parent())
    }

    /// Locate a child-module `path`, resolving it relative to the current
    /// namespace first, then the root, then via `mod_aliases`. Handles absolute
    /// paths (leading empty segment) by going straight to root.
    /// Returns `(absolute_segments, &Namespace)`.
    fn find_module(&self, path: &[String]) -> Option<(Vec<String>, &Namespace)> {
        let (is_abs, eff) = self.split_abs(path);
        if is_abs {
            return self.reg.root.module(eff).map(|m| (eff.to_vec(), m));
        }

        // 1. Child of the current namespace.
        if let Some(m) = self.cur_ns().module(eff) {
            let mut abs = self.ns.clone();
            abs.extend_from_slice(eff);
            return Some((abs, m));
        }
        // 2. Child of root.
        if let Some(m) = self.reg.root.module(eff) {
            return Some((eff.to_vec(), m));
        }
        // 3. Module alias: expand the first segment via mod_aliases.
        if !eff.is_empty() {
            let first = &eff[0];
            let rest = &eff[1..];
            let alias = self
                .cur_ns()
                .mod_aliases
                .get(first)
                .or_else(|| self.reg.root.mod_aliases.get(first));
            if let Some(base) = alias {
                let mut expanded = base.clone();
                expanded.extend_from_slice(rest);
                if let Some(m) = self.reg.root.module(&expanded) {
                    return Some((expanded, m));
                }
            }
        }
        // 4. Sibling of the current *file* (not `self.ns`, which may be
        // deeper than the file's own path inside a nested `(module ...)`
        // block — see `file_ns`'s doc comment): a file in the same directory
        // as the one currently being checked, reached without spelling out
        // its full root-relative path. Tried last so it never shadows an
        // existing root-relative/current-namespace/alias resolution — see
        // `docs/dev/language-design.md` §2.4.
        if let Some(cur_file) = self.file_ns.last() {
            if !cur_file.is_empty() {
                let mut sib = cur_file[..cur_file.len() - 1].to_vec();
                sib.extend_from_slice(eff);
                if let Some(m) = self.reg.root.module(&sib) {
                    return Some((sib, m));
                }
            }
        }
        None
    }

    /// A `use` alias for a bare `name`, looked up current namespace then root.
    fn lookup_alias(&self, name: &str) -> Option<Vec<String>> {
        if let Some(p) = self.cur_ns().aliases.get(name) {
            return Some(p.clone());
        }
        self.reg.root.aliases.get(name).cloned()
    }

    /// Resolve a bare free-function name to its absolute [`Path`], walking
    /// the current namespace's ancestor chain (see [`Self::ns_ancestors`]) —
    /// a name found anywhere on that chain is in scope without needing
    /// `pub`, exactly like a same-module reference.
    fn resolve_fn(&self, name: &str) -> Option<Path> {
        if let Some(path) = self.lookup_alias(name) {
            return self.resolve_fn_path(&path);
        }
        for prefix in self.ns_ancestors() {
            if let Some(m) = self.reg.root.module(prefix) {
                if m.fns.contains_key(name) {
                    let mut segs = prefix.to_vec();
                    segs.push(name.to_string());
                    return Some(Path::from_segments(segs));
                }
            }
        }
        None
    }

    /// Resolve a qualified `module::...::fn` path to its absolute [`Path`].
    fn resolve_fn_path(&self, segs: &[String]) -> Option<Path> {
        if segs.is_empty() {
            return None;
        }
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if let Some(sig) = m.fns.get(&last[0]) {
            if !sig.public && !self.in_scope(&abs) {
                return None;
            }
            let mut full = abs;
            full.push(last[0].clone());
            return Some(Path::from_segments(full));
        }
        None
    }

    /// Resolve a bare macro name to its absolute [`Path`] and the call-site
    /// arity shape ([`MacroShape`]) — walks the ancestor chain like
    /// [`Self::resolve_fn`] (without `use`-alias support, which `defmacro`
    /// doesn't have yet).
    fn resolve_macro(&self, name: &str) -> Option<(Path, MacroShape)> {
        for prefix in self.ns_ancestors() {
            if let Some(m) = self.reg.root.module(prefix) {
                if let Some(def) = m.macros.get(name) {
                    let mut segs = prefix.to_vec();
                    segs.push(name.to_string());
                    return Some((Path::from_segments(segs), MacroShape::of(def)));
                }
            }
        }
        None
    }

    /// Resolve a module-qualified macro name (`mod::macro-name`) to its
    /// absolute [`Path`] and call-site arity shape ([`MacroShape`]) — the
    /// path-qualified counterpart of [`Self::resolve_macro`], mirroring
    /// [`Self::resolve_fn_path`]'s module lookup and visibility rule (public
    /// unless the caller is in scope of the defining module).
    fn resolve_macro_path(&self, segs: &[String]) -> Option<(Path, MacroShape)> {
        if segs.is_empty() {
            return None;
        }
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        let def = m.macros.get(&last[0])?;
        if !def.public && !self.in_scope(&abs) {
            return None;
        }
        let mut full = abs;
        full.push(last[0].clone());
        Some((Path::from_segments(full), MacroShape::of(def)))
    }

    /// Heads with built-in meaning — the expression special forms matched in
    /// [`Self::check_list`] and the top-level forms matched in
    /// [`Self::check_form_dispatch`]. [`Self::try_expand_toplevel_macro`]
    /// never treats these as macro calls, so a macro sharing such a name can
    /// never hijack the built-in form (both existing `match`es already win
    /// over `resolve_macro` by being tried first; this keeps the loader's
    /// pre-expansion consistent with that order).
    fn is_builtin_form_head(name: &str) -> bool {
        matches!(
            name,
            // expression special forms (`check_list`)
            "if" | "let" | "let*" | "progn" | "setf" | "loop" | "break" | "return" | "list"
                | "lambda" | "labels" | "match" | "panic" | "the" | "as" | "try-as" | "compile"
                | "quote" | "quasiquote" | "format" | "print" | "println"
                // top-level forms (`check_form_dispatch`)
                | "pub" | "defun" | "defvar" | "defconstant" | "defmacro" | "module"
                | "defmethod" | "defstruct" | "defenum" | "deftrait" | "impl" | "use" | "load"
        )
    }

    /// If `v` is a compound form whose head resolves to a macro — a bare
    /// name ([`Self::resolve_macro`], current namespace) or a `mod::name`
    /// path ([`Self::resolve_macro_path`], cross-module) — expand it one
    /// step against the unevaluated argument forms and return the expansion;
    /// `Ok(None)` when `v` is not a macro call. A bare built-in form head is
    /// never treated as a macro call — see [`Self::is_builtin_form_head`]
    /// (no path is ever a built-in head, so no equivalent guard is needed on
    /// that branch). Public so the loader (`crate::project`) can pre-expand
    /// a top-level form and scan the expansion for `(use ...)` dependencies
    /// before checking it. The caller must root the returned expansion
    /// (`Heap::push_root`) before anything that may allocate.
    pub fn try_expand_toplevel_macro(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        v: Value,
    ) -> Result<Option<Value>, Error> {
        if !matches!(v, Value::Cons(_)) {
            return Ok(None);
        }
        let elems = heap.list_to_vec(v)?;
        let (macro_path, shape, display) = match elems.first() {
            Some(Value::Symbol(id)) => {
                let head = heap.symbol_name(*id).to_string();
                if Self::is_builtin_form_head(&head) {
                    return Ok(None);
                }
                match self.resolve_macro(&head) {
                    Some((p, shape)) => (p, shape, head),
                    None => return Ok(None),
                }
            }
            Some(Value::Path(pid)) => {
                let segs: Vec<String> =
                    heap.path_segments(*pid).iter().map(|s| heap.symbol_name(*s).to_string()).collect();
                match self.resolve_macro_path(&segs) {
                    Some((p, shape)) => (p, shape, segs.join("::")),
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        let args = &elems[1..];
        Self::check_macro_arity(&display, shape, args.len())?;
        let expanded = interp
            .expand_macro(heap, &macro_path, args.to_vec())
            .map_err(|e| Error::TypeError(format!("macro `{}`: {}", display, e)))?;
        Ok(Some(expanded))
    }

    /// The arity check shared by every macro-expansion site. Only the bounds a
    /// call site can judge from raw argument *count* are checked here; the
    /// finer validation that needs to parse the argument forms (an unknown
    /// `&key` keyword, an odd `&key` plist) is left to `Interp::expand_macro`,
    /// whose error is surfaced the same way (`macro `name`: ...`).
    ///
    /// - A minimum of `required` args always applies.
    /// - With neither `&rest` nor `&key`, the maximum is `required + optional`
    ///   (exact when there are no `&optional` params — the plain-macro case,
    ///   preserving the original exact-arity message).
    /// - With `&rest` or `&key`, there is no call-site upper bound (the
    ///   trailing args are a `&rest` list and/or a keyword plist).
    fn check_macro_arity(head: &str, shape: MacroShape, got: usize) -> Result<(), Error> {
        let MacroShape { required, optional, rest, has_keys } = shape;
        if got < required {
            return Err(Error::TypeError(format!(
                "macro `{}` expects at least {} argument(s), got {}",
                head, required, got
            )));
        }
        if !rest && !has_keys {
            let max = required + optional;
            if got > max {
                return Err(Error::TypeError(if optional == 0 {
                    format!("macro `{}` expects {} argument(s), got {}", head, required, got)
                } else {
                    format!("macro `{}` expects {} to {} argument(s), got {}", head, required, max, got)
                }));
            }
        }
        Ok(())
    }

    /// Resolve a bare constructor name to its `(type path, variant index)`,
    /// walking the ancestor chain (see [`Self::ns_ancestors`]) — every ctor
    /// found this way is in scope by construction, so no separate `public`
    /// check is needed (mirrors [`Self::resolve_fn`]).
    fn resolve_ctor(&self, name: &str) -> Option<(Path, usize)> {
        for prefix in self.ns_ancestors() {
            if let Some(m) = self.reg.root.module(prefix) {
                if let Some(hit) = m.ctors.get(name) {
                    return Some(hit.clone());
                }
            }
        }
        None
    }

    /// The `Sexpr` `cons` variant's `(type path, variant index)`, read from the
    /// type definition directly rather than the bare-name `ctors` map — that
    /// bare `cons` name was removed (Symbol/Sexpr redesign Phase 4b) so `(cons
    /// a b)` resolves to the free `cons<T,U>` pair function. Internal `Sexpr`
    /// cons construction (`list` literals, quasiquote) still needs the variant,
    /// and reaches it through here.
    fn sexpr_cons_ctor(&self) -> (Path, usize) {
        let path = Path::root("sexpr");
        let def = self.reg.type_def(&path).expect("sexpr is built in");
        let idx = def
            .variants
            .iter()
            .position(|v| v.name == "cons")
            .expect("sexpr has a cons variant");
        (path, idx)
    }

    /// Resolve a bare name snapshotted by `(use Type)` (see `Self::check_use`)
    /// to the `(type path, method name)` of the static associated function it
    /// names — the static-method counterpart of [`Self::resolve_ctor`],
    /// checked the same way (current namespace then root).
    fn resolve_static_use(&self, name: &str) -> Option<(Path, String)> {
        if let Some(hit) = self.cur_ns().static_uses.get(name) {
            return Some(hit.clone());
        }
        self.reg.root.static_uses.get(name).cloned()
    }

    /// Wraps an already-checked `&rest` element `e` (statically `elem_ty`,
    /// already resolved to a concrete type — see [`Self::cons_rest_list`])
    /// as a `Sexpr` value, so a sequence of them can be `cons`-ed into a
    /// plain list — see [`sexpr_ty`]'s doc comment. `elem_ty` itself needs no
    /// wrapping (already `Sexpr`); every other supported element type goes
    /// through its own `Sexpr` constructor (`sexpr_ctor_for`) exactly as if
    /// the caller had written e.g. `(Int e)` by hand — `construct_sexpr`
    /// (`crate::eval::interp`) only ever reads the field's *runtime* value
    /// (an `i32`/`i64` argument both evaluate to the same `RtValue::Int`),
    /// so this never goes through the normal field-type validation
    /// `check_construct` would otherwise apply, but is sound for the same
    /// reason. An `elem_ty` with no `Sexpr` encoding (e.g. `Option<T>`) is a
    /// `TypeError` here, not a panic — unlike most of `&rest`'s checking,
    /// this can't happen any earlier than this, since a generic `&rest`'s
    /// declared type may only resolve to something concrete at a given call
    /// site (see `Self::check_call`'s `subst_apply`).
    fn wrap_rest_elem(&self, elem_ty: &Type, e: Typed) -> Result<Typed, Error> {
        if *elem_ty == sexpr_ty() {
            return Ok(e);
        }
        // A heap-repr ADT (`defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>`)
        // needs no `sexpr_ctor_for` wrap — same transparent retype as
        // `check_inner`'s expected-type fallback, see its doc comment for why
        // this is runtime-cost-free. This is what lets `(println "~a" my-
        // struct)`/`(list p ...&rest)` accept a user ADT argument.
        if self.is_heap_repr(elem_ty) {
            return Ok(Typed { loc: e.loc, expr: e.expr, ty: sexpr_ty() });
        }
        let ctor = sexpr_ctor_for(elem_ty).ok_or_else(|| {
            Error::TypeError(format!(
                "&rest: element type {:?} has no Sexpr encoding (use one of i32/i64/f64/bool/char/string/Sexpr)",
                elem_ty
            ))
        })?;
        let (type_name, variant) = self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
        Ok(Typed { loc: None, expr: Expr::Construct { type_name, variant, args: vec![e], mutable: false }, ty: sexpr_ty() })
    }

    /// Collects already-checked `&rest` elements into one `Sexpr` list
    /// expression, back-to-front (mirroring `Interp::bind_macro_args`'s
    /// runtime construction of `defmacro`'s own `&rest` list) — `(cons
    /// (wrap e1) (cons (wrap e2) ... ()))`. The empty case is the `Nil`
    /// literal, reusing the same `Expr::Quote(QuotedSexpr::Nil)` shape `()`
    /// itself checks to.
    fn cons_rest_list(&self, elem_ty: &Type, items: Vec<Typed>) -> Result<Typed, Error> {
        // `sexpr-cons`, not the free `cons` (which is the `cons<T,U>` pair
        // builder now — Symbol/Sexpr redesign Phase 4b): a `&rest` list is a
        // `Sexpr`, built through the island cons layer.
        let cons_path = Path::root("sexpr-cons");
        items.into_iter().rev().try_fold(
            Typed { loc: None, expr: Expr::Quote(QuotedSexpr::Nil), ty: sexpr_ty() },
            |acc, item| {
                let item = self.wrap_rest_elem(elem_ty, item)?;
                Ok(Typed { loc: None, expr: Expr::Call(Ref::synthetic(cons_path.clone()), vec![item, acc]), ty: sexpr_ty() })
            },
        )
    }

    /// Resolve a bare (unqualified) type name by walking the ancestor chain
    /// (see [`Self::ns_ancestors`]) — shared by [`Self::resolve_type_name`]'s
    /// simple-path case and [`Self::resolve_type_path`]'s no-module-qualifier
    /// case (`Type::member` written with a bare `Type`, e.g. `Option::some`,
    /// which reaches here via an empty `mods` prefix and needs exactly the
    /// same lookup a type annotation would use).
    fn resolve_bare_type(&self, name: &str) -> Option<Path> {
        for prefix in self.ns_ancestors() {
            if let Some(m) = self.reg.root.module(prefix) {
                if let Some(def) = m.types.get(name) {
                    return Some(def.name.clone());
                }
            }
        }
        None
    }

    /// Resolve a `module::...::Type` segment path to the type's [`Path`].
    fn resolve_type_path(&self, segs: &[String]) -> Option<Path> {
        if segs.is_empty() {
            return None;
        }
        let (mods, local) = segs.split_at(segs.len() - 1);
        if mods.is_empty() {
            // No module qualifier — `find_module(&[])` would just hand back
            // the current namespace itself (its loop never runs), missing
            // every ancestor including root; walk the chain instead.
            return self.resolve_bare_type(&local[0]);
        }
        let (abs, m) = self.find_module(mods)?;
        if let Some(def) = m.types.get(&local[0]) {
            if !def.public && !self.in_scope(&abs) {
                return None;
            }
            return Some(def.name.clone());
        }
        None
    }

    /// Resolve a nominal type [`Path`] to its canonical (located) form if a
    /// matching type exists; otherwise (type variables, unknown names) return it
    /// unchanged.
    fn resolve_type_name(&self, path: &Path) -> Path {
        if path.is_simple() {
            let name = path.local();
            if let Some(target) = self.lookup_alias(name) {
                if let Some(tp) = self.resolve_type_path(&target) {
                    return tp;
                }
            }
            if let Some(tp) = self.resolve_bare_type(name) {
                return tp;
            }
            return path.clone();
        }
        self.resolve_type_path(path.segments()).unwrap_or_else(|| path.clone())
    }

    /// Resolve a bare global variable/constant name to its `(path, info)`,
    /// walking the ancestor chain like [`Self::resolve_fn`].
    fn resolve_global(&self, name: &str) -> Option<(Path, VarInfo)> {
        if let Some(p) = self.lookup_alias(name) {
            return self.resolve_global_path(&p);
        }
        for prefix in self.ns_ancestors() {
            if let Some(m) = self.reg.root.module(prefix) {
                if let Some(vi) = m.vars.get(name) {
                    let mut segs = prefix.to_vec();
                    segs.push(name.to_string());
                    return Some((Path::from_segments(segs), vi.clone()));
                }
            }
        }
        None
    }

    /// Resolve a qualified `module::...::global` path to its `(path, info)`.
    fn resolve_global_path(&self, segs: &[String]) -> Option<(Path, VarInfo)> {
        if segs.is_empty() {
            return None;
        }
        let (mods, last) = segs.split_at(segs.len() - 1);
        let (abs, m) = self.find_module(mods)?;
        if let Some(vi) = m.vars.get(&last[0]) {
            if !vi.public && !self.in_scope(&abs) {
                return None;
            }
            let mut full = abs;
            full.push(last[0].clone());
            return Some((Path::from_segments(full), vi.clone()));
        }
        None
    }

    /// Reify a bare free-function name as a function value (`FnRef`), if it names
    /// one. `Some(Err(..))` when the name resolves but is a generic function
    /// that can't be instantiated here — see [`Self::fn_ref_node`].
    fn fn_value(&self, name: &str, expected: Option<&Type>) -> Option<Result<Typed, Error>> {
        let fq = self.resolve_fn(name)?;
        Some(self.fn_ref_node(vec![name.to_string()], fq, expected))
    }

    /// Reify a qualified free-function path as a function value (`FnRef`).
    fn fn_path_value(&self, segs: &[String], expected: Option<&Type>) -> Option<Result<Typed, Error>> {
        let fq = self.resolve_fn_path(segs)?;
        Some(self.fn_ref_node(segs.to_vec(), fq, expected))
    }

    /// Build an `FnRef` node carrying the function's `(fn ...)` type.
    ///
    /// A *generic* function has no single value to reference — post-
    /// monomorphization, only concrete specializations exist at runtime — so
    /// its type parameters are resolved against the `expected` function type
    /// (the only call-site information a bare name carries, e.g. a typed
    /// `let` binding or the parameter an argument is checked against) and
    /// the reference is rewritten to the specialization, requested exactly
    /// like a direct call's. With no expectation to resolve from this is a
    /// check-time error (previously it produced a type-variable-ridden
    /// `FnRef` that could never be applied anyway).
    fn fn_ref_node(&self, written: Vec<String>, fq: Path, expected: Option<&Type>) -> Result<Typed, Error> {
        let sig = self.reg.fn_sig(&fq).expect("resolved fn exists").clone();
        let tmpl_ty = Type::Fn(sig.params.clone(), sig.rest.clone().map(Box::new), Box::new(sig.ret.clone()));
        let home = self.ns.clone();
        if sig.type_params.is_empty() {
            let r = Ref { written, home, resolved: fq };
            return Ok(Typed { loc: None, expr: Expr::FnRef(r), ty: tmpl_ty });
        }
        if let Some(exp) = expected {
            let params: HashSet<String> = sig.type_params.iter().cloned().collect();
            let mut subst: HashMap<String, Type> = HashMap::new();
            if unify(&params, &tmpl_ty, exp, &mut subst).is_ok()
                && sig.type_params.iter().all(|p| subst.contains_key(p))
            {
                let targs: Vec<Type> = sig.type_params.iter().map(|p| subst[p.as_str()].clone()).collect();
                let resolved_ty = subst_apply(&tmpl_ty, &subst);
                if !targs.iter().any(|t| self.type_is_open(t)) && self.generic_fn_templates.contains_key(&fq) {
                    let mangled = self.request_fn_specialization(&fq, targs);
                    // A monomorphized specialization has no "as written"
                    // source form of its own — `Ref::synthetic` resolves it
                    // by its own mangled identity directly, exactly where
                    // `Interp::exec` registers it, rather than re-searching
                    // for the (unspecialized) generic template by the
                    // original bare name.
                    return Ok(Typed { loc: None, expr: Expr::FnRef(Ref::synthetic(mangled)), ty: resolved_ty });
                }
                // Open type arguments: we're inside another generic
                // function's diagnostics-only body check — the node is never
                // executed, and that function's own specialization will
                // re-check this reference with the types concrete.
                let r = Ref { written, home, resolved: fq };
                return Ok(Typed { loc: None, expr: Expr::FnRef(r), ty: resolved_ty });
            }
            // Shape mismatch: hand back the generic node so `check`'s
            // ordinary expected-vs-actual reconciliation reports it.
            let r = Ref { written, home, resolved: fq };
            return Ok(Typed { loc: None, expr: Expr::FnRef(r), ty: tmpl_ty });
        }
        Err(Error::TypeError(format!(
            "generic function `{}` used as a value needs a concrete function-type context \
             (e.g. a typed let binding, parameter position, or `the`)",
            fq
        )))
    }

    /// Reify a bare name as an *instance method* value (`MethodRef`), e.g.
    /// `+` where a `(fn (i32 i32) i32)` is expected (see
    /// [`builtin_as_value`](../../../tests/eval_test.rs)-style usage of
    /// `registry::int_assoc`'s arithmetic). Unlike a real call site, there is
    /// no receiver expression to dispatch on, so the receiver type is read
    /// off `expected`'s first parameter instead.
    ///
    /// A generic-*owner* method with a retained [`MethodTemplate`] resolves
    /// the owner's type arguments from `expected` (its first parameter is
    /// the receiver's concrete type) and is rewritten to the specialization,
    /// mirroring [`Self::fn_ref_node`]. A generic method *without* a
    /// template (a Rust builtin like `HashTable::get`) still can't be
    /// reified and fails to match, as before.
    fn method_value(&self, name: &str, expected: Option<&Type>) -> Option<Typed> {
        let Type::Fn(params, _, _) = expected? else { return None };
        let type_fq = match params.first()? {
            Type::Named(n, _) => n.clone(),
            other => prim_type_path(other)?,
        };
        let def = self.reg.type_def(&type_fq)?;
        let af = def.assoc.get(name)?;
        if !af.instance {
            return None;
        }
        let ty = Type::Fn(af.sig.params.clone(), af.sig.rest.clone().map(Box::new), Box::new(af.sig.ret.clone()));
        if !def.params.is_empty() {
            let tparams: HashSet<String> = def.params.iter().cloned().collect();
            let mut subst: HashMap<String, Type> = HashMap::new();
            if unify(&tparams, &ty, expected.unwrap(), &mut subst).is_ok()
                && def.params.iter().all(|p| subst.contains_key(p))
            {
                let targs: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
                if !targs.iter().any(|t| self.type_is_open(t))
                    && self.generic_method_templates.contains_key(&(type_fq.clone(), name.to_string()))
                {
                    let mangled = self.request_method_specialization(&type_fq, name, targs);
                    return Some(Typed { loc: None,
                        expr: Expr::MethodRef { type_name: type_fq, method: mangled, home: self.ns.clone() },
                        ty: subst_apply(&ty, &subst),
                    });
                }
            }
            return None;
        }
        if &ty != expected.unwrap() {
            return None;
        }
        Some(Typed { loc: None, expr: Expr::MethodRef { type_name: type_fq, method: name.to_string(), home: self.ns.clone() }, ty })
    }

    /// `var::field`: if `segs` is `[recv, method]` and `recv` names a bound
    /// local or global whose type has an *instance* associated function
    /// called `method` (in practice always a `defstruct` field accessor —
    /// see `Checker::check_defstruct` — though this doesn't care how the
    /// method came to exist), produces the same `Expr::Assoc` node a
    /// `(method recv)` call would. `None` (not an error) for any other
    /// shape, so the caller falls back to ordinary module/type-path
    /// resolution (`Checker::check`'s `Value::Path` case).
    /// Delegates to `Checker::check_assoc_call` (instead of building the
    /// `Expr::Assoc` node directly) so a *generic* `defstruct`'s field type
    /// gets the receiver's concrete type arguments substituted in exactly
    /// the way any other instance-method call already does — building the
    /// node by hand here once produced an unsubstituted type variable as
    /// the field's type (caught by `generic_defstruct_setf_works` in
    /// `tests/struct_test.rs`, since a bare type variable can't satisfy a
    /// real expected type downstream, even though the type-erased runtime
    /// value was already correct).
    fn try_field_access(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
    ) -> Option<Result<Typed, Error>> {
        let [recv_name, method] = segs else { return None };
        let recv = if let Some(t) = env.get(recv_name) {
            Typed { loc: None, expr: Expr::Var(recv_name.clone()), ty: t.clone() }
        } else {
            let (path, vi) = self.resolve_global(recv_name)?;
            Typed { loc: None, expr: Expr::Global(self.mk_ref(vec![recv_name.clone()], path)), ty: vi.ty }
        };
        let Type::Named(type_fq, _) = &recv.ty else { return None };
        let af = self.reg.type_def(type_fq)?.assoc.get(method)?;
        if !af.instance || !self.assoc_visible(type_fq, af) {
            return None;
        }
        let type_fq = type_fq.clone();
        Some(self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method, receiver: Some(recv), expected: None },
            &[],
            &[],
        ))
    }

    /// Canonicalize a parsed type: resolve every nominal name to its located
    /// [`Path`] so that types compare equal across module boundaries.
    ///
    /// This is also monomorphization's substitution point: while a generic
    /// template is being re-checked ([`Self::specialize_defun`]), a
    /// single-segment name bound in `type_var_bindings` resolves to its
    /// concrete type instead — checked *before* `resolve_type_name` so a
    /// user-defined type that happens to share a type parameter's name can
    /// never shadow the binding.
    fn canon(&self, t: &Type) -> Type {
        match t {
            Type::Named(n, args) => {
                if args.is_empty() && n.is_simple() {
                    if let Some(bound) = self.type_var_bindings.get(n.local()) {
                        return bound.clone();
                    }
                }
                Type::Named(
                    self.resolve_type_name(n),
                    args.iter().map(|a| self.canon(a)).collect(),
                )
            }
            // A trait object's head names a *trait*, which lives in
            // `Namespace::traits` rather than `types` — so it resolves
            // through `resolve_trait_name`, not `resolve_type_name`. An
            // unknown name is left as written for `check_object_safe` to
            // report; `canon` itself never fails.
            Type::Dyn(trait_path, pins) => Type::Dyn(
                self.resolve_trait_path(trait_path).unwrap_or_else(|| trait_path.clone()),
                pins.iter().map(|p| self.canon(p)).collect(),
            ),
            Type::Fn(ps, rest, r) => Type::Fn(
                ps.iter().map(|p| self.canon(p)).collect(),
                rest.as_ref().map(|t| Box::new(self.canon(t))),
                Box::new(self.canon(r)),
            ),
            other => other.clone(),
        }
    }

    /// The lowercase segments of a module/use path (a symbol or a `Value::Path`).
    fn path_to_segs(&self, heap: &Heap, v: Value) -> Result<Vec<String>, Error> {
        match v {
            Value::Symbol(id) => Ok(vec![heap.symbol_name(id).to_string()]),
            Value::Path(id) => Ok(heap
                .path_segments(id)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect()),
            _ => Err(Error::TypeError("expected a name or `::` path".into())),
        }
    }

    // ---- defun ------------------------------------------------------------

    fn check_defun(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevel, Error> {
        let (params, param_locs, rest, ret, bounds, body_start) = self.parse_defun_sig(heap, parts)?;
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let fq_name = self.fq(&name);

        // Register the signature in the current namespace before checking the
        // body so self-recursion works.
        self.check_redef("function", &name, self.cur_ns().fns.get(&name))?;

        // A generic defun additionally retains its raw source form for
        // per-instantiation re-checking — see `FnTemplate`. Retained *before*
        // the body check below: the body may itself contain a call that
        // instantiates this very function at concrete types (polymorphic
        // recursion — rejected later by `SPECIALIZATION_BUDGET`, but the
        // request still consults the template map first).
        if !type_params.is_empty() {
            for &p in parts {
                heap.push_permanent_root(p);
            }
            self.generic_fn_templates.insert(
                fq_name.clone(),
                FnTemplate { parts: parts.to_vec(), ns: self.ns.clone(), type_params: type_params.clone() },
            );
        }

        let sig = FnSig {
            type_params: type_params.clone(),
            params: params.iter().map(|(_, t)| t.clone()).collect(),
            ret: ret.clone(),
            public,
            rest: rest.as_ref().map(|(_, t, _)| t.clone()),
            builtin: false,
            bounds: bounds.clone(),
        };
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);
        if let Some(loc) = def_loc {
            self.reg.def_locs.fns.insert(fq_name.clone(), loc);
        }

        // The body additionally sees the `&rest` parameter (if any) bound to
        // a plain `Sexpr` list collecting every variadic argument (see
        // `sexpr_ty`'s doc comment) — the call-site desugaring
        // (`Self::check_call`) always supplies exactly one such list as the
        // last actual argument, so the stored `params` below (consumed only
        // for its *names* by `Interp::exec`) already lines up 1:1 with
        // arguments at runtime; mirrors `TopLevel::Defmacro::params`
        // including its `&rest` name last.
        let mut params = params;
        let mut param_locs = param_locs;
        if let Some((rname, _, rloc)) = &rest {
            params.push((rname.clone(), sexpr_ty()));
            param_locs.push(rloc.clone());
        }
        let binds: Vec<(String, Type, Option<Loc>)> = params
            .iter()
            .cloned()
            .zip(param_locs)
            .map(|((n, t), l)| (n, t, l))
            .collect();
        let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
        let body_locs = parts_locs.get(body_start..).unwrap_or(&[]);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], body_locs, Some(&ret))?;
        Ok(TopLevel::Defun { name: fq_name, type_params, params, ret, body, public })
    }

    /// Parses a `defun` form's parameter list, return type, and optional
    /// `(where ...)` clause (`parts` = everything after the `defun` keyword),
    /// returning them plus the body's starting index in `parts`. Shared by
    /// [`Self::check_defun`] and [`Self::specialize_defun`] — the latter
    /// re-parses a retained generic template with `type_var_bindings` in
    /// effect, so the very same source annotations come back concrete.
    ///
    /// The `where` clause is parsed *before* the `FnSig` is registered (only
    /// `parts`/`heap` are needed, nothing computed later) so the bounds can
    /// be stored on the signature itself — `Checker::check_call` consults
    /// them for call-site validation, not just body-checking's `Env`.
    #[allow(clippy::type_complexity)]
    fn parse_defun_sig(
        &self,
        heap: &Heap,
        parts: &[Value],
    ) -> Result<(Vec<(String, Type)>, Vec<Option<Loc>>, Option<RestParam>, Type, HashMap<String, Vec<TraitBound>>, usize), Error>
    {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let (params, param_locs, rest) = self.parse_params_rest(heap, parts[1])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);
        let mut body_start = 3;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        if let Some(form) = parts.get(3) {
            if self.is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        Ok((params, param_locs, rest, ret, bounds, body_start))
    }

    // ---- monomorphization ---------------------------------------------------

    /// Records that the current form needs `base` (a generic `defun` with a
    /// retained [`FnTemplate`]) instantiated at the concrete `args`, and
    /// returns the specialized function's mangled [`Path`] for the call site
    /// to reference instead of `base`. Idempotent within one top-level form
    /// (`spec_memo`); the actual definition is generated later by
    /// [`Self::drain_specializations`], because this runs inside expression
    /// checking (`&self`) which cannot re-enter definition checking.
    fn request_fn_specialization(&self, base: &Path, args: Vec<Type>) -> Path {
        let mangled = mangled_fn_path(base, &args);
        let mut memo = self.spec_memo.borrow_mut();
        if memo.insert((mangled.clone(), None)) {
            self.spec_pending.borrow_mut().push(SpecRequest::Fn {
                base: base.clone(),
                args,
                mangled: mangled.clone(),
            });
        }
        mangled
    }

    /// [`Self::request_fn_specialization`]'s method-side counterpart: records
    /// that `type_fq`'s method `base` needs generating at the owner's
    /// concrete type arguments `args`, returning the mangled method name for
    /// the `Expr::Assoc` node to reference. Only ever called when a
    /// [`MethodTemplate`] is retained for the pair.
    fn request_method_specialization(&self, type_fq: &Path, base: &str, args: Vec<Type>) -> String {
        let mangled = mangled_method_name(base, &args);
        let mut memo = self.spec_memo.borrow_mut();
        if memo.insert((type_fq.clone(), Some(mangled.clone()))) {
            self.spec_pending.borrow_mut().push(SpecRequest::Method {
                type_fq: type_fq.clone(),
                base: base.to_string(),
                args,
                mangled: mangled.clone(),
            });
        }
        mangled
    }

    /// Generates every specialization the current form requested (and any a
    /// specialization's own body requests in turn — the worklist converges
    /// for ordinary mutual recursion because `spec_memo` dedupes by mangled
    /// name, and is cut off by [`SPECIALIZATION_BUDGET`] for polymorphic
    /// recursion, which produces a genuinely new instantiation every step).
    fn drain_specializations(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
    ) -> Result<Vec<TopLevel>, Error> {
        let mut out = Vec::new();
        loop {
            let req = self.spec_pending.borrow_mut().pop();
            let Some(req) = req else { break };
            if out.len() >= SPECIALIZATION_BUDGET {
                let last = match &req {
                    SpecRequest::Fn { mangled, .. } => mangled.to_string(),
                    SpecRequest::Method { type_fq, mangled, .. } => format!("{}::{}", type_fq, mangled),
                };
                return Err(Error::TypeError(format!(
                    "monomorphization did not converge after {} instantiations (a polymorphically \
                     recursive generic function — one that calls itself at an ever-growing type — \
                     cannot be compiled; last requested: {})",
                    SPECIALIZATION_BUDGET, last
                )));
            }
            out.push(match req {
                SpecRequest::Fn { .. } => self.specialize_defun(heap, interp, &req)?,
                SpecRequest::Method { .. } => self.specialize_method(heap, interp, &req)?,
            });
        }
        Ok(out)
    }

    /// Re-checks `req.base`'s retained template with its type variables bound
    /// to `req.args`, producing a fully concrete `TopLevel::Defun` named
    /// `req.mangled`. The re-check runs under the template's *defining*
    /// namespace and a fresh loop stack (the same isolation `check_lambda`
    /// applies to a nested function body — a specialization requested from
    /// inside somebody's `loop` must not see that loop as its own).
    fn specialize_defun(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        req: &SpecRequest,
    ) -> Result<TopLevel, Error> {
        let SpecRequest::Fn { base, args, mangled } = req else {
            unreachable!("drain routes Fn requests here")
        };
        let tmpl = self
            .generic_fn_templates
            .get(base)
            .expect("a specialization is only ever requested for a retained template")
            .clone();
        // A specialization's own `public` mirrors the unspecialized
        // template's — see `Self::specialize_method`'s identical treatment.
        let public = self.reg.fn_sig(base).map(|s| s.public).unwrap_or(true);
        let bindings: HashMap<String, Type> =
            tmpl.type_params.iter().cloned().zip(args.iter().cloned()).collect();
        let (saved_ns, saved_loops, saved_bindings) = self.enter_specialization(tmpl.ns.clone(), bindings);
        let result = self.specialize_defun_body(heap, interp, &tmpl, mangled, public);
        self.exit_specialization(saved_ns, saved_loops, saved_bindings);
        result
    }

    /// Swaps in a specialization re-check's context — the template's defining
    /// namespace, a fresh loop stack (the same isolation `check_lambda`
    /// applies to a nested function body: a specialization requested from
    /// inside somebody's `loop` must not see that loop as its own), and the
    /// type-variable bindings `canon` substitutes — returning the saved state
    /// for [`Self::exit_specialization`] to restore.
    fn enter_specialization(
        &mut self,
        ns: Vec<String>,
        bindings: HashMap<String, Type>,
    ) -> (Vec<String>, Vec<Type>, HashMap<String, Type>) {
        let saved_ns = std::mem::replace(&mut self.ns, ns);
        let saved_loops = std::mem::take(&mut *self.loop_stack.borrow_mut());
        let saved_bindings = std::mem::replace(&mut self.type_var_bindings, bindings);
        (saved_ns, saved_loops, saved_bindings)
    }

    fn exit_specialization(
        &mut self,
        saved_ns: Vec<String>,
        saved_loops: Vec<Type>,
        saved_bindings: HashMap<String, Type>,
    ) {
        self.type_var_bindings = saved_bindings;
        *self.loop_stack.borrow_mut() = saved_loops;
        self.ns = saved_ns;
    }

    /// Re-checks or re-synthesizes a generic-owner method at the owner's
    /// concrete type arguments — [`Self::specialize_defun`]'s method-side
    /// counterpart. A written method ([`MethodTemplate::Form`]) is re-checked
    /// from its retained source with the *receiver-written* type-variable
    /// names bound; a generic `defstruct` accessor (`Getter`/`Setter`) has no
    /// source to re-check and is re-synthesized from the `AdtDef`'s field
    /// types instead (the same ASTs `check_defstruct` builds, with the field
    /// type substituted).
    fn specialize_method(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        req: &SpecRequest,
    ) -> Result<TopLevel, Error> {
        let SpecRequest::Method { type_fq, base, args, mangled } = req else {
            unreachable!("drain routes Method requests here")
        };
        // A specialization's own `public` mirrors the unspecialized template
        // method's — `Interp`'s scope tree gates visibility on this bit (see
        // `Expr::Assoc`'s doc comment), so a generated specialization must
        // carry the same one its template declared, not a default.
        let public = self
            .reg
            .type_def(type_fq)
            .and_then(|d| d.assoc.get(base))
            .map(|af| af.sig.public)
            .unwrap_or(true);
        let tmpl = self
            .generic_method_templates
            .get(&(type_fq.clone(), base.clone()))
            .expect("a method specialization is only ever requested for a retained template")
            .clone();
        match tmpl {
            MethodTemplate::Form { parts, ns, written_vars } => {
                let bindings: HashMap<String, Type> =
                    written_vars.into_iter().zip(args.iter().cloned()).collect();
                let (saved_ns, saved_loops, saved_bindings) = self.enter_specialization(ns, bindings);
                let result = self.specialize_method_form(heap, interp, &parts, mangled, public);
                self.exit_specialization(saved_ns, saved_loops, saved_bindings);
                result
            }
            MethodTemplate::Getter { index } => Ok(self.synthesize_accessor(type_fq, args, index, mangled, true, public)),
            MethodTemplate::Setter { index } => Ok(self.synthesize_accessor(type_fq, args, index, mangled, false, public)),
        }
    }

    fn specialize_method_form(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mangled: &str,
        public: bool,
    ) -> Result<TopLevel, Error> {
        let MethodSig { instance, self_name, recv_ty, type_fq, params, ret, body_start, .. } =
            self.parse_defmethod_sig(heap, parts)?;
        let mut binds: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty));
        }
        binds.extend(params.clone());
        // Bounds are deliberately dropped here, mirroring
        // `specialize_defun_body`: with the owner's type variables concrete,
        // every bounded method call resolves against the real receiver type
        // (and was already validated at the call site).
        let env = Env::new().extended(binds);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], &[], Some(&ret))?;
        Ok(TopLevel::Defmethod {
            type_name: type_fq,
            method: mangled.to_string(),
            instance,
            self_name,
            params,
            ret,
            body,
            type_params: Vec::new(),
            public,
        })
    }

    /// Builds a concrete accessor `TopLevel::Defmethod` for a generic
    /// `defstruct`'s field `index`, with the field type substituted at the
    /// owner's concrete `args` — mirrors the ASTs `check_defstruct`
    /// synthesizes, minus any registry mutation.
    fn synthesize_accessor(
        &self,
        type_fq: &Path,
        args: &[Type],
        index: usize,
        mangled: &str,
        getter: bool,
        public: bool,
    ) -> TopLevel {
        let def = self.reg.type_def(type_fq).expect("an accessor template implies the type exists");
        let subst: HashMap<String, Type> =
            def.params.iter().cloned().zip(args.iter().cloned()).collect();
        let field_ty = subst_apply(&def.variants[0].fields[index], &subst);
        let recv_ty = Type::Named(type_fq.clone(), args.to_vec());
        let self_var = Typed { loc: None, expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
        if getter {
            TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: mangled.to_string(),
                instance: true,
                self_name: Some("self".to_string()),
                params: Vec::new(),
                ret: field_ty.clone(),
                body: vec![Typed { loc: None, expr: Expr::FieldGet(Box::new(self_var), index), ty: field_ty }],
                type_params: Vec::new(),
                public,
            }
        } else {
            let value_var = Typed { loc: None, expr: Expr::Var("value".to_string()), ty: field_ty.clone() };
            TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: mangled.to_string(),
                instance: true,
                self_name: Some("self".to_string()),
                params: vec![("value".to_string(), field_ty)],
                ret: Type::Unit,
                body: vec![Typed { loc: None,
                    expr: Expr::FieldSet(Box::new(self_var), index, Box::new(value_var)),
                    ty: Type::Unit,
                }],
                type_params: Vec::new(),
                public,
            }
        }
    }

    fn specialize_defun_body(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        tmpl: &FnTemplate,
        mangled: &Path,
        public: bool,
    ) -> Result<TopLevel, Error> {
        // `bounds` deliberately dropped: the type variables are concrete
        // here, so method calls on them resolve directly against the real
        // receiver type (`check_instance_method`'s ordinary branch, never
        // the bounds branch), and the bounds' own validity was already
        // verified at the call site that requested this instantiation
        // (`check_call`'s where-clause validation).
        let (params, _param_locs, rest, ret, _bounds, body_start) = self.parse_defun_sig(heap, &tmpl.parts)?;
        let mut params = params;
        if let Some((rname, _, _)) = &rest {
            params.push((rname.clone(), sexpr_ty()));
        }
        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &tmpl.parts[body_start..], &[], Some(&ret))?;
        Ok(TopLevel::Defun { name: mangled.clone(), type_params: Vec::new(), params, ret, body, public })
    }

    /// Whether a declared type's *runtime representation* is a heap value
    /// (`RtValue::Sexpr` wrapping a `mem::Value`) — the built-in `Sexpr`
    /// itself, any `AdtKind::Struct` type (`defstruct`/`Vector<T>`/
    /// `cons-cell<K,V>`), `HashTable<K,V>` (boxed since the unification's
    /// Stage 5, though its `AdtDef` still says `Sum` — a recorded historical
    /// asymmetry), a `Scope<V>` whose `V` is itself heap-repr (boxed since
    /// the unification's Stage 8; a scope of anything else — LLVM handles
    /// above all — stays Rust-native), or an enum instantiation
    /// (`Option`/`Result`/`Error`/user `defenum`) *every one of whose
    /// variant fields is itself representable* — see
    /// [`Self::enum_fields_representable`] for why this must recurse rather
    /// than key on "is this a `Sum` with variants": `Option<llvm-value>`
    /// and friends (the (typelisp-hosted) compiler body's own bread and
    /// butter — `loop-exit`/`loop-slot`/every `env`/`fn-env` lookup) are
    /// exactly the same `Sum`-with-variants shape as `Option<i64>`, yet
    /// must stay native (`RtValue::Data`, a Rust-side value with no heap
    /// form at all) since an LLVM handle can never be heap-boxed. This is
    /// the checker-side twin of the interpreter's
    /// `Interp::heap_repr_kind`/`enum_fields_representable` (which carry
    /// the matching `Scope<V>` recursion and enum-field recursion), used to
    /// bake binding-slot routing into `Pattern::Bind` (the one binding site
    /// whose type the evaluator can't read off its own AST node).
    fn is_heap_repr(&self, ty: &Type) -> bool {
        self.is_heap_repr_seen(ty, &mut HashSet::new())
    }

    fn is_heap_repr_seen(&self, ty: &Type, seen: &mut HashSet<Path>) -> bool {
        match ty {
            Type::Named(p, args) if *p == Path::root("scope") && args.len() == 1 => self.is_heap_repr_seen(&args[0], seen),
            Type::Named(p, _) if *p == Path::root("sexpr") || *p == Path::root("hashtable") => true,
            Type::Named(p, args) => match self.reg.type_def(p) {
                Some(d) if d.kind == AdtKind::Struct => true,
                Some(d) if d.kind == AdtKind::Sum && !d.variants.is_empty() => {
                    self.enum_fields_representable(p, &d.params, &d.variants, args, seen)
                }
                _ => false,
            },
            // A trait object is always a `BoxedObj::Dyn` fat box, whatever
            // the concrete value inside is — so it is heap-repr by
            // construction. (Not covered by the `_` arm below, which would
            // silently answer `false` and make every `Type::Dyn` unstorable
            // in a `Sexpr`.)
            Type::Dyn(..) => true,
            _ => false,
        }
    }

    /// Whether every field of every variant of enum type `name` — after
    /// substituting `args` for `params` — is itself representable (a plain
    /// scalar that boxes trivially, e.g. `i64`/`Str`, or recursively
    /// heap-repr by [`Self::is_heap_repr_seen`]). `seen` guards a
    /// self-/mutually-referential `defenum` (e.g. a tree type) from
    /// infinite recursion: revisiting a path already being computed can't
    /// introduce a new native-only leaf, so it's treated as representable
    /// there — the same short-circuit `Sexpr`'s own recursive `Cons`
    /// variant gets for free by being special-cased before ever reaching
    /// here.
    fn enum_fields_representable(&self, name: &Path, params: &[String], variants: &[Variant], args: &[Type], seen: &mut HashSet<Path>) -> bool {
        if !seen.insert(name.clone()) {
            return true;
        }
        let subst: HashMap<String, Type> = params.iter().cloned().zip(args.iter().cloned()).collect();
        let ok = variants.iter().all(|variant| {
            variant.fields.iter().all(|f| {
                let fty = subst_apply(f, &subst);
                is_boxable_scalar(&fty) || self.is_heap_repr_seen(&fty, seen)
            })
        });
        seen.remove(name);
        ok
    }

    /// Whether `t` still mentions an unresolved type variable — a
    /// single-segment `Named` with no registered type definition, the same
    /// convention `check_call`'s where-clause validation already reads
    /// (post-`canon`, every *real* nominal type resolved to a located path
    /// with a `type_def`). True means the surrounding call is being checked
    /// inside another generic function's own (diagnostic) body, where the
    /// type only becomes concrete once *that* function is specialized — so
    /// no instantiation can be generated yet.
    fn type_is_open(&self, t: &Type) -> bool {
        match t {
            Type::Named(n, args) => {
                (args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none())
                    || args.iter().any(|a| self.type_is_open(a))
            }
            Type::Fn(ps, rest, r) => {
                ps.iter().any(|p| self.type_is_open(p))
                    || matches!(rest, Some(t) if self.type_is_open(t))
                    || self.type_is_open(r)
            }
            // The trait head is a trait, never a type variable; only the
            // associated-type pins can still be open.
            Type::Dyn(_, pins) => pins.iter().any(|p| self.type_is_open(p)),
            _ => false,
        }
    }

    /// Whether `v` is a `(where ...)` clause (vs. an ordinary body form) —
    /// `Checker::check_defun` peeks at this to decide whether to consume it.
    fn is_where_clause(&self, heap: &Heap, v: Value) -> Result<bool, Error> {
        Ok(matches!(v, Value::Cons(_))
            && matches!(heap.list_to_vec(v)?.first(), Some(Value::Symbol(id)) if heap.symbol_name(*id) == "where"))
    }

    /// `(where (Trait1 T1 (Assoc1 Concrete1)...) (Trait2 T2)...)`: trait
    /// bounds on a generic `defun`'s type parameters, keyed by parameter
    /// name, each optionally pinning one or more of that trait's associated
    /// types to a concrete type — see `TraitBound`'s doc comment for how
    /// pins are used. Trait names are resolved as bare root-module paths
    /// (`Path::root`, not `Checker::resolve_trait_name`), mirroring
    /// `Checker::check_instance_method`'s existing assumption that every
    /// trait lives at the root module — a pre-existing limitation, not new
    /// here.
    fn parse_where_clause(&self, heap: &Heap, v: Value) -> Result<HashMap<String, Vec<TraitBound>>, Error> {
        let elems = heap.list_to_vec(v)?;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        for clause in &elems[1..] {
            let parts = heap.list_to_vec(*clause)?;
            if parts.len() < 2 {
                return Err(Error::TypeError(
                    "where: each bound must be (Trait type-param (AssocName Type)...)".into(),
                ));
            }
            let trait_name = match parts[0] {
                Value::Symbol(id) => Path::root(heap.symbol_name(id)),
                _ => return Err(Error::TypeError("where: trait name must be a symbol".into())),
            };
            let tparam = match parts[1] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("where: type parameter must be a symbol".into())),
            };
            let mut assoc: HashMap<String, Type> = HashMap::new();
            for pin in &parts[2..] {
                let pin_parts = heap.list_to_vec(*pin)?;
                if pin_parts.len() != 2 {
                    return Err(Error::TypeError("where: associated-type pin must be (AssocName Type)".into()));
                }
                let aname = match pin_parts[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("where: associated type name must be a symbol".into())),
                };
                let aty = self.canon(&parse_type(heap, pin_parts[1])?);
                assoc.insert(aname, aty);
            }
            bounds.entry(tparam).or_default().push(TraitBound { trait_path: trait_name, assoc });
        }
        Ok(bounds)
    }

    /// `parts[0]` of a `defun` form: a bare symbol names an ordinary
    /// function; `(name T1 T2...)` additionally declares generic type
    /// parameter names — mirroring `defmethod`'s `(self Type)` receiver-list
    /// syntax. Type parameter names are just ordinary (lowercase, after
    /// reader case-folding) identifiers; `Checker::check_call` resolves them
    /// against actual argument/expected types with the same `unify`/
    /// `subst_apply` machinery `check_construct` uses for an ADT's `params`.
    /// Parse a definition header (`defun`/`defstruct`/`defenum` name position)
    /// into a name and its declared type-parameter names. The surface form is
    /// `name` (non-generic) or `name<T1,T2...>` — the same angle-bracket
    /// generic syntax a *type* position uses (`Vector<T>`), so all three
    /// defining forms read consistently. The old `(name T1 T2...)` list form
    /// is no longer accepted.
    fn parse_defun_name(&self, heap: &Heap, v: Value) -> Result<(String, Vec<String>), Error> {
        match v {
            Value::Symbol(id) => parse_generic_name_header(heap.symbol_name(id)),
            _ => Err(Error::TypeError(
                "definition name must be a symbol (write `name<T>` for type parameters)".into(),
            )),
        }
    }

    /// `(defmacro name (p1 p2 ...) body...)`: a compile-time code
    /// transformer. Every parameter and the implicit return are `Sexpr` — so,
    /// unlike `defun`, the parameter list is bare names with no type
    /// annotations (they'd always just say `Sexpr`). Registers the signature
    /// (arity only) before checking the body, so self-recursion works,
    /// matching [`Self::check_defun`]. The body is checked exactly like a
    /// `defun`'s; calling it later at macro-expansion time reuses the same
    /// machinery as calling an ordinary function (see [`MacroExpander`]) —
    /// `Interp::exec` stores a `Defmacro` in the very same function table.
    fn check_defmacro(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevel, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError("defmacro: (defmacro name (params) body...)".into()));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmacro: name must be a symbol".into())),
        };
        // Parse the CL-style lambda list into its four ordered regions:
        // `required &optional opt... &rest r &key key...`. An `&optional`/
        // `&key` item is either a bare `name` (default `nil`) or a
        // `(name default-form)` pair; a required or `&rest` item is always a
        // bare name. The markers must appear in this order and at most once
        // each (enforced via the running section rank below).
        let param_vals = heap.list_to_vec(parts[1])?;
        let (required_names, optionals, rest_name, key_specs) =
            self.parse_macro_lambda_list(heap, &param_vals)?;
        let rest = rest_name.is_some();

        // `params` lists every binding name in expansion order — required,
        // then optional, then `&rest`, then key — mirroring how
        // `Interp::bind_macro_args` fills them (see `TopLevel::Defmacro`).
        let mut params: Vec<String> = required_names.clone();
        params.extend(optionals.iter().map(|(n, _)| n.clone()));
        if let Some(r) = &rest_name {
            params.push(r.clone());
        }
        params.extend(key_specs.iter().map(|(n, _)| n.clone()));

        let fq_name = self.fq(&name);
        self.check_redef("macro", &name, self.cur_ns().macros.get(&name))?;
        self.reg.root.module_mut(&self.ns).macros.insert(
            name,
            MacroDef {
                required: required_names.len(),
                optional: optionals.len(),
                rest,
                keys: key_specs.iter().map(|(n, _)| n.clone()).collect(),
                public,
                builtin: false,
            },
        );
        if let Some(loc) = def_loc {
            self.reg.def_locs.macros.insert(fq_name.clone(), loc);
        }

        // Every parameter — and the macro's implicit result — is `Sexpr`. A
        // default-value form is checked against `Sexpr` in an environment
        // holding exactly the params bound *before* it (CL: an optional/key
        // default may reference earlier params, never later ones), which is
        // also the order `bind_macro_args` evaluates them in.
        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        let mut bindings: Vec<(String, Type)> =
            required_names.iter().map(|n| (n.clone(), sexpr_ty.clone())).collect();

        let mut opt_defaults: Vec<Vec<Typed>> = Vec::with_capacity(optionals.len());
        for (n, default) in &optionals {
            let checked = self.check_macro_default(heap, interp, &bindings, *default, &sexpr_ty)?;
            opt_defaults.push(checked);
            bindings.push((n.clone(), sexpr_ty.clone()));
        }
        if let Some(r) = &rest_name {
            bindings.push((r.clone(), sexpr_ty.clone()));
        }
        let mut key_defaults: Vec<(String, Vec<Typed>)> = Vec::with_capacity(key_specs.len());
        for (n, default) in &key_specs {
            let checked = self.check_macro_default(heap, interp, &bindings, *default, &sexpr_ty)?;
            key_defaults.push((n.clone(), checked));
            bindings.push((n.clone(), sexpr_ty.clone()));
        }

        let env = Env::new().extended(bindings);
        let body_locs = parts_locs.get(2..).unwrap_or(&[]);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[2..], body_locs, Some(&sexpr_ty))?;
        let lambda = MacroLambda { required: required_names.len(), optionals: opt_defaults, keys: key_defaults };
        Ok(TopLevel::Defmacro { name: fq_name, params, body, rest, lambda, public })
    }

    /// Check one `&optional`/`&key` default-value form (or none) against
    /// `Sexpr`, in an environment holding the params bound before it. Returns
    /// the checked body as a `Vec<Typed>` — a single-element vector for a
    /// supplied default, or empty for "no default" (binds `nil` at expansion).
    fn check_macro_default(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        bindings: &[(String, Type)],
        default: Option<Value>,
        sexpr_ty: &Type,
    ) -> Result<Vec<Typed>, Error> {
        match default {
            None => Ok(Vec::new()),
            Some(form) => {
                let env = Env::new().extended(bindings.to_vec());
                let typed = self.check(heap, interp, &env, form, Some(sexpr_ty))?;
                Ok(vec![typed])
            }
        }
    }

    /// Parse a `defmacro` lambda list into `(required names, optionals,
    /// &rest name, key specs)`, where each optional/key spec is
    /// `(name, default-form)` with `default-form` `None` for a bare name and
    /// `Some(form)` for a `(name form)` pair. Enforces the CL section order
    /// (`required &optional &rest &key`), each marker at most once, and that
    /// `&rest` names exactly one parameter. See [`Self::check_defmacro`].
    #[allow(clippy::type_complexity)]
    fn parse_macro_lambda_list(
        &self,
        heap: &Heap,
        param_vals: &[Value],
    ) -> Result<(Vec<String>, Vec<(String, Option<Value>)>, Option<String>, Vec<(String, Option<Value>)>), Error> {
        // Section rank: required=0, &optional=1, &rest=2, &key=3. A marker may
        // only advance the rank forward, so each appears at most once and in
        // order.
        let mut rank = 0u8;
        let mut required_names: Vec<String> = Vec::new();
        let mut optionals: Vec<(String, Option<Value>)> = Vec::new();
        let mut rest_name: Option<String> = None;
        let mut keys: Vec<(String, Option<Value>)> = Vec::new();

        for p in param_vals {
            if let Value::Symbol(id) = p {
                match heap.symbol_name(*id) {
                    "&optional" => {
                        if rank >= 1 {
                            return Err(Error::TypeError("defmacro: &optional must precede &rest and &key, and appear once".into()));
                        }
                        rank = 1;
                        continue;
                    }
                    "&rest" => {
                        if rank >= 2 {
                            return Err(Error::TypeError("defmacro: &rest must precede &key, and appear once".into()));
                        }
                        rank = 2;
                        continue;
                    }
                    "&key" => {
                        if rank >= 3 {
                            return Err(Error::TypeError("defmacro: &key may appear only once".into()));
                        }
                        rank = 3;
                        continue;
                    }
                    _ => {}
                }
            }
            match rank {
                0 => required_names.push(Self::macro_param_name(heap, *p, "required")?),
                1 => optionals.push(Self::macro_opt_key_spec(heap, *p, "&optional")?),
                2 => {
                    if rest_name.is_some() {
                        return Err(Error::TypeError("defmacro: &rest takes exactly one parameter name".into()));
                    }
                    rest_name = Some(Self::macro_param_name(heap, *p, "&rest")?);
                }
                _ => keys.push(Self::macro_opt_key_spec(heap, *p, "&key")?),
            }
        }
        if rank == 2 && rest_name.is_none() {
            return Err(Error::TypeError("defmacro: &rest must be followed by exactly one parameter name".into()));
        }
        Ok((required_names, optionals, rest_name, keys))
    }

    /// A bare-symbol lambda-list parameter name (`required`/`&rest`).
    fn macro_param_name(heap: &Heap, v: Value, section: &str) -> Result<String, Error> {
        match v {
            Value::Symbol(id) => Ok(heap.symbol_name(id).to_string()),
            _ => Err(Error::TypeError(format!("defmacro: {} parameter must be a name", section))),
        }
    }

    /// An `&optional`/`&key` item: either a bare `name` (no default) or a
    /// `(name default-form)` pair. Returns `(name, Some(form)|None)`.
    fn macro_opt_key_spec(heap: &Heap, v: Value, section: &str) -> Result<(String, Option<Value>), Error> {
        match v {
            Value::Symbol(id) => Ok((heap.symbol_name(id).to_string(), None)),
            Value::Cons(_) => {
                let items = heap.list_to_vec(v)?;
                if items.is_empty() || items.len() > 2 {
                    return Err(Error::TypeError(format!(
                        "defmacro: {} parameter must be `name` or `(name default)`",
                        section
                    )));
                }
                let name = Self::macro_param_name(heap, items[0], section)?;
                Ok((name, items.get(1).copied()))
            }
            _ => Err(Error::TypeError(format!(
                "defmacro: {} parameter must be `name` or `(name default)`",
                section
            ))),
        }
    }

    /// Parse a `((name type)...)` parameter list (types canonicalized to FQ)
    /// for `labels`/`defmethod` — always fixed-arity (`defun`/`lambda` accept
    /// a trailing `&rest` through `Self::parse_params_rest` instead, and
    /// `defmacro` has its own untyped `&rest` lambda-list marker — see
    /// `Self::check_defmacro`). Also returns each parameter name's own source
    /// position (parallel to the returned params), for the checked body's
    /// `Env` binding (`Env::extended_with_locs`) — see `Self::parse_param_pairs`.
    #[allow(clippy::type_complexity)]
    fn parse_params(&self, heap: &Heap, v: Value) -> Result<(Vec<(String, Type)>, Vec<Option<Loc>>), Error> {
        let elems = heap.list_to_vec(v)?;
        self.parse_param_pairs(heap, &elems)
    }

    /// Parse a `((name type)... [&rest (name elem-type)])` parameter list for
    /// `defun`/`lambda`: an optional trailing `&rest (name elem-type)` marks
    /// the function variadic — every call-site argument from that position on
    /// must have type `elem-type`, individually checked, then collected into
    /// a single `Sexpr` list bound to `name` inside the body (see
    /// `Self::check_defun`/`Self::check_lambda`) — like every Lisp's `&rest`
    /// parameter, an ordinary list, never a homogeneous array type; unlike
    /// `Self::check_defmacro`'s bare `&rest name` (always untyped `Sexpr`),
    /// `defun`/`lambda` additionally check each element against `elem-type`
    /// at every call site, since `defun`/`lambda` parameters always carry an
    /// explicit type. The rest parameter comes back with its name's source
    /// position (see [`RestParam`]), same as the fixed params' parallel locs.
    #[allow(clippy::type_complexity)]
    fn parse_params_rest(
        &self,
        heap: &Heap,
        v: Value,
    ) -> Result<(Vec<(String, Type)>, Vec<Option<Loc>>, Option<RestParam>), Error> {
        let elems = heap.list_to_vec(v)?;
        let rest_marker = elems
            .iter()
            .position(|p| matches!(p, Value::Symbol(id) if heap.symbol_name(*id) == "&rest"));
        match rest_marker {
            Some(i) => {
                if i + 2 != elems.len() {
                    return Err(Error::TypeError(
                        "&rest must be followed by exactly one parameter, as the last item in the parameter list".into(),
                    ));
                }
                let (params, param_locs) = self.parse_param_pairs(heap, &elems[..i])?;
                let (mut rest, mut rest_locs) = self.parse_param_pairs(heap, &elems[i + 1..])?;
                // `&rest`'s declared element type isn't validated against
                // `sexpr_ctor_for` here: a generic `defun` (e.g. `(defun
                // (f T) ((a T) &rest (xs T)) ...)`) may declare it as a bare
                // type parameter, only resolved to something concrete once a
                // call site's actual arguments are checked — see
                // `Checker::wrap_rest_elem`, which validates the *resolved*
                // type instead, every time the list is actually collected.
                let (rname, rty) = rest.remove(0);
                Ok((params, param_locs, Some((rname, rty, rest_locs.remove(0)))))
            }
            None => {
                let (params, param_locs) = self.parse_param_pairs(heap, &elems)?;
                Ok((params, param_locs, None))
            }
        }
    }

    /// Parse a slice of `(name type)` binding forms, alongside each binding's
    /// name's own source position (`heap.list_to_vec_locs`'s first element for
    /// that binding — independent of where `pairs` itself came from, so this
    /// needs no location input from the caller).
    #[allow(clippy::type_complexity)]
    fn parse_param_pairs(&self, heap: &Heap, pairs: &[Value]) -> Result<(Vec<(String, Type)>, Vec<Option<Loc>>), Error> {
        let mut out = Vec::new();
        let mut locs = Vec::new();
        for binding in pairs {
            let pair_locs = heap.list_to_vec_locs(*binding)?;
            let pair: Vec<Value> = pair_locs.iter().map(|(v, _)| *v).collect();
            if pair.len() != 2 {
                return Err(Error::TypeError("parameter must be (name type)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("parameter name must be a symbol".into())),
            };
            out.push((name, self.canon(&parse_type(heap, pair[1])?)));
            locs.push(pair_locs.first().and_then(|(_, l)| l.clone()));
        }
        Ok((out, locs))
    }

    /// Parse `defstruct` field bindings: `(name type)` or `(pub name type)`.
    /// Field visibility is independent of the struct's own `pub` — like every
    /// other `pub` in this language it's opt-in per item, never inherited
    /// from a container — so a field defaults to private (its getter/setter
    /// only reachable from the struct's own module) even on a `pub`
    /// `defstruct`, and `pub` on a field of a non-`pub` struct is legal (the
    /// field is reachable cross-module on any value of that type the current
    /// module's own `pub` API happens to hand out, even though outside code
    /// can't name or construct the type itself).
    fn parse_struct_fields(&self, heap: &Heap, pairs: &[Value]) -> Result<Vec<(String, Type, bool)>, Error> {
        let mut out = Vec::new();
        for binding in pairs {
            let elems = heap.list_to_vec(*binding)?;
            let (public, rest) = match elems.first() {
                Some(Value::Symbol(id)) if heap.symbol_name(*id) == "pub" => (true, &elems[1..]),
                _ => (false, &elems[..]),
            };
            if rest.len() != 2 {
                return Err(Error::TypeError("defstruct: field must be (name type) or (pub name type)".into()));
            }
            let name = match rest[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("defstruct: field name must be a symbol".into())),
            };
            out.push((name, self.canon(&parse_type(heap, rest[1])?), public));
        }
        Ok(out)
    }

    // ---- deftrait / impl ---------------------------------------------------

    /// `(deftrait Name (type AssocName)... (method-name ((self Self) params...) Ret)...)`:
    /// declares a trait as a set of method signature *templates* (no
    /// bodies) — `Self` and any declared associated type name are usable as
    /// ordinary type variables in a signature, exactly like a generic
    /// `defstruct`'s own type parameters (`Checker::parse_struct_fields`).
    /// Registers a [`TraitDef`]; `Checker::check_impl` later supplies bodies
    /// for some concrete implementing type. No type-checking happens here
    /// beyond parsing — a signature template's `Self`/associated-type
    /// variables aren't real types, so there's nothing to check yet.
    fn check_deftrait(&mut self, heap: &Heap, parts: &[Value], public: bool, def_loc: Option<Loc>) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError(
                "deftrait: (deftrait Name (type AssocName)... (method (params...) ret)...)".into(),
            ));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("deftrait: name must be a symbol".into())),
        };
        let mut assoc_types = Vec::new();
        let mut methods = HashMap::new();
        // Source order, kept alongside `methods` for vtable slot numbering.
        let mut method_order: Vec<String> = Vec::new();
        for item in &parts[1..] {
            let elems = heap.list_to_vec(*item)?;
            let head = match elems.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("deftrait: item must start with a symbol".into())),
            };
            if head == "type" {
                if elems.len() != 2 {
                    return Err(Error::TypeError("deftrait: (type AssocName)".into()));
                }
                let aname = match elems[1] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("deftrait: associated type name must be a symbol".into())),
                };
                assoc_types.push(aname);
                continue;
            }
            if elems.len() != 3 {
                return Err(Error::TypeError("deftrait: method signature must be (name (params...) ret)".into()));
            }
            let (params, _param_locs) = self.parse_param_pairs(heap, &heap.list_to_vec(elems[1])?)?;
            let ret = self.canon(&parse_type(heap, elems[2])?);
            let sig = FnSig {
                type_params: vec![],
                params: params.iter().map(|(_, t)| t.clone()).collect(),
                ret,
                public: true,
                rest: None,
                builtin: false,
                bounds: HashMap::new(),
            };
            // `method_order` is the vtable layout, so it must stay in step
            // with `methods` — a duplicated name would silently give the
            // trait two slots for one entry.
            if methods.contains_key(&head) {
                return Err(Error::TypeError(format!("deftrait: duplicate method `{}`", head)));
            }
            method_order.push(head.clone());
            methods.insert(head, sig);
        }
        let fq_name = self.fq(&name);
        self.check_redef("trait", &name, self.cur_ns().traits.get(&name))?;
        self.reg
            .root
            .module_mut(&self.ns)
            .traits
            .insert(name, TraitDef { name: fq_name.clone(), assoc_types, methods, method_order, public, builtin: false });
        if let Some(loc) = def_loc {
            self.reg.def_locs.traits.insert(fq_name.clone(), loc);
        }
        Ok(TopLevel::Module { path: fq_name, body: vec![] })
    }

    /// `(impl TraitName TargetType (type AssocName ConcreteType)... (method-name (recv params...) Ret body...)...)`:
    /// implements `TraitName` for `TargetType`. Each method is checked and
    /// registered exactly like an ordinary `(defmethod method-name ((self
    /// TargetType) params...) Ret body...)` — `Self` (and any of the
    /// trait's associated type names) occurring in a method's *written*
    /// parameter/return type syntax is textually substituted with
    /// `TargetType`/the `(type ...)` binding's concrete type *before*
    /// parsing, by rewriting the read `Value` tree (`Checker::subst_value`),
    /// so the rest of `check_defmethod` never has to know `Self`/associated
    /// types exist — by the time it parses the receiver/parameter/return
    /// types, they're already concrete. This is also why a method needs no
    /// separate "does this satisfy the trait's signature template" check:
    /// it's checked as a perfectly ordinary `defmethod`, just like any other.
    ///
    /// `TargetType`'s own `AdtDef.impls` gets `TraitName`'s path appended —
    /// the one piece of metadata `Checker::check_instance_method`'s
    /// type-variable branch needs later, since by then the method itself is
    /// just one more entry in `TargetType`'s ordinary `assoc` table.
    fn check_impl(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value], public: bool) -> Result<TopLevel, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError("impl: (impl TraitName TargetType (type AssocName Type)... (method ...)...)".into()));
        }
        let trait_name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("impl: trait name must be a symbol".into())),
        };
        let trait_fq = self.resolve_trait_name(&trait_name)?;
        let target_ty = self.canon(&parse_type(heap, parts[1])?);
        let target_fq = match &target_ty {
            Type::Named(n, _) => n.clone(),
            other => match prim_type_path(other) {
                Some(p) => p,
                None => return Err(Error::TypeError("impl: target must be a data type".into())),
            },
        };
        if self.reg.type_def(&target_fq).is_none() {
            return Err(Error::TypeError(format!("impl: unknown type `{}`", target_fq)));
        }

        // `Self` -> the target type's *written* form (`parts[1]`, unparsed —
        // substitution happens before `parse_type` runs on each method, so
        // a generic target like `(VectorIter T)` carries its own type
        // variable `T` through untouched). Each `(type AssocName Type)`
        // binding adds one more substitution, keyed by the trait's
        // associated type name.
        let mut subst: HashMap<String, Value> = HashMap::new();
        subst.insert("self".to_string(), parts[1]);
        // Parsed (not just textual) parallel of `subst`'s `(type AssocName
        // Type)` bindings, for `AdtDef::trait_assoc` — lets
        // `Checker::check_call`/`resolve_trait_assoc_type` later recover
        // what this `impl` concretely binds each associated type to, without
        // re-parsing `subst`'s raw `Value`s.
        let mut assoc_concrete: HashMap<String, Type> = HashMap::new();

        let mut method_forms: Vec<Value> = Vec::new();
        for item in &parts[2..] {
            let elems = heap.list_to_vec(*item)?;
            let head = match elems.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("impl: item must start with a symbol".into())),
            };
            if head == "type" {
                if elems.len() != 3 {
                    return Err(Error::TypeError("impl: (type AssocName Type)".into()));
                }
                let aname = match elems[1] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("impl: associated type name must be a symbol".into())),
                };
                assoc_concrete.insert(aname.clone(), self.canon(&parse_type(heap, elems[2])?));
                subst.insert(aname, elems[2]);
                continue;
            }
            method_forms.push(*item);
        }

        let mut body = Vec::new();
        for m in &method_forms {
            // `(method-name (recv-list) ret body...)`. Substitution applies
            // only to *type* positions — each receiver/parameter's type
            // (never its bound *name*, which would collide with `subst`'s
            // `"self"` key: the receiver is conventionally also named
            // `self`, a plain variable, completely unrelated to the `Self`
            // *type* keyword even though both case-fold to the same
            // string) — and the return type. The body is left untouched:
            // it's executable code, not type syntax, so any `Self`/`Item`
            // appearing there is an ordinary (if confusingly named)
            // variable/function reference, not something to substitute.
            let method_loc = heap.cons_loc(*m);
            // `list_to_vec_locs` so the body forms (`elems[3..]`, left
            // untouched below — see this loop's doc comment) keep their
            // original source position; the rebuilt name/receiver/return
            // slots (`new_elems[0..3]`) have no natural location of their own.
            let elems_locs = heap.list_to_vec_locs(*m)?;
            let elems: Vec<Value> = elems_locs.iter().map(|(v, _)| *v).collect();
            if elems.len() < 3 {
                return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
            }
            let recv_pairs = heap.list_to_vec(elems[1])?;
            let mut new_recv_pairs = Vec::new();
            for pair in &recv_pairs {
                let p = heap.list_to_vec(*pair)?;
                if p.len() != 2 {
                    return Err(Error::TypeError("impl: receiver/parameter must be (name type)".into()));
                }
                let new_ty = Self::subst_value(heap, p[1], &subst)?;
                new_recv_pairs.push(self.list_from_vec(heap, &[p[0], new_ty])?);
            }
            let new_recv_list = self.list_from_vec(heap, &new_recv_pairs)?;
            let new_ret = Self::subst_value(heap, elems[2], &subst)?;
            let mut new_elems = vec![elems[0], new_recv_list, new_ret];
            new_elems.extend_from_slice(&elems[3..]);
            let mut new_elems_locs: Vec<Option<Loc>> = vec![None, None, None];
            new_elems_locs.extend(elems_locs[3..].iter().map(|(_, l)| l.clone()));
            let tl = self.check_defmethod(heap, interp, &new_elems, &new_elems_locs, public, method_loc)?;
            body.push(tl);
        }
        if let Some(def) = self.reg.type_def_mut(&target_fq) {
            def.impls.push(trait_fq.clone());
            def.trait_assoc.insert(trait_fq, assoc_concrete);
        }
        Ok(TopLevel::Module { path: target_fq, body })
    }

    /// Resolve a `deftrait`-defined trait's bare name to its fully-qualified
    /// [`Path`] — mirrors `Checker::resolve_type_name`, but for the separate
    /// `traits` table (traits aren't types and don't share its namespace).
    fn resolve_trait_name(&self, name: &str) -> Result<Path, Error> {
        let mut ns = self.ns.clone();
        loop {
            if let Some(td) = self.reg.root.module(&ns).and_then(|m| m.traits.get(name)) {
                return Ok(td.name.clone());
            }
            if ns.is_empty() {
                break;
            }
            ns.pop();
        }
        Err(Error::TypeError(format!("impl: unknown trait `{}`", name)))
    }

    /// [`Self::resolve_trait_name`] for a written [`Path`], returning `None`
    /// instead of an error: a bare name searches the ancestor module chain
    /// (the same walk), a qualified one is looked up in the named module.
    /// Used by `canon` to normalize a [`Type::Dyn`]'s trait head, which must
    /// not fail — an unresolvable trait is reported later, with a message
    /// about the trait object rather than about an `impl`.
    fn resolve_trait_path(&self, p: &Path) -> Option<Path> {
        if p.is_simple() {
            return self.resolve_trait_name(p.local()).ok();
        }
        self.reg
            .root
            .module(p.parent())
            .and_then(|m| m.traits.get(p.local()))
            .map(|td| td.name.clone())
    }

    /// Every implementation of `trait_path`'s `method` currently registered,
    /// as `(owning type, method name)` — what a compiled `:dyn` call site
    /// needs compiled before it can run (see `Expr::DynCall::impl_targets`).
    ///
    /// Generic owners are skipped: their method has no code until it is
    /// specialized, and a specialization is only named once a concrete
    /// instantiation is boxed — which is exactly where `Expr::DynBox`'s own
    /// slots (already monomorphized by `dyn_vtable_slots`) pull it in.
    fn dyn_impl_targets(&self, trait_path: &Path, method: &str) -> Vec<(Path, String)> {
        self.reg
            .trait_impls(trait_path)
            .into_iter()
            .filter(|ty| {
                self.reg.type_def(ty).is_some_and(|d| d.params.is_empty() && d.assoc.contains_key(method))
            })
            .map(|ty| (ty, method.to_string()))
            .collect()
    }

    /// Whether the built-in `Sexpr` type has a visible instance method named
    /// `method` — how a trait-object receiver decides between dispatching
    /// through its vtable and falling back to the `Sexpr` catalog.
    fn sexpr_has_instance_method(&self, method: &str) -> bool {
        self.reg
            .type_def(&Path::root("sexpr"))
            .and_then(|d| d.assoc.get(method))
            .is_some_and(|af| af.instance)
    }

    /// Wrap an already-checked concrete value as `:dyn trait_path<pins...>`,
    /// laying out the vtable it will dispatch through. The shared
    /// implementation of the implicit widening (`check_inner`'s expectation
    /// reconciliation) and the explicit `(as :dyn Trait e)`.
    fn coerce_to_dyn(&self, value: Typed, trait_path: &Path, pins: &[Type]) -> Result<Typed, Error> {
        // Re-boxing a trait object is not a coercion. Same-trait/same-pins
        // never reaches here (the types compare equal); a *different* trait
        // would be an upcast, which needs a second vtable that the source
        // box's contents can't supply — the concrete type is gone by then.
        if let Type::Dyn(from, from_pins) = &value.ty {
            return Err(Error::TypeError(format!(
                "cannot convert `{}` to `{}` — trait upcasting is not supported; box the concrete value as `:dyn {}` instead",
                mangle_type(&Type::Dyn(from.clone(), from_pins.clone())),
                mangle_type(&Type::Dyn(trait_path.clone(), pins.to_vec())),
                trait_path
            )));
        }
        let slots = self.dyn_vtable_slots(&value.ty, trait_path, pins)?;
        let concrete_key = mangle_type(&value.ty);
        let loc = value.loc.clone();
        Ok(Typed {
            loc,
            expr: Expr::DynBox {
                concrete_key,
                trait_path: trait_path.clone(),
                slots,
                value: Box::new(value),
            },
            ty: Type::Dyn(trait_path.clone(), pins.to_vec()),
        })
    }

    /// Check `(method dyn-receiver args...)` — a call through a trait
    /// object. The receiver's static type names the trait, so the method is
    /// looked up in the `TraitDef` (not in any concrete type's `assoc`
    /// table), and its position in `method_order` *is* the vtable slot.
    /// Returns `None` when `method` isn't one of the trait's, so the caller
    /// can report that with the receiver's type in hand.
    fn check_dyn_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        receiver: Typed,
        trait_path: &Path,
        pins: &[Type],
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let assoc_subst = self.dyn_assoc_subst(trait_path, pins)?;
        let tdef = self.check_object_safe(trait_path)?;
        let Some(slot) = tdef.method_order.iter().position(|n| n == method) else {
            return Err(Error::TypeError(format!(
                "`{}` is not a method of `{}` — a `:dyn {}` value can only call the trait's own methods",
                method, trait_path, trait_path
            )));
        };
        let sig = &tdef.methods[method];
        let want = sig.params.len() - 1; // minus the receiver
        if args.len() != want {
            return Err(Error::TypeError(format!(
                "{}::{}: expected {} argument(s), got {}",
                trait_path,
                method,
                want,
                args.len()
            )));
        }
        // The pins resolve the trait template's associated-type variables in
        // both directions: parameter types to check arguments against, and
        // the return type.
        let expected_params: Vec<Type> =
            sig.params[1..].iter().map(|t| subst_apply(t, &assoc_subst)).collect();
        let ret = subst_apply(&sig.ret, &assoc_subst);
        let mut typed = vec![receiver];
        for (i, (arg, pty)) in args.iter().zip(expected_params.iter()).enumerate() {
            typed.push(self.check_at(heap, interp, env, *arg, Some(pty), nth_loc(arg_locs, i))?);
        }
        Ok(Typed {
            loc: None,
            expr: Expr::DynCall {
                trait_path: trait_path.clone(),
                method: method.to_string(),
                slot,
                impl_targets: self.dyn_impl_targets(trait_path, method),
                args: typed,
            },
            ty: ret,
        })
    }

    /// Whether trait `trait_path` can be used as a trait object (`:dyn
    /// Trait`) at all — "object safety", checked once per `:dyn` type that
    /// actually gets built (coercion / `(as :dyn T e)` / a `:dyn` receiver
    /// call), never at `deftrait` time: a trait is allowed to have methods
    /// that only make sense statically, as long as nobody asks for a trait
    /// object of it.
    ///
    /// Every rule here is one form of "this method has no single entry point
    /// that fits one vtable slot, uniformly across all implementations", and
    /// each message says which. Returns the `TraitDef` so callers that need
    /// it next (slot layout, method lookup) don't re-fetch it.
    fn check_object_safe(&self, trait_path: &Path) -> Result<&TraitDef, Error> {
        let Some(tdef) = self.reg.trait_def(trait_path) else {
            return Err(Error::TypeError(format!("dyn: unknown trait `{}`", trait_path)));
        };
        if tdef.method_order.is_empty() {
            return Err(Error::TypeError(format!(
                "dyn {}: the trait declares no methods, so a trait object of it could not be called",
                trait_path
            )));
        }
        for m in &tdef.method_order {
            let sig = &tdef.methods[m];
            match sig.params.first() {
                Some(t) if is_self_tvar(t) => {}
                _ => {
                    return Err(Error::TypeError(format!(
                        "dyn {}: method `{}` has no `self` receiver — a static associated function has no value to dispatch on",
                        trait_path, m
                    )))
                }
            }
            if sig.params[1..].iter().any(mentions_self) || mentions_self(&sig.ret) {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}` mentions `Self` outside the receiver position, so its signature differs per implementation and cannot share one vtable slot",
                    trait_path, m
                )));
            }
            if !sig.type_params.is_empty() {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}` is generic — a vtable slot holds one compiled entry point, so there is nothing to specialize at the call site",
                    trait_path, m
                )));
            }
            if sig.rest.is_some() {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}` is variadic, which has no single vtable entry point",
                    trait_path, m
                )));
            }
        }
        Ok(tdef)
    }

    /// Check that a `:dyn Trait<pins...>` type pins exactly the trait's
    /// associated types, and return the name->type map that resolves them in
    /// a method signature. Positional, in `TraitDef::assoc_types` declaration
    /// order — `:dyn Iter<i32>` is `Iter` with `Item = i32`.
    fn dyn_assoc_subst(&self, trait_path: &Path, pins: &[Type]) -> Result<HashMap<String, Type>, Error> {
        let tdef = self.check_object_safe(trait_path)?;
        if pins.len() != tdef.assoc_types.len() {
            return Err(Error::TypeError(format!(
                "dyn {}: expected {} associated-type argument(s) ({}), got {} — write `:dyn {}<{}>`",
                trait_path,
                tdef.assoc_types.len(),
                tdef.assoc_types.join(", "),
                pins.len(),
                trait_path,
                tdef.assoc_types.join(",")
            )));
        }
        Ok(tdef.assoc_types.iter().cloned().zip(pins.iter().cloned()).collect())
    }

    /// Lay out the vtable for boxing a `concrete` value as `:dyn
    /// trait_path<pins...>`: the call targets for the trait's methods, in
    /// `TraitDef::method_order` (i.e. slot) order.
    ///
    /// This is also where a trait object is *admitted*: the concrete type
    /// must implement the trait, have a heap representation to box, and bind
    /// the associated types the pins claim. A generic owner's methods are
    /// requested for specialization here (`request_method_specialization`),
    /// so the name recorded in the slot is the monomorphized one — the
    /// "generics are specialized before code generation" rule, applied to
    /// dynamic dispatch.
    fn dyn_vtable_slots(
        &self,
        concrete: &Type,
        trait_path: &Path,
        pins: &[Type],
    ) -> Result<Vec<(Path, String)>, Error> {
        self.dyn_assoc_subst(trait_path, pins)?;
        let tdef = self.check_object_safe(trait_path)?;
        let (type_fq, targs) = match concrete {
            Type::Named(n, args) => (n.clone(), args.clone()),
            other => match prim_type_path(other) {
                Some(p) => (p, Vec::new()),
                None => {
                    return Err(Error::TypeError(format!(
                        "`{}` cannot be used as `:dyn {}`: it is not a nominal type",
                        mangle_type(concrete),
                        trait_path
                    )))
                }
            },
        };
        let Some(def) = self.reg.type_def(&type_fq) else {
            return Err(Error::TypeError(format!("dyn {}: unknown type `{}`", trait_path, type_fq)));
        };
        if !def.impls.contains(trait_path) {
            return Err(Error::TypeError(format!(
                "`{}` does not implement `{}`, so it cannot be used as `:dyn {}`",
                type_fq, trait_path, trait_path
            )));
        }
        // The fat box holds one `Value`, so the concrete value must have a
        // heap representation. This excludes the primitives even though they
        // can carry `impl`s (`impl Eq i32`) — hence a per-*type* rule rather
        // than a per-trait one: `:dyn Eq` is fine for a `defstruct`.
        if !self.is_heap_repr(concrete) {
            return Err(Error::TypeError(format!(
                "`{}` has no heap representation (its values are not `Sexpr`-encodable), so it cannot be boxed as `:dyn {}`",
                mangle_type(concrete),
                trait_path
            )));
        }
        // What this `impl` actually binds each associated type to, with the
        // owner's own type parameters resolved for *this* instantiation.
        for (name, pin) in tdef.assoc_types.iter().zip(pins.iter()) {
            match resolve_trait_assoc_type(def, trait_path, name, &targs) {
                Some(actual) if actual == *pin => {}
                Some(actual) => {
                    return Err(Error::TypeError(format!(
                        "`{}` implements `{}` with `{} = {}`, but `:dyn {}` requires `{} = {}`",
                        type_fq,
                        trait_path,
                        name,
                        mangle_type(&actual),
                        trait_path,
                        name,
                        mangle_type(pin)
                    )))
                }
                None => {
                    return Err(Error::TypeError(format!(
                        "`{}`'s `impl {}` does not bind the associated type `{}`",
                        type_fq, trait_path, name
                    )))
                }
            }
        }
        let subst: HashMap<String, Type> = def.params.iter().cloned().zip(targs.iter().cloned()).collect();
        let mut slots = Vec::with_capacity(tdef.method_order.len());
        for m in &tdef.method_order {
            if !def.assoc.contains_key(m) {
                return Err(Error::TypeError(format!(
                    "`{}`'s `impl {}` is missing method `{}`",
                    type_fq, trait_path, m
                )));
            }
            // Monomorphization, trait-object side: a generic owner's method
            // body is only code-generated once specialized, so the slot must
            // name the specialization — the same rewrite `check_assoc_call`
            // does for a static call.
            let mut name = m.clone();
            if !def.params.is_empty()
                && self.generic_method_templates.contains_key(&(type_fq.clone(), m.clone()))
            {
                if targs.len() != def.params.len() || targs.iter().any(|t| self.type_is_open(t)) {
                    return Err(Error::TypeError(format!(
                        "`{}` is not fully concrete, so `:dyn {}`'s vtable cannot be laid out yet",
                        mangle_type(concrete),
                        trait_path
                    )));
                }
                let args: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
                name = self.request_method_specialization(&type_fq, m, args);
            }
            slots.push((type_fq.clone(), name));
        }
        Ok(slots)
    }

    /// Build a proper list `Value` from `items`, in order — the inverse of
    /// `heap.list_to_vec`, used by `Checker::check_impl` to reassemble a
    /// receiver/parameter form after substituting just its type position.
    fn list_from_vec(&self, heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
        let mut out = Value::Empty;
        for item in items.iter().rev() {
            out = heap.cons(*item, out)?;
        }
        Ok(out)
    }

    /// Rewrite a read `Value` tree, replacing every bare symbol whose
    /// (case-folded) name is a key of `subst` with the corresponding
    /// replacement `Value` — `Checker::check_impl`'s `Self`/associated-type
    /// substitution. Leaves every other node (including non-symbol atoms and
    /// the list spine itself) alone; only used on already-read syntax, never
    /// on data, so there's no quoting concern.
    fn subst_value(heap: &mut Heap, v: Value, subst: &HashMap<String, Value>) -> Result<Value, Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id).to_string();
                Ok(subst.get(&name).copied().unwrap_or(v))
            }
            Value::Cons(_) => {
                let car = heap.car(v)?;
                let cdr = heap.cdr(v)?;
                let new_car = Self::subst_value(heap, car, subst)?;
                let new_cdr = Self::subst_value(heap, cdr, subst)?;
                heap.cons(new_car, new_cdr)
            }
            other => Ok(other),
        }
    }

    // ---- module / defmethod / use -----------------------------------------

    fn check_module(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("module: (module path body...)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let path = self.enter_module(&segs);

        let mut body = Vec::new();
        let mut result = Ok(());
        for form in &parts[1..] {
            match self.check_form(heap, interp, *form) {
                Ok(tl) => body.push(tl),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.exit_module(segs.len());
        result?;
        Ok(TopLevel::Module { path, body })
    }

    /// Push `segs` onto the current namespace path and ensure the (possibly
    /// empty) module namespace exists, returning the resulting absolute
    /// [`Path`]. Public so a filesystem-aware driver (`crate::project`) can
    /// check a file's forms inside the module its path derives to — the
    /// file's whole content is implicitly wrapped in that module without
    /// synthesizing a `(module ...)` form around the read values. Every
    /// `enter_module` must be paired with an [`Self::exit_module`] of the
    /// same segment count, even on a check error in between (mirroring
    /// [`Self::check_module`], which pops before propagating its body's
    /// first error).
    pub fn enter_module(&mut self, segs: &[String]) -> Path {
        for s in segs {
            self.ns.push(s.clone());
        }
        self.reg.root.module_mut(&self.ns);
        Path::from_segments(self.ns.clone())
    }

    /// Pop `depth` segments pushed by [`Self::enter_module`].
    pub fn exit_module(&mut self, depth: usize) {
        for _ in 0..depth {
            self.ns.pop();
        }
    }

    /// [`Self::enter_module`], plus recording `segs` as the file currently
    /// being loaded's own module path — see [`Self::file_ns`]'s doc comment.
    /// `project.rs` calls this (never [`Self::enter_module`] directly) when
    /// wrapping a file's whole content in the module its location derives
    /// to; a nested `(module ...)` form inside that file still goes through
    /// plain [`Self::enter_module`] via [`Self::check_module`].
    pub fn enter_file_module(&mut self, segs: &[String]) -> Path {
        let path = self.enter_module(segs);
        self.file_ns.push(self.ns.clone());
        path
    }

    /// Pop `depth` segments pushed by [`Self::enter_file_module`], plus its
    /// `file_ns` frame.
    pub fn exit_file_module(&mut self, depth: usize) {
        self.exit_module(depth);
        self.file_ns.pop();
    }

    /// Temporarily clear the namespace context (`ns` and `file_ns`) so the
    /// loader can load a *dependency file* from inside another file's check
    /// loop without registering it under that file's module. Dependency
    /// loads normally run with a clean context (`project.rs` scans a file's
    /// `use`s before `enter_file_module`), but a `use` surfaced by macro
    /// expansion is only discovered mid-check — see
    /// `Loader::load_source_inner`. Pair with [`Self::resume_ns_context`].
    pub fn suspend_ns_context(&mut self) -> NsContext {
        NsContext {
            ns: std::mem::take(&mut self.ns),
            file_ns: std::mem::take(&mut self.file_ns),
        }
    }

    /// Restore the context saved by [`Self::suspend_ns_context`]. The
    /// in-between load must have left the context empty again (every
    /// `enter_file_module` paired with an `exit_file_module`, even on error
    /// — see `load_source_inner`).
    pub fn resume_ns_context(&mut self, saved: NsContext) {
        debug_assert!(
            self.ns.is_empty() && self.file_ns.is_empty(),
            "dependency load left an unbalanced namespace context"
        );
        self.ns = saved.ns;
        self.file_ns = saved.file_ns;
    }

    fn check_defmethod(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevel, Error> {
        let MethodSig { method, instance, self_name, self_name_loc, recv_ty, type_fq, params, param_locs, ret, bounds, body_start } =
            self.parse_defmethod_sig(heap, parts)?;

        // A method on a *generic* type whose receiver spells the owner's
        // type parameters out as bare type variables (`(self Option<U>)`) is
        // generic through its owner — retain a re-checkable template (see
        // `MethodTemplate::Form`) and mark the erased `TopLevel` so
        // `Interp::exec` skips it, exactly like a generic `defun`. Any other
        // receiver shape (concrete arguments, no arguments on a static
        // method) declares a method whose signature can't mention the
        // owner's parameters, so it stays on the ordinary path.
        let written_vars: Vec<String> = match &recv_ty {
            Type::Named(_, targs) if !targs.is_empty() => targs
                .iter()
                .filter_map(|t| match t {
                    Type::Named(n, a) if a.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() => {
                        Some(n.local().to_string())
                    }
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let owner_params = self.reg.type_def(&type_fq).map(|d| d.params.len()).unwrap_or(0);
        let is_generic_template = !written_vars.is_empty()
            && written_vars.len() == owner_params
            && matches!(&recv_ty, Type::Named(_, targs) if targs.len() == written_vars.len())
            && {
                let mut distinct = written_vars.clone();
                distinct.sort();
                distinct.dedup();
                distinct.len() == written_vars.len()
            };
        if is_generic_template {
            for &p in parts {
                heap.push_permanent_root(p);
            }
            self.generic_method_templates.insert(
                (type_fq.clone(), method.clone()),
                MethodTemplate::Form { parts: parts.to_vec(), ns: self.ns.clone(), written_vars: written_vars.clone() },
            );
        }

        // Register the signature before checking the body (self-recursion).
        let mut sig_params: Vec<Type> = Vec::new();
        if instance {
            sig_params.push(recv_ty.clone());
        }
        sig_params.extend(params.iter().map(|(_, t)| t.clone()));
        let sig = FnSig { type_params: vec![], params: sig_params, ret: ret.clone(), public, rest: None, builtin: false, bounds: bounds.clone() };
        self.check_redef("method", &method, self.reg.type_def(&type_fq).and_then(|d| d.assoc.get(&method)))?;
        if let Some(def) = self.reg.type_def_mut(&type_fq) {
            def.assoc.insert(method.clone(), AssocFn { sig, instance, builtin: false });
        }
        if let Some(loc) = def_loc {
            self.reg.def_locs.methods.insert((type_fq.clone(), method.clone()), loc);
        }

        let mut binds: Vec<(String, Type, Option<Loc>)> = Vec::new();
        if let Some(s) = &self_name {
            binds.push((s.clone(), recv_ty.clone(), self_name_loc));
        }
        binds.extend(params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)));
        // `with_bounds` mirrors `check_defun`'s body env: inside the body a
        // method call on a `where`-bounded type variable (e.g. `(equals
        // self::car other::car)` with `self::car : A` under `(where (Eq A))`)
        // resolves through `check_instance_method`'s bounds branch to a
        // diagnostics-only `Expr::TraitCall`, exactly as in generic `defun`
        // bodies.
        let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
        let body_locs = parts_locs.get(body_start..).unwrap_or(&[]);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], body_locs, Some(&ret))?;
        let type_params = if is_generic_template { written_vars } else { Vec::new() };
        Ok(TopLevel::Defmethod { type_name: type_fq, method, instance, self_name, params, ret, body, type_params, public })
    }

    /// Parses a `defmethod` form's name, receiver, parameters, and return
    /// type (`parts` = everything after the `defmethod` keyword) — shared by
    /// [`Self::check_defmethod`] and [`Self::specialize_method`], the latter
    /// re-parsing a retained [`MethodTemplate::Form`] with
    /// `type_var_bindings` in effect so the receiver/parameter/return
    /// annotations come back concrete.
    fn parse_defmethod_sig(&self, heap: &mut Heap, parts: &[Value]) -> Result<MethodSig, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError(
                "defmethod: (defmethod name (receiver params...) ret body...)".into(),
            ));
        }
        let method = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmethod: name must be a symbol".into())),
        };
        let sig_list = heap.list_to_vec(parts[1])?;
        if sig_list.is_empty() {
            return Err(Error::TypeError("defmethod: needs a receiver".into()));
        }
        // `(self T)` -> instance method; a bare type name -> static method.
        let (instance, self_name, self_name_loc, type_expr) = match sig_list[0] {
            Value::Cons(_) => {
                let recv_locs = heap.list_to_vec_locs(sig_list[0])?;
                let recv: Vec<Value> = recv_locs.iter().map(|(v, _)| *v).collect();
                if recv.len() != 2 {
                    return Err(Error::TypeError("defmethod: receiver must be (self Type)".into()));
                }
                let sname = match recv[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defmethod: receiver name must be a symbol".into())),
                };
                let sname_loc = recv_locs.first().and_then(|(_, l)| l.clone());
                (true, Some(sname), sname_loc, recv[1])
            }
            Value::Symbol(_) | Value::Path(_) => (false, None, None, sig_list[0]),
            _ => return Err(Error::TypeError("defmethod: receiver must be (self Type) or a type name".into())),
        };
        let recv_ty = self.canon(&parse_type(heap, type_expr)?);
        let type_fq = match &recv_ty {
            Type::Named(n, _) => n.clone(),
            other => match prim_type_path(other) {
                Some(p) => p,
                None => return Err(Error::TypeError("defmethod: receiver must be a data type".into())),
            },
        };
        if self.reg.type_def(&type_fq).is_none() {
            return Err(Error::TypeError(format!("defmethod: unknown type `{}`", type_fq)));
        }
        let (params, param_locs) = self.parse_param_pairs(heap, &sig_list[1..])?;
        let ret = self.canon(&parse_type(heap, parts[2])?);
        // Optional `(where ...)` clause after the return type — identical
        // peek to `parse_defun_sig`'s.
        let mut body_start = 3;
        let mut bounds: HashMap<String, Vec<TraitBound>> = HashMap::new();
        if let Some(form) = parts.get(3) {
            if self.is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        Ok(MethodSig { method, instance, self_name, self_name_loc, recv_ty, type_fq, params, param_locs, ret, bounds, body_start })
    }

    /// `(defstruct Name (field Type)...)` — or, generically,
    /// `(defstruct Name<T1,T2...> (field Type)...)`, the name position
    /// parsed exactly like `(defun name<T1,T2...> ...)`'s
    /// (`Checker::parse_defun_name`, reused as-is) — a mutable product type:
    /// a single constructor, deliberately named `"new"` rather than `Name`
    /// itself, so `Type::new` resolves through the existing `Type::ctor`
    /// branch of `check_path_call` with no new constructor-resolution logic,
    /// and never occupies a bare/flat name (the "don't pollute the
    /// namespace" design goal this whole redesign was scoped around).
    /// `check_construct` builds instances (reading `AdtKind::Struct` to
    /// choose `RtValue::Struct` over `Data`) — generic substitution there
    /// (and in `match`'s `check_ctor_pattern`) is already `AdtDef.params`-
    /// generic, shared with `Option`/`Result`/`HashTable`, so a generic
    /// `defstruct` needs no changes to either.
    ///
    /// Beyond registering the type, this synthesizes one getter and one
    /// setter `defmethod` per field: `f` reads `fields[i]` (`Expr::FieldGet`)
    /// and `set-f` writes it (`Expr::FieldSet`, `set-car`/`set-cdr`'s prefix
    /// convention, no `!`), so `(f instance)`/`(set-f instance v)` — and,
    /// through `Checker::check`'s `Value::Path` sugar and `Checker::check_setf`,
    /// `instance::f`/`(setf instance::f v)` — work immediately; no separate
    /// accessor-declaration step exists. `check_form`'s signature is
    /// `Result<TopLevel, Error>` (one node per top-level form), so the type
    /// registration and every accessor `TopLevel::Defmethod` are bundled into
    /// one `TopLevel::Module` — purely as a grouping device: `Interp::exec`'s
    /// `Module` arm just runs `body` in order and never reads `path`, so this
    /// carries none of an actual `(module ...)`'s namespace-nesting semantics.
    fn check_defstruct(&mut self, heap: &Heap, parts: &[Value], public: bool, def_loc: Option<Loc>) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defstruct: (defstruct name (field type)...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;

        self.check_redef("type", &name, self.cur_ns().types.get(&name))?;
        let type_fq = self.fq(&name);
        // Pre-register a stub under `type_fq` *before* parsing field types, so
        // a self-referential field (e.g. a linked-list-style `(next Vector<node>)`
        // inside `node`'s own definition) resolves its bare `node` reference
        // via `Self::canon`/`resolve_bare_type` to this struct's own
        // qualified path, instead of `resolve_bare_type` finding nothing yet
        // and `canon` falling back to an unqualified guess that could never
        // match the real registration below. Overwritten with the fully
        // parsed `AdtDef` once fields are known.
        self.reg.root.module_mut(&self.ns).add_type(AdtDef {
            name: type_fq.clone(),
            params: type_params.clone(),
            variants: Vec::new(),
            assoc: HashMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Struct,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: HashMap::new(),
        });

        let fields = self.parse_struct_fields(heap, &parts[1..])?;
        if fields.is_empty() {
            return Err(Error::TypeError("defstruct: needs at least one field".into()));
        }
        let field_names: Vec<String> = fields.iter().map(|(n, _, _)| n.clone()).collect();
        let field_types: Vec<Type> = fields.iter().map(|(_, t, _)| t.clone()).collect();
        for (i, n) in field_names.iter().enumerate() {
            if field_names[..i].contains(n) {
                return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", n)));
            }
        }

        let recv_targs: Vec<Type> = type_params.iter().map(|p| Type::Named(Path::root(p), Vec::new())).collect();
        let recv_ty = Type::Named(type_fq.clone(), recv_targs);

        let mut assoc = HashMap::new();
        let mut accessors = Vec::with_capacity(fields.len() * 2);
        for (i, (field_name, field_ty, field_public)) in fields.iter().enumerate() {
            let field_public = *field_public;
            let getter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone()],
                ret: field_ty.clone(),
                public: field_public,
                rest: None,
                builtin: false,
                bounds: HashMap::new(),
            };
            assoc.insert(field_name.clone(), AssocFn { sig: getter_sig, instance: true, builtin: false });
            let self_var = Typed { loc: None, expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
            accessors.push(TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: field_name.clone(),
                instance: true,
                self_name: Some("self".to_string()),
                params: Vec::new(),
                ret: field_ty.clone(),
                body: vec![Typed { loc: None, expr: Expr::FieldGet(Box::new(self_var), i), ty: field_ty.clone() }],
                type_params: type_params.clone(),
                public: field_public,
            });

            // Setter (`set-car`/`set-cdr`'s `set-` prefix, no `!` — see
            // `Checker::check_setf`, which calls this via `(setf p::x v)`).
            let setter_name = format!("set-{}", field_name);
            let setter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone(), field_ty.clone()],
                ret: Type::Unit,
                public: field_public,
                rest: None,
                builtin: false,
                bounds: HashMap::new(),
            };
            assoc.insert(setter_name.clone(), AssocFn { sig: setter_sig, instance: true, builtin: false });
            let self_var = Typed { loc: None, expr: Expr::Var("self".to_string()), ty: recv_ty.clone() };
            let value_var = Typed { loc: None, expr: Expr::Var("value".to_string()), ty: field_ty.clone() };
            accessors.push(TopLevel::Defmethod {
                type_name: type_fq.clone(),
                method: setter_name.clone(),
                instance: true,
                self_name: Some("self".to_string()),
                params: vec![("value".to_string(), field_ty.clone())],
                ret: Type::Unit,
                body: vec![Typed { loc: None,
                    expr: Expr::FieldSet(Box::new(self_var), i, Box::new(value_var)),
                    ty: Type::Unit,
                }],
                type_params: type_params.clone(),
                public: field_public,
            });

            // A generic defstruct's accessors mention the type parameters
            // through the field types, so — like every generic-owner method —
            // the erased pair above is diagnostics-only (`type_params`
            // non-empty, skipped by `Interp::exec`) and each concrete
            // instantiation is generated on demand from these templates.
            // There is no raw source form to re-check (the accessor ASTs are
            // synthesized right here), hence the dedicated `Getter`/`Setter`
            // template kinds — see `Checker::synthesize_accessor`.
            if !type_params.is_empty() {
                self.generic_method_templates
                    .insert((type_fq.clone(), field_name.clone()), MethodTemplate::Getter { index: i });
                self.generic_method_templates
                    .insert((type_fq.clone(), setter_name), MethodTemplate::Setter { index: i });
            }
        }

        let def = AdtDef {
            name: type_fq.clone(),
            params: type_params,
            variants: vec![Variant { name: "new".to_string(), fields: field_types }],
            assoc,
            public,
            builtin: false,
            kind: AdtKind::Struct,
            field_names,
            impls: Vec::new(),
            trait_assoc: HashMap::new(),
        };
        self.reg.root.module_mut(&self.ns).add_type(def);
        if let Some(loc) = def_loc {
            self.reg.def_locs.types.insert(type_fq.clone(), loc);
        }

        let mut body = vec![TopLevel::Defstruct { name: type_fq.clone() }];
        body.extend(accessors);
        Ok(TopLevel::Module { path: type_fq, body })
    }

    /// `(defenum Name (Variant Type...)...)` — or generically
    /// `(defenum Name<T1,T2...> ...)` — a user-defined sum type: a
    /// multi-variant `AdtKind::Sum` `AdtDef`, structurally identical to the
    /// built-in `Option`/`Result` (`registry::option_def`/`result_def`). Each
    /// variant is `(VariantName FieldType...)` (positional fields, no names)
    /// or a bare `VariantName` for a nullary variant. Constructors are reached
    /// qualified (`Name::Variant`) or bare after `(use Name)`, exactly like the
    /// built-ins — `register_ctors` is deliberately *not* called, so no bare
    /// name is claimed until an explicit `use`. `match`/`if-let`, exhaustiveness
    /// checking, generic instantiation, and the `RtValue::Data` runtime
    /// representation are all the shared sum-type machinery, unchanged (that is
    /// how `Option`/`Result` already work). Unlike `defstruct`, no field
    /// accessors/setters are synthesized: an enum value is immutable and its
    /// fields are positional, so there's nothing to run at exec time either —
    /// hence a bare `TopLevel::Defenum` rather than a `Module` bundle.
    fn check_defenum(&mut self, heap: &Heap, parts: &[Value], public: bool, def_loc: Option<Loc>) -> Result<TopLevel, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defenum: (defenum Name (Variant Type...)...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;

        self.check_redef("type", &name, self.cur_ns().types.get(&name))?;
        let type_fq = self.fq(&name);
        // Pre-register a stub under `type_fq` *before* parsing variant field
        // types, so a self-referential field (e.g. `(node i32 tree tree)`
        // inside `tree`'s own definition) resolves its bare `tree` reference
        // via `Self::canon`/`resolve_bare_type` to this enum's own qualified
        // path, instead of `resolve_bare_type` finding nothing yet and
        // `canon` falling back to an unqualified guess that could never
        // match the real registration below. Overwritten with the fully
        // parsed `AdtDef` once variants are known.
        self.reg.root.module_mut(&self.ns).add_type(AdtDef {
            name: type_fq.clone(),
            params: type_params.clone(),
            variants: Vec::new(),
            assoc: HashMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Sum,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: HashMap::new(),
        });

        let mut variants = Vec::new();
        for item in &parts[1..] {
            let (vname, field_vals): (String, Vec<Value>) = match *item {
                // A bare symbol is a nullary variant (e.g. `None`); the
                // parenthesized `(None)` form is equally accepted below.
                Value::Symbol(id) => (heap.symbol_name(id).to_string(), Vec::new()),
                Value::Cons(_) => {
                    let elems = heap.list_to_vec(*item)?;
                    let vname = match elems.first() {
                        Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                        _ => return Err(Error::TypeError("defenum: variant name must be a symbol".into())),
                    };
                    (vname, elems[1..].to_vec())
                }
                _ => {
                    return Err(Error::TypeError(
                        "defenum: variant must be (Name Type...) or a bare Name".into(),
                    ))
                }
            };
            let fields = field_vals
                .iter()
                .map(|v| Ok(self.canon(&parse_type(heap, *v)?)))
                .collect::<Result<Vec<Type>, Error>>()?;
            variants.push(Variant { name: vname, fields });
        }
        if variants.is_empty() {
            return Err(Error::TypeError("defenum: needs at least one variant".into()));
        }
        for (i, v) in variants.iter().enumerate() {
            if variants[..i].iter().any(|w| w.name == v.name) {
                return Err(Error::TypeError(format!("defenum: duplicate variant `{}`", v.name)));
            }
        }

        let def = AdtDef {
            name: type_fq.clone(),
            params: type_params.clone(),
            variants: variants.clone(),
            assoc: HashMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Sum,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: HashMap::new(),
        };
        self.reg.root.module_mut(&self.ns).add_type(def);
        if let Some(loc) = def_loc {
            self.reg.def_locs.types.insert(type_fq.clone(), loc);
        }
        Ok(TopLevel::Defenum { name: type_fq, params: type_params, variants })
    }

    /// `(load "path")` — records the flat-load request for the driver (see
    /// [`TopLevel::Load`]). The argument must be a string *literal* (the
    /// driver resolves it before any subsequent form is checked, so it can't
    /// depend on runtime values).
    fn check_load(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("load: (load \"path\")".into()));
        }
        match parts[0] {
            Value::Str(id) => Ok(TopLevel::Load { path: heap.string(id).to_string() }),
            _ => Err(Error::TypeError("load: expected a string-literal path".into())),
        }
    }

    fn check_use(&mut self, heap: &Heap, parts: &[Value]) -> Result<TopLevel, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("use: (use path)".into()));
        }
        let segs = self.path_to_segs(heap, parts[0])?;
        let bare = segs.last().cloned().unwrap();

        // Try: free function.
        if let Some(target) = self.resolve_fn_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            return Ok(TopLevel::Use { alias: self.fq(&bare), target });
        }
        // Try: type. Beyond the usual alias (so the bare name also resolves
        // in a type-annotation position, e.g. `(x Option)` — see
        // `resolve_type_name`), this snapshots the type's *current*
        // constructors and public static methods as bare names in the
        // current namespace — e.g. `(use option)` makes `some`/`none`
        // callable bare, exactly like `option::some`/`option::none` (see
        // `check_path_call`, which already treats a variant and a
        // non-instance `assoc` entry as the same kind of "static member").
        if let Some(target) = self.resolve_type_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            let def = self.reg.type_def(&target).expect("resolved type exists").clone();
            for (i, v) in def.variants.iter().enumerate() {
                let is_builtin = self
                    .cur_ns()
                    .ctors
                    .get(&v.name)
                    .and_then(|(owner, _)| self.reg.type_def(owner))
                    .map(|d| d.builtin)
                    .unwrap_or(false);
                if self.cur_ns().ctors.contains_key(&v.name) {
                    self.check_redef_outcome("constructor", &v.name, is_builtin)?;
                }
                self.reg.root.module_mut(&self.ns).ctors.insert(v.name.clone(), (target.clone(), i));
            }
            for (name, af) in def.assoc.iter() {
                if af.instance || !af.sig.public {
                    continue;
                }
                let is_builtin = self
                    .cur_ns()
                    .static_uses
                    .get(name)
                    .and_then(|(owner, m)| self.reg.type_def(owner).and_then(|d| d.assoc.get(m)))
                    .map(|af| af.builtin)
                    .unwrap_or(false);
                if self.cur_ns().static_uses.contains_key(name) {
                    self.check_redef_outcome("static method", name, is_builtin)?;
                }
                self.reg
                    .root
                    .module_mut(&self.ns)
                    .static_uses
                    .insert(name.clone(), (target.clone(), name.clone()));
            }
            return Ok(TopLevel::Use { alias: self.fq(&bare), target });
        }
        // Try: module alias — `(use std::math)` makes `math` a short name for `std::math`.
        if let Some((abs, _)) = self.find_module(&segs) {
            let target_path = Path::from_segments(abs.clone());
            self.reg.root.module_mut(&self.ns).mod_aliases.insert(bare.clone(), abs);
            return Ok(TopLevel::Use { alias: self.fq(&bare), target: target_path });
        }
        // Not a known function, type, or module. The path may name a module
        // in a source file that simply hasn't been loaded yet — surface a
        // structured error so a filesystem-aware driver (`crate::project`)
        // can map the segments to a `.typl` file, load it into this same
        // checker, and retry. Displays as "use: unresolved `...`" when no
        // driver handles it.
        Err(Error::ModuleNotLoaded(segs))
    }

    // ---- expressions ------------------------------------------------------

    /// Check `v` as an expression, optionally against an `expected` type.
    /// Thin wrapper over [`Self::check_at`] with no caller-supplied location
    /// hint — for a `v` that is a list form, its own recorded location
    /// (`heap.cons_loc`) is used; for a bare atom checked through here, no
    /// location is available (see [`Self::check_at`] for when one is).
    fn check(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        self.check_at(heap, interp, env, v, expected, None)
    }

    /// Check `v` as an expression, tagging the resulting node (and any error)
    /// with `v`'s source location.
    ///
    /// A list form carries its own recorded location (`heap.cons_loc`, the
    /// opening paren); a bare atom has none of its own (interned symbols are
    /// shared — see `check::locate`'s module doc comment), so `loc_hint` — the
    /// location the reader recorded for this element in its enclosing list
    /// (`heap.list_to_vec_locs`) — fills that gap. This is what lets a `Var`
    /// (local variable) reference carry the position the LSP's hover/goto-
    /// definition need. `cons_loc` takes precedence when both are present.
    ///
    /// Because checking recurses into sub-expressions, the *deepest* failing
    /// sub-expression tags its error first and, since [`Error::at`] keeps the
    /// innermost location, that precise spot is what the message reports.
    fn check_at(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
        loc_hint: Option<Loc>,
    ) -> Result<Typed, Error> {
        let loc = heap.cons_loc(v).or(loc_hint);
        match self.check_inner(heap, interp, env, v, expected) {
            // Record `v`'s location on the checked node so the interpreter can
            // report a *runtime* error there too. A recursive `check` on a
            // sub-form already tagged its own node, so only fill an empty slot.
            Ok(mut typed) => {
                if typed.loc.is_none() {
                    typed.loc = loc;
                }
                // A local-variable reference that resolved against an `Env`
                // binding with a recorded position: record the reference's own
                // position -> the binding's position in `local_refs`, for
                // `check::locate::definition_target`'s `Expr::Var` arm. Both
                // positions are only available together right here — the
                // reference's (`typed.loc`, just filled above) and the
                // binding's (`env.get_loc`) — so this is resolved once now
                // rather than searched again at query time.
                if let (Expr::Var(name), Some(ref_loc)) = (&typed.expr, &typed.loc) {
                    if let Some(bind_loc) = env.get_loc(name) {
                        self.local_refs.borrow_mut().insert((ref_loc.line, ref_loc.col), bind_loc);
                    }
                }
                Ok(typed)
            }
            Err(e) => match loc {
                Some(loc) => Err(e.at(loc)),
                None => Err(e),
            },
        }
    }

    fn check_inner(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let typed = match v {
            Value::Int(n) => Typed { loc: None, expr: Expr::Int(n), ty: int_lit_ty(expected) },
            // A `Value::Boxed` read-literal is `Sexpr::Float`, `bignum` (an
            // integer literal past `i64`'s range), or `ratio` (`n/d` syntax)
            // — all heap-boxed for the same reason (`BoxedObj`'s doc
            // comment). Each is extracted into an owned, heap-independent
            // `Expr` payload exactly like `Float` already is; `bignum`/
            // `ratio` have no `expected`-driven width family the way an
            // integer/float literal does, so their type is always the one
            // primitive `Type::Bignum`/`Type::Ratio`.
            Value::Boxed(id) if heap.is_bignum(id) => {
                Typed { loc: None, expr: Expr::Bignum(heap.bignum_value(id).clone()), ty: Type::Bignum }
            }
            Value::Boxed(id) if heap.is_ratio(id) => {
                Typed { loc: None, expr: Expr::Ratio(heap.ratio_value(id).clone()), ty: Type::Ratio }
            }
            Value::Boxed(id) => Typed { loc: None, expr: Expr::Float(heap.float_value(id)), ty: float_lit_ty(expected) },
            Value::Bool(b) => Typed { loc: None, expr: Expr::Bool(b), ty: Type::Bool },
            Value::Char(c) => Typed { loc: None, expr: Expr::Char(c), ty: Type::Char },
            Value::Str(s) => Typed { loc: None, expr: Expr::Str(heap.string(s).to_string()), ty: Type::Str },
            Value::Empty => {
                // The empty list `()` is the `None` value of `Option<T>` when an
                // option type is expected, the `Nil` value of `Sexpr` when a
                // `Sexpr` is expected, and otherwise the unit value.
                if let Some(Type::Named(n, _)) = expected {
                    if *n == Path::root("option") {
                        // `Option`'s constructors moved to `Option::`/`use
                        // option` (no more bare `none`), but `()` denoting
                        // `None` is a language-core inference rule, not a
                        // name-visibility one — it must keep working
                        // regardless of `use` status, so this bypasses
                        // `resolve_ctor` and goes straight to the known
                        // builtin path/variant index.
                        return self.check_construct(heap, interp, env, (&Path::root("option"), 1), &[], &[], expected);
                    }
                    // `nil` stays a bare-resolvable name (`Sexpr` is exempt
                    // from the use-gated constructor visibility rule), so
                    // this one keeps going through `resolve_ctor` unchanged.
                    if let Some((adt, idx)) = self.resolve_ctor("nil") {
                        if *n == adt {
                            return self.check_construct(heap, interp, env, (&adt, idx), &[], &[], expected);
                        }
                    }
                }
                Typed { loc: None, expr: Expr::Unit, ty: Type::Unit }
            }
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                // A keyword (`:name`) is self-evaluating (CL), so it is
                // matched *before* every binding lookup below — it can never
                // name a variable, global, function or method. `::foo` is the
                // absolute-path syntax, not a keyword, and never reaches here
                // as a `Value::Symbol` (the reader makes it a `Value::Path`);
                // the `!starts_with("::")` guard keeps that distinction
                // explicit alongside `read::reader::validate_keyword`'s.
                if name.starts_with(':') && !name.starts_with("::") {
                    Typed { loc: None, expr: Expr::SymLit(name.to_string()), ty: Type::Symbol }
                } else if let Some(t) = env.get(name) {
                    Typed { loc: None, expr: Expr::Var(name.to_string()), ty: t.clone() }
                } else if let Some((path, vi)) = self.resolve_global(name) {
                    Typed { loc: None, expr: Expr::Global(self.mk_ref(vec![name.to_string()], path)), ty: vi.ty }
                } else if let Some(t) = self.fn_value(name, expected) {
                    t?
                } else if let Some(t) = self.method_value(name, expected) {
                    t
                } else {
                    return Err(Error::TypeError(format!("unbound variable: {}", name)));
                }
            }
            Value::Cons(_) => self.check_list(heap, interp, env, v, expected)?,
            Value::Path(pid) => {
                // A bare `::` path as an expression: a qualified global or a
                // qualified function used as a value.
                let segs: Vec<String> = heap
                    .path_segments(pid)
                    .iter()
                    .map(|s| heap.symbol_name(*s).to_string())
                    .collect();
                // `var::field` — sugar for `(field var)`, a field accessor's
                // ordinary instance-method call (`Checker::check_defstruct`
                // synthesizes one getter `defmethod` per field). Tried first:
                // `segs[0]` here names a *value* (a bound local/global), not
                // a module/type segment the way `resolve_global_path`/
                // `fn_path_value` below expect.
                if let Some(result) = self.try_field_access(heap, interp, env, &segs) {
                    result?
                } else if let Some((path, vi)) = self.resolve_global_path(&segs) {
                    Typed { loc: None, expr: Expr::Global(self.mk_ref(segs.clone(), path)), ty: vi.ty }
                } else if let Some(t) = self.fn_path_value(&segs, expected) {
                    t?
                } else {
                    return Err(Error::TypeError(format!("unresolved path: {}", segs.join("::"))));
                }
            }
        };
        // Reconcile synthesized types against the expectation. Literal and
        // constructor nodes already adopted `expected`, so this only fires on a
        // genuine mismatch. `Never` (a diverging expression) satisfies any
        // expected type.
        if let Some(e) = expected {
            if typed.ty != Type::Never && &typed.ty != e {
                // A `Symbol` is a valid `Sexpr` datum wherever a `Sexpr` is
                // expected (a `gensym`'d temp flowing into a `list`/`cons`/
                // quasiquote code position). Its runtime representation is
                // already exactly its `Sexpr::Sym` — `RtValue::Sexpr(Value::
                // Symbol)`, see `construct_sexpr`'s SEXPR_SYM arm — so widening
                // the static type needs no runtime work. Emit a transparent
                // *retype*, not a `Sexpr::Sym` constructor node: the
                // interpreter treated that constructor as a no-op, but the
                // compiler lowers a variant-5 construct as
                // `compile-construct-sym`, which interns a *name string*.
                // Feeding it an already-built `Symbol` value (a `gensym`'d temp
                // captured by a now-compiled macro-expansion lambda) aborts in
                // `rt_intern_symbol`. A bare retype is correct for both tiers.
                if *e == sexpr_ty() && typed.ty == Type::Symbol {
                    return Ok(Typed { loc: typed.loc, expr: typed.expr, ty: sexpr_ty() });
                }
                // A user ADT instance (`defstruct`/`defenum`, `Vector<T>`,
                // `HashTable<K,V>`, `cons-cell<K,V>`) is a valid `Sexpr` datum
                // wherever a `Sexpr` is expected — the CL-conformant "cons
                // cells hold arbitrary objects" behavior the pretty-printer
                // design discussion (TODO T5) decided this codebase should
                // have. Exactly like the `Symbol` case just above, its
                // runtime representation needs no conversion: every
                // `is_heap_repr` type's instantiation already evaluates to
                // `RtValue::Sexpr(Value::Boxed(_))` (`Expr::Construct`'s
                // mutable/enum arms in `Interp::eval`; `rt_struct_new`/
                // `rt_data_new` in compiled code — see `is_heap_repr`'s doc
                // comment for the two-tier "tagged Sexpr" unification this
                // relies on). A native-repr instantiation (e.g. `Option<llvm-
                // value>`) is excluded by `is_heap_repr` itself and stays a
                // type error, since it has no `Sexpr` encoding at all.
                // A trait object reaching a `Sexpr` expectation is *unwrapped*
                // rather than retyped: the fat box is a dispatch mechanism,
                // and `Sexpr` data has no static trait to dispatch on (you
                // get at it by `match`ing down to a concrete type, which
                // works on the wrapped value directly). Keeping the box would
                // put a second representation of every struct into `Sexpr`
                // data that every runtime type test would then have to know
                // about. Checked before the general `is_heap_repr` retype
                // below, which would otherwise claim it.
                if *e == sexpr_ty() && matches!(typed.ty, Type::Dyn(..)) {
                    return Ok(Typed { loc: typed.loc.clone(), expr: Expr::DynValue(Box::new(typed)), ty: sexpr_ty() });
                }
                if *e == sexpr_ty() && self.is_heap_repr(&typed.ty) {
                    return Ok(Typed { loc: typed.loc, expr: typed.expr, ty: sexpr_ty() });
                }
                // A scalar (`i32`/`f64`/`bignum`/`ratio`/`char`/`bool`/`Str`)
                // has no shared runtime shape with `Sexpr`, so — unlike the
                // two retypes above — this one inserts a real `Sexpr`
                // constructor call (`sexpr_ctor_for`), the same wrap
                // `wrap_rest_elem` applies to `&rest`/`format` elements. This
                // is what lets `(list 1 2)` / `(list p 42)` mix freely, the
                // CL-conformant behavior the same design discussion settled
                // on. `Symbol` is excluded here (handled by the retype
                // above, not a wrap) even though `sexpr_ctor_for` also
                // covers it, since that branch already returned.
                // A concrete value reaching a `:dyn Trait` expectation is
                // boxed as a trait object. Placed alongside the `Sexpr`
                // widenings above because it is the same kind of rule — an
                // implicit widening driven purely by the expectation — and
                // so it inherits exactly their reach: wherever an expected
                // type flows (arguments, fields, `defvar` initializers,
                // return position, an annotated `let`), never a bare
                // `(let ((x obj)) ...)`, which has no expectation at all.
                if let Type::Dyn(trait_path, pins) = e {
                    return self.coerce_to_dyn(typed, trait_path, pins);
                }
                if *e == sexpr_ty() {
                    if let Some(ctor) = sexpr_ctor_for(&typed.ty) {
                        let (type_name, variant) =
                            self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
                        let loc = typed.loc.clone();
                        return Ok(Typed {
                            loc,
                            expr: Expr::Construct { type_name, variant, args: vec![typed], mutable: false },
                            ty: sexpr_ty(),
                        });
                    }
                }
                return Err(Error::TypeError(format!(
                    "type mismatch: expected {:?}, found {:?}",
                    e, typed.ty
                )));
            }
        }
        Ok(typed)
    }

    /// Dispatch a compound form on its head symbol.
    fn check_list(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let elems = heap.list_to_vec(v)?;
        // Per-element source locations parallel to `elems` (see
        // `Heap::list_to_vec_locs`), so each argument checked below can carry
        // its own `Loc` — the position a bare-atom argument (e.g. a local
        // variable reference) needs for the LSP's hover/goto-definition.
        let elem_locs: Vec<Option<Loc>> = heap.list_to_vec_locs(v)?.into_iter().map(|(_, l)| l).collect();
        let args = &elems[1..];
        let arg_locs: &[Option<Loc>] = &elem_locs[1..];

        // A `::`-path head is a module-qualified function or a `Type::method`
        // static associated function.
        if let Value::Path(pid) = elems[0] {
            let segs: Vec<String> = heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect();
            return self.check_path_call(heap, interp, env, &segs, args, arg_locs, expected);
        }

        // A non-symbol head (e.g. a `lambda` literal or any expression) is
        // evaluated and applied as a function value.
        let head = match elems[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => {
                let callee = self.check_at(heap, interp, env, elems[0], None, nth_loc(&elem_locs, 0))?;
                return self.check_apply(heap, interp, env, callee, args, arg_locs);
            }
        };
        match head.as_str() {
            // The reader joins `:dyn Trait` into the list `(:dyn Trait)`
            // wherever it appears, so a `:dyn` written outside a type
            // position lands here as a call. Reject it by name rather than
            // letting it fall through to "unknown function `:dyn`" — the
            // point of spelling trait objects with a reserved keyword is
            // that a misplaced one is diagnosed as such (and that an editor
            // can highlight it unambiguously).
            ":dyn" => {
                return Err(Error::TypeError(
                    "`:dyn` may only appear in a type position (a parameter/field/return type, or a type argument)"
                        .to_string(),
                ))
            }
            "if" => return self.check_if(heap, interp, env, args, arg_locs, expected),
            "let" => return self.check_let(heap, interp, env, args, arg_locs, expected),
            "let*" => return self.check_let_star(heap, interp, env, args, arg_locs, expected),
            "progn" => {
                let (body, ty) = self.check_seq(heap, interp, env, args, arg_locs, expected)?;
                // Represent progn as a let with no bindings.
                return Ok(Typed { loc: None, expr: Expr::Let(Vec::new(), body), ty });
            }
            "setf" => return self.check_setf(heap, interp, env, args, arg_locs),
            "loop" => return self.check_loop(heap, interp, env, args, arg_locs),
            "break" => return self.check_break(args),
            "return" => return self.check_return(heap, interp, env, args, arg_locs),
            "list" => return self.check_list_lit(heap, interp, env, args, arg_locs),
            "format" => return self.check_format(heap, interp, env, args, arg_locs),
            "print" => return self.check_print_like(heap, interp, env, "print-rt", Type::Unit, args, arg_locs),
            "println" => return self.check_print_like(heap, interp, env, "println-rt", Type::Unit, args, arg_locs),
            "lambda" => return self.check_lambda(heap, interp, env, args, arg_locs),
            "labels" => return self.check_labels(heap, interp, env, args, arg_locs, expected),
            "apply" => return self.check_apply_form(heap, interp, env, args, arg_locs),
            "match" => return self.check_match(heap, interp, env, args, arg_locs, expected),
            "panic" => return self.check_panic(heap, interp, env, args, arg_locs),
            "the" => return self.check_the(heap, interp, env, args, arg_locs),
            "as" => return self.check_as(heap, interp, env, args, arg_locs, false),
            "try-as" => return self.check_as(heap, interp, env, args, arg_locs, true),
            "compile" => return self.check_compile(heap, args),
            "quote" => return self.check_quote(heap, args),
            "quasiquote" => return self.check_quasiquote(heap, interp, env, args),
            // Top-level-only forms reaching expression position (e.g. a
            // macro expanding to `(use ...)` inside a function body) get a
            // clear error instead of the misleading "unbound variable" the
            // fallthrough resolution below would produce.
            "use" | "module" => {
                return Err(Error::TypeError(format!(
                    "{}: only allowed at top level, not in expression position",
                    head
                )))
            }
            _ => {}
        }
        // A local variable holding a function value is applied directly (locals
        // shadow free functions). Goes through `check_at` (not a hand-built
        // `Typed`) so this callee reference gets its own position — the same
        // treatment the non-symbol-head branch above already gives its
        // callee — which is what lets e.g. a `labels`-bound function called
        // by name resolve goto-definition back to its own binding.
        if env.get(&head).is_some() {
            let callee = self.check_at(heap, interp, env, elems[0], None, nth_loc(&elem_locs, 0))?;
            return self.check_apply(heap, interp, env, callee, args, arg_locs);
        }
        // A macro call: expand (against the *unevaluated* argument forms,
        // exactly as written — see `MacroExpander`) and recursively check the
        // expansion in place, in the use site's lexical `env`. This is what
        // makes macros unhygienic/CL-style. Checked after special forms and
        // local-variable-as-callee (matching how a constructor/free-function
        // call is resolved below), since a macro is a purely compile-time
        // name with no runtime value to shadow or be shadowed by.
        if let Some((macro_path, shape)) = self.resolve_macro(&head) {
            Self::check_macro_arity(&head, shape, args.len())?;
            let expanded = interp
                .expand_macro(heap, &macro_path, args.to_vec())
                .map_err(|e| Error::TypeError(format!("macro `{}`: {}", head, e)))?;
            heap.push_root(expanded);
            let result = self.check(heap, interp, env, expanded, expected);
            heap.pop_root();
            return result;
        }
        // Constructor, then a receiver-typed instance method on the first
        // argument's type, then the free function, then a global function
        // value, then instance-method dispatch again (for the error message
        // when nothing at all matches). The instance method is tried *before*
        // the free function — CLOS precedent: a type-specific method takes
        // precedence over a same-named ordinary function, which only serves
        // as the generic function's implicit default when no type-specific
        // method matches the call's argument type (see `try_instance_method`'s
        // doc comment). This lets e.g. `HashTable<K,V>`'s `remove`/`count` and
        // the `Sexpr`-list prelude's free-function `remove`/`count` share a
        // name without either having to be renamed.
        if let Some((adt, idx)) = self.resolve_ctor(&head) {
            self.check_construct(heap, interp, env, (&adt, idx), args, arg_locs, expected)
        } else if let Some((type_fq, method)) = self.resolve_static_use(&head) {
            self.check_assoc_call(
                heap,
                interp,
                env,
                AssocCall { type_fq: &type_fq, method: &method, receiver: None, expected },
                args,
                arg_locs,
            )
        } else if let Some(result) = self.try_instance_method(heap, interp, env, &head, args, arg_locs) {
            result
        } else if let Some(fq) = self.resolve_fn(&head) {
            self.check_call(heap, interp, env, std::slice::from_ref(&head), &fq, args, arg_locs)
        } else if let Some((path, vi)) = self.resolve_global(&head) {
            let callee = Typed { loc: None, expr: Expr::Global(self.mk_ref(vec![head.clone()], path)), ty: vi.ty };
            self.check_apply(heap, interp, env, callee, args, arg_locs)
        } else {
            self.check_instance_method(heap, interp, env, &head, args, arg_locs)
        }
    }

    /// Type-check a `lambda`: `(lambda (params) ret body...)`. The body sees the
    /// enclosing locals (a closure) plus the parameters.
    fn check_lambda(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError("lambda: (lambda (params) ret body...)".into()));
        }
        let (params, param_locs, rest) = self.parse_params_rest(heap, args[0])?;
        let ret = self.canon(&parse_type(heap, args[1])?);
        let fn_ty = Type::Fn(
            params.iter().map(|(_, t)| t.clone()).collect(),
            rest.as_ref().map(|(_, t, _)| Box::new(t.clone())),
            Box::new(ret.clone()),
        );
        // The body additionally sees the `&rest` parameter (if any) bound to
        // a plain `Sexpr` list — see `Self::check_defun`'s identical treatment.
        let mut params = params;
        let mut param_locs = param_locs;
        if let Some((rname, _, rloc)) = rest {
            params.push((rname, sexpr_ty()));
            param_locs.push(rloc);
        }
        let binds: Vec<(String, Type, Option<Loc>)> =
            params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)).collect();
        let child = env.extended_with_locs(binds);
        // A lambda is a new function boundary: `break`/`return` cannot reach an
        // outer loop through it, so it checks its body against an empty loop
        // stack (restored afterwards, even on error).
        let saved = self.loop_stack.replace(Vec::new());
        let result = self.check_seq(heap, interp, &child, &args[2..], &arg_locs[2..], Some(&ret));
        self.loop_stack.replace(saved);
        let (body, _) = result?;
        Ok(Typed { loc: None, expr: Expr::Lambda { params, body }, ty: fn_ty })
    }

    /// `(labels ((name (params) ret body...)...) body...)`: like several
    /// `lambda`s, except each one — and the trailing `body` — can call any
    /// of them by name, including itself (CL's `labels`; addresses the
    /// catalog's "no self-referencing local function" gap, since a bare
    /// `lambda` only sees its *enclosing* scope, not a `let`-bound name for
    /// itself). Every function's signature is registered as a local
    /// variable of `Type::Fn` *before* any body is checked (mirroring
    /// `check_defun`'s self-recursion registration), so all of them —
    /// including the trailing `body` — see each other regardless of
    /// definition order.
    fn check_labels(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError(
                "labels: (labels ((name (params) ret body...)...) body...)".into(),
            ));
        }
        let specs = heap.list_to_vec(args[0])?;
        struct Spec {
            name: String,
            params: Vec<(String, Type)>,
            param_locs: Vec<Option<Loc>>,
            ret: Type,
            raw_body: Vec<Value>,
            /// Source locations of `raw_body`'s forms (parallel), so each
            /// body form checked below carries its own `Loc`.
            body_locs: Vec<Option<Loc>>,
        }
        let mut parsed = Vec::new();
        let mut sigs: Vec<(String, Type, Option<Loc>)> = Vec::new();
        for spec in specs {
            // `list_to_vec_locs` so a bare-atom body form (e.g. a local
            // variable reference in the function's body) keeps its position,
            // and so the function name's own position (`parts_locs[0]`) is
            // available for its `Env` binding below.
            let parts_locs = heap.list_to_vec_locs(spec)?;
            let parts: Vec<Value> = parts_locs.iter().map(|(v, _)| *v).collect();
            if parts.len() < 3 {
                return Err(Error::TypeError(
                    "labels: binding must be (name (params) ret body...)".into(),
                ));
            }
            let name = match parts[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("labels: name must be a symbol".into())),
            };
            let name_loc = parts_locs.first().and_then(|(_, l)| l.clone());
            let (params, param_locs) = self.parse_params(heap, parts[1])?;
            let ret = self.canon(&parse_type(heap, parts[2])?);
            let fn_ty = Type::Fn(params.iter().map(|(_, t)| t.clone()).collect(), None, Box::new(ret.clone()));
            sigs.push((name.clone(), fn_ty, name_loc));
            let body_locs = parts_locs[3..].iter().map(|(_, l)| l.clone()).collect();
            parsed.push(Spec { name, params, param_locs, ret, raw_body: parts[3..].to_vec(), body_locs });
        }
        // Every function's name is visible to every body (including its
        // own) and to the trailing `body` — registered up front, like
        // `check_defun`'s pre-body signature insert.
        let labels_env = env.extended_with_locs(sigs);

        let mut defs = Vec::new();
        for Spec { name, params, param_locs, ret, raw_body, body_locs } in parsed {
            let binds: Vec<(String, Type, Option<Loc>)> =
                params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)).collect();
            let fn_env = labels_env.extended_with_locs(binds);
            // A new function boundary, same as `lambda`: `break`/`return`
            // can't reach an outer loop through it.
            let saved = self.loop_stack.replace(Vec::new());
            let result = self.check_seq(heap, interp, &fn_env, &raw_body, &body_locs, Some(&ret));
            self.loop_stack.replace(saved);
            let (body, _) = result?;
            defs.push((name, params, body));
        }
        let (body, ty) = self.check_seq(heap, interp, &labels_env, &args[1..], &arg_locs[1..], expected)?;
        Ok(Typed { loc: None, expr: Expr::Labels { defs, body }, ty })
    }

    /// Type-check applying a function *value* `callee` to `args`. If
    /// `callee`'s type is variadic (`Type::Fn`'s `rest` is `Some`), every
    /// argument past the fixed parameters is checked against the rest
    /// element type and collected into a single `Sexpr` list actual
    /// argument — the value-level counterpart of `Self::check_call`'s
    /// desugaring for a named function.
    fn check_apply(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        callee: Typed,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let (params, rest, ret) = match &callee.ty {
            Type::Fn(p, r, ret) => (p.clone(), r.clone(), (**ret).clone()),
            other => return Err(Error::TypeError(format!("value is not callable: {:?}", other))),
        };
        let fixed = params.len();
        match &rest {
            None if args.len() != fixed => {
                return Err(Error::TypeError(format!(
                    "function expects {} argument(s), got {}",
                    fixed,
                    args.len()
                )));
            }
            Some(_) if args.len() < fixed => {
                return Err(Error::TypeError(format!(
                    "function expects at least {} argument(s), got {}",
                    fixed,
                    args.len()
                )));
            }
            _ => {}
        }
        let mut typed = Vec::new();
        for (i, (arg, pty)) in args[..fixed].iter().zip(params.iter()).enumerate() {
            typed.push(self.check_at(heap, interp, env, *arg, Some(pty), nth_loc(arg_locs, i))?);
        }
        if let Some(elem_ty) = &rest {
            let mut rest_typed = Vec::new();
            for (i, arg) in args[fixed..].iter().enumerate() {
                rest_typed.push(self.check_at(heap, interp, env, *arg, Some(elem_ty), nth_loc(arg_locs, fixed + i))?);
            }
            typed.push(self.cons_rest_list(elem_ty, rest_typed)?);
        }
        Ok(Typed { loc: None, expr: Expr::Apply(Box::new(callee), typed), ty: ret })
    }

    /// `(apply f arg1 ... argN rest-list)`: call the *variadic* function
    /// value `f` — its type must be `(fn (T1..Tn) &rest Te) R)` — with
    /// `arg1..argN` bound to its fixed parameters and `rest-list` (already a
    /// plain `Sexpr` list, e.g. a `quote`d list or one built by `cons`)
    /// passed straight through as its `&rest` argument, instead of
    /// `Self::check_apply`'s usual one-by-one collection from individually-
    /// typed trailing expressions. Note `rest-list`'s elements are *not*
    /// checked against `Te` here — same as CL's `apply`, which never checks a
    /// list's contents either; the callee's own body is responsible for
    /// whatever it does with each element. Desugars to the very same
    /// `Expr::Apply` shape `check_apply` produces for `(f arg1 .. argN e1 e2
    /// e3)` if `rest-list` were three literal elements `e1 e2 e3` instead of
    /// one dynamic list — so the interpreter needs no `apply`-specific
    /// evaluation at all.
    fn check_apply_form(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError("apply: (apply function arg... rest-list)".into()));
        }
        let callee = self.check_at(heap, interp, env, args[0], None, nth_loc(arg_locs, 0))?;
        let (params, ret) = match &callee.ty {
            Type::Fn(p, Some(_), ret) => (p.clone(), (**ret).clone()),
            Type::Fn(_, None, _) => {
                return Err(Error::TypeError(
                    "apply: function has no `&rest` parameter to apply a list to".into(),
                ))
            }
            other => return Err(Error::TypeError(format!("apply: value is not callable: {:?}", other))),
        };
        let (fixed_args, list_arg) = args[1..].split_at(args.len() - 2);
        if fixed_args.len() != params.len() {
            return Err(Error::TypeError(format!(
                "apply: expected {} fixed argument(s) before the rest list, got {}",
                params.len(),
                fixed_args.len()
            )));
        }
        let mut typed = Vec::new();
        for (i, (arg, pty)) in fixed_args.iter().zip(params.iter()).enumerate() {
            typed.push(self.check_at(heap, interp, env, *arg, Some(pty), nth_loc(arg_locs, i + 1))?);
        }
        let list_loc = nth_loc(arg_locs, args.len() - 1);
        typed.push(self.check_at(heap, interp, env, list_arg[0], Some(&sexpr_ty()), list_loc)?);
        Ok(Typed { loc: None, expr: Expr::Apply(Box::new(callee), typed), ty: ret })
    }

    /// A `::`-qualified call: a module-qualified macro, free function, or
    /// `Type::method` static associated function.
    // Cohesive checker entry point: `heap`/`interp`/`env`/`arg_locs`/`expected`
    // are the invariant type-checking context threaded through every such
    // method. Bundling them into a struct would have to carry `&mut Heap`
    // alongside shared borrows and would ripple through the whole checker for
    // no readability gain, so the arg count stays as-is.
    #[allow(clippy::too_many_arguments)]
    fn check_path_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        // A module-qualified macro call, e.g. `mod::my-macro` — checked
        // first, mirroring the bare-name case in `check_list` (a macro is a
        // purely compile-time name, resolved before any runtime call shape;
        // `resolve_macro_path` is the `::`-qualified counterpart of the
        // bare-name `resolve_macro` that branch uses).
        if let Some((macro_path, shape)) = self.resolve_macro_path(segs) {
            let display = segs.join("::");
            Self::check_macro_arity(&display, shape, args.len())?;
            let expanded = interp
                .expand_macro(heap, &macro_path, args.to_vec())
                .map_err(|e| Error::TypeError(format!("macro `{}`: {}", display, e)))?;
            heap.push_root(expanded);
            let result = self.check(heap, interp, env, expanded, expected);
            heap.pop_root();
            return result;
        }
        // A module-qualified free function, e.g. `math::id`.
        if let Some(fq) = self.resolve_fn_path(segs) {
            return self.check_call(heap, interp, env, segs, &fq, args, arg_locs);
        }
        // Otherwise `Type::member`: split the last segment as the member and
        // resolve the prefix as a type. The member is a constructor or a static
        // associated function of that type.
        if segs.len() >= 2 {
            let (type_segs, member) = segs.split_at(segs.len() - 1);
            let member = &member[0];
            if let Some(type_fq) = self.resolve_type_path(type_segs) {
                let def = self.reg.type_def(&type_fq).expect("resolved type exists");
                if let Some(variant) = def.variants.iter().position(|v| &v.name == member) {
                    return self.check_construct(heap, interp, env, (&type_fq, variant), args, arg_locs, expected);
                }
                if let Some(af) = def.assoc.get(member) {
                    if self.assoc_visible(&type_fq, af) {
                        if af.instance {
                            return Err(Error::TypeError(format!(
                                "`{}::{}` is an instance method; call it as ({} obj ...)",
                                type_fq, member, member
                            )));
                        }
                        return self.check_assoc_call(
                            heap,
                            interp,
                            env,
                            AssocCall { type_fq: &type_fq, method: member.as_str(), receiver: None, expected },
                            args,
                            arg_locs,
                        );
                    }
                }
            }
        }
        Err(Error::TypeError(format!("unresolved path: {}", segs.join("::"))))
    }

    /// Dispatch a bare call `(m recv args...)` as an instance method on the
    /// static type of its first argument.
    /// Try resolving `(method args...)` as a call to an instance method on
    /// the first argument's type. Returns `None` — not an error — when no
    /// such method exists for that type, so `check_list` can fall back to a
    /// free function of the same name: this mirrors CLOS, where a
    /// type-specific method takes precedence over an ordinary function of
    /// the same name, and that ordinary function only acts as the generic
    /// function's default when no type-specific method matches the call's
    /// argument type (an existing `defun` becomes the default method when a
    /// same-named `defgeneric`/`defmethod` is introduced later).
    ///
    /// The first argument is checked once here with `expected: None`, purely
    /// to learn its type — if that doesn't resolve to a method, `check_list`'s
    /// free-function fallback re-checks it against the free function's real
    /// parameter type, so e.g. an integer literal still gets that parameter's
    /// exact width rather than the throwaway default this `check` would have
    /// given it. Any error checking the first argument (e.g. an unbound
    /// variable) also yields `None`: it isn't a real "no such method"
    /// failure, but the free-function fallback (or, if none exists either,
    /// the final `check_instance_method` call in `check_list`) re-checks the
    /// same argument and surfaces the real error there instead.
    fn try_instance_method(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Option<Result<Typed, Error>> {
        let mut recv = self.check_at(heap, interp, env, *args.first()?, None, nth_loc(arg_locs, 0)).ok()?;
        // A trait-object receiver dispatches its trait's own methods through
        // the vtable, and everything else through the built-in `Sexpr`
        // catalog — see `check_instance_method`'s fuller comment.
        if let Type::Dyn(trait_path, pins) = recv.ty.clone() {
            if self.reg.trait_def(&trait_path).is_some_and(|t| t.methods.contains_key(method)) {
                return Some(self.check_dyn_call(
                    heap, interp, env, recv, &trait_path, &pins, method, &args[1..], &arg_locs[1..],
                ));
            }
            if !self.sexpr_has_instance_method(method) {
                return None;
            }
            recv = Typed { loc: recv.loc.clone(), expr: Expr::DynValue(Box::new(recv)), ty: sexpr_ty() };
        }
        let type_fq = match &recv.ty {
            Type::Named(n, _) => Some(n.clone()),
            other => prim_type_path(other),
        }?;
        let def = self.reg.type_def(&type_fq)?;
        match def.assoc.get(method) {
            Some(af) if af.instance && self.assoc_visible(&type_fq, af) => {}
            _ => return None,
        }
        Some(self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method, receiver: Some(recv), expected: None },
            &args[1..],
            &arg_locs[1..],
        ))
    }

    /// The final fallback when nothing else matched `(method args...)` in
    /// `check_list`: re-derives the same instance-method lookup
    /// `try_instance_method` does, but — since there's no free function left
    /// to defer to — propagates a real error from checking the first
    /// argument instead of swallowing it, and reports `NoSuchFunction` if no
    /// matching method exists either.
    fn check_instance_method(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if !args.is_empty() {
            let mut recv = self.check_at(heap, interp, env, args[0], None, nth_loc(arg_locs, 0))?;
            // Trait-object receiver: one of the trait's own methods dispatches
            // through the vtable; anything else is the built-in `Sexpr`
            // catalog (`eq`/`equal`/`print`/...), which applies to a trait
            // object exactly as to the heap value it wraps — the same
            // transparency printing and comparison have. Unlike
            // `try_instance_method`'s peek this is the reporting path, so a
            // name that is neither gets `check_dyn_call`'s message naming
            // the trait rather than a bare "no such function".
            if let Type::Dyn(trait_path, pins) = recv.ty.clone() {
                if self.reg.trait_def(&trait_path).is_some_and(|t| t.methods.contains_key(method)) {
                    return self.check_dyn_call(
                        heap, interp, env, recv, &trait_path, &pins, method, &args[1..], &arg_locs[1..],
                    );
                }
                if !self.sexpr_has_instance_method(method) {
                    return self.check_dyn_call(
                        heap, interp, env, recv, &trait_path, &pins, method, &args[1..], &arg_locs[1..],
                    );
                }
                recv = Typed { loc: recv.loc.clone(), expr: Expr::DynValue(Box::new(recv)), ty: sexpr_ty() };
            }
            let type_fq = match &recv.ty {
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            if let Some(type_fq) = &type_fq {
                if let Some(def) = self.reg.type_def(type_fq) {
                    let visible = def
                        .assoc
                        .get(method)
                        .is_some_and(|af| af.instance && self.assoc_visible(type_fq, af));
                    if visible {
                        return self.check_assoc_call(
                            heap,
                            interp,
                            env,
                            AssocCall { type_fq, method, receiver: Some(recv), expected: None },
                            &args[1..],
                            &arg_locs[1..],
                        );
                    }
                }
                // No concrete `AdtDef` named `type_fq` — it may be one of
                // this function's own `where`-bounded type parameters
                // (`Env::bounds`, keyed by the lowercase type-variable name,
                // exactly what `type_fq.local()` is for a bare `Type::Named`
                // type variable like `t`). Search every trait it's bound to
                // for a matching method; see `Expr::TraitCall`'s doc comment
                // for why the implementing type is resolved at runtime
                // instead of here.
                if let Some(bound_traits) = env.bounds.get(type_fq.local()) {
                    for tb in bound_traits {
                        let Some(tdef) = self.reg.trait_def(&tb.trait_path) else { continue };
                        let Some(sig) = tdef.methods.get(method) else { continue };
                        let want = sig.params.len() - 1;
                        if args.len() - 1 != want {
                            return Err(Error::TypeError(format!(
                                "{}: expected {} argument(s), got {}",
                                method,
                                want,
                                args.len() - 1
                            )));
                        }
                        let mut typed_args = vec![recv];
                        for (i, a) in args[1..].iter().enumerate() {
                            typed_args.push(self.check_at(heap, interp, env, *a, None, nth_loc(&arg_locs[1..], i))?);
                        }
                        // This bound's `where`-clause associated-type pins
                        // (e.g. `(Item i32)`) resolve the trait method
                        // template's otherwise-opaque associated-type
                        // variables (e.g. `Option<Item>`) wherever pinned —
                        // a no-op (returns `sig.ret` unchanged) when
                        // `tb.assoc` is empty, exactly matching pre-pin
                        // behavior.
                        let ret_ty = subst_apply(&sig.ret, &tb.assoc);
                        return Ok(Typed { loc: None,
                            expr: Expr::TraitCall { method: method.to_string(), args: typed_args },
                            ty: ret_ty,
                        });
                    }
                }
            }
        }
        Err(Error::NoSuchFunction(method.to_string()))
    }

    /// Check a call to a type-associated function. For an instance method the
    /// already-checked `receiver` fills the first parameter slot. `call` bundles
    /// `(type path, method name)`, the receiver, and the call site's expected
    /// type into one parameter to keep the arity down.
    ///
    /// Generic owners (e.g. the built-in `HashTable<K,V>`): the method's
    /// signature template mentions `def.params` (`k`/`v`) as type variables,
    /// the same way a constructor's field templates do (see
    /// [`Self::check_construct`]). The concrete bindings come from wherever
    /// they're available — an instance call already knows them from the
    /// receiver's resolved type, `Type::Named(type_fq, concrete_args)`; a
    /// static call with no receiver (e.g. `HashTable::new`, which has no
    /// argument to infer `K`/`V` from either) instead seeds them from
    /// `expected`, exactly like a field-less constructor such as `None`
    /// learns its type argument from `expected`. If a parameter is bound by
    /// neither, it's left as a free type variable and surfaces as a normal
    /// "cannot infer" error, matching `check_construct`.
    fn check_assoc_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        call: AssocCall,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let AssocCall { type_fq, method, receiver, expected } = call;
        let instance = receiver.is_some();
        let def = self.reg.type_def(type_fq).expect("assoc type exists").clone();
        let af = def.assoc[method].clone();

        let mut subst: HashMap<String, Type> = HashMap::new();
        let concrete_args: Option<&[Type]> = match (&receiver, expected) {
            (Some(r), _) => match &r.ty {
                Type::Named(n, args) if n == type_fq => Some(args.as_slice()),
                _ => None,
            },
            (None, Some(Type::Named(n, args))) if n == type_fq => Some(args.as_slice()),
            (None, _) => None,
        };
        if let Some(args) = concrete_args {
            if args.len() == def.params.len() {
                for (p, a) in def.params.iter().zip(args.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
            }
        }

        let offset = if instance { 1 } else { 0 };
        let expected_params: Vec<Type> =
            af.sig.params[offset..].iter().map(|t| subst_apply(t, &subst)).collect();
        if args.len() != expected_params.len() {
            return Err(Error::TypeError(format!(
                "{}::{}: expected {} argument(s), got {}",
                type_fq,
                method,
                expected_params.len(),
                args.len()
            )));
        }
        let mut typed = Vec::new();
        if let Some(r) = receiver {
            typed.push(r);
        }
        for (i, (arg, pty)) in args.iter().zip(expected_params.iter()).enumerate() {
            typed.push(self.check_at(heap, interp, env, *arg, Some(pty), nth_loc(arg_locs, i))?);
        }
        for p in &def.params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type argument `{}` for `{}::{}`",
                    p, type_fq, method
                )));
            }
        }
        // Method-side `where`-bound validation — the mirror of
        // `check_call`'s. A bounded method's bounds are keyed by the owner's
        // own type parameters (that's all a method signature can mention),
        // which `subst` above already resolved from the receiver — so e.g.
        // `(equals p q)` on a `cons-cell<point,i32>` whose `point` lacks an
        // `Eq` impl is rejected here with a real "does not implement trait"
        // error instead of an opaque `NoSuchFunction` from deep inside the
        // specialization drain.
        if !af.sig.bounds.is_empty() {
            self.validate_where_bounds(&format!("{}::{}", type_fq, method), &af.sig.bounds, &subst, &env.bounds)?;
        }
        // Monomorphization, method side: a call on a generic owner whose
        // type arguments are fully concrete is rewritten to the specialized
        // method (generated by `check_form`'s drain) — the mirror of
        // `check_call`'s free-function rewrite. Only methods with a retained
        // template qualify: builtin generic methods (`Vector<T>::get`, ...)
        // are Rust implementations dispatched by their plain name, and stay
        // untouched.
        let mut method_name = method.to_string();
        if !def.params.is_empty()
            && self.generic_method_templates.contains_key(&(type_fq.clone(), method.to_string()))
        {
            let targs: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
            if !targs.iter().any(|t| self.type_is_open(t)) {
                method_name = self.request_method_specialization(type_fq, method, targs);
            }
        }
        Ok(Typed { loc: None,
            expr: Expr::Assoc {
                type_name: type_fq.clone(),
                method: method_name,
                instance,
                args: typed,
                home: self.ns.clone(),
            },
            ty: subst_apply(&af.sig.ret, &subst),
        })
    }

    fn check_if(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.len() != 3 {
            return Err(Error::TypeError("if: (if cond then else)".into()));
        }
        let cond = self.check_at(heap, interp, env, args[0], Some(&Type::Bool), nth_loc(arg_locs, 0))?;
        let then = self.check_at(heap, interp, env, args[1], expected, nth_loc(arg_locs, 1))?;
        // A diverging (`Never`) then branch must not constrain the else branch.
        let else_expected = non_never(&then.ty).or(expected);
        let els = self.check_at(heap, interp, env, args[2], else_expected, nth_loc(arg_locs, 2))?;
        let ty = join_types(&then.ty, &els.ty)?;
        Ok(Typed { loc: None, expr: Expr::If(Box::new(cond), Box::new(then), Box::new(els)), ty })
    }

    fn check_panic(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("panic: (panic message)".into()));
        }
        let msg = self.check_at(heap, interp, env, args[0], Some(&Type::Str), nth_loc(arg_locs, 0))?;
        Ok(Typed { loc: None, expr: Expr::Panic(Box::new(msg)), ty: Type::Never })
    }

    /// `(the Type expr)`: a type annotation, e.g. `(the i64 5)` to make an
    /// integer literal default to `i64` instead of `i32`, or to pin down a
    /// generic call's type argument the way an `expected` type elsewhere
    /// would. Purely a checking-time hint with no runtime behavior of its
    /// own — `Type` simply becomes `expr`'s `expected` (the same role it
    /// plays for a `defun` parameter or `let` binding annotation), and the
    /// returned `Typed` is exactly `expr`'s own (no new `Expr` variant; `the`
    /// vanishes after checking).
    fn check_the(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("the: (the Type expr)".into()));
        }
        let ty = self.canon(&parse_type(heap, args[0])?);
        self.check_at(heap, interp, env, args[1], Some(&ty), nth_loc(arg_locs, 1))
    }

    /// `(as Type expr)` / `(try-as Type expr)`: a Rust-`as`-flavored
    /// primitive cast between the numeric/`char` types — unrelated to
    /// `Sexpr` (that's `match`'s job — see the "Phase 5 方針転換" note in
    /// `docs/dev/symbol-sexpr-redesign.md`). `as` panics on a partial
    /// conversion's failure; `try-as` returns `Option<Type>` instead,
    /// `None` on failure. Deliberately closed over the pairs in
    /// [`as_conversion`] — not a general coercion mechanism.
    ///
    /// Every conversion here already exists as a `registry.rs` instance
    /// method (`int_assoc`/`float_assoc`/`bignum_assoc`/`ratio_assoc`/
    /// `char_assoc`); `as`/`try-as` desugar straight into a call on it via
    /// [`Self::check_assoc_call`] with the already-checked `expr` as the
    /// receiver — no new `Expr` variant, no new interp logic beyond the two
    /// `Option`-returning counterparts `as_conversion` names for the two
    /// partial pairs (`try-int->char`/`try-bignum->int`, `registry.rs`).
    fn check_as(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        try_variant: bool,
    ) -> Result<Typed, Error> {
        let form_name = if try_variant { "try-as" } else { "as" };
        if args.len() != 2 {
            return Err(Error::TypeError(format!("{}: ({} Type expr)", form_name, form_name)));
        }
        let target = self.canon(&parse_type(heap, args[0])?);
        let src = self.check_at(heap, interp, env, args[1], None, nth_loc(arg_locs, 1))?;

        // Identity: same type, a no-op cast.
        if src.ty == target {
            return Ok(if try_variant { wrap_some(src, target) } else { src });
        }
        // `i32`<->`i64`: a pure relabel, no runtime effect. `RtValue::Int` is
        // uniformly `i64` regardless of which static width labels it (see
        // `registry::int_assoc`'s `int->float` doc comment) — there is no
        // real 32-bit-truncating representation anywhere in this codebase
        // for `as`/`try-as` to imitate, so crossing widths is total in both
        // directions.
        if matches!((&src.ty, &target), (Type::I32, Type::I64) | (Type::I64, Type::I32)) {
            let relabeled = Typed { ty: target.clone(), ..src };
            return Ok(if try_variant { wrap_some(relabeled, target) } else { relabeled });
        }
        // Boxing as a trait object — the explicit spelling of the same
        // widening the expectation-driven coercion performs. Never a
        // `try-as`: whether a concrete type implements a trait is settled
        // statically, so the conversion either always succeeds or is a type
        // error, and an `Option` result would be misleading either way.
        if let Type::Dyn(trait_path, pins) = &target {
            if try_variant {
                return Err(Error::TypeError(format!(
                    "try-as: `{}` either always succeeds or is a type error (a type's trait impls are known statically) — use `as`",
                    mangle_type(&target)
                )));
            }
            return self.coerce_to_dyn(src, trait_path, pins);
        }

        let (panic_method, try_method) = as_conversion(&src.ty, &target).ok_or_else(|| {
            Error::TypeError(format!(
                "{}: no conversion from {:?} to {:?} (as/try-as cover only the numeric/char catalog: int/f64/bignum/ratio/char)",
                form_name, src.ty, target
            ))
        })?;
        let owner = prim_type_path(&src.ty).expect("as_conversion only matches primitive source types");
        let method = if try_variant { try_method.unwrap_or(panic_method) } else { panic_method };
        let called = self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &owner, method, receiver: Some(src), expected: None },
            &[],
            &[],
        )?;

        // `float->int`/`bignum->int`/`char->int` are always registered with
        // an `I32` return even when the caller asked for `i64` — see
        // `as_conversion`'s doc comment. Whenever this branch is reached
        // with `target == I64`, `called`'s (unwrapped, for a try_method
        // result) type is always exactly `I32` by construction of
        // `as_conversion`'s table.
        if try_variant {
            match try_method {
                Some(_) => Ok(if target == Type::I64 { retype_option(called, target) } else { called }),
                None => {
                    let called = if target == Type::I64 { Typed { ty: Type::I64, ..called } } else { called };
                    Ok(wrap_some(called, target))
                }
            }
        } else if target == Type::I64 {
            Ok(Typed { ty: Type::I64, ..called })
        } else {
            Ok(called)
        }
    }

    /// `(compile name)` / `(compile type::method)`: the name being compiled
    /// is program structure, not runtime data, so — unlike an ordinary call —
    /// its argument is special-cased here to read as an unevaluated symbol or
    /// `::`-path rather than a checked expression. A string (`(compile
    /// "name")`) is a type error: it would let the same name be spelled two
    /// incompatible ways for no benefit.
    ///
    /// Builds a [`CompileTarget`] directly from whichever resolution this
    /// already has to do anyway (`resolve_fn`/`resolve_fn_path` for a free
    /// function, `resolve_bare_type`/`resolve_type_path` for a `type::method`)
    /// instead of discarding the resolved `Path` and handing `Interp` a bare
    /// name string to re-resolve with an unqualified, module-blind search —
    /// the same `written`+`home` independent re-resolution every other
    /// reference (`Call`/`Global`/`FnRef`) gets, not a special case.
    ///
    /// A name this can't resolve (never registered, or genuinely private
    /// from here — `resolve_fn`/`resolve_fn_path` already fold "exists but
    /// not visible" into "doesn't resolve", same as every other reference)
    /// is deliberately *not* a check-time error: a `CompileTarget::Fn` is
    /// still built, with a best-effort placeholder `resolved` `Path` (never
    /// looked at unless resolution also fails again at runtime, in which
    /// case it's only used to name the failure) — preserving the existing
    /// `EvalError::NoSuchFunction` this has always surfaced through
    /// `Interp::resolve_fn_ref` at the actual `(compile ...)` call, not a
    /// check-time rejection.
    fn check_compile(&self, heap: &Heap, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("compile: (compile name) — expected exactly 1 argument".into()));
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            Value::Path(pid) => heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect::<Vec<_>>()
                .join("::"),
            _ => {
                return Err(Error::TypeError(
                    "compile: expected a symbol or path naming a function, e.g. (compile foo) or (compile point::x) — not a string".into(),
                ))
            }
        };
        let generic_err = || {
            Error::TypeError(format!(
                "compile: `{}` is generic — a generic function has no single compiled body; \
                 call it at concrete types and compile those uses' enclosing functions instead",
                name
            ))
        };
        // Shared by both places that build a `CompileTarget::Fn`: a bare name
        // (`resolve_fn`) and a module-qualified one that turned out not to
        // name a `type::method` (`resolve_fn_path`) — same generic check,
        // same "no resolution -> best-effort placeholder `Path`, deferring
        // to a runtime `NoSuchFunction`" fallback (see this function's own
        // doc comment for why that's deliberate).
        let fn_target = |written: Vec<String>, resolved: Option<Path>| -> Result<CompileTarget, Error> {
            if resolved.as_ref().is_some_and(|fq| self.generic_fn_templates.contains_key(fq)) {
                return Err(generic_err());
            }
            let resolved = resolved.unwrap_or_else(|| Path::from_segments(written.clone()));
            Ok(CompileTarget::Fn(self.mk_ref(written, resolved)))
        };
        let target = match name.rsplit_once("::") {
            None => fn_target(vec![name.clone()], self.resolve_fn(&name))?,
            Some((type_part, method)) => {
                let type_segs: Vec<String> = type_part.split("::").map(|s| s.to_string()).collect();
                let type_fq = if type_segs.len() == 1 {
                    self.resolve_bare_type(&type_segs[0])
                } else {
                    self.resolve_type_path(&type_segs)
                }
                .filter(|tp| self.reg.type_def(tp).is_some_and(|def| def.assoc.contains_key(method)));
                match type_fq {
                    // A genuine `type::method` — the type exists and has
                    // this associated function/method.
                    Some(type_fq) => {
                        if self.generic_method_templates.contains_key(&(type_fq.clone(), method.to_string())) {
                            return Err(generic_err());
                        }
                        CompileTarget::Method { type_name: type_fq, method: method.to_string(), home: self.ns.clone() }
                    }
                    // Not a type::method — a module-qualified free function
                    // instead (e.g. `(compile m::inc)`), or genuinely nothing
                    // at all (`(compile bogus::x)`).
                    None => {
                        let full_segs: Vec<String> = name.split("::").map(|s| s.to_string()).collect();
                        let resolved = self.resolve_fn_path(&full_segs);
                        fn_target(full_segs, resolved)?
                    }
                }
            }
        };
        Ok(Typed { loc: None, expr: Expr::CompileFn(target), ty: Type::Bool })
    }

    /// `(quote datum)`: `datum` as a literal `Sexpr` value, unevaluated. See
    /// [`Expr::Quote`] for why this converts to an owned [`QuotedSexpr`]
    /// rather than keeping the raw read `Value`.
    fn check_quote(&self, heap: &Heap, args: &[Value]) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("quote: (quote datum)".into()));
        }
        let qs = value_to_quoted(heap, args[0])?;
        Ok(Typed { loc: None, expr: Expr::Quote(qs), ty: Type::Named(Path::root("sexpr"), vec![]) })
    }

    /// `(quasiquote template)`: like `quote`, but `(unquote x)` sub-forms are
    /// evaluated (checked as an ordinary expression in the surrounding
    /// lexical scope, so they may reference locals) and spliced in directly.
    /// `x` must itself be `Sexpr`-typed — no implicit coercion from `i32`/
    /// `bool`/etc, matching the rest of the language's strict-type-matching
    /// rules. This is the natural fit for macro-writing (a `defmacro`
    /// parameter, or any other already-`Sexpr` sub-expression, splices in
    /// as-is); embedding a non-`Sexpr` runtime value as quoted data needs an
    /// explicit conversion (not provided yet — there is no `int->sexpr` etc.).
    /// A pure syntax-to-`Expr` desugaring — same idea as `list` building
    /// nested `Expr::Construct{Cons,..}` (`check_list_lit`) — so it needs no
    /// new runtime machinery; `(unquote-splicing x)` (`,@x`) is the one
    /// exception, desugaring to a call to the prelude's `sexpr-append` (see
    /// `check_qq_template`'s doc comment) since the spliced list's length
    /// isn't known until runtime. A quasiquote nested inside another is
    /// treated as ordinary literal data (no depth tracking).
    fn check_quasiquote(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
    ) -> Result<Typed, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("quasiquote: (quasiquote template)".into()));
        }
        self.check_qq_template(heap, interp, env, args[0])
    }

    /// Recursively desugar one quasiquote template node. See [`Self::check_quasiquote`].
    ///
    /// `(unquote-splicing x)` (`,@x`) as a list *element* (i.e. `car` of the
    /// cons being processed) splices `x`'s elements into the result in place,
    /// rather than nesting `x` itself as one element — CL/Scheme's usual
    /// `,@` semantics, used so a macro's `&rest body` (already one `Sexpr`
    /// list) can be spliced into a template as multiple sibling forms
    /// (e.g. `` `(progn ,@body) `` for a `body` of three forms expands to a
    /// 3-element `progn`, not a `progn` wrapping one 3-element list). Since
    /// `x`'s length isn't known until runtime, this can't be expressed as a
    /// static `cons` nest like the non-splicing case — it desugars to a call
    /// to the prelude's `sexpr-append` (`Checker::resolve_fn`, the same lookup
    /// `check_call`'s caller uses), joining `x` with the recursively
    /// desugared rest of the list. This means `,@` requires the prelude to
    /// be loaded (`sexpr-append` registered) — acceptable since the macros that
    /// actually need `,@` (`until`/`while-let`/`case`/`do`, roadmap step 8)
    /// live in the prelude themselves.
    fn check_qq_template(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
    ) -> Result<Typed, Error> {
        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        if let Value::Cons(_) = v {
            let car = heap.car(v)?;
            let cdr = heap.cdr(v)?;
            if is_symbol(heap, car, "unquote") {
                if let Value::Cons(_) = cdr {
                    let x = heap.car(cdr)?;
                    if heap.cdr(cdr)?.is_empty() {
                        // `check`'s reconciliation wraps a `Symbol` (e.g. a
                        // `gensym`'d temp) into `Sexpr::Sym` here automatically;
                        // any other non-`Sexpr` value is still rejected.
                        return self.check(heap, interp, env, x, Some(&sexpr_ty));
                    }
                }
                return Err(Error::TypeError("unquote: (unquote datum)".into()));
            }
            if let Value::Cons(_) = car {
                let car_car = heap.car(car)?;
                if is_symbol(heap, car_car, "unquote-splicing") {
                    let car_cdr = heap.cdr(car)?;
                    if let Value::Cons(_) = car_cdr {
                        let x = heap.car(car_cdr)?;
                        if heap.cdr(car_cdr)?.is_empty() {
                            let spliced = self.check(heap, interp, env, x, Some(&sexpr_ty))?;
                            let rest = self.check_qq_template(heap, interp, env, cdr)?;
                            let append_fq = self.resolve_fn("sexpr-append").ok_or_else(|| {
                                Error::TypeError(
                                    "unquote-splicing (,@) requires the prelude's `sexpr-append` to be loaded"
                                        .into(),
                                )
                            })?;
                            return Ok(Typed { loc: None,
                                expr: Expr::Call(self.mk_ref(vec!["sexpr-append".to_string()], append_fq), vec![spliced, rest]),
                                ty: sexpr_ty,
                            });
                        }
                    }
                    return Err(Error::TypeError("unquote-splicing: (unquote-splicing datum)".into()));
                }
            }
            let car_t = self.check_qq_template(heap, interp, env, car)?;
            let cdr_t = self.check_qq_template(heap, interp, env, cdr)?;
            let (adt, cons_idx) = self.sexpr_cons_ctor();
            return Ok(Typed { loc: None,
                expr: Expr::Construct { type_name: adt, variant: cons_idx, args: vec![car_t, cdr_t], mutable: false },
                ty: sexpr_ty,
            });
        }
        let qs = value_to_quoted(heap, v)?;
        Ok(Typed { loc: None, expr: Expr::Quote(qs), ty: sexpr_ty })
    }

    fn check_let(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let: (let ((name val)...) body...)".into()));
        }
        let mut binds = Vec::new();
        let mut name_locs: Vec<Option<Loc>> = Vec::new();
        for binding in heap.list_to_vec(args[0])? {
            // `list_to_vec_locs` so the binding value's own position reaches
            // `check_at` (a bare-atom value keeps its `Loc` for hover), and so
            // the binding name's own position is available for the child
            // `Env` (a later reference resolves goto-definition to here).
            let pair_locs = heap.list_to_vec_locs(binding)?;
            let pair: Vec<Value> = pair_locs.iter().map(|(v, _)| *v).collect();
            if pair.len() != 2 {
                return Err(Error::TypeError("let: binding must be (name val)".into()));
            }
            let name = match pair[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("let: binding name must be a symbol".into())),
            };
            name_locs.push(pair_locs.first().and_then(|(_, l)| l.clone()));
            // Binding values are checked in the *outer* environment (CL `let`).
            // Recovery boundary (B4): an ill-typed init becomes a `Never`-typed
            // hole, so the binding still enters scope (as `Never`, which unifies
            // with any later use) and the body — with all its bindings — stays
            // available to completion.
            let val_loc = pair_locs.get(1).and_then(|(_, l)| l.clone());
            let val = self.recovered(
                self.check_at(heap, interp, env, pair[1], None, val_loc.clone()),
                val_loc,
            )?;
            binds.push((name, val));
        }
        let env_binds: Vec<(String, Type, Option<Loc>)> = binds
            .iter()
            .zip(name_locs)
            .map(|((n, t), l)| (n.clone(), t.ty.clone(), l))
            .collect();
        let child = env.extended_with_locs(env_binds);
        let (body, ty) = self.check_seq(heap, interp, &child, &args[1..], &arg_locs[1..], expected)?;
        Ok(Typed { loc: None, expr: Expr::Let(binds, body), ty })
    }

    /// `let*`: like `let` but each binding sees the earlier ones. Desugars to
    /// nested single-binding `let`s.
    fn check_let_star(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("let*: (let* ((name val)...) body...)".into()));
        }
        let binds = heap.list_to_vec(args[0])?;
        self.let_star_rec(heap, interp, env, &binds, &args[1..], &arg_locs[1..], expected)
    }

    // Same invariant checking context as `check_path_call` — see its comment.
    #[allow(clippy::too_many_arguments)]
    fn let_star_rec(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        binds: &[Value],
        body: &[Value],
        body_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if binds.is_empty() {
            let (body, ty) = self.check_seq(heap, interp, env, body, body_locs, expected)?;
            return Ok(Typed { loc: None, expr: Expr::Let(Vec::new(), body), ty });
        }
        let pair_locs = heap.list_to_vec_locs(binds[0])?;
        let pair: Vec<Value> = pair_locs.iter().map(|(v, _)| *v).collect();
        if pair.len() != 2 {
            return Err(Error::TypeError("let*: binding must be (name val)".into()));
        }
        let name = match pair[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("let*: binding name must be a symbol".into())),
        };
        let name_loc = pair_locs.first().and_then(|(_, l)| l.clone());
        let val_loc = pair_locs.get(1).and_then(|(_, l)| l.clone());
        // Recovery boundary (B4): see `check_let`.
        let val = self.recovered(
            self.check_at(heap, interp, env, pair[1], None, val_loc.clone()),
            val_loc,
        )?;
        let child = env.extended_with_locs(vec![(name.clone(), val.ty.clone(), name_loc)]);
        let inner = self.let_star_rec(heap, interp, &child, &binds[1..], body, body_locs, expected)?;
        let ty = inner.ty.clone();
        Ok(Typed { loc: None, expr: Expr::Let(vec![(name, val)], vec![inner]), ty })
    }

    /// `(setf var value)`: assign to a bound variable. The value must match the
    /// variable's type; the expression evaluates to that value.
    fn check_setf(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("setf: (setf var value)".into()));
        }
        let value_loc = nth_loc(arg_locs, 1);
        if let Value::Path(pid) = args[0] {
            let segs: Vec<String> = heap
                .path_segments(pid)
                .iter()
                .map(|s| heap.symbol_name(*s).to_string())
                .collect();
            return self.check_field_set(heap, interp, env, &segs, args[1], value_loc);
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("setf: target must be a variable".into())),
        };
        if let Some(ty) = env.get(&name).cloned() {
            let value = self.check_at(heap, interp, env, args[1], Some(&ty), value_loc)?;
            return Ok(Typed { loc: None, expr: Expr::Set(name, Box::new(value)), ty });
        }
        if let Some((path, vi)) = self.resolve_global(&name) {
            if !vi.mutable {
                return Err(Error::TypeError(format!("setf: cannot assign to constant `{}`", name)));
            }
            let value = self.check_at(heap, interp, env, args[1], Some(&vi.ty), value_loc)?;
            return Ok(Typed { loc: None, expr: Expr::SetGlobal(self.mk_ref(vec![name.clone()], path), Box::new(value)), ty: vi.ty });
        }
        Err(Error::TypeError(format!("setf: unbound variable: {}", name)))
    }

    /// `(setf var::field value)`: the write counterpart of
    /// `Checker::try_field_access` — resolves `var`'s type's `set-field`
    /// setter (synthesized by `Checker::check_defstruct` alongside the
    /// getter) and delegates to `Checker::check_assoc_call` exactly like
    /// `try_field_access` does, so a generic `defstruct`'s field type gets
    /// the receiver's concrete type arguments substituted (see that
    /// function's doc comment for why building the `Expr::Assoc` node by
    /// hand here once got this wrong).
    fn check_field_set(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
        value: Value,
        value_loc: Option<Loc>,
    ) -> Result<Typed, Error> {
        let [recv_name, field] = segs else {
            return Err(Error::TypeError(format!("setf: unresolved path: {}", segs.join("::"))));
        };
        let recv = if let Some(t) = env.get(recv_name) {
            Typed { loc: None, expr: Expr::Var(recv_name.clone()), ty: t.clone() }
        } else if let Some((path, vi)) = self.resolve_global(recv_name) {
            Typed { loc: None, expr: Expr::Global(self.mk_ref(vec![recv_name.clone()], path)), ty: vi.ty }
        } else {
            return Err(Error::TypeError(format!("setf: unbound variable: {}", recv_name)));
        };
        let Type::Named(type_fq, _) = &recv.ty else {
            return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", recv_name, field)));
        };
        let setter = format!("set-{}", field);
        let is_field_setter = self
            .reg
            .type_def(type_fq)
            .and_then(|d| d.assoc.get(&setter))
            .is_some_and(|af| af.instance && self.assoc_visible(type_fq, af));
        if !is_field_setter {
            return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", type_fq, field)));
        }
        let type_fq = type_fq.clone();
        self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method: &setter, receiver: Some(recv), expected: None },
            std::slice::from_ref(&value),
            &[value_loc],
        )
    }

    /// `(defvar (name Type) value)` / `(defconstant (name Type) value)`.
    /// Registers a global in the current namespace. The declared type is
    /// *mandatory* — a global's type is part of the program's public
    /// surface, so it is never inferred from the initializer (unlike a
    /// `let` binding, whose whole scope is in view; the untyped
    /// `(defvar name value)` form was removed 2026-07-03). The annotation
    /// also gives the initializer its expected type, which
    /// post-monomorphization is what lets e.g. a generic function value
    /// (`(defvar (f (fn (i32) i32)) identity)`) resolve its type arguments
    /// at all.
    fn check_defvar(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mutable: bool,
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevel, Error> {
        if parts.len() != 2 {
            return Err(Error::TypeError("defvar/defconstant: (defvar (name Type) value)".into()));
        }
        let (name, ann) = match parts[0] {
            Value::Cons(_) => {
                let pair = heap.list_to_vec(parts[0])?;
                if pair.len() != 2 {
                    return Err(Error::TypeError("defvar: name must be (name Type)".into()));
                }
                let name = match pair[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defvar: name must be a symbol".into())),
                };
                (name, self.canon(&parse_type(heap, pair[1])?))
            }
            Value::Symbol(id) => {
                return Err(Error::TypeError(format!(
                    "defvar: a global needs a declared type — write (defvar ({} Type) value)",
                    heap.symbol_name(id)
                )))
            }
            _ => return Err(Error::TypeError("defvar: name must be (name Type)".into())),
        };
        // The value is checked at the top level (no locals), but globals/fns are
        // visible via the registry.
        let value = self.check(heap, interp, &Env::new(), parts[1], Some(&ann))?;
        let ty = ann;
        self.check_redef("variable", &name, self.cur_ns().vars.get(&name))?;
        self.reg
            .root
            .module_mut(&self.ns)
            .vars
            .insert(name.clone(), VarInfo { ty: ty.clone(), mutable, public, builtin: false });
        let fq_name = self.fq(&name);
        if let Some(loc) = def_loc {
            self.reg.def_locs.vars.insert(fq_name.clone(), loc);
        }
        Ok(TopLevel::Defvar { name: fq_name, ty, value, mutable, public })
    }

    /// `(loop body...)`: an infinite loop, exited via `break`/`return`. Its
    /// type is the join of every `break`/`return` reached directly inside it
    /// (not crossing a nested loop/lambda); `Never` if it never exits.
    fn check_loop(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let (body, ty) = self.check_loop_body(heap, interp, env, args, arg_locs, Type::Never)?;
        Ok(Typed { loc: None, expr: Expr::Loop(body), ty })
    }

    /// Check a loop body sequence with a fresh loop-stack frame seeded at
    /// `seed` (the loop's known result type if fixed, e.g. `Unit` for
    /// `while`/`dotimes`/`dolist`; `Never` for `loop`, refined by any
    /// `break`/`return` found). Returns the checked body and the frame's final
    /// type. The frame is popped even if checking the body fails.
    fn check_loop_body(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        body: &[Value],
        body_locs: &[Option<Loc>],
        seed: Type,
    ) -> Result<(Vec<Typed>, Type), Error> {
        self.loop_stack.borrow_mut().push(seed);
        let result = self.check_seq(heap, interp, env, body, body_locs, None);
        let ty = self.loop_stack.borrow_mut().pop().expect("pushed above");
        let (body, _) = result?;
        Ok((body, ty))
    }

    /// `(break)`: exit the nearest enclosing loop with no value (`Unit`). Type
    /// `Never` (diverges; satisfies any expectation).
    fn check_break(&self, args: &[Value]) -> Result<Typed, Error> {
        if !args.is_empty() {
            return Err(Error::TypeError("break: (break), takes no arguments".into()));
        }
        self.contribute_loop_exit(Type::Unit)?;
        Ok(Typed { loc: None, expr: Expr::Break, ty: Type::Never })
    }

    /// `(return)` / `(return value)`: exit the nearest enclosing loop,
    /// optionally with a value (`Unit` if omitted). Type `Never`.
    fn check_return(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() > 1 {
            return Err(Error::TypeError("return: (return) or (return value)".into()));
        }
        let top = self
            .loop_stack
            .borrow()
            .last()
            .cloned()
            .ok_or_else(|| Error::TypeError("return: not inside a loop".into()))?;
        let expected = non_never(&top).cloned();
        let value = match args.first() {
            Some(v) => Some(self.check_at(heap, interp, env, *v, expected.as_ref(), nth_loc(arg_locs, 0))?),
            None => None,
        };
        let ty = value.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Unit);
        self.contribute_loop_exit(ty)?;
        Ok(Typed { loc: None, expr: Expr::Return(value.map(Box::new)), ty: Type::Never })
    }

    /// Unify a `break`/`return` value's type into the nearest enclosing loop's
    /// accumulated exit type, erroring if there is no enclosing loop or the
    /// types disagree (mirrors how `match`/`cond` arms must agree).
    fn contribute_loop_exit(&self, ty: Type) -> Result<(), Error> {
        let mut stack = self.loop_stack.borrow_mut();
        let top = stack
            .last_mut()
            .ok_or_else(|| Error::TypeError("break/return: not inside a loop".into()))?;
        *top = join_types(&*top, &ty)?;
        Ok(())
    }

    /// `(list e1 e2 ... en)`: build a `Sexpr` cons-list from `Sexpr`-typed
    /// elements (each checked against `Sexpr`, so `()` adopts `Nil`).
    /// Desugars to nested `(Cons e1 (Cons e2 (... (Nil))))`; `(list)` is `(Nil)`.
    fn check_list_lit(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let sexpr_ty = Type::Named(Path::root("sexpr"), vec![]);
        let (adt, cons_idx) = self.sexpr_cons_ctor();
        let (_, nil_idx) = self.resolve_ctor("nil").expect("sexpr::nil is built in");
        let mut acc = Typed { loc: None,
            expr: Expr::Construct { type_name: adt.clone(), variant: nil_idx, args: Vec::new(), mutable: false },
            ty: sexpr_ty.clone(),
        };
        for (i, &elem) in args.iter().enumerate().rev() {
            let e = self.check_at(heap, interp, env, elem, Some(&sexpr_ty), nth_loc(arg_locs, i))?;
            acc = Typed { loc: None,
                expr: Expr::Construct { type_name: adt.clone(), variant: cons_idx, args: vec![e, acc], mutable: false },
                ty: sexpr_ty.clone(),
            };
        }
        Ok(acc)
    }

    /// Collects a heterogeneous run of already-checked expressions into one
    /// `Sexpr` list — the `&rest`-free counterpart of [`Self::cons_rest_list`]
    /// used by the `format`/`print`/`println` special forms, whose trailing
    /// arguments each keep their *own* natural type rather than sharing one
    /// declared element type. Every element is wrapped into its `Sexpr`
    /// encoding by [`Self::wrap_rest_elem`] against its own inferred type (so a
    /// value whose type has no `Sexpr` encoding is the same clear `TypeError`
    /// a bad `&rest` element gets), then `sexpr-cons`-ed together back-to-front.
    fn cons_hetero_sexpr(&self, items: Vec<Typed>) -> Result<Typed, Error> {
        let cons_path = Path::root("sexpr-cons");
        items.into_iter().rev().try_fold(
            Typed { loc: None, expr: Expr::Quote(QuotedSexpr::Nil), ty: sexpr_ty() },
            |acc, item| {
                let item = self.wrap_rest_elem(&item.ty.clone(), item)?;
                Ok(Typed { loc: None, expr: Expr::Call(Ref::synthetic(cons_path.clone()), vec![item, acc]), ty: sexpr_ty() })
            },
        )
    }

    /// The `(format dest control &rest args)` special form — CL's `format`
    /// adapted to typelisp's `bool` (`dest` is `true` for CL's `t`, writing to
    /// stdout, or `false` for CL's `nil`, only building the string). The
    /// control string's directives are interpreted at runtime by
    /// `Interp::run_format`; here we only type the fixed arguments (`dest:
    /// bool`, `control: string`) and collect the variadic tail into one
    /// `Sexpr` list, then lower to the internal `format-rt` builtin. Always
    /// returns the built `string` (statically) — it is additionally written to
    /// stdout when `dest` is `true`, but a single return type keeps the static
    /// system simple, and the string is useful to a `false` caller.
    fn check_format(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError(
                "format: expected at least a destination (bool) and a control string".to_string(),
            ));
        }
        let dest = self.check_at(heap, interp, env, args[0], Some(&Type::Bool), nth_loc(arg_locs, 0))?;
        let control = self.check_at(heap, interp, env, args[1], Some(&Type::Str), nth_loc(arg_locs, 1))?;
        let mut items = Vec::new();
        for (i, &a) in args[2..].iter().enumerate() {
            items.push(self.check_at(heap, interp, env, a, None, nth_loc(arg_locs, 2 + i))?);
        }
        let list = self.cons_hetero_sexpr(items)?;
        Ok(Typed {
            loc: None,
            expr: Expr::Call(Ref::synthetic(Path::root("format-rt")), vec![dest, control, list]),
            ty: Type::Str,
        })
    }

    /// The `(print control &rest args)` / `(println control &rest args)`
    /// special forms — thin front-ends over the same directive engine as
    /// [`Self::check_format`], always writing to stdout (`println` with a
    /// trailing newline). Lowers to the `print-rt`/`println-rt` builtin (per
    /// `builtin`) and yields `unit`.
    fn check_print_like(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        builtin: &str,
        ret: Type,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError(format!(
                "{}: expected at least a control string",
                if builtin == "println-rt" { "println" } else { "print" }
            )));
        }
        let control = self.check_at(heap, interp, env, args[0], Some(&Type::Str), nth_loc(arg_locs, 0))?;
        let mut items = Vec::new();
        for (i, &a) in args[1..].iter().enumerate() {
            items.push(self.check_at(heap, interp, env, a, None, nth_loc(arg_locs, 1 + i))?);
        }
        let list = self.cons_hetero_sexpr(items)?;
        Ok(Typed {
            loc: None,
            expr: Expr::Call(Ref::synthetic(Path::root(builtin)), vec![control, list]),
            ty: ret,
        })
    }

    // Same invariant checking context as `check_path_call` — see its comment.
    #[allow(clippy::too_many_arguments)]
    fn check_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        written: &[String],
        name: &Path,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Typed, Error> {
        let sig = self.reg.fn_sig(name).expect("caller checked presence").clone();
        let fixed = sig.params.len();
        match &sig.rest {
            None if args.len() != fixed => {
                return Err(Error::TypeError(format!(
                    "{}: expected {} argument(s), got {}",
                    name,
                    fixed,
                    args.len()
                )));
            }
            Some(_) if args.len() < fixed => {
                return Err(Error::TypeError(format!(
                    "{}: expected at least {} argument(s), got {}",
                    name,
                    fixed,
                    args.len()
                )));
            }
            _ => {}
        }
        // Non-generic functions (the overwhelming majority) take the fast
        // path: `params`/`subst` stay empty, so `subst_apply`/`type_has_param`
        // below are no-ops and behavior is identical to before generics were
        // added. Generic functions resolve `sig.type_params` against the
        // actual argument types with the same `unify`/`subst_apply` accumulation
        // `check_construct` uses for an ADT's `params`.
        let params: HashSet<String> = sig.type_params.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut typed = Vec::new();
        for (i, (arg, pty)) in args[..fixed].iter().zip(sig.params.iter()).enumerate() {
            let st = subst_apply(pty, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, i))?;
            unify(&params, pty, &ta.ty, &mut subst)?;
            typed.push(ta);
        }
        // A variadic function: every remaining argument is individually
        // checked against the (possibly still-generic) rest element type,
        // then collected into a single `Sexpr` list actual argument — see
        // `sexpr_ty`'s doc comment and `Self::check_apply`'s value-level
        // counterpart.
        if let Some(elem_ty) = &sig.rest {
            let mut rest_typed = Vec::new();
            for (i, arg) in args[fixed..].iter().enumerate() {
                let st = subst_apply(elem_ty, &subst);
                let exp = if type_has_param(&st, &params) { None } else { Some(st) };
                let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, fixed + i))?;
                unify(&params, elem_ty, &ta.ty, &mut subst)?;
                rest_typed.push(ta);
            }
            let resolved = subst_apply(elem_ty, &subst);
            typed.push(self.cons_rest_list(&resolved, rest_typed)?);
        }
        // Associated-type pins participate in *inference*, not just
        // verification: when a `where` bound pins an associated type to one of
        // `name`'s own type parameters (e.g. `reverse`'s `(where (Iter I (Item
        // A)))`, where `A` appears in no ordinary argument and so is otherwise
        // uninferrable), resolve the concrete type's real binding for that
        // associated type and unify it into `subst`. Runs before the
        // "cannot infer" check below so the pinned variable counts as
        // resolved; the strict validation loop further down then re-checks the
        // (now-inferred) pin for consistency. A `unify` failure here is
        // ignored — the validation loop reports pin mismatches with a precise
        // message.
        for (tparam, trait_bounds) in &sig.bounds {
            let Some(concrete) = subst.get(tparam).cloned() else { continue };
            // `concrete` may itself be a **bare** unresolved type variable —
            // the enclosing generic function's own type parameter, forwarded
            // straight through (this call is nested inside another
            // `where`-bounded generic's own diagnostic body-check, mirroring
            // `Self::validate_where_bounds`'s bare-variable case below).
            // There is no registered type to look an associated-type impl up
            // on yet, but the *enclosing* function's own `where` clause may
            // already pin the same associated type on the same trait —
            // propagate that pin into `subst` instead of leaving `name`'s
            // parameter uninferred (e.g. `elt` calling `nth n it` with `it:
            // I` forwards `elt`'s own `(Iter I (Item A))` pin so `nth`'s own
            // `A` resolves to `elt`'s own, still-open `A`; both become
            // concrete together once the enclosing function is specialized).
            if let Type::Named(n, args) = &concrete {
                if args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() {
                    let var_name = n.local();
                    if let Some(caller_tbs) = env.bounds.get(var_name) {
                        for tb in trait_bounds {
                            let Some(caller_tb) =
                                caller_tbs.iter().find(|c| c.trait_path == tb.trait_path)
                            else {
                                continue;
                            };
                            for (assoc_name, declared_ty) in &tb.assoc {
                                if let Some(caller_ty) = caller_tb.assoc.get(assoc_name) {
                                    let _ = unify(&params, declared_ty, caller_ty, &mut subst);
                                }
                            }
                        }
                    }
                    continue;
                }
            }
            let type_fq = match &concrete {
                Type::Named(n, _) if n.is_simple() && self.reg.type_def(n).is_none() => None,
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            let Some(type_fq) = type_fq else { continue };
            let Some(def) = self.reg.type_def(&type_fq) else { continue };
            let concrete_args: &[Type] = match &concrete {
                Type::Named(_, args) => args.as_slice(),
                _ => &[],
            };
            for tb in trait_bounds {
                for (assoc_name, declared_ty) in &tb.assoc {
                    if let Some(actual) =
                        resolve_trait_assoc_type(def, &tb.trait_path, assoc_name, concrete_args)
                    {
                        let _ = unify(&params, declared_ty, &actual, &mut subst);
                    }
                }
            }
        }
        for p in &sig.type_params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type parameter `{}` for `{}`",
                    p, name
                )));
            }
        }
        // Call-site `where`-bound validation: for each of `name`'s own type
        // parameters that carries trait bounds, check the concrete type
        // `subst` resolved it to actually implements every required trait
        // (and, if the bound pins an associated type, that the concrete
        // type's *own* binding for that associated type matches what the
        // `where` clause declared). Skipped when the resolved type is
        // itself still a *bare* unresolved type variable (this call is
        // nested inside another generic function and the type only becomes
        // concrete further up the call chain) — propagating the *caller's
        // own* bounds through to verify an equivalent bound is already
        // declared on the outer function is real but currently unexercised
        // by any code in this repo (no `where`-bounded function forwards its
        // own type parameter into another `where`-bounded call), so it's
        // left to the existing runtime `Expr::TraitCall` fallback, same as
        // before this validation existed. Note the skip only catches a bare
        // type variable (`T` itself) — a type variable *wrapped* in a
        // concrete type (e.g. `Vector<U>` for an outer `U`) is still
        // validated strictly below, which is correct for plain
        // trait-membership checks (`Vector<U>` never implements `Iter`
        // regardless of `U`) but could in principle reject an associated-type
        // pin that would actually hold once `U` resolves further up the call
        // chain — also unexercised today.
        self.validate_where_bounds(&name.to_string(), &sig.bounds, &subst, &env.bounds)?;
        // Monomorphization: a call that instantiates a generic function at
        // fully concrete types is rewritten to reference the specialized
        // definition (generated by `check_form`'s drain). If any type
        // argument is still open we are inside another generic function's
        // diagnostic body-check — keep the original (never-executed) call;
        // the enclosing function's own specialization will re-check this
        // very call with the types concrete. Builtin generic free functions
        // (no template) keep their runtime-dispatched call as-is.
        let mut call_path = name.clone();
        let mut specialized = false;
        if !sig.type_params.is_empty() && self.generic_fn_templates.contains_key(name) {
            let targs: Vec<Type> = sig.type_params.iter().map(|p| subst[p.as_str()].clone()).collect();
            if !targs.iter().any(|t| self.type_is_open(t)) {
                call_path = self.request_fn_specialization(name, targs);
                specialized = true;
            }
        }
        // A monomorphized specialization has no "as written" source form of
        // its own (see `Self::fn_ref_node`'s identical case) — resolve it by
        // its own mangled identity directly rather than re-searching for the
        // unspecialized generic template under the original bare/qualified
        // name.
        let r = if specialized {
            Ref::synthetic(call_path)
        } else {
            self.mk_ref(written.to_vec(), call_path)
        };
        Ok(Typed { loc: None, expr: Expr::Call(r, typed), ty: subst_apply(&sig.ret, &subst) })
    }

    /// Call-site `where`-bound validation shared by [`Self::check_call`]
    /// (free functions) and [`Self::check_assoc_call`] (methods): for each
    /// bounded type parameter, check the concrete type `subst` resolved it
    /// to actually implements every required trait (and that any pinned
    /// associated type matches). `name` is only for error messages
    /// (`fn-name` / `type::method`).
    ///
    /// Two cases where `concrete` isn't (fully) resolved, both arising when
    /// this call is nested inside another generic function's own
    /// (diagnostic) body-check — the type only becomes concrete once the
    /// *enclosing* function is specialized, which re-runs this same
    /// validation with everything resolved (`Self::specialize_defun_body`
    /// deliberately drops its own `bounds` for this reason — the call sites
    /// inside its re-checked body do the validating):
    ///
    /// - `concrete` is a **bare** unresolved type variable (literally the
    ///   enclosing function's own type parameter, e.g. `T` passed straight
    ///   through) — `caller_bounds` (`Env::bounds`, the enclosing function's
    ///   own `where` clause) is checked for an equivalent declared bound on
    ///   that same variable instead of silently skipping, so forwarding a
    ///   type parameter into a bounded call without declaring a matching
    ///   bound on it is now a real error instead of an opaque runtime
    ///   `Expr::TraitCall` failure two calls later.
    /// - `concrete` **wraps** a still-open type variable (e.g. `Vector<U>`
    ///   for an outer `U`) — trait-membership (`def.impls`) is still checked
    ///   strictly (a type *constructor*'s trait impls don't depend on which
    ///   concrete type parameter it's instantiated with), but an associated-
    ///   type pin comparison is skipped when either side still mentions an
    ///   open variable — `U`'s own binding for that associated type isn't
    ///   knowable yet, and comparing against the unresolved placeholder would
    ///   reject pins that turn out to hold once `U` resolves further up the
    ///   call chain.
    fn validate_where_bounds(
        &self,
        name: &str,
        bounds: &HashMap<String, Vec<TraitBound>>,
        subst: &HashMap<String, Type>,
        caller_bounds: &HashMap<String, Vec<TraitBound>>,
    ) -> Result<(), Error> {
        for (tparam, trait_bounds) in bounds {
            let Some(concrete) = subst.get(tparam) else { continue };
            if let Type::Named(n, args) = concrete {
                if args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() {
                    let var_name = n.local();
                    let declared = caller_bounds.get(var_name);
                    for tb in trait_bounds {
                        let satisfied = declared.is_some_and(|caller_tbs| {
                            caller_tbs.iter().any(|caller_tb| {
                                caller_tb.trait_path == tb.trait_path
                                    && tb.assoc.iter().all(|(k, v)| caller_tb.assoc.get(k) == Some(v))
                            })
                        });
                        if !satisfied {
                            return Err(Error::TypeError(format!(
                                "{}: type parameter `{}` is only known here as the enclosing function's own `{}`, \
                                 which has no `where` bound declaring it implements {:?} — the enclosing function's \
                                 signature needs a matching `where` clause on `{}`",
                                name, tparam, var_name, tb.trait_path, var_name
                            )));
                        }
                    }
                    continue;
                }
            }
            let type_fq = match concrete {
                Type::Named(n, _) => Some(n.clone()),
                other => prim_type_path(other),
            };
            let Some(type_fq) = type_fq else { continue };
            let def = self.reg.type_def(&type_fq).ok_or_else(|| {
                Error::TypeError(format!(
                    "{}: type {:?} does not implement a trait required by `where` clause on type parameter `{}`",
                    name, concrete, tparam
                ))
            })?;
            for tb in trait_bounds {
                if !def.impls.contains(&tb.trait_path) {
                    return Err(Error::TypeError(format!(
                        "{}: type {:?} does not implement trait {:?} required by `where` clause on type parameter `{}`",
                        name, concrete, tb.trait_path, tparam
                    )));
                }
                let concrete_args: &[Type] = match concrete {
                    Type::Named(_, args) => args.as_slice(),
                    _ => &[],
                };
                for (assoc_name, declared_ty) in &tb.assoc {
                    // The pin may itself be one of `name`'s own type parameters
                    // (e.g. `(Item A)` on a generic `map`), inferred from the
                    // other arguments into `subst` above — resolve it before
                    // comparing, so a *polymorphic* pin is verified against the
                    // concrete type `A` actually unified to, not the bare
                    // variable. A concrete pin (`(Item i32)`) is unaffected
                    // (`subst_apply` is a no-op on a type with no variables).
                    let declared_ty = subst_apply(declared_ty, subst);
                    if self.type_is_open(&declared_ty) {
                        continue;
                    }
                    let actual = resolve_trait_assoc_type(def, &tb.trait_path, assoc_name, concrete_args);
                    match actual {
                        Some(actual) if actual == declared_ty => {}
                        Some(actual) if self.type_is_open(&actual) => {}
                        actual => {
                            return Err(Error::TypeError(format!(
                                "{}: type {:?}'s associated type `{}` is {:?}, but `where` clause requires {:?}",
                                name, concrete, assoc_name, actual, declared_ty
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// `ctor` is `(type path, variant index)` — the same pair [`Self::resolve_ctor`]
    /// returns, bundled into one parameter to keep the arity down.
    // Same invariant checking context as `check_path_call` — see its comment.
    #[allow(clippy::too_many_arguments)]
    fn check_construct(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        ctor: (&Path, usize),
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        let (adt_name, variant) = ctor;
        let def = self.reg.type_def(adt_name).expect("indexed adt exists").clone();
        let fields = &def.variants[variant].fields;
        if args.len() != fields.len() {
            return Err(Error::TypeError(format!(
                "{}: expected {} field(s), got {}",
                def.variants[variant].name,
                fields.len(),
                args.len()
            )));
        }
        let params: HashSet<String> = def.params.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();

        // Seed the substitution from an expected `Named(adt, args)` type, so a
        // field-less constructor such as `None` can learn its type argument.
        if let Some(Type::Named(n, eargs)) = expected {
            if n == adt_name && eargs.len() == def.params.len() {
                for (p, a) in def.params.iter().zip(eargs.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
            }
        }

        let mut typed_args = Vec::new();
        for (i, (arg, field)) in args.iter().zip(fields.iter()).enumerate() {
            let st = subst_apply(field, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, i))?;
            unify(&params, field, &ta.ty, &mut subst)?;
            typed_args.push(ta);
        }

        let mut result_args = Vec::new();
        for p in &def.params {
            match subst.get(p) {
                Some(t) => result_args.push(t.clone()),
                None => {
                    return Err(Error::TypeError(format!(
                        "cannot infer type argument `{}` for `{}`",
                        p, adt_name
                    )))
                }
            }
        }
        Ok(Typed { loc: None,
            expr: Expr::Construct {
                type_name: adt_name.clone(),
                variant,
                args: typed_args,
                mutable: def.kind == AdtKind::Struct,
            },
            ty: Type::Named(adt_name.clone(), result_args),
        })
    }

    // ---- match / if-let ---------------------------------------------------

    fn check_match(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if args.is_empty() {
            return Err(Error::TypeError("match: (match expr arms...)".into()));
        }
        let mut scrut = self.check_at(heap, interp, env, args[0], None, nth_loc(arg_locs, 0))?;
        // Matching a trait object back down to concrete types: unwrap the fat
        // box to the `Sexpr` inside and let the existing `Sexpr` downcast
        // patterns do the rest (`check_ctor_pattern`'s downcast branch and
        // `(the T p)`), so there is exactly one runtime type test in the
        // language rather than a second one just for `:dyn`. Correctly
        // non-exhaustive, too: the set of implementing types is open, so a
        // catch-all arm is required — which falls out of `Sexpr`'s own
        // eleven-variant exhaustiveness rule with nothing added.
        if let Type::Dyn(..) = scrut.ty {
            scrut = Typed { loc: scrut.loc.clone(), expr: Expr::DynValue(Box::new(scrut)), ty: sexpr_ty() };
        }
        let (adt_name, _) = self.expect_adt(&scrut.ty)?;
        // `match` covers every sum type, `Sexpr` included. Symbol/Sexpr
        // redesign Phase 5 fenced `Sexpr` off here (its structure was to be
        // navigated only through the `sexpr-*` accessor island), but that
        // stance was reversed in preparation for a user-facing `(read)`:
        // read data's type is only known at runtime, and `match` — with type
        // refinement and exhaustiveness over the eleven `Sexpr` variants — is
        // the language's natural eliminator for it. The runtime machinery
        // (`match_sexpr_ctor` in the interpreter, `compile-sexpr-tag-test`/
        // `compile-sexpr-field` in `compiler.rs`) predates the fence and
        // serves both eras unchanged. `Value::Path` (an `a::b` token) has its
        // own `path` variant (added 2026-07-19, `registry::sexpr_def`'s
        // eleventh) — `(path s)` binds `s : Sexpr`, a fresh proper list of
        // the segments as `sym`s (the only `match_sexpr_ctor` arm that
        // allocates — see its doc comment for the GC-rooting that needs).
        // Both `sym`'s and `path`'s payloads compile too (`compile-sexpr-
        // field`/`compile-construct-sexpr` variants `5`/`10` in
        // `compiler.rs`, plus the new `rt_path_to_list`/`rt_list_to_path`
        // runtime shims for `path`'s list building).
        let total_variants = self.reg.type_def(&adt_name).expect("adt exists").variants.len();

        let mut arms = Vec::new();
        let mut covered: HashSet<usize> = HashSet::new();
        let mut catchall = false;
        let mut result_ty: Option<Type> = expected.cloned();
        // Set (B2) when an arm is skipped in `recover` mode: a skipped arm's
        // variant coverage is unknown, so the exhaustiveness check (B1) below
        // must be suppressed for this `match` to avoid a spurious cascade.
        let mut arm_recovered = false;

        for arm_val in &args[1..] {
            let arm_loc = heap.cons_loc(*arm_val);
            // Per-arm recovery boundary (B2): an arm whose shape or pattern is
            // ill-typed is recorded and skipped (rather than aborting the whole
            // `match`), so the surviving arms' bindings still reach completion.
            // The arm *body* recovers at the finer `check_seq` element boundary
            // (B3), so a bad body form holes just that form, not the arm.
            macro_rules! recover_arm {
                ($e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) if self.recover => {
                            arm_recovered = true;
                            self.push_recovered(e, arm_loc.clone());
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                };
            }
            // `list_to_vec_locs` so each arm body form keeps its own position
            // (a bare-atom body, e.g. a pattern-bound variable, stays hoverable).
            let parts_locs = recover_arm!(heap.list_to_vec_locs(*arm_val));
            let parts: Vec<Value> = parts_locs.iter().map(|(v, _)| *v).collect();
            if parts.is_empty() {
                recover_arm!(Err(Error::TypeError("match: arm must be (pattern body...)".into())));
            }
            let (pat, binds) =
                recover_arm!(self.check_pattern(heap, &scrut.ty, parts[0], parts_locs[0].1.clone()));
            match &pat {
                // A Sexpr-downcast `Ctor` pattern's `type_name` names the
                // *downcast target* (a user struct/enum), not the
                // scrutinee's own ADT — it must not count toward this
                // match's exhaustiveness over `adt_name`'s variants (design
                // plan's "網羅性" rule). Restricting to a same-ADT `type_name`
                // covers both the ordinary case (always same-ADT) and this
                // one in a single guard.
                Pattern::Ctor { type_name, variant, .. } if *type_name == adt_name => {
                    covered.insert(*variant);
                }
                Pattern::Wildcard | Pattern::Bind(..) => catchall = true,
                _ => {}
            }
            let arm_env = env.extended_with_locs(binds);
            // Diverging arms don't constrain the result type; concrete arms must
            // all agree (Never joins with anything).
            let arm_expected = result_ty.as_ref().and_then(non_never);
            let body_locs: Vec<Option<Loc>> = parts_locs[1..].iter().map(|(_, l)| l.clone()).collect();
            let (body, body_ty) =
                recover_arm!(self.check_seq(heap, interp, &arm_env, &parts[1..], &body_locs, arm_expected));
            result_ty = Some(match result_ty {
                None => body_ty,
                // A body that disagrees with the other arms' type: record it in
                // recover mode but keep this arm (its subtree is valid, only the
                // type join failed) and the previously-agreed result type.
                Some(r) => match join_types(&r, &body_ty) {
                    Ok(joined) => joined,
                    Err(e) if self.recover => {
                        self.push_recovered(e, arm_loc.clone());
                        r
                    }
                    Err(e) => return Err(e),
                },
            });
            arms.push(Arm { pat, body });
        }

        // Exhaustiveness (B1): in `recover` mode a non-exhaustive `match`
        // records the error but still returns a fully-typed `Match` node (every
        // arm was already checked) — this is exactly what lets completion work
        // inside a non-catchall arm, where truncating the source deletes the
        // arms that would have made the match exhaustive. Suppressed when an
        // arm was skipped (B2), since coverage is then unknown.
        if !catchall && !arm_recovered && covered.len() != total_variants {
            let e = Error::TypeError(format!(
                "non-exhaustive match on `{}`: {}/{} variants covered",
                adt_name,
                covered.len(),
                total_variants
            ));
            if self.recover {
                self.push_recovered(e, nth_loc(arg_locs, 0));
            } else {
                return Err(e);
            }
        }
        let ty = match result_ty {
            Some(ty) => ty,
            // No arms survived (all skipped in recover mode, or a genuinely
            // empty `match`): a `Never`-typed hole stands in for the value.
            None if self.recover => Type::Never,
            None => return Err(Error::TypeError("match: no arms".into())),
        };
        Ok(Typed { loc: None, expr: Expr::Match(Box::new(scrut), arms), ty })
    }

    /// Check a pattern against the type of the value it matches, returning the
    /// pattern and the variable bindings it introduces — each with the source
    /// position of its name in the pattern (`loc`: where `v` itself sits, the
    /// element position its enclosing list recorded — the arm's own for a
    /// whole-arm variable pattern, the constructor list's for a field
    /// sub-pattern), so a later reference in the arm body can resolve
    /// goto-definition back to it (`Env::extended_with_locs`,
    /// `DefLocs::local_refs`).
    fn check_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
        loc: Option<Loc>,
    ) -> Result<(Pattern, PatternBindings), Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                if name == "_" {
                    Ok((Pattern::Wildcard, Vec::new()))
                } else {
                    Ok((
                        Pattern::Bind(name.to_string(), self.is_heap_repr(expected)),
                        vec![(name.to_string(), expected.clone(), loc)],
                    ))
                }
            }
            Value::Int(n) => {
                if expected.is_integer() {
                    Ok((Pattern::Int(n), Vec::new()))
                } else {
                    Err(Error::TypeError(format!(
                        "integer pattern does not match type {:?}",
                        expected
                    )))
                }
            }
            Value::Bool(b) if *expected == Type::Bool => Ok((Pattern::Bool(b), Vec::new())),
            Value::Char(c) if *expected == Type::Char => Ok((Pattern::Char(c), Vec::new())),
            Value::Cons(_) => self.check_ctor_pattern(heap, expected, v),
            other => Err(Error::TypeError(format!("pattern does not match {:?}: {:?}", expected, other))),
        }
    }

    fn check_ctor_pattern(
        &self,
        heap: &Heap,
        expected: &Type,
        v: Value,
    ) -> Result<(Pattern, PatternBindings), Error> {
        // `list_to_vec_locs` so each field sub-pattern keeps its own recorded
        // position — a bound name's `Loc` is what goto-definition on a later
        // reference resolves to (see `check_pattern`'s doc comment).
        let parts_locs = heap.list_to_vec_locs(v)?;
        let parts: Vec<Value> = parts_locs.iter().map(|(v, _)| *v).collect();
        if parts.is_empty() {
            return Err(Error::TypeError("pattern: empty list".into()));
        }

        // Sexpr downcast patterns (design plan's "出す" section — the CL-
        // conformant counterpart of Stage 1's "入れる" retype/wrap). Only
        // attempted against a `Sexpr` scrutinee, and only for a head that
        // isn't one of `Sexpr`'s own eleven built-in variant names, so
        // `(int n)`/`(cons a d)`/... keep meaning exactly what they always
        // have.
        if *expected == sexpr_ty() {
            if let Value::Symbol(id) = parts[0] {
                if heap.symbol_name(id) == "the" {
                    return self.check_type_test_pattern(heap, &parts, &parts_locs);
                }
            }
            const BUILTIN_SEXPR_CTORS: &[&str] =
                &["nil", "int", "float", "char", "bool", "sym", "str", "cons", "bignum", "ratio", "path"];
            let is_builtin_head =
                matches!(parts[0], Value::Symbol(id) if BUILTIN_SEXPR_CTORS.contains(&heap.symbol_name(id)));
            if !is_builtin_head {
                if let Some((adt_name, targs, variant)) = self.resolve_sexpr_downcast_ctor(heap, parts[0])? {
                    return self.check_ctor_pattern_fields(heap, adt_name, targs, variant, &parts, &parts_locs, true);
                }
            }
        }

        let ctor = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
        };
        // The constructor is resolved against the scrutinee's type (its
        // variants), not the namespace — the type context disambiguates it.
        let (adt_name, targs) = self.expect_adt(expected)?;
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let variant = def.variants.iter().position(|vr| vr.name == ctor).ok_or_else(|| {
            Error::TypeError(format!("`{}` is not a constructor of `{}`", ctor, adt_name))
        })?;
        self.check_ctor_pattern_fields(heap, adt_name, targs, variant, &parts, &parts_locs, false)
    }

    /// Resolve a Sexpr-downcast pattern's head to `(adt path, type args,
    /// variant index)`: a registered `defstruct` type's bare name
    /// (field-destructuring, its sole `new` variant, index 0 — e.g. `(point
    /// x y)`), a bare enum variant name (resolved exactly like a bare
    /// constructor *expression* is — [`Self::resolve_ctor`], in scope only
    /// after `(use EnumType)`), or a `::`-qualified enum variant (`(color::
    /// red)`, the escape hatch for a bare-name collision between two
    /// visible enums — `Self::resolve_type_path` on the prefix, the variant
    /// name as the last segment). `None` means the head names neither,
    /// letting the caller fall through to the ordinary (non-downcast)
    /// "not a constructor of Sexpr" error.
    fn resolve_sexpr_downcast_ctor(&self, heap: &Heap, head: Value) -> Result<Option<(Path, Vec<Type>, usize)>, Error> {
        let no_generics = |name: &Path, def: &AdtDef| -> Result<(), Error> {
            if def.params.is_empty() {
                Ok(())
            } else {
                Err(Error::TypeError(format!(
                    "pattern: `{}` is generic — a downcast pattern can't infer its type arguments; \
                     use `(the {}<...> p)` for a whole-value bind instead",
                    name, name
                )))
            }
        };
        match head {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id);
                if let Some(type_fq) = self.resolve_bare_type(name) {
                    if let Some(def) = self.reg.type_def(&type_fq) {
                        if def.kind == AdtKind::Struct {
                            no_generics(&type_fq, def)?;
                            return Ok(Some((type_fq, Vec::new(), 0)));
                        }
                    }
                }
                if let Some((adt, idx)) = self.resolve_ctor(name) {
                    if adt != Path::root("sexpr") {
                        let def = self.reg.type_def(&adt).expect("resolved ctor's type is registered");
                        no_generics(&adt, def)?;
                        return Ok(Some((adt, Vec::new(), idx)));
                    }
                }
                Ok(None)
            }
            Value::Path(pid) => {
                let segs: Vec<String> =
                    heap.path_segments(pid).iter().map(|s| heap.symbol_name(*s).to_string()).collect();
                if segs.len() < 2 {
                    return Ok(None);
                }
                let (type_segs, member) = segs.split_at(segs.len() - 1);
                if let Some(type_fq) = self.resolve_type_path(type_segs) {
                    if let Some(def) = self.reg.type_def(&type_fq) {
                        if let Some(idx) = def.variants.iter().position(|vr| vr.name == member[0]) {
                            no_generics(&type_fq, def)?;
                            return Ok(Some((type_fq, Vec::new(), idx)));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// `(the Type pattern)` — a whole-value Sexpr downcast that binds (or
    /// further destructures) the matched value at its own declared `Type`
    /// rather than field-by-field, preserving a mutable struct's identity
    /// and the only way to pull a `Vector<T>`/`HashTable<K,V>` back out of a
    /// `Sexpr` (see [`Pattern::TypeTest`]'s doc comment).
    fn check_type_test_pattern(
        &self,
        heap: &Heap,
        parts: &[Value],
        parts_locs: &[(Value, Option<Loc>)],
    ) -> Result<(Pattern, PatternBindings), Error> {
        if parts.len() != 3 {
            return Err(Error::TypeError("pattern: (the Type pattern)".into()));
        }
        let ty = self.canon(&parse_type(heap, parts[1])?);
        if !self.is_heap_repr(&ty) {
            return Err(Error::TypeError(format!(
                "pattern: `{:?}` has no Sexpr representation, cannot downcast with `the`",
                ty
            )));
        }
        let (pat, binds) = self.check_pattern(heap, &ty, parts[2], parts_locs[2].1.clone())?;
        Ok((Pattern::TypeTest(ty, Box::new(pat)), binds))
    }

    /// Shared tail of [`Self::check_ctor_pattern`]: given a resolved `(adt
    /// path, type args, variant index)` — whether from the ordinary
    /// same-type ctor lookup or a Sexpr-downcast resolution — validates the
    /// field count and type-checks each field sub-pattern.
    fn check_ctor_pattern_fields(
        &self,
        heap: &Heap,
        adt_name: Path,
        targs: Vec<Type>,
        variant: usize,
        parts: &[Value],
        parts_locs: &[(Value, Option<Loc>)],
        downcast: bool,
    ) -> Result<(Pattern, PatternBindings), Error> {
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let subst: HashMap<String, Type> = def.params.iter().cloned().zip(targs.iter().cloned()).collect();

        let fields = &def.variants[variant].fields;
        if parts.len() - 1 != fields.len() {
            return Err(Error::TypeError(format!(
                "pattern `{}`: expected {} field(s), got {}",
                def.variants[variant].name,
                fields.len(),
                parts.len() - 1
            )));
        }
        let mut sub_pats = Vec::new();
        let mut binds = Vec::new();
        let mut sexpr_fields = Vec::new();
        let mut field_types = Vec::new();
        for ((sub, sub_loc), field) in parts_locs[1..].iter().zip(fields.iter()) {
            let field_ty = subst_apply(field, &subst);
            // Baked into the pattern here (where the instantiated field
            // type is in hand) so the type-erased interpreter can decode a
            // boxed struct's `Sexpr`-declared field faithfully — see
            // `Pattern::Ctor::sexpr_fields`'s doc comment.
            sexpr_fields.push(matches!(&field_ty, Type::Named(p, _) if *p == Path::root("sexpr")));
            // Kept in full alongside `sexpr_fields` for compiled code's own
            // per-field decode — see `Pattern::Ctor::field_types`'s doc
            // comment.
            field_types.push(field_ty.clone());
            let (p, b) = self.check_pattern(heap, &field_ty, *sub, sub_loc.clone())?;
            sub_pats.push(p);
            binds.extend(b);
        }
        Ok((Pattern::Ctor { type_name: adt_name, variant, args: sub_pats, sexpr_fields, field_types, downcast }, binds))
    }

    // ---- helpers ----------------------------------------------------------

    /// Check a body sequence (`expr...`), returning the typed exprs and the type
    /// of the last (the sequence's type; `Unit` if empty). Only the last element
    /// is checked against `expected`.
    fn check_seq(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        body: &[Value],
        body_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<(Vec<Typed>, Type), Error> {
        if body.is_empty() {
            if let Some(e) = expected {
                if *e != Type::Unit {
                    return Err(Error::TypeError(format!(
                        "empty body has type Unit, expected {:?}",
                        e
                    )));
                }
            }
            return Ok((Vec::new(), Type::Unit));
        }
        let mut out = Vec::new();
        let last = body.len() - 1;
        for (i, expr) in body.iter().enumerate() {
            let exp = if i == last { expected } else { None };
            let loc = nth_loc(body_locs, i);
            // Body-element recovery boundary (B3): one ill-typed form in a
            // sequence becomes a `Never`-typed hole rather than aborting the
            // whole body, so the surviving forms — and every `let`/`match`
            // binding they introduce — still reach completion. This is the
            // highest-leverage boundary: `check_seq` is the body checker for
            // `defun`/`defmethod`/`defmacro`/`lambda`/`let`/`labels`/`loop`
            // bodies and `match` arm bodies alike.
            out.push(self.recovered(self.check_at(heap, interp, env, *expr, exp, loc.clone()), loc)?);
        }
        let ty = out[last].ty.clone();
        Ok((out, ty))
    }

    /// Require `ty` to be a registered data type, returning its path and args.
    fn expect_adt(&self, ty: &Type) -> Result<(Path, Vec<Type>), Error> {
        match ty {
            Type::Named(n, args) if self.reg.type_def(n).is_some() => Ok((n.clone(), args.clone())),
            other => Err(Error::TypeError(format!(
                "expected a data type, found {:?}",
                other
            ))),
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Checker::new()
    }
}

// ---- free helpers ---------------------------------------------------------

/// The `i`-th element of a parallel element-location slice (see
/// `Heap::list_to_vec_locs`), or `None` when the index is out of range or that
/// element carried no recorded location. Used throughout `check_*` to pass a
/// bare atom argument's own source location into [`Checker::check_at`].
fn nth_loc(locs: &[Option<Loc>], i: usize) -> Option<Loc> {
    locs.get(i).cloned().flatten()
}

/// Recursively convert a raw read `Value` into an owned [`QuotedSexpr`] (see
/// [`Expr::Quote`] for why `quote` can't just keep the heap pointer). A
/// `Value::Path` (a `::`-qualified token, e.g. `a::b`, appearing inside
/// quoted data — most commonly inside a macro body that generates
/// module-qualified code) mirrors to `QuotedSexpr::Path`, its segments in
/// written order (see `docs/dev/language-design.md` §2.1 for `Value::Path`
/// itself). `alloc_quoted` (`eval/interp.rs`) is this function's inverse.
fn value_to_quoted(heap: &Heap, v: Value) -> Result<QuotedSexpr, Error> {
    Ok(match v {
        Value::Empty => QuotedSexpr::Nil,
        Value::Int(n) => QuotedSexpr::Int(n),
        // `Sexpr::Float`/`bignum`/`ratio` are all heap-boxed (`Value::Boxed`,
        // see `BoxedObj`).
        Value::Boxed(id) if heap.is_bignum(id) => QuotedSexpr::Bignum(heap.bignum_value(id).clone()),
        Value::Boxed(id) if heap.is_ratio(id) => QuotedSexpr::Ratio(heap.ratio_value(id).clone()),
        Value::Boxed(id) => QuotedSexpr::Float(heap.float_value(id)),
        Value::Char(c) => QuotedSexpr::Char(c),
        Value::Bool(b) => QuotedSexpr::Bool(b),
        Value::Symbol(id) => QuotedSexpr::Sym(heap.symbol_name(id).to_string()),
        Value::Str(id) => QuotedSexpr::Str(heap.string(id).to_string()),
        Value::Cons(_) => {
            let car = value_to_quoted(heap, heap.car(v)?)?;
            let cdr = value_to_quoted(heap, heap.cdr(v)?)?;
            QuotedSexpr::Cons(Box::new(car), Box::new(cdr))
        }
        Value::Path(id) => {
            QuotedSexpr::Path(heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect())
        }
    })
}

/// Whether `v` is the symbol named `name`.
fn is_symbol(heap: &Heap, v: Value, name: &str) -> bool {
    matches!(v, Value::Symbol(id) if heap.symbol_name(id) == name)
}

/// `Checker::check_as`'s conversion table: `(from, to)` -> `(panic_method,
/// try_method)`, where `panic_method` is the existing `registry.rs` instance
/// method `as` always calls, and `try_method` — `Some` only for the two
/// *partial* pairs — is the `Option`-returning counterpart `try-as` calls
/// instead (`None` here means the pair is total: `try-as` calls
/// `panic_method` too and wraps the result in `Some`).
///
/// Every `to`-is-`i64` request is looked up as if `to` were `i32` instead:
/// `float->int`/`bignum->int`/`char->int` (`registry.rs`) are always
/// registered with a hardcoded `I32` return even though the underlying
/// computation already produces a full `i64` (`RtValue::Int` is uniformly
/// `i64` regardless of which static width labels it) — `check_as` relabels
/// the result to `I64` afterward on that branch, so this table only ever
/// needs to name the `i32` method once. An `i32`<->`i64` source/target pair
/// itself never reaches this table — `check_as` relabels that directly,
/// before the lookup.
fn as_conversion(from: &Type, to: &Type) -> Option<(&'static str, Option<&'static str>)> {
    use Type::*;
    let to_key = if *to == I64 { I32 } else { to.clone() };
    match (from, &to_key) {
        (I32, F64) | (I64, F64) => Some(("int->float", None)),
        (I32, Bignum) | (I64, Bignum) => Some(("int->bignum", None)),
        (I32, Ratio) | (I64, Ratio) => Some(("int->ratio", None)),
        (I32, Char) | (I64, Char) => Some(("int->char", Some("try-int->char"))),
        (F64, I32) => Some(("float->int", None)),
        (F64, Bignum) => Some(("float->bignum", None)),
        (F64, Ratio) => Some(("float->ratio", None)),
        (Char, I32) => Some(("char->int", None)),
        (Bignum, Ratio) => Some(("bignum->ratio", None)),
        (Bignum, F64) => Some(("bignum->float", None)),
        (Bignum, I32) => Some(("bignum->int", Some("try-bignum->int"))),
        (Ratio, F64) => Some(("ratio->float", None)),
        (Ratio, Bignum) => Some(("ratio->bignum", None)),
        _ => None,
    }
}

/// `Some(inner)` as a `Typed` — `Checker::check_as`'s `try-as` wrapper for a
/// total conversion's already-computed result.
fn wrap_some(inner: Typed, target: Type) -> Typed {
    let ty = Type::Named(Path::root("option"), vec![target]);
    Typed { loc: None, expr: Expr::Construct { type_name: Path::root("option"), variant: 0, args: vec![inner], mutable: false }, ty }
}

/// Relabels an already-computed `Option<I32>` `Typed` (an `as_conversion`
/// `try_method` call's result) to `Option<inner_target>` — `Checker::check_as`'s
/// `i64`-target counterpart of the plain relabel it does for a total
/// conversion; see [`as_conversion`]'s doc comment for why the underlying
/// `Option`'s runtime payload doesn't actually change.
fn retype_option(opt: Typed, inner_target: Type) -> Typed {
    Typed { ty: Type::Named(Path::root("option"), vec![inner_target]), ..opt }
}

/// `Some(t)` unless `t` is `Never` (which never constrains an expectation).
fn non_never(t: &Type) -> Option<&Type> {
    if *t == Type::Never {
        None
    } else {
        Some(t)
    }
}

/// Join two branch types: `Never` is absorbed by the other; otherwise the two
/// must be equal.
fn join_types(a: &Type, b: &Type) -> Result<Type, Error> {
    if *a == Type::Never {
        return Ok(b.clone());
    }
    if *b == Type::Never {
        return Ok(a.clone());
    }
    if a == b {
        return Ok(a.clone());
    }
    Err(Error::TypeError(format!(
        "branches have incompatible types: {:?} vs {:?}",
        a, b
    )))
}

/// `Sexpr` — the type a `&rest` parameter's collected arguments are bound as
/// inside a `defun`/`lambda` body, and the type `apply`'s trailing list
/// argument must have (see `Checker::parse_params_rest`/`check_call`/
/// `check_apply_form`). Every Lisp's `&rest` parameter is an ordinary list
/// (built from cons cells), never a homogeneous array type — `defmacro`'s own
/// `&rest` already works this way unconditionally (`Interp::bind_macro_args`);
/// `defun`/`lambda`'s `&rest` now matches it, just with an added call-site
/// check (below) that every individual variadic argument satisfies the
/// declared element type before it's wrapped into the list.
fn sexpr_ty() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

/// The `Sexpr` constructor name that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping — see [`Checker::wrap_rest_elem`]).
/// `&rest`'s declared element type must be one of these (or `Sexpr`) since
/// nothing else has a lossless `Sexpr` encoding to collect into a list with.
fn sexpr_ctor_for(elem_ty: &Type) -> Option<&'static str> {
    match elem_ty {
        Type::I64 | Type::I32 => Some("int"),
        Type::F64 => Some("float"),
        Type::Bignum => Some("bignum"),
        Type::Ratio => Some("ratio"),
        Type::Char => Some("char"),
        Type::Bool => Some("bool"),
        Type::Str => Some("str"),
        Type::Symbol => Some("sym"),
        _ => None,
    }
}

/// The type of an integer literal: the expected integer type if any, else `i32`.
fn int_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_integer() => t.clone(),
        _ => Type::I32,
    }
}

/// The type of a float literal: the expected float type if any, else `f64`.
fn float_lit_ty(expected: Option<&Type>) -> Type {
    match expected {
        Some(t) if t.is_float() => t.clone(),
        _ => Type::F64,
    }
}

/// Whether `path` is a type variable in `params` (a single-segment name).
fn is_param(path: &crate::Path, params: &HashSet<String>) -> bool {
    path.is_simple() && params.contains(path.local())
}

/// Whether `t` mentions any type parameter in `params`.
fn type_has_param(t: &Type, params: &HashSet<String>) -> bool {
    match t {
        Type::Named(n, args) => {
            (args.is_empty() && is_param(n, params)) || args.iter().any(|a| type_has_param(a, params))
        }
        Type::Fn(ps, rest, r) => {
            ps.iter().any(|p| type_has_param(p, params))
                || matches!(rest, Some(t) if type_has_param(t, params))
                || type_has_param(r, params)
        }
        // Only the associated-type pins can mention a parameter — the head
        // is a trait name, never a type variable.
        Type::Dyn(_, pins) => pins.iter().any(|p| type_has_param(p, params)),
        _ => false,
    }
}

/// Renders a canonicalized (fully module-qualified) type as the string used
/// inside a specialization's mangled name — a faithful, unambiguous
/// serialization, so two distinct canonical types can never render equal
/// (which is what lets `mangled_fn_path` double as the instantiation's
/// identity). Nested generics render Rust-style: `vector<cons-cell<string,i32>>`.
fn mangle_type(t: &Type) -> String {
    match t {
        Type::I8 => "i8".into(),
        Type::I16 => "i16".into(),
        Type::I32 => "i32".into(),
        Type::I64 => "i64".into(),
        Type::Isize => "isize".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::Usize => "usize".into(),
        Type::F32 => "f32".into(),
        Type::F64 => "f64".into(),
        Type::Bignum => "bignum".into(),
        Type::Ratio => "ratio".into(),
        Type::Bool => "bool".into(),
        Type::Char => "char".into(),
        Type::Str => "string".into(),
        Type::Symbol => "symbol".into(),
        Type::Unit => "()".into(),
        Type::Never => "!".into(),
        Type::Named(p, args) if args.is_empty() => p.to_string(),
        Type::Named(p, args) => {
            format!("{}<{}>", p, args.iter().map(mangle_type).collect::<Vec<_>>().join(","))
        }
        // The space is load-bearing, the same way `mangled_method_name`'s is:
        // the reader treats it as a token boundary outside `<>`, so no
        // user-written name can ever collide with a mangled one.
        Type::Dyn(p, pins) if pins.is_empty() => format!("dyn {}", p),
        Type::Dyn(p, pins) => {
            format!("dyn {}<{}>", p, pins.iter().map(mangle_type).collect::<Vec<_>>().join(","))
        }
        Type::Fn(ps, rest, r) => {
            let mut inner: Vec<String> = ps.iter().map(mangle_type).collect();
            if let Some(t) = rest {
                inner.push(format!("&rest {}", mangle_type(t)));
            }
            format!("(fn ({}) {})", inner.join(","), mangle_type(r))
        }
    }
}

/// The specialized function's [`Path`] for `base` instantiated at `args`:
/// the base path with its final segment rewritten to e.g. `"identity <i32>"`.
/// The space is load-bearing: the reader treats whitespace as a delimiter,
/// so no source-written symbol can ever spell this name — a user `(defun
/// identity<i32> ...)` (a perfectly legal token) can therefore never collide
/// with a generated specialization in `Interp`'s function table.
fn mangled_fn_path(base: &Path, args: &[Type]) -> Path {
    let mut segs = base.parent().to_vec();
    segs.push(mangled_method_name(base.local(), args));
    Path::from_segments(segs)
}

/// The mangled *local* name shared by function and method specializations —
/// e.g. `"unwrap <i32>"`. See [`mangled_fn_path`]'s doc comment for why the
/// space makes collisions with source-written names impossible.
fn mangled_method_name(base: &str, args: &[Type]) -> String {
    format!("{} <{}>", base, args.iter().map(mangle_type).collect::<Vec<_>>().join(","))
}

/// A type whose runtime value always boxes trivially via
/// `crate::eval::interp::rtvalue_to_struct_field` — the scalar half of
/// `Checker::enum_fields_representable`'s "is this field representable"
/// test (the other half being [`Checker::is_heap_repr_seen`]'s recursive
/// one). Mirrored by `crate::eval::interp::is_boxable_scalar` — the two
/// must agree on exactly this set, since together they decide whether an
/// enum instantiation is heap- or native-repr on both sides of the
/// checker/interpreter twin contract.
pub(crate) fn is_boxable_scalar(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || matches!(ty, Type::Bool | Type::Char | Type::Str | Type::Bignum | Type::Ratio)
}

/// Replace type parameters in `t` with their bindings from `subst`.
/// `pub(crate)`: `crate::eval::interp`'s `data_variant_field_types` reuses it
/// to instantiate a generic enum's variant field types when decoding a
/// compiled global (`Interp::enum_defs`).
pub(crate) fn subst_apply(t: &Type, subst: &HashMap<String, Type>) -> Type {
    match t {
        Type::Named(n, args) if args.is_empty() && n.is_simple() => match subst.get(n.local()) {
            Some(bound) => bound.clone(),
            None => t.clone(),
        },
        Type::Named(n, args) => {
            Type::Named(n.clone(), args.iter().map(|a| subst_apply(a, subst)).collect())
        }
        Type::Fn(ps, rest, r) => Type::Fn(
            ps.iter().map(|p| subst_apply(p, subst)).collect(),
            rest.as_ref().map(|t| Box::new(subst_apply(t, subst))),
            Box::new(subst_apply(r, subst)),
        ),
        // Only the associated-type pins can mention a substitutable
        // variable; the head names a trait.
        Type::Dyn(p, pins) => Type::Dyn(p.clone(), pins.iter().map(|t| subst_apply(t, subst)).collect()),
        other => other.clone(),
    }
}

/// Resolve what a concrete instantiation of `def` (e.g. `vector-iter<i32>`,
/// `concrete_args = [i32]`) actually binds trait `trait_path`'s associated
/// type `assoc_name` to — reading `AdtDef::trait_assoc`'s raw (possibly
/// still `def.params`-generic) binding and substituting `concrete_args` in,
/// the same `params`-zip `Checker::check_assoc_call` already does for method
/// signatures. `None` if `def` has no recorded binding for that trait/name
/// pair.
/// Whether `t` is a trait signature template's `Self` type variable. Written
/// `Self` in source; `parse_type_name` case-folds it, so it canonicalizes to
/// the single-segment path `self` — there is no type by that name, so no real
/// type can collide with it.
fn is_self_tvar(t: &Type) -> bool {
    matches!(t, Type::Named(p, args) if args.is_empty() && p.is_simple() && p.local() == "self")
}

/// Whether `t` mentions `Self` anywhere — [`is_self_tvar`] applied
/// recursively, so `Option<Self>` and `(fn (Self) i32)` count too.
fn mentions_self(t: &Type) -> bool {
    if is_self_tvar(t) {
        return true;
    }
    match t {
        Type::Named(_, args) => args.iter().any(mentions_self),
        Type::Dyn(_, pins) => pins.iter().any(mentions_self),
        Type::Fn(ps, rest, r) => {
            ps.iter().any(mentions_self) || matches!(rest, Some(t) if mentions_self(t)) || mentions_self(r)
        }
        _ => false,
    }
}

fn resolve_trait_assoc_type(def: &AdtDef, trait_path: &Path, assoc_name: &str, concrete_args: &[Type]) -> Option<Type> {
    let raw = def.trait_assoc.get(trait_path)?.get(assoc_name)?;
    if concrete_args.len() == def.params.len() {
        let subst: HashMap<String, Type> = def.params.iter().cloned().zip(concrete_args.iter().cloned()).collect();
        Some(subst_apply(raw, &subst))
    } else {
        Some(raw.clone())
    }
}

/// Unify a field template (which may mention `params`) against a concrete
/// `actual` type, accumulating parameter bindings in `subst`.
fn unify(
    params: &HashSet<String>,
    tmpl: &Type,
    actual: &Type,
    subst: &mut HashMap<String, Type>,
) -> Result<(), Error> {
    // A diverging value (`Never`) unifies with any template without binding.
    if *actual == Type::Never {
        return Ok(());
    }
    if let Type::Named(n, args) = tmpl {
        if args.is_empty() && is_param(n, params) {
            let key = n.local().to_string();
            return match subst.get(&key) {
                Some(bound) if bound == actual => Ok(()),
                Some(bound) => Err(Error::TypeError(format!(
                    "conflicting types for `{}`: {:?} vs {:?}",
                    key, bound, actual
                ))),
                None => {
                    subst.insert(key, actual.clone());
                    Ok(())
                }
            };
        }
    }
    match (tmpl, actual) {
        (Type::Named(n1, a1), Type::Named(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
            for (t, a) in a1.iter().zip(a2.iter()) {
                unify(params, t, a, subst)?;
            }
            Ok(())
        }
        // Two trait objects unify when they name the same trait and their
        // associated-type pins unify pairwise — `:dyn Iter<T>` against
        // `:dyn Iter<i32>` binds `T`.
        (Type::Dyn(n1, a1), Type::Dyn(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
            for (t, a) in a1.iter().zip(a2.iter()) {
                unify(params, t, a, subst)?;
            }
            Ok(())
        }
        (Type::Fn(p1, rest1, r1), Type::Fn(p2, rest2, r2)) if p1.len() == p2.len() => {
            for (t, a) in p1.iter().zip(p2.iter()) {
                unify(params, t, a, subst)?;
            }
            match (rest1, rest2) {
                (Some(t1), Some(t2)) => unify(params, t1, t2, subst)?,
                (None, None) => {}
                _ => {
                    return Err(Error::TypeError(format!(
                        "type mismatch: expected {:?}, found {:?}",
                        tmpl, actual
                    )))
                }
            }
            unify(params, r1, r2, subst)
        }
        _ if tmpl == actual => Ok(()),
        _ => Err(Error::TypeError(format!(
            "type mismatch: expected {:?}, found {:?}",
            tmpl, actual
        ))),
    }
}

#[cfg(test)]
mod header_tests {
    use super::{parse_generic_name_header, CL_COMPARISON_OPERATORS};

    #[test]
    fn a_plain_name_has_no_type_parameters() {
        assert_eq!(parse_generic_name_header("identity").unwrap(), ("identity".to_string(), vec![]));
        // Hyphens are ordinary name characters, not token boundaries.
        assert_eq!(parse_generic_name_header("cons-cell").unwrap(), ("cons-cell".to_string(), vec![]));
    }

    #[test]
    fn an_angle_bracket_header_splits_into_name_and_params() {
        assert_eq!(parse_generic_name_header("pair<a,b>").unwrap(), ("pair".to_string(), vec!["a".to_string(), "b".to_string()]));
        assert_eq!(parse_generic_name_header("identity<t>").unwrap(), ("identity".to_string(), vec!["t".to_string()]));
    }

    #[test]
    fn every_cl_comparison_operator_is_a_verbatim_name() {
        // `char<`/`string<=`/... must not be mistaken for a generic header —
        // the whole point of the whitelist. `<`/`<=`/... are already safe
        // (punctuation-led) but round-trip here too.
        for op in CL_COMPARISON_OPERATORS {
            assert_eq!(
                parse_generic_name_header(op).unwrap(),
                (op.to_string(), vec![]),
                "operator {:?} should be a verbatim name with no type parameters",
                op
            );
        }
    }

    #[test]
    fn a_non_comparison_operator_name_is_still_verbatim() {
        // Other punctuation-led / arrow-like names stay verbatim too.
        assert_eq!(parse_generic_name_header("->").unwrap(), ("->".to_string(), vec![]));
        assert_eq!(parse_generic_name_header("+").unwrap(), ("+".to_string(), vec![]));
    }

    #[test]
    fn a_malformed_generic_header_is_rejected() {
        // Starts `ident<` but isn't a known operator and doesn't close: an error,
        // not a silent verbatim name (catches genuine typos like `pair<a`).
        assert!(parse_generic_name_header("pair<a").is_err());
        assert!(parse_generic_name_header("pair<a,>").is_err());
    }
}
