//! interp-closure removal Stage 3: producing the committed, precompiled
//! compiler-island artifact that Stage 4's `compiler::load_aot` loads at
//! runtime *instead of* interpreting the island's bodies.
//!
//! [`build_island_bitcode`] compiles every island `defun` (`compiler.rs`'s
//! `SOURCE`) into one shared LLVM module and serializes it to bitcode. The
//! bootstrap binary (`src/bin/bootstrap_island.rs`) writes those bytes to
//! `src/compiler_island.bc`, which is committed and `include_bytes!`'d by
//! `load_aot`.
//!
//! Only the *native bodies* are committed — no checker/interpreter state.
//! `load_aot` rebuilds that by re-checking `SOURCE` (registering each
//! `defun`'s `FnDef`, which allocates no closures — an island body is never
//! *called* interpreted, only compiled); the bitcode supplies the compiled
//! function bodies that make those calls native.
//! A [`SOURCE_HASH_GLOBAL`] i64 global carrying [`island_source_hash`] of
//! `SOURCE` is embedded in the module so `load_aot` and the
//! `island_artifacts_are_fresh` test can detect a `.bc` gone stale relative
//! to `SOURCE`, with no separate sidecar file. That hash covers the *forms*
//! the reader produced rather than the source bytes, so editing a comment does
//! not invalidate the artifact — see its doc comment for why the distinction
//! is worth a reader pass, and why `prelude`'s fasl cache still uses the
//! byte-based [`crate::fasl::source_hash`] instead.
//!
//! **That hash covers one of the artifact's two inputs.** The bitcode is a
//! compilation of `SOURCE` *by the Rust-side LLVM builders*
//! (`eval_llvm_builtin_method`), so changing a builder changes the artifact
//! just as changing `SOURCE` does. Regenerating after Stage D made this
//! visible: with `SOURCE` untouched, the emitted indirect apply went from a
//! `rt_closure_fnptr` + env-loop + indirect-call sequence to a single
//! `rt_apply_any` call, and the committed `.bc` had been carrying the old one
//! with nothing complaining.
//!
//! The other input is covered by comparing the *output* instead of guessing at
//! inputs: `the_committed_island_matches_a_fresh_build`
//! (`tests/island_artifacts_test.rs`) rebuilds the bitcode and diffs the
//! bytes. That works because regeneration is a fixpoint — this function
//! installs the committed `.bc` and drives *its* `compile-function`, yet what
//! gets emitted is decided by `SOURCE` and the Rust builders rather than by
//! which generation is driving, so building from the result reproduces it.
//!
//! Building this installs the *committed* (previous) `.bc` first and drives
//! its **native** `compile-function` to emit each defun's IR — the snapshot
//! chain (interp-closure removal Stage 8b), so regeneration no longer depends
//! on interpreted closures and they can be deleted entirely (Stage 8c). The
//! very first `.bc` was built by the interpreted island (before this chain
//! existed); every one since is recompiled by its predecessor. See
//! [`build_island_bitcode`]'s own `install_island_bitcode(..., false)` call.

use std::cell::RefCell;
use std::rc::Rc;

use inkwell::module::Module;
use inkwell::AddressSpace;

use crate::check::core;
use crate::{Checker, Heap, Interp, Reader, Value};

/// The name of the i64 global the island bitcode carries its source hash in.
/// Read back by [`read_embedded_source_hash`].
pub const SOURCE_HASH_GLOBAL: &str = "__typelisp_island_source_hash";

/// Builds the compiler island's AOT bitcode in a throwaway environment.
///
/// The module mirrors [`crate::compile::aot::compile_file`]'s shared module
/// (rt_* forward declarations + one `add_compiled_function` per defun, each
/// defun's `labels` siblings emitted alongside it by the island's own
/// `compile-labels`) minus the `main` wrapper — the island is a library of
/// compiled functions, not an executable. No `add_global_mapping` is done
/// here: the rt_* addresses are supplied at load time by `load_aot`'s
/// `CompiledFn::new_multi`, and the bitcode only needs to carry the
/// declarations (which it does).
pub fn build_island_bitcode() -> Result<Vec<u8>, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();

    crate::load_prelude(&mut heap, &mut chk, &mut interp);

    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, crate::compiler::SOURCE)
        .map_err(|e| format!("island read failed: {}", e))?;
    chk.predeclare_program(&mut heap, &forms);
    let mut fn_names: Vec<String> = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut heap, &interp, v).map_err(|e| format!("island check failed: {}", e))?;
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        if let Some(name) = defun_name(&heap, tl) {
            fn_names.push(name);
        }
        interp.exec(&mut heap, tl).map_err(|e| format!("island exec failed: {}", e))?;
    }

    // Install the *committed* island bitcode so the compile loop below drives
    // the previous build's **native** `compile-function`, not an interpreted
    // one (the snapshot chain — interp-closure removal Stage 8b). `check_hash`
    // is `false`: the committed `.bc` is by construction one generation behind
    // the `SOURCE` we're recompiling, so a hash mismatch is expected. Any
    // island `defun` added since that `.bc` is simply absent from it and gets
    // compiled fresh by the just-installed native `compile-function`. This is
    // what lets interpreted closures be deleted (Stage 8c): regenerating the
    // island no longer needs the interpreter to tree-walk `compile-function`'s
    // own `labels`/`lambda`s. (The very first `.bc`, before this chain existed,
    // was built by the interpreted island; every one since is built by its
    // predecessor.)
    interp
        .install_island_bitcode(crate::compiler::ISLAND_BITCODE, &fn_names, false)
        .map_err(|e| format!("island bootstrap install of the committed .bc failed: {}", e))?;

    let ctx = crate::compile::llvm_context();
    let module = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("compiler_island");
        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        for (name, _) in crate::eval::interp::rt_extern_functions() {
            module.add_function(name, fn_ty, None);
        }
        // Embed the source hash as an i64 global with a constant initializer
        // (internal linkage — it's read by value from the parsed module, never
        // linked against).
        let i64_ty = ctx.i64_type();
        let hash_global = module.add_global(i64_ty, None, SOURCE_HASH_GLOBAL);
        hash_global.set_initializer(&i64_ty.const_int(island_source_hash(crate::compiler::SOURCE)?, false));
        hash_global.set_constant(true);
        // Forward-declare *every* island function before compiling any body.
        // The island's own `compile-call` resolves a call target with
        // `(get-function m "tl_<callee>")`, which fails outright if the callee
        // has no declaration yet — so compiling one at a time only works while
        // the island's call graph happens to be a DAG in declaration order.
        // It is not: since top-level `defun`s may reference each other freely
        // (`Checker::predeclare_program`) the island is written as ~60 mutually
        // recursive top-level functions rather than one `labels` block, and
        // `compile-value` calls helpers declared below it. `add-function`
        // reuses an existing declaration rather than adding a second one
        // (`llvm_module_add_function`), so a body compiled later simply fills
        // in the shell declared here. This is the same shape `Interp::
        // compile_scc` already uses for a JIT'd cycle.
        for name in &fn_names {
            let sym = crate::compile::symbols::user_symbol_name(name);
            if module.get_function(&sym).is_none() {
                module.add_function(&sym, fn_ty, None);
            }
        }
        Rc::new(RefCell::new(module))
    };
    // `add_compiled_function` locks `COMPILE_LOCK` per LLVM builtin call
    // (`eval_llvm_builtin_method`) and `Mutex` isn't reentrant, so it must
    // run without the lock held — same constraint `aot::compile_file`
    // documents at its own loop.
    for name in &fn_names {
        let internal_name = crate::compile::symbols::user_symbol_name(name);
        interp
            .add_compiled_function(&mut heap, module.clone(), name, &internal_name)
            .map_err(|e| format!("island compile of `{}` failed: {}", name, e))?;
    }

    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    let module = module.borrow();
    module.verify().map_err(|e| format!("island module failed verification: {}", e))?;
    Ok(module.write_bitcode_to_memory().as_slice().to_vec())
}

/// The island's staleness key: a hash of what the reader *read*, not of the
/// bytes it read them from.
///
/// The committed `.bc` is a compilation of `SOURCE`, so it goes stale exactly
/// when the *forms* change. Comments, indentation and blank lines are not
/// forms; hashing the text made every one of them invalidate a 1 MB binary
/// artifact, which in practice meant `SOURCE`'s comments were frozen — 52
/// lines of them still named a module deleted in Stage C, because correcting a
/// comment would have demanded a regeneration.
///
/// Reading is the right way to drop them, not a comment-stripping pass over
/// the text. A stripper has to decide what a `;` means, and it is not always a
/// comment (`";"` inside a string literal, `#\;`). Getting that wrong fails in
/// the worst direction available here: the hash stops covering real code, and
/// the freshness guard starts calling a stale island fresh. The reader already
/// makes that distinction correctly, so this asks it instead of re-deriving it.
///
/// **Source positions are deliberately excluded.** They live in the cons cells
/// themselves (`Heap::cons`'s location slots), so hashing them would put line
/// numbers back in and undo the whole point. That is only sound because
/// nothing downstream of the reader carries a position into compiled code:
/// `core_bridge` emits none and `src/compiler.rs` embeds none, so two `SOURCE`
/// texts that read alike compile to identical bitcode.
///
/// Distinct from [`source_hash`], which stays byte-based for fasl caches — a
/// fasl stores checked state *including* positions (`OwnedForm` carries both
/// of a cell's location slots since v19), so for those a moved line genuinely
/// is a change.
pub fn island_source_hash(source: &str) -> Result<u64, String> {
    use std::hash::{Hash, Hasher};

    let mut heap = Heap::with_capacity(1 << 18);
    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, source)
        .map_err(|e| format!("island source hash: read failed: {}", e))?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    forms.len().hash(&mut hasher);
    for form in forms {
        hash_form(&heap, form, &mut hasher)?;
    }
    Ok(hasher.finish())
}

/// Feeds one read form into `hasher` as a preorder walk.
///
/// Every variant contributes a distinct tag byte before its payload, and only
/// `Cons` has children, so the tag sequence determines the tree unambiguously.
/// Interned ids (`SymId`/`StrId`/`PathId`) are *not* hashed — they are
/// positions in this throwaway heap's intern tables, so the same text read
/// twice in a different order would hash differently. The name behind the id
/// is what the source says.
///
/// Iterative because `SOURCE` nests deeply (a 4,000-line file of `defun`s whose
/// bodies are one long `cdr` chain each); recursion here would be a stack
/// overflow waiting for a big enough island.
fn hash_form(
    heap: &Heap,
    root: Value,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) -> Result<(), String> {
    use std::hash::Hash;

    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        match v {
            Value::Empty => 0u8.hash(hasher),
            Value::Int(n) => {
                1u8.hash(hasher);
                n.hash(hasher);
            }
            Value::Char(c) => {
                2u8.hash(hasher);
                c.hash(hasher);
            }
            Value::Bool(b) => {
                3u8.hash(hasher);
                b.hash(hasher);
            }
            Value::Symbol(id) => {
                4u8.hash(hasher);
                heap.symbol_name(id).hash(hasher);
            }
            Value::Str(id) => {
                5u8.hash(hasher);
                heap.string(id).hash(hasher);
            }
            Value::Path(id) => {
                6u8.hash(hasher);
                let segments = heap.path_segments(id);
                segments.len().hash(hasher);
                for seg in segments {
                    heap.symbol_name(*seg).hash(hasher);
                }
            }
            Value::Cons(_) => {
                7u8.hash(hasher);
                let car = heap.car(v).map_err(|e| format!("island source hash: {}", e))?;
                let cdr = heap.cdr(v).map_err(|e| format!("island source hash: {}", e))?;
                // Push in reverse so `car`'s whole subtree is walked first.
                stack.push(cdr);
                stack.push(car);
            }
            Value::Boxed(id) if heap.is_float(id) => {
                8u8.hash(hasher);
                // Bit pattern, not the `f64`: `f64` isn't `Hash`, and two
                // literals that differ only as NaN payloads differ in source.
                heap.float_value(id).to_bits().hash(hasher);
            }
            Value::Boxed(id) if heap.is_bignum(id) => {
                9u8.hash(hasher);
                heap.bignum_value(id).hash(hasher);
            }
            Value::Boxed(id) if heap.is_ratio(id) => {
                10u8.hash(hasher);
                heap.ratio_value(id).hash(hasher);
            }
            // Float/bignum/ratio are the only boxes a *reader* can produce.
            // Anything else means the reader gained a representation this hash
            // would silently ignore — which is exactly the failure this
            // function exists to prevent, so it is an error, not a default.
            Value::Boxed(_) => {
                return Err("island source hash: the reader produced a boxed value this hash does not cover".to_string())
            }
        }
    }
    Ok(())
}

/// Reads the [`SOURCE_HASH_GLOBAL`] i64 constant back out of an
/// already-parsed island `module` — the staleness key `load_aot` and the
/// freshness test compare against [`island_source_hash`]`(SOURCE)`. `None` if
/// the global is absent or not a constant integer (a `.bc` from before this
/// global existed, or a corrupt one).
pub fn read_embedded_source_hash(module: &Module<'static>) -> Option<u64> {
    let global = module.get_global(SOURCE_HASH_GLOBAL)?;
    global.get_initializer()?.into_int_value().get_zero_extended_constant()
}

/// The last segment of a `(defun PATH ...)` form's name, or `None` for anything
/// else — the island is all `defun`s, and this is what names each one for
/// `install_island_bitcode`'s symbol list.
fn defun_name(heap: &Heap, tl: Value) -> Option<String> {
    if core::op(heap, tl) != Some("defun") {
        return None;
    }
    match core::field(heap, tl, 0)? {
        Value::Path(id) => Some(crate::types::path_from_id(heap, id).last_segment().to_string()),
        Value::Symbol(id) => Some(heap.symbol_name(id).to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::island_source_hash;

    fn hash(src: &str) -> u64 {
        island_source_hash(src).expect("hashing failed")
    }

    /// The point of the whole function: a comment is not a form, so editing one
    /// must not invalidate `src/compiler_island.bc`. Blank lines and
    /// indentation likewise.
    #[test]
    fn comments_and_layout_do_not_change_the_island_hash() {
        let bare = "(defun f ((n i64)) i64 (+ n 1))";
        let commented = r#"
;; A comment nobody should have to regenerate a 1 MB artifact to fix.
(defun f ((n i64)) i64
    ;; ...including one in the middle,
    (+ n 1))   ;; and one at the end.
"#;
        assert_eq!(hash(bare), hash(commented));
    }

    /// Whereas a change to the forms themselves must.
    #[test]
    fn a_changed_form_changes_the_island_hash() {
        assert_ne!(hash("(defun f () i64 (+ 1 1))"), hash("(defun f () i64 (+ 1 2))"));
    }

    /// A string is not a comment, however it reads.
    ///
    /// This is the case that rules out stripping comments from the *text*: a
    /// stripper cutting at the first `;` would truncate both of these to the
    /// same prefix and call a changed island unchanged — the freshness guard
    /// failing open, which is the one direction it must never fail in. Asking
    /// the reader costs a parse and cannot get this wrong.
    #[test]
    fn a_semicolon_inside_a_string_is_not_a_comment() {
        assert_ne!(hash(r#"(defun f () string (g ";" 1))"#), hash(r#"(defun f () string (g ";" 2))"#));
    }

    /// Two texts that read to the same forms hash alike no matter where the
    /// forms sat, because positions live in the cons cells and are skipped.
    #[test]
    fn source_positions_are_not_part_of_the_island_hash() {
        assert_eq!(hash("(f 1)\n(g 2)"), hash("\n\n\n(f 1)\n\n\n\n(g 2)\n\n"));
    }

    /// Interned ids are heap-local, so the hash uses the names behind them.
    /// Reading the same symbols in a different order would otherwise assign
    /// different `SymId`s and produce a different hash for the same program.
    #[test]
    fn the_hash_follows_names_not_intern_ids() {
        assert_ne!(hash("(alpha beta)"), hash("(beta alpha)"));
        assert_eq!(hash("(alpha beta)"), hash("(alpha beta)"));
    }

    /// A symbol and a string spelled alike are different forms.
    #[test]
    fn a_symbol_and_a_string_with_the_same_text_differ() {
        assert_ne!(hash("(f abc)"), hash(r#"(f "abc")"#));
    }
}
