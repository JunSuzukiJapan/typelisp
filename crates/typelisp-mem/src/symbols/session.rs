//! `SessionNames`: what names mean in one session.

use std::collections::{HashMap, HashSet};
use super::{table, Import, NOT_WELL_KNOWN, NsId, PathStep, SymRef};

/// What names mean in one session: which modules the session has, which
/// names are types and what their members are, and what each `use` brought
/// in. The symbols themselves, and the tree of module names they belong to,
/// are the process's (see the module documentation); a name means something
/// only in the session that defined it. One per heap (`Heap::names`).
#[derive(Debug)]
pub struct SessionNames {
    /// The modules this session has: the fixed ones, every module a form was
    /// read in, every `(module ...)` read or checked, and their ancestors.
    pub(super) modules: HashSet<NsId>,
    /// The types this session has, by their names' symbols, each with the
    /// names of its members (constructors, field accessors, static methods,
    /// methods): member name -> the member's symbol. A type is not a module;
    /// this is what `Type::member` is read against.
    types: HashMap<SymRef, HashMap<Box<str>, SymRef>>,
    /// The module each type in [`Self::types`] is defined in. A type's name
    /// need not be its module's own symbol — a built-in type at the root is
    /// named by the symbol `std` offers — so the symbol alone does not say.
    type_homes: HashMap<SymRef, NsId>,
    /// What `use` brought into a module, by the module and the symbol it was
    /// brought in under (that module's own).
    pub(super) imports: HashMap<(NsId, SymRef), Import>,
    /// What every module reads a name as without bringing it in: the
    /// standard library's definitions, by name. Consulted by [`Self::read`]
    /// after what the module brought in itself.
    exports: HashMap<&'static str, SymRef>,
    /// The symbols a `(shadow ...)` made their modules' own ([`Self::shadow`]).
    shadowed: HashSet<SymRef>,
    /// The module of the top-level form being checked and run — CL's
    /// `*package*`: what a `read` at run time reads its names in.
    current: NsId,
}

impl SessionNames {
    /// A session with the fixed modules and nothing defined.
    pub fn new() -> SessionNames {
        let mut names = SessionNames {
            modules: HashSet::new(),
            types: HashMap::new(),
            type_homes: HashMap::new(),
            imports: HashMap::new(),
            exports: HashMap::new(),
            shadowed: HashSet::new(),
            current: NsId::ROOT,
        };
        for ns in [NsId::ROOT, NsId::KEYWORD, NsId::STD, NsId::CORE] {
            names.modules.insert(ns);
        }
        names
    }

    /// The module a `read` at run time reads its names in: the one the last
    /// [`Self::enter_module`] named.
    pub fn current_module(&self) -> NsId {
        self.current
    }

    /// Makes `ns` the module of the top-level form about to be checked and
    /// run.
    pub fn enter_module(&mut self, ns: NsId) {
        self.current = ns;
    }

    /// Records `ns`, and every module it is inside, as modules of this
    /// session.
    pub fn declare_module(&mut self, ns: NsId) {
        let t = table();
        let mut cur = Some(ns);
        while let Some(id) = cur {
            if !self.modules.insert(id) {
                break;
            }
            cur = t.nodes[id.0 as usize].parent;
        }
    }

    /// Records `ty` — the name of a type `home` defines — as a type, so a
    /// path can step into it (`ty::member`).
    pub fn declare_type(&mut self, home: NsId, ty: SymRef) {
        self.types.entry(ty).or_default();
        let prev = *self.type_homes.entry(ty).or_insert(home);
        assert!(prev == home, "type `{}` declared in two modules: {:?} and {:?}", ty.name(), prev, home);
    }

    /// The module that defines the type named `ty` ([`Self::declare_type`]).
    pub fn type_home(&self, ty: SymRef) -> Option<NsId> {
        self.type_homes.get(&ty).copied()
    }

    /// Records `member` as one of type `ty`'s members, under its name.
    pub fn declare_member(&mut self, ty: SymRef, member: SymRef) {
        self.types.entry(ty).or_default().insert(member.name().into(), member);
    }

    /// Records that `local` — `into`'s own symbol — stands for `what` in
    /// `into`, as a `use` there says.
    pub fn import_name(&mut self, into: NsId, local: SymRef, what: Import) {
        self.imports.insert((into, local), what);
    }

    /// Registers what a `use` written in `ns` brings in, when its path —
    /// `segments`, as written — leads to a module or a type this session
    /// has: under the path's last segment, read in `ns`. `false` when it
    /// leads to neither (a definition, or a module not loaded yet), which
    /// the `use` registers once it has resolved the path itself.
    pub fn import_written(&mut self, ns: NsId, segments: &[&str]) -> bool {
        let Some(last) = segments.last() else { return false };
        let (local, what) = {
            let mut t = table();
            let mut at: Option<PathStep> = None;
            for (i, seg) in segments.iter().enumerate() {
                let here = match at {
                    None if i == 0 && seg.is_empty() => {
                        at = Some(PathStep::Module(NsId::ROOT));
                        continue;
                    }
                    None => t.first_step(self, ns, seg),
                    Some(PathStep::Module(m)) => t.step_down(self, m, seg),
                    Some(PathStep::Type(_)) => None,
                };
                match here {
                    Some((_, step)) => at = Some(step),
                    None => return false,
                }
            }
            let what = match at {
                Some(PathStep::Module(id)) => Import::Module(id),
                Some(PathStep::Type(ty)) => Import::Type(ty),
                None => return false,
            };
            (t.read(ns, last), what)
        };
        self.import_name(ns, local, what);
        true
    }

    /// Makes every module read `sym`'s name as `sym` ([`Self::read`]).
    ///
    /// Panics if another symbol of the same name is already exported: one
    /// name read in a module is one symbol, so the library cannot offer two.
    pub fn export(&mut self, sym: SymRef) {
        let prev = *self.exports.entry(sym.name()).or_insert(sym);
        assert!(prev == sym, "two definitions offered to every module as `{}`: {:?} and {:?}", sym.name(), prev, sym);
    }

    /// Whether every module reads `sym`'s name as `sym` ([`Self::export`]).
    pub fn offers(&self, sym: SymRef) -> bool {
        self.exports.get(sym.name()) == Some(&sym)
    }

    /// The type `sym` names, when the standard library offers it to every
    /// module ([`Self::export`]): a path's first segment reaches it from any
    /// module, as the bare name does, though the type is defined elsewhere.
    pub(super) fn offered_type(&self, sym: SymRef) -> Option<PathStep> {
        (self.offers(sym) && self.type_home(sym).is_some()).then_some(PathStep::Type(sym))
    }

    /// Makes `ns` read `name` as its own symbol from now on, in front of what
    /// the standard library offers every module under it — what
    /// `(shadow name)` written in `ns` says. The symbol is the one `ns`
    /// already owns under the name, else a new one.
    ///
    /// A reserved word cannot be shadowed: every module holds the same one,
    /// which is what lets a definition form mean the same thing everywhere.
    pub fn shadow(&mut self, ns: NsId, name: &str) -> Result<SymRef, String> {
        let mut t = table();
        let own = match t.lookup(ns, name) {
            Some(s) if s.home() == NsId::SYSTEM => {
                return Err(format!("shadow: `{}` is a reserved word, which every module shares", name));
            }
            Some(s) if s.home() == ns => s,
            _ => t.create(ns, name, NOT_WELL_KNOWN),
        };
        self.shadowed.insert(own);
        Ok(own)
    }

    /// What the name `name`, written in `ns`, reads as: a member of the fixed
    /// vocabulary `ns` holds under it, what a `(shadow ...)` in `ns` made its
    /// own, or what a `use` in `ns` brought in under it; else what the standard library offers every module under it
    /// ([`Self::export`]); else, in a module of the standard library, what
    /// `std` imports under it; else [`intern_in`]'s symbol.
    ///
    /// Any other symbol `ns`'s table holds under the name was made by reading
    /// it there earlier, which is no reason for it to stand in front of the
    /// library's.
    pub fn read(&self, ns: NsId, name: &str) -> SymRef {
        let mut t = table();
        if ns != NsId::KEYWORD {
            if let Some(own) = t.lookup(ns, name) {
                if own.well_known() != NOT_WELL_KNOWN || self.shadowed.contains(&own) || self.imports.contains_key(&(ns, own)) {
                    return own;
                }
            }
            if let Some(sym) = self.exports.get(name) {
                return *sym;
            }
            // A module of the standard library reads the library's names as
            // `std` imports them, offered to every module or not: the library
            // is written against all of itself.
            if ns != NsId::STD && t.within_std(ns) {
                if let Some(local) = t.lookup(NsId::STD, name) {
                    if let Some(Import::Item(sym) | Import::Type(sym)) = self.imports.get(&(NsId::STD, local)) {
                        return *sym;
                    }
                }
            }
        }
        t.read(ns, name)
    }

    /// What `local` stands for in `ns` by a `use`, if anything.
    pub fn imported(&self, ns: NsId, local: SymRef) -> Option<Import> {
        self.imports.get(&(ns, local)).copied()
    }

    /// The symbols a path written in `ns` reads as: `segments` taken one
    /// step at a time, each read in what the step before led to.
    ///
    /// The first segment leads where its name does in `ns` (an import, a
    /// child module, a type), else where it does in the root; an empty first
    /// segment is the root itself (`::name`). Every segment after it but the
    /// last must lead into a module, or to a type, after which only the last
    /// — a member of that type — may follow. The last is read in the module
    /// the path reached, or looked up among the type's members. A path that
    /// cannot be followed is an error that says where it stopped.
    pub fn read_path(&self, ns: NsId, segments: &[&str]) -> Result<Vec<SymRef>, String> {
        self.follow_path(ns, segments).map(|(syms, _)| syms)
    }

    /// [`Self::read_path`], with where the last segment was read: the module
    /// the path reached, or the type whose member it names.
    pub fn follow_path(&self, ns: NsId, segments: &[&str]) -> Result<(Vec<SymRef>, PathStep), String> {
        let mut t = table();
        let (last, steps) = segments.split_last().ok_or_else(|| "an empty path".to_string())?;
        let mut out = Vec::with_capacity(segments.len());
        let mut at: Option<PathStep> = None;
        for (i, seg) in steps.iter().enumerate() {
            let here = match at {
                None if seg.is_empty() => {
                    // `::a::b`: from the root. The empty segment stays a
                    // segment of the path, for the checker's "from root"
                    // reading.
                    out.push(t.read(NsId::ROOT, ""));
                    at = Some(PathStep::Module(NsId::ROOT));
                    continue;
                }
                None => t.first_step(self, ns, seg),
                Some(PathStep::Module(m)) => t.step_down(self, m, seg),
                Some(PathStep::Type(_)) => {
                    return Err(format!("`{}`: a type's member has no members of its own", segments[..=i].join("::")))
                }
            };
            let (sym, step) = here.ok_or_else(|| {
                format!("`{}`: no module or type `{}` is visible here", segments.join("::"), segments[..=i].join("::"))
            })?;
            out.push(sym);
            at = Some(step);
        }
        let reached = at.expect("a path has at least two segments");
        let last_sym = match reached {
            PathStep::Module(m) => t.read(m, last),
            PathStep::Type(ty) => *self.types[&ty].get(*last).ok_or_else(|| {
                format!("`{}`: type `{}` has no member `{}`", segments.join("::"), ty.name(), last)
            })?,
        };
        out.push(last_sym);
        Ok((out, reached))
    }

    /// Where the segments of a path — the symbols [`Self::read_path`] read
    /// them as — lead, for a path written in `ns`: the module the last of
    /// them reaches, or the type. Taken step by step as `read_path` took
    /// them, so a form read in `ns` reaches what its reading did. The first
    /// segment is also looked up in the module its symbol belongs to, after
    /// `ns`: a path a macro's expansion carried in from the macro's module
    /// leads where it did there.
    pub fn reached(&self, ns: NsId, segments: &[SymRef]) -> Result<PathStep, String> {
        let t = table();
        let spelled = |n: usize| segments[..n].iter().map(|s| s.name()).collect::<Vec<_>>().join("::");
        let (first, rest) = segments.split_first().ok_or_else(|| "an empty path".to_string())?;
        let mut at = if first.name().is_empty() {
            PathStep::Module(NsId::ROOT)
        } else {
            let home = (!first.is_shared_vocabulary()).then(|| first.home());
            [Some(ns), home]
                .into_iter()
                .flatten()
                .find_map(|outer| t.step_in_symbol(self, outer, *first))
                .or_else(|| self.offered_type(*first))
                .or_else(|| [NsId::ROOT, NsId::STD].into_iter().find_map(|outer| t.step_in_symbol(self, outer, *first)))
                .ok_or_else(|| format!("`{}`: no module or type `{}` is visible here", spelled(segments.len()), first.name()))?
        };
        for (i, seg) in rest.iter().enumerate() {
            at = match at {
                PathStep::Module(m) => t.step_down_symbol(self, m, *seg).ok_or_else(|| {
                    format!("`{}`: no module or type `{}` is visible here", spelled(segments.len()), spelled(i + 2))
                })?,
                PathStep::Type(_) => {
                    return Err(format!("`{}`: a type's member has no members of its own", spelled(i + 2)))
                }
            };
        }
        Ok(at)
    }

    /// Whether `name`, read in `ns` as the first segment of a path, names a
    /// module or a type — what [`Self::read_path`] can step into. Anything
    /// else before a `::` names a value (`p::x`), whose type only the
    /// analysis knows.
    pub fn names_scope(&self, ns: NsId, name: &str) -> bool {
        name.is_empty() || self.first_segment(ns, name).is_some()
    }

    /// The type `name`, read in `ns`, refers to — found the way the first
    /// segment of a path is ([`Self::read_path`]) — if it refers to one.
    pub fn type_named(&self, ns: NsId, name: &str) -> Option<SymRef> {
        match self.first_segment(ns, name)? {
            PathStep::Type(t) => Some(t),
            PathStep::Module(_) => None,
        }
    }

    /// Where `name`, read in `ns` as the first segment of a path, leads
    /// ([`Self::read_path`]): a module or a type, if either.
    pub fn first_segment(&self, ns: NsId, name: &str) -> Option<PathStep> {
        table().first_step(self, ns, name).map(|(_, step)| step)
    }
}
