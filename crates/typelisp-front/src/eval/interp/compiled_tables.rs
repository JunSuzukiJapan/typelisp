//! The tables compiled code reads that the interpreter fills in: vtables,
//! trait ids, supertrait upcasts, and the type descriptors an executable is
//! built with.

use crate::{Path, SymRef};
use super::Interp;

impl Interp {
    /// Publish every interned vtable to the compiled tier, resolving each
    /// slot to its method's current native entry point. Called after each
    /// SCC finishes so a table whose targets have just been compiled becomes
    /// callable; re-publishing tables that were already complete is
    /// harmless, and is what keeps a vtable correct when one of its methods
    /// is compiled *later* than the function that boxes for it.
    ///
    /// A slot whose method has no compiled form yet is published as 0, which
    /// the driver reads as "this one is interpreted — ask the interpreter
    /// for its closure". That is not an error: a trait object's concrete type
    /// can be a user `defstruct` whose methods nobody ever compiled, while the
    /// code dispatching on it (a prelude stream method, say) is native.
    pub fn publish_vtables(&self) {
        for id in 0..self.vtables.borrow().len() {
            self.publish_vtable(id as u32);
        }
    }

    /// [`Self::publish_vtables`] for one table.
    pub(super) fn publish_vtable(&self, id: u32) {
        let slots = match self.vtables.borrow().get(id as usize) {
            Some(s) => s.clone(),
            None => return,
        };
        // The ABI travels with the address: which convention an entry point
        // answers to is a property of that entry point, and a table of bare
        // addresses cannot say.
        let addrs: Vec<(usize, u8)> = slots
            .iter()
            .map(|(type_name, method)| {
                self.root
                    .borrow()
                    .get_method(type_name, *method)
                    .and_then(|f| f.compiled.borrow().as_ref().map(|c| (c.address(), c.body_abi())))
                    .unwrap_or((0, typelisp_abi::BODY_ABI_CLASSIC))
            })
            .collect();
        typelisp_rt::vtable_define(id, addrs);
    }

    /// Every vtable interned so far, as `(id, slots)` — for a code generator
    /// that has to emit the table itself rather than patch it in from JIT
    /// addresses (`compile::aot::build_main_wrapper`).
    /// Every enum in the program as `(definition, variant index, variant
    /// name)` — what an AOT executable's startup registers so its printer can
    /// name a variant (`typelisp_print::aot`), since the box carries only the
    /// index.
    pub fn enum_variant_descriptors(&self) -> Vec<(Path, usize, String)> {
        let (_, enums) = self.root.borrow().collect_struct_and_enum_types();
        let mut out = Vec::new();
        for (path, def) in &enums {
            for (i, (name, _)) in def.variants.iter().enumerate() {
                out.push((path.clone(), i, name.clone()));
            }
        }
        // `collect_struct_and_enum_types` returns a `HashMap`, so sort for a
        // deterministic startup sequence — two runs of `compile-file` on the
        // same source must produce the same executable.
        out.sort_by_cached_key(|(path, i, _)| (path.to_string(), *i));
        out
    }

    /// Every struct's field names, as `(definition, index, name)` for
    /// `typelisp_print::aot::rt_print_field_name` — the executable's copy of
    /// what [`Self::field_name`] reads from the scope tree here. Sorted, for
    /// the same reason [`Self::enum_variant_descriptors`] is.
    pub fn field_name_descriptors(&self) -> Vec<(Path, i64, String)> {
        let mut raw = Vec::new();
        self.root.borrow().collect_field_names(&mut raw);
        let mut out: Vec<(Path, i64, String)> = raw.into_iter().map(|(path, index, name)| (path, index as i64, name)).collect();
        out.sort_by_cached_key(|(path, index, _)| (path.to_string(), *index));
        out
    }

    /// Every type's field templates, as `(definition, variant or NO_VARIANT,
    /// index or EVERY_FIELD, template)` for
    /// `typelisp_print::aot::rt_print_field_template` — the executable's copy
    /// of what [`Self::field_is_niched_option`] reads from the scope tree
    /// here. Sorted, for the same reason [`Self::enum_variant_descriptors`]
    /// is.
    pub fn field_template_descriptors(&self) -> Vec<(Path, i64, i64, crate::TypeId)> {
        let mut raw = Vec::new();
        self.root.borrow().collect_field_templates(&mut raw);
        let mut out: Vec<(Path, i64, i64, crate::TypeId)> = raw
            .into_iter()
            .map(|(path, variant, index, t)| {
                (
                    path,
                    variant.map_or(typelisp_print::aot::NO_VARIANT, |v| v as i64),
                    index.map_or(typelisp_print::aot::EVERY_FIELD, |i| i as i64),
                    t,
                )
            })
            .collect();
        out.sort_by_cached_key(|(path, variant, index, _)| (path.to_string(), *variant, *index));
        out
    }

    pub fn vtable_descriptors(&self) -> Vec<(u32, Vec<(Path, SymRef)>)> {
        self.vtables.borrow().iter().cloned().enumerate().map(|(i, s)| (i as u32, s)).collect()
    }

    /// The vtable id for boxing a `concrete`-typed value as
    /// `trait_path` — interning the (type, trait) pair so repeated boxing
    /// reuses one table. The sole source of vtable ids; the compiled tier's
    /// parallel table (`typelisp_rt`) is keyed by the same numbers.
    ///
    /// `slots` comes from the checker (`dyn-new`), already in
    /// `TraitDef::method_order` order and already monomorphized, so this only
    /// records it.
    pub(crate) fn vtable_id_for(&self, concrete: crate::TypeId, trait_path: &Path, slots: &[(Path, SymRef)]) -> u32 {
        let key = (concrete, trait_path.clone());
        if let Some(&id) = self.vtable_ids.borrow().get(&key) {
            return id;
        }
        let id = {
            let mut tables = self.vtables.borrow_mut();
            let id = tables.len() as u32;
            tables.push(slots.to_vec());
            id
        };
        self.vtable_ids.borrow_mut().insert(key, id);
        // Publish immediately, not only after the next `compile_scc`: with a
        // precompiled prelude the targets may already have native bodies
        // while nothing in this session ever compiles anything, and then the
        // compiled tier would never learn the table exists. A slot whose
        // method is *not* compiled publishes as 0, which the driver reads
        // as "ask the interpreter".
        self.publish_vtable(id);
        id
    }

    /// The id interning `trait_path` for upcast lookups — the trait half of
    /// [`Self::dyn_upcasts`]'s key, and the constant a compiled
    /// `dyn-upcast` bakes in. Ids are dense and per-`Interp`, like
    /// vtable ids; nothing outside this pair of tables reads them.
    pub fn trait_id_for(&self, trait_path: &Path) -> u32 {
        if let Some(&id) = self.trait_ids.borrow().get(trait_path) {
            return id;
        }
        let mut ids = self.trait_ids.borrow_mut();
        let id = ids.len() as u32;
        ids.insert(trait_path.clone(), id);
        id
    }

    /// Intern a boxing site's vtable *and* every supertrait table it carries
    /// (`dyn-new`'s `SUPERS`), recording how to get from any one of them to
    /// any other. Returns the ids, the boxed trait's own first.
    ///
    /// Every pair is registered, not just (sub -> super): the entries only
    /// ever say "the vtable *this same concrete type* uses for that trait",
    /// which is true regardless of the direction the two traits are related
    /// in, and a chain of upcasts can arrive at any of these tables before
    /// asking for the next. Admissibility is the checker's business — an
    /// entry nothing is allowed to ask for is simply never read.
    pub fn register_dyn_box(
        &self,
        concrete: crate::TypeId,
        trait_path: &Path,
        slots: &[(Path, SymRef)],
        supers: &[(Path, Vec<(Path, SymRef)>)],
    ) -> Vec<u32> {
        let id = self.vtable_id_for(concrete, trait_path, slots);
        if supers.is_empty() {
            return vec![id];
        }
        let mut tables: Vec<(u32, u32)> = vec![(id, self.trait_id_for(trait_path))];
        for (sup, sup_slots) in supers {
            tables.push((self.vtable_id_for(concrete, sup, sup_slots), self.trait_id_for(sup)));
        }
        for &(from, _) in &tables {
            for &(to, to_trait) in &tables {
                self.dyn_upcasts.borrow_mut().insert((from, to_trait), to);
                typelisp_rt::upcast_define(from, to_trait, to);
            }
        }
        tables.into_iter().map(|(id, _)| id).collect()
    }

    /// Every upcast registered so far, as `(source vtable, trait id, target
    /// vtable)` — [`Self::vtable_descriptors`]'s counterpart for the code
    /// generator that has to emit the table itself instead of filling it in
    /// from a running interpreter (`compile::aot::build_main_wrapper`).
    /// Sorted so a rebuild of the same program emits the same startup code.
    pub fn upcast_descriptors(&self) -> Vec<(u32, u32, u32)> {
        let mut out: Vec<(u32, u32, u32)> =
            self.dyn_upcasts.borrow().iter().map(|(&(from, t), &to)| (from, t, to)).collect();
        out.sort_unstable();
        out
    }
}
