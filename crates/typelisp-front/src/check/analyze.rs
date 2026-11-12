//! Analysis: expands macros, registers definitions and bindings, resolves
//! names and infers types over read `Sexpr` values, and builds the analyzed
//! tree (`Checked`). What does not agree stays in the tree for checking
//! (`check::verify`); the tree is lowered to core IR by `check::lower`.
//!
//! Inference is bidirectional in a small way: literal and constructor nodes
//! may be analyzed *against* an expected type (so integer literals adopt the
//! expected integer type and a nullary `None` learns its type argument), while
//! everything else synthesizes its own type and is reconciled against the
//! expectation.



use crate::{wk, Error, Heap, Loc, Path, RootScope, Type, Value};

use typelisp_mem::SymRef;
use super::semantic::TypeUse;

use super::core::{self, Checked, Items};
use super::forms;
use super::info::*;
use crate::dump::{CheckerDelta, RegistrySignature};

mod attribute;
mod c_struct;
mod ffi_callback;
pub use attribute::{TestCase, TestForms, TestReturn};
use crate::typeck::registry::{AdtKind, MacroDef};

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
/// Whether `ty` is, or contains, one of the FFI's raw machine words.
///
/// Recursive because a type argument can hold one *syntactically* — the checker
/// refuses `Vector<ptr>` where it is written (see `Checker::reject_raw_word_nested`),
/// and this is the second line rather than the first, so a spelling that slips
/// past there still cannot become a value outside `unsafe`.
fn type_mentions_raw_word(ty: &Type) -> bool {
    match ty {
        Type::Ptr | Type::CLong | Type::CULong | Type::PtrTo(_) => true,
        Type::Named(_, args) | Type::Dyn(_, args) => args.iter().any(type_mentions_raw_word),
        Type::Fn(ps, rest, ret) => {
            ps.iter().any(type_mentions_raw_word)
                || rest.as_deref().map(type_mentions_raw_word).unwrap_or(false)
                || type_mentions_raw_word(ret)
        }
        _ => false,
    }
}

pub trait MacroExpander {
    fn expand_macro(&self, heap: &mut Heap, path: &Path, raw_args: Vec<Value>) -> Result<Value, String>;

    /// Runs a checked form whose effect the next form's checking depends on —
    /// a `(defmacro ...)`, so that a later [`Self::expand_macro`] on its path
    /// finds a body to run, or a readtable registration.
    ///
    /// Every other definition reaches the evaluator through `exec`, called by
    /// the driver on the form the checker returned, and between top-level
    /// forms the driver runs these too (`project::needs_immediate_exec`).
    /// Between the forms *inside* one top-level form there is no driver: a
    /// `macrolet` binding is created while an expression is being checked, and
    /// a `defmacro` in a `(module ...)` block is checked along with the rest
    /// of the block, yet each has to be runnable before the very next form of
    /// the same top-level form is checked. These are the edges where checking
    /// installs something itself.
    fn install(&self, heap: &mut Heap, form: Value) -> Result<(), String>;
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

/// The call-site arity shape of a macro, derived from its [`MacroDef`] by
/// [`Info::resolve_macro`]/[`Info::macro_at`]
/// and consumed by [`MacroShape::check_arity`]. Carries only what a raw
/// argument *count* can be judged against — the `&key` names themselves aren't
/// needed here (keyword validation happens in `Interp::expand_macro`).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MacroShape {
    required: usize,
    optional: usize,
    rest: bool,
    has_keys: bool,
}

impl MacroShape {
    pub(crate) fn of(def: &MacroDef) -> MacroShape {
        MacroShape { required: def.required, optional: def.optional, rest: def.rest, has_keys: !def.keys.is_empty() }
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
    pub(crate) fn check_arity(self, head: &str, got: usize) -> Result<(), Error> {
        let MacroShape { required, optional, rest, has_keys } = self;
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
}








/// What happens when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
/// reuses a name already defined by *non-builtin* code (the same module, in
/// the same namespace). Redefining a builtin (`typeck::builtins`) is always an
/// error, regardless of this setting.
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


/// The synthetic `TopLevel::Module` path under which `check_form` bundles
/// monomorphized specializations together with the form that requested them.
/// Contains a space, which [`crate::read::Reader`] treats as a delimiter — so
/// no user-written `module` form can ever collide with it.
pub const MONO_BUNDLE_MODULE: &str = "<monomorph specializations>";


/// The variable a vector literal is built in (`Checker::check_vector_literal`).
/// Spelled with a leading `$` for the reason [`MATCH_SCRUT`] is: no reader
/// produces it, so it cannot capture a name the program wrote — and a
/// literal's elements are constants, so they never refer to it either.
const VECTOR_LITERAL_VAR: &str = "$vector-literal";

/// The element type the context of a `#(..)`/`#nA(..)` literal fixes:
/// `T` of an expected `Vector<T>`/`Array<T>` (`collection` names which), or
/// `Option<Sexpr>` when the literal is expected as S-expression data. `None`
/// when the context says nothing, and the first element decides.
pub(crate) fn literal_element_type(expected: Option<&Type>, collection: &Path) -> Option<Type> {
    match expected {
        Some(Type::Named(p, args)) if p == collection && args.len() == 1 => {
            Some(args[0].clone())
        }
        Some(t) if is_sexpr_expectation(t) => Some(option_of_sexpr()),
        _ => None,
    }
}

/// One element of a literal collection, as the form that evaluates to it. A
/// literal's contents are never evaluated, so a symbol, a path or a list is
/// quoted; numbers, strings, characters, booleans and nested literals
/// evaluate to themselves already. In a data vector (`data`) every element
/// is quoted, so that each one is S-expression data.
pub(crate) fn literal_element_form(heap: &mut Heap, e: Value, data: bool) -> Result<Value, Error> {
    if data || matches!(e, Value::Symbol(_) | Value::Path(_) | Value::Cons(_)) {
        let quote = heap.intern_symbol("quote");
        forms::list_from_vec_locs(heap, &[(quote, None), (e, None)])
    } else {
        Ok(e)
    }
}

/// `(let (($vector-literal (the Vector<T> (Vector::new))))
///    (push $vector-literal E)... $vector-literal)` for the elements `elems`.
/// With no `elem_ty`, the vector is `(Vector::new)` and the first element
/// decides `T`.
pub(crate) fn vector_literal_form(heap: &mut Heap, elem_ty: Option<&Type>, elems: &[Value]) -> Result<Value, Error> {
    let data = elem_ty.is_some_and(is_option_of_sexpr);
    let mut s = RootScope::new(heap);
    let var = s.intern_symbol(VECTOR_LITERAL_VAR);
    let new = forms::member_form(&mut s, &Path::builtin(typelisp_mem::wk::VECTOR), "new")?;
    s.push_root(new);
    let new_call = forms::list_from_vec_locs(&mut s, &[(new, None)])?;
    s.push_root(new_call);
    let init = match elem_ty {
        Some(elem_ty) => {
            let vec_ty = forms::type_to_form(&mut s, &Type::Named(Path::builtin(typelisp_mem::wk::VECTOR), vec![elem_ty.clone()]))?;
            s.push_root(vec_ty);
            let the = s.intern_symbol("the");
            forms::list_from_vec_locs(&mut s, &[(the, None), (vec_ty, None), (new_call, None)])?
        }
        None => new_call,
    };
    s.push_root(init);
    let binding = forms::list_from_vec_locs(&mut s, &[(var, None), (init, None)])?;
    s.push_root(binding);
    let bindings = forms::list_from_vec_locs(&mut s, &[(binding, None)])?;
    s.push_root(bindings);
    let let_sym = s.intern_symbol("let");
    let push = Value::Symbol(s.names().read(typelisp_mem::NsId::STD, "push"));
    let mut items: Vec<(Value, Option<Loc>)> = vec![(let_sym, None), (bindings, None)];
    for &e in elems {
        let ef = literal_element_form(&mut s, e, data)?;
        s.push_root(ef);
        let call = forms::list_from_vec_locs(&mut s, &[(push, None), (var, None), (ef, None)])?;
        s.push_root(call);
        items.push((call, None));
    }
    items.push((var, None));
    forms::list_from_vec_locs(&mut s, &items)
}

/// `Option`'s variant indices (`typeck::builtins`'s `Option`). Written down
/// once because three places already depend on the order — `()`'s inference
/// rule, the niche pattern shapes, and `check_construct`'s `None` shortcut.
pub(crate) const OPTION_SOME: usize = 0;
pub(crate) const OPTION_NONE: usize = 1;

/// `Sexpr`'s `nil` variant — the old empty list.
///
/// The variant is still *in* `sexpr_def`, and deliberately so: its number is
/// burned into the island's IR and into compiled code (`crate::sexpr_variant`
/// says why numbers are only ever appended). Removing the entry would
/// renumber every variant after it.
///
/// What is gone is its *surface*: `(nil)` can no longer be written as a
/// constructor or a pattern, because a writable empty-list `Sexpr` would
/// make `Option<Sexpr>`'s niche unsound — `(Option::some (nil))` and
/// `(Option::none)` would be the same word. The empty list is reached as
/// `Option<Sexpr>`'s `none`, and the core IR still spells it `(construct
/// sexpr 0 ..)`, which the checker emits itself.
pub(crate) const SEXPR_RESERVED_VARIANT: usize = crate::sexpr_variant::NIL;

/// `Sexpr`'s old `bignum` variant, retired with the type: a bignum box is an
/// `int` now. Same treatment as [`SEXPR_RESERVED_VARIANT`]:
/// the slot stays so nothing renumbers, and its surface is refused.
const SEXPR_RETIRED_BIGNUM: usize = crate::sexpr_variant::RETIRED_BIGNUM;

/// Message for a `(bignum ..)` constructor or pattern.
pub(crate) const BIGNUM_IS_GONE: &str = "`bignum` is no longer a `Sexpr` constructor: the `bignum` type was folded \
     into `int`, so a big integer is `(int n)` like any other.";

/// Message for a `(nil)` that survived in source somewhere.
pub(crate) const NIL_IS_GONE: &str = "`nil` is no longer a `Sexpr` constructor: the empty list is \
     `Option<Sexpr>`'s `none`. Write `()` where an `Option<Sexpr>` is expected, or \
     `(Option::none)`; match it with `((none) ...)`.";

/// Is `(adt, variant)` the retired `Sexpr::nil`?
pub(crate) fn is_retired_nil(adt: &Path, variant: usize) -> bool {
    variant == SEXPR_RESERVED_VARIANT && crate::types::path_is_builtin(adt, typelisp_mem::wk::SEXPR)
}

/// Is `(adt, variant)` the retired `Sexpr::bignum`?
pub(crate) fn is_retired_bignum(adt: &Path, variant: usize) -> bool {
    variant == SEXPR_RETIRED_BIGNUM && crate::types::path_is_builtin(adt, typelisp_mem::wk::SEXPR)
}

/// Whether a Rust-implemented builtin method's `int`-typed parameters and
/// result cross the compiled boundary as **raw machine words**.
///
/// An `int` is a tagged word in a register (`Repr::Int`), and the `rt_*`
/// shims that implement the builtin catalog — `rt_str_substring`,
/// `rt_hashtable_bucket_*`, the LLVM builders' `rt_llvm_call` — read and
/// answer plain `i64`s, as they did when these parameters were `i32`. So the
/// call site does the conversion: an `int` argument goes through
/// `(untag-int E)` (a fixnum's payload; a bignum is a language error — an
/// index or a count that size is a program's mistake) and an `int` result
/// through `(tag-int E)`. The interpreter's `untag-int` is the same check
/// with no shift, so both tiers refuse the same programs.
///
/// The exceptions are the builtins whose native lowering already speaks
/// tagged ints, because their answer can be a bignum: every method of `int`
/// and of `rational`, the conversions *into* `int` (`float->int`, `ratio->int`,
/// `numerator`, `denominator`, `int->int`), and `Chan`'s `len`/`cap`, whose
/// answers arrive through the suspension driver tagged like every wake value.
/// `ash`/`logbitp` on the fixed widths are exceptions too, the other way
/// round: their count is an `int` the island untags itself (`untag-int-value`),
/// the same way it does for `int`'s own. This list and the island's lowering
/// are one decision written twice; `tests/int_default_test.rs` runs each of
/// them in both tiers.
pub(super) fn int_boundary_raw(type_name: &Path, method: SymRef) -> bool {
    use crate::types::path_is_builtin;
    // `rational`'s shims (`typelisp_rt::rational`) speak tagged words
    // throughout, the `int` answers included.
    if path_is_builtin(type_name, typelisp_mem::wk::INT) || path_is_builtin(type_name, typelisp_mem::wk::REPR_RATIONAL) {
        return false;
    }
    if path_is_builtin(type_name, typelisp_mem::wk::CHAN) && matches!(method.well_known(), wk::M_LEN | wk::M_CAP) {
        return false;
    }
    !matches!(
        method.well_known(),
        wk::M_FLOAT_TO_INT | wk::M_RATIO_TO_INT | wk::M_NUMERATOR | wk::M_DENOMINATOR | wk::M_INT_TO_INT | wk::M_ASH | wk::M_LOGBITP
    )
}

/// [`int_boundary_raw`] for a free builtin: `sexpr-int` answers the tagged
/// word an `int` node already is.
pub(super) fn int_boundary_native_fn(name: SymRef) -> bool {
    name.is(wk::F_SEXPR_INT)
}

/// The `(TEMPLATE...)` list a `defstruct`/`defenum` form carries: each
/// field's type, the owning type's parameters written as `(type-param N)`
/// ([`core::type_form`]).
pub(super) fn field_template_list(heap: &mut Heap, fields: &[Type]) -> Result<Value, Error> {
    let mut items = Items::new(heap);
    for field in fields {
        let t = core::type_form(items.heap(), field)?;
        items.push(t);
    }
    items.finish_list()
}

/// A list of strings, each rooted as it is made — `defstruct`'s field names.
pub(super) fn string_list(heap: &mut Heap, items: &[String]) -> Result<Value, Error> {
    let mut list = Items::new(heap);
    for s in items {
        let v = list.heap().alloc_string(s.clone());
        list.push(v);
    }
    list.finish_list()
}

/// `(untag-int E)` — see [`int_boundary_raw`].
pub(crate) fn untag_int_form(heap: &mut Heap, e: Value) -> Result<Value, Error> {
    core::tagged(heap, wk::UNTAG_INT, &[e])
}

/// `(tag-int E)` — see [`int_boundary_raw`].
pub(super) fn tag_int_form(heap: &mut Heap, e: Value) -> Result<Value, Error> {
    core::tagged(heap, wk::TAG_INT, &[e])
}

/// One `labels` definition on its way into a node: its name, its parameters
/// (name and declared type), its return type, and its already-checked body
/// forms.
pub(super) type LabelsDef = (SymRef, Vec<(SymRef, Type)>, Type, Vec<Checked>);

/// What a `defmacro` lambda list parses into: the required parameter names,
/// the `&optional` parameters (each with its default form, if it declared
/// one), the `&rest` name, and the `&key` parameters in the same shape as the
/// optionals.
pub(crate) type MacroLambdaList = (Vec<SymRef>, Vec<(SymRef, Option<Value>)>, Option<SymRef>, Vec<(SymRef, Option<Value>)>);

/// Opaque saved namespace context — see [`Checker::suspend_ns_context`].
pub struct NsContext {
    ns: typelisp_mem::NsId,
    file_ns: Vec<typelisp_mem::NsId>,
}

/// What writing `defmethod` says: methods are written in the type's `impl`.



pub struct Checker {
    /// The module context: the module forms are checked in, and the files'
    /// own modules — see [`Info`].
    info: Info,
    /// What to do when a `defun`/`defmethod`/`defmacro`/`defvar`/`defstruct`
    /// reuses a non-builtin name — see [`RedefPolicy`]. Defaults to `Warn`;
    /// override with [`Self::set_redef_policy`] (e.g. from a CLI flag).
    redef_policy: RedefPolicy,
    /// The checker proper (`docs/dev/checker-rebuild-plan.md` 6).
    rebuilt: crate::typeck::toplevel::TypeChecker,
}

/// The forms [`Checker::check_form_dispatch`] matches at top level. Each one
/// defines or declares something for the whole program, so none of them
/// means anything inside an expression.
/// Every name [`Checker::check_list`] dispatches on as a special form — what
/// [`Checker::is_builtin_form_head`] keeps a macro of the same name from
/// taking over. One list, which `editor_keyword_sync_test` compares with the
/// dispatch itself: a second, hand-kept copy had fallen behind by twelve
/// names, and a macro named `block` took over `(block ...)` at top level
/// while one inside a function still meant the special form.
pub const SPECIAL_FORM_HEADS: &[u32] = &[
    wk::IF, wk::LET, wk::LET_STAR, wk::PROGN, wk::UNSAFE, wk::SETF,
    wk::INCF, wk::DECF, wk::ROTATEF, wk::SHIFTF, wk::LOOP, wk::BREAK,
    wk::RETURN, wk::BLOCK, wk::RETURN_FROM, wk::CATCH, wk::THROW, wk::UNWIND_PROTECT,
    wk::LIST, wk::LAMBDA, wk::LABELS, wk::MACROLET, wk::SYMBOL_MACROLET, wk::MATCH,
    wk::PANIC, wk::THE, wk::AS, wk::TRY_AS, wk::COMPILE, wk::TASK,
    wk::THREAD, wk::SELECT, wk::QUOTE, wk::QUASIQUOTE, wk::FORMAT, wk::PRINT,
    wk::PRINTLN, wk::EPRINT, wk::EPRINTLN, wk::SOURCE_FILE, wk::PPRINT, wk::PPRINT_FILL,
    wk::PPRINT_LINEAR, wk::PPRINT_TABULAR, wk::PPRINT_LOGICAL_BLOCK, wk::C_ALLOC, wk::C_REF, wk::C_DEREF,
    wk::DEF_C_STRUCT, wk::DYN, wk::APPLY, wk::MAKE_INSTANCE, wk::DOCUMENTATION, wk::TRACE,
    wk::UNTRACE, wk::STEP, wk::DISASSEMBLE, wk::ED,
];

/// Which call sugar ([`Checker::check_call_sugar`]) a call of `head` with
/// `nargs` arguments is: forms keyed on the name *and* the argument count,
/// which no single definition could express. `concatenate_is_fn` says
/// whether `concatenate` names a function here, which then wins.
pub(crate) fn call_sugar(head: &str, nargs: usize, concatenate_is_fn: impl FnOnce() -> bool) -> Option<CallSugar> {
    const ARITH: [&str; 11] = ["+", "-", "*", "/", "max", "min", "logand", "logior", "logxor", "gcd", "lcm"];
    const CMP: [&str; 6] = ["<", "<=", ">", ">=", "=", "/="];
    Some(if nargs <= 1 && (ARITH.contains(&head) || CMP.contains(&head)) {
        CallSugar::NullaryOrUnary
    } else if nargs > 2 && ARITH.contains(&head) {
        CallSugar::VariadicArith
    } else if nargs > 2 && CMP.contains(&head) {
        CallSugar::VariadicCmp
    } else if head == "append" && nargs > 2 {
        CallSugar::VariadicAppend
    } else if head == "concatenate" && !concatenate_is_fn() {
        CallSugar::Concatenate
    } else if head == "aref" && nargs > 0 {
        CallSugar::Aref
    } else if head == "log" && nargs == 2 {
        CallSugar::LogWithBase
    } else if head == "atan" && nargs == 2 {
        CallSugar::Atan2
    } else {
        return None;
    })
}

/// The kinds of call sugar [`call_sugar`] tells apart.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CallSugar {
    NullaryOrUnary,
    VariadicArith,
    VariadicCmp,
    VariadicAppend,
    Concatenate,
    Aref,
    LogWithBase,
    Atan2,
}

pub(crate) const TOPLEVEL_FORM_HEADS: &[u32] = &[
    wk::PUB, wk::DEFUN, wk::DEFSIGNATURE, wk::DEFFFI, wk::DEFVAR, wk::DEFPARAMETER,
    wk::DEFCONSTANT, wk::DEFMACRO, wk::MODULE, wk::DEFMETHOD, wk::DEFSTRUCT, wk::DEFENUM,
    wk::DEFTRAIT, wk::DEFTYPE, wk::IMPL, wk::USE, wk::LOAD, wk::IMPORT,
    wk::SHADOWING_IMPORT, wk::IN_MODULE, wk::DEF_C_STRUCT, wk::SHADOW,
];

impl Checker {
    pub fn new() -> Checker {
        Checker {
            info: Info::new(),
            redef_policy: RedefPolicy::default(),
            rebuilt: crate::typeck::toplevel::TypeChecker::default(),
        }
    }

    /// What the `~/name/` directives checked so far resolved to — see
    /// [`Self::format_calls`]. Read by `compile::aot::compile_file`, which
    /// turns each into a startup registration so the directive works in a
    /// standalone executable.
    pub fn format_call_targets(&self) -> Vec<crate::CompileTarget> {
        self.rebuilt.format_call_targets()
    }

    /// Where the registry's writes are as of now: the "before" half of
    /// [`Self::capture_delta`]. Take one, load a unit, then ask what changed.
    pub fn signature(&self) -> RegistrySignature {
        RegistrySignature::at(self.rebuilt.generation())
    }

    /// Every entry the registry holds beyond the built-in definitions, one
    /// line each and sorted, for a test to compare two sessions by.
    pub fn registry_entries(&self) -> Vec<String> {
        self.rebuilt.entries()
    }

    /// Everything the registry was written since `before`, for a unit of a
    /// dump. See [`crate::dump`] for why a unit carries its own additions
    /// rather than the whole world.
    pub fn capture_delta(&self, heap: &Heap, before: &RegistrySignature) -> Result<CheckerDelta, String> {
        let unit = self.rebuilt.capture(heap, before.generation()).map_err(|e| format!("writing the checker's registry failed: {}", e))?;
        let registry = bincode::serialize(&unit).map_err(|e| format!("writing the checker's registry failed: {}", e))?;
        Ok(CheckerDelta { registry })
    }

    /// Reinstates what a unit of a dump wrote of the registry.
    pub fn apply_delta(&mut self, delta: CheckerDelta, heap: &mut Heap) -> Result<(), String> {
        let written: crate::typeck::wire::RebuiltUnit = bincode::deserialize(&delta.registry).map_err(|e| {
            format!(
                "reading the checker's part of the dump failed: {} — its shape has changed since the dump was made; run scripts/regen-prelude-bitcode.sh",
                e
            )
        })?;
        let unit = written.restore(heap).map_err(|e| e.to_string())?;
        self.rebuilt.take_unit(heap, unit);
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
        self.rebuilt.take_warnings()
    }

    /// Turns error-recovery mode on or off. The LSP
    /// enables it before a check so a file with type errors still yields a
    /// partial tree and a full list of errors; everything else leaves it off
    /// and keeps the strict "abort on first error" behaviour.
    pub fn set_recover(&mut self, on: bool) {
        self.rebuilt.set_recover(on);
    }

    /// Set the namespace that subsequent `check_form`/`check_form_at` calls
    /// treat as "current". Used by the CLI's `run_file` to place a runtime
    /// `(eval ...)` in the script's own file-derived module (so the script's
    /// module-scoped globals/functions resolve from an eval'd form, and
    /// eval-defined names register there too), matching how those forms were
    /// checked at load time.
    pub fn set_current_ns(&mut self, ns: typelisp_mem::NsId) {
        self.info.ns = ns;
        self.publish_name_view();
    }

    /// Drains and returns every error accumulated at a recovery boundary while
    /// recover mode was on. Each already carries its own
    /// source location. Empty unless `recover` is enabled.
    pub fn take_errors(&self) -> Vec<Error> {
        self.rebuilt.take_errors()
    }

    /// Drains and returns every user-defined type/trait name occurrence
    /// recorded so far — the LSP turns these into
    /// `textDocument/semanticTokens` (`crate::check::semantic`).
    pub fn take_type_uses(&self) -> Vec<TypeUse> {
        self.rebuilt.take_type_uses()
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
        // The form is run in the module it is checked in, which is where a
        // `read` it makes at run time reads.
        heap.names().enter_module(self.current_module());
        let ns = self.current_module();
        let site = crate::typeck::use_form::Site { ns, file: self.info.file_ns.last().copied(), policy: self.redef_policy };
        let r = self.rebuilt.check_form(heap, interp, site, v, heap.cons_loc(v).or(loc_hint));
        if let Some(ns) = self.rebuilt.take_entered() {
            self.info.ns = ns;
        }
        r
    }

    /// Writes what the registry defines into `heap`'s table of what names
    /// mean. The first form [`Self::check_form_at`] checks does it, and
    /// each form after tells it what it wrote; a driver that reads before
    /// checking anything — a prelude or a dump just loaded — does it first.
    pub fn publish_names(&self, heap: &Heap) {
        self.rebuilt.publish_names(heap);
    }

    /// Reports every `defsignature` in this unit that never got a definition.
    ///
    /// Called by a file/module loader once the unit's forms are all checked.
    /// The REPL deliberately does *not* call it: a declaration typed on one
    /// line and its definition on the next are two batches, and both are one
    /// session.
    pub fn finish_unit(&mut self) -> Result<(), Error> {
        let names: Vec<String> = self.rebuilt.take_predeclared().iter().map(Path::to_string).collect();
        if names.is_empty() {
            return Ok(());
        }
        let list = names.join("`, `");
        let (subject, verb) = if names.len() == 1 { ("declaration", "has") } else { ("declarations", "have") };
        Err(Error::TypeError(format!(
            "defsignature: `{}` {} no definition in this file — a {} promises one",
            list, verb, subject
        )))
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
        self.rebuilt.expr_type()
    }

    /// Every bare name visible for completion while checking the module
    /// `module_path`; none for a module the session does not have.
    pub fn completion_candidates(&self, module_path: &[String]) -> Vec<super::CompletionCandidate> {
        let Some(here) = typelisp_mem::symbols::find_ns(module_path) else {
            return Vec::new();
        };
        self.rebuilt.completion_candidates(here)
    }

    /// Where each definition, and each local variable a reference names,
    /// was written, and the type of each expression: what the LSP's
    /// goto-definition and hover read.
    pub fn def_locs(&self) -> super::DefLocs {
        self.rebuilt.def_locs()
    }

    /// The documentation of each definition.
    pub fn docs(&self) -> super::Docs {
        self.rebuilt.docs()
    }

    /// Every definition the session holds, the library's and the program's.
    pub fn definitions(&self) -> Vec<super::Defined> {
        self.rebuilt.definitions()
    }

    /// Every name a `use` brought into a module.
    pub fn imports(&self) -> Vec<super::Imported> {
        self.rebuilt.imports()
    }

    /// What the module-level function `path` returns, when there is one.
    pub fn fn_ret(&self, path: &Path) -> Option<Type> {
        self.rebuilt.fn_ret(path)
    }

    /// The kind of the data type `path`, when there is one.
    pub fn type_kind(&self, path: &Path) -> Option<AdtKind> {
        self.rebuilt.type_kind(path)
    }


    // ---- namespace navigation & resolution --------------------------------
    //
    // Identities are structured `Path`s, never joined `"a::b"` strings: the only
    // `::`-string decoding lives in the reader/parser. Resolution navigates the
    // namespace tree, current namespace first then root.


















    /// If `v` is a compound form whose head resolves to a macro — a bare
    /// name ([`Info::resolve_macro`], current namespace) or a `mod::name`
    /// path ([`Info::macro_at`], cross-module) — expand it one
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
        self.rebuilt.expand_toplevel_macro(heap, interp, self.info.ns, v)
    }












    /// Rule C, half two: the storage a raw word may not go into.
    ///
    /// [`Self::reject_raw_word_nested`] covers the slots that are spelled as
    /// part of a type; these are spelled as declarations. Same reason, and the
    /// same non-answer from `unsafe`.
    pub(crate) fn reject_raw_word_storage(ty: &Type, what: &str, name: &str) -> Result<(), Error> {
        if type_mentions_raw_word(ty) {
            return Err(Error::TypeError(format!(
                "{} `{}` is declared `{}`, and a raw C word cannot be stored: the slot tags what \
                 it holds, which would drop the word's top three bits — the same reason this \
                 language has no 64-bit integer type. Keep it in a local inside `(unsafe ...)`, \
                 or convert it (`as i32`, `as bignum`).",
                what,
                name,
                ty
            )));
        }
        Ok(())
    }


    // ---- defun ------------------------------------------------------------


    // ---- monomorphization ---------------------------------------------------



    /// A bare-symbol lambda-list parameter name (`required`/`&rest`).
    fn macro_param_name(v: Value, section: &str) -> Result<SymRef, Error> {
        match v {
            Value::Symbol(id) => Ok(id),
            _ => Err(Error::TypeError(format!("defmacro: {} parameter must be a name", section))),
        }
    }

    /// An `&optional`/`&key` item: either a bare `name` (no default) or a
    /// `(name default-form)` pair. Returns `(name, Some(form)|None)`.
    fn macro_opt_key_spec(heap: &Heap, v: Value, section: &str) -> Result<(SymRef, Option<Value>), Error> {
        match v {
            Value::Symbol(id) => Ok((id, None)),
            Value::Cons(_) => {
                let items = heap.list_to_vec(v)?;
                if items.is_empty() || items.len() > 2 {
                    return Err(Error::TypeError(format!(
                        "defmacro: {} parameter must be `name` or `(name default)`",
                        section
                    )));
                }
                let name = Checker::macro_param_name(items[0], section)?;
                Ok((name, items.get(1).copied()))
            }
            _ => Err(Error::TypeError(format!(
                "defmacro: {} parameter must be `name` or `(name default)`",
                section
            ))),
        }
    }


    // ---- deftrait / impl ---------------------------------------------------




    // ---- module / defmethod / use -----------------------------------------

    /// Move into the module `segs` names from the current one, one child per
    /// segment, and ensure the (possibly empty) module namespace exists,
    /// returning the module as a [`Path`]. Public so a filesystem-aware driver (`crate::project`) can
    /// check a file's forms inside the module its path derives to — the
    /// file's whole content is implicitly wrapped in that module without
    /// synthesizing a `(module ...)` form around the read values. The caller
    /// restores the module it was in, even on a check error in between
    /// ([`Self::check_module`] does it before propagating its body's first
    /// error).
    ///
    /// The root's `keyword` is the keyword namespace (CL's `KEYWORD`
    /// package), which owns every keyword and nothing else, so no module may
    /// take its place.
    pub fn enter_module(&mut self, segs: &[String]) -> Result<Path, Error> {
        if self.info.ns == typelisp_mem::NsId::ROOT && segs.first().is_some_and(|s| s.eq_ignore_ascii_case(typelisp_mem::symbols::KEYWORD_MODULE)) {
            return Err(Error::TypeError(format!(
                "module `{}` is the keyword namespace, which holds the keywords and nothing else — choose another name",
                typelisp_mem::symbols::KEYWORD_MODULE
            )));
        }
        self.info.ns = segs.iter().fold(self.info.ns, |ns, s| typelisp_mem::symbols::child_ns(ns, s));
        self.rebuilt.enter_module(self.info.ns);
        self.publish_name_view();
        Ok(Path::of_module(self.info.ns))
    }

    /// Makes the printer write paths for the module being checked now
    /// (`typelisp_mem::NameView`): its own types, and the ones it has brought
    /// in, by their last segment. Called wherever the current module changes,
    /// and before each form, since a form can bring a type in.
    fn publish_name_view(&self) {
        self.rebuilt.publish_name_view(self.info.ns);
    }

    /// [`Self::enter_module`], plus recording `segs` as the file currently
    /// being loaded's own module path — see [`Info::file_ns`]'s doc comment.
    /// `project.rs` calls this (never [`Self::enter_module`] directly) when
    /// wrapping a file's whole content in the module its location derives
    /// to; a nested `(module ...)` form inside that file still goes through
    /// plain [`Self::enter_module`] via [`Self::check_module`].
    /// The symbol table's module for the namespace forms are checked in now —
    /// the one the next form has to be read in for its names to be this
    /// module's.
    pub fn current_module(&self) -> typelisp_mem::NsId {
        self.info.ns
    }

    pub fn enter_file_module(&mut self, segs: &[String]) -> Result<Path, Error> {
        let base = self.info.ns;
        let path = self.enter_module(segs)?;
        self.info.ns_base.push(base);
        self.info.file_ns.push(self.info.ns);
        Ok(path)
    }

    /// Undo [`Self::enter_file_module`]: return to the module this file was
    /// entered from (an `(in-module ...)` in the file may have moved the
    /// current one anywhere) and drop its `file_ns` frame.
    pub fn exit_file_module(&mut self) {
        self.info.ns = self.info.ns_base.pop().expect("exit_file_module without enter_file_module");
        self.info.file_ns.pop();
        self.publish_name_view();
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
            ns: std::mem::replace(&mut self.info.ns, typelisp_mem::NsId::ROOT),
            file_ns: std::mem::take(&mut self.info.file_ns),
        }
    }

    /// Restore the context saved by [`Self::suspend_ns_context`]. The
    /// in-between load must have left the context empty again (every
    /// `enter_file_module` paired with an `exit_file_module`, even on error
    /// — see `load_source_inner`).
    pub fn resume_ns_context(&mut self, saved: NsContext) {
        debug_assert!(
            self.info.ns == typelisp_mem::NsId::ROOT && self.info.file_ns.is_empty(),
            "dependency load left an unbalanced namespace context"
        );
        self.info.ns = saved.ns;
        self.info.file_ns = saved.file_ns;
        self.publish_name_view();
    }

    /// What a `(use PATH)` written in the module `ns` brings in, registered
    /// in `heap`'s table of names for the forms read after it — the reader's
    /// `use`, once the file behind `path` (if any) is loaded — so they read
    /// each name it brings in as `ns`'s own symbol, the one the import is
    /// recorded under, rather than as whatever the standard library offers
    /// under the same name. Resolved as [`Self::check_use`] resolves it
    /// ([`Self::use_target`]). A path that resolves to nothing is left for
    /// `check_use` to report.
    pub fn register_use_for_reading(&self, heap: &Heap, ns: typelisp_mem::NsId, path: &[typelisp_mem::SymRef]) {
        let site = crate::typeck::use_form::Site { ns, file: self.info.file_ns.last().copied(), policy: self.redef_policy };
        self.rebuilt.register_use_for_reading(heap, site, path)
    }

    // ---- expressions ------------------------------------------------------



    // ---- match / if-let ---------------------------------------------------

    // ---- helpers ----------------------------------------------------------

}

impl Default for Checker {
    fn default() -> Self {
        Checker::new()
    }
}

// ---- free helpers ---------------------------------------------------------

/// The `Sexpr` variant that destructures a literal of `v`'s kind —
/// `check_pattern`'s suggestion when a by-identity literal is written
/// against a `Sexpr` scrutinee. Only the four kinds that reach it (a
/// `Str` literal, and the three boxed numbers) are named; anything else
/// falls back to the variant that binds without destructuring, which is
/// always true and always available.
pub(crate) fn sexpr_variant_for_literal(heap: &Heap, v: Value) -> &'static str {
    match v {
        Value::Str(_) => "str",
        Value::Boxed(id) if heap.is_f64(id) => "f64",
        Value::Boxed(id) if heap.is_f32(id) => "f32",
        Value::Boxed(id) if heap.is_bignum(id) => "int",
        Value::Boxed(id) if heap.is_ratio(id) => "ratio",
        _ => "the sexpr",
    }
}


/// Whether `v` is the pre-interned symbol numbered `konst`.
///
/// Takes the number rather than a name: the syntax words this asks about are
/// pre-interned (`BUILTIN_SYMBOLS`), so the test is an integer comparison and
/// there is no spelling to get wrong.
fn is_symbol(v: Value, konst: u32) -> bool {
    matches!(v, Value::Symbol(id) if id.is(konst))
}

/// `t::0` on a tuple: the field that holds element `0`, or an error naming
/// the tuple's arity when there is no such element. `None` when `ty` is not
/// a tuple or `name` is not an index, so the name is looked up as written.
pub(crate) fn tuple_element_field(ty: &Type, name: &str) -> Result<Option<String>, Error> {
    let Type::Named(path, _) = ty else { return Ok(None) };
    let Some(arity) = crate::types::tuple_arity(path) else { return Ok(None) };
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    match name.parse::<usize>() {
        Ok(i) if i < arity => Ok(Some(crate::types::tuple_field(i))),
        _ => Err(Error::TypeError(format!(
            "`{}` has {} element(s), numbered 0 to {}; there is no element `{}`",
            ty,
            arity,
            arity - 1,
            name
        ))),
    }
}

/// A `match` arm's pattern with every `(:or p1 p2 ...)` in it multiplied
/// out: one pattern per combination of alternatives, in the order the arms
/// they stand for are tried (left to right, the outer alternative varying
/// slowest). A pattern with no `:or` comes back as itself, so it keeps the
/// source positions its cells carry.
///
/// `(:= expr)` and `'datum` hold an expression and a datum, not patterns, so
/// nothing inside them is an alternative. The lists built here are rooted
/// on the heap's stack, which the enclosing top-level check releases.
pub(crate) fn expand_or_patterns(heap: &mut Heap, v: Value) -> Result<Vec<Value>, Error> {
    let Value::Cons(_) = v else { return Ok(vec![v]) };
    let items = heap.list_to_vec(v)?;
    let Some(&head) = items.first() else { return Ok(vec![v]) };
    if is_symbol(head, wk::KW_OR) {
        if items.len() < 2 {
            return Err(Error::TypeError("pattern: `(:or)` needs at least one alternative".into()));
        }
        let mut out = Vec::new();
        for alt in &items[1..] {
            out.extend(expand_or_patterns(heap, *alt)?);
        }
        return Ok(out);
    }
    if is_symbol(head, wk::COLON_EQUALS) || is_symbol(head, wk::QUOTE) {
        return Ok(vec![v]);
    }
    let mut per_item: Vec<Vec<Value>> = vec![vec![head]];
    for item in &items[1..] {
        per_item.push(expand_or_patterns(heap, *item)?);
    }
    if per_item.iter().all(|alts| alts.len() == 1) {
        return Ok(vec![v]);
    }
    let mut combos: Vec<Vec<Value>> = vec![Vec::new()];
    for alts in &per_item {
        combos = combos
            .into_iter()
            .flat_map(|prefix| {
                alts.iter().map(move |a| {
                    let mut c = prefix.clone();
                    c.push(*a);
                    c
                })
            })
            .collect();
    }
    let mut out = Vec::with_capacity(combos.len());
    for c in combos {
        let list = core::list(heap, &c)?;
        heap.push_root(list);
        out.push(list);
    }
    Ok(out)
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
pub(crate) fn as_conversion(from: &Type, to: &Type) -> Option<(&'static str, Option<&'static str>)> {
    use Type::*;
    // Widths do not appear in a conversion method's name: every integer type
    // has the same `int->float`/`int->bignum`/... catalog (they share one
    // runtime representation), and both float types the same `float->int`.
    // So the table is written between *families*, and `check_as` relabels the
    // result to the width that was actually asked for.
    let to_key = called_width(to);
    match (from, &to_key) {
        // A C word can be *read* as a `bignum`, and as nothing else here. It is
        // the only target that never loses anything, which is the whole point:
        // it is how a `size_t` too large for `i32` is read at all (`as i32`
        // truncates and `try-as i32` answers `none`). `f64` would round,
        // `ratio` and `char` are not what a machine word means, and none of
        // the three is registered for these types anyway
        // (`registry::c_word_assoc`). Ahead of the integer arm, which these
        // deliberately fail.
        _ if from.is_c_word() => match &to_key {
            Int => Some(("int->int", None)),
            _ => None,
        },
        _ if from.is_int_family() => match &to_key {
            F64 => Some(("int->float", None)),
            Ratio => Some(("int->ratio", None)),
            Char => Some(("int->char", Some("try-int->char"))),
            _ => None,
        },
        _ if from.is_float() => match &to_key {
            Int => Some(("float->int", None)),
            Ratio => Some(("float->ratio", None)),
            _ => None,
        },
        (Char, Int) => Some(("char->int", None)),
        (Ratio, F64) => Some(("ratio->float", None)),
        (Ratio, Int) => Some(("ratio->int", None)),
        (Ratio, Rational) => Some(("ratio->rational", None)),
        // `as int` truncates, as `ratio`'s does; `try-as int` answers whether
        // the value is an integer (CL's `integerp`).
        (Rational, Int) => Some(("rational->int", Some("try-rational->int"))),
        (Rational, Ratio) => Some(("rational->ratio", None)),
        (Rational, F64) => Some(("rational->float", None)),
        _ => None,
    }
}

/// The width a conversion *into* `ty`'s family is actually registered at:
/// `i32` for every integer, `f64` for both floats, and `ty` itself for
/// everything else. See [`as_conversion`].
pub(crate) fn called_width(ty: &Type) -> Type {
    if ty.is_integer() {
        Type::Int
    } else if ty.is_float() {
        Type::F64
    } else {
        ty.clone()
    }
}


pub(super) fn vtable_form(heap: &mut Heap, slots: &[(Path, SymRef)]) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    for (ty, method) in slots {
        let tp = core::definition_form(ty);
        let one = core::list(f.heap(), &[tp, Value::Symbol(*method)])?;
        f.push(one);
    }
    f.finish_list()
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
pub(crate) fn sexpr_ty() -> Type {
    Type::Named(Path::builtin(typelisp_mem::wk::SEXPR), vec![])
}

/// `Option<Sexpr>` — the type an S-expression datum has once the empty list
/// is `none` (see [`is_option_of_sexpr`]).
pub(crate) fn option_of_sexpr() -> Type {
    Type::Named(Path::builtin(typelisp_mem::wk::OPTION), vec![sexpr_ty()])
}

/// Does an expected type ask for an S-expression?
///
/// Both spellings answer yes. `Option<Sexpr>` is what a datum has, and it is
/// what the implicit widenings should fire on — a `Symbol`, a heap value or a
/// `:dyn` reaching "an S-expression is wanted here" is the same rule
/// whichever of the two the site declared. Bare `Sexpr` still means
/// *non-empty*, and a widening into it is just as sound: nothing these rules
/// produce is the empty list.
pub(crate) fn is_sexpr_expectation(ty: &Type) -> bool {
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
pub(crate) fn is_option_of_sexpr(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, args)
        if crate::types::path_is_builtin(p, typelisp_mem::wk::OPTION)
            && args.len() == 1
            && matches!(&args[0], Type::Named(a, _) if crate::types::path_is_builtin(a, typelisp_mem::wk::SEXPR)))
}

/// The `Sexpr` constructor name that exactly represents a value of `elem_ty`
/// (`Sexpr` itself needs no wrapping — see [`Checker::wrap_rest_elem`]).
/// `&rest`'s declared element type must be one of these (or `Sexpr`) since
/// nothing else has a lossless `Sexpr` encoding to collect into a list with.
pub(crate) fn sexpr_ctor_for(elem_ty: &Type) -> Option<&'static str> {
    match elem_ty {
        // One variant per width, spelled by the type's own name. There is no
        // folding left to do: `Sexpr` has an `i8` and a `u32` because the
        // types do, and a value that arrives here as a `u32` comes back out
        // of the `Sexpr` as a `u32`.
        Type::I8 => Some("i8"),
        Type::I16 => Some("i16"),
        Type::I32 => Some("i32"),
        Type::U8 => Some("u8"),
        Type::U16 => Some("u16"),
        Type::U32 => Some("u32"),
        Type::F32 => Some("f32"),
        Type::F64 => Some("f64"),
        Type::Int => Some("int"),
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
pub(crate) fn trait_operator_method(op: SymRef) -> Option<SymRef> {
    const TABLE: [(&str, &str); 15] = [
        ("+", "add"),
        ("-", "sub"),
        ("*", "mul"),
        ("/", "div"),
        ("rem", "remainder"),
        ("logand", "bit-and"),
        ("logior", "bit-or"),
        ("logxor", "bit-xor"),
        ("lognot", "bit-not"),
        // `Eq`/`Ord` predate the arithmetic traits and already had these four
        // under spelled-out names; the operators reach them the same way.
        ("=", "equals"),
        ("/=", "not-equals"),
        ("<", "less"),
        ("<=", "less-equal"),
        (">", "greater"),
        (">=", "greater-equal"),
    ];
    static MEMBERS: std::sync::OnceLock<Vec<(SymRef, SymRef)>> = std::sync::OnceLock::new();
    let members = MEMBERS.get_or_init(|| {
        TABLE
            .iter()
            .map(|(o, m)| (typelisp_mem::symbols::member_symbol(o), typelisp_mem::symbols::member_symbol(m)))
            .collect()
    });
    members.iter().find(|(o, _)| *o == op).map(|(_, m)| *m)
}




/// Whether `v` is a `(:where ...)` clause (vs. an ordinary body form) —
/// `Checker::check_defun` peeks at this to decide whether to consume it.
pub(crate) fn is_where_clause(heap: &Heap, v: Value) -> Result<bool, Error> {
    Ok(matches!(v, Value::Cons(_)) && heap.list_to_vec(v)?.first().is_some_and(|h| is_symbol(*h, wk::KW_WHERE)))
}

pub(crate) fn parse_macro_lambda_list(heap: &Heap, param_vals: &[Value]) -> Result<MacroLambdaList, Error> {
    // Section rank: required=0, &optional=1, &rest=2, &key=3. A marker may
    // only advance the rank forward, so each appears at most once and in
    // order.
    let mut rank = 0u8;
    let mut required_names: Vec<SymRef> = Vec::new();
    let mut optionals: Vec<(SymRef, Option<Value>)> = Vec::new();
    let mut rest_name: Option<SymRef> = None;
    let mut keys: Vec<(SymRef, Option<Value>)> = Vec::new();

    for p in param_vals {
        if let Value::Symbol(id) = p {
            match id.well_known() {
                wk::OPTIONAL => {
                    if rank >= 1 {
                        return Err(Error::TypeError("defmacro: &optional must precede &rest and &key, and appear once".into()));
                    }
                    rank = 1;
                    continue;
                }
                wk::REST => {
                    if rank >= 2 {
                        return Err(Error::TypeError("defmacro: &rest must precede &key, and appear once".into()));
                    }
                    rank = 2;
                    continue;
                }
                wk::KEY => {
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
            0 => required_names.push(Checker::macro_param_name(*p, "required")?),
            1 => optionals.push(Checker::macro_opt_key_spec(heap, *p, "&optional")?),
            2 => {
                if rest_name.is_some() {
                    return Err(Error::TypeError("defmacro: &rest takes exactly one parameter name".into()));
                }
                rest_name = Some(Checker::macro_param_name(*p, "&rest")?);
            }
            _ => keys.push(Checker::macro_opt_key_spec(heap, *p, "&key")?),
        }
    }
    if rank == 2 && rest_name.is_none() {
        return Err(Error::TypeError("defmacro: &rest must be followed by exactly one parameter name".into()));
    }
    Ok((required_names, optionals, rest_name, keys))
}

