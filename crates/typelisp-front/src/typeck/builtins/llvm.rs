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
    // `f64` arithmetic/comparison (`eval_llvm_builtin_method`'s
    // `llvm_builder_build_float_op`/`_fcmp`): both operands and the arithmetic
    // result are the *same* `i64`-carried representation every other
    // `llvm-value` uses (a compiled `f64`'s raw `f64::to_bits` pattern) — the
    // `i64`<->`double` `bitcast`ing is entirely internal to each op, so these
    // are ordinary `i64`-in/`i64`-out builtins like `build-add`, and
    // `compile-assoc`'s f64 branch composes them exactly like the int one
    // composes `build-add`/`build-icmp-*`. A comparison yields a `0`/`1`
    // `i64` (the `bool` representation), matching `build-icmp-*`.
    for name in ["build-fadd", "build-fsub", "build-fmul", "build-fdiv", "build-frem"] {
        def.instance(name, vec![builder_ty(), value_ty(), value_ty()], value_ty());
    }
    for name in ["build-fcmp-lt", "build-fcmp-le", "build-fcmp-gt", "build-fcmp-ge", "build-fcmp-eq", "build-fcmp-ne"] {
        def.instance(name, vec![builder_ty(), value_ty(), value_ty()], value_ty());
    }
    // `f64` transcendentals (`float-native-method?`'s `sqrt`/`floor`/`ceiling`/
    // `round`/`truncate`): each lowers to the matching LLVM intrinsic
    // (`llvm.sqrt.f64`/...) declared on demand in `module` — unlike
    // `build-fadd`/..., these need the module to look the intrinsic
    // declaration up in (`eval_llvm_builtin_method`'s
    // `llvm_builder_build_float_unary_intrinsic`), so they take `llvm-module`
    // as a second argument the same way `build-make-closure` does.
    for name in ["build-fsqrt", "build-ffloor", "build-fceil", "build-fround", "build-ftrunc", "build-fsin", "build-fcos", "build-fexp", "build-flog"] {
        def.instance(name, vec![builder_ty(), module_ty(), value_ty()], value_ty());
    }
    // `expt` (`f64,f64->f64`): the binary counterpart of the unary
    // transcendentals above, `llvm.pow.f64`. `max`/`min` are the same shape
    // (`llvm.maxnum.f64`/`llvm.minnum.f64`).
    for name in ["build-fpow", "build-fmaxnum", "build-fminnum"] {
        def.instance(name, vec![builder_ty(), module_ty(), value_ty(), value_ty()], value_ty());
    }
    // `float->int` (`f64->i32`, narrowing, truncating toward zero): a single
    // `fptosi` instruction, no heap allocation and no module lookup needed —
    // unlike `float->bignum`/`float->ratio`, which stay non-native (see
    // `float-native-method?`'s doc comment).
    def.instance("build-fptosi", vec![builder_ty(), module_ty(), value_ty()], value_ty());
    // `int->float` (widening, always exact for the widths this language has):
    // `sitofp` to `double`, then `bitcast` back to the raw word a compiled
    // `f64` is carried in. No module parameter — unlike `fptosi`'s saturating
    // form, `sitofp` is a plain instruction with no intrinsic to overload and
    // no input it is undefined on.
    def.instance("build-sitofp", vec![builder_ty(), value_ty()], value_ty());
    // `float->f32` (and every `f32` arithmetic result): round the operand to
    // binary32 and widen it straight back. Two instructions, no intrinsic and
    // no module lookup — `fptrunc` then `fpext`, which is exactly "the
    // nearest binary32 value" written in IR.
    def.instance("build-fround32", vec![builder_ty(), value_ty()], value_ty());
    // `int`'s overflow-checked `+`/`-`/`*` on two fixnum words: the result,
    // with the overflow flag stored into an `alloca-args` slot — see
    // `llvm_builder_build_int_overflow`.
    for name in ["build-sadd-overflow", "build-ssub-overflow", "build-smul-overflow"] {
        def.instance(name, vec![builder_ty(), module_ty(), value_ty(), value_ty(), value_ty()], value_ty());
    }
    // `alloca-args`/`store-arg`/`build-call`: building a direct call to an
    // already-declared function (`get-function`'s result). `alloca-args`
    // stack-allocates a fresh `[count x i64]` array (mirroring the fixed-ABI
    // argument array every compiled function already expects) and returns
    // its decayed element pointer; `store-arg` fills in one slot at a time
    // (the same loop shape `compiler.rs`'s `bind-params` already uses for
    // reading arguments, just writing instead); `build-call` then calls the
    // target with that pointer plus a literal arg count, exactly matching
    // `CompiledSignature`'s `i64 fn(i64* args, i32 argc)` shape — so calling
    // a compiled function looks the same whether the call originates from
    // Rust (`compile::CompiledFn::call`) or from another compiled function.
    def.instance("alloca-args", vec![builder_ty(), Type::Int], value_ty());
    def.instance("store-arg", vec![builder_ty(), value_ty(), Type::Int, value_ty()], Type::Unit);
    // The one-word slot a function uses for a role — see
    // `llvm_builtins::llvm_builder_scratch_slot`.
    def.instance("scratch-slot", vec![builder_ty(), Type::Int], value_ty());
    def.instance("build-call", vec![builder_ty(), function_ty(), value_ty(), Type::Int], value_ty());
    // `load-env`/`build-call-with-env`: the captures counterpart of
    // `load-arg`/`build-call`, for functions declared via
    // `add-function-with-env`. `load-env` reads logical captured slot
    // `index` out of the function's env array (`load-arg`'s GEP pattern,
    // against the env parameter instead of the args one); `build-call-with-env`
    // calls a target under the extended ABI, passing both the args array
    // (as `build-call` already does) and an env array built the same way
    // (`alloca-args`/`store-arg`, just filled with captured values instead
    // of call arguments).
    def.instance("load-env", vec![builder_ty(), function_ty(), Type::Int], value_ty());
    def.instance("build-call-with-env", vec![builder_ty(), function_ty(), value_ty(), Type::Int, value_ty(), Type::Int], value_ty());
    // `build-make-closure`: `BoxedObj::CompiledClosure`
    // (`typelisp-mem`) — the GC-heap runtime representation of a `lambda`
    // value that *escapes* its defining function (rather than being called
    // directly while statically known, like every `apply`/`call` site
    // above) — labels/closures Stage 4, flipped off the raw `malloc`'d,
    // reference-counted `ClosureBox` it used to be at the closure-
    // representation unification (see that variant's own doc comment). The
    // resulting value is a tagged `Sexpr` reference, exactly like every
    // other `BoxedObj` constructor's result (`rt_struct_new`, `rt_data_new`,
    // ...) — no longer a bare untagged `i64` the way the old `ClosureBox`
    // pointer was.
    //
    // `build-make-closure` takes the already-compiled `target` (declared
    // under `add-function-with-env`'s ABI — *every* closure-boxed function
    // uses that ABI, capturing or not, so applying one never has to decide
    // which ABI to call through), an env array built the same way a
    // direct capturing call already builds one (`alloca-args`/`store-arg`/
    // `compile-env-args`), and `sexpr_mask` (a bitmask, one bit per captured
    // slot, computed entirely at compile time by `compiler.rs`'s
    // `compute-sexpr-mask`) marking which captured slots are tagged `Sexpr`
    // values for the GC mark phase to trace — see
    // `interp::llvm_builder_build_make_closure`'s doc comment for how it
    // gets copied into the new box via `rt_closure_new`.
    // Applying such a box is `coroutine-apply`'s job since C5: the callee
    // isn't a statically-known `llvm-function`, just a tagged closure
    // reference, and reading `fn_ptr`/`env_len`/each captured slot back out
    // is the *driver*'s (`typelisp_rt::resolve_closure`) — because only it
    // can act on the answer. The builtin that used to do it here
    // (`build-closure-apply`, a call to `rt_apply_any`) is gone; what
    // follows describes the box it read.
    // The reads are via `rt_closure_fnptr`/`rt_closure_env_len`/
    // `rt_closure_env_get` at runtime and calls through
    // `build_indirect_call` instead — see that builtin's own doc comment.
    // Neither builtin needs a retain/release counterpart anymore: the box's
    // lifetime is the GC's business now, including a `labels` cycle between
    // two mutually-capturing siblings, which mark-sweep reclaims like any
    // other unreachable structure.
    def.instance("build-make-closure", vec![builder_ty(), module_ty(), function_ty(), value_ty(), Type::Int, Type::Int, Type::Int], value_ty());
    // The generic-pointer read counterpart of `store-arg` — see
    // `interp::llvm_builder_load_raw`'s doc comment.
    def.instance("load-raw", vec![builder_ty(), value_ty(), Type::Int], value_ty());
    // `build-slot-ptr`: `load-raw`'s GEP without the load — the *address* of
    // one word in an `i64` array. What makes a compiled frame usable as the
    // home of a local (Phase C): `env` maps a name to a pointer, and
    // `resolve-value`/`compile-set`/`retain-bindings` read and write through
    // it with `load-raw`/`store-arg`. Handing them a slot carved out of a
    // frame rather than a fresh `alloca-args` is then the whole of the
    // change — everything downstream already speaks pointers.
    def.instance("build-slot-ptr", vec![builder_ty(), value_ty(), Type::Int], value_ty());
    // The frame under construction (Phase C1). A function's locals live in a
    // compiled frame, and two things about it cannot be held in the island: a
    // slot counter, and the frame's size — which is only known once the body
    // has been emitted, since the body is what allocates the slots.
    //
    // Both are keyed on the *builder*, which is one-to-one with the function
    // being built (each of `compile-function`/`compile-lambda`/
    // `compile-labels-bodies` makes its own). That is why none of the island's
    // signatures grows an argument to carry the frame around — `compile-value`
    // already has 16, and this phase exists partly to shorten that list.
    //
    // `frame-slot` and `frame-slot-rooted` carve out the next slot of the frame
    // `coroutine-begin` opened, the second marking it for the collector;
    // `coroutine-end` writes the final count into the prologue's placeholder.
    // The count is therefore by construction the number of slots handed out —
    // there is no separate counting pass to drift from the emitting one.
    def.instance("frame-slot", vec![builder_ty()], value_ty());
    def.instance("frame-slot-rooted", vec![builder_ty(), module_ty()], value_ty());
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
    def.instance("coroutine-call", vec![builder_ty(), module_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    def.instance("coroutine-call-env", vec![
                builder_ty(),
                module_ty(),
                value_ty(),
                value_ty(),
                Type::Int,
                value_ty(),
                Type::Int,
            ], value_ty());
    // `coroutine-apply`: `coroutine-call` with a function *value* in place of
    // an address. Same instruction sequence here; the driver is where the
    // difference is, because only it can look at the value and decide who can
    // run it — a chain frame for a compiled body, the continuation stack for
    // an interpreted one.
    def.instance("coroutine-apply", vec![builder_ty(), module_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    // `coroutine-dyn-call`: `coroutine-apply` for a callee named by a vtable
    // slot. The slot can hold a compiled body under either ABI, or nothing at
    // all when the concrete type's method is interpreted — which is why the
    // dispatch is the driver's and not the call site's.
    def.instance("coroutine-dyn-call", vec![builder_ty(), module_ty(), value_ty(), value_ty(), value_ty(), Type::Int], value_ty());
    // `coroutine-suspend` is `coroutine-call` with the callee left out: set the
    // resume point, hand control back with `STATUS_SUSPEND`, and answer with
    // whatever the task was waiting for once it is resumed. The suspending
    // builtin's own shim (`rt_suspend_*`) has already been called by then and
    // has recorded *what* is being waited for, so nothing about that needs to
    // reach this builtin.
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
    // Stage 6 of the Sexpr-representation plan (`docs/implementation-log.md`): generic
    // malloc/free + pointer conversion — the `ClosureBox` generalization a
    // general ADT box (`Option`/`Result`/`defstruct`) needs, without baking
    // in `ClosureBox`'s own fixed header layout. `build-malloc`/`build-free`
    // mirror `alloca-args`'s stack-allocation shape but on the heap;
    // `build-int-to-ptr`/`build-ptr-to-int` let `compiler.rs` cross between
    // "every compiled value is a plain i64" (`build-ret`/`store-arg`'s value
    // operand/`build-icmp-*`/...) and the pointer `load-raw`/`store-arg`'s
    // *array* operand already expects, exactly the conversion
    // `build-make-closure` already does internally for `ClosureBox`, just
    // exposed generically here.
    def.instance("build-malloc", vec![builder_ty(), Type::Int], value_ty());
    def.instance("build-free", vec![builder_ty(), value_ty()], Type::Unit);
    def.instance("build-int-to-ptr", vec![builder_ty(), value_ty()], value_ty());
    def.instance("build-ptr-to-int", vec![builder_ty(), value_ty()], value_ty());
    // `build-fn-address`: a declared function's address as a plain `i64` —
    // `build-ptr-to-int` applied to an `llvm-function` instead of to a
    // `build-malloc`'d block. Every Lisp call needs it: `coroutine-call`
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
