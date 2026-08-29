//! The type checker: lowers read `Sexpr` values into a typed AST while checking.
//!
//! Checking is bidirectional in a small way: literal and constructor nodes may
//! be checked *against* an expected type (so integer literals adopt the expected
//! integer type and a nullary `None` learns its type argument), while everything
//! else synthesizes its own type and is reconciled against the expectation.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};

use std::rc::Rc;

use crate::{parse_type_spanned, prim_type_path, Error, Heap, Loc, Path, RootScope, SymId, Type, TypeNameSpan, Value};

use super::loop_dsl;
use typelisp_read::name_lexer::{NameLexer, NameTok};
use super::semantic::{TypeKind, TypeUse};

use super::resolved::{CompileTarget, Pattern, Ref};
use super::core::{self, Checked, Items};
use super::forms;
use super::repr::Repr;
use crate::dump::{CheckerDelta, RegistrySignature};
use super::registry::{opt_key_effective_ty, AdtDef, AdtKind, BlanketImpl, AssocFn, FnSig, MacroDef, Namespace, OptKeyParam, Registry, TraitBound, TraitDef, TraitDefault, TypeAlias, VarInfo, Variant};

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
/// `check`'s own resolution types (`Ref`/`CompileTarget`/`Pattern`), so a direct `use
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

/// What checking a top-level form produces: a *core top-level form*, one of
/// `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`/`defenum`/`module`/
/// `use`/`load`/`expr` (`tests/core_vocabulary_test.rs` has an example of
/// each).
///
/// This used to be a Rust `enum TopLevel`, and the change is the same one the
/// expression side makes: the program is cons cells from the reader through to
/// evaluation, so a definition is a form like any other. It buys two things
/// beyond uniformity. The self-hosted compiler can be handed a definition
/// directly — the old enum never reached the island, so the bridge had to
/// express the boundary itself — and a `fasl` writes the same cons cells every
/// other stored form is written as, with no second serialized shape.
///
/// Deliberately *not* in the vocabulary: a generic definition. The checker
/// keeps those as raw reader forms in its own template tables and emits only
/// monomorphized specializations, so nothing downstream needs a node for a
/// body that must never run.
pub type TopLevelForm = Value;

/// The `&optional`/`&key` structure of a `defmacro` lambda list, beyond its
/// leading required params (and any single trailing `&rest`, whose presence is
/// tracked separately by the `rest` flag on the `defmacro` form).
///
/// The checker's own working shape while parsing a macro's lambda list; it is
/// written into the `(defmacro ...)` form's own lambda field (see
/// [`Checker::defmacro_form`]) and read back from there at expansion time, so
/// this type does not outlive checking and needs no serialized form of its own.
/// A default-value body is a list of core forms, evaluated when the argument is
/// omitted in an environment where the earlier params are already bound (an
/// empty body binds `nil`). A plain fixed-or-`&rest` macro carries a `required`
/// equal to its fixed-param count with both `optionals` and `keys` empty (the
/// common case).
#[derive(Clone, Debug, PartialEq)]
pub struct MacroLambda {
    /// Number of leading required (positional, non-defaulted) params — the
    /// call's minimum argument count.
    pub required: usize,
    /// Default-value body for each `&optional` param, in order.
    pub optionals: Vec<Vec<Value>>,
    /// Each `&key` param's `(name, default-value body)`, in order — matched at
    /// the call site by the keyword symbol `:name`.
    pub keys: Vec<(String, Vec<Value>)>,
}

/// A `&rest` parameter's `(name, elem-type, name's source position)`, as
/// returned by [`Checker::parse_params_rest`] alongside the fixed params.
type RestParam = (String, Type, Option<Loc>);

/// One raw `&optional`/`&key` item: `(name, declared type, name's source
/// position, the default form as written)`. The default is *unparsed* here —
/// see [`Checker::parse_opt_key_spec`].
type OptKeySpec = (String, Type, Option<Loc>, Option<Value>);

/// A full lambda list's four regions, in CL's own order: the required
/// `(name type)` params (with their names' source positions), `&optional`,
/// `&rest`, `&key` — [`Checker::parse_params_full`]'s result.
type ParamsFull = (Vec<(String, Type)>, Vec<Option<Loc>>, Vec<OptKeySpec>, Option<RestParam>, Vec<OptKeySpec>);

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
    bounds: std::rc::Rc<BTreeMap<String, Vec<TraitBound>>>,
}

impl Env {
    fn new() -> Env {
        Env { vars: Vec::new(), bounds: std::rc::Rc::new(BTreeMap::new()) }
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
    fn with_bounds(&self, bounds: BTreeMap<String, Vec<TraitBound>>) -> Env {
        Env { vars: self.vars.clone(), bounds: std::rc::Rc::new(bounds) }
    }
}

/// Bundles [`Checker::check_assoc_call`]'s arguments to keep its arity down.
struct AssocCall<'a> {
    type_fq: &'a Path,
    method: &'a str,
    /// `Some` for an instance call (already-checked receiver, fills the
    /// method's first parameter slot); `None` for a static call.
    receiver: Option<Checked>,
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
impl Definable for TypeAlias {
    /// No alias is ever built in — `deftype` is the only thing that makes
    /// one, so redefining always goes through the ordinary `RedefPolicy`.
    fn builtin(&self) -> bool {
        false
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
/// (`construct`'s `MUTABLE`, `Pattern::Ctor::sexpr_fields`, method
/// resolution, an unresolved bounded-generic call vs `assoc`) would have to be re-derived
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
    /// A `BlanketImpl` at one concrete `target` type — the whole `impl`,
    /// since a blanket's methods are only ever wanted together.
    Blanket { trait_path: Path, target: Type },
}

/// Everything a check records outside its returned form, captured
/// so a speculative check can be undone — see [`Checker::check_mark`] /
/// [`Checker::rollback_to`].
struct CheckMark {
    errors: usize,
    warnings: usize,
    type_uses: usize,
    spec_pending: usize,
    /// Restored wholesale rather than truncated: a set has no length to cut
    /// back to, and it is small (one top-level form's instantiations).
    spec_memo: HashSet<(Path, Option<String>)>,
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
    /// The method's *runtime* parameters, receiver excluded: the required
    /// ones, then one per `&optional` at its effective type, then the
    /// `&rest` name (a plain `Sexpr` list), then one per `&key` — the same
    /// order and the same effective-type rule as a `defun`'s
    /// (`Checker::check_defun_opt_key`). Everything downstream of the header
    /// — the body's `Env`, the emitted definition form, the compile
    /// pipeline — sees only these, so a `&optional`/`&key` method is an
    /// ordinary fixed-arity one past this point; only the registered
    /// `FnSig` and the call site (`Checker::check_assoc_call`) know
    /// otherwise.
    params: Vec<(String, Type)>,
    /// Each of `params`'s name's own source position, parallel to `params` —
    /// see `Checker::parse_param_pairs`.
    param_locs: Vec<Option<Loc>>,
    /// The `&optional` items as written, for the registered signature. The
    /// default forms are still unchecked here — `Checker::check_defmethod_in`
    /// checks them, the header-only callers
    /// (`Checker::precheck_trait_defaults`, `Checker::precheck_blanket_impl`)
    /// don't need to.
    optionals: Vec<OptKeySpec>,
    /// The `&rest` parameter as written (`(name elem-type)`); `params`
    /// already carries the name bound to a plain `Sexpr` list.
    rest: Option<RestParam>,
    /// The `&key` items as written — see `optionals`.
    keys: Vec<OptKeySpec>,
    ret: Type,
    /// `(where (Trait TypeVar ...))` bounds after the return type — same
    /// syntax and parse as a free `defun`'s (`Checker::parse_defun_sig`).
    /// Lets an `impl` method on a generic owner require trait bounds on the
    /// owner's own type variables (e.g. `cons-cell<A,B>`'s `equals` needing
    /// `(where (Eq A) (Eq B))` for its recursive field comparisons).
    bounds: BTreeMap<String, Vec<TraitBound>>,
    /// Index of the first body form in the `defmethod` parts: 4 when a
    /// `where` clause is present, 3 otherwise (plus 1 more if a leading
    /// docstring was also consumed — see `doc`).
    body_start: usize,
    /// A leading docstring right before the body, if present — same rule as
    /// a free `defun`'s (`Checker::take_leading_docstring`).
    doc: Option<String>,
}

/// One `defstruct` field as written: `(name Type)`, `(pub name Type)`, or
/// either with a trailing default form.
struct StructField {
    name: String,
    ty: Type,
    public: bool,
    /// The default value, unchecked — spliced into a generated constructor's
    /// source (see [`Checker::check_defstruct`]) and recorded in
    /// [`Registry::struct_defaults`] so an `:include`ing child can reuse it.
    default: Option<SlotDefault>,
}

/// Where a [`StructField`]'s default form comes from.
enum SlotDefault {
    /// Written in this `defstruct`'s own source, so it is part of the form
    /// the caller already holds rooted for the length of the check.
    Written(Value),
    /// Inherited through `(:include Parent)`, read back out of
    /// [`Registry::struct_defaults`]. Kept detached and rebuilt at each
    /// splice site rather than materialized up front: a `Vec<Value>` is
    /// invisible to the collector, and everything between here and the
    /// generated constructor allocates.
    Inherited(crate::owned_form::OwnedForm),
}

/// A `(:constructor name)` / `(:constructor name (slot...))` option.
struct CtorSpec {
    name: String,
    /// `None` — a keyword constructor: every slot is a `&key` at its own
    /// default, so every slot must have one. `Some(params)` — a BOA
    /// constructor taking exactly these slots positionally; every slot it
    /// does not name is filled from that slot's default.
    boa: Option<Vec<BoaParam>>,
}

/// One parameter of a BOA constructor's lambda list: a slot name, optionally
/// after an `&optional` marker (in which case that slot's own default becomes
/// the parameter's).
struct BoaParam {
    name: String,
    optional: bool,
}

/// A `(defstruct (Name option...) ...)` option list, parsed.
#[derive(Default)]
struct StructOptions {
    constructors: Vec<CtorSpec>,
    copier: Option<String>,
    /// `(:include Parent)` — the parent type as written, for
    /// `parse_type_here_at`.
    include: Option<(Value, Option<Loc>)>,
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

/// The variable a [`Pattern::Guard`]'s test form reads the scrutinee
/// through — see that variant's doc comment for why one fixed name is
/// enough for every guard in a program. Spelled with a leading `$` so it
/// cannot collide with a name a user wrote: the reader has no way to
/// produce it, and the checker interns it directly.
const MATCH_SCRUT: &str = "$match-scrut";

/// `Option`'s variant indices (`check::registry::option_def`). Written down
/// once because three places already depend on the order — `()`'s inference
/// rule, the niche pattern shapes, and `check_construct`'s `None` shortcut.
const OPTION_SOME: usize = 0;
const OPTION_NONE: usize = 1;

/// `Sexpr`'s variant 0 — the old `nil`.
///
/// The variant is still *in* `sexpr_def`, and deliberately so: three places
/// have its numbering baked in (`eval::interp`'s `SEXPR_*`,
/// `compile::core_bridge`'s copy, and `Repr::field_kind`, which borrows the
/// same numbers), and `bignum`/`ratio` were appended rather than inserted
/// precisely so those constants would stay valid. Removing the entry would
/// renumber all ten.
///
/// What is gone is its *surface*: `(nil)` can no longer be written as a
/// constructor or a pattern, because a writable empty-list `Sexpr` would
/// make `Option<Sexpr>`'s niche unsound — `(Option::some (nil))` and
/// `(Option::none)` would be the same word. The empty list is reached as
/// `Option<Sexpr>`'s `none`, and the core IR still spells it `(construct
/// sexpr 0 ..)`, which the checker emits itself.
const SEXPR_RESERVED_VARIANT: usize = 0;

/// Message for a `(nil)` that survived in source somewhere.
const NIL_IS_GONE: &str = "`nil` is no longer a `Sexpr` constructor: the empty list is \
     `Option<Sexpr>`'s `none`. Write `()` where an `Option<Sexpr>` is expected, or \
     `(Option::none)`; match it with `((none) ...)`.";

/// Is `(adt, variant)` the retired `Sexpr::nil`?
fn is_retired_nil(adt: &Path, variant: usize) -> bool {
    variant == SEXPR_RESERVED_VARIANT && crate::types::path_is_builtin(adt, "sexpr")
}

/// How deep a blanket impl's own bounds may be chased before the search is
/// declared non-terminating (`impl<T> A T (where (B T))` together with
/// `impl<T> B T (where (A T))` would recur forever). Small on purpose:
/// legitimate chains are one or two links.
const BLANKET_BOUND_DEPTH: usize = 16;

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
    /// function boundary; they mean "nearest enclosing loop" and nothing
    /// else. Crossing one is what `catch`/`throw` are for, and they carry
    /// their type on the symbol rather than on this stack for exactly that
    /// reason ([`Self::throw_tags`]).
    loop_stack: RefCell<Vec<Type>>,
    /// The type each `catch`/`throw` symbol carries, learned from the first
    /// use of that symbol and enforced on every later one.
    ///
    /// `catch`/`throw` are *dynamic* — CL's, not a lexical `block`/
    /// `return-from` — so a `(throw 'found v)` can sit in a function called
    /// from inside `(catch 'found ...)`, with no lexical nesting between
    /// them. That rules out the mechanism [`Self::loop_stack`] uses, where
    /// `break`/`return` unify into the enclosing frame they can see: the
    /// throw site cannot see its catch site at all.
    ///
    /// What both sites *can* see is the symbol, so that is where the type
    /// lives. `(catch 'found body)` has `body`'s type, and registers it under
    /// `found`; `(throw 'found v)` checks `v` against whatever `found` is
    /// already known to carry (or establishes it, when the throw is checked
    /// first). Using one symbol for two types is a type error, which is the
    /// price of dynamic tags in a statically typed language — and what lets
    /// `(throw 'found 42)` be rejected where `found` carries strings.
    ///
    /// Keyed by the symbol's printed name rather than its `SymId`: the id is
    /// interned per `Heap`, and the message for a mismatch has to name the
    /// symbol anyway.
    throw_tags: RefCell<std::collections::BTreeMap<String, Type>>,
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
    /// a partial form still comes back. Defaults to `false`, which
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
    /// Scratch accumulator for `DefLocs::node_types`, drained alongside
    /// `local_refs` — same reason, same lifetime.
    node_types: RefCell<HashMap<(u32, u32), Type>>,
    /// The type of the top-level expression the last [`Self::check_form_at`]
    /// checked, or `None` if that form was a definition — see
    /// [`Self::expr_type`].
    expr_ty: Option<Type>,
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
    type_var_bindings: BTreeMap<String, Type>,
    /// Every occurrence of a user-defined type/trait name resolved so far,
    /// with its exact source span — recorded wherever the checker's grammar
    /// put a type name (annotations, definition headers, `Type::member`
    /// heads, ctor patterns) and drained by [`Self::take_type_uses`] for the
    /// LSP's semantic tokens. A `RefCell` for the same reason as
    /// `loop_stack`/`warnings`: annotation parsing runs under `&self`.
    type_uses: RefCell<Vec<TypeUse>>,
    /// Fully-qualified names a `defsignature` has announced and no `defun`
    /// has yet defined ([`Self::check_defsignature`]). The matching
    /// `check_defun` removes its own entry, which is how it knows the
    /// registry entry already under that name is the declaration it is
    /// fulfilling rather than a genuine redefinition (see
    /// [`Self::claim_predeclared`]). A name still here when the unit is
    /// finished is a declaration nothing defined, which
    /// [`Self::finish_unit`] reports: a declaration promises a definition.
    predeclared: HashSet<String>,
    /// Monotonic counter for the synthetic temporary names `Self::place_dedup`
    /// mints (e.g. `%place-tmp-3`) when desugaring `incf`/`decf`/`rotatef`/
    /// `shiftf` over a call-form place. Prefixed with a character the reader
    /// never produces in an identifier, so a temporary can never collide
    /// with a name written in source — no uniqueness check needed beyond
    /// incrementing this counter.
    place_tmp_counter: Cell<u32>,
    /// Set while [`Self::check_match`] *probes* an arm body that has no
    /// expected type to offer it yet. A constructor call whose type argument
    /// nothing determines — `(result::ok v)`, where no argument mentions
    /// `E` — then yields [`Type::Never`] in that slot (an *inference hole*)
    /// instead of failing, so the arm still reports the half of its type it
    /// does know and its siblings can supply the rest ([`merge_holes`]).
    /// Never set while the retained tree is built: every probed arm that made
    /// a hole is re-checked with this clear, so a hole can only ever inform
    /// an expectation, never survive into a lowered node.
    infer_probe: Cell<bool>,
    /// Holes ([`Self::infer_probe`]) made and still standing in the tree
    /// checked so far. Snapshotted around each probe and restored when the
    /// probed subtree is thrown away, so it counts holes in *retained* output
    /// — which is what lets a `match` nested inside a probed arm settle its
    /// own holes without forcing the outer arm to be re-checked as well.
    probe_holes: Cell<u32>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            reg: Registry::with_builtins(),
            ns: Vec::new(),
            file_ns: Vec::new(),
            loop_stack: RefCell::new(Vec::new()),
            throw_tags: RefCell::new(std::collections::BTreeMap::new()),
            redef_policy: RedefPolicy::default(),
            warnings: RefCell::new(Vec::new()),
            recover: false,
            errors: RefCell::new(Vec::new()),
            local_refs: RefCell::new(HashMap::new()),
            node_types: RefCell::new(HashMap::new()),
            expr_ty: None,
            generic_fn_templates: HashMap::new(),
            generic_method_templates: HashMap::new(),
            spec_memo: RefCell::new(HashSet::new()),
            spec_pending: RefCell::new(Vec::new()),
            type_var_bindings: BTreeMap::new(),
            type_uses: RefCell::new(Vec::new()),
            place_tmp_counter: Cell::new(0),
            predeclared: HashSet::new(),
            infer_probe: Cell::new(false),
            probe_holes: Cell::new(0),
        }
    }

    /// Every registry entry's content hash, as of now.
    ///
    /// The "before" half of [`Self::capture_delta`]: take one, load a unit,
    /// then ask what changed. Hashes rather than a copy of the state, because
    /// this is taken at startup on the path a dump exists to make fast — and
    /// hashes of what a table *would serialize to* is exactly the question the
    /// delta asks.
    pub fn signature(&self, heap: &Heap) -> Result<RegistrySignature, String> {
        let mut sig = crate::dump::signature(
            &self.reg.root,
            &self.reg.docs,
            &self.reg.struct_defaults,
            &self.throw_tags.borrow(),
            &self.sorted_predeclared(),
        )
?;
        self.visit_templates(heap, &mut |cat, key, _wire_bytes, hash| {
            sig.record(cat, key, hash);
            Ok(())
        })?;
        Ok(sig)
    }

    /// Every checker entry that is new or different since `before`.
    ///
    /// See [`crate::dump`] for why a unit carries its own additions rather
    /// than the whole world.
    pub fn capture_delta(&self, heap: &Heap, before: &RegistrySignature) -> Result<CheckerDelta, String> {
        let entries = crate::dump::delta(
            &self.reg.root,
            &self.reg.docs,
            &self.reg.struct_defaults,
            &self.throw_tags.borrow(),
            &self.sorted_predeclared(),
            before,
        )?;

        let mut fn_templates = Vec::new();
        let mut method_templates = Vec::new();
        self.visit_templates(heap, &mut |cat, key, bytes, hash| {
            if before.unchanged(cat, &key, hash) {
                return Ok(());
            }
            if cat == crate::dump::cat::FN_TEMPLATE {
                fn_templates.push((crate::dump::parse_path(&key), bincode_read(&bytes)?));
            } else {
                let (path, name) = key
                    .split_once('|')
                    .ok_or_else(|| format!("dump: `{}` is not a method key", key))?;
                method_templates
                    .push(((crate::dump::parse_path(path), name.to_string()), bincode_read(&bytes)?));
            }
            Ok(())
        })?;
        Ok(CheckerDelta { entries, fn_templates, method_templates })
    }

    /// Reinstates a unit's checker additions in `self`.
    ///
    /// Each rebuilt template form is pushed as a **permanent** root, matching
    /// what the definition-checking path does for a template it retains: a
    /// template must outlive any number of collections between here and its
    /// last instantiation, and nothing else refers to it.
    pub fn apply_delta(&mut self, delta: CheckerDelta, heap: &mut Heap) -> Result<(), String> {
        let mut predeclared = std::mem::take(&mut self.predeclared);
        crate::dump::apply_entries(
            &mut self.reg.root,
            &mut self.reg.docs,
            &mut self.reg.struct_defaults,
            &mut self.throw_tags.borrow_mut(),
            &mut |name| {
                predeclared.insert(name);
            },
            delta.entries,
        )?;
        self.predeclared = predeclared;

        let rebuild = |parts: &[crate::owned_form::OwnedForm], heap: &mut Heap| -> Result<Vec<Value>, String> {
            let mut out = Vec::with_capacity(parts.len());
            for p in parts {
                let v = crate::owned_form::owned_to_value(heap, p).map_err(|e| e.to_string())?;
                heap.push_permanent_root(v);
                out.push(v);
            }
            Ok(out)
        };
        for (path, t) in delta.fn_templates {
            let parts = rebuild(&t.parts, heap)?;
            self.generic_fn_templates
                .insert(path, FnTemplate { parts, ns: t.ns, type_params: t.type_params });
        }
        for (key, t) in delta.method_templates {
            let restored = match t {
                crate::dump::WireMethodTemplate::Form { parts, ns, written_vars } => {
                    MethodTemplate::Form { parts: rebuild(&parts, heap)?, ns, written_vars }
                }
                crate::dump::WireMethodTemplate::Getter { index } => MethodTemplate::Getter { index },
                crate::dump::WireMethodTemplate::Setter { index } => MethodTemplate::Setter { index },
            };
            self.generic_method_templates.insert(key, restored);
        }
        Ok(())
    }

    /// The pre-declared names in a fixed order, so two runs of the same load
    /// produce the same bytes.
    fn sorted_predeclared(&self) -> Vec<String> {
        let mut v: Vec<String> = self.predeclared.iter().cloned().collect();
        v.sort();
        v
    }

    /// Feeds every retained generic template to `visit` as `(category, key,
    /// serialized wire form, hash)`.
    ///
    /// The templates live on the `Checker` rather than in the `Registry`, so
    /// `dump::walk` cannot see them; this is their half of the same walk, and
    /// both [`Self::signature`] and [`Self::capture_delta`] go through it for
    /// the same reason `dump::walk` is written once.
    fn visit_templates(
        &self,
        heap: &Heap,
        visit: &mut dyn FnMut(u8, String, Vec<u8>, u64) -> Result<(), String>,
    ) -> Result<(), String> {
        let owned = |parts: &[Value]| -> Result<Vec<crate::owned_form::OwnedForm>, String> {
            parts
                .iter()
                .map(|v| crate::owned_form::value_to_owned(heap, *v).map_err(|e| e.to_string()))
                .collect()
        };

        let mut fns: Vec<(&Path, &FnTemplate)> = self.generic_fn_templates.iter().collect();
        fns.sort_by_key(|(p, _)| p.to_string());
        for (path, t) in fns {
            let w = crate::dump::WireFnTemplate {
                parts: owned(&t.parts)?,
                ns: t.ns.clone(),
                type_params: t.type_params.clone(),
            };
            let (bytes, hash) = crate::dump::wire(&w)?;
            visit(crate::dump::cat::FN_TEMPLATE, path.to_string(), bytes, hash)?;
        }

        let mut methods: Vec<(&(Path, String), &MethodTemplate)> =
            self.generic_method_templates.iter().collect();
        methods.sort_by_key(|(k, _)| (k.0.to_string(), k.1.clone()));
        for (key, t) in methods {
            let w = match t {
                MethodTemplate::Form { parts, ns, written_vars } => crate::dump::WireMethodTemplate::Form {
                    parts: owned(parts)?,
                    ns: ns.clone(),
                    written_vars: written_vars.clone(),
                },
                MethodTemplate::Getter { index } => crate::dump::WireMethodTemplate::Getter { index: *index },
                MethodTemplate::Setter { index } => crate::dump::WireMethodTemplate::Setter { index: *index },
            };
            let (bytes, hash) = crate::dump::wire(&w)?;
            visit(crate::dump::cat::METHOD_TEMPLATE, format!("{}|{}", key.0, key.1), bytes, hash)?;
        }
        Ok(())
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

    /// Drains and returns every user-defined type/trait name occurrence
    /// recorded so far (see [`Self::type_uses`]) — the LSP turns these into
    /// `textDocument/semanticTokens` (`crate::check::semantic`).
    pub fn take_type_uses(&self) -> Vec<TypeUse> {
        std::mem::take(&mut *self.type_uses.borrow_mut())
    }

    /// How `ty`'s values are represented at runtime — the only residue of the
    /// type system the lowered form carries (see [`Repr`]).
    ///
    /// Resolved through the live [`Registry`] rather than a pair of frozen
    /// struct/enum sets: a file's own `defstruct` is registered as that file is
    /// checked, so a set snapshotted at entry would answer `None` for a type
    /// defined a few forms earlier.
    fn repr(&self, ty: &Type) -> Repr {
        Repr::of_by(ty, &|p| self.reg.type_def(p).map(|d| d.kind))
    }

    /// [`Self::repr`], written as the core IR spells it — for the repr
    /// positions in a lowered node (a `let` binding, a call argument, a struct
    /// field access).
    fn repr_form(&self, heap: &mut Heap, ty: &Type) -> Result<Value, Error> {
        self.repr(ty).write(heap)
    }

    /// One repr per type, as the parallel list every argument/parameter
    /// position uses.
    fn repr_forms(&self, heap: &mut Heap, tys: impl IntoIterator<Item = Type>) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        for ty in tys {
            let r = self.repr(&ty).write(f.heap())?;
            f.push(r);
        }
        f.finish_list()
    }

    // ---- core-form builders -------------------------------------------
    //
    // One function per tag in `tests/core_vocabulary_test.rs`, so the shape a
    // node is built in is written down exactly once and the reader who wants to
    // know what `(assoc ...)`'s fields are can find the answer next to the
    // consumers' own readers. Every one of them builds through `core::Items`,
    // which keeps each finished field rooted until the node is assembled — see
    // `check::core`'s module doc comment for why that is not optional.



    /// `(global (WRITTEN...) (HOME...) PATH REPR)`.
    ///
    /// Three names, because a global reference is three separate facts: what
    /// was written at the reference site, the module it was written in, and the
    /// absolute path the checker resolved it to. The evaluator re-derives the
    /// target from the first two (see [`Ref`]); the compile pipeline, which has
    /// no way to redo resolution, reads the third.
    ///
    /// **Returns a rooted form**, for the same reason [`Self::var_form`] does —
    /// see there.
    fn global_form(&self, heap: &mut Heap, r: &Ref, ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let written = forms::sym_list(f.heap(), &r.written)?;
        f.push(written);
        let home = forms::sym_list(f.heap(), &r.home)?;
        f.push(home);
        let path = forms::path_form(f.heap(), &r.resolved);
        f.push(path);
        let repr = self.repr_form(f.heap(), ty)?;
        f.push(repr);
        let form = f.finish("global")?;
        Ok(forms::rooted(heap, form))
    }

    /// `(set-global (WRITTEN...) (HOME...) PATH REPR FORM)` — see
    /// [`Self::global_form`] for the name triple.
    fn set_global_form(&self, heap: &mut Heap, r: &Ref, ty: &Type, value: Value) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let written = forms::sym_list(f.heap(), &r.written)?;
        f.push(written);
        let home = forms::sym_list(f.heap(), &r.home)?;
        f.push(home);
        let path = forms::path_form(f.heap(), &r.resolved);
        f.push(path);
        let repr = self.repr_form(f.heap(), ty)?;
        f.push(repr);
        f.push(value);
        f.finish("set-global")
    }

    /// `(let ((SYM REPR FORM)...) BODY...)`.
    ///
    /// Also the only sequencing node: `progn` is `(let () ...)`, a `let` with
    /// no bindings, so there is one shape for "run these in order" rather than
    /// two.
    fn let_form(&self, heap: &mut Heap, binds: &[(String, Type, Value)], body: &[Value]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut bs = Items::new(f.heap());
        for (name, ty, val) in binds {
            let sym = bs.heap().intern_symbol(name);
            let repr = self.repr_form(bs.heap(), ty)?;
            let one = core::list(bs.heap(), &[sym, repr, *val])?;
            bs.push(one);
        }
        let binds = bs.finish_list()?;
        f.push(binds);
        f.extend(body.iter().copied());
        f.finish("let")
    }


    /// `(call (WRITTEN...) (HOME...) PATH (REPR...) ARG...)` — a call to a free
    /// function.
    ///
    /// The representations are the *arguments'* own, one per argument in the
    /// same order: the island reads them to decide the GC-root bookkeeping
    /// around each argument as it crosses into a compiled frame.
    fn call_form(&self, heap: &mut Heap, r: &Ref, args: &[Checked]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let written = forms::sym_list(f.heap(), &r.written)?;
        f.push(written);
        let home = forms::sym_list(f.heap(), &r.home)?;
        f.push(home);
        let path = forms::path_form(f.heap(), &r.resolved);
        f.push(path);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish("call")
    }

    /// `(assoc PATH SYM INSTANCE (HOME...) RET-REPR (REPR...) ARG...)` — an
    /// instance method (`args[0]` is the receiver) or a static associated
    /// function.
    ///
    /// `ret` is here for the receiver-less case: with no receiver to classify,
    /// the result is the only thing that says whether `(vector::new)` is a
    /// container operation the island open-codes or an ordinary call.
    fn assoc_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        home: &[String],
        ret: &Type,
        args: &[Checked],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = forms::path_form(f.heap(), type_name);
        f.push(ty_path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        f.push(Value::Bool(instance));
        let home = forms::sym_list(f.heap(), home)?;
        f.push(home);
        let ret = self.repr_form(f.heap(), ret)?;
        f.push(ret);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish("assoc")
    }

    /// `(fnref (WRITTEN...) (HOME...) PATH (PARAM-REPR...))` — a named free
    /// function reified as a value.
    ///
    /// The parameter representations are carried because the bridge turns this
    /// into a closure construction, and there is no call site here to read them
    /// from. A `&rest` parameter is simply the last entry (always `sexpr`), so
    /// it needs no flag of its own.
    fn fnref_form(&self, heap: &mut Heap, r: &Ref, params: &[Type]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let written = forms::sym_list(f.heap(), &r.written)?;
        f.push(written);
        let home = forms::sym_list(f.heap(), &r.home)?;
        f.push(home);
        let path = forms::path_form(f.heap(), &r.resolved);
        f.push(path);
        let reprs = self.repr_forms(f.heap(), params.to_vec())?;
        f.push(reprs);
        f.finish("fnref")
    }

    /// `(methodref PATH SYM (HOME...) (PARAM-REPR...))` — see
    /// [`Self::fnref_form`] for the representation list.
    fn methodref_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        home: &[String],
        params: &[Type],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ty_path = forms::path_form(f.heap(), type_name);
        f.push(ty_path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        let home = forms::sym_list(f.heap(), home)?;
        f.push(home);
        let reprs = self.repr_forms(f.heap(), params.to_vec())?;
        f.push(reprs);
        f.finish("methodref")
    }

    /// `(apply CALLEE RET-REPR (REPR...) ARG...)` — applying a function *value*.
    ///
    /// The return representation is here for the same reason `assoc` carries
    /// one: the callee may be a *compiled* closure, and a value coming back out
    /// of compiled code has to be decoded by its declared representation, never
    /// by its shape. Unlike a `call`, there is no name to look a signature up
    /// by — the callee is a value — so the call site is the only place that
    /// knows. The island ignores the field (`apply`/`apply-indirect` take
    /// argument pairs only).
    fn apply_form(&self, heap: &mut Heap, callee: Value, ret: &Type, args: &[Checked]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(callee);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish("apply")
    }

    /// The declared field types of `adt`'s variant `variant`, with `args`
    /// substituted for the definition's own type parameters — the instantiated
    /// field types [`Self::construct_form`] takes.
    ///
    /// For a non-generic type (`Sexpr`, an error type) `args` is empty and this
    /// is just the declaration. For a generic one it is the substitution that
    /// makes the answer meaningful at all — see `construct_form`'s doc comment.
    fn variant_field_tys(&self, adt: &Path, variant: usize, args: &[Type]) -> Vec<Type> {
        let Some(def) = self.reg.type_def(adt) else { return Vec::new() };
        let Some(v) = def.variants.get(variant) else { return Vec::new() };
        let subst: BTreeMap<String, Type> =
            def.params.iter().cloned().zip(args.iter().cloned()).collect();
        v.fields.iter().map(|t| subst_apply(t, &subst)).collect()
    }

    /// `(construct PATH VARIANT MUTABLE (REPR...) ARG...)`.
    ///
    /// `field_tys` are the variant's *declared* field types with this site's
    /// type arguments already substituted in — not the argument expressions'
    /// own types, which can be narrower (a `!`-typed argument, a `Symbol` value
    /// widening into a `Sexpr` field). Which of the two decides the shape a
    /// field is stored in is settled: the declaration does, on both sides of
    /// every boundary.
    ///
    /// The representations travel with the *site* rather than being read back
    /// from the type's own `defstruct`/`defenum`, because for a generic ADT the
    /// definition cannot answer the question: `Option`'s `Some` field is
    /// declared `T`, and `T` has no representation. The instantiation is known
    /// only here. A definition-keyed table cannot be patched to cover it
    /// either — monomorphization erases, so `Maybe<i64>` and `Maybe<string>`
    /// are both at the path `Maybe` and only one set of fields could be
    /// recorded there. (Getting this wrong emitted field kind `0` and the
    /// island aborted with `compile-sexpr-field: field type is not
    /// representable in compiled code yet`.)
    fn construct_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        variant: usize,
        mutable: bool,
        field_tys: &[Type],
        args: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), type_name);
        f.push(path);
        f.push(Value::Int(variant as i64));
        f.push(Value::Bool(mutable));
        let reprs = self.repr_forms(f.heap(), field_tys.iter().cloned())?;
        f.push(reprs);
        f.extend(args.iter().copied());
        f.finish("construct")
    }

    /// `(field-get OBJ IDX REPR)`.
    ///
    /// Unlike `construct` this *does* spell out a representation: it names only
    /// an index, and the object is an arbitrary expression whose type is
    /// exactly what the lowered form no longer carries.
    fn field_get_form(&self, heap: &mut Heap, obj: Value, idx: usize, field_ty: &Type) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(obj);
        f.push(Value::Int(idx as i64));
        let repr = self.repr_form(f.heap(), field_ty)?;
        f.push(repr);
        f.finish("field-get")
    }

    /// `(field-set OBJ IDX REPR VALUE)` — see [`Self::field_get_form`].
    fn field_set_form(
        &self,
        heap: &mut Heap,
        obj: Value,
        idx: usize,
        field_ty: &Type,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(obj);
        f.push(Value::Int(idx as i64));
        let repr = self.repr_form(f.heap(), field_ty)?;
        f.push(repr);
        f.push(value);
        f.finish("field-set")
    }

    /// `(match SCRUT REPR (PAT BODY...)...)`.
    ///
    /// The scrutinee's representation is what a whole-value `(pat-bind x)`
    /// binds at, since such a pattern names no type of its own.
    fn match_form(
        &self,
        heap: &mut Heap,
        scrut: Value,
        scrut_ty: &Type,
        arms: &[(Value, Vec<Value>)],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        f.push(scrut);
        let repr = self.repr_form(f.heap(), scrut_ty)?;
        f.push(repr);
        for (pat, body) in arms {
            let mut a = Items::new(f.heap());
            a.push(*pat);
            a.extend(body.iter().copied());
            let arm = a.finish_list()?;
            f.push(arm);
        }
        f.finish("match")
    }

    /// `(pat-wild)` / `(pat-bind SYM)` / `(pat-lit LITERAL)` /
    /// `(pat-guard SYM TEST)` /
    /// `(pat-ctor PATH VARIANT DOWNCAST SUB...)` /
    /// `(pat-typetest PATH SUB)`.
    ///
    /// A constructor pattern carries its type, its variant index and whether it
    /// is a downcast — and nothing about its fields' representations. The AST's
    /// `Pattern::Ctor` recorded `sexpr_fields` and `field_types` per pattern
    /// site; a field's representation is a property of the *type*, so the
    /// bridge reads it from that type's own `defstruct`/`defenum` form and the
    /// same fact is recorded once instead of once per site. `Pattern::Bind`'s
    /// inert `bool` disappears here for the same reason it existed —
    /// nothing read it.
    fn pattern_form(&self, heap: &mut Heap, pat: &Pattern) -> Result<Value, Error> {
        let form = match pat {
            Pattern::Wildcard => core::tagged(heap, "pat-wild", &[])?,
            Pattern::Bind(name) => {
                let sym = heap.intern_symbol(name);
                core::tagged(heap, "pat-bind", &[sym])?
            }
            Pattern::Int(n) => {
                let lit = core::tagged(heap, "int", &[Value::Int(*n)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, "pat-lit", &[lit])?
            }
            Pattern::Bool(b) => {
                let lit = core::tagged(heap, "bool", &[Value::Bool(*b)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, "pat-lit", &[lit])?
            }
            Pattern::Char(c) => {
                let lit = core::tagged(heap, "char", &[Value::Char(*c)])?;
                let mut s = RootScope::new(heap);
                s.push_root(lit);
                core::tagged(&mut s, "pat-lit", &[lit])?
            }
            Pattern::Guard { name, form } => {
                // The test form is already lowered — it came out of
                // `check_at` — so this only wraps it. It is rooted where it
                // was built (`check_at` pushes every node it produces onto
                // the heap's root stack, and `check_form_at` is what
                // truncates that stack), which is what lets it sit in a
                // `Pattern` on the Rust side across the allocations its
                // siblings make.
                let sym = heap.intern_symbol(name);
                core::tagged(heap, "pat-guard", &[sym, *form])?
            }
            // `Option<Sexpr>`'s two shapes (`check_ctor_pattern_fields`).
            // `some` needs no test of its own: under the niche the unwrapped
            // value *is* the same word, and every `Sexpr` variant test
            // already excludes the empty-list immediate — so a `(some (int
            // n))` arm is the `int` test alone. `pat-nonempty` exists for the
            // case where the sub-pattern tests nothing (`(some x)`), which
            // must still reject the empty list.
            Pattern::Empty => core::tagged(heap, "pat-empty", &[])?,
            Pattern::NonEmpty(sub) => {
                let inner = self.pattern_form(heap, sub)?;
                core::tagged(heap, "pat-nonempty", &[inner])?
            }
            // The empty list is its own pattern node, not `Sexpr`'s variant 0.
            // Today the two are the same test; they stop being the same when
            // `nil` leaves `Sexpr` and the empty list becomes `Option<Sexpr>`'s
            // `none` (docs/dev/null-elimination-plan.md §3.2.1) — at which
            // point there is no variant 0 to name, but there is still an empty
            // list to test for. Splitting the node now is what lets that
            // change touch only the checker's choice of node, not the two
            // consumers that compile one.
            Pattern::Ctor { type_name, variant: 0, args, downcast: false, .. }
                if args.is_empty() && crate::types::path_is_builtin(type_name, "sexpr") =>
            {
                core::tagged(heap, "pat-empty", &[])?
            }
            Pattern::Ctor { type_name, variant, args, field_types, downcast, .. } => {
                let mut f = Items::new(heap);
                let tp = forms::path_form(f.heap(), type_name);
                f.push(tp);
                f.push(Value::Int(*variant as i64));
                f.push(Value::Bool(*downcast));
                // Per-field representations, for the same reason
                // `construct` carries them — see `construct_form`.
                let reprs = self.repr_forms(f.heap(), field_types.iter().cloned())?;
                f.push(reprs);
                for sub in args {
                    let one = self.pattern_form(f.heap(), sub)?;
                    f.push(one);
                }
                f.finish("pat-ctor")?
            }
            Pattern::TypeTest(ty, sub) => {
                // The ADT path only: a heap box's `type_name` never encodes
                // type arguments, so the runtime test cannot see them either.
                let path = match ty {
                    Type::Named(p, _) => p.clone(),
                    other => prim_type_path(other).ok_or_else(|| {
                        Error::TypeError(format!("(the {:?} pattern): not a named type", other))
                    })?,
                };
                let mut f = Items::new(heap);
                let tp = forms::path_form(f.heap(), &path);
                f.push(tp);
                let one = self.pattern_form(f.heap(), sub)?;
                f.push(one);
                f.finish("pat-typetest")?
            }
        };
        Ok(forms::rooted(heap, form))
    }

    /// `(lambda ((SYM REPR)...) RET-REPR BODY...)`.
    fn lambda_form(
        &self,
        heap: &mut Heap,
        params: &[(String, Type)],
        ret: &Type,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.extend(body.iter().copied());
        f.finish("lambda")
    }

    /// `((SYM REPR)...)` — a parameter list, shared by `lambda`/`labels` and
    /// the top-level definition forms.
    fn param_list_form(&self, heap: &mut Heap, params: &[(String, Type)]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        for (name, ty) in params {
            let sym = f.heap().intern_symbol(name);
            let repr = self.repr_form(f.heap(), ty)?;
            let one = core::list(f.heap(), &[sym, repr])?;
            f.push(one);
        }
        f.finish_list()
    }

    /// `(labels ((SYM ((SYM REPR)...) RET-REPR BODY...)...) BODY...)` —
    /// mutually recursive local functions, each of the same shape a `lambda`
    /// has plus a name.
    fn labels_form(
        &self,
        heap: &mut Heap,
        defs: &[(String, Vec<(String, Type)>, Type, Vec<Value>)],
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let mut ds = Items::new(f.heap());
        for (name, params, ret, def_body) in defs {
            let mut d = Items::new(ds.heap());
            let sym = d.heap().intern_symbol(name);
            d.push(sym);
            let ps = self.param_list_form(d.heap(), params)?;
            d.push(ps);
            let r = self.repr_form(d.heap(), ret)?;
            d.push(r);
            d.extend(def_body.iter().copied());
            let one = d.finish_list()?;
            ds.push(one);
        }
        let defs = ds.finish_list()?;
        f.push(defs);
        f.extend(body.iter().copied());
        f.finish("labels")
    }

    /// `(dyn-new "KEY" TRAIT ((TYPE SYM)...) ((TRAIT ((TYPE SYM)...))...) REPR
    /// FORM)` — boxing a concrete value as a trait object.
    ///
    /// The trailing representation is the boxed value's own, and it cannot be
    /// derived from the concrete type's *name* (all the rest of the node
    /// carries): a struct and an enum differ in whether the island has to root
    /// the value across the boxing call.
    fn dyn_new_form(
        &self,
        heap: &mut Heap,
        concrete_key: &str,
        trait_path: &Path,
        slots: &[(Path, String)],
        supers: &[(Path, Vec<(Path, String)>)],
        value_ty: &Type,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let key = f.heap().alloc_string(concrete_key.to_string());
        f.push(key);
        let tp = forms::path_form(f.heap(), trait_path);
        f.push(tp);
        let slot_list = vtable_form(f.heap(), slots)?;
        f.push(slot_list);
        let mut sup = Items::new(f.heap());
        for (trait_path, slots) in supers {
            let tp = forms::path_form(sup.heap(), trait_path);
            let mut one = Items::new(sup.heap());
            one.push(tp);
            let sl = vtable_form(one.heap(), slots)?;
            one.push(sl);
            let entry = one.finish_list()?;
            sup.push(entry);
        }
        let supers = sup.finish_list()?;
        f.push(supers);
        let repr = self.repr_form(f.heap(), value_ty)?;
        f.push(repr);
        f.push(value);
        f.finish("dyn-new")
    }











    /// `(dyn-call TRAIT SYM SLOT ((TYPE SYM)...) (REPR...) ARG...)` — a call
    /// through a trait object's vtable, `args[0]` the receiver.
    ///
    /// The implementation list is not what dispatches — that is the vtable's
    /// job, and the point is that the target is unknown until run time — but it
    /// tells the compiler which bodies must exist natively before this call
    /// site can run natively.
    fn dyn_call_form(
        &self,
        heap: &mut Heap,
        trait_path: &Path,
        method: &str,
        slot: usize,
        impl_targets: &[(Path, String)],
        args: &[Checked],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let tp = forms::path_form(f.heap(), trait_path);
        f.push(tp);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        f.push(Value::Int(slot as i64));
        let targets = vtable_form(f.heap(), impl_targets)?;
        f.push(targets);
        let reprs = self.repr_forms(f.heap(), args.iter().map(|a| a.ty.clone()))?;
        f.push(reprs);
        f.extend(args.iter().map(|a| a.form));
        f.finish("dyn-call")
    }


    // ---- top-level form builders --------------------------------------

    /// `(defun PATH ((SYM REPR)...) RET-REPR PUBLIC BODY...)`.
    ///
    /// `public` is baked in so `eval::scope`'s qualified-path resolution can
    /// enforce visibility without the checker's `Registry` at runtime.
    fn defun_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        params: &[(String, Type)],
        ret: &Type,
        public: bool,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.push(Value::Bool(public));
        f.extend(body.iter().copied());
        f.finish("defun")
    }

    /// [`Self::defun_form`] unless this `defun` is *generic*, in which case an
    /// empty `(module PATH)`.
    ///
    /// A generic definition has no core form (see [`TopLevelForm`]): its body
    /// was checked with the type variables still abstract, for
    /// definition-time diagnostics only, and every call site was rewritten to
    /// a monomorphized specialization. Emitting it would leave a silently
    /// callable stale twin behind — which is why the old evaluator opened with
    /// a `type_params.is_empty()` guard on both definition arms. Emitting
    /// nothing instead removes the guard *and* the reason for it: an empty
    /// `module` is already the "this defined a name, there is nothing to run"
    /// form (`deftrait`/`impl` produce one), so no new vocabulary is needed and
    /// nothing downstream has to recognize a body it must refuse to register.
    fn definition_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        type_params: &[String],
        params: &[(String, Type)],
        ret: &Type,
        public: bool,
        body: &[Value],
    ) -> Result<Value, Error> {
        if !type_params.is_empty() {
            return forms::module_form(heap, name, &[]);
        }
        self.defun_form(heap, name, params, ret, public, body)
    }

    /// [`Self::defmethod_form`]'s counterpart of [`Self::definition_form`] —
    /// same reasoning, for a generic *owner*'s method.
    #[allow(clippy::too_many_arguments)]
    fn method_definition_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        type_params: &[String],
        params: &[(String, Type)],
        ret: &Type,
        public: bool,
        body: &[Value],
    ) -> Result<Value, Error> {
        if !type_params.is_empty() {
            return forms::module_form(heap, type_name, &[]);
        }
        self.defmethod_form(heap, type_name, method, instance, params, ret, public, body)
    }

    /// `(defmethod PATH SYM INSTANCE ((SYM REPR)...) RET-REPR PUBLIC BODY...)`.
    ///
    /// The receiver is the first parameter for an instance method, not a field
    /// of its own the way `TopLevel::Defmethod::self_name` was: `params` is
    /// what the body binds, in order, and `instance` says whether the first
    /// entry is the receiver.
    fn defmethod_form(
        &self,
        heap: &mut Heap,
        type_name: &Path,
        method: &str,
        instance: bool,
        params: &[(String, Type)],
        ret: &Type,
        public: bool,
        body: &[Value],
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), type_name);
        f.push(path);
        let m = f.heap().intern_symbol(method);
        f.push(m);
        f.push(Value::Bool(instance));
        let ps = self.param_list_form(f.heap(), params)?;
        f.push(ps);
        let r = self.repr_form(f.heap(), ret)?;
        f.push(r);
        f.push(Value::Bool(public));
        f.extend(body.iter().copied());
        f.finish("defmethod")
    }


    /// `(defvar PATH REPR MUTABLE PUBLIC FORM)` — a global variable
    /// (`mutable`) or constant.
    fn defvar_form(
        &self,
        heap: &mut Heap,
        name: &Path,
        ty: &Type,
        mutable: bool,
        public: bool,
        value: Value,
    ) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let r = self.repr_form(f.heap(), ty)?;
        f.push(r);
        f.push(Value::Bool(mutable));
        f.push(Value::Bool(public));
        f.push(value);
        f.finish("defvar")
    }

    /// `(defstruct PATH (REPR...))` — the type's field representations, which
    /// is where the bridge reads a `construct`'s and a pattern's field kinds
    /// from.
    fn defstruct_form(&self, heap: &mut Heap, name: &Path, fields: &[Type]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let reprs = self.repr_forms(f.heap(), fields.to_vec())?;
        f.push(reprs);
        f.finish("defstruct")
    }

    /// `(defenum PATH (SYM...) ((REPR...)...))` — the variant names, then each
    /// variant's field representations.
    fn defenum_form(&self, heap: &mut Heap, name: &Path, variants: &[Variant]) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let path = forms::path_form(f.heap(), name);
        f.push(path);
        let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
        let names = forms::sym_list(f.heap(), &names)?;
        f.push(names);
        let mut vs = Items::new(f.heap());
        for v in variants {
            let reprs = self.repr_forms(vs.heap(), v.fields.to_vec())?;
            vs.push(reprs);
        }
        let per_variant = vs.finish_list()?;
        f.push(per_variant);
        f.finish("defenum")
    }







    /// The single catch helper used at expression-level recovery boundaries: a
    /// pass-through in strict mode, and in `recover` mode it records a failing
    /// sub-check's error (tagged with `loc` if it lacks a more specific one)
    /// and substitutes a [`Self::hole`] so checking continues.
    fn recovered(&self, heap: &mut Heap, r: Result<Checked, Error>, loc: Option<Loc>) -> Result<Checked, Error> {
        match r {
            Err(e) if self.recover => {
                self.push_recovered(e, loc.clone());
                forms::hole(heap, loc)
            }
            other => other,
        }
    }

    /// Records a recoverable error at a boundary that has no form slot to
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

    /// Where the side tables a check writes to stand right now. Paired with
    /// [`Self::rollback_to`] around a check whose result may be thrown away
    /// ([`Self::check_match`]'s arm probe), so a discarded attempt leaves
    /// behind neither a diagnostic, nor a duplicate semantic token, nor a
    /// queued instantiation of a type only the discarded attempt believed in
    /// (a probe can type a value as `Result<string, !>`, and that must not
    /// reach [`Self::drain_specializations`] as something to generate).
    ///
    /// `local_refs` is the one table left alone: it is keyed by source
    /// position, so a re-check simply writes the same entries again.
    fn check_mark(&self) -> CheckMark {
        CheckMark {
            errors: self.errors.borrow().len(),
            warnings: self.warnings.borrow().len(),
            type_uses: self.type_uses.borrow().len(),
            spec_pending: self.spec_pending.borrow().len(),
            spec_memo: self.spec_memo.borrow().clone(),
        }
    }

    /// Undo everything recorded since [`Self::check_mark`].
    fn rollback_to(&self, mark: CheckMark) {
        self.errors.borrow_mut().truncate(mark.errors);
        self.warnings.borrow_mut().truncate(mark.warnings);
        self.type_uses.borrow_mut().truncate(mark.type_uses);
        self.spec_pending.borrow_mut().truncate(mark.spec_pending);
        *self.spec_memo.borrow_mut() = mark.spec_memo;
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

    /// Types and traits share one name space per module, as in Rust: a
    /// `defstruct`/`defenum` and a `deftrait` in the same namespace may not
    /// carry the same name, in either definition order. They live in
    /// *separate* tables (`Namespace::types` / `Namespace::traits`, which is
    /// why nothing stops them mechanically — a `Type::Dyn`'s head is resolved
    /// against `traits`, a `Type::Named`'s against `types`), so this is the
    /// one place the rule is enforced. Without it `Foo` in type position and
    /// `:dyn Foo` would silently name two unrelated definitions.
    ///
    /// Called by both sides: `kind` is what is being defined now, and the
    /// message points at the other table's occupant.
    fn check_type_trait_clash(&self, kind: &str, name: &str) -> Result<(), Error> {
        let (clashes, other) = match kind {
            "trait" => (self.cur_ns().types.contains_key(name), "type"),
            _ => (self.cur_ns().traits.contains_key(name), "trait"),
        };
        if clashes {
            return Err(Error::TypeError(format!(
                "cannot define {} `{}`: a {} of that name already exists here — types and traits share one name space (write them under different modules if both are needed)",
                kind, name, other
            )));
        }
        // A `deftype` alias occupies the same name space as both: it is a
        // spelling *for* a type, so a later `defstruct`/`defenum`/`deftrait`
        // of that name would make the same written name mean two things
        // depending on which registration a lookup consulted first.
        if kind != "type alias" && self.cur_ns().type_aliases.contains_key(name) {
            return Err(Error::TypeError(format!(
                "cannot define {} `{}`: a `deftype` alias of that name already exists here",
                kind, name
            )));
        }
        Ok(())
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
    pub fn check_form(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value) -> Result<TopLevelForm, Error> {
        self.check_form_at(heap, interp, v, None)
    }

    /// [`Self::check_form`] with a caller-supplied location for `v` itself.
    /// A list form carries its own recorded location (`heap.cons_loc`) and
    /// ignores the hint, but a *bare atom* at top level (e.g. a lone `42`)
    /// has no heap identity to key a location on — the reader hands its span
    /// alongside the value (`Reader::read_all_in_spanned`) and this is where
    /// it enters the checker. Mirrors [`Self::check_at`] vs [`Self::check`].
    pub fn check_form_at(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value, loc_hint: Option<Loc>) -> Result<TopLevelForm, Error> {
        debug_assert!(
            self.spec_pending.borrow().is_empty(),
            "specialization requests must never leak across check_form calls"
        );
        // Location of this top-level form, attached to any error that lacks a
        // more specific one (a body-expression error already carries the
        // deeper location from `check`, and `Error::at` keeps that innermost
        // one — see its doc comment).
        let loc = heap.cons_loc(v).or(loc_hint);
        // Where the root stack stood before any of this form's nodes were
        // built. `check_at` roots every node it produces and pops none of them
        // (see there for why); this is the single matching release, taken on
        // every exit path below including the error ones.
        let roots_base = heap.root_count();
        // Belongs to *this* form: a definition must not report the previous
        // form's expression type — see [`Self::expr_type`].
        self.expr_ty = None;
        let primary = self.check_form_dispatch(heap, interp, v, loc.clone());
        // Rooted before the drain below, not after: `drain_specializations`
        // re-checks a whole template per instantiation, so it allocates heavily,
        // and the top-level form builders (`defun_form`, `expr_form`, ...) hand
        // back an unrooted node the way every core builder does. Held only in a
        // Rust local, the primary form would be collected out from under the
        // bundle — and, with the cell recycled, the bundle would end up holding a
        // *fragment* of it. Released with everything else by `truncate_roots`.
        let primary = primary.map(|tl| forms::rooted(heap, tl));
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
        self.reg.def_locs.node_types.extend(self.node_types.borrow_mut().drain());
        let result = match bundled {
            Ok((specs, tl)) if specs.is_empty() => Ok(tl),
            Ok((mut specs, tl)) => {
                specs.push(tl);
                forms::module_form(heap, &Path::root(MONO_BUNDLE_MODULE), &specs)
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
        let result = match result {
            Err(e) if self.recover => {
                self.spec_pending.borrow_mut().clear();
                self.errors.borrow_mut().push(e);
                let hole = forms::hole(heap, loc)?;
                forms::expr_form(heap, hole.form)
            }
            other => other,
        };
        // The returned form is deliberately *not* left rooted: it belongs to
        // the caller now, and every driver hands it straight to `Interp::exec`
        // (which roots it) or to a `fasl` writer. A caller that allocates in
        // between must root it first.
        heap.truncate_roots(roots_base);
        result
    }

    /// `(defsignature name (T...) Ret)` — a forward declaration.
    ///
    /// Top-level `defun`s are checked and executed one form at a time, in
    /// source order, so a body can only call a name the checker has already
    /// seen. Mutual recursion therefore needs the signature said ahead of the
    /// definition, and this is where it is said. It replaces the implicit
    /// pre-pass this checker used to run over a whole file before checking
    /// any of it (`predeclare_program`, 2026-08-01): that pass required
    /// reading every form before checking the first, which is exactly what a
    /// reader macro cannot allow — the reader has to be able to run a form
    /// before reading the next one.
    ///
    /// The declaration is *checked*, not merely recorded: the definition that
    /// follows must agree with it (see `check_defun_fixed`), and a
    /// declaration with no definition is reported when the unit finishes
    /// (see [`Self::finish_unit`]). That is the difference from the old
    /// implicit pass, which could only ever add a name and never reject one.
    ///
    /// Parameters are types alone — there is no body here for a name to mean
    /// anything to.
    ///
    /// # What cannot be declared
    ///
    /// - **Type parameters.** A generic `defun` is instantiated from its
    ///   retained source form (`request_fn_specialization` consults the
    ///   template map first), and a declaration has no body to retain. A
    ///   forward call would resolve and then fail to instantiate, so the
    ///   declaration is rejected here instead.
    /// - **`&optional`/`&key`.** Their `FnSig` carries each defaulted
    ///   parameter's *checked* default expression, which a call site that
    ///   omits the argument splices in verbatim; a declaration has nowhere to
    ///   put one, and registering the signature without them would let a
    ///   forward call see an incomplete one.
    /// - **Anything but `defun`.** `defmacro` needs its body `exec`'d, not
    ///   merely registered, before it can expand. A type's registration *is*
    ///   what the code registering it needs, so a type is a genuinely harder
    ///   case than a function; `defmethod` follows from that, since a method
    ///   registers into its owner's `TypeDef`.
    fn check_defsignature(
        &mut self,
        heap: &mut Heap,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
    ) -> Result<TopLevelForm, Error> {
        if parts.len() != 3 {
            return Err(Error::TypeError("defsignature: (defsignature name (param-type...) return-type)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        if !type_params.is_empty() {
            return Err(Error::TypeError(format!(
                "defsignature: `{}` takes type parameters, and a generic function cannot be forward-declared — \
                 instantiating one needs its body, which a declaration does not have. Define it before its callers.",
                name
            )));
        }
        let (params, rest) = self.parse_signature_params(heap, parts[1])?;
        let ret = self.parse_type_here_at(heap, parts[2], parts_locs.get(2).and_then(|l| l.as_ref()))?;

        let fq_name = self.fq(&name);
        if self.predeclared.contains(&fq_name.to_string()) {
            return Err(Error::TypeError(format!("defsignature: `{}` is already declared", name)));
        }
        // A declaration *after* the definition is not a redefinition to warn
        // about — it is a declaration that can never do anything, and saying
        // so plainly beats the "no definition in this file" the end-of-unit
        // check would otherwise report about a name that is plainly defined.
        // The redefinition path still runs for the built-in case, which has
        // its own message.
        if let Some(existing) = self.cur_ns().fns.get(&name) {
            if existing.builtin {
                self.check_redef("function", &name, Some(existing))?;
            }
            return Err(Error::TypeError(format!(
                "defsignature: `{}` is already defined — a declaration has to come before the definition it announces",
                name
            )));
        }

        let sig = FnSig {
            type_params: Vec::new(),
            params,
            ret,
            public,
            rest,
            builtin: false,
            bounds: BTreeMap::new(),
            optionals: Vec::new(),
            keys: Vec::new(),
        };
        self.reg.root.module_mut(&self.ns).fns.insert(name, sig);
        self.predeclared.insert(fq_name.to_string());
        forms::defsignature_form(heap)
    }

    /// A `defsignature`'s parameter list: bare types, with an optional
    /// trailing `&rest T`. The `(name type)` pairs `defun` writes have no
    /// counterpart here — see [`Self::check_defsignature`].
    fn parse_signature_params(&self, heap: &Heap, v: Value) -> Result<(Vec<Type>, Option<Type>), Error> {
        let elems_locs = heap.list_to_vec_locs(v)?;
        let is_rest_marker =
            |p: &Value| matches!(p, Value::Symbol(id) if *id == SymId::REST);
        if elems_locs.iter().any(|(p, _)| {
            matches!(p, Value::Symbol(id) if *id == SymId::OPTIONAL || *id == SymId::KEY)
        }) {
            return Err(Error::TypeError(
                "defsignature: `&optional`/`&key` cannot be forward-declared — a declaration has nowhere to put \
                 their default expressions, which call sites splice in verbatim. Define such a function before its callers."
                    .into(),
            ));
        }
        let rest_at = elems_locs.iter().position(|(p, _)| is_rest_marker(p));
        let (fixed, rest_ty) = match rest_at {
            Some(i) => {
                if i + 2 != elems_locs.len() {
                    return Err(Error::TypeError(
                        "&rest must be followed by exactly one type, as the last item in the parameter list".into(),
                    ));
                }
                let (v, loc) = &elems_locs[i + 1];
                (&elems_locs[..i], Some(self.parse_type_here_at(heap, *v, loc.as_ref())?))
            }
            None => (&elems_locs[..], None),
        };
        let mut params = Vec::with_capacity(fixed.len());
        for (v, loc) in fixed {
            params.push(self.parse_type_here_at(heap, *v, loc.as_ref())?);
        }
        Ok((params, rest_ty))
    }

    /// Reports every `defsignature` in this unit that never got a definition.
    ///
    /// Called by a file/module loader once the unit's forms are all checked.
    /// The REPL deliberately does *not* call it: a declaration typed on one
    /// line and its definition on the next are two batches, and both are one
    /// session.
    pub fn finish_unit(&mut self) -> Result<(), Error> {
        if self.predeclared.is_empty() {
            return Ok(());
        }
        let mut names = self.sorted_predeclared();
        self.predeclared.clear();
        let list = names.join("`, `");
        let (subject, verb) = if names.len() == 1 { ("declaration", "has") } else { ("declarations", "have") };
        names.clear();
        Err(Error::TypeError(format!(
            "defsignature: `{}` {} no definition in this file — a {} promises one",
            list, verb, subject
        )))
    }

    /// True when `fq` was declared by a `defsignature` and no `defun` has
    /// claimed it yet — consumed by `check_defun` so it treats the existing
    /// registry entry as the declaration it fulfils rather than a
    /// redefinition to report. Claiming removes the entry, so a genuine
    /// second definition of the same name still trips `check_redef`.
    fn claim_predeclared(&mut self, fq: &str) -> bool {
        self.predeclared.remove(fq)
    }

    /// [`Self::check_form`]'s dispatch body (the pre-monomorphization
    /// `check_form`, unchanged) — split out so the public entry point can
    /// wrap it with specialization draining. `def_loc` is `v`'s own source
    /// location (already computed by the caller via `heap.cons_loc(v)`),
    /// threaded down into whichever `check_def*` this form dispatches to so
    /// it can record where the name it registers was defined (`Registry::
    /// def_locs`, consulted by the LSP's goto-definition).
    fn check_form_dispatch(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, v: Value, def_loc: Option<Loc>) -> Result<TopLevelForm, Error> {
        if let Value::Cons(_) = v {
            let elems = heap.list_to_vec(v)?;
            // Per-element locations parallel to `elems`, so a `def*` body form
            // that is a bare atom (e.g. `(defun id ((x i32)) i32 x)`, whose
            // body is a lone parameter reference) keeps its own `Loc` for
            // hover — threaded to the body-checking helpers as `parts_locs`.
            let elem_locs: Vec<Option<Loc>> = heap.list_to_vec_locs(v)?.into_iter().map(|(_, l)| l).collect();
            let parts_locs = &elem_locs[1..];
            if let Some(Value::Symbol(id)) = elems.first() {
                // `impl<T>` lexes as a single symbol, exactly like `defstruct
                // vector-iter<T>`'s name does, so the head has to be split
                // before it can be matched. Only `impl` takes parameters on
                // the *head*; every other definition form carries them on the
                // name that follows.
                let (head, head_params) = parse_generic_name_header(heap.symbol_name(*id))?;
                if head == "impl" && !head_params.is_empty() {
                    return self.check_impl_generic(heap, interp, &elems[1..], parts_locs, head_params);
                }
                match *id {
                    SymId::PUB => return self.check_pub(heap, interp, &elems[1..], parts_locs, def_loc),
                    SymId::DEFUN => return self.check_defun(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    SymId::DEFSIGNATURE => return self.check_defsignature(heap, &elems[1..], parts_locs, false),
                    SymId::DEFVAR => return self.check_defvar(heap, interp, &elems[1..], true, false, def_loc),
                    SymId::DEFCONSTANT => return self.check_defvar(heap, interp, &elems[1..], false, false, def_loc),
                    SymId::DEFMACRO => return self.check_defmacro(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    SymId::MODULE => return self.check_module(heap, interp, &elems[1..]),
                    SymId::DEFMETHOD => return self.check_defmethod(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    SymId::DEFSTRUCT => return self.check_defstruct(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    SymId::DEFENUM => return self.check_defenum(heap, &elems[1..], parts_locs, false, def_loc),
                    SymId::DEFTRAIT => return self.check_deftrait(heap, interp, &elems[1..], parts_locs, false, def_loc),
                    SymId::DEFTYPE => return self.check_deftype(heap, &elems[1..], parts_locs, false, def_loc),
                    SymId::IMPL => return self.check_impl(heap, interp, &elems[1..], parts_locs),
                    SymId::USE => return self.check_use(heap, &elems[1..], parts_locs),
                    SymId::LOAD => return self.check_load(heap, &elems[1..]),
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
        // The one place a top-level expression's type is in hand. It is gone
        // from the form itself (see `core::Checked`), so record it here — see
        // [`Self::expr_type`].
        self.expr_ty = Some(t.ty.clone());
        forms::expr_form(heap, t.form)
    }

    /// The type the last [`Self::check_form_at`] proved for its top-level
    /// expression, or `None` if that form was a definition (which has no
    /// value).
    ///
    /// A separate output rather than part of the returned form: the type stops
    /// at the checker (see [`core::Checked`]), so a caller that wants it has to
    /// be *handed* it. It stays a field rather than a second return value
    /// because every driver in the tree wants only the form — what it evaluates
    /// to is self-describing at runtime — and asking afterwards keeps those
    /// call sites unchanged.
    pub fn expr_type(&self) -> Option<&Type> {
        self.expr_ty.as_ref()
    }

    /// The type/constructor registry — exposed so a caller (e.g. the REPL)
    /// can resolve an ADT variant's constructor name when printing an enum
    /// result (the box stores the variant index, not its name).
    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    /// `(pub defun ...)` / `(pub defmethod ...)` etc. — mark the next definition public.
    fn check_pub(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value], parts_locs: &[Option<Loc>], def_loc: Option<Loc>) -> Result<TopLevelForm, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("pub: expected a definition form".into()));
        }
        let inner_locs: &[Option<Loc>] = if parts_locs.len() > 1 { &parts_locs[1..] } else { &[] };
        if let Value::Symbol(id) = parts[0] {
            match id {
                SymId::DEFUN => return self.check_defun(heap, interp, &parts[1..], inner_locs, true, def_loc),
                SymId::DEFSIGNATURE => return self.check_defsignature(heap, &parts[1..], inner_locs, true),
                SymId::DEFVAR => return self.check_defvar(heap, interp, &parts[1..], true, true, def_loc),
                SymId::DEFCONSTANT => return self.check_defvar(heap, interp, &parts[1..], false, true, def_loc),
                SymId::DEFMACRO => return self.check_defmacro(heap, interp, &parts[1..], inner_locs, true, def_loc),
                SymId::DEFMETHOD => return self.check_defmethod(heap, interp, &parts[1..], inner_locs, true, def_loc),
                SymId::DEFSTRUCT => return self.check_defstruct(heap, interp, &parts[1..], inner_locs, true, def_loc),
                SymId::DEFENUM => return self.check_defenum(heap, &parts[1..], inner_locs, true, def_loc),
                SymId::DEFTYPE => return self.check_deftype(heap, &parts[1..], inner_locs, true, def_loc),
                _ => {}
            }
        }
        Err(Error::TypeError(
            "pub: expected defun/defsignature/defmacro/defmethod/defstruct/defenum/deftype/defvar/defconstant".into(),
        ))
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
            "if" | "let" | "let*" | "progn" | "setf" | "incf" | "decf" | "rotatef" | "shiftf"
                | "loop" | "break" | "return" | "catch" | "throw" | "unwind-protect" | "list"
                | "lambda" | "labels" | "match" | "panic" | "the" | "as" | "try-as" | "compile"
                | "quote" | "quasiquote" | "format" | "print" | "println"
                | "pprint" | "pprint-fill" | "pprint-linear" | "pprint-tabular"
                | "pprint-logical-block"
                // top-level forms (`check_form_dispatch`)
                | "pub" | "defun" | "defsignature" | "defvar" | "defconstant" | "defmacro" | "module"
                | "defmethod" | "defstruct" | "defenum" | "deftrait" | "deftype" | "impl" | "use" | "load"
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
    /// (an `i32`/`i64` argument both evaluate to the same `Value::Int`),
    /// so this never goes through the normal field-type validation
    /// `check_construct` would otherwise apply, but is sound for the same
    /// reason. An `elem_ty` with no `Sexpr` encoding (e.g. `Option<T>`) is a
    /// `TypeError` here, not a panic — unlike most of `&rest`'s checking,
    /// this can't happen any earlier than this, since a generic `&rest`'s
    /// declared type may only resolve to something concrete at a given call
    /// site (see `Self::check_call`'s `subst_apply`).
    fn wrap_rest_elem(&self, heap: &mut Heap, elem_ty: &Type, e: Checked) -> Result<Checked, Error> {
        if is_sexpr_expectation(elem_ty) {
            return Ok(e);
        }
        // A heap-repr ADT (`defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>`)
        // needs no `sexpr_ctor_for` wrap — same transparent retype as
        // `check_inner`'s expected-type fallback, see its doc comment for why
        // this is runtime-cost-free. This is what lets `(println "~a" my-
        // struct)`/`(list p ...&rest)` accept a user ADT argument.
        // The float types are the heap-repr types this shortcut must *not*
        // take.
        // `is_heap_repr` answers about the **interpreter's** representation,
        // where an `f64` is a `BoxedObj::Float` and therefore already a
        // `Sexpr::Float`. A *compiled* `f64` is neither boxed nor tagged: it
        // is the raw `f64::to_bits` pattern in an i64 slot, so retyping it
        // hands the printer a word whose low bits happen to read as the `Int`
        // tag — `(format false "~a" x)` in a compiled function printed
        // `to_bits(x) >> 3` instead of the number. Going through the `Float`
        // constructor instead is free in the interpreter
        // (`construct_sexpr_core`'s `SEXPR_FLOAT` arm validates and passes the
        // value straight back) and is exactly the `rt_float_new` box the
        // island emits for that variant. `string`/`bignum`/`ratio` genuinely
        // do share their tagged word between both worlds and stay on the
        // shortcut. `f32` is `f64`'s case exactly — one runtime
        // representation, two static widths — so the exclusion is by family,
        // not by naming `f64`.
        if self.is_heap_repr(elem_ty) && !elem_ty.is_float() {
            // A retype, not a node: the form is unchanged and only the
            // checker's own view of its type widens.
            return Ok(Checked::new(e.form, sexpr_ty()));
        }
        let ctor = sexpr_ctor_for(elem_ty).ok_or_else(|| {
            Error::TypeError(format!(
                "&rest: element type {:?} has no Sexpr encoding (use a number, bool/char/string/Sexpr)",
                elem_ty
            ))
        })?;
        let (type_name, variant) = self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
        let field_tys = self.variant_field_tys(&type_name, variant, &[]);
        let form = self.construct_form(heap, &type_name, variant, false, &field_tys, &[e.form])?;
        Ok(Checked::new(forms::rooted(heap, form), sexpr_ty()))
    }

    /// Collects already-checked `&rest` elements into one `Sexpr` list
    /// expression, back-to-front (mirroring `Interp::bind_macro_args`'s
    /// runtime construction of `defmacro`'s own `&rest` list) — `(cons
    /// (wrap e1) (cons (wrap e2) ... ()))`. The empty case is the `Nil`
    /// literal, reusing the same `(quote ())` shape `()`
    /// itself checks to.
    fn cons_rest_list(&self, heap: &mut Heap, elem_ty: &Type, items: Vec<Checked>) -> Result<Checked, Error> {
        // `sexpr-cons`, not the free `cons` (which is the `cons<T,U>` pair
        // builder now — Symbol/Sexpr redesign Phase 4b): a `&rest` list is a
        // `Sexpr`, built through the island cons layer.
        let cons_path = Path::root("sexpr-cons");
        let r = Ref::synthetic(cons_path);
        let nil = forms::quote_nil(heap)?;
        let mut acc = Checked::new(nil, option_of_sexpr());
        for item in items.into_iter().rev() {
            let mut s = RootScope::new(heap);
            s.push_root(acc.form);
            let item = self.wrap_rest_elem(&mut s, elem_ty, item)?;
            s.push_root(item.form);
            let form = self.call_form(&mut s, &r, &[item, acc])?;
            acc = Checked::new(form, option_of_sexpr());
        }
        Ok(Checked::new(forms::rooted(heap, acc.form), acc.ty))
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

    /// Resolve a written type name to a `deftype` alias, if it names one.
    ///
    /// Searched exactly like a type ([`Self::resolve_bare_type`] /
    /// [`Self::resolve_type_path`]), including `use` aliases and the
    /// `pub`-visibility rule for a qualified name — an alias is a spelling
    /// for a type and is reached the same ways one is.
    fn resolve_type_alias(&self, path: &Path) -> Option<TypeAlias> {
        if path.is_simple() {
            let name = path.last_segment();
            if let Some(target) = self.lookup_alias(name) {
                if let Some(a) = self.resolve_type_alias_path(&target) {
                    return Some(a);
                }
            }
            for prefix in self.ns_ancestors() {
                if let Some(m) = self.reg.root.module(prefix) {
                    if let Some(a) = m.type_aliases.get(name) {
                        return Some(a.clone());
                    }
                }
            }
            return None;
        }
        self.resolve_type_alias_path(path.segments())
    }

    /// [`Self::resolve_type_alias`] for a `module::...::Name` segment path.
    fn resolve_type_alias_path(&self, segs: &[String]) -> Option<TypeAlias> {
        if segs.is_empty() {
            return None;
        }
        let (mods, local) = segs.split_at(segs.len() - 1);
        if mods.is_empty() {
            for prefix in self.ns_ancestors() {
                if let Some(m) = self.reg.root.module(prefix) {
                    if let Some(a) = m.type_aliases.get(&local[0]) {
                        return Some(a.clone());
                    }
                }
            }
            return None;
        }
        let (abs, m) = self.find_module(mods)?;
        let a = m.type_aliases.get(&local[0])?;
        if !a.public && !self.in_scope(&abs) {
            return None;
        }
        Some(a.clone())
    }

    /// Reject a `deftype` alias written with the wrong number of type
    /// arguments, before [`Self::canon`] silently declines to expand it.
    ///
    /// `canon` cannot fail — it is called from places with no `Result` to
    /// return — so an arity mismatch there would just leave the name
    /// unexpanded and surface much later as "unknown type". This walks the
    /// *written* type (pre-`canon`, so the paths are still as spelled) and
    /// says what is actually wrong.
    fn check_type_alias_arity(&self, t: &Type) -> Result<(), Error> {
        match t {
            Type::Named(n, args) => {
                if let Some(a) = self.resolve_type_alias(n) {
                    if a.params.len() != args.len() {
                        return Err(Error::TypeError(format!(
                            "type alias `{}` takes {} type argument(s), got {}",
                            a.name,
                            a.params.len(),
                            args.len()
                        )));
                    }
                }
                for x in args {
                    self.check_type_alias_arity(x)?;
                }
                Ok(())
            }
            Type::Dyn(_, pins) => pins.iter().try_for_each(|p| self.check_type_alias_arity(p)),
            Type::Fn(ps, rest, r) => {
                ps.iter().try_for_each(|p| self.check_type_alias_arity(p))?;
                if let Some(t) = rest {
                    self.check_type_alias_arity(t)?;
                }
                self.check_type_alias_arity(r)
            }
            _ => Ok(()),
        }
    }

    /// Resolve a nominal type [`Path`] to its canonical (located) form if a
    /// matching type exists; otherwise (type variables, unknown names) return it
    /// unchanged.
    fn resolve_type_name(&self, path: &Path) -> Path {
        if path.is_simple() {
            let name = path.last_segment();
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
    fn fn_value(&self, heap: &mut Heap, name: &str, expected: Option<&Type>) -> Option<Result<Checked, Error>> {
        let fq = self.resolve_fn(name)?;
        Some(self.fn_ref_node(heap, vec![name.to_string()], fq, expected))
    }

    /// Reify a qualified free-function path as a function value (`FnRef`).
    fn fn_path_value(&self, heap: &mut Heap, segs: &[String], expected: Option<&Type>) -> Option<Result<Checked, Error>> {
        let fq = self.resolve_fn_path(segs)?;
        Some(self.fn_ref_node(heap, segs.to_vec(), fq, expected))
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
    fn fn_ref_node(&self, heap: &mut Heap, written: Vec<String>, fq: Path, expected: Option<&Type>) -> Result<Checked, Error> {
        let sig = self.reg.fn_sig(&fq).expect("resolved fn exists").clone();
        let tmpl_ty = fn_value_type(&sig);
        let home = self.ns.clone();
        // The referenced function's own parameter types, which the bridge
        // needs to build a closure with no call site to read them from —
        // every one the runtime function really takes, in declared order.
        // A `&rest` parameter is a `sexpr` entry in the list (it has no slot
        // of its own there the way `Type::Fn` gives it one).
        let ref_params = |sig: &FnSig| {
            let mut ps = fn_value_params(sig);
            if sig.rest.is_some() {
                // Between the `&optional`s and the `&key`s, which is where
                // `Self::check_defun` puts it. The two cannot both appear
                // (the language refuses `&key` beside `&optional`/`&rest`),
                // so this only ever lands at the end of one of them.
                ps.insert(sig.params.len() + sig.optionals.len(), option_of_sexpr());
            }
            ps
        };
        if sig.type_params.is_empty() {
            let r = Ref { written, home, resolved: fq };
            let form = self.fnref_form(heap, &r, &ref_params(&sig))?;
            return Ok(Checked::new(form, tmpl_ty));
        }
        if let Some(exp) = expected {
            let params: HashSet<String> = sig.type_params.iter().cloned().collect();
            let mut subst: BTreeMap<String, Type> = BTreeMap::new();
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
                    let r = Ref::synthetic(mangled);
                    let params = match self.reg.fn_sig(&r.resolved) {
                        Some(sig) => ref_params(sig),
                        // The specialization is generated at the end of this
                        // form, so its signature may not be registered yet;
                        // the resolved type says the same thing.
                        None => match &resolved_ty {
                            Type::Fn(ps, rest, _) => {
                                let mut ps = ps.clone();
                                if rest.is_some() {
                                    ps.push(option_of_sexpr());
                                }
                                ps
                            }
                            _ => Vec::new(),
                        },
                    };
                    let form = self.fnref_form(heap, &r, &params)?;
                    return Ok(Checked::new(form, resolved_ty));
                }
                // Open type arguments: we're inside another generic
                // function's diagnostics-only body check — the node is never
                // executed, and that function's own specialization will
                // re-check this reference with the types concrete.
                let r = Ref { written, home, resolved: fq };
                let form = self.fnref_form(heap, &r, &ref_params(&sig))?;
                return Ok(Checked::new(form, resolved_ty));
            }
            // Shape mismatch: hand back the generic node so `check`'s
            // ordinary expected-vs-actual reconciliation reports it.
            let r = Ref { written, home, resolved: fq };
            let form = self.fnref_form(heap, &r, &ref_params(&sig))?;
            return Ok(Checked::new(form, tmpl_ty));
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
    fn method_value(&self, heap: &mut Heap, name: &str, expected: Option<&Type>) -> Option<Result<Checked, Error>> {
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
            let mut subst: BTreeMap<String, Type> = BTreeMap::new();
            if unify(&tparams, &ty, expected.unwrap(), &mut subst).is_ok()
                && def.params.iter().all(|p| subst.contains_key(p))
            {
                let targs: Vec<Type> = def.params.iter().map(|p| subst[p.as_str()].clone()).collect();
                if !targs.iter().any(|t| self.type_is_open(t))
                    && self.generic_method_templates.contains_key(&(type_fq.clone(), name.to_string()))
                {
                    let mangled = self.request_method_specialization(&type_fq, name, targs);
                    let resolved_ty = subst_apply(&ty, &subst);
                    let params = match &resolved_ty {
                        Type::Fn(ps, _, _) => ps.clone(),
                        _ => Vec::new(),
                    };
                    let home = self.ns.clone();
                    return Some(
                        self.methodref_form(heap, &type_fq, &mangled, &home, &params)
                            .map(|form| Checked::new(form, resolved_ty)),
                    );
                }
            }
            return None;
        }
        if &ty != expected.unwrap() {
            return None;
        }
        let params = af.sig.params.clone();
        let home = self.ns.clone();
        Some(self.methodref_form(heap, &type_fq, name, &home, &params).map(|form| Checked::new(form, ty)))
    }

    /// `var::field`: if `segs` is `[recv, method]` and `recv` names a bound
    /// local or global whose type has an *instance* associated function
    /// called `method` (in practice always a `defstruct` field accessor —
    /// see `Checker::check_defstruct` — though this doesn't care how the
    /// method came to exist), produces the same `assoc` node a
    /// `(method recv)` call would. `None` (not an error) for any other
    /// shape, so the caller falls back to ordinary module/type-path
    /// resolution (`Checker::check`'s `Value::Path` case).
    /// Delegates to `Checker::check_assoc_call` (instead of building the
    /// `assoc` node directly) so a *generic* `defstruct`'s field type
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
    ) -> Option<Result<Checked, Error>> {
        let [recv_name, method] = segs else { return None };
        // The receiver is held across the whole of `check_assoc_call`, which
        // allocates plenty before `assoc_form` finally pushes it — so it has to
        // stay rooted. `var_form`/`global_form` root what they return for
        // exactly this reason; see `var_form`'s doc comment.
        let recv = if let Some(t) = env.get(recv_name) {
            let ty = t.clone();
            match forms::var_form(heap, recv_name) {
                Ok(form) => Checked::new(form, ty),
                Err(e) => return Some(Err(e)),
            }
        } else {
            let (path, vi) = self.resolve_global(recv_name)?;
            let ty = vi.ty;
            let r = self.mk_ref(vec![recv_name.clone()], path);
            match self.global_form(heap, &r, &ty) {
                Ok(form) => Checked::new(form, ty),
                Err(e) => return Some(Err(e)),
            }
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
                    if let Some(bound) = self.type_var_bindings.get(n.last_segment()) {
                        return bound.clone();
                    }
                }
                // A `deftype` alias is a spelling, not a type: rewrite it away
                // here and nothing downstream ever learns it existed. The
                // stored body is already canonical and already
                // alias-expanded, so this is one substitution, not a loop.
                // A wrong argument count declines to expand (this cannot
                // fail) — `Self::check_type_alias_arity` reports it at the
                // annotation instead.
                if let Some(alias) = self.resolve_type_alias(n) {
                    if alias.params.len() == args.len() {
                        let subst: BTreeMap<String, Type> = alias
                            .params
                            .iter()
                            .cloned()
                            .zip(args.iter().map(|a| self.canon(a)))
                            .collect();
                        return subst_apply(&alias.body, &subst);
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

    /// Parse a *written* type annotation into its canonical [`Type`] — the
    /// single entry point every annotation site goes through (`defun`
    /// parameters and return types, `defstruct`/`defenum` fields, `defvar`,
    /// `the`/`as`, `deftrait`/`impl` signatures).
    ///
    /// Beyond `parse_type` + [`Self::canon`] it rejects one confusion the
    /// shared type/trait name space (see [`Self::check_type_trait_clash`])
    /// makes worth naming outright: a *trait* written where a type belongs,
    /// e.g. `Result<i32, Error>` for the prelude's `Error` trait. Without
    /// this the name would silently pass as an unresolved one (the same
    /// treatment a generic's type variables get, which is why `canon` cannot
    /// reject unknown names in general) and only surface much later as a
    /// mismatch against a type nothing can ever inhabit.
    ///
    /// Being that single entry point is also what makes it the place to
    /// record where types are *written*: every name parsed out of the
    /// annotation that resolves to a user-defined type or trait is pushed to
    /// [`Self::type_uses`], which the LSP turns into semantic tokens. `loc`
    /// is the annotation's own source span, needed only for a bare-atom
    /// annotation — a list form's elements carry their own recorded spans
    /// (see [`parse_type_spanned`]) — and the recording is simply skipped
    /// where no span is available.
    fn parse_type_here_at(&self, heap: &Heap, v: Value, loc: Option<&Loc>) -> Result<Type, Error> {
        let mut spans = Vec::new();
        let written = parse_type_spanned(heap, v, loc, &mut spans)?;
        self.check_type_alias_arity(&written)?;
        let ty = self.canon(&written);
        for span in &spans {
            self.record_type_span(span);
        }
        self.reject_trait_in_type_position(&ty)?;
        Ok(ty)
    }

    /// Resolve one written name from an annotation and, when it names a
    /// user-defined type or trait, record the occurrence. Uses the same
    /// resolution [`Self::canon`] applies, so what gets recorded is exactly
    /// what the annotation meant: a bound type variable is skipped, a
    /// builtin (`Option`, `Vector`, ...) is left to the editors' grammars,
    /// and a name that resolves to nothing (an unbound type variable of a
    /// generic template) records nothing.
    fn record_type_span(&self, span: &TypeNameSpan) {
        if span.dyn_head {
            if let Some(fq) = self.resolve_trait_path(&span.path) {
                self.record_trait_use(&fq, span.loc.clone());
            }
            return;
        }
        if span.path.is_simple() && self.type_var_bindings.contains_key(span.path.last_segment()) {
            return;
        }
        let fq = self.resolve_type_name(&span.path);
        self.record_type_use(&fq, span.loc.clone());
    }

    /// Record a resolved *type* name occurrence at `loc`, if `fq` names a
    /// non-builtin type.
    fn record_type_use(&self, fq: &Path, loc: Loc) {
        if let Some(def) = self.reg.type_def(fq) {
            if def.builtin {
                return;
            }
            // `AdtKind::Struct` is produced only by `defstruct`; every
            // `defenum` is a `Sum`.
            let kind = match def.kind {
                AdtKind::Struct => TypeKind::Struct,
                _ => TypeKind::Enum,
            };
            self.type_uses.borrow_mut().push(TypeUse { path: fq.clone(), kind, loc });
        }
    }

    /// Record a resolved *trait* name occurrence at `loc`, if `fq` names a
    /// non-builtin trait.
    fn record_trait_use(&self, fq: &Path, loc: Loc) {
        if let Some(def) = self.reg.trait_def(fq) {
            if def.builtin {
                return;
            }
            self.type_uses.borrow_mut().push(TypeUse { path: fq.clone(), kind: TypeKind::Trait, loc });
        }
    }

    /// Record the *type* segment of a written `::` path whose prefix resolved
    /// to the type `fq` — `rect::new`/`mod::color::red`, where the type is the
    /// second-to-last written segment (the last one names a member).
    fn record_path_type_use(&self, segs: &[String], fq: &Path, head_loc: Option<&Loc>) {
        if segs.len() < 2 {
            return;
        }
        self.record_path_seg_use(segs, segs.len() - 2, fq, head_loc);
    }

    /// Record segment `idx` of a written `::` path as an occurrence of the
    /// type `fq`. Its span is computed from `head_loc` (the whole token's
    /// span) by walking the segments before it — a token never spans lines,
    /// so only the columns move.
    ///
    /// `segs` are the *interned* (case-folded) segments, so the arithmetic
    /// only lands on the right source columns when the whole reconstructed
    /// path is as long as the token it was read from — which also confirms
    /// the token really is a plain `a::b` path with nothing else in it.
    /// Anything else records nothing rather than painting the wrong range.
    fn record_path_seg_use(&self, segs: &[String], idx: usize, fq: &Path, head_loc: Option<&Loc>) {
        let Some(base) = head_loc else { return };
        if idx >= segs.len() {
            return;
        }
        let written: u32 = segs.iter().map(|s| s.chars().count() as u32).sum::<u32>()
            + 2 * (segs.len() as u32 - 1);
        if written != base.end_col.saturating_sub(base.col) {
            return;
        }
        let mut off = 0u32;
        for s in &segs[..idx] {
            off += s.chars().count() as u32 + 2; // the segment and its `::`
        }
        let col = base.col + off;
        let len = segs[idx].chars().count() as u32;
        let loc = Loc::new(Rc::clone(&base.file), base.line, col).with_end(base.line, col + len);
        self.record_type_use(fq, loc);
    }

    /// Record a definition header's own name (`(defstruct rect ...)`'s
    /// `rect`) as a type/trait occurrence. `name_len` is the name's length in
    /// chars — the header token may continue with `<T,...>` type parameters,
    /// which are not part of the name, so the name is a prefix of the token
    /// and never longer than it. A `name_len` that does exceed the token
    /// means the two disagree about the text (the same case-folding caveat
    /// `types::SpanRec::record` documents); record nothing then.
    fn record_def_name_use(&self, fq: &Path, kind: TypeKind, name_loc: Option<&Loc>, name_len: usize) {
        let Some(base) = name_loc else { return };
        if name_len as u32 > base.end_col.saturating_sub(base.col) {
            return;
        }
        let loc = Loc::new(Rc::clone(&base.file), base.line, base.col)
            .with_end(base.line, base.col + name_len as u32);
        self.type_uses.borrow_mut().push(TypeUse { path: fq.clone(), kind, loc });
    }

    /// [`Self::parse_type_here_at`]'s check, applied to every `Type::Named` head
    /// inside `ty`: a name that resolves to no type but *does* name a trait
    /// in scope is a trait object written without its `:dyn`.
    fn reject_trait_in_type_position(&self, ty: &Type) -> Result<(), Error> {
        match ty {
            Type::Named(p, args) => {
                if self.reg.type_def(p).is_none() && p.is_simple() {
                    if let Ok(trait_path) = self.resolve_trait_name(p.last_segment()) {
                        return Err(Error::TypeError(format!(
                            "`{}` is a trait, not a type — write `:dyn {}` for a trait object",
                            trait_path.last_segment(),
                            trait_path.last_segment()
                        )));
                    }
                }
                for a in args {
                    self.reject_trait_in_type_position(a)?;
                }
            }
            Type::Dyn(_, pins) => {
                for p in pins {
                    self.reject_trait_in_type_position(p)?;
                }
            }
            Type::Fn(params, rest, ret) => {
                for p in params {
                    self.reject_trait_in_type_position(p)?;
                }
                if let Some(r) = rest {
                    self.reject_trait_in_type_position(r)?;
                }
                self.reject_trait_in_type_position(ret)?;
            }
            _ => {}
        }
        Ok(())
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
    ) -> Result<TopLevelForm, Error> {
        if parts.len() >= 2 && params_declare_opt_key(heap, parts[1])? {
            return self.check_defun_opt_key(heap, interp, parts, parts_locs, public, def_loc);
        }
        self.check_defun_fixed(heap, interp, parts, parts_locs, public, def_loc)
    }

    /// The ordinary (no `&optional`/`&key`) `defun` path — every existing
    /// fixed/`&rest`/generic `defun`, unchanged. Split out from
    /// `Self::check_defun` so that path stays exactly as it was before
    /// `&optional`/`&key` support existed; see `Self::check_defun_opt_key`
    /// for the sibling that handles those two sections (deliberately a
    /// separate, self-contained function rather than threading new
    /// branches through generics/monomorphization — mirrors how
    /// `Self::check_defmacro` is already its own function rather than a
    /// branch of this one).
    fn check_defun_fixed(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        let (params, param_locs, rest, ret, bounds, body_start, doc) = self.parse_defun_sig(heap, parts, parts_locs)?;
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let fq_name = self.fq(&name);

        // Register the signature in the current namespace before checking the
        // body so self-recursion works. A signature a `defsignature` already
        // put there is *this same definition*, not a redefinition — claiming
        // it consumes the entry, so a genuine second `defun` of the name still
        // reports through `check_redef` as before. The declared signature is
        // kept to compare against below: a declaration nobody checks is a
        // declaration nobody can trust.
        let declared = if self.claim_predeclared(&fq_name.to_string()) {
            self.cur_ns().fns.get(&name).cloned()
        } else {
            self.check_redef("function", &name, self.cur_ns().fns.get(&name))?;
            None
        };

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
            optionals: Vec::new(),
            keys: Vec::new(),
        };
        if let Some(decl) = &declared {
            signature_agrees(&name, decl, &sig)?;
        }
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);
        if let Some(loc) = def_loc {
            self.reg.def_locs.fns.insert(fq_name.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.fns.insert(fq_name.clone(), doc);
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
            params.push((rname.clone(), option_of_sexpr()));
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
        self.definition_form(heap, &fq_name, &type_params, &params, &ret, public, &body)
    }

    /// The `&optional`/`&key` `defun` path — `Self::check_defun` routes here
    /// when `Self::params_declare_opt_key` sees either marker in the raw
    /// parameter list. Supports generic type parameters and a `where` clause
    /// (`Self::check_call_opt_key` runs the same `unify`/`subst`/bounds
    /// machinery `Self::check_call` does for an ordinary generic call), with
    /// one deliberate restriction not present on the non-opt/key path: a
    /// defaulted `&optional`/`&key` parameter's declared type may not
    /// reference this function's own type parameters (see the
    /// `type_has_param` check below) — a call site that omits such an
    /// argument would splice the checked default node in verbatim
    /// (`Self::check_call_opt_key`, no re-checking against the call's
    /// concrete types), and that node's `.ty` would then still name the
    /// abstract type parameter instead of the instantiation's concrete type,
    /// which downstream compile-side code (`core_bridge`'s `binding_kind`)
    /// reads to decide GC/boxing representation. A defaultless parameter
    /// has no such node to splice (an `Option::none` is synthesized fresh at
    /// the call site, already `subst_apply`-ed) so is unrestricted.
    ///
    /// Otherwise deliberately narrower than `Self::check_defun_fixed` in one
    /// way, surfaced as an explicit error rather than silently falling back
    /// to some approximation:
    ///
    /// - A default-value expression sees *no* earlier parameter bound (not
    ///   even an earlier `&optional`/`&key` one, and not a required one
    ///   either) — checked in an empty `Env`, so it may reference only
    ///   globals/other functions/literals. Real CL lets a later default
    ///   reference an earlier parameter's actual argument value; supporting
    ///   that here would require re-binding every parameter by name at each
    ///   call site (a `let*` scaffold) — dropped for this first cut. A
    ///   default that tries to reference an earlier parameter fails with an
    ///   ordinary "unbound variable" `Error::TypeError`, not a wrong answer.
    ///
    /// See `Self::check_call_opt_key` for how a call site fills in whatever
    /// this leaves out.
    fn check_defun_opt_key(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        let fq_name = self.fq(&name);
        let (required, required_locs, optionals_raw, rest, keys_raw) = self.parse_defun_params_full(heap, parts[1])?;
        let ret = self.parse_type_here_at(heap, parts[2], parts_locs.get(2).and_then(|l| l.as_ref()))?;
        let mut body_start = 3;
        let mut bounds: BTreeMap<String, Vec<TraitBound>> = BTreeMap::new();
        if let Some(form) = parts.get(3) {
            if is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        let doc = take_leading_docstring(heap, parts, body_start);
        if doc.is_some() {
            body_start += 1;
        }

        self.check_redef("function", &name, self.cur_ns().fns.get(&name))?;

        // A generic `&optional`/`&key` defun additionally retains its raw
        // source form for per-instantiation re-checking, exactly like
        // `Self::check_defun_fixed` — registered before the default-value
        // checks below (self-recursion in a default expression is no more
        // supported than in a body, but consistency costs nothing) and
        // before the body check, for the same self-recursion reason.
        if !type_params.is_empty() {
            for &p in parts {
                heap.push_permanent_root(p);
            }
            self.generic_fn_templates.insert(
                fq_name.clone(),
                FnTemplate { parts: parts.to_vec(), ns: self.ns.clone(), type_params: type_params.clone() },
            );
        }

        let type_param_set: HashSet<String> = type_params.iter().cloned().collect();
        let mut optionals = Vec::with_capacity(optionals_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &optionals_raw {
            // Rejected *before* the default is checked, so this restriction —
            // not whatever the default's own text happens to fail on when
            // checked in an empty `Env` — is the error the user sees.
            if default_raw.is_some() && type_has_param(decl_ty, &type_param_set) {
                return Err(Error::TypeError(format!(
                    "defun {}: &optional parameter `{}` may not default when its type mentions the \
                     function's own type parameter — declare it with no default (`Option<{:?}>`) instead",
                    name, pname, decl_ty
                )));
            }
            let default = self.check_opt_key_default(heap, interp, pname, decl_ty, default_raw.clone())?;
            optionals.push(OptKeyParam { name: pname.clone(), decl_ty: decl_ty.clone(), default });
        }
        let mut keys = Vec::with_capacity(keys_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &keys_raw {
            if default_raw.is_some() && type_has_param(decl_ty, &type_param_set) {
                return Err(Error::TypeError(format!(
                    "defun {}: &key parameter `{}` may not default when its type mentions the \
                     function's own type parameter — declare it with no default (`Option<{:?}>`) instead",
                    name, pname, decl_ty
                )));
            }
            let default = self.check_opt_key_default(heap, interp, pname, decl_ty, default_raw.clone())?;
            keys.push(OptKeyParam { name: pname.clone(), decl_ty: decl_ty.clone(), default });
        }

        let sig = FnSig {
            type_params: type_params.clone(),
            params: required.iter().map(|(_, t)| t.clone()).collect(),
            ret: ret.clone(),
            public,
            rest: rest.as_ref().map(|(_, t, _)| t.clone()),
            builtin: false,
            bounds: bounds.clone(),
            optionals: optionals.clone(),
            keys: keys.clone(),
        };
        self.reg.root.module_mut(&self.ns).fns.insert(name.clone(), sig);
        if let Some(loc) = def_loc {
            self.reg.def_locs.fns.insert(fq_name.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.fns.insert(fq_name.clone(), doc);
        }

        // Runtime params, in declared order: required, then `&optional`
        // (effective type — `decl_ty` when defaulted, `Option<decl_ty>`
        // otherwise), then the `&rest` name (plain `Sexpr` list), then
        // `&key` (same effective-type rule) — mirrors
        // `TopLevel::Defmacro::params`'s own region order. `&key`/`&optional`
        // never coexist (`Self::parse_defun_params_full` rejects that), so
        // exactly one of the two loops below ever runs.
        let mut params: Vec<(String, Type)> = required.clone();
        let mut param_locs: Vec<Option<Loc>> = required_locs.clone();
        for (o, (_, _, loc, _)) in optionals.iter().zip(optionals_raw.iter()) {
            params.push((o.name.clone(), o.effective_ty()));
            param_locs.push(loc.clone());
        }
        if let Some((rname, _, rloc)) = &rest {
            params.push((rname.clone(), option_of_sexpr()));
            param_locs.push(rloc.clone());
        }
        for (k, (_, _, loc, _)) in keys.iter().zip(keys_raw.iter()) {
            params.push((k.name.clone(), k.effective_ty()));
            param_locs.push(loc.clone());
        }

        let binds: Vec<(String, Type, Option<Loc>)> =
            params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)).collect();
        let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
        let body_locs = parts_locs.get(body_start..).unwrap_or(&[]);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], body_locs, Some(&ret))?;
        self.definition_form(heap, &fq_name, &type_params, &params, &ret, public, &body)
    }

    /// Checks one `&optional`/`&key` default-value form (or none) for
    /// `Self::check_defun_opt_key`, against the parameter's declared type,
    /// in an environment with no other parameters bound — see that
    /// method's doc comment for why. `None` in means "no default was
    /// written"; `None` out means the same thing (`OptKeyParam::default`).
    fn check_opt_key_default(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        _pname: &str,
        decl_ty: &Type,
        default_raw: Option<Value>,
    ) -> Result<Option<crate::owned_form::OwnedForm>, Error> {
        match default_raw {
            None => Ok(None),
            Some(form) => {
                let env = Env::new();
                let checked = self.check(heap, interp, &env, form, Some(decl_ty))?;
                // Detached from the heap right here — see `OptKeyParam::
                // default` for why the signature cannot hold a `Value`.
                Ok(Some(crate::owned_form::value_to_owned(heap, checked.form)?))
            }
        }
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
        parts_locs: &[Option<Loc>],
    ) -> Result<
        (Vec<(String, Type)>, Vec<Option<Loc>>, Option<RestParam>, Type, BTreeMap<String, Vec<TraitBound>>, usize, Option<String>),
        Error,
    > {
        if parts.len() < 3 {
            return Err(Error::TypeError("defun: (defun name (params) ret body...)".into()));
        }
        let (params, param_locs, rest) = self.parse_params_rest(heap, parts[1])?;
        let ret = self.parse_type_here_at(heap, parts[2], parts_locs.get(2).and_then(|l| l.as_ref()))?;
        let mut body_start = 3;
        let mut bounds: BTreeMap<String, Vec<TraitBound>> = BTreeMap::new();
        if let Some(form) = parts.get(3) {
            if is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        let doc = take_leading_docstring(heap, parts, body_start);
        if doc.is_some() {
            body_start += 1;
        }
        Ok((params, param_locs, rest, ret, bounds, body_start, doc))
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
    /// the `assoc` node to reference. Only ever called when a
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
    ) -> Result<Vec<TopLevelForm>, Error> {
        let mut out = Vec::new();
        loop {
            let req = self.spec_pending.borrow_mut().pop();
            let Some(req) = req else { break };
            if out.len() >= SPECIALIZATION_BUDGET {
                let last = match &req {
                    SpecRequest::Fn { mangled, .. } => mangled.to_string(),
                    SpecRequest::Method { type_fq, mangled, .. } => format!("{}::{}", type_fq, mangled),
                    SpecRequest::Blanket { trait_path, target } => {
                        format!("impl {} {}", trait_path, mangle_type(target))
                    }
                };
                return Err(Error::TypeError(format!(
                    "monomorphization did not converge after {} instantiations (a polymorphically \
                     recursive generic function — one that calls itself at an ever-growing type — \
                     cannot be compiled; last requested: {})",
                    SPECIALIZATION_BUDGET, last
                )));
            }
            // Each finished specialization is rooted as it lands in `out`, and
            // stays rooted: the drain runs until the queue is empty, and every
            // later specialization allocates. A `Vec<Value>` is invisible to the
            // collector, so without this an early specialization is freed while a
            // later one is being built and its cells are handed straight back
            // out — the bundle then holds a *fragment* of the form that used to
            // be there (a body list where a `defun` belongs), which `exec`
            // rejects as "not a top-level core form". Released with the rest of
            // the form's roots by `check_form_at`'s single `truncate_roots`.
            match req {
                SpecRequest::Fn { .. } => {
                    let form = self.specialize_defun(heap, interp, &req)?;
                    out.push(forms::rooted(heap, form));
                }
                SpecRequest::Method { .. } => {
                    let form = self.specialize_method(heap, interp, &req)?;
                    out.push(forms::rooted(heap, form));
                }
                SpecRequest::Blanket { trait_path, target } => {
                    for form in self.materialize_blanket_impl(heap, interp, &trait_path, &target)? {
                        out.push(forms::rooted(heap, form));
                    }
                }
            }
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
    ) -> Result<TopLevelForm, Error> {
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
        let bindings: BTreeMap<String, Type> =
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
        bindings: BTreeMap<String, Type>,
    ) -> (Vec<String>, Vec<Type>, BTreeMap<String, Type>) {
        let saved_ns = std::mem::replace(&mut self.ns, ns);
        let saved_loops = std::mem::take(&mut *self.loop_stack.borrow_mut());
        let saved_bindings = std::mem::replace(&mut self.type_var_bindings, bindings);
        (saved_ns, saved_loops, saved_bindings)
    }

    fn exit_specialization(
        &mut self,
        saved_ns: Vec<String>,
        saved_loops: Vec<Type>,
        saved_bindings: BTreeMap<String, Type>,
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
    ) -> Result<TopLevelForm, Error> {
        let SpecRequest::Method { type_fq, base, args, mangled } = req else {
            unreachable!("drain routes Method requests here")
        };
        // A specialization's own `public` mirrors the unspecialized template
        // method's — `Interp`'s scope tree gates visibility on this bit (see
        // `assoc`'s doc comment), so a generated specialization must
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
                let bindings: BTreeMap<String, Type> =
                    written_vars.into_iter().zip(args.iter().cloned()).collect();
                let (saved_ns, saved_loops, saved_bindings) = self.enter_specialization(ns, bindings);
                let result = self.specialize_method_form(heap, interp, &parts, mangled, public);
                self.exit_specialization(saved_ns, saved_loops, saved_bindings);
                result
            }
            MethodTemplate::Getter { index } => self.synthesize_accessor(heap, type_fq, args, index, mangled, true, public),
            MethodTemplate::Setter { index } => self.synthesize_accessor(heap, type_fq, args, index, mangled, false, public),
        }
    }

    fn specialize_method_form(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        mangled: &str,
        public: bool,
    ) -> Result<TopLevelForm, Error> {
        let MethodSig { instance, self_name, recv_ty, type_fq, params, ret, body_start, .. } =
            self.parse_defmethod_sig(heap, parts, &[])?;
        // The receiver is the first parameter, not a field beside them: the
        // core form binds what the body binds, in order.
        let mut all: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            all.push((s.clone(), recv_ty));
        }
        all.extend(params);
        // Bounds are deliberately dropped here, mirroring
        // `specialize_defun_body`: with the owner's type variables concrete,
        // every bounded method call resolves against the real receiver type
        // (and was already validated at the call site).
        let env = Env::new().extended(all.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], &[], Some(&ret))?;
        self.defmethod_form(heap, &type_fq, mangled, instance, &all, &ret, public, &body)
    }

    /// Builds a concrete accessor `TopLevel::Defmethod` for a generic
    /// `defstruct`'s field `index`, with the field type substituted at the
    /// owner's concrete `args` — mirrors the ASTs `check_defstruct`
    /// synthesizes, minus any registry mutation.
    fn synthesize_accessor(
        &self,
        heap: &mut Heap,
        type_fq: &Path,
        args: &[Type],
        index: usize,
        mangled: &str,
        getter: bool,
        public: bool,
    ) -> Result<TopLevelForm, Error> {
        let def = self.reg.type_def(type_fq).expect("an accessor template implies the type exists");
        let subst: BTreeMap<String, Type> =
            def.params.iter().cloned().zip(args.iter().cloned()).collect();
        let field_ty = subst_apply(&def.variants[0].fields[index], &subst);
        let recv_ty = Type::Named(type_fq.clone(), args.to_vec());
        let self_param = ("self".to_string(), recv_ty);
        if getter {
            // Rooted for the same reason the setter branch below roots: the body
            // form is built *before* `defmethod_form`, which then allocates its
            // path, its method symbol, its parameter list and its return
            // representation before `Items::extend` finally pushes the body — and
            // any of those allocations can collect an unrooted form. (This arm
            // did not root, and `gc_stress` caught it as `push_root given freed
            // cell` inside `Items::push`.)
            let self_var = forms::var_form(heap, "self")?;
            let mut s = RootScope::new(heap);
            s.push_root(self_var);
            let get = self.field_get_form(&mut s, self_var, index, &field_ty)?;
            s.push_root(get);
            self.defmethod_form(&mut s, type_fq, mangled, true, &[self_param], &field_ty, public, &[get])
        } else {
            let self_var = forms::var_form(heap, "self")?;
            let mut s = RootScope::new(heap);
            s.push_root(self_var);
            let value_var = forms::var_form(&mut s, "value")?;
            s.push_root(value_var);
            let set = self.field_set_form(&mut s, self_var, index, &field_ty, value_var)?;
            s.push_root(set);
            let params = [self_param, ("value".to_string(), field_ty)];
            self.defmethod_form(&mut s, type_fq, mangled, true, &params, &Type::Unit, public, &[set])
        }
    }

    fn specialize_defun_body(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        tmpl: &FnTemplate,
        mangled: &Path,
        public: bool,
    ) -> Result<TopLevelForm, Error> {
        if params_declare_opt_key(heap, tmpl.parts[1])? {
            return self.specialize_defun_body_opt_key(heap, interp, tmpl, mangled, public);
        }
        // `bounds` deliberately dropped: the type variables are concrete
        // here, so method calls on them resolve directly against the real
        // receiver type (`check_instance_method`'s ordinary branch, never
        // the bounds branch), and the bounds' own validity was already
        // verified at the call site that requested this instantiation
        // (`check_call`'s where-clause validation).
        let (params, _param_locs, rest, ret, _bounds, body_start, _doc) = self.parse_defun_sig(heap, &tmpl.parts, &[])?;
        let mut params = params;
        if let Some((rname, _, _)) = &rest {
            params.push((rname.clone(), option_of_sexpr()));
        }
        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &tmpl.parts[body_start..], &[], Some(&ret))?;
        self.defun_form(heap, mangled, &params, &ret, public, &body)
    }

    /// [`Self::specialize_defun_body`]'s `&optional`/`&key` counterpart —
    /// routed to whenever the retained template's parameter list declares
    /// either section (`Self::params_declare_opt_key`). Re-parses the
    /// retained source form (`Self::parse_defun_params_full`, plus the
    /// return type and an optional `where` clause exactly as
    /// `Self::check_defun_opt_key` does) with `Self::type_var_bindings` in
    /// effect, so `Self::parse_type_here_at`/`canon` substitute the
    /// template's type-parameter names for concrete types throughout —
    /// required params, `&optional`/`&key` declared types, the return type,
    /// and (via `Self::check_opt_key_default`) each default expression's
    /// re-check, no code changes needed for any of that (mirrors
    /// `Self::specialize_defun_body`'s own reliance on the same mechanism).
    /// Every defaulted `&optional`/`&key` parameter's declared type is
    /// already guaranteed concrete (`Self::check_defun_opt_key`'s
    /// `type_has_param` restriction), so a default's freshly re-checked
    /// node checked here has a genuinely concrete type — see the module's
    /// `check_defun_opt_key` doc comment for why that matters downstream.
    ///
    /// The signature is not registered on `self.reg` — like
    /// `Self::specialize_defun_body`, nothing besides `Interp`/the compile
    /// pipeline ever looks a specialization's `FnSig` up; `Self::check_call_opt_key`
    /// already validated the call site against the *unspecialized* generic
    /// signature before requesting this instantiation.
    fn specialize_defun_body_opt_key(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        tmpl: &FnTemplate,
        mangled: &Path,
        public: bool,
    ) -> Result<TopLevelForm, Error> {
        let (required, _required_locs, optionals_raw, rest, keys_raw) =
            self.parse_defun_params_full(heap, tmpl.parts[1])?;
        let ret = self.parse_type_here_at(heap, tmpl.parts[2], None)?;
        let mut body_start = 3;
        if let Some(form) = tmpl.parts.get(3) {
            if is_where_clause(heap, *form)? {
                body_start = 4;
            }
        }

        let mut optionals = Vec::with_capacity(optionals_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &optionals_raw {
            let default = self.check_opt_key_default(heap, interp, pname, decl_ty, default_raw.clone())?;
            optionals.push(OptKeyParam { name: pname.clone(), decl_ty: decl_ty.clone(), default });
        }
        let mut keys = Vec::with_capacity(keys_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &keys_raw {
            let default = self.check_opt_key_default(heap, interp, pname, decl_ty, default_raw.clone())?;
            keys.push(OptKeyParam { name: pname.clone(), decl_ty: decl_ty.clone(), default });
        }

        let mut params: Vec<(String, Type)> = required.clone();
        for o in &optionals {
            params.push((o.name.clone(), o.effective_ty()));
        }
        if let Some((rname, _, _)) = &rest {
            params.push((rname.clone(), option_of_sexpr()));
        }
        for k in &keys {
            params.push((k.name.clone(), k.effective_ty()));
        }

        let env = Env::new().extended(params.clone());
        let (body, _) = self.check_seq(heap, interp, &env, &tmpl.parts[body_start..], &[], Some(&ret))?;
        self.defun_form(heap, mangled, &params, &ret, public, &body)
    }

    /// Whether a declared type's *runtime representation* is a heap value —
    /// the built-in `Sexpr`
    /// itself, any `AdtKind::Struct` type (`defstruct`/`Vector<T>`/
    /// `cons-cell<K,V>`), `HashTable<K,V>` (boxed since the unification's
    /// Stage 5, though its `AdtDef` still says `Sum` — a recorded historical
    /// asymmetry), any `Scope<V>`, any trait object, a `random-state`, or any
    /// enum instantiation (`Option`/`Result`/`Error`/user `defenum`).
    ///
    /// Flat facts, all of them. This used to recurse twice — on a `Scope`'s
    /// element type and on an enum's variant field types — because a value
    /// whose parts the heap could not carry had to fall back to a Rust-side
    /// representation, and the routing decision had to predict that. Both
    /// fallbacks are gone (Phase 1a for `Scope<V>`, `RtValue::Data`'s deletion
    /// for enums), so both recursions are too.
    ///
    /// The checker-side twin of `Interp::is_heap_repr_ty`, used to bake
    /// binding-slot routing into `Pattern::Bind` (the one binding site whose
    /// type the evaluator can't read off its own AST node).
    fn is_heap_repr(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(p, args) if *p == Path::root("scope") && args.len() == 1 => true,
            Type::Named(p, _) if *p == Path::root("sexpr") || *p == Path::root("hashtable") => true,
            Type::Named(p, _) => match self.reg.type_def(p) {
                Some(d) if d.kind == AdtKind::Struct => true,
                Some(d) if d.kind == AdtKind::Sum && !d.variants.is_empty() => true,
                _ => false,
            },
            // A trait object is always a `BoxedObj::Dyn` fat box, whatever
            // the concrete value inside is — so it is heap-repr by
            // construction. (Not covered by the `_` arm below, which would
            // silently answer `false` and make every `Type::Dyn` unstorable
            // in a `Sexpr`.)
            Type::Dyn(..) => true,
            // `random-state`/`bignum`/`ratio`/`f64`/`string` are heap values
            // with no `Type::Named` spelling — see `Interp::is_heap_repr_ty`'s
            // matching arm for why routing these to a heap slot is a
            // correctness requirement rather than a choice.
            Type::RandomState | Type::Bignum | Type::Ratio | Type::F64 | Type::Str => true,
            _ => false,
        }
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



    /// `(where (Trait1 T1 (Assoc1 Concrete1)...) (Trait2 T2)...)`: trait
    /// bounds on a generic `defun`'s type parameters, keyed by parameter
    /// name, each optionally pinning one or more of that trait's associated
    /// types to a concrete type — see `TraitBound`'s doc comment for how
    /// pins are used. Trait names are resolved as bare root-module paths
    /// (`Path::root`, not `Checker::resolve_trait_name`), mirroring
    /// `Checker::check_instance_method`'s existing assumption that every
    /// trait lives at the root module — a pre-existing limitation, not new
    /// here.
    fn parse_where_clause(&self, heap: &Heap, v: Value) -> Result<BTreeMap<String, Vec<TraitBound>>, Error> {
        let elems = heap.list_to_vec(v)?;
        let mut bounds: BTreeMap<String, Vec<TraitBound>> = BTreeMap::new();
        for clause in &elems[1..] {
            let parts_locs = heap.list_to_vec_locs(*clause)?;
            let parts: Vec<Value> = parts_locs.iter().map(|(v, _)| *v).collect();
            if parts.len() < 2 {
                return Err(Error::TypeError(
                    "where: each bound must be (Trait type-param (AssocName Type)...)".into(),
                ));
            }
            // Resolved to the trait's fully-qualified path (not left as the
            // written bare name) so a bound compares equal to the same trait
            // reached any other way — `Type::Dyn`'s head, an `impl`'s trait,
            // `TraitDef::name` — all of which are qualified. Inside a module
            // (every file is one) a bare `err2` would otherwise be stored as
            // root `err2` and match none of them.
            let trait_name = match parts[0] {
                Value::Symbol(id) => self.resolve_trait_name(heap.symbol_name(id))?,
                _ => return Err(Error::TypeError("where: trait name must be a symbol".into())),
            };
            if let Some(l) = parts_locs[0].1.as_ref() {
                self.record_trait_use(&trait_name, l.clone());
            }
            let tparam = match parts[1] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("where: type parameter must be a symbol".into())),
            };
            let assoc = self.parse_trait_bound_pins(heap, &parts[2..], "where")?;
            bounds.entry(tparam).or_default().push(TraitBound { trait_path: trait_name, assoc });
        }
        Ok(bounds)
    }

    /// The `(AssocName Type)...` tail shared by a `where` bound and a
    /// supertrait entry — the two spellings of one concept, so they parse
    /// through one function. `what` names the enclosing form for diagnostics.
    fn parse_trait_bound_pins(
        &self,
        heap: &Heap,
        pins: &[Value],
        what: &str,
    ) -> Result<BTreeMap<String, Type>, Error> {
        let mut assoc: BTreeMap<String, Type> = BTreeMap::new();
        for pin in pins {
            let pin_locs = heap.list_to_vec_locs(*pin)?;
            let pin_parts: Vec<Value> = pin_locs.iter().map(|(v, _)| *v).collect();
            if pin_parts.len() != 2 {
                return Err(Error::TypeError(format!(
                    "{}: associated-type pin must be (AssocName Type)",
                    what
                )));
            }
            let aname = match pin_parts[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => {
                    return Err(Error::TypeError(format!(
                        "{}: associated type name must be a symbol",
                        what
                    )))
                }
            };
            let aty = self.parse_type_here_at(heap, pin_parts[1], pin_locs[1].1.as_ref())?;
            assoc.insert(aname, aty);
        }
        Ok(assoc)
    }

    /// `deftrait`'s mandatory supertrait slot: `()`, or a list whose elements
    /// are each a bare trait name or `(Trait (Assoc Type)...)`. A supertrait
    /// is Rust's `trait Sub where Self: Super`, so this is exactly a `where`
    /// bound with the type-variable slot dropped.
    ///
    /// Every associated type of a supertrait must be pinned here. Without
    /// that, a `:dyn Sub<...>`'s pins could not be composed down the chain to
    /// give an inherited method's signature a concrete meaning, and
    /// `TraitDef::assoc_types` would have to grow the inherited names —
    /// changing `:dyn` pin arity for every existing trait. The pin's type may
    /// reference `Sub`'s *own* associated types (`(deftrait Collection (type
    /// Elem) ((Iter (Item Elem))) ...)`), which resolve when the `:dyn`'s own
    /// pins are substituted.
    fn parse_supertrait_list(
        &self,
        heap: &Heap,
        name: &str,
        v: Value,
    ) -> Result<Vec<TraitBound>, Error> {
        let elem_locs = heap.list_to_vec_locs(v).map_err(|_| {
            Error::TypeError(format!(
                "deftrait {}: the supertrait list must be a list — write `()` for none",
                name
            ))
        })?;
        let mut out: Vec<TraitBound> = Vec::new();
        for (elem, loc) in &elem_locs {
            // A bare symbol is the no-pins spelling; a list is `(Trait pins...)`.
            let (head, head_loc, pins) = match elem {
                Value::Symbol(_) => (*elem, loc.clone(), Vec::new()),
                _ => {
                    let parts_locs = heap.list_to_vec_locs(*elem)?;
                    let Some((head, head_loc)) = parts_locs.first().cloned() else {
                        return Err(Error::TypeError(format!(
                            "deftrait {}: a supertrait must be a trait name or (Trait (Assoc Type)...)",
                            name
                        )));
                    };
                    let pins = parts_locs[1..].iter().map(|(v, _)| *v).collect();
                    (head, head_loc, pins)
                }
            };
            let trait_path = self.resolve_supertrait_name(heap, name, head, head_loc.as_ref())?;
            let assoc = self.parse_trait_bound_pins(heap, &pins, "deftrait")?;
            if let Some(sdef) = self.reg.trait_def(&trait_path) {
                for a in &sdef.assoc_types {
                    if !assoc.contains_key(a) {
                        return Err(Error::TypeError(format!(
                            "deftrait {}: supertrait `{}` has an associated type `{}`, which must be pinned here — \
                             write `({} ({} <type>))`",
                            name, trait_path, a, trait_path, a
                        )));
                    }
                }
            }
            Self::push_supertrait(&mut out, name, trait_path, assoc)?;
        }
        Ok(out)
    }

    /// Resolve one supertrait name, with a diagnostic that points at the
    /// mandatory-slot rule — the overwhelmingly likely mistake is a
    /// pre-supertrait `(deftrait Name (method ...) ...)` whose first item got
    /// read as the supertrait list.
    fn resolve_supertrait_name(
        &self,
        heap: &Heap,
        name: &str,
        v: Value,
        loc: Option<&Loc>,
    ) -> Result<Path, Error> {
        let written = match v {
            Value::Symbol(id) => heap.symbol_name(id),
            _ => {
                return Err(Error::TypeError(format!(
                    "deftrait {}: supertrait name must be a symbol",
                    name
                )))
            }
        };
        let path = self.resolve_trait_name(written).map_err(|_| {
            Error::TypeError(format!(
                "deftrait {}: unknown supertrait `{}` — the form is (deftrait Name (Super...) items...), \
                 so the list right after the name is the supertrait list (write `()` for none)",
                name, written
            ))
        })?;
        if let Some(l) = loc {
            self.record_trait_use(&path, l.clone());
        }
        Ok(path)
    }

    /// Append a direct supertrait, rejecting a repeat of the same trait
    /// (whose second set of pins could disagree with the first, leaving no
    /// single substitution for its inherited methods).
    fn push_supertrait(
        out: &mut Vec<TraitBound>,
        name: &str,
        trait_path: Path,
        assoc: BTreeMap<String, Type>,
    ) -> Result<(), Error> {
        if out.iter().any(|b| b.trait_path == trait_path) {
            return Err(Error::TypeError(format!(
                "deftrait {}: `{}` is listed as a supertrait twice",
                name, trait_path
            )));
        }
        out.push(TraitBound { trait_path, assoc });
        Ok(())
    }

    /// Lay out the trait's vtable: every transitively inherited method first
    /// (supertraits in written order, first occurrence wins), then the
    /// trait's own methods. Each supertrait's `vtable_order` is already
    /// transitive, so one pass over the direct supertraits suffices — no
    /// fixpoint, and no cycle check, because a supertrait must already be
    /// fully defined to have been resolvable at all.
    ///
    /// Returns the names and, in step, the trait that declares each.
    fn linearize_vtable(
        &self,
        name: &str,
        fq_name: &Path,
        supertraits: &[TraitBound],
        method_order: &[String],
    ) -> Result<(Vec<String>, Vec<Path>), Error> {
        let mut order: Vec<String> = Vec::new();
        let mut owner: Vec<Path> = Vec::new();
        for sup in supertraits {
            let Some(sdef) = self.reg.trait_def(&sup.trait_path) else {
                return Err(Error::TypeError(format!(
                    "deftrait {}: unknown supertrait `{}`",
                    name, sup.trait_path
                )));
            };
            for (m, decl) in sdef.vtable_order.iter().zip(sdef.vtable_owner.iter()) {
                match order.iter().position(|x| x == m) {
                    // A diamond (`D(B,C)`, `B(A)`, `C(A)`): `A`'s methods
                    // reach here twice, declared by `A` both times, so the
                    // second visit is a no-op and `A` keeps one slot.
                    Some(i) if &owner[i] == decl => {}
                    Some(i) => {
                        return Err(Error::TypeError(format!(
                            "deftrait {}: method `{}` is inherited from both `{}` and `{}` — \
                             a vtable has one slot per name",
                            name, m, owner[i], decl
                        )))
                    }
                    None => {
                        order.push(m.clone());
                        owner.push(decl.clone());
                    }
                }
            }
        }
        for m in method_order {
            if let Some(i) = order.iter().position(|x| x == m) {
                return Err(Error::TypeError(format!(
                    "deftrait {}: method `{}` is already inherited from `{}` — \
                     redeclaring it would need a way to say which one a call means, and there is none",
                    name, m, owner[i]
                )));
            }
            order.push(m.clone());
            owner.push(fq_name.clone());
        }
        Ok((order, owner))
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
    ) -> Result<TopLevelForm, Error> {
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
            parse_macro_lambda_list(heap, &param_vals)?;
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
        let doc = take_leading_docstring(heap, parts, 2);
        let body_start = if doc.is_some() { 3 } else { 2 };
        if let Some(doc) = doc {
            self.reg.docs.macros.insert(fq_name.clone(), doc);
        }

        // Every parameter — and the macro's implicit result — is an
        // S-expression, which is `Option<Sexpr>`: a macro is handed the forms
        // as written, and `()` is one of them. A
        // default-value form is checked against it in an environment
        // holding exactly the params bound *before* it (CL: an optional/key
        // default may reference earlier params, never later ones), which is
        // also the order `bind_macro_args` evaluates them in.
        let sexpr_ty = option_of_sexpr();
        let mut bindings: Vec<(String, Type)> =
            required_names.iter().map(|n| (n.clone(), sexpr_ty.clone())).collect();

        let mut opt_defaults: Vec<Vec<Value>> = Vec::with_capacity(optionals.len());
        for (n, default) in &optionals {
            let checked = self.check_macro_default(heap, interp, &bindings, *default, &sexpr_ty)?;
            opt_defaults.push(checked);
            bindings.push((n.clone(), sexpr_ty.clone()));
        }
        if let Some(r) = &rest_name {
            bindings.push((r.clone(), sexpr_ty.clone()));
        }
        let mut key_defaults: Vec<(String, Vec<Value>)> = Vec::with_capacity(key_specs.len());
        for (n, default) in &key_specs {
            let checked = self.check_macro_default(heap, interp, &bindings, *default, &sexpr_ty)?;
            key_defaults.push((n.clone(), checked));
            bindings.push((n.clone(), sexpr_ty.clone()));
        }

        let env = Env::new().extended(bindings);
        let body_locs = parts_locs.get(body_start..).unwrap_or(&[]);
        let (body, _) = self.check_seq(heap, interp, &env, &parts[body_start..], body_locs, Some(&sexpr_ty))?;
        let lambda = MacroLambda { required: required_names.len(), optionals: opt_defaults, keys: key_defaults };
        forms::defmacro_form(heap, &fq_name, &params, rest, &lambda, public, &body)
    }

    /// Check one `&optional`/`&key` default-value form (or none) against
    /// `Sexpr`, in an environment holding the params bound before it. Returns
    /// the checked body as a `Vec<Value>` — a single-element vector for a
    /// supplied default, or empty for "no default" (binds `nil` at expansion).
    fn check_macro_default(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        bindings: &[(String, Type)],
        default: Option<Value>,
        sexpr_ty: &Type,
    ) -> Result<Vec<Value>, Error> {
        match default {
            None => Ok(Vec::new()),
            Some(form) => {
                let env = Env::new().extended(bindings.to_vec());
                let checked = self.check(heap, interp, &env, form, Some(sexpr_ty))?;
                Ok(vec![checked.form])
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
                let name = Checker::macro_param_name(heap, items[0], section)?;
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
            .position(|p| matches!(p, Value::Symbol(id) if *id == SymId::REST));
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
            out.push((name, self.parse_type_here_at(heap, pair[1], pair_locs.get(1).and_then(|(_, l)| l.as_ref()))?));
            locs.push(pair_locs.first().and_then(|(_, l)| l.clone()));
        }
        Ok((out, locs))
    }


    /// An `&optional`/`&key` item for `defun`/`defmethod`: `(name type)` or
    /// `(name type default-form)`. Returns `(name, type, name's loc, default
    /// raw form)` — the default form is left unchecked (this is a
    /// `&Heap`-only parse helper, like `Self::macro_opt_key_spec`);
    /// `Checker::check_defun_opt_key` / `Checker::check_defmethod_in` check
    /// it afterwards. `what` is the defining form's own keyword, so the
    /// error a user sees names the form they actually wrote.
    fn parse_opt_key_spec(&self, heap: &Heap, v: Value, what: &str, section: &str) -> Result<(String, Type, Option<Loc>, Option<Value>), Error> {
        let items_locs = heap.list_to_vec_locs(v)?;
        if items_locs.len() < 2 || items_locs.len() > 3 {
            return Err(Error::TypeError(format!(
                "{}: {} parameter must be `(name type)` or `(name type default)`",
                what, section
            )));
        }
        let name = match items_locs[0].0 {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError(format!("{}: {} parameter name must be a symbol", what, section))),
        };
        let name_loc = items_locs[0].1.clone();
        let ty = self.parse_type_here_at(heap, items_locs[1].0, items_locs[1].1.as_ref())?;
        let default = items_locs.get(2).map(|(v, _)| *v);
        Ok((name, ty, name_loc, default))
    }

    /// The full `&optional`/`&rest`/`&key` breakdown of a `defun`'s
    /// parameter list, beyond its leading required `(name type)` params —
    /// see `Self::check_defun_opt_key`. Mirrors `Self::parse_macro_lambda_list`'s
    /// CL section order (`required &optional &rest &key`), but (unlike
    /// macros) `&key` may not combine with `&optional`/`&rest` in the same
    /// list: CL's own well-known ambiguity between a positionally-filled
    /// `&optional` and a label-matched `&key` argument when both sections
    /// are present (which of the two consumes a given trailing argument
    /// depends on its *value*, not just its position) — sidestepped
    /// entirely by forbidding the combination, matching how every
    /// motivating case (`sort`'s `&key`, a BOA constructor's `&optional`)
    /// uses only one of the two.
    #[allow(clippy::type_complexity)]
    fn parse_defun_params_full(&self, heap: &Heap, v: Value) -> Result<ParamsFull, Error> {
        let elems = heap.list_to_vec(v)?;
        self.parse_params_full(heap, &elems, "defun")
    }

    /// [`Self::parse_defun_params_full`] over an already-flattened parameter
    /// list, with the defining form's keyword for error messages. `defmethod`
    /// needs this shape: its parameter list's first item is the receiver, so
    /// what reaches here is a *slice* of the signature list, not the list
    /// itself.
    #[allow(clippy::type_complexity)]
    fn parse_params_full(&self, heap: &Heap, elems: &[Value], what: &str) -> Result<ParamsFull, Error> {
        let elems: Vec<Value> = elems.to_vec();
        let mut rank = 0u8;
        let mut required_raw: Vec<Value> = Vec::new();
        let mut optionals_raw: Vec<Value> = Vec::new();
        let mut rest_raw: Vec<Value> = Vec::new();
        let mut keys_raw: Vec<Value> = Vec::new();
        for p in &elems {
            if let Value::Symbol(id) = p {
                match *id {
                    SymId::OPTIONAL => {
                        if rank >= 1 {
                            return Err(Error::TypeError(format!("{}: &optional must precede &rest and &key, and appear once", what)));
                        }
                        rank = 1;
                        continue;
                    }
                    SymId::REST => {
                        if rank >= 2 {
                            return Err(Error::TypeError(format!("{}: &rest must precede &key, and appear once", what)));
                        }
                        rank = 2;
                        continue;
                    }
                    SymId::KEY => {
                        if rank >= 3 {
                            return Err(Error::TypeError(format!("{}: &key may appear only once", what)));
                        }
                        rank = 3;
                        continue;
                    }
                    _ => {}
                }
            }
            match rank {
                0 => required_raw.push(*p),
                1 => optionals_raw.push(*p),
                2 => rest_raw.push(*p),
                _ => keys_raw.push(*p),
            }
        }
        if rank == 2 && rest_raw.is_empty() {
            return Err(Error::TypeError(format!("{}: &rest must be followed by exactly one parameter", what)));
        }
        if rest_raw.len() > 1 {
            return Err(Error::TypeError(format!(
                "{}: &rest takes exactly one parameter, as the last item in the parameter list",
                what
            )));
        }
        if !keys_raw.is_empty() && (!optionals_raw.is_empty() || !rest_raw.is_empty()) {
            return Err(Error::TypeError(format!(
                "{}: &key cannot be combined with &optional/&rest in the same parameter list",
                what
            )));
        }

        let (required, required_locs) = self.parse_param_pairs(heap, &required_raw)?;
        let mut optionals = Vec::with_capacity(optionals_raw.len());
        for p in optionals_raw {
            optionals.push(self.parse_opt_key_spec(heap, p, what, "&optional")?);
        }
        let rest = match rest_raw.first() {
            Some(r) => {
                let (rp, rl) = self.parse_param_pairs(heap, std::slice::from_ref(r))?;
                let (rname, rty) = rp.into_iter().next().expect("checked non-empty above");
                Some((rname, rty, rl.into_iter().next().flatten()))
            }
            None => None,
        };
        let mut keys = Vec::with_capacity(keys_raw.len());
        for p in keys_raw {
            keys.push(self.parse_opt_key_spec(heap, p, what, "&key")?);
        }
        Ok((required, required_locs, optionals, rest, keys))
    }

    /// Parse `defstruct` field bindings: `(name type)` or `(pub name type)`,
    /// each optionally with a trailing default form (`(name type default)`).
    ///
    /// Field visibility is independent of the struct's own `pub` — like every
    /// other `pub` in this language it's opt-in per item, never inherited
    /// from a container — so a field defaults to private (its getter/setter
    /// only reachable from the struct's own module) even on a `pub`
    /// `defstruct`, and `pub` on a field of a non-`pub` struct is legal (the
    /// field is reachable cross-module on any value of that type the current
    /// module's own `pub` API happens to hand out, even though outside code
    /// can't name or construct the type itself).
    ///
    /// The default is *not* checked here (this is a `&Heap`-only parse
    /// helper, like [`Self::parse_opt_key_spec`]): it is spliced into a
    /// generated constructor's source and checked there, against that
    /// constructor's own parameter type.
    fn parse_struct_fields(&self, heap: &Heap, pairs: &[Value]) -> Result<Vec<StructField>, Error> {
        let mut out = Vec::new();
        for binding in pairs {
            let elem_locs = heap.list_to_vec_locs(*binding)?;
            let elems: Vec<Value> = elem_locs.iter().map(|(v, _)| *v).collect();
            let (public, rest, rest_locs) = match elems.first() {
                Some(Value::Symbol(id)) if *id == SymId::PUB => (true, &elems[1..], &elem_locs[1..]),
                _ => (false, &elems[..], &elem_locs[..]),
            };
            if rest.len() < 2 || rest.len() > 3 {
                return Err(Error::TypeError(
                    "defstruct: field must be (name type), (pub name type), or either with a default".into(),
                ));
            }
            let name = match rest[0] {
                Value::Symbol(id) => heap.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("defstruct: field name must be a symbol".into())),
            };
            out.push(StructField {
                name,
                ty: self.parse_type_here_at(heap, rest[1], rest_locs[1].1.as_ref())?,
                public,
                default: rest.get(2).copied().map(SlotDefault::Written),
            });
        }
        Ok(out)
    }

    /// The slots a `(:include Parent)` contributes, in the parent's own
    /// order: name, type (with the parent's type parameters substituted at
    /// the arguments written here), visibility, and default.
    ///
    /// The defaults come from [`Registry::struct_defaults`] rather than from
    /// the parent's `AdtDef`, so this works across a module boundary and
    /// across a dump reload — a `:include`d parent may have been defined in
    /// another file entirely.
    fn included_fields(&self, heap: &mut Heap, parent_form: Value, loc: Option<&Loc>) -> Result<Vec<StructField>, Error> {
        let parent_ty = self.parse_type_here_at(heap, parent_form, loc)?;
        let Type::Named(parent_path, args) = &parent_ty else {
            return Err(Error::TypeError(format!(
                "defstruct: (:include {:?}) — a struct is what can be included",
                parent_ty
            )));
        };
        let Some(def) = self.reg.type_def(parent_path) else {
            return Err(Error::TypeError(format!("defstruct: (:include {}) — unknown type", parent_path)));
        };
        if def.kind != AdtKind::Struct || def.variants.len() != 1 {
            return Err(Error::TypeError(format!(
                "defstruct: (:include {}) — only a `defstruct` has slots to include",
                parent_path
            )));
        }
        if args.len() != def.params.len() {
            return Err(Error::TypeError(format!(
                "defstruct: (:include {}) takes {} type argument(s), got {}",
                parent_path,
                def.params.len(),
                args.len()
            )));
        }
        let subst: BTreeMap<String, Type> = def.params.iter().cloned().zip(args.iter().cloned()).collect();
        let defaults = self.reg.struct_defaults.get(parent_path);
        let mut out = Vec::with_capacity(def.field_names.len());
        for (i, fname) in def.field_names.iter().enumerate() {
            // A slot's visibility lives on its accessor, which is where
            // `check_defstruct` put it.
            let public = def.assoc.get(fname).map(|af| af.sig.public).unwrap_or(false);
            let default = defaults
                .and_then(|d| d.get(i))
                .and_then(|d| d.as_ref())
                .map(|owned| SlotDefault::Inherited(owned.clone()));
            out.push(StructField {
                name: fname.clone(),
                ty: subst_apply(&def.variants[0].fields[i], &subst),
                public,
                default,
            });
        }
        Ok(out)
    }

    /// Parse a `(defstruct (Name option...) ...)` option list — everything
    /// after the name inside that leading list.
    ///
    /// Three CL options are refused outright rather than approximated,
    /// because what they are *for* does not exist here:
    ///
    /// - **`:conc-name`** prefixes CL's accessors so that two structs' slots
    ///   do not collide in one flat function namespace. Accessors here are
    ///   methods dispatched on the receiver's type, so there is nothing to
    ///   collide; worse, a prefix would break `instance::field`, which is the
    ///   language's own way to reach a slot and knows only the slot's name.
    /// - **`:predicate`** answers "is this value a `point`?" at runtime. A
    ///   type here is a compile-time classification with no runtime witness,
    ///   and there is no position where a value of unknown-but-possibly-point
    ///   type exists (`match` on `Sexpr` is fenced off, a `:dyn` cannot be
    ///   downcast) — so the generated predicate could only ever return
    ///   `true`.
    /// - **`:type` / `:initial-offset` / `:named`** re-lay the value out as a
    ///   list or vector. The representation is the compiler's (D1); nothing
    ///   in the language observes it.
    fn parse_struct_options(&self, heap: &Heap, opts: &[Value], opt_locs: &[Option<Loc>]) -> Result<StructOptions, Error> {
        let mut out = StructOptions::default();
        for (i, opt) in opts.iter().enumerate() {
            // A bare symbol here is almost always the *old* generic header,
            // `(defstruct (Pair A B) ...)`, which the option list now occupies
            // the syntax of. Say that, rather than "an option must be a list".
            if let Value::Symbol(id) = opt {
                let name = heap.symbol_name(*id);
                if !name.starts_with(':') {
                    return Err(Error::TypeError(format!(
                        "defstruct: `{}` is not an option — a definition name's type parameters go on the name itself (`Name<{}>`); the list after `defstruct` is the option list (`:constructor`, `:copier`, `:include`)",
                        name, name
                    )));
                }
            }
            let items = heap.list_to_vec(*opt).map_err(|_| {
                Error::TypeError("defstruct: each option must be a list, e.g. (:constructor make-point)".into())
            })?;
            let key = match items.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("defstruct: an option starts with a keyword, e.g. (:copier copy-point)".into())),
            };
            match key.as_str() {
                ":constructor" => out.constructors.push(self.parse_ctor_option(heap, &items[1..])?),
                ":copier" => {
                    let name = single_name_option(heap, &items[1..], ":copier")?;
                    if out.copier.is_some() {
                        return Err(Error::TypeError("defstruct: :copier may appear only once".into()));
                    }
                    out.copier = Some(name);
                }
                ":include" => {
                    if items.len() != 2 {
                        return Err(Error::TypeError("defstruct: (:include Parent) takes exactly one type".into()));
                    }
                    if out.include.is_some() {
                        return Err(Error::TypeError("defstruct: :include may appear only once".into()));
                    }
                    out.include = Some((items[1], opt_locs.get(i).and_then(|l| l.clone())));
                }
                ":conc-name" => {
                    return Err(Error::TypeError(
                        "defstruct: :conc-name has nothing to do here — accessors are methods dispatched on the receiver's type, so slot names never collide, and a prefix would break `instance::field`"
                            .into(),
                    ))
                }
                ":predicate" => {
                    return Err(Error::TypeError(
                        "defstruct: :predicate has nothing to answer — a type is a compile-time classification with no runtime witness, so the generated predicate could only ever return true"
                            .into(),
                    ))
                }
                ":type" | ":initial-offset" | ":named" => {
                    return Err(Error::TypeError(format!(
                        "defstruct: `{}` re-lays the value out as a list or vector; the representation belongs to the compiler here and nothing in the language observes it",
                        key
                    )))
                }
                other => {
                    return Err(Error::TypeError(format!(
                        "defstruct: unknown option `{}` (known: :constructor, :copier, :include)",
                        other
                    )))
                }
            }
        }
        Ok(out)
    }

    /// `(:constructor name)` or `(:constructor name (slot... [&optional slot...]))`.
    fn parse_ctor_option(&self, heap: &Heap, rest: &[Value]) -> Result<CtorSpec, Error> {
        let name = match rest.first() {
            Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
            _ => {
                return Err(Error::TypeError(
                    "defstruct: (:constructor name) needs a name — `new` is the structural constructor and is always present, so there is nothing to suppress".into(),
                ))
            }
        };
        let boa = match rest.len() {
            1 => None,
            2 => {
                let items = heap.list_to_vec(rest[1])?;
                let mut params = Vec::new();
                let mut optional = false;
                for it in &items {
                    let Value::Symbol(id) = it else {
                        return Err(Error::TypeError(
                            "defstruct: a BOA constructor's lambda list holds slot names, not types — each slot's type is its declaration's".into(),
                        ));
                    };
                    let n = heap.symbol_name(*id);
                    if n == "&optional" {
                        if optional {
                            return Err(Error::TypeError("defstruct: &optional may appear only once".into()));
                        }
                        optional = true;
                        continue;
                    }
                    if n.starts_with('&') {
                        return Err(Error::TypeError(format!(
                            "defstruct: a BOA constructor's lambda list takes slot names and `&optional`, not `{}`",
                            n
                        )));
                    }
                    params.push(BoaParam { name: n.to_string(), optional });
                }
                Some(params)
            }
            _ => {
                return Err(Error::TypeError(
                    "defstruct: (:constructor name) or (:constructor name (slot...))".into(),
                ))
            }
        };
        Ok(CtorSpec { name, boa })
    }

    // ---- deftrait / impl ---------------------------------------------------

    /// `(deftrait Name (Super...) (type AssocName)... (method-name ((self Self) params...) Ret)...)`:
    /// declares a trait as a set of method signature *templates* (no
    /// bodies) — `Self` and any declared associated type name are usable as
    /// ordinary type variables in a signature, exactly like a generic
    /// `defstruct`'s own type parameters (`Checker::parse_struct_fields`).
    /// Registers a [`TraitDef`]; `Checker::check_impl` later supplies bodies
    /// for some concrete implementing type. A bodyless signature template is
    /// not type-checked here beyond parsing — its `Self`/associated-type
    /// variables aren't real types, so there's nothing to check yet — but a
    /// method that *has* a default body has that body checked right away, by
    /// [`Self::precheck_trait_defaults`].
    ///
    /// The supertrait list is a *mandatory* positional slot (write `()` for
    /// none), which is what keeps the trait name the first symbol after
    /// `deftrait` — both editors' type-name highlighting depends on that.
    /// Each element is a bare trait name, or `(Trait (Assoc Type)...)` when
    /// the supertrait has associated types to pin: exactly
    /// `Checker::parse_where_clause`'s bound shape minus the type-variable
    /// slot, since a supertrait's variable is always `Self`.
    fn check_deftrait(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError(
                "deftrait: (deftrait Name (Super...) (type AssocName)... (method (params...) ret)...) \
                 — the supertrait list is required; write `()` for none"
                    .into(),
            ));
        }
        let name = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("deftrait: name must be a symbol".into())),
        };
        let fq_name = self.fq(&name);
        // A docstring, right after the supertrait list and before the item
        // list — one per trait (CL's `defgeneric` docstring lives on the
        // generic function as a whole, not per method; individual `impl`
        // methods get their own via `Checker::check_defmethod`'s leading
        // docstring instead).
        let (doc, item_start) = match parts.get(2) {
            Some(Value::Str(id)) => (Some(heap.string(*id).to_string()), 3),
            _ => (None, 2),
        };
        self.check_redef("trait", &name, self.cur_ns().traits.get(&name))?;
        self.check_type_trait_clash("trait", &name)?;
        // Parse the supertraits *before* the self-referential stub goes in,
        // so `(deftrait Foo (Foo) ...)` fails on an unknown trait rather than
        // resolving against the empty stub and silently self-inheriting.
        let supertraits = self.parse_supertrait_list(heap, &name, parts[1])?;
        // Pre-register a stub under `fq_name` *before* parsing the method
        // signatures, so a method that mentions a trait object of the very
        // trait being defined (`(source ((self Self)) Option<:dyn Error>)` in
        // the prelude's `Error`) resolves its own name through
        // `Self::canon`/`resolve_trait_path`. Same device, same reason, as
        // `check_defstruct`'s self-referential-field stub; overwritten with
        // the fully parsed `TraitDef` below.
        self.reg.root.module_mut(&self.ns).traits.insert(
            name.clone(),
            TraitDef {
                name: fq_name.clone(),
                assoc_types: Vec::new(),
                methods: BTreeMap::new(),
                method_order: Vec::new(),
                public,
                builtin: false,
                supertraits: Vec::new(),
                vtable_order: Vec::new(),
                vtable_owner: Vec::new(),
                defaults: BTreeMap::new(),
            },
        );
        let mut assoc_types = Vec::new();
        let mut methods = BTreeMap::new();
        // Source order, kept alongside `methods` for vtable slot numbering.
        let mut method_order: Vec<String> = Vec::new();
        let mut defaults: BTreeMap<String, TraitDefault> = BTreeMap::new();
        // The same items as live `Value`s, for the declaration-time body check
        // below — which needs the written forms, not the `OwnedForm`s the
        // registry keeps for `impl` to replay.
        let mut default_written: Vec<(Vec<Value>, Vec<Option<Loc>>)> = Vec::new();
        for item in &parts[item_start..] {
            let elem_locs = heap.list_to_vec_locs(*item)?;
            let elems: Vec<Value> = elem_locs.iter().map(|(v, _)| *v).collect();
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
            if elems.len() < 3 {
                return Err(Error::TypeError(
                    "deftrait: method must be (name (params...) ret) or (name (params...) ret body...)".into(),
                ));
            }
            let (params, _param_locs) = self.parse_param_pairs(heap, &heap.list_to_vec(elems[1])?)?;
            let ret = self.parse_type_here_at(heap, elems[2], elem_locs[2].1.as_ref())?;
            // `(name (params) ret [where] [doc] body...)`. Everything past
            // `ret` is optional; with no body forms left this is an ordinary
            // bodyless signature, exactly as before defaults existed.
            let mut at = 3;
            let mut bounds = BTreeMap::new();
            if let Some(f) = elems.get(at) {
                if is_where_clause(heap, *f)? {
                    bounds = self.parse_where_clause(heap, *f)?;
                    at += 1;
                }
            }
            // The usual CL rule (`take_leading_docstring`): a string is a
            // docstring only when a body form follows, since a lone trailing
            // string is the default body's return value.
            let mdoc = take_leading_docstring(heap, &elems, at);
            if mdoc.is_some() {
                at += 1;
            }
            let sig = FnSig {
                type_params: vec![],
                params: params.iter().map(|(_, t)| t.clone()).collect(),
                ret,
                public: true,
                rest: None,
                builtin: false,
                bounds,
                optionals: Vec::new(),
                keys: Vec::new(),
            };
            // `method_order` is the vtable layout, so it must stay in step
            // with `methods` — a duplicated name would silently give the
            // trait two slots for one entry.
            if methods.contains_key(&head) {
                return Err(Error::TypeError(format!("deftrait: duplicate method `{}`", head)));
            }
            if at < elems.len() {
                // A default body: retain the item verbatim for `check_impl`
                // to replay. `elems` is the item minus its enclosing cell, so
                // it round-trips through `list_from_vec`/`list_to_vec`.
                let item = elems
                    .iter()
                    .map(|v| crate::owned_form::value_to_owned(heap, *v))
                    .collect::<Result<Vec<_>, _>>()?;
                defaults.insert(head.clone(), TraitDefault { item, ns: self.ns.clone() });
                default_written
                    .push((elems.clone(), elem_locs.iter().map(|(_, l)| l.clone()).collect()));
            }
            if let Some(d) = mdoc {
                self.reg.docs.trait_methods.insert((fq_name.clone(), head.clone()), d);
            }
            method_order.push(head.clone());
            methods.insert(head, sig);
        }
        let (vtable_order, vtable_owner) =
            self.linearize_vtable(&name, &fq_name, &supertraits, &method_order)?;
        let assoc_names = assoc_types.clone();
        self.reg.root.module_mut(&self.ns).traits.insert(
            name.clone(),
            TraitDef {
                name: fq_name.clone(),
                assoc_types,
                methods,
                method_order,
                public,
                builtin: false,
                supertraits,
                vtable_order,
                vtable_owner,
                defaults,
            },
        );
        // After the real `TraitDef` is in place, never before: a default body
        // may call the trait's own (or an inherited) method on `self`, which
        // resolves through the entry just registered.
        self.precheck_trait_defaults(heap, interp, &fq_name, &assoc_names, &default_written)?;
        if let Some(loc) = def_loc {
            self.reg.def_locs.traits.insert(fq_name.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.traits.insert(fq_name.clone(), doc);
        }
        self.record_def_name_use(
            &fq_name,
            TypeKind::Trait,
            parts_locs.first().and_then(|l| l.as_ref()),
            name.chars().count(),
        );
        forms::module_form(heap, &fq_name, &[])
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
    ///
    /// Every method registered here is **public**, and there is no way to ask
    /// for anything else: `pub` may not be attached to an `impl`
    /// (`docs/syntax.md` §pub), so a private trait impl is not something a
    /// program can express — and Rust, whose visibility rules this follows,
    /// has no such thing either (a trait's methods are callable wherever the
    /// value is). Registering them privately instead made a trait method
    /// unusable from outside its type's module, including from the trait's
    /// *own* default bodies, which are checked in the trait's namespace
    /// (`Checker::check_defmethod_in`'s `body_ns`) and so stood outside the
    /// implementing module by construction.
    fn check_impl(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value], parts_locs: &[Option<Loc>]) -> Result<TopLevelForm, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError("impl: (impl TraitName TargetType (type AssocName Type)... (method ...)...)".into()));
        }
        // A bare name searches the ancestor module chain; a `::` path names
        // the module outright, which is the only way to implement a trait
        // that lives somewhere other than an ancestor of the `impl`.
        let segs = path_to_segs(heap, parts[0])
            .map_err(|_| Error::TypeError("impl: trait name must be a name or `::` path".into()))?;
        let written = Path::from_segments(segs.clone());
        let trait_fq = self.resolve_trait_path(&written).ok_or_else(|| {
            Error::TypeError(format!("impl: unknown trait `{}`", segs.join("::")))
        })?;
        if let Some(l) = parts_locs.first().and_then(|l| l.as_ref()) {
            self.record_trait_use(&trait_fq, l.clone());
        }
        let target_ty = self.parse_type_here_at(heap, parts[1], parts_locs.get(1).and_then(|l| l.as_ref()))?;
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
        let mut assoc_concrete: BTreeMap<String, Type> = BTreeMap::new();

        // An optional impl-level `(where ...)`, right after the target type:
        // bounds that hold for *every* method of this `impl`, so the owner's
        // type parameters are constrained once instead of on each method.
        // Merged into each method's own clause below, textually, so the
        // retained `MethodTemplate::Form` carries them into specialization
        // too — a parameter would have to be threaded through
        // `specialize_method_form` as well.
        let mut item_start = 2;
        let mut impl_where: Option<Value> = None;
        if let Some(form) = parts.get(2) {
            if is_where_clause(heap, *form)? {
                impl_where = Some(*form);
                item_start = 3;
            }
        }

        let mut method_forms: Vec<Value> = Vec::new();
        for item in &parts[item_start..] {
            let elem_locs = heap.list_to_vec_locs(*item)?;
            let elems: Vec<Value> = elem_locs.iter().map(|(v, _)| *v).collect();
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
                assoc_concrete.insert(aname.clone(), self.parse_type_here_at(heap, elems[2], elem_locs[2].1.as_ref())?);
                subst.insert(aname, elems[2]);
                continue;
            }
            method_forms.push(*item);
        }

        // Fill in every trait method this `impl` left out but the trait gave
        // a default body. Appended *after* the written ones so a default that
        // calls a sibling method (`(not-equals ... (not (equals self other)))`)
        // finds the `impl`'s own `equals` already registered.
        let written: Vec<String> = method_forms
            .iter()
            .map(|m| match heap.list_to_vec(*m)?.first() {
                Some(Value::Symbol(id)) => Ok(heap.symbol_name(*id).to_string()),
                _ => Err(Error::TypeError("impl: method name must be a symbol".into())),
            })
            .collect::<Result<_, Error>>()?;
        let defaulted: Vec<(String, Vec<crate::owned_form::OwnedForm>, Vec<String>)> = self
            .reg
            .trait_def(&trait_fq)
            .map(|t| {
                t.method_order
                    .iter()
                    .filter(|m| !written.contains(m))
                    .filter_map(|m| {
                        t.defaults.get(m).map(|d| (m.clone(), d.item.clone(), d.ns.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut default_ns: Vec<Option<Vec<String>>> = vec![None; method_forms.len()];
        for (_, item, ns) in &defaulted {
            let form = Self::owned_item_to_form(heap, item)?;
            // The written items are reachable from the enclosing top-level
            // form, which the caller roots; a synthesized one has no such
            // owner, and `check_defmethod` may retain it as a
            // `MethodTemplate::Form` besides.
            heap.push_permanent_root(form);
            method_forms.push(form);
            default_ns.push(Some(ns.clone()));
        }

        let mut body = Vec::new();
        for (mi, m) in method_forms.iter().enumerate() {
            let method_loc = heap.cons_loc(*m);
            let (new_elems, new_elems_locs) = self.subst_method_item(heap, *m, &subst, impl_where)?;
            let tl = self.check_defmethod_in(
                heap,
                interp,
                &new_elems,
                &new_elems_locs,
                true, // see this function's doc comment
                method_loc,
                default_ns[mi].clone(),
            )?;
            // Rooted as it lands: `body` is a `Vec<Value>`, which the collector
            // cannot see, and the *next* method's checking allocates heavily.
            // An unrooted earlier member is collected and its cell recycled, so
            // the module bundle ends up holding a fragment of it — observed as
            // `exec: not a top-level core form: (())` for `(impl charinput
            // two-way-stream)`, whose first method vanished while the two
            // defaulted ones behind it survived. Released with the rest of this
            // top-level form by `check_form_at`'s `truncate_roots`.
            body.push(forms::rooted(heap, tl));
        }
        self.check_impl_conformance(&trait_fq, &target_fq, &target_ty, &written, &assoc_concrete)?;
        self.check_supertrait_impls(&trait_fq, &target_fq, &assoc_concrete)?;
        if let Some(def) = self.reg.type_def_mut(&target_fq) {
            def.impls.push(trait_fq.clone());
            def.trait_assoc.insert(trait_fq, assoc_concrete);
        }
        forms::module_form(heap, &target_fq, &body)
    }

    /// Rewrite one `impl` method item's *type* positions through `subst`,
    /// returning the rebuilt `(method-name (recv params...) ret (where ...)?
    /// body...)` element list and its per-element locations — exactly the
    /// shape [`Self::check_defmethod_in`] and [`Self::parse_defmethod_sig`]
    /// consume.
    ///
    /// Substitution applies only to *type* positions — each receiver/
    /// parameter's type (never its bound *name*, which would collide with
    /// `subst`'s `"self"` key: the receiver is conventionally also named
    /// `self`, a plain variable, completely unrelated to the `Self` *type*
    /// keyword even though both case-fold to the same string), the return
    /// type, and the method's own `(where ...)`, which is merged with the
    /// impl-level clause into a single one. The body is left untouched: it's
    /// executable code, not type syntax, so any `Self`/`Item` appearing there
    /// is an ordinary (if confusingly named) variable/function reference, not
    /// something to substitute.
    ///
    /// Shared by [`Self::check_impl`], where `subst` maps `Self` to the
    /// concrete target type, and [`Self::precheck_blanket_impl`], where it
    /// maps `Self` to the target *variable*.
    fn subst_method_item(
        &self,
        heap: &mut Heap,
        m: Value,
        subst: &HashMap<String, Value>,
        impl_where: Option<Value>,
    ) -> Result<(Vec<Value>, Vec<Option<Loc>>), Error> {
        // `list_to_vec_locs` so the body forms (`elems[3..]`, left untouched
        // — see this method's doc comment) keep their original source
        // position; the rebuilt name/receiver/return slots
        // (`new_elems[0..3]`) have no natural location of their own.
        let elems_locs = heap.list_to_vec_locs(m)?;
        let elems: Vec<Value> = elems_locs.iter().map(|(v, _)| *v).collect();
        if elems.len() < 3 {
            return Err(Error::TypeError("impl: method must be (name (recv params...) ret body...)".into()));
        }
        let recv_pairs_locs = heap.list_to_vec_locs(elems[1])?;
        // Everything rebuilt below accumulates in Rust `Vec`s while later
        // rebuilds keep allocating, so each fresh value is rooted as it is
        // produced — a `Vec<Value>` is invisible to the collector.
        let mut heap = RootScope::new(heap);
        let heap = &mut *heap;
        let mut new_recv_pairs = Vec::new();
        for (pair, pair_loc) in &recv_pairs_locs {
            // A lambda-list marker reaches here as a bare symbol, and the
            // `(name type)` rebuild below would report it as an improper
            // list. Say what is actually wrong instead: a trait method's
            // arity is fixed by its vtable slot, so its `impl` cannot add
            // regions the `deftrait` (which has no syntax for them) did not
            // declare. `Checker::check_impl_conformance` makes the same
            // refusal for the other way in — an *inherent* method that a
            // later `impl` adopts as the trait's.
            if let Value::Symbol(id) = pair {
                let name = heap.symbol_name(*id).to_string();
                if name.starts_with('&') {
                    return Err(Error::TypeError(format!(
                        "impl: method `{}` may not declare {} — a trait method's arity is fixed by \
                         its vtable slot",
                        heap.symbol_name(match elems[0] {
                            Value::Symbol(n) => n,
                            _ => *id,
                        }),
                        name
                    )));
                }
            }
            let p_locs = heap.list_to_vec_locs(*pair)?;
            let p: Vec<Value> = p_locs.iter().map(|(v, _)| *v).collect();
            if p.len() != 2 {
                return Err(Error::TypeError("impl: receiver/parameter must be (name type)".into()));
            }
            let new_ty = Self::subst_value(heap, p[1], subst)?;
            heap.push_root(new_ty);
            // Keep each slot's original span when substitution left it
            // unchanged (the usual case for a concrete type annotation),
            // so the rebuilt signature still records/locates like the
            // written one; a substituted `Self` has no span of its own.
            let ty_loc = if new_ty == p[1] { p_locs[1].1.clone() } else { None };
            let pair_form = forms::list_from_vec_locs(heap, &[(p[0], p_locs[0].1.clone()), (new_ty, ty_loc)])?;
            heap.push_root(pair_form);
            new_recv_pairs.push((pair_form, pair_loc.clone()));
        }
        let new_recv_list = forms::list_from_vec_locs(heap, &new_recv_pairs)?;
        heap.push_root(new_recv_list);
        let new_ret = Self::subst_value(heap, elems[2], subst)?;
        heap.push_root(new_ret);
        let ret_loc = if new_ret == elems[2] { elems_locs[2].1.clone() } else { None };
        let own_where = match elems.get(3) {
            Some(f) if is_where_clause(heap, *f)? => Some(Self::subst_value(heap, *f, subst)?),
            _ => None,
        };
        if let Some(w) = own_where {
            heap.push_root(w);
        }
        let rest_start = if own_where.is_some() { 4 } else { 3 };
        let merged = self.merge_where_clauses(heap, impl_where, own_where, subst)?;
        if let Some(w) = merged {
            heap.push_root(w);
        }
        let mut new_elems = vec![elems[0], new_recv_list, new_ret];
        let mut new_elems_locs: Vec<Option<Loc>> = vec![None, None, ret_loc];
        if let Some(w) = merged {
            new_elems.push(w);
            new_elems_locs.push(None);
        }
        new_elems.extend_from_slice(&elems[rest_start..]);
        new_elems_locs.extend(elems_locs[rest_start..].iter().map(|(_, l)| l.clone()));
        Ok((new_elems, new_elems_locs))
    }

    /// Rebuild a retained [`TraitDefault::item`] as a live list.
    ///
    /// Every node allocates, and any allocation can collect, so each
    /// converted element is rooted as it is produced and the partially built
    /// spine is rooted across its own `cons` — the discipline the reader
    /// follows for exactly the same reason. Roots are popped LIFO on both the
    /// success and the error path.
    fn owned_item_to_form(heap: &mut Heap, item: &[crate::owned_form::OwnedForm]) -> Result<Value, Error> {
        let mut vals: Vec<Value> = Vec::with_capacity(item.len());
        for f in item {
            match crate::owned_form::owned_to_value(heap, f) {
                Ok(v) => {
                    heap.push_root(v);
                    vals.push(v);
                }
                Err(e) => {
                    for _ in 0..vals.len() {
                        heap.pop_root();
                    }
                    return Err(e);
                }
            }
        }
        let mut out = Ok(Value::Empty);
        for v in vals.iter().rev() {
            let Ok(tail) = out else { break };
            heap.push_root(tail);
            // core-build-ok: rebuilds a fasl-restored syntax form, not a core
            // node. Every element is rooted in `vals` above and the growing
            // tail is rooted across this call.
            out = heap.cons(*v, tail);
            heap.pop_root();
        }
        for _ in 0..vals.len() {
            heap.pop_root();
        }
        out
    }

    /// Fuse an `impl`-level `(where ...)` and a method's own into the single
    /// clause `Checker::parse_defmethod_sig` expects, impl-level bounds
    /// first. `None` when neither is present, so a method with no bounds is
    /// rebuilt exactly as before.
    fn merge_where_clauses(
        &self,
        heap: &mut Heap,
        impl_where: Option<Value>,
        own_where: Option<Value>,
        subst: &HashMap<String, Value>,
    ) -> Result<Option<Value>, Error> {
        if impl_where.is_none() && own_where.is_none() {
            return Ok(None);
        }
        // As in `subst_method_sig`: substituted bounds pile up in `items` while
        // later ones keep allocating, and a `Vec<Value>` is invisible to the
        // collector — so each one is rooted the moment it is built.
        let mut heap = RootScope::new(heap);
        let heap = &mut *heap;
        let mut items: Vec<(Value, Option<Loc>)> = Vec::new();
        let mut push_bounds = |v: Value, heap: &mut Heap, subst: &HashMap<String, Value>| -> Result<(), Error> {
            let elems = heap.list_to_vec_locs(v)?;
            if items.is_empty() {
                // Reuse the source clause's own `where` head symbol.
                items.push((elems[0].0, elems[0].1.clone()));
            }
            for (b, l) in &elems[1..] {
                let bound = Self::subst_value(heap, *b, subst)?;
                heap.push_root(bound);
                items.push((bound, l.clone()));
            }
            Ok(())
        };
        if let Some(w) = impl_where {
            push_bounds(w, heap, subst)?;
        }
        if let Some(w) = own_where {
            // Already substituted by the caller; pass an empty map so a bound
            // naming a variable called `self` isn't rewritten twice.
            push_bounds(w, heap, &HashMap::new())?;
        }
        Ok(Some(forms::list_from_vec_locs(heap, &items)?))
    }

    /// Verify that an `impl` actually implements its trait: every method
    /// present (or defaulted), every associated type bound, no strays, and
    /// each signature matching the trait's template.
    ///
    /// None of this existed before — `check_impl` never consulted the
    /// `TraitDef` at all, so a missing method went unnoticed until someone
    /// built a trait object of the type (`dyn_vtable_slots`), and a method
    /// with the wrong signature was simply registered as written. Both are
    /// caught here, at the `impl`, where the fix is.
    ///
    /// Only the trait's *own* methods are considered: a supertrait's are
    /// supplied by that trait's own `impl`, whose presence
    /// [`Self::check_supertrait_impls`] separately requires.
    fn check_impl_conformance(
        &self,
        trait_fq: &Path,
        target_fq: &Path,
        target_ty: &Type,
        written: &[String],
        assoc_concrete: &BTreeMap<String, Type>,
    ) -> Result<(), Error> {
        let Some(tdef) = self.reg.trait_def(trait_fq) else { return Ok(()) };
        for (i, m) in written.iter().enumerate() {
            if !tdef.method_order.contains(m) {
                let hint = if tdef.vtable_order.contains(m) {
                    " — it is inherited, so it belongs to the supertrait's own `impl`"
                } else {
                    ""
                };
                return Err(Error::TypeError(format!(
                    "impl {} {}: `{}` is not a method of `{}`{}",
                    trait_fq, target_fq, m, trait_fq, hint
                )));
            }
            if written[..i].contains(m) {
                return Err(Error::TypeError(format!(
                    "impl {} {}: method `{}` is given twice",
                    trait_fq, target_fq, m
                )));
            }
        }
        for a in &tdef.assoc_types {
            if !assoc_concrete.contains_key(a) {
                return Err(Error::TypeError(format!(
                    "impl {} {}: associated type `{}` is not bound — write `(type {} <type>)`",
                    trait_fq, target_fq, a, a
                )));
            }
        }
        // `Self` and the associated types, as this `impl` binds them, so a
        // template can be compared against what was actually registered.
        let mut subst: BTreeMap<String, Type> = assoc_concrete.clone();
        let Some(def) = self.reg.type_def(target_fq) else { return Ok(()) };
        // The *parsed* target type, not one rebuilt from `target_fq`: a
        // primitive target is `Type::I32`, never `Named("i32")`, and the two
        // do not compare equal even though they print the same.
        subst.insert("self".to_string(), target_ty.clone());
        for m in &tdef.method_order {
            let Some(af) = def.assoc.get(m) else {
                return Err(Error::TypeError(format!(
                    "impl {} {}: missing method `{}` (`{}` declares no default body for it)",
                    trait_fq, target_fq, m, trait_fq
                )));
            };
            let want = &tdef.methods[m];
            let want_params: Vec<Type> = want.params.iter().map(|t| subst_apply(t, &subst)).collect();
            let want_ret = subst_apply(&want.ret, &subst);
            let want_instance = want.params.first().is_some_and(is_self_tvar);
            let describe = |params: &[Type], ret: &Type| {
                format!(
                    "({}) {}",
                    params.iter().map(mangle_type).collect::<Vec<_>>().join(" "),
                    mangle_type(ret)
                )
            };
            if af.instance != want_instance {
                return Err(Error::TypeError(format!(
                    "impl {} {}: method `{}` is {}, but `{}` declares it {}",
                    trait_fq,
                    target_fq,
                    m,
                    if af.instance { "an instance method" } else { "a static function" },
                    trait_fq,
                    if want_instance { "an instance method" } else { "a static function" }
                )));
            }
            // A trait method's arity is fixed by its vtable slot: a `:dyn`
            // receiver's call site fills its arguments in from the *trait's*
            // declaration, a concrete receiver's from this `impl`'s, and the
            // two must be the same call. `deftrait` has no `&optional`/
            // `&key`/`&rest` syntax at all, so the only way they could
            // disagree is an `impl` declaring sections the trait doesn't —
            // rejected here rather than left to miscompile. (Without this the
            // conformance comparison below would pass: `FnSig::params` holds
            // only the *required* parameters.)
            if !af.sig.optionals.is_empty() || !af.sig.keys.is_empty() || af.sig.rest.is_some() {
                return Err(Error::TypeError(format!(
                    "impl {} {}: method `{}` may not declare &optional/&key/&rest — a trait method's \
                     arity is fixed by its vtable slot, and `{}` declares none",
                    trait_fq, target_fq, m, trait_fq
                )));
            }
            if af.sig.params != want_params || af.sig.ret != want_ret {
                return Err(Error::TypeError(format!(
                    "impl {} {}: method `{}` is `{}`, but `{}` declares `{}`",
                    trait_fq,
                    target_fq,
                    m,
                    describe(&af.sig.params, &af.sig.ret),
                    trait_fq,
                    describe(&want_params, &want_ret)
                )));
            }
        }
        Ok(())
    }

    /// The trait a blanket impl would supply `method` from for a receiver of
    /// type `ty`, if any — the trait must declare the method (its own or
    /// inherited), have a blanket impl, and cover `ty`.
    ///
    /// Returns the trait path; the caller reads the signature template off it
    /// and requests materialization.
    fn blanket_method_owner(&self, ty: &Type, type_fq: &Path, method: &str) -> Option<Path> {
        if self.reg.type_def(type_fq).is_none() {
            return None; // a bare type variable — the bounds branch handles it
        }
        fn walk(ns: &Namespace, out: &mut Vec<Path>) {
            out.extend(ns.blanket_impls.iter().map(|b| b.trait_path.clone()));
            for c in ns.modules.values() {
                walk(c, out);
            }
        }
        let mut candidates = Vec::new();
        walk(&self.reg.root, &mut candidates);
        if candidates.is_empty() {
            // The overwhelmingly common case: no blanket impl anywhere, so
            // this is a plain "no such method" and the tree walk above is the
            // only cost paid.
            return None;
        }
        // Sorted so the trait picked for a method two blankets could supply
        // does not depend on `HashMap` iteration order.
        candidates.sort_by_key(|p| p.to_string());
        let mut found: Option<Path> = None;
        for tp in candidates {
            let Some(tdef) = self.reg.trait_def(&tp) else { continue };
            if !tdef.vtable_order.iter().any(|m| m == method) {
                continue;
            }
            let tb = TraitBound { trait_path: tp.clone(), assoc: BTreeMap::new() };
            if self.type_implements(ty, &tb, 0) {
                found = Some(tp);
                break;
            }
        }
        found
    }

    /// Check a call to a method a blanket impl supplies, against the trait's
    /// signature template with `Self` bound to the receiver's type.
    ///
    /// The definition does not exist yet — `type_implements` has queued it —
    /// so this cannot go through `check_assoc_call`, which reads
    /// `AdtDef::assoc`. It emits the same `assoc` node that lookup
    /// would have, naming the method the materialization will register.
    #[allow(clippy::too_many_arguments)]
    fn check_blanket_method_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        receiver: Checked,
        type_fq: &Path,
        trait_path: &Path,
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let tdef = self
            .reg
            .trait_def(trait_path)
            .ok_or_else(|| Error::TypeError(format!("unknown trait `{}`", trait_path)))?;
        let (_, sig) = self.reg.trait_method(tdef, method).ok_or_else(|| {
            Error::TypeError(format!("`{}` is not a method of `{}`", method, trait_path))
        })?;
        let mut subst: BTreeMap<String, Type> = BTreeMap::new();
        subst.insert("self".to_string(), receiver.ty.clone());
        let want = sig.params.len() - 1;
        if args.len() != want {
            return Err(Error::TypeError(format!(
                "{}::{}: expected {} argument(s), got {}",
                trait_path,
                method,
                want,
                args.len()
            )));
        }
        let mut typed = vec![receiver];
        for (i, (a, pty)) in args.iter().zip(sig.params[1..].iter()).enumerate() {
            let expect = subst_apply(pty, &subst);
            typed.push(self.check_at(heap, interp, env, *a, Some(&expect), nth_loc(arg_locs, i))?);
        }
        let ty = subst_apply(&sig.ret, &subst);
        let home = self.ns.clone();
        let form = self.assoc_form(heap, type_fq, method, true, &home, &ty, &typed)?;
        Ok(Checked::new(form, ty))
    }

    /// Generate a blanket `impl` at one concrete target type.
    ///
    /// Rebuilds the stored items as a live `(impl Trait Target ...)` form and
    /// runs it through [`Self::check_impl`] — the same path a written `impl`
    /// takes, so completeness checking, the supertrait obligation, default
    /// bodies and `Self` substitution all apply identically, and there is no
    /// second implementation of "what an impl means" to drift from the first.
    /// Runs under the blanket's own namespace, since its method bodies were
    /// written there.
    fn materialize_blanket_impl(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        trait_path: &Path,
        target: &Type,
    ) -> Result<Vec<TopLevelForm>, Error> {
        let Some(b) = self.reg.blanket_impl(trait_path) else { return Ok(Vec::new()) };
        let (target_var, ns) = (b.target_var.clone(), b.ns.clone());
        let mut items: Vec<Vec<crate::owned_form::OwnedForm>> = Vec::new();
        for (aname, ty) in &b.assoc {
            items.push(vec![
                crate::owned_form::OwnedForm::Sym("type".into()),
                crate::owned_form::OwnedForm::Sym(aname.clone()),
                ty.clone(),
            ]);
        }
        items.extend(b.methods.iter().cloned());

        let target_written = Self::type_to_written_value(heap, target)?;
        heap.push_permanent_root(target_written);
        // `T` -> the target's written form, everywhere the items mention it.
        // `check_impl` will then substitute `Self` on top, to the same type.
        let mut subst: HashMap<String, Value> = HashMap::new();
        subst.insert(target_var, target_written);

        // As a `Value::Path` when qualified: a *symbol* spelled `a::b` is one
        // segment, and `check_impl` would look for a trait of that literal
        // name.
        let segs: Vec<String> = trait_path.segments().to_vec();
        let trait_v = crate::owned_form::owned_to_value(
            heap,
            &if segs.len() > 1 {
                crate::owned_form::OwnedForm::Path(segs)
            } else {
                crate::owned_form::OwnedForm::Sym(trait_path.last_segment().to_string())
            },
        )?;
        heap.push_permanent_root(trait_v);
        let mut parts: Vec<Value> = vec![trait_v, target_written];
        for item in &items {
            let form = Self::owned_item_to_form(heap, item)?;
            heap.push_permanent_root(form);
            parts.push(Self::subst_value(heap, form, &subst)?);
        }
        for &p in &parts {
            heap.push_permanent_root(p);
        }
        let locs: Vec<Option<Loc>> = vec![None; parts.len()];
        let saved = self.enter_specialization(ns, self.type_var_bindings.clone());
        let checked = self.check_impl(heap, interp, &parts, &locs);
        let (sn, sl, sb) = saved;
        self.exit_specialization(sn, sl, sb);
        let checked = checked?;
        // `check_impl` wraps its methods in a `module` purely as a grouping
        // device; the caller wants the definitions themselves.
        match core::op_sym(heap, checked) {
            Some(SymId::MODULE) => Ok(core::fields(heap, checked)?.split_off(1)),
            _ => Ok(vec![checked]),
        }
    }

    /// Does `ty` implement `tb`'s trait, with `tb`'s associated-type pins?
    ///
    /// An explicit `impl` is consulted first (`AdtDef::impls`, which the
    /// supertrait obligation keeps closed under inheritance), then the
    /// trait's blanket impl if it has one and `ty` satisfies its bounds.
    /// Answering via a blanket also *requests* the materialization, so the
    /// definitions exist by the time the form that asked is executed.
    ///
    /// An explicit impl always wins over a blanket. Rust makes that overlap a
    /// hard error; here it is a silent preference, because whether a blanket
    /// covers a type depends on bounds that a later form can still establish,
    /// so "do these overlap" has no answer at any single point in the file.
    /// Preferring the explicit one is at least order-independent.
    fn type_implements(&self, ty: &Type, tb: &TraitBound, depth: usize) -> bool {
        // Cut polymorphic recursion (`impl<T> A T (where (B T))` plus
        // `impl<T> B T (where (A T))`), which would otherwise ask the same
        // question one wrapper deeper forever.
        if depth > BLANKET_BOUND_DEPTH {
            return false;
        }
        let type_fq = match ty {
            Type::Named(n, _) => Some(n.clone()),
            other => prim_type_path(other),
        };
        if let Some(def) = type_fq.as_ref().and_then(|p| self.reg.type_def(p)) {
            if def.impls.contains(&tb.trait_path) {
                return true;
            }
        }
        let Some(blanket) = self.reg.blanket_impl(&tb.trait_path) else { return false };
        // The blanket's own bounds, read at `ty`: every bound on the target
        // variable must hold, recursively.
        // No bound on the target variable means the blanket covers every
        // type — `(impl<T> Show T)` with no `where` is Rust's `impl<T> Show
        // for T`, which applies unconditionally.
        let covered = blanket
            .bounds
            .get(&blanket.target_var)
            .map(|bs| bs.iter().all(|b| self.type_implements(ty, b, depth + 1)))
            .unwrap_or(true);
        if !covered {
            return false;
        }
        // Only a *closed* type can be materialized at: an open one is some
        // generic context's type variable (a blanket impl's own target, while
        // `Self::precheck_blanket_impl` checks its bodies, or a generic
        // `defun`'s parameter), and `check_impl` has no type to implement
        // anything for. Answering "yes, covered" for it is still right — every
        // concrete type it stands for is covered, and each of those requests
        // its own materialization when it arrives.
        if let Some(fq) = type_fq {
            if !self.type_is_open(ty) {
                self.request_blanket_materialization(&tb.trait_path, &fq, ty);
            }
        }
        true
    }

    /// Queue a blanket impl's materialization at one concrete type, deduped
    /// through the same `spec_memo` monomorphization uses.
    fn request_blanket_materialization(&self, trait_path: &Path, type_fq: &Path, ty: &Type) {
        let key = format!("blanket {} {}", trait_path, mangle_type(ty));
        if self.spec_memo.borrow_mut().insert((type_fq.clone(), Some(key))) {
            self.spec_pending.borrow_mut().push(SpecRequest::Blanket {
                trait_path: trait_path.clone(),
                target: ty.clone(),
            });
        }
    }

    /// Render a [`Type`] back as the source syntax that parses to it — the
    /// substitution value for `Self` when an `impl`'s method items are
    /// rebuilt.
    ///
    /// `check_impl` can use the target type *as written* because the user
    /// wrote it; a blanket materialization has no written form to reuse, only
    /// a resolved `Type`. `mangle_type` already produces exactly the type
    /// grammar for nominal and primitive types (`a::b<c,d>` is one symbol
    /// that `parse_type_name_rec` reads straight back), so this is that plus
    /// the shapes with no single-token spelling.
    fn type_to_written_value(heap: &mut Heap, t: &Type) -> Result<Value, Error> {
        match t {
            Type::Unit => Ok(Value::Empty),
            Type::Dyn(..) | Type::Fn(..) | Type::Never => Err(Error::TypeError(format!(
                "`{}` cannot be the target of a blanket impl — it is not a nominal type",
                mangle_type(t)
            ))),
            other => Ok(heap.intern_symbol(&mangle_type(other))),
        }
    }

    /// `(impl<T> Trait Target ...)` — an `impl` whose head declares type
    /// parameters.
    ///
    /// When `Target` is one of those parameters spelled bare, this is a
    /// *blanket* impl and takes [`Self::check_blanket_impl`]. When it is a
    /// type constructor applied to them (`(impl<T> Show Vector<T> ...)`),
    /// there is a single owning `AdtDef` and the ordinary path already
    /// handles it — the parameters are then only documentation, since
    /// `check_defmethod` infers them from the receiver.
    fn check_impl_generic(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        head_params: Vec<String>,
    ) -> Result<TopLevelForm, Error> {
        if parts.len() < 2 {
            return Err(Error::TypeError(
                "impl: (impl<T> TraitName Target (where ...) (method ...)...)".into(),
            ));
        }
        let target_var = match parts[1] {
            Value::Symbol(id) => {
                let n = heap.symbol_name(id).to_string();
                (head_params.contains(&n) && self.reg.type_def(&Path::root(&n)).is_none()).then_some(n)
            }
            _ => None,
        };
        match target_var {
            Some(v) => self.check_blanket_impl(heap, interp, parts, parts_locs, v, head_params),
            None => self.check_impl(heap, interp, parts, parts_locs),
        }
    }

    /// Record a blanket `impl`, registering nothing on any type.
    ///
    /// Nothing is *generated* here, and deliberately so: the target is a type
    /// variable, so every method body it holds is generic, and generating one
    /// before a concrete type asks for it would be exactly the eager
    /// expansion monomorphization exists to avoid. The bodies are replayed
    /// per covered type by [`Self::materialize_blanket_impl`].
    ///
    /// They are nonetheless *checked* once here, abstractly, the way Rust
    /// checks a blanket impl's bodies whether or not anything uses it — see
    /// [`Self::precheck_blanket_impl`] for what that pass can and cannot
    /// catch.
    fn check_blanket_impl(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        target_var: String,
        head_params: Vec<String>,
    ) -> Result<TopLevelForm, Error> {
        let segs = path_to_segs(heap, parts[0])
            .map_err(|_| Error::TypeError("impl: trait name must be a name or `::` path".into()))?;
        let written = Path::from_segments(segs.clone());
        let trait_fq = self
            .resolve_trait_path(&written)
            .ok_or_else(|| Error::TypeError(format!("impl: unknown trait `{}`", segs.join("::"))))?;
        if let Some(l) = parts_locs.first().and_then(|l| l.as_ref()) {
            self.record_trait_use(&trait_fq, l.clone());
        }
        // One blanket impl per trait. Coarser than Rust's overlap analysis —
        // it rejects two blankets whose bounds could never both apply — but
        // it is decidable without a whole-program view, which on-demand
        // materialization is never going to have.
        if let Some(prev) = self.reg.blanket_impl(&trait_fq) {
            return Err(Error::TypeError(format!(
                "impl<{}> {} {}: `{}` already has a blanket impl — a trait may have at most one, \
                 since which of two applies to a given type could depend on bounds established later",
                head_params.join(","),
                trait_fq,
                target_var,
                prev.trait_path
            )));
        }
        let mut at = 2;
        let mut bounds = BTreeMap::new();
        let mut impl_where = None;
        if let Some(f) = parts.get(2) {
            if is_where_clause(heap, *f)? {
                bounds = self.parse_where_clause(heap, *f)?;
                impl_where = Some(*f);
                at = 3;
            }
        }
        for tp in bounds.keys() {
            if !head_params.contains(tp) {
                return Err(Error::TypeError(format!(
                    "impl<{}> {}: `where` bounds `{}`, which is not one of the impl's type parameters",
                    head_params.join(","),
                    trait_fq,
                    tp
                )));
            }
        }
        let mut assoc = Vec::new();
        let mut methods = Vec::new();
        // The same items as live `Value`s, for the declaration-time body
        // check below — which needs the written forms, not the `OwnedForm`s
        // the registry keeps for later replay.
        let mut assoc_written: Vec<(String, Value)> = Vec::new();
        let mut method_forms: Vec<Value> = Vec::new();
        for item in &parts[at..] {
            let elems = heap.list_to_vec(*item)?;
            let head = match elems.first() {
                Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
                _ => return Err(Error::TypeError("impl: item must start with a symbol".into())),
            };
            let owned = elems
                .iter()
                .map(|v| crate::owned_form::value_to_owned(heap, *v))
                .collect::<Result<Vec<_>, Error>>()?;
            if head == "type" {
                if elems.len() != 3 {
                    return Err(Error::TypeError("impl: (type AssocName Type)".into()));
                }
                let aname = match elems[1] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("impl: associated type name must be a symbol".into())),
                };
                assoc_written.push((aname.clone(), elems[2]));
                assoc.push((aname, owned[2].clone()));
                continue;
            }
            method_forms.push(*item);
            methods.push(owned);
        }
        // Registered *before* the bodies are checked, so a method that calls
        // one of this very blanket's siblings on a covered type resolves —
        // the same "signature first, body second" order every definition form
        // here follows for self-recursion.
        self.reg.root.module_mut(&self.ns).blanket_impls.push(BlanketImpl {
            trait_path: trait_fq.clone(),
            target_var: target_var.clone(),
            bounds,
            assoc,
            methods,
            ns: self.ns.clone(),
        });
        self.precheck_blanket_impl(
            heap,
            interp,
            &trait_fq,
            &target_var,
            impl_where,
            &assoc_written,
            &method_forms,
        )?;
        let path = self.fq("impl");
        forms::module_form(heap, &path, &[])
    }

    /// Type-check a blanket `impl`'s method bodies at its declaration, with
    /// the target left abstract — what Rust does for `impl<T: Ord> Clamp for
    /// T { ... }`, whose bodies are checked once against the declared bounds
    /// whether or not any type ever reaches the impl.
    ///
    /// The bodies are checked exactly the way a generic `defun`'s body is:
    /// the target variable parses to an unresolved [`Type::Named`], and a
    /// method call on it resolves through `Env::bounds` to a diagnostics-only
    /// the unreachable-`panic` node ([`Self::check_instance_method`]'s bounds branch).
    /// Two bound sets are in scope, matching what every materialization will
    /// have: the impl's own `(where ...)` — already merged into each method's
    /// clause by [`Self::subst_method_item`] — and the trait being
    /// implemented, since inside `impl<T> Clamp T` the target *does* implement
    /// `Clamp`, so a method may call its siblings on `self`.
    ///
    /// Nothing checked here is kept: the checked bodies are dropped and no
    /// signature is registered anywhere (there is no type to register one
    /// on). [`Self::materialize_blanket_impl`] re-checks each body per covered
    /// type, where the target is concrete and the trait-call placeholders
    /// resolve to real methods; this pass catches only what is wrong for
    /// *every* target — but catches it at the impl, where the fix is, instead
    /// of at whichever unrelated form first happens to use a covered type.
    #[allow(clippy::too_many_arguments)]
    fn precheck_blanket_impl(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        trait_fq: &Path,
        target_var: &str,
        impl_where: Option<Value>,
        assoc: &[(String, Value)],
        methods: &[Value],
    ) -> Result<(), Error> {
        if methods.is_empty() {
            return Ok(());
        }
        // `Self` -> the target variable as written, and each associated type
        // -> what this impl binds it to (which may itself mention the target
        // variable) — the abstract counterpart of `check_impl`'s `subst`.
        let mut subst: HashMap<String, Value> = HashMap::new();
        subst.insert("self".to_string(), heap.intern_symbol(target_var));
        let mut pins: BTreeMap<String, Type> = BTreeMap::new();
        for (aname, written) in assoc {
            subst.insert(aname.clone(), *written);
            pins.insert(aname.clone(), self.parse_type_here_at(heap, *written, None)?);
        }
        let self_bound = TraitBound { trait_path: trait_fq.clone(), assoc: pins };
        for m in methods {
            let (elems, locs) = self.subst_method_item(heap, *m, &subst, impl_where)?;
            let sig = self.parse_defmethod_sig_inner(heap, &elems, &locs, true)?;
            let MethodSig { self_name, self_name_loc, recv_ty, params, param_locs, ret, mut bounds, body_start, .. } = sig;
            // The rebuilt header slots (`elems[..body_start]`: the receiver
            // list always, the return type and merged `where` when
            // substitution changed them) are freshly allocated and reachable
            // from nothing anyone else roots, while checking the body below
            // allocates — macro expansion, at least. Rooted permanently, like
            // the forms `materialize_blanket_impl` builds: the LIFO root
            // stack cannot express "alive until this check finishes" when a
            // macro expansion runs the interpreter in between. The body forms
            // are the caller's to keep alive, exactly as in `check_impl`.
            for &e in &elems[..body_start.min(elems.len())] {
                heap.push_permanent_root(e);
            }
            bounds.entry(target_var.to_string()).or_default().push(self_bound.clone());
            let mut binds: Vec<(String, Type, Option<Loc>)> = Vec::new();
            if let Some(s) = &self_name {
                binds.push((s.clone(), recv_ty.clone(), self_name_loc));
            }
            binds.extend(params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)));
            let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
            let body_locs = locs.get(body_start..).unwrap_or(&[]);
            self.check_seq(heap, interp, &env, &elems[body_start..], body_locs, Some(&ret))?;
        }
        Ok(())
    }

    /// Type-check a `deftrait`'s default method bodies at its declaration,
    /// with the receiver left abstract — what Rust does for a provided method
    /// in a `trait` block, whose body is checked once against `Self: Trait`
    /// whether or not any `impl` ever inherits it.
    ///
    /// This is [`Self::precheck_blanket_impl`] with the target variable fixed
    /// to `Self`, and it works for the same reason: in a trait's signatures
    /// `Self` is *already* a type variable ([`is_self_tvar`]), so the body
    /// checks exactly the way a generic `defun`'s does — a method call on
    /// `self` resolves through `Env::bounds` to a diagnostics-only
    /// an unreachable `panic`. The one bound needed is `Self: this trait`, which
    /// is what an implementor always satisfies; it covers the inherited
    /// methods too, since `Registry::trait_method` searches the supertrait
    /// chain.
    ///
    /// The trait's own associated types are pinned to themselves — `Item` is
    /// the type variable `Item` — so a sibling call declared to return `Item`
    /// agrees with a body that produces one, without either side knowing what
    /// `Item` will be.
    ///
    /// Nothing checked here is kept, and nothing is registered: the body is
    /// re-checked per `impl` that inherits it (`Checker::check_impl`'s
    /// defaulted-method fill-in), where `Self` is concrete and the trait-call
    /// placeholders resolve to real methods. This pass catches only what is
    /// wrong for *every* implementor — but catches it at the `deftrait`,
    /// where the fix is, instead of at whichever `impl` first omits the
    /// method, or nowhere at all when none ever does.
    fn precheck_trait_defaults(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        trait_fq: &Path,
        assoc_types: &[String],
        items: &[(Vec<Value>, Vec<Option<Loc>>)],
    ) -> Result<(), Error> {
        if items.is_empty() {
            return Ok(());
        }
        let pins = assoc_types
            .iter()
            .map(|a| (a.clone(), Type::Named(Path::root(a), vec![])))
            .collect();
        let self_bound = TraitBound { trait_path: trait_fq.clone(), assoc: pins };
        for (elems, locs) in items {
            let sig = self.parse_defmethod_sig_inner(heap, elems, locs, true)?;
            let MethodSig {
                self_name, self_name_loc, recv_ty, params, param_locs, ret, mut bounds, body_start, ..
            } = sig;
            // Keyed by the type variable's name, which for `Self` is `"self"`
            // — what `is_self_tvar` matches and what `Type::Named`'s single
            // segment holds once the reader has case-folded it.
            bounds.entry("self".to_string()).or_default().push(self_bound.clone());
            let mut binds: Vec<(String, Type, Option<Loc>)> = Vec::new();
            if let Some(s) = &self_name {
                binds.push((s.clone(), recv_ty.clone(), self_name_loc));
            }
            binds.extend(params.iter().cloned().zip(param_locs).map(|((n, t), l)| (n, t, l)));
            let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
            let body_locs = locs.get(body_start..).unwrap_or(&[]);
            self.check_seq(heap, interp, &env, &elems[body_start..], body_locs, Some(&ret))?;
        }
        Ok(())
    }

    /// Rust's `impl Ord for X` requires `impl Eq for X`: implementing a trait
    /// obliges the type to implement every supertrait, with associated types
    /// bound the way the supertrait entry pins them.
    ///
    /// Only the *direct* supertraits are checked — each one's own `impl`
    /// already discharged its parents, so the closure follows by induction.
    /// That is also what lets `Checker::validate_where_bounds` keep answering
    /// "does this type implement `Eq`?" with a flat `AdtDef::impls` lookup:
    /// the list is closed under supertraits.
    ///
    /// The rule is *textual precedence* — `impl Eq X` must be checked before
    /// `impl Ord X`. Stricter than Rust, and deliberately so: it is the only
    /// discharge point that is deterministic in the REPL, under incremental
    /// `load`, and across a fasl capture boundary, none of which have an
    /// end-of-program at which to settle deferred obligations.
    fn check_supertrait_impls(
        &self,
        trait_fq: &Path,
        target_fq: &Path,
        assoc_concrete: &BTreeMap<String, Type>,
    ) -> Result<(), Error> {
        let Some(tdef) = self.reg.trait_def(trait_fq) else { return Ok(()) };
        if tdef.supertraits.is_empty() {
            return Ok(());
        }
        let Some(def) = self.reg.type_def(target_fq) else { return Ok(()) };
        let targs: Vec<Type> = def.params.iter().map(|p| Type::Named(Path::root(p), vec![])).collect();
        for sup in &tdef.supertraits {
            if !def.impls.contains(&sup.trait_path) {
                return Err(Error::TypeError(format!(
                    "impl {} {}: `{}` requires `{}`, but `{}` has no `impl {}` — write it before this one",
                    trait_fq, target_fq, trait_fq, sup.trait_path, target_fq, sup.trait_path
                )));
            }
            // The supertrait entry pins its associated types, possibly in
            // terms of *this* trait's — which this `impl` has just bound.
            for (aname, pinned) in &sup.assoc {
                let want = subst_apply(pinned, assoc_concrete);
                if self.type_is_open(&want) {
                    continue;
                }
                match resolve_trait_assoc_type(def, &sup.trait_path, aname, &targs) {
                    Some(actual) if actual == want => {}
                    Some(actual) if self.type_is_open(&actual) => {}
                    Some(actual) => {
                        return Err(Error::TypeError(format!(
                            "impl {} {}: `{}` requires `{}`'s `{} = {}`, but `{}`'s `impl {}` binds it to `{}`",
                            trait_fq,
                            target_fq,
                            trait_fq,
                            sup.trait_path,
                            aname,
                            mangle_type(&want),
                            target_fq,
                            sup.trait_path,
                            mangle_type(&actual)
                        )))
                    }
                    None => {
                        return Err(Error::TypeError(format!(
                            "impl {} {}: `{}`'s `impl {}` does not bind the associated type `{}`",
                            trait_fq, target_fq, target_fq, sup.trait_path, aname
                        )))
                    }
                }
            }
        }
        Ok(())
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
            return self.resolve_trait_name(p.last_segment()).ok();
        }
        self.reg
            .root
            .module(p.parent())
            .and_then(|m| m.traits.get(p.last_segment()))
            .map(|td| td.name.clone())
    }

    /// Every implementation of `trait_path`'s `method` currently registered,
    /// as `(owning type, method name)` — what a compiled `:dyn` call site
    /// needs compiled before it can run (see `dyn-call`'s `IMPL_TARGETS`).
    ///
    /// Generic owners are skipped: their method has no code until it is
    /// specialized, and a specialization is only named once a concrete
    /// instantiation is boxed — which is exactly where `dyn-new`'s own
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

    /// Convert a `:dyn from<from_pins>` value to `:dyn to<to_pins>`, which is
    /// admissible exactly when `to` is a supertrait of `from` whose pins agree.
    ///
    /// How much work that takes depends on the layouts. For every trait on
    /// `from`'s *leftmost* supertrait spine, `to`'s `vtable_order` is a
    /// prefix of `from`'s (`Checker::linearize_vtable` emits the direct
    /// supertraits' already-linearized methods in written order before its
    /// own), so the source box already *is* a valid `:dyn to` — every slot
    /// the target can name holds the same entry at the same index — and the
    /// conversion is a pure retype with no node inserted.
    ///
    /// For a second or later supertrait the prefix fails — `D(B,C)`'s vtable
    /// is `[B's..., C's..., D's...]`, so `C`'s slots start at a nonzero
    /// offset and a `:dyn C` call site's baked-in constant would index the
    /// wrong entry. That case gets an [`dyn-upcast`], which swaps the
    /// box's table for the one `dyn-new`'s `SUPERS` recorded for the same
    /// concrete type.
    fn upcast_dyn(
        &self,
        heap: &mut Heap,
        value: Checked,
        from: &Path,
        from_pins: &[Type],
        to: &Path,
        to_pins: &[Type],
    ) -> Result<Checked, Error> {
        let from_ty = Type::Dyn(from.clone(), from_pins.to_vec());
        let to_ty = Type::Dyn(to.clone(), to_pins.to_vec());
        let mismatch = |why: &str| {
            Error::TypeError(format!(
                "cannot convert `{}` to `{}` — {}; box the concrete value as `:dyn {}` instead",
                mangle_type(&from_ty),
                mangle_type(&to_ty),
                why,
                to
            ))
        };
        if !self.trait_inherits(from, to) {
            return Err(mismatch(&format!("`{}` does not inherit `{}`", from, to)));
        }
        let fdef = self.check_object_safe(from)?;
        let own = self.dyn_assoc_subst(from, from_pins)?;
        // The target's pins must be what `from`'s pins imply for it — an
        // upcast may not silently reinterpret an associated type.
        let Some(implied) = self.trait_assoc_subst_for(fdef, &own, to) else {
            return Err(mismatch(&format!("`{}` is not in `{}`'s supertrait chain", to, from)));
        };
        let tdef = self.check_object_safe(to)?;
        for (name, want) in tdef.assoc_types.iter().zip(to_pins.iter()) {
            match implied.get(name) {
                Some(actual) if actual == want => {}
                Some(actual) => {
                    return Err(mismatch(&format!(
                        "`{}` pins `{}`'s `{}` to `{}`, not `{}`",
                        from,
                        to,
                        name,
                        mangle_type(actual),
                        mangle_type(want)
                    )))
                }
                None => {
                    return Err(mismatch(&format!("`{}`'s `{}` is not pinned by `{}`", to, name, from)))
                }
            }
        }
        // Prefix layouts: the box is already a valid `:dyn to`, so nothing
        // below the checker learns that an upcast happened.
        if fdef.vtable_order.starts_with(&tdef.vtable_order) {
            return Ok(Checked::new(value.form, to_ty));
        }
        // Otherwise the target's slot numbering differs and the box has to
        // be re-made around `to`'s own table for this concrete type. Which
        // table that is depends on the value, not on this site, so the node
        // only names the target trait and the swap is a runtime lookup on
        // the box's vtable id.
        let form = forms::dyn_upcast_form(heap, to, value.form)?;
        Ok(Checked::new(form, to_ty))
    }

    /// Every transitive supertrait of `trait_path`, nearest first, without
    /// duplicates — the traits a `:dyn trait_path` value can be upcast to,
    /// and hence the tables [`dyn-new`] has to lay out alongside its
    /// own (see that field's doc comment for why they are laid out at the
    /// boxing site).
    fn supertrait_closure(&self, trait_path: &Path) -> Vec<Path> {
        let mut out: Vec<Path> = Vec::new();
        let mut stack = vec![trait_path.clone()];
        while let Some(p) = stack.pop() {
            let Some(def) = self.reg.trait_def(&p) else { continue };
            for sup in &def.supertraits {
                if !out.contains(&sup.trait_path) {
                    out.push(sup.trait_path.clone());
                    stack.push(sup.trait_path.clone());
                }
            }
        }
        out
    }

    /// The supertrait vtables an [`dyn-new`] carries: for each trait in
    /// `trait_path`'s supertrait closure, the same concrete type's slot
    /// table for *it*.
    ///
    /// Derived from `slots` by name rather than by re-running
    /// `dyn_vtable_slots` per supertrait: a supertrait's `vtable_order` is a
    /// sub-sequence of `trait_path`'s (linearization keeps one slot per
    /// name), so the entry a supertrait's slot must hold is literally the
    /// one `slots` already holds for that name. Deriving it cannot disagree
    /// with the table the box itself dispatches through, and cannot fail on
    /// a trait whose admission checks the boxed trait already passed.
    fn dyn_super_vtables(
        &self,
        trait_path: &Path,
        order: &[String],
        slots: &[(Path, String)],
    ) -> Vec<(Path, Vec<(Path, String)>)> {
        let mut out = Vec::new();
        for sup in self.supertrait_closure(trait_path) {
            let Some(sdef) = self.reg.trait_def(&sup) else { continue };
            let mut sup_slots = Vec::with_capacity(sdef.vtable_order.len());
            for m in &sdef.vtable_order {
                match order.iter().position(|x| x == m).and_then(|i| slots.get(i)) {
                    Some(entry) => sup_slots.push(entry.clone()),
                    // Unreachable while linearization is transitive: every
                    // method of a supertrait is in the subtrait's own order.
                    // Dropping the table rather than emitting a short one
                    // keeps a wrong slot number from ever being callable —
                    // the upcast then fails at the point of conversion.
                    None => {
                        sup_slots.clear();
                        break;
                    }
                }
            }
            if !sup_slots.is_empty() {
                out.push((sup, sup_slots));
            }
        }
        out
    }

    /// Wrap an already-checked concrete value as `:dyn trait_path<pins...>`,
    /// laying out the vtable it will dispatch through. The shared
    /// implementation of the implicit widening (`check_inner`'s expectation
    /// reconciliation) and the explicit `(as :dyn Trait e)`.
    ///
    /// `env` is consulted only for the erased-generic case: inside a generic
    /// function's definition-time body check the value's type may still be a
    /// `where`-bounded type variable, which has no vtable to lay out *yet*.
    fn coerce_to_dyn(
        &self,
        heap: &mut Heap,
        env: &Env,
        value: Checked,
        trait_path: &Path,
        pins: &[Type],
    ) -> Result<Checked, Error> {
        // Re-boxing a trait object. Same-trait/same-pins never reaches here
        // (the types compare equal). A *supertrait* is an upcast, and along
        // the leftmost supertrait spine it is free: inherited methods occupy
        // the low slots of the subtrait's vtable in the supertrait's own
        // order (`TraitDef::vtable_order`), so the box already *is* a valid
        // `:dyn Super` — every slot the target can name holds the same entry
        // at the same index. Retype it and keep the expression untouched;
        // nothing below the checker learns that an upcast happened.
        //
        // Any other target needs a second vtable the source box cannot
        // supply, because the concrete type is gone by then.
        if let Type::Dyn(from, from_pins) = value.ty.clone() {
            return self.upcast_dyn(heap, value, &from, &from_pins, trait_path, pins);
        }
        // Boxing a `where`-bounded type variable (`(defun f<E> ... (where (Error E))
        // ... (as :dyn Error e))`): which vtable to build is only knowable once
        // `E` is concrete, so the generic body — diagnostics-only, never
        // executed — carries the same erased-generic placeholder a bounded
        // method call leaves behind (an unreachable `panic`), and each
        // specialization re-checks this site with `E` substituted, taking the
        // real `dyn_vtable_slots` path below. The bound is what makes it
        // admissible: every instantiation must implement the trait, so the
        // conversion is guaranteed to be layable-out later (the pins, the
        // heap-representation rule, and the associated-type bindings are all
        // verified there, on the concrete type).
        if let Type::Named(p, targs) = &value.ty {
            let is_tvar = targs.is_empty() && p.is_simple() && self.reg.type_def(p).is_none();
            if is_tvar && env.bounds.get(p.last_segment()).is_some_and(|bs| bs.iter().any(|b| b.trait_path == *trait_path)) {
                let what = format!("as :dyn {}", trait_path);
                let form = forms::erased_generic_form(heap, &what)?;
                return Ok(Checked::new(form, Type::Dyn(trait_path.clone(), pins.to_vec())));
            }
        }
        let slots = self.dyn_vtable_slots(&value.ty, trait_path, pins)?;
        // The supertraits' tables, laid out here because this is the last
        // point where both halves of a vtable's identity — the concrete type
        // and the trait — are in hand. A later `dyn-upcast` of this box
        // only has the trait.
        let order = &self.check_object_safe(trait_path)?.vtable_order;
        let supers = self.dyn_super_vtables(trait_path, order, &slots);
        let concrete_key = mangle_type(&value.ty);
        let form = self.dyn_new_form(
            heap,
            &concrete_key,
            trait_path,
            &slots,
            &supers,
            &value.ty,
            value.form,
        )?;
        Ok(Checked::new(form, Type::Dyn(trait_path.clone(), pins.to_vec())))
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
        receiver: Checked,
        trait_path: &Path,
        pins: &[Type],
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let own_subst = self.dyn_assoc_subst(trait_path, pins)?;
        let tdef = self.check_object_safe(trait_path)?;
        let Some(slot) = tdef.vtable_order.iter().position(|n| n == method) else {
            return Err(Error::TypeError(format!(
                "`{}` is not a method of `{}` — a `:dyn {}` value can only call the trait's own or inherited methods",
                method, trait_path, trait_path
            )));
        };
        let decl = &tdef.vtable_owner[slot];
        let Some((_, sig)) = self.reg.trait_method(tdef, method) else {
            return Err(Error::TypeError(format!(
                "{}::{}: inherited from `{}`, which has no registered signature",
                trait_path, method, decl
            )));
        };
        // An inherited method's template is written in the *declaring*
        // trait's associated types, so the pins have to be composed down to
        // it before they can resolve anything.
        let Some(assoc_subst) = self.trait_assoc_subst_for(tdef, &own_subst, decl) else {
            return Err(Error::TypeError(format!(
                "{}::{}: `{}` is not in `{}`'s supertrait chain",
                trait_path, method, decl, trait_path
            )));
        };
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
        let targets = self.dyn_impl_targets(trait_path, method);
        let form = self.dyn_call_form(heap, trait_path, method, slot, &targets, &typed)?;
        Ok(Checked::new(form, ret))
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
        if tdef.vtable_order.is_empty() {
            return Err(Error::TypeError(format!(
                "dyn {}: the trait declares no methods, so a trait object of it could not be called",
                trait_path
            )));
        }
        // Over the *linearized* order: an inherited method occupies a vtable
        // slot just like an own one, so it has to clear the same bar. `where`
        // names the declaring trait when it isn't `trait_path` itself, since
        // that's where the offending signature has to be fixed.
        for (m, decl) in tdef.vtable_order.iter().zip(tdef.vtable_owner.iter()) {
            let Some((_, sig)) = self.reg.trait_method(tdef, m) else {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}`, inherited from `{}`, has no registered signature",
                    trait_path, m, decl
                )));
            };
            let whose =
                if decl == trait_path { String::new() } else { format!(" (inherited from `{}`)", decl) };
            match sig.params.first() {
                Some(t) if is_self_tvar(t) => {}
                _ => {
                    return Err(Error::TypeError(format!(
                        "dyn {}: method `{}`{} has no `self` receiver — a static associated function has no value to dispatch on",
                        trait_path, m, whose
                    )))
                }
            }
            if sig.params[1..].iter().any(mentions_self) || mentions_self(&sig.ret) {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}`{} mentions `Self` outside the receiver position, so its signature differs per implementation and cannot share one vtable slot",
                    trait_path, m, whose
                )));
            }
            if !sig.type_params.is_empty() {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}`{} is generic — a vtable slot holds one compiled entry point, so there is nothing to specialize at the call site",
                    trait_path, m, whose
                )));
            }
            if sig.rest.is_some() {
                return Err(Error::TypeError(format!(
                    "dyn {}: method `{}`{} is variadic, which has no single vtable entry point",
                    trait_path, m, whose
                )));
            }
        }
        Ok(tdef)
    }

    /// Check that a `:dyn Trait<pins...>` type pins exactly the trait's
    /// associated types, and return the name->type map that resolves them in
    /// a method signature. Positional, in `TraitDef::assoc_types` declaration
    /// order — `:dyn Iter<i32>` is `Iter` with `Item = i32`.
    fn dyn_assoc_subst(&self, trait_path: &Path, pins: &[Type]) -> Result<BTreeMap<String, Type>, Error> {
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

    /// Compose an associated-type substitution down the supertrait chain.
    ///
    /// `own` resolves `tdef`'s *own* associated types (from a `:dyn`'s pins,
    /// or a `where` bound's). An inherited method's signature template is
    /// written in terms of its *declaring* trait's associated types, so
    /// checking a call to one needs that trait's substitution instead. A
    /// supertrait entry pins every one of its associated types
    /// (`Checker::parse_supertrait_list` enforces it), and those pins may
    /// mention `tdef`'s own associated types — so descending one level means
    /// applying `own` to the pins, and the result is the next level's `own`.
    ///
    /// Returns `None` when `target` is not in `tdef`'s supertrait closure.
    /// Every consult point that resolves a possibly-inherited method goes
    /// through here; writing the composition out four times would be four
    /// subtly different bugs.
    fn trait_assoc_subst_for(
        &self,
        tdef: &TraitDef,
        own: &BTreeMap<String, Type>,
        target: &Path,
    ) -> Option<BTreeMap<String, Type>> {
        if tdef.name == *target {
            return Some(own.clone());
        }
        for sup in &tdef.supertraits {
            let sdef = self.reg.trait_def(&sup.trait_path)?;
            let next: BTreeMap<String, Type> =
                sup.assoc.iter().map(|(k, v)| (k.clone(), subst_apply(v, own))).collect();
            if let Some(found) = self.trait_assoc_subst_for(sdef, &next, target) {
                return Some(found);
            }
        }
        None
    }

    /// Whether `sub` is `sup`, or inherits from it transitively — the
    /// question `where`-bound discharge and `:dyn` upcasting both ask.
    fn trait_inherits(&self, sub: &Path, sup: &Path) -> bool {
        if sub == sup {
            return true;
        }
        let Some(sdef) = self.reg.trait_def(sub) else { return false };
        sdef.supertraits.iter().any(|b| self.trait_inherits(&b.trait_path, sup))
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
        if !def.impls.contains(trait_path)
            && !self.type_implements(
                concrete,
                &TraitBound { trait_path: trait_path.clone(), assoc: BTreeMap::new() },
                0,
            )
        {
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
        let subst: BTreeMap<String, Type> = def.params.iter().cloned().zip(targs.iter().cloned()).collect();
        // Over the linearized order, so an inherited method gets its slot
        // too. The supertrait obligation (`Checker::check_supertrait_impls`)
        // guarantees `def` also has an `impl` of every supertrait, hence has
        // those methods in `assoc` — the check below is what would catch it
        // if that ever stopped holding.
        let mut slots = Vec::with_capacity(tdef.vtable_order.len());
        for (m, decl) in tdef.vtable_order.iter().zip(tdef.vtable_owner.iter()) {
            if !def.assoc.contains_key(m) {
                // A blanket impl may be supplying it: `type_implements`
                // above queued the materialization, whose `defmethod` is
                // emitted ahead of this form, so the slot can name the
                // method even though it is not registered *yet* — a slot is
                // only a name, resolved when the vtable is published.
                // `m` is declared by `decl`, so only `decl`'s blanket impl
                // can be the one supplying it.
                if self.reg.blanket_impl(decl).is_none() {
                    return Err(Error::TypeError(format!(
                        "`{}`'s `impl {}` is missing method `{}`",
                        type_fq, decl, m
                    )));
                }
                slots.push((type_fq.clone(), m.clone()));
                continue;
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
                // `new_car` may be a freshly rebuilt subtree that nothing else
                // references, and rebuilding the cdr allocates — so it has to
                // be rooted across that call, not just across the final cons.
                let mut s = RootScope::new(heap);
                let new_car = Self::subst_value(&mut s, car, subst)?;
                s.push_root(new_car);
                let new_cdr = Self::subst_value(&mut s, cdr, subst)?;
                s.push_root(new_cdr);
                // core-build-ok: rewrites a read syntax tree in place, not a
                // core node. Both halves are rooted in `s` just above.
                s.cons(new_car, new_cdr)
            }
            other => Ok(other),
        }
    }

    // ---- module / defmethod / use -----------------------------------------

    fn check_module(&mut self, heap: &mut Heap, interp: &dyn MacroExpander, parts: &[Value]) -> Result<TopLevelForm, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("module: (module path body...)".into()));
        }
        let segs = path_to_segs(heap, parts[0])?;
        let path = self.enter_module(&segs);

        let mut body = Vec::new();
        let mut result = Ok(());
        for form in &parts[1..] {
            match self.check_form(heap, interp, *form) {
                // Rooted as it lands, for the same reason `check_impl`'s member
                // loop does it: `check_form` hands back an *unrooted* form by
                // contract, and every later member's checking allocates.
                Ok(tl) => body.push(forms::rooted(heap, tl)),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.exit_module(segs.len());
        result?;
        forms::module_form(heap, &path, &body)
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
    ) -> Result<TopLevelForm, Error> {
        self.check_defmethod_in(heap, interp, parts, parts_locs, public, def_loc, None)
    }

    /// [`Self::check_defmethod`], with the *body*'s namespace optionally
    /// overridden — how `check_impl` replays a trait's default method body.
    ///
    /// Only the body moves: the header (receiver, parameters, return type)
    /// has already had `Self` and the associated types substituted for the
    /// `impl`'s concrete ones, in the `impl`'s own namespace, and re-parsing
    /// it under the trait's would fail to resolve the target type. The body,
    /// by contrast, was written inside the `deftrait` and must resolve the
    /// helpers visible *there* — including that module's private ones.
    #[allow(clippy::too_many_arguments)]
    fn check_defmethod_in(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
        body_ns: Option<Vec<String>>,
    ) -> Result<TopLevelForm, Error> {
        let MethodSig {
            method,
            instance,
            self_name,
            self_name_loc,
            recv_ty,
            type_fq,
            params,
            param_locs,
            optionals: optionals_raw,
            rest,
            keys: keys_raw,
            ret,
            bounds,
            body_start,
            doc,
        } = self.parse_defmethod_sig(heap, parts, parts_locs)?;
        // How many of `params` are required — the rest are the `&optional`
        // /`&rest`/`&key` region, which the registered signature carries
        // separately (see the `FnSig` built below) rather than as parameters.
        let required_n = params.len() - (optionals_raw.len() + usize::from(rest.is_some()) + keys_raw.len());

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
                        Some(n.last_segment().to_string())
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
        //
        // A generic method's registered signature is rewritten into the
        // *owner type's* parameter names first. `Checker::check_assoc_call`
        // specializes a call by zipping `def.params` against the receiver's
        // concrete arguments, so a signature written in any other names is
        // simply not substituted — `(defmethod keepif ((self Vector<A>) (pred
        // (fn (A) bool))) ...)` kept its `a` and rejected an `(fn (i32)
        // bool)` argument, while the identical method spelled `Vector<T>`
        // (the name `Vector` itself was declared with) worked. Renaming here
        // rather than teaching `check_assoc_call` a second vocabulary keeps
        // one name in play past this point; the *body* is deliberately left
        // in the written names below, since that is what its text says, and
        // `MethodTemplate::Form` keeps `written_vars` for the same reason.
        let owner_rename: BTreeMap<String, Type> = if is_generic_template {
            self.reg
                .type_def(&type_fq)
                .map(|d| {
                    written_vars
                        .iter()
                        .cloned()
                        .zip(d.params.iter().map(|p| Type::Named(Path::root(p), Vec::new())))
                        .filter(|(w, t)| !matches!(t, Type::Named(p, _) if p.last_segment() == w))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        let rename = |t: &Type| {
            if owner_rename.is_empty() { t.clone() } else { subst_apply(t, &owner_rename) }
        };
        // Only the *required* params are signature params — an
        // `&optional`/`&key` one lives in `FnSig::optionals`/`keys`, where
        // `Checker::check_assoc_call` finds its default, and a `&rest` one in
        // `FnSig::rest`. This mirrors `Checker::check_defun_opt_key` exactly.
        let mut sig_params: Vec<Type> = Vec::new();
        if instance {
            sig_params.push(rename(&recv_ty));
        }
        sig_params.extend(params[..required_n].iter().map(|(_, t)| rename(t)));

        // A defaulted `&optional`/`&key` parameter's declared type may not
        // mention the owner's type parameters, for the same reason a
        // `defun`'s may not mention its own (see
        // `Checker::check_defun_opt_key`): a call site that omits the
        // argument splices the checked default node in verbatim, and that
        // node's type would still name the abstract variable rather than the
        // instantiation's concrete one. A defaultless parameter is
        // unrestricted — its omitted value is an `Option::none` built fresh
        // at the call's own resolved type.
        let owner_param_set: HashSet<String> = self
            .reg
            .type_def(&type_fq)
            .map(|d| d.params.iter().cloned().collect())
            .unwrap_or_default();
        let mut sig_optionals = Vec::with_capacity(optionals_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &optionals_raw {
            let decl_ty = rename(decl_ty);
            // Rejected *before* the default is checked, so this restriction —
            // not whatever the default's own text happens to fail on when
            // checked in an empty `Env` — is the error the user sees.
            if default_raw.is_some() && type_has_param(&decl_ty, &owner_param_set) {
                return Err(Error::TypeError(format!(
                    "defmethod {}::{}: &optional parameter `{}` may not default when its type \
                     mentions the owner's type parameter — declare it with no default instead",
                    type_fq, method, pname
                )));
            }
            let default = self.check_opt_key_default(heap, interp, pname, &decl_ty, *default_raw)?;
            sig_optionals.push(OptKeyParam { name: pname.clone(), decl_ty, default });
        }
        let mut sig_keys = Vec::with_capacity(keys_raw.len());
        for (pname, decl_ty, _loc, default_raw) in &keys_raw {
            let decl_ty = rename(decl_ty);
            if default_raw.is_some() && type_has_param(&decl_ty, &owner_param_set) {
                return Err(Error::TypeError(format!(
                    "defmethod {}::{}: &key parameter `{}` may not default when its type \
                     mentions the owner's type parameter — declare it with no default instead",
                    type_fq, method, pname
                )));
            }
            let default = self.check_opt_key_default(heap, interp, pname, &decl_ty, *default_raw)?;
            sig_keys.push(OptKeyParam { name: pname.clone(), decl_ty, default });
        }
        // The bounds are keyed by type-variable *name*, so they move too —
        // `validate_where_bounds` looks its keys up in the same `subst`
        // `check_assoc_call` builds from `def.params`.
        let sig_bounds: BTreeMap<String, Vec<TraitBound>> = bounds
            .iter()
            .map(|(k, bs)| {
                let k = match owner_rename.get(k) {
                    Some(Type::Named(p, _)) => p.last_segment().to_string(),
                    _ => k.clone(),
                };
                let bs = bs
                    .iter()
                    .map(|b| TraitBound {
                        trait_path: b.trait_path.clone(),
                        assoc: b.assoc.iter().map(|(n, t)| (n.clone(), rename(t))).collect(),
                    })
                    .collect();
                (k, bs)
            })
            .collect();
        let sig = FnSig {
            type_params: vec![],
            params: sig_params,
            ret: rename(&ret),
            public,
            rest: rest.as_ref().map(|(_, t, _)| rename(t)),
            builtin: false,
            bounds: sig_bounds,
            optionals: sig_optionals,
            keys: sig_keys,
        };
        self.check_redef("method", &method, self.reg.type_def(&type_fq).and_then(|d| d.assoc.get(&method)))?;
        if let Some(def) = self.reg.type_def_mut(&type_fq) {
            def.assoc.insert(method.clone(), AssocFn { sig, instance, builtin: false });
        }
        if let Some(loc) = def_loc {
            self.reg.def_locs.methods.insert((type_fq.clone(), method.clone()), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.methods.insert((type_fq.clone(), method.clone()), doc);
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
        // diagnostics-only unreachable `panic`, exactly as in generic `defun`
        // bodies.
        let env = Env::new().with_bounds(bounds).extended_with_locs(binds);
        let body_locs = parts_locs.get(body_start..).unwrap_or(&[]);
        // `enter_specialization` with the *current* type-variable bindings
        // carried over: only the namespace is meant to move, and a default
        // replayed inside a generic owner's specialization still needs its
        // bindings in effect.
        let saved = body_ns.map(|ns| {
            let bindings = self.type_var_bindings.clone();
            self.enter_specialization(ns, bindings)
        });
        let checked = self.check_seq(heap, interp, &env, &parts[body_start..], body_locs, Some(&ret));
        if let Some((sn, sl, sb)) = saved {
            self.exit_specialization(sn, sl, sb);
        }
        let (body, _) = checked?;
        let type_params = if is_generic_template { written_vars } else { Vec::new() };
        let mut all: Vec<(String, Type)> = Vec::new();
        if let Some(s) = &self_name {
            all.push((s.clone(), recv_ty));
        }
        all.extend(params);
        self.method_definition_form(heap, &type_fq, &method, instance, &type_params, &all, &ret, public, &body)
    }

    /// Parses a `defmethod` form's name, receiver, parameters, and return
    /// type (`parts` = everything after the `defmethod` keyword) — shared by
    /// [`Self::check_defmethod`] and [`Self::specialize_method`], the latter
    /// re-parsing a retained [`MethodTemplate::Form`] with
    /// `type_var_bindings` in effect so the receiver/parameter/return
    /// annotations come back concrete.
    fn parse_defmethod_sig(&self, heap: &mut Heap, parts: &[Value], parts_locs: &[Option<Loc>]) -> Result<MethodSig, Error> {
        self.parse_defmethod_sig_inner(heap, parts, parts_locs, false)
    }

    /// [`Self::parse_defmethod_sig`], with the "receiver must name a
    /// registered type" rejection optionally lifted.
    ///
    /// `abstract_receiver` is set only by [`Self::precheck_blanket_impl`],
    /// whose receiver *is* the blanket's target type variable and so names no
    /// type by construction — `type_fq` then holds that variable's name, the
    /// same unresolved single-segment [`Path`] a generic `defun`'s type
    /// parameter parses to.
    fn parse_defmethod_sig_inner(
        &self,
        heap: &mut Heap,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        abstract_receiver: bool,
    ) -> Result<MethodSig, Error> {
        if parts.len() < 3 {
            return Err(Error::TypeError(
                "defmethod: (defmethod name (receiver params...) ret body...)".into(),
            ));
        }
        let method = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("defmethod: name must be a symbol".into())),
        };
        let sig_list_locs = heap.list_to_vec_locs(parts[1])?;
        let sig_list: Vec<Value> = sig_list_locs.iter().map(|(v, _)| *v).collect();
        if sig_list.is_empty() {
            return Err(Error::TypeError("defmethod: needs a receiver".into()));
        }
        // `(self T)` -> instance method; a bare type name -> static method.
        let (instance, self_name, self_name_loc, type_expr, type_expr_loc) = match sig_list[0] {
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
                let ty_loc = recv_locs.get(1).and_then(|(_, l)| l.clone());
                (true, Some(sname), sname_loc, recv[1], ty_loc)
            }
            Value::Symbol(_) | Value::Path(_) => {
                (false, None, None, sig_list[0], sig_list_locs.first().and_then(|(_, l)| l.clone()))
            }
            _ => return Err(Error::TypeError("defmethod: receiver must be (self Type) or a type name".into())),
        };
        let recv_ty = self.parse_type_here_at(heap, type_expr, type_expr_loc.as_ref())?;
        let type_fq = match &recv_ty {
            Type::Named(n, _) => n.clone(),
            other => match prim_type_path(other) {
                Some(p) => p,
                None => return Err(Error::TypeError("defmethod: receiver must be a data type".into())),
            },
        };
        if !abstract_receiver && self.reg.type_def(&type_fq).is_none() {
            return Err(Error::TypeError(format!("defmethod: unknown type `{}`", type_fq)));
        }
        let (required, required_locs, optionals, rest, keys) =
            self.parse_params_full(heap, &sig_list[1..], "defmethod")?;
        // Runtime params, in the order `MethodSig::params` documents. With no
        // `&optional`/`&rest`/`&key` written this is just the required list,
        // exactly what `parse_param_pairs` used to return here.
        let mut params = required;
        let mut param_locs = required_locs;
        for (name, ty, loc, default) in &optionals {
            params.push((name.clone(), opt_key_effective_ty(ty, default.is_some())));
            param_locs.push(loc.clone());
        }
        if let Some((rname, _, rloc)) = &rest {
            params.push((rname.clone(), option_of_sexpr()));
            param_locs.push(rloc.clone());
        }
        for (name, ty, loc, default) in &keys {
            params.push((name.clone(), opt_key_effective_ty(ty, default.is_some())));
            param_locs.push(loc.clone());
        }
        let ret = self.parse_type_here_at(heap, parts[2], parts_locs.get(2).and_then(|l| l.as_ref()))?;
        // Optional `(where ...)` clause after the return type — identical
        // peek to `parse_defun_sig`'s.
        let mut body_start = 3;
        let mut bounds: BTreeMap<String, Vec<TraitBound>> = BTreeMap::new();
        if let Some(form) = parts.get(3) {
            if is_where_clause(heap, *form)? {
                bounds = self.parse_where_clause(heap, *form)?;
                body_start = 4;
            }
        }
        let doc = take_leading_docstring(heap, parts, body_start);
        if doc.is_some() {
            body_start += 1;
        }
        Ok(MethodSig {
            method,
            instance,
            self_name,
            self_name_loc,
            recv_ty,
            type_fq,
            params,
            param_locs,
            optionals,
            rest,
            keys,
            ret,
            bounds,
            body_start,
            doc,
        })
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
    /// choose a mutable struct box over an enum one) — generic substitution there
    /// (and in `match`'s `check_ctor_pattern`) is already `AdtDef.params`-
    /// generic, shared with `Option`/`Result`/`HashTable`, so a generic
    /// `defstruct` needs no changes to either.
    ///
    /// Beyond registering the type, this synthesizes one getter and one
    /// setter `defmethod` per field: `f` reads `fields[i]` (`field-get`)
    /// and `set-f` writes it (`field-set`, `set-car`/`set-cdr`'s prefix
    /// convention, no `!`), so `(f instance)`/`(set-f instance v)` — and,
    /// through `Checker::check`'s `Value::Path` sugar and `Checker::check_setf`,
    /// `instance::f`/`(setf instance::f v)` — work immediately; no separate
    /// accessor-declaration step exists. `check_form`'s signature is
    /// `Result<TopLevel, Error>` (one node per top-level form), so the type
    /// registration and every accessor `TopLevel::Defmethod` are bundled into
    /// one `TopLevel::Module` — purely as a grouping device: `Interp::exec`'s
    /// `Module` arm just runs `body` in order and never reads `path`, so this
    /// carries none of an actual `(module ...)`'s namespace-nesting semantics.
    #[allow(clippy::too_many_arguments)]
    fn check_defstruct(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defstruct: (defstruct name (field type)...)".into()));
        }
        // CL's `(defstruct (name option...) ...)`: a *list* in the name
        // position carries options. A bare symbol is the common case and
        // means no options.
        let (name_form, options) = match parts[0] {
            Value::Cons(_) => {
                let items_locs = heap.list_to_vec_locs(parts[0])?;
                let items: Vec<Value> = items_locs.iter().map(|(v, _)| *v).collect();
                let locs: Vec<Option<Loc>> = items_locs.iter().map(|(_, l)| l.clone()).collect();
                let Some(&head) = items.first() else {
                    return Err(Error::TypeError("defstruct: (defstruct (Name option...) ...) needs a name".into()));
                };
                (head, self.parse_struct_options(heap, &items[1..], &locs[1..])?)
            }
            _ => (parts[0], StructOptions::default()),
        };
        let (name, type_params) = self.parse_defun_name(heap, name_form)?;
        // A leading docstring, right after the name and before the field
        // list — CL's `(defstruct (name options) documentation slot...)`
        // position. Unlike `defun`'s, unambiguous: a field is always a
        // `(name Type ...)` list, never a bare string, so no "followed by
        // more forms" guard is needed.
        let (doc, field_start) = match parts.get(1) {
            Some(Value::Str(id)) => (Some(heap.string(*id).to_string()), 2),
            _ => (None, 1),
        };

        self.check_redef("type", &name, self.cur_ns().types.get(&name))?;
        self.check_type_trait_clash("type", &name)?;
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
            assoc: BTreeMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Struct,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: BTreeMap::new(),
        });

        let mut fields = self.parse_struct_fields(heap, &parts[field_start..])?;
        // `(:include Parent)`: the parent's slots come first, exactly as in
        // CL. What this does *not* do is establish a type relation — a child
        // is not a subtype of its parent, the parent's methods do not apply
        // to it, and no `typep`-style test relates them. There is no
        // subtyping in this language (see the doc comment above); a shared
        // interface is what `deftrait` is for. Inheriting the slot *list* is
        // the part that carries over.
        if let Some((parent_form, parent_loc)) = options.include {
            let inherited = self.included_fields(heap, parent_form, parent_loc.as_ref())?;
            let mut all = inherited;
            all.extend(fields);
            fields = all;
        }
        if fields.is_empty() {
            return Err(Error::TypeError("defstruct: needs at least one field".into()));
        }
        let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        let field_types: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
        for (i, n) in field_names.iter().enumerate() {
            if field_names[..i].contains(n) {
                return Err(Error::TypeError(format!("defstruct: duplicate field `{}`", n)));
            }
        }
        // A slot default is only ever read by a generated constructor. With
        // none declared it would be dead configuration, so say so here rather
        // than let it sit unused.
        if options.constructors.is_empty() {
            if let Some(f) = fields.iter().find(|f| f.default.is_some()) {
                return Err(Error::TypeError(format!(
                    "defstruct {}: slot `{}` has a default, but nothing would use it — a default is read by the constructors this `defstruct` generates, so declare one with `(:constructor name)` (or drop the default; `new` always takes every slot)",
                    name, f.name
                )));
            }
        }

        let recv_targs: Vec<Type> = type_params.iter().map(|p| Type::Named(Path::root(p), Vec::new())).collect();
        let recv_ty = Type::Named(type_fq.clone(), recv_targs);

        let mut assoc = BTreeMap::new();
        // The bundle's members, each rooted from the moment it is built. The
        // loop below allocates once per accessor, so an accessor finished two
        // fields ago is exactly as collectible as the one being built now —
        // holding them in `Items` is what makes that automatic. The
        // `(defstruct ...)` node goes first because the type has to exist
        // before its methods.
        let mut members = Items::new(heap);
        let def_node = self.defstruct_form(members.heap(), &type_fq, &field_types)?;
        members.push(def_node);
        for (i, field) in fields.iter().enumerate() {
            let (field_name, field_ty) = (&field.name, &field.ty);
            let field_public = field.public;
            let getter_sig = FnSig {
                type_params: vec![],
                params: vec![recv_ty.clone()],
                ret: field_ty.clone(),
                public: field_public,
                rest: None,
                builtin: false,
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            };
            assoc.insert(field_name.clone(), AssocFn { sig: getter_sig, instance: true, builtin: false });
            let getter = {
                // The intermediates (`self`, then the `field-get` around it)
                // have to stay rooted until the method node holding them is
                // finished; this scope releases them however it is left.
                let mut s = RootScope::new(members.heap());
                let self_var = forms::var_form(&mut s, "self")?;
                s.push_root(self_var);
                let get = self.field_get_form(&mut s, self_var, i, field_ty)?;
                s.push_root(get);
                self.method_definition_form(
                    &mut s,
                    &type_fq,
                    field_name,
                    true,
                    &type_params,
                    &[("self".to_string(), recv_ty.clone())],
                    field_ty,
                    field_public,
                    &[get],
                )?
            };
            members.push(getter);

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
                bounds: BTreeMap::new(),
                optionals: Vec::new(),
                keys: Vec::new(),
            };
            assoc.insert(setter_name.clone(), AssocFn { sig: setter_sig, instance: true, builtin: false });
            let setter = {
                let mut s = RootScope::new(members.heap());
                let self_var = forms::var_form(&mut s, "self")?;
                s.push_root(self_var);
                let value_var = forms::var_form(&mut s, "value")?;
                s.push_root(value_var);
                let set = self.field_set_form(&mut s, self_var, i, field_ty, value_var)?;
                s.push_root(set);
                self.method_definition_form(
                    &mut s,
                    &type_fq,
                    &setter_name,
                    true,
                    &type_params,
                    &[("self".to_string(), recv_ty.clone()), ("value".to_string(), field_ty.clone())],
                    &Type::Unit,
                    field_public,
                    &[set],
                )?
            };
            members.push(setter);

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
            params: type_params.clone(),
            variants: vec![Variant { name: "new".to_string(), fields: field_types }],
            assoc,
            public,
            builtin: false,
            kind: AdtKind::Struct,
            field_names,
            impls: Vec::new(),
            trait_assoc: BTreeMap::new(),
        };
        self.reg.root.module_mut(&self.ns).add_type(def);
        if let Some(loc) = def_loc.clone() {
            self.reg.def_locs.types.insert(type_fq.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.types.insert(type_fq.clone(), doc);
        }
        self.record_def_name_use(
            &type_fq,
            TypeKind::Struct,
            parts_locs.first().and_then(|l| l.as_ref()),
            name.chars().count(),
        );
        // Record the slot defaults for a later `(:include ...)` — including
        // one in another file, or after a dump reload. Only when there is
        // something to record, so the table stays sparse.
        if fields.iter().any(|f| f.default.is_some()) {
            let mut owned = Vec::with_capacity(fields.len());
            for f in &fields {
                owned.push(match &f.default {
                    Some(SlotDefault::Written(v)) => Some(crate::owned_form::value_to_owned(members.heap(), *v)?),
                    Some(SlotDefault::Inherited(o)) => Some(o.clone()),
                    None => None,
                });
            }
            self.reg.struct_defaults.insert(type_fq.clone(), owned);
        }

        // Generated constructors and the copier, *after* the type is
        // registered: unlike the accessors (whose ASTs are synthesized
        // directly), these are synthesized as **source** and run through
        // `check_defmethod_in`. That is what buys them the whole method
        // pipeline for free — `&key`/`&optional` filling (Phase 5b),
        // visibility, and, for a generic owner, the `MethodTemplate::Form`
        // retention that makes them specializable. It needs the type and its
        // `new` to already exist, hence the position.
        for spec in &options.constructors {
            let form = self.constructor_source(members.heap(), &name, &type_params, &fields, spec)?;
            let checked = self.check_synthetic_defmethod(members.heap(), interp, form, public, def_loc.clone())?;
            members.push(checked);
        }
        if let Some(copier) = &options.copier {
            let form = self.copier_source(members.heap(), &name, &type_params, &fields, copier)?;
            let checked = self.check_synthetic_defmethod(members.heap(), interp, form, public, def_loc.clone())?;
            members.push(checked);
        }

        // `(module Name (defstruct ...) (defmethod ...)...)`: the definition
        // and its accessors travel as one unit, so a caller that records this
        // form records all of it.
        let body: Vec<Value> = members.as_slice().to_vec();
        forms::module_form(members.heap(), &type_fq, &body)
    }

    /// Check a `defmethod` this checker synthesized itself: `parts` is the
    /// form *without* the leading `defmethod` symbol, already rooted by the
    /// caller.
    ///
    /// Split out because the synthesized source has no source positions of
    /// its own — every `parts_locs` entry is `None`, and the definition's own
    /// `def_loc` is the `defstruct`'s, which is where a reader looking for
    /// the generated method should in fact land.
    fn check_synthetic_defmethod(
        &mut self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        form: Value,
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<Value, Error> {
        // `form` is one list, so rooting it keeps every part alive across the
        // allocation that checking it does — a `Vec<Value>` would not.
        heap.push_root(form);
        let result = (|| {
            let parts = heap.list_to_vec(form)?;
            let locs: Vec<Option<Loc>> = vec![None; parts.len()];
            self.check_defmethod_in(heap, interp, &parts, &locs, public, def_loc, None)
        })();
        heap.pop_root();
        result
    }

    /// A [`Type`] rendered back into the surface syntax a type annotation is
    /// written in, so a synthesized definition can carry it.
    ///
    /// [`mangle_type`] alone is not enough: it is a *serialization* (commas
    /// inside `(fn ...)`, a bare `dyn` head), tuned for making two distinct
    /// canonical types render differently, not for being read back. It is
    /// exactly right for the atomic cases, which is why they go through it.
    fn type_source(&self, heap: &mut Heap, t: &Type) -> Result<Value, Error> {
        match t {
            Type::Fn(ps, rest, r) => {
                let mut out = Items::new(heap);
                let head = out.heap().intern_symbol("fn");
                out.push(head);
                let param_list = {
                    let mut params = Items::new(out.heap());
                    for p in ps {
                        let f = self.type_source(params.heap(), p)?;
                        params.push(f);
                    }
                    if let Some(elem) = rest {
                        let marker = params.heap().intern_symbol("&rest");
                        params.push(marker);
                        let f = self.type_source(params.heap(), elem)?;
                        params.push(f);
                    }
                    params.finish_list()?
                };
                out.push(param_list);
                let ret = self.type_source(out.heap(), r)?;
                out.push(ret);
                out.finish_list()
            }
            // The reader joins the two-word `:dyn Trait` spelling into this
            // list, so this is what an annotation site actually sees.
            Type::Dyn(path, pins) => {
                let mut f = Items::new(heap);
                let head = f.heap().intern_symbol(":dyn");
                f.push(head);
                let inner = mangle_type(&Type::Named(path.clone(), pins.clone()));
                let name = f.heap().intern_symbol(&inner);
                f.push(name);
                f.finish_list()
            }
            other => Ok(heap.intern_symbol(&mangle_type(other))),
        }
    }

    /// The receiver/return annotation for a generated static constructor:
    /// the struct's own written name, with its type parameters spelled back
    /// on (`point<T>`), which is what makes `check_defmethod_in` retain a
    /// specializable template for a generic owner.
    fn owner_type_source(&self, heap: &mut Heap, name: &str, type_params: &[String]) -> Value {
        let text = if type_params.is_empty() {
            name.to_string()
        } else {
            format!("{}<{}>", name, type_params.join(","))
        };
        heap.intern_symbol(&text)
    }

    /// One slot's default value form, materialized into `heap`.
    fn slot_default_source(&self, heap: &mut Heap, d: &SlotDefault) -> Result<Value, Error> {
        match d {
            SlotDefault::Written(v) => Ok(*v),
            SlotDefault::Inherited(o) => crate::owned_form::owned_to_value(heap, o),
        }
    }

    /// Synthesize a `(:constructor ...)` option's `defmethod` **source** —
    /// everything after the `defmethod` keyword, as one list.
    ///
    /// Source rather than a lowered AST (which is how the accessors are
    /// built) because a constructor needs the whole method pipeline:
    /// `&key`/`&optional` filling, visibility, and — for a generic owner —
    /// the `MethodTemplate::Form` retention that makes it specializable.
    /// Synthesizing source and handing it to `check_defmethod_in` gets all of
    /// that with no second implementation.
    ///
    /// The body is always `(Name::new slot...)`: `new` stays the one
    /// structural constructor, and everything generated here is a *way of
    /// calling it*.
    fn constructor_source(
        &self,
        heap: &mut Heap,
        name: &str,
        type_params: &[String],
        fields: &[StructField],
        spec: &CtorSpec,
    ) -> Result<Value, Error> {
        // One outer builder holds every finished piece rooted; each
        // sub-list is built in a nested scope that reborrows its heap.
        let mut out = Items::new(heap);
        let mname = out.heap().intern_symbol(&spec.name);
        out.push(mname);

        // `bound` marks the slots a parameter supplies; the rest come from
        // their own defaults.
        let mut bound: Vec<bool> = vec![false; fields.len()];
        let mut sig = Items::new(out.heap());
        let recv = self.owner_type_source(sig.heap(), name, type_params);
        sig.push(recv);
        match &spec.boa {
            None => {
                // Keyword constructor: every slot is a `&key` at its own
                // default. A slot without one cannot be filled — there is no
                // "unbound slot" here the way CL has.
                let marker = sig.heap().intern_symbol("&key");
                sig.push(marker);
                for (i, f) in fields.iter().enumerate() {
                    let Some(d) = &f.default else {
                        return Err(Error::TypeError(format!(
                            "defstruct {}: (:constructor {}) takes every slot by keyword, so slot `{}` needs a default — give it one, or declare a BOA constructor `(:constructor {} ({} ...))` that takes it positionally",
                            name, spec.name, f.name, spec.name, f.name
                        )));
                    };
                    let item = self.param_source(sig.heap(), &f.name, &f.ty, Some(d))?;
                    sig.push(item);
                    bound[i] = true;
                }
            }
            Some(params) => {
                let mut seen_optional = false;
                for p in params {
                    let Some(i) = fields.iter().position(|f| f.name == p.name) else {
                        return Err(Error::TypeError(format!(
                            "defstruct {}: (:constructor {}) names `{}`, which is not a slot",
                            name, spec.name, p.name
                        )));
                    };
                    if bound[i] {
                        return Err(Error::TypeError(format!(
                            "defstruct {}: (:constructor {}) names slot `{}` twice",
                            name, spec.name, p.name
                        )));
                    }
                    let default = if p.optional {
                        if !seen_optional {
                            let marker = sig.heap().intern_symbol("&optional");
                            sig.push(marker);
                            seen_optional = true;
                        }
                        match &fields[i].default {
                            Some(d) => Some(d),
                            None => {
                                return Err(Error::TypeError(format!(
                                    "defstruct {}: (:constructor {}) makes slot `{}` optional, so that slot needs a default to fall back on",
                                    name, spec.name, p.name
                                )))
                            }
                        }
                    } else {
                        None
                    };
                    let item = self.param_source(sig.heap(), &fields[i].name, &fields[i].ty, default)?;
                    sig.push(item);
                    bound[i] = true;
                }
            }
        }
        let sig_list = sig.finish_list()?;
        out.push(sig_list);
        let ret = self.owner_type_source(out.heap(), name, type_params);
        out.push(ret);

        // `(Name::new v...)`: a bound slot contributes its parameter's name,
        // an unbound one its own default.
        let mut call = Items::new(out.heap());
        let ctor_path = Path::from_segments(vec![name.to_string(), "new".to_string()]);
        let head = forms::path_form(call.heap(), &ctor_path);
        call.push(head);
        for (i, field) in fields.iter().enumerate() {
            let arg = if bound[i] {
                call.heap().intern_symbol(&field.name)
            } else {
                match &field.default {
                    Some(d) => self.slot_default_source(call.heap(), d)?,
                    None => {
                        return Err(Error::TypeError(format!(
                            "defstruct {}: (:constructor {}) does not take slot `{}` and that slot has no default, so the constructor could not fill it",
                            name, spec.name, field.name
                        )))
                    }
                }
            };
            call.push(arg);
        }
        let body = call.finish_list()?;
        out.push(body);
        out.finish_list()
    }

    /// One `(name Type)` / `(name Type default)` parameter item.
    fn param_source(&self, heap: &mut Heap, name: &str, ty: &Type, default: Option<&SlotDefault>) -> Result<Value, Error> {
        let mut f = Items::new(heap);
        let n = f.heap().intern_symbol(name);
        f.push(n);
        let t = self.type_source(f.heap(), ty)?;
        f.push(t);
        if let Some(d) = default {
            let v = self.slot_default_source(f.heap(), d)?;
            f.push(v);
        }
        f.finish_list()
    }

    /// Synthesize a `(:copier name)` option's `defmethod` source: an
    /// *instance* method returning a fresh value with the same slot values —
    /// CL's copier is likewise shallow.
    fn copier_source(
        &self,
        heap: &mut Heap,
        name: &str,
        type_params: &[String],
        fields: &[StructField],
        copier: &str,
    ) -> Result<Value, Error> {
        let mut out = Items::new(heap);
        let mname = out.heap().intern_symbol(copier);
        out.push(mname);

        let sig_list = {
            let mut sig = Items::new(out.heap());
            let recv = {
                let mut recv_pair = Items::new(sig.heap());
                let sname = recv_pair.heap().intern_symbol("self");
                recv_pair.push(sname);
                let owner = self.owner_type_source(recv_pair.heap(), name, type_params);
                recv_pair.push(owner);
                recv_pair.finish_list()?
            };
            sig.push(recv);
            sig.finish_list()?
        };
        out.push(sig_list);
        let ret = self.owner_type_source(out.heap(), name, type_params);
        out.push(ret);

        let body = {
            let mut call = Items::new(out.heap());
            let ctor_path = Path::from_segments(vec![name.to_string(), "new".to_string()]);
            let head = forms::path_form(call.heap(), &ctor_path);
            call.push(head);
            for field in fields {
                // `self::slot` — the accessor sugar, so the copier reads
                // exactly what the getter does.
                let acc = Path::from_segments(vec!["self".to_string(), field.name.clone()]);
                let read = forms::path_form(call.heap(), &acc);
                call.push(read);
            }
            call.finish_list()?
        };
        out.push(body);
        out.finish_list()
    }

    /// `(deftype Name Type)` — or, generically, `(deftype Name<T,U> Type)`,
    /// the name position parsed exactly like a `defun`'s
    /// ([`Self::parse_defun_name`]) — CL's `deftype`, narrowed to what a
    /// statically typed language can mean by it: a **spelling** for a type,
    /// not a type of its own.
    ///
    /// The body is stored already canonical and already alias-expanded (see
    /// [`TypeAlias`]), so a use site's expansion in [`Self::canon`] is one
    /// substitution rather than a fixpoint, and an alias cycle is
    /// unconstructible rather than merely detected. Because the rewrite
    /// happens inside the type parser, nothing downstream ever learns an
    /// alias existed: `mangle_type`, the monomorphization keys, the dump, the
    /// compile pipeline and every error message all show the expansion. That
    /// is the deliberate trade — CL's `deftype` is likewise a
    /// *type-specifier* abbreviation, not a distinct type, and `typep` on one
    /// asks about the expansion.
    ///
    /// Two things it is therefore *not*:
    ///
    /// - **not a new type.** `(deftype meters i32)` makes `meters` and `i32`
    ///   the same type; nothing catches passing one where the other is
    ///   meant. A distinct type is what `defstruct` is for.
    /// - **not a predicate.** CL's `(deftype small () '(integer 0 9))`
    ///   describes a *set of values*, checked at runtime by `typep`. Here a
    ///   type is a compile-time classification with no runtime witness, so a
    ///   value-restricting alias has nothing to restrict.
    ///
    /// Registers into [`Namespace::type_aliases`], which shares the
    /// type/trait name space ([`Self::check_type_trait_clash`]). Emits an
    /// empty `module` form: like `deftrait`, there is nothing to run.
    fn check_deftype(
        &mut self,
        heap: &mut Heap,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("deftype: (deftype Name Type)".into()));
        }
        let (name, params) = self.parse_defun_name(heap, parts[0])?;
        // A leading docstring, in the same position `defstruct`/`defenum`
        // take one. Unambiguous for the same reason: a type is never written
        // as a string literal.
        let (doc, body_at) = match parts.get(1) {
            Some(Value::Str(id)) => (Some(heap.string(*id).to_string()), 2),
            _ => (None, 1),
        };
        if parts.len() != body_at + 1 {
            return Err(Error::TypeError("deftype: (deftype Name Type) — exactly one type follows the name".into()));
        }
        for (i, p) in params.iter().enumerate() {
            if params[..i].contains(p) {
                return Err(Error::TypeError(format!("deftype: duplicate type parameter `{}`", p)));
            }
        }

        // A type of this name is not a redefinition of this alias — the two
        // live in different tables, so `RedefPolicy` allowing a redefinition
        // would leave *both* registered, with the alias silently winning at
        // every use site. Refuse outright, the way a trait of that name is
        // refused.
        if self.cur_ns().types.contains_key(&name) {
            return Err(Error::TypeError(format!(
                "cannot define type alias `{}`: a type of that name already exists here",
                name
            )));
        }
        self.check_redef("type alias", &name, self.cur_ns().type_aliases.get(&name))?;
        self.check_type_trait_clash("type alias", &name)?;

        let body = self.parse_type_here_at(heap, parts[body_at], parts_locs.get(body_at).and_then(|l| l.as_ref()))?;
        // The alias is not registered yet, so a self-reference parsed above
        // as an unresolved bare name — indistinguishable from a type
        // variable, and it would expand a use site into a type nothing can
        // inhabit. Say so here instead. (Mutual cycles cannot arise: storing
        // expanded bodies means `B`'s body already contains `A`'s body, not
        // `A`.)
        let self_name: HashSet<String> = std::iter::once(name.clone()).collect();
        if type_has_param(&body, &self_name) {
            return Err(Error::TypeError(format!(
                "deftype {}: an alias may not mention itself — it is a spelling, expanded where it is written, so there is nothing to recurse into",
                name
            )));
        }

        let fq = self.fq(&name);
        self.reg
            .root
            .module_mut(&self.ns)
            .type_aliases
            .insert(name.clone(), TypeAlias { name: fq.clone(), params, body, public });
        if let Some(loc) = def_loc {
            self.reg.def_locs.types.insert(fq.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.types.insert(fq.clone(), doc);
        }
        forms::module_form(heap, &fq, &[])
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
    /// checking, generic instantiation, and the enum-box runtime
    /// representation are all the shared sum-type machinery, unchanged (that is
    /// how `Option`/`Result` already work). Unlike `defstruct`, no field
    /// accessors/setters are synthesized: an enum value is immutable and its
    /// fields are positional, so there's nothing to run at exec time either —
    /// hence a bare `TopLevel::Defenum` rather than a `Module` bundle.
    fn check_defenum(&mut self, heap: &mut Heap, parts: &[Value], parts_locs: &[Option<Loc>], public: bool, def_loc: Option<Loc>) -> Result<TopLevelForm, Error> {
        if parts.is_empty() {
            return Err(Error::TypeError("defenum: (defenum Name (Variant Type...)...)".into()));
        }
        let (name, type_params) = self.parse_defun_name(heap, parts[0])?;
        // A leading docstring, right after the name and before the variant
        // list — same position/rule as `defstruct`'s (no ambiguity: a
        // variant is always a symbol or a `(Name Type...)` list, never a
        // bare string).
        let (doc, variant_start) = match parts.get(1) {
            Some(Value::Str(id)) => (Some(heap.string(*id).to_string()), 2),
            _ => (None, 1),
        };

        self.check_redef("type", &name, self.cur_ns().types.get(&name))?;
        self.check_type_trait_clash("type", &name)?;
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
            assoc: BTreeMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Sum,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: BTreeMap::new(),
        });

        let mut variants = Vec::new();
        for item in &parts[variant_start..] {
            let (vname, field_vals): (String, Vec<(Value, Option<Loc>)>) = match *item {
                // A bare symbol is a nullary variant (e.g. `None`); the
                // parenthesized `(None)` form is equally accepted below.
                Value::Symbol(id) => (heap.symbol_name(id).to_string(), Vec::new()),
                Value::Cons(_) => {
                    let elems = heap.list_to_vec_locs(*item)?;
                    let vname = match elems.first() {
                        Some((Value::Symbol(id), _)) => heap.symbol_name(*id).to_string(),
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
                .map(|(v, l)| self.parse_type_here_at(heap, *v, l.as_ref()))
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
            assoc: BTreeMap::new(),
            public,
            builtin: false,
            kind: AdtKind::Sum,
            field_names: Vec::new(),
            impls: Vec::new(),
            trait_assoc: BTreeMap::new(),
        };
        self.reg.root.module_mut(&self.ns).add_type(def);
        if let Some(loc) = def_loc {
            self.reg.def_locs.types.insert(type_fq.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.types.insert(type_fq.clone(), doc);
        }
        self.record_def_name_use(
            &type_fq,
            TypeKind::Enum,
            parts_locs.first().and_then(|l| l.as_ref()),
            name.chars().count(),
        );
        self.defenum_form(heap, &type_fq, &variants)
    }

    /// `(load "path")` — records the flat-load request for the driver (see
    /// [`TopLevel::Load`]). The argument must be a string *literal* (the
    /// driver resolves it before any subsequent form is checked, so it can't
    /// depend on runtime values).
    ///
    /// Keeps its receiver for the same reason as [`Self::check_quote`]: one arm
    /// of a dispatch table whose siblings all take one.
    fn check_load(&mut self, heap: &mut Heap, parts: &[Value]) -> Result<TopLevelForm, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("load: (load \"path\")".into()));
        }
        match parts[0] {
            Value::Str(id) => {
                let path = heap.string(id).to_string();
                forms::load_form(heap, &path)
            }
            _ => Err(Error::TypeError("load: expected a string-literal path".into())),
        }
    }

    fn check_use(&mut self, heap: &mut Heap, parts: &[Value], parts_locs: &[Option<Loc>]) -> Result<TopLevelForm, Error> {
        if parts.len() != 1 {
            return Err(Error::TypeError("use: (use path)".into()));
        }
        let segs = path_to_segs(heap, parts[0])?;
        let bare = segs.last().cloned().unwrap();
        let path_loc = parts_locs.first().and_then(|l| l.as_ref());

        // Try: free function.
        if let Some(target) = self.resolve_fn_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            let alias = self.fq(&bare);
            return forms::use_form(heap, &alias, &target);
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
            // Unlike `rect::new`, a `use` path's *last* segment is the type
            // itself — the import names it and nothing else.
            self.record_path_seg_use(&segs, segs.len() - 1, &target, path_loc);
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
            let alias = self.fq(&bare);
            return forms::use_form(heap, &alias, &target);
        }
        // Try: `deftype` alias. Only the name is imported — an alias has no
        // constructors or static methods of its own (whatever its *body*
        // names keeps its own name). The `aliases` entry is what
        // `resolve_type_alias`'s bare-name branch follows.
        if let Some(a) = self.resolve_type_alias_path(&segs) {
            self.reg.root.module_mut(&self.ns).aliases.insert(bare.clone(), segs);
            let alias = self.fq(&bare);
            return forms::use_form(heap, &alias, &a.name);
        }
        // Try: module alias — `(use std::math)` makes `math` a short name for `std::math`.
        if let Some((abs, _)) = self.find_module(&segs) {
            let target_path = Path::from_segments(abs.clone());
            self.reg.root.module_mut(&self.ns).mod_aliases.insert(bare.clone(), abs);
            let alias = self.fq(&bare);
            return forms::use_form(heap, &alias, &target_path);
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
    ) -> Result<Checked, Error> {
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
    ) -> Result<Checked, Error> {
        let loc = heap.cons_loc(v).or(loc_hint);
        match self.check_inner(heap, interp, env, v, expected) {
            // Record `v`'s location on the lowered node so the interpreter can
            // report a *runtime* error there too. A recursive `check` on a
            // sub-form already tagged its own node, so only fill an empty slot.
            //
            // The position goes in `Heap`'s `code_locs` table, keyed by the
            // node's own cons cell, rather than in a `loc` field of the node —
            // there is no field any more, and a core form is data whose shape
            // is fixed by the vocabulary. `core::tagged_at` is the same act
            // performed at build time by a site that has the position in hand.
            Ok(checked) => {
                // Root the node for the rest of this top-level form, with no
                // matching pop of its own (`check_form_at` truncates the stack
                // back to where it started).
                //
                // This is what makes the checker GC-safe by construction
                // rather than by discipline. A node lives in a Rust local
                // between being built here and being pushed into its parent's
                // `core::Items`, and `Heap::cons` can collect at any point in
                // between — that window is precisely where the four historical
                // root leaks in this file were (`subst_value`,
                // `list_from_vec_locs`, `subst_method_sig`,
                // `merge_where_clauses`). An enclosing `Items`/`RootScope`
                // truncating past this push is harmless: by then the node is
                // rooted by that scope, or reachable from the finished parent.
                heap.push_root(checked.form);
                if let Value::Cons(cr) = checked.form {
                    if heap.cons_loc(checked.form).is_none() {
                        if let Some(loc) = loc.clone() {
                            heap.set_cons_loc(cr, loc);
                        }
                    }
                }
                // A local-variable reference that resolved against an `Env`
                // binding with a recorded position: record the reference's own
                // position -> the binding's position in `local_refs`, for
                // `check::locate::definition_target`'s variable arm. Both
                // positions are only available together right here — the
                // reference's (just recorded above) and the binding's
                // (`env.get_loc`) — so this is resolved once now rather than
                // searched again at query time.
                // The type the checker just proved, keyed by the position it
                // was proved at — the only place both are in hand, since the
                // type does not travel with the lowered form. See
                // `DefLocs::node_types`.
                if let Some(l) = &loc {
                    self.node_types
                        .borrow_mut()
                        .entry((l.line, l.col))
                        .or_insert_with(|| checked.ty.clone());
                }
                if let (Some("var"), Some(ref_loc)) = (core::op(heap, checked.form), loc) {
                    if let Some(Value::Symbol(id)) = core::field(heap, checked.form, 0) {
                        let name = heap.symbol_name(id).to_string();
                        if let Some(bind_loc) = env.get_loc(&name) {
                            self.local_refs.borrow_mut().insert((ref_loc.line, ref_loc.col), bind_loc);
                        }
                    }
                }
                Ok(checked)
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
    ) -> Result<Checked, Error> {
        let typed = match v {
            Value::Int(n) => Checked::new(core::tagged(heap, "int", &[Value::Int(n)])?, int_lit_ty(expected)),
            // A `Value::Boxed` read-literal is `Sexpr::Float`, `bignum` (an
            // integer literal past `i64`'s range), or `ratio` (`n/d` syntax)
            // — all heap-boxed for the same reason (`BoxedObj`'s doc comment).
            // The literal node keeps the *same box* the reader made rather
            // than a copy of the scalar: it is a heap value the collector
            // reaches through the node, which is itself reachable from the
            // registered function body, so there is nothing to keep
            // heap-independent the way the old owned `Expr` payload had to be
            // (`bignum` held a `BigInt`). `bignum`/`ratio` have no
            // `expected`-driven width family the way an integer/float literal
            // does, so their type is always the one primitive.
            Value::Boxed(id) if heap.is_bignum(id) => {
                Checked::new(core::tagged(heap, "bignum", &[Value::Boxed(id)])?, Type::Bignum)
            }
            Value::Boxed(id) if heap.is_ratio(id) => {
                Checked::new(core::tagged(heap, "ratio", &[Value::Boxed(id)])?, Type::Ratio)
            }
            Value::Boxed(id) => {
                Checked::new(core::tagged(heap, "float", &[Value::Boxed(id)])?, float_lit_ty(expected))
            }
            Value::Bool(b) => Checked::new(core::tagged(heap, "bool", &[Value::Bool(b)])?, Type::Bool),
            Value::Char(c) => Checked::new(core::tagged(heap, "char", &[Value::Char(c)])?, Type::Char),
            Value::Str(s) => Checked::new(core::tagged(heap, "str", &[Value::Str(s)])?, Type::Str),
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
                        return self.check_construct(heap, interp, env, (&Path::root("option"), OPTION_NONE), &[], &[], expected);
                    }
                    // The `Sexpr::nil` branch that used to sit here is gone:
                    // the empty list is `Option<Sexpr>`'s `none` now, which
                    // the `option` arm just above already produces. A `()`
                    // written where a bare `Sexpr` is expected is no longer
                    // an empty list at all — `Sexpr` means "a non-empty
                    // S-expression" — so it falls through to `Unit` like any
                    // other non-option, non-sexpr context.
                }
                Checked::new(core::tagged(heap, "unit", &[])?, Type::Unit)
            }
            Value::Symbol(id) => {
                let name = heap.symbol_name(id).to_string();
                let name = name.as_str();
                // A keyword (`:name`) is self-evaluating (CL), so it is
                // matched *before* every binding lookup below — it can never
                // name a variable, global, function or method. `::foo` is the
                // absolute-path syntax, not a keyword, and never reaches here
                // as a `Value::Symbol` (the reader makes it a `Value::Path`);
                // the `!starts_with("::")` guard keeps that distinction
                // explicit alongside `read::reader::validate_keyword`'s.
                if name.starts_with(':') && !name.starts_with("::") {
                    // The interned symbol *including* the leading colon, so
                    // "same name -> same object" falls out of the heap's own
                    // interning with no separate keyword table.
                    Checked::new(core::tagged(heap, "sym", &[Value::Symbol(id)])?, Type::Symbol)
                } else if let Some(t) = env.get(name) {
                    let t = t.clone();
                    Checked::new(forms::var_form(heap, name)?, t)
                } else if let Some((path, vi)) = self.resolve_global(name) {
                    let ty = vi.ty;
                    let r = self.mk_ref(vec![name.to_string()], path);
                    Checked::new(self.global_form(heap, &r, &ty)?, ty)
                } else if let Some(t) = self.fn_value(heap, name, expected) {
                    t?
                } else if let Some(t) = self.method_value(heap, name, expected) {
                    t?
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
                    let ty = vi.ty;
                    let r = self.mk_ref(segs.clone(), path);
                    Checked::new(self.global_form(heap, &r, &ty)?, ty)
                } else if let Some(t) = self.fn_path_value(heap, &segs, expected) {
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
                // already exactly its `Sexpr::Sym` — a `Value::Symbol`, see
                // `construct_sexpr`'s SEXPR_SYM arm — so widening
                // the static type needs no runtime work. Emit a transparent
                // *retype*, not a `Sexpr::Sym` constructor node: the
                // interpreter treated that constructor as a no-op, but the
                // compiler lowers a variant-5 construct as
                // `compile-construct-sym`, which interns a *name string*.
                // Feeding it an already-built `Symbol` value (a `gensym`'d temp
                // captured by a now-compiled macro-expansion lambda) aborts in
                // `rt_intern_symbol`. A bare retype is correct for both tiers.
                // A `Sexpr` reaching an `Option<Sexpr>` expectation widens
                // for free. This is the one direction the empty list's niche
                // makes safe: `Option<Sexpr>` is represented *exactly* like a
                // `Sexpr` (`check/repr.rs`), with `none` taking the
                // empty-list word — so a `Sexpr`, which is by construction
                // never that word, is already a well-formed non-empty
                // `Option<Sexpr>`. No runtime work, hence a bare retype.
                //
                // The opposite direction stays a type error, and that is
                // where this migration's static checking lives: narrowing an
                // `Option<Sexpr>` to a `Sexpr` is the claim "this is not the
                // empty list", which only a `match` (or `unwrap`) can
                // discharge.
                if is_option_of_sexpr(e) && typed.ty == sexpr_ty() {
                    return Ok(Checked::new(typed.form, e.clone()));
                }
                if is_sexpr_expectation(e) && typed.ty == Type::Symbol {
                    return Ok(Checked::new(typed.form, e.clone()));
                }
                // A user ADT instance (`defstruct`/`defenum`, `Vector<T>`,
                // `HashTable<K,V>`, `cons-cell<K,V>`) is a valid `Sexpr` datum
                // wherever a `Sexpr` is expected — the CL-conformant "cons
                // cells hold arbitrary objects" behavior the pretty-printer
                // design discussion (TODO T5) decided this codebase should
                // have. Exactly like the `Symbol` case just above, its
                // runtime representation needs no conversion: every
                // `is_heap_repr` type's instantiation already evaluates to
                // a `Value::Boxed(_)` (`construct`'s
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
                if is_sexpr_expectation(e) && matches!(typed.ty, Type::Dyn(..)) {
                    let form = forms::dyn_value_form(heap, typed.form)?;
                    return Ok(Checked::new(form, e.clone()));
                }
                // `Option<Sexpr>` is excluded: it *is* heap-repr by
                // `is_heap_repr`'s reckoning (an `option` is a `Sum` with
                // variants), so without this guard the retype above would
                // fire on it and hand a bare `Sexpr` expectation a value
                // that may be the empty list — a narrowing, wearing a
                // widening's clothes. It type-checked and then fell over at
                // runtime: an exhaustive ten-arm `match` on the resulting
                // `Sexpr` found no arm to take, because the empty list is
                // not one of `Sexpr`'s shapes. Narrowing is what `match`
                // and `unwrap` are for.
                if is_sexpr_expectation(e) && self.is_heap_repr(&typed.ty) && !is_option_of_sexpr(&typed.ty) {
                    return Ok(Checked::new(typed.form, e.clone()));
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
                    return self.coerce_to_dyn(heap, env, typed, trait_path, pins);
                }
                if is_sexpr_expectation(e) {
                    if let Some(ctor) = sexpr_ctor_for(&typed.ty) {
                        let (type_name, variant) =
                            self.resolve_ctor(ctor).expect("sexpr constructors are always registered");
                        let field_tys = self.variant_field_tys(&type_name, variant, &[]);
                        let form =
                            self.construct_form(heap, &type_name, variant, false, &field_tys, &[typed.form])?;
                        return Ok(Checked::new(form, e.clone()));
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
    ) -> Result<Checked, Error> {
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
            return self.check_path_call(heap, interp, env, &segs, nth_loc(&elem_locs, 0).as_ref(), args, arg_locs, expected);
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
        // SPECIAL-FORM DISPATCH BEGIN
        // The arms below are the authoritative list of special forms. They have
        // no runtime representation to enumerate, so `tests/editor_keyword_sync_test.rs`
        // reads them out of this file between these two sentinels to check that
        // both editor definitions know every one. Keep the sentinels in place.
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
                let form = self.let_form(heap, &[], &body)?;
                return Ok(Checked::new(form, ty));
            }
            "setf" => return self.check_setf(heap, interp, env, args, arg_locs),
            "incf" => return self.check_incf_decf(heap, interp, env, args, "+"),
            "decf" => return self.check_incf_decf(heap, interp, env, args, "-"),
            "rotatef" => return self.check_rotatef_shiftf(heap, interp, env, args, false),
            "shiftf" => return self.check_rotatef_shiftf(heap, interp, env, args, true),
            "loop" => return self.check_loop(heap, interp, env, args, arg_locs, expected),
            "catch" => return self.check_catch(heap, interp, env, args, arg_locs, expected),
            "throw" => return self.check_throw(heap, interp, env, args, arg_locs),
            "unwind-protect" => return self.check_unwind_protect(heap, interp, env, args, arg_locs, expected),
            "break" => return self.check_break(heap, args),
            "return" => return self.check_return(heap, interp, env, args, arg_locs),
            "list" => return self.check_list_lit(heap, interp, env, args, arg_locs),
            "format" => return self.check_format(heap, interp, env, args, arg_locs),
            "print" => return self.check_print_like(heap, interp, env, "print-rt", Type::Unit, args, arg_locs),
            "println" => return self.check_print_like(heap, interp, env, "println-rt", Type::Unit, args, arg_locs),
            // These two are *shadowable* special forms: a local binding named
            // `read` (a lambda called `(read)`, say) must keep winning, the
            // way it did when `read` was an ordinary built-in function. The
            // dispatch here runs before the local-variable lookup further
            // down, so the guard has to be explicit — unlike `if`/`let`/
            // `format`, which `is_builtin_form_head` reserves outright.
            "pprint" | "pprint-fill" | "pprint-linear" | "pprint-tabular" => {
                return self.check_pprint(heap, interp, env, head.as_str(), args, arg_locs)
            }
            "pprint-logical-block" => return self.check_pprint_logical_block(heap, interp, env, args, arg_locs),
            "lambda" => return self.check_lambda(heap, interp, env, args, arg_locs),
            "labels" => return self.check_labels(heap, interp, env, args, arg_locs, expected),
            "apply" => return self.check_apply_form(heap, interp, env, args, arg_locs),
            "match" => return self.check_match(heap, interp, env, args, arg_locs, expected),
            "panic" => return self.check_panic(heap, interp, env, args, arg_locs),
            "the" => return self.check_the(heap, interp, env, args, arg_locs),
            "as" => return self.check_as(heap, interp, env, args, arg_locs, false),
            "try-as" => return self.check_as(heap, interp, env, args, arg_locs, true),
            "compile" => return self.check_compile(heap, args),
            "documentation" => return self.check_documentation(heap, args),
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
        // SPECIAL-FORM DISPATCH END
        // A local variable holding a function value is applied directly (locals
        // shadow free functions). Goes through `check_at` (not a hand-built
        // node) so this callee reference gets its own position — the same
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
        } else if args.len() <= 1
            && matches!(
                head.as_str(),
                "+" | "-" | "*" | "/" | "max" | "min" | "logand" | "logior" | "logxor" | "gcd" | "lcm" | "<" | "<=" | ">" | ">=" | "=" | "/="
            )
        {
            self.check_nullary_or_unary_numeric_op(heap, interp, env, &head, args, expected)
        } else if args.len() > 2 && matches!(head.as_str(), "+" | "-" | "*" | "/" | "max" | "min" | "logand" | "logior" | "logxor" | "gcd" | "lcm") {
            self.check_variadic_arith(heap, interp, env, &head, args, expected)
        } else if args.len() > 2 && matches!(head.as_str(), "<" | "<=" | ">" | ">=" | "=" | "/=") {
            self.check_variadic_cmp(heap, interp, env, &head, args)
        } else if head == "aref" && !args.is_empty() {
            self.check_aref(heap, interp, env, args[0], &args[1..], None, expected)
        } else if head == "log" && args.len() == 2 {
            self.check_log_with_base(heap, interp, env, args, expected)
        } else if head == "atan" && args.len() == 2 {
            // CL's two-argument `(atan y x)`: the prelude's `atan2`. Sugar
            // here for the same reason two-argument `log` is — `defmethod`
            // overloads on receiver type, never on arity, and one-argument
            // `atan` is already `f64`'s built-in method.
            self.check_renamed_call(heap, interp, env, "atan2", args, arg_locs)
        } else if let Some(result) = self.try_instance_method(heap, interp, env, &head, args, arg_locs) {
            result
        } else if let Some(result) = self.try_instance_method_swapped(heap, interp, env, &head, args, arg_locs) {
            result
        } else if let Some(fq) = self.resolve_fn(&head) {
            self.check_call(heap, interp, env, std::slice::from_ref(&head), &fq, args, arg_locs)
        } else if let Some((path, vi)) = self.resolve_global(&head) {
            let r = self.mk_ref(vec![head.clone()], path);
            let form = self.global_form(heap, &r, &vi.ty)?;
            let callee = Checked::new(form, vi.ty);
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
    ) -> Result<Checked, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError("lambda: (lambda (params) ret body...)".into()));
        }
        if params_declare_opt_key(heap, args[0])? {
            return Err(Error::TypeError(OPT_KEY_NEEDS_A_NAME.replace("{}", "lambda")));
        }
        let (params, param_locs, rest) = self.parse_params_rest(heap, args[0])?;
        let ret = self.parse_type_here_at(heap, args[1], arg_locs.get(1).and_then(|l| l.as_ref()))?;
        let fn_ty = Type::Fn(
            params.iter().map(|(_, t)| t.clone()).collect(),
            rest.as_ref().map(|(_, t, _)| Box::new(t.clone())),
            Box::new(ret.clone()),
        );
        // The body additionally sees the `&rest` parameter (if any) bound to
        // a plain S-expression list — see `Self::check_defun`'s identical
        // treatment. `Option<Sexpr>`, like every other list here: the empty
        // list is what a `&rest` binds when no variadic argument was passed,
        // so a bare `Sexpr` could not even spell the common case.
        let mut params = params;
        let mut param_locs = param_locs;
        if let Some((rname, _, rloc)) = rest {
            params.push((rname, option_of_sexpr()));
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
        let form = self.lambda_form(heap, &params, &ret, &body)?;
        Ok(Checked::new(form, fn_ty))
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
    ) -> Result<Checked, Error> {
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
            if params_declare_opt_key(heap, parts[1])? {
                return Err(Error::TypeError(OPT_KEY_NEEDS_A_NAME.replace("{}", "labels")));
            }
            let (params, param_locs) = self.parse_params(heap, parts[1])?;
            let ret = self.parse_type_here_at(heap, parts[2], parts_locs.get(2).and_then(|(_, l)| l.as_ref()))?;
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
            defs.push((name, params, ret, body));
        }
        let (body, ty) = self.check_seq(heap, interp, &labels_env, &args[1..], &arg_locs[1..], expected)?;
        let form = self.labels_form(heap, &defs, &body)?;
        Ok(Checked::new(form, ty))
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
        callee: Checked,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
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
            typed.push(self.cons_rest_list(heap, elem_ty, rest_typed)?);
        }
        let form = self.apply_form(heap, callee.form, &ret, &typed)?;
        Ok(Checked::new(form, ret))
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
    /// `apply` shape `check_apply` produces for `(f arg1 .. argN e1 e2
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
    ) -> Result<Checked, Error> {
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
        typed.push(self.check_at(heap, interp, env, list_arg[0], Some(&option_of_sexpr()), list_loc)?);
        let form = self.apply_form(heap, callee.form, &ret, &typed)?;
        Ok(Checked::new(form, ret))
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
        head_loc: Option<&Loc>,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
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
                // The head token names this type in its second-to-last
                // segment (`rect::new`, `mod::color::red`) — record that
                // segment's span for semantic highlighting.
                self.record_path_type_use(segs, &type_fq, head_loc);
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
    ) -> Option<Result<Checked, Error>> {
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
            let form = match forms::dyn_value_form(heap, recv.form) {
                Ok(f) => f,
                Err(e) => return Some(Err(e)),
            };
            recv = Checked::new(form, sexpr_ty());
        }
        let type_fq = self.assoc_receiver_path(&recv.ty, method)?;
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
    /// Does `Sexpr`'s own assoc table carry `method`?
    ///
    /// The test that lets an `Option<Sexpr>` receiver reach that table
    /// without shadowing `Option`'s own methods — see the receiver match in
    /// [`Self::check_instance_method`].
    fn sexpr_has_method(&self, method: &str) -> bool {
        self.reg
            .type_def(&Path::root("sexpr"))
            .is_some_and(|d| d.assoc.get(method).is_some_and(|af| af.instance))
    }

    /// Whose assoc table serves `method` called on a receiver of type `ty`.
    ///
    /// Normally the receiver's own type. The exception is `Option<Sexpr>`:
    /// it *is* the type an S-expression has, so `Sexpr`'s catalog
    /// (`eq`/`eql`/`equals`/`print`/...) has to reach it — the same "an
    /// option over `Sexpr` behaves as an S-expression" rule the `match`
    /// sugar follows. `Sexpr`'s signatures already take `Option<Sexpr>`, so
    /// nothing else changes; without it the whole catalog disappears the
    /// moment a value is typed `Option<Sexpr>`.
    ///
    /// `Option`'s own methods (`unwrap`/`is-some`/...) still resolve: the
    /// redirection is conditional on `Sexpr` actually having the method, so
    /// anything it does not have falls through to `option` as before.
    ///
    /// Every receiver-type lookup goes through here. Three separate copies
    /// of this decision are what let a value pattern keep working on a
    /// `Sexpr` scrutinee while quietly failing on an `Option<Sexpr>` one.
    fn assoc_receiver_path(&self, ty: &Type, method: &str) -> Option<Path> {
        match ty {
            t if is_option_of_sexpr(t) && self.sexpr_has_method(method) => Some(Path::root("sexpr")),
            Type::Named(n, _) => Some(n.clone()),
            other => prim_type_path(other),
        }
    }

    fn check_instance_method(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
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
                if self
                    .reg
                    .trait_def(&trait_path)
                    .is_some_and(|t| t.vtable_order.iter().any(|m| m == method))
                {
                    return self.check_dyn_call(
                        heap, interp, env, recv, &trait_path, &pins, method, &args[1..], &arg_locs[1..],
                    );
                }
                if !self.sexpr_has_instance_method(method) {
                    return self.check_dyn_call(
                        heap, interp, env, recv, &trait_path, &pins, method, &args[1..], &arg_locs[1..],
                    );
                }
                let form = forms::dyn_value_form(heap, recv.form)?;
                recv = Checked::new(form, sexpr_ty());
            }
            let type_fq = self.assoc_receiver_path(&recv.ty, method);
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
                // A method no `impl` put on the type, but a blanket impl
                // provides. The signature comes from the `TraitDef` template
                // with `Self` bound to the receiver — the definition itself
                // is generated by the queued materialization, which
                // `check_form_at` drains ahead of this form.
                if let Some(t) =
                    self.blanket_method_owner(&recv.ty, type_fq, method)
                {
                    return self.check_blanket_method_call(
                        heap, interp, env, recv, type_fq, &t, method, &args[1..], &arg_locs[1..],
                    );
                }
                // No concrete `AdtDef` named `type_fq` — it may be one of
                // this function's own `where`-bounded type parameters
                // (`Env::bounds`, keyed by the lowercase type-variable name,
                // exactly what `type_fq.last_segment()` is for a bare `Type::Named`
                // type variable like `t`). Search every trait it's bound to
                // for a matching method; see `Checker::trait_call_panic_form`
                // for why the implementing type is resolved at runtime
                // instead of here.
                if let Some(bound_traits) = env.bounds.get(type_fq.last_segment()) {
                    // An operator on a bounded type variable means its trait's
                    // method: `(+ a b)` under `(where (Add T))` is `(add a b)`.
                    // The trait cannot declare the operator itself — an `impl`
                    // naming a method `+` is refused, because `+` belongs to
                    // the primitive types' built-in tables — so the two
                    // spellings meet here, in the one place a receiver is known
                    // to be a type variable rather than a type. A bound that
                    // declares the written name itself always wins: this is a
                    // fallback for a name no bound has, never a rename.
                    let method = match trait_operator_method(method) {
                        Some(alias)
                            if !bound_traits.iter().any(|tb| {
                                self.reg
                                    .trait_def(&tb.trait_path)
                                    .is_some_and(|t| self.reg.trait_method(t, method).is_some())
                            }) =>
                        {
                            alias
                        }
                        _ => method,
                    };
                    for tb in bound_traits {
                        let Some(tdef) = self.reg.trait_def(&tb.trait_path) else { continue };
                        // Inherited methods included: bounding `T` by `Ord`
                        // makes `Eq`'s `equals` callable on it, since `Ord`
                        // obliges every implementor to implement `Eq` too.
                        let Some((decl, sig)) = self.reg.trait_method(tdef, method) else { continue };
                        let decl_path = decl.name.clone();
                        let want = sig.params.len() - 1;
                        if args.len() - 1 != want {
                            return Err(Error::TypeError(format!(
                                "{}: expected {} argument(s), got {}",
                                method,
                                want,
                                args.len() - 1
                            )));
                        }
                        // Checked for their own diagnostics only — the node
                        // they used to fill is never executed, so the lowered
                        // forms are dropped here.
                        for (i, a) in args[1..].iter().enumerate() {
                            self.check_at(heap, interp, env, *a, None, nth_loc(&arg_locs[1..], i))?;
                        }
                        // This bound's `where`-clause associated-type pins
                        // (e.g. `(Item i32)`) resolve the trait method
                        // template's otherwise-opaque associated-type
                        // variables (e.g. `Option<Item>`) wherever pinned —
                        // a no-op (returns `sig.ret` unchanged) when
                        // `tb.assoc` is empty, exactly matching pre-pin
                        // behavior.
                        // ...and composed down the supertrait chain when the
                        // method is an inherited one, since its template is
                        // written in the declaring trait's associated types.
                        let Some(mut assoc) = self.trait_assoc_subst_for(tdef, &tb.assoc, &decl_path)
                        else {
                            return Err(Error::TypeError(format!(
                                "{}: `{}` declares `{}`, but is not in `{}`'s supertrait chain",
                                method, decl_path, method, tb.trait_path
                            )));
                        };
                        // `Self` in the template *is* this bounded type
                        // variable. Without this binding a trait method
                        // declared to return `Self` — `(add ((self Self)
                        // (other Self)) Self)` — hands the literal `self`
                        // type variable back to a call justified by `(where
                        // (Add T))`, and the enclosing generic's `T` return
                        // position rejects it ("expected t, found self").
                        // Same `"self"` key that `subst_method_sig` and
                        // `check_trait_call` bind for a concrete receiver;
                        // no prelude trait method returned `Self` (they all
                        // return `bool` or an associated type), which is why
                        // the gap survived until the `Number` trait layer
                        // needed it.
                        assoc.insert("self".to_string(), recv.ty.clone());
                        let ret_ty = subst_apply(&sig.ret, &assoc);
                        let form = forms::erased_generic_form(heap, method)?;
                        return Ok(Checked::new(form, ret_ty));
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
    ) -> Result<Checked, Error> {
        let AssocCall { type_fq, method, receiver, expected } = call;
        let instance = receiver.is_some();
        let def = self.reg.type_def(type_fq).expect("assoc type exists").clone();
        let af = def.assoc[method].clone();

        let mut subst: BTreeMap<String, Type> = BTreeMap::new();
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
        let mut typed = Vec::new();
        if let Some(r) = receiver {
            typed.push(r);
        }
        let owner_params: HashSet<String> = def.params.iter().cloned().collect();
        if af.sig.optionals.is_empty() && af.sig.keys.is_empty() && af.sig.rest.is_none() {
            let declared = &af.sig.params[offset..];
            if args.len() != declared.len() {
                return Err(Error::TypeError(format!(
                    "{}::{}: expected {} argument(s), got {}",
                    type_fq,
                    method,
                    declared.len(),
                    args.len()
                )));
            }
            // The arguments refine `subst` too, exactly as in
            // `Self::check_call`. For an instance method the receiver already
            // fixed every owner parameter and this only confirms it; what
            // needs it is a **static** function on a generic owner
            // (`cell::of`), where there is no receiver and the expected type
            // may say nothing — the argument is the only evidence. Without
            // this such a call could only be written where its result type
            // was already known.
            for (i, (arg, pty)) in args.iter().zip(declared.iter()).enumerate() {
                let st = subst_apply(pty, &subst);
                let exp = if type_has_param(&st, &owner_params) { None } else { Some(st) };
                let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, i))?;
                unify(&owner_params, pty, &ta.ty, &mut subst)?;
                typed.push(ta);
            }
        } else {
            let who = format!("{}::{}", type_fq, method);
            self.push_assoc_opt_key_args(
                heap, interp, env, &who, &af.sig, offset, &owner_params, &mut subst, args, arg_locs, &mut typed,
            )?;
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
        let ty = subst_apply(&af.sig.ret, &subst);
        let home = self.ns.clone();
        let form = self.assoc_form(heap, type_fq, &method_name, instance, &home, &ty, &typed)?;
        Ok(Checked::new(form, ty))
    }

    /// [`Self::check_assoc_call`]'s `&optional`/`&key`/`&rest` path — the
    /// method-side twin of [`Self::check_call_opt_key`], and it fills the
    /// argument list in exactly the same way: one checked actual per runtime
    /// parameter, in `MethodSig::params` order, so nothing downstream ever
    /// learns that this method's lambda list had regions in it.
    ///
    /// Simpler than the free-function version in one respect: a method
    /// signature has no type parameters of its own (`FnSig::type_params` is
    /// always empty for an `AssocFn`), so there is no inference pass here.
    /// The only substitution in play is `subst`, already resolved from the
    /// receiver's — or the expected type's — own type arguments by the
    /// caller, which is also why a defaulted parameter may not mention the
    /// owner's type parameters (`Self::check_defmethod_in` rejects that at
    /// the definition).
    #[allow(clippy::too_many_arguments)]
    fn push_assoc_opt_key_args(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        who: &str,
        sig: &FnSig,
        offset: usize,
        owner_params: &HashSet<String>,
        subst: &mut BTreeMap<String, Type>,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        typed: &mut Vec<Checked>,
    ) -> Result<(), Error> {
        let required = &sig.params[offset..];
        if args.len() < required.len() {
            return Err(Error::TypeError(format!(
                "{}: expected at least {} argument(s), got {}",
                who,
                required.len(),
                args.len()
            )));
        }
        for (i, (arg, pty)) in args[..required.len()].iter().zip(required.iter()).enumerate() {
            let st = subst_apply(pty, subst);
            let exp = if type_has_param(&st, owner_params) { None } else { Some(st) };
            let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, i))?;
            unify(owner_params, pty, &ta.ty, subst)?;
            typed.push(ta);
        }
        let tail = &args[required.len()..];
        let tail_locs = arg_locs.get(required.len()..).unwrap_or(&[]);

        if !sig.keys.is_empty() {
            // `&key`: the trailing arguments are `:name value` pairs matched
            // by label. `&key` never coexists with `&optional`/`&rest`
            // (`Self::parse_params_full` rejects the combination).
            if tail.len() % 2 != 0 {
                return Err(Error::TypeError(format!(
                    "{}: keyword arguments must be given as `:name value` pairs",
                    who
                )));
            }
            let mut supplied: HashMap<String, Checked> = HashMap::new();
            let mut i = 0;
            while i < tail.len() {
                let kw = match tail[i] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError(format!("{}: expected a `:name` keyword, got a non-symbol", who))),
                };
                let Some(kw) = kw.strip_prefix(':').map(|s| s.to_string()) else {
                    return Err(Error::TypeError(format!("{}: expected a `:name` keyword, got `{}`", who, kw)));
                };
                let Some(key) = sig.keys.iter().find(|k| k.name == kw) else {
                    return Err(Error::TypeError(format!("{}: unknown keyword argument :{}", who, kw)));
                };
                if supplied.contains_key(&kw) {
                    return Err(Error::TypeError(format!("{}: duplicate keyword argument :{}", who, kw)));
                }
                let st = subst_apply(&key.decl_ty, subst);
                let exp = if type_has_param(&st, owner_params) { None } else { Some(st) };
                let checked = self.check_at(heap, interp, env, tail[i + 1], exp.as_ref(), nth_loc(tail_locs, i + 1))?;
                unify(owner_params, &key.decl_ty, &checked.ty, subst)?;
                supplied.insert(kw, checked);
                i += 2;
            }
            for key in &sig.keys {
                let val = match supplied.remove(&key.name) {
                    Some(checked) => {
                        if key.default.is_some() {
                            checked
                        } else {
                            let target = checked.ty.clone();
                            wrap_some(heap, self, checked, target)?
                        }
                    }
                    None => match &key.default {
                        Some(d) => forms::splice_default(heap, d, &key.decl_ty)?,
                        None => option_none(heap, self, subst_apply(&key.effective_ty(), subst))?,
                    },
                };
                typed.push(val);
            }
            return Ok(());
        }

        // `&optional` (+ possibly `&rest`): filled strictly by position.
        let supplied_n = tail.len().min(sig.optionals.len());
        let mut supplied: Vec<Checked> = Vec::with_capacity(supplied_n);
        for (i, opt) in sig.optionals.iter().enumerate().take(supplied_n) {
            let st = subst_apply(&opt.decl_ty, subst);
            let exp = if type_has_param(&st, owner_params) { None } else { Some(st) };
            let checked = self.check_at(heap, interp, env, tail[i], exp.as_ref(), nth_loc(tail_locs, i))?;
            unify(owner_params, &opt.decl_ty, &checked.ty, subst)?;
            supplied.push(checked);
        }
        let after = &tail[supplied_n..];
        let after_locs = tail_locs.get(supplied_n..).unwrap_or(&[]);
        let mut rest_typed: Vec<Checked> = Vec::new();
        match &sig.rest {
            Some(elem_ty) => {
                for (i, arg) in after.iter().enumerate() {
                    let st = subst_apply(elem_ty, subst);
                    let exp = if type_has_param(&st, owner_params) { None } else { Some(st) };
                    let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(after_locs, i))?;
                    unify(owner_params, elem_ty, &ta.ty, subst)?;
                    rest_typed.push(ta);
                }
            }
            None if !after.is_empty() => {
                return Err(Error::TypeError(format!(
                    "{}: expected at most {} argument(s), got {}",
                    who,
                    required.len() + sig.optionals.len(),
                    args.len()
                )));
            }
            None => {}
        }
        let mut supplied = supplied.into_iter();
        for (i, opt) in sig.optionals.iter().enumerate() {
            let val = if i < supplied_n {
                let checked = supplied.next().expect("one checked value per supplied optional");
                if opt.default.is_some() {
                    checked
                } else {
                    let target = checked.ty.clone();
                    wrap_some(heap, self, checked, target)?
                }
            } else {
                match &opt.default {
                    Some(d) => forms::splice_default(heap, d, &opt.decl_ty)?,
                    None => option_none(heap, self, subst_apply(&opt.effective_ty(), subst))?,
                }
            };
            typed.push(val);
        }
        if let Some(elem_ty) = &sig.rest {
            let resolved = subst_apply(elem_ty, subst);
            typed.push(self.cons_rest_list(heap, &resolved, rest_typed)?);
        }
        Ok(())
    }

    fn check_if(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        if args.len() != 3 {
            return Err(Error::TypeError("if: (if cond then else)".into()));
        }
        let cond = self.check_at(heap, interp, env, args[0], Some(&Type::Bool), nth_loc(arg_locs, 0))?;
        let then = self.check_at(heap, interp, env, args[1], expected, nth_loc(arg_locs, 1))?;
        // A diverging (`Never`) then branch must not constrain the else branch.
        let else_expected = non_never(&then.ty).or(expected);
        let els = self.check_at(heap, interp, env, args[2], else_expected, nth_loc(arg_locs, 2))?;
        let ty = join_types(&then.ty, &els.ty)?;
        let form = forms::if_form(heap, cond.form, then.form, els.form)?;
        Ok(Checked::new(form, ty))
    }

    fn check_panic(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("panic: (panic message)".into()));
        }
        let msg = self.check_at(heap, interp, env, args[0], Some(&Type::Str), nth_loc(arg_locs, 0))?;
        let form = forms::panic_form(heap, msg.form)?;
        Ok(Checked::new(form, Type::Never))
    }

    /// `(the Type expr)`: a type annotation, e.g. `(the i64 5)` to make an
    /// integer literal default to `i64` instead of `i32`, or to pin down a
    /// generic call's type argument the way an `expected` type elsewhere
    /// would. Purely a checking-time hint with no runtime behavior of its
    /// own — `Type` simply becomes `expr`'s `expected` (the same role it
    /// plays for a `defun` parameter or `let` binding annotation), and the
    /// returned form is exactly `expr`'s own (no node of its own; `the`
    /// vanishes after checking).
    fn check_the(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("the: (the Type expr)".into()));
        }
        let ty = self.parse_type_here_at(heap, args[0], arg_locs.first().and_then(|l| l.as_ref()))?;
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
    ) -> Result<Checked, Error> {
        let form_name = if try_variant { "try-as" } else { "as" };
        if args.len() != 2 {
            return Err(Error::TypeError(format!("{}: ({} Type expr)", form_name, form_name)));
        }
        let target = self.parse_type_here_at(heap, args[0], arg_locs.first().and_then(|l| l.as_ref()))?;
        let src = self.check_at(heap, interp, env, args[1], None, nth_loc(arg_locs, 1))?;

        // Identity: same type, a no-op cast.
        if src.ty == target {
            return if try_variant { wrap_some(heap, self, src, target) } else { Ok(src) };
        }
        // Between two integer widths, or between the two float widths: a pure
        // relabel, no runtime effect. `Value::Int` is uniformly `i64` and
        // `Value::Float` uniformly `f64` regardless of which static width
        // labels them (see `registry::int_assoc`'s `int->float` doc comment) —
        // there is no truncating representation anywhere in this codebase for
        // `as`/`try-as` to imitate, so crossing widths is total in both
        // directions. `try-as` between them is therefore always `some`, which
        // is worth knowing when reading code that asks: it says the *static*
        // type changed and nothing else did.
        if (src.ty.is_integer() && target.is_integer()) || (src.ty.is_float() && target.is_float()) {
            let relabeled = Checked::new(src.form, target.clone());
            return if try_variant { wrap_some(heap, self, relabeled, target) } else { Ok(relabeled) };
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
            return self.coerce_to_dyn(heap, env, src, trait_path, pins);
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
        // an `I32` return, and `int->float` with an `F64` one, whichever width
        // the caller asked for — see `as_conversion`'s doc comment. The result
        // is relabeled to the target here, which is all a width is: by
        // construction of `as_conversion`'s table, `called`'s (unwrapped, for
        // a `try_method` result) type is the catalog width of the target's own
        // family.
        let relabel = target != called_width(&target);
        if try_variant {
            match try_method {
                Some(_) => Ok(if relabel { retype_option(called, target) } else { called }),
                None => {
                    let called = if relabel { Checked::new(called.form, target.clone()) } else { called };
                    wrap_some(heap, self, called, target)
                }
            }
        } else if relabel {
            Ok(Checked::new(called.form, target))
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
    /// A name this can't resolve is a check-time error, like every other
    /// unresolvable reference in the language. There is nothing left to look
    /// up at runtime — the resolution above is the answer — so deferring the
    /// failure to `Interp::resolve_fn_ref` would only move a diagnosis the
    /// checker already made past the point where it can name a source
    /// position. "Genuinely private from here" fails the same way: both
    /// `resolve_fn` and `resolve_fn_path` fold "exists but not visible" into
    /// "doesn't resolve", so `(compile other-modules-private)` is rejected
    /// with the rest, exactly as an ordinary call to it would be.
    ///
    /// Three distinguishable failures, three messages: an unresolvable bare
    /// name, a `Type::method` whose type resolved but has no such member, and
    /// a `a::b` that names neither a type's member nor a free function.
    fn check_compile(&self, heap: &mut Heap, args: &[Value]) -> Result<Checked, Error> {
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
        // same "no resolution -> check-time error".
        let fn_target = |written: Vec<String>, resolved: Option<Path>| -> Result<CompileTarget, Error> {
            let resolved = resolved.ok_or_else(|| {
                Error::TypeError(format!("compile: no function `{}` is visible from here", name))
            })?;
            if self.generic_fn_templates.contains_key(&resolved) {
                return Err(generic_err());
            }
            Ok(CompileTarget::Fn(self.mk_ref(written, resolved)))
        };
        // A macro resolves too. Its body is an ordinary `Sexpr -> Sexpr`
        // function — the *interpreter* keeps it in the same `fns` table a
        // `defun` goes in, so a `CompileTarget::Fn` reaches it — but the
        // *checker* keeps functions and macros in separate maps, so
        // `resolve_fn` alone comes up empty and `(compile <macro>)` used to be
        // rejected as "no function ... is visible from here". Functions first:
        // that is the existing resolution, and this only widens what a name
        // that resolved to nothing can still mean.
        let fn_or_macro = |name: &str| self.resolve_fn(name).or_else(|| self.resolve_macro(name).map(|(p, _)| p));
        let target = match name.rsplit_once("::") {
            None => fn_target(vec![name.clone()], fn_or_macro(&name))?,
            Some((type_part, method)) => {
                let type_segs: Vec<String> = type_part.split("::").map(|s| s.to_string()).collect();
                // Kept separate from `type_fq` below: whether the *type* half
                // resolved is what tells "this type has no such method" apart
                // from "this names nothing at all", and both are reachable
                // only after `resolve_fn_path` has also come up empty.
                let type_path = if type_segs.len() == 1 {
                    self.resolve_bare_type(&type_segs[0])
                } else {
                    self.resolve_type_path(&type_segs)
                };
                let type_fq = type_path
                    .clone()
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
                    // at all (`(compile bogus::x)`, `(compile point::bogus)`).
                    None => {
                        let full_segs: Vec<String> = name.split("::").map(|s| s.to_string()).collect();
                        match self.resolve_fn_path(&full_segs) {
                            Some(resolved) => fn_target(full_segs, Some(resolved))?,
                            None => {
                                return Err(Error::TypeError(match &type_path {
                                    Some(tp) => format!(
                                        "compile: type `{}` has no associated function or method `{}`",
                                        tp, method
                                    ),
                                    None => format!(
                                        "compile: `{}` names neither a type's method nor a function visible from here",
                                        name
                                    ),
                                }))
                            }
                        }
                    }
                }
            }
        };
        let form = forms::compile_fn_form(heap, &target)?;
        Ok(Checked::new(form, Type::Bool))
    }

    /// `(documentation name)` / `(documentation Type::method)`: like
    /// `(compile ...)`, `name` is program structure, not runtime data, so it
    /// reads as an unevaluated symbol or `::`-path — see `Self::check_compile`'s
    /// doc comment, whose `name.rsplit_once("::")` dispatch this mirrors.
    /// Resolved entirely at check time, since the checker always knows where
    /// a name resolves (there's nothing left to look up at runtime): a bare
    /// name is tried, in order, as a variable, a function, a type, a trait,
    /// then a macro (first hit wins — the same var-before-fn-before-method
    /// priority a bare identifier gets when checked as an ordinary
    /// expression, `Self::check`'s `Value::Symbol` arm), and the result — a
    /// docstring if the resolved definition has one, `none` if it doesn't —
    /// is baked in directly as an `Option<Str>` constant. Resolving to
    /// *nothing at all* is a check-time error, same as any other unbound
    /// reference; unlike `(compile ...)`, there's no runtime fallback to
    /// defer to, since this never produces a runtime lookup in the first
    /// place. Module-qualified free names (`mod::name`, as opposed to
    /// `Type::method`) are out of scope for now.
    fn check_documentation(&self, heap: &mut Heap, args: &[Value]) -> Result<Checked, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("documentation: (documentation name) — expected exactly 1 argument".into()));
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
                    "documentation: expected a symbol or path naming a definition, e.g. (documentation foo) or (documentation point::x) — not a string".into(),
                ))
            }
        };
        // Resolved to the docstring first, then built — `self.reg` lookups
        // borrow `self` while `doc_option` needs `heap` mutably, so the two
        // cannot be nested.
        let doc: Option<Option<String>> = match name.rsplit_once("::") {
            None => {
                if let Some((path, _)) = self.resolve_global(&name) {
                    Some(self.reg.docs.vars.get(&path).cloned())
                } else if let Some(path) = self.resolve_fn(&name) {
                    Some(self.reg.docs.fns.get(&path).cloned())
                } else if let Some(path) = self.resolve_bare_type(&name) {
                    Some(self.reg.docs.types.get(&path).cloned())
                } else if let Ok(path) = self.resolve_trait_name(&name) {
                    Some(self.reg.docs.traits.get(&path).cloned())
                } else if let Some((path, _)) = self.resolve_macro(&name) {
                    Some(self.reg.docs.macros.get(&path).cloned())
                } else {
                    return Err(Error::TypeError(format!(
                        "documentation: no definition named `{}`",
                        name
                    )));
                }
            }
            Some((type_part, method)) => {
                let type_segs: Vec<String> = type_part.split("::").map(|s| s.to_string()).collect();
                let type_fq = if type_segs.len() == 1 { self.resolve_bare_type(&type_segs[0]) } else { self.resolve_type_path(&type_segs) }
                    .filter(|tp| self.reg.type_def(tp).is_some_and(|def| def.assoc.contains_key(method)));
                match type_fq {
                    Some(type_fq) => {
                        Some(self.reg.docs.methods.get(&(type_fq, method.to_string())).cloned())
                    }
                    None => {
                        return Err(Error::TypeError(format!(
                            "documentation: `{}` is not a known `Type::method` — module-qualified free names/types/traits/macros are not supported here",
                            name
                        )))
                    }
                }
            }
        };
        self.doc_option(heap, doc.flatten())
    }

    /// `(documentation ...)`'s result baked in: `Option::some` of the
    /// docstring, or `Option::none` at `Option<string>`.
    fn doc_option(&self, heap: &mut Heap, doc: Option<String>) -> Result<Checked, Error> {
        match doc {
            Some(d) => {
                let text = forms::str_lit_form(heap, &d)?;
                let inner = Checked::new(text, Type::Str);
                wrap_some(heap, self, inner, Type::Str)
            }
            None => {
                let ty = Type::Named(Path::root("option"), vec![Type::Str]);
                option_none(heap, self, ty)
            }
        }
    }

    /// `(quote datum)`: `datum` as a literal `Sexpr` value, unevaluated.
    ///
    /// The datum travels as the reader's own heap value — see
    /// [`forms::quote_form`] for why the lowered node needs no owned copy of it.
    ///
    /// Reads no checker state, and keeps its receiver anyway: it is one arm of
    /// `check_special`'s dispatch table, where every sibling takes one. The
    /// constructors that were in the same position moved to [`crate::check::forms`]
    /// precisely because nothing held them to a shape; a dispatch family does.
    fn check_quote(&self, heap: &mut Heap, args: &[Value]) -> Result<Checked, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("quote: (quote datum)".into()));
        }
        let form = forms::quote_form(heap, args[0])?;
        // A quoted *symbol* is a `Symbol`, not S-expression data.
        //
        // For every other atom there is an unquoted spelling that already
        // carries the precise type — `42` is an integer, `"s"` a `string`,
        // `#\a` a `char`, `true` a `bool`. The symbol is the one value whose
        // *only* literal syntax is the quote, so typing `'foo` as data is
        // the same mistake as typing `42` as data would be: it throws away
        // what the program said. It also made equality order-dependent —
        // `(equal (string->symbol "foo") 'foo)` failed to check while the
        // reverse checked — because only one side had lost its type.
        //
        // Nothing is closed off by this: a `Symbol` widens into an
        // S-expression position by a bare retype (`Checker::check_atom`'s
        // `Type::Symbol` arm, which is a *retype* precisely because the
        // runtime value is already the datum), so `'foo` still flows into
        // `list`/`cons`/quasiquote code positions exactly as before.
        //
        // Integers are deliberately *not* given the same treatment: an
        // integer literal has no single type to be given, it adopts the
        // width its context asks for (`the_overrides_an_integer_literals_
        // default_type`). A symbol has no such ambiguity.
        if matches!(args[0], Value::Symbol(_)) {
            return Ok(Checked::new(form, Type::Symbol));
        }
        // Everything else is S-expression data: `Option<Sexpr>`, not
        // `Sexpr`, because `'()` is a datum a quote may produce and the
        // empty list is `none` now. `Sexpr` alone would exclude exactly one
        // writable literal, and there is no implicit coercion to widen it
        // back (the checker has no subtyping — see `coerce_to_dyn`, the only
        // conversion it does).
        Ok(Checked::new(form, option_of_sexpr()))
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
    /// nested `construct`s of `Sexpr`'s `cons` variant (`check_list_lit`) — so it needs no
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
    ) -> Result<Checked, Error> {
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
    ) -> Result<Checked, Error> {
        let sexpr_ty = option_of_sexpr();
        if let Value::Cons(_) = v {
            let car = heap.car(v)?;
            let cdr = heap.cdr(v)?;
            if is_symbol(car, SymId::UNQUOTE) {
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
                if is_symbol(car_car, SymId::UNQUOTE_SPLICING) {
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
                            let r = self.mk_ref(vec!["sexpr-append".to_string()], append_fq);
                            let form = self.call_form(heap, &r, &[spliced, rest])?;
                            // Rooted like every other node handed back from
                            // here (see `forms::rooted`). Without this, a
                            // `,@` splice sitting in the middle of a template
                            // was collectible the moment the *enclosing*
                            // template node allocated — which is the very
                            // next thing that happens, since this returns
                            // into a `check_qq_template` that goes on to
                            // check its own `cdr` and build a `construct`.
                            return Ok(Checked::new(forms::rooted(heap, form), sexpr_ty));
                        }
                    }
                    return Err(Error::TypeError("unquote-splicing: (unquote-splicing datum)".into()));
                }
            }
            let car_t = self.check_qq_template(heap, interp, env, car)?;
            let cdr_t = self.check_qq_template(heap, interp, env, cdr)?;
            let (adt, cons_idx) = self.sexpr_cons_ctor();
            let field_tys = self.variant_field_tys(&adt, cons_idx, &[]);
            let form =
                self.construct_form(heap, &adt, cons_idx, false, &field_tys, &[car_t.form, cdr_t.form])?;
            return Ok(Checked::new(forms::rooted(heap, form), sexpr_ty));
        }
        // A leaf of the template: literal data, so the datum travels as the
        // reader's own value — see `Self::quote_form`.
        let form = forms::quote_form(heap, v)?;
        Ok(Checked::new(forms::rooted(heap, form), sexpr_ty))
    }

    fn check_let(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
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
            let checked = self.check_at(heap, interp, env, pair[1], None, val_loc.clone());
            let val = self.recovered(heap, checked, val_loc)?;
            binds.push((name, val));
        }
        let env_binds: Vec<(String, Type, Option<Loc>)> = binds
            .iter()
            .zip(name_locs)
            .map(|((n, t), l)| (n.clone(), t.ty.clone(), l))
            .collect();
        let child = env.extended_with_locs(env_binds);
        let (body, ty) = self.check_seq(heap, interp, &child, &args[1..], &arg_locs[1..], expected)?;
        let node_binds: Vec<(String, Type, Value)> =
            binds.into_iter().map(|(n, c)| (n, c.ty, c.form)).collect();
        let form = self.let_form(heap, &node_binds, &body)?;
        Ok(Checked::new(form, ty))
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
    ) -> Result<Checked, Error> {
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
    ) -> Result<Checked, Error> {
        // Every node this returns is rooted, because the recursive step holds
        // the *inner* `let`'s form across building the outer one — and
        // `let_form` allocates a binding list, an interned name and a
        // representation before `Items::extend` pushes the body. Only a form
        // `check_at` produced is rooted already; one a `*_form` builder returned
        // is not (see [`Self::rooted`]).
        if binds.is_empty() {
            let (body, ty) = self.check_seq(heap, interp, env, body, body_locs, expected)?;
            let form = self.let_form(heap, &[], &body)?;
            return Ok(Checked::new(forms::rooted(heap, form), ty));
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
        let checked = self.check_at(heap, interp, env, pair[1], None, val_loc.clone());
        let val = self.recovered(heap, checked, val_loc)?;
        let child = env.extended_with_locs(vec![(name.clone(), val.ty.clone(), name_loc)]);
        let inner = self.let_star_rec(heap, interp, &child, &binds[1..], body, body_locs, expected)?;
        let ty = inner.ty.clone();
        let form = self.let_form(heap, &[(name, val.ty, val.form)], &[inner.form])?;
        Ok(Checked::new(forms::rooted(heap, form), ty))
    }

    /// `(setf place value)`: three kinds of `place` are recognized — a bound
    /// variable, `var::field` (`Self::check_field_set`), and a call-form
    /// place `(accessor recv key...)` (`Self::check_setf_call_place`), this
    /// project's stand-in for CL's `(setf (gethash k h) v)`/`(setf (aref a
    /// i) v)`: `recv`'s statically-known type just needs an instance method
    /// named `set-{accessor}` (the builtin containers' literal `get`/`set`
    /// is a kept-for-compatibility special case — see that function's doc
    /// comment for why no `defsetf`-style *registration* form is needed at
    /// all here). For the variable/global case, the value must match the
    /// variable's type; the expression evaluates to that value (the
    /// call-form case instead evaluates to whatever the setter itself
    /// returns — see `check_setf_call_place`'s doc comment).
    fn check_setf(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("setf: (setf place value)".into()));
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
        if let Value::Cons(_) = args[0] {
            return self.check_setf_call_place(heap, interp, env, args[0], args[1]);
        }
        let name = match args[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError(
                "setf: target must be a variable, `var::field`, or a `(get recv key...)` call form".into(),
            )),
        };
        if let Some(ty) = env.get(&name).cloned() {
            let value = self.check_at(heap, interp, env, args[1], Some(&ty), value_loc)?;
            let form = forms::set_form(heap, &name, value.form)?;
            return Ok(Checked::new(form, ty));
        }
        if let Some((path, vi)) = self.resolve_global(&name) {
            if !vi.mutable {
                return Err(Error::TypeError(format!("setf: cannot assign to constant `{}`", name)));
            }
            let value = self.check_at(heap, interp, env, args[1], Some(&vi.ty), value_loc)?;
            let r = self.mk_ref(vec![name.clone()], path);
            let form = self.set_global_form(heap, &r, &vi.ty, value.form)?;
            return Ok(Checked::new(form, vi.ty));
        }
        Err(Error::TypeError(format!("setf: unbound variable: {}", name)))
    }

    /// `(setf var::field value)`: the write counterpart of
    /// `Checker::try_field_access` — resolves `var`'s type's `set-field`
    /// setter (synthesized by `Checker::check_defstruct` alongside the
    /// getter) and delegates to `Checker::check_assoc_call` exactly like
    /// `try_field_access` does, so a generic `defstruct`'s field type gets
    /// the receiver's concrete type arguments substituted (see that
    /// function's doc comment for why building the `assoc` node by
    /// hand here once got this wrong).
    fn check_field_set(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        segs: &[String],
        value: Value,
        value_loc: Option<Loc>,
    ) -> Result<Checked, Error> {
        let [recv_name, field] = segs else {
            return Err(Error::TypeError(format!("setf: unresolved path: {}", segs.join("::"))));
        };
        let recv = if let Some(ty) = env.get(recv_name).cloned() {
            let form = forms::var_form(heap, recv_name)?;
            Checked::new(form, ty)
        } else if let Some((path, vi)) = self.resolve_global(recv_name) {
            let r = self.mk_ref(vec![recv_name.clone()], path);
            let form = self.global_form(heap, &r, &vi.ty)?;
            Checked::new(form, vi.ty)
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

    /// `(setf (accessor recv key...) value)`: this project's analogue of
    /// CL's `(setf (gethash k h) v)`/`(setf (aref a i) v)` — a call-form
    /// place. Unlike CL's `defsetf`/`define-setf-expander` (a *runtime*
    /// name-to-name registry, needed because Lisp-2 function names carry no
    /// type), no separate registration form or registry exists here: `recv`
    /// is checked first (`Self::check_at`) to learn its *static* type, then
    /// the setter is resolved the same way `Self::check_field_set` resolves
    /// a struct field's setter — by looking for an instance method named
    /// `set-{accessor}` on that type. Any user type opts in just by
    /// defining that method (e.g. `defmethod get`/`defmethod set-get` for a
    /// `foo` accessor's `set-foo`), and — since resolution is keyed by
    /// `(type, name)` rather than by name alone — two unrelated types may
    /// reuse the same accessor name with unrelated setters, something CL's
    /// single global table cannot do. The `get`/`set` (not `set-get`) pair
    /// both builtin containers (`Vector<T>`, `HashTable<K,V>`) already ship
    /// is kept as a literal fallback for backward compatibility rather than
    /// renaming their existing `set` method. The result is whatever the
    /// setter itself returns (`Unit` for the two builtin containers) rather
    /// than the newly stored value — the same `Unit`-typed shortfall
    /// `Self::check_field_set`'s synthesized setter already has, kept for
    /// consistency rather than giving the two call-form/field place kinds
    /// different result-type rules.
    fn check_setf_call_place(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        place: Value,
        value: Value,
    ) -> Result<Checked, Error> {
        let items = heap
            .list_to_vec(place)
            .map_err(|_| Error::TypeError("setf: place must be a proper list".into()))?;
        let Some((head, rest)) = items.split_first() else {
            return Err(Error::TypeError("setf: empty place".into()));
        };
        let Value::Symbol(head_id) = *head else {
            return Err(Error::TypeError("setf: place's head must be a function name".into()));
        };
        let accessor = heap.symbol_name(head_id).to_string();
        // `(setf (aref a i j) v)`: variadic bare subscripts, which no
        // `set-...` method can take. Rewritten into `Array<T>`'s own
        // `Vector<i32>`-subscript `set` by the same expansion the reading
        // form uses — see `Self::check_aref`.
        if accessor == "aref" {
            let Some((recv_form, subs)) = rest.split_first() else {
                return Err(Error::TypeError("setf: `(aref ...)` place needs an array".into()));
            };
            return self.check_aref(heap, interp, env, *recv_form, subs, Some(value), None);
        }
        let Some((recv_form, key_args)) = rest.split_first() else {
            return Err(Error::TypeError(format!("setf: `({} ...)` place needs a receiver argument", accessor)));
        };
        let recv = self.check_at(heap, interp, env, *recv_form, None, None)?;
        let Type::Named(type_fq, _) = &recv.ty else {
            return Err(Error::TypeError(format!("setf: {:?} has no settable accessor `{}`", recv.ty, accessor)));
        };
        let type_fq = type_fq.clone();
        let mut candidates = vec![format!("set-{}", accessor)];
        if accessor == "get" {
            candidates.push("set".to_string());
        }
        let setter = candidates.iter().find(|name| {
            self.reg
                .type_def(&type_fq)
                .and_then(|d| d.assoc.get(name.as_str()))
                .is_some_and(|af| af.instance && self.assoc_visible(&type_fq, af))
        });
        let Some(setter) = setter else {
            let tried = candidates.iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(" or ");
            return Err(Error::TypeError(format!("setf: `{}` has no setter for `{}` (looked for {})", type_fq, accessor, tried)));
        };
        let mut call_args: Vec<Value> = key_args.to_vec();
        call_args.push(value);
        let arg_locs = vec![None; call_args.len()];
        self.check_assoc_call(
            heap,
            interp,
            env,
            AssocCall { type_fq: &type_fq, method: setter, receiver: Some(recv), expected: None },
            &call_args,
            &arg_locs,
        )
    }

    /// Mints a synthetic name guaranteed never to collide with a
    /// user-written identifier — prefixed with `%`, a character the reader
    /// never produces inside a symbol token — for the temporaries
    /// `Self::place_dedup` binds. Monotonic per-`Checker`, so nested/
    /// repeated `incf`/`rotatef`/`shiftf` expansions within one checking run
    /// never reuse a name.
    fn gensym_place(&self) -> String {
        let n = self.place_tmp_counter.get();
        self.place_tmp_counter.set(n + 1);
        format!("%place-tmp-{}", n)
    }

    /// Splits `place` into (a) a list of `(temp-name, subform)` bindings
    /// that evaluate each of `place`'s own subexpressions exactly once, and
    /// (b) a rewritten copy of `place` that reads only those temporaries.
    /// A variable/`var::field` place has no subexpressions to duplicate (its
    /// receiver, if any, is already required to be a bare bound name — see
    /// `Checker::check_field_set`) so it comes back with an empty binding
    /// list and itself unchanged; a call-form place `(get recv key...)`
    /// binds `recv` and every `key` argument.
    ///
    /// This is what lets `Self::check_incf_decf`/`Self::check_rotatef_shiftf`
    /// reference a call-form place *twice* — once to read its current value,
    /// once to write the new one — without evaluating an impure `recv`/`key`
    /// subform (e.g. `(get (next-table!) (compute-key!))`) more than once,
    /// matching CL's `get-setf-expansion` guarantee.
    fn place_dedup(&self, heap: &mut Heap, place: Value) -> Result<(Vec<(Value, Value)>, Value), Error> {
        match place {
            Value::Symbol(_) | Value::Path(_) => Ok((Vec::new(), place)),
            Value::Cons(_) => {
                let items = heap
                    .list_to_vec(place)
                    .map_err(|_| Error::TypeError("place: must be a proper list".into()))?;
                let Some((head, rest)) = items.split_first() else {
                    return Err(Error::TypeError("place: empty call form".into()));
                };
                let mut bindings = Vec::new();
                let mut new_items = vec![*head];
                for sub in rest {
                    let tmp_sym = heap.intern_symbol(&self.gensym_place());
                    bindings.push((tmp_sym, *sub));
                    new_items.push(tmp_sym);
                }
                let pairs: Vec<(Value, Option<Loc>)> = new_items.into_iter().map(|v| (v, None)).collect();
                let new_place = forms::list_from_vec_locs(heap, &pairs)?;
                Ok((bindings, new_place))
            }
            _ => Err(Error::TypeError(
                "place: target must be a variable, `var::field`, or a `(get recv key...)` call form".into(),
            )),
        }
    }


    /// `(incf place)` / `(incf place delta)` and `(decf place)` /
    /// `(decf place delta)`: CL's increment/decrement-a-place macros.
    /// Implemented directly against `Checker::check_setf`'s place
    /// vocabulary rather than as a `prelude.rs` `defmacro` — `setf` is a
    /// protected builtin form (`Self::is_builtin_form_head`) a `defmacro`
    /// expansion could never call into otherwise. `delta` defaults to the
    /// integer literal `1`; `Self::place_dedup` guarantees a call-form
    /// place's subexpressions are evaluated exactly once even though the
    /// expansion below reads `place` and then writes it.
    fn check_incf_decf(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        op: &str,
    ) -> Result<Checked, Error> {
        let form_name = if op == "+" { "incf" } else { "decf" };
        if args.is_empty() || args.len() > 2 {
            return Err(Error::TypeError(format!("{}: ({} place) or ({} place delta)", form_name, form_name, form_name)));
        }
        let delta = args.get(1).copied().unwrap_or(Value::Int(1));
        let (bindings, place) = self.place_dedup(heap, args[0])?;
        let op_sym = heap.intern_symbol(op);
        let value_form = forms::list_from_vec_locs(heap, &[(op_sym, None), (place, None), (delta, None)])?;
        let setf_sym = heap.intern_symbol("setf");
        let setf_form = forms::list_from_vec_locs(heap, &[(setf_sym, None), (place, None), (value_form, None)])?;
        let expansion = forms::wrap_let_star(heap, bindings, setf_form)?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, None);
        heap.pop_root();
        result
    }

    /// `(rotatef place...)` / `(shiftf place... newvalue)`: CL's cyclic
    /// place-rotation macros, sharing one implementation since `shiftf` is
    /// `rotatef` with a final "place" that's an ordinary value expression
    /// instead of a place, and with the first place's *old* value returned
    /// instead of `()`. Each place is read into a fresh temporary (via
    /// `Self::place_dedup` + one more temporary per place for its current
    /// value) before any write happens, then written in rotated order —
    /// exactly CL's "all subforms left-to-right, then all the assignments"
    /// sequencing, so e.g. `(rotatef (get t 0) (get t 1))` on a two-element
    /// vector swaps them correctly even though both places share the same
    /// receiver `t`.
    fn check_rotatef_shiftf(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        is_shift: bool,
    ) -> Result<Checked, Error> {
        let (place_args, newvalue): (&[Value], Option<Value>) = if is_shift {
            let Some((last, rest)) = args.split_last() else {
                return Err(Error::TypeError("shiftf: (shiftf place... newvalue)".into()));
            };
            if rest.is_empty() {
                return Err(Error::TypeError("shiftf: (shiftf place... newvalue)".into()));
            }
            (rest, Some(*last))
        } else {
            (args, None)
        };

        let mut all_bindings: Vec<(Value, Value)> = Vec::new();
        let mut places: Vec<Value> = Vec::new();
        for p in place_args {
            let (b, np) = self.place_dedup(heap, *p)?;
            all_bindings.extend(b);
            places.push(np);
        }
        let mut value_names: Vec<Value> = Vec::new();
        for p in &places {
            let name = heap.intern_symbol(&self.gensym_place());
            all_bindings.push((name, *p));
            value_names.push(name);
        }
        let nv_name = if let Some(nv) = newvalue {
            let name = heap.intern_symbol(&self.gensym_place());
            all_bindings.push((name, nv));
            Some(name)
        } else {
            None
        };

        let n = places.len();
        let setf_sym = heap.intern_symbol("setf");
        let mut body_forms: Vec<Value> = Vec::with_capacity(n + 1);
        for i in 0..n {
            let target = if i + 1 < n {
                value_names[i + 1]
            } else if let Some(nvn) = nv_name {
                nvn
            } else {
                value_names[0]
            };
            // Rooted as it lands: these are freshly built syntax lists held
            // only in a `Vec<Value>`, and each later iteration allocates more.
            let one = forms::list_from_vec_locs(heap, &[(setf_sym, None), (places[i], None), (target, None)])?;
            body_forms.push(forms::rooted(heap, one));
        }
        body_forms.push(if is_shift { value_names.first().copied().unwrap_or(Value::Empty) } else { Value::Empty });

        let progn_sym = heap.intern_symbol("progn");
        let mut progn_items: Vec<(Value, Option<Loc>)> = vec![(progn_sym, None)];
        progn_items.extend(body_forms.into_iter().map(|f| (f, None)));
        let body = forms::list_from_vec_locs(heap, &progn_items)?;
        let expansion = forms::wrap_let_star(heap, all_bindings, body)?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, None);
        heap.pop_root();
        result
    }

    /// CL's `(aref a i j k ...)`: bare subscripts, however many the array's
    /// rank calls for. A `defmethod` cannot express that — it resolves by
    /// arity, never by a trailing run of same-typed arguments — so
    /// `Array<T>`'s own indexing takes the subscripts as one `Vector<i32>`
    /// (the shape `row-major-index`/`in-bounds` want anyway) and `aref` is
    /// rewritten into it here:
    ///
    /// ```text
    /// (aref a i j)  =>  (let* ((%a a) (%idx (the Vector<i32> (Vector::new))))
    ///                     (progn (push %idx i) (push %idx j) (get %a %idx)))
    /// ```
    ///
    /// The array is bound first so it is evaluated before the subscripts,
    /// the order it is written in — in the rewritten form it is otherwise
    /// read last, at the `get`. The subscripts appear once each and stay in
    /// place, so nothing else needs a temporary.
    ///
    /// `(setf (aref a i j) v)` is the same rewrite ending in `set` rather
    /// than `get` — see `Self::check_setf_call_place`, which reaches this
    /// through `Self::aref_expansion`'s `value` argument. Both spellings
    /// therefore go through `Array<T>`'s ordinary `get`/`set`, which is
    /// where the subscripts are range-checked.
    ///
    /// Unconditional sugar on the *name*, like two-argument `atan` above: a
    /// user-written `aref` of their own is shadowed by it. It has exactly
    /// one meaning — `Array<T>` indexing — and never inspects the receiver
    /// to choose a second one; a `Vector<T>`, `string` or `HashTable<K,V>`
    /// is indexed with `get`/`set` directly, and asking `aref` for one is an
    /// error. The receiver's type *is* read on the failure path, but only to
    /// write a better sentence about that error (see below).
    fn check_aref(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        recv: Value,
        subs: &[Value],
        value: Option<Value>,
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let expansion = self.aref_expansion(heap, recv, subs, value)?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, expected);
        heap.pop_root();
        let Err(err) = result else { return result };
        // The rewritten form is not what the user wrote, so a type error
        // inside it names types they never mentioned — `expected I32, found
        // vector<I32>` for `(aref v 0)` on a `Vector<T>`, pointing at a
        // `get` call that is not in their source. Re-check the receiver
        // alone (only here, on the way out with an error already in hand)
        // to say which type they actually indexed. Not a second meaning for
        // `aref`, just a better sentence about the same failure.
        let Ok(recv_ty) = self.check_at(heap, interp, env, recv, None, None).map(|c| c.ty) else {
            return Err(err);
        };
        if matches!(&recv_ty, Type::Named(p, _) if crate::types::path_is_builtin(p, "array")) {
            return Err(err);
        }
        Err(Error::TypeError(format!(
            "aref: `{}` is not an `Array<T>`. `aref` collects its subscripts into the \
             `Vector<i32>` an array is indexed by; a `Vector<T>`, `string` or `HashTable<K,V>` \
             takes its key directly, as `(get x k)` / `(setf (get x k) v)`.",
            mangle_type(&recv_ty)
        )))
    }

    /// The rewrite [`Self::check_aref`] documents. `value` is `None` for a
    /// read (`get`) and `Some(form)` for a write (`set`), which is the only
    /// difference between the two.
    fn aref_expansion(
        &self,
        heap: &mut Heap,
        recv: Value,
        subs: &[Value],
        value: Option<Value>,
    ) -> Result<Value, Error> {
        let a = heap.intern_symbol(&self.gensym_place());
        let idx = heap.intern_symbol(&self.gensym_place());
        let empty = self.the_form(heap, "Vector", &Type::I32, "Vector::new")?;
        let empty = forms::rooted(heap, empty);
        let push = heap.intern_symbol("push");
        let progn = heap.intern_symbol("progn");
        let getter = heap.intern_symbol("get");
        let setter = heap.intern_symbol("set");
        // Rooted as each lands: these are freshly built syntax lists held
        // only in a `Vec<Value>`, and every later iteration allocates more.
        let mut body: Vec<(Value, Option<Loc>)> = vec![(progn, None)];
        for s in subs {
            let one = forms::list_from_vec_locs(heap, &[(push, None), (idx, None), (*s, None)])?;
            body.push((forms::rooted(heap, one), None));
        }
        let tail = match value {
            None => forms::list_from_vec_locs(heap, &[(getter, None), (a, None), (idx, None)])?,
            Some(v) => forms::list_from_vec_locs(heap, &[(setter, None), (a, None), (idx, None), (v, None)])?,
        };
        body.push((forms::rooted(heap, tail), None));
        let body = forms::list_from_vec_locs(heap, &body)?;
        let body = forms::rooted(heap, body);
        forms::wrap_let_star(heap, vec![(a, recv), (idx, empty)], body)
    }

    /// CL's variadic arithmetic operators (`+ - * / max min`): `(op a b c
    /// ...)` with 3+ operands is sugar for the left fold `(op (op (op a b)
    /// c) ...)`. Each operand appears exactly once in the rewritten form, so
    /// unlike `Self::check_variadic_cmp` no let-bound temporaries are needed
    /// to guard against double-evaluating an impure operand. Called only for
    /// `args.len() > 2`; the plain 2-argument case goes through the existing
    /// `Self::try_instance_method` builtin-method path unchanged.
    fn check_variadic_arith(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        op: &str,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let op_sym = heap.intern_symbol(op);
        let mut acc = args[0];
        for next in &args[1..] {
            acc = forms::list_from_vec_locs(heap, &[(op_sym, None), (acc, None), (*next, None)])?;
        }
        heap.push_root(acc);
        let result = self.check(heap, interp, env, acc, expected);
        heap.pop_root();
        result
    }

    /// CL's variadic comparison operators (`< <= > >= = /=`): `(op a b c
    /// ...)` with 3+ operands means every adjacent pair compares true, e.g.
    /// `(< a b c)` is `(and (< a b) (< b c))` — *not* a left fold, since `<`
    /// doesn't return a value of the compared type to feed into the next
    /// call. Because each operand would otherwise appear in two adjacent
    /// comparisons, every operand is first bound to a fresh `let*` temporary
    /// (`Self::gensym_place`) so an impure operand (e.g. `(< (f) (g) (h))`)
    /// is still evaluated exactly once, left to right — matching CL's
    /// evaluation-order guarantee for this case. Called only for
    /// `args.len() > 2`.
    fn check_variadic_cmp(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        op: &str,
        args: &[Value],
    ) -> Result<Checked, Error> {
        let mut bindings: Vec<(Value, Value)> = Vec::with_capacity(args.len());
        let mut names: Vec<Value> = Vec::with_capacity(args.len());
        for a in args {
            let name = heap.intern_symbol(&self.gensym_place());
            bindings.push((name, *a));
            names.push(name);
        }
        let op_sym = heap.intern_symbol(op);
        let mut cmp_forms: Vec<(Value, Option<Loc>)> = Vec::with_capacity(names.len() - 1);
        for w in names.windows(2) {
            cmp_forms.push((forms::list_from_vec_locs(heap, &[(op_sym, None), (w[0], None), (w[1], None)])?, None));
        }
        let and_sym = heap.intern_symbol("and");
        let mut and_items: Vec<(Value, Option<Loc>)> = vec![(and_sym, None)];
        and_items.extend(cmp_forms);
        let and_form = forms::list_from_vec_locs(heap, &and_items)?;
        let expansion = forms::wrap_let_star(heap, bindings, and_form)?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, None);
        heap.pop_root();
        result
    }


    /// `(- x)` (negation) / `(/ x)` (reciprocal): CL's unary forms of the
    /// otherwise-binary `-`/`/`. Neither can be spelled as `(- 0 x)`/`(/ 1
    /// x)` directly — a bare `0`/`1` literal only ever resolves to `i32` (or
    /// whatever `expected` says), never `bignum`/`ratio`, so that would
    /// break for those two types exactly the way `docs/dev/cl-missing-
    /// classes-and-methods.md`'s `abs`/`signum` note already documents for
    /// binary-op receiver position. Binding `x` to a temporary and writing
    /// `(- (- %t %t) %t)` / `(/ (/ %t %t) %t)` sidesteps the whole issue: `(-
    /// %t %t)`/`(/ %t %t)` is `x`'s own `0`/`1` *of its own type*, with no
    /// literal involved at all, so this works uniformly across all five
    /// numeric types with no type inspection here. `%t` is bound via
    /// `Self::gensym_place` so `x` (possibly impure) is evaluated exactly
    /// once despite appearing three times in the expansion.
    fn check_unary_negate_or_invert(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        op: &str,
        x: Value,
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let tmp = heap.intern_symbol(&self.gensym_place());
        let op_sym = heap.intern_symbol(op);
        let self_op_self = forms::list_from_vec_locs(heap, &[(op_sym, None), (tmp, None), (tmp, None)])?;
        let outer = forms::list_from_vec_locs(heap, &[(op_sym, None), (self_op_self, None), (tmp, None)])?;
        let expansion = forms::wrap_let_star(heap, vec![(tmp, x)], outer)?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, expected);
        heap.pop_root();
        result
    }

    /// `(op)` / `(op x)` for CL's normally-binary arithmetic/bitwise/
    /// comparison operators — the 0- and 1-argument edge cases
    /// `Self::check_variadic_arith`/`check_variadic_cmp` deliberately don't
    /// cover (those two only ever see `args.len() > 2`). Dispatched from
    /// `Self::check_list` for `args.len() <= 1`:
    /// - `+`/`*`/`max`/`min`/`logand`/`logior`/`logxor` with one argument is
    ///   CL's identity case — just that argument, unchanged.
    /// - `+`/`logior`/`logxor`/`*`/`logand` with zero arguments is CL's
    ///   defined identity element (`Self::numeric_identity_literal`); `-`,
    ///   `/`, `max`, `min` and every comparison require at least one
    ///   argument in CL and get a clear arity error here instead of falling
    ///   through to a confusing "unbound variable"/instance-method-not-found
    ///   message.
    /// - `-`/`/` with one argument is negation/reciprocal
    ///   (`Self::check_unary_negate_or_invert`).
    /// - A comparison with one argument is trivially `true` in CL (nothing
    ///   to compare against) but must still evaluate that argument once, for
    ///   its side effects and so it gets type-checked — `(progn x true)`.
    fn check_nullary_or_unary_numeric_op(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        op: &str,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let Some(&x) = args.first() else {
            let value = match op {
                // CL's identities: `(gcd)` is 0 (every integer divides 0) and
                // `(lcm)` is 1, the same way `(+)` is 0 and `(*)` is 1.
                "+" | "logior" | "logxor" | "gcd" => 0,
                "*" | "lcm" => 1,
                "logand" => -1,
                _ => return Err(Error::TypeError(format!("{}: requires at least 1 argument", op))),
            };
            let lit = forms::numeric_identity_literal(heap, expected, value);
            return self.check(heap, interp, env, lit, expected);
        };
        match op {
            "+" | "*" | "max" | "min" | "logand" | "logior" | "logxor" => self.check(heap, interp, env, x, expected),
            "-" | "/" => self.check_unary_negate_or_invert(heap, interp, env, op, x, expected),
            // CL: one-argument `gcd`/`lcm` are the absolute value, not the
            // argument itself — `(gcd -4)` is 4. `abs` is a prelude method on
            // every type that has a binary `gcd`, so this expands rather than
            // passing through the way `+`/`max` do.
            "gcd" | "lcm" => {
                let abs_sym = heap.intern_symbol("abs");
                let expansion = forms::list_from_vec_locs(heap, &[(abs_sym, None), (x, None)])?;
                heap.push_root(expansion);
                let result = self.check(heap, interp, env, expansion, expected);
                heap.pop_root();
                result
            }
            "<" | "<=" | ">" | ">=" | "=" | "/=" => {
                let progn_sym = heap.intern_symbol("progn");
                let expansion = forms::list_from_vec_locs(heap, &[(progn_sym, None), (x, None), (Value::Bool(true), None)])?;
                heap.push_root(expansion);
                let result = self.check(heap, interp, env, expansion, Some(&Type::Bool));
                heap.pop_root();
                result
            }
            _ => unreachable!(),
        }
    }

    /// Check `(written args...)` as a call to the prelude function `name` —
    /// for a CL spelling whose implementation is an ordinary `defun` under a
    /// different name (`(atan y x)` -> `atan2`).
    fn check_renamed_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        name: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let fq = self
            .resolve_fn(name)
            .ok_or_else(|| Error::TypeError(format!("{}: the prelude's `{}` is not loaded", name, name)))?;
        self.check_call(heap, interp, env, &[name.to_string()], &fq, args, arg_locs)
    }

    /// CL's 2-argument `(log number base)`: the change-of-base identity
    /// `(/ (log number) (log base))`. The 1-argument natural-log `log` is
    /// the plain `f64` builtin (`registry::float_assoc`); `defmethod` can't
    /// overload the same name by arity (only by receiver type — see
    /// `docs/dev/cl-missing-classes-and-methods.md`'s `floor`/`floor-div`
    /// note for the same constraint), so the 2-argument form is sugar
    /// dispatched by arity here in the checker instead, exactly like
    /// `Self::check_variadic_arith`. Each operand appears exactly once in
    /// the rewritten form, so no let-bound temporaries are needed.
    fn check_log_with_base(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let log_sym = heap.intern_symbol("log");
        let log_number = forms::list_from_vec_locs(heap, &[(log_sym, None), (args[0], None)])?;
        let log_base = forms::list_from_vec_locs(heap, &[(log_sym, None), (args[1], None)])?;
        let div_sym = heap.intern_symbol("/");
        let expansion = forms::list_from_vec_locs(heap, &[(div_sym, None), (log_number, None), (log_base, None)])?;
        heap.push_root(expansion);
        let result = self.check(heap, interp, env, expansion, expected);
        heap.pop_root();
        result
    }

    /// A same-named 2-argument instance method may be defined with the
    /// receiver in *either* position — e.g. this project's own `Vector<T>`
    /// method `push` takes the vector first (`(push vec item)`, the
    /// "receiver first" convention every instance-method call in this
    /// language otherwise uses — see `Self::try_instance_method`'s doc
    /// comment), while `push` is also CL's place macro, item first
    /// (`(push item place)`). Tried only after the receiver-first order has
    /// already failed to resolve, so it never shadows the primary
    /// convention when both would apply.
    fn try_instance_method_swapped(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        method: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Option<Result<Checked, Error>> {
        if args.len() != 2 {
            return None;
        }
        let swapped_args = [args[1], args[0]];
        let swapped_locs = [nth_loc(arg_locs, 1), nth_loc(arg_locs, 0)];
        self.try_instance_method(heap, interp, env, method, &swapped_args, &swapped_locs)
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
    ) -> Result<TopLevelForm, Error> {
        if parts.len() != 2 && parts.len() != 3 {
            return Err(Error::TypeError(
                "defvar/defconstant: (defvar (name Type) value) or (defvar (name Type) value \"doc\")".into(),
            ));
        }
        // A trailing docstring — CL's `defvar`/`defparameter`/`defconstant`
        // order (name, initial-value, documentation-string), not the
        // leading position `defun`/`defmacro` use for theirs, since there's
        // no body here to put it in front of.
        let doc = match parts.get(2) {
            None => None,
            Some(Value::Str(id)) => Some(heap.string(*id).to_string()),
            Some(_) => return Err(Error::TypeError("defvar/defconstant: third argument must be a docstring".into())),
        };
        let (name, ann) = match parts[0] {
            Value::Cons(_) => {
                let pair_locs = heap.list_to_vec_locs(parts[0])?;
                let pair: Vec<Value> = pair_locs.iter().map(|(v, _)| *v).collect();
                if pair.len() != 2 {
                    return Err(Error::TypeError("defvar: name must be (name Type)".into()));
                }
                let name = match pair[0] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => return Err(Error::TypeError("defvar: name must be a symbol".into())),
                };
                (name, self.parse_type_here_at(heap, pair[1], pair_locs[1].1.as_ref())?)
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
        if let Some(doc) = doc {
            self.reg.docs.vars.insert(fq_name.clone(), doc);
        }
        self.defvar_form(heap, &fq_name, &ty, mutable, public, value.form)
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
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        // CL's own rule: a `loop` whose first form is a keyword is the
        // extended DSL, anything else is the simple loop this always was.
        if loop_dsl::is_dsl(heap, args) {
            return self.check_loop_dsl(heap, interp, env, args, arg_locs, expected);
        }
        let (body, ty) = self.check_loop_body(heap, interp, env, args, arg_locs, Type::Never)?;
        let form = forms::loop_form(heap, &body)?;
        Ok(Checked::new(form, ty))
    }

    /// CL's extended `loop` (cl-parity-plan.md Stage 4b). Two passes, and
    /// the reason is the plan's one wrong assumption: it expected
    /// `(let ((acc (Vector::new))) … (push acc e) … acc)` to infer its own
    /// element type, and it does not — `Vector::new`'s type argument comes
    /// from the *expected* type, forwards, so a later `push` cannot reach
    /// back for it. So the accumulators' types are worked out here, before
    /// any source is written, and written into the tree
    /// ([`super::loop_dsl`]'s module comment).
    ///
    /// Pass 1 checks one representative expression per `:with`/`:for` clause
    /// to learn what that variable is, extending an `Env` as it goes, then
    /// checks each accumulated expression in it. Those checks are thrown
    /// away — only the types are kept. Checking a body twice is not new
    /// here: a generic `defun`'s body is checked once for diagnostics and
    /// again per specialization.
    fn check_loop_dsl(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let plan = loop_dsl::parse(heap, args, arg_locs)?;

        // Pass 1a: the loop variables, each learned in the environment its
        // predecessors built (`:with j = (* i 2)` after `:for i …` is legal
        // in CL, and `let*` gives it the same meaning here).
        let mut binds: Vec<(String, Type, Option<Loc>)> = Vec::new();
        for var in &plan.vars {
            let probe = loop_dsl::var_probe(heap, var)?;
            let probe = forms::rooted(heap, probe);
            let scoped = env.extended_with_locs(binds.clone());
            let ty = self.check_at(heap, interp, &scoped, probe, None, var.loc.clone())?.ty;
            binds.push((var.name.clone(), ty, var.loc.clone()));
        }
        let scoped = env.extended_with_locs(binds);

        // Pass 1b: each accumulator's own type, and the `Option` a
        // `:thereis` leaves behind when nothing matched.
        let mut acc_inits: Vec<(String, Value)> = Vec::new();
        for acc in &plan.accs {
            let init = self.loop_acc_init(heap, interp, &scoped, acc)?;
            acc_inits.push((acc.name.clone(), init));
        }
        let thereis_none = match loop_dsl::thereis_expr(&plan) {
            Some(expr) => {
                let ty = self.check_at(heap, interp, &scoped, expr, None, None)?.ty;
                let Type::Named(p, targs) = &ty else {
                    return Err(loop_thereis_error(&ty));
                };
                if p.last_segment() != "option" || targs.len() != 1 {
                    return Err(loop_thereis_error(&ty));
                }
                let elem = targs[0].clone();
                Some(self.the_form(heap, "Option", &elem, "Option::none")?)
            }
            None => None,
        };

        let resolved = loop_dsl::Resolved { acc_inits, thereis_none };
        let built = loop_dsl::build(heap, &plan, &resolved)?;
        let built = forms::rooted(heap, built);
        self.check_at(heap, interp, env, built, expected, arg_locs.first().cloned().flatten())
    }

    /// The initializer for one accumulator, at the type its accumulated
    /// expression turned out to have.
    fn loop_acc_init(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        acc: &loop_dsl::AccVar,
    ) -> Result<Value, Error> {
        use loop_dsl::Acc;
        // `:count` counts passes, not elements, so its accumulator's type
        // does not depend on what was written.
        if acc.acc == Acc::Count {
            return Ok(Value::Int(0));
        }
        let probe = loop_dsl::acc_probe(heap, acc)?;
        let probe = forms::rooted(heap, probe);
        let ty = self.check_at(heap, interp, env, probe, None, None)?.ty;
        match acc.acc {
            Acc::Collect | Acc::Append => self.the_form(heap, "Vector", &ty, "Vector::new"),
            Acc::Sum => Ok(forms::numeric_identity_literal(heap, Some(&ty), 0)),
            Acc::Maximize | Acc::Minimize => self.the_form(heap, "Option", &ty, "Option::none"),
            Acc::Count => unreachable!("handled above"),
        }
    }

    /// `(the OUTER<ELEM> (CTOR))` with the type written out — the checker
    /// putting a type it worked out into the tree it is building, since
    /// nothing downstream can rederive it.
    ///
    /// The written form is [`mangle_type`]'s, which is the surface syntax,
    /// and the round trip through [`Self::parse_type_here_at`] is *checked*
    /// rather than assumed: not every type has surface syntax — a function
    /// type's parentheses cannot survive inside `<>`
    /// (`read::reader::extend_angle_token`) — and refusing to write one says
    /// so here instead of emitting a form that fails somewhere less legible.
    fn the_form(&self, heap: &mut Heap, outer: &str, elem: &Type, ctor: &str) -> Result<Value, Error> {
        let text = format!("{}<{}>", outer, mangle_type(elem));
        let ty_form = heap.intern_symbol(&text);
        let ok = match self.parse_type_here_at(heap, ty_form, None) {
            Ok(Type::Named(_, args)) => args.len() == 1 && args[0] == *elem,
            _ => false,
        };
        if !ok {
            return Err(Error::TypeError(format!(
                "loop: `{}` has no written form, so this accumulator's type cannot be stated",
                text
            )));
        }
        let the = heap.intern_symbol("the");
        // A `::` name is a `Value::Path`, not a symbol — the reader splits it
        // at read time, so a symbol spelled `Vector::new` would resolve to
        // nothing.
        let ctor_path = Path::from_segments(ctor.split("::").map(str::to_string).collect());
        let ctor_sym = forms::path_form(heap, &ctor_path);
        let call = forms::list_from_vec_locs(heap, &[(ctor_sym, None)])?;
        let call = forms::rooted(heap, call);
        forms::list_from_vec_locs(heap, &[(the, None), (ty_form, None), (call, None)])
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
    ) -> Result<(Vec<Value>, Type), Error> {
        self.loop_stack.borrow_mut().push(seed);
        let result = self.check_seq(heap, interp, env, body, body_locs, None);
        let ty = self.loop_stack.borrow_mut().pop().expect("pushed above");
        let (body, _) = result?;
        Ok((body, ty))
    }

    /// The symbol named by a `catch`/`throw`'s first argument, which must be
    /// a literal `'sym` — never a computed form.
    ///
    /// CL evaluates the tag, and compares tags with `eq` at run time. Here it
    /// has to be literal, because the symbol is what carries the thrown
    /// value's *type* ([`Self::throw_tags`]): a computed tag would leave the
    /// checker with nothing to check `throw`'s value against. That is the one
    /// place this departs from CL's `catch`/`throw`, and it costs little —
    /// computed tags are vanishingly rare, and every use of one would have
    /// been unstatable in a typed language anyway.
    fn throw_tag(&self, heap: &Heap, form: Value, who: &str) -> Result<(String, Value), Error> {
        let quoted = heap
            .list_to_vec(form)
            .ok()
            .filter(|elems| elems.len() == 2)
            .filter(|elems| matches!(elems[0], Value::Symbol(id) if id == SymId::QUOTE))
            .map(|elems| elems[1]);
        match quoted {
            Some(sym @ Value::Symbol(id)) => Ok((heap.symbol_name(id).to_string(), sym)),
            _ => Err(Error::TypeError(format!("{}: the tag must be a literal symbol, as in ({} 'done ...)", who, who))),
        }
    }

    /// Records that `tag` carries `ty`, or checks it against what an earlier
    /// `catch`/`throw` on the same symbol already established.
    fn unify_throw_tag(&self, tag: &str, ty: &Type, who: &str) -> Result<(), Error> {
        let mut tags = self.throw_tags.borrow_mut();
        match tags.get(tag) {
            // `Never` never pins a tag down: it is what a `body` consisting
            // only of a `throw` has, and adopting it would make the tag's type
            // depend on which use the checker happened to reach first.
            Some(_) if matches!(ty, Type::Never) => Ok(()),
            Some(known) if known == ty => Ok(()),
            Some(known) => Err(Error::TypeError(format!(
                "{}: `{}` carries {:?}, but this use carries {:?}",
                who, tag, known, ty
            ))),
            None => {
                if !matches!(ty, Type::Never) {
                    tags.insert(tag.to_string(), ty.clone());
                }
                Ok(())
            }
        }
    }

    /// `(catch 'tag body)`: run `body`, and produce the value of a
    /// `(throw 'tag v)` fired anywhere it reaches instead, if one fires.
    ///
    /// The form's type is `body`'s — which is also the type `tag` is recorded
    /// as carrying, since a thrown value takes the place of `body`'s result.
    fn check_catch(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("catch: (catch 'tag body)".into()));
        }
        let (tag, sym) = self.throw_tag(heap, args[0], "catch")?;
        let body = self.check_at(heap, interp, env, args[1], expected, nth_loc(arg_locs, 1))?;
        self.unify_throw_tag(&tag, &body.ty, "catch")?;
        // Joined with the tag's own type, not simply `body`'s: a body that
        // only throws has type `Never`, yet the form still produces whatever
        // the throw delivered. `join_types` leaves the ordinary case alone,
        // since `Never` joins away and an agreeing tag joins to the same type.
        let ty = match self.throw_tags.borrow().get(&tag) {
            Some(carried) => join_types(&body.ty, carried)?,
            None => body.ty.clone(),
        };
        // The *tag's* type, not the form's: those differ exactly when the body
        // falls off its end normally, and what crosses the boundary is what the
        // throw carried.
        let carried = self.throw_tags.borrow().get(&tag).cloned().unwrap_or_else(|| ty.clone());
        let tag_form = forms::quote_form(heap, sym)?;
        let repr = self.repr_form(heap, &carried)?;
        let form = forms::catch_form(heap, tag_form, body.form, repr)?;
        Ok(Checked::new(form, ty))
    }

    /// `(throw 'tag value)`: leave for the nearest dynamically enclosing
    /// `(catch 'tag ...)`, delivering `value` as its result. Type `Never`
    /// (diverges; satisfies any expectation), since a `throw` never produces
    /// a value where it stands.
    fn check_throw(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("throw: (throw 'tag value)".into()));
        }
        let (tag, sym) = self.throw_tag(heap, args[0], "throw")?;
        // Checked against the tag's known type when there is one, so the
        // mismatch is reported at the value rather than after the fact.
        let expected = self.throw_tags.borrow().get(&tag).cloned();
        let value = self.check_at(heap, interp, env, args[1], expected.as_ref(), nth_loc(arg_locs, 1))?;
        self.unify_throw_tag(&tag, &value.ty, "throw")?;
        let tag_form = forms::quote_form(heap, sym)?;
        let repr = self.repr_form(heap, &value.ty)?;
        let form = forms::throw_form(heap, tag_form, value.form, repr)?;
        Ok(Checked::new(form, Type::Never))
    }

    /// `(unwind-protect protected cleanup)`: run `protected`, then `cleanup`
    /// — whether `protected` finished normally or left by a non-local exit
    /// (`throw`, `break`, `return`, or a `panic`).
    ///
    /// The form's type is `protected`'s; `cleanup` runs for its effect and its
    /// value is discarded, exactly as in CL.
    fn check_unwind_protect(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("unwind-protect: (unwind-protect protected cleanup)".into()));
        }
        let protected = self.check_at(heap, interp, env, args[0], expected, nth_loc(arg_locs, 0))?;
        let cleanup = self.check_at(heap, interp, env, args[1], None, nth_loc(arg_locs, 1))?;
        let ty = protected.ty.clone();
        let form = forms::unwind_protect_form(heap, protected.form, cleanup.form)?;
        Ok(Checked::new(form, ty))
    }

    /// `(break)`: exit the nearest enclosing loop with no value (`Unit`). Type
    /// `Never` (diverges; satisfies any expectation).
    fn check_break(&self, heap: &mut Heap, args: &[Value]) -> Result<Checked, Error> {
        if !args.is_empty() {
            return Err(Error::TypeError("break: (break), takes no arguments".into()));
        }
        self.contribute_loop_exit(Type::Unit)?;
        let form = forms::break_form(heap)?;
        Ok(Checked::new(form, Type::Never))
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
    ) -> Result<Checked, Error> {
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
        let form = forms::return_form(heap, value.map(|c| c.form))?;
        Ok(Checked::new(form, Type::Never))
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
    ) -> Result<Checked, Error> {
        // A list of S-expressions is an S-expression, so both the elements
        // and the result are `Option<Sexpr>` — the empty list this starts
        // from is exactly `none`.
        let sexpr_ty = option_of_sexpr();
        let (adt, cons_idx) = self.sexpr_cons_ctor();
        let cons_tys = self.variant_field_tys(&adt, cons_idx, &[]);
        // The empty list, built straight from `Sexpr`'s reserved variant 0:
        // `resolve_ctor("nil")` cannot answer any more (`nil` is gone from
        // the surface — see `SEXPR_RESERVED_VARIANT`), and the checker is
        // exactly the caller that is still allowed to name it.
        let mut acc = self.construct_form(heap, &adt, SEXPR_RESERVED_VARIANT, false, &[], &[])?;
        for (i, &elem) in args.iter().enumerate().rev() {
            // The accumulator is a finished node that the next element's own
            // checking can collect, so it stays rooted across that step.
            let mut s = RootScope::new(heap);
            s.push_root(acc);
            let e = self.check_at(&mut s, interp, env, elem, Some(&sexpr_ty), nth_loc(arg_locs, i))?;
            acc = self.construct_form(&mut s, &adt, cons_idx, false, &cons_tys, &[e.form, acc])?;
        }
        Ok(Checked::new(acc, sexpr_ty))
    }

    /// Collects a heterogeneous run of already-checked expressions into one
    /// `Sexpr` list — the `&rest`-free counterpart of [`Self::cons_rest_list`]
    /// used by the `format`/`print`/`println` special forms, whose trailing
    /// arguments each keep their *own* natural type rather than sharing one
    /// declared element type. Every element is wrapped into its `Sexpr`
    /// encoding by [`Self::wrap_rest_elem`] against its own inferred type (so a
    /// value whose type has no `Sexpr` encoding is the same clear `TypeError`
    /// a bad `&rest` element gets), then `sexpr-cons`-ed together back-to-front.
    fn cons_hetero_sexpr(&self, heap: &mut Heap, items: Vec<Checked>) -> Result<Checked, Error> {
        let r = Ref::synthetic(Path::root("sexpr-cons"));
        let nil = forms::quote_nil(heap)?;
        let mut acc = Checked::new(nil, option_of_sexpr());
        for item in items.into_iter().rev() {
            let mut s = RootScope::new(heap);
            s.push_root(acc.form);
            let elem_ty = item.ty.clone();
            let item = self.wrap_rest_elem(&mut s, &elem_ty, item)?;
            s.push_root(item.form);
            let form = self.call_form(&mut s, &r, &[item, acc])?;
            acc = Checked::new(form, option_of_sexpr());
        }
        Ok(Checked::new(forms::rooted(heap, acc.form), acc.ty))
    }

    /// The `(format dest control &rest args)` special form — CL's `format`
    /// adapted to typelisp's `bool` (`dest` is `true` for CL's `t`, writing to
    /// stdout, or `false` for CL's `nil`, only building the string). The
    /// control string's directives are interpreted at runtime by
    /// `Interp::run_format`; here we only type the fixed arguments (`dest:
    /// bool`, `control: string`) and collect the variadic tail into one
    /// `Sexpr` list, then lower to the internal `format-rt` builtin. With a
    /// `bool` destination it always returns the built `string` (statically) —
    /// it is additionally written to stdout when `dest` is `true`, but a
    /// single return type keeps the static system simple, and the string is
    /// useful to a `false` caller.
    ///
    /// A destination of any *other* type is CL's stream destination, handled
    /// by [`Self::check_format_to_stream`].
    fn check_format(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() < 2 {
            return Err(Error::TypeError(
                "format: expected at least a destination (bool or a `CharOutput` stream) and a control string"
                    .to_string(),
            ));
        }
        // The destination decides which of the two lowerings applies, so it is
        // typed with nothing expected of it first. `true`/`false` (and any
        // other `bool`) keep the string-building lowering below; anything else
        // is a stream.
        let dest = self.check_at(heap, interp, env, args[0], None, nth_loc(arg_locs, 0))?;
        if dest.ty != Type::Bool {
            return self.check_format_to_stream(heap, interp, env, &dest.ty, args, arg_locs);
        }
        let control = self.check_at(heap, interp, env, args[1], Some(&Type::Str), nth_loc(arg_locs, 1))?;
        let mut items = Vec::new();
        for (i, &a) in args[2..].iter().enumerate() {
            items.push(self.check_at(heap, interp, env, a, None, nth_loc(arg_locs, 2 + i))?);
        }
        let list = self.cons_hetero_sexpr(heap, items)?;
        let r = Ref::synthetic(Path::root("format-rt"));
        let form = self.call_form(heap, &r, &[dest, control, list])?;
        Ok(Checked::new(form, Type::Str))
    }

    /// `(format stream control args…)` — CL's stream destination.
    ///
    /// Lowered by rewriting the whole form to
    /// `(write-string stream (format false control args…))` and checking
    /// *that*, rather than by building the call here: `write-string` is a
    /// `CharOutput` method, and how it resolves depends on the receiver
    /// (a concrete stream type, a `:dyn CharOutput`, or a bound type
    /// variable inside a `(where (CharOutput S))` function). Handing the
    /// rewritten form back to [`Self::check`] is what lets all three work
    /// without this special form knowing anything about trait dispatch.
    ///
    /// The value is `write-string`'s, i.e. `()` — unlike the `bool`-destination
    /// form, which yields the built string. CL agrees (`(format stream …)`
    /// returns `nil`), and the alternative would mean naming a temporary to
    /// avoid formatting twice.
    ///
    /// The destination is checked twice as a result — once above to learn its
    /// type, once again inside the rewritten form. Checking is free of runtime
    /// effect, and the rewrite is what keeps this to a dozen lines.
    fn check_format_to_stream(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        dest_ty: &Type,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        // A destination that is neither `bool` nor a stream is a mistake worth
        // naming as one, rather than reporting as a missing `write-string`
        // method. Only asked of a type that is definitely concrete: a bound
        // type variable is spelled `Named` too (with no registry entry), and
        // whether *it* implements `CharOutput` is settled by its `where`
        // clause where the call resolves, not here.
        let concrete =
            prim_type_path(dest_ty).is_some() || matches!(dest_ty, Type::Named(p, _) if self.reg.type_def(p).is_some());
        if concrete
            // Lowercase: symbol names are interned case-folded, so that is how
            // the prelude's `CharOutput` is spelled in the registry.
            && !self.type_implements(dest_ty, &TraitBound { trait_path: Path::root("charoutput"), assoc: BTreeMap::new() }, 0)
        {
            return Err(Error::TypeError(format!(
                "format: the destination must be `true` (stdout), `false` (build the string only), \
                 or a stream implementing `CharOutput` — found `{:?}`",
                dest_ty
            )));
        }
        // `(format false control args…)`, as source: the same special form,
        // one destination over.
        let mut inner: Vec<(Value, Option<Loc>)> =
            vec![(heap.intern_symbol("format"), None), (Value::Bool(false), None)];
        for (i, &a) in args[1..].iter().enumerate() {
            inner.push((a, nth_loc(arg_locs, 1 + i)));
        }
        let inner = forms::list_from_vec_locs(heap, &inner)?;
        heap.push_root(inner);
        let write_string = heap.intern_symbol("write-string");
        let form =
            forms::list_from_vec_locs(heap, &[(write_string, None), (args[0], nth_loc(arg_locs, 0)), (inner, None)]);
        let result = form.and_then(|form| {
            heap.push_root(form);
            let r = self.check(heap, interp, env, form, None);
            heap.pop_root();
            r
        });
        heap.pop_root();
        result
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
    ) -> Result<Checked, Error> {
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
        let list = self.cons_hetero_sexpr(heap, items)?;
        let r = Ref::synthetic(Path::root(builtin));
        let form = self.call_form(heap, &r, &[control, list])?;
        Ok(Checked::new(form, ret))
    }

    /// The `pprint` family — `(pprint x)`, `(pprint-fill x)`,
    /// `(pprint-linear x)`, `(pprint-tabular x [colinc])` — CLHS 22.2.1's
    /// four ready-made layouts, writing to stdout.
    ///
    /// These are special forms rather than `defun`s for the same reason
    /// `print` is: the argument keeps its own natural type and is wrapped into
    /// its `Sexpr` encoding here ([`Self::wrap_rest_elem`]), so `(pprint 42)`
    /// and `(pprint my-struct)` both work without an implicit-coercion rule in
    /// the type system. Unlike `~A`/`~S`, they pretty-print whatever
    /// `*print-pretty*` says, matching CL — `pprint` is defined as printing
    /// "as if `*print-pretty*` were true".
    fn check_pprint(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        form: &str,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let takes_colinc = form == "pprint-tabular";
        let max = if takes_colinc { 2 } else { 1 };
        if args.is_empty() || args.len() > max {
            return Err(Error::TypeError(if takes_colinc {
                "pprint-tabular: expected an object and an optional column width".to_string()
            } else {
                format!("{}: expected exactly one object to print", form)
            }));
        }
        let value = self.check_at(heap, interp, env, args[0], None, nth_loc(arg_locs, 0))?;
        let elem_ty = value.ty.clone();
        let value = self.wrap_rest_elem(heap, &elem_ty, value)?;
        // The column width is only meaningful to `pprint-tabular`; the other
        // three take the default (1) so one builtin serves all four.
        let colinc = match args.get(1) {
            Some(a) => self.check_at(heap, interp, env, *a, Some(&Type::I64), nth_loc(arg_locs, 1))?,
            None => Checked::new(core::tagged(heap, "int", &[Value::Int(0)])?, Type::I64),
        };
        let which = Checked::new(forms::str_lit_form(heap, form)?, Type::Str);
        let r = Ref::synthetic(Path::root("pprint-rt"));
        let node = self.call_form(heap, &r, &[which, value, colinc])?;
        Ok(Checked::new(node, Type::Unit))
    }

    /// `(pprint-logical-block (obj :prefix "(" :suffix ")") body…)` — CLHS
    /// `pprint-logical-block`, minus the stream argument typelisp has no
    /// streams for.
    ///
    /// `obj` is the list `pprint-pop` walks (write `()` for a block that
    /// iterates nothing). The options are CL's, as keywords: `:prefix`,
    /// `:suffix`, `:per-line-prefix` (mutually exclusive with `:prefix`, as in
    /// CL). Everything printed inside the body — by `print`, `(format true …)`
    /// or `pprint` — becomes the block's content, and the `pprint-newline` /
    /// `pprint-indent` / `pprint-tab` builtins place its line breaks; see
    /// `Interp`'s `PrettySession` for how that implicit "current pretty
    /// stream" works.
    ///
    /// A special form rather than a macro because the options are keyword
    /// arguments over a nested spec list, and because the block must open
    /// before and close after an arbitrary body sequence.
    fn check_pprint_logical_block(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.is_empty() {
            return Err(Error::TypeError(
                "pprint-logical-block: (pprint-logical-block (obj :prefix p :suffix s) body...)".into(),
            ));
        }
        let spec = heap.list_to_vec(args[0])?;
        let Some(&obj_form) = spec.first() else {
            return Err(Error::TypeError(
                "pprint-logical-block: the spec needs at least the object to iterate (write `()` for none)".into(),
            ));
        };
        // The affix literals below are built here rather than read from
        // source, so nothing else roots them — this scope does, for as long as
        // the body checking that can collect them is still running.
        let mut s = RootScope::new(heap);
        let obj = self.check_at(&mut s, interp, env, obj_form, Some(&option_of_sexpr()), nth_loc(arg_locs, 0))?;
        let empty = forms::str_lit_form(&mut s, "")?;
        s.push_root(empty);
        let mut prefix = Checked::new(empty, Type::Str);
        let mut suffix = Checked::new(empty, Type::Str);
        let mut per_line = false;
        let mut opts = spec[1..].iter();
        while let Some(&key) = opts.next() {
            let name = match key {
                Value::Symbol(id) => s.symbol_name(id).to_string(),
                _ => return Err(Error::TypeError("pprint-logical-block: expected a keyword option".into())),
            };
            let Some(&value) = opts.next() else {
                return Err(Error::TypeError(format!("pprint-logical-block: {} needs a value", name)));
            };
            let value = self.check_at(&mut s, interp, env, value, Some(&Type::Str), None)?;
            match name.as_str() {
                ":prefix" => prefix = value,
                ":per-line-prefix" => {
                    prefix = value;
                    per_line = true;
                }
                ":suffix" => suffix = value,
                other => {
                    return Err(Error::TypeError(format!(
                        "pprint-logical-block: unknown option {} (expected :prefix, :per-line-prefix or :suffix)",
                        other
                    )))
                }
            }
        }
        // `(progn (open …) body… (close))`, represented the way `progn` is:
        // a `let` with no bindings. The block's own value is `unit` — the body
        // is run for its printing effect.
        let flag = Checked::new(core::tagged(&mut s, "bool", &[Value::Bool(per_line)])?, Type::Bool);
        s.push_root(flag.form);
        let start_r = Ref::synthetic(Path::root("pprint-block-start-rt"));
        let start = self.call_form(&mut s, &start_r, &[obj, prefix, flag, suffix])?;
        s.push_root(start);
        let mut seq = vec![start];
        for (i, &form) in args[1..].iter().enumerate() {
            let e = self.check_at(&mut s, interp, env, form, None, nth_loc(arg_locs, 1 + i))?;
            seq.push(e.form);
        }
        let end_r = Ref::synthetic(Path::root("pprint-block-end-rt"));
        let end = self.call_form(&mut s, &end_r, &[])?;
        s.push_root(end);
        seq.push(end);
        let form = self.let_form(&mut s, &[], &seq)?;
        Ok(Checked::new(form, Type::Unit))
    }

    // Same invariant checking context as `check_path_call` — see its comment.
    #[allow(clippy::too_many_arguments)]
    /// Associated-type pins participate in *inference*, not just
    /// verification, for every call path that has a `where` clause —
    /// [`Self::check_call`] and [`Self::check_call_opt_key`] both run this
    /// before their "cannot infer type parameter" check, so a parameter that
    /// appears in no argument but is pinned by a bound counts as resolved.
    ///
    /// Failures are deliberately silent: `Self::validate_where_bounds` runs
    /// afterwards and reports a pin mismatch with a precise message, so a
    /// `unify` that cannot make two pins agree here should leave the
    /// diagnosis to it rather than raising a vaguer error first.
    fn infer_pinned_assoc_types(
        &self,
        env: &Env,
        sig: &FnSig,
        params: &HashSet<String>,
        subst: &mut BTreeMap<String, Type>,
    ) {
        for (tparam, trait_bounds) in &sig.bounds {
            let Some(concrete) = subst.get(tparam).cloned() else { continue };
            // `concrete` may itself be a **bare** unresolved type variable —
            // the enclosing generic function's own type parameter, forwarded
            // straight through (this call is nested inside another
            // `where`-bounded generic's own diagnostic body-check, mirroring
            // `Self::validate_where_bounds`'s own bare-variable case).
            // There is no registered type to look an associated-type impl up
            // on yet, but the *enclosing* function's own `where` clause may
            // already pin the same associated type on the same trait —
            // propagate that pin into `subst` instead of leaving the callee's
            // parameter uninferred (e.g. `elt` calling `nth n it` with `it:
            // I` forwards `elt`'s own `(Iter I (Item A))` pin so `nth`'s own
            // `A` resolves to `elt`'s own, still-open `A`; both become
            // concrete together once the enclosing function is specialized).
            if let Type::Named(n, args) = &concrete {
                if args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() {
                    let var_name = n.last_segment();
                    if let Some(caller_tbs) = env.bounds.get(var_name) {
                        for tb in trait_bounds {
                            let Some(caller_tb) =
                                caller_tbs.iter().find(|c| c.trait_path == tb.trait_path)
                            else {
                                continue;
                            };
                            for (assoc_name, declared_ty) in &tb.assoc {
                                if let Some(caller_ty) = caller_tb.assoc.get(assoc_name) {
                                    let _ = unify(&params, declared_ty, caller_ty, subst);
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
                        let _ = unify(&params, declared_ty, &actual, subst);
                    }
                }
            }
        }
    }

    fn check_call(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        written: &[String],
        name: &Path,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let sig = self.reg.fn_sig(name).expect("caller checked presence").clone();
        if !sig.optionals.is_empty() || !sig.keys.is_empty() {
            return self.check_call_opt_key(heap, interp, env, written, name, &sig, args, arg_locs);
        }
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
        let mut subst: BTreeMap<String, Type> = BTreeMap::new();
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
            typed.push(self.cons_rest_list(heap, &resolved, rest_typed)?);
        }
        self.infer_pinned_assoc_types(env, &sig, &params, &mut subst);
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
        // left to the unreachable-`panic` node, same as
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
        let ty = subst_apply(&sig.ret, &subst);
        let form = self.call_form(heap, &r, &typed)?;
        Ok(Checked::new(form, ty))
    }

    /// [`Self::check_call`]'s `&optional`/`&key` path — routed to whenever
    /// `sig.optionals`/`sig.keys` is non-empty; at most one of the two is
    /// non-empty (never both — `Self::parse_defun_params_full`). Builds the
    /// same flat, fully-saturated `call` an ordinary
    /// fixed-arity call produces — one actual argument per runtime
    /// parameter, in `TopLevel::Defun::params` order — so nothing
    /// downstream (`Interp::apply`, the compile pipeline's `core_bridge`/
    /// self-hosted `compile-call`) needs to know `&optional`/`&key` exist at
    /// all; an omitted argument is filled in right here with either the
    /// parameter's checked default expression or (no default) an
    /// `Option::none` at its type — see `check::registry::OptKeyParam::
    /// effective_ty`.
    ///
    /// `sig.type_params`/`sig.bounds` may be non-empty (`Self::
    /// check_defun_opt_key` now allows generics), in which case this runs
    /// the same two-pass `unify`/`subst_apply` inference `Self::check_call`
    /// does, restricted to what's actually observable: required arguments
    /// (always) plus whichever `&optional`/`&key` arguments the call site
    /// actually supplies (an omitted one contributes nothing — there is no
    /// value to unify against). A type parameter that only ever appears in
    /// an omitted argument is therefore uninferrable and rejected outright
    /// (no attempt at partial/lazy inference) — see the module's
    /// `check_defun_opt_key` doc comment for why a defaulted parameter's
    /// declared type can never depend on one of these anyway; only a
    /// defaultless (`Option<T>`-effective) parameter can, and omitting it
    /// simply produces `Option::none` at the call's own resolved `T` (built
    /// fresh here via `subst_apply`, never reusing a stale node).
    #[allow(clippy::too_many_arguments)]
    fn check_call_opt_key(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        written: &[String],
        name: &Path,
        sig: &FnSig,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        let required_n = sig.params.len();
        if args.len() < required_n {
            return Err(Error::TypeError(format!(
                "{}: expected at least {} argument(s), got {}",
                name,
                required_n,
                args.len()
            )));
        }
        let params: HashSet<String> = sig.type_params.iter().cloned().collect();
        let mut subst: BTreeMap<String, Type> = BTreeMap::new();

        let mut typed = Vec::with_capacity(required_n + sig.optionals.len().max(sig.keys.len()));
        for (i, (arg, pty)) in args[..required_n].iter().zip(sig.params.iter()).enumerate() {
            let st = subst_apply(pty, &subst);
            let exp = if type_has_param(&st, &params) { None } else { Some(st) };
            let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(arg_locs, i))?;
            unify(&params, pty, &ta.ty, &mut subst)?;
            typed.push(ta);
        }
        let tail = &args[required_n..];
        let tail_locs = arg_locs.get(required_n..).unwrap_or(&[]);

        // `supplied_keys`/`supplied_opts` hold pass 1's already-checked
        // values (unify already applied) — pass 2 below only needs to fill
        // in whatever they're missing, using the now-final `subst`.
        let mut supplied_keys: HashMap<String, Checked> = HashMap::new();
        let mut opt_supplied_n = 0usize;
        let mut opt_supplied: Vec<Checked> = Vec::new();
        let mut rest_typed: Vec<Checked> = Vec::new();

        if !sig.keys.is_empty() {
            // `&key`: every trailing argument is a `:name value` pair,
            // matched by label — `Self::check_defun_opt_key`/
            // `Self::parse_defun_params_full` guarantee `sig.rest` is `None`
            // and `sig.optionals` is empty whenever `sig.keys` isn't.
            if tail.len() % 2 != 0 {
                return Err(Error::TypeError(format!(
                    "{}: keyword arguments must be given as `:name value` pairs",
                    name
                )));
            }
            let mut i = 0;
            while i < tail.len() {
                let kw_name = match tail[i] {
                    Value::Symbol(id) => heap.symbol_name(id).to_string(),
                    _ => {
                        return Err(Error::TypeError(format!(
                            "{}: expected a `:name` keyword, got a non-symbol",
                            name
                        )))
                    }
                };
                let Some(kw_name) = kw_name.strip_prefix(':').map(|s| s.to_string()) else {
                    return Err(Error::TypeError(format!(
                        "{}: expected a `:name` keyword, got `{}`",
                        name, kw_name
                    )));
                };
                let Some(key) = sig.keys.iter().find(|k| k.name == kw_name) else {
                    return Err(Error::TypeError(format!("{}: unknown keyword argument :{}", name, kw_name)));
                };
                if supplied_keys.contains_key(&kw_name) {
                    return Err(Error::TypeError(format!("{}: duplicate keyword argument :{}", name, kw_name)));
                }
                let val_loc = nth_loc(tail_locs, i + 1);
                let st = subst_apply(&key.decl_ty, &subst);
                let exp = if type_has_param(&st, &params) { None } else { Some(st) };
                let checked = self.check_at(heap, interp, env, tail[i + 1], exp.as_ref(), val_loc)?;
                unify(&params, &key.decl_ty, &checked.ty, &mut subst)?;
                supplied_keys.insert(kw_name, checked);
                i += 2;
            }
        } else {
            // `&optional` (+ possibly `&rest`): filled strictly by position,
            // left to right — CL's own rule, and the reason `&key` can't
            // combine with it (see `Self::parse_defun_params_full`'s doc
            // comment).
            opt_supplied_n = tail.len().min(sig.optionals.len());
            for (i, opt) in sig.optionals.iter().enumerate().take(opt_supplied_n) {
                let st = subst_apply(&opt.decl_ty, &subst);
                let exp = if type_has_param(&st, &params) { None } else { Some(st) };
                let checked = self.check_at(heap, interp, env, tail[i], exp.as_ref(), nth_loc(tail_locs, i))?;
                unify(&params, &opt.decl_ty, &checked.ty, &mut subst)?;
                opt_supplied.push(checked);
            }
            let after_optionals = &tail[opt_supplied_n..];
            let after_optionals_locs = tail_locs.get(opt_supplied_n..).unwrap_or(&[]);
            match &sig.rest {
                Some(elem_ty) => {
                    for (i, arg) in after_optionals.iter().enumerate() {
                        let st = subst_apply(elem_ty, &subst);
                        let exp = if type_has_param(&st, &params) { None } else { Some(st) };
                        let ta = self.check_at(heap, interp, env, *arg, exp.as_ref(), nth_loc(after_optionals_locs, i))?;
                        unify(&params, elem_ty, &ta.ty, &mut subst)?;
                        rest_typed.push(ta);
                    }
                }
                None if !after_optionals.is_empty() => {
                    return Err(Error::TypeError(format!(
                        "{}: expected at most {} argument(s), got {}",
                        name,
                        required_n + sig.optionals.len(),
                        args.len()
                    )));
                }
                None => {}
            }
        }

        self.infer_pinned_assoc_types(env, sig, &params, &mut subst);
        for p in &sig.type_params {
            if !subst.contains_key(p) {
                return Err(Error::TypeError(format!(
                    "cannot infer type parameter `{}` for `{}`",
                    p, name
                )));
            }
        }
        self.validate_where_bounds(&name.to_string(), &sig.bounds, &subst, &env.bounds)?;

        // Monomorphization — same as `Self::check_call`'s identical block.
        let mut call_path = name.clone();
        let mut specialized = false;
        if !sig.type_params.is_empty() && self.generic_fn_templates.contains_key(name) {
            let targs: Vec<Type> = sig.type_params.iter().map(|p| subst[p.as_str()].clone()).collect();
            if !targs.iter().any(|t| self.type_is_open(t)) {
                call_path = self.request_fn_specialization(name, targs);
                specialized = true;
            }
        }

        // Pass 2: fill in whatever pass 1 left out, now that `subst` is
        // final — a default expression's `.ty` is already concrete (`Self::
        // check_defun_opt_key`'s restriction), so it's spliced in verbatim;
        // an omitted defaultless parameter's `Option::none` is built fresh
        // at `subst_apply(effective_ty)`, never reusing any earlier node.
        if !sig.keys.is_empty() {
            let mut supplied = supplied_keys;
            for key in &sig.keys {
                let val = match supplied.remove(&key.name) {
                    Some(checked) => {
                        if key.default.is_some() {
                            checked
                        } else {
                            let target = checked.ty.clone();
                            wrap_some(heap, self, checked, target)?
                        }
                    }
                    None => match &key.default {
                        Some(d) => forms::splice_default(heap, d, &key.decl_ty)?,
                        None => option_none(heap, self, subst_apply(&key.effective_ty(), &subst))?,
                    },
                };
                typed.push(val);
            }
        } else {
            let mut opt_supplied = opt_supplied.into_iter();
            for (i, opt) in sig.optionals.iter().enumerate() {
                let val = if i < opt_supplied_n {
                    let checked = opt_supplied.next().expect("one checked value per supplied optional");
                    if opt.default.is_some() {
                        checked
                    } else {
                        let target = checked.ty.clone();
                        wrap_some(heap, self, checked, target)?
                    }
                } else {
                    match &opt.default {
                        Some(d) => forms::splice_default(heap, d, &opt.decl_ty)?,
                        None => option_none(heap, self, subst_apply(&opt.effective_ty(), &subst))?,
                    }
                };
                typed.push(val);
            }
            if let Some(elem_ty) = &sig.rest {
                let resolved = subst_apply(elem_ty, &subst);
                typed.push(self.cons_rest_list(heap, &resolved, rest_typed)?);
            }
        }

        let r = if specialized {
            Ref::synthetic(call_path)
        } else {
            self.mk_ref(written.to_vec(), call_path)
        };
        let ty = subst_apply(&sig.ret, &subst);
        let form = self.call_form(heap, &r, &typed)?;
        Ok(Checked::new(form, ty))
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
    ///   an unreachable-`panic` failure two calls later.
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
        bounds: &BTreeMap<String, Vec<TraitBound>>,
        subst: &BTreeMap<String, Type>,
        caller_bounds: &BTreeMap<String, Vec<TraitBound>>,
    ) -> Result<(), Error> {
        for (tparam, trait_bounds) in bounds {
            let Some(concrete) = subst.get(tparam) else { continue };
            if let Type::Named(n, args) = concrete {
                if args.is_empty() && n.is_simple() && self.reg.type_def(n).is_none() {
                    let var_name = n.last_segment();
                    let declared = caller_bounds.get(var_name);
                    for tb in trait_bounds {
                        // A caller bound discharges this one when it names
                        // the same trait *or a subtrait of it*: `(where (Ord
                        // T))` already obliges `T` to implement `Eq`, so it
                        // satisfies a callee's `(where (Eq T))`. The pins are
                        // composed down the chain for the same reason
                        // `check_dyn_call` composes them.
                        let satisfied = declared.is_some_and(|caller_tbs| {
                            caller_tbs.iter().any(|caller_tb| {
                                if !self.trait_inherits(&caller_tb.trait_path, &tb.trait_path) {
                                    return false;
                                }
                                let Some(cdef) = self.reg.trait_def(&caller_tb.trait_path) else {
                                    return false;
                                };
                                let Some(at) = self.trait_assoc_subst_for(
                                    cdef,
                                    &caller_tb.assoc,
                                    &tb.trait_path,
                                ) else {
                                    return false;
                                };
                                // `tb` is the *callee's declared* bound, so
                                // its pins are written in the callee's own
                                // type parameters (`find-if`'s `(Item A)`).
                                // `at` holds the caller's, in the caller's.
                                // Comparing them raw only agreed when the two
                                // functions happened to spell the variable
                                // with the same letter — which is why the
                                // prelude's `elt` could delegate to `nth`
                                // (both say `A`) while the identical shape
                                // spelled `B` could not, and why no pin that
                                // is a *structured* type (`(Item
                                // cons-cell<K,V>)`) ever matched. Resolving
                                // the callee's side through the call's own
                                // `subst` first puts both in the caller's
                                // vocabulary. A parameter `subst` never bound
                                // is left as-is by `subst_apply`, which is
                                // exactly the previous behavior.
                                tb.assoc
                                    .iter()
                                    .all(|(k, v)| at.get(k) == Some(&subst_apply(v, subst)))
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
                // `type_implements` rather than a bare `def.impls` scan, so a
                // blanket impl covering `concrete` counts (and gets queued for
                // materialization).
                if !self.type_implements(concrete, tb, 0) {
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
    ) -> Result<Checked, Error> {
        let (adt_name, variant) = ctor;
        if is_retired_nil(adt_name, variant) {
            return Err(Error::TypeError(NIL_IS_GONE.to_string()));
        }
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
        let mut subst: BTreeMap<String, Type> = BTreeMap::new();

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
                // Nothing here determines `p`: no field mentions it and no
                // expected type supplied it. `(result::ok v)` is the standard
                // case — `Ok` carries a `T` and says nothing about `E`.
                //
                // Under a `match`-arm probe ([`Self::infer_probe`]) that is
                // not yet an error: leave an inference hole for the arm's
                // siblings to fill, and let `check_match` re-check this arm
                // once they have. Anywhere else there is no second source of
                // information, so it is exactly as fatal as it always was.
                None if self.infer_probe.get() => {
                    self.probe_holes.set(self.probe_holes.get() + 1);
                    result_args.push(Type::Never);
                }
                None => {
                    return Err(Error::TypeError(format!(
                        "cannot infer type argument `{}` for `{}`",
                        p, adt_name
                    )))
                }
            }
        }
        let mutable = def.kind == AdtKind::Struct;
        let arg_forms: Vec<Value> = typed_args.iter().map(|a| a.form).collect();
        // The declared field types with *this* site's arguments substituted in.
        // Recomputed after the loop above rather than reusing its own `st`: a
        // later field's `unify` can be what determines an earlier one's type
        // parameter, so only the finished `subst` is complete.
        let field_tys: Vec<Type> = fields.iter().map(|f| subst_apply(f, &subst)).collect();
        // `Option<Sexpr>` is niche-represented (`check/repr.rs`), so neither
        // of its constructors builds a box: `some v` *is* `v`, and `none` is
        // the empty-list immediate. The pattern side reads the same encoding
        // (`Pattern::Empty`/`Pattern::NonEmpty`), and the two must agree —
        // a boxed `none` would sail past `pat-empty`'s word comparison and
        // be taken for a `some`.
        //
        // Here is where the instantiation is known, the same reason
        // `field_tys` is computed here rather than at the definition.
        if crate::types::path_is_builtin(adt_name, "option")
            && matches!(result_args.first(), Some(Type::Named(a, _)) if *a == Path::root("sexpr"))
        {
            let form = match variant {
                // `(some v)` lowers to `v`'s own already-checked form.
                OPTION_SOME => arg_forms[0],
                // `(none)` is the empty list, which the core IR still spells
                // as `sexpr`'s variant 0 — see the null-elimination plan's
                // "変種番号は詰めない".
                _ => self.construct_form(heap, &Path::root("sexpr"), 0, false, &[], &[])?,
            };
            return Ok(Checked::new(forms::rooted(heap, form), Type::Named(adt_name.clone(), result_args)));
        }
        let form = self.construct_form(heap, adt_name, variant, mutable, &field_tys, &arg_forms)?;
        Ok(Checked::new(forms::rooted(heap, form), Type::Named(adt_name.clone(), result_args)))
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
    ) -> Result<Checked, Error> {
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
            let form = forms::dyn_value_form(heap, scrut.form)?;
            scrut = Checked::new(forms::rooted(heap, form), sexpr_ty());
        }
        // A scrutinee that is *not* a data type is matched by value alone:
        // its arms can only be literals, `(= expr)` guards, and a catch-all,
        // so there is no variant coverage to compute and the catch-all is
        // mandatory (checked below, where an ADT's own exhaustiveness is).
        // Before, `expect_adt`'s error stopped this dead — which meant a
        // `string`/`symbol`/`i32` scrutinee could not be matched at all, and
        // a string literal had no pattern position it was legal in.
        let adt = self.expect_adt(&scrut.ty).ok();
        let adt_name = adt.as_ref().map(|(n, _)| n.clone());
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
        // `Sexpr`'s reserved variant 0 is not writable, so it cannot be
        // covered and must not be demanded — see `SEXPR_RESERVED_VARIANT`.
        // A `match` whose scrutinee is `Option<Sexpr>` writes the `Sexpr`
        // shapes and `none` in one flat arm list (the niche's match sugar,
        // `check_ctor_pattern`), so its coverage universe is neither
        // `option`'s two variants nor `sexpr`'s ten but their union.
        //
        // The two index without colliding because `Sexpr`'s variant 0 is the
        // slot the empty list vacated (`SEXPR_RESERVED_VARIANT`) and `none`
        // is precisely what moved out of it: `none` takes 0, the ten writable
        // shapes keep 1..=10, and the count is `sexpr`'s own `variants.len()`
        // with nothing subtracted.
        let sexpr_sugar = is_option_of_sexpr(&scrut.ty);
        let sexpr_variants =
            || self.reg.type_def(&Path::root("sexpr")).expect("sexpr is always registered").variants.len();
        let total_variants = if sexpr_sugar {
            sexpr_variants()
        } else {
            adt_name.as_ref().map_or(0, |n| {
                let all = self.reg.type_def(n).expect("adt exists").variants.len();
                if crate::types::path_is_builtin(n, "sexpr") { all - 1 } else { all }
            })
        };

        /// An arm held back by the probe (B4) for the second pass below: its
        /// slot in `arms`, everything checking its body again needs, and the
        /// pattern that check has already accepted.
        struct Deferred {
            slot: usize,
            pat: Pattern,
            binds: PatternBindings,
            body: Vec<Value>,
            body_locs: Vec<Option<Loc>>,
            arm_loc: Option<Loc>,
        }

        // `Option` because a deferred arm's slot is filled by the second pass,
        // and arm order is match semantics — first match wins.
        let mut arms: Vec<Option<(Pattern, Vec<Value>)>> = Vec::new();
        let mut covered: HashSet<usize> = HashSet::new();
        let mut catchall = false;
        let mut result_ty: Option<Type> = expected.cloned();
        // What the arms *between them* know about the result type, holes and
        // all — the expectation the second pass hands the deferred arms.
        // `result_ty` stays the type of the arms actually retained.
        let mut probe_ty: Option<Type> = expected.cloned();
        let mut deferred: Vec<Deferred> = Vec::new();
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
            let (pat, binds) = recover_arm!(self.check_pattern(
                heap,
                interp,
                env,
                &scrut.ty,
                parts[0],
                parts_locs[0].1.clone()
            ));
            match &pat {
                // ---- the `Option<Sexpr>` sugar's flat coverage (see
                // `sexpr_sugar` above). Placed ahead of the general arms
                // because under the sugar both of them would answer, and
                // answer with the wrong index space.
                Pattern::Ctor { type_name, variant, .. }
                    if sexpr_sugar && crate::types::path_is_builtin(type_name, "sexpr") =>
                {
                    covered.insert(*variant);
                }
                Pattern::Empty if sexpr_sugar => {
                    covered.insert(SEXPR_RESERVED_VARIANT);
                }
                // `(some x)` under the sugar means "any non-empty shape",
                // which is all ten of them at once.
                Pattern::NonEmpty(sub)
                    if sexpr_sugar && matches!(**sub, Pattern::Wildcard | Pattern::Bind(..)) =>
                {
                    covered.extend(1..sexpr_variants());
                }
                // A Sexpr-downcast `Ctor` pattern's `type_name` names the
                // *downcast target* (a user struct/enum), not the
                // scrutinee's own ADT — it must not count toward this
                // match's exhaustiveness over `adt_name`'s variants (design
                // plan's "網羅性" rule). Restricting to a same-ADT `type_name`
                // covers both the ordinary case (always same-ADT) and this
                // one in a single guard.
                Pattern::Ctor { type_name, variant, .. } if Some(type_name) == adt_name.as_ref() => {
                    covered.insert(*variant);
                }
                // `Option<Sexpr>`'s two niche shapes, which
                // `check_ctor_pattern_fields` produced in place of an
                // `option` ctor pattern. They cover `option`'s own variants,
                // so exhaustiveness counts them as such — otherwise a
                // complete `((some x) ..) ((none) ..)` would be rejected.
                Pattern::Empty if adt_name.as_ref().map(|p| crate::types::path_is_builtin(p, "option")) == Some(true) => {
                    covered.insert(OPTION_NONE);
                }
                // `(some P)` covers `some` only when `P` itself matches
                // anything; `(some (int n))` leaves other `Sexpr` shapes
                // uncovered, exactly as a nested ctor pattern would.
                Pattern::NonEmpty(sub)
                    if matches!(**sub, Pattern::Wildcard | Pattern::Bind(..))
                        && adt_name.as_ref().map(|p| crate::types::path_is_builtin(p, "option")) == Some(true) =>
                {
                    covered.insert(OPTION_SOME);
                }
                Pattern::Wildcard | Pattern::Bind(..) => catchall = true,
                _ => {}
            }
            let arm_env = env.extended_with_locs(binds.clone());
            // Diverging arms don't constrain the result type; concrete arms must
            // all agree (Never joins with anything).
            let arm_expected = result_ty.as_ref().and_then(non_never);
            let body_locs: Vec<Option<Loc>> = parts_locs[1..].iter().map(|(_, l)| l.clone()).collect();
            // Probe boundary (B4): with no result type agreed yet there is no
            // expectation to hand this body, and a body that needs one —
            // `(result::ok v)`, whose `E` only the sibling `(err e)` arm
            // names — cannot type itself. So check it with inference holes
            // allowed ([`Self::infer_probe`]), and if it made one (or failed
            // outright, which a sibling's type may equally well fix), keep
            // only what it revealed about the result type and hold the arm
            // back for the second pass. Nothing the probe checked is
            // retained: its diagnostics are rolled back and its body is
            // discarded, so the arm is checked for real exactly once.
            let probing = arm_expected.is_none();
            let mark = self.check_mark();
            let holes = self.probe_holes.get();
            let outer_probe = self.infer_probe.replace(probing);
            let checked = self.check_seq(heap, interp, &arm_env, &parts[1..], &body_locs, arm_expected);
            self.infer_probe.set(outer_probe);
            let settled = checked.is_ok()
                && self.probe_holes.get() == holes
                && self.errors.borrow().len() == mark.errors;
            if probing && !settled {
                self.rollback_to(mark);
                self.probe_holes.set(holes);
                if let Ok((_, ty)) = &checked {
                    probe_ty = Some(match probe_ty {
                        None => ty.clone(),
                        Some(p) => merge_holes(&p, ty).unwrap_or(p),
                    });
                }
                deferred.push(Deferred {
                    slot: arms.len(),
                    pat,
                    binds,
                    body: parts[1..].to_vec(),
                    body_locs,
                    arm_loc,
                });
                arms.push(None);
                continue;
            }
            let (body, body_ty) = recover_arm!(checked);
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
            if let Some(r) = &result_ty {
                probe_ty = Some(match probe_ty {
                    None => r.clone(),
                    Some(p) => merge_holes(&p, r).unwrap_or(p),
                });
            }
            arms.push(Some((pat, body)));
        }

        // Second pass, for the arms the probe held back (B4). By now the arms
        // have pooled what each of them knew, and if nothing is missing from
        // the pool it is the expectation the deferred body was short of. With
        // nothing to offer, the body is re-checked exactly as it was checked
        // the first time — unprobed, with no expectation — so what the user
        // sees is the arm's own error, the same one this `match` reported
        // before there was a second pass at all.
        let pooled = probe_ty
            .as_ref()
            .and_then(non_never)
            .filter(|t| !has_open_hole(t))
            .cloned();
        for d in deferred {
            let arm_env = env.extended_with_locs(d.binds);
            let checked =
                self.check_seq(heap, interp, &arm_env, &d.body, &d.body_locs, pooled.as_ref());
            let (body, body_ty) = match checked {
                Ok(v) => v,
                Err(e) if self.recover => {
                    arm_recovered = true;
                    self.push_recovered(e, d.arm_loc.clone());
                    continue;
                }
                Err(e) => return Err(e),
            };
            result_ty = Some(match result_ty {
                None => body_ty,
                Some(r) => match join_types(&r, &body_ty) {
                    Ok(joined) => joined,
                    Err(e) if self.recover => {
                        self.push_recovered(e, d.arm_loc.clone());
                        r
                    }
                    Err(e) => return Err(e),
                },
            });
            arms[d.slot] = Some((d.pat, body));
        }

        // Exhaustiveness (B1): in `recover` mode a non-exhaustive `match`
        // records the error but still returns a fully-typed `Match` node (every
        // arm was already checked) — this is exactly what lets completion work
        // inside a non-catchall arm, where truncating the source deletes the
        // arms that would have made the match exhaustive. Suppressed when an
        // arm was skipped (B2), since coverage is then unknown.
        // A type with no variants is never covered by its arms: there is
        // nothing to enumerate, so only a catch-all closes the match.
        let covers_every_variant = adt_name.is_some() && covered.len() == total_variants;
        if !catchall && !arm_recovered && !covers_every_variant {
            let e = Error::TypeError(match &adt_name {
                Some(adt_name) => format!(
                    "non-exhaustive match on `{}`: {}/{} variants covered",
                    adt_name,
                    covered.len(),
                    total_variants
                ),
                // No variants to count: every arm was a value test, and a
                // value test can only ever be a partial answer.
                None => format!(
                    "non-exhaustive match on {:?}: a type with no variants can only be matched \
                     by value, so a `_` arm is required",
                    scrut.ty
                ),
            });
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
        // Patterns are lowered here, at the end: `pattern_form` roots each
        // node it returns, so the ones built earlier survive the allocations
        // the later ones make.
        let mut lowered: Vec<(Value, Vec<Value>)> = Vec::new();
        for (pat, body) in arms.into_iter().flatten() {
            let pat_form = self.pattern_form(heap, &pat)?;
            lowered.push((pat_form, body));
        }
        let form = self.match_form(heap, scrut.form, &scrut.ty, &lowered)?;
        Ok(Checked::new(form, ty))
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
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        expected: &Type,
        v: Value,
        loc: Option<Loc>,
    ) -> Result<(Pattern, PatternBindings), Error> {
        match v {
            Value::Symbol(id) => {
                let name = heap.symbol_name(id).to_string();
                if name == "_" {
                    return Ok((Pattern::Wildcard, Vec::new()));
                }
                // A bare name that *is* a constructor of the scrutinee's own
                // type is that constructor, not a new variable — `(match c
                // (red 1) (blue 2))`. Without this the arm silently binds
                // `c` to a variable called `red`, matches everything, and
                // counts as a catch-all for the exhaustiveness check, so
                // `blue`'s arm becomes unreachable and nothing says so.
                // A variant that *has* fields still has to be written with
                // its fields: `check_ctor_pattern_fields`' arity check is
                // what reports that, and it reads better than anything this
                // site could say ("expected 1 field(s), got 0").
                if let Some(pat) = self.check_bare_ctor_pattern(heap, interp, env, expected, v, loc.as_ref())? {
                    return Ok(pat);
                }
                Ok((Pattern::Bind(name.clone()), vec![(name, expected.clone(), loc)]))
            }
            Value::Int(n) => {
                if expected.is_integer() {
                    Ok((Pattern::Int(n), Vec::new()))
                } else {
                    // Not an integer type. Handed to `value_pattern`
                    // anyway, not refused here, so that the message comes
                    // from where every other mismatched literal's does: the
                    // `(equals $match-scrut 1)` it builds reports "expected
                    // F64, found I32" against an `f64` scrutinee, naming
                    // both types, where this site could only say "integer
                    // pattern does not match type F64". Nothing is widened
                    // by the detour — an integer literal is no more an
                    // `f64`/`bignum` in a pattern than it is anywhere else.
                    self.value_pattern(heap, interp, env, expected, v, loc)
                }
            }
            // Same shape as the integer arm above: the immediate pattern
            // when the scrutinee is exactly this type, the general value
            // test otherwise — which is what lets `#\a`/`true` be matched
            // against a `Sexpr` scrutinee, where the literal is implicitly
            // an `Sexpr` and `Eq`'s `eq` compares the two immediates.
            Value::Bool(b) => {
                if *expected == Type::Bool {
                    Ok((Pattern::Bool(b), Vec::new()))
                } else {
                    self.value_pattern(heap, interp, env, expected, v, loc)
                }
            }
            Value::Char(c) => {
                if *expected == Type::Char {
                    Ok((Pattern::Char(c), Vec::new()))
                } else {
                    self.value_pattern(heap, interp, env, expected, v, loc)
                }
            }
            // The literals with no immediate word: a `string` is a pointer,
            // an `f64`/`bignum`/`ratio` is a box id. Comparing those words
            // is identity, not equality, so each is matched by a *test* —
            // see `Pattern::Guard`.
            //
            // Against a `Sexpr` scrutinee that test would be `Eq`'s `eq`
            // (`prelude.rs`'s `impl Eq sexpr`, deliberately identity), and
            // identity on one of *these* is the identity of a `Str`/box —
            // an arm that type-checks and never matches. Refused with the
            // variant pattern named instead, which destructures to the
            // scalar's own type and so compares it by value. The immediate
            // literals (`'foo`, an integer, a `char`, a `bool`) have no
            // such gap and are left alone; so is an explicit `(= expr)`,
            // where the author asked for `equals` in as many words.
            Value::Str(_) | Value::Boxed(_) if is_sexpr_expectation(expected) => {
                Err(Error::TypeError(format!(
                    "pattern: `{}` against a `Sexpr` scrutinee would compare by identity — \
                     `Eq` on `sexpr` is `eq`, so this arm could never match a separately built \
                     value. Write `({} {})` to destructure it and compare by value instead",
                    core::print(heap, v),
                    sexpr_variant_for_literal(heap, v),
                    core::print(heap, v)
                )))
            }
            Value::Str(_) | Value::Boxed(_) => self.value_pattern(heap, interp, env, expected, v, loc),
            Value::Cons(_) => self.check_ctor_pattern(heap, interp, env, expected, v),
            other => Err(Error::TypeError(format!("pattern does not match {:?}: {:?}", expected, other))),
        }
    }

    /// A bare symbol pattern that names a constructor of `expected`, or
    /// `None` when it names no constructor and is therefore an ordinary
    /// binding.
    ///
    /// Two resolutions, the same two `check_ctor_pattern` performs for a
    /// parenthesized head: the scrutinee type's own variants, and — for a
    /// `Sexpr` scrutinee — a downcast to a visible user ADT. Neither can
    /// reach a *generic* ADT's field-destructuring form, so neither can be
    /// silently wrong about type arguments.
    fn check_bare_ctor_pattern(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        expected: &Type,
        v: Value,
        loc: Option<&Loc>,
    ) -> Result<Option<(Pattern, PatternBindings)>, Error> {
        let Value::Symbol(id) = v else { return Ok(None) };
        let name = heap.symbol_name(id).to_string();
        // A one-element `(name)` pattern is exactly what the parenthesized
        // spelling of a no-field variant already is, so both resolutions
        // below are handed the same shape they always see.
        let parts = [v];
        let parts_locs = [(v, loc.cloned())];
        if is_sexpr_expectation(expected) {
            if let Some((adt_name, targs, variant)) = self.resolve_sexpr_downcast_ctor(heap, v, loc)? {
                return self
                    .check_ctor_pattern_fields(heap, interp, env, adt_name, targs, variant, &parts, &parts_locs, true)
                    .map(Some);
            }
            return Ok(None);
        }
        let Ok((adt_name, targs)) = self.expect_adt(expected) else { return Ok(None) };
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let Some(variant) = def.variants.iter().position(|vr| vr.name == name) else { return Ok(None) };
        self.check_ctor_pattern_fields(heap, interp, env, adt_name, targs, variant, &parts, &parts_locs, false).map(Some)
    }

    /// Lower a value pattern — a non-immediate literal, or `(= expr)`'s
    /// expression — into the [`Pattern::Guard`] that tests it.
    ///
    /// The test is built as *source* (`(equals $match-scrut EXPR)`) and run
    /// through `check_at`, deliberately: that is the one path on which
    /// instance-method resolution, trait bounds and monomorphization all
    /// happen, so a user type's own `Eq` impl is what compares it, a type
    /// with no `Eq` is a type error rather than a silently-never-matching
    /// arm, and the compile pipeline sees an ordinary `assoc` call it
    /// already knows how to translate and to follow as a dependency.
    fn value_pattern(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        expected: &Type,
        expr: Value,
        loc: Option<Loc>,
    ) -> Result<(Pattern, PatternBindings), Error> {
        // Asked before the call is built, because "no method `equals`" is
        // not what went wrong: the type is one nothing can compare.
        if !self.implements_eq(env, expected) {
            return Err(Error::TypeError(format!(
                "pattern: `{}` cannot be compared by value — its type {:?} does not implement `Eq`",
                core::print(heap, expr),
                expected
            )));
        }
        let form = {
            let mut s = RootScope::new(heap);
            let equals = s.intern_symbol("equals");
            let scrut = s.intern_symbol(MATCH_SCRUT);
            core::list(&mut s, &[equals, scrut, expr])?
        };
        let guard_env =
            env.extended_with_locs(vec![(MATCH_SCRUT.to_string(), expected.clone(), None)]);
        let checked = self.check_at(heap, interp, &guard_env, form, Some(&Type::Bool), loc)?;
        if checked.ty != Type::Bool && checked.ty != Type::Never {
            return Err(Error::TypeError(format!(
                "pattern: comparing with `equals` produced {:?}, not a bool",
                checked.ty
            )));
        }
        Ok((Pattern::Guard { name: MATCH_SCRUT.to_string(), form: checked.form }, Vec::new()))
    }

    /// Whether `ty` can be compared with `equals` — the same two lookups
    /// [`Self::try_instance_method`] and its bounded-type-variable fallback
    /// would do for the call [`Self::value_pattern`] is about to build,
    /// asked first so that "this type cannot be compared" is the error
    /// rather than "no such method `equals`", which names the mechanism
    /// instead of the problem.
    ///
    /// The second lookup is why a generic body can use a value pattern at
    /// all: `(defun same<A> ((x A) (y A)) i32 (where (Eq A)) (match x ((= y)
    /// 1) (_ 0)))` is checked once with `A` still a type variable, and there
    /// is no `AdtDef` to ask — the `where` clause is the promise, and the
    /// call it lowers to becomes a real one when monomorphization re-checks
    /// the body with `A` known. `trait_method` searches inherited methods
    /// too, so `(where (Ord A))` is equally enough.
    fn implements_eq(&self, env: &Env, ty: &Type) -> bool {
        // The same receiver resolution the `equals` call this is clearing
        // the way for will perform — asked here so "this type cannot be
        // compared" is the error rather than a mismatch from deeper in.
        let type_fq = self.assoc_receiver_path(ty, "equals");
        let Some(type_fq) = type_fq else { return false };
        if let Some(def) = self.reg.type_def(&type_fq) {
            if matches!(def.assoc.get("equals"), Some(af) if af.instance) {
                return true;
            }
        }
        env.bounds.get(type_fq.last_segment()).is_some_and(|bounds| {
            bounds.iter().any(|tb| {
                self.reg.trait_def(&tb.trait_path).is_some_and(|t| self.reg.trait_method(t, "equals").is_some())
            })
        })
    }

    fn check_ctor_pattern(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
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
        let loc = parts_locs[0].1.clone();

        // `(= expr)` — the escape into ordinary evaluation, and the only way
        // to compare against a value with no literal syntax (a `defstruct`
        // instance, a global, a computed one). Ahead of every resolution
        // below because `=` names no constructor and never could.
        if is_symbol(parts[0], SymId::EQUALS) {
            if parts.len() != 2 {
                return Err(Error::TypeError("pattern: (= expr)".into()));
            }
            return self.value_pattern(heap, interp, env, expected, parts[1], loc);
        }
        // `'foo` — a symbol literal. It reaches here as `(quote foo)`, and
        // it is *not* checked as an expression the way `(= expr)`'s argument
        // is: a quoted datum's type is `Sexpr`, so `(equals sym 'foo)`
        // would be a type error. The comparison wanted is between two
        // `symbol`s, so the test is built over the symbol's *name* —
        // `string->symbol` is the bridge, and interning is exactly what
        // makes the resulting comparison identity-as-equality on both
        // sides of the compile boundary.
        if is_symbol(parts[0], SymId::QUOTE) && parts.len() == 2 {
            if let Value::Symbol(id) = parts[1] {
                let name = heap.symbol_name(id).to_string();
                let expr = {
                    let mut s = RootScope::new(heap);
                    let f = s.intern_symbol("string->symbol");
                    let lit = s.alloc_string(name);
                    s.push_root(lit);
                    core::list(&mut s, &[f, lit])?
                };
                return self.value_pattern(heap, interp, env, expected, expr, loc);
            }
        }

        // Sexpr downcast patterns (design plan's "出す" section — the CL-
        // conformant counterpart of Stage 1's "入れる" retype/wrap). Only
        // attempted against a `Sexpr` scrutinee, and only for a head that
        // isn't one of `Sexpr`'s own eleven built-in variant names, so
        // `(int n)`/`(cons a d)`/... keep meaning exactly what they always
        // have.
        if is_sexpr_expectation(expected) {
            if let Value::Symbol(id) = parts[0] {
                if id == SymId::THE {
                    return self.check_type_test_pattern(heap, interp, env, &parts, &parts_locs);
                }
            }
            const BUILTIN_SEXPR_CTORS: &[SymId] = &[
                SymId::NIL,
                SymId::INT,
                SymId::FLOAT,
                SymId::CHAR,
                SymId::BOOL,
                SymId::SYM,
                SymId::STR,
                SymId::CONS,
                SymId::BIGNUM,
                SymId::RATIO,
                SymId::PATH,
            ];
            let is_builtin_head = matches!(parts[0], Value::Symbol(id) if BUILTIN_SEXPR_CTORS.contains(&id));
            if !is_builtin_head {
                if let Some((adt_name, targs, variant)) = self.resolve_sexpr_downcast_ctor(heap, parts[0], parts_locs[0].1.as_ref())? {
                    return self.check_ctor_pattern_fields(heap, interp, env, adt_name, targs, variant, &parts, &parts_locs, true);
                }
            }
        }

        let ctor = match parts[0] {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
        };

        // An `Option<Sexpr>` scrutinee accepts `Sexpr`'s own constructors
        // directly, as if the `some` had already been peeled: `(int n)` and
        // `(none)` sit in one arm list. Without this a caller would have to
        // nest — `((some x) (match x ((int n) ..) ..)) ((none) ..)` — which
        // is a plain regression against the single-level `match` that
        // `Sexpr` (with its own `nil`) allows today.
        //
        // Exactly `Option<sexpr>`, matching the niche's own rule
        // (`check/repr.rs`): in an `Option<Option<Sexpr>>` an `(int n)` arm
        // could not say which level it peeled.
        //
        // The `some` needs no test of its own here. Under the niche the
        // unwrapped value is the same word, and every `Sexpr` variant test
        // already rejects the empty-list immediate, so re-checking the
        // pattern against `Sexpr` produces the right code as it stands.
        if is_option_of_sexpr(expected) {
            const BUILTIN_SEXPR_CTORS: &[&str] =
                &["nil", "int", "float", "char", "bool", "sym", "str", "cons", "bignum", "ratio", "path"];
            if BUILTIN_SEXPR_CTORS.contains(&ctor.as_str()) {
                return self.check_ctor_pattern(heap, interp, env, &sexpr_ty(), v);
            }
        }

        // The constructor is resolved against the scrutinee's type (its
        // variants), not the namespace — the type context disambiguates it.
        let (adt_name, targs) = self.expect_adt(expected)?;
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let variant = def.variants.iter().position(|vr| vr.name == ctor).ok_or_else(|| {
            Error::TypeError(format!("`{}` is not a constructor of `{}`", ctor, adt_name))
        })?;
        self.check_ctor_pattern_fields(heap, interp, env, adt_name, targs, variant, &parts, &parts_locs, false)
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
    fn resolve_sexpr_downcast_ctor(
        &self,
        heap: &Heap,
        head: Value,
        head_loc: Option<&Loc>,
    ) -> Result<Option<(Path, Vec<Type>, usize)>, Error> {
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
                            // The pattern head *is* the type name — its
                            // element span covers exactly the token.
                            if let Some(l) = head_loc {
                                self.record_type_use(&type_fq, l.clone());
                            }
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
                            // `color::red` — the type sits in the token's
                            // second-to-last segment.
                            self.record_path_type_use(&segs, &type_fq, head_loc);
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
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        parts: &[Value],
        parts_locs: &[(Value, Option<Loc>)],
    ) -> Result<(Pattern, PatternBindings), Error> {
        if parts.len() != 3 {
            return Err(Error::TypeError("pattern: (the Type pattern)".into()));
        }
        let ty = self.parse_type_here_at(heap, parts[1], parts_locs[1].1.as_ref())?;
        if !self.is_heap_repr(&ty) {
            return Err(Error::TypeError(format!(
                "pattern: `{:?}` has no Sexpr representation, cannot downcast with `the`",
                ty
            )));
        }
        let (pat, binds) = self.check_pattern(heap, interp, env, &ty, parts[2], parts_locs[2].1.clone())?;
        Ok((Pattern::TypeTest(ty, Box::new(pat)), binds))
    }

    /// Shared tail of [`Self::check_ctor_pattern`]: given a resolved `(adt
    /// path, type args, variant index)` — whether from the ordinary
    /// same-type ctor lookup or a Sexpr-downcast resolution — validates the
    /// field count and type-checks each field sub-pattern.
    fn check_ctor_pattern_fields(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        adt_name: Path,
        targs: Vec<Type>,
        variant: usize,
        parts: &[Value],
        parts_locs: &[(Value, Option<Loc>)],
        downcast: bool,
    ) -> Result<(Pattern, PatternBindings), Error> {
        if is_retired_nil(&adt_name, variant) {
            return Err(Error::TypeError(NIL_IS_GONE.to_string()));
        }
        let def = self.reg.type_def(&adt_name).expect("adt exists").clone();
        let subst: BTreeMap<String, Type> = def.params.iter().cloned().zip(targs.iter().cloned()).collect();

        let fields = &def.variants[variant].fields;
        if parts.len() - 1 != fields.len() {
            return Err(Error::TypeError(format!(
                "pattern `{}`: expected {} field(s), got {}",
                def.variants[variant].name,
                fields.len(),
                parts.len() - 1
            )));
        }
        let mut sub_pats: Vec<Pattern> = Vec::new();
        let mut binds = Vec::new();
        let mut field_types = Vec::new();
        for ((sub, sub_loc), field) in parts_locs[1..].iter().zip(fields.iter()) {
            // The declared field type with *this* site's type arguments
            // substituted in. Recorded here because here is the only place it is
            // known — see `Pattern::Ctor::field_types`'s doc comment.
            let field_ty = subst_apply(field, &subst);
            field_types.push(field_ty.clone());
            let (p, b) = self.check_pattern(heap, interp, env, &field_ty, *sub, sub_loc.clone())?;
            sub_pats.push(p);
            binds.extend(b);
        }
        // `Option<Sexpr>` is niche-represented (`check/repr.rs`): `none` is
        // the empty-list immediate and `some v` is `v` itself. Here — and
        // only here — both the ADT and its type arguments are known, so this
        // is where that instantiation becomes a pattern shape of its own
        // rather than a ctor pattern nothing downstream could classify.
        if !downcast && crate::types::path_is_builtin(&adt_name, "option") && targs.len() == 1 {
            if let Type::Named(a, _) = &targs[0] {
                if *a == Path::root("sexpr") {
                    return Ok((
                        match variant {
                            OPTION_SOME => Pattern::NonEmpty(Box::new(sub_pats.remove(0))),
                            _ => Pattern::Empty,
                        },
                        binds,
                    ));
                }
            }
        }
        Ok((Pattern::Ctor { type_name: adt_name, variant, args: sub_pats, field_types, downcast }, binds))
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
    ) -> Result<(Vec<Value>, Type), Error> {
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
        // The lowered forms are held in a plain `Vec`, which the collector
        // cannot see — but every node `check_at` produces is pushed onto the
        // heap's root stack there and stays until `check_form_at` truncates it,
        // so an element rooted three allocations ago is still rooted now.
        let mut out: Vec<Value> = Vec::new();
        let mut ty = Type::Unit;
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
            let one = self.check_at(heap, interp, env, *expr, exp, loc.clone());
            let checked = self.recovered(heap, one, loc)?;
            ty = checked.ty;
            out.push(checked.form);
        }
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

/// The `Sexpr` variant that destructures a literal of `v`'s kind —
/// `check_pattern`'s suggestion when a by-identity literal is written
/// against a `Sexpr` scrutinee. Only the four kinds that reach it (a
/// `Str` literal, and the three boxed numbers) are named; anything else
/// falls back to the variant that binds without destructuring, which is
/// always true and always available.
fn sexpr_variant_for_literal(heap: &Heap, v: Value) -> &'static str {
    match v {
        Value::Str(_) => "str",
        Value::Boxed(id) if heap.is_float(id) => "float",
        Value::Boxed(id) if heap.is_bignum(id) => "bignum",
        Value::Boxed(id) if heap.is_ratio(id) => "ratio",
        _ => "the sexpr",
    }
}

/// Whether `v` is the interned symbol `sym`.
///
/// Takes a [`SymId`] rather than a name: the syntax words this asks about are
/// pre-interned (`BUILTIN_SYMBOLS`), so the test is an integer comparison and
/// there is no spelling to get wrong.
fn is_symbol(v: Value, sym: SymId) -> bool {
    matches!(v, Value::Symbol(id) if id == sym)
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
/// computation already produces a full `i64` (`Value::Int` is uniformly
/// `i64` regardless of which static width labels it) — `check_as` relabels
/// the result to `I64` afterward on that branch, so this table only ever
/// needs to name the `i32` method once. An `i32`<->`i64` source/target pair
/// itself never reaches this table — `check_as` relabels that directly,
/// before the lookup.
fn as_conversion(from: &Type, to: &Type) -> Option<(&'static str, Option<&'static str>)> {
    use Type::*;
    // Widths do not appear in a conversion method's name: every integer type
    // has the same `int->float`/`int->bignum`/... catalog (they share one
    // runtime representation), and both float types the same `float->int`.
    // So the table is written between *families*, and `check_as` relabels the
    // result to the width that was actually asked for.
    let to_key = called_width(to);
    match (from, &to_key) {
        _ if from.is_integer() => match &to_key {
            F64 => Some(("int->float", None)),
            Bignum => Some(("int->bignum", None)),
            Ratio => Some(("int->ratio", None)),
            Char => Some(("int->char", Some("try-int->char"))),
            _ => None,
        },
        _ if from.is_float() => match &to_key {
            I32 => Some(("float->int", None)),
            Bignum => Some(("float->bignum", None)),
            Ratio => Some(("float->ratio", None)),
            _ => None,
        },
        (Char, I32) => Some(("char->int", None)),
        (Bignum, Ratio) => Some(("bignum->ratio", None)),
        (Bignum, F64) => Some(("bignum->float", None)),
        (Bignum, I32) => Some(("bignum->int", Some("try-bignum->int"))),
        (Ratio, F64) => Some(("ratio->float", None)),
        (Ratio, Bignum) => Some(("ratio->bignum", None)),
        _ => None,
    }
}

/// The width a conversion *into* `ty`'s family is actually registered at:
/// `i32` for every integer, `f64` for both floats, and `ty` itself for
/// everything else. See [`as_conversion`].
fn called_width(ty: &Type) -> Type {
    if ty.is_integer() {
        Type::I32
    } else if ty.is_float() {
        Type::F64
    } else {
        ty.clone()
    }
}



/// `((TYPE SYM)...)` — a vtable's contents, or the implementation targets a
/// `dyn-call` could reach: each entry the owning type's path and the method
/// name.
fn vtable_form(heap: &mut Heap, slots: &[(Path, String)]) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    for (ty, method) in slots {
        let tp = forms::path_form(f.heap(), ty);
        let m = f.heap().intern_symbol(method);
        let one = core::list(f.heap(), &[tp, m])?;
        f.push(one);
    }
    f.finish_list()
}

/// `Some(inner)` as a [`Checked`] — `Checker::check_as`'s `try-as` wrapper for a
/// total conversion's already-computed result.
fn wrap_some(heap: &mut Heap, checker: &Checker, inner: Checked, target: Type) -> Result<Checked, Error> {
    let ty = Type::Named(Path::root("option"), vec![target.clone()]);
    let form = checker.construct_form(heap, &Path::root("option"), 0, false, &[target], &[inner.form])?;
    Ok(Checked::new(forms::rooted(heap, form), ty))
}

/// `Option::none` as a [`Checked`], already at `ty` (an `Option<_>` type) —
/// `Checker::check_call_opt_key`'s filler for an omitted `&optional`/`&key`
/// argument that has no default. `variant: 1` is `option`'s `none` variant
/// (see `check::registry::option_def`), the CL-`nil` counterpart of
/// `wrap_some`'s `variant: 0`.
fn option_none(heap: &mut Heap, checker: &Checker, ty: Type) -> Result<Checked, Error> {
    let form = checker.construct_form(heap, &Path::root("option"), 1, false, &[], &[])?;
    Ok(Checked::new(forms::rooted(heap, form), ty))
}

/// Relabels an already-computed `Option<I32>` [`Checked`] (an `as_conversion`
/// `try_method` call's result) to `Option<inner_target>` — `Checker::check_as`'s
/// `i64`-target counterpart of the plain relabel it does for a total
/// conversion; see [`as_conversion`]'s doc comment for why the underlying
/// `Option`'s runtime payload doesn't actually change.
fn retype_option(opt: Checked, inner_target: Type) -> Checked {
    Checked::new(opt.form, Type::Named(Path::root("option"), vec![inner_target]))
}

/// `Some(t)` unless `t` is `Never` (which never constrains an expectation).
fn non_never(t: &Type) -> Option<&Type> {
    if *t == Type::Never {
        None
    } else {
        Some(t)
    }
}

/// Join two branch types: [`merge_holes`], with a diagnostic when they don't
/// merge. `Never` is absorbed by the other side, so a diverging branch never
/// constrains the result.
fn join_types(a: &Type, b: &Type) -> Result<Type, Error> {
    merge_holes(a, b).ok_or_else(|| {
        Error::TypeError(format!("branches have incompatible types: {:?} vs {:?}", a, b))
    })
}

/// Fill each side's inference holes from the other, or `None` if they
/// disagree on anything that is not a hole.
///
/// A hole is the [`Type::Never`] a probed `match` arm left where it could not
/// infer a type argument (`Checker::infer_probe`): `(result::ok text)` probes
/// to `Result<string, !>` and its sibling `(result::err e)` to
/// `Result<!, FileError>`, and merging the two yields the
/// `Result<string, FileError>` that neither arm knew on its own — the
/// arm-to-arm unification a constraint solver would do, done structurally
/// because holes come from exactly one place.
///
/// Merging *at the top level* is the older, narrower rule this generalizes:
/// `!` is the bottom type, so a diverging branch takes the other's type.
/// Applying it inside type arguments as well is the same subtyping one level
/// down, and outside a probe it is reachable only from a written-out `!`
/// argument (`Result<i32, !>`), where it is equally correct.
fn merge_holes(a: &Type, b: &Type) -> Option<Type> {
    if a == b {
        return Some(a.clone());
    }
    match (a, b) {
        (Type::Never, t) | (t, Type::Never) => Some(t.clone()),
        (Type::Named(pa, aa), Type::Named(pb, ba)) if pa == pb => {
            merge_hole_lists(aa, ba).map(|args| Type::Named(pa.clone(), args))
        }
        (Type::Dyn(pa, aa), Type::Dyn(pb, ba)) if pa == pb => {
            merge_hole_lists(aa, ba).map(|args| Type::Dyn(pa.clone(), args))
        }
        (Type::Fn(pa, ra, reta), Type::Fn(pb, rb, retb)) => {
            let params = merge_hole_lists(pa, pb)?;
            let rest = match (ra, rb) {
                (None, None) => None,
                (Some(x), Some(y)) => Some(Box::new(merge_holes(x, y)?)),
                _ => return None,
            };
            Some(Type::Fn(params, rest, Box::new(merge_holes(reta, retb)?)))
        }
        _ => None,
    }
}

/// [`merge_holes`] over two equal-length type lists.
fn merge_hole_lists(a: &[Type], b: &[Type]) -> Option<Vec<Type>> {
    (a.len() == b.len())
        .then(|| a.iter().zip(b).map(|(x, y)| merge_holes(x, y)).collect::<Option<Vec<_>>>())
        .flatten()
}

/// Whether `t` still carries an inference hole ([`merge_holes`]) — a
/// [`Type::Never`] *nested* inside it. A bare `Never` is not a hole: that is
/// an honestly diverging expression, which every caller here already handles
/// through [`non_never`].
fn has_open_hole(t: &Type) -> bool {
    fn nested(t: &Type) -> bool {
        *t == Type::Never || has_open_hole(t)
    }
    match t {
        Type::Named(_, args) | Type::Dyn(_, args) => args.iter().any(nested),
        Type::Fn(params, rest, ret) => {
            params.iter().any(nested)
                || rest.as_deref().is_some_and(nested)
                || nested(ret)
        }
        _ => false,
    }
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

/// `Option<Sexpr>` — the type an S-expression datum has once the empty list
/// is `none` (see [`is_option_of_sexpr`]).
fn option_of_sexpr() -> Type {
    Type::Named(Path::root("option"), vec![sexpr_ty()])
}

/// Does an expected type ask for an S-expression?
///
/// Both spellings answer yes. `Option<Sexpr>` is what a datum has, and it is
/// what the implicit widenings should fire on — a `Symbol`, a heap value or a
/// `:dyn` reaching "an S-expression is wanted here" is the same rule
/// whichever of the two the site declared. Bare `Sexpr` still means
/// *non-empty*, and a widening into it is just as sound: nothing these rules
/// produce is the empty list.
fn is_sexpr_expectation(ty: &Type) -> bool {
    *ty == sexpr_ty() || is_option_of_sexpr(ty)
}

/// Is `ty` exactly `Option<Sexpr>` — the type the empty list's niche belongs
/// to (`check/repr.rs`)?
///
/// One predicate because three decisions have to agree on the same boundary:
/// how the value is represented, how its constructors lower, and which
/// patterns a `match` on it accepts. Were any of them drawn wider — around
/// "an `Option` whose payload *represents* like a `Sexpr`", which
/// `Option<Option<Sexpr>>` does — the outer level would claim the same
/// empty-list word as the inner one and the two `none`s would collide.
fn is_option_of_sexpr(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, args)
        if crate::types::path_is_builtin(p, "option")
            && args.len() == 1
            && matches!(&args[0], Type::Named(a, _) if *a == Path::root("sexpr")))
}

/// The `Sexpr` constructor name that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping — see [`Checker::wrap_rest_elem`]).
/// `&rest`'s declared element type must be one of these (or `Sexpr`) since
/// nothing else has a lossless `Sexpr` encoding to collect into a list with.
fn sexpr_ctor_for(elem_ty: &Type) -> Option<&'static str> {
    match elem_ty {
        // Every integer width is one `Sexpr::Int` and both float widths one
        // `Sexpr::Float`: the widths differ statically and share a runtime
        // value, so there is nothing else they could encode as. Written as
        // guards rather than a variant list so a width added later is
        // encodable the day it is added.
        _ if elem_ty.is_integer() => Some("int"),
        _ if elem_ty.is_float() => Some("float"),
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
/// The trait method an operator spells when its receiver is a `where`-bounded
/// type variable — `+` is `Add`'s `add`, `<` is `Ord`'s `less`.
///
/// One table, not a rule: the prelude's numeric traits pick these names
/// (`prelude.rs`'s "arithmetic traits" section says why they cannot be the
/// operators), and this is the other half of that choice. A name absent from
/// the table is passed through unchanged, and so is one a bound declares
/// itself.
fn trait_operator_method(op: &str) -> Option<&'static str> {
    Some(match op {
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "/" => "div",
        "rem" => "remainder",
        "logand" => "bit-and",
        "logior" => "bit-or",
        "logxor" => "bit-xor",
        "lognot" => "bit-not",
        // `Eq`/`Ord` predate the arithmetic traits and already had these four
        // under spelled-out names; the operators reach them the same way.
        "=" => "equals",
        "/=" => "not-equals",
        "<" => "less",
        "<=" => "less-equal",
        ">" => "greater",
        ">=" => "greater-equal",
        _ => return None,
    })
}

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
    path.is_simple() && params.contains(path.last_segment())
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
/// The parameter types a *function value* of this signature really takes:
/// the required ones, then `&optional`, then `&key`. `FnSig::params` holds
/// the required list alone because a call site that resolved the callee *by
/// name* fills the rest in from the declaration
/// (`Checker::push_opt_key_args`) — a function value has no name to look
/// that up by, so the value's own type has to state the whole arity and its
/// caller supplies every argument itself. A parameter with no default is
/// `Option<T>` here, the same effective type it has inside the body.
///
/// Without this a reference to such a function type-checked as its required
/// arity and then failed at run time ("the closure takes 1 argument(s),
/// given 0") — the runtime function has one parameter per declared name,
/// whatever region it was declared in.
fn fn_value_params(sig: &FnSig) -> Vec<Type> {
    let mut ps = sig.params.clone();
    let effective =
        |p: &OptKeyParam| opt_key_effective_ty(&p.decl_ty, p.default.is_some());
    ps.extend(sig.optionals.iter().map(effective));
    ps.extend(sig.keys.iter().map(effective));
    ps
}

/// [`fn_value_params`] as a `Type::Fn`, with `&rest` in the slot that type
/// keeps for it.
fn fn_value_type(sig: &FnSig) -> Type {
    Type::Fn(fn_value_params(sig), sig.rest.clone().map(Box::new), Box::new(sig.ret.clone()))
}

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
        Type::RandomState => "random-state".into(),
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
    segs.push(mangled_method_name(base.last_segment(), args));
    Path::from_segments(segs)
}

/// The mangled *local* name shared by function and method specializations —
/// e.g. `"unwrap <i32>"`. See [`mangled_fn_path`]'s doc comment for why the
/// space makes collisions with source-written names impossible.
fn mangled_method_name(base: &str, args: &[Type]) -> String {
    format!("{} <{}>", base, args.iter().map(mangle_type).collect::<Vec<_>>().join(","))
}

/// Replace type parameters in `t` with their bindings from `subst`.
/// `pub(crate)`: `crate::eval::interp`'s `data_variant_field_types` reuses it
/// to instantiate a generic enum's variant field types when decoding a
/// compiled global (`Interp::enum_defs`).
pub(crate) fn subst_apply(t: &Type, subst: &BTreeMap<String, Type>) -> Type {
    match t {
        Type::Named(n, args) if args.is_empty() && n.is_simple() => match subst.get(n.last_segment()) {
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
    // type-identity-ok: `Self` is a type *variable*, not a built-in type — a trait
    // signature's `Self` parses to the single-segment `Type::Named` every type variable
    // does, so `is_simple` here is the variable-ness test, not a built-in test.
    matches!(t, Type::Named(p, args) if args.is_empty() && p.is_simple() && p.last_segment() == "self")
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
        let subst: BTreeMap<String, Type> = def.params.iter().cloned().zip(concrete_args.iter().cloned()).collect();
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
    subst: &mut BTreeMap<String, Type>,
) -> Result<(), Error> {
    // A diverging value (`Never`) unifies with any template without binding.
    if *actual == Type::Never {
        return Ok(());
    }
    if let Type::Named(n, args) = tmpl {
        if args.is_empty() && is_param(n, params) {
            let key = n.last_segment().to_string();
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

/// The lowercase segments of a module/use path (a symbol or a `Value::Path`).
fn path_to_segs(heap: &Heap, v: Value) -> Result<Vec<String>, Error> {
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

/// Whether `v` is a `(where ...)` clause (vs. an ordinary body form) —
/// `Checker::check_defun` peeks at this to decide whether to consume it.
/// Rejects a `defun` that disagrees with the `defsignature` that promised it.
///
/// A declaration is only worth writing if it is binding: a call checked
/// against the declaration and then dispatched to a definition of another
/// shape is exactly the kind of quiet mismatch static types exist to stop.
/// The old implicit pre-pass could not do this — it *derived* the signature
/// from the definition, so the two agreed by construction.
///
/// Docstrings, parameter *names* and the body play no part: a signature is
/// the types, `pub`, and the `&rest` tail.
fn signature_agrees(name: &str, declared: &FnSig, defined: &FnSig) -> Result<(), Error> {
    let mismatch = |what: &str, decl: String, def: String| {
        Err(Error::TypeError(format!(
            "`{}` does not match its `defsignature`: {} declared as {}, defined as {}",
            name, what, decl, def
        )))
    };
    if declared.params.len() != defined.params.len() {
        return mismatch(
            "parameter count",
            declared.params.len().to_string(),
            defined.params.len().to_string(),
        );
    }
    for (i, (d, f)) in declared.params.iter().zip(&defined.params).enumerate() {
        if d != f {
            return mismatch(&format!("parameter {}", i + 1), format!("{:?}", d), format!("{:?}", f));
        }
    }
    if declared.ret != defined.ret {
        return mismatch("return type", format!("{:?}", declared.ret), format!("{:?}", defined.ret));
    }
    if declared.rest != defined.rest {
        return mismatch("`&rest`", format!("{:?}", declared.rest), format!("{:?}", defined.rest));
    }
    if !defined.type_params.is_empty() {
        return mismatch("type parameters", "none".to_string(), format!("{:?}", defined.type_params));
    }
    if !defined.optionals.is_empty() || !defined.keys.is_empty() {
        return mismatch(
            "parameter kinds",
            "required only".to_string(),
            "`&optional`/`&key`".to_string(),
        );
    }
    if declared.public != defined.public {
        let vis = |p: bool| if p { "`pub`".to_string() } else { "private".to_string() };
        return mismatch("visibility", vis(declared.public), vis(defined.public));
    }
    Ok(())
}

fn is_where_clause(heap: &Heap, v: Value) -> Result<bool, Error> {
    Ok(matches!(v, Value::Cons(_))
        && matches!(heap.list_to_vec(v)?.first(), Some(Value::Symbol(id)) if *id == SymId::WHERE))
}

/// Peeks `parts[at]` for a leading docstring — CL's rule for `defun`/
/// `defmacro`/`defmethod`: a string literal right before the body is a
/// docstring only when at least one more body form follows it, since a
/// *lone* trailing string is the function's return value, not
/// documentation, and the two are otherwise indistinguishable. Used by
/// `Self::parse_defun_sig`, `Self::check_defun_opt_key`,
/// `Self::parse_defmethod_sig` and `Self::check_defmacro`, each of which
/// bumps its own `body_start` by one when this returns `Some`.
fn take_leading_docstring(heap: &Heap, parts: &[Value], at: usize) -> Option<String> {
    match parts.get(at) {
        Some(Value::Str(id)) if parts.len() > at + 1 => Some(heap.string(*id).to_string()),
        _ => None,
    }
}

fn parse_macro_lambda_list(
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
            match *id {
                SymId::OPTIONAL => {
                    if rank >= 1 {
                        return Err(Error::TypeError("defmacro: &optional must precede &rest and &key, and appear once".into()));
                    }
                    rank = 1;
                    continue;
                }
                SymId::REST => {
                    if rank >= 2 {
                        return Err(Error::TypeError("defmacro: &rest must precede &key, and appear once".into()));
                    }
                    rank = 2;
                    continue;
                }
                SymId::KEY => {
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
            0 => required_names.push(Checker::macro_param_name(heap, *p, "required")?),
            1 => optionals.push(Checker::macro_opt_key_spec(heap, *p, "&optional")?),
            2 => {
                if rest_name.is_some() {
                    return Err(Error::TypeError("defmacro: &rest takes exactly one parameter name".into()));
                }
                rest_name = Some(Checker::macro_param_name(heap, *p, "&rest")?);
            }
            _ => keys.push(Checker::macro_opt_key_spec(heap, *p, "&key")?),
        }
    }
    if rank == 2 && rest_name.is_none() {
        return Err(Error::TypeError("defmacro: &rest must be followed by exactly one parameter name".into()));
    }
    Ok((required_names, optionals, rest_name, keys))
}

/// Whether a `defun`/`lambda` raw parameter list declares an
/// `&optional` or `&key` section — `Checker::check_defun` uses this to
/// route to `Self::check_defun_opt_key` instead of the ordinary
/// fixed/`&rest`-only path; `Checker::check_lambda` uses it only to
/// reject the combination with a clear error (see that method).
/// Why `&optional`/`&key` are a property of a *named* callee only, reported
/// by `lambda` and `labels` with the form's own name filled in.
///
/// A call site fills an omitted argument in with the parameter's **checked
/// default expression** (`Checker::check_call_opt_key` /
/// `Checker::push_assoc_opt_key_args`), which it reads off the callee's
/// `FnSig`/`AssocFn` — reachable only because the callee was resolved by
/// name. A `lambda` is reached through its *value*, whose only description
/// is `Type::Fn`: parameter types, a `&rest` element type, a return type. It
/// has nowhere to put an expression, and putting one there would make two
/// lambdas of identical signature but different defaults into two different
/// types. `labels` is the same case — its functions are ordinary
/// `Type::Fn`-typed locals, passable as values.
///
/// `&rest` is unaffected and works on both: it is entirely a matter of
/// types, and `Type::Fn` has a slot for it.
const OPT_KEY_NEEDS_A_NAME: &str = "{}: &optional/&key need a named callee (defun/defmethod) — a \
     function value is described by its `Type::Fn` alone, which has no place to carry a default \
     expression for a call site to fill in. `&rest` does work here.";

/// A `defstruct` option that takes exactly one name (`(:copier copy-point)`).
fn single_name_option(heap: &Heap, rest: &[Value], what: &str) -> Result<String, Error> {
    match rest {
        [Value::Symbol(id)] => Ok(heap.symbol_name(*id).to_string()),
        _ => Err(Error::TypeError(format!("defstruct: ({} name) takes exactly one name", what))),
    }
}

fn params_declare_opt_key(heap: &Heap, v: Value) -> Result<bool, Error> {
    let elems = heap.list_to_vec(v)?;
    Ok(elems.iter().any(|p| {
        matches!(p, Value::Symbol(id) if *id == SymId::OPTIONAL || *id == SymId::KEY)
    }))
}

/// One serialized entry back into the type it was written from.
///
/// Free rather than a method so [`Checker::capture_delta`]'s two template
/// arms, which deserialize different types, can share the error wording.
fn bincode_read<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    bincode::deserialize(bytes).map_err(|e| format!("dump: reading a template: {}", e))
}

/// `loop :thereis` was handed something that is not an `Option`.
///
/// CL's `thereis` answers the first non-nil value the clause produced; the
/// type that says "a value or nothing" here is `Option<T>`, so that is what
/// the clause takes. A `bool` test wants `:always`/`:never` instead.
fn loop_thereis_error(ty: &Type) -> Error {
    Error::TypeError(format!(
        "loop :thereis: expected an `Option<T>`, got `{}` — CL's `thereis` answers the first \
         non-nil value, and `Option` is what that is here (a `bool` test is `:always`/`:never`)",
        mangle_type(ty)
    ))
}
