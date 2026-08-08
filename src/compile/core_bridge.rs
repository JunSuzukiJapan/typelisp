//! Translates a *core form* (`crate::check::core`) into the shape the
//! self-hosted compiler in `crate::compiler` pattern-matches on.
//!
//! Both sides are cons cells, so this is a cons-to-cons rewrite rather than
//! the tree-to-cons walk `compile::ast_bridge` does today. That is the whole
//! point of the change: the checker used to lower cons cells into a Rust tree
//! and the bridge turned that tree straight back into cons cells, and nothing
//! in between needed the tree.
//!
//! Being additive, this is built one tag at a time while `ast_bridge` still
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
//! not good enough — `ast_bridge` balances sixteen pops by hand in one
//! function, and one `?` skips them all.

use std::collections::HashMap;

use typelisp_mem::{Error, Heap, RootScope, Value};

use crate::check::core::{self, Items};
use crate::check::repr::Repr;
use crate::type_key::type_key_of;
use crate::types::{path_is_builtin, path_is_builtin_any, Path, LLVM_METHOD_RECEIVER_TYPES};

use super::ast_bridge::{
    llvm_op_id, user_symbol_name, HASHTABLE_BUILTIN_METHODS, VECTOR_BUILTIN_METHODS,
};

/// Every field representation of every type this translation can mention.
///
/// A field's `Repr` is a property of the *type*, so the core IR records it
/// once, on the `defstruct`/`defenum` — not once per `construct` and once more
/// per pattern the way `Pattern::Ctor`'s `field_types`/`sexpr_fields` did. The
/// price is that the bridge needs the definitions on hand, which is what this
/// is: a snapshot taken from the same core forms, so there is still no live
/// `Registry` anywhere in this module.
#[derive(Default, Debug)]
pub struct Definitions {
    /// A `defstruct`'s fields.
    structs: HashMap<Path, Vec<Repr>>,
    /// A `defenum`'s fields, per variant.
    enums: HashMap<Path, Vec<Vec<Repr>>>,
}

impl Definitions {
    pub fn new() -> Definitions {
        Definitions::default()
    }

    /// Record a `(defstruct PATH (R...))` or
    /// `(defenum PATH (SYM...) ((R...)...))` core form.
    ///
    /// Anything else is ignored rather than refused: this is meant to be fed
    /// a whole program's top level, and every other form there is simply not a
    /// type definition.
    pub fn record(&mut self, heap: &Heap, form: Value) -> Result<(), Error> {
        let Some(tag) = core::op(heap, form) else { return Ok(()) };
        match tag {
            "defstruct" => {
                let parts = core::fields(heap, form)?;
                let [path, fields] = parts[..] else { return Err(malformed(heap, form)) };
                let path = as_path(heap, path).ok_or_else(|| malformed(heap, form))?;
                self.structs.insert(path, repr_list(heap, fields)?);
            }
            "defenum" => {
                let parts = core::fields(heap, form)?;
                let [path, _variants, fields] = parts[..] else { return Err(malformed(heap, form)) };
                let path = as_path(heap, path).ok_or_else(|| malformed(heap, form))?;
                let per_variant = heap
                    .list_to_vec(fields)?
                    .into_iter()
                    .map(|v| repr_list(heap, v))
                    .collect::<Result<Vec<_>, _>>()?;
                self.enums.insert(path, per_variant);
            }
            _ => {}
        }
        Ok(())
    }

    /// The field representations of `path`'s `variant`, or `None` if this is
    /// not a type with fields to read — a built-in `sexpr` constructor, whose
    /// field shapes the island derives from the variant number itself.
    fn fields_of(&self, path: &Path, variant: usize) -> Option<&[Repr]> {
        if let Some(fs) = self.structs.get(path) {
            return Some(fs);
        }
        self.enums.get(path)?.get(variant).map(Vec::as_slice)
    }

    /// Whether `path` is a `defstruct` — which the island needs because a
    /// struct has exactly one variant and so gets no tag test, only field
    /// extraction.
    fn is_struct(&self, path: &Path) -> bool {
        self.structs.contains_key(path)
    }
}

/// What a whole translation reads and never changes.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub defs: &'a Definitions,
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
            let name = symbol_field(heap, form, 0)?;
            let name_v = heap.alloc_string(name);
            let mut f = Items::new(heap);
            f.push(name_v);
            // `is-fn`, which `compile-var` does not read — see the module
            // comment. The `cellvar` split a captured name needs arrives with
            // `lambda`/`labels`, which is what creates a captured name in the
            // first place.
            f.push(Value::Bool(false));
            f.finish("var")
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
            let name_pair = s.cons(name, Value::Int(repr.binding_kind()))?;
            s.push_root(name_pair);
            let init = to_island(&mut s, init, cx)?;
            s.push_root(init);
            let pair = s.cons(name_pair, init)?;
            s.push_root(pair);
            pairs.push(pair);
        }
        core::list(&mut s, &pairs)?
    };

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

/// `(construct PATH N MUTABLE E...)` -> `(construct is-sexpr mutable
/// type-name-form variant field...)`.
///
/// The three-way choice the island makes off the header — build one of
/// `sexpr`'s own variants, build a mutable `BoxedObj::Struct`, or build a
/// `BoxedObj::Enum` — is decided here, from the type path and the `mutable`
/// flag the checker already resolved, which is what keeps `compiler.rs`
/// registry-free.
///
/// The fields' kinds come from the *type's* definition rather than from the
/// argument expressions, which is the whole reason [`Definitions`] exists. A
/// `sexpr` construct is the exception: its variants' shapes follow from the
/// variant number, so the island derives them and the fields go untagged.
fn translate_construct(heap: &mut Heap, form: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, form)?;
    if parts.len() < 3 {
        return Err(malformed(heap, form));
    }
    let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, form))?;
    let Value::Int(variant) = parts[1] else { return Err(malformed(heap, form)) };
    let Value::Bool(mutable) = parts[2] else { return Err(malformed(heap, form)) };
    let args = parts[3..].to_vec();
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
    let field_reprs = cx.defs.fields_of(&path, variant as usize).ok_or_else(|| {
        Error::TypeError(format!("compile: no definition recorded for the type `{}` (internal error)", path))
    })?;
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
    let kinds: Vec<Repr> = field_reprs.to_vec();
    let pairs = arg_pairs_with(f.heap(), &kinds, &args, Repr::field_kind, cx)?;
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

/// `(pat-ctor PATH N DOWNCAST P...)` -> `(pat-ctor variant (subpat...) kind
/// (field-kind...) downcast type-name-form)`.
fn translate_ctor_pattern(heap: &mut Heap, pat: Value, cx: Ctx) -> Result<Value, Error> {
    let parts = core::fields(heap, pat)?;
    if parts.len() < 3 {
        return Err(malformed(heap, pat));
    }
    let path = as_path(heap, parts[0]).ok_or_else(|| malformed(heap, pat))?;
    let Value::Int(variant) = parts[1] else { return Err(malformed(heap, pat)) };
    let Value::Bool(downcast) = parts[2] else { return Err(malformed(heap, pat)) };
    let subpats = parts[3..].to_vec();

    let kind = if path == Path::root("sexpr") {
        MATCH_KIND_SEXPR
    } else if cx.defs.is_struct(&path) {
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
        let reprs = cx.defs.fields_of(&path, variant as usize).ok_or_else(|| {
            Error::TypeError(format!("compile: no definition recorded for the type `{}` (internal error)", path))
        })?;
        if reprs.len() != subpats.len() {
            return Err(Error::TypeError(format!(
                "compile: `{}` variant {} has {} fields, matched with {} sub-patterns (internal error)",
                path,
                variant,
                reprs.len(),
                subpats.len()
            )));
        }
        reprs.iter().map(|r| Value::Int(r.field_kind())).collect()
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

    /// Translate one hand-written core form and print the island form.
    ///
    /// `gc_stress` is on throughout: the bridge conses for nearly every node,
    /// so every allocation collects, and an intermediate the builders failed
    /// to root would be reclaimed before the node holding it exists. That is
    /// the failure mode `ast_bridge` shipped twice.
    fn bridged(src: &str) -> String {
        bridged_with(&[], src)
    }

    /// Translate `src` with `defs` (each a `defstruct`/`defenum` core form)
    /// recorded first.
    fn bridged_with(defs: &[&str], src: &str) -> String {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        let r = Reader::new();
        let definitions = record_all(&mut h, &r, defs);
        let cx = Ctx { defs: &definitions };
        let mut vs = r.read_all(&mut h, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected one form in {:?}", src);
        let core_form = vs.pop().unwrap();
        h.push_root(core_form);
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
        let cx = Ctx { defs: &definitions };
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
            r#"(assoc "m::point" "area" true (0 var "p" false))"#
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

    /// A construct's field kinds come from the *type*, not from the argument
    /// expressions — which is why the bridge needs the definitions at all.
    #[test]
    fn a_construct_tags_its_fields_from_the_types_definition() {
        assert_eq!(
            bridged_with(&DEFS, "(construct point 0 true (int 1) (int 2))"),
            r#"(construct false true (str (int 112) (int 111) (int 105) (int 110) (int 116)) 0 (1 int 1) (1 int 2))"#
        );
        // An enum field of `sexpr` is already tagged, hence the passthrough
        // kind 6 — read off the variant's own field list.
        assert_eq!(
            bridged_with(&DEFS, "(construct option 1 false (var x))"),
            r#"(construct false false (str (int 111) (int 112) (int 116) (int 105) (int 111) (int 110)) 1 (6 var "x" false))"#
        );
        // `sexpr`'s own variants take untagged fields: their shapes follow
        // from the variant number, so the island derives them.
        assert_eq!(bridged_with(&DEFS, "(construct sexpr 7 false (int 1) (int 2))"), "(construct true false () 7 (int 1) (int 2))");
    }

    /// A construct whose arity disagrees with its definition is an internal
    /// error: the island would tag a field with another field's kind.
    #[test]
    fn a_construct_that_disagrees_with_its_definition_is_refused() {
        let e = refused_with(&DEFS, "(construct point 0 true (int 1))");
        assert!(e.contains("has 2 fields, constructed with 1"), "{}", e);
        let e = refused_with(&[], "(construct point 0 true (int 1) (int 2))");
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
            bridged_with(&DEFS, "(match (var v) enum ((pat-ctor option 0 false) (int 0)) ((pat-ctor option 1 false (pat-bind x)) (var x)))"),
            r#"(match false (var "v" false) (((pat-ctor 0 () 1 () false ()) int 0) ((pat-ctor 1 ((pat-bind "x")) 1 (6) false ()) var "x" false)) 1)"#
        );
        // A struct scrutinee: kind 2, and the field kinds come from the
        // `defstruct`.
        assert_eq!(
            bridged_with(&DEFS, "(match (var p) struct ((pat-ctor point 0 false (pat-bind a) (pat-wild)) (var a)))"),
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
        let plain = bridged_with(&DEFS, "(match (var v) sexpr ((pat-ctor point 0 false (pat-wild) (pat-wild)) (int 1)))");
        assert!(plain.contains("(pat-ctor 0 ((pat-wild) (pat-wild)) 2 (1 1) false ())"), "{}", plain);
        let down = bridged_with(&DEFS, "(match (var v) sexpr ((pat-ctor point 0 true (pat-wild) (pat-wild)) (int 1)))");
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

    /// A tag with no translation yet says which tag, rather than emitting
    /// something the island would misread.
    #[test]
    fn an_untranslated_tag_names_itself() {
        assert!(refused("(loop (break))").contains("`loop`"), "{}", refused("(loop (break))"));
        assert!(refused("(quote (a b))").contains("`quote`"));
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
