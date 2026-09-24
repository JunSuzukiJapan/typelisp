//! What a worker thread prints with — `docs/dev/os-threads-design.md` §8
//! ("typl", the detached print hooks).
//!
//! The printer asks its environment six things it cannot read off the heap
//! ([`typelisp_print::runtime::PrintHooks`]): an enum variant's name, whether
//! a field holds a niche-represented `Option`, a type's `print-object`, a
//! `~/name/` method, and the two sets of control variables. On the
//! interpreter's thread the `Interp` answers from its module tree. A worker
//! has no `Interp` — its tables are `Rc`s and `RefCell`s the interpreter's
//! thread mutates — so it answers from a [`PrintSnapshot`] the interpreter
//! publishes: plain data, with the control variables as *where to read them*
//! rather than what they held, so a `setf` of `*print-base*` is seen on the
//! next print as it is on the interpreter's thread.
//!
//! A method the printer would call is either compiled — then the snapshot has
//! its address, and a worker calls it the way the interpreter would — or
//! interpreted, which a worker cannot run: printing such a value there is a
//! panic in the printing task, saying so, rather than a rendering that
//! silently ignores the method.
//!
//! The snapshot is republished whenever what it copies changes (a type, a
//! method, a compilation, a global's storage): at the next drive, or at once
//! if a drive is running and a worker may be printing now
//! ([`Interp::printer_tables_changed`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use typelisp_mem::{Heap, Value};

use super::{CompiledBody, Interp};
use crate::check::repr::Repr;
use crate::eval::value::Slot;
use crate::Path;

/// Everything a worker's printer knows about the program — see the module
/// comment.
#[derive(Default)]
pub(crate) struct PrintSnapshot {
    /// `(base type key, variant index) -> variant name`.
    enum_names: HashMap<(String, usize), String>,
    /// `(base type key, variant or NO_VARIANT, index or EVERY_FIELD) ->
    /// the field's type key template` — `Interp::field_template_descriptors`.
    field_templates: HashMap<(String, i64, i64), String>,
    /// Every method with a `print-object` or a `~/name/` method's signature,
    /// by `(receiver type path, method name)`.
    methods: HashMap<(Path, String), PrintMethod>,
    /// Where each printer control variable lives, by name.
    globals: HashMap<String, GlobalRef>,
}

/// A method the printer may call.
enum PrintMethod {
    /// Compiled: callable from any thread, given how its arguments cross.
    Compiled { address: usize, abi: u8, receiver: Repr },
    /// Interpreted: only the interpreter's thread can run it.
    Interpreted,
}

/// Where a global's value lives — read afresh on every print.
enum GlobalRef {
    /// A compiled-global slot (`Interp::promote_global`).
    Compiled(usize),
    /// The interpreter's own cell.
    Cell(Slot),
}

impl PrintSnapshot {
    fn global(&self, heap: &Heap, name: &str) -> Option<Value> {
        match self.globals.get(name)? {
            GlobalRef::Compiled(id) => typelisp_rt::global_perm_idx(*id).map(|i| heap.permanent_root(i)),
            GlobalRef::Cell(slot) => Some(slot.get(heap)),
        }
    }
}

/// Where an `Interp` publishes its snapshot, shared with its crew's threads.
pub(crate) type SharedSnapshot = Arc<parking_lot::RwLock<Arc<PrintSnapshot>>>;

thread_local! {
    /// This worker's interpreter's snapshot — set by [`install`], on a thread
    /// that has no interpreter of its own.
    static SNAPSHOT: RefCell<Option<SharedSnapshot>> = const { RefCell::new(None) };

    /// The values whose own `print-object` is running on this thread right
    /// now — the re-entry guard `Interp::printing` is on the interpreter's.
    static PRINTING: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// Makes this thread print from `snapshot`. What a worker of the interpreter
/// that owns it runs before it steps anything.
pub(crate) fn install(snapshot: SharedSnapshot) {
    SNAPSHOT.with(|cell| *cell.borrow_mut() = Some(snapshot));
    typelisp_print::runtime::set_print_hooks(Some(WORKER_PRINT_HOOKS));
}

/// The snapshot as of right now.
fn current() -> Arc<PrintSnapshot> {
    SNAPSHOT.with(|cell| match cell.borrow().as_ref() {
        Some(shared) => Arc::clone(&shared.read()),
        None => typelisp_abi::fatal("a worker's printer was installed with no snapshot to answer from"),
    })
}

const WORKER_PRINT_HOOKS: typelisp_print::runtime::PrintHooks = typelisp_print::runtime::PrintHooks {
    enum_variant_name: |key, variant| {
        let base = crate::type_key::split_key(key).0;
        current().enum_names.get(&(base.to_string(), variant)).cloned()
    },
    field_is_niched_option: |key, variant, index| {
        let base = crate::type_key::split_key(key).0.to_string();
        let variant = variant.map_or(typelisp_print::aot::NO_VARIANT, |v| v as i64);
        let snapshot = current();
        let t = &snapshot.field_templates;
        match t
            .get(&(base.clone(), variant, index as i64))
            .or_else(|| t.get(&(base, variant, typelisp_print::aot::EVERY_FIELD)))
        {
            Some(template) => {
                let args = typelisp_mem::type_key_args(key);
                typelisp_mem::option_prints_wrapped(&typelisp_mem::instantiate_key_template(template, &args))
            }
            None => false,
        }
    },
    print_object: worker_print_object,
    format_call: worker_format_call,
    opts: |heap| {
        let snapshot = current();
        super::pretty_opts_from(&|name| snapshot.global(heap, name))
    },
    print_vars: |heap| {
        let snapshot = current();
        super::print_vars_from(&|name| snapshot.global(heap, name))
    },
};

/// `Interp::print_object`, from the snapshot.
fn worker_print_object(heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
    let Some((type_path, method)) = super::print_object_target(heap, v) else {
        return Ok(None);
    };
    if PRINTING.with(|p| p.borrow().contains(&v)) {
        return Ok(None);
    }
    let snapshot = current();
    let (address, abi, receiver) = match snapshot.methods.get(&(type_path.clone(), method)) {
        None => return Ok(None),
        Some(PrintMethod::Interpreted) => return Err(interpreted_off_main("print-object", &type_path)),
        Some(PrintMethod::Compiled { address, abi, receiver }) => (*address, *abi, receiver.clone()),
    };
    let args = [encode(heap, v, &receiver)?, i64::from(escape)];
    PRINTING.with(|p| p.borrow_mut().push(v));
    let result = call(address, abi, &args);
    PRINTING.with(|p| {
        p.borrow_mut().pop();
    });
    match result? {
        Value::Str(id) => Ok(Some(heap.string(id).to_string())),
        other => Err(format!("print-object on `{}` returned {:?}, not a string", type_path, other)),
    }
}

/// `Interp::format_call`, from the snapshot.
fn worker_format_call(heap: &mut Heap, name: &str, v: Value, colon: bool, at: bool) -> Result<String, String> {
    let (type_path, looked_up) = super::format_call_target(heap, name, v)?;
    let snapshot = current();
    let (address, abi, receiver) = match snapshot.methods.get(&(type_path.clone(), looked_up)) {
        None => return Err(format!("format: ~/{}/ — `{}` has no method `{}`", name, type_path, name)),
        Some(PrintMethod::Interpreted) => return Err(interpreted_off_main(&format!("~/{}/", name), &type_path)),
        Some(PrintMethod::Compiled { address, abi, receiver }) => (*address, *abi, receiver.clone()),
    };
    // A narrow integer is a box in a `Sexpr` and a plain word as a
    // parameter — opened on the way in, as `Interp::format_call` does.
    let receiver_value = match v {
        Value::Boxed(id) if receiver == Repr::Narrow => match heap.narrow_box(id) {
            Some(n) => Value::Int(n.value),
            None => v,
        },
        _ => v,
    };
    let args = [encode(heap, receiver_value, &receiver)?, i64::from(colon), i64::from(at)];
    match call(address, abi, &args)? {
        Value::Str(id) => Ok(heap.string(id).to_string()),
        other => Err(format!("format: ~/{}/ on `{}` returned {:?}, not a string", name, type_path, other)),
    }
}

fn encode(heap: &Heap, v: Value, repr: &Repr) -> Result<i64, String> {
    super::encode_crossing_value(heap, &v, repr).map_err(|e| e.to_string())
}

/// Calls a printer method's compiled body and reads back its tagged string
/// — a failure inside it (a `(panic ...)`) comes back as the printer's error,
/// as `Interp::enter`'s does on the interpreter's thread.
fn call(address: usize, abi: u8, args: &[i64]) -> Result<Value, String> {
    crate::eval::crossing::catch_compiled_panic(|| super::call_body(address, abi, args))
        .map(typelisp_rt::decode)
        .map_err(|e| e.to_string())
}

fn interpreted_off_main(what: &str, type_path: &Path) -> String {
    format!(
        "{} of `{}` is not compiled, and a worker thread cannot run it; \
         `(compile ...)` it, or print the value from the main task",
        what, type_path
    )
}

impl Interp {
    /// Something a worker's printer copies has changed: a type, a method, a
    /// compilation, a global's storage. Republished at once if a drive is
    /// running — a worker may be printing now — and at the start of the next
    /// drive otherwise, so that loading a program is not a republish per
    /// definition.
    pub fn printer_tables_changed(&self) {
        self.print_generation.set(self.print_generation.get() + 1);
        if self.driving.get() {
            self.publish_print_snapshot();
        }
    }

    /// Republishes the snapshot if anything changed since it was last
    /// published — what a drive does before it starts
    /// (`Interp::drive_one`), with no worker running yet.
    pub(super) fn refresh_print_snapshot(&self) {
        if self.published_generation.get() != self.print_generation.get() {
            // Between drives no worker is inside a method the previous
            // snapshot named, so the bodies it kept alive may go.
            self.printer_bodies.borrow_mut().clear();
            self.publish_print_snapshot();
        }
    }

    /// Builds the snapshot from the module tree and publishes it.
    ///
    /// The compiled bodies it names are kept alive here for as long as a
    /// worker may be calling one: a redefinition drops the `FnDef`'s body,
    /// and a body nobody holds is retired to be destroyed (see
    /// `compile::retire_llvm`) — which, mid-call on a worker, would be freed
    /// code. Kept until the next drive starts ([`Self::refresh_print_snapshot`]).
    fn publish_print_snapshot(&self) {
        let mut snapshot = PrintSnapshot::default();
        for (key, variant, name) in self.enum_variant_descriptors() {
            snapshot.enum_names.insert((key, variant), name);
        }
        for (key, variant, index, template) in self.field_template_descriptors() {
            snapshot.field_templates.insert((key, variant, index), template);
        }
        let mut methods = Vec::new();
        self.root.borrow().collect_methods(&[], &mut methods);
        let mut bodies = self.printer_bodies.borrow_mut();
        for (type_path, method, f) in methods {
            if !super::is_print_object_sig(&f) && super::format_call_receiver_is_word(&f).is_none() {
                continue;
            }
            let entry = match (f.compiled.borrow().as_ref(), f.sig.as_ref()) {
                (Some(body), Some((params, _))) => {
                    bodies.push(Rc::clone(body) as Rc<dyn CompiledBody>);
                    PrintMethod::Compiled { address: body.address(), abi: body.body_abi(), receiver: params[0].clone() }
                }
                _ => PrintMethod::Interpreted,
            };
            snapshot.methods.insert((type_path, method), entry);
        }
        // The names are the ones the two readers ask for, learned by asking:
        // a variable added to either is in the snapshot without a list here
        // to keep in step.
        let asked = RefCell::new(Vec::new());
        let record = |name: &str| -> Option<Value> {
            asked.borrow_mut().push(name.to_string());
            None
        };
        super::print_vars_from(&record);
        super::pretty_opts_from(&record);
        for name in asked.into_inner() {
            let path = Path::root(&name);
            let place = match self.compiled_global_id(&path) {
                Some(id) => Some(GlobalRef::Compiled(id)),
                None => self.root.borrow().get_global(&path).map(GlobalRef::Cell),
            };
            if let Some(place) = place {
                snapshot.globals.insert(name, place);
            }
        }
        *self.worker_print.write() = Arc::new(snapshot);
        self.published_generation.set(self.print_generation.get());
    }
}
