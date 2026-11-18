//! The call graph a `compile` closes over: each definition's call targets, and
//! its strongly connected components, which compile together.

use std::collections::{HashMap, HashSet};
use typelisp_mem::{Heap, SymRef};
use crate::CompileTarget;
use crate::compile::externs::{is_native_lowered_primitive_method, is_rt_builtin};
use crate::compile::symbols::HASHTABLE_BUILTIN_METHODS;
use crate::eval::interp::{EvalError, Interp};
use crate::types::{path_is_builtin, path_is_builtin_any, LLVM_METHOD_RECEIVER_TYPES, NATIVE_LOWERED_PRIMITIVES, Path};

/// `node`'s own outgoing edges in the top-level compile call graph —
/// every concrete function/method *instantiation* `node`'s body calls,
/// filtered exactly the way `compile_function_rec` always has: self-
/// recursion and `rt_*`/native builtins excluded from the functions;
/// `Vector`/`HashTable`'s op-node-lowered builtin methods and
/// natively-lowered primitive-receiver builtins excluded from the methods. A method target with no registered
/// implementation at all (a builtin `compile-assoc` doesn't lower
/// natively, e.g. `f64::sqrt`) is a compile-time error here, same as
/// before Stage 5 — this is the one path that produces a real user-
/// facing error out of graph construction, everything else just shapes
/// the graph [`crate::compile::driver::compute_sccs`] walks. Shared by that graph walk and
/// [`crate::compile::driver::compile_scc`] (which needs the same edges again, in typed
/// form, to know what to forward-declare/wire as `externals`).
pub(crate) fn call_graph_edges(interp: &Interp, heap: &Heap, node: &CompileTarget) -> Result<Vec<CompileTarget>, EvalError> {
    let (_, body) = interp.compiled_fn_body(node)?;
    let targets = match crate::compile::core_bridge::collect_targets(heap, &body) {
        Ok(t) => t,
        Err(e) => return Err(EvalError::Panic(e.to_string())),
    };
    let mut edges = Vec::new();

    edges.extend(
        targets
            .calls
            .into_iter()
            .filter(|p| !matches!(node, CompileTarget::Fn(own) if own == p) && !is_rt_builtin(p))
            .map(CompileTarget::Fn),
    );

    let method_targets: Vec<(Path, SymRef)> = targets
        .methods
        .into_iter()
        .filter(|(t, m)| !matches!(node, CompileTarget::Method(own_t, own_m) if own_t == t && own_m == m))
        .filter(|key| {
            // `Vector<T>`'s field-backed builtin methods (`new`/`get`/
            // `set`/`len`/`push`/`pop`) are lowered to a `vector-op` node
            // (`core_bridge::translate_vector_method` -> `rt_struct_*`),
            // not a method call, so — like the native primitive methods
            // below — they are never a real call target. `Vector::iter`
            // is deliberately excluded from this list: it is a genuine
            // prelude `defmethod` (`vector-iter::new`) and must be
            // `compile`d like any other method.
            if path_is_builtin(&key.0, typelisp_mem::wk::VECTOR) && matches!(key.1.name(), "new" | "get" | "set" | "len" | "push" | "pop") {
                return false;
            }
            // `HashTable<K,V>`'s builtin methods lowered to a `hashtable-op`
            // node (`core_bridge::translate_hashtable_method`) are likewise
            // never a real call target. The prelude `defmethod`s — `iter`,
            // and `get`/`set`/`remove`/`maphash`/`size` — are deliberately
            // absent from that list, so they are validated and transitively
            // compiled like any other method.
            if path_is_builtin(&key.0, typelisp_mem::wk::HASHTABLE) && HASHTABLE_BUILTIN_METHODS.contains(&key.1.well_known()) {
                return false;
            }
            // `llvm-*`/`scope` builtin methods are natively lowered to
            // the `rt_llvm_call` dispatch shim (an `llvm-op` node,
            // interp-closure removal Stage 1) — like `vector-op`/
            // `hashtable-op` above, never a real call target. A
            // heap-repr `Scope<V>` method has no compiled lowering and
            // panics inside `compile-assoc-user`'s `get-function`
            // instead, per the convention in the next comment.
            if path_is_builtin_any(&key.0, &LLVM_METHOD_RECEIVER_TYPES) {
                return false;
            }
            // A user-registered method is a real call target even on a
            // primitive receiver (`i32::equals`); only the natively
            // lowered `i64`/`i32`/`char`/`string`/`f64`/`bignum`/`ratio`
            // builtins (`+`, `<`, `=`, `lt`, `length`, `sqrt`, `fadd`,
            // `rt_bignum_add`, ...) are excluded — those become LLVM
            // instructions / `rt_str_*`/`rt_bignum_*`/`rt_ratio_*` calls
            // in `compile-assoc`, not function calls. A builtin on these
            // receivers that `compile-assoc` does *not* lower natively
            // (`i64::int->char`, `string::upcase`, ...) is kept as a target
            // so the `!interp.methods.contains_key` check below rejects it
            // with a clean up-front error — otherwise it reaches the
            // island's `get-function` guard, an unrecoverable
            // `rt_llvm_call` abort under the AOT-native island
            // (interp-closure removal Stage 8a). `is_native_lowered_primitive_method`
            // is the Rust twin of the island's `*-native-method?` list.
            // A method that **suspends** is lowered too, and not as a call at
            // all: `core_bridge` turns it into a `(suspend ...)` node. So it is
            // not a target, for the same reason a natively lowered primitive
            // method is not — there is no function for the graph to reach.
            //
            // A builtin *static* method (`Thread::current-id`) is the same:
            // `core_bridge` lowers it to a plain `rt_*` call, and there is no
            // function for the graph to reach either.
            if key.0.is_std()
                && (crate::compile::externs::rt_suspend_method_symbol(&key.0, key.1).is_some()
                    || crate::compile::externs::rt_static_method_symbol(&key.0, key.1).is_some())
            {
                return false;
            }
            interp.root.borrow().has_method(&key.0, key.1)
                || !path_is_builtin_any(&key.0, &NATIVE_LOWERED_PRIMITIVES)
                || !is_native_lowered_primitive_method(&key.0, key.1)
        })
        .collect();
    for (type_name, method) in &method_targets {
        if !interp.root.borrow().has_method(type_name, *method) {
            return Err(EvalError::Uncompilable {
                caller: node.link_name(),
                target: typelisp_mem::ConcreteNames::method_link(type_name, method.name()),
            });
        }
    }
    edges.extend(method_targets.into_iter().map(|(p, m)| CompileTarget::Method(p, m)));
    Ok(edges)
}

/// Tarjan's algorithm over the top-level compile call graph, rooted at
/// `name`, restricted to the induced subgraph of not-yet-`compile`d
/// nodes (labels/closures Stage 5) — an edge into an already-compiled
/// target is a leaf for this traversal's purposes, since its address is
/// already known and needs no further graph treatment. Returns every
/// strongly connected component this traversal reaches, **in the order
/// Tarjan completes them**: a classic property of the algorithm is that
/// this finish order is a reverse topological order of the SCC
/// condensation — if `A` calls something in a different SCC `B`, `B`
/// finishes (and is pushed onto the result) before `A` does. That is
/// exactly the order [`crate::compile::driver::compile_function`] needs to hand to
/// [`crate::compile::driver::compile_scc`]: every SCC's external dependencies are already
/// compiled by the time it's processed. A single-member SCC with no
/// self-loop is the common case (an ordinary, non-recursive-with-others
/// function); a multi-member SCC is genuine mutual recursion across
/// separate top-level functions, unsupported before this stage.
pub(crate) fn compute_sccs(interp: &Interp, heap: &Heap, root: &CompileTarget) -> Result<Vec<Vec<CompileTarget>>, EvalError> {
    let mut counter = 0usize;
    let mut indices: HashMap<CompileTarget, usize> = HashMap::new();
    let mut lowlink: HashMap<CompileTarget, usize> = HashMap::new();
    let mut on_stack: HashSet<CompileTarget> = HashSet::new();
    let mut stack: Vec<CompileTarget> = Vec::new();
    let mut sccs: Vec<Vec<CompileTarget>> = Vec::new();
    scc_strongconnect(interp, heap, root, &mut counter, &mut indices, &mut lowlink, &mut on_stack, &mut stack, &mut sccs)?;
    Ok(sccs)
}

/// One node's worth of Tarjan's `strongconnect` — see
/// [`crate::compile::driver::compute_sccs`]'s doc comment for the algorithm-level
/// contract. Recursive over [`crate::compile::driver::call_graph_edges`]; an edge whose
/// target is already compiled is skipped outright (never entered into
/// `indices` at all), so it never contributes a spurious singleton SCC.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scc_strongconnect(
    interp: &Interp,
    heap: &Heap,
    node: &CompileTarget,
    counter: &mut usize,
    indices: &mut HashMap<CompileTarget, usize>,
    lowlink: &mut HashMap<CompileTarget, usize>,
    on_stack: &mut HashSet<CompileTarget>,
    stack: &mut Vec<CompileTarget>,
    sccs: &mut Vec<Vec<CompileTarget>>,
) -> Result<(), EvalError> {
    indices.insert(node.clone(), *counter);
    lowlink.insert(node.clone(), *counter);
    *counter += 1;
    stack.push(node.clone());
    on_stack.insert(node.clone());

    for target in call_graph_edges(interp, heap, node)? {
        if interp.is_compiled(&target) {
            continue;
        }
        if !indices.contains_key(&target) {
            scc_strongconnect(interp, heap, &target, counter, indices, lowlink, on_stack, stack, sccs)?;
            let merged = lowlink[node].min(lowlink[&target]);
            lowlink.insert(node.clone(), merged);
        } else if on_stack.contains(&target) {
            let merged = lowlink[node].min(indices[&target]);
            lowlink.insert(node.clone(), merged);
        }
    }

    if lowlink[node] == indices[node] {
        let mut component = Vec::new();
        loop {
            let w = stack.pop().expect("node's own strongconnect frame pushed it onto the stack");
            on_stack.remove(&w);
            let is_root = w == *node;
            component.push(w);
            if is_root {
                break;
            }
        }
        sccs.push(component);
    }
    Ok(())
}
