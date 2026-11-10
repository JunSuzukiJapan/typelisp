//! Checking one top-level form: the rebuilt checker's entry point.

use std::cell::{OnceCell, RefCell};

use crate::check::lower::CoreForms;
use crate::check::{MacroExpander, MONO_BUNDLE_MODULE};
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::intern_in;
use typelisp_mem::SymRef;

use super::define::Definer;
use super::macros::expand_toplevel;
use super::module::module_site;
use super::defenum::check_defenum;
use super::deftrait::check_deftrait;
use super::defmethod::check_defmethod;
use super::defstruct::check_defstruct;
use super::defun::{check_defun, parse_header};
use super::defvar::{check_defvar, Which};
use super::impl_block::check_impl;
use super::use_form::{check_use, register_for_reading, Site};
use super::expr::{check_expr, Cx};
use super::generic::{Base, Name, Request, Requests};
use super::locals::Locals;
use super::lower::{lower_rooted, Lowering};
use super::registry::{Registry, TypeEntry};
use super::scope::TypeScope;
use super::subst::Subst;
use super::wire::{Bodies, RebuiltBodies};

/// The rebuilt checker, as the current one hands it each top-level form
/// while the switch `TYPELISP_TYPECK` is on (`docs/dev/checker-rebuild-plan.md` 6).
#[derive(Debug, Default)]
pub(crate) struct TypeChecker {
    /// What the program has defined, the current checker's registry copied
    /// in first: when it is first needed (8.3), which may be the reader
    /// asking what a `use` brings in before any form is checked.
    reg: OnceCell<Registry>,
    /// The type of the last form, when it was an expression.
    expr_ty: Option<Type>,
    /// The warnings checking has given and no one has taken yet.
    warnings: RefCell<Vec<String>>,
    /// The generic bodies dumps wrote, read before the registry was made,
    /// for it to take when it is ([`Self::take_bodies`]).
    pending: RefCell<Vec<Bodies>>,
    /// Every C callback entry the checked code asks for, in the order first
    /// asked.
    callbacks: Vec<crate::ffi_callback::CallbackSig>,
    /// Every definition a `~/name/` directive calls.
    format_calls: std::collections::BTreeSet<crate::CompileTarget>,
    /// The module the last form, an `(in-module path)`, made the current
    /// one, for the driver, which keeps the current module, to take.
    entered: Option<typelisp_mem::NsId>,
}

impl TypeChecker {
    /// The rebuilt checker when the switch is on, `None` otherwise.
    pub(crate) fn from_switch() -> Option<TypeChecker> {
        std::env::var_os("TYPELISP_TYPECK").is_some_and(|v| !v.is_empty()).then(TypeChecker::default)
    }

    /// The registry, `old` (the current checker's registry, which holds the
    /// built-in definitions and the prelude) copied in when it is first
    /// needed.
    fn registry(&self, old: &crate::check::Registry) -> &Registry {
        self.reg.get_or_init(|| {
            let mut reg = Registry::default();
            super::bridge::import(old, &mut reg);
            self.pending.take().into_iter().for_each(|b| b.adopt(&mut reg));
            reg.commit();
            reg
        })
    }

    /// `v`, a form written at `site`, checked: its core form, unrooted.
    /// `expander` expands its macro calls.
    ///
    /// What the form defines is kept only if checking it succeeds.
    pub(crate) fn check_form(
        &mut self,
        heap: &mut Heap,
        old: &crate::check::Registry,
        expander: &dyn MacroExpander,
        site: Site,
        v: Value,
        loc: Option<Loc>,
    ) -> Result<Value, Error> {
        self.registry(old);
        let reg = self.reg.get_mut().expect("the registry was just made");
        publish_name_view(reg, site.ns);
        self.expr_ty = None;
        let entered = in_module(heap, site, v).map_err(|e| match &loc {
            Some(l) => e.at(l.clone()),
            None => e,
        })?;
        if let Some(inner) = entered {
            self.entered = Some(inner.ns);
            return crate::check::forms::module_form(heap, &Path::of_module(inner.ns), &[]);
        }
        // Every value the tree holds is rooted from here until it is lowered
        // into the form, which is then reachable from nothing but the caller.
        // Nothing allocates between releasing them and the caller rooting it.
        let roots_base = heap.root_count();
        let result = check_toplevel(reg, expander, heap, site, v, loc.clone(), &mut self.warnings.borrow_mut());
        heap.truncate_roots(roots_base);
        match result {
            Ok((form, ty, reached)) => {
                self.format_calls.extend(reached.format_calls);
                for sig in reached.callbacks {
                    if !self.callbacks.contains(&sig) {
                        self.callbacks.push(sig);
                    }
                }
                if crate::types::in_std(site.ns) {
                    super::library::reexport(reg);
                    super::library::publish(reg, &mut heap.names());
                }
                let types: Vec<Path> = reg.types.written().copied().collect();
                let methods: Vec<(Path, SymRef)> = reg.methods_written().copied().collect();
                reg.commit();
                publish(reg, heap, &types, &methods);
                publish_name_view(reg, site.ns);
                self.expr_ty = ty;
                Ok(form)
            }
            Err(e) => {
                reg.rollback();
                Err(match loc {
                    Some(l) => e.at(l),
                    None => e,
                })
            }
        }
    }

    /// The generic bodies a dump wrote (`docs/dev/checker-rebuild-plan.md`
    /// 8.3): what a registry copied from the current checker's lacks of the
    /// prelude.
    /// Every C callback entry the checked code asks for, for an executable
    /// to emit.
    pub(crate) fn ffi_callbacks(&self) -> Vec<crate::ffi_callback::CallbackSig> {
        self.callbacks.clone()
    }

    /// Every definition a `~/name/` directive calls, for an executable to
    /// register.
    pub(crate) fn format_call_targets(&self) -> Vec<crate::CompileTarget> {
        self.format_calls.iter().cloned().collect()
    }

    pub(crate) fn take_bodies(&mut self, bodies: Bodies) {
        match self.reg.get_mut() {
            Some(reg) => {
                bodies.adopt(reg);
                reg.commit();
            }
            None => self.pending.get_mut().push(bodies),
        }
    }

    /// What `(use path)` written at `site` brings in, registered for reading
    /// the forms after it (`use_form::register_for_reading`).
    pub(crate) fn register_use_for_reading(&self, heap: &Heap, old: &crate::check::Registry, site: Site, path: &[SymRef]) {
        register_for_reading(self.registry(old), heap, site, path);
    }

    /// `v`, written in the module `ns`, expanded once when it is a call of a
    /// macro, for a driver that reads what the expansion `use`s before the
    /// form is checked.
    pub(crate) fn expand_toplevel_macro(&self, heap: &mut Heap, old: &crate::check::Registry, expander: &dyn MacroExpander, ns: typelisp_mem::NsId, v: Value) -> Result<Option<Value>, Error> {
        super::macros::expand_toplevel(self.registry(old), expander, heap, ns, v)
    }

    /// The functions a `defsignature` declared that no `defun` has defined,
    /// forgotten: what finishing a unit reports.
    pub(crate) fn take_predeclared(&mut self) -> Vec<Path> {
        let Some(reg) = self.reg.get_mut() else { return Vec::new() };
        let mut paths: Vec<Path> = reg.predeclared.iter().map(|(p, _)| *p).collect();
        for p in &paths {
            reg.predeclared.remove(p);
        }
        reg.commit();
        paths.sort_by_key(|p| p.to_string());
        paths
    }

    /// The type of the last form [`Self::check_form`] checked, when it was an
    /// expression.
    pub(crate) fn expr_type(&self) -> Option<&Type> {
        self.expr_ty.as_ref()
    }

    /// The module the last form made the current one, when it was an
    /// `(in-module path)`.
    pub(crate) fn take_entered(&mut self) -> Option<typelisp_mem::NsId> {
        self.entered.take()
    }

    /// The warnings given since they were last taken.
    pub(crate) fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
    }
}

/// The core macro layer and the prelude checked by a checker of their own,
/// their generic bodies, and the methods their `impl`s took from a trait's
/// default body, written down for the prelude's dump. Nothing is run:
/// `expander`, which expands the macro calls, has run the prelude already.
///
/// The source is fixed: a form the checker refuses is a bug of the checker,
/// not the program's.
pub(crate) fn prelude_bodies(heap: &mut Heap, expander: &dyn MacroExpander) -> Result<RebuiltBodies, Error> {
    let base = crate::check::Registry::with_builtins();
    let mut checker = TypeChecker::default();
    let site = Site { ns: typelisp_mem::NsId::STD, file: None, policy: crate::check::RedefPolicy::Error };
    let reader = crate::Reader::new();
    for src in [crate::core_macros::SOURCE, crate::prelude::SOURCE.as_str()] {
        let mut forms = reader.forms_within(crate::LIBRARY_FILE, src, typelisp_mem::NsId::STD);
        while let Some((v, loc)) = forms.next_form(heap)? {
            let base_roots = heap.root_count();
            heap.push_root(v);
            let checked = checker.check_form(heap, &base, expander, site, v, Some(loc));
            heap.truncate_roots(base_roots);
            checked?;
        }
    }
    RebuiltBodies::of(heap, checker.reg.get().expect("checking made it"))
}

/// `v`'s core form, its type when it is an expression, and what its code
/// asks to be reached from outside it. The specializations the form asks for come first,
/// then the functions lifted for its callbacks, in one bundle with it.
fn check_toplevel(
    reg: &mut Registry,
    expander: &dyn MacroExpander,
    heap: &mut Heap,
    site: Site,
    v: Value,
    loc: Option<Loc>,
    warnings: &mut Vec<String>,
) -> Result<(Value, Option<Type>, super::generic::Reached), Error> {
    let mut requests = Requests::default();
    let (form, ty) = check_primary(reg, expander, heap, site, v, loc, &mut requests, warnings)?;
    let form = bundled(heap, reg, &mut requests, form)?;
    Ok((form, ty, requests.take_reached()))
}

/// `form`, rooted, after what it needs made first: the specializations
/// asked for and the functions lifted since the last bundle, in one bundle
/// with it.
pub(crate) fn bundled(heap: &mut Heap, reg: &Registry, requests: &mut Requests, form: Value) -> Result<Value, Error> {
    heap.push_root(form);
    let mut forms = Vec::new();
    while let Some(req) = requests.next()? {
        let spec = specialize(heap, reg, requests, &req)?;
        heap.push_root(spec);
        forms.push(spec);
    }
    forms.extend(requests.take_lifted());
    if forms.is_empty() {
        return Ok(form);
    }
    forms.push(form);
    let bundle = crate::check::forms::module_form(heap, &Path::root(MONO_BUNDLE_MODULE), &forms)?;
    heap.push_root(bundle);
    Ok(bundle)
}

/// The site the form `v` written at `site` makes current, when it is an
/// `(in-module path)`: the module `path` names under `site`'s.
fn in_module(heap: &Heap, site: Site, v: Value) -> Result<Option<Site>, Error> {
    if !matches!(heap.car(v), Ok(Value::Symbol(h)) if h.is(wk::IN_MODULE)) {
        return Ok(None);
    }
    match heap.list_to_vec_locs(v)?.as_slice() {
        [_, path] => Ok(Some(super::module::module_site(heap, site, std::slice::from_ref(path))?.0)),
        _ => Err(Error::TypeError("in-module: (in-module path)".into())),
    }
}

#[allow(clippy::too_many_arguments)]
fn check_primary(
    reg: &mut Registry,
    expander: &dyn MacroExpander,
    heap: &mut Heap,
    site: Site,
    v: Value,
    loc: Option<Loc>,
    requests: &mut Requests,
    warnings: &mut Vec<String>,
) -> Result<(Value, Option<Type>), Error> {
    let ns = site.ns;
    if let Some(expanded) = expand_toplevel(reg, expander, heap, ns, v)? {
        return check_primary(reg, expander, heap, site, expanded, loc, requests, warnings);
    }
    if matches!(heap.car(v), Ok(Value::Symbol(h)) if h.is(wk::MODULE)) {
        let elems = heap.list_to_vec_locs(v)?;
        let (inner, path) = module_site(heap, site, &elems[1..])?;
        let mut body = Vec::new();
        // Types in messages are written as the module being checked reads
        // them.
        publish_name_view(reg, inner.ns);
        let mut inner = inner;
        for (form, form_loc) in &elems[2..] {
            // `(in-module path)` in the body checks the rest of the body in
            // the module `path` names under the one it is written in.
            if let Some(entered) = in_module(heap, inner, *form)? {
                inner = entered;
                publish_name_view(reg, inner.ns);
                continue;
            }
            let (checked, _) = check_primary(reg, expander, heap, inner, *form, form_loc.clone(), requests, warnings)?;
            heap.push_root(checked);
            // Run now, it runs with what it needs made.
            let checked = match crate::project::needs_immediate_exec(heap, checked) {
                true => {
                    let checked = bundled(heap, reg, requests, checked)?;
                    expander.install(heap, checked).map_err(Error::TypeError)?;
                    checked
                }
                false => checked,
            };
            body.push(checked);
        }
        publish_name_view(reg, ns);
        return Ok((crate::check::forms::module_form(heap, &path, &body)?, None));
    }
    let d = &mut Definer { site, warnings, expander, public: false, loc: loc.clone() };
    if let Value::Cons(_) = v {
        let all = heap.list_to_vec_locs(v)?;
        // `(pub definition)`: the definition, visible outside its module.
        let elems = match all.split_first() {
            Some(((Value::Symbol(p), _), inner)) if p.is(wk::PUB) => {
                d.public = true;
                pub_definition(inner)?
            }
            _ => &all[..],
        };
        // `impl<T, U>` reads as `(%type-apply impl T U)`.
        if crate::types::is_type_application(heap, elems[0].0) {
            let (head, names) = parse_header(heap, elems[0].0)?;
            if head.is(wk::IMPL) {
                return Ok((check_impl(reg, d, heap, requests, &names, &elems[1..])?, None));
            }
        }
        if let Value::Symbol(head) = elems[0].0 {
            if head.is(wk::IMPL) {
                return Ok((check_impl(reg, d, heap, requests, &[], &elems[1..])?, None));
            }
            let args = &elems[1..];
            match head.well_known() {
                wk::DEFUN => return Ok((check_defun(reg, d, heap, requests, args)?, None)),
                wk::DEFSTRUCT => return Ok((check_defstruct(reg, d, heap, requests, args)?, None)),
                wk::DEFENUM => return Ok((check_defenum(reg, d, heap, args)?, None)),
                wk::DEFTRAIT => return Ok((check_deftrait(reg, d, heap, args)?, None)),
                wk::DEFTYPE => return Ok((super::deftype::check_deftype(reg, d, heap, args)?, None)),
                wk::DEFSIGNATURE => return Ok((super::defsignature::check_defsignature(reg, d, heap, args)?, None)),
                wk::DEFFFI => return Ok((super::defffi::check_defffi(reg, d, heap, args)?, None)),
                wk::DEFMACRO => return Ok((super::defmacro::check_defmacro(reg, d, heap, requests, args)?, None)),
                wk::DEFMETHOD => return Ok((check_defmethod(reg, d, heap, requests, args)?, None)),
                wk::DEFVAR => return Ok((check_defvar(reg, d, heap, requests, Which::Defvar, args)?, None)),
                wk::DEFPARAMETER => return Ok((check_defvar(reg, d, heap, requests, Which::Defparameter, args)?, None)),
                wk::DEFCONSTANT => return Ok((check_defvar(reg, d, heap, requests, Which::Defconstant, args)?, None)),
                wk::USE | wk::IMPORT => {
                    let public = d.public;
                    return Ok((check_use(reg, heap, site, args, false, public, d.warnings)?, None));
                }
                wk::LOAD => {
                    return match args {
                        [(Value::Str(id), _)] => {
                            let path = heap.string(*id).to_string();
                            Ok((crate::check::forms::load_form(heap, &path)?, None))
                        }
                        [_] => Err(Error::TypeError("load: expected a string-literal path".into())),
                        _ => Err(Error::TypeError("load: (load \"path\")".into())),
                    };
                }
                wk::SHADOWING_IMPORT => return Ok((check_use(reg, heap, site, args, true, false, d.warnings)?, None)),
                // The reader made each name the module's own; nothing is left
                // to run.
                wk::SHADOW => return Ok((crate::check::forms::module_form(heap, &Path::in_ns(ns, "nothing"), &[])?, None)),
                wk::UNSAFE if super::c_memory::declares_c_structs(heap, args)? => {
                    return Ok((super::c_memory::check_c_struct_block(reg, d, heap, args)?, None));
                }
                _ => {}
            }
        }
    }
    let mut typed = {
        let scope = TypeScope::empty();
        let cx = Cx::new(reg, expander, ns, &scope, None);
        let mut typed = check_expr(&cx, heap, &Locals::empty(), v, None, loc)?;
        cx.settle(heap, std::slice::from_mut(&mut typed))?;
        typed
    };
    let subst = Subst::default();
    let mut lw = Lowering { reg, subst: &subst, requests };
    let form = lower_rooted(heap, &mut lw, &typed)?;
    let form = crate::check::core::tagged(heap, wk::EXPR, &[form])?;
    let ty = std::mem::replace(&mut typed.ty, Type::Unit);
    Ok((form, Some(ty)))
}

/// The definition a `(pub ...)` makes visible outside its module, `inner`
/// being what follows the `pub`.
fn pub_definition(inner: &[(Value, Option<Loc>)]) -> Result<&[(Value, Option<Loc>)], Error> {
    let Some((Value::Symbol(head), _)) = inner.first() else {
        return Err(Error::TypeError("pub: expected a definition form".into()));
    };
    match head.well_known() {
        wk::DEFUN | wk::DEFVAR | wk::DEFPARAMETER | wk::DEFCONSTANT | wk::DEFMETHOD | wk::DEFSTRUCT | wk::DEFENUM | wk::DEFTRAIT | wk::DEFTYPE | wk::DEFSIGNATURE | wk::DEFFFI | wk::USE | wk::DEFMACRO => Ok(inner),
        _ if head.is(wk::DEF_C_STRUCT) => Err(Error::TypeError(
            "def-c-struct is written at top level, inside `(unsafe ...)`: (unsafe (def-c-struct name (field type)...))".into(),
        )),
        _ => Err(Error::TypeError(
            "pub: expected defun/defsignature/defffi/defmacro/defstruct/defenum/deftrait/deftype/defvar/defparameter/defconstant/use".into(),
        )),
    }
}

/// Makes the printer write types as the module `ns` reads them
/// (`typelisp_mem::NameView`): those it brought in, and those `std::prelude`
/// re-exports that no type of its own or brought in takes the name of, by
/// their last segment.
fn publish_name_view(reg: &Registry, ns: typelisp_mem::NsId) {
    let is_type = |p: &Path| matches!(reg.types.get(p), Some(TypeEntry::Type(_) | TypeEntry::Trait(_)));
    let mut imports = std::collections::HashMap::new();
    let mut taken = std::collections::HashSet::new();
    for ((at, bare), target) in reg.imports.iter() {
        if *at == ns && is_type(target) {
            imports.insert((target.ns(), target.name_symbol()), bare.name());
            taken.insert(bare.name());
        }
    }
    let prelude = crate::types::prelude_ns();
    for ((at, bare), ()) in reg.reexports.iter() {
        let Some(target) = reg.imports.get(&(*at, *bare)) else { continue };
        if *at != prelude || !is_type(target) || taken.contains(bare.name()) || is_type(&Path::in_ns(ns, bare.name())) {
            continue;
        }
        imports.entry((target.ns(), target.name_symbol())).or_insert(bare.name());
    }
    typelisp_print::shared::set_name_view(typelisp_mem::NameView::new(ns, imports));
}

/// Tells the session's table of what names mean (`Heap::names`) about the
/// types `types` and the methods `methods` a form defined, so a path read in
/// a later form can step into the type (`Shape::circle`, `Pair::new`). A
/// type's members are its variants, its fields and its methods, read as in
/// the type's own module.
fn publish(reg: &Registry, heap: &Heap, types: &[Path], methods: &[(Path, SymRef)]) {
    let mut names = heap.names();
    for path in types {
        let Some(def) = reg.type_def(path) else { continue };
        let (home, ty) = (path.ns(), path.name_symbol());
        names.declare_type(home, ty);
        for member in def.variants.iter().map(|v| v.name).chain(def.fields.iter().map(|f| f.name)) {
            names.declare_member(ty, intern_in(home, member.name()));
        }
    }
    for (owner, method) in methods {
        names.declare_member(owner.name_symbol(), intern_in(owner.ns(), method.name()));
    }
}

/// The generic function or method `req.base` specialized at `req.args`: its
/// body lowered with the type parameters replaced, under the
/// specialization's name.
fn specialize(heap: &mut Heap, reg: &Registry, requests: &mut Requests, req: &Request) -> Result<Value, Error> {
    let type_params = match req.base {
        Base::Fn(p) => &reg.fn_def(&p).expect("a specialization is asked for only of a defined function").type_params,
        Base::Method(owner, m) => &reg.method(&owner, m).expect("a specialization is asked for only of a defined method").sig.type_params,
    };
    let (template, subst) = match (reg.template(&req.base), req.base) {
        (Some(template), _) => (template, Subst::positional(type_params, &req.args)),
        // A method an `impl` took from the trait's default body is that
        // body, the trait's parameters where the `impl` puts them.
        (None, Base::Method(owner, m)) if reg.defaulted(&owner, m).is_some() => {
            let (trait_path, at) = reg.defaulted(&owner, m).expect("checked above");
            let outer = Subst::positional(type_params, &req.args);
            let at: Vec<Type> = at.iter().map(|t| outer.apply(t)).collect();
            super::trait_impl::default_body(reg, trait_path, m, &at)
        }
        // The prelude's generic definitions were checked by the current
        // checker, which keeps their source, not a checked tree.
        (None, _) => unimplemented!("typeck: specializing {:?}, which this checker has no checked body of", req.base),
    };
    let mut lw = Lowering { reg, subst: &subst, requests };
    let (params, ret, body) = super::defun::lower_template(heap, &mut lw, &template)?;
    match (req.base, req.name) {
        (Base::Fn(_), Name::Fn(name)) => reg.defun_form(heap, &name, &params, &ret, &body),
        (Base::Method(owner, m), Name::Method(name)) => {
            let instance = reg.method(&owner, m).expect("checked above").instance;
            reg.defmethod_form(heap, &owner, name, instance, &params, &ret, &body)
        }
        _ => unreachable!("a specialization is named as what it specializes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Interp, Reader};
    use typelisp_mem::symbols::NsId;

    /// The module the tests' forms are written in.
    const ROOT_SITE: Site = Site { ns: NsId::ROOT, file: None, policy: crate::check::RedefPolicy::Error };

    /// `src`'s forms, read, checked by the rebuilt checker and run in order:
    /// the last one's value and type.
    fn run(src: &str) -> (Heap, Value, Type) {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        crate::check::Registry::with_builtins().publish_names(&mut heap.names());
        let mut forms = reader.forms(src);
        let mut last = None;
        while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
            heap.push_root(v);
            let form = checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), &interp, ROOT_SITE, v, Some(loc)).expect("check failed");
            heap.push_root(form);
            if let Some(value) = interp.exec(&mut heap, form).expect("run failed") {
                last = Some((value, checker.expr_type().expect("an expression has a type").clone()));
            }
        }
        let (value, ty) = last.expect("the last form is an expression");
        (heap, value, ty)
    }

    /// `src`'s forms checked in order, the last expected to fail: its message.
    fn check_err(src: &str) -> String {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        crate::check::Registry::with_builtins().publish_names(&mut heap.names());
        let mut forms = reader.forms(src);
        let mut result = Ok(Value::Empty);
        while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
            heap.push_root(v);
            result = checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), &interp, ROOT_SITE, v, Some(loc));
        }
        match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected the last form to fail"),
        }
    }

    /// `src`'s forms checked in order, each expected to check: the last
    /// one's type.
    fn check_ok(src: &str) -> Type {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        crate::check::Registry::with_builtins().publish_names(&mut heap.names());
        let mut forms = reader.forms(src);
        while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
            heap.push_root(v);
            let form = checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), &interp, ROOT_SITE, v, Some(loc)).expect("check failed");
            heap.push_root(form);
        }
        checker.expr_type().expect("the last form is an expression").clone()
    }

    #[test]
    fn integer_literals() {
        let (_, v, ty) = run("42");
        assert_eq!((v, ty), (Value::Int(42), Type::Int));
        let (heap, v, ty) = run("123456789012345678901234567890");
        assert_eq!(ty, Type::Int);
        let Value::Boxed(id) = v else { panic!("a bignum is boxed: {:?}", v) };
        assert_eq!(heap.bignum_value(id).to_string(), "123456789012345678901234567890");
    }

    #[test]
    fn other_literals() {
        assert_eq!(run("true").1, Value::Bool(true));
        assert_eq!(run("#\\a").1, Value::Char('a'));
        assert_eq!(run("()").2, Type::Unit);
        assert_eq!(run(":key").2, Type::Symbol);
        let (heap, v, ty) = run("2.5");
        let Value::Boxed(id) = v else { panic!("a float is boxed: {:?}", v) };
        assert_eq!((heap.f64_value(id), ty), (2.5, Type::F64));
        let (heap, v, ty) = run("\"abc\"");
        let Value::Str(id) = v else { panic!("a string: {:?}", v) };
        assert_eq!((heap.string(id).to_string(), ty), ("abc".to_string(), Type::Str));
        assert_eq!(run("1/3").2, Type::Ratio);
    }

    #[test]
    fn a_function_with_let_and_if() {
        let src = "(defun pick ((c bool) (a int) (b int)) int (let ((x a)) (if c x b)))\n(pick false 1 2)";
        assert_eq!(run(src).1, Value::Int(2));
    }

    /// A parameter's type decides an integer literal's: `u8` here.
    #[test]
    fn an_argument_is_checked_against_its_parameter() {
        assert_eq!(run("(defun id8 ((x u8)) u8 x)\n(id8 200)").2, Type::U8);
        let msg = check_err("(defun id8 ((x u8)) u8 x)\n(id8 300)");
        assert!(msg.contains("integer literal 300 is out of range for u8"), "{}", msg);
    }

    #[test]
    fn a_function_calls_itself() {
        let src = "(defun last ((n int) (stop bool)) int (if stop n (last 7 true)))\n(last 1 false)";
        assert_eq!(run(src).1, Value::Int(7));
    }

    /// An error is reported where the form that is wrong was written.
    #[test]
    fn an_error_is_where_it_was_written() {
        let msg = check_err("(defun f () int true)");
        assert!(msg.contains(":1:17:"), "{}", msg);
    }

    #[test]
    fn mismatches() {
        let msg = check_err("(defun f () int true)");
        assert!(msg.contains("expected `int`, found `bool`"), "{}", msg);
        let msg = check_err("(if 1 2 3)");
        assert!(msg.contains("expected `bool`, found `int`"), "{}", msg);
        let msg = check_err("(if true 2 \"x\")");
        assert!(msg.contains("expected `int`, found `string`"), "{}", msg);
        let msg = check_err("(defun f ((a int)) int a)\n(f)");
        assert!(msg.contains("expected 1 argument(s), got 0"), "{}", msg);
    }

    #[test]
    fn a_generic_function_is_specialized_for_each_instantiation() {
        let src = "(defun id<T> ((x T)) T x)\n(defun pick<T> ((c bool) (a T) (b T)) T (if c (id a) b))\n(pick false (id 1) 2)";
        assert_eq!(run(src).1, Value::Int(2));
        let src = "(defun id<T> ((x T)) T x)\n(id true)";
        assert_eq!(run(src).2, Type::Bool);
    }

    /// The names a generic function's parameters are declared with do not
    /// matter to a caller that declares the same names.
    #[test]
    fn the_same_names_in_caller_and_callee() {
        let src = "(defun id<T> ((x T)) T x)\n(defun wrap<T> ((y T)) T (id y))\n(wrap #\\z)";
        assert_eq!(run(src).1, Value::Char('z'));
    }

    #[test]
    fn a_generic_function_calls_itself() {
        let src = "(defun again<T> ((x T) (stop bool)) T (if stop x (again x true)))\n(again 9 false)";
        assert_eq!(run(src).1, Value::Int(9));
    }

    /// The body is checked once, with `T` abstract: a `T` is not an `int`.
    #[test]
    fn a_type_parameter_is_no_particular_type() {
        let msg = check_err("(defun f<T> ((x T)) int x)");
        assert!(msg.contains("expected `int`, found `T`"), "{}", msg);
        let msg = check_err("(defun f<T> ((x T)) T 1)");
        assert!(msg.contains("expected `T`, found `int`"), "{}", msg);
    }

    /// A parameter's type decides the argument's even through a hole: the
    /// first argument makes `T` a `u8`, and the second must be one.
    #[test]
    fn one_argument_decides_anothers_type() {
        let src = "(defun id8 ((x u8)) u8 x)\n(defun same<T> ((a T) (b T)) T b)\n(same (id8 1) 300)";
        let msg = check_err(src);
        assert!(msg.contains("integer literal 300 is out of range for u8"), "{}", msg);
    }

    #[test]
    fn a_type_argument_nothing_decides() {
        let msg = check_err("(defun mk<T> () int 1)\n(mk)");
        assert!(msg.contains("cannot infer type parameter `T`"), "{}", msg);
    }

    #[test]
    fn a_type_parameter_declared_twice() {
        let msg = check_err("(defun f<T, T> ((x T)) T x)");
        assert!(msg.contains("type parameter `T` is declared twice"), "{}", msg);
    }

    const PAIR: &str = "(defstruct Pair<T, U> (first T) (second U))\n\
        (defun mk () Pair<int,string> (make-instance (quote Pair) :first 1 :second \"Hello\"))\n";

    #[test]
    fn a_struct_its_accessors_and_its_fields() {
        assert_eq!(run(&format!("{}(first (mk))", PAIR)).1, Value::Int(1));
        assert_eq!(run(&format!("{}(let ((p (mk))) (set-first p 5) p::first)", PAIR)).1, Value::Int(5));
        assert_eq!(run(&format!("{}(mk)", PAIR)).2.to_string(), "Pair<int,string>");
    }

    /// A method declares its type parameters in its name; the receiver's
    /// type only uses them, and which names `Pair` declared does not matter.
    #[test]
    fn a_method_declares_its_own_type_parameters() {
        let src = format!("{}(defmethod pair-first<T, U> ((p Pair<T,U>)) T p::first)\n(pair-first (mk))", PAIR);
        assert_eq!(run(&src).1, Value::Int(1));
        let src = format!(
            "{}(defmethod swap<A, B> ((p Pair<A,B>)) Pair<B,A> (make-instance (quote Pair) :first p::second :second p::first))\n(first (swap (mk)))",
            PAIR
        );
        assert_eq!(run(&src).2, Type::Str);
        // `mk<T>`'s `T` is not `Pair`'s: they are two parameters.
        let src = format!("{}(defmethod with<T> ((p Pair<int,string>) (x T)) T x)\n(with (mk) true)", PAIR);
        assert_eq!(run(&src).1, Value::Bool(true));
    }

    #[test]
    fn a_receiver_type_does_not_declare_anything() {
        let msg = check_err(&format!("{}(defmethod bad ((p Pair<T,U>)) T p::first)", PAIR));
        assert!(msg.contains("unknown type `T`"), "{}", msg);
    }

    /// `impl<T, U>` declares what the target and the methods use; each
    /// method adds its own after them.
    #[test]
    fn an_impl_declares_its_type_parameters() {
        let src = format!(
            "{}(impl<T, U> Pair<T, U> (defmethod fst (self) T self::first) \
               (defmethod swap (self) Pair<U,T> (make-instance (quote Pair) :first self::second :second self::first)) \
               (defmethod me (self) Self self))\n(fst (swap (me (mk))))",
            PAIR
        );
        assert_eq!(run(&src).2, Type::Str);
        let msg = check_err(&format!("{}(impl Pair<T,U> (defmethod bad (self) T self::first))", PAIR));
        assert!(msg.contains("unknown type `T`"), "{}", msg);
    }

    /// The case that started the rebuild: the method's own `T` is no
    /// parameter of `Pair`'s, whatever `Pair` called its own.
    #[test]
    fn a_methods_own_parameter_may_have_any_name() {
        let src = format!("{}(impl<A, B> Pair<A, B> (defmethod mk<T> (self (x T)) T x))\n(mk (mk) true)", PAIR);
        assert_eq!(run(&src).1, Value::Bool(true));
        let msg = check_err(&format!("{}(impl<T> Pair<T, int> (defmethod bad<T> (self) T self::first))", PAIR));
        assert!(msg.contains("type parameter `T` is declared twice"), "{}", msg);
    }

    #[test]
    fn make_instance_infers_and_checks() {
        let msg = check_err(&format!("{}(make-instance (quote Pair) :first 1)", PAIR));
        assert!(msg.contains("slot `second` is not given"), "{}", msg);
        let msg = check_err(&format!("{}(defun f () Pair<int,int> (make-instance (quote Pair) :first 1 :second \"x\"))", PAIR));
        assert!(msg.contains("expected `int`, found `string`"), "{}", msg);
        // Written out of order, the values still run in the order written.
        assert_eq!(run(&format!("{}(second (make-instance (quote Pair) :second 2 :first 1))", PAIR)).1, Value::Int(2));
    }

    const SHAPE: &str = "(defenum Shape (Circle int) (Rect int int) Dot)\n";

    #[test]
    fn an_enum_is_made_and_matched() {
        let area = "(defun area ((s Shape)) int (match s ((Circle r) r) ((Rect w h) (* w h)) (Dot 0)))\n";
        assert_eq!(run(&format!("{}{}(area (Shape::Rect 2 3))", SHAPE, area)).1, Value::Int(6));
        assert_eq!(run(&format!("{}{}(area (Shape::Dot))", SHAPE, area)).1, Value::Int(0));
        let msg = check_err(&format!("{}(Shape::Circle \"x\")", SHAPE));
        assert!(msg.contains("expected `int`, found `string`"), "{}", msg);
    }

    #[test]
    fn a_generic_recursive_enum() {
        let src = "(defenum Tree<T> (Leaf) (Node Tree<T> T Tree<T>))\n\
            (defun size<T> ((t Tree<T>)) int (match t ((Leaf) 0) ((Node l _ r) (+ 1 (+ (size l) (size r))))))\n\
            (size (Tree::Node (Tree::Leaf) \"a\" (Tree::Node (Tree::Leaf) \"b\" (Tree::Leaf))))";
        assert_eq!(run(src).1, Value::Int(2));
    }

    /// A nested pattern covers only what it matches.
    #[test]
    fn a_match_must_cover_every_value() {
        let msg = check_err(&format!("{}(defun f ((s Shape)) int (match s ((Circle r) r) (Dot 0)))", SHAPE));
        assert!(msg.contains("`(Rect _ _)` is not covered"), "{}", msg);
        let src = format!("{}(defenum Box (Full Shape) Empty)\n(defun f ((b Box)) int (match b ((Full (Circle r)) r) (Empty 0)))", SHAPE);
        let msg = check_err(&src);
        assert!(msg.contains("`(Full (Rect _ _))` is not covered"), "{}", msg);
        let src = format!("{}(defenum Box (Full Shape) Empty)\n(defun f ((b Box)) int (match b ((Full (Circle r)) r) ((Full _) 1) (Empty 0)))", SHAPE);
        assert!(run(&format!("{}\n(f (Box::Full (Shape::Dot)))", src)).1 == Value::Int(1));
    }

    #[test]
    fn a_bare_name_in_an_arm_is_a_variant() {
        let msg = check_err(&format!("{}(defun f ((s Shape)) int (match s (Square 0) (_ 1)))", SHAPE));
        assert!(msg.contains("`Square` is not a variant of `Shape`"), "{}", msg);
    }

    /// `Option<int>` is represented by a niche; the pattern reads it the way
    /// the value is built.
    #[test]
    fn an_option_is_matched() {
        let src = "(defun get ((o Option<int>)) int (match o ((some x) x) ((none) 0)))\n";
        assert_eq!(run(&format!("{}(get (Option::some 3))", src)).1, Value::Int(3));
        assert_eq!(run(&format!("{}(get (Option::none))", src)).1, Value::Int(0));
    }

    #[test]
    fn a_static_method_through_its_type() {
        let src = format!(
            "{}(impl<T, U> Pair<T, U> (defmethod new ((a T) (b U)) Self (make-instance (quote Pair) :first a :second b)))\n\
             (second (Pair::new 1 \"x\"))",
            PAIR
        );
        assert_eq!(run(&src).2, Type::Str);
    }

    /// A number literal no type decides where it is written is decided by
    /// the rest of the form, as in Rust; with nothing deciding it, it is an
    /// `int`.
    #[test]
    fn a_number_literal_is_decided_by_its_uses() {
        let id8 = "(defun id8 ((x u8)) u8 x)\n";
        assert_eq!(run(&format!("{}(let ((x 1)) (id8 x))", id8)).2, Type::U8);
        let msg = check_err(&format!("{}(let ((x 300)) (id8 x))", id8));
        assert!(msg.contains("integer literal 300 is out of range for u8") && msg.contains(":2:10:"), "{}", msg);
        assert_eq!(run("(let ((x 1)) x)").2, Type::Int);
        // A receiver is decided at once, as its default: `int`'s `+`.
        assert_eq!(run("(+ 1 2)").2, Type::Int);
        let msg = check_err(&format!("{}(id8 (+ 1 2))", id8));
        assert!(msg.contains("expected `u8`, found `int`"), "{}", msg);
    }

    /// The first argument is checked once whatever the call turns out to
    /// be: a function whose name is also some type's method, nested in its
    /// own first argument, takes time in proportion to the depth.
    #[test]
    fn a_first_argument_is_checked_once() {
        let depth = 200;
        let src = format!(
            "(defstruct Cursor (next int))\n(defun next ((n int)) int n)\n{}0{}",
            "(next ".repeat(depth),
            ")".repeat(depth)
        );
        assert_eq!(run(&src).1, Value::Int(0));
    }

    /// `(use Type::*)` brings a type's variants in bare, for values and for
    /// patterns alike; `(use Type::variant)` brings one.
    #[test]
    fn use_brings_variants_in_bare() {
        let src = format!("{}(use Shape::*)\n(match (Rect 2 3) ((Rect w h) (* w h)) (_ 0))", SHAPE);
        assert_eq!(run(&src).1, Value::Int(6));
        let src = format!("{}(use Shape::Dot)\n(Dot)", SHAPE);
        assert_eq!(run(&src).2.to_string(), "Shape");
        let msg = check_err(&format!("{}(use Shape::Dot)\n(Circle 1)", SHAPE));
        assert!(msg.contains("no such function"), "{}", msg);
    }

    /// A bare name stands for one member: a `use` of another type's member
    /// under it is refused, unless it says it means to replace it.
    #[test]
    fn a_used_member_name_stands_for_one_member() {
        let two = format!("{}(defenum Light (Dot) Off)\n(use Shape::*)\n", SHAPE);
        let msg = check_err(&format!("{}(use Light::*)", two));
        assert!(msg.contains("`Dot` already stands for `Shape::Dot` here"), "{}", msg);
        assert_eq!(run(&format!("{}(shadowing-import Light::*)\n(Dot)", two)).2.to_string(), "Light");
    }

    #[test]
    fn a_use_of_nothing_is_a_module_not_loaded() {
        let msg = check_err("(use no::such::thing)");
        assert!(msg.contains("use: unresolved `no::such::thing`"), "{}", msg);
    }

    #[test]
    fn global_variables() {
        assert_eq!(run("(defvar (*n* int) 3)\n*n*").1, Value::Int(3));
        assert_eq!(run("(defconstant (k u8) 7)\nk").2, Type::U8);
        let msg = check_err("(defvar *n* 3)");
        assert!(msg.contains("a global needs a declared type"), "{}", msg);
        let msg = check_err("(defvar (*n* u8) 300)");
        assert!(msg.contains("integer literal 300 is out of range for u8"), "{}", msg);
    }

    /// A name has one meaning in its name space, and defining it again is
    /// what the redefinition policy says: here, an error.
    #[test]
    fn a_definition_takes_its_name_once() {
        let msg = check_err("(defun f () int 1)\n(defvar (f int) 2)");
        assert!(msg.contains("cannot define variable `f`: a function of that name already exists here"), "{}", msg);
        let msg = check_err("(defun f () int 1)\n(defun f () int 2)");
        assert!(msg.contains("function `f` is already defined"), "{}", msg);
        let msg = check_err(&format!("{}(defstruct Shape (x int))", SHAPE));
        assert!(msg.contains("type `Shape` is already defined"), "{}", msg);
        // `first` is already the field's accessor.
        let msg = check_err(&format!("{}(impl<T, U> Pair<T, U> (defmethod first (self) T self::first))", PAIR));
        assert!(msg.contains("method `first` is already defined"), "{}", msg);
    }

    #[test]
    fn the_checks_against_a_written_type() {
        assert_eq!(run("(the u8 3)").2, Type::U8);
        let msg = check_err("(the bool 3)");
        assert!(msg.contains("expected `bool`, found `int`"), "{}", msg);
    }

    /// `format` checks to a string, `println` to `()`. A number literal
    /// argument's type is the one the rest of the form decides.
    #[test]
    fn format_and_println_check() {
        assert_eq!(check_ok("(let ((x 1)) (format false \"~a, ~a and ~d\" \"s\" 2.5 x))"), Type::Str);
        assert_eq!(check_ok("(println \"~a\" (the u8 1))"), Type::Unit);
    }

    /// The directives are checked against the arguments' types.
    #[test]
    fn format_directives_take_their_arguments() {
        let msg = check_err("(format false \"~a ~a\" 1)");
        assert!(msg.contains("~a has no argument left to consume"), "{}", msg);
        let msg = check_err("(format false \"~{~a~}\" \"s\")");
        assert!(msg.contains("~{ needs a list"), "{}", msg);
        let msg = check_err("(let ((c \"~a\")) (println c 1))");
        assert!(msg.contains("the control string must be a literal"), "{}", msg);
    }

    /// A generic body is checked once: a `T` may be printed by `~a`.
    #[test]
    fn format_of_a_type_parameter() {
        assert_eq!(check_ok("(defun show<T> ((x T)) string (format false \"<~a>\" x))\n(show #\\q)"), Type::Str);
    }

    /// `setf` of a local or a global is the value assigned; a constant is
    /// not assigned.
    #[test]
    fn setf_assigns_a_variable() {
        assert_eq!(run("(let ((x 1)) (setf x 5) x)").1, Value::Int(5));
        assert_eq!(run("(defvar (g u8) 1)\n(setf g 7)\ng").1, Value::Int(7));
        let msg = check_err("(defconstant (c i32) 1)\n(setf c 2)");
        assert!(msg.contains("setf: cannot assign to constant `c`"), "{}", msg);
        let msg = check_err("(let ((x 1)) (setf x true))");
        assert!(msg.contains("expected `int`, found `bool`"), "{}", msg);
    }

    /// `setf` of a field goes through the struct's setter, at the
    /// receiver's type arguments.
    #[test]
    fn setf_assigns_a_field() {
        let src = "(defstruct Q<T> (y T))\n(let ((q (make-instance 'Q :y #\\a))) (setf q::y #\\b) q::y)";
        assert_eq!(run(src).1, Value::Char('b'));
        let msg = check_err("(defstruct P (x i32))\n(let ((p (make-instance 'P :x 1))) (setf p::z 2))");
        assert!(msg.contains("has no field `z`"), "{}", msg);
    }

    /// A loop's value is what leaves it; every exit gives one type, and a
    /// loop nothing leaves is `!`.
    #[test]
    fn a_loop_is_what_leaves_it() {
        assert_eq!(run("(let ((i 0)) (loop (if (= i 3) (return i) (setf i (+ i 1)))))").1, Value::Int(3));
        assert_eq!(run("(defun f () u8 (loop (return 4)))\n(f)").2, Type::U8);
        assert_eq!(check_ok("(the bool (loop ()))"), Type::Never);
        let msg = check_err("(loop (return \"a\") (return 1))");
        assert!(msg.contains("expected `string`, found `int`"), "{}", msg);
        let msg = check_err("(loop (return 1) (break))");
        assert!(msg.contains("branches have incompatible types: `int` vs `()`"), "{}", msg);
        let msg = check_err("(return 1)");
        assert!(msg.contains("return: not inside a loop"), "{}", msg);
    }

    /// `unwind-protect` is its protected form's value, the cleanup run after
    /// it however it leaves; the cleanup's own value is dropped.
    #[test]
    fn unwind_protect_is_its_protected_value() {
        let src = "(let ((n 0)) (let ((r (loop (unwind-protect (return 5) (setf n (+ n 1)))))) (+ r n)))";
        assert_eq!(run(src).1, Value::Int(6));
        assert_eq!(run("(defun f () u8 (unwind-protect 3 \"done\"))\n(f)").2, Type::U8);
        let msg = check_err("(unwind-protect 1)");
        assert!(msg.contains("unwind-protect: (unwind-protect protected cleanup)"), "{}", msg);
    }

    /// `panic` never returns, so it is any type a branch needs.
    #[test]
    fn panic_never_returns() {
        assert_eq!(check_ok("(if true 1 (panic \"no\"))"), Type::Int);
        let msg = check_err("(panic 1)");
        assert!(msg.contains("expected `string`, found `int`"), "{}", msg);
    }

    /// `list` is an `Option<Sexpr>`, each item held as a `Sexpr`.
    #[test]
    fn list_holds_its_items_as_sexprs() {
        assert_eq!(check_ok("(list 1 \"a\" 'b (list))"), crate::check::analyze::option_of_sexpr());
    }

    /// `as` converts through the source type's method, a narrower width
    /// after it; `try-as` is `none` where the value would be lost.
    #[test]
    fn as_converts_between_numbers() {
        assert_eq!(run("(as int 2.7)").1, Value::Int(2));
        assert_eq!(run("(as u8 300)").2, Type::U8);
        assert_eq!(run("(try-as u8 30)").2, Type::Named(Path::builtin(typelisp_mem::wk::OPTION), vec![Type::U8]));
        let msg = check_err("(as string 1)");
        assert!(msg.contains("as: no conversion from `int` to `string`"), "{}", msg);
    }

    /// A call leaves out an `&optional` or `&key` parameter for its
    /// default, or `none` when it has none; `&rest` takes the arguments
    /// left as one list.
    #[test]
    fn optional_key_and_rest_parameters() {
        assert_eq!(run("(defun f ((a int) &optional (b int 10)) int (+ a b))\n(f 1)").1, Value::Int(11));
        assert_eq!(run("(defun f ((a int) &optional (b int 10)) int (+ a b))\n(f 1 5)").1, Value::Int(6));
        assert_eq!(run("(defun f (&key (x int 0) (y int 0)) int (+ x (* 10 y)))\n(f :y 4 :x 3)").1, Value::Int(43));
        assert_eq!(run("(defun f (&key (x int 0) (y int 0)) int (+ x (* 10 y)))\n(f)").1, Value::Int(0));
        assert_eq!(run("(defun f ((a int) &rest (xs int)) int a)\n(f 1 2 3)").1, Value::Int(1));
        let msg = check_err("(defun f (&key (x int 0)) int x)\n(f :y 1)");
        assert!(msg.contains("unknown keyword argument :y"), "{}", msg);
        let msg = check_err("(defun f (&key (x int 0)) int x)\n(f :x 1 :x 2)");
        assert!(msg.contains("duplicate keyword argument :x"), "{}", msg);
        let msg = check_err("(defun f ((a int) &optional (b int 1)) int a)\n(f 1 2 3)");
        assert!(msg.contains("expected at most 2 argument(s), got 3"), "{}", msg);
        let msg = check_err("(defun f (&optional (a int 1) &key (b int 2)) int a)");
        assert!(msg.contains("&key cannot be combined with &optional/&rest"), "{}", msg);
        let msg = check_err("(defun f<T> (&key (x Option<T> (Option::none))) Option<T> x)");
        assert!(msg.contains("may not default when its type mentions the function's own type parameter"), "{}", msg);
    }

    /// A method takes `&optional`, `&rest` and `&key` parameters as a
    /// function does; a trait method may not, its arity being the trait's.
    #[test]
    fn method_parameter_sections() {
        let src = "(defstruct thing (n int))
(impl thing
  (pub defmethod add (self &optional (k int 1) &rest (xs int)) int (+ self::n k))
  (pub defmethod make (&key (n int 0)) thing (make-instance 'thing :n n)))
(defmethod twice ((t thing) &key (by int 2)) int (* t::n by))
(let ((x (thing::make :n 5))) (+ (+ (add x) (add x 10 1 2)) (+ (twice x) (twice x :by 3))))";
        assert_eq!(run(src).1, Value::Int(46));
        let msg = check_err("(deftrait greet () (defmethod hello (self) int))
(defstruct thing (n int))
(impl greet thing (defmethod hello (self &key (extra int 0)) int (+ self::n extra)))");
        assert!(msg.contains("arity is fixed by its vtable slot"), "{}", msg);
    }

    /// A `lambda`, a `labels` function and a named function are values of
    /// a function type, which a call applies.
    #[test]
    fn function_values() {
        let twice = "(defun twice ((f (fn (int) int)) (x int)) int (f (f x)))\n";
        assert_eq!(run(&format!("{}(twice (lambda ((n int)) int (* n 3)) 2)", twice)).1, Value::Int(18));
        assert_eq!(run(&format!("{}(defun inc ((n int)) int (+ n 1))\n(twice inc 5)", twice)).1, Value::Int(7));
        assert_eq!(run("(let ((k 10)) (let ((f (lambda ((n int)) int (+ n k)))) (f 1)))").1, Value::Int(11));
        assert_eq!(run("(apply (lambda ((a int) &rest (xs int)) int a) 7 '(1 2))").1, Value::Int(7));
        let src = "(labels ((ev ((n int)) bool (if (= n 0) true (od (- n 1)))) (od ((n int)) bool (if (= n 0) false (ev (- n 1))))) (ev 10))";
        assert_eq!(run(src).1, Value::Bool(true));
        let msg = check_err("((lambda ((a int)) int a) 1 2)");
        assert!(msg.contains("function expects 1 argument(s), got 2"), "{}", msg);
        let msg = check_err("(let ((x 1)) (x 2))");
        assert!(msg.contains("value is not callable: `int`"), "{}", msg);
        let msg = check_err("(lambda (&optional (a int 1)) int a)");
        assert!(msg.contains("&optional/&key need a named callee"), "{}", msg);
        let msg = check_err("(loop (let ((f (lambda () int (return 1)))) (break)))");
        assert!(msg.contains("return: not inside a loop"), "{}", msg);
    }

    /// Call sugar is checked as the form it stands for, and `let*` binds in
    /// order.
    #[test]
    fn call_sugar_and_let_star() {
        assert_eq!(run("(+ 1 2 3 4)").1, Value::Int(10));
        assert_eq!(run("(- 5)").1, Value::Int(-5));
        assert_eq!(run("(+)").1, Value::Int(0));
        assert_eq!(run("(let* ((a 1) (b (+ a 1))) (* b 10))").1, Value::Int(20));
        let msg = check_err("(-)");
        assert!(msg.contains("-: requires at least 1 argument"), "{}", msg);
        let msg = check_err("(let* ((a 1) (b)) a)");
        assert!(msg.contains("let*: binding must be (name val)"), "{}", msg);
    }

    /// `compile` and the other tools name one body: a function, a macro or
    /// a method, never a generic one.
    #[test]
    fn tools_name_one_body() {
        assert_eq!(check_ok("(defun sq ((n int)) int (* n n))\n(compile sq)"), Type::Bool);
        let msg = check_err("(defun id<T> ((x T)) T x)\n(compile id)");
        assert!(msg.contains("compile: `id` is generic"), "{}", msg);
        let msg = check_err("(compile nope)");
        assert!(msg.contains("compile: no function `nope` is visible from here"), "{}", msg);
        let msg = check_err("(defvar (v int) 1)\n(compile v)");
        assert!(msg.contains("compile: `v` is a variable, not a function"), "{}", msg);
    }

    /// A generic body calls the methods of its type parameters' bounds, a
    /// supertrait's too; a call's type arguments must meet the bounds.
    #[test]
    fn where_bounds() {
        let traits = "(deftrait shape () (defmethod area (self) int))
(deftrait named (shape) (defmethod label (self) string))
(defstruct sq (n int))
(impl shape sq (defmethod area (self) int (* self::n self::n)))
(impl named sq (defmethod label (self) string \"square\"))
(defun total<T> ((a T) (b T)) int (:where (shape T)) (+ (area a) (area b)))
(defun via-named<T> ((a T)) int (:where (named T)) (area a))
";
        assert_eq!(run(&format!("{}(total (make-instance 'sq :n 3) (make-instance 'sq :n 4))", traits)).1, Value::Int(25));
        assert_eq!(run(&format!("{}(via-named (make-instance 'sq :n 3))", traits)).1, Value::Int(9));
        let msg = check_err(&format!("{}(total 1 2)", traits));
        assert!(msg.contains("type `int` does not implement trait `shape` required by `:where` clause on type parameter `T`"), "{}", msg);
        let msg = check_err(&format!("{}(defun outer<U> ((u U)) int (total u u))", traits));
        assert!(msg.contains("only known here as the enclosing function's own `U`"), "{}", msg);
        let msg = check_err(&format!("{}(defun f<T> ((a T)) int (:where (shape U)) 1)", traits));
        assert!(msg.contains("`U` is not a type parameter this definition declares"), "{}", msg);
    }

    /// A bound's pin agrees with what the `impl` binds the associated type
    /// to, which decides the pin's type parameter.
    #[test]
    fn pinned_associated_types() {
        let src = "(deftrait source () (:type Item) (defmethod take (self) Item))
(defstruct ints (n int))
(impl source ints (:type Item int) (defmethod take (self) int self::n))
(defstruct box<T> (v T))
(impl<T> source box<T> (:type Item T) (defmethod take (self) T self::v))
(defun grab<S, A> ((s S)) A (:where (source S (Item A))) (take s))
(defun grab2<S, A> ((s S)) A (:where (source S (Item A))) (grab s))
(defun grab-str<S> ((s S)) string (:where (source S (Item string))) (take s))
";
        assert_eq!(run(&format!("{}(+ 1 (grab (make-instance 'ints :n 41)))", src)).1, Value::Int(42));
        assert_eq!(run(&format!("{}(grab2 (make-instance 'box :v true))", src)).1, Value::Bool(true));
        let msg = check_err(&format!("{}(grab-str (make-instance 'ints :n 41))", src));
        assert!(msg.contains("type `ints`'s associated type `Item` is `int`, but `:where` clause requires `string`"), "{}", msg);
    }

    /// A symbol macro is its form where the name is written, a place too;
    /// a binding inside hides it.
    #[test]
    fn symbol_macros() {
        let src = "(defstruct p (x int))\n(defun f () int (let ((v (make-instance 'p :x 1))) (symbol-macrolet ((px v::x)) (setf px 5) (+ px (let ((px 10)) px)))))\n(f)";
        assert_eq!(run(src).1, Value::Int(15));
    }

    /// `task` and `thread` take a call, and nothing else.
    #[test]
    fn spawning() {
        let msg = check_err("(task (if true 1 2))");
        assert!(msg.contains("`task` cannot start `if`"), "{}", msg);
        let msg = check_err("(thread 5)");
        assert!(msg.contains("`thread` takes a call form"), "{}", msg);
    }

    /// A supertrait's associated types are pinned in the subtrait's
    /// parameters, and an `impl` of the subtrait needs the supertrait's to
    /// bind them so.
    #[test]
    fn pinned_supertraits() {
        let src = "(deftrait src () (:type Item) (defmethod take (self) Item))
(deftrait twice ((src (Item Out))) (:type Out) (defmethod two (self) Out))
(defstruct b (n int))
(impl src b (:type Item int) (defmethod take (self) int self::n))
";
        let ok = format!("{}(impl twice b (:type Out int) (defmethod two (self) int (* 2 (take self))))\n(defun g<T> ((x T)) int (:where (twice T (Out int))) (+ (take x) (two x)))\n", src);
        assert_eq!(run(&format!("{}(g (make-instance 'b :n 5))", ok)).1, Value::Int(15));
        let msg = check_err(&format!("{}(impl twice b (:type Out bool) (defmethod two (self) bool true))", src));
        assert!(msg.contains("`twice` requires `src`'s `Item = bool`, but `b`'s `impl src` binds it to `int`"), "{}", msg);
    }

    /// A literal of the value's own type is matched by its word, any other
    /// value by `equals` (the prelude's, not loaded here); `true` and
    /// `false` cover a `bool`.
    #[test]
    fn value_patterns() {
        assert_eq!(run("(defun n ((x i32)) int (match x (0 10) (1 11) (_ 12)))\n(n 1)").1, Value::Int(11));
        assert_eq!(run("(defun b ((x bool)) int (match x (true 1) (false 0)))\n(b false)").1, Value::Int(0));
        let msg = check_err("(defun b ((x bool)) int (match x (true 1)))");
        assert!(msg.contains("`false` is not covered"), "{}", msg);
        let msg = check_err("(defstruct p (k int))\n(defun s ((x p)) int (match x (1 1) (_ 0)))");
        assert!(msg.contains("cannot be compared by value — its type `p` does not implement `Eq`"), "{}", msg);
    }

    /// A value is boxed as a trait object where one is expected, and its
    /// methods are called through the table; a trait object of a subtrait
    /// is one of its supertraits.
    #[test]
    fn trait_objects() {
        let src = "(deftrait shape () (defmethod area (self) int))
(deftrait solid (shape) (defmethod vol (self) int))
(defstruct sq (n int))
(defstruct cube (n int))
(impl shape sq (defmethod area (self) int (* self::n self::n)))
(impl shape cube (defmethod area (self) int (* 6 self::n self::n)))
(impl solid cube (defmethod vol (self) int (* self::n self::n self::n)))
(defun a ((s :dyn shape)) int (area s))
(defun up ((s :dyn solid)) :dyn shape s)
(defun g<T> ((x T)) :dyn shape (:where (shape T)) (as :dyn shape x))
";
        assert_eq!(run(&format!("{}(a (make-instance 'sq :n 3))", src)).1, Value::Int(9));
        assert_eq!(run(&format!("{}(a (up (make-instance 'cube :n 1)))", src)).1, Value::Int(6));
        assert_eq!(run(&format!("{}(vol (the :dyn solid (make-instance 'cube :n 2)))", src)).1, Value::Int(8));
        assert_eq!(run(&format!("{}(a (g (make-instance 'sq :n 4)))", src)).1, Value::Int(16));
        let msg = check_err(&format!("{}(defstruct ci (n int))\n(a (make-instance 'ci :n 1))", src));
        assert!(msg.contains("`ci` does not implement `shape`, so it cannot be used as `:dyn shape`"), "{}", msg);
        let msg = check_err(&format!("{}(defun b ((s :dyn shape)) :dyn solid s)", src));
        assert!(msg.contains("`shape` does not inherit `solid`"), "{}", msg);
        let msg = check_err("(deftrait a () (defmethod m (self) int))\n(deftrait b () (defmethod m (self) int))\n(deftrait c (a b))");
        assert!(msg.contains("method `m` is inherited from both `a` and `b`"), "{}", msg);
    }

    /// An `impl` that writes no method the type already has adopts it,
    /// when the signature is the trait's; a method for one instantiation is
    /// that instantiation's.
    #[test]
    fn adopted_and_instantiation_methods() {
        let src = "(deftrait sized () (defmethod size (self) int))
(defstruct bag (n int))
(impl bag (pub defmethod size (self) int self::n))
(impl sized bag)
(defun sz<T> ((x T)) int (:where (sized T)) (size x))
";
        assert_eq!(run(&format!("{}(sz (make-instance 'bag :n 3))", src)).1, Value::Int(3));
        let msg = check_err("(deftrait sized () (defmethod size (self) int))\n(defstruct bag (n int))\n(impl bag (pub defmethod size (self) bool true))\n(impl sized bag)");
        assert!(msg.contains("method `size` is `(bag) bool`, but `sized` declares `(bag) int`"), "{}", msg);
        let src = "(defstruct two<A,B> (a A) (b B))
(impl two<int,int> (pub defmethod sum (self) int (+ self::a self::b)))
";
        assert_eq!(run(&format!("{}(sum (the two<int,int> (make-instance 'two :a 1 :b 2)))", src)).1, Value::Int(3));
    }

    /// A `Sexpr` is matched by its variants; an `Option<Sexpr>` by them and
    /// `none` in one list, which must cover both.
    #[test]
    fn sexpr_patterns() {
        let f = "(defun k ((x Option<Sexpr>)) int (match x ((cons a _) 1) ((int n) n) ((none) 0) (_ 9)))\n";
        assert_eq!(run(&format!("{}(k (quote (1)))", f)).1, Value::Int(1));
        assert_eq!(run(&format!("{}(k 5)", f)).1, Value::Int(5));
        assert_eq!(run(&format!("{}(k ())", f)).1, Value::Int(0));
        assert_eq!(run(&format!("{}(k (quote a))", f)).1, Value::Int(9));
        assert_eq!(run("(defun m ((x Option<Sexpr>)) int (match x ((some (int n)) n) ((some _) 1) ((none) 0)))\n(m 4)").1, Value::Int(4));
        let msg = check_err("(defun k ((x Option<Sexpr>)) int (match x ((some _) 1)))");
        assert!(msg.contains("`(none)` is not covered"), "{}", msg);
        let msg = check_err("(defun k ((x Sexpr)) int (match x ((cons a _) 1)))");
        assert!(msg.contains("`(int _)` is not covered"), "{}", msg);
        let msg = check_err("(defun k ((x Option<Sexpr>)) int (match x ((nil) 1) (_ 0)))");
        assert!(msg.contains("`nil` is no longer a `Sexpr` constructor"), "{}", msg);
    }

    /// A documentation string is kept, not checked as the body; `()` where
    /// an `Option` is expected is `none`; an `Option<Sexpr>` has `Sexpr`'s
    /// methods.
    #[test]
    fn docs_none_and_sexpr_receivers() {
        assert_eq!(run("(defun f () int \"one\" 1)\n(f)").1, Value::Int(1));
        assert_eq!(run("(defvar (v int) 2 \"two\")\nv").1, Value::Int(2));
        assert_eq!(run("(defun g ((o Option<int>)) int (match o ((some x) x) (none 0)))\n(g ())").1, Value::Int(0));
        assert_eq!(run("(eq (quote a) (quote a))").1, Value::Bool(true));
        let msg = check_err("(defun f () string \"doc\" 1)");
        assert!(msg.contains("expected `string`, found `int`"), "{}", msg);
    }

    /// A trait's default body is checked once at the `deftrait`, and an
    /// `impl` that leaves the method out has it at its own types.
    #[test]
    fn default_bodies() {
        let src = "(deftrait counted () (:type Item) (defmethod item (self) Item) (defmethod n (self) int) \
(defmethod twice (self) int (* 2 (n self))) (defmethod first (self) Option<Item> (some (item self))))
(defstruct c (k int))
(impl counted c (:type Item bool) (defmethod item (self) bool true) (defmethod n (self) int self::k))
(defstruct w<T> (v T))
(impl<T> counted w<T> (:type Item T) (defmethod item (self) T self::v) (defmethod n (self) int 5) (defmethod twice (self) int 0))
(defun t2<X> ((x X)) int (:where (counted X)) (twice x))
";
        assert_eq!(run(&format!("{}(twice (make-instance 'c :k 21))", src)).1, Value::Int(42));
        assert_eq!(run(&format!("{}(t2 (make-instance 'c :k 4))", src)).1, Value::Int(8));
        assert_eq!(run(&format!("{}(t2 (make-instance 'w :v 1))", src)).1, Value::Int(0));
        assert_eq!(run(&format!("{}(match (first (make-instance 'w :v 7)) ((some v) v) (none 0))", src)).1, Value::Int(7));
        let msg = check_err("(deftrait a () (defmethod n (self) int) (defmethod m (self) string (n self)))");
        assert!(msg.contains("expected `string`, found `int`"), "{}", msg);
        let msg = check_err("(deftrait a () (defmethod n (self) int) (defmethod m (self) int (n self)))\n(defstruct d (k int))\n(impl a d)");
        assert!(msg.contains("missing method `n`"), "{}", msg);
    }

    /// An `impl`'s `(:where ...)` bounds every method's type parameters; a
    /// trait's `impl` with one is an `impl` for the arguments meeting it.
    #[test]
    fn method_and_impl_bounds() {
        let src = "(deftrait shape () (defmethod area (self) int))
(defstruct sq (n int))
(impl shape sq (defmethod area (self) int (* self::n self::n)))
(defstruct pair<T> (a T) (b T))
(impl<T> pair<T> (:where (shape T)) (pub defmethod sum (self) int (+ (area self::a) (area self::b))))
(impl<T> shape pair<T> (:where (shape T)) (defmethod area (self) int (sum self)))
(defmethod bigger<U> ((p pair<U>)) int (:where (shape U)) (* 2 (sum p)))
(defun total<T> ((x T)) int (:where (shape T)) (area x))
";
        let p = "(make-instance 'pair :a (make-instance 'sq :n 2) :b (make-instance 'sq :n 3))";
        assert_eq!(run(&format!("{}(+ (sum {p}) (+ (bigger {p}) (total {p})))", src, p = p)).1, Value::Int(52));
        let msg = check_err(&format!("{}(total (make-instance 'pair :a 1 :b 2))", src));
        assert!(msg.contains("type `pair<int>` does not implement trait `shape`"), "{}", msg);
        let msg = check_err(&format!("{}(sum (make-instance 'pair :a 1 :b 2))", src));
        assert!(msg.contains("pair::sum: type `int` does not implement trait `shape`"), "{}", msg);
    }

    /// A macro's parameters and result are S-expression data; a quasiquote
    /// builds it, `,x` putting a value in.
    #[test]
    fn defmacro_and_quasiquote() {
        assert_eq!(run("(defmacro twice (x) `(+ ,x ,x))\n(twice 4)").1, Value::Int(8));
        assert_eq!(run("(defmacro opt (a &optional (b '2)) `(* ,a ,b))\n(opt 5)").1, Value::Int(10));
        let msg = check_err("(defun f () int 1)\n(defmacro f (x) x)");
        assert!(msg.contains("cannot define macro `f`: a function of that name already exists here"), "{}", msg);
        let msg = check_err("(defmacro f (x &rest) x)");
        assert!(msg.contains("defmacro: &rest must be followed by exactly one parameter name"), "{}", msg);
    }

    /// A quoted symbol is a `Symbol`; any other datum is `Option<Sexpr>`.
    #[test]
    fn quote_types_its_datum() {
        assert_eq!(check_ok("'foo"), Type::Symbol);
        assert_eq!(check_ok("'(1 2)"), crate::check::analyze::option_of_sexpr());
        assert_eq!(check_ok("'()"), crate::check::analyze::option_of_sexpr());
        let msg = check_err("(quote a b)");
        assert!(msg.contains("quote: (quote datum)"), "{}", msg);
    }

    /// Where a `Sexpr` is expected, a value with an encoding is widened
    /// into one; an `Option<Sexpr>` is not narrowed. An `int` and a `ratio`
    /// widen to `rational`.
    #[test]
    fn widening_to_what_is_expected() {
        let sexpr = Type::Named(Path::builtin(typelisp_mem::wk::SEXPR), Vec::new());
        assert_eq!(check_ok("(the Sexpr 'a)"), sexpr);
        assert_eq!(check_ok("(the Sexpr \"s\")"), sexpr);
        assert_eq!(check_ok("(defstruct P (x i32))\n(the Sexpr (make-instance 'P :x 1))"), sexpr);
        let msg = check_err("(the Sexpr '(1 2))");
        assert!(msg.contains("type mismatch") && msg.contains("Sexpr`, found `") && msg.contains("Option<"), "{}", msg);
        assert_eq!(run("(defun r ((x rational)) rational x)\n(let ((x 3)) (r x))").1, Value::Int(3));
        assert_eq!(check_ok("(the rational 1/2)"), Type::Rational);
    }

    /// A trait's methods, given bodies by an `impl`, are the type's: an
    /// associated type's name in a method's signature is the type the
    /// `impl` binds it to.
    #[test]
    fn a_trait_implemented_for_a_type() {
        let src = "(deftrait Item () (:type Out) (defmethod item (self) Out))\n\
                   (defstruct R (n u8))\n\
                   (impl Item R (:type Out u8) (defmethod item (self) Out self::n))\n\
                   (item (make-instance 'R :n 7))";
        let (_, v, ty) = run(src);
        assert_eq!((v, ty), (Value::Int(7), Type::U8));
        let src = "(deftrait G () (defmethod g (self) i32))\n(defstruct W<T> (v T))\n\
                   (impl<T> G W<T> (defmethod g (self) i32 1))\n(g (make-instance 'W :v #\\a))";
        assert_eq!(run(src).1, Value::Int(1));
    }

    /// An `impl` gives the trait's methods with the trait's signatures, and
    /// only those.
    #[test]
    fn an_impl_is_what_its_trait_declares() {
        let pre = "(deftrait G () (defmethod g (self) i32))\n(defstruct R (n i32))\n";
        let msg = check_err(&format!("{}(impl G R (defmethod g (self) bool true))", pre));
        assert!(msg.contains("method `g` is `") && msg.contains("but `G` declares `"), "{}", msg);
        let msg = check_err(&format!("{}(impl G R)", pre));
        assert!(msg.contains("missing method `g`"), "{}", msg);
        let msg = check_err(&format!("{}(impl G R (defmethod g (self) i32 1) (defmethod h (self) i32 2))", pre));
        assert!(msg.contains("`h` is not a method of `G`"), "{}", msg);
        let msg = check_err("(deftrait G () (defmethod g (self) i32))\n(deftrait H (G))\n(defstruct R (n i32))\n(impl H R)");
        assert!(msg.contains("`H` requires `G`"), "{}", msg);
    }

    /// A type may be written as a function's type, a path, a list or a
    /// tuple.
    #[test]
    fn the_spellings_of_a_type() {
        assert_eq!(check_ok("(defun f ((g (fn (i32 &rest u8) bool)) (p #{i32 string})) i32 1)\n2"), Type::Int);
        assert_eq!(check_ok("(defun h ((v std::vec::Vector<u8>)) u8 1)\n(h (the (Vector u8) (Vector::new)))"), Type::U8);
        let msg = check_err("(defun f ((g (fn i32))) i32 1)");
        assert!(msg.contains("fn type must be (fn (params) ret)"), "{}", msg);
        let msg = check_err("(defun f ((v (Vector))) i32 1)");
        assert!(msg.contains("a type application must be (Name Arg...)"), "{}", msg);
    }

    /// A module's forms are checked in it; what it does not make `pub` is
    /// not there for another module.
    #[test]
    fn a_module_and_what_it_makes_pub() {
        let src = "(module geo (pub defun area ((w u8) (h u8)) u8 (* w h)) (pub defvar (unit u8) 1))\n(geo::area geo::unit 3)";
        let (_, v, ty) = run(src);
        assert_eq!((v, ty), (Value::Int(3), Type::U8));
        let msg = check_err("(module geo (defun hidden () i32 1))\n(geo::hidden)");
        assert!(msg.contains("unresolved path: geo::hidden"), "{}", msg);
        let msg = check_err("(pub impl foo)");
        assert!(msg.contains("pub: expected defun"), "{}", msg);
    }

    /// A definition whose checking fails is not kept (plan 4.4): calling it
    /// afterwards is not a call to anything this checker defined.
    #[test]
    fn a_failed_definition_is_not_kept() {
        let mut heap = Heap::with_capacity(1 << 16);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        let mut forms = reader.forms("(defun f () int true)");
        let (v, loc) = forms.next_form(&mut heap).unwrap().unwrap();
        heap.push_root(v);
        assert!(checker.check_form(&mut heap, &crate::check::Registry::with_builtins(), &interp, ROOT_SITE, v, Some(loc)).is_err());
        let f = crate::types::Path::of_symbol(NsId::ROOT, typelisp_mem::symbols::intern_in(NsId::ROOT, "f"));
        assert!(checker.reg.get().expect("made by the check").fn_def(&f).is_none());
    }

    /// The rebuilt checker checks the core macro layer and the prelude, and
    /// what it makes of them runs: what the prelude's dump takes its generic
    /// bodies from (`prelude_bodies`).
    #[test]
    fn the_prelude_checks_and_runs() {
        let mut heap = Heap::with_capacity(1 << 20);
        let reader = Reader::new();
        let interp = Interp::new();
        let mut checker = TypeChecker::default();
        let old = crate::check::Registry::with_builtins();
        old.publish_names(&mut heap.names());
        let site = Site { ns: NsId::STD, file: None, policy: crate::check::RedefPolicy::Error };
        for src in [crate::core_macros::SOURCE, crate::prelude::SOURCE.as_str()] {
            let mut forms = reader.forms_within(crate::LIBRARY_FILE, src, NsId::STD);
            while let Some((v, loc)) = forms.next_form(&mut heap).expect("read failed") {
                heap.push_root(v);
                let form = checker.check_form(&mut heap, &old, &interp, site, v, Some(loc.clone())).unwrap_or_else(|e| panic!("{:?}: {:?}", loc, e));
                heap.push_root(form);
                interp.exec(&mut heap, form).unwrap_or_else(|e| panic!("{:?}: {:?}", loc, e));
            }
        }
        // Its bodies, written down and read back, are all there.
        let reg = checker.reg.get().expect("made by the checks");
        let written = bincode::serialize(&RebuiltBodies::of(&heap, reg).expect("the bodies are written")).expect("the bodies serialize");
        let read: RebuiltBodies = bincode::deserialize(&written).expect("the bodies deserialize");
        let mut fresh = Registry::default();
        read.restore(&mut heap).expect("the bodies are read").adopt(&mut fresh);
        assert!(reg.templates().count() > 0);
        assert_eq!(fresh.templates().count(), reg.templates().count());
        assert_eq!(fresh.all_defaulted().count(), reg.all_defaulted().count());
    }
}
