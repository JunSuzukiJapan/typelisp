//! The shared Rust-only runtime library compiled code calls into directly —
//! the destination for builtins that genuinely can't be written in typelisp
//! (direct cons-heap access, raw GC-heap bookkeeping; see `docs/TODO.md`'s
//! "Sexpr表現 + Match/Construct/共有Rustライブラリ 実装計画" section for the
//! full plan this module is Stage 0/1 of).
//!
//! Every function here is declared under the exact same ABI as a compiled
//! typelisp function (see [`crate::compile::CompiledSignature`]) —
//! `unsafe extern "C" fn(*const i64, u32) -> i64` — so the existing
//! cross-function-call wiring (JIT: `add_global_mapping`'s `externals`
//! list in [`crate::compile::CompiledFn::new`]; AOT: ordinary linker symbol
//! resolution against this crate's `staticlib` artifact, see `aot.rs`'s
//! `write_executable`) treats a call to one of these exactly like a call to
//! another already-compiled typelisp function — no new call mechanism is
//! needed, only `#[no_mangle]` so the linker sees a plain, unmangled C
//! symbol name.
//!
//! `rt_ping` is Stage 0's deliberately trivial placeholder: it exists only
//! to prove that *some* Rust function defined in this crate is callable
//! from both a JIT-compiled function (via `add_global_mapping`) and an
//! AOT-linked native executable (via the system linker) before any real
//! heap/Sexpr machinery is built on top — see
//! `aot_output_can_call_an_rt_extern_function` (`aot.rs`) and
//! `jit_can_call_an_rt_extern_function` (this module's own tests) for the
//! two halves of that proof.

/// Returns `args[0] + 1` if `argc >= 1`, otherwise `0`. No real runtime
/// behavior depends on this — see the module doc comment.
///
/// # Safety
///
/// If `argc >= 1`, `args` must be non-null and point to at least one valid,
/// readable `i64` — exactly what every compiled-function call site already
/// guarantees for its own `args` array (see [`crate::compile::CompiledSignature`]).
#[no_mangle]
pub unsafe extern "C" fn rt_ping(args: *const i64, argc: u32) -> i64 {
    if argc >= 1 {
        *args + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::rt_ping;

    #[test]
    fn rt_ping_adds_one_to_its_first_argument() {
        let args = [41i64];
        assert_eq!(unsafe { rt_ping(args.as_ptr(), 1) }, 42);
    }

    #[test]
    fn rt_ping_returns_zero_with_no_arguments() {
        assert_eq!(unsafe { rt_ping(std::ptr::null(), 0) }, 0);
    }
}
