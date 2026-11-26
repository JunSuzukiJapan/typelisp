//! The (typelisp-hosted) `compile`/`compile-file` compiler's view of LLVM:
//! five built-in types whose methods the runtime implements
//! (`eval_llvm_builtin_method` in `crate::eval::interp`). `llvm-basic-block`
//! and `llvm-value` have no methods of their own — a block is only ever
//! produced by `append-block` and consumed by `position-at-end` — but they
//! are types so they have a `Path` to appear as a parameter or a result. In
//! `%core`, the compiler island's module: nothing outside the compiler works
//! with these, so the standard library does not offer them.

use super::super::registry::AdtKind;
use crate::types::{Path, Type};
use typelisp_mem::wk;

use super::super::registry::Registry;
use super::BuiltinType;

/// The five LLVM types.
pub(super) fn define(reg: &mut Registry) {
    module(reg);
    function(reg);
    builder(reg);
    basic_block(reg);
    value(reg);
}

fn module_ty() -> Type {
    Type::Named(Path::builtin(wk::LLVM_MODULE), vec![])
}

fn function_ty() -> Type {
    Type::Named(Path::builtin(wk::LLVM_FUNCTION), vec![])
}

fn basic_block_ty() -> Type {
    Type::Named(Path::builtin(wk::LLVM_BASIC_BLOCK), vec![])
}

fn builder_ty() -> Type {
    Type::Named(Path::builtin(wk::LLVM_BUILDER), vec![])
}

fn value_ty() -> Type {
    Type::Named(Path::builtin(wk::LLVM_VALUE), vec![])
}

/// An in-progress LLVM module. `add-function` always declares a function
/// under one fixed C ABI — `i64 name(i64* args, i32 argc)` — regardless of
/// the typelisp-level function's actual arity; `llvm-builder::load-arg`
/// reads a logical parameter back out of that array. Fixing the ABI this
/// way (rather than generating a distinct LLVM signature per arity) is what
/// lets `compile::CompiledFn` call *any* compiled function through one Rust
/// function-pointer type — other return types are a later phase's concern.
fn module(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::LLVM_MODULE), AdtKind::Sum, &[]);
    def.associated("create", vec![Type::Str], module_ty());
    def.instance("add-function", vec![module_ty(), Type::Str], function_ty());
    def.instance("verify", vec![module_ty()], Type::Bool);
    def.instance("to-string", vec![module_ty()], Type::Str);
    // `get-function`: look up an already-declared `llvm-function` by name in
    // this module — the core primitive direct calls (self-recursion,
    // `labels`-sibling calls, top-level `defun`-to-`defun` calls) are built
    // on, since every callee in those cases is something this same module
    // already added via `add-function` before the caller's body is compiled.
    def.instance("get-function", vec![module_ty(), Type::Str], function_ty());
    // `c-long-bits`: how wide C's `long` is on the machine the module is for
    // (`typelisp_mem::target::DataModel`) — the width of a `c-long`, which
    // the width casts are told like any other integer's.
    def.instance("c-long-bits", vec![module_ty()], Type::Int);
    // `add-function-with-env`: declares a function under the *extended*
    // ABI a `labels` block with outer-scope captures needs — `i64 name(i64*
    // args, i32 argc, i64* env, i32 env_len)` — used instead of
    // `add-function` exactly when that block's shared captured-name list
    // (`check::freevars::free_vars`) is non-empty. Every sibling in
    // such a block shares this one extended signature, even ones whose own
    // body doesn't reference every captured name (see `compiler.rs`'s
    // `compile-labels` doc comment for why captures aren't computed
    // per-sibling).
    def.instance("add-function-with-env", vec![module_ty(), Type::Str], function_ty());
    // The coroutine ABI (Phase C2): `i64 f(i64 frame)`. One parameter, because
    // entering a function and resuming it have to be the same call — otherwise
    // every indirect call would have to know which of the two it was making.
    // What comes back is a status word, not the value; the value is in the
    // frame, which is what makes stopping mid-body sayable at all.
    def.instance("add-coroutine-function", vec![module_ty(), Type::Str], function_ty());
}

/// A declared LLVM function (a `Module::add-function` result).
fn function(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::LLVM_FUNCTION), AdtKind::Sum, &[]);
    def.instance("append-block", vec![function_ty(), Type::Str], basic_block_ty());
    // A function's raw LLVM parameter — distinct from `load-arg`, which reads a
    // *logical* argument out of the `i64*` array the old ABI passes. Under the
    // coroutine ABI the frame is a real parameter, so there is no array.
    def.instance("function-param", vec![function_ty(), Type::Int], value_ty());
}

/// An LLVM basic block. No methods of its own yet (Phase 0) — produced by
/// `llvm-function::append-block`, consumed by `llvm-builder::position-at-end`.
fn basic_block(reg: &mut Registry) {
    BuiltinType::new(reg, Path::builtin(wk::LLVM_BASIC_BLOCK), AdtKind::Sum, &[]);
}

/// An IR builder. `load-arg` reads logical parameter `index` out of a
/// function's fixed-ABI argument array (see `module`'s doc
/// comment); `build-add`/`build-sub`/`build-mul` cover Phase 1's scalar
/// arithmetic (comparisons, which return `bool` rather than `i64`, are a
/// later phase's concern — keeping every builtin here `i64`-in-`i64`-out
/// for now).
fn builder(reg: &mut Registry) {
    let mut def = BuiltinType::new(reg, Path::builtin(wk::LLVM_BUILDER), AdtKind::Sum, &[]);
    def.associated("create", vec![], builder_ty());
    def.instance("position-at-end", vec![builder_ty(), basic_block_ty()], Type::Unit);
    // `const-word`: an LLVM 64-bit constant. The *generated* code's word is 64
    // bits whatever the source language's integer types are, so this builtin
    // outlives `i64` (the name it used to carry, which claimed a source type
    // that no longer exists). Its argument is an `i32` because every caller
    // passes something small — a tag mask, a slot index, `0`/`1`/`-1`; the two
    // places that need a full-width constant (`compile-int`/`compile-float`)
    // assemble it from two 32-bit halves in LLVM instead.
    def.instance("const-word", vec![builder_ty(), Type::Int], value_ty());
    def.instance("build-ret", vec![builder_ty(), value_ty()], Type::Unit);
    def.instance("load-arg", vec![builder_ty(), function_ty(), Type::Int], value_ty());
    def.instance("build-add", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    def.instance("build-sub", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    def.instance("build-mul", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    // Stage 2 of the Sexpr-representation plan (`docs/implementation-log.md`): bitwise
    // primitives for packing/unpacking the tagged `i64` representation of a
    // compiled `Sexpr` value (3 low tag bits + payload — see that doc's
    // tag table). Deliberately generic, type-agnostic `i64`-in-`i64`-out
    // ops, the same as `build-add`/`build-sub`/`build-mul` — the actual
    // tagging/untagging logic is plain typelisp in `compiler.rs`, built out
    // of these, the same way `compile-if`/`compile-loop` are built out of
    // `build-icmp-*`/`build-cond-br`.
    def.instance("build-and", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    def.instance("build-or", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    // `xor` (CL `logxor`, plus `lognot`'s `(xor n -1)` — a bare LLVM
    // instruction like `build-and`/`build-or`).
    def.instance("build-xor", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    // `(select cond then else)`: `max`/`min` (integers, `bignum`, `ratio`) all
    // lower to this — see `llvm_builder_build_select`'s doc comment.
    def.instance("build-select", vec![builder_ty(), value_ty(), value_ty(), value_ty()], value_ty());
    def.instance("build-shl", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    // Logical (unsigned) vs. arithmetic (sign-extending) right shift: the
    // tagged representation's payload bits must never be sign-extended back
    // in when shifting a Cons/Symbol/Str/Path pointer or index out from
    // under its tag (`build-lshr`), but a genuine signed `Sexpr::i32`
    // fixnum's sign bit must survive untagging (`build-ashr`) or negative
    // integers would come back corrupted.
    def.instance("build-lshr", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    def.instance("build-ashr", vec![builder_ty(), value_ty(), value_ty()], value_ty());
    // The numeric builders (`compile::llvm_builtins::numeric`): one method
    // of a fixed-width integer (its `wsig`) or a float (its width in bits),
    // computed at the type's own width. The method is named by its spelling.
    def.instance("build-fixed-binop", vec![builder_ty(), Type::Int, Type::Str, value_ty(), value_ty()], value_ty());
    def.instance("build-fixed-div-check", vec![builder_ty(), Type::Int, Type::Str, value_ty(), value_ty()], value_ty());
    def.instance("build-fixed-unary", vec![builder_ty(), module_ty(), Type::Int, Type::Str, value_ty()], value_ty());
    def.instance("build-float-binop", vec![builder_ty(), module_ty(), Type::Int, Type::Str, value_ty(), value_ty()], value_ty());
    def.instance("build-float-unary", vec![builder_ty(), module_ty(), Type::Int, Type::Str, value_ty()], value_ty());
    // `int`'s overflow-checked `+`/`-`/`*` on two fixnum words: the result,
    // with the overflow flag stored into a place — see
    // `llvm_builder_build_int_overflow`.
    for name in ["build-sadd-overflow", "build-ssub-overflow", "build-smul-overflow"] {
        def.instance(name, vec![builder_ty(), module_ty(), value_ty(), value_ty(), value_ty()], value_ty());
    }
    // `load-env` reads logical captured slot `index` out of the env array of a
    // function declared via `add-function-with-env`, as `load-arg` reads the
    // args one.
    def.instance("load-env", vec![builder_ty(), function_ty(), Type::Int], value_ty());
    // The coroutine prologue (Phase C2). The island that compiles the *next*
    // island is the one already committed: it calls these builtins through
    // `rt_llvm_call` with the arity it was built against, so changing an
    // existing signature breaks the bootstrap at the one moment it cannot be
    // repaired from.
    //
    // `coroutine-begin` takes the function as well as the builder — it appends
    // the prologue's blocks — and leaves the builder at the start of the body.
    // `coroutine-call` returns the callee's value, having split the block
    // underneath the caller. `coroutine-end` takes the body's value because
    // under this ABI the `ret` carries the status, not the answer.
    def.instance("coroutine-begin", vec![builder_ty(), module_ty(), function_ty()], Type::Unit);
    // `coroutine-suspend` is `coroutine-call` with the callee left out: set the
    // resume point, hand control back with `STATUS_SUSPEND`, and answer with
    // whatever the task was waiting for once it is resumed. The suspending
    // builtin's own shim (`rt_suspend_*`) has already been called by then and
    // has recorded *what* is being waited for, so nothing about that needs to
    // reach this builtin.
    // Where the code being built keeps values (`llvm_builtins::slots`).
    def.associated("no-value", vec![], value_ty());
    def.instance("arg-list", vec![builder_ty(), Type::Int], value_ty());
    def.instance("temp-slot", vec![builder_ty()], value_ty());
    def.instance("scratch-slot", vec![builder_ty(), Type::Int], value_ty());
    def.instance("frame-slot", vec![builder_ty()], value_ty());
    def.instance("frame-slot-rooted", vec![builder_ty()], value_ty());
    def.instance("frame-slots", vec![builder_ty()], value_ty());
    def.instance("add-frame-slot", vec![builder_ty(), value_ty()], Type::Unit);
    def.instance("add-frame-slot-rooted", vec![builder_ty(), value_ty()], Type::Unit);
    def.instance("slot-set", vec![builder_ty(), value_ty(), Type::Int, value_ty()], Type::Unit);
    def.instance("slot-get", vec![builder_ty(), value_ty(), Type::Int], value_ty());
    // Constants, conversions, and the tag of a field's value
    // (`llvm_builtins::values`). An `int` here is how a value is carried
    // (`Carrier::code`), or a field kind.
    def.instance("const-value", vec![builder_ty(), Type::Int, Type::Int, Type::Int], value_ty());
    def.instance("const-like", vec![builder_ty(), value_ty(), Type::Int, Type::Int], value_ty());
    def.instance("convert", vec![builder_ty(), Type::Int, Type::Int, value_ty()], value_ty());
    def.instance("coerce", vec![builder_ty(), Type::Int, value_ty()], value_ty());
    def.instance("build-not", vec![builder_ty(), value_ty()], value_ty());
    // `build-size-of`: how many bytes a value of `v`'s LLVM type takes, as a
    // `u32` — LLVM's own `sizeof`, which it works out for the machine the
    // module is for once it knows that machine.
    def.instance("build-size-of", vec![builder_ty(), value_ty()], value_ty());
    def.instance("tag-field", vec![builder_ty(), module_ty(), value_ty(), Type::Int], value_ty());
    def.instance("untag-field", vec![builder_ty(), module_ty(), value_ty(), Type::Int], value_ty());
    // A global variable, by its id and its field kind (`compile::globals`).
    def.instance("global-get", vec![builder_ty(), module_ty(), Type::Int, Type::Int], value_ty());
    def.instance("global-put", vec![builder_ty(), module_ty(), Type::Int, Type::Int, value_ty()], value_ty());
    def.instance("global-new", vec![builder_ty(), module_ty(), Type::Int, Type::Int, value_ty()], value_ty());
    // Calls: what is called, the slots its arguments are in, and, where the
    // answer comes back in a word, how it is carried.
    def.instance("build-call", vec![builder_ty(), function_ty(), value_ty()], value_ty());
    def.instance("build-make-closure", vec![builder_ty(), module_ty(), function_ty(), value_ty(), Type::Int, Type::Int], value_ty());
    def.instance("coroutine-call", vec![builder_ty(), module_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    def.instance("coroutine-call-env", vec![builder_ty(), module_ty(), value_ty(), value_ty(), Type::Int, value_ty()], value_ty());
    def.instance("coroutine-apply", vec![builder_ty(), module_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    def.instance("coroutine-dyn-call", vec![builder_ty(), module_ty(), value_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    def.instance("entry-value", vec![builder_ty(), module_ty(), Type::Str, Type::Int, Type::Int], value_ty());
    def.instance("coroutine-suspend", vec![builder_ty(), module_ty()], value_ty());
    // `frame-set-handler`/`frame-clear-handler`: write the frame's unwind
    // handler slot (`typelisp_abi::FRAME_HANDLER_SLOT`) — the `pc` of a
    // region's dispatch block, or `0` for none. The block is named rather
    // than the id, because the id is the builtin's own bookkeeping: the same
    // dispatch chain that resumes a call resumes a handler, and only the
    // builder knows what ids it has handed out.
    def.instance("frame-set-handler", vec![builder_ty(), module_ty(), basic_block_ty()], Type::Unit);
    def.instance("frame-clear-handler", vec![builder_ty(), module_ty()], Type::Unit);
    // `coroutine-unwind`: end the activation with `STATUS_UNWIND` — what a
    // region that declines the unwind in flight does when no enclosing region
    // is left in this function. The driver pops the frame and carries on
    // looking, which is the dynamic half of `emit-unwind-onward`.
    def.instance("coroutine-unwind", vec![builder_ty(), module_ty()], Type::Unit);
    def.instance("coroutine-end", vec![builder_ty(), module_ty(), value_ty()], Type::Unit);
    // `entry-safepoint`: a function's entry, its arguments in its frame.
    def.instance("entry-safepoint", vec![builder_ty(), module_ty()], Type::Unit);
    // `loop-safepoint`: a loop's back edge, offering the scheduler a turn on
    // the way in a coroutine.
    def.instance("loop-safepoint", vec![builder_ty(), module_ty(), basic_block_ty()], Type::Unit);
    // `build-icmp-lt`/`-le`/`-gt`/`-ge`/`-eq`/`-ne`: `i64` comparisons —
    // `compiler.rs`'s `compile-assoc` dispatches `<`/`<=`/`>`/`>=`/(`=`,`eq`)/`/=`
    // to these (`if`/comparisons work, labels/closures Stage 5). Each widens
    // the underlying `icmp` instruction's `i1` result back to `i64` (0/1) via
    // `build_int_z_extend`, matching every other builtin's "every compiled
    // value is a plain i64" convention (see `llvm_module_def`'s doc comment)
    // — keeping a `bool` result indistinguishable in representation from any
    // other `i64` is what lets `compile-if`'s `build-cond-br` (just below)
    // accept either one uniformly.
    for name in ["build-icmp-lt", "build-icmp-le", "build-icmp-gt", "build-icmp-ge", "build-icmp-eq", "build-icmp-ne"] {
        def.instance(name, vec![builder_ty(), value_ty(), value_ty()], value_ty());
    }
    // `build-cond-br`: branches to `then`/`else` depending on whether `cond`
    // (an ordinary `i64`-valued `llvm-value`, typically a `build-icmp-*`
    // result or a `bool` literal) is zero — `compile-if`'s primitive.
    def.instance("build-cond-br", vec![builder_ty(), value_ty(), basic_block_ty(), basic_block_ty()], Type::Unit);
    // `build-br`: an unconditional branch — `compile-if`'s then/else arms use
    // this to join back at the merge block.
    def.instance("build-br", vec![builder_ty(), basic_block_ty()], Type::Unit);
    // `block-terminated?` (`loop`/`break`/`return`): whether the builder's
    // *current* insertion block already ends in a terminator instruction —
    // a host-level query (returns a real `Bool`, not a compiled `i64` the
    // way every `build-*` primitive above does), the same flavor as
    // `llvm-module::verify`. `compile-if`'s branches and `compile-loop-body`'s
    // statement sequencing both need this: a `break`/`return` reached
    // directly (or via a taken `if` branch) already ends the current block
    // with its own unconditional branch to the loop's exit block, and LLVM
    // allows only one terminator per block — anything that would otherwise
    // unconditionally append more instructions after compiling a sub-form
    // (`compile-if`'s store-then-branch-to-merge, `compile-loop-body`'s next
    // statement) must check this first and skip emitting if it's already
    // true, or the resulting IR is malformed.
    def.instance("block-terminated?", vec![builder_ty()], Type::Bool);
    // `build-fn-address`: a declared function's address, as a word. Every Lisp call needs it: `coroutine-call`
    // names its callee to the driver rather than being a `call` instruction
    // with it, so the target has to be a value that can be handed over.
    def.instance("build-fn-address", vec![builder_ty(), function_ty()], value_ty());
    // `load-type`: the word of the type at place `N` of the compiling
    // process's type table, read out of the module's own table — the island's
    // `(type-ref N)`.
    def.instance("load-type", vec![builder_ty(), module_ty(), Type::Int], value_ty());
}

/// An LLVM SSA value (e.g. a constant). No methods of its own yet — produced
/// by `llvm-builder::const-word`, consumed by `llvm-builder::build-ret`.
fn value(reg: &mut Registry) {
    BuiltinType::new(reg, Path::builtin(wk::LLVM_VALUE), AdtKind::Sum, &[]);
}
