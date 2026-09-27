//! `def-c-struct` and typed pointers: memory laid out the way C lays it out,
//! allocated inside `(unsafe ...)` and freed when that form is left.
//!
//! ```lisp
//! (unsafe (def-c-struct point (x i32) (y i32)))
//!
//! (unsafe
//!   (let ((ps (c-alloc point 3)))       ; (ptr point), zeroed
//!     (setf (c-ref ps 1)::x 5) ...))    ; not this: see `c-ref` below
//! ```
//!
//! # Who owns an allocation
//!
//! The outermost `(unsafe ...)` of a function body (or of a top-level form)
//! owns every `c-alloc` inside it. That form is lowered to
//!
//! ```text
//! (let ((" c-arena" (" c-arena-open")))
//!   (unwind-protect BODY (" c-arena-close" " c-arena")))
//! ```
//!
//! so leaving it by any path frees the memory. A `lambda` or `labels`
//! function inside starts over: it is a function of its own, may run after
//! the form it was written in is left, and cannot capture a typed pointer.
//!
//! # Keeping a typed pointer inside its owner
//!
//! A typed pointer is a raw word, so the rules for `ptr` already keep it out
//! of storage and type arguments and out of expressions outside `unsafe`.
//! What remains is every way a value can outlive the form it was made in, and
//! each is refused here: being the value of an `unsafe` form, being captured
//! by a closure, being passed to a `task`/`thread`, being thrown.
//!
//! # Pointers from C
//!
//! A typed pointer C hands over — a callback's argument, a `defffi` result,
//! a pointer field read back — is checked against the live allocations at
//! run time (`typelisp_rt::c_mem::check`): memory C allocated is refused.

use super::*;
use crate::c_struct as b;
use crate::check::registry::{CField, CStructDef};
use typelisp_rt::c_mem;

/// Whether `ty` is, or contains, a typed pointer.
pub(super) fn type_mentions_typed_ptr(ty: &Type) -> bool {
    match ty {
        Type::PtrTo(_) => true,
        Type::Named(_, args) | Type::Dyn(_, args) => args.iter().any(type_mentions_typed_ptr),
        Type::Fn(ps, rest, ret) => {
            ps.iter().any(type_mentions_typed_ptr)
                || rest.as_deref().map(type_mentions_typed_ptr).unwrap_or(false)
                || type_mentions_typed_ptr(ret)
        }
        _ => false,
    }
}

/// The load/store kind of a C scalar type, `ptr` or a typed pointer.
fn c_kind(ty: &Type) -> Option<i64> {
    Some(match ty {
        Type::I8 => c_mem::KIND_I8,
        Type::I16 => c_mem::KIND_I16,
        Type::I32 => c_mem::KIND_I32,
        Type::U8 => c_mem::KIND_U8,
        Type::U16 => c_mem::KIND_U16,
        Type::U32 => c_mem::KIND_U32,
        Type::CLong => c_mem::KIND_C_LONG,
        Type::CULong => c_mem::KIND_C_ULONG,
        Type::F32 => c_mem::KIND_F32,
        Type::F64 => c_mem::KIND_F64,
        Type::Bool => c_mem::KIND_BOOL,
        Type::Ptr | Type::PtrTo(_) => c_mem::KIND_PTR,
        _ => return None,
    })
}

/// What a typed pointer may point at, and a `def-c-struct` field may be,
/// spelled out for an error message.
const C_TYPES: &str = "i8, i16, i32, u8, u16, u32, c-long, c-ulong, f32, f64, bool, ptr, \
                       a typed pointer (ptr T), or a def-c-struct";

impl Checker {
    // ---- declarations ------------------------------------------------------

    /// A top-level `(unsafe ...)` whose every form is a `def-c-struct`
    /// (possibly `pub`): `Some(true)`. One that holds none: `Some(false)`, an
    /// ordinary expression. A mixture is refused — the declarations and the
    /// expression would be checked by two different rules.
    pub(super) fn unsafe_declares_c_structs(heap: &Heap, forms: &[Value]) -> Result<bool, Error> {
        let is_decl = |v: &Value| -> bool {
            let Ok(items) = heap.list_to_vec(*v) else { return false };
            let head = |i: usize| match items.get(i) {
                Some(Value::Symbol(id)) => Some(heap.symbol_name(*id)),
                _ => None,
            };
            head(0) == Some("def-c-struct") || (head(0) == Some("pub") && head(1) == Some("def-c-struct"))
        };
        let n = forms.iter().filter(|v| is_decl(v)).count();
        if n > 0 && n != forms.len() {
            return Err(Error::TypeError(
                "an `(unsafe ...)` that declares C structs holds nothing but `def-c-struct`s — \
                 write the expressions in an `(unsafe ...)` of their own"
                    .into(),
            ));
        }
        Ok(n > 0)
    }

    /// `(unsafe (def-c-struct ...)...)` at top level: registers each struct.
    /// Nothing runs, so the form is an empty `module` named after the last
    /// struct (like `deftype`'s).
    pub(super) fn check_c_struct_block(
        &mut self,
        heap: &mut Heap,
        forms: &[Value],
        forms_locs: &[Option<Loc>],
        def_loc: Option<Loc>,
    ) -> Result<TopLevelForm, Error> {
        let mut last = None;
        for (i, form) in forms.iter().enumerate() {
            let items = heap.list_to_vec_locs(*form)?;
            let (public, at) = match items.first() {
                Some((Value::Symbol(id), _)) if heap.symbol_name(*id) == "pub" => (true, 2),
                _ => (false, 1),
            };
            let parts: Vec<Value> = items[at..].iter().map(|(v, _)| *v).collect();
            let locs: Vec<Option<Loc>> = items[at..].iter().map(|(_, l)| l.clone()).collect();
            let loc = forms_locs.get(i).cloned().flatten().or_else(|| def_loc.clone());
            last = Some(self.check_def_c_struct(heap, &parts, &locs, public, loc)?);
        }
        let last = last.ok_or_else(|| Error::TypeError("internal error: an empty C struct block".into()))?;
        forms::module_form(heap, &last, &[])
    }

    /// `(def-c-struct name "doc"? (field type)...)`.
    fn check_def_c_struct(
        &mut self,
        heap: &mut Heap,
        parts: &[Value],
        parts_locs: &[Option<Loc>],
        public: bool,
        def_loc: Option<Loc>,
    ) -> Result<Path, Error> {
        let usage = "def-c-struct: (def-c-struct name (field type)...)";
        let name = match parts.first() {
            Some(Value::Symbol(id)) => heap.symbol_name(*id).to_string(),
            _ => return Err(Error::TypeError(usage.into())),
        };
        let (doc, fields_at) = match parts.get(1) {
            Some(Value::Str(id)) => (Some(heap.string(*id).to_string()), 2),
            _ => (None, 1),
        };
        if parts.len() <= fields_at {
            return Err(Error::TypeError(format!("def-c-struct {}: a C struct needs at least one field", name)));
        }
        let ns = self.cur_ns();
        if ns.types.contains_key(&name) || ns.type_aliases.contains_key(&name) {
            return Err(Error::TypeError(format!(
                "cannot define C struct `{}`: a type of that name already exists here",
                name
            )));
        }
        self.check_redef("C struct", &name, self.cur_ns().c_structs.get(&name))?;
        self.check_type_trait_clash("C struct", &name)?;
        let fq = self.fq(&name);
        // Registered with no fields while its own fields are read, so that a
        // field can point at the struct being defined (`(next (ptr node))`).
        let placeholder = CStructDef { name: fq.clone(), fields: Vec::new(), size: 0, align: 1, public };
        let previous = self.reg.root.module_mut(&self.ns).c_structs.insert(name.clone(), placeholder);
        let result = self.c_struct_fields(heap, &name, &fq, &parts[fields_at..], &parts_locs[fields_at.min(parts_locs.len())..]);
        let ns = self.reg.root.module_mut(&self.ns);
        let (fields, size, align) = match result {
            Ok(laid_out) => laid_out,
            Err(e) => {
                match previous {
                    Some(p) => ns.c_structs.insert(name, p),
                    None => ns.c_structs.remove(&name),
                };
                return Err(e);
            }
        };
        ns.c_structs.insert(name, CStructDef { name: fq.clone(), fields, size, align, public });
        if let Some(loc) = def_loc {
            self.reg.def_locs.types.insert(fq.clone(), loc);
        }
        if let Some(doc) = doc {
            self.reg.docs.types.insert(fq.clone(), doc);
        }
        Ok(fq)
    }

    /// The fields of `def-c-struct name`, laid out: C's rules — each field
    /// at the next multiple of its alignment, the struct as aligned as its
    /// most aligned field, its size rounded up to that.
    fn c_struct_fields(
        &self,
        heap: &Heap,
        name: &str,
        fq: &Path,
        specs: &[Value],
        locs: &[Option<Loc>],
    ) -> Result<(Vec<CField>, u64, u64), Error> {
        let mut fields: Vec<CField> = Vec::new();
        let (mut offset, mut align) = (0u64, 1u64);
        for (i, spec) in specs.iter().enumerate() {
            let items = heap.list_to_vec_locs(*spec).map_err(|_| {
                Error::TypeError(format!("def-c-struct {}: each field is (name type)", name))
            })?;
            let (Some((Value::Symbol(fid), _)), Some((tv, tloc)), 2) = (items.first(), items.get(1), items.len()) else {
                return Err(Error::TypeError(format!("def-c-struct {}: each field is (name type)", name)));
            };
            let fname = heap.symbol_name(*fid).to_string();
            if fields.iter().any(|f| f.name == fname) {
                return Err(Error::TypeError(format!("def-c-struct {}: field `{}` is declared twice", name, fname)));
            }
            let tloc = tloc.clone().or_else(|| locs.get(i).cloned().flatten());
            let ty = self.parse_c_field_type(heap, *tv, tloc.as_ref())?;
            if ty == Type::CStruct(fq.clone()) {
                return Err(Error::TypeError(format!(
                    "def-c-struct {}: field `{}` holds the struct itself, which would make it infinitely \
                     large — a field can point at it instead: (ptr {})",
                    name, fname, name
                )));
            }
            let (fsize, falign) = self.c_layout(&ty)?;
            offset = offset.div_ceil(falign) * falign;
            fields.push(CField { name: fname, ty, offset });
            offset += fsize;
            align = align.max(falign);
        }
        Ok((fields, offset.div_ceil(align) * align, align))
    }

    /// A field type: a C type, a typed pointer, or a `def-c-struct` by name
    /// (held by value).
    fn parse_c_field_type(&self, heap: &Heap, v: Value, loc: Option<&Loc>) -> Result<Type, Error> {
        let written = crate::types::parse_type_spanned(heap, v, loc, &mut Vec::new())?;
        if let Type::Named(n, args) = &written {
            if args.is_empty() {
                if let Some(def) = self.resolve_c_struct(n) {
                    return Ok(Type::CStruct(def.name));
                }
            }
        }
        let ty = self.canon(&written);
        self.reject_bad_pointee(&ty)?;
        if c_kind(&ty).is_none() {
            return Err(Error::TypeError(format!("a C struct field cannot be `{}` — it must be one of {}", ty, C_TYPES)));
        }
        Ok(ty)
    }

    /// The `def-c-struct` `path` names, resolved the way a type name is:
    /// `use` aliases, then the current module and its ancestors; a qualified
    /// name must be `pub` unless it is in scope.
    pub(super) fn resolve_c_struct(&self, path: &Path) -> Option<CStructDef> {
        if path.is_simple() {
            let name = path.last_segment();
            if let Some(target) = self.lookup_alias(name) {
                if let Some(d) = self.resolve_c_struct_segs(&target) {
                    return Some(d);
                }
            }
        }
        self.resolve_c_struct_segs(path.segments())
    }

    fn resolve_c_struct_segs(&self, segs: &[String]) -> Option<CStructDef> {
        let (mods, local) = segs.split_at(segs.len().checked_sub(1)?);
        if mods.is_empty() {
            return self
                .ns_ancestors()
                .into_iter()
                .find_map(|prefix| self.reg.root.module(prefix)?.c_structs.get(&local[0]).cloned());
        }
        let (abs, m) = self.find_module(mods)?;
        let d = m.c_structs.get(&local[0])?;
        if !d.public && !self.in_scope(&abs) {
            return None;
        }
        Some(d.clone())
    }

    /// The definition behind a resolved [`Type::CStruct`].
    fn c_struct_def(&self, fq: &Path) -> Result<CStructDef, Error> {
        let segs = fq.segments();
        let (mods, local) = segs.split_at(segs.len() - 1);
        self.reg
            .root
            .module(mods)
            .and_then(|m| m.c_structs.get(&local[0]))
            .cloned()
            .ok_or_else(|| Error::TypeError(format!("internal error: C struct `{}` is not registered", fq)))
    }

    /// `(sizeof, _Alignof)` of a C type on LP64.
    fn c_layout(&self, ty: &Type) -> Result<(u64, u64), Error> {
        Ok(match ty {
            Type::I8 | Type::U8 | Type::Bool => (1, 1),
            Type::I16 | Type::U16 => (2, 2),
            Type::I32 | Type::U32 | Type::F32 => (4, 4),
            Type::CLong | Type::CULong | Type::F64 | Type::Ptr | Type::PtrTo(_) => (8, 8),
            Type::CStruct(p) => {
                let d = self.c_struct_def(p)?;
                (d.size, d.align)
            }
            other => return Err(Error::TypeError(format!("`{}` is not a C type — it must be one of {}", other, C_TYPES))),
        })
    }

    /// Every `(offset, key)` a C object of type `ty` placed at `base` starts
    /// at: itself, and — for a struct — each field, recursively.
    fn c_positions(&self, ty: &Type, base: u64, out: &mut Vec<(u64, String)>) -> Result<(), Error> {
        out.push((base, crate::type_key::type_key_of_type(ty)));
        if let Type::CStruct(p) = ty {
            for f in self.c_struct_def(p)?.fields {
                self.c_positions(&f.ty, base + f.offset, out)?;
            }
        }
        Ok(())
    }

    /// The layout descriptor `c-alloc` hands the runtime for element type `ty`.
    fn c_desc(&self, ty: &Type) -> Result<String, Error> {
        let (size, align) = self.c_layout(ty)?;
        let mut positions = Vec::new();
        self.c_positions(ty, 0, &mut positions)?;
        let mut s = format!("{}{}{}", size, c_mem::DESC_FIELD, align);
        for (off, key) in positions {
            s.push(c_mem::DESC_FIELD);
            s.push_str(&format!("{}{}{}", off, c_mem::DESC_PAIR, key));
        }
        Ok(s)
    }

    // ---- types ---------------------------------------------------------------

    /// [`Self::canon`] for a typed pointer's pointee: a name that is a
    /// `def-c-struct` becomes [`Type::CStruct`]; anything else is canonical
    /// as an ordinary type is.
    pub(super) fn canon_pointee(&self, t: &Type) -> Type {
        if let Type::Named(n, args) = t {
            if args.is_empty() {
                if let Some(def) = self.resolve_c_struct(n) {
                    return Type::CStruct(def.name);
                }
            }
        }
        self.canon(t)
    }

    /// Refuses a typed pointer, anywhere in `ty`, to something that is not a
    /// C type.
    pub(super) fn reject_bad_pointee(&self, ty: &Type) -> Result<(), Error> {
        match ty {
            Type::PtrTo(inner) => match &**inner {
                Type::CStruct(_) => Ok(()),
                t if c_kind(t).is_some() => self.reject_bad_pointee(t),
                t => Err(Error::TypeError(format!(
                    "`(ptr {})`: a typed pointer points at {} — `{}` is none of these",
                    t, C_TYPES, t
                ))),
            },
            Type::Named(_, args) | Type::Dyn(_, args) => args.iter().try_for_each(|a| self.reject_bad_pointee(a)),
            Type::Fn(ps, rest, ret) => {
                ps.iter().try_for_each(|a| self.reject_bad_pointee(a))?;
                if let Some(r) = rest {
                    self.reject_bad_pointee(r)?;
                }
                self.reject_bad_pointee(ret)
            }
            _ => Ok(()),
        }
    }

    // ---- ownership -------------------------------------------------------------

    /// `(unsafe body...)`. See the module comment for what the owning form
    /// becomes.
    pub(super) fn check_unsafe(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let owner = self.c_arena.get().is_none();
        if owner {
            self.c_arena.set(Some(false));
        }
        self.unsafe_depth.set(self.unsafe_depth.get() + 1);
        let result = self.check_seq(heap, interp, env, args, arg_locs, expected);
        // Restored on the error path too: in recovery mode the checker keeps
        // going after this returns `Err`, and a depth left standing would make
        // every later form in the file unsafe.
        self.unsafe_depth.set(self.unsafe_depth.get() - 1);
        let allocates = owner && self.c_arena.get() == Some(true);
        if owner {
            self.c_arena.set(None);
        }
        let (body, ty) = result?;
        if type_mentions_typed_ptr(&ty) {
            return Err(Error::TypeError(format!(
                "an `(unsafe ...)` cannot answer `{}`: the memory a typed pointer points at is freed \
                 when the `unsafe` that allocated it is left. Copy what is needed into ordinary \
                 values (a `defstruct`, numbers) inside the `unsafe`, and answer those.",
                ty
            )));
        }
        let form = self.let_form(heap, &[], &body)?;
        if !allocates {
            return Ok(Checked::new(form, ty));
        }
        let form = forms::rooted(heap, form);
        let open = self.c_call(heap, b::ARENA_OPEN, &[])?;
        let arena_var = forms::var_form(heap, b::ARENA_VAR)?;
        let close = self.c_call(heap, b::ARENA_CLOSE, &[Checked::new(arena_var, Type::CULong)])?;
        let repr = self.repr_form(heap, &ty)?;
        let protected = forms::unwind_protect_form(heap, form, close, repr)?;
        let protected = forms::rooted(heap, protected);
        let wrapped = self.let_form(heap, &[(b::ARENA_VAR.to_string(), Type::CULong, open)], &[protected])?;
        Ok(Checked::new(wrapped, ty))
    }

    /// Runs `f` — the checking of a function body — with no owning `unsafe`
    /// in view: the body is a function of its own, and its allocations need
    /// an `unsafe` of its own.
    pub(super) fn with_own_arena<R>(&self, f: impl FnOnce() -> R) -> R {
        let saved = self.c_arena.replace(None);
        let r = f();
        self.c_arena.set(saved);
        r
    }

    /// Refuses a closure that captures a typed pointer: the closure can be
    /// called after the owning `unsafe` is left.
    pub(super) fn reject_captured_typed_ptrs(
        &self,
        heap: &Heap,
        env: &Env,
        body: &[Value],
        bound: &[String],
        what: &str,
    ) -> Result<(), Error> {
        let bound: HashSet<typelisp_mem::SymRef> = bound.iter().map(|n| typelisp_mem::symbols::intern(n)).collect();
        for sym in crate::check::freevars::free_vars(heap, body, &bound, &HashSet::new())? {
            let name = sym.name();
            if let Some(ty) = env.get(name) {
                if type_mentions_typed_ptr(ty) {
                    return Err(Error::TypeError(format!(
                        "{} captures `{}`, a `{}`: the closure can run after the `unsafe` that \
                         allocated the memory is left and has freed it. Pass it as an argument instead.",
                        what, name, ty
                    )));
                }
            }
        }
        Ok(())
    }

    /// Records `checked` as an expression whose value is a typed pointer, so
    /// that a `task`/`thread` can refuse one among its arguments.
    pub(super) fn note_typed_ptr_node(&self, checked: &Checked) {
        if type_mentions_typed_ptr(&checked.ty) {
            self.typed_ptr_nodes.borrow_mut().push(checked.form);
        }
    }

    /// Refuses a `task`/`thread` whose call passes a typed pointer: the task
    /// can outlive the `unsafe` that owns the memory.
    pub(super) fn reject_typed_ptr_task_args(&self, heap: &Heap, call: Value, form_name: &str) -> Result<(), Error> {
        let items = heap.list_to_vec(call)?;
        let nodes = self.typed_ptr_nodes.borrow();
        if items.iter().any(|v| nodes.contains(v)) {
            return Err(Error::TypeError(format!(
                "a typed pointer cannot be passed to `{}`: the task can run after the `unsafe` that \
                 allocated the memory is left and has freed it",
                form_name
            )));
        }
        Ok(())
    }

    // ---- expressions -------------------------------------------------------------

    /// A call to one of the internal builtins.
    fn c_call(&self, heap: &mut Heap, name: &str, args: &[Checked]) -> Result<Value, Error> {
        let r = Ref::synthetic(Path::root(name));
        let form = self.call_form(heap, &r, args)?;
        Ok(forms::rooted(heap, form))
    }

    /// A raw-word constant argument.
    fn c_word(&self, heap: &mut Heap, interp: &dyn MacroExpander, n: u64) -> Result<Checked, Error> {
        self.check(heap, interp, &Env::new(), Value::Int(n as i64), Some(&Type::I32))
    }

    fn c_text(&self, heap: &mut Heap, text: &str) -> Result<Checked, Error> {
        let form = forms::str_lit_form(heap, text)?;
        Ok(Checked::new(forms::rooted(heap, form), Type::Str))
    }

    /// `(" c-ptr-check" FORM KEY)` — `form`, a typed pointer to `pointee`
    /// that came from C, once it is known to point into live memory.
    pub(super) fn c_checked_from_c(&self, heap: &mut Heap, form: Value, pointee: &Type) -> Result<Value, Error> {
        let key = self.c_text(heap, &crate::type_key::type_key_of_type(pointee))?;
        self.c_call(heap, b::PTR_CHECK, &[Checked::new(form, Type::Ptr), key])
    }

    /// An `int` argument, as the raw word the builtin reads.
    fn c_int_arg(&self, heap: &mut Heap, interp: &dyn MacroExpander, env: &Env, v: Value, loc: Option<&Loc>) -> Result<Checked, Error> {
        let n = self.check_at(heap, interp, env, v, Some(&Type::Int), loc.cloned())?;
        let form = untag_int_form(heap, n.form)?;
        Ok(Checked::new(forms::rooted(heap, form), Type::I32))
    }

    /// A typed pointer argument.
    fn c_ptr_arg(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        v: Value,
        loc: Option<&Loc>,
        who: &str,
    ) -> Result<(Checked, Type), Error> {
        let p = self.check_at(heap, interp, env, v, None, loc.cloned())?;
        match &p.ty {
            Type::PtrTo(inner) => {
                let inner = (**inner).clone();
                Ok((p, inner))
            }
            other => Err(Error::TypeError(format!("{}: expected a typed pointer (ptr T), got `{}`", who, other))),
        }
    }

    /// `(c-alloc T)` / `(c-alloc T n)`: `n` zeroed `T`s (one by default),
    /// owned by the enclosing `unsafe`.
    pub(super) fn check_c_alloc(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.is_empty() || args.len() > 2 {
            return Err(Error::TypeError("c-alloc: (c-alloc Type) or (c-alloc Type count)".into()));
        }
        if self.c_arena.get().is_none() {
            return Err(Error::TypeError(
                "c-alloc: the memory belongs to the `(unsafe ...)` it is allocated in, and is freed when \
                 that is left — write it inside one, in the same function"
                    .into(),
            ));
        }
        let written = crate::types::parse_type(heap, args[0])?;
        let elem = self.canon_pointee(&written);
        let ptr_ty = Type::PtrTo(Box::new(elem.clone()));
        self.reject_bad_pointee(&ptr_ty)?;
        let count = match args.get(1) {
            Some(n) => self.c_int_arg(heap, interp, env, *n, nth_loc(arg_locs, 1).as_ref())?,
            None => self.c_word(heap, interp, 1)?,
        };
        let desc = self.c_desc(&elem)?;
        let desc = self.c_text(heap, &desc)?;
        let arena = Checked::new(forms::var_form(heap, b::ARENA_VAR)?, Type::CULong);
        self.c_arena.set(Some(true));
        let form = self.c_call(heap, b::ALLOC, &[arena, count, desc])?;
        Ok(Checked::new(form, ptr_ty))
    }

    /// `(c-ref p i)`: a pointer to the `i`th element from `p`, which must
    /// stay inside `p`'s allocation.
    pub(super) fn check_c_ref(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 2 {
            return Err(Error::TypeError("c-ref: (c-ref pointer index)".into()));
        }
        let (p, elem) = self.c_ptr_arg(heap, interp, env, args[0], nth_loc(arg_locs, 0).as_ref(), "c-ref")?;
        let ty = p.ty.clone();
        let i = self.c_int_arg(heap, interp, env, args[1], nth_loc(arg_locs, 1).as_ref())?;
        let (size, _) = self.c_layout(&elem)?;
        let size = self.c_word(heap, interp, size)?;
        let key = self.c_text(heap, &crate::type_key::type_key_of_type(&elem))?;
        let form = self.c_call(heap, b::INDEX, &[p, i, size, key])?;
        Ok(Checked::new(form, ty))
    }

    /// The value a load of `ty` at `addr + offset` answers.
    fn c_load(&self, heap: &mut Heap, interp: &dyn MacroExpander, addr: Checked, offset: u64, ty: &Type) -> Result<Checked, Error> {
        if let Type::CStruct(_) = ty {
            // A struct held by value: its address, not a load.
            let off = self.c_word(heap, interp, offset)?;
            let form = self.c_call(heap, b::OFFSET, &[addr, off])?;
            return Ok(Checked::new(form, Type::PtrTo(Box::new(ty.clone()))));
        }
        let kind = c_kind(ty).ok_or_else(|| Error::TypeError(format!("internal error: no load for `{}`", ty)))?;
        let off = self.c_word(heap, interp, offset)?;
        let kind_arg = self.c_word(heap, interp, kind as u64)?;
        let form = self.c_call(heap, b::LOAD, &[addr, off, kind_arg])?;
        let form = match ty {
            // A pointer read back from memory C may have written.
            Type::PtrTo(pointee) => self.c_checked_from_c(heap, form, pointee)?,
            _ => form,
        };
        Ok(Checked::new(form, ty.clone()))
    }

    /// Stores `value` (already checked against `ty`) at `addr + offset`, and
    /// answers it — `setf` answers what it stored.
    fn c_store(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        addr: Checked,
        offset: u64,
        ty: &Type,
        value: Checked,
    ) -> Result<Checked, Error> {
        let kind = c_kind(ty).ok_or_else(|| {
            Error::TypeError(format!("setf: a field holding a struct by value (`{}`) cannot be assigned as a whole — assign its fields", ty))
        })?;
        const TEMP: &str = " c-value";
        let temp = forms::var_form(heap, TEMP)?;
        let off = self.c_word(heap, interp, offset)?;
        let kind_arg = self.c_word(heap, interp, kind as u64)?;
        let store = self.c_call(heap, b::STORE, &[addr, off, kind_arg, Checked::new(temp, ty.clone())])?;
        let answer = forms::var_form(heap, TEMP)?;
        let form = self.let_form(heap, &[(TEMP.to_string(), ty.clone(), value.form)], &[store, answer])?;
        Ok(Checked::new(form, ty.clone()))
    }

    /// `(c-deref p)`: the scalar `p` points at.
    pub(super) fn check_c_deref(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        args: &[Value],
        arg_locs: &[Option<Loc>],
    ) -> Result<Checked, Error> {
        if args.len() != 1 {
            return Err(Error::TypeError("c-deref: (c-deref pointer)".into()));
        }
        let (p, pointee) = self.c_ptr_arg(heap, interp, env, args[0], nth_loc(arg_locs, 0).as_ref(), "c-deref")?;
        if let Type::CStruct(s) = &pointee {
            return Err(Error::TypeError(format!(
                "c-deref: `{}` is a C struct, which is not a value — read its fields: p::field",
                s
            )));
        }
        self.c_load(heap, interp, p, 0, &pointee)
    }

    /// `(setf (c-deref p) value)`.
    pub(super) fn check_c_deref_set(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        place_args: &[Value],
        value: Value,
    ) -> Result<Checked, Error> {
        if place_args.len() != 1 {
            return Err(Error::TypeError("setf: (setf (c-deref pointer) value)".into()));
        }
        let (p, pointee) = self.c_ptr_arg(heap, interp, env, place_args[0], None, "c-deref")?;
        let v = self.check_at(heap, interp, env, value, Some(&pointee), None)?;
        self.c_store(heap, interp, p, 0, &pointee, v)
    }

    /// The field `field` of the C struct `recv` points at, if `recv` is a
    /// pointer to one — `recv::field`.
    pub(super) fn c_field_get(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        recv: Checked,
        field: &str,
    ) -> Option<Result<Checked, Error>> {
        let Type::PtrTo(inner) = &recv.ty else { return None };
        let Type::CStruct(s) = &**inner else { return None };
        let s = s.clone();
        Some(self.c_field(&s, field).and_then(|f| self.c_load(heap, interp, recv, f.offset, &f.ty)))
    }

    /// `(setf recv::field value)` for a pointer to a C struct, if `recv` is
    /// one.
    pub(super) fn c_field_set(
        &self,
        heap: &mut Heap,
        interp: &dyn MacroExpander,
        env: &Env,
        recv: Checked,
        field: &str,
        value: Value,
        value_loc: Option<&Loc>,
    ) -> Option<Result<Checked, Error>> {
        let Type::PtrTo(inner) = &recv.ty else { return None };
        let Type::CStruct(s) = &**inner else { return None };
        let s = s.clone();
        Some((|| {
            let f = self.c_field(&s, field)?;
            let v = self.check_at(heap, interp, env, value, Some(&f.ty), value_loc.cloned())?;
            self.c_store(heap, interp, recv, f.offset, &f.ty, v)
        })())
    }

    fn c_field(&self, s: &Path, field: &str) -> Result<CField, Error> {
        self.c_struct_def(s)?
            .fields
            .into_iter()
            .find(|f| f.name == field)
            .ok_or_else(|| Error::TypeError(format!("C struct `{}` has no field `{}`", s, field)))
    }
}
