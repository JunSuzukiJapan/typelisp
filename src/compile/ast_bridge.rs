//! Bridges the checker's typed AST ([`Typed`]/[`Expr`]) into a plain `Sexpr`
//! the (typelisp-hosted) compiler body can pattern-match on directly — the
//! same "give the compiler ordinary data, not a Rust API" approach
//! `prelude.rs` already uses for the standard library.
//!
//! Each node becomes a tagged list `(tag . fields...)`, e.g. `Expr::Int(42)`
//! -> `(int 42)`. Only the shapes a current compiler phase actually consumes
//! are translated for real; anything else becomes `(unsupported "<Variant>")`
//! so the compiler body can `panic` with a clear message instead of the
//! bridge silently doing the wrong thing — add a real translation here only
//! once a phase needs to compile that node.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use super::freevars::{labels_free_vars, lambda_free_vars};
use crate::{Arm, Error, Expr, Heap, LabelDef, Path, Pattern, QuotedSexpr, Type, Typed, Value};

/// The five read-only inputs every scoped `translate_*` threads through
/// unchanged, bundled so a call passes one `cx` instead of re-listing all
/// five, and a new scope re-binds only the field that actually changed
/// (`Ctx { direct: &siblings, ..cx }`) rather than restating the rest:
/// - `direct`: names resolving to a direct call — in-scope `labels`
///   siblings/self (see [`ast_to_sexpr_scoped`]/[`translate_apply`]). Empty at
///   the top level.
/// - `outer_captured`: an enclosing `labels` block's captured-name list, so a
///   nested block can forward those values along (see [`translate_labels`]).
/// - `structs`: the type paths the checker resolved to `AdtKind::Struct` (see
///   [`struct_field_kind`]); fixed for a whole translation, never re-bound.
/// - `enums`: the type paths of user `defenum`s (`Interp::enum_defs`'s keys
///   at the only real call site) — pre-resolved plain data like `structs`,
///   keeping this module `Registry`-free. Only [`global_field_kind`]'s
///   kind-`10` classification reads it; fixed for a whole translation.
/// - `globals`: every `Expr::Global`/`Expr::SetGlobal` path this translation
///   may reference, mapped to its already-promoted compiled-global id
///   (`typelisp_rt::global_new`'s return value) — populated by
///   `Interp::add_compiled_function` *before* translation starts (via
///   [`collect_global_targets`]), so every reference this walk reaches is
///   guaranteed present; see [`translate_global`]. Fixed for a whole
///   translation, exactly like `structs`.
///
/// `Copy` so it passes by value freely — it's five shared references.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    direct: &'a HashSet<String>,
    outer_captured: &'a [(String, Type)],
    structs: &'a HashSet<Path>,
    enums: &'a HashSet<Path>,
    globals: &'a HashMap<Path, usize>,
}

/// A process-wide counter for synthesizing unique LLVM symbol names for
/// anonymous functions — every `lambda`/`Expr::FnRef`-forwarding-wrapper/
/// immediately-invoked-lambda gets one (labels/closures Stage 4), since none
/// of those have a name from user source the way a `labels` def or `defun`
/// does. Process-wide (not per-translation-pass or per-outer-function) so
/// two lambdas compiled into the same shared AOT module — even from two
/// *different* `defun`s — can never collide, with no mangling scheme to get
/// right.
static LAMBDA_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_lambda_name(prefix: &str) -> String {
    let n = LAMBDA_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}${}", prefix, n)
}

/// Conses a proper list from `items` (in order), rooting as it goes — the
/// same push/pop discipline `crate::eval::interp::alloc_quoted` uses for
/// `Expr::Quote`, just generalized to N items instead of a fixed two
/// (`car`/`cdr`). The building block both `tagged` (below) and any
/// untagged sub-list (e.g. a `labels` def's parameter-name list) use.
fn list_of(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for item in items.iter().rev() {
        heap.push_root(*item);
        heap.push_root(acc);
        let next = heap.cons(*item, acc);
        heap.pop_root();
        heap.pop_root();
        acc = next?;
    }
    Ok(acc)
}

/// Conses `tag` onto a list built from `items` — see [`list_of`].
fn tagged(heap: &mut Heap, tag: &str, items: &[Value]) -> Result<Value, Error> {
    let list = list_of(heap, items)?;
    let tag_sym = heap.intern_symbol(tag);
    heap.push_root(tag_sym);
    heap.push_root(list);
    let result = heap.cons(tag_sym, list);
    heap.pop_root();
    heap.pop_root();
    result
}

/// A user-defined free function/`labels` sibling's own LLVM symbol name —
/// see [`crate::compile::USER_SYMBOL_PREFIX`]'s doc comment for why this is
/// never `logical_name` unprefixed. The single place [`translate_call`]/
/// [`translate_fnref`]'s embedded `(call ...)`-node name string is built,
/// so it always agrees with whatever `Interp::compile_function_rec`/
/// `compile::aot` declare/wire the *actual* LLVM function under.
pub(crate) fn user_symbol_name(logical_name: &str) -> String {
    format!("{}{}", crate::compile::USER_SYMBOL_PREFIX, logical_name)
}

/// A user-defined method's own LLVM symbol name — the `Expr::Assoc`
/// counterpart of [`user_symbol_name`]. `compiler.rs`'s `compile-assoc-user`
/// mangles `type-name`/`method` back into this exact same `tl_type::method`
/// string (its own `(append "tl_" (append type-name (append "::" method)))`)
/// before its `get-function` lookup, so this must stay in lockstep with
/// that — see [`crate::compile::USER_SYMBOL_PREFIX`]'s doc comment.
pub(crate) fn user_method_symbol_name(type_name: &Path, method: &str) -> String {
    format!("{}{}::{}", crate::compile::USER_SYMBOL_PREFIX, type_name.local(), method)
}

/// Builds a `(str (int c0) (int c1) ...)` node for a compile-time-known
/// host `&str` — [`ast_to_sexpr_scoped`]'s `Expr::Str` arm and
/// [`translate_construct`]'s `type-name-str` header field (Stage 3 of the
/// Sexpr/RtValue unification plan, `docs/implementation-log.md`) both need
/// this. Not a pre-allocated `Value::Str`: a literal's content is entirely
/// known at compile time, but the `StrId` a compile-time `Heap::alloc_string`
/// would produce here is meaningless to the *target* program — the JIT's own
/// running `Heap` outlives this call, but AOT's compiled executable
/// allocates its own `Heap` from scratch at startup (`rt_heap_init`), with no
/// `StrId` table shared with this one at all. So each character becomes an
/// ordinary `(int c)` node instead, letting `compiler.rs`'s `compile-str`
/// re-embed the content as `const-i64` operands and rebuild the string at
/// *run* time via `rt_str_new`.
fn str_literal_form(heap: &mut Heap, s: &str) -> Result<Value, Error> {
    let mut char_nodes = Vec::with_capacity(s.chars().count());
    for c in s.chars() {
        let node = tagged(heap, "int", &[Value::Int(c as i64)])?;
        heap.push_root(node);
        char_nodes.push(node);
    }
    let result = tagged(heap, "str", &char_nodes);
    for _ in 0..char_nodes.len() {
        heap.pop_root();
    }
    result
}

/// Builds a `(bignum (int sign) (int d0) (int d1) ...)` node for a
/// compile-time-known `BigInt` — [`ast_to_sexpr_scoped`]'s `Expr::Bignum` arm
/// and [`ratio_literal_form`] (for a ratio literal's numerator/denominator)
/// both need this. Same reasoning as [`str_literal_form`] for why this isn't
/// a pre-allocated `Value`: a `BigInt`'s digits are compile-time-known, but a
/// compile-time `Heap::alloc_bignum`'s `BoxId` would be meaningless to the
/// *target* program (AOT's compiled executable has its own, separate `Heap`
/// with no shared box table). `sign` (`-1`/`0`/`1`) and each base-2^32 digit
/// (`BigInt::to_u32_digits`, least-significant first) become their own `int`
/// node, mirroring `rt_bignum_new`'s own "argc-many raw payload scalars"
/// convention (`compiler.rs`'s `compile-bignum-literal` builds the args array
/// from exactly these nodes).
fn bignum_literal_form(heap: &mut Heap, n: &num_bigint::BigInt) -> Result<Value, Error> {
    let (sign, digits) = n.to_u32_digits();
    let sign_val = match sign {
        num_bigint::Sign::Minus => -1,
        num_bigint::Sign::NoSign => 0,
        num_bigint::Sign::Plus => 1,
    };
    let mut nodes = Vec::with_capacity(digits.len() + 1);
    let sign_node = tagged(heap, "int", &[Value::Int(sign_val)])?;
    heap.push_root(sign_node);
    nodes.push(sign_node);
    for d in digits {
        let node = tagged(heap, "int", &[Value::Int(d as i64)])?;
        heap.push_root(node);
        nodes.push(node);
    }
    let result = tagged(heap, "bignum", &nodes);
    for _ in 0..nodes.len() {
        heap.pop_root();
    }
    result
}

/// Builds a `(ratio numer-form denom-form)` node for a compile-time-known
/// `BigRational` — [`ast_to_sexpr_scoped`]'s `Expr::Ratio` arm. Each of
/// `numer()`/`denom()` becomes its own nested [`bignum_literal_form`]
/// (`compiler.rs`'s `compile-ratio-literal` compiles both, then calls
/// `rt_ratio_from_bignums`).
fn ratio_literal_form(heap: &mut Heap, r: &num_rational::BigRational) -> Result<Value, Error> {
    let numer_form = bignum_literal_form(heap, r.numer())?;
    heap.push_root(numer_form);
    let denom_form = match bignum_literal_form(heap, r.denom()) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(denom_form);
    let result = tagged(heap, "ratio", &[numer_form, denom_form]);
    heap.pop_root();
    heap.pop_root();
    result
}

/// `binding_kind`'s three possible results — see that function's doc
/// comment. Kept as plain `i64` constants (not a Rust enum) since the only
/// thing that ever consumes one is `compiler.rs`, as a `Sexpr` `Int`.
const KIND_PLAIN: i64 = 0;
const KIND_FN: i64 = 1;
const KIND_SEXPR: i64 = 2;

/// Whether `ty` is the built-in `Sexpr` type — shared by every place that
/// needs to single it out specifically: [`translate_match`] (a `Sexpr`
/// scrutinee is [`MATCH_KIND_SEXPR`]), [`translate_construct`] (`Sexpr`'s own
/// variants compile to the tagged-`i64`/`rt_cons` representation Stage
/// 2/3/4/5 already built, every *other* ADT compiles to a freshly
/// `malloc`'d box instead — Stage 6 of the Sexpr-representation plan,
/// `docs/implementation-log.md`), and [`binding_kind`].
fn is_sexpr_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(p, _) if *p == Path::root("sexpr"))
}

/// [`translate_match`]/[`pattern_to_sexpr`]'s scrutinee-representation tags
/// (`compiler.rs`'s `compile-match`/`compile-pattern-test` read the same
/// numbering back): a tagged `Sexpr` (the tagged-`i64` bit tests
/// `compile-sexpr-tag-test`/`compile-sexpr-field` do — needs a GC root of
/// its own, since it's an ordinary heap-managed value); a sum-ADT box
/// (`Option`/`Result`/a `defenum`, `compile-construct-box`'s raw `malloc`'d
/// array tested by `compile-box-tag-test`/`compile-box-field` — never
/// GC-managed, so needs no root); a boxed struct (`defstruct`/`Vector`,
/// properly tagged like a `Sexpr` — needs the same root — but single-variant,
/// so no tag test is ever emitted, only per-field extraction via
/// `compile-struct-field`).
const MATCH_KIND_SEXPR: i64 = 0;
const MATCH_KIND_BOX: i64 = 1;
const MATCH_KIND_STRUCT: i64 = 2;

/// [`MATCH_KIND_SEXPR`]/[`MATCH_KIND_BOX`]/[`MATCH_KIND_STRUCT`] for a
/// constructor pattern's/scrutinee's own type path — shared by
/// [`translate_match`] (via [`match_scrut_kind`]) and [`pattern_to_sexpr`]'s
/// `Pattern::Ctor` arm, so a nested sub-pattern's own type gets the same
/// classification as a top-level scrutinee of that same type.
fn match_kind_for_path(p: &Path, cx: Ctx) -> i64 {
    if *p == Path::root("sexpr") {
        MATCH_KIND_SEXPR
    } else if cx.structs.contains(p) {
        MATCH_KIND_STRUCT
    } else {
        MATCH_KIND_BOX
    }
}

/// [`match_kind_for_path`] over a scrutinee's full static `Type` — `None`
/// for a type that's never a `match` scrutinee in practice (anything but
/// `Type::Named`), which [`translate_match`] turns into `unsupported`.
fn match_scrut_kind(ty: &Type, cx: Ctx) -> Option<i64> {
    match ty {
        Type::Named(p, _) => Some(match_kind_for_path(p, cx)),
        _ => None,
    }
}

/// Classifies a bound name's (or `let` binding's) static type for the
/// per-binding bookkeeping `compiler.rs` must do at the point such a value
/// crosses a binding boundary (a function's own parameters/captures, a
/// `let` binding): a `Fn`-typed one needs `ClosureBox` retain/release
/// (labels/closures Stage 4); a `Sexpr`-typed *or* `Str`-typed one needs
/// GC-root push/pop instead (Stage 6 of the Sexpr-representation plan,
/// `docs/implementation-log.md` — the "Sexprルート挿入パス"; `Str` joins it
/// in Stage 7) — a tagged `i64` that may point into the GC-managed cons/
/// string heap, never something `build-closure-retain`/`build-closure-release`
/// could safely touch; anything else needs neither. `Str` reuses `KIND_SEXPR`
/// rather than a fourth tag of its own because a bare `Type::Str` value's
/// compiled representation *is* the exact same tagged immediate a
/// `Sexpr::Str` is (`compiler.rs`'s `compile-sexpr-field`/
/// `compile-construct-sexpr` doc comments) — the same `rt_push_sexpr_root`/
/// `rt_pop_sexpr_root` calls this drives already protect it correctly with
/// no changes of their own. A single `kind` tag (rather than independent
/// booleans) keeps the cases mutually exclusive by construction, the same
/// way a `Typed` node has exactly one static type — see `compiler.rs`'s
/// `retain-bindings`/`release-bindings`/`bind-let-values`/`restore-let-values`.
fn binding_kind(ty: &Type) -> i64 {
    if matches!(ty, Type::Fn(..)) {
        KIND_FN
    } else if is_sexpr_type(ty) || matches!(ty, Type::Str | Type::Bignum | Type::Ratio) {
        // `bignum`/`ratio` join `Str` here for the same reason
        // `struct_field_kind` gives them its own passthrough kind `6`: a
        // bare `Type::Bignum`/`Type::Ratio` value's compiled representation
        // *is* the tagged `TAG_BOXED` pointer a boxed `Sexpr::Bignum`/
        // `Ratio` already is, so it needs the exact same GC-root push/pop
        // protection across a binding boundary — leaving it `KIND_PLAIN`
        // would silently drop the GC root on a value that's actually a
        // live heap pointer.
        KIND_SEXPR
    } else {
        KIND_PLAIN
    }
}

/// Builds a `Sexpr` list of `(name . kind)` pairs from typed
/// parameter/captured names — every such list needs this; see
/// [`binding_kind`] for what `kind` distinguishes and why (Stage 6
/// generalized this from a plain `is-fn` `Bool` to a 3-way `Int` tag).
/// `pub(crate)`: [`crate::eval::Interp::add_compiled_function`] needs this
/// exact same construction for a top-level `defun`'s own parameter list —
/// sharing it (rather than that method re-deriving its own, now-stale
/// `Bool`-tagged copy) is what keeps the two from desyncing the way they
/// did when Stage 6 first generalized this tag.
pub(crate) fn tagged_sym_list(heap: &mut Heap, names: &[(String, Type)]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for (n, ty) in names.iter().rev() {
        let sym = heap.intern_symbol(n);
        let kind = Value::Int(binding_kind(ty));
        heap.push_root(sym);
        let pair = heap.cons(sym, kind);
        heap.pop_root();
        let pair = pair?;
        heap.push_root(pair);
        heap.push_root(acc);
        let next = heap.cons(pair, acc);
        heap.pop_root();
        heap.pop_root();
        acc = next?;
    }
    Ok(acc)
}

/// Classifies a `defstruct`/`Vector<T>`/`cons-cell<K,V>` field's static type
/// for the tagged-`Sexpr` encode/decode `compile-construct-boxed-struct`/
/// `compile-field-get`/`compile-field-set` must do at the `BoxedObj::Struct`
/// boundary (Stage 3 of the Sexpr/RtValue unification plan,
/// `docs/implementation-log.md`) — every field, not just a `Sexpr`/
/// `Str`-typed one, crosses that boundary as a properly tagged `i64`
/// (`rt_struct_new`'s own `decode()` call on each argument), unlike a
/// general-ADT box's untagged raw slots ([`binding_kind`]'s `KIND_PLAIN`).
/// Reuses `Sexpr`'s own variant numbering (`registry::sexpr_def`'s `1`=int
/// `2`=float `3`=char `4`=bool `6`=str) rather than inventing a parallel
/// scheme, so `compiler.rs`'s `compile-sexpr-field` (decode) and a
/// symmetrical `compile-tag-struct-field` (encode) can reuse that same
/// per-variant bit manipulation verbatim instead of duplicating it. A
/// `Type::Named` field type splits on `structs` — the set of type paths the
/// checker resolved to `AdtKind::Struct` (the exact same check-time fact
/// `Expr::Construct`'s `mutable` field carries per construct site, handed in
/// as plain pre-resolved data so this module still needs no live `Registry`):
/// a nested `defstruct`/`Vector<T>`/`cons-cell<K,V>` field is itself already
/// a boxed-struct-tagged `Sexpr` value, so it joins `Str`/`Sexpr`'s
/// passthrough kind `6` (both `compile-tag-struct-field` and
/// `compile-sexpr-field` leave a kind-`6` value untouched — exactly right
/// for a value that is already properly tagged). `0` (otherwise `nil`, never
/// a valid field type) marks the types with a genuine representation gap
/// left: `Type::Fn` (no `ClosureBox` integration with a GC-tracked struct
/// field yet — Closure unification, Stage 6a-6b), a general-ADT `Option`/
/// `Result`/user sum type (an untagged raw `malloc`'d pointer that must
/// *not* flow into `rt_struct_new` unconverted — it has no GC-heap
/// representation until general ADTs are themselves boxed; a *global* of
/// such a type is fine, via [`global_field_kind`]'s kind-`10` permanent-
/// root path), and a still-generic type variable (a `defstruct`'s own `T`
/// field compiled from the generic definition — resolving it would take
/// monomorphized compilation, not more type information at this site). A clear compile-time panic
/// (`compile-tag-struct-field`/`compile-sexpr-field`'s own existing "not
/// representable yet" message) is the accepted result for those, the same
/// as a `Sym` field in a `Sexpr` construct.
fn struct_field_kind(ty: &Type, structs: &HashSet<Path>) -> i64 {
    match ty {
        _ if ty.is_integer() => 1,
        _ if ty.is_float() => 2,
        Type::Char => 3,
        Type::Bool => 4,
        Type::Str => 6,
        // A `Symbol` value is an already-tagged immediate (an interned
        // `Value::Symbol`), the same passthrough case as `Str`/`Sexpr`.
        Type::Symbol => 6,
        // `bignum`/`ratio` are always a tagged `TAG_BOXED` pointer at a
        // `BoxedObj::Bignum`/`Ratio` (no fixed-width native form the way
        // `f64` has — see `crates/typelisp-rt/src/lib.rs`'s "bignum/ratio
        // compiled representation" section), so they join `Str`/`Symbol`'s
        // passthrough kind `6` rather than needing a tag of their own.
        Type::Bignum | Type::Ratio => 6,
        _ if is_sexpr_type(ty) => 6,
        Type::Named(p, _) if structs.contains(p) => 6,
        _ => 0,
    }
}

/// [`struct_field_kind`]'s counterpart for a `defvar`'s own declared type —
/// used only by [`translate_global`]/[`translate_set_global`]/
/// [`ast_to_sexpr_for_global_init`], never for an ordinary `defstruct`
/// field (which stays on plain `struct_field_kind`, still `0`/unsupported
/// for `Option`/`Result` — see that function's own doc comment on why:
/// a *field*'s raw box pointer has nowhere GC-safe to live inside another
/// heap-managed struct, but a *global*'s permanent-root slot is exactly
/// that safe home, via `Interp::promote_global`'s `Value::Int(ptr >> 3)`
/// encoding). Kind `10` — not `struct_field_kind`'s own `0`/unsupported —
/// tells `compiler.rs`'s `compile-global`/`compile-set-global`/
/// `compile-global-init` to take that same shift-tagged path instead of
/// panicking. Covers `Option`/`Result` and every user `defenum` in `enums`
/// ([`Ctx::enums`]) — the kind-`10` path is variant-agnostic (a
/// shift-tagged raw box pointer, whatever its layout), so the same
/// compiled code handles all three; the layout-aware half lives on the
/// Rust side (`Interp`'s `data_variant_field_types`).
fn global_field_kind(ty: &Type, structs: &HashSet<Path>, enums: &HashSet<Path>) -> i64 {
    match ty {
        Type::Named(p, args) if p.local() == "option" && p.is_simple() && args.len() == 1 => 10,
        Type::Named(p, args) if p.local() == "result" && p.is_simple() && args.len() == 2 => 10,
        Type::Named(p, _) if enums.contains(p) => 10,
        other => struct_field_kind(other, structs),
    }
}

/// `Vector<T>`'s builtin methods that have no compiled `defmethod` body and so
/// must be lowered to a dedicated `vector-op` node (`translate_vector_method`)
/// rather than routed through the generic `assoc` path. Deliberately *not*
/// `iter`: that one is a real prelude `defmethod` (`vector-iter::new`), a
/// normal compiled method, so it stays on the `assoc` path. `HashTable<K,V>`
/// shares the `new`/`get`/`set` names but a different `type_name`, so the
/// `type_name.local() == "vector"` guard at the call site keeps them apart.
const VECTOR_BUILTIN_METHODS: [&str; 5] = ["new", "get", "set", "len", "push"];

/// The element `kind` ([`struct_field_kind`]) for a `Vector<T>` method call:
/// `T` from the receiver's `Vector<T>` type (`args[0]`), or `0` for `new`
/// (no receiver — an empty struct has no element to tag). Uniform across
/// `get`/`set`/`len`/`push` since every one either reads or writes a `T`.
fn vector_element_kind(args: &[Typed], structs: &HashSet<Path>) -> i64 {
    match args.first().map(|a| &a.ty) {
        Some(Type::Named(p, targs)) if p.local() == "vector" => {
            targs.first().map(|t| struct_field_kind(t, structs)).unwrap_or(0)
        }
        _ => 0,
    }
}

/// A `Vector<T>` builtin method call -> a dedicated `(vector-op method kind
/// arg-form...)` node, lowered directly to `rt_struct_new` (`new`) /
/// `rt_struct_field_count` (`len`) / `rt_struct_field_get` (`get`) /
/// `rt_struct_field_set` (`set`) / `rt_struct_push_field` (`push`) in
/// `compiler.rs`'s `compile-vector-op`, because none of these has a compiled
/// `defmethod` body. `kind` is `T`'s [`struct_field_kind`]: the element
/// crosses the `BoxedObj::Struct` boundary tagged, so `compile-vector-op`
/// tags (`push`/`set`) and untags (`get`) it with `compile-tag-struct-field`/
/// `compile-sexpr-field`, exactly as `field-get`/`field-set` do for a
/// fixed-arity field. `new` carries a `(str "vector")` type-name form
/// ([`str_literal_form`]) — the sole "argument" it needs, reusing
/// `compile-construct-boxed-struct`'s empty-field path — while the others
/// translate their receiver/index/value operands as plain value forms
/// ([`ast_list_to_sexpr`]); the one header `kind` covers the sole field that
/// crosses the boundary, so no per-arg `(kind . form)` pairing is needed.
fn translate_vector_method(heap: &mut Heap, method: &str, kind: i64, args: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let kind_v = Value::Int(kind);
    let method_v = heap.alloc_string(method.to_string());
    heap.push_root(method_v);
    let forms = if method == "new" {
        // `new` has no runtime operands; its lone "form" is the `"vector"`
        // type-name literal `compile-construct-boxed-struct` feeds
        // `rt_struct_new` (an empty field list builds an empty vector).
        match str_literal_form(heap, "vector") {
            Ok(form) => {
                heap.push_root(form);
                vec![form]
            }
            Err(e) => {
                heap.pop_root(); // method_v
                return Err(e);
            }
        }
    } else {
        match ast_list_to_sexpr(heap, args, cx) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // method_v
                return Err(e);
            }
        }
    };
    let mut items = vec![method_v, kind_v];
    items.extend(forms.iter().copied());
    let result = tagged(heap, "vector-op", &items);
    for _ in 0..forms.len() {
        heap.pop_root();
    }
    heap.pop_root(); // method_v
    result
}

/// `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op` node
/// (`translate_hashtable_method` -> the `rt_hashtable_*` family) rather than
/// the generic `assoc` path — every one except `iter` (a genuine prelude
/// `defmethod`, `hashtable-iter::new`). `get`/`remove` return `Option<V>`, a
/// `malloc`'d sum-ADT box built directly by `compiler.rs`'s
/// `compile-hashtable-op` (`rt_hashtable_contains` + `rt_hashtable_get_raw`/
/// `rt_hashtable_remove_raw`, decoded per `V`'s `struct_field_kind` the same
/// way a `BoxedObj::Struct` field read already is) rather than through the
/// ordinary `Expr::Construct` path (there is no *source* `Option::some`/
/// `none` call site here to translate — the box is synthesized straight from
/// the runtime lookup's found/not-found outcome).
const HASHTABLE_BUILTIN_METHODS: [&str; 9] = ["new", "set", "get", "remove", "count", "clear", "keys", "values", "entries"];

/// The `(K-kind, V-kind)` ([`struct_field_kind`]) for a `HashTable<K,V>`
/// method call — from the receiver's `HashTable<K,V>` type (`args[0]`), or the
/// method's `HashTable<K,V>` return type for `new` (no receiver). Only `set`
/// actually consults them (to tag its key/value at the map boundary); the
/// enumerators/`count`/`clear` pass `0`.
fn hashtable_kv_kinds(args: &[Typed], ty: &Type, structs: &HashSet<Path>) -> (i64, i64) {
    let ht_ty = args.first().map(|a| &a.ty).unwrap_or(ty);
    match ht_ty {
        Type::Named(p, targs) if p.local() == "hashtable" && targs.len() == 2 => {
            (struct_field_kind(&targs[0], structs), struct_field_kind(&targs[1], structs))
        }
        _ => (0, 0),
    }
}

/// A `HashTable<K,V>` builtin method call -> `(hashtable-op method key-kind
/// val-kind operand-form...)`, the map counterpart of
/// [`translate_vector_method`]. `new` carries no operands (an empty map);
/// every other method translates its receiver (and, for `set`, key/value)
/// operands as plain value forms. See [`compile-hashtable-op`] in
/// `compiler.rs` for the per-method lowering.
fn translate_hashtable_method(
    heap: &mut Heap,
    method: &str,
    key_kind: i64,
    val_kind: i64,
    args: &[Typed],
    cx: Ctx,
) -> Result<Value, Error> {
    let method_v = heap.alloc_string(method.to_string());
    heap.push_root(method_v);
    let forms = if method == "new" {
        Vec::new()
    } else {
        match ast_list_to_sexpr(heap, args, cx) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // method_v
                return Err(e);
            }
        }
    };
    let mut items = vec![method_v, Value::Int(key_kind), Value::Int(val_kind)];
    items.extend(forms.iter().copied());
    let result = tagged(heap, "hashtable-op", &items);
    for _ in 0..forms.len() {
        heap.pop_root();
    }
    heap.pop_root(); // method_v
    result
}

/// `(unsupported "<Variant>")` — see this module's doc comment.
fn unsupported(heap: &mut Heap, variant: &str) -> Result<Value, Error> {
    let name = heap.alloc_string(variant.to_string());
    tagged(heap, "unsupported", &[name])
}

/// Translates each of `items` in order, rooting every translated `Value` as
/// it goes (so an earlier sibling survives a later sibling's own `heap.cons`
/// calls — the same concern `tagged` has for its own `items`, just one level
/// up). On success every value in the returned `Vec` is left rooted; the
/// caller must pop exactly that many roots once it's done embedding them in
/// whatever it builds next (e.g. a body sequence — see `translate_lambda`'s
/// arm below). On error, pops everything pushed so far before propagating.
/// `cx` is threaded through unchanged — see [`Ctx`]/[`ast_to_sexpr_scoped`]'s
/// doc comments.
fn ast_list_to_sexpr(heap: &mut Heap, items: &[Typed], cx: Ctx) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        match ast_to_sexpr_scoped(heap, item, cx) {
            Ok(v) => {
                heap.push_root(v);
                values.push(v);
            }
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        }
    }
    Ok(values)
}

/// The call-argument counterpart of [`ast_list_to_sexpr`]: each translated
/// form is wrapped as `(kind . form)` rather than left bare — `apply`/
/// `apply-indirect`/`call`/`assoc`'s argument lists need this (a method's
/// receiver/arguments, like an ordinary call's, can be `Fn`- or
/// `Sexpr`-typed, unlike `i64`/`i32`'s own built-in arithmetic operands,
/// always plain `i64`s, which is why `compile-assoc`'s arithmetic branch
/// alone still only needs to unwrap the tag, never act on it).
/// `compile-call-args` reads `kind` to decide whether a given argument's
/// value needs the automatic `ClosureBox` retain/release treatment (`kind =
/// 1`) or a `push-sexpr-root` (`kind = 2`, Stage 8 of the
/// Sexpr-representation plan, `docs/implementation-log.md` — a call argument is exactly
/// the kind of fresh, unnamed temporary the "Sexprルート挿入パス" left
/// unprotected: computing a *later* argument could allocate and reclaim an
/// *earlier* one's still-unrooted cons cell before the call ever happens).
/// A plain `Bool` tag (`is-fn`) before this stage — generalized the same way
/// [`tagged_sym_list`]'s own pair shape was in Stage 6, reusing
/// [`binding_kind`] rather than re-deriving the same 3-way classification a
/// third time.
fn tagged_ast_list_to_sexpr(heap: &mut Heap, items: &[Typed], cx: Ctx) -> Result<Vec<Value>, Error> {
    tagged_ast_list_to_sexpr_with(heap, items, cx, binding_kind)
}

/// [`translate_construct`]'s `mutable` (`defstruct`/`Vector<T>`/
/// `cons-cell<K,V>`) branch counterpart of [`tagged_ast_list_to_sexpr`] —
/// identical `(kind . form)` tagging, just classified by [`struct_field_kind`]
/// instead of [`binding_kind`] (Stage 3 of the Sexpr/RtValue unification
/// plan, `docs/implementation-log.md`): a `BoxedObj::Struct` field's encode
/// needs to know exactly *which* tagged-`Sexpr` shape to build, not merely
/// whether it needs GC-root/`ClosureBox` bookkeeping.
fn struct_field_ast_list_to_sexpr(heap: &mut Heap, items: &[Typed], cx: Ctx) -> Result<Vec<Value>, Error> {
    let structs = cx.structs;
    tagged_ast_list_to_sexpr_with(heap, items, cx, move |ty| struct_field_kind(ty, structs))
}

/// Shared by [`tagged_ast_list_to_sexpr`]/[`struct_field_ast_list_to_sexpr`]
/// — see either's doc comment for the two `kind_fn` classifiers this is
/// parameterized over.
fn tagged_ast_list_to_sexpr_with(
    heap: &mut Heap,
    items: &[Typed],
    cx: Ctx,
    kind_fn: impl Fn(&Type) -> i64,
) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let form = match ast_to_sexpr_scoped(heap, item, cx) {
            Ok(v) => v,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        let kind = Value::Int(kind_fn(&item.ty));
        heap.push_root(form);
        let pair = heap.cons(kind, form);
        heap.pop_root();
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(pair);
        values.push(pair);
    }
    Ok(values)
}

/// Translates one typed AST node. See the module doc comment for the tagged
/// shape and which variants are real vs. `unsupported` placeholders today.
/// `structs` is the set of type paths the checker resolved to
/// `AdtKind::Struct` (`Interp::struct_types` at the only real call site) —
/// see [`struct_field_kind`] for the one classification it drives. `globals`
/// is every global variable this translation may reference, already
/// promoted to a compiled-global id — see [`Ctx`]'s doc comment and
/// [`collect_global_targets`].
pub fn ast_to_sexpr(
    heap: &mut Heap,
    typed: &Typed,
    structs: &HashSet<Path>,
    enums: &HashSet<Path>,
    globals: &HashMap<Path, usize>,
) -> Result<Value, Error> {
    let direct = HashSet::new();
    let cx = Ctx { direct: &direct, outer_captured: &[], structs, enums, globals };
    ast_to_sexpr_scoped(heap, typed, cx)
}

/// Translates a `defvar`'s initializer expression `value` for AOT's
/// synthesized global-init sequence (`Interp::add_compiled_global_init`,
/// `compile::aot`): `(global-init kind value-form)`, `compiler.rs`'s
/// `compile-global-init` tag. `kind` is [`struct_field_kind`]'s
/// classification of the global's own declared type (`value.ty`) — the
/// same tagging step [`translate_set_global`] uses, since both end up
/// calling a `rt_global_*` function that expects a properly tagged
/// `Sexpr` payload (`rt_global_new` here, vs. `rt_global_set` there).
/// Unlike an ordinary function body, there is no separate `id` to look up
/// in `globals`/[`Ctx::globals`] for *this* node itself — `rt_global_new`
/// assigns the id at the call site, by call order (see that function's
/// doc comment) — though `value` may itself reference *other* globals,
/// which is exactly what `globals` is still for.
pub fn ast_to_sexpr_for_global_init(
    heap: &mut Heap,
    value: &Typed,
    structs: &HashSet<Path>,
    enums: &HashSet<Path>,
    globals: &HashMap<Path, usize>,
) -> Result<Value, Error> {
    let direct = HashSet::new();
    let cx = Ctx { direct: &direct, outer_captured: &[], structs, enums, globals };
    let kind = Value::Int(global_field_kind(&value.ty, structs, enums));
    let form = ast_to_sexpr_scoped(heap, value, cx)?;
    heap.push_root(form);
    let result = tagged(heap, "global-init", &[kind, form]);
    heap.pop_root(); // form
    result
}

/// `direct` is the set of names that resolve to a direct call rather than
/// (in a later phase) a boxed/indirect one: currently in-scope `labels`
/// siblings, including the def being compiled itself (self-recursion) —
/// see [`translate_apply`]. Empty at the top level (a `defun`'s own body
/// has no such names; they only come into existence inside a `labels`
/// form, see [`translate_labels`]).
fn ast_to_sexpr_scoped(heap: &mut Heap, typed: &Typed, cx: Ctx) -> Result<Value, Error> {
    match &typed.expr {
        Expr::Int(n) => tagged(heap, "int", &[Value::Int(*n)]),
        // The `f64`'s raw bit pattern (`f64::to_bits`), *not* a pre-boxed
        // `Value::Boxed` — same reason `Expr::Str` embeds raw characters
        // instead of a pre-allocated `Value::Str` below: a compile-time
        // `Heap::alloc_float`'s `BoxId` would be meaningless to the *target*
        // program (AOT's compiled executable allocates its own fresh `Heap`
        // at startup, with no boxed-object table shared with this one).
        // `compiler.rs`'s `compile-float` boxes it for real at IR-build
        // time, via a fresh `rt_float_new` call in the compiled function
        // itself — mirroring `compile-str`'s own `rt_str_new` call.
        Expr::Float(f) => tagged(heap, "float", &[Value::Int(f.to_bits() as i64)]),
        // `(bignum (int sign) (int d0) ...)`/`(ratio numer-form denom-form)`
        // — see `bignum_literal_form`/`ratio_literal_form`'s doc comments.
        // `compiler.rs`'s `compile-bignum-literal`/`compile-ratio-literal`
        // lower these to `rt_bignum_new`/`rt_ratio_from_bignums` calls.
        Expr::Bignum(n) => bignum_literal_form(heap, n),
        Expr::Ratio(r) => ratio_literal_form(heap, r),
        Expr::Bool(b) => tagged(heap, "bool", &[Value::Bool(*b)]),
        Expr::Char(c) => tagged(heap, "char", &[Value::Char(*c)]),
        // `(str (int c0) (int c1) ...)` — Stage 7 of the Sexpr-representation
        // plan (`docs/implementation-log.md`). See [`str_literal_form`]'s doc
        // comment for why not a pre-allocated `Value::Str`.
        Expr::Str(s) => str_literal_form(heap, s),
        Expr::Unit => tagged(heap, "unit", &[]),
        // Always `(var name)`, even when `name` is a currently in-scope
        // `labels` sibling/self (`direct.contains(name)`) — that only
        // matters to [`translate_apply`], which intercepts the *callee*
        // position of an `Expr::Apply` before a bare `Expr::Var` node for
        // that name would ever reach here. A sibling *referenced as a
        // value* rather than called (e.g. a nested `lambda` capturing it, or
        // a `labels` block's trailing body returning it bare) still becomes
        // this same plain `(var name)` — deliberately: the "is this an
        // ordinary value or a sibling that needs boxing into a `ClosureBox`"
        // judgment is made entirely at compile time instead, by
        // `compiler.rs`'s `resolve-value` (a follow-up to labels/closures
        // Stage 4) — see that function's doc comment for why doing it there
        // (where the relevant `fn-env` is still in scope) rather than here
        // avoids colliding with `compile-lambda`'s own, separate decision to
        // always give an escaping `lambda`'s body a *fresh* `fn-env`.
        Expr::Var(name) => {
            let v = heap.alloc_string(name.clone());
            let is_fn = Value::Bool(matches!(typed.ty, Type::Fn(..)));
            tagged(heap, "var", &[v, is_fn])
        }
        // `(assoc type-name method instance arg...)` — a fixed 3-field
        // header (both strings, then the receiver flag) followed by the
        // translated argument list. `compile_value` only matters about
        // `Expr::Var`'s `name`/`Expr::Assoc`'s `type_name`/`method` as
        // plain text, so they're translated as `Str`s like `Expr::Str`
        // (rather than e.g. interned symbols) — there's no reason for the
        // compiler body to treat them differently from any other string.
        // `type_name` is encoded by its *local* (unqualified) segment —
        // matching `Interp::method_key`'s own "drop qualification, match
        // local name only" convention — since `compiler.rs`'s
        // `compile-assoc` mangles it back into a `type-name::method`
        // lookup name that must agree with the literal string a standalone
        // `(compile "type-name::method")` call used as that method's own
        // LLVM function name (see `compile-assoc`'s doc comment). Arguments
        // are tagged via [`tagged_ast_list_to_sexpr`] rather than the
        // untagged [`ast_list_to_sexpr`]: unlike the built-in `i64`/`i32`
        // arithmetic operands (always plain `i64`s), a user-defined
        // method's receiver/arguments can be `Fn`- or `Sexpr`-typed and need
        // the same `compile-call-args` retain/GC-root treatment an ordinary
        // call's arguments already get.
        Expr::Assoc { type_name, method, instance, args } => {
            // `Vector<T>`'s field-backed builtin methods (`new`/`get`/`set`/
            // `len`/`push`) have no compiled `defmethod` body; lower them to a
            // dedicated `vector-op` node carrying the element kind (see
            // `translate_vector_method`). Guarded by `type_name` so
            // `HashTable::get`/`set`/`new` (a different type, different
            // runtime) and `Vector`'s own `iter` (a real prelude `defmethod`)
            // both stay on the generic `assoc` path below.
            if type_name.local() == "vector" && VECTOR_BUILTIN_METHODS.contains(&method.as_str()) {
                return translate_vector_method(heap, method, vector_element_kind(args, cx.structs), args, cx);
            }
            // `HashTable<K,V>`'s builtin methods (except the `Option`-returning
            // `get`/`remove` and the `iter` `defmethod`) lower to a
            // `hashtable-op` node — see `translate_hashtable_method`.
            if type_name.local() == "hashtable" && HASHTABLE_BUILTIN_METHODS.contains(&method.as_str()) {
                let (kk, vk) = hashtable_kv_kinds(args, &typed.ty, cx.structs);
                return translate_hashtable_method(heap, method, kk, vk, args, cx);
            }
            let type_name_v = heap.alloc_string(type_name.local().to_string());
            heap.push_root(type_name_v);
            let method_v = heap.alloc_string(method.clone());
            heap.push_root(method_v);
            let arg_values = match tagged_ast_list_to_sexpr(heap, args, cx) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // method_v
                    heap.pop_root(); // type_name_v
                    return Err(e);
                }
            };
            let mut items = vec![type_name_v, method_v, Value::Bool(*instance)];
            items.extend(arg_values.iter().copied());
            let result = tagged(heap, "assoc", &items);
            for _ in 0..arg_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // method_v
            heap.pop_root(); // type_name_v
            result
        }
        // Everything below needs either multi-child rooting (a child's
        // translated `Value` must stay rooted while its siblings are
        // translated) or a node shape this phase doesn't compile yet
        // (`Construct`/`FieldGet`/`FieldSet`/`Match`/`Loop`/...). Both are
        // deliberately deferred to the phase that first needs them, rather
        // than building out unrooted multi-child plumbing nothing exercises
        // yet — see the module doc comment.
        Expr::Global(path) => translate_global(heap, path, &typed.ty, cx),
        Expr::FnRef(path) => translate_fnref(heap, path, &typed.ty),
        Expr::MethodRef { type_name, method } => translate_methodref(heap, type_name, method, &typed.ty),
        Expr::If(cond, then, els) => translate_if(heap, cond, then, els, &typed.ty, cx),
        Expr::Let(binds, body) => translate_let(heap, binds, body, cx),
        Expr::Call(path, args) => translate_call(heap, path, args, cx),
        Expr::Lambda { params, body } => translate_lambda(heap, params, body, cx.structs, cx.enums, cx.globals),
        Expr::Labels { defs, body } => translate_labels(heap, defs, body, cx),
        Expr::Apply(callee, args) => translate_apply(heap, callee, args, cx),
        Expr::Construct { type_name, variant, args, mutable } => translate_construct(heap, type_name, &typed.ty, *variant, args, *mutable, cx),
        Expr::FieldGet(obj, idx) => translate_field_get(heap, obj, *idx, &typed.ty, cx),
        Expr::FieldSet(obj, idx, value) => translate_field_set(heap, obj, *idx, value, cx),
        Expr::Match(scrut, arms) => translate_match(heap, scrut, arms, &typed.ty, cx),
        Expr::Set(name, value) => translate_set(heap, name, value, &typed.ty, cx),
        Expr::SetGlobal(path, value) => translate_set_global(heap, path, value, cx),
        Expr::Loop(body) => translate_loop(heap, body, cx),
        Expr::Break => tagged(heap, "break", &[]),
        Expr::Return(value) => translate_return(heap, value, cx),
        Expr::Panic(msg) => translate_panic(heap, msg, cx),
        Expr::Quote(datum) => translate_quote(heap, datum),
        // Diagnostics-only since monomorphization (see `Expr::TraitCall`'s
        // doc comment): a generic function's erased body is never registered
        // for execution and can't be `compile`d at all, so this node is
        // unreachable from any compilable source. The old dispatch-chain
        // lowering it used to have is gone.
        Expr::TraitCall { .. } => unsupported(heap, "TraitCall"),
    }
}

/// `Expr::If(cond, then, els)` -> `(if is-fn cond-form then-form else-form)`
/// (if/let/comparisons, labels/closures Stage 5). `is-fn` is `typed.ty`'s own
/// `Fn`-ness — the same per-node tag `Expr::Var`'s own `(var name is-fn)`
/// carries — because `if` doesn't introduce a function boundary the way a
/// `call`/`apply`/`labels`/`lambda` result does: nothing here automatically
/// "freshens" a branch's value the way `bare-returned-own-name`/R1-R4 do at an
/// actual function exit. `compiler.rs`'s `compile-if` reads this tag to know
/// whether it must retain a *borrowed* branch value before it can safely flow
/// out as the `if`'s own (necessarily fresh, by the time any caller sees it)
/// result — see that function's doc comment for the full reasoning. Every
/// other compiled node shape here is already either inherently fresh
/// (`int`/`bool`/arithmetic/a fresh `lambda`/`labels` box) or a function
/// boundary that already freshens its result, so `if` is the one place this
/// module needs to carry that information explicitly rather than letting
/// `compiler.rs` assume it.
fn translate_if(heap: &mut Heap, cond: &Typed, then: &Typed, els: &Typed, ty: &Type, cx: Ctx) -> Result<Value, Error> {
    let is_fn = Value::Bool(matches!(ty, Type::Fn(..)));
    let cond_v = ast_to_sexpr_scoped(heap, cond, cx)?;
    heap.push_root(cond_v);
    let then_v = match ast_to_sexpr_scoped(heap, then, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // cond_v
            return Err(e);
        }
    };
    heap.push_root(then_v);
    let els_v = match ast_to_sexpr_scoped(heap, els, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // then_v
            heap.pop_root(); // cond_v
            return Err(e);
        }
    };
    heap.push_root(els_v);
    let result = tagged(heap, "if", &[is_fn, cond_v, then_v, els_v]);
    heap.pop_root(); // els_v
    heap.pop_root(); // then_v
    heap.pop_root(); // cond_v
    result
}

/// `Expr::Let(binds, body)` -> `(let (((name-sym . kind) . value-form)...)
/// body-form...)` (if/let/comparisons, labels/closures Stage 5; the nested
/// `(name-sym . kind)` pair was a bare `name-sym` before Stage 6 of the
/// Sexpr-representation plan — see below). Unlike a `labels`/`lambda`
/// parameter or captured-name list, a binding pair's own retain/release
/// bookkeeping is lighter: `compiler.rs`'s `compile-let` only ever moves
/// already-computed values into/out of `env` by name, and the existing
/// `name-is-borrowed?`/`form-is-borrowed?` checks (keyed purely on "is this
/// name present in `env`") already do the right thing for a `let`-bound
/// `Fn`-typed value with no further bookkeeping. A `let`-bound `Sexpr`-typed
/// value is different, though: it needs an active GC root for as long as
/// its slot is live (`bind-let-values`/`restore-let-values`'s
/// `rt_push_sexpr_root`/`rt_pop_sexpr_root` — see [`binding_kind`]), so
/// Stage 6 added `kind` here too, mirroring [`tagged_sym_list`]'s own pair
/// shape exactly (reused, not reinvented). `body` may hold any number of
/// forms (including zero, CL `let`'s own `Unit`-typed empty-body case) —
/// translated the same variadic way [`translate_loop`] already translates
/// its own body statements; `compiler.rs`'s `compile-let-body` is what
/// gives the trailing forms CL `let`'s actual "sequence, last form's value
/// wins" semantics (lifted for Stage 8 of the Sexpr-representation plan,
/// `docs/implementation-log.md` — `dolist`'s own macro expansion always produces a
/// multi-statement inner `let` body: `,@body` followed by a hidden `setf`).
fn translate_let(heap: &mut Heap, binds: &[(String, Typed)], body: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let mut pair_values = Vec::with_capacity(binds.len());
    for (name, val) in binds {
        let name_sym = heap.intern_symbol(name);
        let kind = Value::Int(binding_kind(&val.ty));
        heap.push_root(name_sym);
        let name_pair = heap.cons(name_sym, kind);
        heap.pop_root(); // name_sym
        let name_pair = match name_pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..pair_values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(name_pair);
        let val_v = match ast_to_sexpr_scoped(heap, val, cx) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // name_pair
                for _ in 0..pair_values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(val_v);
        let pair = heap.cons(name_pair, val_v);
        heap.pop_root(); // val_v
        heap.pop_root(); // name_pair
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..pair_values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(pair);
        pair_values.push(pair);
    }
    let bindings_list = match list_of(heap, &pair_values) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..pair_values.len() {
                heap.pop_root();
            }
            return Err(e);
        }
    };
    for _ in 0..pair_values.len() {
        heap.pop_root();
    }
    heap.push_root(bindings_list);
    let body_values = match ast_list_to_sexpr(heap, body, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // bindings_list
            return Err(e);
        }
    };
    let mut items = Vec::with_capacity(1 + body_values.len());
    items.push(bindings_list);
    items.extend(body_values.iter().copied());
    let result = tagged(heap, "let", &items);
    for _ in 0..body_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // bindings_list
    result
}

/// `Expr::Set(name, value)` -> `(set name-str kind value-form)` (`loop`/
/// `break`/`return`/`setf`). `kind` is `typed.ty`'s own [`binding_kind`] —
/// for a `Set` node that's the *target variable's* type (`Checker::check_setf`
/// builds `Typed { expr: Expr::Set(name, value), ty }` with `ty` taken from
/// `env.get(&name)`, not from `value`'s own type — though the two always
/// agree, since `value` is checked against that same type), the same role
/// `translate_if`'s `is_fn` plays for a branch value: `compiler.rs`'s
/// `compile-set` reuses `compile-if-branch`'s retain-a-borrowed-value-before-
/// it-escapes-into-longer-lived-storage logic for the new value before
/// storing it into the target's slot, since that slot can outlive whatever
/// activation computed a borrowed value (exactly the same boundary an `if`
/// merge crosses) — derived from `kind = 1` (fn) rather than carried as a
/// separate `Bool`, now that `compile-set` also needs `kind = 2` (sexpr) to
/// know whether the target's slot has a second word holding its GC-root
/// stack index (`bind-params`/`bind-captures`/`bind-let-values`, the only
/// three binding sites a `setf` target's name can resolve to): a `setf`
/// overwrites the slot in place but the GC root pushed for it at bind time
/// is a value snapshot, not a live view of the slot — left unupdated, the
/// *new* value would sit completely unrooted for the rest of the binding's
/// scope (`crates/typelisp-rt/src/lib.rs`'s
/// `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
/// demonstrates exactly this at the raw-builtin level). `compile-set` calls
/// `rt_set_sexpr_root` with that recorded index to fix the *same* root entry
/// in place instead.
fn translate_set(heap: &mut Heap, name: &str, value: &Typed, ty: &Type, cx: Ctx) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let kind = Value::Int(binding_kind(ty));
    let form = match ast_to_sexpr_scoped(heap, value, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // name_v
            return Err(e);
        }
    };
    heap.push_root(form);
    let result = tagged(heap, "set", &[name_v, kind, form]);
    heap.pop_root(); // form
    heap.pop_root(); // name_v
    result
}

/// `Expr::Global(path)` -> `(global id kind)` — `id` is `path`'s already-
/// promoted compiled-global id ([`Ctx::globals`], populated by
/// `Interp::add_compiled_function` via [`collect_global_targets`] before
/// translation starts). `kind` is [`struct_field_kind`]'s classification of
/// the global's own declared type — a global's permanent-root storage
/// always holds a properly tagged `Sexpr` (whatever `rt_global_new`/
/// `rtvalue_to_struct_field` produced at promotion time — see
/// `Interp::promote_global`'s doc comment), but a *plain*-typed reference
/// (e.g. `i64`) needs untagging back to its own bare compiled
/// representation before this expression's result can be used like any
/// other `i64` value — `compiler.rs`'s `compile-global` does that with
/// [`struct_field_kind`]'s existing `compile-sexpr-field` decoder, the same
/// one a `defstruct` field read already uses (`translate_field_get`). No
/// `Ctx::direct`/`kind`-based GC-root bookkeeping is needed here the way
/// [`translate_set`]'s local-variable case needs, though: the global's
/// storage *is* a permanent GC root already, so there is no separate root
/// to keep in sync with, only the one slot itself.
fn translate_global(heap: &mut Heap, path: &Path, ty: &Type, cx: Ctx) -> Result<Value, Error> {
    let id = global_id(path, cx)?;
    let kind = Value::Int(global_field_kind(ty, cx.structs, cx.enums));
    tagged(heap, "global", &[Value::Int(id as i64), kind])
}

/// `Expr::SetGlobal(path, value)` -> `(set-global id kind value-form)` —
/// `kind` (see [`translate_global`]) tells `compiler.rs`'s
/// `compile-set-global` how to *tag* `value-form`'s own compiled result
/// (`compile-tag-struct-field`, the encode-side mirror of
/// `compile-sexpr-field`) before `rt_global_set` overwrites the permanent
/// root with it — the same encode step a `defstruct` field write already
/// does (`translate_field_set`). No local-binding-style GC-root
/// bookkeeping is needed here either, for the same reason
/// [`translate_global`]'s doc comment gives.
fn translate_set_global(heap: &mut Heap, path: &Path, value: &Typed, cx: Ctx) -> Result<Value, Error> {
    let id = global_id(path, cx)?;
    let kind = Value::Int(global_field_kind(&value.ty, cx.structs, cx.enums));
    let form = ast_to_sexpr_scoped(heap, value, cx)?;
    heap.push_root(form);
    let result = tagged(heap, "set-global", &[Value::Int(id as i64), kind, form]);
    heap.pop_root(); // form
    result
}

/// Looks `path` up in [`Ctx::globals`] — always present by construction:
/// `Interp::add_compiled_function` promotes every path
/// [`collect_global_targets`] finds in this same body *before* translation
/// starts, and that walker follows the exact same node shapes this module's
/// `translate_*` functions do (both are driven off [`Expr`]'s variants), so
/// a miss here means the two have desynced — an internal bridge bug, not a
/// user-facing one, hence the loud `TypeError` rather than a silent
/// fallback.
fn global_id(path: &Path, cx: Ctx) -> Result<usize, Error> {
    cx.globals.get(path).copied().ok_or_else(|| {
        Error::TypeError(format!(
            "compile: global \"{}\" was not promoted before translation (internal error)",
            path
        ))
    })
}

/// `Expr::Loop(body)` -> `(loop body-form...)` (`loop`/`break`/`return`):
/// each body statement translated in order, untagged (unlike a call
/// argument list — these are executed for effect/control, not consumed as
/// values, so no `is-fn` retain bookkeeping applies to them directly; only
/// whichever `break`/`return` eventually exits the loop carries that tag,
/// see [`translate_return`]). `compiler.rs`'s `compile-loop` builds the
/// actual loop/exit blocks and merge slot; `compile-loop-body` walks this
/// list.
fn translate_loop(heap: &mut Heap, body: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let body_values = ast_list_to_sexpr(heap, body, cx)?;
    let result = tagged(heap, "loop", &body_values);
    for _ in 0..body_values.len() {
        heap.pop_root();
    }
    result
}

/// `Expr::Return(value)` -> `(return is-fn value-form)` (`loop`/`break`/
/// `return`). A value-less `(return)` is translated as if its value were an
/// explicit `Expr::Unit` literal (`is-fn` always `false`, `value-form` the
/// real `(unit)` tag `Expr::Unit` itself already produces) rather than a
/// second, special-cased tag — `compiler.rs`'s `compile-return` (and
/// `compile-value`'s "unit" arm it relies on) handles both shapes through
/// the exact same path this way. `(break)`, by contrast, never carries a
/// value at all (not even an implicit `Unit` one) — see its own tag, built
/// directly in [`ast_to_sexpr_scoped`].
fn translate_return(heap: &mut Heap, value: &Option<Box<Typed>>, cx: Ctx) -> Result<Value, Error> {
    match value {
        Some(v) => {
            let is_fn = Value::Bool(matches!(v.ty, Type::Fn(..)));
            let form = ast_to_sexpr_scoped(heap, v, cx)?;
            heap.push_root(form);
            let result = tagged(heap, "return", &[is_fn, form]);
            heap.pop_root();
            result
        }
        None => {
            let form = tagged(heap, "unit", &[])?;
            heap.push_root(form);
            let result = tagged(heap, "return", &[Value::Bool(false), form]);
            heap.pop_root();
            result
        }
    }
}

/// `Expr::Panic(msg)` -> `(panic msg-form)`. `msg` is always `Str`-typed
/// (`Checker::check_panic` requires it), so — unlike [`translate_match`]/
/// [`translate_field_get`] — no `kind`/scrutinee-shape dispatch is needed
/// here at all: `compiler.rs`'s `compile-panic` just compiles `msg-form`
/// (already the properly tagged `Sexpr::Str` representation [`Expr::Str`]'s
/// own translation produces) and hands it to `rt_panic`, which prints it and
/// aborts the process — the only safe way to fail out of compiled code (no
/// landing pads to unwind through across the JIT/AOT native-code boundary).
/// `Expr::Panic`'s own checked type is `Never`, so nothing downstream ever
/// reads this node's value for real.
fn translate_panic(heap: &mut Heap, msg: &Typed, cx: Ctx) -> Result<Value, Error> {
    let msg_v = ast_to_sexpr_scoped(heap, msg, cx)?;
    heap.push_root(msg_v);
    let result = tagged(heap, "panic", &[msg_v]);
    heap.pop_root();
    result
}

/// `Expr::MethodRef { type_name, method }` -> a non-capturing `lambda` tag
/// that forwards every argument straight through to an `(assoc type-name
/// method true arg...)` call on the reified instance method — the
/// [`translate_fnref`] forwarding-wrapper trick (labels/closures Stage 4),
/// generalized from a free `defun` to an instance method: reaches the exact
/// same `compile-assoc`/`compile-assoc-user` dispatch an ordinary
/// `recv::method` call site already goes through (native `i32`/`i64`/`f64`/
/// `char`/`string` arithmetic, or a `compile-assoc-user` mangled-name call
/// for anything else — a user-defined method, or one on a primitive
/// receiver), so no separate runtime representation for "instance method
/// used as a value" is needed, the same reasoning [`translate_fnref`]'s own
/// doc comment gives for a bare top-level function. Param names are
/// synthesized positionally from `ty`'s arity, exactly like
/// [`translate_fnref`] — `params[0]` is the receiver
/// (`Checker::method_value`'s own `Type::Fn(af.sig.params, ..)` keeps the
/// method's own registered signature unchanged, receiver included), matching
/// `Expr::Assoc`'s own convention that the receiver is simply `args[0]`, so
/// no special-casing is needed here. Unlike [`translate_fnref`], this
/// function does *not* need a `&rest`-forwarding parameter appended: a
/// `defmethod`'s parameter list has no `&rest` syntax at all (`MethodSig`
/// carries no `rest` field, `Checker::parse_defmethod_sig` never parses one),
/// and no builtin `AssocFn` registration sets `FnSig::rest` either — so `ty`
/// reaching this function is `Type::Fn(_, None, _)` in every reachable case.
/// `params` is built the same way as [`translate_fnref`]'s purely so both
/// stay visibly in sync should `defmethod` ever gain `&rest` support.
fn translate_methodref(heap: &mut Heap, type_name: &Path, method: &str, ty: &Type) -> Result<Value, Error> {
    let params: Vec<(String, Type)> = match ty {
        Type::Fn(params, ..) => params.iter().enumerate().map(|(i, t)| (format!("arg{}", i), t.clone())).collect(),
        _ => return unsupported(heap, "MethodRef"),
    };

    let type_name_v = heap.alloc_string(type_name.local().to_string());
    heap.push_root(type_name_v);
    let method_v = heap.alloc_string(method.to_string());
    heap.push_root(method_v);
    let mut var_values = Vec::with_capacity(params.len());
    for (n, t) in &params {
        let is_fn = matches!(t, Type::Fn(..));
        let s = heap.alloc_string(n.clone());
        heap.push_root(s);
        let v = match tagged(heap, "var", &[s, Value::Bool(is_fn)]) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // s
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // method_v
                heap.pop_root(); // type_name_v
                return Err(e);
            }
        };
        heap.pop_root(); // s
        heap.push_root(v);
        let pair = heap.cons(Value::Int(binding_kind(t)), v);
        heap.pop_root(); // v
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // method_v
                heap.pop_root(); // type_name_v
                return Err(e);
            }
        };
        heap.push_root(pair);
        var_values.push(pair);
    }
    let mut assoc_items = vec![type_name_v, method_v, Value::Bool(true)];
    assoc_items.extend(var_values.iter().copied());
    let call_body = match tagged(heap, "assoc", &assoc_items) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..var_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // method_v
            heap.pop_root(); // type_name_v
            return Err(e);
        }
    };
    for _ in 0..var_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // method_v
    heap.pop_root(); // type_name_v
    heap.push_root(call_body);
    let result = build_lambda_tag(heap, &fresh_lambda_name("methodref"), &[], &params, call_body);
    heap.pop_root(); // call_body
    result
}

/// `Expr::Quote(datum)` -> a synthesized `(construct true false empty
/// variant arg-form...)` node — the exact wire shape [`translate_construct`]
/// already produces for a *written* `Sexpr` constructor call (e.g. `(Int
/// 5)`/`(Cons a b)`) — built directly from the literal `datum` rather than
/// from any real `Typed` sub-expression (there is none; `datum` is already
/// fully known at check time, `Checker`'s own `QuotedSexpr`). Reusing
/// `compile-construct-sexpr`'s existing per-variant dispatch this way needs
/// no new machinery in `compiler.rs` for most leaves: `Nil`/`Int`/`Float`/
/// `Bool`/`Char`/`Str`/`Cons` already have a real compiled representation
/// (used today by an ordinary `Sexpr` constructor call), so a quoted literal
/// made only of those reuses it verbatim, recursively for a `Cons`'s two
/// fields. `Bignum`/`Ratio` *do* have compiled support too
/// (`bignum_literal_form`/`ratio_literal_form`, variants `8`/`9`).
///
/// `Sym`/`Path` (variants `5`/`10`) are the two exceptions: a `Sym`'s/
/// `Path`'s tagged payload is a `SymId`/`PathId` with no compile-time-known
/// value the way every other leaf's payload is (the interning table only
/// exists in whichever `Heap` ends up running the code — AOT's target
/// program has its own, separate one) — the same reason `Str` embeds its
/// *characters*, not a pre-allocated `Value::Str`, rather than that these
/// two variants have no compiled representation at all. `compiler.rs`'s
/// dedicated `compile-construct-sym`/`compile-construct-path` (called
/// directly by `compile-construct`, *not* folded into
/// `compile-construct-sexpr`'s own dispatch — see that function's own call
/// site for why) mirror `Str`'s approach: each segment's *name* is embedded
/// as an ordinary `(str (int c0) ...)` node (built by [`str_literal_form`],
/// the exact same helper `Str` itself uses) and interned for real at
/// startup, via `rt_intern_symbol`/`rt_intern_path`.
fn translate_quote(heap: &mut Heap, datum: &QuotedSexpr) -> Result<Value, Error> {
    match datum {
        QuotedSexpr::Nil => tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(0)]),
        QuotedSexpr::Sym(name) => {
            let leaf = str_literal_form(heap, name)?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(5), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Path(segs) => {
            let mut leaves = Vec::with_capacity(segs.len());
            for seg in segs {
                let leaf = match str_literal_form(heap, seg) {
                    Ok(v) => v,
                    Err(e) => {
                        for _ in &leaves {
                            heap.pop_root();
                        }
                        return Err(e);
                    }
                };
                heap.push_root(leaf);
                leaves.push(leaf);
            }
            let mut fields = vec![Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(10)];
            fields.extend(leaves.iter().copied());
            let result = tagged(heap, "construct", &fields);
            for _ in &leaves {
                heap.pop_root();
            }
            result
        }
        QuotedSexpr::Int(n) => {
            let leaf = tagged(heap, "int", &[Value::Int(*n)])?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(1), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Float(f) => {
            let leaf = tagged(heap, "float", &[Value::Int(f.to_bits() as i64)])?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(2), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Char(c) => {
            let leaf = tagged(heap, "char", &[Value::Char(*c)])?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(3), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Bool(b) => {
            let leaf = tagged(heap, "bool", &[Value::Bool(*b)])?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(4), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Str(s) => {
            let leaf = str_literal_form(heap, s)?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(6), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Bignum(n) => {
            let leaf = bignum_literal_form(heap, n)?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(8), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Ratio(r) => {
            let leaf = ratio_literal_form(heap, r)?;
            heap.push_root(leaf);
            let result = tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(9), leaf]);
            heap.pop_root();
            result
        }
        QuotedSexpr::Cons(car, cdr) => {
            // A nested `Sym`/`Path` leaf translates to a valid
            // `(unsupported "Quote")` node, not an `Err` (`unsupported`'s own
            // signature) — propagate it as this whole `Cons`'s result instead
            // of embedding it as an ordinary field value, so the caller sees
            // the same clean `unsupported` outcome regardless of nesting
            // depth.
            let car_form = translate_quote(heap, car)?;
            if is_unsupported_tag(heap, car_form) {
                return Ok(car_form);
            }
            heap.push_root(car_form);
            let cdr_form = match translate_quote(heap, cdr) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // car_form
                    return Err(e);
                }
            };
            if is_unsupported_tag(heap, cdr_form) {
                heap.pop_root(); // car_form
                return Ok(cdr_form);
            }
            heap.push_root(cdr_form);
            let result =
                tagged(heap, "construct", &[Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(7), car_form, cdr_form]);
            heap.pop_root(); // cdr_form
            heap.pop_root(); // car_form
            result
        }
    }
}

/// Whether `v` is a `(unsupported "<Variant>")` node — see [`unsupported`].
/// [`translate_quote`]'s `Cons` case needs this: `unsupported` returns `Ok`,
/// not `Err` (it's valid `Sexpr` data, just data `compiler.rs` would panic
/// on), so a recursive sub-translation going `unsupported` doesn't `?`-
/// propagate on its own the way a real error would.
fn is_unsupported_tag(heap: &Heap, v: Value) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(id)) if heap.symbol_name(id) == "unsupported")
}

/// `Expr::Labels { defs, body }` -> `(labels (captured-sym...) ((name
/// (param-sym...) single-body-form)...) single-trailing-body-form)`.
///
/// `captured` (labels/closures Stage 2: outer-scope capture) is the whole
/// block's free-variable list (`freevars::labels_free_vars`) — empty when
/// no def references anything outside its own parameters/siblings, exactly
/// reproducing Stage 1's no-capture shape (just with an extra empty list
/// field). Every sibling shares this *one* list rather than each getting its
/// own narrower one — see `labels_free_vars`'s doc comment for why a shared
/// environment, not a per-sibling one, is what lets sibling-to-sibling calls
/// work without a second, transitive analysis pass. When this `labels` block
/// itself nests inside another, `captured_names` (computed here, from
/// `outer_captured`) becomes the *new* `outer_captured` passed down to each
/// def's own body translation and to the trailing body — both can reach an
/// enclosing block's own sibling (`compiler.rs`'s `fn-env` is now a real
/// scope stack shared down through nesting — see that module's doc comment),
/// and forwarding that sibling's own captured values along requires having
/// them on hand, which `labels_free_vars`'s unconditional prefix-copy of
/// `outer_captured` guarantees.
///
/// A def's body may otherwise reference its own parameters, any sibling/self
/// name (resolved as a direct call when it's the callee of an `Apply`, see
/// [`translate_apply`], or boxed into a `ClosureBox` on demand when
/// referenced bare as a value, see `compiler.rs`'s `resolve-value`), or any
/// of `captured`'s names — only a genuinely nonexistent name is rejected at
/// compile time (`resolve-value`'s "unbound variable" panic).
/// Every def's body and the trailing body must be a single expression — the
/// same restriction `Interp::add_compiled_function` already applies to a
/// `defun`'s own body, just extended uniformly to `labels` rather than
/// lifted here.
fn translate_labels(heap: &mut Heap, defs: &[LabelDef], body: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let mut siblings = cx.direct.clone();
    for (name, _, _) in defs {
        siblings.insert(name.clone());
    }

    let captured_names = labels_free_vars(defs, cx.direct, cx.outer_captured);
    // The scope each def's body / the trailing body sees: siblings become
    // directly callable, and this block's captured list becomes the enclosing
    // one for any nested block. `structs` is invariant.
    let inner = Ctx { direct: &siblings, outer_captured: &captured_names, ..cx };
    let captured_list = tagged_sym_list(heap, &captured_names)?;
    heap.push_root(captured_list);

    let mut def_values = Vec::with_capacity(defs.len());
    for (name, params, fbody) in defs {
        if fbody.len() != 1 {
            for _ in 0..def_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // captured_list
            return Err(Error::TypeError(format!(
                "compile: labels function \"{}\" has a multi-expression body, not yet supported",
                name
            )));
        }
        match translate_labels_def(heap, name, params, &fbody[0], inner) {
            Ok(v) => {
                heap.push_root(v);
                def_values.push(v);
            }
            Err(e) => {
                for _ in 0..def_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // captured_list
                return Err(e);
            }
        }
    }
    let defs_list = match list_of(heap, &def_values) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..def_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // captured_list
            return Err(e);
        }
    };
    for _ in 0..def_values.len() {
        heap.pop_root();
    }
    heap.push_root(defs_list);

    if body.len() != 1 {
        heap.pop_root(); // defs_list
        heap.pop_root(); // captured_list
        return Err(Error::TypeError("compile: labels body has a multi-expression body, not yet supported".into()));
    }
    let body_v = match ast_to_sexpr_scoped(heap, &body[0], inner) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // defs_list
            heap.pop_root(); // captured_list
            return Err(e);
        }
    };
    heap.push_root(body_v);
    let result = tagged(heap, "labels", &[captured_list, defs_list, body_v]);
    heap.pop_root(); // body_v
    heap.pop_root(); // defs_list
    heap.pop_root(); // captured_list
    result
}

/// One `labels` def: `(name-str (param-sym...) single-body-form)` — an
/// untagged 3-element list (its fixed position within `labels`'s own
/// already-tagged shape makes a separate tag unnecessary).
fn translate_labels_def(
    heap: &mut Heap,
    name: &str,
    params: &[(String, crate::Type)],
    body: &Typed,
    cx: Ctx,
) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let param_list = match tagged_sym_list(heap, params) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(param_list);
    let body_v = match ast_to_sexpr_scoped(heap, body, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(body_v);
    let result = list_of(heap, &[name_v, param_list, body_v]);
    heap.pop_root(); // body_v
    heap.pop_root(); // param_list
    heap.pop_root(); // name_v
    result
}

/// `Expr::Apply(callee, args)` dispatches on `callee`'s shape (labels/closures
/// Stage 4 extends this from the Stage 1 direct-only version):
/// - a `Var` naming a currently in-scope `labels` sibling/self ->
///   [`translate_direct_apply`] (unchanged since Stage 1).
/// - a `lambda` literal being called immediately (an IIFE) ->
///   [`translate_immediate_lambda_call`] — never escapes, so no `ClosureBox`.
/// - anything else (a captured closure variable, a higher-order function's
///   own parameter, ...) -> [`translate_indirect_apply`] — `callee`'s value
///   is evaluated and called through at runtime, since nothing here can
///   know ahead of time which `ClosureBox` it'll be.
fn translate_apply(heap: &mut Heap, callee: &Typed, args: &[Typed], cx: Ctx) -> Result<Value, Error> {
    match &callee.expr {
        Expr::Var(n) if cx.direct.contains(n) => translate_direct_apply(heap, n, args, cx),
        Expr::Lambda { params, body } => translate_immediate_lambda_call(heap, params, body, args, cx),
        _ => translate_indirect_apply(heap, callee, args, cx),
    }
}

/// `(apply name arg...)` — a direct call to a `labels` sibling/self
/// (Stage 1 scope, unchanged).
fn translate_direct_apply(heap: &mut Heap, name: &str, args: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![name_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "apply", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // name_v
    result
}

/// `((lambda (params) body) args...)` — an immediately-invoked `lambda`
/// literal as the callee of its own `Apply` (labels/closures Stage 4): never
/// escapes, so it needs no `ClosureBox` at all. Translated as if it were a
/// single-def, non-recursive `labels` block instead (`(labels ()
/// ((name (params) body)) (name args...))`), reusing [`translate_labels`]'s
/// entire mechanism — declare-then-compile, captured-list sharing, direct-
/// call dispatch into the *outer* scope's own siblings, all for free —
/// rather than teaching `compiler.rs` a second way to build essentially the
/// same kind of LLVM function. Passing the *outer* `direct` set through
/// (rather than an empty one, unlike [`translate_lambda`]'s escaping case)
/// is deliberate and safe specifically because this lambda never escapes:
/// it's compiled within the very same `compile-labels`-style scope its call
/// site already has, so it can call outer `labels` siblings directly just
/// like another sibling could (the synthesized `labels` block is, after
/// all, nested at exactly the point the source `Apply` was). `name` is a
/// fresh, process-wide-unique local name ([`fresh_lambda_name`]) —
/// `translate_labels_def`/`compile-labels`'s own `outer_fn_name$inner_name`
/// mangling still guarantees no collision with anything else in the same
/// module even though this name never came from user source.
fn translate_immediate_lambda_call(
    heap: &mut Heap,
    params: &[(String, Type)],
    lambda_body: &[Typed],
    args: &[Typed],
    cx: Ctx,
) -> Result<Value, Error> {
    let name = fresh_lambda_name("__lambda");
    let dummy_ty = Type::Unit;
    let def: LabelDef = (name.clone(), params.to_vec(), lambda_body.to_vec());
    let call = Typed { loc: None,
        expr: Expr::Apply(Box::new(Typed { loc: None, expr: Expr::Var(name), ty: dummy_ty.clone() }), args.to_vec()),
        ty: dummy_ty,
    };
    translate_labels(heap, &[def], std::slice::from_ref(&call), cx)
}

/// `(apply-indirect callee-form arg...)` — the general indirect-dispatch
/// case (labels/closures Stage 4): `callee` is translated and (at
/// `compiler.rs`'s `compile-apply-indirect`) evaluated like any other value,
/// then called through `build-closure-apply` at runtime. A deliberately
/// distinct tag from `(apply name arg...)` — *not* the plan's originally
/// sketched `(apply (direct name) arg...)`/`(apply (indirect callee) arg...)`
/// unification — so Stage 1-3's already-shipped `(apply name arg...)` shape
/// (and `compiler.rs`'s/tests' existing assumptions about it) needs no
/// change at all.
fn translate_indirect_apply(heap: &mut Heap, callee: &Typed, args: &[Typed], cx: Ctx) -> Result<Value, Error> {
    let callee_v = ast_to_sexpr_scoped(heap, callee, cx)?;
    heap.push_root(callee_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![callee_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "apply-indirect", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // callee_v
    result
}

/// Builds `(lambda name-str (captured-sym...) (param-sym...)
/// single-body-form)` — the tagged shape both a real escaping `Expr::Lambda`
/// ([`translate_lambda`]) and a synthesized `Expr::FnRef` forwarding wrapper
/// ([`translate_fnref`]) produce. `name` is the caller's responsibility to
/// make unique ([`fresh_lambda_name`]) — both source forms are anonymous at
/// the typelisp level, unlike a `labels` def. `body` must already be rooted
/// by the caller (the same convention [`tagged`]'s own `items` slice
/// elements rely on) and remains the caller's to pop afterward; this
/// function only roots/pops what it itself allocates.
fn build_lambda_tag(heap: &mut Heap, name: &str, captured: &[(String, Type)], params: &[(String, Type)], body: Value) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let captured_list = match tagged_sym_list(heap, captured) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // name_v
            return Err(e);
        }
    };
    heap.push_root(captured_list);
    let param_list = match tagged_sym_list(heap, params) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // captured_list
            heap.pop_root(); // name_v
            return Err(e);
        }
    };
    heap.push_root(param_list);
    let result = tagged(heap, "lambda", &[name_v, captured_list, param_list, body]);
    heap.pop_root(); // param_list
    heap.pop_root(); // captured_list
    heap.pop_root(); // name_v
    result
}

/// `Expr::Lambda { params, body }` -> `(lambda name (captured...)
/// (param-sym...) single-body-form)` (labels/closures Stage 4) — a `lambda`
/// value that *escapes*: reached anywhere a `Expr::Lambda` is its own typed
/// node rather than the literal callee of its own enclosing `Apply`
/// ([`translate_immediate_lambda_call`] handles that case instead, via a
/// completely different, boxing-free translation). `compiler.rs`'s
/// `compile-lambda` (the consumer of this tag) therefore never has to decide
/// whether to box the result — every `lambda` tag it ever sees is, by
/// construction, the escaping case.
///
/// The lambda's own body is translated with an *empty* `direct` set,
/// matching [`lambda_free_vars`]'s own treatment of enclosing `labels`
/// siblings: a nested `lambda` never gets direct-call access to them (only
/// a `labels` def's own siblings do), so a sibling name referenced here
/// becomes an ordinary capture attempt instead — which now resolves
/// correctly (a follow-up to labels/closures Stage 4): `compiler.rs`'s
/// `compile-lambda` builds *this* lambda's own `ClosureBox` env array in the
/// *outer* scope (where the real `fn-env` is still available), so
/// `compile-env-args`/`resolve-value` box the sibling there, and the boxed
/// value flows into this lambda's own `env` via the ordinary `bind-captures`
/// path — no special-casing needed inside the lambda's own body at all.
fn translate_lambda(
    heap: &mut Heap,
    params: &[(String, Type)],
    body: &[Typed],
    structs: &HashSet<Path>,
    enums: &HashSet<Path>,
    globals: &HashMap<Path, usize>,
) -> Result<Value, Error> {
    if body.len() != 1 {
        return Err(Error::TypeError("compile: lambda has a multi-expression body, not yet supported".into()));
    }
    let captured_names = lambda_free_vars(params, body);
    // An empty `outer_captured`, not whatever the enclosing scope's own was:
    // unlike `labels` (which shares `compiler.rs`'s `fn-env` scope stack
    // down through nesting), a `lambda` is compiled with a *fresh* `fn-env`
    // (`compile-lambda`'s own `(new-fn-env)`) — so a `labels` block nested
    // inside *this* body has no enclosing block's captured-list to prefix
    // its own with, regardless of what scope the `lambda` itself sits in.
    let direct = HashSet::new();
    let cx = Ctx { direct: &direct, outer_captured: &[], structs, enums, globals };
    let body_v = ast_to_sexpr_scoped(heap, &body[0], cx)?;
    heap.push_root(body_v);
    let result = build_lambda_tag(heap, &fresh_lambda_name("lambda"), &captured_names, params, body_v);
    heap.pop_root(); // body_v
    result
}

/// The `Sexpr` type a `&rest`-forwarding synthetic parameter always has —
/// [`translate_fnref`]'s counterpart to `Checker::sexpr_ty()` (private to
/// `checker.rs`), needed here because `ast_bridge.rs` synthesizes this
/// parameter itself rather than receiving it pre-typed on a real `Typed`
/// node.
fn rest_sexpr_type() -> Type {
    Type::Named(Path::root("sexpr"), vec![])
}

/// `Expr::FnRef(path)` -> a non-capturing `lambda` tag that just forwards
/// every argument straight through to the named top-level `defun`
/// (labels/closures Stage 4): `(lambda fnref$N () (arg0 arg1 ...) (call
/// path-local-name (var arg0) (var arg1) ...))`. Lets a top-level function
/// used as a first-class value (e.g. passed where a `(fn (i64) i64)` is
/// expected) reach the exact same `ClosureBox` machinery a real `lambda`
/// value does, rather than inventing a second runtime representation for
/// "function reference" values — `compile-lambda` builds this wrapper's
/// `ClosureBox` exactly like any other, `compile-call` (already built for
/// labels/closures Stage 3) handles the forwarding call inside it. Param
/// names are synthesized positionally from `ty`'s fixed arity (the only
/// place that arity is available — an `Expr::FnRef` carries no parameter
/// names of its own, just a `Path`); when `ty`'s target is variadic (its
/// `rest` field is `Some`), one more synthetic `Sexpr`-typed trailing
/// parameter is appended and forwarded too — the exact same `(name,
/// rest_sexpr_type())` append `Checker::check_defun`/`check_lambda` already
/// do for a directly declared `&rest` parameter, so the wrapper closure ends
/// up with the identical N+1-param shape a real `&rest`-taking closure has.
/// No downstream special-casing is needed for this: the `ClosureBox` calling
/// convention (`build-closure-apply`/`compile-apply-indirect`) passes
/// arguments through an arity-generic array, exactly like [`translate_lambda`]
/// already relies on for a `&rest`-declared `lambda` literal.
fn translate_fnref(heap: &mut Heap, path: &Path, ty: &Type) -> Result<Value, Error> {
    let mut params: Vec<(String, Type)> = match ty {
        Type::Fn(params, ..) => params.iter().enumerate().map(|(i, t)| (format!("arg{}", i), t.clone())).collect(),
        _ => return unsupported(heap, "FnRef"),
    };
    if let Type::Fn(_, Some(_), _) = ty {
        let rest_name = format!("arg{}", params.len());
        params.push((rest_name, rest_sexpr_type()));
    }

    // See `translate_call`'s matching check: `sexpr-car`/`sexpr-cdr`/
    // `sexpr-cons` are never prefixed.
    let raw_name = path.local();
    let target_v = if crate::eval::interp::is_rt_builtin_name(raw_name) {
        heap.alloc_string(raw_name.to_string())
    } else {
        heap.alloc_string(user_symbol_name(raw_name))
    };
    heap.push_root(target_v);
    let mut var_values = Vec::with_capacity(params.len());
    for (n, t) in &params {
        let is_fn = matches!(t, Type::Fn(..));
        let s = heap.alloc_string(n.clone());
        heap.push_root(s);
        let v = match tagged(heap, "var", &[s, Value::Bool(is_fn)]) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // s
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // target_v
                return Err(e);
            }
        };
        heap.pop_root(); // s
        // wrap as `(kind . form)` — the same tagged shape every other
        // `apply`/`apply-indirect`/`call` argument list uses (see
        // `tagged_ast_list_to_sexpr`), built by hand here since these `var`
        // forms are synthesized from `ty`'s arity rather than translated
        // from real `Typed` argument nodes.
        heap.push_root(v);
        let pair = heap.cons(Value::Int(binding_kind(t)), v);
        heap.pop_root(); // v
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // target_v
                return Err(e);
            }
        };
        heap.push_root(pair);
        var_values.push(pair);
    }
    let mut call_items = vec![target_v];
    call_items.extend(var_values.iter().copied());
    let call_body = match tagged(heap, "call", &call_items) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..var_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // target_v
            return Err(e);
        }
    };
    for _ in 0..var_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // target_v
    heap.push_root(call_body);
    let result = build_lambda_tag(heap, &fresh_lambda_name("fnref"), &[], &params, call_body);
    heap.pop_root(); // call_body
    result
}

/// `Expr::Call(path, args)` -> `(call name arg...)` (labels/closures Stage
/// 3) — same shape as [`translate_apply`]'s output, but for a call to a
/// *top-level* `defun` rather than a `labels` sibling/self. Always
/// resolvable: `Checker::resolve_fn` only ever lets a `defun` call a name
/// that's already fully registered (itself included, for self-recursion —
/// see `Checker::check_defun`'s doc comment), so unlike [`translate_apply`]
/// there's no indirect/boxed case to fall back to `unsupported` for. Only
/// `path`'s local (final) segment is kept — `compiler.rs`'s `compile-call`
/// looks the callee up by that same plain name via `get-function` against
/// the destination module, matching how every compiled top-level function
/// is itself declared under its local name (`Interp::add_compiled_function`).
fn translate_call(heap: &mut Heap, path: &Path, args: &[Typed], cx: Ctx) -> Result<Value, Error> {
    // `sexpr-car`/`sexpr-cdr`/`sexpr-cons` are rewritten to `rt_car`/`rt_cdr`/
    // `rt_cons` by `compiler.rs`'s `compile-call` itself, matching on this
    // exact literal name (`crate::eval::interp::is_rt_builtin_name`) — they
    // never go through `declare_external_function`/`user_symbol_name` at all
    // (`Interp::compile_function_rec` excludes them from `call_targets` for
    // the same reason), so prefixing them here would break that match.
    let raw_name = path.local();
    let name_v = if crate::eval::interp::is_rt_builtin_name(raw_name) {
        heap.alloc_string(raw_name.to_string())
    } else {
        heap.alloc_string(user_symbol_name(raw_name))
    };
    heap.push_root(name_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![name_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "call", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // name_v
    result
}

/// Translates one [`Pattern`] into a tagged `Sexpr` `compiler.rs`'s
/// `compile-pattern-test` walks: `(pat-wild)` / `(pat-bind name-str)` /
/// `(pat-lit n)` (an `Int`/`Bool`/`Char` literal sub-pattern, collapsed to
/// one shared tag — see the note on `n` below) / `(pat-ctor variant-i64
/// (sub-pattern...) scrut-kind field-kinds)`. The trailing `scrut-kind`
/// ([`MATCH_KIND_SEXPR`]/[`MATCH_KIND_BOX`]/[`MATCH_KIND_STRUCT`], via
/// [`match_kind_for_path`]) records this constructor's own type
/// representation (`type_name`). It is per-node — not inherited from the
/// enclosing match — because a nested sub-pattern can differ from its parent
/// (e.g. a `Sexpr`-typed field inside a `defenum` box). `field-kinds` is a
/// list of each subpattern's own [`struct_field_kind`], meaningful only for
/// `scrut-kind = MATCH_KIND_STRUCT` (`compiler.rs`'s `compile-ctor-subpatterns`
/// needs a boxed struct's own per-field decode — `int`/`float`/`char`/`bool`/
/// passthrough — where a sum-ADT box or `Sexpr` scrutinee's field extraction
/// is driven by the shared `variant` instead); the empty list otherwise.
///
/// `n` for `(pat-lit n)`: precomputed here, in Rust, to whatever raw
/// `i64` the *compiled* representation of that literal would be —
/// `Bool`'s `0`/`1` (matching `compile-bool`'s own convention) and
/// `Char`'s Unicode scalar value, not just `Int`'s payload — so
/// `compiler.rs`'s `compile-pattern-test` only ever needs one plain
/// `build-icmp-eq` against a `const-i64`, regardless of which of the
/// three literal kinds it came from (no `char`/`bool` -> `i64` conversion
/// primitive exists in the compiled language yet, so doing this
/// conversion on the Rust side avoids needing one).
fn pattern_to_sexpr(heap: &mut Heap, pat: &Pattern, cx: Ctx) -> Result<Value, Error> {
    match pat {
        Pattern::Wildcard => tagged(heap, "pat-wild", &[]),
        Pattern::Bind(name, _) => {
            let v = heap.alloc_string(name.clone());
            tagged(heap, "pat-bind", &[v])
        }
        Pattern::Int(n) => tagged(heap, "pat-lit", &[Value::Int(*n)]),
        Pattern::Bool(b) => tagged(heap, "pat-lit", &[Value::Int(if *b { 1 } else { 0 })]),
        Pattern::Char(c) => tagged(heap, "pat-lit", &[Value::Int(*c as i64)]),
        Pattern::Ctor { type_name, variant, args, field_types, .. } => {
            // How *this* constructor's own type is represented, so
            // `compile-pattern-test` picks the right tag test (or, for a
            // boxed struct, no test at all — a `defstruct` always has
            // exactly one variant) and field extraction (a nested
            // sub-pattern can differ from its parent — e.g. a `Sexpr`-typed
            // field inside a `defenum` box).
            let kind = match_kind_for_path(type_name, cx);
            let sub_values = pattern_list_to_sexpr(heap, args, cx)?;
            let list = match list_of(heap, &sub_values) {
                Ok(v) => v,
                Err(e) => {
                    for _ in 0..sub_values.len() {
                        heap.pop_root();
                    }
                    return Err(e);
                }
            };
            for _ in 0..sub_values.len() {
                heap.pop_root();
            }
            heap.push_root(list);
            let kind_values: Vec<Value> = if kind == MATCH_KIND_STRUCT {
                field_types.iter().map(|t| Value::Int(struct_field_kind(t, cx.structs))).collect()
            } else {
                Vec::new()
            };
            let kinds_list = match list_of(heap, &kind_values) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // list
                    return Err(e);
                }
            };
            heap.push_root(kinds_list);
            // `scrut-kind`/`field-kinds` appended last so existing
            // `(pat-ctor variant subpats)` field indices (and their tests)
            // stay valid.
            let result = tagged(heap, "pat-ctor", &[Value::Int(*variant as i64), list, Value::Int(kind), kinds_list]);
            heap.pop_root(); // kinds_list
            heap.pop_root(); // list
            result
        }
    }
}

/// Translates each of `pats` in order, rooting every translated `Value` as
/// it goes — the pattern-tree counterpart of [`ast_list_to_sexpr`].
fn pattern_list_to_sexpr(heap: &mut Heap, pats: &[Pattern], cx: Ctx) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(pats.len());
    for p in pats {
        match pattern_to_sexpr(heap, p, cx) {
            Ok(v) => {
                heap.push_root(v);
                values.push(v);
            }
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        }
    }
    Ok(values)
}

/// `Expr::Match(scrut, arms)` -> `(match is-fn scrutinee-form
/// ((pattern-form . body-form)...) scrut-kind)` (Stage 5,
/// `docs/implementation-log.md`) for a `Sexpr` scrutinee
/// ([`MATCH_KIND_SEXPR`]), a sum-ADT box scrutinee (`Option`/`Result`/a user
/// `defenum`, since 2026-07-09, [`MATCH_KIND_BOX`]): the box a
/// `compile-construct-box` builds, tested by its variant-tag slot 0
/// (`compiler.rs`'s `compile-box-tag-test`/`compile-box-field`) instead of the
/// tagged-`i64` bit-test `compile-sexpr-tag-test` does for `Sexpr`, and — since
/// this struct-kind extension — a boxed-struct scrutinee (`defstruct`/
/// `Vector`, [`MATCH_KIND_STRUCT`]): a properly tagged `Sexpr` like the plain
/// `Sexpr` case, but single-variant, so `compiler.rs`'s `compile-pattern-test`
/// emits no tag test at all for it, only per-field extraction
/// (`compile-struct-field`). `scrut-kind` is appended last (and each
/// `pat-ctor` carries its own trailing `scrut-kind`/`field-kinds`, so a nested
/// sub-pattern can differ from its parent) so the existing field indices —
/// and their tests — stay valid. `is_fn` mirrors [`translate_if`]'s own tag of
/// the same name — `match`, like `if`, introduces no function-activation
/// boundary of its own, so a borrowed `Fn`-typed arm result needs the same
/// explicit retain `compiler.rs`'s `compile-if-branch` already provides
/// (reused as-is for each arm's body — see `compile-match-arms`'s doc
/// comment). Each arm's body is restricted to a single expression, the same
/// limit every other multi-expression body shape in this module has
/// ([`translate_let`]'s body, a `labels` def's body, ...).
fn translate_match(heap: &mut Heap, scrut: &Typed, arms: &[Arm], ty: &Type, cx: Ctx) -> Result<Value, Error> {
    let kind = match match_scrut_kind(&scrut.ty, cx) {
        Some(k) => k,
        None => return unsupported(heap, "Match"),
    };
    let is_fn = Value::Bool(matches!(ty, Type::Fn(..)));
    let kind_v = Value::Int(kind);
    let scrut_v = ast_to_sexpr_scoped(heap, scrut, cx)?;
    heap.push_root(scrut_v);
    let arm_values = match translate_arms(heap, arms, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // scrut_v
            return Err(e);
        }
    };
    let arms_list = match list_of(heap, &arm_values) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..arm_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // scrut_v
            return Err(e);
        }
    };
    for _ in 0..arm_values.len() {
        heap.pop_root();
    }
    heap.push_root(arms_list);
    // `scrut-kind` appended last so existing `(match is-fn scrut arms)` field
    // indices (and their tests) stay valid.
    let result = tagged(heap, "match", &[is_fn, scrut_v, arms_list, kind_v]);
    heap.pop_root(); // arms_list
    heap.pop_root(); // scrut_v
    result
}

/// Translates each [`Arm`] into a `(pattern-form . body-form)` pair,
/// rooting every pair as it goes — [`translate_match`]'s own helper, kept
/// separate purely to give the per-arm rooting its own clean error-
/// cleanup scope (mirrors [`ast_list_to_sexpr`]'s shape, one level up).
fn translate_arms(heap: &mut Heap, arms: &[Arm], cx: Ctx) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(arms.len());
    for arm in arms {
        if arm.body.len() != 1 {
            for _ in 0..values.len() {
                heap.pop_root();
            }
            return Err(Error::TypeError("compile: match arm has a multi-expression body, not yet supported".into()));
        }
        let pat_v = match pattern_to_sexpr(heap, &arm.pat, cx) {
            Ok(v) => v,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(pat_v);
        let body_v = match ast_to_sexpr_scoped(heap, &arm.body[0], cx) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // pat_v
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(body_v);
        let pair = heap.cons(pat_v, body_v);
        heap.pop_root(); // body_v
        heap.pop_root(); // pat_v
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(pair);
        values.push(pair);
    }
    Ok(values)
}

/// `Expr::Construct { variant, args, mutable }` -> `(construct is-sexpr-bool
/// mutable-bool type-name-str variant-i64 arg-form...)` (Stage 6
/// of the Sexpr-representation plan for the header's first three fields,
/// extended by Stage 3 of the Sexpr/RtValue unification plan —
/// `docs/implementation-log.md` — with `mutable`/`type-name-str`). `is-sexpr`
/// ([`is_sexpr_type`], read off `ty` — the node's own checked type,
/// `Type::Named(type_name, _)`, the same `Path` `Checker::check_construct`
/// built `type_name` from) and `mutable` (threaded straight through from
/// `Expr::Construct`'s own field, `Checker::check_construct`'s
/// `def.kind == AdtKind::Struct`) together drive the 3-way dispatch
/// `compiler.rs`'s `compile-construct` makes: `is-sexpr` builds one of
/// `Sexpr`'s own 8 variants out of the tagged-`i64`/`rt_cons` representation
/// Stage 2/3/4/5 already built (the inverse of `compile-sexpr-field`'s
/// extraction); `mutable` (a `defstruct`/`Vector<T>`/`cons-cell<K,V>`
/// instance) builds a `BoxedObj::Struct` via `rt_struct_new`
/// (`compile-construct-boxed-struct`) — the same tagged `Value::Boxed`
/// representation the interpreter's own `Expr::Construct` `mutable` arm
/// already uses (`heap.alloc_struct`), so a value either side builds
/// interoperates with the other; every other ADT (`Option`/`Result`/a user
/// sum type) allocates a fresh `malloc`'d box instead
/// (`compile-construct-box`, a variant-tag slot then one slot per field) —
/// deciding all of this here, from `ty`/`mutable`, is what lets
/// `compiler.rs` stay registry-free, the same reason [`translate_match`]
/// reads `is_sexpr_type` off the scrutinee rather than `compile-match`
/// re-deriving it from a type name string.
///
/// `args`' own translation differs between the three branches: `is-sexpr`
/// stays plain/untagged ([`ast_list_to_sexpr`], the same as `Expr::Assoc`'s
/// own arithmetic operands) — `Sexpr`'s own fields (an `Int`'s payload, a
/// `Cons`'s two `Sexpr` fields, ...) are never `Fn`- or (recursively)
/// `Sexpr`-typed in a way `compile-construct-sexpr` doesn't already
/// special-case (its `Cons` arm roots both fields itself). `mutable` uses
/// [`struct_field_ast_list_to_sexpr`] — a `(kind . form)` tagging by
/// [`struct_field_kind`] (not [`binding_kind`]: a struct field's `kind` must
/// pick the exact tagged-`Sexpr` shape to build, not merely whether GC-root/
/// `ClosureBox` bookkeeping is needed). The remaining (general-ADT) case
/// reuses [`tagged_ast_list_to_sexpr`] — the exact same `(kind . form)`
/// wrapping `Expr::Call`'s own call-argument list gets — so
/// `compile-construct-box-fields` can tell which fields are `Sexpr`-typed
/// (`kind = 2`) and need [`crate::compile::runtime::rt_push_permanent_sexpr_root`]'s
/// protection once stored into the box: unlike a call argument's root (popped
/// right after the call returns), a box field's value must stay reachable
/// for the box's own lifetime, which can outlive this activation — see that
/// function's doc comment. A `Fn`-typed field (`kind = 1`) still gets no
/// `ClosureBox` retain at all (deliberately leaked, same as before — see
/// `compiler.rs`'s `compile-construct-box` doc comment); only the
/// GC-root gap this tag closes is in scope here.
///
/// `type-name-str` ([`str_literal_form`]) is that struct's own local type
/// name, only ever built for the `mutable` branch (an empty placeholder,
/// `Value::Empty`, elsewhere); still always present in the tagged shape for a
/// uniform header regardless of which branch `compiler.rs`'s
/// `compile-construct` takes.
fn translate_construct(
    heap: &mut Heap,
    type_name: &Path,
    ty: &Type,
    variant: usize,
    args: &[Typed],
    mutable: bool,
    cx: Ctx,
) -> Result<Value, Error> {
    let is_sexpr = is_sexpr_type(ty);
    let type_name_form = if !is_sexpr && mutable { str_literal_form(heap, type_name.local())? } else { Value::Empty };
    heap.push_root(type_name_form);
    let arg_result = if is_sexpr {
        ast_list_to_sexpr(heap, args, cx)
    } else if mutable {
        struct_field_ast_list_to_sexpr(heap, args, cx)
    } else {
        tagged_ast_list_to_sexpr(heap, args, cx)
    };
    let arg_values = match arg_result {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // type_name_form
            return Err(e);
        }
    };
    let mut items = vec![Value::Bool(is_sexpr), Value::Bool(mutable), type_name_form, Value::Int(variant as i64)];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "construct", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // type_name_form
    result
}

/// Encodes a field index as a `Sexpr` list of exactly `idx` (otherwise
/// meaningless) elements, for [`translate_field_get`]/[`translate_field_set`]
/// to embed `usize` data `compiler.rs` can read back out as a host `i32` —
/// `store-arg`/`load-raw`'s slot-index parameter is `i32`
/// (`registry::llvm_builder_def`), but `Sexpr`'s only numeric literal kind
/// (`Value::Int`) always decodes to `i64` (`compiler.rs`'s `sexpr-int`) with
/// no narrowing primitive exposed to compiled/compiler-hosted code to bring
/// it down to `i32` — unlike `compile-sexpr-field`'s own `idx`, which never
/// faces this problem because it's a *host*-side loop counter
/// (`compile-ctor-subpatterns`'s own recursion), never decoded from Sexpr
/// data at all. `compiler.rs`'s `sexpr-list-length` already returns `i32`
/// (it counts cells, the same arity-counting role it already plays for
/// `compile-apply`/`compile-call`'s own argument arrays), so building a
/// list of the right *length* and re-deriving `idx` from that length sidesteps
/// the missing conversion entirely, at the cost of `idx` `Cons` cells
/// instead of one `Int` — cheap, since `idx` is always small (a `defstruct`'s
/// field position) and this list is never read for its own contents, only
/// its length.
fn idx_unary_list(heap: &mut Heap, idx: usize) -> Result<Value, Error> {
    list_of(heap, &vec![Value::Bool(false); idx])
}

/// `Expr::FieldGet(obj, idx)` -> `(field-get idx-unary-list kind-i64
/// obj-form)` (Stage 6, extended by Stage 3 of the Sexpr/RtValue
/// unification plan — `docs/implementation-log.md` — with `kind-i64`) — see
/// [`idx_unary_list`] for why `idx` isn't simply a `Sexpr` `Int`. Always
/// targets a `BoxedObj::Struct` `compiler.rs`'s `compile-construct-boxed-struct`
/// built — this node is only ever synthesized by `Checker::check_defstruct`
/// as a field accessor's own body (see `Expr::FieldGet`'s doc comment), and
/// every `defstruct` is `mutable` (`Checker::check_construct`'s
/// `def.kind == AdtKind::Struct`), so `obj`'s static type is always some
/// `defstruct`, never `Sexpr` — no `is_sexpr_type` dispatch is needed here
/// the way [`translate_construct`] needs one. `kind` ([`struct_field_kind`]
/// of `field_ty`, the field's own static type — `typed.ty` at this node's
/// call site, since a `FieldGet`'s checked type *is* the field's type) tells
/// `compile-field-get` how to decode the raw `Sexpr` `rt_struct_field_get`
/// returns back into this field's own compiled representation (`compile-
/// sexpr-field`'s exact per-variant bit manipulation, reused verbatim).
fn translate_field_get(heap: &mut Heap, obj: &Typed, idx: usize, field_ty: &Type, cx: Ctx) -> Result<Value, Error> {
    let idx_list = idx_unary_list(heap, idx)?;
    heap.push_root(idx_list);
    let obj_v = match ast_to_sexpr_scoped(heap, obj, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // idx_list
            return Err(e);
        }
    };
    heap.push_root(obj_v);
    let kind = Value::Int(struct_field_kind(field_ty, cx.structs));
    let result = tagged(heap, "field-get", &[idx_list, kind, obj_v]);
    heap.pop_root(); // obj_v
    heap.pop_root(); // idx_list
    result
}

/// `Expr::FieldSet(obj, idx, value)` -> `(field-set idx-unary-list kind-i64
/// obj-form value-form)` (Stage 6, extended the same way
/// [`translate_field_get`] was) — `kind` is [`struct_field_kind`] of
/// `value`'s own static type (equal to the field's, by the checker's own
/// `Expr::FieldSet` construction — see `Checker::check_defstruct`'s setter
/// body), telling `compile-field-set` how to *encode* the already-compiled
/// value before `rt_struct_field_set` stores it (`compile-tag-struct-field`,
/// the encode-side mirror of `compile-sexpr-field`'s decode `compile-field-get`
/// reuses). `value` carries no `is-fn`-style retain tag for the same reason
/// a `construct` field doesn't.
fn translate_field_set(heap: &mut Heap, obj: &Typed, idx: usize, value: &Typed, cx: Ctx) -> Result<Value, Error> {
    let idx_list = idx_unary_list(heap, idx)?;
    heap.push_root(idx_list);
    let obj_v = match ast_to_sexpr_scoped(heap, obj, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // idx_list
            return Err(e);
        }
    };
    heap.push_root(obj_v);
    let value_v = match ast_to_sexpr_scoped(heap, value, cx) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // obj_v
            heap.pop_root(); // idx_list
            return Err(e);
        }
    };
    heap.push_root(value_v);
    let kind = Value::Int(struct_field_kind(&value.ty, cx.structs));
    let result = tagged(heap, "field-set", &[idx_list, kind, obj_v, value_v]);
    heap.pop_root(); // value_v
    heap.pop_root(); // obj_v
    heap.pop_root(); // idx_list
    result
}

/// Accumulates both kinds of pre-declaration target [`Interp::compile_function`]
/// needs before compiling a body: free-function `Expr::Call`/`Expr::FnRef`
/// targets (`calls`) and instance/static-method `Expr::Assoc` targets
/// (`methods`, `(type_name, method)` pairs) — gathered by one walk
/// ([`collect_calls`]) since both are collected from the exact same set of
/// node shapes; see [`collect_call_targets`]/[`collect_assoc_targets`].
///
/// [`Interp::compile_function`]: crate::eval::Interp::compile_function
#[derive(Default)]
struct CallTargets {
    calls: Vec<Path>,
    methods: Vec<(Path, String)>,
    globals: Vec<Path>,
}

/// Collects every distinct top-level [`Path`] an `Expr::Call`/`Expr::FnRef`
/// reachable from `typed` refers to, in first-encounter order. Mirrors
/// exactly the node shapes [`ast_to_sexpr_scoped`] actually recurses into —
/// anything that becomes `unsupported` there has no calls worth collecting,
/// since translating that node would fail before ever reaching them anyway.
///
/// JIT-only (labels/closures Stage 3): `Interp::compile_function` calls this
/// *before* running the typelisp compiler body, to learn which other
/// already-`compile`d top-level functions this body's `(call ...)`s need
/// forward-declared (no body) in its own throwaway module and wired to their
/// real address via `add_global_mapping` — `compile-call`'s `get-function`
/// would otherwise panic, not knowing about anything outside the module it
/// was handed. `compile::aot::compile_file` needs none of this: its one
/// shared module already has every earlier `defun`'s real body in it by the
/// time a later one's call needs to find it.
pub fn collect_call_targets(typed: &Typed) -> Vec<Path> {
    let mut targets = CallTargets::default();
    collect_calls(typed, &mut targets);
    targets.calls
}

/// The `Expr::Assoc` counterpart of [`collect_call_targets`]: every
/// `(type_name, method)` pair this body calls as an instance/static method,
/// in first-encounter order — the composability gap `compile-assoc` used to
/// dead-end into an "unsupported receiver type" panic for (see that
/// function's doc comment in `compiler.rs`). `Interp::compile_function` uses
/// this to require — and forward-declare, under the same mangled
/// `type-name::method` name `compile-assoc` itself looks up — every
/// *user-defined* method target (a `defmethod`/`defstruct` accessor/setter),
/// exactly the way it already does for a plain `Expr::Call` target; a
/// built-in receiver (`i64`/`i32`, compiled natively with no external call at
/// all) needs no such treatment, so the caller filters those out itself
/// rather than this walker trying to guess which `type_name`s are built in.
pub fn collect_assoc_targets(typed: &Typed) -> Vec<(Path, String)> {
    let mut targets = CallTargets::default();
    collect_calls(typed, &mut targets);
    targets.methods
}

/// The `Expr::Global`/`Expr::SetGlobal` counterpart of
/// [`collect_call_targets`]: every distinct global variable this body reads
/// or assigns, in first-encounter order. `Interp::compile_function` uses
/// this to promote each one to a compiled-global slot (a permanent GC root
/// — see `typelisp_rt::rt_global_new`) before generating IR, the same
/// "resolve every external dependency up front" shape it already applies to
/// `collect_call_targets`/`collect_assoc_targets`.
pub fn collect_global_targets(typed: &Typed) -> Vec<Path> {
    let mut targets = CallTargets::default();
    collect_calls(typed, &mut targets);
    targets.globals
}

fn collect_calls(typed: &Typed, targets: &mut CallTargets) {
    match &typed.expr {
        Expr::Call(path, args) => {
            if !targets.calls.contains(path) {
                targets.calls.push(path.clone());
            }
            for a in args {
                collect_calls(a, targets);
            }
        }
        // `translate_fnref` turns this into a forwarding `(call
        // path-local-name ...)` wrapper, so `path` needs the same
        // pre-declaration treatment as a real `Expr::Call` would.
        Expr::FnRef(path) => {
            if !targets.calls.contains(path) {
                targets.calls.push(path.clone());
            }
        }
        Expr::Assoc { type_name, method, args, .. } => {
            let key = (type_name.clone(), method.clone());
            if !targets.methods.contains(&key) {
                targets.methods.push(key);
            }
            for a in args {
                collect_calls(a, targets);
            }
        }
        Expr::Apply(callee, args) => {
            collect_calls(callee, targets);
            for a in args {
                collect_calls(a, targets);
            }
        }
        // Covers both an escaping `lambda` value's own body and an
        // immediately-invoked lambda literal's body (the latter reached via
        // the `Apply` arm above recursing into its `callee`, which is this
        // same `Expr::Lambda` node) — either way, a `Call` inside it still
        // needs the same JIT-only pre-declaration treatment.
        Expr::Lambda { body, .. } => {
            for e in body {
                collect_calls(e, targets);
            }
        }
        Expr::Labels { defs, body } => {
            for (_, _, def_body) in defs {
                for e in def_body {
                    collect_calls(e, targets);
                }
            }
            for e in body {
                collect_calls(e, targets);
            }
        }
        // if/let/comparisons, labels/closures Stage 5: a `Call` can sit
        // inside either branch of an `If` or a `Let`'s binding values/body —
        // both are now real translations (see `translate_if`/`translate_let`),
        // so this walker must follow them too, the same way it already
        // follows `Labels`'/`Lambda`'s bodies.
        Expr::If(cond, then, els) => {
            collect_calls(cond, targets);
            collect_calls(then, targets);
            collect_calls(els, targets);
        }
        Expr::Let(binds, body) => {
            for (_, value) in binds {
                collect_calls(value, targets);
            }
            for e in body {
                collect_calls(e, targets);
            }
        }
        // `loop`/`break`/`return`/`setf`: a `Call` can sit inside a loop's
        // body sequence, a `setf`'s new value, or a `return`'s value — all
        // now real translations (see `translate_loop`/`translate_set`/
        // `translate_return`), so this walker must follow them too. `Break`
        // carries no sub-expression at all.
        Expr::Loop(body) => {
            for e in body {
                collect_calls(e, targets);
            }
        }
        Expr::Set(_, value) => collect_calls(value, targets),
        Expr::Return(Some(v)) => collect_calls(v, targets),
        Expr::Return(None) => {}
        // Stage 5: a `Call` can sit in a `match`'s scrutinee or any arm's
        // body (`translate_match`'s own translation is now real) — pattern
        // trees themselves never contain a `Call` (a [`Pattern`] has no
        // sub-expression slot at all), so only the scrutinee/bodies need
        // walking here.
        Expr::Match(scrut, arms) => {
            collect_calls(scrut, targets);
            for arm in arms {
                for e in &arm.body {
                    collect_calls(e, targets);
                }
            }
        }
        // Stage 6: a `Call` can sit among a `Construct`'s field arguments,
        // or in a `FieldGet`/`FieldSet`'s own object/value sub-expressions —
        // all now real translations (see `translate_construct`/
        // `translate_field_get`/`translate_field_set`), so this walker must
        // follow them too.
        Expr::Construct { args, .. } => {
            for a in args {
                collect_calls(a, targets);
            }
        }
        Expr::FieldGet(obj, _) => collect_calls(obj, targets),
        Expr::FieldSet(obj, _, value) => {
            collect_calls(obj, targets);
            collect_calls(value, targets);
        }
        Expr::Global(path) => {
            if !targets.globals.contains(path) {
                targets.globals.push(path.clone());
            }
        }
        Expr::SetGlobal(path, value) => {
            if !targets.globals.contains(path) {
                targets.globals.push(path.clone());
            }
            collect_calls(value, targets);
        }
        // `translate_methodref` turns this into a forwarding `(assoc
        // type-name method true ...)` wrapper (mirroring `Expr::FnRef`'s own
        // forwarding-`call`-wrapper treatment above), so `(type_name,
        // method)` needs the exact same pre-declaration treatment a direct
        // `Expr::Assoc` call already gets.
        Expr::MethodRef { type_name, method } => {
            let key = (type_name.clone(), method.clone());
            if !targets.methods.contains(&key) {
                targets.methods.push(key);
            }
        }
        // A `Call` can sit inside `panic`'s own message expression.
        Expr::Panic(msg) => collect_calls(msg, targets),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    fn typed(expr: Expr, ty: Type) -> Typed {
        Typed { loc: None, expr, ty }
    }

    /// Shadows `super::ast_to_sexpr` with empty `structs`/`enums` sets and
    /// no promoted globals — the pre-existing tests here never involve a
    /// struct-typed field, an enum-typed global, or a global reference, so
    /// threading them through each call adds nothing. A test that *does*
    /// care (nested-struct field classification, `Global`/`SetGlobal`)
    /// calls `super::ast_to_sexpr` with real ones instead.
    fn ast_to_sexpr(heap: &mut Heap, typed: &Typed) -> Result<Value, Error> {
        super::ast_to_sexpr(heap, typed, &HashSet::new(), &HashSet::new(), &HashMap::new())
    }

    /// Unpacks a tagged-list `Value` into (tag name, field values), asserting
    /// it's a proper list (every `cdr` until the final `Empty` is itself a
    /// `Cons`, matching `tagged`'s own construction).
    fn untag(heap: &Heap, v: Value) -> (String, Vec<Value>) {
        let sym = heap.car(v).expect("tagged list has a car");
        let tag = match sym {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            other => panic!("expected a tag symbol, got {:?}", other),
        };
        let mut fields = Vec::new();
        let mut rest = heap.cdr(v).expect("tagged list has a cdr");
        while !rest.is_empty() {
            fields.push(heap.car(rest).expect("list field has a car"));
            rest = heap.cdr(rest).expect("list field has a cdr");
        }
        (tag, fields)
    }

    /// Unwraps a tagged call-argument pair `(kind . form)` — see
    /// `tagged_ast_list_to_sexpr`.
    fn untag_arg(heap: &Heap, pair: Value) -> (i64, Value) {
        let kind = match heap.car(pair).expect("arg pair has a car") {
            Value::Int(n) => n,
            other => panic!("expected an Int kind tag, got {:?}", other),
        };
        let form = heap.cdr(pair).expect("arg pair has a cdr");
        (kind, form)
    }

    /// Unwraps a tagged name pair `(name . kind)` — see `tagged_sym_list`.
    fn untag_name(heap: &Heap, pair: Value) -> (String, i64) {
        let name = match heap.car(pair).expect("name pair has a car") {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            other => panic!("expected a Sym, got {:?}", other),
        };
        let kind = match heap.cdr(pair).expect("name pair has a cdr") {
            Value::Int(n) => n,
            other => panic!("expected an Int kind tag, got {:?}", other),
        };
        (name, kind)
    }

    #[test]
    fn translates_an_int_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Int(42), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "int");
        assert_eq!(fields, vec![Value::Int(42)]);
    }

    #[test]
    fn translates_a_bool_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Bool(true), Type::Bool)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "bool");
        assert_eq!(fields, vec![Value::Bool(true)]);
    }

    #[test]
    fn translates_a_str_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Str("hi".to_string()), Type::Str)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "str");
        // Each character is its own `(int c)` node (not a pre-allocated
        // `Value::Str`) — see `ast_to_sexpr_scoped`'s `Expr::Str` doc
        // comment for why.
        let codepoints: Vec<i64> = fields
            .iter()
            .map(|f| {
                let (tag, fields) = untag(&heap, *f);
                assert_eq!(tag, "int");
                match fields[0] {
                    Value::Int(n) => n,
                    other => panic!("expected an Int, got {:?}", other),
                }
            })
            .collect();
        assert_eq!(codepoints, vec!['h' as i64, 'i' as i64]);
    }

    #[test]
    fn translates_an_empty_str_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Str(String::new()), Type::Str)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "str");
        assert!(fields.is_empty());
    }

    #[test]
    fn translates_a_var_reference() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Var("a".to_string()), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "var");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "a"),
            other => panic!("expected a Str, got {:?}", other),
        }
        assert_eq!(fields[1], Value::Bool(false), "an I64-typed var is never Fn-typed");
    }

    #[test]
    fn translates_an_instance_method_call() {
        let mut heap = Heap::with_capacity(1 << 10);
        let a = typed(Expr::Var("a".to_string()), Type::I64);
        let b = typed(Expr::Var("b".to_string()), Type::I64);
        let assoc = Expr::Assoc {
            type_name: crate::Path::root("i64"),
            method: "+".to_string(),
            instance: true,
            args: vec![a, b],
        };
        let v = ast_to_sexpr(&mut heap, &typed(assoc, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "assoc");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "i64"),
            other => panic!("expected a Str, got {:?}", other),
        }
        match fields[1] {
            Value::Str(id) => assert_eq!(heap.string(id), "+"),
            other => panic!("expected a Str, got {:?}", other),
        }
        assert_eq!(fields[2], Value::Bool(true));
        let (arg0_kind, arg0_form) = untag_arg(&heap, fields[3]);
        assert_eq!(arg0_kind, 0, "an I64 argument is KIND_PLAIN");
        let (arg0_tag, arg0_fields) = untag(&heap, arg0_form);
        assert_eq!(arg0_tag, "var");
        match arg0_fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "a"),
            other => panic!("expected a Str, got {:?}", other),
        }
        let (_, arg1_form) = untag_arg(&heap, fields[4]);
        let (arg1_tag, _) = untag(&heap, arg1_form);
        assert_eq!(arg1_tag, "var");
    }

    #[test]
    fn an_unimplemented_node_becomes_an_explicit_unsupported_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrutinee = Box::new(typed(Expr::Int(1), Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Match(scrutinee, Vec::new()), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "unsupported");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "Match"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    fn fn_ty() -> Type {
        Type::Fn(vec![Type::I64], None, Box::new(Type::I64))
    }

    fn expect_str(heap: &Heap, v: Value) -> String {
        match v {
            Value::Str(id) => heap.string(id).to_string(),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    /// Walks a proper `Sexpr` list into a `Vec` of its elements (`car`/`cdr`
    /// until `Empty`), the same loop shape several tests below repeat.
    fn list_elems(heap: &Heap, mut list: Value) -> Vec<Value> {
        let mut elems = Vec::new();
        while !list.is_empty() {
            elems.push(heap.car(list).unwrap());
            list = heap.cdr(list).unwrap();
        }
        elems
    }

    /// `Expr::Labels { defs: [(f, .. (g x)), (g, .. x)], body: (f 5) }` —
    /// `f` calls its sibling `g` directly, and the trailing body calls `f`
    /// directly too. Confirms both the `(labels (captured...) ((name
    /// (params) body)...) trailing-body)` shape (an empty captured list,
    /// since neither def references anything outside its own
    /// parameters/siblings — labels/closures Stage 1 scope) and that
    /// sibling/self calls become `(apply name arg...)`, not `(unsupported
    /// "Apply")`.
    #[test]
    fn translates_a_labels_form_with_a_sibling_call() {
        let mut heap = Heap::with_capacity(1 << 10);
        let g_body = vec![typed(Expr::Var("x".to_string()), Type::I64)];
        let f_body = vec![typed(
            Expr::Apply(
                Box::new(typed(Expr::Var("g".to_string()), fn_ty())),
                vec![typed(Expr::Var("x".to_string()), Type::I64)],
            ),
            Type::I64,
        )];
        let defs = vec![
            ("f".to_string(), vec![("x".to_string(), Type::I64)], f_body),
            ("g".to_string(), vec![("x".to_string(), Type::I64)], g_body),
        ];
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("f".to_string()), fn_ty())), vec![typed(Expr::Int(5), Type::I64)]),
            Type::I64,
        )];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Labels { defs, body }, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");
        assert!(fields[0].is_empty(), "expected an empty captured list, got {:?}", fields[0]);

        let defs_seen = list_elems(&heap, fields[1]);
        assert_eq!(defs_seen.len(), 2);

        let f_def = defs_seen[0];
        assert_eq!(expect_str(&heap, heap.car(f_def).unwrap()), "f");
        let f_rest = heap.cdr(f_def).unwrap();
        let f_params = heap.car(f_rest).unwrap();
        let (param0_name, _) = untag_name(&heap, heap.car(f_params).unwrap());
        assert_eq!(param0_name, "x");
        let f_body_v = heap.car(heap.cdr(f_rest).unwrap()).unwrap();
        let (f_body_tag, f_body_fields) = untag(&heap, f_body_v);
        assert_eq!(f_body_tag, "apply");
        assert_eq!(expect_str(&heap, f_body_fields[0]), "g");

        let (body_tag, body_fields) = untag(&heap, fields[2]);
        assert_eq!(body_tag, "apply");
        assert_eq!(expect_str(&heap, body_fields[0]), "f");
    }

    /// labels/closures Stage 2: `go`'s body references `offset`, an outer
    /// (enclosing-`defun`) name that's neither its own parameter nor a
    /// sibling — confirms the captured list is non-empty and carries that
    /// name, as a `Sym` (matching the parameter list's own convention).
    #[test]
    fn translates_a_labels_form_that_captures_an_outer_scope_name() {
        let mut heap = Heap::with_capacity(1 << 10);
        let go_body = vec![typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![typed(Expr::Var("k".to_string()), Type::I64), typed(Expr::Var("offset".to_string()), Type::I64)],
            },
            Type::I64,
        )];
        let defs = vec![("go".to_string(), vec![("k".to_string(), Type::I64)], go_body)];
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("go".to_string()), fn_ty())), vec![typed(Expr::Int(1), Type::I64)]),
            Type::I64,
        )];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Labels { defs, body }, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");

        let captured = list_elems(&heap, fields[0]);
        assert_eq!(captured.len(), 1);
        let (name, kind) = untag_name(&heap, captured[0]);
        assert_eq!(name, "offset");
        assert_eq!(kind, KIND_PLAIN);
    }

    /// A call to a function value that *isn't* a currently in-scope `labels`
    /// sibling/self (here: no enclosing `labels` at all) has no direct-call
    /// target to resolve to — labels/closures Stage 4 makes this a real
    /// `apply-indirect` translation (`callee`'s value, evaluated and called
    /// through `build-closure-apply` at runtime) rather than `unsupported`.
    #[test]
    fn an_apply_to_a_name_outside_the_current_labels_scope_is_indirect() {
        let mut heap = Heap::with_capacity(1 << 10);
        let callee = typed(Expr::Var("not-a-sibling".to_string()), fn_ty());
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Apply(Box::new(callee), vec![typed(Expr::Int(1), Type::I64)]), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "apply-indirect");
        let (callee_tag, callee_fields) = untag(&heap, fields[0]);
        assert_eq!(callee_tag, "var");
        assert_eq!(expect_str(&heap, callee_fields[0]), "not-a-sibling");
        let (kind, arg_form) = untag_arg(&heap, fields[1]);
        assert_eq!(kind, KIND_PLAIN);
        let (arg_tag, _) = untag(&heap, arg_form);
        assert_eq!(arg_tag, "int");
    }

    /// labels/closures Stage 3: a top-level `Expr::Call` becomes `(call name
    /// arg...)` — always a real translation, never `unsupported`, since
    /// `Checker::resolve_fn` guarantees the callee is already a registered
    /// `defun` (see `translate_call`'s doc comment). The embedded name gets
    /// the `tl_` `USER_SYMBOL_PREFIX` prefix — every user-defined function's
    /// own LLVM symbol name, so a user function can never collide with a
    /// libc/libm symbol LLVM itself calls (e.g. `frem` lowering to `fmod`).
    #[test]
    fn translates_a_call_to_another_top_level_function() {
        let mut heap = Heap::with_capacity(1 << 10);
        let call = Expr::Call(crate::Path::root("square"), vec![typed(Expr::Var("a".to_string()), Type::I64)]);
        let v = ast_to_sexpr(&mut heap, &typed(call, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "call");
        assert_eq!(expect_str(&heap, fields[0]), "tl_square");
        let (kind, arg_form) = untag_arg(&heap, fields[1]);
        assert_eq!(kind, KIND_PLAIN);
        let (arg_tag, arg_fields) = untag(&heap, arg_form);
        assert_eq!(arg_tag, "var");
        assert_eq!(expect_str(&heap, arg_fields[0]), "a");
    }

    /// Only the local segment of a qualified `Path` survives translation —
    /// `compiler.rs`'s `compile-call` looks callees up by plain (prefixed)
    /// name, the same way every compiled top-level function is itself
    /// declared (see `translate_call`'s doc comment).
    #[test]
    fn translates_a_call_keeping_only_the_paths_local_segment() {
        let mut heap = Heap::with_capacity(1 << 10);
        let call = Expr::Call(crate::Path::of(&["geo", "distance"]), vec![]);
        let v = ast_to_sexpr(&mut heap, &typed(call, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "call");
        assert_eq!(expect_str(&heap, fields[0]), "tl_distance");
    }

    #[test]
    fn collect_call_targets_is_empty_for_a_body_with_no_calls() {
        let v = typed(Expr::Int(1), Type::I64);
        assert!(collect_call_targets(&v).is_empty());
    }

    #[test]
    fn collect_call_targets_finds_a_direct_top_level_call() {
        let v = typed(Expr::Call(crate::Path::root("g"), vec![typed(Expr::Int(1), Type::I64)]), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// `collect_call_targets` recurses into a `labels` block's def bodies and
    /// trailing body too — a top-level `Expr::Call` can appear nested inside
    /// either (e.g. a `labels` sibling itself calling out to another
    /// top-level `defun`), and `Interp::compile_function` needs every one of
    /// them pre-declared, not just calls sitting directly in the outer body.
    #[test]
    fn collect_call_targets_finds_a_call_nested_inside_a_labels_def_body() {
        let inner = typed(Expr::Call(crate::Path::root("helper"), vec![typed(Expr::Int(1), Type::I64)]), Type::I64);
        let defs = vec![("f".to_string(), vec![], vec![inner])];
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("f".to_string()), fn_ty())), vec![]),
            Type::I64,
        )];
        let v = typed(Expr::Labels { defs, body }, Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("helper")]);
    }

    #[test]
    fn collect_call_targets_deduplicates_repeated_calls_to_the_same_target() {
        let v = typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![
                    typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64),
                    typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64),
                ],
            },
            Type::I64,
        );
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// labels/closures Stage 4: a standalone (escaping) `Expr::Lambda` ->
    /// `(lambda name (captured...) (param-sym...) single-body-form)`, with
    /// an empty captured list when the body only references its own
    /// parameter.
    #[test]
    fn translates_an_escaping_lambda() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = Expr::Lambda {
            params: vec![("y".to_string(), Type::I64)],
            body: vec![typed(Expr::Var("y".to_string()), Type::I64)],
        };
        let v = ast_to_sexpr(&mut heap, &typed(lambda, fn_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        assert!(!expect_str(&heap, fields[0]).is_empty(), "expected a non-empty synthesized name");
        assert!(fields[1].is_empty(), "expected an empty captured list, got {:?}", fields[1]);
        let params = list_elems(&heap, fields[2]);
        assert_eq!(params.len(), 1);
        let (param0_name, kind) = untag_name(&heap, params[0]);
        assert_eq!(param0_name, "y");
        assert_eq!(kind, KIND_PLAIN);
        let (body_tag, body_fields) = untag(&heap, fields[3]);
        assert_eq!(body_tag, "var");
        assert_eq!(expect_str(&heap, body_fields[0]), "y");
    }

    /// Two separately-translated lambdas get distinct synthesized names —
    /// `fresh_lambda_name`'s whole reason for existing (no two anonymous
    /// functions can collide once compiled into the same shared module).
    #[test]
    fn two_escaping_lambdas_get_distinct_synthesized_names() {
        let mut heap = Heap::with_capacity(1 << 10);
        let make = || Expr::Lambda { params: vec![], body: vec![typed(Expr::Int(1), Type::I64)] };
        let v1 = ast_to_sexpr(&mut heap, &typed(make(), fn_ty())).unwrap();
        let v2 = ast_to_sexpr(&mut heap, &typed(make(), fn_ty())).unwrap();
        let (_, f1) = untag(&heap, v1);
        let (_, f2) = untag(&heap, v2);
        assert_ne!(expect_str(&heap, f1[0]), expect_str(&heap, f2[0]));
    }

    /// labels/closures Stage 4: a `lambda` value that captures an outer name
    /// has it in the captured list, the same as a `labels` block would.
    #[test]
    fn translates_a_lambda_capturing_an_outer_name() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = Expr::Lambda {
            params: vec![("y".to_string(), Type::I64)],
            body: vec![typed(
                Expr::Assoc {
                    type_name: crate::Path::root("i64"),
                    method: "+".to_string(),
                    instance: true,
                    args: vec![typed(Expr::Var("y".to_string()), Type::I64), typed(Expr::Var("x".to_string()), Type::I64)],
                },
                Type::I64,
            )],
        };
        let v = ast_to_sexpr(&mut heap, &typed(lambda, fn_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        let captured = list_elems(&heap, fields[1]);
        assert_eq!(captured.len(), 1);
        let (name, kind) = untag_name(&heap, captured[0]);
        assert_eq!(name, "x");
        assert_eq!(kind, KIND_PLAIN);
    }

    /// labels/closures Stage 4: `((lambda (params) body) args...)` becomes a
    /// single-def, non-recursive `labels` block — not a `lambda` tag at
    /// all — proving the immediate-call path never boxes the function.
    #[test]
    fn translates_an_immediately_invoked_lambda_as_a_single_def_labels_block() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = typed(
            Expr::Lambda { params: vec![("y".to_string(), Type::I64)], body: vec![typed(Expr::Var("y".to_string()), Type::I64)] },
            fn_ty(),
        );
        let apply = Expr::Apply(Box::new(lambda), vec![typed(Expr::Int(5), Type::I64)]);
        let v = ast_to_sexpr(&mut heap, &typed(apply, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");
        assert!(fields[0].is_empty(), "expected an empty captured list, got {:?}", fields[0]);
        let defs_seen = list_elems(&heap, fields[1]);
        assert_eq!(defs_seen.len(), 1);
        let (body_tag, body_fields) = untag(&heap, fields[2]);
        assert_eq!(body_tag, "apply");
        let def_name = expect_str(&heap, heap.car(defs_seen[0]).unwrap());
        assert_eq!(expect_str(&heap, body_fields[0]), def_name);
    }

    /// labels/closures Stage 4: `Expr::FnRef(path)` becomes a non-capturing
    /// `lambda` that forwards positionally-synthesized arguments straight
    /// through to a `(call path-local-name ...)` — the embedded name is
    /// `tl_`-prefixed, same as an ordinary `Expr::Call` (see
    /// `translate_call`'s test above).
    #[test]
    fn translates_an_fnref_as_a_forwarding_lambda() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ty = Type::Fn(vec![Type::I64, Type::I64], None, Box::new(Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::FnRef(crate::Path::root("add2")), ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        assert!(fields[1].is_empty(), "expected an empty captured list, got {:?}", fields[1]);
        let params = list_elems(&heap, fields[2]);
        assert_eq!(params.len(), 2);
        let (body_tag, body_fields) = untag(&heap, fields[3]);
        assert_eq!(body_tag, "call");
        assert_eq!(expect_str(&heap, body_fields[0]), "tl_add2");
        assert_eq!(body_fields.len() - 1, 2, "expected one forwarded arg per parameter");
        for (i, param) in params.iter().enumerate() {
            let (param_name, _) = untag_name(&heap, *param);
            let (_, arg_form) = untag_arg(&heap, body_fields[1 + i]);
            let (arg_tag, arg_fields) = untag(&heap, arg_form);
            assert_eq!(arg_tag, "var");
            assert_eq!(expect_str(&heap, arg_fields[0]), param_name);
        }
    }

    /// `Expr::Panic(msg)` -> `(panic msg-form)`.
    #[test]
    fn translates_a_panic() {
        let mut heap = Heap::with_capacity(1 << 10);
        let msg = typed(Expr::Str("boom".to_string()), Type::Str);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Panic(Box::new(msg)), Type::Never)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "panic");
        assert_eq!(fields.len(), 1);
        let (msg_tag, _) = untag(&heap, fields[0]);
        assert_eq!(msg_tag, "str");
    }

    /// `Expr::MethodRef { type_name, method }` becomes a non-capturing
    /// `lambda` that forwards positionally-synthesized arguments straight
    /// through to an `(assoc type-name method true ...)` call — the
    /// `Expr::FnRef` forwarding-wrapper trick, generalized to an instance
    /// method (`arg0` is the receiver, matching `Expr::Assoc`'s own
    /// convention).
    #[test]
    fn translates_a_methodref_as_a_forwarding_lambda() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ty = Type::Fn(vec![Type::I64, Type::I64], None, Box::new(Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::MethodRef { type_name: Path::root("i64"), method: "+".to_string() }, ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        assert!(fields[1].is_empty(), "expected an empty captured list, got {:?}", fields[1]);
        let params = list_elems(&heap, fields[2]);
        assert_eq!(params.len(), 2);
        let (body_tag, body_fields) = untag(&heap, fields[3]);
        assert_eq!(body_tag, "assoc");
        assert_eq!(expect_str(&heap, body_fields[0]), "i64");
        assert_eq!(expect_str(&heap, body_fields[1]), "+");
        assert_eq!(body_fields[2], Value::Bool(true), "the receiver is an instance method call");
        assert_eq!(body_fields.len() - 3, 2, "expected one forwarded arg per parameter");
        for (i, param) in params.iter().enumerate() {
            let (param_name, _) = untag_name(&heap, *param);
            let (_, arg_form) = untag_arg(&heap, body_fields[3 + i]);
            let (arg_tag, arg_fields) = untag(&heap, arg_form);
            assert_eq!(arg_tag, "var");
            assert_eq!(expect_str(&heap, arg_fields[0]), param_name);
        }
    }

    /// `Expr::Quote(datum)` reuses `translate_construct`'s own wire shape:
    /// `(construct true false empty variant arg-form...)`, built directly
    /// from the literal `QuotedSexpr` — an `Int` leaf here.
    #[test]
    fn translates_a_quoted_int() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Quote(QuotedSexpr::Int(5)), sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[0], Value::Bool(true), "a quoted literal is always Sexpr");
        assert_eq!(fields[1], Value::Bool(false));
        assert!(fields[2].is_empty(), "type-name-str is an unused placeholder for the is-sexpr branch");
        assert_eq!(fields[3], Value::Int(1), "Int is Sexpr variant 1");
        let (leaf_tag, leaf_fields) = untag(&heap, fields[4]);
        assert_eq!(leaf_tag, "int");
        assert_eq!(leaf_fields, vec![Value::Int(5)]);
    }

    /// A quoted list (`Cons` of `Cons`es, terminated by `Nil`) translates
    /// recursively — each `Cons` becomes its own nested `construct` node,
    /// matching `compile-construct-sexpr`'s own variant-7 (`Cons`) arm, which
    /// expects each field to itself be a `compile-value`-dispatchable form.
    #[test]
    fn translates_a_quoted_list_recursively() {
        let mut heap = Heap::with_capacity(1 << 10);
        let datum = QuotedSexpr::Cons(Box::new(QuotedSexpr::Int(1)), Box::new(QuotedSexpr::Nil));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Quote(datum), sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[3], Value::Int(7), "Cons is Sexpr variant 7");
        let (car_tag, car_fields) = untag(&heap, fields[4]);
        assert_eq!(car_tag, "construct");
        assert_eq!(car_fields[3], Value::Int(1));
        let (cdr_tag, cdr_fields) = untag(&heap, fields[5]);
        assert_eq!(cdr_tag, "construct");
        assert_eq!(cdr_fields[3], Value::Int(0), "Nil is Sexpr variant 0");
    }

    /// A quoted symbol nested inside a `Cons` compiles: `translate_quote`'s
    /// `Sym` arm produces a `(construct true false empty 5 name-form)` node
    /// like any other leaf, recursed into the same way `Int`/`Str`/... are.
    #[test]
    fn a_quoted_symbol_nested_in_a_cons_compiles() {
        let mut heap = Heap::with_capacity(1 << 10);
        let datum = QuotedSexpr::Cons(Box::new(QuotedSexpr::Sym("foo".to_string())), Box::new(QuotedSexpr::Nil));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Quote(datum), sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[3], Value::Int(7), "Cons is Sexpr variant 7");
        let (car_tag, car_fields) = untag(&heap, fields[4]);
        assert_eq!(car_tag, "construct");
        assert_eq!(car_fields[3], Value::Int(5), "Sym is Sexpr variant 5");
        let (cdr_tag, cdr_fields) = untag(&heap, fields[5]);
        assert_eq!(cdr_tag, "construct");
        assert_eq!(cdr_fields[3], Value::Int(0), "Nil is Sexpr variant 0");
    }

    /// A quoted `::`-path compiles: `translate_quote`'s `Path` arm produces
    /// a `(construct true false empty 10 seg-form...)` node, one leaf per
    /// segment.
    #[test]
    fn a_quoted_path_compiles() {
        let mut heap = Heap::with_capacity(1 << 10);
        let datum = QuotedSexpr::Path(vec!["dep".to_string(), "head".to_string()]);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Quote(datum), sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[3], Value::Int(10), "Path is Sexpr variant 10");
        assert_eq!(fields.len(), 6, "2 segments -> 2 leaf fields after the 4 header fields");
    }

    /// if/let/comparisons, labels/closures Stage 5: `Expr::If` -> `(if is-fn
    /// cond-form then-form else-form)`. An `I64`-typed `if` carries `is-fn =
    /// false` — see `translate_if`'s doc comment for why this tag exists at
    /// all (an `if` doesn't freshen a borrowed branch value the way a
    /// function-boundary node does, so `compiler.rs`'s `compile-if` needs to
    /// know when it must).
    #[test]
    fn translates_an_if_expression() {
        let mut heap = Heap::with_capacity(1 << 10);
        let cond = typed(Expr::Bool(true), Type::Bool);
        let then = typed(Expr::Int(1), Type::I64);
        let els = typed(Expr::Int(2), Type::I64);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::If(Box::new(cond), Box::new(then), Box::new(els)), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "if");
        assert_eq!(fields[0], Value::Bool(false), "an I64-typed if is never Fn-typed");
        let (cond_tag, _) = untag(&heap, fields[1]);
        assert_eq!(cond_tag, "bool");
        let (then_tag, then_fields) = untag(&heap, fields[2]);
        assert_eq!(then_tag, "int");
        assert_eq!(then_fields, vec![Value::Int(1)]);
        let (else_tag, else_fields) = untag(&heap, fields[3]);
        assert_eq!(else_tag, "int");
        assert_eq!(else_fields, vec![Value::Int(2)]);
    }

    /// An `if` whose unified type is `Fn` (both branches return a closure)
    /// carries `is-fn = true` — `compile-if` uses this to know it must
    /// consider retaining a borrowed branch before the merge.
    #[test]
    fn an_fn_typed_if_carries_an_is_fn_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let cond = typed(Expr::Bool(true), Type::Bool);
        let then = typed(Expr::Var("f".to_string()), fn_ty());
        let els = typed(Expr::Var("g".to_string()), fn_ty());
        let v = ast_to_sexpr(&mut heap, &typed(Expr::If(Box::new(cond), Box::new(then), Box::new(els)), fn_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "if");
        assert_eq!(fields[0], Value::Bool(true));
    }

    /// if/let/comparisons, labels/closures Stage 5: `Expr::Let` -> `(let
    /// (((name-sym . kind) . value-form)...) body-form...)` — the nested
    /// `(name-sym . kind)` pair (reusing `tagged_sym_list`'s own shape)
    /// replaced a bare `name-sym` in Stage 6 of the Sexpr-representation
    /// plan, once a `let`-bound `Sexpr`-typed value needed the same
    /// GC-root push/pop bookkeeping a parameter/capture already gets (see
    /// `binding_kind`'s doc comment).
    #[test]
    fn translates_a_let_expression() {
        let mut heap = Heap::with_capacity(1 << 10);
        let binds = vec![("x".to_string(), typed(Expr::Int(5), Type::I64))];
        let body = vec![typed(Expr::Var("x".to_string()), Type::I64)];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Let(binds, body), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "let");
        let pairs = list_elems(&heap, fields[0]);
        assert_eq!(pairs.len(), 1);
        let (name, kind) = untag_name(&heap, heap.car(pairs[0]).unwrap());
        assert_eq!(name, "x");
        assert_eq!(kind, KIND_PLAIN);
        let (value_tag, value_fields) = untag(&heap, heap.cdr(pairs[0]).unwrap());
        assert_eq!(value_tag, "int");
        assert_eq!(value_fields, vec![Value::Int(5)]);
        assert_eq!(fields.len(), 2, "single-statement body: exactly one trailing field");
        let (body_tag, body_fields) = untag(&heap, fields[1]);
        assert_eq!(body_tag, "var");
        assert_eq!(expect_str(&heap, body_fields[0]), "x");
    }

    /// Stage 8 of the Sexpr-representation plan (`docs/implementation-log.md`): a `let`
    /// body may now hold more than one statement — translated the same
    /// variadic way `translate_loop` already translates its own body
    /// (`translates_a_loop_with_a_multi_statement_body`, above) — needed
    /// for `dolist`'s own macro expansion, whose inner `let` body is always
    /// `,@body` followed by a hidden `setf`.
    #[test]
    fn translates_a_let_expression_with_a_multi_statement_body() {
        let mut heap = Heap::with_capacity(1 << 10);
        let binds = vec![("x".to_string(), typed(Expr::Int(1), Type::I64))];
        let body = vec![typed(Expr::Break, Type::Never), typed(Expr::Var("x".to_string()), Type::I64)];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Let(binds, body), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "let");
        assert_eq!(fields.len(), 3, "bindings + 2 body statements");
        let (f1_tag, _) = untag(&heap, fields[1]);
        assert_eq!(f1_tag, "break");
        let (f2_tag, _) = untag(&heap, fields[2]);
        assert_eq!(f2_tag, "var");
    }

    /// The empty-body edge case (CL `let`'s own `Unit`-typed `(let
    /// ((x 1)))`) — no trailing fields at all besides the bindings list.
    #[test]
    fn translates_a_let_expression_with_an_empty_body() {
        let mut heap = Heap::with_capacity(1 << 10);
        let binds = vec![("x".to_string(), typed(Expr::Int(1), Type::I64))];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Let(binds, Vec::new()), Type::Unit)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "let");
        assert_eq!(fields.len(), 1, "bindings only, no body statements");
    }

    /// `collect_call_targets` recurses into both arms of an `If` — a `Call`
    /// inside either branch still needs JIT pre-declaration.
    #[test]
    fn collect_call_targets_finds_a_call_nested_inside_an_if_branch() {
        let cond = typed(Expr::Bool(true), Type::Bool);
        let then = typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64);
        let els = typed(Expr::Int(0), Type::I64);
        let v = typed(Expr::If(Box::new(cond), Box::new(then), Box::new(els)), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// `collect_call_targets` recurses into both a `Let`'s binding values and
    /// its body.
    #[test]
    fn collect_call_targets_finds_a_call_nested_inside_a_let_binding() {
        let binds = vec![("x".to_string(), typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64))];
        let body = vec![typed(Expr::Var("x".to_string()), Type::I64)];
        let v = typed(Expr::Let(binds, body), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// `loop`/`break`/`return`/`setf`: `Expr::Loop` -> `(loop form...)`, each
    /// body statement translated in order, untagged.
    #[test]
    fn translates_a_loop_with_a_multi_statement_body() {
        let mut heap = Heap::with_capacity(1 << 10);
        let body = vec![typed(Expr::Break, Type::Never), typed(Expr::Int(1), Type::I64)];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Loop(body), Type::Unit)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "loop");
        assert_eq!(fields.len(), 2);
        let (f0_tag, _) = untag(&heap, fields[0]);
        assert_eq!(f0_tag, "break");
        let (f1_tag, f1_fields) = untag(&heap, fields[1]);
        assert_eq!(f1_tag, "int");
        assert_eq!(f1_fields, vec![Value::Int(1)]);
    }

    /// `Expr::Break` -> a bare `(break)` tag, no fields.
    #[test]
    fn translates_a_break() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Break, Type::Never)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "break");
        assert!(fields.is_empty());
    }

    /// `Expr::Return(Some(value))` -> `(return is-fn value-form)`.
    #[test]
    fn translates_a_return_with_a_value() {
        let mut heap = Heap::with_capacity(1 << 10);
        let value = Box::new(typed(Expr::Int(5), Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Return(Some(value)), Type::Never)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "return");
        assert_eq!(fields[0], Value::Bool(false));
        let (value_tag, value_fields) = untag(&heap, fields[1]);
        assert_eq!(value_tag, "int");
        assert_eq!(value_fields, vec![Value::Int(5)]);
    }

    /// `Expr::Return(None)` -> `(return false (unit))` — the implicit `Unit`
    /// value, not a second special-cased tag.
    #[test]
    fn translates_a_value_less_return_as_an_implicit_unit() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Return(None), Type::Never)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "return");
        assert_eq!(fields[0], Value::Bool(false));
        let (value_tag, value_fields) = untag(&heap, fields[1]);
        assert_eq!(value_tag, "unit");
        assert!(value_fields.is_empty());
    }

    /// A `Fn`-typed `return` value carries `is-fn = true` — the same tag
    /// `compile-return` forwards into `compile-if-branch`'s retain logic.
    #[test]
    fn an_fn_typed_return_value_carries_an_is_fn_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let value = Box::new(typed(Expr::Var("f".to_string()), fn_ty()));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Return(Some(value)), Type::Never)).unwrap();
        let (_, fields) = untag(&heap, v);
        assert_eq!(fields[0], Value::Bool(true));
    }

    /// `Expr::Set(name, value)` -> `(set name-str kind value-form)` — `ty`
    /// is the *target variable's* type (here `I64`, `KIND_PLAIN`), not
    /// `value`'s own (also `I64` here, but the two needn't be the same node).
    #[test]
    fn translates_a_setf() {
        let mut heap = Heap::with_capacity(1 << 10);
        let set = Expr::Set("x".to_string(), Box::new(typed(Expr::Int(9), Type::I64)));
        let v = ast_to_sexpr(&mut heap, &typed(set, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "set");
        assert_eq!(expect_str(&heap, fields[0]), "x");
        assert_eq!(fields[1], Value::Int(0), "I64 is KIND_PLAIN");
        let (value_tag, value_fields) = untag(&heap, fields[2]);
        assert_eq!(value_tag, "int");
        assert_eq!(value_fields, vec![Value::Int(9)]);
    }

    /// A `setf` whose target is `Sexpr`-typed carries `KIND_SEXPR` (`2`) —
    /// `compile-set`'s signal to update the target's GC-root entry in place
    /// via `rt_set_sexpr_root`, not just overwrite the value slot.
    #[test]
    fn translates_a_setf_targeting_a_sexpr_local_with_kind_sexpr() {
        let mut heap = Heap::with_capacity(1 << 10);
        let set = Expr::Set("s".to_string(), Box::new(typed(Expr::Var("s".to_string()), sexpr_ty())));
        let v = ast_to_sexpr(&mut heap, &typed(set, sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "set");
        assert_eq!(fields[1], Value::Int(2), "Sexpr is KIND_SEXPR");
    }

    /// `collect_call_targets` recurses into a `Loop`'s body, a `Set`'s
    /// value, and a `Return`'s value.
    #[test]
    fn collect_call_targets_finds_calls_nested_inside_loop_set_and_return() {
        let loop_body = vec![typed(Expr::Call(crate::Path::root("a"), vec![]), Type::I64)];
        let v = typed(Expr::Loop(loop_body), Type::Unit);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("a")]);

        let set = Expr::Set("x".to_string(), Box::new(typed(Expr::Call(crate::Path::root("b"), vec![]), Type::I64)));
        let v = typed(set, Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("b")]);

        let ret = Expr::Return(Some(Box::new(typed(Expr::Call(crate::Path::root("c"), vec![]), Type::I64))));
        let v = typed(ret, Type::Never);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("c")]);
    }

    // ---- Stage 5 of the Sexpr-representation plan: `Match` --------------

    fn sexpr_ty() -> Type {
        Type::Named(Path::root("sexpr"), vec![])
    }

    /// `consp`'s own shape (`prelude.rs`): `(match s ((Cons _ _) true) (_
    /// false)))` -> `(match is-fn scrutinee-form ((pattern-form .
    /// body-form)...))`, with every sub-pattern here a `Wildcard` (no
    /// `Bind` extraction needed at all).
    #[test]
    fn translates_a_match_over_a_sexpr_scrutinee_with_wildcard_ctor_patterns() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrut = typed(Expr::Var("s".to_string()), sexpr_ty());
        let arms = vec![
            Arm {
                pat: Pattern::Ctor { type_name: Path::root("sexpr"), variant: 7, args: vec![Pattern::Wildcard, Pattern::Wildcard], sexpr_fields: vec![true, true], field_types: vec![] },
                body: vec![typed(Expr::Bool(true), Type::Bool)],
            },
            Arm { pat: Pattern::Wildcard, body: vec![typed(Expr::Bool(false), Type::Bool)] },
        ];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Match(Box::new(scrut), arms), Type::Bool)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "match");
        assert_eq!(fields[0], Value::Bool(false), "a Bool-typed match is never Fn-typed");
        let (scrut_tag, scrut_fields) = untag(&heap, fields[1]);
        assert_eq!(scrut_tag, "var");
        assert_eq!(expect_str(&heap, scrut_fields[0]), "s");

        let arm_pairs = list_elems(&heap, fields[2]);
        assert_eq!(arm_pairs.len(), 2);

        let pat0 = heap.car(arm_pairs[0]).unwrap();
        let (pat0_tag, pat0_fields) = untag(&heap, pat0);
        assert_eq!(pat0_tag, "pat-ctor");
        assert_eq!(pat0_fields[0], Value::Int(7));
        let subpats = list_elems(&heap, pat0_fields[1]);
        assert_eq!(subpats.len(), 2);
        for sp in subpats {
            assert_eq!(untag(&heap, sp).0, "pat-wild");
        }
        let body0 = heap.cdr(arm_pairs[0]).unwrap();
        assert_eq!(untag(&heap, body0).0, "bool");

        let pat1 = heap.car(arm_pairs[1]).unwrap();
        assert_eq!(untag(&heap, pat1).0, "pat-wild");
    }

    /// `translate_match` only translates a scrutinee whose static type is
    /// `Type::Named` (`Sexpr`, a sum-ADT box, or a boxed struct — see
    /// [`match_scrut_kind`]); anything else (no real `match` scrutinee type
    /// in practice) stays `unsupported`.
    #[test]
    fn match_over_a_non_named_scrutinee_type_is_unsupported() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrut = typed(Expr::Int(0), Type::I64);
        let arms = vec![Arm { pat: Pattern::Wildcard, body: vec![typed(Expr::Bool(true), Type::Bool)] }];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Match(Box::new(scrut), arms), Type::Bool)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "unsupported");
        assert_eq!(expect_str(&heap, fields[0]), "Match");
    }

    /// A boxed-struct (`defstruct`) scrutinee now translates for real
    /// (`MATCH_KIND_STRUCT`): the `pat-ctor`'s `scrut-kind` field is `2`, and
    /// `field-kinds` carries each field's own `struct_field_kind` — `1` (int)
    /// for the first `i64` field here.
    #[test]
    fn translates_a_match_over_a_struct_scrutinee() {
        let mut heap = Heap::with_capacity(1 << 10);
        let point = Path::root("point");
        let mut structs = HashSet::new();
        structs.insert(point.clone());
        let globals = HashMap::new();
        let direct = HashSet::new();
        let enums = HashSet::new();
        let cx = Ctx { direct: &direct, outer_captured: &[], structs: &structs, enums: &enums, globals: &globals };

        let point_ty = Type::Named(point.clone(), vec![]);
        let scrut = typed(Expr::Var("p".to_string()), point_ty.clone());
        let arms = vec![Arm {
            pat: Pattern::Ctor {
                type_name: point,
                variant: 0,
                args: vec![Pattern::Bind("a".to_string(), false), Pattern::Bind("b".to_string(), false)],
                sexpr_fields: vec![false, false],
                field_types: vec![Type::I64, Type::I64],
            },
            body: vec![typed(Expr::Var("a".to_string()), Type::I64)],
        }];
        let v = translate_match(&mut heap, &scrut, &arms, &Type::I64, cx).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "match");
        assert_eq!(fields[3], Value::Int(MATCH_KIND_STRUCT), "a defstruct scrutinee is MATCH_KIND_STRUCT");

        let arm_pairs = list_elems(&heap, fields[2]);
        let pat0 = heap.car(arm_pairs[0]).unwrap();
        let (pat0_tag, pat0_fields) = untag(&heap, pat0);
        assert_eq!(pat0_tag, "pat-ctor");
        assert_eq!(pat0_fields[2], Value::Int(MATCH_KIND_STRUCT));
        let field_kinds = list_elems(&heap, pat0_fields[3]);
        assert_eq!(field_kinds, vec![Value::Int(1), Value::Int(1)], "both fields are i64 (struct_field_kind 1)");
    }

    /// A `Bind` sub-pattern (e.g. `(Cons h _)`) -> `(pat-bind name-str)`.
    #[test]
    fn translates_a_bind_subpattern_inside_a_ctor_pattern() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrut = typed(Expr::Var("s".to_string()), sexpr_ty());
        let arms = vec![Arm {
            pat: Pattern::Ctor { type_name: Path::root("sexpr"), variant: 7, args: vec![Pattern::Bind("h".to_string(), true), Pattern::Wildcard], sexpr_fields: vec![true, true], field_types: vec![] },
            body: vec![typed(Expr::Var("h".to_string()), sexpr_ty())],
        }];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Match(Box::new(scrut), arms), sexpr_ty())).unwrap();
        let (_, fields) = untag(&heap, v);
        let arm_pairs = list_elems(&heap, fields[2]);
        let pat0 = heap.car(arm_pairs[0]).unwrap();
        let (_, pat0_fields) = untag(&heap, pat0);
        let subpats = list_elems(&heap, pat0_fields[1]);
        let (bind_tag, bind_fields) = untag(&heap, subpats[0]);
        assert_eq!(bind_tag, "pat-bind");
        assert_eq!(expect_str(&heap, bind_fields[0]), "h");
        assert_eq!(untag(&heap, subpats[1]).0, "pat-wild");
    }

    /// A nested `Ctor` sub-pattern (`(Cons _ (Nil))` — `prelude.rs`'s
    /// `last`/`butlast`) translates recursively, not just one level deep.
    #[test]
    fn translates_a_nested_ctor_subpattern() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrut = typed(Expr::Var("s".to_string()), sexpr_ty());
        let inner_nil = Pattern::Ctor { type_name: Path::root("sexpr"), variant: 0, args: vec![], sexpr_fields: vec![], field_types: vec![] };
        let arms = vec![
            Arm {
                pat: Pattern::Ctor { type_name: Path::root("sexpr"), variant: 7, args: vec![Pattern::Wildcard, inner_nil], sexpr_fields: vec![true, true], field_types: vec![] },
                body: vec![typed(Expr::Bool(true), Type::Bool)],
            },
            Arm { pat: Pattern::Wildcard, body: vec![typed(Expr::Bool(false), Type::Bool)] },
        ];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Match(Box::new(scrut), arms), Type::Bool)).unwrap();
        let (_, fields) = untag(&heap, v);
        let arm_pairs = list_elems(&heap, fields[2]);
        let pat0 = heap.car(arm_pairs[0]).unwrap();
        let (_, pat0_fields) = untag(&heap, pat0);
        let subpats = list_elems(&heap, pat0_fields[1]);
        assert_eq!(untag(&heap, subpats[0]).0, "pat-wild");
        let (nested_tag, nested_fields) = untag(&heap, subpats[1]);
        assert_eq!(nested_tag, "pat-ctor");
        assert_eq!(nested_fields[0], Value::Int(0));
        assert!(list_elems(&heap, nested_fields[1]).is_empty());
    }

    /// `Pattern::Int`/`Bool`/`Char` all collapse to one shared `(pat-lit
    /// n)` tag, `n` precomputed to whatever raw `i64` the *compiled*
    /// representation of that literal would be — see `pattern_to_sexpr`'s
    /// doc comment for why (no `char`/`bool` -> `i64` conversion exists
    /// in the compiled language itself yet).
    #[test]
    fn int_bool_and_char_subpatterns_collapse_to_a_shared_pat_lit_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let pat_lit_payload = |heap: &mut Heap, pat: &Pattern| -> i64 {
            let direct = HashSet::new();
            let structs = HashSet::new();
            let enums = HashSet::new();
            let globals = HashMap::new();
            let cx = Ctx { direct: &direct, outer_captured: &[], structs: &structs, enums: &enums, globals: &globals };
            let v = pattern_to_sexpr(heap, pat, cx).unwrap();
            let (tag, fields) = untag(heap, v);
            assert_eq!(tag, "pat-lit");
            match fields[0] {
                Value::Int(n) => n,
                other => panic!("expected an Int payload, got {:?}", other),
            }
        };
        assert_eq!(pat_lit_payload(&mut heap, &Pattern::Int(5)), 5);
        assert_eq!(pat_lit_payload(&mut heap, &Pattern::Bool(true)), 1);
        assert_eq!(pat_lit_payload(&mut heap, &Pattern::Bool(false)), 0);
        assert_eq!(pat_lit_payload(&mut heap, &Pattern::Char('A')), 'A' as i64);
    }

    /// A `match` arm whose body is more than one expression isn't
    /// supported yet — mirrors `translate_let`/`translate_labels_def`'s
    /// own single-expression-body restriction.
    #[test]
    fn match_rejects_a_multi_expression_arm_body() {
        let mut heap = Heap::with_capacity(1 << 10);
        let scrut = typed(Expr::Var("s".to_string()), sexpr_ty());
        let arms = vec![Arm {
            pat: Pattern::Wildcard,
            body: vec![typed(Expr::Bool(true), Type::Bool), typed(Expr::Bool(false), Type::Bool)],
        }];
        let err = ast_to_sexpr(&mut heap, &typed(Expr::Match(Box::new(scrut), arms), Type::Bool)).unwrap_err();
        match err {
            Error::TypeError(msg) => assert!(msg.contains("multi-expression body"), "message was: {}", msg),
            other => panic!("expected a TypeError, got {:?}", other),
        }
    }

    /// `collect_call_targets` recurses into a `Match`'s scrutinee and
    /// every arm's body — a pattern tree itself never contains a `Call`
    /// (no sub-expression slot at all), so only those two need walking.
    #[test]
    fn collect_call_targets_finds_calls_nested_inside_a_match_scrutinee_and_arm_body() {
        let scrut = typed(Expr::Call(crate::Path::root("a"), vec![]), sexpr_ty());
        let arms = vec![Arm { pat: Pattern::Wildcard, body: vec![typed(Expr::Call(crate::Path::root("b"), vec![]), Type::I64)] }];
        let v = typed(Expr::Match(Box::new(scrut), arms), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("a"), crate::Path::root("b")]);
    }

    // ---- Stage 6 of the Sexpr-representation plan: Construct/FieldGet/FieldSet --

    /// `Expr::Construct` over `Sexpr` itself (`(Int n)`'s own typed shape)
    /// -> `(construct true false empty variant-i64 arg-form...)` —
    /// `is-sexpr` is `true` since the node's own checked type is `Sexpr`,
    /// dispatching `compiler.rs`'s `compile-construct` to
    /// `compile-construct-sexpr` rather than either boxed path; `mutable` is
    /// always `false` here, and `type-name-str` is an unused placeholder.
    #[test]
    fn translates_a_sexpr_construct() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ctor = Expr::Construct {
            type_name: Path::root("sexpr"),
            variant: 1,
            args: vec![typed(Expr::Int(5), Type::I64)],
            mutable: false,
        };
        let v = ast_to_sexpr(&mut heap, &typed(ctor, sexpr_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[0], Value::Bool(true), "Sexpr's own Construct dispatches to the tagged-i64 path");
        assert_eq!(fields[1], Value::Bool(false), "mutable is always false for a Sexpr construct");
        assert!(fields[2].is_empty(), "type-name-str is an unused placeholder for the is-sexpr branch");
        assert_eq!(fields[3], Value::Int(1));
        let (arg_tag, arg_fields) = untag(&heap, fields[4]);
        assert_eq!(arg_tag, "int");
        assert_eq!(arg_fields, vec![Value::Int(5)]);
    }

    /// `Expr::Construct` over a general ADT (`Option<i64>`'s `Some`, an
    /// `AdtKind::Sum`) -> `(construct false false empty variant-i64
    /// (kind . arg-form)...)` — `is-sexpr` is `false` and `mutable` is
    /// `false`, dispatching to `compile-construct-box`'s `malloc`'d-box path.
    /// Multiple args are each translated in order, each wrapped in the same
    /// `(kind . form)` shape `Expr::Call`'s own argument list gets
    /// (`tagged_ast_list_to_sexpr`/`binding_kind`) — an `i64` field is
    /// `KIND_PLAIN`, a `bool` field likewise (see `translate_construct`'s
    /// doc comment for why this is needed: `compile-construct-box-fields`
    /// reads `kind` to decide which fields need `rt_push_permanent_sexpr_root`).
    #[test]
    fn translates_a_general_adt_construct() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ctor = Expr::Construct {
            type_name: Path::root("option"),
            variant: 0,
            args: vec![typed(Expr::Int(7), Type::I64), typed(Expr::Bool(true), Type::Bool)],
            mutable: false,
        };
        let ty = Type::Named(Path::root("option"), vec![Type::I64]);
        let v = ast_to_sexpr(&mut heap, &typed(ctor, ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[0], Value::Bool(false), "a non-Sexpr ADT dispatches to a boxed path");
        assert_eq!(fields[1], Value::Bool(false), "an AdtKind::Sum construct is not mutable");
        assert!(fields[2].is_empty(), "type-name-str is an unused placeholder for the general-ADT branch");
        assert_eq!(fields[3], Value::Int(0));
        let (kind0, arg0_form) = untag_arg(&heap, fields[4]);
        assert_eq!(kind0, 0, "an i64 field is KIND_PLAIN");
        let (arg0_tag, arg0_fields) = untag(&heap, arg0_form);
        assert_eq!(arg0_tag, "int");
        assert_eq!(arg0_fields, vec![Value::Int(7)]);
        let (kind1, arg1_form) = untag_arg(&heap, fields[5]);
        assert_eq!(kind1, 0, "a bool field is KIND_PLAIN");
        let (arg1_tag, _) = untag(&heap, arg1_form);
        assert_eq!(arg1_tag, "bool");
    }

    /// A `Sexpr`-typed field of a general ADT (e.g. an `Option<Sexpr>`
    /// field) is tagged `KIND_SEXPR` — the case
    /// `compile-construct-box-fields` actually roots.
    #[test]
    fn a_sexpr_typed_field_of_a_general_adt_construct_is_tagged_kind_sexpr() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ctor = Expr::Construct {
            type_name: Path::root("holder"),
            variant: 0,
            args: vec![typed(Expr::Var("s".to_string()), sexpr_ty())],
            mutable: false,
        };
        let ty = Type::Named(Path::root("holder"), vec![]);
        let v = ast_to_sexpr(&mut heap, &typed(ctor, ty)).unwrap();
        let (_, fields) = untag(&heap, v);
        let (kind, _) = untag_arg(&heap, fields[4]);
        assert_eq!(kind, 2, "a Sexpr-typed field is KIND_SEXPR");
    }

    /// `Expr::Construct` over a `mutable` `defstruct` (`AdtKind::Struct`) ->
    /// `(construct false true type-name-str variant-i64 (kind . arg-form)...)`
    /// (Stage 3 of the Sexpr/RtValue unification plan,
    /// `docs/implementation-log.md`) — `mutable` is `true`, dispatching to
    /// `compile-construct-boxed-struct`'s `rt_struct_new` path.
    /// `type-name-str` is the struct's own local name, embedded as a
    /// `(str (int c0) ...)` literal form (`str_literal_form`) rather than a
    /// pre-allocated `Value::Str` (see that function's doc comment). Each
    /// field is tagged by `struct_field_kind`, not `binding_kind` — an `i64`
    /// field is kind `1`, a `bool` field kind `4`.
    #[test]
    fn translates_a_mutable_struct_construct() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ctor = Expr::Construct {
            type_name: Path::root("point"),
            variant: 0,
            args: vec![typed(Expr::Int(3), Type::I64), typed(Expr::Bool(true), Type::Bool)],
            mutable: true,
        };
        let ty = Type::Named(Path::root("point"), vec![]);
        let v = ast_to_sexpr(&mut heap, &typed(ctor, ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields[0], Value::Bool(false), "a defstruct is never Sexpr-typed");
        assert_eq!(fields[1], Value::Bool(true), "an AdtKind::Struct construct is mutable");
        let (name_tag, name_fields) = untag(&heap, fields[2]);
        assert_eq!(name_tag, "str", "type-name-str is a (str (int c)...) literal form");
        let (c0_tag, c0_fields) = untag(&heap, name_fields[0]);
        assert_eq!(c0_tag, "int");
        assert_eq!(c0_fields, vec![Value::Int('p' as i64)]);
        assert_eq!(fields[3], Value::Int(0));
        let (kind0, arg0_form) = untag_arg(&heap, fields[4]);
        assert_eq!(kind0, 1, "an i64 field is struct-field-kind Int");
        let (arg0_tag, arg0_fields) = untag(&heap, arg0_form);
        assert_eq!(arg0_tag, "int");
        assert_eq!(arg0_fields, vec![Value::Int(3)]);
        let (kind1, _) = untag_arg(&heap, fields[5]);
        assert_eq!(kind1, 4, "a bool field is struct-field-kind Bool");
    }

    /// A field-less `Construct` (e.g. `None`) still produces a well-formed
    /// `(construct false false empty variant-i64)` tag with no trailing
    /// argument forms.
    #[test]
    fn translates_a_field_less_construct() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ctor = Expr::Construct { type_name: Path::root("option"), variant: 1, args: vec![], mutable: false };
        let ty = Type::Named(Path::root("option"), vec![Type::I64]);
        let v = ast_to_sexpr(&mut heap, &typed(ctor, ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "construct");
        assert_eq!(fields.len(), 4, "no field-arguments beyond is-sexpr/mutable/type-name-str/variant");
    }

    /// `Expr::FieldGet(obj, idx)` -> `(field-get idx-unary-list kind-i64
    /// obj-form)` — `idx` is recovered from the *length* of an
    /// otherwise-meaningless list (`idx_unary_list`'s doc comment explains
    /// why a plain `Sexpr` `Int` can't carry it instead); `kind` is the
    /// field's own `struct_field_kind` (here `Type::I64`, kind `1`).
    #[test]
    fn translates_a_field_get() {
        let mut heap = Heap::with_capacity(1 << 10);
        let point_ty = Type::Named(Path::root("point"), vec![]);
        let obj = typed(Expr::Var("p".to_string()), point_ty);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::FieldGet(Box::new(obj), 2), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "field-get");
        assert_eq!(list_elems(&heap, fields[0]).len(), 2, "idx 2 encoded as a 2-element unary list");
        assert_eq!(fields[1], Value::Int(1), "an i64 field's kind is struct-field-kind Int");
        let (obj_tag, obj_fields) = untag(&heap, fields[2]);
        assert_eq!(obj_tag, "var");
        assert_eq!(expect_str(&heap, obj_fields[0]), "p");
    }

    /// `idx = 0` encodes as the *empty* list — `sexpr-list-length` on
    /// `Empty` is `0`, so `compile-field-get`/`compile-field-set` recover
    /// the right offset (`0`, no header slots in a `BoxedObj::Struct`'s own
    /// field vector) with no special case needed for the first field.
    #[test]
    fn a_field_index_of_zero_encodes_as_an_empty_list() {
        let mut heap = Heap::with_capacity(1 << 10);
        let point_ty = Type::Named(Path::root("point"), vec![]);
        let obj = typed(Expr::Var("p".to_string()), point_ty);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::FieldGet(Box::new(obj), 0), Type::I64)).unwrap();
        let (_, fields) = untag(&heap, v);
        assert!(fields[0].is_empty(), "idx 0 encodes as the empty list, got {:?}", fields[0]);
    }

    /// `Expr::FieldSet(obj, idx, value)` -> `(field-set idx-unary-list
    /// kind-i64 obj-form value-form)`.
    #[test]
    fn translates_a_field_set() {
        let mut heap = Heap::with_capacity(1 << 10);
        let point_ty = Type::Named(Path::root("point"), vec![]);
        let obj = typed(Expr::Var("p".to_string()), point_ty);
        let value = typed(Expr::Int(99), Type::I64);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::FieldSet(Box::new(obj), 1, Box::new(value)), Type::Unit)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "field-set");
        assert_eq!(list_elems(&heap, fields[0]).len(), 1);
        assert_eq!(fields[1], Value::Int(1), "the stored value's kind is struct-field-kind Int");
        let (obj_tag, obj_fields) = untag(&heap, fields[2]);
        assert_eq!(obj_tag, "var");
        assert_eq!(expect_str(&heap, obj_fields[0]), "p");
        let (value_tag, value_fields) = untag(&heap, fields[3]);
        assert_eq!(value_tag, "int");
        assert_eq!(value_fields, vec![Value::Int(99)]);
    }

    /// `collect_call_targets` recurses into a `Construct`'s field
    /// arguments.
    #[test]
    fn collect_call_targets_finds_a_call_nested_inside_a_construct_argument() {
        let ctor = Expr::Construct {
            type_name: Path::root("option"),
            variant: 0,
            args: vec![typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64)],
            mutable: false,
        };
        let v = typed(ctor, Type::Named(Path::root("option"), vec![Type::I64]));
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// `collect_call_targets` recurses into a `FieldGet`'s object and a
    /// `FieldSet`'s object/value sub-expressions.
    #[test]
    fn collect_call_targets_finds_calls_nested_inside_field_get_and_field_set() {
        let point_ty = Type::Named(Path::root("point"), vec![]);
        let obj = typed(Expr::Call(crate::Path::root("get-point"), vec![]), point_ty.clone());
        let v = typed(Expr::FieldGet(Box::new(obj), 0), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("get-point")]);

        let obj = typed(Expr::Call(crate::Path::root("get-point"), vec![]), point_ty);
        let value = typed(Expr::Call(crate::Path::root("new-value"), vec![]), Type::I64);
        let v = typed(Expr::FieldSet(Box::new(obj), 0, Box::new(value)), Type::Unit);
        assert_eq!(
            collect_call_targets(&v),
            vec![crate::Path::root("get-point"), crate::Path::root("new-value")]
        );
    }

    #[test]
    fn collect_assoc_targets_is_empty_for_a_body_with_no_method_calls() {
        let v = typed(Expr::Int(1), Type::I64);
        assert!(collect_assoc_targets(&v).is_empty());
    }

    /// The composability gap this whole module exists to close: a
    /// user-defined method call (`p::x`, desugared to `Expr::Assoc`) is a
    /// real pre-declaration target, unlike a built-in `i64`/`i32` one —
    /// `Interp::compile_function` itself is what filters those back out
    /// (see [`collect_assoc_targets`]'s doc comment), so this walker reports
    /// every `Expr::Assoc` it sees without trying to guess which receivers
    /// are built in.
    #[test]
    fn collect_assoc_targets_finds_an_instance_method_call() {
        let a = typed(Expr::Var("p".to_string()), Type::Named(Path::root("point"), vec![]));
        let assoc = Expr::Assoc { type_name: Path::root("point"), method: "x".to_string(), instance: true, args: vec![a] };
        let v = typed(assoc, Type::I64);
        assert_eq!(collect_assoc_targets(&v), vec![(Path::root("point"), "x".to_string())]);
        assert!(collect_call_targets(&v).is_empty(), "an Expr::Assoc target is never a call target");
    }

    #[test]
    fn collect_assoc_targets_deduplicates_repeated_calls_to_the_same_method() {
        let a = typed(Expr::Var("p".to_string()), Type::Named(Path::root("point"), vec![]));
        let inner = typed(
            Expr::Assoc { type_name: Path::root("point"), method: "x".to_string(), instance: true, args: vec![a] },
            Type::I64,
        );
        // The outer `Expr::Assoc` is itself a target too (`collect_assoc_targets`
        // reports every receiver, including a built-in `i64`/`i32` one — see its
        // doc comment for why filtering those out is `Interp::compile_function`'s
        // job, not this walker's), so both ends up in the result; only the
        // *repeated* `point::x` target is deduplicated.
        let outer = typed(
            Expr::Assoc { type_name: Path::root("i64"), method: "+".to_string(), instance: true, args: vec![inner.clone(), inner] },
            Type::I64,
        );
        assert_eq!(
            collect_assoc_targets(&outer),
            vec![(Path::root("i64"), "+".to_string()), (Path::root("point"), "x".to_string())]
        );
    }

    /// `collect_assoc_targets` recurses into a `labels`/`lambda` body and an
    /// `if`/`let`'s branches/bindings — the same node shapes
    /// `collect_call_targets` already follows, since both walk the same
    /// `collect_calls`.
    #[test]
    fn collect_assoc_targets_recurses_into_an_if_branch() {
        let receiver = typed(Expr::Var("p".to_string()), Type::Named(Path::root("point"), vec![]));
        let then_branch = typed(
            Expr::Assoc { type_name: Path::root("point"), method: "x".to_string(), instance: true, args: vec![receiver] },
            Type::I64,
        );
        let v = typed(
            Expr::If(Box::new(typed(Expr::Bool(true), Type::Bool)), Box::new(then_branch), Box::new(typed(Expr::Int(0), Type::I64))),
            Type::I64,
        );
        assert_eq!(collect_assoc_targets(&v), vec![(Path::root("point"), "x".to_string())]);
    }
}
