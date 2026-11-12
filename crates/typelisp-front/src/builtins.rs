//! The built-in definitions — the types, functions and methods implemented
//! in Rust — for the layers outside the checker that have to agree with
//! them: the compiler's shims and its ABI description, and the tests that
//! hold the runtime to the signatures.

use crate::typeck::registry::Registry;
use typelisp_mem::wk;
use crate::types::Path;
use typelisp_mem::SymRef;

pub use crate::typeck::registry::{AdtKind, Field, FnDef, MethodDef, TypeDef, Variant};

/// The built-in definitions, written into a registry of their own.
pub struct Builtins {
    reg: Registry,
}

impl Builtins {
    pub fn new() -> Builtins {
        let mut reg = Registry::default();
        crate::typeck::builtins::define(&mut reg);
        Builtins { reg }
    }

    /// Every built-in free function, by its path, in the order of the
    /// paths' spellings.
    pub fn functions(&self) -> Vec<(Path, &FnDef)> {
        let mut out: Vec<(Path, &FnDef)> = self.reg.values.iter().filter_map(|(p, _)| Some((*p, self.reg.fn_def(p)?))).filter(|(_, d)| d.builtin).collect();
        out.sort_by_key(|(p, _)| p.to_string());
        out
    }

    /// The built-in free function `path`.
    pub fn function(&self, path: &Path) -> Option<&FnDef> {
        self.reg.fn_def(path).filter(|d| d.builtin)
    }

    /// Every built-in type, by its path, in the order of the paths'
    /// spellings.
    pub fn types(&self) -> Vec<(Path, &TypeDef)> {
        let mut out: Vec<(Path, &TypeDef)> = self.reg.types.iter().filter_map(|(p, _)| Some((*p, self.reg.type_def(p)?))).filter(|(_, d)| d.builtin).collect();
        out.sort_by_key(|(p, _)| p.to_string());
        out
    }

    /// The built-in type `path`.
    pub fn type_def(&self, path: &Path) -> Option<&TypeDef> {
        self.reg.type_def(path).filter(|d| d.builtin)
    }

    /// The methods of the type `path`, by their member symbols, in the order
    /// of their names. A method's type parameters are the type's, then its
    /// own.
    pub fn methods(&self, path: &Path) -> Vec<(SymRef, &MethodDef)> {
        let mut out: Vec<(SymRef, &MethodDef)> = self.reg.methods_of(*path).collect();
        out.sort_by_key(|(m, _)| m.name());
        out
    }
}

impl Default for Builtins {
    fn default() -> Self {
        Builtins::new()
    }
}

/// The builtins the checker lowers a form to without giving them a
/// signature — `format`, `print` and the pretty printer's directives, C
/// memory ([`crate::c_struct`]), a C callback's address
/// ([`crate::ffi_callback`]): they live in `std::%internal`, in no table, so
/// no source reaches one by its name.
pub fn lowered_builtins() -> impl Iterator<Item = u32> {
    [wk::F_FORMAT_RT, wk::F_PRINT_RT, wk::F_PRINTLN_RT, wk::F_PPRINT_RT, wk::F_PPRINT_BLOCK_START_RT, wk::F_PPRINT_BLOCK_END_RT]
        .into_iter()
        .chain(crate::c_struct::BUILTINS.iter().map(|(name, _)| *name))
        .chain(std::iter::once(crate::ffi_callback::ADDRESS_BUILTIN))
}
