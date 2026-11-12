//! The standard library's definitions, made what every module reads.
//!
//! `std` imports every definition of the library's modules, and
//! `std::prelude` re-exports what [`prelude_offers`] offers, with the
//! library's types its functions and variables mention: a function a module
//! may call bare has a type the module may write. Each is keyed by the name
//! read in that module, as a `(use PATH)` written there would key it. Done
//! after every form the library defines something in.

use crate::types::{prelude_ns, std_module, Path, Type};
use typelisp_mem::symbols::{intern_in, Import, NsId, SessionNames};

use super::registry::{Registry, Since, ValueDef};

/// The modules inside `std` that, with `std` itself, make up the library
/// every module can draw on: `std` imports each of their definitions, no two
/// of them define one name, and `std::prelude` re-exports from them what
/// [`prelude_offers`] says ([`reexport`]).
pub(crate) const PRELUDE_MODULES: &[&str] = &["random", "time", "repl", "env", "net", "thread", "sync", "symbol", "process", "hash", "error", "cmp", "ops", "reader", "eval", "num", "char", "string", "control", "iter", "seq", "fmt", "path", "io", "fs", "collections", "array", "option", "result", "vec", "sexpr", "primitive"];

/// The modules of [`PRELUDE_MODULES`] `std::prelude` offers whole: the
/// primitive types, `Option`, `Result`, `Vector`, iteration, S-expressions
/// and lists, the sequence functions, the control macros, comparison and the
/// arithmetic traits.
const PRELUDE_WHOLE_MODULES: &[&str] = &["primitive", "option", "result", "vec", "iter", "sexpr", "seq", "control", "cmp", "ops"];

/// Definitions `std::prelude` offers by name, as `(module, name)`, from
/// modules it does not offer whole: the `Error` trait and `assert`,
/// `print-object`, and the types that stand for a Common Lisp standard type
/// under another spelling (`hash-table`, `array`, `bit-vector`, `stream`,
/// and the input and output streams `input-stream-p`/`output-stream-p`
/// tell apart), whose operations are their methods rather than functions
/// that would bring the type in, with `Hash`, which a key of a `HashTable`
/// implements ([`prelude_offers`]).
const PRELUDE_NAMED: &[(&str, &str)] = &[
    ("error", "Error"),
    ("error", "assert"),
    ("fmt", "print-object"),
    ("collections", "HashTable"),
    ("hash", "Hash"),
    ("array", "Array"),
    ("array", "BitVector"),
    ("io", "Stream"),
    ("io", "InputStream"),
    ("io", "OutputStream"),
];

/// Whether `std::prelude` offers every module the library's definition
/// `path`, before the types its functions and variables mention are added
/// ([`reexport`]): a definition of `std` itself or
/// of a module of [`PRELUDE_WHOLE_MODULES`], one of [`PRELUDE_NAMED`], or one
/// whose name is Common Lisp's ([`crate::cl_symbols`]) — what a Common Lisp
/// program uses without importing anything. The rest of the library is
/// reached with a `use` or a path.
pub(crate) fn prelude_offers(path: &Path) -> bool {
    let ns = path.ns();
    let name = path.name_symbol().name();
    let module_is = |seg: &str| crate::types::std_module(seg) == ns;
    ns == NsId::STD
        || PRELUDE_WHOLE_MODULES.iter().any(|seg| module_is(seg))
        || PRELUDE_NAMED.iter().any(|&(seg, n)| n == name && module_is(seg))
        || crate::cl_symbols::is_common_lisp_symbol(name)
}

/// Bring the library's definitions into `std` and `std::prelude`.
pub(crate) fn reexport(reg: &mut Registry) {
    let library = |m: NsId| m == NsId::STD || PRELUDE_MODULES.iter().any(|seg| std_module(seg) == m);
    let mut defined: Vec<Path> = reg.values.iter().map(|(p, _)| *p).chain(reg.types.iter().map(|(p, _)| *p)).filter(|p| library(p.ns())).collect();
    defined.extend(reg.ctors.iter().filter(|((m, _), _)| library(*m)).map(|((m, s), _)| Path::of_symbol(*m, *s)));
    let mut offered: Vec<Path> = defined.iter().copied().filter(prelude_offers).collect();
    let mut mentioned = Vec::new();
    for path in &offered {
        match reg.values.get(path) {
            Some(ValueDef::Fn(def)) => {
                for t in def.params.iter().chain(def.rest.as_ref()).chain(std::iter::once(&def.ret)) {
                    mentioned_types(t, &mut mentioned);
                }
            }
            Some(ValueDef::Var(var)) => mentioned_types(&var.ty, &mut mentioned),
            _ => {}
        }
    }
    offered.extend(mentioned.into_iter().filter(|p| defined.contains(p)));
    let prelude = prelude_ns();
    for path in offered {
        let key = (prelude, intern_in(prelude, path.name_symbol().name()));
        if reg.imports.get(&key) != Some(&path) {
            reg.imports.insert(key, path);
        }
        if reg.reexports.get(&key).is_none() {
            reg.reexports.insert(key, ());
        }
    }
    for path in defined.into_iter().filter(|p| p.ns() != NsId::STD) {
        let key = (NsId::STD, intern_in(NsId::STD, path.name_symbol().name()));
        if reg.imports.get(&key) != Some(&path) {
            reg.imports.insert(key, path);
        }
    }
}

/// Tell the session's names the imports and re-exports written since
/// `since`: each import as what it brings in, and each re-export as offered
/// to every module.
pub(crate) fn publish(reg: &Registry, names: &mut SessionNames, since: Since) {
    for key in reg.imports.keys(since) {
        let Some(target) = reg.imports.get(&key) else { continue };
        let what = match reg.type_def(target) {
            Some(_) => Import::Type(target.name_symbol()),
            None => Import::Item(target.name_symbol()),
        };
        names.import_name(key.0, key.1, what);
    }
    for key in reg.reexports.keys(since) {
        if let Some(target) = reg.imports.get(&key) {
            names.export(target.name_symbol());
        }
    }
}

/// The named types and traits `t` mentions, its arguments' included.
fn mentioned_types(t: &Type, out: &mut Vec<Path>) {
    match t {
        Type::Named(p, args) | Type::Dyn(p, args) => {
            out.push(*p);
            for a in args {
                mentioned_types(a, out);
            }
        }
        Type::Fn(params, rest, ret) => {
            for p in params.iter().chain(rest.as_deref()).chain(std::iter::once(&**ret)) {
                mentioned_types(p, out);
            }
        }
        Type::PtrTo(inner) => mentioned_types(inner, out),
        Type::Proj(trait_path, inner, _) => {
            out.push(*trait_path);
            mentioned_types(inner, out);
        }
        _ => {}
    }
}
