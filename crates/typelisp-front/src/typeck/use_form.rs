//! `use`: bringing a definition, a type's members or a module in under a
//! short name.
//!
//! `(use path...)`, `(import path...)` (CL's spelling of the same) and
//! `(shadowing-import path...)` (one that means to take a name something
//! else already holds). `(pub use path...)` also re-exports what each path
//! brings in. A path names, in the module its other segments lead to, a
//! function, a global or a macro, a type, a trait, or a type alias; or it is
//! `Type::member` or `Type::*`, a type's variants and public static methods
//! brought in bare; or it names a module.

use crate::check::RedefPolicy;
use crate::types::Path;
use crate::{Error, Heap, Loc, Value};
use typelisp_mem::symbols::{find_exact, intern_in, ns_parent, Import, NsId, PathStep};
use typelisp_mem::SymRef;

use super::registry::{Registry, TypeEntry, ValueDef};
use super::resolve::{find_named, sees};

/// Where a form is checked: its module, the module of the file it was read
/// from, and what a definition that takes a name already defined does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Site {
    pub(crate) ns: NsId,
    pub(crate) file: Option<NsId>,
    pub(crate) policy: RedefPolicy,
}

/// What a `use` path names.
#[derive(Clone, Debug)]
enum UseTarget {
    /// A function, a global or a macro.
    Value(Path),
    Type(Path),
    Trait(Path),
    Alias(Path),
    /// The members of a type: one, by its name, or every one.
    Members(Path, Option<String>),
    Module(NsId),
}

/// `(use path...)` — `paths` the paths — and its core form, unrooted. What
/// it brings in is registered for the forms read after it too.
pub(crate) fn check_use(
    reg: &mut Registry,
    heap: &mut Heap,
    site: Site,
    paths: &[(Value, Option<Loc>)],
    shadowing: bool,
    public: bool,
    warnings: &mut Vec<String>,
) -> Result<Value, Error> {
    let what = if shadowing { "shadowing-import" } else { "use" };
    if paths.is_empty() {
        return Err(Error::TypeError(format!("{}: ({} path...)", what, what)));
    }
    let mut forms = Vec::with_capacity(paths.len());
    for (path, _) in paths {
        let form = use_one(reg, heap, site, *path, shadowing, public, warnings)?;
        heap.push_root(form);
        forms.push(form);
    }
    if let [one] = forms[..] {
        return Ok(one);
    }
    // The wrapper only groups: a `use` does nothing when it is run.
    crate::check::forms::module_form(heap, &Path::in_ns(site.ns, "use"), &forms)
}

/// One path of a `use`.
fn use_one(
    reg: &mut Registry,
    heap: &mut Heap,
    site: Site,
    path: Value,
    shadowing: bool,
    public: bool,
    warnings: &mut Vec<String>,
) -> Result<Value, Error> {
    let syms: Vec<SymRef> = match path {
        Value::Symbol(id) => vec![id],
        Value::Path(id) => heap.path_segments(id).to_vec(),
        _ => return Err(Error::TypeError("expected a name or `::` path".into())),
    };
    let spelled: Vec<String> = syms.iter().map(|s| heap.symbol_spelling(*s).to_string()).collect();
    let bare_name = spelled.last().expect("a path has a last segment").clone();
    let bare = intern_in(site.ns, &bare_name);
    let alias = Path::of_symbol(site.ns, bare);
    let Some(target) = use_target(reg, heap, site, &syms) else {
        // The path may name a module in a file not loaded yet: a driver that
        // loads files maps the segments to one and checks this form again.
        return Err(Error::ModuleNotLoaded(spelled));
    };
    let form = match &target {
        UseTarget::Value(def) | UseTarget::Type(def) | UseTarget::Trait(def) | UseTarget::Alias(def) => {
            import_collision(reg, site, &bare_name, bare, *def, shadowing, warnings)?;
            reg.imports.insert((site.ns, bare), *def);
            crate::check::forms::use_form(heap, &alias, def)?
        }
        UseTarget::Members(ty, which) => {
            if public {
                return Err(Error::TypeError(format!(
                    "pub use: `{}` names a type's members, and only a definition is re-exported",
                    spelled.join("::")
                )));
            }
            for (name, variant) in members(reg, *ty).into_iter().filter(|(n, _)| which.as_ref().is_none_or(|w| w == n)) {
                import_member(reg, site, *ty, &name, variant, shadowing)?;
            }
            crate::check::forms::use_form(heap, &alias, ty)?
        }
        UseTarget::Module(m) => {
            if public {
                return Err(Error::TypeError(format!(
                    "pub use: `{}` is a module, and only a definition is re-exported",
                    spelled.join("::")
                )));
            }
            reg.module_aliases.insert((site.ns, bare), *m);
            crate::check::forms::use_form(heap, &alias, &Path::of_module(*m))?
        }
    };
    register(reg, heap, site.ns, &bare_name, &target);
    if public {
        reg.reexports.insert((site.ns, bare), ());
    }
    Ok(form)
}

/// What a `(use path)` written at `site` brings in, registered in `heap`'s
/// table of names for the forms read after it, so they read each name it
/// brings in as the module's own symbol, the one the import is recorded
/// under. The reader calls this as it reads the `use`, before the form is
/// checked; a path that names nothing is left for the check to report.
pub(crate) fn register_for_reading(reg: &Registry, heap: &Heap, site: Site, path: &[SymRef]) {
    let Some(last) = path.last() else { return };
    if let Some(target) = use_target(reg, heap, site, path) {
        register(reg, heap, site.ns, &heap.symbol_spelling(*last), &target);
    }
}

fn register(reg: &Registry, heap: &Heap, ns: NsId, bare: &str, target: &UseTarget) {
    let mut names = Vec::new();
    match target {
        UseTarget::Value(def) | UseTarget::Alias(def) => names.push((bare.to_string(), Import::Item(def.name_symbol()))),
        UseTarget::Type(def) | UseTarget::Trait(def) => names.push((bare.to_string(), Import::Type(def.name_symbol()))),
        UseTarget::Module(m) => names.push((bare.to_string(), Import::Module(*m))),
        UseTarget::Members(ty, which) => {
            let members = members(reg, *ty).into_iter().filter(|(n, _)| which.as_ref().is_none_or(|w| w == n));
            names.extend(members.map(|(name, _)| (name, Import::Member(ty.name_symbol()))));
        }
    }
    for (name, what) in names {
        heap.names().import_name(ns, intern_in(ns, &name), what);
    }
}

/// What the `use` path `syms`, written at `site`, names.
fn use_target(reg: &Registry, heap: &Heap, site: Site, syms: &[SymRef]) -> Option<UseTarget> {
    if let Some(found) = use_definition(reg, heap, site, syms) {
        return Some(found);
    }
    // `Type::member` or `Type::*`.
    let (last, prefix) = syms.split_last()?;
    let Some(UseTarget::Type(ty)) = use_definition(reg, heap, site, prefix) else { return None };
    if last.name() == "*" {
        return Some(UseTarget::Members(ty, None));
    }
    let name = last.name();
    members(reg, ty).iter().any(|(m, _)| m == name).then(|| UseTarget::Members(ty, Some(name.to_string())))
}

/// [`use_target`] for a path that names a definition or a module. A
/// one-segment path is a bare name. A longer one is read segment by segment
/// as the reader reads any path. A path whose first segment names no module
/// here may start at a file in the same directory: `(use arrow)` in
/// `geo/point.typl` names the module of `geo/arrow.typl`.
fn use_definition(reg: &Registry, heap: &Heap, site: Site, syms: &[SymRef]) -> Option<UseTarget> {
    let spelled: Vec<&str> = syms.iter().map(|s| s.name()).collect();
    let (first, rest) = syms.split_first()?;
    if rest.is_empty() {
        if let Some(found) = target_at(reg, site.ns, &Path::of_symbol(site.ns, *first)) {
            return Some(found);
        }
        match heap.names().first_segment(site.ns, spelled[0]) {
            Some(PathStep::Module(m)) => return Some(UseTarget::Module(m)),
            Some(PathStep::Type(_)) => return None,
            None => {}
        }
    } else if let Some(found) = target_in(reg, heap, site.ns, site.ns, &spelled) {
        return Some(found);
    }
    let dir = ns_parent(site.file?)?;
    let sibling = find_exact(dir, first.name())?;
    let PathStep::Module(m) = heap.names().reached(dir, &[sibling]).ok()? else { return None };
    if rest.is_empty() {
        return Some(UseTarget::Module(m));
    }
    target_in(reg, heap, site.ns, dir, &spelled)
}

/// [`use_definition`] for a path of two or more segments written in `here`,
/// read from the module `from`.
fn target_in(reg: &Registry, heap: &Heap, here: NsId, from: NsId, spelled: &[&str]) -> Option<UseTarget> {
    let (walked, step) = heap.names().follow_path(from, spelled).ok()?;
    let last = *walked.last().expect("a path has a last segment");
    if let PathStep::Module(m) = step {
        if let Some(found) = target_at(reg, here, &Path::of_symbol(m, last)) {
            return Some(found);
        }
    }
    match heap.names().reached(from, &walked).ok()? {
        PathStep::Module(m) => Some(UseTarget::Module(m)),
        PathStep::Type(_) => None,
    }
}

/// The definition at `p`, seen from `here`: a function, a global or a macro,
/// a type, a trait, or a type alias.
fn target_at(reg: &Registry, here: NsId, p: &Path) -> Option<UseTarget> {
    if let Some((path, _)) = written(reg, here, p, |r, at| r.values.get(at).map(|v| (v.public(), ()))) {
        return Some(UseTarget::Value(path));
    }
    let entry = |r: &Registry, at: &Path| match r.types.get(at)? {
        TypeEntry::Type(def) => Some((def.public, 0)),
        TypeEntry::Trait(def) => Some((def.public, 1)),
        TypeEntry::Alias { public } => Some((*public, 2)),
        TypeEntry::CStruct => None,
    };
    match written(reg, here, p, entry)? {
        (path, 0) => Some(UseTarget::Type(path)),
        (path, 1) => Some(UseTarget::Trait(path)),
        (path, _) => Some(UseTarget::Alias(path)),
    }
}

/// What `lookup` finds at the written path `p`, seen from `here`, with the
/// definition's path: in `p`'s module when it is visible from `here`; in
/// another module, through a re-export of it; in `here`, as a bare name
/// resolves ([`find_named`]).
fn written<T>(reg: &Registry, here: NsId, p: &Path, lookup: impl Fn(&Registry, &Path) -> Option<(bool, T)> + Copy) -> Option<(Path, T)> {
    if let Some((public, v)) = lookup(reg, p) {
        return (public || sees(here, p.ns())).then_some((*p, v));
    }
    if p.ns() != here {
        let key = (p.ns(), p.name_symbol());
        reg.reexports.get(&key)?;
        let target = *reg.imports.get(&key)?;
        return written(reg, here, &target, lookup);
    }
    find_named(reg, here, p.name_symbol(), |ns, key| lookup(reg, &Path::of_symbol(ns, key)).map(|(_, v)| v))
}

/// The members of the type `ty` a `use` can bring in bare: its variants,
/// each with its index, and its public static methods.
fn members(reg: &Registry, ty: Path) -> Vec<(String, Option<usize>)> {
    let Some(def) = reg.type_def(&ty) else { return Vec::new() };
    let variants = def.variants.iter().enumerate().map(|(i, v)| (v.name.name().to_string(), Some(i)));
    let mut statics: Vec<(String, Option<usize>)> =
        reg.methods_of(ty).filter(|(_, m)| !m.instance && m.sig.public).map(|(name, _)| (name.name().to_string(), None)).collect();
    statics.sort();
    let mut out: Vec<(String, Option<usize>)> = variants.collect();
    for (name, v) in statics {
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, v));
        }
    }
    out
}

/// Bring the member `name` of the type `ty` in bare: a variant, `variant`
/// its index, or a static method. A bare name already standing for another
/// type's member, or for a definition of this module, is an error, as two
/// `use`s of one name are in Rust: which one a call means would depend on the
/// order of the `use`s. `shadowing` replaces the member instead.
fn import_member(reg: &mut Registry, site: Site, ty: Path, name: &str, variant: Option<usize>, shadowing: bool) -> Result<(), Error> {
    let key = (site.ns, intern_in(site.ns, name));
    let held = match (reg.ctors.get(&key), reg.static_uses.get(&key)) {
        (Some((owner, i)), _) if variant != Some(*i) || *owner != ty => Some(format!("`{}::{}`", owner, name)),
        (_, Some((owner, _))) if variant.is_some() || *owner != ty => Some(format!("`{}::{}`", owner, name)),
        _ if reg.values.get(&Path::of_symbol(key.0, key.1)).is_some() => Some("a definition of this module".to_string()),
        _ => None,
    };
    if let Some(held) = held.filter(|_| !shadowing) {
        return Err(Error::TypeError(format!(
            "use: `{}` already stands for {} here — write `{}::{}`, or say `shadowing-import` to replace it",
            name, held, ty, name
        )));
    }
    match variant {
        Some(i) => {
            reg.static_uses.remove(&key);
            reg.ctors.insert(key, (ty, i));
        }
        None => {
            reg.ctors.remove(&key);
            reg.static_uses.insert(key, (ty, typelisp_mem::symbols::member_symbol(name)));
        }
    }
    Ok(())
}

/// What importing `def` under the bare name `bare_name` (the symbol `bare`)
/// collides with at `site`, reported as `site.policy` says. Importing this
/// module's own definition of the name, or the same definition again,
/// collides with nothing.
fn import_collision(
    reg: &Registry,
    site: Site,
    bare_name: &str,
    bare: SymRef,
    def: Path,
    shadowing: bool,
    warnings: &mut Vec<String>,
) -> Result<(), Error> {
    if shadowing || (def.ns() == site.ns && def.name_symbol() == bare) {
        return Ok(());
    }
    let key = (site.ns, bare);
    if reg.imports.get(&key) == Some(&def) {
        return Ok(());
    }
    let here = Path::of_symbol(site.ns, bare);
    let message = if reg.imports.get(&key).is_some() || reg.module_aliases.get(&key).is_some() {
        format!("import of `{}` replaces an earlier import of the same name", bare_name)
    } else {
        let (value, ty) = (reg.values.get(&here), reg.types.get(&here));
        let held = match (value, ty) {
            (Some(ValueDef::Fn(_)), _) => "function",
            (_, Some(TypeEntry::Type(_))) => "type",
            (_, Some(TypeEntry::Trait(_))) => "trait",
            (Some(ValueDef::Macro { .. }), _) => "macro",
            (Some(ValueDef::Var(_)), _) => "global",
            (_, Some(TypeEntry::Alias { .. })) => "type alias",
            _ => return Ok(()),
        };
        format!(
            "import of `{}` does nothing: the {} of that name defined here wins. \
             Call it by its path, or say `shadowing-import` to mean it",
            bare_name, held
        )
    };
    match site.policy {
        RedefPolicy::Error => Err(Error::TypeError(message)),
        RedefPolicy::Warn => {
            warnings.push(format!("warning: {}", message));
            Ok(())
        }
        RedefPolicy::Silent => Ok(()),
    }
}

