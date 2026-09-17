//! Which names a core form refers to but does not bind.
//!
//! The cons counterpart of `compile::freevars`, which answers the same two
//! questions about the Rust AST. Two things drop out of the move:
//!
//! * **Only names come back, not types.** The AST walk had to carry a `Type`
//!   out with every name, because that was the only place a captured
//!   variable's representation could be read from. Here the bridge already has
//!   a scope of representations (`Ctx`'s own), so this returns bare symbols
//!   and the bridge looks each one up where it was bound.
//! * **A symbol is a `u32`.** Every set and comparison here was a `String`
//!   hash before.
//!
//! # Why this is a grammar walk and not a cons walk
//!
//! It would be shorter to descend into every cons cell and treat every
//! `(var X)` found as a reference. It would also be wrong: a core form
//! contains plenty of lists that are *data* — a `home` path, a representation
//! list, a parameter list, and above all anything under `quote`, which can
//! perfectly well be the literal datum `(var x)`. Telling a node from a datum
//! needs the grammar, so the grammar is written out, one arm per tag.

use std::collections::HashSet;

use typelisp_mem::{wk, Error, Heap, SymRef, Value};

use crate::check::core;

/// The names `body` refers to but does not bind, in first-encounter order —
/// which is the order they become slots in the captured environment.
///
/// `bound` is what the enclosing binder already provides (a lambda's
/// parameters, a `labels` def's). `siblings` is every name that resolves to a
/// direct call rather than a value: referring to one of those is *calling* it,
/// through a separate namespace, so counting it here would manufacture a
/// captured slot for something that is never captured.
pub fn free_vars(
    heap: &Heap,
    body: &[Value],
    bound: &HashSet<SymRef>,
    siblings: &HashSet<SymRef>,
) -> Result<Vec<SymRef>, Error> {
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for form in body {
        walk(heap, *form, bound, siblings, &mut seen, &mut order)?;
    }
    Ok(order)
}

/// The names some closure nested inside `body` captures.
///
/// The bridge intersects this with its own parameters and `let` bindings to
/// decide which of them have to be promoted to a shared cell at bind time —
/// the promotion a captured binding needs for an assignment inside the
/// capturing closure to be visible everywhere else that shares it.
///
/// Deliberately imprecise about shadowing, exactly as the AST version is: a
/// name that is both an outer binding and an unrelated inner one can be
/// flagged from either occurrence. An unnecessary cell costs a little codegen
/// and never correctness, and the caller's intersection against its own bound
/// names discards the rest.
pub fn names_captured_by_nested(heap: &Heap, body: &[Value]) -> Result<HashSet<SymRef>, Error> {
    let mut out = HashSet::new();
    for form in body {
        walk_nested(heap, *form, &mut out)?;
    }
    Ok(out)
}

fn note(name: SymRef, bound: &HashSet<SymRef>, siblings: &HashSet<SymRef>, seen: &mut HashSet<SymRef>, order: &mut Vec<SymRef>) {
    if bound.contains(&name) || siblings.contains(&name) {
        return;
    }
    if seen.insert(name) {
        order.push(name);
    }
}

fn sym(heap: &Heap, form: Value, i: usize) -> Result<SymRef, Error> {
    match core::field(heap, form, i) {
        Some(Value::Symbol(id)) => Ok(id),
        _ => Err(Error::TypeError(format!("compile: expected a name in {}", core::print(heap, form)))),
    }
}

fn walk(
    heap: &Heap,
    form: Value,
    bound: &HashSet<SymRef>,
    siblings: &HashSet<SymRef>,
    seen: &mut HashSet<SymRef>,
    order: &mut Vec<SymRef>,
) -> Result<(), Error> {
    let Some(tag) = core::op_sym(heap, form) else { return Ok(()) };
    let walk_all = |forms: &[Value], seen: &mut HashSet<SymRef>, order: &mut Vec<SymRef>| -> Result<(), Error> {
        for f in forms {
            walk(heap, *f, bound, siblings, seen, order)?;
        }
        Ok(())
    };
    match tag.well_known() {
        // No sub-forms at all. `quote`'s datum is the important one: it is
        // user data, and descending into it is exactly the mistake the module
        // comment describes.
        wk::INT_ANY_WIDTH | wk::INT | wk::FLOAT_ANY_WIDTH | wk::BIGNUM | wk::RATIO | wk::CHAR | wk::BOOL | wk::STR | wk::SYM | wk::UNIT | wk::QUOTE | wk::GLOBAL | wk::FNREF | wk::METHODREF | wk::COMPILE_FN | wk::TRACE | wk::UNTRACE | wk::DISASSEMBLE_FN | wk::BREAK => Ok(()),

        wk::VAR => {
            note(sym(heap, form, 0)?, bound, siblings, seen, order);
            Ok(())
        }
        // An assignment refers to its target as much as a read does, so the
        // name counts — a closure that only ever *writes* a captured binding
        // still captures it.
        wk::SET => {
            note(sym(heap, form, 0)?, bound, siblings, seen, order);
            walk_all(&[core::field(heap, form, 1).unwrap_or(Value::Empty)], seen, order)
        }
        wk::SET_GLOBAL => walk_all(&[core::field(heap, form, 4).unwrap_or(Value::Empty)], seen, order),

        // The `else` link is iterated rather than recursed: a dispatch chain
        // the size of the island's own is deep enough to overflow the stack,
        // and only that side chains. Both other children stay shallow.
        wk::IF => {
            let mut node = form;
            loop {
                let parts = core::fields(heap, node)?;
                let [cond, then, els] = parts[..] else {
                    return Err(Error::TypeError(format!("compile: malformed if: {}", core::print(heap, node))));
                };
                walk(heap, cond, bound, siblings, seen, order)?;
                walk(heap, then, bound, siblings, seen, order)?;
                match core::op_sym(heap, els).map(|s| s.well_known()) {
                    Some(wk::IF) => node = els,
                    _ => return walk(heap, els, bound, siblings, seen, order),
                }
            }
        }

        wk::LET => {
            let parts = core::fields(heap, form)?;
            let Some((binds, body)) = parts.split_first() else { return Ok(()) };
            let binds = heap.list_to_vec(*binds)?;
            // Initialisers see the *outer* scope: a binding is not in scope
            // for its own value.
            let mut inner = bound.clone();
            for b in &binds {
                let items = heap.list_to_vec(*b)?;
                let [Value::Symbol(name), _, init] = items[..] else {
                    return Err(Error::TypeError(format!("compile: malformed binding: {}", core::print(heap, *b))));
                };
                walk(heap, init, bound, siblings, seen, order)?;
                inner.insert(name);
            }
            for f in body {
                walk(heap, *f, &inner, siblings, seen, order)?;
            }
            Ok(())
        }

        // A nested `lambda` gets no direct-call access to an enclosing
        // block's siblings — only a `labels` def's own siblings do — so a
        // sibling name referred to here is an ordinary capture, and
        // `siblings` is cleared rather than passed down.
        wk::LAMBDA => {
            let parts = core::fields(heap, form)?;
            if parts.len() < 2 {
                return Ok(());
            }
            let mut inner = bound.clone();
            for p in heap.list_to_vec(parts[0])? {
                if let Ok(Value::Symbol(name)) = heap.car(p) {
                    inner.insert(name);
                }
            }
            for f in &parts[2..] {
                walk(heap, *f, &inner, &HashSet::new(), seen, order)?;
            }
            Ok(())
        }

        wk::LABELS => {
            let parts = core::fields(heap, form)?;
            let Some((defs, body)) = parts.split_first() else { return Ok(()) };
            let defs = heap.list_to_vec(*defs)?;
            let mut inner_siblings = siblings.clone();
            for d in &defs {
                if let Ok(Value::Symbol(name)) = heap.car(*d) {
                    inner_siblings.insert(name);
                }
            }
            for d in &defs {
                let items = heap.list_to_vec(*d)?;
                if items.len() < 3 {
                    continue;
                }
                let mut inner = bound.clone();
                for p in heap.list_to_vec(items[1])? {
                    if let Ok(Value::Symbol(name)) = heap.car(p) {
                        inner.insert(name);
                    }
                }
                for f in &items[3..] {
                    walk(heap, *f, &inner, &inner_siblings, seen, order)?;
                }
            }
            for f in body {
                walk(heap, *f, bound, &inner_siblings, seen, order)?;
            }
            Ok(())
        }

        wk::MATCH => {
            let parts = core::fields(heap, form)?;
            if parts.is_empty() {
                return Ok(());
            }
            walk(heap, parts[0], bound, siblings, seen, order)?;
            for arm in &parts[2..] {
                let items = heap.list_to_vec(*arm)?;
                let Some((pat, arm_body)) = items.split_first() else { continue };
                let mut inner = bound.clone();
                pattern_bindings(heap, *pat, &mut inner)?;
                // A `pat-guard`'s test is an ordinary expression sitting
                // inside the pattern, and it can name anything the arm body
                // could — `(= limit)` reads the enclosing `limit`. Missing
                // these would make a closure over such a `match` capture one
                // slot too few. `inner` already holds the guard's own
                // scrutinee name (`pattern_bindings` inserts it), so the
                // one name the test is guaranteed to use is not counted.
                pattern_guard_tests(heap, *pat, &mut |g| walk(heap, g, &inner, siblings, seen, order))?;
                for f in arm_body {
                    walk(heap, *f, &inner, siblings, seen, order)?;
                }
            }
            Ok(())
        }

        // `(select ARM...)`. The operands are `(var ...)` nodes naming the
        // `let` the checker wrapped around this one, so they are read from
        // the surrounding scope like any other name; a receive arm's own
        // binding is not.
        wk::SELECT => {
            for arm in core::fields(heap, form)? {
                let items = heap.list_to_vec(arm)?;
                let Some(Value::Str(tag_id)) = items.first() else { continue };
                match heap.string(*tag_id) {
                    "recv" => {
                        if items.len() != 5 {
                            continue;
                        }
                        walk(heap, items[3], bound, siblings, seen, order)?;
                        let mut inner = bound.clone();
                        if let Value::Symbol(sym) = items[1] {
                            inner.insert(sym);
                        }
                        walk(heap, items[4], &inner, siblings, seen, order)?;
                    }
                    "send" => {
                        if items.len() != 5 {
                            continue;
                        }
                        walk(heap, items[2], bound, siblings, seen, order)?;
                        walk(heap, items[3], bound, siblings, seen, order)?;
                        walk(heap, items[4], bound, siblings, seen, order)?;
                    }
                    "else" => {
                        if items.len() == 2 {
                            walk(heap, items[1], bound, siblings, seen, order)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }

        // The rest differ only in where their sub-forms start.
        _ => {
            let forms = plain_sub_forms(heap, form, tag)?;
            walk_all(&forms, seen, order)
        }
    }
}

/// The sub-forms of a node that binds nothing.
///
/// Split out because both walks need the same answer, and because writing the
/// offsets down once is what keeps them from drifting apart from the
/// vocabulary.
fn plain_sub_forms(heap: &Heap, form: Value, tag: SymRef) -> Result<Vec<Value>, Error> {
    let parts = core::fields(heap, form)?;
    let from = |n: usize| -> Vec<Value> { parts.iter().skip(n).copied().collect() };
    Ok(match tag.well_known() {
        // written, home, path, representations, then the arguments.
        wk::CALL => from(4),
        // type, method, instance, home, result, representations, then the rest.
        wk::ASSOC => from(7),
        // The callee, then (past the return representation and the argument
        // representations) the arguments.
        wk::APPLY => {
            let mut v = vec![parts.first().copied().unwrap_or(Value::Empty)];
            v.extend(from(3));
            v
        }
        // `(loop REPR BODY...)` — everything past the repr. The repr leads
        // here (the body is variadic, so it could not trail), and it is not a
        // form: a parametric one is a list headed by a symbol, so walking it
        // looks exactly like walking a node and fails with "the free-variable
        // walk does not know the tag `vector`" — `field-get`'s trap below.
        wk::LOOP => from(1),
        // `(go RET-R CALL)` — the call it wraps is an ordinary node, and every
        // name in it (callee and arguments alike) is read from the scope the
        // `go` is written in, because that is where they are evaluated. The
        // representation ahead of it is not a form.
        wk::GO => from(1),
        // `(spawn RET-R LAMBDA)` / `(tag R E)` — the bridge's own rewriting of
        // a `go` (`translate_go`): one form each, past a representation.
        wk::SPAWN | wk::TAG => from(1),
        // `(step FORM)` — the form is an ordinary expression and names
        // whatever the surrounding scope holds. The three REPL tool nodes
        // beside it (`trace`/`untrace`/`disassemble-fn`) carry only resolved
        // *names* and so are leaves, listed with `compile-fn` above. All four
        // are refused a lowering by the bridge; walking them correctly is what
        // lets that refusal be the error a user sees.
        wk::STEP => from(0),
        wk::RETURN | wk::PANIC | wk::DYN_VALUE | wk::UNTAG_INT | wk::TAG_INT => from(0),
        // `(some-of REPR E)` / `(box-option KEY E)` — the operand only; the
        // representation is not a form (a parametric one is a list headed by
        // a symbol, `field-get`'s trap below) and the key is a `(str ...)`.
        wk::SOME_OF | wk::BOX_OPTION => from(1),
        // `(block NAME BODY REPR)` — the body only. The name is a `(str ...)`
        // node and *not* a sub-form to walk: it is a compile-time label,
        // resolved by the checker, so a variable can never hide in it. Walking
        // it would be harmless here but would say the opposite about what a
        // block name is. The trailing repr is not a form at all, for the reason
        // `loop` above gives.
        wk::BLOCK => parts.get(1).copied().into_iter().collect(),
        // `(return-from NAME [VALUE])` — everything past the name, and there is
        // no repr: the value's representation is the *block's*, read there.
        wk::RETURN_FROM => from(1),
        // `(field-get OBJ IDX REPR)` — the object only. The index is an
        // integer and the representation is not a form at all: a parametric one
        // (`(vector int)`, `(hashtable str int)`) is a *list* whose head is a
        // symbol, so walking it looks exactly like walking a node and fails with
        // "the free-variable walk does not know the tag `vector`".
        wk::FIELD_GET => parts.first().copied().into_iter().collect(),
        wk::DYN_UPCAST => from(1),
        // `(catch TAG BODY REPR)` / `(throw TAG VALUE REPR)` — the middle
        // field only. The tag is a quoted datum and the representation is not
        // a form at all; a parametric one (`(vector int)`) is a list headed by
        // a symbol, so walking it would look exactly like walking a node and
        // fail with "the free-variable walk does not know the tag `vector`" —
        // the same trap `field-get` documents.
        wk::CATCH | wk::THROW => parts.get(1).copied().into_iter().collect(),
        // `(unwind-protect PROTECTED CLEANUP REPR)` — both halves are ordinary
        // forms (a cleanup refers to names from the scope it is written in,
        // exactly as the protected form does); the trailing repr is not one.
        wk::UNWIND_PROTECT => parts.iter().take(2).copied().collect(),
        wk::FIELD_SET => {
            let mut v = vec![parts.first().copied().unwrap_or(Value::Empty)];
            v.extend(from(3));
            v
        }
        // path, variant, mutable, field representations, then the fields.
        wk::CONSTRUCT => from(5),
        // The boxed value, past the vtable tables and the value's own
        // representation.
        wk::DYN_NEW => from(5),
        // type, method, slot, table, representations, then the arguments.
        wk::DYN_CALL => from(5),
        _ => {
            return Err(Error::TypeError(format!(
                "compile: the free-variable walk does not know the tag `{}`",
                tag.name()
            )))
        }
    })
}

fn pattern_bindings(heap: &Heap, pat: Value, out: &mut HashSet<SymRef>) -> Result<(), Error> {
    let Some(tag) = core::op_sym(heap, pat) else { return Ok(()) };
    match tag.well_known() {
        wk::PAT_BIND => {
            out.insert(sym(heap, pat, 0)?);
        }
        wk::PAT_CTOR => {
            // path, key, variant, downcast, field representations, then
            // sub-patterns.
            for p in core::fields(heap, pat)?.iter().skip(5) {
                pattern_bindings(heap, *p, out)?;
            }
        }
        wk::PAT_TYPETEST => {
            if let Some(inner) = core::field(heap, pat, 2) {
                pattern_bindings(heap, inner, out)?;
            }
        }
        // `(some P)` over a niched `Option`, which the niche turns into this
        // node rather than a `pat-ctor` (`Checker::pattern_form`). Missing it
        // made `P`'s names look *free* in the arm body, so a closure around
        // the whole `match` captured a name bound inside it — and then had no
        // binder to read a representation from. Field 0 is the payload
        // representation, not a pattern.
        wk::PAT_SOME => {
            if let Some(inner) = core::field(heap, pat, 1) {
                pattern_bindings(heap, inner, out)?;
            }
        }
        // The name the test form reads the scrutinee through. Not a
        // user-visible binding — the arm body cannot name it — but it *is*
        // bound while the test runs, which is what this set is asked about.
        wk::PAT_GUARD => {
            out.insert(sym(heap, pat, 0)?);
        }
        _ => {}
    }
    Ok(())
}

/// Calls `f` on every `pat-guard` test form in `pat`, however deeply nested.
fn pattern_guard_tests(
    heap: &Heap,
    pat: Value,
    f: &mut dyn FnMut(Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let Some(tag) = core::op_sym(heap, pat) else { return Ok(()) };
    match tag.well_known() {
        wk::PAT_GUARD => {
            if let Some(test) = core::field(heap, pat, 1) {
                f(test)?;
            }
        }
        wk::PAT_CTOR => {
            for p in core::fields(heap, pat)?.iter().skip(4) {
                pattern_guard_tests(heap, *p, f)?;
            }
        }
        wk::PAT_TYPETEST => {
            if let Some(inner) = core::field(heap, pat, 1) {
                pattern_guard_tests(heap, inner, f)?;
            }
        }
        wk::PAT_SOME => {
            if let Some(inner) = core::field(heap, pat, 1) {
                pattern_guard_tests(heap, inner, f)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Finds nested closures and takes their free variables whole, without
/// descending past one — the free-variable walk already resolves arbitrarily
/// deep beneath it, so a name two closures down still surfaces.
fn walk_nested(heap: &Heap, form: Value, out: &mut HashSet<SymRef>) -> Result<(), Error> {
    let Some(tag) = core::op_sym(heap, form) else { return Ok(()) };
    match tag.well_known() {
        wk::LAMBDA => {
            let parts = core::fields(heap, form)?;
            if parts.len() < 2 {
                return Ok(());
            }
            let mut bound = HashSet::new();
            for p in heap.list_to_vec(parts[0])? {
                if let Ok(Value::Symbol(name)) = heap.car(p) {
                    bound.insert(name);
                }
            }
            out.extend(free_vars(heap, &parts[2..], &bound, &HashSet::new())?);
            Ok(())
        }
        wk::LABELS => {
            let parts = core::fields(heap, form)?;
            let Some((defs, body)) = parts.split_first() else { return Ok(()) };
            let defs = heap.list_to_vec(*defs)?;
            let mut siblings = HashSet::new();
            for d in &defs {
                if let Ok(Value::Symbol(name)) = heap.car(*d) {
                    siblings.insert(name);
                }
            }
            for d in &defs {
                let items = heap.list_to_vec(*d)?;
                if items.len() < 3 {
                    continue;
                }
                let mut bound = HashSet::new();
                for p in heap.list_to_vec(items[1])? {
                    if let Ok(Value::Symbol(name)) = heap.car(p) {
                        bound.insert(name);
                    }
                }
                out.extend(free_vars(heap, &items[3..], &bound, &siblings)?);
            }
            // The trailing body is not a closure of its own, so it is walked
            // for closures nested *within* it rather than taken whole.
            for f in body {
                walk_nested(heap, *f, out)?;
            }
            Ok(())
        }
        wk::LET => {
            let parts = core::fields(heap, form)?;
            let Some((binds, body)) = parts.split_first() else { return Ok(()) };
            for b in heap.list_to_vec(*binds)? {
                if let Some(init) = heap.list_to_vec(b)?.get(2) {
                    walk_nested(heap, *init, out)?;
                }
            }
            for f in body {
                walk_nested(heap, *f, out)?;
            }
            Ok(())
        }
        wk::MATCH => {
            let parts = core::fields(heap, form)?;
            if parts.is_empty() {
                return Ok(());
            }
            walk_nested(heap, parts[0], out)?;
            for arm in &parts[2..] {
                let items = heap.list_to_vec(*arm)?;
                if let Some(pat) = items.first() {
                    pattern_guard_tests(heap, *pat, &mut |g| walk_nested(heap, g, out))?;
                }
                for f in items.iter().skip(1) {
                    walk_nested(heap, *f, out)?;
                }
            }
            Ok(())
        }
        wk::IF => {
            for f in core::fields(heap, form)? {
                walk_nested(heap, f, out)?;
            }
            Ok(())
        }
        // `(select ARM...)`. Each arm is a list, not a node, so the default
        // arm's `plain_sub_forms` cannot reach into one.
        wk::SELECT => {
            for arm in core::fields(heap, form)? {
                for f in heap.list_to_vec(arm)?.iter().skip(1) {
                    walk_nested(heap, *f, out)?;
                }
            }
            Ok(())
        }
        wk::SET => {
            if let Some(v) = core::field(heap, form, 1) {
                walk_nested(heap, v, out)?;
            }
            Ok(())
        }
        wk::SET_GLOBAL => {
            if let Some(v) = core::field(heap, form, 4) {
                walk_nested(heap, v, out)?;
            }
            Ok(())
        }
        wk::INT_ANY_WIDTH | wk::INT | wk::FLOAT_ANY_WIDTH | wk::BIGNUM | wk::RATIO | wk::CHAR | wk::BOOL | wk::STR | wk::SYM | wk::UNIT | wk::QUOTE | wk::VAR | wk::GLOBAL | wk::FNREF | wk::METHODREF | wk::COMPILE_FN | wk::TRACE | wk::UNTRACE | wk::DISASSEMBLE_FN | wk::BREAK => Ok(()),
        _ => {
            for f in plain_sub_forms(heap, form, tag)? {
                walk_nested(heap, f, out)?;
            }
            Ok(())
        }
    }
}
