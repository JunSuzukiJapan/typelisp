//! Reading and evaluating hand-written core forms, for the unit tests of
//! [`core_eval`](super::core_eval) and [`core_cps`](super::core_cps).

use typelisp_mem::{Heap, StdModule, Value};

use crate::eval::value::EvalError;

use super::Interp;

/// A binding name as core IR text read in `%core` spells it — the same
/// symbol [`read1`] reads for that name.
pub(super) fn core_symbol(name: &str) -> Value {
    Value::Symbol(typelisp_mem::symbols::intern_in(typelisp_mem::NsId::CORE, name))
}

/// A heap in stress mode: every `cons` collects first, so a value the
/// evaluator (or a continuation frame) holds without rooting dies at the very
/// next allocation rather than one run in a hundred.
pub(super) fn stress_heap() -> Heap {
    let mut h = Heap::with_capacity(1 << 16);
    h.set_gc_stress(true);
    h
}

/// Read exactly one core form. The reader conses, so this already runs
/// under `gc_stress` when the heap is one of ours.
pub(super) fn read1(heap: &mut Heap, src: &str) -> Value {
    // Core IR, read in the core IR's module so its tags are the IR's own.
    // The standard library's modules its paths name are declared first,
    // since a path is read one segment at a time. The built-in helpers live
    // in `std::%internal`, a module of every session the registry is
    // published into; there is no registry here.
    heap.names().declare_module(typelisp_mem::symbols::ns_of(&["std".to_string(), crate::INTERNAL_MODULE.to_string()]));
    for m in [StdModule::OPTION, StdModule::RESULT, StdModule::VEC, StdModule::SEXPR, StdModule::COLLECTIONS, StdModule::PRIMITIVE] {
        heap.names().declare_module(m.ns());
    }
    let mut vs: Vec<Value> = crate::Reader::new()
        .read_all_in_spanned_within(heap, "<core>", src, typelisp_mem::NsId::CORE)
        .expect("read failed")
        .into_iter()
        .map(|(v, _)| crate::check::core::name_written_definitions(heap, v))
        .collect();
    assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
    vs.pop().unwrap()
}

/// Evaluate a core form written as source, in the empty environment.
///
/// Also asserts that the root stack is exactly where it started: a leaked
/// root is not a crash, it is a value that can never be collected, and the
/// only place it shows up is here.
pub(super) fn eval_src(heap: &mut Heap, src: &str) -> Result<Value, EvalError> {
    let form = read1(heap, src);
    heap.push_root(form);
    let before = heap.root_count();
    let interp = Interp::new();
    let out = interp.eval_core(heap, form, Value::Empty);
    assert_eq!(heap.root_count(), before, "eval_core leaked or over-popped roots on {:?}", src);
    out
}

pub(super) fn eval_ok(heap: &mut Heap, src: &str) -> Value {
    eval_src(heap, src).unwrap_or_else(|e| panic!("{:?} failed to evaluate: {}", src, e))
}
