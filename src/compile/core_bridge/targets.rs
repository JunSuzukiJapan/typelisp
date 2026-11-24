//! What a body refers to that has to exist before it is compiled: its call
//! targets, globals and the trait-object boxes it builds.

use typelisp_mem::{wk, Error, Heap, SymRef, Value};
use crate::check::core;
use crate::types::Path;

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
    pub methods: Vec<(Path, SymRef)>,
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
    pub concrete: crate::TypeId,
    pub trait_path: Path,
    pub slots: Vec<(Path, SymRef)>,
    pub supers: Vec<(Path, Vec<(Path, SymRef)>)>,
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
    typelisp_mem::grow_stack(|| collect_into_here(heap, form, t))
}

/// [`collect_into`] on the stack it is given.
fn collect_into_here(heap: &Heap, form: Value, t: &mut Targets) -> Result<(), Error> {
    if !matches!(form, Value::Cons(_)) {
        return Ok(());
    }
    let Some(tag) = core::op_wk(heap, form) else {
        // A cons whose head is not a symbol: not a node but a *list* of them —
        // a `match` arm (`(PATTERN BODY...)`), a `labels` definition. Walk its
        // elements rather than treating it as a form with fields, which would
        // skip everything past the head.
        for e in heap.list_to_vec(form)? {
            collect_into(heap, e, t)?;
        }
        return Ok(());
    };
    match tag {
        // The one fence in the whole walk. User data appears nowhere else, so
        // this is the only place a cons can be something other than a node —
        // and a quoted `(call ...)` datum is data, not a dependency.
        wk::QUOTE => return Ok(()),
        // `(call PATH REPR (REPR...) ARG...)`, and `fnref`'s first field is the
        // same path — the bridge turns a `fnref` into a forwarding call, so
        // its target needs the same treatment.
        wk::CALL | wk::FNREF => {
            if let Some(p) = core::definition_field(heap, form, 0) {
                push_unique(&mut t.calls, p);
            }
        }
        // `(assoc PATH SYM ...)` / `(methodref PATH SYM ...)`.
        wk::ASSOC | wk::METHODREF => {
            if let (Some(p), Some(Value::Symbol(m))) = (core::definition_field(heap, form, 0), core::field(heap, form, 1)) {
                push_unique(&mut t.methods, (p, m));
            }
        }
        // `(global PATH REPR)` and `set-global`'s same leading path.
        wk::GLOBAL | wk::SET_GLOBAL => {
            if let Some(p) = core::definition_field(heap, form, 0) {
                push_unique(&mut t.globals, p);
            }
        }
        // Boxing a trait object is what pulls its whole vtable into the call
        // graph: the *call* through it has no statically known target, but
        // every target it could reach is right here in the slot list.
        wk::DYN_NEW => {
            let Some(concrete) = core::field(heap, form, 0).and_then(|k| typelisp_mem::type_of_form(heap, k)) else { return Ok(()) };
            let Some(trait_path) = core::definition_field(heap, form, 1) else { return Ok(()) };
            let slots = vtable_at(heap, form, 2)?;
            let supers = supers_at(heap, form, 3)?;
            for key in &slots {
                push_unique(&mut t.methods, *key);
            }
            if !t.dyn_boxes.iter().any(|s| s.concrete == concrete && s.trait_path == trait_path) {
                t.dyn_boxes.push(DynBoxSite { concrete, trait_path, slots, supers });
            }
        }
        // The conversion calls nothing — it swaps the box's table for one the
        // *boxing* site already registered. Only the target trait's id has to
        // be interned before translation.
        wk::DYN_UPCAST => {
            if let Some(p) = core::definition_field(heap, form, 0) {
                push_unique(&mut t.dyn_upcasts, p);
            }
        }
        // `(dyn-call TRAIT SYM SLOT ((TYPE SYM)...) REPR (REPR...) ARG...)` — no
        // static target, but every implementation it could dispatch to must
        // exist natively before this body can run natively.
        wk::DYN_CALL => {
            if let Some(p) = core::definition_field(heap, form, 0) {
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

/// Field `i` as a vtable `((TYPE SYM)...)`.
fn vtable_at(heap: &Heap, form: Value, i: usize) -> Result<Vec<(Path, SymRef)>, Error> {
    let Some(list) = core::field(heap, form, i) else { return Ok(Vec::new()) };
    vtable_of(heap, list)
}

/// A vtable slot list `((TYPE SYM)...)`, read from the list itself.
///
/// Separate from [`vtable_at`] because a supertrait entry is a *bare* list with
/// no tag, and [`core::field`] counts past a tag — so an index into one is off
/// by one. Taking the list directly removes the arithmetic instead of getting it
/// right twice. (Getting it wrong here emitted an empty supertrait vtable, and
/// the only symptom was an AOT executable aborting on the empty slot — the
/// abort `rt_vtable_slot` used to raise, now the driver's no-interpreter
/// case.)
fn vtable_of(heap: &Heap, list: Value) -> Result<Vec<(Path, SymRef)>, Error> {
    let mut out = Vec::new();
    for entry in heap.list_to_vec(list)? {
        let parts = heap.list_to_vec(entry)?;
        if let [ty, m] = parts.as_slice() {
            let Some(path) = core::definition_value(*ty) else { continue };
            if let Value::Symbol(id) = m {
                out.push((path, *id));
            }
        }
    }
    Ok(out)
}

/// Field `i` as a supertrait table list `((TRAIT ((TYPE SYM)...))...)`.
#[allow(clippy::type_complexity)]
fn supers_at(heap: &Heap, form: Value, i: usize) -> Result<Vec<(Path, Vec<(Path, SymRef)>)>, Error> {
    let Some(list) = core::field(heap, form, i) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for entry in heap.list_to_vec(list)? {
        let parts = heap.list_to_vec(entry)?;
        if let [ty, slots] = parts.as_slice() {
            let Some(path) = core::definition_value(*ty) else { continue };
            // The already-destructured element, not `core::field(entry, 1)` —
            // see [`vtable_of`].
            out.push((path, vtable_of(heap, *slots)?));
        }
    }
    Ok(out)
}
