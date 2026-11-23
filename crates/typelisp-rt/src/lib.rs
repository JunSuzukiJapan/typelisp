//! The shared Rust-only runtime library compiled code calls into directly —
//! the destination for builtins that genuinely can't be written in typelisp
//! (direct cons-heap access, raw GC-heap bookkeeping; see
//! `docs/dev/implementation-log.md`, "Sexpr表現 + Match/Construct/共有Rust
//! ライブラリ 実装計画" section, for the full plan this crate is Stage 0/1 of).
//!
//! A separate crate, depending on nothing but `typelisp-mem` (no
//! `inkwell`/LLVM) — Stage 1 found that linking the *whole* `typelisp` crate
//! (which embeds all of LLVM) into an AOT-compiled executable just to reach
//! these few functions drags in LLVM's entire system-library footprint
//! (`libc++`, zlib, libffi, terminfo, ...) for no benefit. This crate's
//! `staticlib` artifact stays small and dependency-free, which is all
//! `compile-file`'s linker step needs.
//!
//! Every function here is declared under the exact same ABI as a compiled
//! typelisp function (`unsafe extern "C" fn(*const i64, u32) -> i64`,
//! `typelisp::compile::CompiledSignature`) — so the existing cross-function-
//! call wiring (JIT: the `externals` list of
//! `typelisp::compile::CompiledFn::new`; AOT: ordinary linker symbol
//! resolution against this crate's `staticlib`, see `typelisp::compile::aot`'s
//! `write_executable`) treats a call to one of these exactly like a call to
//! another already-compiled typelisp function — no new call mechanism is
//! needed, only `#[no_mangle]` so the linker sees a plain, unmangled C
//! symbol name.
//!
//! `rt_ping` is Stage 0's deliberately trivial placeholder: it exists only
//! to prove that *some* Rust function defined in this crate is callable
//! from both a JIT-compiled function (via its `externals`) and an
//! AOT-linked native executable (via the system linker) before any real
//! heap/Sexpr machinery is built on top.

// The printer's shims (`rt_format`/`rt_print`/...) and the reader's
// (`rt_read`) are defined in `typelisp-print` and `typelisp-read`, not here,
// so that a program which never prints does not link the directive engine and
// one which never reads does not link the reader — see those crates' doc
// comments for the measurements.
//
// But `compile-file` links exactly one archive, *this* crate's `staticlib`,
// and rustc leaves an entirely unreferenced dependency out of it (measured: 0
// `typelisp_print` members in `libtypelisp_rt.a` without the line below, 117
// with it). These two re-exports are that reference. They generate no code and
// have no callers on purpose — deleting them as dead would make every
// `println` and every `read` in a compiled program fail to link.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use typelisp_print;
pub use typelisp_read;

pub mod coroutine;

pub mod equality;
pub mod mem_limit;
pub mod c_mem;
pub mod ffi_callback;
pub mod integer;
#[cfg(all(feature = "std", not(target_family = "wasm")))]
pub mod net;
pub mod rational;
#[cfg(all(feature = "std", not(target_family = "wasm")))]
pub mod net_builtin;
#[cfg(all(feature = "std", not(target_family = "wasm")))]
pub mod x509;
#[cfg(feature = "std")]
pub mod os;
pub mod readtable;
pub mod sched;
pub mod shared;
pub mod stream;
pub mod stream_builtin;
pub mod sys_builtin;

/// Returns `args[0] + 1` if `argc >= 1`, otherwise `0`. No real runtime
/// behavior depends on this — see the module doc comment.
///
/// # Safety
///
/// If `argc >= 1`, `args` must be non-null and point to at least one valid,
/// readable `i64` — exactly what every compiled-function call site already
/// guarantees for its own `args` array.
#[no_mangle]
pub unsafe extern "C" fn rt_ping(args: *const i64, argc: u32) -> i64 {
    if argc >= 1 {
        *args + 1
    } else {
        0
    }
}

// The compiled calling convention — the tagged-word encoding, the active
// `Heap`, and how a shim fails — lives one crate down, in `typelisp-abi`, so
// that `typelisp-print` can define its own shims without depending on this
// crate (which depends on *it*). Re-exported rather than merely imported: the
// `typelisp` crate reaches for several of these as `typelisp_rt::...`, which
// is where they have always been from its point of view.
pub use typelisp_abi::{
    active_heap, decode, encode, fatal, raise, set_active_heap, tagged_arg,
    unwind_interpreted_error, CompiledPanic, CompiledThrow, InterpretedUnwind,
};

mod program;
mod cons;
mod scalar_box;
mod bignum_ratio;
mod structs;
mod enum_data;
mod closure;
mod frame;
mod hashtable;
mod roots;
mod unwind;
mod string;
mod ffi_string;
mod symbol;
mod arith;
mod sexpr_collection;
mod random;
mod global;
mod dyn_trait;
mod io_shims;
pub use program::*;
pub use cons::*;
pub use scalar_box::*;
pub use bignum_ratio::*;
pub use structs::*;
pub use enum_data::*;
pub use closure::*;
pub use frame::*;
pub use hashtable::*;
pub use roots::*;
pub use unwind::*;
pub use string::*;
pub use ffi_string::*;
pub use symbol::*;
pub use arith::*;
pub use sexpr_collection::*;
pub use random::*;
pub use global::*;
pub use dyn_trait::*;
pub use io_shims::*;

#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, PathId, StrId, Value};

    use super::{
        active_heap, decode, encode, reset_global_table, rt_car, rt_cdr, rt_cons, rt_global_get, rt_global_new, rt_global_set,
        rt_heap_init, rt_heap_live_count, rt_ping, rt_pop_sexpr_root, rt_push_permanent_sexpr_root, rt_push_sexpr_root, rt_root_count,
        rt_char_equalp, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append, rt_str_eq, rt_str_equalp, rt_str_length, rt_str_lt,
        rt_str_new, rt_str_ref, rt_hashtable_bucket_count, rt_hashtable_bucket_delete, rt_hashtable_bucket_key,
        rt_hashtable_bucket_put, rt_hashtable_bucket_value, rt_hashtable_count, rt_hashtable_entries,
        rt_hashtable_keys, rt_hashtable_new, rt_data_field, rt_data_new, rt_data_variant,
        rt_cell_get, rt_cell_new, rt_cell_set, rt_closure_env_get, rt_closure_env_len, rt_closure_fnptr, rt_closure_new,
        rt_struct_field_count, rt_struct_field_get, rt_struct_field_set, rt_struct_new, rt_struct_pop_field, rt_struct_push_field, set_active_heap,
    };

    #[test]
    fn rt_ping_adds_one_to_its_first_argument() {
        let args = [41i64];
        assert_eq!(unsafe { rt_ping(args.as_ptr(), 1) }, 42);
    }

    #[test]
    fn rt_ping_returns_zero_with_no_arguments() {
        assert_eq!(unsafe { rt_ping(core::ptr::null(), 0) }, 0);
    }

    #[test]
    fn rt_heap_live_count_reflects_the_registered_heaps_real_state() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);
        assert_eq!(unsafe { rt_heap_live_count(core::ptr::null(), 0) }, 0);

        // Goes through `rt_cons`, not `heap.cons(...)` directly: once `heap`
        // has been registered as the active `Heap` via a raw pointer, every
        // further mutation must go through that same raw-pointer-derived
        // access path (the `rt_*` functions, via `active_heap()`) rather
        // than the original `&mut heap` binding — under Stacked Borrows,
        // reborrowing `heap` directly (as `heap.cons(...)` implicitly does)
        // invalidates the raw pointer `set_active_heap` was given, which
        // `rt_heap_live_count`'s own `active_heap()` call below then trips.
        let cons_args = [encode(Value::Int(1)), encode(Value::Empty)];
        unsafe { rt_cons(cons_args.as_ptr(), 2) };
        assert_eq!(unsafe { rt_heap_live_count(core::ptr::null(), 0) }, 1);
    }

    #[test]
    fn rt_heap_init_registers_a_freshly_created_heap() {
        let args = [4i64];
        assert_eq!(unsafe { rt_heap_init(args.as_ptr(), 1) }, 0);
        assert_eq!(unsafe { rt_heap_live_count(core::ptr::null(), 0) }, 0);

        // `rt_heap_init` deliberately never frees the `Heap` it allocates
        // (see its own doc comment — it must outlive every compiled function
        // in an AOT-linked process, i.e. until the OS reclaims it at exit).
        // That's the right trade-off in production, but this test's process
        // doesn't exit here, so reclaim it explicitly to keep the test
        // itself leak-free under Miri's leak checker rather than suppressing
        // that checker (which would also hide a *real* leak introduced
        // elsewhere in this same test in the future).
        unsafe {
            let heap_ptr = active_heap() as *mut Heap;
            drop(Box::from_raw(heap_ptr));
        }
        set_active_heap(core::ptr::null_mut());
    }

    #[test]
    fn encode_decode_round_trips_every_immediate_variant() {
        for n in [0i64, 1, -1, 42, -42, i64::MIN >> 1, i64::MAX >> 1] {
            assert_eq!(decode(encode(Value::Int(n))), Value::Int(n), "Int({})", n);
        }
        for c in ['a', 'Z', '0', '\u{10FFFF}', '\0'] {
            assert_eq!(decode(encode(Value::Char(c))), Value::Char(c), "Char({:?})", c);
        }
        assert_eq!(decode(encode(Value::Bool(true))), Value::Bool(true));
        assert_eq!(decode(encode(Value::Bool(false))), Value::Bool(false));
        assert_eq!(decode(encode(Value::Empty)), Value::Empty);
        // A real interned symbol: the tagged payload is now its header's
        // address, so there is no id to fabricate.
        let sym = typelisp_mem::symbols::intern("round-trip");
        assert_eq!(decode(encode(Value::Symbol(sym))), Value::Symbol(sym));
        assert_eq!(decode(encode(Value::Str(StrId::from_u32(9)))), Value::Str(StrId::from_u32(9)));
        assert_eq!(decode(encode(Value::Path(PathId::from_u32(3)))), Value::Path(PathId::from_u32(3)));
    }

    #[test]
    fn encode_decode_round_trips_a_cons() {
        let mut heap = Heap::with_capacity(8);
        let pair = heap.cons(Value::Int(1), Value::Int(2)).expect("cons failed");
        assert_eq!(decode(encode(pair)), pair);
    }

    #[test]
    fn rt_cons_rt_car_rt_cdr_round_trip_through_a_real_heap() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let car_in = encode(Value::Int(1));
        let cdr_in = encode(Value::Int(2));
        let cons_args = [car_in, cdr_in];
        let pair = unsafe { rt_cons(cons_args.as_ptr(), 2) };

        let one_arg = [pair];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(1));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(2));
    }

    #[test]
    fn rt_set_car_and_rt_set_cdr_mutate_in_place() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cons_args = [encode(Value::Int(1)), encode(Value::Int(2))];
        let pair = unsafe { rt_cons(cons_args.as_ptr(), 2) };

        let set_car_args = [pair, encode(Value::Int(99))];
        assert_eq!(unsafe { rt_set_car(set_car_args.as_ptr(), 2) }, 0, "set-car returns Unit (0)");
        let set_cdr_args = [pair, encode(Value::Empty)];
        assert_eq!(unsafe { rt_set_cdr(set_cdr_args.as_ptr(), 2) }, 0, "set-cdr returns Unit (0)");

        let one_arg = [pair];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(99));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Empty);
    }

    /// Forces a real `Heap::gc()` (capacity 4, more than 4 conses follow)
    /// while a `precious` cons cell is rooted via [`rt_push_sexpr_root`] —
    /// proving the root genuinely keeps it (and what it points to) alive
    /// across a collection triggered by *other*, unrelated allocations, not
    /// just that the API doesn't crash.
    #[test]
    fn rt_push_sexpr_root_protects_a_value_across_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let pushed = unsafe { rt_push_sexpr_root(&precious as *const i64, 1) };
        assert_eq!(pushed, precious, "rt_push_sexpr_root returns its argument unchanged");

        // Exhausts the remaining 3 free cells and forces at least one GC;
        // none of these throwaway conses are rooted, so each is garbage by
        // the time the next one runs.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let popped = unsafe { rt_pop_sexpr_root(core::ptr::null(), 0) };
        assert_eq!(popped, precious, "the popped root is the same value that was pushed");

        let one_arg = [precious];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// The negative case `rt_push_sexpr_root`'s test above guards against:
    /// without rooting it, the *same* cons cell gets reclaimed by the GC the
    /// later allocations trigger and reused for one of them — so reading
    /// through the original (now-dangling, from the language's perspective)
    /// tagged value no longer shows what it was consed with. Confirms the
    /// problem [`rt_cons`]'s doc comment describes is real, not
    /// hypothetical, and that the fixed-arena sweep
    /// (`Heap::gc`, lowest-index-first free-list rebuild) makes this
    /// deterministic rather than a flaky one-in-a-while corruption.
    #[test]
    fn an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let unrooted_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let unrooted = unsafe { rt_cons(unrooted_args.as_ptr(), 2) };
        // Deliberately not pushed as a root.

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [unrooted];
        let car_after = decode(unsafe { rt_car(one_arg.as_ptr(), 1) });
        let cdr_after = decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) });
        assert_ne!((car_after, cdr_after), (Value::Int(111), Value::Int(222)));
    }

    /// [`rt_push_permanent_sexpr_root`]'s own version of
    /// [`rt_push_sexpr_root_protects_a_value_across_a_gc_triggered_by_other_allocations`]
    /// — protects a value across a forced `gc()` with *no* matching pop at
    /// all (unlike the ordinary root, which the other test still pops at
    /// the end).
    #[test]
    fn rt_push_permanent_sexpr_root_protects_a_value_across_a_gc_with_no_matching_pop() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let pushed = unsafe { rt_push_permanent_sexpr_root(&precious as *const i64, 1) };
        assert_eq!(pushed, precious, "rt_push_permanent_sexpr_root returns its argument unchanged");

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [precious];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// A permanent root pushed in between an ordinary push/pop pair must
    /// not desync that pair's LIFO accounting — the reason
    /// [`rt_push_permanent_sexpr_root`] feeds a separate `Vec`
    /// (`Heap::push_permanent_root`) rather than the same stack
    /// [`rt_push_sexpr_root`]/[`rt_pop_sexpr_root`] share.
    #[test]
    fn rt_push_permanent_sexpr_root_does_not_desync_an_interleaved_ordinary_root_pop() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let ordinary_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let ordinary = unsafe { rt_cons(ordinary_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&ordinary as *const i64, 1) };

        let permanent_args = [encode(Value::Int(2)), encode(Value::Int(2))];
        let permanent = unsafe { rt_cons(permanent_args.as_ptr(), 2) };
        unsafe { rt_push_permanent_sexpr_root(&permanent as *const i64, 1) };

        let popped = unsafe { rt_pop_sexpr_root(core::ptr::null(), 0) };
        assert_eq!(popped, ordinary, "the ordinary root pops, unaffected by the permanent push");
    }

    /// The problem [`rt_set_sexpr_root`] exists to fix, reproduced directly:
    /// a `kind = 2` binding's GC root is pushed once, at bind time, holding
    /// its *original* value (`bind-let-values`'s `rt_push_sexpr_root` call).
    /// `compile-set` (pre-fix) only overwrote the binding's own value slot —
    /// it never touched that root — so a `setf` reassigning the binding to a
    /// freshly built value left the new value with no root at all, exposed
    /// to the very next GC any unrelated allocation triggers. This is
    /// exactly [`an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`]'s
    /// scenario, just reached via a stale *existing* root rather than no
    /// root ever having been pushed.
    #[test]
    fn a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        // `(let ((s (Cons (Int 1) (Int 1)))) ...)` — bind-let-values pushes
        // a root for the *original* value.
        let original_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let original = unsafe { rt_cons(original_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&original as *const i64, 1) };

        // `(setf s (Cons (Int 111) (Int 222)))` — overwrites s's own slot
        // with this new value, but (without rt_set_sexpr_root) no root is
        // ever pushed or updated for it.
        let reassigned_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let reassigned = unsafe { rt_cons(reassigned_args.as_ptr(), 2) };

        // Many unrelated allocations, forcing real gc() calls under a tiny
        // heap — same technique as the un-rooted-value test above.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [reassigned];
        let car_after = decode(unsafe { rt_car(one_arg.as_ptr(), 1) });
        let cdr_after = decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) });
        assert_ne!(
            (car_after, cdr_after),
            (Value::Int(111), Value::Int(222)),
            "a setf-reassigned value with no updated root is expected to be corrupted -- \
             this test documents the bug rt_set_sexpr_root fixes, see the test below"
        );
    }

    /// [`rt_set_sexpr_root`]'s own fix for the test above: capture the
    /// binding's root-stack index via [`rt_root_count`] right before the
    /// initial [`rt_push_sexpr_root`] (mirroring `compiler.rs`'s
    /// `retain-bindings`/`bind-let-values`), then hand that same index to
    /// `rt_set_sexpr_root` on every `setf` instead of pushing a second root
    /// — the reassigned value now survives the identical GC pressure the
    /// test above shows corrupts it without this fix.
    #[test]
    fn rt_set_sexpr_root_protects_a_setf_reassigned_value_across_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let idx = unsafe { rt_root_count(core::ptr::null(), 0) };
        let original_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let original = unsafe { rt_cons(original_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&original as *const i64, 1) };

        let reassigned_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let reassigned = unsafe { rt_cons(reassigned_args.as_ptr(), 2) };
        let set_args = [idx, reassigned];
        let returned = unsafe { rt_set_sexpr_root(set_args.as_ptr(), 2) };
        assert_eq!(returned, reassigned, "rt_set_sexpr_root returns its new-value argument unchanged");

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [reassigned];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));

        let popped = unsafe { rt_pop_sexpr_root(core::ptr::null(), 0) };
        assert_eq!(popped, reassigned, "popping the binding's root now yields the reassigned value, not the original");
    }

    /// The word of the type at `path` (walked down the module tree from the
    /// root) applied to the types `args` are the words of — what a compiled
    /// call site reads out of its type table and hands the shims.
    fn make_type(path: &[&str], args: &[i64]) -> i64 {
        use typelisp_mem::symbols::{child_ns, intern_in, NsId};
        let (name, modules) = path.split_last().expect("a path names something");
        let ns = modules.iter().fold(NsId::ROOT, |ns, seg| child_ns(ns, seg));
        let args: Vec<typelisp_mem::TypeId> = args.iter().map(|w| typelisp_mem::TypeId::from_word(*w).expect("a type's word")).collect();
        typelisp_mem::intern_type(typelisp_mem::TypeShape::Named(ns, intern_in(ns, name), args)).word()
    }

    /// Builds a tagged `Value::Str` via [`rt_str_new`] itself, rather than a
    /// direct `heap.alloc_string(...)` call — every test below needs
    /// pre-existing string content to exercise `rt_str_length`/`rt_str_ref`/
    /// `rt_str_eq`/`rt_str_append` against, and (per the existing
    /// `rt_heap_live_count_reflects_the_registered_heaps_real_state` test's
    /// own comment) once a `Heap` is registered via [`set_active_heap`], every
    /// further mutation must go through that same raw-pointer-derived access
    /// path (here, another `rt_*` call) rather than reborrowing the original
    /// `&mut heap` binding directly, which Stacked Borrows treats as
    /// invalidating the raw pointer.
    fn make_str(s: &str) -> i64 {
        let args: Vec<i64> = s.chars().map(|c| c as i64).collect();
        unsafe { rt_str_new(args.as_ptr(), args.len() as u32) }
    }

    #[test]
    fn rt_str_new_builds_a_string_from_raw_char_scalars() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("hi");
        match decode(tagged) {
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), "hi"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_str_new_with_no_characters_builds_an_empty_string() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_str_new(core::ptr::null(), 0) };
        match decode(tagged) {
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), ""),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_str_length_counts_unicode_scalar_values_not_bytes() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("héllo");
        assert_eq!(unsafe { rt_str_length([tagged].as_ptr(), 1) }, 5);
    }

    #[test]
    fn rt_str_ref_returns_the_nth_char_as_a_bare_scalar() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("hi");
        let args = [tagged, 1];
        assert_eq!(unsafe { rt_str_ref(args.as_ptr(), 2) }, 'i' as i64);
    }

    #[test]
    fn rt_str_eq_compares_content_not_identity() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("hi");
        let b = make_str("hi");
        let c = make_str("bye");
        assert_eq!(unsafe { rt_str_eq([a, b].as_ptr(), 2) }, 1, "separately allocated, equal content");
        assert_eq!(unsafe { rt_str_eq([a, c].as_ptr(), 2) }, 0);
    }

    #[test]
    fn rt_str_equalp_is_ascii_case_insensitive() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("Hello");
        let b = make_str("hello");
        let c = make_str("HELLO");
        let d = make_str("world");
        assert_eq!(unsafe { rt_str_equalp([a, b].as_ptr(), 2) }, 1, "differ only in case");
        assert_eq!(unsafe { rt_str_equalp([a, c].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_str_equalp([a, d].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_str_eq([a, b].as_ptr(), 2) }, 0, "plain `eq` still distinguishes case");
    }

    #[test]
    fn rt_char_equalp_is_ascii_case_insensitive() {
        // `char` args are raw code points, not tagged Sexprs — no active heap
        // is needed.
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'a' as i64].as_ptr(), 2) }, 1, "A equalp a");
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'A' as i64].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'b' as i64].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_char_equalp(['1' as i64, '1' as i64].as_ptr(), 2) }, 1, "non-letters compare by value");
    }

    #[test]
    fn rt_str_lt_orders_lexicographically_by_content() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("abc");
        let b = make_str("abd");
        let a2 = make_str("abc");
        assert_eq!(unsafe { rt_str_lt([a, b].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_str_lt([b, a].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_str_lt([a, a2].as_ptr(), 2) }, 0, "equal content is not strictly less");
    }

    #[test]
    fn rt_str_append_concatenates_into_a_fresh_string() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("foo");
        let b = make_str("bar");
        let tagged = unsafe { rt_str_append([a, b].as_ptr(), 2) };
        match decode(tagged) {
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), "foobar"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_struct_new_builds_a_boxed_struct_with_its_fields() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_type(&["point"], &[]);
        let x = encode(Value::Int(1));
        let y = encode(Value::Int(2));
        let args = [type_name, x, y];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 3) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(&*typelisp_mem::type_name(h.struct_type_key(id)), "point");
                assert_eq!(h.struct_field_count(id), 2);
            }
            other => panic!("expected a boxed struct, got {:?}", other),
        }

        let get_x = [tagged, 0];
        let get_y = [tagged, 1];
        assert_eq!(decode(unsafe { rt_struct_field_get(get_x.as_ptr(), 2) }), Value::Int(1));
        assert_eq!(decode(unsafe { rt_struct_field_get(get_y.as_ptr(), 2) }), Value::Int(2));
    }

    #[test]
    fn rt_struct_new_with_no_fields_builds_an_empty_struct() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_type(&["unit-struct"], &[]);
        let args = [type_name];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 1) };

        match decode(tagged) {
            Value::Boxed(id) => assert_eq!(unsafe { active_heap() }.struct_field_count(id), 0),
            other => panic!("expected a boxed struct, got {:?}", other),
        }
    }

    #[test]
    fn rt_data_new_builds_a_boxed_enum_with_its_variant_and_fields() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_type(&["Option"], &[]);
        let payload = encode(Value::Int(42));
        // args = [type-name, raw variant index, field0]
        let args = [type_name, 0, payload];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 3) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert!(h.is_enum(id));
                assert!(!h.is_struct(id), "an enum box must not read as a struct");
                assert_eq!(&*typelisp_mem::type_name(h.enum_type_key(id)), "Option");
                assert_eq!(h.enum_variant(id), 0);
                assert_eq!(h.enum_field_count(id), 1);
            }
            other => panic!("expected a boxed enum, got {:?}", other),
        }

        assert_eq!(unsafe { rt_data_variant([tagged].as_ptr(), 1) }, 0);
        assert_eq!(decode(unsafe { rt_data_field([tagged, 0].as_ptr(), 2) }), Value::Int(42));
    }

    #[test]
    fn rt_data_new_with_no_fields_builds_a_nullary_variant() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_type(&["Option"], &[]);
        let args = [type_name, 1];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 2) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(h.enum_variant(id), 1);
                assert_eq!(h.enum_field_count(id), 0);
            }
            other => panic!("expected a boxed enum, got {:?}", other),
        }
        assert_eq!(unsafe { rt_data_variant([tagged].as_ptr(), 1) }, 1);
    }

    /// A rooted enum box keeps its heap-referencing field alive across a
    /// collection — the mark-phase fan-out `push_boxed_nested`'s `Enum` arm
    /// provides, mirroring what a struct's fields already get.
    #[test]
    fn a_rooted_enum_box_keeps_its_cons_field_alive_across_gc() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cell = heap.cons(Value::Int(7), Value::Empty).expect("cons");
        let type_name = make_type(&["Option"], &[]);
        let args = [type_name, 0, encode(cell)];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 3) };
        let boxed = decode(tagged);

        heap.push_root(boxed);
        heap.gc();

        let id = match boxed {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed enum, got {:?}", other),
        };
        match unsafe { active_heap() }.enum_field(id, 0) {
            Value::Cons(_) => {
                assert_eq!(heap.car(unsafe { active_heap() }.enum_field(id, 0)).expect("car"), Value::Int(7));
            }
            other => panic!("expected the cons field to survive, got {:?}", other),
        }
    }

    #[test]
    fn rt_closure_new_round_trips_fnptr_mask_and_slots() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        // slot 0 (masked): a tagged int; slot 1 (unmasked): raw float bits,
        // stored and handed back verbatim.
        let tagged_int = encode(Value::Int(7));
        let raw_bits = (1.5f64).to_bits() as i64;
        let fn_ptr = 0x1000i64; // opaque at this layer — only stored
        let args = [fn_ptr, 0b01, tagged_int, raw_bits];
        let clo = unsafe { rt_closure_new(args.as_ptr(), 4) };

        match decode(clo) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert!(h.is_compiled_closure(id));
                assert_eq!(h.compiled_closure_mask(id), 0b01);
            }
            other => panic!("expected a boxed compiled closure, got {:?}", other),
        }
        assert_eq!(unsafe { rt_closure_fnptr([clo].as_ptr(), 1) }, fn_ptr);
        assert_eq!(unsafe { rt_closure_env_len([clo].as_ptr(), 1) }, 2);
        assert_eq!(unsafe { rt_closure_env_get([clo, 0].as_ptr(), 2) }, tagged_int);
        assert_eq!(unsafe { rt_closure_env_get([clo, 1].as_ptr(), 2) }, raw_bits);
    }

    /// A rooted compiled closure keeps its captured cell — and through it
    /// the cell's contents — alive across a collection: the mark-phase
    /// fan-out `push_boxed_nested`'s `CompiledClosure` arm. (Its interpreted
    /// twin `BoxedObj::Closure` has an arm of its own, tracing `params`/
    /// `ret`/`body`/`env`; `tests/mem_test.rs` covers that one.)
    #[test]
    fn a_rooted_compiled_closure_keeps_its_captured_cell_alive_across_gc() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cons = heap.cons(Value::Int(7), Value::Empty).expect("cons");
        let cell = unsafe { rt_cell_new([encode(cons)].as_ptr(), 1) };
        let clo = unsafe { rt_closure_new([0x1000, 0b1, cell].as_ptr(), 3) };
        let boxed = decode(clo);

        heap.push_root(boxed);
        heap.gc();

        let got_cell = unsafe { rt_closure_env_get([clo, 0].as_ptr(), 2) };
        assert_eq!(got_cell, cell, "the captured cell slot survives verbatim");
        let contents = unsafe { rt_cell_get([got_cell].as_ptr(), 1) };
        assert_eq!(heap.car(decode(contents)).expect("car"), Value::Int(7));
    }

    #[test]
    fn rt_cell_set_mutates_the_shared_cell_both_worlds_see() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cell = unsafe { rt_cell_new([encode(Value::Int(1))].as_ptr(), 1) };
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(1)));
        assert_eq!(unsafe { rt_cell_set([cell, encode(Value::Int(2))].as_ptr(), 2) }, 0, "cell-set returns Unit (0)");
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(2)));

        // The very same heap object is what the interpreter's own cell API
        // sees — a compiled `setf` and an interp `setf` mutate one binding.
        let id = match decode(cell) {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed cell, got {:?}", other),
        };
        let h = unsafe { active_heap() };
        assert!(h.is_cell(id));
        assert_eq!(h.cell_get(id), Value::Int(2));
        h.cell_set(id, Value::Int(3));
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(3)));
    }

    /// An unreachable compiled closure is swept like any other box. (Neither
    /// closure box needs a swept-token report to an interpreter side table
    /// any more: `CompiledClosure` carries its env inline, and the
    /// interpreted `BoxedObj::Closure` carries its own code as heap data. The
    /// whole dead-token mechanism went with the side table.)
    #[test]
    fn an_unreachable_compiled_closure_is_swept() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let clo = unsafe { rt_closure_new([0x1000, 0].as_ptr(), 2) };
        let id = match decode(clo) {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed compiled closure, got {:?}", other),
        };
        heap.gc(); // nothing roots it

        assert!(!heap.is_compiled_closure(id), "the unrooted closure box was swept");
    }

    #[test]
    fn rt_struct_field_set_mutates_in_place() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_type(&["point"], &[]);
        let args = [type_name, encode(Value::Int(1)), encode(Value::Int(2))];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 3) };

        let set_args = [tagged, 0, encode(Value::Int(99))];
        assert_eq!(unsafe { rt_struct_field_set(set_args.as_ptr(), 3) }, 0, "field-set returns Unit (0)");

        let get_x = [tagged, 0];
        let get_y = [tagged, 1];
        assert_eq!(decode(unsafe { rt_struct_field_get(get_x.as_ptr(), 2) }), Value::Int(99));
        assert_eq!(decode(unsafe { rt_struct_field_get(get_y.as_ptr(), 2) }), Value::Int(2), "the other field is untouched");
    }

    #[test]
    fn rt_struct_field_count_returns_the_raw_field_count() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let empty = unsafe { rt_struct_new([make_type(&["Vector"], &[])].as_ptr(), 1) };
        assert_eq!(unsafe { rt_struct_field_count([empty].as_ptr(), 1) }, 0, "empty vector has length 0");

        let three = [make_type(&["Vector"], &[]), encode(Value::Int(10)), encode(Value::Int(20)), encode(Value::Int(30))];
        let tagged = unsafe { rt_struct_new(three.as_ptr(), 4) };
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 3, "raw count, not a tagged Sexpr Int");
    }

    #[test]
    fn rt_hashtable_new_set_count_and_entries_round_trip() {
        let mut heap = Heap::with_capacity(64);
        set_active_heap(&mut heap as *mut Heap);

        let ht = unsafe { rt_hashtable_new([make_type(&["HashTable"], &[make_type(&["i32"], &[]), make_type(&["i32"], &[])]), encode(Value::Empty), encode(Value::Empty)].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 0, "a fresh map is empty");

        // Two int->int entries (keys/values as raw tagged `Sexpr` ints). The
        // hash and the index are the caller's: this layer never computes
        // either, which is what lets a `defstruct` be a key.
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 1, 0, encode(Value::Int(1)), encode(Value::Int(10))].as_ptr(), 5) }, 0);
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 2, 0, encode(Value::Int(2)), encode(Value::Int(20))].as_ptr(), 5) }, 0);
        // Writing at an index that already exists overwrites rather than
        // appending, so the count doesn't grow.
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 1, 0, encode(Value::Int(1)), encode(Value::Int(99))].as_ptr(), 5) }, 0);
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 2, "two distinct keys");

        // `keys` builds a `Vector` whose field count matches the entry count.
        let keys = unsafe { rt_hashtable_keys([ht, make_type(&["Vector"], &[make_type(&["i32"], &[])])].as_ptr(), 2) };
        match decode(keys) {
            Value::Boxed(id) => assert_eq!(unsafe { active_heap() }.struct_field_count(id), 2),
            other => panic!("expected a boxed vector, got {:?}", other),
        }

        // `entries` builds a `Vector` of `cons-cell`s, one per entry.
        let entries = unsafe { rt_hashtable_entries([ht, make_type(&["Vector"], &[make_type(&["std", "sexpr", "cons-cell"], &[make_type(&["i32"], &[]), make_type(&["i32"], &[])])])].as_ptr(), 2) };
        match decode(entries) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(h.struct_field_count(id), 2, "one cons-cell per entry");
                // Each element is itself a boxed cons-cell (2 fields: car, cdr).
                match h.struct_field(id, 0) {
                    Value::Boxed(cell) => assert_eq!(h.struct_field_count(cell), 2, "a cons-cell has car and cdr"),
                    other => panic!("expected a boxed cons-cell, got {:?}", other),
                }
            }
            other => panic!("expected a boxed vector, got {:?}", other),
        }
    }

    /// Two keys that share a hash sit in one bucket, in insertion order, and
    /// deleting one keeps the other — the collision handling the prelude's
    /// `get`/`set`/`remove` walk with the key type's own `equals`.
    #[test]
    fn rt_hashtable_bucket_holds_colliding_keys_and_deletes_one() {
        let mut heap = Heap::with_capacity(64);
        set_active_heap(&mut heap as *mut Heap);

        let ht = unsafe { rt_hashtable_new([make_type(&["HashTable"], &[make_type(&["i32"], &[]), make_type(&["i32"], &[])]), encode(Value::Empty), encode(Value::Empty)].as_ptr(), 3) };
        let hash = 5;
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 0, "no bucket yet");

        unsafe { rt_hashtable_bucket_put([ht, hash, 0, encode(Value::Int(7)), encode(Value::Int(70))].as_ptr(), 5) };
        unsafe { rt_hashtable_bucket_put([ht, hash, 1, encode(Value::Int(8)), encode(Value::Int(80))].as_ptr(), 5) };
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 2);
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 2, "the table counts every bucket");

        assert_eq!(decode(unsafe { rt_hashtable_bucket_key([ht, hash, 0].as_ptr(), 3) }), Value::Int(7));
        assert_eq!(decode(unsafe { rt_hashtable_bucket_value([ht, hash, 1].as_ptr(), 3) }), Value::Int(80));

        unsafe { rt_hashtable_bucket_delete([ht, hash, 0].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 1, "the other survives");
        assert_eq!(decode(unsafe { rt_hashtable_bucket_key([ht, hash, 0].as_ptr(), 3) }), Value::Int(8), "and moves up");

        unsafe { rt_hashtable_bucket_delete([ht, hash, 0].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 0, "an emptied bucket is dropped");
    }

    #[test]
    fn rt_struct_push_field_grows_the_struct_and_appends_at_the_end() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_struct_new([make_type(&["Vector"], &[])].as_ptr(), 1) };
        assert_eq!(unsafe { rt_struct_push_field([tagged, encode(Value::Int(7))].as_ptr(), 2) }, 0, "push returns Unit (0)");
        assert_eq!(unsafe { rt_struct_push_field([tagged, encode(Value::Int(8))].as_ptr(), 2) }, 0);

        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 2, "grew from 0 to 2");
        assert_eq!(decode(unsafe { rt_struct_field_get([tagged, 0].as_ptr(), 2) }), Value::Int(7));
        assert_eq!(decode(unsafe { rt_struct_field_get([tagged, 1].as_ptr(), 2) }), Value::Int(8), "second push lands at index 1");
    }

    #[test]
    fn rt_struct_pop_field_shrinks_the_struct_and_returns_the_last_element() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_struct_new([make_type(&["Vector"], &[])].as_ptr(), 1) };
        unsafe { rt_struct_push_field([tagged, encode(Value::Int(7))].as_ptr(), 2) };
        unsafe { rt_struct_push_field([tagged, encode(Value::Int(8))].as_ptr(), 2) };

        assert_eq!(decode(unsafe { rt_struct_pop_field([tagged].as_ptr(), 1) }), Value::Int(8), "pop returns the last-pushed element");
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 1, "shrank from 2 to 1");
        assert_eq!(decode(unsafe { rt_struct_pop_field([tagged].as_ptr(), 1) }), Value::Int(7));
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 0, "empty after popping both elements");
        // Popping past empty aborts the process (panicking across an `extern
        // "C"` boundary can't unwind — the same "abort, don't unwind"
        // convention every other `rt_*` bounds violation follows, see
        // `compiler.rs`'s `compile-vector-op` doc comment), so it isn't
        // exercised here with `#[should_panic]`; [`Heap::struct_pop_field`]'s
        // own doc comment covers the emptiness check.
    }

    /// A struct field that itself holds a cons must survive a GC the same
    /// way a cons reachable through *another* cons's `car`/`cdr` does — the
    /// direct test that `Heap::gc`'s mark phase actually traces into a
    /// `BoxedObj::Struct`'s fields (`push_boxed_nested`), not just marks the
    /// box slot itself and stops.
    #[test]
    fn gc_traces_into_a_rooted_structs_fields() {
        let mut heap = Heap::with_capacity(16);
        set_active_heap(&mut heap as *mut Heap);

        let inner_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let inner_cons = unsafe { rt_cons(inner_args.as_ptr(), 2) };

        let type_name = make_type(&["wrapper"], &[]);
        let struct_args = [type_name, inner_cons];
        let boxed = unsafe { rt_struct_new(struct_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&boxed as *const i64, 1) };

        // Exhaust the remaining free cells with unrelated garbage to force a
        // real GC.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let get_field = [boxed, 0];
        let field = unsafe { rt_struct_field_get(get_field.as_ptr(), 2) };
        let one_arg = [field];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// The negative-case counterpart to the test above — an *unrooted*
    /// struct's field-held cons is not protected from a GC triggered by
    /// other allocations, confirming the previous test's positive result
    /// isn't a coincidence of a GC never actually running.
    #[test]
    fn gc_reclaims_an_unrooted_structs_fields() {
        let mut heap = Heap::with_capacity(16);
        set_active_heap(&mut heap as *mut Heap);

        let inner_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let inner_cons = unsafe { rt_cons(inner_args.as_ptr(), 2) };

        let type_name = make_type(&["wrapper"], &[]);
        let struct_args = [type_name, inner_cons];
        let _boxed = unsafe { rt_struct_new(struct_args.as_ptr(), 2) };
        // Deliberately not rooted.

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }
        unsafe { active_heap() }.gc();

        // The struct itself is unreachable, so its box slot was recycled by
        // the sweep above — and with it, the only path that kept the
        // field's cons cell (already exercised as garbage by the unrelated
        // allocations, but this pins down the box side specifically).
        assert_eq!(unsafe { active_heap() }.box_count(), 0, "the unrooted struct was reclaimed");
    }

    /// Round-trips a global variable through `rt_global_new`/`rt_global_get`
    /// — the id `rt_global_new` returns is what later `rt_global_get`/
    /// `rt_global_set` calls address it by.
    #[test]
    fn rt_global_new_and_rt_global_get_round_trip() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let initial = [encode(Value::Int(42))];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };
        assert_eq!(id, 0, "the first global on a freshly reset table gets id 0");

        let id_args = [id];
        assert_eq!(decode(unsafe { rt_global_get(id_args.as_ptr(), 1) }), Value::Int(42));
    }

    /// `rt_global_set` overwrites a global's value in place; a later
    /// `rt_global_get` sees the new value.
    #[test]
    fn rt_global_set_overwrites_in_place() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let initial = [encode(Value::Int(1))];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };

        let set_args = [id, encode(Value::Int(99))];
        let returned = unsafe { rt_global_set(set_args.as_ptr(), 2) };
        assert_eq!(decode(returned), Value::Int(99), "rt_global_set returns its new value unchanged");

        let id_args = [id];
        assert_eq!(decode(unsafe { rt_global_get(id_args.as_ptr(), 1) }), Value::Int(99));
    }

    /// A global's id (`rt_global_new`'s return value) is a small sequential
    /// counter driven purely by `rt_global_new` call order — an unrelated
    /// permanent-root push (`rt_push_permanent_sexpr_root`, what ordinary
    /// boxed-struct field construction uses) interleaved between two
    /// globals must not perturb that numbering, even though both features
    /// share the same underlying `Heap::permanent_roots` storage.
    #[test]
    fn rt_global_ids_are_sequential_regardless_of_interleaved_permanent_roots() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let first = [encode(Value::Int(10))];
        let first_id = unsafe { rt_global_new(first.as_ptr(), 1) };

        let unrelated = [encode(Value::Int(-1))];
        unsafe { rt_push_permanent_sexpr_root(unrelated.as_ptr(), 1) };

        let second = [encode(Value::Int(20))];
        let second_id = unsafe { rt_global_new(second.as_ptr(), 1) };

        assert_eq!((first_id, second_id), (0, 1));
        let first_args = [first_id];
        let second_args = [second_id];
        assert_eq!(decode(unsafe { rt_global_get(first_args.as_ptr(), 1) }), Value::Int(10));
        assert_eq!(decode(unsafe { rt_global_get(second_args.as_ptr(), 1) }), Value::Int(20));
    }

    /// A global survives a forced `gc()` triggered by unrelated allocations
    /// with no matching pop at all — the same permanent-root protection
    /// [`rt_push_permanent_sexpr_root_protects_a_value_across_a_gc_with_no_matching_pop`]
    /// proves for its own direct use of `Heap::push_permanent_root`.
    #[test]
    fn rt_global_protects_its_value_across_a_gc_triggered_by_other_allocations() {
        reset_global_table();
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let initial = [precious];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let id_args = [id];
        let surviving = unsafe { rt_global_get(id_args.as_ptr(), 1) };
        let one_arg = [surviving];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }
}
