//! Which builtins make values of the types the runtime itself makes, so that
//! an executable registers what the printer needs about such a type only
//! when the code it can run calls one of them (`super::live_types`).
//!
//! A value the runtime makes with no compiled code naming its type is
//! invisible to the walk over type tables (`super::reach::TypeReach`). For
//! the containers (`Option`, `Result`, `Vector`, ...) the runtime makes them
//! everywhere, and they count as always made (`typelisp_mem::runtime_builds`).
//! The types in [`TRACKED`] are made in a few places only, each a builtin
//! listed in [`makers`]: those count as made when the executable reaches the
//! builtin's shim.
//!
//! The list was read off the runtime's sources with the language server
//! (who refers to each type's constant, and who calls those, up to the
//! shims), completed by hand where it cannot see (a call a macro writes).
//! `tests/type_identity_guard_test.rs` holds the places that make these
//! values to the ones it was read off, so a new one is not missed.

use typelisp_mem::{wk, StdModule, TypeId, TypeShape, TypeSpec};

/// The definitions whose values the runtime makes only through the builtins
/// [`makers`] lists. `ParseIntError` is among them with no builtin: the
/// prelude makes it, and compiled code reads its type to.
pub(super) const TRACKED: &[(StdModule, &str)] = &[
    (StdModule::FS, "FileError"),
    (StdModule::NET, "NetError"),
    (StdModule::READER, "ReadError"),
    (StdModule::NUM, "ParseFloatError"),
    (StdModule::NUM, "ParseIntError"),
    (StdModule::TIME, "universal-time"),
    (StdModule::TIME, "internal-time"),
    (StdModule::REPL, "heap-info"),
];

/// The builtins that make a value outside their result-type tables, with the
/// type they make: `sys_builtin`'s `universal_time_value`,
/// `internal_time_value` and `heap_info`, and the reader's two shims, which
/// make a `ReadError` on a failed read.
const DIRECT: &[(u32, TypeSpec)] = &[
    (wk::F_GET_UNIVERSAL_TIME, TypeSpec::Named(StdModule::TIME, "universal-time", &[])),
    (wk::F_GET_INTERNAL_REAL_TIME, TypeSpec::Named(StdModule::TIME, "internal-time", &[])),
    (wk::F_GET_INTERNAL_RUN_TIME, TypeSpec::Named(StdModule::TIME, "internal-time", &[])),
    (wk::F_HEAP_INFO, TypeSpec::Named(StdModule::REPL, "heap-info", &[])),
    (wk::F_READ, typelisp_read::shim::READ_RESULT_TYPE),
    (wk::F_READ_DATUM_AT, typelisp_read::shim::READ_DATUM_RESULT_TYPE),
];

/// Every builtin that can make a value of a [`TRACKED`] type, with a type
/// its value is made of. The stream, socket and system builtins make what
/// their result-type tables say and nothing else: each table's lookup stops
/// the process for a builtin with no row (`stream_builtin::lookup`), and the
/// one value they make outside the `Result` (`file-write-date`'s
/// `universal-time`) is in its row.
fn makers() -> impl Iterator<Item = &'static (u32, TypeSpec)> {
    use typelisp_rt::{net_builtin, stream_builtin, sys_builtin};
    stream_builtin::RESULT_TYPES
        .iter()
        .chain(stream_builtin::INNER_TYPES)
        .chain(net_builtin::RESULT_TYPES)
        .chain(net_builtin::INNER_TYPES)
        .chain(sys_builtin::RESULT_TYPES)
        .chain(DIRECT)
}

/// `ty`'s definition, when it is a named type.
fn definition(ty: TypeId) -> Option<(typelisp_mem::NsId, &'static str)> {
    match typelisp_mem::type_shape(ty) {
        TypeShape::Named(ns, sym, _) => Some((ns, sym.name())),
        _ => None,
    }
}

/// Whether `ty` is of a [`TRACKED`] definition.
pub(super) fn tracked(ty: TypeId) -> bool {
    definition(ty).is_some_and(|(ns, name)| TRACKED.iter().any(|(m, n)| m.ns() == ns && *n == name))
}

/// Whether `spec` names the definition `(ns, name)`, itself or in an
/// argument.
fn names(spec: &TypeSpec, ns: typelisp_mem::NsId, name: &str) -> bool {
    match spec {
        TypeSpec::Named(m, n, args) => (m.ns() == ns && *n == name) || args.iter().any(|a| names(a, ns, name)),
        TypeSpec::Fn(params, ret) => params.iter().any(|p| names(p, ns, name)) || names(ret, ns, name),
        TypeSpec::Unit => false,
    }
}

/// Whether a builtin the executable reaches (`reached`, asked of a shim's
/// symbol) can make a value of the tracked type `ty`.
pub(super) fn made(ty: TypeId, reached: impl Fn(&str) -> bool) -> Result<bool, String> {
    let Some((ns, name)) = definition(ty) else { return Ok(false) };
    for (builtin, spec) in makers() {
        if !names(spec, ns, name) {
            continue;
        }
        let symbol = typelisp_mem::symbols::well_known_symbol(*builtin);
        let shim = crate::compile::externs::rt_builtin_symbol(symbol)
            .ok_or_else(|| format!("internal error: the builtin `{}` makes a `{}` and has no shim", symbol.name(), name))?;
        if reached(shim) {
            return Ok(true);
        }
    }
    Ok(false)
}
