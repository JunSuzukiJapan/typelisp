//! The type information layer: what the names a form is checked against
//! mean — the registry of definitions, the module the checker is in, and the
//! lexical scopes (local variables, type variables, local macros). The
//! analysis writes it; checking and lowering only read it
//! (`docs/dev/explicit-symbols-plan.md`, 3.9 and 3.10).


use typelisp_mem::NsId;

use super::registry::Registry;


/// See the module documentation.
pub(super) struct Info {
    pub(super) reg: Registry,
    /// The module forms are checked in now.
    pub(super) ns: NsId,
    /// Stack of enclosing *files'* own modules, innermost last — set by
    /// [`Checker::enter_file_module`]/[`Checker::exit_file_module`] (`project.rs`'s
    /// file-level wrapping only, never [`Checker::enter_module`]'s nested
    /// `(module ...)` forms), so it stays fixed at "the loading file's own
    /// module" even while `self.ns` moves into a nested `module` block inside
    /// that file. `Checker::use_target`'s sibling-file
    /// resolution reads the top of this stack (never `self.ns` itself)
    /// so `(use foo)` inside a nested `module` block still resolves `foo`
    /// relative to the *file's* directory, not that inner block's own
    /// namespace (which has no filesystem counterpart to be a sibling of).
    /// A recursively-loaded dependency pushes and fully pops its own frame
    /// before the file that `use`s it resumes checking (`project.rs`'s
    /// scan-before-check ordering), so this never needs more than simple
    /// stack discipline.
    pub(super) file_ns: Vec<NsId>,
    /// The modules to return to on [`Checker::exit_file_module`], innermost
    /// last: `(in-module ...)` moves the current module for the rest of the
    /// enclosing unit with no closing form, so leaving the unit restores the
    /// module it was entered from.
    pub(super) ns_base: Vec<NsId>,
}

impl Info {
    pub(super) fn new() -> Info {
        Info {
            reg: Registry::with_builtins(),
            ns: NsId::ROOT,
            file_ns: Vec::new(),
            ns_base: Vec::new(),
        }
    }

}

