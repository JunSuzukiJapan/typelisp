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
//! Both halves are committed: the checked state the island's definitions
//! produce, and the bitcode holding their bodies. `load_aot` applies the first
//! (registering each `defun`'s `FnDef`, which allocates no closures — an island
//! body is never *called* interpreted, only compiled) and installs the second
//! over it. The dump records a digest of `SOURCE`, which is what `load_aot` and
//! `island_artifacts_are_fresh` check for staleness.
//!
//! **That digest covers one of the artifact's two inputs.** The bitcode is a
//! compilation of `SOURCE` *by the Rust-side LLVM builders*
//! (`eval_llvm_builtin_method`), so changing a builder changes the artifact
//! just as changing `SOURCE` does. Regenerating after Stage D made this
//! visible: with `SOURCE` untouched, the emitted indirect apply went from a
//! `rt_closure_fnptr` + env-loop + indirect-call sequence to a single
//! `rt_apply_any` call, and the committed artifact had been carrying the old
//! one with nothing complaining.
//!
//! The other input is covered by comparing the *output* instead of guessing at
//! inputs: `the_committed_island_matches_a_fresh_build`
//! (`tests/island_artifacts_test.rs`) rebuilds the bitcode and diffs the
//! bytes. That works because regeneration is a fixpoint — this function
//! installs the committed `.bc` and drives *its* `compile-function`, yet what
//! gets emitted is decided by `SOURCE` and the Rust builders rather than by
//! which generation is driving, so building from the result reproduces it.
//!
//! **The fixpoint is reached in one pass only when the change does not alter
//! the island's own code generation.** When it does, the first pass emits the
//! new `SOURCE` through the *previous* generation's `compile-function`, so the
//! result carries the old shape and a build from it differs — run the script
//! again. 2026-08-16's static-exit cleanups are the worked example: reordering
//! `compile-break`'s `store` and its `rt_truncate_sexpr_roots` call changed the
//! IR emitted at every `break`, and the island has two of its own, so one pass
//! left exactly those two sites in the old order and
//! `the_committed_island_matches_a_fresh_build` failed on a 4568-byte
//! difference. A second pass converged, and a third confirmed it.
//!
//! Building this installs the *committed* (previous) `.bc` first and drives
//! its **native** `compile-function` to emit each defun's IR — the snapshot
//! chain (interp-closure removal Stage 8b), so regeneration no longer depends
//! on interpreted closures and they can be deleted entirely (Stage 8c). The
//! very first `.bc` was built by the interpreted island (before this chain
//! existed); every one since is recompiled by its predecessor. See
//! [`build_island_bitcode`]'s own `install_island_bitcode(..., false)` call.

use crate::check::core;
use crate::compile::symbols::CompiledItem;
use crate::{Checker, Heap, Interp, Reader, Value};

/// Builds the compiler island's AOT bitcode in a throwaway environment.
///
/// The module mirrors [`crate::compile::aot::compile_file`]'s shared module
/// (rt_* forward declarations + one `add_compiled_function` per defun, each
/// defun's `labels` siblings emitted alongside it by the island's own
/// `compile-labels`) minus the `main` wrapper — the island is a library of
/// compiled functions, not an executable. No addresses are supplied
/// here: the rt_* addresses are supplied at load time by `load_aot`'s
/// `CompiledFn::new_multi`, and the bitcode only needs to carry the
/// declarations (which it does).
pub fn build_island_artifact() -> Result<Vec<u8>, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();

    // Interpreted: the island needs the prelude's *definitions* in scope, not
    // its compiled bodies — and depending on `prelude_compiled.bc` here would
    // close a cycle, since that artifact is built by the island.
    crate::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);

    // After the prelude, before the island: what the island unit records is
    // what the *island* added, and the checker they share cannot say
    // afterwards which of them contributed what.
    let before = chk.signature(&heap)?;

    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, &crate::compiler::SOURCE)
        .map_err(|e| format!("island read failed: {}", e))?;
    let forms_digest = hash_read_forms(&heap, &forms)?;
    let mut items: Vec<CompiledItem> = Vec::new();
    let mut checked: Vec<Value> = Vec::new();
    for v in forms {
        let tl = chk.check_form(&mut heap, &interp, v).map_err(|e| format!("island check failed: {}", e))?;
        for w in chk.take_warnings() {
            eprintln!("{}", w);
        }
        collect_island_items(&heap, tl, &mut items)?;
        // Rooted and never popped: these go into the dump at the end of this
        // function, with the whole compile loop in between. The heap is a
        // throwaway the generator drops on the way out.
        heap.push_root(tl);
        checked.push(tl);
        interp.exec(&mut heap, tl).map_err(|e| format!("island exec failed: {}", e))?;
    }
    let delta = chk.capture_delta(&heap, &before)?;

    // Install the *committed* island bitcode so the compile loop below drives
    // the previous build's **native** `compile-function`, not an interpreted
    // one (the snapshot chain — interp-closure removal Stage 8b). Only the
    // bitcode half of the committed dump is taken, and its checked state is
    // ignored: the definitions are already in scope from the read/check/exec
    // above, which is what this generation is *about*. No digest check either
    // — the committed dump is by construction one generation behind the
    // `SOURCE` being recompiled, so a mismatch is expected. Any
    // island `defun` added since that `.bc` is simply absent from it and gets
    // compiled fresh by the just-installed native `compile-function`. This is
    // what lets interpreted closures be deleted (Stage 8c): regenerating the
    // island no longer needs the interpreter to tree-walk `compile-function`'s
    // own `labels`/`lambda`s. (The very first `.bc`, before this chain existed,
    // was built by the interpreted island; every one since is built by its
    // predecessor.)
    //
    // **Except when the committed island cannot compile the new `SOURCE` at
    // all** — when the checker has started emitting a core-IR node the
    // committed bodies' `compile-value` does not know, in every body
    // including the island's own (the `int` boundary's `untag-int` was this:
    // the checker wraps every `int` argument to a builtin, so no `SOURCE`
    // without the node could be written to teach the island the node). Then
    // the previous generation is no help, and the only island that knows the
    // new vocabulary is this `SOURCE` *interpreted* — which is how the very
    // first `.bc` was built. `TYPELISP_BOOTSTRAP_INTERPRETED=1` asks for
    // that: nothing is installed, and the compile loop below drives the
    // interpreter's `compile-function`. Slow, and used for exactly one
    // generation; the result is an ordinary artifact the next regeneration
    // drives natively as always.
    if std::env::var_os("TYPELISP_BOOTSTRAP_INTERPRETED").is_some() {
        eprintln!("island bootstrap: driving the INTERPRETED island (TYPELISP_BOOTSTRAP_INTERPRETED is set)");
    } else {
        crate::compile::driver::install_compiled_library(&interp, crate::compile::CompiledLibrary {
            label: "compiler island",
            bitcode: typelisp_front::dump::parse(crate::compiler::ISLAND_DUMP, "compiler island")?
                .first()
                .ok_or_else(|| "island: the committed dump holds no units".to_string())?
                .bitcode,
            body_abi: crate::compiler::ISLAND_DUMP_BODY_ABI,
            body_layout: crate::compiler::ISLAND_DUMP_BODY_LAYOUT,
            items: &items,
        })
        .map_err(|e| format!("island bootstrap install of the committed .bc failed: {}", e))?;
    }

    // The island announces its ring with `defsignature` and is written as ~60
    // mutually recursive top-level functions rather than one `labels` block,
    // so nothing can be compiled until everything is declared — see
    // `fresh_module_with_declarations`.
    let module = crate::compile::driver::fresh_module_with_declarations("compiler_island", &items);
    // `add_compiled_function` locks `COMPILE_LOCK` per LLVM builtin call
    // (`eval_llvm_builtin_method`) and `Mutex` isn't reentrant, so it must
    // run without the lock held — same constraint `aot::compile_file`
    // documents at its own loop.
    for item in &items {
        let node_name = item.node_name();
        crate::compile::driver::add_compiled_function(&interp, &mut heap, module.clone(), &node_name, &item.symbol_name())
            .map_err(|e| format!("island compile of `{}` failed: {}", node_name, e))?;
    }

    let bitcode = {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
        let bitcode = {
            let m = module.borrow();
            crate::compile::verify_module_naming_functions(&m, "island module").map(|()| {
                // `as_slice` deliberately includes LLVM's guaranteed trailing
                // NUL (inkwell's `MemoryBuffer::get_size` is
                // `LLVMGetBufferSize() + 1`), so the committed artifact is one
                // byte longer than the bitstream. **Do not trim it**: the loader
                // is `MemoryBuffer::create_from_memory_range_copy`
                // (`Interp::install_compiled_library`), which asserts that last
                // byte is NUL and passes `len - 1` as the real size. Trimming
                // would not even fail the assert here — the bitstream's own last
                // byte is padding zero — it would hand LLVM a buffer one byte
                // short instead.
                //
                // The visible cost is that `llvm-dis`/`llvm-bcanalyzer` reject
                // the file as written ("Bitcode stream should be a multiple of 4
                // bytes in length"; the artifact is 4n+1). To inspect one, drop
                // the last byte into a scratch copy.
                m.write_bitcode_to_memory().as_slice().to_vec()
            })
        };
        // Destroyed with the guard still held: `module` was declared before it,
        // so its own drop would run after the guard released, and `~Module`
        // unregisters every value name from the shared LLVM Context (see
        // `compile::COMPILE_LOCK`).
        drop(module);
        bitcode?
    };

    let state = typelisp_front::dump::capture_types_with_abi(
        &heap,
        delta,
        "compiler island",
        Some(typelisp_front::dump::source_digest(&crate::compiler::SOURCE)),
        Some(forms_digest),
        &checked,
        items.iter().map(crate::compile::prelude_bootstrap::unit_item).collect(),
        // The island defines 121 `defun`s and one `defmacro`, and no globals at
        // all — nothing here to give compiled-slot storage to.
        Vec::new(),
        // The bodies just written came out of the island running *now*, so
        // they answer to whatever it emits; what they will emit in turn is
        // `SOURCE`'s business, and across a changeover those are two different
        // answers for one generation.
        crate::compile::EMITTED_BODY_ABI,
        crate::compiler::SOURCE_EMITS_ABI,
        crate::compile::EMITTED_LAYOUT,
        crate::compiler::SOURCE_EMITS_LAYOUT,
    )?;
    let types = typelisp_front::dump::write_state(&state)?;
    Ok(typelisp_front::dump::write(&[(types, bitcode)]))
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
/// Shared by both artifacts (the compiler island and the prelude): the
/// property wanted — a comment must not invalidate a megabyte of bitcode — is
/// the same for each, and so is the reason positions can be skipped.
pub fn island_source_hash(source: &str) -> Result<u64, String> {
    let mut heap = Heap::with_capacity(1 << 18);
    let reader = Reader::new();
    let forms = reader
        .read_all(&mut heap, source)
        .map_err(|e| format!("island source hash: read failed: {}", e))?;
    hash_read_forms(&heap, &forms)
}

/// [`island_source_hash`] over forms already read, for a loader that just read
/// them.
///
/// `prelude::load` needs the hash of the same source it is in the middle of
/// loading. Going through [`island_source_hash`] there would read the prelude a
/// second time into a second 256K-cell `Heap`, at every startup, to reproduce a
/// parse that just happened — measurable next to a load this whole artifact
/// exists to make faster. The value is identical either way: the hash covers
/// names rather than intern ids and skips positions, so which heap the forms
/// were read into cannot affect it.
pub fn hash_read_forms(heap: &Heap, forms: &[Value]) -> Result<u64, String> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    forms.len().hash(&mut hasher);
    for form in forms {
        hash_form(heap, *form, &mut hasher)?;
    }
    Ok(hasher.finish())
}

/// Feeds one read form into `hasher` as a preorder walk.
///
/// Every variant contributes a distinct tag byte before its payload, and only
/// `Cons` has children, so the tag sequence determines the tree unambiguously.
/// Interned ids (`SymRef`/`StrId`/`PathId`) are *not* hashed — they are
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
                for seg in segments.iter() {
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
            // Bit pattern, not the float: a float isn't `Hash`, and two
            // literals that differ only as NaN payloads differ in source. The
            // two widths get different discriminants, so a source that says
            // `f32` does not hash the same as one that says `f64`.
            Value::Boxed(id) if heap.is_f64(id) => {
                8u8.hash(hasher);
                heap.f64_value(id).to_bits().hash(hasher);
            }
            Value::Boxed(id) if heap.is_f32(id) => {
                14u8.hash(hasher);
                heap.f32_value(id).to_bits().hash(hasher);
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

/// The last segment of a `(defun PATH ...)` form's name, or `None` for anything
/// else — the island is all `defun`s, and this is what names each one for
/// `install_island_bitcode`'s symbol list.
fn collect_island_items(heap: &Heap, tl: Value, out: &mut Vec<CompiledItem>) -> Result<(), String> {
    let tag = core::op(heap, tl).map(str::to_string).unwrap_or_default();
    match tag.as_str() {
        // The same three shapes `aot::collect_aot_item` flattens, and for the
        // same reason: a `(module ...)` is a container, and the enclosing
        // module is already baked into each item's own path. The one that
        // reaches the island is the **monomorphization bundle** — a form that
        // instantiates a generic comes back wrapped together with the
        // specializations it needs (`Checker::check_form`'s doc comment says
        // why they cannot be separated). Both the specialization and the form
        // that wanted it are ordinary monomorphic definitions by then; without
        // this recursion both were dropped on the floor, and the first island
        // function to call any generic vanished from the module along with it
        // — showing up as `get-function: no function named "tl_<caller>"` from
        // some *other* function's call site.
        "module" => {
            for item in core::fields(heap, tl).map_err(|e| e.to_string())?.into_iter().skip(1) {
                collect_island_items(heap, item, out)?;
            }
        }
        "defun" => {
            let path = core::path_field(heap, tl, 0)
                .ok_or_else(|| "island: defun without a name".to_string())?;
            out.push(CompiledItem::Fn(path));
        }
        "defmethod" => {
            let type_path = core::path_field(heap, tl, 0)
                .ok_or_else(|| "island: defmethod without a type".to_string())?;
            let method = match core::field(heap, tl, 1) {
                Some(Value::Symbol(id)) => heap.symbol_name(id).to_string(),
                _ => return Err("island: defmethod without a name".to_string()),
            };
            out.push(CompiledItem::Method(type_path, method));
        }
        // Declares a signature and emits no body of its own.
        "defsignature" => {}
        other => {
            return Err(format!(
                "island: the SOURCE grew a top-level `{}`, which this bootstrap does not know how to emit",
                other
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_island_items, island_source_hash};
    use crate::{Checker, Heap, Interp, Reader};

    /// A form that instantiates a generic comes back wrapped in a
    /// `<monomorph specializations>` module together with the specializations
    /// it needs, so the enumeration has to look *inside* a `module` — see
    /// [`collect_island_items`].
    ///
    /// Without the recursion this returns nothing at all: the caller is
    /// dropped along with its specialization, and the failure surfaces far
    /// away, as `get-function: no function named "tl_<caller>"` raised while
    /// compiling some *other* function that calls it.
    #[test]
    fn a_generic_call_yields_both_the_caller_and_its_specialization() {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        crate::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
        let reader = Reader::new();
        let forms = reader
            .read_all(&mut heap, "(defun island-probe ((e Option<Sexpr>)) Sexpr (unwrap (sexpr-car e)))")
            .expect("read failed");
        let mut items = Vec::new();
        for v in forms {
            let tl = chk.check_form(&mut heap, &interp, v).expect("check failed");
            collect_island_items(&heap, tl, &mut items).expect("collection failed");
        }
        let names: Vec<String> = items.iter().map(|i| i.node_name()).collect();
        assert!(names.iter().any(|n| n == "island-probe"), "the caller itself is missing: {:?}", names);
        assert!(
            names.len() > 1,
            "the specialization `unwrap` was instantiated at must travel with its caller: {:?}",
            names
        );
    }

    /// The island's own top level is `defun`s and `defsignature`s; anything
    /// else would emit nothing and go unnoticed, so it is refused by name.
    #[test]
    fn an_unknown_top_level_shape_is_refused_rather_than_skipped() {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        crate::prelude::load_interpreted(&mut heap, &mut chk, &mut interp);
        let reader = Reader::new();
        let forms = reader.read_all(&mut heap, "(defvar (island-probe-global i32) 1)").expect("read failed");
        let tl = chk.check_form(&mut heap, &interp, forms[0]).expect("check failed");
        let mut items = Vec::new();
        let err = collect_island_items(&heap, tl, &mut items).expect_err("a defvar must be refused");
        assert!(err.contains("defvar"), "the message should name the shape: {}", err);
    }

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
    /// different `SymRef`s and produce a different hash for the same program.
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
