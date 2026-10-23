//! The `PrintShared` `Arc` handle Phase 1d of `docs/dev/os-threads-design.md`
//! promotes `aot`'s `ENUM_NAMES` table into — see `typelisp_rt::shared`'s
//! module doc comment for the same story on the runtime crate's side; this
//! is its `typelisp-print` counterpart (§5's classification table splits
//! the thread-local tables across the two crates they already lived in).
//!
//! Every table an AOT executable's startup sequence fills is here — the
//! enum variant names, the field templates, and the `print-object`/`~/name/`
//! method addresses — so a worker thread given main's handle
//! ([`set_print_shared`]) prints exactly what main would (Phase 3).
//! `PRINTING` is not: it is per-*call* re-entry-guard state for whichever
//! `print-object` is running on this thread right now, not runtime-entity
//! data, so it never moves at all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// Everything `aot.rs` registers at startup — see the module doc comment
/// for what stays behind and why.
#[derive(Default)]
pub struct PrintShared {
    /// `(definition, variant index) -> variant name`.
    pub(crate) enum_names: RwLock<crate::aot::VariantNames>,
    /// `(definition, variant index or NO_VARIANT, field index or
    /// EVERY_FIELD) -> the field's declared type`, filled by
    /// `rt_print_field_template` — what says a struct or enum field holds a
    /// niche-represented `Option` (`PrintEnv::field_is_niched_option`).
    pub(crate) field_templates: RwLock<crate::aot::FieldTemplates>,
    /// `(definition, field index) -> field name`, filled by
    /// `rt_print_field_name` — what a struct prints before each field's
    /// value (`PrintEnv::field_name`).
    pub(crate) field_names: RwLock<crate::aot::FieldNames>,
    /// `type display -> the address of that type's compiled `print-object``,
    /// filled by `rt_print_object_method`.
    pub(crate) print_object: RwLock<HashMap<String, usize>>,
    /// `definition -> the address of its compiled body`, filled by
    /// `rt_format_call_method` — what `~/name/` calls through. The name is the
    /// one the checker resolved a directive to (`CompileTarget::link_name`),
    /// and only the definitions some directive in this program resolved to
    /// are registered.
    pub(crate) format_call: RwLock<HashMap<String, usize>>,
    /// `printer control variable -> its compiled global id`, filled by
    /// `rt_print_global` — where the AOT hooks read `*print-base*` and the
    /// rest from.
    pub(crate) globals: RwLock<HashMap<String, usize>>,
    /// Turns a compiled global id into its permanent-root position. The
    /// runtime owns that table and installs this before the program runs
    /// (`typelisp_print::aot::set_global_slots`); this crate sits below it.
    pub(crate) global_slot: RwLock<Option<fn(usize) -> Option<usize>>>,
    /// The module whose reader type names are written for, and what it has
    /// brought in — CL's `*package*` as the printer sees it. The checker
    /// sets it as it moves between modules; an AOT executable's startup
    /// registers its entry module's (`rt_print_name_module`,
    /// `rt_print_name_import`). Shared with the workers like the rest, so a
    /// task prints names the way the thread that started it does.
    pub(crate) name_view: RwLock<Arc<typelisp_mem::NameView>>,
}

/// How this thread writes paths now — see [`PrintShared::name_view`].
pub fn name_view() -> Arc<typelisp_mem::NameView> {
    Arc::clone(&print_shared().name_view.read())
}

/// Makes `view` how this thread (and the workers sharing its tables) write
/// paths.
pub fn set_name_view(view: typelisp_mem::NameView) {
    let shared = print_shared();
    let mut slot = shared.name_view.write();
    if **slot != view {
        *slot = Arc::new(view);
    }
}

thread_local! {
    /// This thread's `PrintShared` handle — `None` until [`print_shared`]
    /// first creates one.
    static PRINT_SHARED: RefCell<Option<Arc<PrintShared>>> = const { RefCell::new(None) };
}

/// This thread's `PrintShared`, creating one the first time it is asked
/// for — the same "auto-initializes on first use" behavior the bare
/// `thread_local!` it replaces already had.
pub fn print_shared() -> Arc<PrintShared> {
    PRINT_SHARED.with(|cell| cell.borrow_mut().get_or_insert_with(|| Arc::new(PrintShared::default())).clone())
}

/// Makes this thread use `shared` — another thread's [`print_shared`] — so
/// the two print the same enum names and dispatch to the same methods. What
/// a worker thread calls before it runs anything, beside
/// `typelisp_rt::shared::set_rt_shared`.
pub fn set_print_shared(shared: Arc<PrintShared>) {
    PRINT_SHARED.with(|cell| *cell.borrow_mut() = Some(shared));
}
