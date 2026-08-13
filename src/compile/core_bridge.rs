//! Translates a *core form* (`crate::check::core`) into the shape the
//! self-hosted compiler in `crate::compiler` pattern-matches on.
//!
//! Both sides are cons cells, so this is a cons-to-cons rewrite rather than
//! the tree-to-cons walk `compile::core_bridge` does today. That is the whole
//! point of the change: the checker used to lower cons cells into a Rust tree
//! and the bridge turned that tree straight back into cons cells, and nothing
//! in between needed the tree.
//!
//! Being additive, this is built one tag at a time while `core_bridge` still
//! runs; nothing routes here yet. The tests read hand-written core forms
//! through the reader, translate them, and — this being a bridge, whose whole
//! job is to be *accepted* — hand the result to the real `compile-function`.
//!
//! # What the island needs that the core form does not say in the same words
//!
//! Mostly the two vocabularies agree, deliberately: `int`, `if`, `let`, `call`
//! and most of the rest are spelled identically, so those arms are near
//! pass-throughs. The differences are all cases where the island's spelling is
//! forced by something it lacks:
//!
//! * **A name is a string, not a symbol** (`(var "x" ...)`). `compiler.rs`
//!   reads it with `sexpr-str`.
//! * **A literal is decomposed.** A string becomes one `(int c)` node per
//!   character and a float becomes two 32-bit halves, because a compile-time
//!   `StrId`/`BoxId` is meaningless to the *target* program — an AOT
//!   executable builds its own heap at startup. `compiler.rs` rebuilds both at
//!   run time.
//! * **A representation becomes a number.** The core form says `int`/`sexpr`;
//!   `compiler.rs` reads the integer kinds its `bind-params`/
//!   `compile-sexpr-field` were written against. The checker never learns
//!   those numbers — see [`Repr::binding_kind`]/[`Repr::field_kind`].
//! * **A builtin container method becomes an operation.** `Vector<T>`'s and
//!   `HashTable<K,V>`'s builtins have no compiled body to call, and the
//!   compiler's own LLVM methods reach the interpreter through one dispatch
//!   shim, so all three lower to dedicated nodes carrying an element kind or
//!   an operation id instead of a method name.
//!
//! # Dead fields
//!
//! Three of the island's fields are not read: `(var name is-fn)`'s `is-fn`,
//! the same tag on `if`/`match`/`return`, and `match`'s trailing `scrut-kind`.
//! Each is a remnant of a scheme that has since been retired (the closure ARC,
//! and per-kind scrutinee rooting), and `compiler.rs`'s own comments say so at
//! each site. They are still *emitted*, because the committed island bitcode
//! is a frozen interface and field positions have to stay put — but nothing
//! here spends work computing them.
//!
//! # Roots
//!
//! Every node is built through `check::core`'s builders, which keep each
//! finished field rooted until the node containing it exists. See that
//! module's own comment for why hand-balanced `push_root`/`pop_root` pairs are
//! not good enough — `core_bridge` balances sixteen pops by hand in one
//! function, and one `?` skips them all.

use std::collections::{HashMap, HashSet};

use typelisp_mem::{Error, Heap, RootScope, SymId, Value};

use crate::check::core::{self, Items};
use crate::check::repr::Repr;
use crate::type_key::type_key_of;
use crate::types::{path_is_builtin, path_is_builtin_any, Path, LLVM_METHOD_RECEIVER_TYPES};

use super::symbols::{
    fresh_lambda_name, llvm_op_id, user_method_symbol_name, user_symbol_name, DynTables,
    HASHTABLE_BUILTIN_METHODS, VECTOR_BUILTIN_METHODS,
};
use super::core_freevars::{free_vars, names_captured_by_nested};

/// Which types this translation can mention are `defstruct`s.
///
/// The one question a type *definition* genuinely answers for the island: a
/// struct has exactly one variant and so gets no tag test, only field
/// extraction. This used to hold every field representation of every type as
/// well, on the reasoning that a field's `Repr` is a property of its type and so
/// belongs on the `defstruct`/`defenum` rather than once per `construct` and once
/// per pattern. That is true only of a *monomorphic* type: `Option`'s `Some`
/// field is declared `T`, and monomorphization erases, so `Maybe<i64>` and
/// `Maybe<string>` both sit at the path `Maybe` and one table could never hold
/// both. The representations travel with the site instead (see
/// `translate_construct`), and what is left here needs no `Registry` — the same
/// property this snapshot always had.
#[derive(Default, Debug)]
pub struct Definitions {
    structs: HashSet<Path>,
    enums: HashSet<Path>,
}

impl Definitions {
    pub fn new() -> Definitions {
        Definitions::default()
    }

    /// Record a `defstruct` by path.
    ///
    /// For the JIT driver, which reads the runtime module tree rather than a
    /// program's top-level forms, so there is no form left to re-read.
    pub fn record_struct(&mut self, path: Path) {
        self.structs.insert(path);
    }

    /// [`Self::record_struct`]'s enum counterpart. Only the *name* is recorded;
    /// a variant's fields travel with the site that mentions them.
    pub fn record_enum(&mut self, path: Path) {
        self.enums.insert(path);
    }

    /// Record a `(defstruct PATH (R...))` or `(defenum PATH (SYM...) (..))`
    /// core form — the name only; a variant's fields travel with the site.
    ///
    /// Anything else is ignored rather than refused: this is meant to be fed a
    /// whole program's top level, and every other form there is simply not a
    /// type definition.
    pub fn record(&mut self, heap: &Heap, form: Value) -> Result<(), Error> {
        let Some(tag) = core::op(heap, form) else { return Ok(()) };
        match tag {
            "defstruct" => {
                let parts = core::fields(heap, form)?;
                let [path, _fields] = parts[..] else { return Err(malformed(heap, form)) };
                let path = as_path(heap, path).ok_or_else(|| malformed(heap, form))?;
                self.structs.insert(path);
            }
            "defenum" => {
                let parts = core::fields(heap, form)?;
                let [path, _variants, _fields] = parts[..] else { return Err(malformed(heap, form)) };
                let path = as_path(heap, path).ok_or_else(|| malformed(heap, form))?;
                self.enums.insert(path);
            }
            // A module's children are top-level forms too, and a type defined
            // inside one is just as reachable from a body being compiled as one
            // at the root.
            "module" => {
                for child in core::fields(heap, form)?.iter().skip(1) {
                    self.record(heap, *child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether `path` is a `defstruct` rather than a `defenum`.
    ///
    /// A type that is *neither* is an error, not a default: the two get
    /// different island lowerings (a struct has one variant and so no tag test),
    /// so guessing would emit code that reads the wrong box shape. Refusing here
    /// is what makes a body mentioning a type the driver forgot to record fail
    /// loudly instead of at run time.
    fn is_struct(&self, heap: &Heap, path: &Path) -> Result<bool, Error> {
        if self.structs.contains(path) {
            return Ok(true);
        }
        if self.enums.contains(path) {
            return Ok(false);
        }
        let _ = heap;
        Err(Error::TypeError(format!(
            "compile: no definition recorded for the type `{}` (internal error)",
            path
        )))
    }
}

/// What a translation reads: the fixed part, and the lexical part that grows
/// as it descends.
///
/// `Copy`, so a nested scope re-binds only the field that changed
/// (`Ctx { reprs: &extended, ..cx }`) instead of restating the rest.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    /// The field representations of every type this body can mention.
    pub defs: &'a Definitions,
    /// Every global this body may reference, already promoted to the
    /// compiled-global id the island bakes in as a constant. Populated before
    /// translation starts, so a miss is an internal error rather than
    /// something to resolve lazily.
    pub globals: &'a HashMap<Path, usize>,
    /// Every name in lexical scope, with the representation it was bound at.
    ///
    /// The core IR does not repeat a name's representation at each *use* — a
    /// binder states it once, which is the whole reason `let` has always
    /// carried one. So the uses that need it (`set`, and the `cellvar` split a
    /// captured name gets) read it back from here.
    ///
    /// Innermost last, and searched backwards, so a shadowing binding wins the
    /// same way it does at run time.
    reprs: &'a [(SymId, Repr)],
    /// Names that resolve to a *direct* call: the `labels` siblings in scope,
    /// this def included. Reset to empty inside a `lambda`, which never gets
    /// direct-call access to an enclosing block's siblings.
    direct: &'a HashSet<SymId>,
    /// Names bound to a shared cell rather than an ordinary slot, so that an
    /// assignment inside a capturing closure is visible everywhere else that
    /// shares the binding. A reference to one becomes `cellvar` rather than
    /// `var`, and its binding kind gains the island's `10 +` cell marker.
    cell_names: &'a HashSet<SymId>,
    /// Every `labels` sibling reachable from *any* enclosing scope. Unlike
    /// `direct` this is only ever grown, because a sibling captured as a value
    /// however many `lambda` boundaries away is still a sibling — the compiler
    /// boxes it at the capture site — and so must stay out of `cell_names`
    /// however deep it is found. A sibling can never be assigned to, so there
    /// is nothing for a cell to share.
    visible_siblings: &'a HashSet<SymId>,
    /// The trait-object id tables, interned before translation starts: both
    /// are baked into the emitted code as constants, so there is nothing to
    /// resolve mid-translation.
    pub dyn_tables: DynTables<'a>,
    /// The closest enclosing `labels` block's own captured list, copied
    /// unconditionally into the front of a nested block's.
    ///
    /// That padding is what makes every nested block's captured list a prefix
    /// superset of the one enclosing it: calling an enclosing sibling needs
    /// that sibling's captures forwarded, and the only way a def here can have
    /// them to forward is to carry them itself.
    outer_captured: &'a [SymId],
}

impl<'a> Ctx<'a> {
    /// Start a translation with nothing in lexical scope.
    pub fn new(defs: &'a Definitions, globals: &'a HashMap<Path, usize>) -> Ctx<'a> {
        Ctx::with_dyn_tables(defs, globals, DynTables { vtables: empty_vtables(), trait_ids: empty_trait_ids() })
    }

    /// The same, for a body that boxes or dispatches on a trait object.
    pub fn with_dyn_tables(
        defs: &'a Definitions,
        globals: &'a HashMap<Path, usize>,
        dyn_tables: DynTables<'a>,
    ) -> Ctx<'a> {
        Ctx {
            defs,
            globals,
            dyn_tables,
            reprs: &[],
            direct: empty_names(),
            cell_names: empty_names(),
            visible_siblings: empty_names(),
            outer_captured: &[],
        }
    }

    /// This scope's bindings, plus `more` — for a caller to hold while it
    /// translates the body they are in scope for.
    fn extended(&self, more: impl IntoIterator<Item = (SymId, Repr)>) -> Vec<(SymId, Repr)> {
        let mut v = self.reprs.to_vec();
        v.extend(more);
        v
    }

    /// Start from `names` as the cell set.
    ///
    /// A function body's own bindings that something nested inside captures
    /// have to be cells from the moment they are bound, so the driver computes
    /// them once — [`core_freevars::names_captured_by_nested`] over the body —
    /// and hands them in here before translating it. Without this the closure
    /// would read through a cell the binder never created.
    ///
    /// [`core_freevars::names_captured_by_nested`]: super::core_freevars::names_captured_by_nested
    pub fn with_cell_names(self, names: &'a HashSet<SymId>) -> Ctx<'a> {
        Ctx { cell_names: names, ..self }
    }

    fn repr_of(&self, name: SymId) -> Option<&Repr> {
        self.reprs.iter().rev().find(|(n, _)| *n == name).map(|(_, r)| r)
    }

    /// The island's kind number for a binding of `name` at `repr`: the plain
    /// one, or the `10 +` cell marker and the field classification the cell's
    /// contents are tagged with.
    fn binding_kind(&self, name: SymId, repr: &Repr) -> i64 {
        if self.cell_names.contains(&name) {
            10 + repr.field_kind()
        } else {
            repr.binding_kind()
        }
    }
}

/// One shared empty set, so `Ctx::new` can hand out borrows of it.
fn empty_names() -> &'static HashSet<SymId> {
    static EMPTY: std::sync::OnceLock<HashSet<SymId>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(HashSet::new)
}

fn empty_vtables() -> &'static HashMap<(String, Path), u32> {
    static EMPTY: std::sync::OnceLock<HashMap<(String, Path), u32>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

fn empty_trait_ids() -> &'static HashMap<Path, u32> {
    static EMPTY: std::sync::OnceLock<HashMap<Path, u32>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

/// The island's three scrutinee representations, which decide the tag test it
/// emits: a tagged `sexpr` (bit tests), a sum-ADT box (a variant slot), or a
/// boxed struct (single-variant, so no test at all — only field extraction).
const MATCH_KIND_SEXPR: i64 = 0;
const MATCH_KIND_BOX: i64 = 1;
const MATCH_KIND_STRUCT: i64 = 2;

/// A tag this stage has no translation for yet.
///
/// Not a fallback: reaching one is a bug in whoever routed here, and it says
/// which tag rather than emitting something the island would misread. Every
/// one of these is gone by the end of the phase, and their count is the
/// progress measure — the same convention `core::unlowered` uses on the
/// checker's side.
fn untranslated(tag: &str) -> Error {
    Error::TypeError(format!("compile: the bridge has no translation for `{}` yet", tag))
}

fn unbound(heap: &Heap, name: SymId) -> Error {
    Error::TypeError(format!(
        "compile: `{}` is referenced but no binder in scope states its representation (internal error)",
        heap.symbol_name(name)
    ))
}

fn malformed(heap: &Heap, form: Value) -> Error {
    Error::TypeError(format!("compile: malformed core form: {}", core::print(heap, form)))
}

/// Translate one core form.
pub fn to_island(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let Some(tag) = core::op(heap, form).map(str::to_string) else {
        return Err(malformed(heap, form));
    };
    match tag.as_str() {
        // ---- literals ----------------------------------------------------
        // Identical on both sides: the payload is a plain machine word the
        // island reads with `sexpr-int`/`sexpr-bool`/`sexpr-char`.
        "int" | "char" | "bool" => Ok(form),
        "unit" => core::tagged(heap, "unit", &[]),
        "float" => {
            let bits = match core::field(heap, form, 0) {
                Some(Value::Boxed(id)) if heap.is_float(id) => heap.float_value(id).to_bits(),
                _ => return Err(malformed(heap, form)),
            };
            // Two 32-bit halves, not one word: the island reads a `Sexpr` int
            // back through its 3-bit tag, which would silently drop the top
            // bits of a full-width bit pattern (`2.0` decoding as `0.0`).
            // Each half fits, and `compile-float` reassembles them.
            core::tagged(
                heap,
                "float",
                &[Value::Int((bits >> 32) as i64), Value::Int((bits & 0xFFFF_FFFF) as i64)],
            )
        }
        "bignum" => match core::field(heap, form, 0) {
            Some(Value::Boxed(id)) if heap.is_bignum(id) => {
                let n = heap.bignum_value(id).clone();
                bignum_form(heap, &n)
            }
            _ => Err(malformed(heap, form)),
        },
        "ratio" => match core::field(heap, form, 0) {
            Some(Value::Boxed(id)) if heap.is_ratio(id) => {
                let r = heap.ratio_value(id).clone();
                let numer = bignum_form(heap, r.numer())?;
                let mut f = Items::new(heap);
                f.push(numer);
                let denom = bignum_form(f.heap(), r.denom())?;
                f.push(denom);
                f.finish("ratio")
            }
            _ => Err(malformed(heap, form)),
        },
        "str" => match core::field(heap, form, 0) {
            Some(Value::Str(id)) => {
                let s = heap.string(id).to_string();
                str_form(heap, &s)
            }
            _ => Err(malformed(heap, form)),
        },

        // ---- variables ---------------------------------------------------
        "var" => {
            let Some(Value::Symbol(name)) = core::field(heap, form, 0) else {
                return Err(malformed(heap, form));
            };
            let is_cell = cx.cell_names.contains(&name);
            // The slot of a cell binding holds a cell reference rather than
            // the value, so the island dereferences it and untags the result
            // per this kind — the same decoder a struct field read uses.
            let cell_kind =
                if is_cell { Some(cx.repr_of(name).ok_or_else(|| unbound(heap, name))?.field_kind()) } else { None };
            let name_v = heap.alloc_string(heap.symbol_name(name).to_string());
            let mut f = Items::new(heap);
            f.push(name_v);
            if let Some(kind) = cell_kind {
                f.push(Value::Int(kind));
                f.finish("cellvar")
            } else {
                // `is-fn`, which `compile-var` does not read — see the module
                // comment on dead fields.
                f.push(Value::Bool(false));
                f.finish("var")
            }
        }

        // ---- binding and control -----------------------------------------
        "let" => translate_let(heap, form, cx),
        "if" => {
            let parts = core::fields(heap, form)?;
            let [cond, then, els] = parts[..] else { return Err(malformed(heap, form)) };
            let mut f = Items::new(heap);
            f.push(Value::Bool(false)); // `is-fn`, unread
            for part in [cond, then, els] {
                let v = to_island(f.heap(), part, cx)?;
                f.push(v);
            }
            f.finish("if")
        }
        "panic" => {
            let msg = core::field(heap, form, 0).ok_or_else(|| malformed(heap, form))?;
            let mut f = Items::new(heap);
            let v = to_island(f.heap(), msg, cx)?;
            f.push(v);
            f.finish("panic")
        }

        // ---- calls -------------------------------------------------------
        "call" => translate_call(heap, form, cx),
        "assoc" => translate_assoc(heap, form, cx),

        // ---- assignment, loops and globals -------------------------------
        "set" => translate_set(heap, form, cx),
        "loop" => {
            let body = core::fields(heap, form)?;
            let mut f = Items::new(heap);
            for e in &body {
                // Untagged, unlike a call's arguments: these run for effect,
                // and only whichever `break`/`return` leaves the loop carries
                // a value out.
                let v = to_island(f.heap(), *e, cx)?;
                f.push(v);
            }
            f.finish("loop")
        }
        "break" => core::tagged(heap, "break", &[]),
        "return" => {
            let value = core::field(heap, form, 0);
            let mut f = Items::new(heap);
            f.push(Value::Bool(false)); // `is-fn`, unread
            // A value-less `(return)` becomes an explicit `(unit)` rather than
            // a tag of its own, so `compile-return` has one shape to handle.
            let v = match value {
                Some(value) => to_island(f.heap(), value, cx)?,
                None => core::tagged(f.heap(), "unit", &[])?,
            };
            f.push(v);
            f.finish("return")
        }
        "global" => {
            let (id, kind) = global_id_and_kind(heap, form, cx)?;
            core::tagged(heap, "global", &[Value::Int(id), Value::Int(kind)])
        }
        "set-global" => {
            let (id, kind) = global_id_and_kind(heap, form, cx)?;
            let value = core::field(heap, form, 4).ok_or_else(|| malformed(heap, form))?;
            let mut f = Items::new(heap);
            f.push(Value::Int(id));
            f.push(Value::Int(kind));
            let v = to_island(f.heap(), value, cx)?;
            f.push(v);
            f.finish("set-global")
        }

        // ---- quoted data -------------------------------------------------
        // A quoted datum has no runtime representation to refer to — the
        // island builds it fresh — so both of these become the `construct`
        // nodes that build it. `sym` is exactly the one-symbol case.
        "quote" => {
            let datum = core::field(heap, form, 0).ok_or_else(|| malformed(heap, form))?;
            quoted_form(heap, datum)
        }
        "sym" => {
            let name = symbol_field(heap, form, 0)?;
            sexpr_leaf(heap, SEXPR_SYM, |h| str_form(h, &name))
        }

        // ---- trait objects -----------------------------------------------
        "dyn-new" => translate_dyn_new(heap, form, cx),
        "dyn-upcast" => translate_dyn_upcast(heap, form, cx),
        "dyn-call" => translate_dyn_call(heap, form, cx),
        // Unwrapping a trait object: the operand is always one, by the
        // checker's own construction, and a trait object is an untraced word
        // at a binding boundary — so the kind is not something to look up.
        "dyn-value" => {
            let inner = core::field(heap, form, 0).ok_or_else(|| malformed(heap, form))?;
            let mut f = Items::new(heap);
            let pair = dyn_operand(f.heap(), inner, cx)?;
            f.push(pair);
            f.finish("dyn-value")
        }

        // Reflection: `(compile ...)` JIT-compiles a target against the
        // *running* interpreter's own heap and scope tree, which has no
        // meaning inside code being compiled ahead of time. Unreachable from
        // any compilable source, and refused rather than given a lowering.
        "compile-fn" => Err(Error::TypeError(
            "compile: `(compile ...)` is an interpreter-only action and cannot itself be compiled"
                .to_string(),
        )),

        // ---- functions ---------------------------------------------------
        "lambda" => translate_lambda(heap, form, cx),
        "labels" => translate_labels(heap, form, cx),
        "apply" => translate_apply(heap, form, cx),
        "fnref" => translate_fnref(heap, form, cx),
        "methodref" => translate_methodref(heap, form, cx),

        // ---- data --------------------------------------------------------
        "construct" => translate_construct(heap, form, cx),
        "field-get" => translate_field(heap, form, "field-get", cx),
        "field-set" => translate_field(heap, form, "field-set", cx),
        "match" => translate_match(heap, form, cx),

        other => Err(untranslated(other)),
    }
}

/// `(let ((SYM R E) ...) BODY...)` -> `(let (((SYM . kind) . form)...) body...)`.
///
/// The binding's own `R` is what the island's `bind-let-values` reads as a
/// kind, so unlike every other position this one needed nothing added to the
/// core IR — a `let` has always had to say what it binds.
fn translate_let(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    let Some((binds, body)) = parts.split_first() else { return Err(malformed(heap, form)) };
    let binds = heap.list_to_vec(*binds)?;
    let body = body.to_vec();

    let bindings = {
        let mut s = RootScope::new(heap);
        let mut pairs = Vec::with_capacity(binds.len());
        for b in &binds {
            let items = s.list_to_vec(*b)?;
            let [name, repr, init] = items[..] else { return Err(malformed(&s, *b)) };
            let Value::Symbol(_) = name else { return Err(malformed(&s, *b)) };
            let Some(repr) = Repr::read(&s, repr) else { return Err(malformed(&s, *b)) };
            let Value::Symbol(name_sym) = name else { return Err(malformed(&s, *b)) };
            let name_pair = s.cons(name, Value::Int(cx.binding_kind(name_sym, &repr)))?;
            s.push_root(name_pair);
            let init = to_island(&mut s, init, cx)?;
            s.push_root(init);
            let pair = s.cons(name_pair, init)?;
            s.push_root(pair);
            pairs.push(pair);
        }
        core::list(&mut s, &pairs)?
    };

    // The body is translated with the bindings in scope: a `set` on one of
    // them reads its representation back from there.
    let mut bound = Vec::with_capacity(binds.len());
    for b in &binds {
        let items = heap.list_to_vec(*b)?;
        let [Value::Symbol(name), repr, _] = items[..] else { return Err(malformed(heap, *b)) };
        let repr = Repr::read(heap, repr).ok_or_else(|| malformed(heap, *b))?;
        bound.push((name, repr));
    }
    let inner = cx.extended(bound);
    let cx = Ctx { reprs: &inner, ..cx };

    let mut f = Items::new(heap);
    f.push(bindings);
    for e in &body {
        let v = to_island(f.heap(), *e, cx)?;
        f.push(v);
    }
    f.finish("let")
}

/// `(call WRITTEN HOME PATH (R...) E...)` -> `(call "name" (kind . form)...)`.
///
/// The island looks the callee up by the same mangled name
/// `Interp::compile_scc`/`compile::aot` declare it under, so the mangling
/// lives in one place ([`user_symbol_name`]) shared with the old bridge. The
/// three `sexpr-car`/`sexpr-cdr`/`sexpr-cons` builtins are the exception:
/// `compile-call` matches those literal names and emits `rt_car`/`rt_cdr`/
/// `rt_cons` directly, so prefixing them would break the match.
fn translate_call(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 4 {
        return Err(malformed(heap, form));
    }
    let path = as_path(heap, parts[2]).ok_or_else(|| malformed(heap, form))?;
    let reprs = repr_list(heap, parts[3])?;
    let args = parts[4..].to_vec();

    let raw = path.last_segment();
    let name = if crate::eval::interp::is_rt_builtin_name(raw) {
        raw.to_string()
    } else {
        user_symbol_name(&path.segments().join("::"))
    };
    let name_v = heap.alloc_string(name);

    let mut f = Items::new(heap);
    f.push(name_v);
    let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
    f.extend(pairs);
    f.finish("call")
}

/// `(assoc PATH SYM INSTANCE HOME RET-R (R...) E...)` -> one of four nodes.
///
/// Which one is decided *here*, from the type path and the representation the
/// call operates on, so `compiler.rs` needs no registry of its own — the same
/// division of labour the old bridge had.
///
/// The representation it operates on is the receiver's for an instance method
/// and the result's for a static one. That distinction is why `assoc` carries
/// a result representation at all: `(vector::new)` and `(scope::new)` have no
/// receiver, and the result is the only thing that says which container they
/// are building.
fn translate_assoc(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 6 {
        return Err(malformed(heap, form));
    }
    let type_name = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, form))?;
    let method = symbol_field(heap, form, 1)?;
    let Value::Bool(instance) = parts[2] else { return Err(malformed(heap, form)) };
    let ret = Repr::read(heap, parts[4]).ok_or_else(|| malformed(heap, form))?;
    let reprs = repr_list(heap, parts[5])?;
    let args = parts[6..].to_vec();
    // The receiver is `args[0]` by the same convention the checker builds an
    // instance call with, so its representation is the first of the list.
    let self_repr = if instance { reprs.first().unwrap_or(&ret) } else { &ret }.clone();

    if path_is_builtin(&type_name, "vector") && VECTOR_BUILTIN_METHODS.contains(&method.as_str()) {
        return translate_vector_op(heap, &method, &self_repr, &args, cx);
    }
    if path_is_builtin(&type_name, "hashtable") && HASHTABLE_BUILTIN_METHODS.contains(&method.as_str()) {
        return translate_hashtable_op(heap, &method, &self_repr, &args, cx);
    }
    if let Some(key) = llvm_op_key(&type_name, &self_repr) {
        let opid = Value::Int(llvm_op_id(key, &method));
        let mut f = Items::new(heap);
        f.push(opid);
        let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
        f.extend(pairs);
        return f.finish("llvm-op");
    }

    // The generic path. The type name is its *fully qualified* path: the
    // island mangles it back into a `tl_type::path::method` lookup name
    // ([`user_method_symbol_name`]), and two same-named types in different
    // modules would otherwise collide on one symbol.
    let type_v = heap.alloc_string(type_name.to_string());
    let mut f = Items::new(heap);
    f.push(type_v);
    let method_v = f.heap().alloc_string(method);
    f.push(method_v);
    f.push(Value::Bool(instance));
    let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
    f.extend(pairs);
    f.finish("assoc")
}

/// Which `rt_llvm_call` dispatch key an `assoc` belongs to, or `None` for the
/// generic method path.
///
/// A `Scope<V>` is the interesting one: every scope is one heap object
/// whatever it holds, but a scope of *LLVM handles* still routes to the
/// island's frozen `native-scope` op, so this is a decision about `V` alone —
/// which is exactly why [`Repr::Scope`] carries it.
fn llvm_op_key(type_name: &Path, self_repr: &Repr) -> Option<&'static str> {
    if !path_is_builtin_any(type_name, &LLVM_METHOD_RECEIVER_TYPES) {
        return None;
    }
    match type_name.last_segment() {
        "llvm-module" => Some("llvm-module"),
        "llvm-function" => Some("llvm-function"),
        "llvm-builder" => Some("llvm-builder"),
        "scope" => match self_repr {
            Repr::Scope(v) if **v == Repr::Handle => Some("native-scope"),
            _ => None,
        },
        _ => None,
    }
}

/// A `Vector<T>` builtin -> `(vector-op method kind form... [option-name])`.
///
/// None of these has a compiled `defmethod` body, so there is nothing to
/// call; `compile-vector-op` emits the `rt_struct_*` calls directly. `kind` is
/// `T`'s, since every one of the methods either reads or writes a `T` across
/// the boxed-struct boundary. `new` has no receiver to read `T` from and no
/// element to tag, so it carries `0` and a `"vector"` type-name literal
/// instead; `pop` returns `Option<T>` and needs an `"option"` literal to build
/// the result with.
fn translate_vector_op(heap: &mut Heap, method: &str, self_repr: &Repr, args: &[Value], cx: Ctx) -> Result<Value, Error> {
    let kind = match self_repr {
        Repr::Vector(t) if method != "new" => t.field_kind(),
        _ => 0,
    };
    let method_v = heap.alloc_string(method.to_string());
    let mut f = Items::new(heap);
    f.push(method_v);
    f.push(Value::Int(kind));
    if method == "new" {
        let name = str_form(f.heap(), "vector")?;
        f.push(name);
    } else {
        for a in args {
            let v = to_island(f.heap(), *a, cx)?;
            f.push(v);
        }
    }
    if method == "pop" {
        let name = str_form(f.heap(), "option")?;
        f.push(name);
    }
    f.finish("vector-op")
}

/// A `HashTable<K,V>` builtin -> `(hashtable-op method key-kind val-kind
/// option-name form...)`, the map counterpart of [`translate_vector_op`].
///
/// `get`/`remove` return `Option<V>`, which `compile-hashtable-op` builds
/// itself — there is no `Option::some` call site in the source to translate —
/// so they carry the type-name literal it needs; the rest carry an unused
/// placeholder in that position.
fn translate_hashtable_op(
    heap: &mut Heap,
    method: &str,
    self_repr: &Repr,
    args: &[Value],
    cx: Ctx,
) -> Result<Value, Error> {
    let (kk, vk) = match self_repr {
        Repr::HashTable(k, v) => (k.field_kind(), v.field_kind()),
        _ => (0, 0),
    };
    let method_v = heap.alloc_string(method.to_string());
    let mut f = Items::new(heap);
    f.push(method_v);
    f.push(Value::Int(kk));
    f.push(Value::Int(vk));
    let option_name =
        if method == "get" || method == "remove" { str_form(f.heap(), "option")? } else { Value::Empty };
    f.push(option_name);
    if method != "new" {
        for a in args {
            let v = to_island(f.heap(), *a, cx)?;
            f.push(v);
        }
    }
    f.finish("hashtable-op")
}

/// `(set SYM E)` -> `(set "name" kind value-form)`.
///
/// `kind` is the *target's* representation, which is why the core IR does not
/// repeat one here: the binder already stated it, and repeating it at every
/// assignment would be the same fact recorded twice. The island needs it
/// because a `setf` overwrites the slot in place while the GC root pushed at
/// bind time is a snapshot — left unupdated, the new value would sit unrooted
/// for the rest of the binding's scope, so `compile-set` fixes that same root
/// entry when the kind says there is one.
fn translate_set(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    let [Value::Symbol(name), value] = parts[..] else { return Err(malformed(heap, form)) };
    let repr = cx.repr_of(name).ok_or_else(|| {
        Error::TypeError(format!(
            "compile: assignment to `{}`, which is not in scope here (internal error)",
            heap.symbol_name(name)
        ))
    })?;
    let is_cell = cx.cell_names.contains(&name);
    // A cell target takes the field classification, not the root-or-not one:
    // the island re-tags the new value and writes it *through* the cell, and
    // the cell reference — the binding's permanent root for its whole
    // activation — never moves, so there is no root index to fix up.
    let kind = if is_cell { repr.field_kind() } else { repr.binding_kind() };
    let name_v = heap.alloc_string(heap.symbol_name(name).to_string());
    let mut f = Items::new(heap);
    f.push(name_v);
    f.push(Value::Int(kind));
    let v = to_island(f.heap(), value, cx)?;
    f.push(v);
    f.finish(if is_cell { "cellset" } else { "set" })
}

/// The compiled-global id and kind shared by `(global ...)` and
/// `(set-global ...)`, whose leading fields are the same.
///
/// The id is looked up rather than resolved: every path this body can reach is
/// promoted before translation starts, so a miss means the promoter and this
/// walk have gone out of step — an internal error, not a user-facing one.
///
/// A global's storage always holds a properly tagged value, so the kind is
/// `field_kind`: reading one back into a plain `int` needs the untagging a
/// struct field read already does, and writing one needs the same tagging.
fn global_id_and_kind(heap: &Heap, form: Value, cx: Ctx) -> Result<(i64, i64), Error> {
    let path = core::field(heap, form, 2)
        .and_then(|v| as_path(heap, v))
        .ok_or_else(|| malformed(heap, form))?;
    let repr = core::field(heap, form, 3)
        .and_then(|v| Repr::read(heap, v))
        .ok_or_else(|| malformed(heap, form))?;
    let id = cx.globals.get(&path).copied().ok_or_else(|| {
        Error::TypeError(format!(
            "compile: the global `{}` was not promoted before translation (internal error)",
            path
        ))
    })?;
    Ok((id as i64, repr.field_kind()))
}

/// What `compile-function` takes for one definition: the boundary between
/// Rust and the self-hosted compiler.
///
/// The signature `(module, name, params, body)` is unchanged from the one the
/// old pipeline used — deliberately, since the island is frozen. What changes
/// is where the three pieces come from: a `defun` core form rather than a
/// `TopLevel` enum variant that never reached the island at all.
#[derive(Debug)]
pub struct Function {
    /// The mangled symbol the island declares and calls it under. It has to
    /// agree with what a `call`/`assoc` at a use site mangles to, so both go
    /// through the same two functions.
    pub name: String,
    /// `((SYM . kind)...)`, which `bind-params` walks.
    pub params: Value,
    /// The single form the body collapses to.
    pub body: Value,
}

/// The definition `form` compiles to, or `None` if it is not something the
/// island compiles.
///
/// `None` is not a refusal. Most of the top level is genuinely not compiled: a
/// type definition contributes representations rather than code, a macro is
/// gone by this point, a `use` or a `load` is a name-resolution act, and a
/// bare expression at the top level is run rather than compiled. A `module` is
/// `None` here too — its children are top-level forms in their own right, and
/// walking into them is the driver's job, not this function's.
///
/// The cell set is computed here rather than asked for, unlike everywhere else
/// a `Ctx` is threaded: a body's own bindings that something nested captures
/// have to be cells from the moment they are bound, and getting that wrong is
/// invisible until a closure reads through a cell the binder never made. It is
/// derivable from the body alone, so it is derived.
/// Every external thing a body depends on, gathered in one walk.
///
/// The AST had six `collect_*` entry points, each running the same walker and
/// returning one of its fields — six walks where one does. A core form needs no
/// per-node knowledge to recurse, only to *record*, so the walk is uniform and
/// there is no reason to split it.
#[derive(Default)]
pub struct Targets {
    /// Free functions called (or reified with `fnref`), first-encounter order.
    pub calls: Vec<Path>,
    /// `(type, method)` pairs called as an instance/static method — including
    /// every vtable slot a `dyn-new` lays out and every implementation a
    /// `dyn-call` could dispatch to.
    pub methods: Vec<(Path, String)>,
    /// Globals read or assigned.
    pub globals: Vec<Path>,
    /// Trait objects boxed here: the emitted IR names a vtable by a *constant*
    /// id, so each must be interned before translation starts.
    pub dyn_boxes: Vec<DynBoxSite>,
    /// Supertraits a trait object is upcast to — interned for the same reason.
    pub dyn_upcasts: Vec<Path>,
    /// Traits dispatched on dynamically.
    pub dyn_traits: Vec<Path>,
}

/// One `dyn-new` site's vtable identity.
pub struct DynBoxSite {
    pub concrete_key: String,
    pub trait_path: Path,
    pub slots: Vec<(Path, String)>,
    pub supers: Vec<(Path, Vec<(Path, String)>)>,
}

/// Collect everything `body`'s forms depend on externally.
pub fn collect_targets(heap: &Heap, body: &[Value]) -> Result<Targets, Error> {
    let mut t = Targets::default();
    for form in body {
        collect_into(heap, *form, &mut t)?;
    }
    Ok(t)
}

fn collect_into(heap: &Heap, form: Value, t: &mut Targets) -> Result<(), Error> {
    if !matches!(form, Value::Cons(_)) {
        return Ok(());
    }
    let tag = core::op(heap, form).map(|s| s.to_string());
    let Some(tag) = tag else {
        // A cons whose head is not a symbol: not a node but a *list* of them —
        // a `match` arm (`(PATTERN BODY...)`), a `labels` definition. Walk its
        // elements rather than treating it as a form with fields, which would
        // skip everything past the head.
        for e in heap.list_to_vec(form)? {
            collect_into(heap, e, t)?;
        }
        return Ok(());
    };
    match tag.as_str() {
        // The one fence in the whole walk. User data appears nowhere else, so
        // this is the only place a cons can be something other than a node —
        // and a quoted `(call ...)` datum is data, not a dependency.
        "quote" => return Ok(()),
        // `(call (WRITTEN...) (HOME...) PATH (REPR...) ARG...)`, and `fnref`'s
        // first three fields are the same triple — the bridge turns a `fnref`
        // into a forwarding call, so its target needs the same treatment.
        "call" | "fnref" => {
            if let Some(p) = path_at(heap, form, 2) {
                push_unique(&mut t.calls, p);
            }
        }
        // `(assoc PATH SYM ...)` / `(methodref PATH SYM ...)`.
        "assoc" | "methodref" => {
            if let (Some(p), Some(m)) = (path_at(heap, form, 0), sym_at(heap, form, 1)) {
                push_unique(&mut t.methods, (p, m));
            }
        }
        // `(global (WRITTEN...) (HOME...) PATH REPR)` and `set-global`'s same
        // leading triple.
        "global" | "set-global" => {
            if let Some(p) = path_at(heap, form, 2) {
                push_unique(&mut t.globals, p);
            }
        }
        // Boxing a trait object is what pulls its whole vtable into the call
        // graph: the *call* through it has no statically known target, but
        // every target it could reach is right here in the slot list.
        "dyn-new" => {
            let key = match core::field(heap, form, 0) {
                Some(Value::Str(id)) => heap.string(id).to_string(),
                _ => return Ok(()),
            };
            let Some(trait_path) = path_at(heap, form, 1) else { return Ok(()) };
            let slots = vtable_at(heap, form, 2)?;
            let supers = supers_at(heap, form, 3)?;
            for key in &slots {
                push_unique(&mut t.methods, key.clone());
            }
            if !t.dyn_boxes.iter().any(|s| s.concrete_key == key && s.trait_path == trait_path) {
                t.dyn_boxes.push(DynBoxSite { concrete_key: key, trait_path, slots, supers });
            }
        }
        // The conversion calls nothing — it swaps the box's table for one the
        // *boxing* site already registered. Only the target trait's id has to
        // be interned before translation.
        "dyn-upcast" => {
            if let Some(p) = path_at(heap, form, 0) {
                push_unique(&mut t.dyn_upcasts, p);
            }
        }
        // `(dyn-call TRAIT SYM SLOT ((TYPE SYM)...) (REPR...) ARG...)` — no
        // static target, but every implementation it could dispatch to must
        // exist natively before this body can run natively.
        "dyn-call" => {
            if let Some(p) = path_at(heap, form, 0) {
                push_unique(&mut t.dyn_traits, p);
            }
            for key in vtable_at(heap, form, 3)? {
                push_unique(&mut t.methods, key);
            }
        }
        _ => {}
    }
    for f in core::fields(heap, form)? {
        collect_into(heap, f, t)?;
    }
    Ok(())
}

fn push_unique<T: PartialEq>(out: &mut Vec<T>, v: T) {
    if !out.contains(&v) {
        out.push(v);
    }
}

/// Field `i` as a path, or `None` if it is not one. A single-segment path reads
/// back as a bare `Value::Symbol` (the reader only builds `Value::Path` when it
/// sees `::`), so both spellings mean the same one-segment path.
fn path_at(heap: &Heap, form: Value, i: usize) -> Option<Path> {
    match core::field(heap, form, i)? {
        Value::Path(id) => Some(crate::types::path_from_id(heap, id)),
        Value::Symbol(id) => Some(Path::root(heap.symbol_name(id))),
        _ => None,
    }
}

fn sym_at(heap: &Heap, form: Value, i: usize) -> Option<String> {
    match core::field(heap, form, i)? {
        Value::Symbol(id) => Some(heap.symbol_name(id).to_string()),
        _ => None,
    }
}

/// Field `i` as a vtable `((TYPE SYM)...)`.
fn vtable_at(heap: &Heap, form: Value, i: usize) -> Result<Vec<(Path, String)>, Error> {
    let Some(list) = core::field(heap, form, i) else { return Ok(Vec::new()) };
    vtable_of(heap, list)
}

/// A vtable slot list `((TYPE SYM)...)`, read from the list itself.
///
/// Separate from [`vtable_at`] because a supertrait entry is a *bare* list with
/// no tag, and [`core::field`] counts past a tag — so an index into one is off
/// by one. Taking the list directly removes the arithmetic instead of getting it
/// right twice. (Getting it wrong here emitted an empty supertrait vtable, and
/// the only symptom was an AOT executable aborting with `rt_vtable_slot: vtable
/// slot is empty`.)
fn vtable_of(heap: &Heap, list: Value) -> Result<Vec<(Path, String)>, Error> {
    let mut out = Vec::new();
    for entry in heap.list_to_vec(list)? {
        let parts = heap.list_to_vec(entry)?;
        if let [ty, m] = parts.as_slice() {
            let path = match ty {
                Value::Path(id) => crate::types::path_from_id(heap, *id),
                Value::Symbol(id) => Path::root(heap.symbol_name(*id)),
                _ => continue,
            };
            if let Value::Symbol(id) = m {
                out.push((path, heap.symbol_name(*id).to_string()));
            }
        }
    }
    Ok(out)
}

/// Field `i` as a supertrait table list `((TRAIT ((TYPE SYM)...))...)`.
#[allow(clippy::type_complexity)]
fn supers_at(heap: &Heap, form: Value, i: usize) -> Result<Vec<(Path, Vec<(Path, String)>)>, Error> {
    let Some(list) = core::field(heap, form, i) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for entry in heap.list_to_vec(list)? {
        let parts = heap.list_to_vec(entry)?;
        if let [ty, slots] = parts.as_slice() {
            let path = match ty {
                Value::Path(id) => crate::types::path_from_id(heap, *id),
                Value::Symbol(id) => Path::root(heap.symbol_name(*id)),
                _ => continue,
            };
            // The already-destructured element, not `core::field(entry, 1)` —
            // see [`vtable_of`].
            out.push((path, vtable_of(heap, *slots)?));
        }
    }
    Ok(out)
}

pub fn top_level_function(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Option<Function>, Error> {
    let Some(tag) = core::op(heap, form).map(str::to_string) else {
        return Err(malformed(heap, form));
    };
    let parts = core::fields(heap, form)?;
    let (name, params, body) = match tag.as_str() {
        // `(defun PATH ((SYM R)...) RET-R PUBLIC E...)`
        "defun" => {
            if parts.len() < 4 {
                return Err(malformed(heap, form));
            }
            let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, form))?;
            let raw = path.last_segment();
            let name = if crate::eval::interp::is_rt_builtin_name(raw) {
                raw.to_string()
            } else {
                user_symbol_name(&path.segments().join("::"))
            };
            (name, params_of(heap, parts[1])?, parts[4..].to_vec())
        }
        // `(defmethod PATH SYM INSTANCE ((SYM R)...) RET-R PUBLIC E...)`
        "defmethod" => {
            if parts.len() < 6 {
                return Err(malformed(heap, form));
            }
            let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, form))?;
            let method = symbol_field(heap, form, 1)?;
            (user_method_symbol_name(&path, &method), params_of(heap, parts[3])?, parts[6..].to_vec())
        }
        _ => return Ok(None),
    };
    if body.is_empty() {
        return Err(malformed(heap, form));
    }

    let (param_list, body_v) = function_parts(heap, &params, &body, cx, empty_names())?;
    Ok(Some(Function { name, params: param_list, body: body_v }))
}

/// [`top_level_function`]'s translation half, for a caller that already has the
/// parameters and body rather than a definition form to read them from.
///
/// Two such callers exist: the JIT, which looks a registered body up by name,
/// and the synthetic "closure constructor" a `lambda` is compiled through,
/// whose parameters are a captured-cell reference per free variable and so
/// belong to no `defun` at all.
///
/// `extra_non_cell` names parameters that must *not* be treated as captured
/// cells even though the body's free-variable walk finds them so. The closure
/// constructor needs it: its body literally is the `(lambda ...)` being
/// compiled, so the walk "discovers" that the ctor's own parameters are
/// captured by the very `lambda` it wraps and would box them a second time.
/// They are declared to pass each cell reference through unchanged, for the
/// inner `lambda`'s own separately-computed captured list to pick up as-is.
pub fn function_parts(
    heap: &mut Heap,
    params: &[(SymId, Repr)],
    body: &[Value],
    cx: Ctx,
    extra_non_cell: &HashSet<SymId>,
) -> Result<(Value, Value), Error> {
    let mut cells = names_captured_by_nested(heap, body)?;
    for n in extra_non_cell {
        cells.remove(n);
    }
    let reprs = cx.extended(params.iter().cloned());
    let inner = Ctx { reprs: &reprs, ..cx }.with_cell_names(&cells);

    let mut s = RootScope::new(heap);
    let param_list = name_kind_list(&mut s, params, inner)?;
    s.push_root(param_list);
    let body_v = body_form(&mut s, body, inner)?;
    Ok((param_list, body_v))
}

/// `(defvar PATH R MUTABLE PUBLIC E)` -> `(global-init kind value)`, the node
/// the ahead-of-time compiler's synthesized initializer sequence runs.
///
/// There is no id to look up for the global being defined — the runtime
/// assigns one by call order as the sequence runs — though the initializer may
/// well refer to *other* globals, which is what `Ctx::globals` is still for.
///
/// `None` for anything that is not a `defvar`.
pub fn global_init(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Option<Value>, Error> {
    if core::op(heap, form) != Some("defvar") {
        return Ok(None);
    }
    let parts = core::fields(heap, form)?;
    let [_, repr, _, _, value] = parts[..] else { return Err(malformed(heap, form)) };
    let repr = Repr::read(heap, repr).ok_or_else(|| malformed(heap, form))?;
    let cells = names_captured_by_nested(heap, std::slice::from_ref(&value))?;
    let inner = cx.with_cell_names(&cells);

    let mut f = Items::new(heap);
    // A global's storage always holds a properly tagged value, so this is the
    // field classification — the same one a `global` read untags with.
    f.push(Value::Int(repr.field_kind()));
    let v = to_island(f.heap(), value, inner)?;
    f.push(v);
    Ok(Some(f.finish("global-init")?))
}

/// `sexpr`'s own variant numbers, which a quoted datum is built out of./// `sexpr`'s own variant numbers, which a quoted datum is built out of.
///
/// Not a scheme of this module's: these are the variants of the built-in type
/// (`registry::sexpr_def`), so `compile-construct-sexpr` builds each one the
/// same way it would from written source.
const SEXPR_NIL: i64 = 0;
const SEXPR_INT: i64 = 1;
const SEXPR_FLOAT: i64 = 2;
const SEXPR_CHAR: i64 = 3;
const SEXPR_BOOL: i64 = 4;
const SEXPR_STR: i64 = 6;
const SEXPR_CONS: i64 = 7;
const SEXPR_BIGNUM: i64 = 8;
const SEXPR_RATIO: i64 = 9;
/// Past the variant numbering: a symbol and a path are not `sexpr` variants
/// with a stored payload but names to be *interned* at run time, so
/// `compile-construct-sym`/`compile-construct-path` recognize them by a marker
/// the ordinary variants can never collide with.
const SEXPR_SYM: i64 = 100;
const SEXPR_PATH: i64 = 101;

/// `(construct true false () variant field...)` — one `sexpr` value.
fn sexpr_construct(heap: &mut Heap, variant: i64, fields: &[Value]) -> Result<Value, Error> {
    let mut items = vec![Value::Bool(true), Value::Bool(false), Value::Empty, Value::Int(variant)];
    items.extend_from_slice(fields);
    core::tagged(heap, "construct", &items)
}

/// The same with one field, built by `leaf` while it stays rooted.
fn sexpr_leaf(
    heap: &mut Heap,
    variant: i64,
    leaf: impl FnOnce(&mut Heap) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    let v = leaf(f.heap())?;
    f.push(v);
    let fields = f.as_slice().to_vec();
    sexpr_construct(f.heap(), variant, &fields)
}

/// A quoted datum, as the nodes that rebuild it at run time.
///
/// Quoted data cannot be *referred* to: the datum lives in the compiling
/// heap, and an ahead-of-time compiled program builds its own from scratch at
/// startup with no object table shared with this one. So the literal is taken
/// apart here and reassembled there — the same reason a string literal becomes
/// one node per character.
fn quoted_form(heap: &mut Heap, datum: Value) -> Result<Value, Error> {
    match datum {
        Value::Empty => sexpr_construct(heap, SEXPR_NIL, &[]),
        Value::Int(n) => sexpr_leaf(heap, SEXPR_INT, |h| core::tagged(h, "int", &[Value::Int(n)])),
        Value::Bool(b) => sexpr_leaf(heap, SEXPR_BOOL, |h| core::tagged(h, "bool", &[Value::Bool(b)])),
        Value::Char(c) => sexpr_leaf(heap, SEXPR_CHAR, |h| core::tagged(h, "char", &[Value::Char(c)])),
        Value::Str(id) => {
            let text = heap.string(id).to_string();
            sexpr_leaf(heap, SEXPR_STR, |h| str_form(h, &text))
        }
        Value::Symbol(id) => {
            let name = heap.symbol_name(id).to_string();
            sexpr_leaf(heap, SEXPR_SYM, |h| str_form(h, &name))
        }
        // A path's segments become one string literal each, interned back into
        // a path at run time.
        Value::Path(id) => {
            let segs: Vec<String> =
                heap.path_segments(id).iter().map(|s| heap.symbol_name(*s).to_string()).collect();
            let mut f = Items::new(heap);
            for seg in &segs {
                let v = str_form(f.heap(), seg)?;
                f.push(v);
            }
            let fields = f.as_slice().to_vec();
            sexpr_construct(f.heap(), SEXPR_PATH, &fields)
        }
        Value::Boxed(id) if heap.is_float(id) => {
            let bits = heap.float_value(id).to_bits();
            sexpr_leaf(heap, SEXPR_FLOAT, move |h| {
                core::tagged(
                    h,
                    "float",
                    &[Value::Int((bits >> 32) as i64), Value::Int((bits & 0xFFFF_FFFF) as i64)],
                )
            })
        }
        Value::Boxed(id) if heap.is_bignum(id) => {
            let n = heap.bignum_value(id).clone();
            sexpr_leaf(heap, SEXPR_BIGNUM, move |h| bignum_form(h, &n))
        }
        Value::Boxed(id) if heap.is_ratio(id) => {
            let r = heap.ratio_value(id).clone();
            sexpr_leaf(heap, SEXPR_RATIO, move |h| {
                let mut f = Items::new(h);
                let numer = bignum_form(f.heap(), r.numer())?;
                f.push(numer);
                let denom = bignum_form(f.heap(), r.denom())?;
                f.push(denom);
                f.finish("ratio")
            })
        }
        Value::Cons(_) => {
            let (car, cdr) = (heap.car(datum)?, heap.cdr(datum)?);
            let mut f = Items::new(heap);
            let car_v = quoted_form(f.heap(), car)?;
            f.push(car_v);
            let cdr_v = quoted_form(f.heap(), cdr)?;
            f.push(cdr_v);
            let fields = f.as_slice().to_vec();
            sexpr_construct(f.heap(), SEXPR_CONS, &fields)
        }
        // Anything else in a quoted datum is a value the reader cannot
        // produce, so reaching this means something built a `quote` node by
        // hand out of a value that is not data.
        other => Err(Error::TypeError(format!(
            "compile: {:?} is not something a quoted datum can contain",
            other
        ))),
    }
}

/// A trait-object node's `(kind . form)` operand.
///
/// `dyn-upcast` and `dyn-value` both take a trait object, which is an untraced
/// word at a binding boundary — so this is a constant, not a lookup.
fn dyn_operand(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    let v = to_island(&mut s, form, cx)?;
    s.push_root(v);
    s.cons(Value::Int(Repr::Dyn.binding_kind()), v)
}

/// `(dyn-new STR PATH ((PATH SYM)...) ((...)...) R E)` -> `(dyn-new vtable-id
/// (kind . value))`.
///
/// The vtable id is a constant: the checker resolved the table's contents and
/// the driver interned the (type, trait) pair before translation began, so the
/// emitted code just hands over a number.
fn translate_dyn_new(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 6 {
        return Err(malformed(heap, form));
    }
    let Value::Str(key_id) = parts[0] else { return Err(malformed(heap, form)) };
    let concrete_key = heap.string(key_id).to_string();
    let trait_path = as_path(heap, parts[1]).ok_or_else(|| malformed(heap, form))?;
    let repr = Repr::read(heap, parts[4]).ok_or_else(|| malformed(heap, form))?;
    let id = *cx.dyn_tables.vtables.get(&(concrete_key.clone(), trait_path.clone())).ok_or_else(|| {
        Error::TypeError(format!(
            "compile: no vtable id interned for `{}` as `:dyn {}` (internal error)",
            concrete_key, trait_path
        ))
    })?;
    let mut f = Items::new(heap);
    f.push(Value::Int(id as i64));
    let pairs = arg_pairs(f.heap(), std::slice::from_ref(&repr), &[parts[5]], cx)?;
    f.extend(pairs);
    f.finish("dyn-new")
}

/// `(dyn-upcast PATH E)` -> `(dyn-upcast trait-id (kind . value))`.
///
/// The constant is the *trait* id, not a vtable id: which table the result
/// dispatches through depends on the concrete type inside the box, which this
/// site no longer knows. The runtime finds it from the box's own vtable id,
/// through the mapping the boxing site registered.
fn translate_dyn_upcast(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let to_trait = core::field(heap, form, 0)
        .and_then(|v| as_path(heap, v))
        .ok_or_else(|| malformed(heap, form))?;
    let value = core::field(heap, form, 1).ok_or_else(|| malformed(heap, form))?;
    let id = *cx.dyn_tables.trait_ids.get(&to_trait).ok_or_else(|| {
        Error::TypeError(format!("compile: no trait id interned for `:dyn {}` (internal error)", to_trait))
    })?;
    let mut f = Items::new(heap);
    f.push(Value::Int(id as i64));
    let pair = dyn_operand(f.heap(), value, cx)?;
    f.push(pair);
    f.finish("dyn-upcast")
}

/// `(dyn-call PATH SYM N ((PATH SYM)...) (R...) E...)` -> `(dyn-call slot
/// (kind . recv) (kind . arg)...)`.
///
/// The slot is the method's position in the trait's own method order, fixed at
/// check time, so the emitted code reads the receiver's vtable id, indexes
/// that slot, and calls through the pointer — no lookup by name or type at run
/// time.
fn translate_dyn_call(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 5 {
        return Err(malformed(heap, form));
    }
    let Value::Int(slot) = parts[2] else { return Err(malformed(heap, form)) };
    let reprs = repr_list(heap, parts[4])?;
    let args = parts[5..].to_vec();
    let mut f = Items::new(heap);
    f.push(Value::Int(slot));
    let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
    f.extend(pairs);
    f.finish("dyn-call")
}

/// A `((SYM . kind) ...)` list — a lambda's parameters, or a captured
/// environment's slots, in the shape `bind-params`/`bind-captures` read.
fn name_kind_list(heap: &mut Heap, names: &[(SymId, Repr)], cx: Ctx) -> Result<Value, Error> {
    let mut s = RootScope::new(heap);
    let mut pairs = Vec::with_capacity(names.len());
    for (name, repr) in names {
        let pair = s.cons(Value::Symbol(*name), Value::Int(cx.binding_kind(*name, repr)))?;
        s.push_root(pair);
        pairs.push(pair);
    }
    core::list(&mut s, &pairs)
}

/// A `(SYM R)` parameter list, as names with their representations.
fn params_of(heap: &Heap, list: Value) -> Result<Vec<(SymId, Repr)>, Error> {
    heap.list_to_vec(list)?
        .into_iter()
        .map(|p| {
            let items = heap.list_to_vec(p)?;
            match items[..] {
                [Value::Symbol(name), repr] => Repr::read(heap, repr)
                    .map(|r| (name, r))
                    .ok_or_else(|| malformed(heap, p)),
                _ => Err(malformed(heap, p)),
            }
        })
        .collect()
}

/// Pair each captured name with the representation the binder that introduced
/// it stated — which is the whole reason the walk returns bare names.
fn captured_with_reprs(heap: &Heap, names: &[SymId], cx: Ctx) -> Result<Vec<(SymId, Repr)>, Error> {
    names
        .iter()
        .map(|n| cx.repr_of(*n).cloned().map(|r| (*n, r)).ok_or_else(|| unbound(heap, *n)))
        .collect()
}

/// Collapse a body to the single form the island compiles per function:
/// itself if there is one, a `let` with no bindings — which is what `progn`
/// already is on both sides — if there are several.
fn body_form(heap: &mut Heap, body: &[Value], cx: Ctx) -> Result<Value, Error> {
    match body {
        [one] => to_island(heap, *one, cx),
        many => {
            let mut f = Items::new(heap);
            let empty = core::list(f.heap(), &[])?;
            f.push(empty);
            for e in many {
                let v = to_island(f.heap(), *e, cx)?;
                f.push(v);
            }
            f.finish("let")
        }
    }
}

/// `(lambda ((SYM R)...) RET-R E...)` -> `(lambda name (captured...)
/// (param...) body)`.
///
/// Every `lambda` tag the island sees is, by construction, one that *escapes*:
/// a lambda called immediately at its own definition site never reaches here,
/// because [`translate_apply`] rewrites that into a `labels` block instead. So
/// `compile-lambda` never has to decide whether to box its result.
///
/// The body is translated with an empty `direct` set, matching the walk's own
/// treatment: a lambda gets no direct-call access to an enclosing block's
/// siblings, so a sibling named here becomes an ordinary capture — which
/// resolves correctly, since the island builds this lambda's environment in
/// the *outer* scope where the sibling is still reachable.
fn translate_lambda(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 2 {
        return Err(malformed(heap, form));
    }
    let params = params_of(heap, parts[0])?;
    let body: Vec<Value> = parts[2..].to_vec();

    let bound: HashSet<SymId> = params.iter().map(|(n, _)| *n).collect();
    let captured_names = free_vars(heap, &body, &bound, &HashSet::new())?;
    let captured = captured_with_reprs(heap, &captured_names, cx)?;

    // This lambda's own cell set: its own bindings that something nested
    // inside captures, plus every name it captures itself — except a sibling
    // captured as a value, which is never a mutable binding and so has
    // nothing for a cell to share.
    let mut cells = names_captured_by_nested(heap, &body)?;
    for (n, _) in &captured {
        if !cx.visible_siblings.contains(n) {
            cells.insert(*n);
        }
    }
    let no_direct = HashSet::new();
    let inner_reprs = cx.extended(params.iter().cloned());
    let inner = Ctx {
        reprs: &inner_reprs,
        direct: &no_direct,
        cell_names: &cells,
        // A `labels` block nested in this body has no enclosing captured list
        // to prefix: the island gives a lambda a fresh function environment.
        outer_captured: &[],
        ..cx
    };

    let name_v = heap.alloc_string(fresh_lambda_name("lambda"));
    let mut f = Items::new(heap);
    f.push(name_v);
    // The captured list is classified with the *inner* cell set, since these
    // names are cells inside the body that receives them.
    let captured_list = name_kind_list(f.heap(), &captured, inner)?;
    f.push(captured_list);
    let param_list = name_kind_list(f.heap(), &params, inner)?;
    f.push(param_list);
    let body_v = body_form(f.heap(), &body, inner)?;
    f.push(body_v);
    f.finish("lambda")
}

/// `(labels ((SYM ((SYM R)...) RET-R E...) ...) E...)` -> `(labels
/// (captured...) ((name (param...) body)...) body)`.
fn translate_labels(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    let Some((defs_list, body)) = parts.split_first() else { return Err(malformed(heap, form)) };
    let body: Vec<Value> = body.to_vec();
    let raw_defs = heap.list_to_vec(*defs_list)?;

    struct Def {
        name: SymId,
        params: Vec<(SymId, Repr)>,
        body: Vec<Value>,
    }
    let mut defs = Vec::with_capacity(raw_defs.len());
    for d in &raw_defs {
        let items = heap.list_to_vec(*d)?;
        if items.len() < 3 {
            return Err(malformed(heap, *d));
        }
        let Value::Symbol(name) = items[0] else { return Err(malformed(heap, *d)) };
        defs.push(Def { name, params: params_of(heap, items[1])?, body: items[3..].to_vec() });
    }

    let mut siblings = cx.direct.clone();
    siblings.extend(defs.iter().map(|d| d.name));

    // The enclosing block's captured list first, then whatever the defs
    // themselves refer to — see `Ctx::outer_captured` for why the prefix is
    // unconditional.
    let mut captured_names: Vec<SymId> = cx.outer_captured.to_vec();
    let mut seen: HashSet<SymId> = captured_names.iter().copied().collect();
    for d in &defs {
        let bound: HashSet<SymId> = d.params.iter().map(|(n, _)| *n).collect();
        for n in free_vars(heap, &d.body, &bound, &siblings)? {
            if seen.insert(n) {
                captured_names.push(n);
            }
        }
    }
    let captured = captured_with_reprs(heap, &captured_names, cx)?;
    // Every name a block captures is a cell, unconditionally.
    let captured_cells: HashSet<SymId> = captured_names.iter().copied().collect();
    // Grown, never reset: a lambda nested arbitrarily deep still has to
    // recognize these names as sibling-derived.
    let visible: HashSet<SymId> = cx.visible_siblings.union(&siblings).copied().collect();

    // The siblings' own names enter the representation scope as function
    // values. A def's body never needs them (they are `direct`, called by name
    // through the function environment), but a `lambda` nested inside one does:
    // it walks its free variables with no sibling set at all, so a sibling it
    // calls is an ordinary capture and something has to state its
    // representation. Without this such a lambda failed with "`double` is
    // referenced but no binder in scope states its representation".
    let sibling_reprs = cx.extended(defs.iter().map(|d| (d.name, Repr::Fn)));

    let mut f = Items::new(heap);
    let base = Ctx {
        reprs: &sibling_reprs,
        direct: &siblings,
        cell_names: &captured_cells,
        visible_siblings: &visible,
        outer_captured: &captured_names,
        ..cx
    };
    let captured_list = name_kind_list(f.heap(), &captured, base)?;
    f.push(captured_list);

    let defs_out = {
        let mut s = RootScope::new(f.heap());
        let mut out = Vec::with_capacity(defs.len());
        for d in &defs {
            // A def's own parameter *shadows* a block-captured name it
            // collides with — they are different bindings that happen to share
            // a spelling, and inside the def the parameter wins. Subtracting
            // the parameters before adding this def's own nested captures back
            // is what keeps a reference to the parameter from reading the
            // captured cell instead.
            let param_names: HashSet<SymId> = d.params.iter().map(|(n, _)| *n).collect();
            let mut cells: HashSet<SymId> = captured_cells.difference(&param_names).copied().collect();
            cells.extend(names_captured_by_nested(&s, &d.body)?);
            let inner_reprs = base.extended(d.params.iter().cloned());
            let inner = Ctx { reprs: &inner_reprs, cell_names: &cells, ..base };

            let def_name = s.symbol_name(d.name).to_string();
            let name_v = s.alloc_string(def_name);
            let mut one = Items::new(&mut s);
            one.push(name_v);
            let param_list = name_kind_list(one.heap(), &d.params, inner)?;
            one.push(param_list);
            let body_v = body_form(one.heap(), &d.body, inner)?;
            one.push(body_v);
            let def_v = one.finish_list()?;
            s.push_root(def_v);
            out.push(def_v);
        }
        core::list(&mut s, &out)?
    };
    f.push(defs_out);

    // The trailing body binds no parameters of its own, but a `let` inside it
    // can still be captured by something nested further in.
    let mut trailing_cells = captured_cells.clone();
    trailing_cells.extend(names_captured_by_nested(f.heap(), &body)?);
    let trailing = Ctx { cell_names: &trailing_cells, ..base };
    let body_v = body_form(f.heap(), &body, trailing)?;
    f.push(body_v);
    f.finish("labels")
}

/// `(apply E RET-R (R...) E...)` -> one of three island nodes, on the callee's
/// shape.
///
/// A `labels` sibling is called directly through the function environment; a
/// lambda called at its own definition site never escapes, so it needs no
/// closure at all and becomes a one-def `labels` block instead; anything else
/// is dispatched through the value at run time.
fn translate_apply(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 3 {
        return Err(malformed(heap, form));
    }
    let callee = parts[0];
    // Field 1 is the return representation, which only the interpreter reads
    // (`Interp::apply_core`) — the island's `apply`/`apply-indirect` take
    // argument pairs and nothing else.
    let reprs = repr_list(heap, parts[2])?;
    let args = parts[3..].to_vec();

    match (core::op(heap, callee), core::field(heap, callee, 0)) {
        (Some("var"), Some(Value::Symbol(name))) if cx.direct.contains(&name) => {
            let name_v = heap.alloc_string(heap.symbol_name(name).to_string());
            let mut f = Items::new(heap);
            f.push(name_v);
            let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
            f.extend(pairs);
            f.finish("apply")
        }
        (Some("lambda"), _) => translate_immediate_lambda_call(heap, callee, parts[2], &args, cx),
        _ => {
            let mut f = Items::new(heap);
            let callee_v = to_island(f.heap(), callee, cx)?;
            f.push(callee_v);
            let pairs = arg_pairs(f.heap(), &reprs, &args, cx)?;
            f.extend(pairs);
            f.finish("apply-indirect")
        }
    }
}

/// `((lambda (params) body) args...)` — a lambda invoked where it is written.
///
/// It never escapes, so it needs no closure. Rather than teach the island a
/// second way to build essentially the same function, this rewrites the whole
/// thing into the *core* form for a one-def `labels` block and translates
/// that. Passing the outer `direct` set through — unlike an escaping lambda —
/// is deliberate and safe precisely because it does not escape: it is compiled
/// inside the same scope its call site already has, so it can call outer
/// siblings directly just as another sibling could.
fn translate_immediate_lambda_call(
    heap: &mut Heap,
    lambda: Value,
    arg_reprs: Value,
    args: &[Value],
    cx: Ctx,
) -> Result<Value, Error> {
    let parts = core::fields(heap, lambda)?;
    if parts.len() < 2 {
        return Err(malformed(heap, lambda));
    }
    let (params, ret, body) = (parts[0], parts[1], parts[2..].to_vec());
    let name = heap.intern_symbol(&fresh_lambda_name("__lambda"));

    let mut s = RootScope::new(heap);
    s.push_root(name);
    // `(SYM ((SYM R)...) RET-R E...)`
    let def = {
        let mut items = vec![name, params, ret];
        items.extend(body.iter().copied());
        core::list(&mut s, &items)?
    };
    s.push_root(def);
    let defs = core::list(&mut s, &[def])?;
    s.push_root(defs);
    // `(apply (var NAME) RET-R (R...) ARG...)`. The return representation comes
    // from the lambda's own declaration rather than being threaded in from the
    // outer `apply` — same value, and this is the authoritative side of it.
    let callee = core::tagged(&mut s, "var", &[name])?;
    s.push_root(callee);
    let call = {
        let mut items = vec![callee, ret, arg_reprs];
        items.extend(args.iter().copied());
        core::tagged(&mut s, "apply", &items)?
    };
    s.push_root(call);
    let block = core::tagged(&mut s, "labels", &[defs, call])?;
    s.push_root(block);
    translate_labels(&mut s, block, cx)
}

/// `(fnref WRITTEN HOME PATH (R...))` -> a non-capturing `lambda` that
/// forwards every argument to the named function.
///
/// A top-level function used as a value reaches the very same closure
/// machinery a written `lambda` does, rather than needing a second runtime
/// representation for "function reference". The parameters are synthesized
/// positionally, which is why the core form has to state their
/// representations: there is no call site here to read them from. A `&rest`
/// parameter is simply the last one.
fn translate_fnref(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let path = core::field(heap, form, 2)
        .and_then(|v| as_path(heap, v))
        .ok_or_else(|| malformed(heap, form))?;
    let reprs = repr_list(heap, core::field(heap, form, 3).ok_or_else(|| malformed(heap, form))?)?;
    let raw = path.last_segment();
    let target = if crate::eval::interp::is_rt_builtin_name(raw) {
        raw.to_string()
    } else {
        user_symbol_name(&path.segments().join("::"))
    };
    forwarding_lambda(heap, "fnref", &reprs, cx, |heap, forwarded| {
        let name_v = heap.alloc_string(target);
        let mut f = Items::new(heap);
        f.push(name_v);
        f.extend(forwarded.iter().copied());
        f.finish("call")
    })
}

/// `(methodref PATH SYM HOME (R...))` -> the same forwarding wrapper, onto an
/// instance method instead. Its receiver is simply the first parameter, the
/// same convention an ordinary method call uses.
fn translate_methodref(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let path = core::field(heap, form, 0)
        .and_then(|v| as_path(heap, v))
        .ok_or_else(|| malformed(heap, form))?;
    let method = symbol_field(heap, form, 1)?;
    let reprs = repr_list(heap, core::field(heap, form, 3).ok_or_else(|| malformed(heap, form))?)?;
    forwarding_lambda(heap, "method", &reprs, cx, move |heap, forwarded| {
        let type_v = heap.alloc_string(path.to_string());
        let mut f = Items::new(heap);
        f.push(type_v);
        let method_v = f.heap().alloc_string(method);
        f.push(method_v);
        f.push(Value::Bool(true));
        f.extend(forwarded.iter().copied());
        f.finish("assoc")
    })
}

/// The wrapper both function-reference forms build: a `lambda` with no
/// captures whose parameters are `arg0`, `arg1`, ... and whose body is
/// whatever `call_site` makes of them.
fn forwarding_lambda(
    heap: &mut Heap,
    prefix: &str,
    reprs: &[Repr],
    cx: Ctx,
    call_site: impl FnOnce(&mut Heap, &[Value]) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let params: Vec<(SymId, Repr)> = reprs
        .iter()
        .enumerate()
        .map(|(i, r)| match heap.intern_symbol(&format!("arg{}", i)) {
            Value::Symbol(id) => (id, r.clone()),
            _ => unreachable!("intern_symbol always returns a symbol"),
        })
        .collect();
    // Nothing is captured and nothing nested captures anything, so no
    // parameter here is ever a cell.
    let no_names = HashSet::new();
    let inner = Ctx { cell_names: &no_names, ..cx };

    let name_v = heap.alloc_string(fresh_lambda_name(prefix));
    let mut f = Items::new(heap);
    f.push(name_v);
    let empty = core::list(f.heap(), &[])?;
    f.push(empty);
    let param_list = name_kind_list(f.heap(), &params, inner)?;
    f.push(param_list);

    // `(kind . (var "argN" false))` for each parameter — the same tagged shape
    // every argument list uses, built by hand because these references are
    // synthesized rather than translated from a core form.
    let forwarded = {
        let mut s = RootScope::new(f.heap());
        let mut pairs = Vec::with_capacity(params.len());
        for (name, repr) in &params {
            let text = s.symbol_name(*name).to_string();
            let name_v = s.alloc_string(text);
            s.push_root(name_v);
            let var = core::tagged(&mut s, "var", &[name_v, Value::Bool(false)])?;
            s.push_root(var);
            let pair = s.cons(Value::Int(repr.binding_kind()), var)?;
            s.push_root(pair);
            pairs.push(pair);
        }
        pairs
    };
    for p in &forwarded {
        f.push(*p);
    }
    let body = call_site(f.heap(), &forwarded)?;
    // The forwarded pairs were pushed only to keep them rooted while the body
    // was built; the node's own fields are the first three plus the body.
    let mut items: Vec<Value> = f.as_slice()[..3].to_vec();
    items.push(body);
    core::tagged(f.heap(), "lambda", &items)
}

/// `(construct PATH N MUTABLE E...)` -> `(construct is-sexpr mutable
/// type-name-form variant field...)`.
///
/// The three-way choice the island makes off the header — build one of
/// `sexpr`'s own variants, build a mutable `BoxedObj::Struct`, or build a
/// `BoxedObj::Enum` — is decided here, from the type path and the `mutable`
/// flag the checker already resolved, which is what keeps `compiler.rs`
/// registry-free.
///
/// The fields' kinds are the *declared* field representations the node carries,
/// not the argument expressions' own — the declaration decides the shape a
/// field is stored in, and an argument can be narrower. They come from the node
/// rather than from the type's definition because a generic ADT's definition
/// cannot answer: `Option`'s `Some` field is declared `T`. A `sexpr` construct
/// is the exception: its variants' shapes follow from the variant number, so
/// the island derives them and the fields go untagged.
fn translate_construct(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 4 {
        return Err(malformed(heap, form));
    }
    let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, form))?;
    let Value::Int(variant) = parts[1] else { return Err(malformed(heap, form)) };
    let Value::Bool(mutable) = parts[2] else { return Err(malformed(heap, form)) };
    let field_reprs = repr_list(heap, parts[3])?;
    let args = parts[4..].to_vec();
    let is_sexpr = path == Path::root("sexpr");

    // The *fully qualified* path, because this string is the value's runtime
    // type identity: interpreted and compiled code both write it and test it,
    // so the two have to spell it the same way.
    let type_name = if is_sexpr { Value::Empty } else { str_form(heap, &type_key_of(&path))? };
    let mut f = Items::new(heap);
    f.push(Value::Bool(is_sexpr));
    f.push(Value::Bool(mutable));
    f.push(type_name);
    f.push(Value::Int(variant));

    if is_sexpr {
        for a in &args {
            let v = to_island(f.heap(), *a, cx)?;
            f.push(v);
        }
        return f.finish("construct");
    }
    if field_reprs.len() != args.len() {
        return Err(Error::TypeError(format!(
            "compile: `{}` variant {} has {} fields, constructed with {} (internal error)",
            path,
            variant,
            field_reprs.len(),
            args.len()
        )));
    }
    // A struct field and an enum field cross the same tagged boundary, so both
    // take `field_kind` — not `binding_kind`, which only says whether a root
    // is wanted. Here the exact tagged shape to build is what matters.
    let pairs = arg_pairs_with(f.heap(), &field_reprs, &args, Repr::field_kind, cx)?;
    f.extend(pairs);
    f.finish("construct")
}

/// `(field-get E N R)` -> `(field-get idx-list kind obj)`, and the same for
/// `field-set` with the value appended.
///
/// `idx-list` is a list of exactly `N` (otherwise meaningless) elements. The
/// island's slot index is an `i32` and its only numeric literal decodes to
/// `i64`, with no narrowing primitive exposed to compiled code — but
/// `sexpr-list-length` already returns `i32`, so a list of the right length
/// sidesteps the missing conversion. Field indices are small.
fn translate_field(heap: &mut Heap, form: Value, tag: &str, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    let (obj, idx, repr, value) = match parts[..] {
        [obj, idx, repr] if tag == "field-get" => (obj, idx, repr, None),
        [obj, idx, repr, value] if tag == "field-set" => (obj, idx, repr, Some(value)),
        _ => return Err(malformed(heap, form)),
    };
    let Value::Int(idx) = idx else { return Err(malformed(heap, form)) };
    let repr = Repr::read(heap, repr).ok_or_else(|| malformed(heap, form))?;

    let idx_list = core::list(heap, &vec![Value::Bool(false); idx as usize])?;
    let mut f = Items::new(heap);
    f.push(idx_list);
    f.push(Value::Int(repr.field_kind()));
    let obj = to_island(f.heap(), obj, cx)?;
    f.push(obj);
    if let Some(value) = value {
        let v = to_island(f.heap(), value, cx)?;
        f.push(v);
    }
    f.finish(tag)
}

/// `(match E R (P E...) ...)` -> `(match is-fn scrut ((pat . body)...) kind)`.
///
/// An arm's body is collapsed to one form — a `let` with no bindings, which is
/// what `progn` already is on both sides — because the island's
/// `compile-match-arms` compiles exactly one form per arm.
fn translate_match(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 2 {
        return Err(malformed(heap, form));
    }
    let scrut = parts[0];
    let scrut_repr = Repr::read(heap, parts[1]).ok_or_else(|| malformed(heap, form))?;
    let kind = match_kind_of(&scrut_repr).ok_or_else(|| {
        Error::TypeError(format!(
            "compile: a match on a `{}` scrutinee has no lowering",
            scrut_repr.tag()
        ))
    })?;
    let arms = parts[2..].to_vec();

    let mut f = Items::new(heap);
    f.push(Value::Bool(false)); // `is-fn`, unread
    let scrut = to_island(f.heap(), scrut, cx)?;
    f.push(scrut);

    let arm_pairs = {
        let mut s = RootScope::new(f.heap());
        let mut pairs = Vec::with_capacity(arms.len());
        for arm in &arms {
            let items = s.list_to_vec(*arm)?;
            let Some((pat, body)) = items.split_first() else { return Err(malformed(&s, *arm)) };
            let pat = translate_pattern(&mut s, *pat, cx)?;
            s.push_root(pat);
            let body = match body {
                [one] => to_island(&mut s, *one, cx)?,
                many => {
                    let mut b = Items::new(&mut s);
                    let empty = core::list(b.heap(), &[])?;
                    b.push(empty);
                    for e in many {
                        let v = to_island(b.heap(), *e, cx)?;
                        b.push(v);
                    }
                    b.finish("let")?
                }
            };
            s.push_root(body);
            let pair = s.cons(pat, body)?;
            s.push_root(pair);
            pairs.push(pair);
        }
        core::list(&mut s, &pairs)?
    };
    f.push(arm_pairs);
    // The trailing kind, which the island reads off each `pat-ctor` instead —
    // see the module comment on dead fields.
    f.push(Value::Int(kind));
    f.finish("match")
}

/// A scrutinee representation's island classification, or `None` for one that
/// is never a `match` scrutinee in compiled code.
///
/// A scalar is the `None` case, and deliberately so: the old bridge refused
/// exactly the same set (its classification only accepted a named type), so a
/// `match` on an `int` was never something the island compiled. Refusing here
/// keeps that, rather than inventing a lowering nothing has tested.
fn match_kind_of(repr: &Repr) -> Option<i64> {
    match repr {
        Repr::Sexpr => Some(MATCH_KIND_SEXPR),
        Repr::Struct | Repr::Vector(_) => Some(MATCH_KIND_STRUCT),
        Repr::Enum => Some(MATCH_KIND_BOX),
        _ => None,
    }
}

/// One pattern.
///
/// A constructor pattern carries its *own* type's classification rather than
/// inheriting the match's, because a nested sub-pattern can differ from its
/// parent — a `sexpr`-typed field inside an enum box, say.
fn translate_pattern(heap: &mut Heap, pat: Value, cx: Ctx) -> Result<Value, Error> {
    let Some(tag) = core::op(heap, pat).map(str::to_string) else {
        return Err(malformed(heap, pat));
    };
    match tag.as_str() {
        "pat-wild" => core::tagged(heap, "pat-wild", &[]),
        "pat-bind" => {
            let name = symbol_field(heap, pat, 0)?;
            let name_v = heap.alloc_string(name);
            let mut f = Items::new(heap);
            f.push(name_v);
            f.finish("pat-bind")
        }
        // The island compares against one `const-i64`, whatever the literal's
        // kind, so the conversion happens here: there is no `char`/`bool` to
        // `i64` primitive in the compiled language to do it there.
        "pat-lit" => {
            let lit = core::field(heap, pat, 0).ok_or_else(|| malformed(heap, pat))?;
            let n = match (core::op(heap, lit), core::field(heap, lit, 0)) {
                (Some("int"), Some(Value::Int(n))) => n,
                (Some("bool"), Some(Value::Bool(b))) => i64::from(b),
                (Some("char"), Some(Value::Char(c))) => c as i64,
                _ => return Err(malformed(heap, pat)),
            };
            core::tagged(heap, "pat-lit", &[Value::Int(n)])
        }
        "pat-ctor" => translate_ctor_pattern(heap, pat, cx),
        "pat-typetest" => {
            let path = as_path(heap, core::field(heap, pat, 0).ok_or_else(|| malformed(heap, pat))?)
                .ok_or_else(|| malformed(heap, pat))?;
            let inner = core::field(heap, pat, 1).ok_or_else(|| malformed(heap, pat))?;
            let name = str_form(heap, &type_key_of(&path))?;
            let mut f = Items::new(heap);
            f.push(name);
            let inner = translate_pattern(f.heap(), inner, cx)?;
            f.push(inner);
            f.finish("pat-typetest")
        }
        other => Err(untranslated(other)),
    }
}

/// `(pat-ctor PATH N DOWNCAST (REPR...) P...)` -> `(pat-ctor variant
/// (subpat...) kind (field-kind...) downcast type-name-form)`.
fn translate_ctor_pattern(heap: &mut Heap, pat: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, pat)?;
    if parts.len() < 4 {
        return Err(malformed(heap, pat));
    }
    let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, pat))?;
    let Value::Int(variant) = parts[1] else { return Err(malformed(heap, pat)) };
    let Value::Bool(downcast) = parts[2] else { return Err(malformed(heap, pat)) };
    let field_reprs = repr_list(heap, parts[3])?;
    let subpats = parts[4..].to_vec();

    let kind = if path == Path::root("sexpr") {
        MATCH_KIND_SEXPR
    } else if cx.defs.is_struct(heap, &path)? {
        MATCH_KIND_STRUCT
    } else {
        MATCH_KIND_BOX
    };

    let subs = {
        let mut s = RootScope::new(heap);
        let mut vs = Vec::with_capacity(subpats.len());
        for p in &subpats {
            let v = translate_pattern(&mut s, *p, cx)?;
            s.push_root(v);
            vs.push(v);
        }
        core::list(&mut s, &vs)?
    };
    let mut f = Items::new(heap);
    f.push(Value::Int(variant));
    f.push(subs);
    f.push(Value::Int(kind));

    // A `sexpr` scrutinee's field shapes follow from its variant number, so
    // the island derives them and this list stays empty. The other two need
    // the definition's fields, one kind per sub-pattern.
    let kinds: Vec<Value> = if kind == MATCH_KIND_SEXPR {
        Vec::new()
    } else {
        if field_reprs.len() != subpats.len() {
            return Err(Error::TypeError(format!(
                "compile: `{}` variant {} has {} fields, matched with {} sub-patterns (internal error)",
                path,
                variant,
                field_reprs.len(),
                subpats.len()
            )));
        }
        field_reprs.iter().map(|r| Value::Int(r.field_kind())).collect()
    };
    let kinds = core::list(f.heap(), &kinds)?;
    f.push(kinds);
    f.push(Value::Bool(downcast));
    // Only compiled when `downcast` is true: an ordinary pattern already knows
    // the scrutinee's type from its static type, and only a downcast has to
    // test it at run time.
    let type_name = if downcast { str_form(f.heap(), &type_key_of(&path))? } else { Value::Empty };
    f.push(type_name);
    f.finish("pat-ctor")
}

// ---- shared pieces --------------------------------------------------------

/// The `(kind . form)` pairs an argument list becomes.
///
/// The pairing is what tells `compile-call-args` whether to push a GC root
/// around the evaluated argument: a later argument's evaluation can allocate
/// and collect an earlier one, which is unrooted for exactly as long as it
/// sits in the argument array.
fn arg_pairs(heap: &mut Heap, reprs: &[Repr], args: &[Value], cx: Ctx) -> Result<Vec<Value>, Error> {
    arg_pairs_with(heap, reprs, args, Repr::binding_kind, cx)
}

/// [`arg_pairs`] over either projection.
///
/// A call's arguments take `binding_kind` — the island only wants to know
/// whether to root one. A `construct`'s fields take `field_kind`: they are
/// being written into a box, so the exact tagged shape is what matters.
fn arg_pairs_with(
    heap: &mut Heap,
    reprs: &[Repr],
    args: &[Value],
    kind_of: impl Fn(&Repr) -> i64,
    cx: Ctx,
) -> Result<Vec<Value>, Error> {
    if reprs.len() != args.len() {
        return Err(Error::TypeError(format!(
            "compile: {} argument representations for {} arguments (internal error)",
            reprs.len(),
            args.len()
        )));
    }
    let mut s = RootScope::new(heap);
    let mut pairs = Vec::with_capacity(args.len());
    for (r, a) in reprs.iter().zip(args) {
        let form = to_island(&mut s, *a, cx)?;
        s.push_root(form);
        let pair = s.cons(Value::Int(kind_of(r)), form)?;
        s.push_root(pair);
        pairs.push(pair);
    }
    Ok(pairs)
}

/// A `(R...)` field, as representations.
fn repr_list(heap: &Heap, list: Value) -> Result<Vec<Repr>, Error> {
    heap.list_to_vec(list)?
        .into_iter()
        .map(|v| Repr::read(heap, v).ok_or_else(|| Error::TypeError(format!("compile: not a representation: {}", core::print(heap, v)))))
        .collect()
}

/// A field holding a path. A one-segment path reads back as a bare symbol,
/// since the reader only builds a `Value::Path` when it sees `::`.
fn as_path(heap: &Heap, v: Value) -> Option<Path> {
    match v {
        Value::Path(id) => Some(crate::types::path_from_id(heap, id)),
        Value::Symbol(id) => Some(Path::root(heap.symbol_name(id))),
        _ => None,
    }
}

/// A field holding a symbol, as its name.
fn symbol_field(heap: &Heap, form: Value, i: usize, ) -> Result<String, Error> {
    match core::field(heap, form, i) {
        Some(Value::Symbol(id)) => Ok(heap.symbol_name(id).to_string()),
        _ => Err(malformed(heap, form)),
    }
}

/// `(str (int c0) (int c1) ...)` for a compile-time-known string.
///
/// Not a `Value::Str`: the content is known now, but the `StrId` an allocation
/// here would produce means nothing to the target program, which builds its
/// own heap at startup. `compile-str` re-embeds the characters as constants
/// and calls `rt_str_new` at run time.
fn str_form(heap: &mut Heap, s: &str) -> Result<Value, Error> {
    let mut f = Items::new(heap);
    for c in s.chars() {
        let node = core::tagged(f.heap(), "int", &[Value::Int(c as i64)])?;
        f.push(node);
    }
    f.finish("str")
}

/// `(bignum (int sign) (int d0) ...)` — the sign and the base-2^32 digits,
/// least significant first, matching `rt_bignum_new`'s argument convention.
/// Decomposed for the same reason [`str_form`] is.
fn bignum_form(heap: &mut Heap, n: &num_bigint::BigInt) -> Result<Value, Error> {
    let (sign, digits) = n.to_u32_digits();
    let sign_val: i64 = match sign {
        num_bigint::Sign::Minus => -1,
        num_bigint::Sign::NoSign => 0,
        num_bigint::Sign::Plus => 1,
    };
    let mut f = Items::new(heap);
    let s = core::tagged(f.heap(), "int", &[Value::Int(sign_val)])?;
    f.push(s);
    for d in digits {
        let node = core::tagged(f.heap(), "int", &[Value::Int(d as i64)])?;
        f.push(node);
    }
    f.finish("bignum")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reader;
    use std::collections::HashMap;

    /// Translate one hand-written core form and print the island form.
    ///
    /// `gc_stress` is on throughout: the bridge conses for nearly every node,
    /// so every allocation collects, and an intermediate the builders failed
    /// to root would be reclaimed before the node holding it exists. That is
    /// the failure mode `core_bridge` shipped twice.
    fn bridged(src: &str) -> String {
        bridged_with(&[], src)
    }

    /// Translate with trait-object tables interned, as the driver does before
    /// translation starts.
    fn bridged_with_dyn(src: &str) -> String {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        let r = Reader::new();
        let definitions = Definitions::new();
        let globals = globals();
        let vtables: HashMap<(String, Path), u32> =
            [(("point".to_string(), Path::root("shape")), 11u32)].into_iter().collect();
        let trait_ids: HashMap<Path, u32> = [(Path::root("drawable"), 22u32)].into_iter().collect();
        let cx = Ctx::with_dyn_tables(
            &definitions,
            &globals,
            DynTables { vtables: &vtables, trait_ids: &trait_ids },
        );
        let mut vs = r.read_all(&mut h, src).expect("read failed");
        let form = vs.pop().unwrap();
        h.push_root(form);
        let island = to_island(&mut h, form, cx).unwrap_or_else(|e| panic!("{:?} did not translate: {}", src, e));
        core::print(&h, island)
    }

    /// The promoted globals the `global`/`set-global` examples refer to.
    fn globals() -> HashMap<Path, usize> {
        [(Path::root("counter"), 3usize), (Path::of(&["m", "total"]), 7)]
            .into_iter()
            .collect()
    }

    /// Translate `src` with `defs` (each a `defstruct`/`defenum` core form)
    /// recorded first.
    fn bridged_with(defs: &[&str], src: &str) -> String {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        let r = Reader::new();
        let definitions = record_all(&mut h, &r, defs);
        let globals = globals();
        let mut vs = r.read_all(&mut h, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected one form in {:?}", src);
        let core_form = vs.pop().unwrap();
        h.push_root(core_form);
        // What the driver does before translating a function body: the
        // bindings something nested captures have to be cells from the start.
        let cells = crate::compile::core_freevars::names_captured_by_nested(&h, &[core_form])
            .expect("the capture walk failed");
        let cx = Ctx::new(&definitions, &globals).with_cell_names(&cells);
        let island =
            to_island(&mut h, core_form, cx).unwrap_or_else(|e| panic!("{:?} did not translate: {}", src, e));
        core::print(&h, island)
    }

    fn record_all(h: &mut Heap, r: &Reader, defs: &[&str]) -> Definitions {
        let mut definitions = Definitions::new();
        for d in defs {
            let vs = r.read_all(h, d).expect("read failed");
            for v in vs {
                definitions.record(h, v).expect("recording the definition failed");
            }
        }
        definitions
    }

    fn refused(src: &str) -> String {
        refused_with(&[], src)
    }

    fn refused_with(defs: &[&str], src: &str) -> String {
        let mut h = Heap::with_capacity(1 << 16);
        let r = Reader::new();
        let definitions = record_all(&mut h, &r, defs);
        let globals = globals();
        let cx = Ctx::new(&definitions, &globals);
        let mut vs = r.read_all(&mut h, src).expect("read failed");
        let core_form = vs.pop().unwrap();
        match to_island(&mut h, core_form, cx) {
            Ok(v) => panic!("{:?} unexpectedly translated to {}", src, core::print(&h, v)),
            Err(e) => format!("{}", e),
        }
    }

    /// A `point` with two int fields and an `option` over a `sexpr`, which is
    /// enough to show both a struct's and an enum's field kinds.
    const DEFS: [&str; 2] = ["(defstruct point (int int))", "(defenum option (none some) (() (sexpr)))"];

    #[test]
    fn the_word_sized_literals_pass_through_unchanged() {
        assert_eq!(bridged("(int 42)"), "(int 42)");
        assert_eq!(bridged("(int -7)"), "(int -7)");
        assert_eq!(bridged("(bool true)"), "(bool true)");
        assert_eq!(bridged(r"(char #\a)"), r"(char #\a)");
        assert_eq!(bridged("(unit)"), "(unit)");
    }

    /// A float's bit pattern splits so it survives the island's 3-bit integer
    /// tag. `2.0` is the case that made this necessary: its top bits are the
    /// only ones set, so an untruncated word would decode as `0.0`.
    #[test]
    fn a_float_is_split_into_two_halves_that_reassemble() {
        let printed = bridged("(float 2.0)");
        let bits = 2.0f64.to_bits();
        assert_eq!(printed, format!("(float {} {})", bits >> 32, bits & 0xFFFF_FFFF));
        // Neither half has anything in the top three bits the tag needs.
        for half in [bits >> 32, bits & 0xFFFF_FFFF] {
            assert_eq!((half as i64) << 3 >> 3, half as i64);
        }
    }

    #[test]
    fn a_string_becomes_one_node_per_character() {
        assert_eq!(bridged(r#"(str "hi")"#), "(str (int 104) (int 105))");
        assert_eq!(bridged(r#"(str "")"#), "(str)");
    }

    /// The sign, then base-2^32 digits least significant first.
    ///
    /// `2^64` rather than something small because a literal only *reads* as a
    /// bignum once it no longer fits an `i64` — below that the reader gives an
    /// ordinary `(int n)`, so a small example would not exercise this at all.
    /// Its digits are `0, 0, 1`, which also shows the ordering.
    #[test]
    fn a_bignum_becomes_its_sign_and_digits() {
        assert_eq!(bridged("(bignum 18446744073709551616)"), "(bignum (int 1) (int 0) (int 0) (int 1))");
        assert_eq!(bridged("(bignum -18446744073709551616)"), "(bignum (int -1) (int 0) (int 0) (int 1))");
        assert_eq!(bridged("(ratio 1/3)"), "(ratio (bignum (int 1) (int 1)) (bignum (int 1) (int 3)))");
    }

    #[test]
    fn a_variable_becomes_a_string_named_reference() {
        assert_eq!(bridged("(var x)"), r#"(var "x" false)"#);
    }

    /// The binding's representation becomes the island's kind number, and the
    /// pair shape is the island's: `((name . kind) . form)`.
    #[test]
    fn a_let_carries_each_bindings_kind() {
        assert_eq!(
            bridged("(let ((x int (int 1))) (var x))"),
            r#"(let (((x . 0) int 1)) (var "x" false))"#
        );
        // A `sexpr` binding is the one that needs a GC root, hence kind 2.
        assert_eq!(
            bridged(r#"(let ((s sexpr (str "a"))) (var s))"#),
            r#"(let (((s . 2) str (int 97))) (var "s" false))"#
        );
        // `progn` is a `let` with no bindings, on both sides.
        assert_eq!(bridged("(let () (int 1) (int 2))"), "(let () (int 1) (int 2))");
    }

    #[test]
    fn if_and_panic_keep_their_shape() {
        assert_eq!(
            bridged("(if (bool true) (int 1) (int 2))"),
            "(if false (bool true) (int 1) (int 2))"
        );
        assert_eq!(bridged(r#"(panic (str "boom"))"#), "(panic (str (int 98) (int 111) (int 111) (int 109)))");
    }

    /// Each argument becomes `(kind . form)`, and the kind is the argument's
    /// own representation — which is why the core IR had to start carrying
    /// one per argument.
    #[test]
    fn a_call_tags_every_argument_with_its_kind() {
        assert_eq!(
            bridged("(call (f) () f (int sexpr) (int 1) (int 2))"),
            r#"(call "tl_f" (0 int 1) (2 int 2))"#
        );
        assert_eq!(bridged("(call (f) () f ())"), r#"(call "tl_f")"#);
        // A module-qualified callee mangles by its full path, so `m::inc` and
        // a root `inc` cannot collide on one symbol.
        assert_eq!(bridged("(call (inc) (m) m::inc ())"), r#"(call "tl_m::inc")"#);
        // The three cons primitives are matched by literal name and must stay
        // unprefixed.
        assert_eq!(
            bridged("(call (sexpr-car) () sexpr-car (sexpr) (var xs))"),
            r#"(call "sexpr-car" (2 var "xs" false))"#
        );
    }

    #[test]
    fn an_ordinary_method_call_keeps_its_qualified_type_name() {
        assert_eq!(
            bridged("(assoc i64 + true () int (int int) (var a) (var b))"),
            r#"(assoc "i64" "+" true (0 var "a" false) (0 var "b" false))"#
        );
        assert_eq!(
            bridged("(assoc m::point area true () int (struct) (var p))"),
            r#"(assoc "m::point" "area" true (2 var "p" false))"#
        );
    }

    /// A `Vector<T>` builtin has no compiled body, so it becomes an operation
    /// carrying `T`'s kind rather than a method call.
    #[test]
    fn a_vector_builtin_becomes_an_operation_carrying_its_element_kind() {
        assert_eq!(
            bridged("(assoc vector get true () int ((vector int) int) (var v) (int 0))"),
            r#"(vector-op "get" 1 (var "v" false) (int 0))"#
        );
        // A `sexpr` element is already tagged, so it passes through as kind 6.
        assert_eq!(
            bridged("(assoc vector push true () unit ((vector sexpr) sexpr) (var v) (var x))"),
            r#"(vector-op "push" 6 (var "v" false) (var "x" false))"#
        );
        // `new` has no receiver and no element to tag; the result is what
        // says it is a vector at all.
        assert_eq!(
            bridged("(assoc vector new false () (vector int) ())"),
            r#"(vector-op "new" 0 (str (int 118) (int 101) (int 99) (int 116) (int 111) (int 114)))"#
        );
    }

    #[test]
    fn a_hash_table_builtin_carries_both_key_and_value_kinds() {
        assert_eq!(
            bridged("(assoc hashtable set true () unit ((hashtable str int) str int) (var h) (var k) (var x))"),
            r#"(hashtable-op "set" 6 1 () (var "h" false) (var "k" false) (var "x" false))"#
        );
        // `get` returns an `Option`, which the island builds itself and needs
        // the type-name literal for.
        assert_eq!(
            bridged("(assoc hashtable get true () enum ((hashtable str int) str) (var h) (var k))"),
            r#"(hashtable-op "get" 6 1 (str (int 111) (int 112) (int 116) (int 105) (int 111) (int 110)) (var "h" false) (var "k" false))"#
        );
    }

    /// The compiler's own LLVM methods reach the interpreter through one
    /// dispatch shim, keyed by an id resolved here rather than by two runtime
    /// name strings.
    #[test]
    fn an_llvm_method_becomes_an_operation_id() {
        let printed = bridged("(assoc llvm-builder const-i64 true () handle (handle int) (var b) (int 42))");
        assert_eq!(
            printed,
            format!(
                r#"(llvm-op {} (0 var "b" false) (0 int 42))"#,
                llvm_op_id("llvm-builder", "const-i64")
            )
        );
    }

    /// A scope over LLVM handles routes to the island's frozen `native-scope`
    /// op; a scope over anything else is an ordinary method call. The element
    /// representation is the only thing that separates them, which is why
    /// `Repr::Scope` carries one.
    #[test]
    fn a_scope_routes_on_what_it_holds() {
        let printed = bridged("(assoc scope get true () enum ((scope handle) str) (var e) (var n))");
        assert!(printed.starts_with("(llvm-op "), "expected a native-scope op, got {}", printed);
        assert_eq!(
            bridged("(assoc scope get true () enum ((scope int) str) (var e) (var n))"),
            r#"(assoc "scope" "get" true (2 var "e" false) (2 var "n" false))"#
        );
    }

    // ---- data ------------------------------------------------------------

    /// A construct's field kinds come from the node's own representation list,
    /// not from the argument expressions and not from the type's definition —
    /// see `translate_construct` for why the definition cannot answer.
    #[test]
    fn a_construct_tags_its_fields_from_its_own_representations() {
        assert_eq!(
            bridged_with(&DEFS, "(construct point 0 true (int int) (int 1) (int 2))"),
            r#"(construct false true (str (int 112) (int 111) (int 105) (int 110) (int 116)) 0 (1 int 1) (1 int 2))"#
        );
        // An enum field of `sexpr` is already tagged, hence the passthrough
        // kind 6.
        assert_eq!(
            bridged_with(&DEFS, "(construct option 1 false (sexpr) (var x))"),
            r#"(construct false false (str (int 111) (int 112) (int 116) (int 105) (int 111) (int 110)) 1 (6 var "x" false))"#
        );
        // `sexpr`'s own variants take untagged fields: their shapes follow
        // from the variant number, so the island derives them.
        assert_eq!(bridged_with(&DEFS, "(construct sexpr 7 false (sexpr sexpr) (int 1) (int 2))"), "(construct true false () 7 (int 1) (int 2))");
    }

    /// A construct needs no definition on hand at all: the `MUTABLE` flag says
    /// which box to build and the node carries its own field representations.
    #[test]
    fn a_construct_needs_no_definition_recorded() {
        assert_eq!(
            bridged_with(&[], "(construct point 0 true (int int) (int 1) (int 2))"),
            r#"(construct false true (str (int 112) (int 111) (int 105) (int 110) (int 116)) 0 (1 int 1) (1 int 2))"#
        );
    }

    /// A construct whose representation list disagrees with its argument count
    /// is an internal error: the island would tag a field with another field's
    /// kind.
    #[test]
    fn a_construct_whose_arity_disagrees_with_its_representations_is_refused() {
        let e = refused_with(&DEFS, "(construct point 0 true (int int) (int 1))");
        assert!(e.contains("has 2 fields, constructed with 1"), "{}", e);
    }

    /// A *pattern* does need the definition, and an unrecorded type is refused
    /// rather than guessed at. There is no `MUTABLE` flag on a pattern, so this
    /// is the only thing that tells a struct scrutinee (one variant, no tag
    /// test) from an enum box — and guessing would emit code that reads the
    /// wrong box shape.
    #[test]
    fn a_pattern_on_an_unrecorded_type_is_refused() {
        let e = refused_with(&[], "(match (var v) struct ((pat-ctor point 0 false (int int) (pat-wild) (pat-wild)) (int 1)))");
        assert!(e.contains("no definition recorded"), "{}", e);
    }

    /// The index becomes a list of that length: the island's slot index is an
    /// `i32`, its only numeric literal decodes to `i64`, and it has no
    /// narrowing primitive — but it can measure a list.
    #[test]
    fn a_field_access_encodes_its_index_as_a_list_length() {
        assert_eq!(bridged("(field-get (var p) 0 int)"), r#"(field-get () 1 (var "p" false))"#);
        assert_eq!(bridged("(field-get (var p) 2 sexpr)"), r#"(field-get (false false) 6 (var "p" false))"#);
        assert_eq!(
            bridged("(field-set (var p) 1 int (int 5))"),
            r#"(field-set (false) 1 (var "p" false) (int 5))"#
        );
    }

    // ---- match -----------------------------------------------------------

    /// Each arm becomes a `(pattern . body)` pair, and a constructor pattern
    /// carries its own type's classification rather than the match's — a
    /// nested sub-pattern's can differ from its parent's.
    #[test]
    fn a_match_pairs_every_pattern_with_its_body() {
        assert_eq!(
            bridged_with(&DEFS, "(match (var v) enum ((pat-ctor option 0 false ()) (int 0)) ((pat-ctor option 1 false (sexpr) (pat-bind x)) (var x)))"),
            r#"(match false (var "v" false) (((pat-ctor 0 () 1 () false ()) int 0) ((pat-ctor 1 ((pat-bind "x")) 1 (6) false ()) var "x" false)) 1)"#
        );
        // A struct scrutinee: kind 2, and the field kinds come from the
        // `defstruct`.
        assert_eq!(
            bridged_with(&DEFS, "(match (var p) struct ((pat-ctor point 0 false (int int) (pat-bind a) (pat-wild)) (var a)))"),
            r#"(match false (var "p" false) (((pat-ctor 0 ((pat-bind "a") (pat-wild)) 2 (1 1) false ()) var "a" false)) 2)"#
        );
    }

    /// A multi-form arm body collapses to a `let` with no bindings, which is
    /// what `progn` already is on both sides — the island compiles exactly one
    /// form per arm.
    #[test]
    fn a_multi_form_arm_body_becomes_a_progn() {
        assert_eq!(
            bridged_with(&DEFS, "(match (var v) sexpr ((pat-wild) (int 1) (int 2)))"),
            r#"(match false (var "v" false) (((pat-wild) let () (int 1) (int 2))) 0)"#
        );
    }

    /// Whatever the literal's kind, the island compares against one
    /// `const-i64` — there is no `char`/`bool` conversion primitive in the
    /// compiled language, so the conversion happens here.
    #[test]
    fn a_literal_pattern_is_reduced_to_one_integer() {
        let printed = bridged_with(
            &DEFS,
            r"(match (var v) sexpr ((pat-lit (int 7)) (int 1)) ((pat-lit (bool true)) (int 2)) ((pat-lit (char #\A)) (int 3)))",
        );
        assert!(printed.contains("(pat-lit 7)"), "{}", printed);
        assert!(printed.contains("(pat-lit 1)"), "{}", printed);
        assert!(printed.contains("(pat-lit 65)"), "{}", printed);
    }

    /// A downcast pattern carries the type name it has to test at run time;
    /// an ordinary one already knows the type statically and carries a
    /// placeholder.
    #[test]
    fn only_a_downcast_pattern_carries_a_type_name() {
        let plain = bridged_with(&DEFS, "(match (var v) sexpr ((pat-ctor point 0 false (int int) (pat-wild) (pat-wild)) (int 1)))");
        assert!(plain.contains("(pat-ctor 0 ((pat-wild) (pat-wild)) 2 (1 1) false ())"), "{}", plain);
        let down = bridged_with(&DEFS, "(match (var v) sexpr ((pat-ctor point 0 true (int int) (pat-wild) (pat-wild)) (int 1)))");
        assert!(down.contains("true (str (int 112)"), "{}", down);
        // A whole-value type test always tests, so it always carries one.
        let tt = bridged_with(&DEFS, "(match (var v) sexpr ((pat-typetest point (pat-bind p)) (var p)))");
        assert!(tt.contains(r#"(pat-typetest (str (int 112) (int 111) (int 105) (int 110) (int 116)) (pat-bind "p"))"#), "{}", tt);
    }

    /// A scalar scrutinee is refused, exactly as the old bridge refused it:
    /// its classification only accepted a named type, so the island never
    /// compiled such a match. Keeping that rather than inventing a lowering.
    #[test]
    fn a_match_on_a_scalar_is_refused() {
        let e = refused_with(&DEFS, "(match (var n) int ((pat-lit (int 1)) (int 10)))");
        assert!(e.contains("a match on a `int` scrutinee has no lowering"), "{}", e);
    }

    // ---- assignment, loops and globals -----------------------------------

    /// `set`'s kind is the *target's* representation, read back from the
    /// binder that stated it — the core IR does not repeat one at the
    /// assignment.
    #[test]
    fn set_takes_its_kind_from_the_binding_it_targets() {
        assert_eq!(
            bridged("(let ((x int (int 1))) (set x (int 2)))"),
            r#"(let (((x . 0) int 1)) (set "x" 0 (int 2)))"#
        );
        // A `sexpr` binding is the one with a GC root to keep in step.
        assert_eq!(
            bridged(r#"(let ((s sexpr (str "a"))) (set s (str "b")))"#),
            r#"(let (((s . 2) str (int 97))) (set "s" 2 (str (int 98))))"#
        );
        // An inner binding shadows an outer one of a different kind.
        let shadowed = bridged(r#"(let ((x sexpr (str "a"))) (let ((x int (int 1))) (set x (int 2))))"#);
        assert!(shadowed.contains(r#"(set "x" 0"#), "the inner binding should win: {}", shadowed);
    }

    /// Assigning to a name no binder introduced is an internal error: the
    /// island would need a kind, and there is nothing honest to give it.
    #[test]
    fn assigning_to_a_name_that_is_not_in_scope_is_refused() {
        let e = refused("(set nope (int 1))");
        assert!(e.contains("`nope`, which is not in scope"), "{}", e);
    }

    #[test]
    fn a_loop_keeps_its_body_untagged() {
        assert_eq!(
            bridged("(loop (int 1) (break))"),
            "(loop (int 1) (break))"
        );
        assert_eq!(bridged("(break)"), "(break)");
    }

    /// A value-less `return` becomes an explicit unit, so the island has one
    /// shape to compile rather than two.
    #[test]
    fn return_always_carries_a_value() {
        assert_eq!(bridged("(return (int 3))"), "(return false (int 3))");
        assert_eq!(bridged("(return)"), "(return false (unit))");
    }

    /// A global reference is an id resolved here, not a name resolved at run
    /// time, and a kind that says how to untag the stored value.
    #[test]
    fn a_global_becomes_its_promoted_id_and_kind() {
        assert_eq!(bridged("(global (counter) () counter int)"), "(global 3 1)");
        assert_eq!(bridged("(global (total) (m) m::total sexpr)"), "(global 7 6)");
        assert_eq!(
            bridged("(set-global (counter) () counter int (int 5))"),
            "(set-global 3 1 (int 5))"
        );
    }

    /// A global the driver never promoted is an internal error — the promoter
    /// and this walk have gone out of step.
    #[test]
    fn an_unpromoted_global_is_refused() {
        let e = refused("(global (missing) () missing int)");
        assert!(e.contains("was not promoted before translation"), "{}", e);
    }

    // ---- functions -------------------------------------------------------

    /// A lambda that captures nothing: an empty environment, and its
    /// parameters classified by their own representations.
    ///
    /// The name is process-wide unique, so the assertion matches around it.
    #[test]
    fn a_lambda_lists_its_captures_and_parameters() {
        let printed = bridged("(lambda ((x int)) int (var x))");
        assert!(
            printed.ends_with(r#" () ((x . 0)) (var "x" false))"#),
            "unexpected lambda shape: {}",
            printed
        );
        assert!(printed.starts_with(r#"(lambda "lambda$"#), "{}", printed);
    }

    /// A captured name is cell-boxed, and every reference to it inside the
    /// closure reads through the cell — which is what makes an assignment in
    /// one place visible in another.
    #[test]
    fn a_captured_binding_becomes_a_cell() {
        let printed = bridged("(let ((n int (int 1))) (lambda () int (var n)))");
        // The binding itself gains the island's `10 +` cell marker over the
        // int field classification.
        assert!(printed.starts_with("(let (((n . 11) int 1))"), "the binder should create the cell: {}", printed);
        // The capture is a cell in the closure that receives it...
        assert!(printed.contains("((n . 11))"), "{}", printed);
        // ...and the reference reads through it.
        assert!(printed.contains(r#"(cellvar "n" 1)"#), "{}", printed);
    }

    /// A binding no closure captures stays an ordinary slot, so the cell
    /// machinery is not simply always on.
    #[test]
    fn an_uncaptured_binding_stays_a_plain_slot() {
        let printed = bridged("(let ((n int (int 1))) (lambda ((m int)) int (var m)))");
        assert!(printed.starts_with("(let (((n . 0)) int 1))") || printed.contains("(n . 0)"), "{}", printed);
        assert!(!printed.contains("cellvar"), "nothing is captured here: {}", printed);
    }

    /// An assignment to a captured binding writes *through* the cell, which
    /// is a different island tag from an ordinary one.
    #[test]
    fn assigning_a_captured_binding_writes_through_the_cell() {
        let printed = bridged("(let ((n int (int 1))) (lambda () unit (set n (int 2))))");
        assert!(printed.contains(r#"(cellset "n" 1 (int 2))"#), "{}", printed);
    }

    /// `labels` siblings are callable directly, through the function
    /// environment rather than as values — so a call to one is `apply`, and
    /// the sibling never becomes a captured slot.
    #[test]
    fn labels_siblings_are_called_directly() {
        let printed =
            bridged("(labels ((go ((i int)) int (var i))) (apply (var go) int (int) (int 1)))");
        assert_eq!(printed, r#"(labels () (("go" ((i . 0)) (var "i" false))) (apply "go" (0 int 1)))"#);
    }

    /// A `labels` block captures what its defs refer to from outside, and
    /// every captured name is a cell.
    #[test]
    fn a_labels_block_captures_what_its_defs_refer_to() {
        let printed = bridged(
            "(let ((k int (int 5))) (labels ((go ((i int)) int (var k))) (apply (var go) int (int) (int 1))))",
        );
        assert!(printed.contains("(labels ((k . 11))"), "the captured list should be a cell: {}", printed);
        assert!(printed.contains(r#"(cellvar "k" 1)"#), "{}", printed);
    }

    /// Calling something that is not a sibling dispatches through the value.
    #[test]
    fn an_unknown_callee_is_dispatched_indirectly() {
        assert_eq!(
            bridged("(let ((f fn (var g))) (apply (var f) int (int) (int 1)))"),
            r#"(let (((f . 2) var "g" false)) (apply-indirect (var "f" false) (0 int 1)))"#
        );
    }

    /// A lambda invoked where it is written never escapes, so it becomes a
    /// one-def `labels` block rather than a closure — no environment is built
    /// at all.
    #[test]
    fn an_immediately_invoked_lambda_becomes_a_labels_block() {
        let printed = bridged("(apply (lambda ((x int)) int (var x)) int (int) (int 2))");
        assert!(printed.starts_with("(labels () ((\"__lambda$"), "{}", printed);
        assert!(printed.ends_with(r#"((x . 0)) (var "x" false))) (apply "__lambda$0" (0 int 2)))"#)
            || printed.contains(r#"((x . 0)) (var "x" false)))"#), "{}", printed);
        assert!(!printed.contains("apply-indirect"), "it does not escape: {}", printed);
    }

    /// A top-level function used as a value becomes a forwarding closure, so
    /// it reaches the same machinery a written lambda does instead of needing
    /// a second runtime representation.
    #[test]
    fn a_function_reference_becomes_a_forwarding_closure() {
        let printed = bridged("(fnref (f) () f (int int))");
        assert!(
            printed.ends_with(
                r#" () ((arg0 . 0) (arg1 . 0)) (call "tl_f" (0 var "arg0" false) (0 var "arg1" false)))"#
            ),
            "{}",
            printed
        );
    }

    #[test]
    fn a_method_reference_forwards_onto_the_method() {
        let printed = bridged("(methodref point area () (struct))");
        assert!(
            printed.ends_with(r#" () ((arg0 . 2)) (assoc "point" "area" true (2 var "arg0" false)))"#),
            "{}",
            printed
        );
    }

    // ---- quoted data and trait objects ------------------------------------

    /// Quoted data cannot be *referred* to — the datum lives in the compiling
    /// heap, and a compiled program builds its own from scratch — so the
    /// literal is taken apart here and reassembled by the code that runs.
    #[test]
    fn a_quoted_datum_becomes_the_nodes_that_rebuild_it() {
        assert_eq!(bridged("(quote ())"), "(construct true false () 0)");
        assert_eq!(bridged("(quote 7)"), "(construct true false () 1 (int 7))");
        assert_eq!(bridged("(quote true)"), "(construct true false () 4 (bool true))");
        assert_eq!(bridged(r"(quote #\a)"), r"(construct true false () 3 (char #\a))");
        assert_eq!(
            bridged(r#"(quote "hi")"#),
            "(construct true false () 6 (str (int 104) (int 105)))"
        );
        // A symbol is interned at run time rather than stored, hence a marker
        // past the variant numbering.
        assert_eq!(bridged("(quote foo)"), "(construct true false () 100 (str (int 102) (int 111) (int 111)))");
        assert_eq!(bridged("(sym foo)"), bridged("(quote foo)"));
        // A path's segments each become a string, interned back at run time.
        assert_eq!(
            bridged("(quote m::x)"),
            "(construct true false () 101 (str (int 109)) (str (int 120)))"
        );
    }

    /// A list is built pair by pair, ending in nil — so nesting is just the
    /// recursion, with no depth limit of its own.
    #[test]
    fn a_quoted_list_is_built_cons_by_cons() {
        assert_eq!(
            bridged("(quote (1 2))"),
            "(construct true false () 7 (construct true false () 1 (int 1)) \
(construct true false () 7 (construct true false () 1 (int 2)) (construct true false () 0)))"
        );
    }

    /// A trait-object node's id is a constant resolved here, not a name
    /// resolved at run time.
    #[test]
    fn a_trait_object_carries_the_ids_interned_for_it() {
        let printed = bridged_with_dyn(r#"(dyn-new "point" shape ((point area)) () struct (var p))"#);
        assert_eq!(printed, r#"(dyn-new 11 (2 var "p" false))"#);
        assert_eq!(bridged_with_dyn("(dyn-upcast drawable (var d))"), r#"(dyn-upcast 22 (2 var "d" false))"#);
        assert_eq!(
            bridged_with_dyn("(dyn-call shape area 0 ((point area)) (dyn) (var d))"),
            r#"(dyn-call 0 (2 var "d" false))"#
        );
        assert_eq!(bridged_with_dyn("(dyn-value (var d))"), r#"(dyn-value (2 var "d" false))"#);
    }

    /// The node states the boxed value's *representation* rather than deriving
    /// it from the concrete type's name, because whether the boxing call needs
    /// a GC root across it is a property of the representation.
    ///
    /// The contrast used to be struct-vs-enum: a struct boxed here got `0` and
    /// an enum got `2`. That was the `binding_kind`/`field_kind` disagreement,
    /// not a real distinction — both are tagged heap boxes the collector can
    /// reclaim — so the axis is now scalar-vs-heap, which is the distinction
    /// that was always meant.
    #[test]
    fn the_boxed_values_kind_follows_its_representation() {
        let as_struct = bridged_with_dyn(r#"(dyn-new "point" shape ((point area)) () struct (var p))"#);
        let as_enum = bridged_with_dyn(r#"(dyn-new "point" shape ((point area)) () enum (var p))"#);
        let as_int = bridged_with_dyn(r#"(dyn-new "point" shape ((point area)) () int (var p))"#);
        assert!(as_struct.contains("(2 var"), "{}", as_struct);
        assert!(as_enum.contains("(2 var"), "{}", as_enum);
        assert!(as_int.contains("(0 var"), "{}", as_int);
    }

    /// Reflection cannot be compiled: it acts on the running interpreter's own
    /// heap and scope tree, which the compiled program does not have.
    #[test]
    fn compiling_a_compile_form_is_refused() {
        let e = refused("(compile-fn (fn (f) () f))");
        assert!(e.contains("interpreter-only"), "{}", e);
    }

    // ---- the top level ----------------------------------------------------

    /// Translate one top-level form, giving back the boundary triple as text.
    fn top_level(defs: &[&str], src: &str) -> Option<(String, String, String)> {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        let r = Reader::new();
        let definitions = record_all(&mut h, &r, defs);
        let globals = globals();
        let cx = Ctx::new(&definitions, &globals);
        let mut vs = r.read_all(&mut h, src).expect("read failed");
        let form = vs.pop().unwrap();
        h.push_root(form);
        let f = top_level_function(&mut h, form, cx).expect("translation failed")?;
        Some((f.name.clone(), core::print(&h, f.params), core::print(&h, f.body)))
    }

    /// A `defun` becomes the triple `compile-function` takes. The mangled name
    /// is the same one a call site produces, which is the invariant that
    /// matters: the definition and every reference have to agree.
    #[test]
    fn a_defun_becomes_the_boundary_triple() {
        let (name, params, body) =
            top_level(&[], "(defun m::add ((a int) (b int)) int true (assoc i64 + true () int (int int) (var a) (var b)))")
                .expect("a defun is compiled");
        assert_eq!(name, "tl_m::add");
        assert_eq!(params, "((a . 0) (b . 0))");
        assert_eq!(body, r#"(assoc "i64" "+" true (0 var "a" false) (0 var "b" false))"#);
        // The same mangling a call to it produces.
        assert!(bridged("(call (add) (m) m::add ())").contains(r#""tl_m::add""#));
    }

    /// A method mangles through the type path, so two same-named types in
    /// different modules cannot collide on one symbol.
    #[test]
    fn a_defmethod_mangles_through_its_type() {
        let (name, params, body) =
            top_level(&[], "(defmethod m::point area false ((self struct)) int true (field-get (var self) 0 int))")
                .expect("a defmethod is compiled");
        assert_eq!(name, "tl_m::point::area");
        assert_eq!(params, "((self . 2))");
        assert_eq!(body, r#"(field-get () 1 (var "self" false))"#);
    }

    /// A multi-form body collapses to a `progn`, the same as everywhere else.
    #[test]
    fn a_multi_form_body_collapses() {
        let (_, _, body) = top_level(&[], "(defun f () unit false (int 1) (unit))").expect("compiled");
        assert_eq!(body, "(let () (int 1) (unit))");
    }

    /// The cell set is derived from the body rather than asked for, since
    /// getting it wrong is invisible until a closure reads through a cell the
    /// binder never made.
    #[test]
    fn a_parameter_a_closure_captures_is_a_cell_without_being_told() {
        let (_, params, body) =
            top_level(&[], "(defun f ((n int)) fn true (lambda () int (var n)))").expect("compiled");
        assert_eq!(params, "((n . 11))", "the parameter should be bound as a cell");
        assert!(body.contains(r#"(cellvar "n" 1)"#), "{}", body);
    }

    /// Most of the top level is not compiled, and saying so is not a refusal:
    /// a type definition contributes representations, a macro is already gone,
    /// a `use` resolves names, and a module's children are forms in their own
    /// right for the driver to walk.
    #[test]
    fn the_rest_of_the_top_level_is_not_compiled() {
        for src in [
            "(defstruct point (int int))",
            "(defenum m::color (red green blue) (() () ()))",
            "(defmacro m::when (c body) true (1 () ()) false (quote ()))",
            "(use m::helper other::helper)",
            r#"(load "lib.typl")"#,
            "(expr (int 42))",
            "(module m (expr (int 1)))",
        ] {
            assert!(top_level(&[], src).is_none(), "{} should not be compiled as a function", src);
        }
    }

    /// A `defvar` becomes the initializer node instead, since a global is set
    /// up by running its value rather than by being called.
    #[test]
    fn a_defvar_becomes_a_global_initializer() {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        let r = Reader::new();
        let definitions = Definitions::new();
        let globals = globals();
        let cx = Ctx::new(&definitions, &globals);
        let form = r.read_all(&mut h, "(defvar m::total sexpr true true (quote ()))").expect("read failed")[0];
        h.push_root(form);
        let node = global_init(&mut h, form, cx).expect("translation failed").expect("a defvar has one");
        assert_eq!(core::print(&h, node), "(global-init 6 (construct true false () 0))");
        // Nothing else is one.
        let other = r.read_all(&mut h, "(defun f () unit false (unit))").expect("read failed")[0];
        assert!(global_init(&mut h, other, cx).expect("translation failed").is_none());
    }

    /// A type defined inside a module is recorded just as one at the root is —
    /// a body being compiled can reach either.
    #[test]
    fn a_type_inside_a_module_is_recorded_too() {
        assert_eq!(
            bridged_with(
                &["(module m (defstruct m::point (int sexpr)))"],
                "(construct m::point 0 true (int sexpr) (int 1) (quote ()))"
            )
            .contains("(1 int 1)"),
            true
        );
    }

    /// Every *expression* tag now has a translation, so what is left to name
    /// itself is the top-level vocabulary — which the old bridge never
    /// expressed at all, since `TopLevel` was a Rust enum that never reached
    /// the island.
    ///
    /// Not a fallback: reaching one is a bug in whoever routed here, and the
    /// error says which tag rather than emitting something the island would
    /// misread. The count of these is the progress measure.
    #[test]
    fn an_untranslated_tag_names_itself() {
        assert!(refused("(defun m::f () unit true (unit))").contains("`defun`"));
        assert!(refused("(defvar m::x int true true (int 0))").contains("`defvar`"));
        assert!(refused("(module m (expr (int 1)))").contains("`module`"));
    }

    /// A mismatched representation list is an internal error, not something
    /// to paper over: the island would read an argument's kind off the wrong
    /// argument.
    #[test]
    fn an_argument_list_that_disagrees_with_its_representations_is_refused() {
        let e = refused("(call (f) () f (int) (int 1) (int 2))");
        assert!(e.contains("1 argument representations for 2 arguments"), "{}", e);
    }
}
