//! Bridges the checker's typed AST (`check::ast::Expr`/`Typed`) into typelisp
//! data (the `AstExpr` builtin ADT, see `check::registry::ast_expr_def`) so
//! the typelisp-written compiler (`compile::compiler_source`) can pattern-match
//! over a function's body with the language's own `match`.
//!
//! Phase 1/2/3 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) only bridges
//! the narrow `i64`/`bool`/`f64`/`char`/`Sexpr`-scalar subset needed to
//! compile a function like `(defun max2 ((a i64) (b i64)) i64 (if (< a b) b
//! a))` or `(defun gt ((a i64) (b i64)) bool (> a b))`: integer/bool/float/
//! char literals, the `Nil` `Sexpr` literal, parameter references, `if`
//! (any position, not just tail — see the `Expr::If` arm below),
//! `let`, calls to other compiled functions, `i64`'s/`f64`'s binary
//! arithmetic/comparison instance methods, `char`'s `eq`/`lt`, and
//! `cons`/`car`/`cdr`/`null`/`consp`/`atom`/`set-car`/`set-cdr` (Phase 5
//! widened every `Sexpr` value's bridged variants from `Nil`/`Cons` only to
//! all of `Int`/`Float`/`Char`/`Bool`/`Symbol`/`Str`/`Cons`/`Nil` — see
//! `crate::eval::interp::value_to_tag_payload`'s doc comment; `Path` has no
//! `Sexpr` constructor at the language level at all, so it stays untested),
//! local `setf` on `i64`/`bool`/`f64`/`char`/`Sexpr` (Phase 5b added
//! `Sexpr` — see the `Expr::Set` arm below for how its reassigned pointer
//! gets re-rooted), `Expr::Unit`, a minimal `panic` (message not
//! bridged — see the `Expr::Panic` arm), and `loop`/`break`/`return` on
//! `i64`/`bool`/`f64`/`char` results (ループ・分岐構文の整理 —
//! [docs/TODO.md](../../docs/TODO.md)「ステップ5」; `while`/`dotimes`/
//! `dolist`/`when`/`unless`/`cond`/`and`/`or`/`if-let` no longer need their
//! own bridging at all — they're `defmacro`s over `if`/`match`/`loop` now,
//! see `src/prelude.rs`'s "loop/branch primitive reduction" comment, so
//! they arrive here already expanded into the forms this module *does*
//! bridge). Anything else returns `None` — `compile` then refuses the
//! function outright rather than miscompiling it (see
//! `Interp::builtin_ast_body` in `src/eval/interp.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::{Expr, Interp, Path, QuotedSexpr, RtValue, Type, Typed};

/// Variant indices of `AstExpr`'s constructors — must match
/// `check::registry::ast_expr_def`'s `variants` order exactly (see that
/// definition's doc comment on why these indices are append-only, and on
/// the one exception: this set was renumbered once, when `awhile` —
/// superseded by `aloop` now that `while` is a `defmacro` over `loop` —
/// was dropped).
const A_INT: usize = 0;
const A_BOOL: usize = 1;
const A_VAR: usize = 2;
const A_IF: usize = 3;
const A_BINOP: usize = 4;
const A_FLOAT: usize = 5;
const A_LET: usize = 6;
const A_CALL: usize = 7;
const A_CHAR: usize = 8;
const A_NIL: usize = 9;
const A_CONS: usize = 10;
const A_CAR: usize = 11;
const A_CDR: usize = 12;
const A_NULLP: usize = 13;
const A_SET: usize = 14;
const A_UNIT: usize = 15;
const A_PANIC: usize = 16;
const A_LOOP: usize = 17;
const A_BREAK: usize = 18;
const A_RETURN: usize = 19;
const A_CONSP: usize = 20;
const A_SETCAR: usize = 21;
const A_SETCDR: usize = 22;

/// `i64`'s and `f64`'s binary arithmetic/comparison instance methods
/// (`registry::int_assoc`/`float_assoc`) — the only `Expr::Assoc` shapes
/// Phase 1/2c bridge. Both types register the same operator strings, so one
/// list covers either receiver type.
const BINOPS: [&str; 9] = ["+", "-", "*", "<", "<=", ">", ">=", "=", "/="];

/// The scalar types bridged at all (Phase 1/2b/2c/2d/3) — shared by every
/// guard that needs to tell "a value this narrow scope can carry" from
/// "something `compile` must refuse" (`Var`'s/`Call`'s wrapping `Typed.ty`).
fn is_bridgeable_scalar_type(t: &Type) -> bool {
    matches!(t, Type::I64 | Type::Bool | Type::F64 | Type::Char) || is_sexpr_value_type(t)
}

/// Whether `t` is the `Sexpr` type itself — `compile`'s `Sexpr` support
/// (Phase 3) only ever sees this one type (no type parameters: `Sexpr` is
/// not generic), unlike e.g. `Vector<T>`.
fn is_sexpr_value_type(t: &Type) -> bool {
    matches!(t, Type::Named(p, params) if params.is_empty() && *p == Path::root("sexpr"))
}

fn data(variant: usize, fields: Vec<RtValue>) -> RtValue {
    RtValue::Data { type_name: Path::root("astexpr"), variant, fields }
}

fn rt_vector(items: Vec<RtValue>) -> RtValue {
    RtValue::Vector(Rc::new(RefCell::new(items)))
}

/// `Option<AstExpr>` (for `areturn`'s value field) — mirrors
/// `crate::eval::interp::option_value`, duplicated here since that one
/// isn't `pub`.
fn option_value(v: Option<RtValue>) -> RtValue {
    match v {
        Some(x) => RtValue::Data { type_name: Path::root("option"), variant: 0, fields: vec![x] },
        None => RtValue::Data { type_name: Path::root("option"), variant: 1, fields: vec![] },
    }
}

/// The `i64`/`bool`/`f64`/`char` subset `aloop`/`areturn` restrict a loop's
/// own result type to (still excluding `Sexpr`, even after Phase 5b added
/// `Sexpr` `setf` support for *named* bindings — see `Expr::Set`'s arm
/// below): a loop's `exit`-block result phi merges an *anonymous* value with
/// no binding name/position of its own to look up in `roots`, so the
/// per-variable `roots`-table mechanism `aset` relies on doesn't apply to it
/// at all; rooting it would need a different mechanism, left as a
/// follow-up). Returns `None` for anything else (including `Unit`/`Never`,
/// which never actually need a real phi — see `compile-value`'s `ALoop` arm).
fn loop_result_kind(t: &Type) -> Option<(bool, bool, bool)> {
    match t {
        Type::I64 => Some((false, false, false)),
        Type::Bool => Some((true, false, false)),
        Type::F64 => Some((false, true, false)),
        Type::Char => Some((false, false, true)),
        Type::Unit | Type::Never => Some((false, false, false)),
        _ => None,
    }
}

/// Converts one checked expression into an `AstExpr` value, or `None` if it
/// uses a construct outside Phase 1/2's scope. Takes `interp` (Phase 2f)
/// solely to check `Expr::Call`'s callee against `Interp.compiled` — every
/// other case is a pure tree conversion with no interpreter state involved.
/// `group` (Phase 2g, self/mutual recursion) is the set of function names
/// currently being compiled *together* in the same `LlvmModule` as `t`'s own
/// function — `compile`'s singleton `[name]` for an ordinary single-function
/// compile (making a self-recursive call bridgeable), or `compile-group`'s
/// full list for several functions compiled together so they can call each
/// other before any of them is registered in `Interp.compiled`. A callee in
/// `group` hasn't finished compiling yet (no JIT'd address exists), but its
/// `LlvmFunction` declaration already exists in the shared module by the time
/// any group member's body is built (`compile-group` declares all of them
/// up front) — see `crate::compile::compiler_source`'s doc comment.
pub(crate) fn typed_to_ast(t: &Typed, interp: &Interp, group: &[Path]) -> Option<RtValue> {
    match &t.expr {
        Expr::Int(n) => Some(data(A_INT, vec![RtValue::Int(*n)])),
        Expr::Bool(b) => Some(data(A_BOOL, vec![RtValue::Bool(*b)])),
        Expr::Float(n) => Some(data(A_FLOAT, vec![RtValue::Float(*n)])),
        Expr::Char(c) => Some(data(A_CHAR, vec![RtValue::Char(*c)])),
        // `Sexpr`'s `Nil` literal (Phase 3) — `()`/`(quote ())`. Every other
        // `QuotedSexpr` shape (`Int`/`Float`/`Char`/`Bool`/`Sym`/`Str`/`Cons`)
        // stays unbridged: a non-empty quoted literal would need `cons`-ing
        // a whole structure at compile time, which `compile-value` has no
        // primitive for (only `cons`/`car`/`cdr` at *runtime*, see `A_CONS`
        // below).
        Expr::Quote(QuotedSexpr::Nil) => Some(data(A_NIL, vec![])),
        // A `Var`'s own `Expr` doesn't carry its type — Phase 1/2 only ever
        // bind `i64`/`bool`/`f64`/`char` parameters *or* `let`-bound locals
        // (Phase 2e) of those same types, so checking the wrapping `Typed.ty`
        // here is how a stray unsupported-type variable (impossible today,
        // not by construction) would be caught rather than silently
        // mistyped downstream.
        Expr::Var(name) if is_bridgeable_scalar_type(&t.ty) => {
            Some(data(A_VAR, vec![RtValue::Str(name.clone())]))
        }
        // `if` (ループ・分岐構文の整理): now bridged in *any* position, not
        // just tail — see `compile-value`'s `AIf` arm (continuation block +
        // phi, the same mechanism `while`/`loop` use for their own header/
        // exit blocks) and `compile-tail`'s now-generic `_` arm (no
        // dedicated `AIf` handling there anymore; it just calls
        // `compile-value` and `build-ret*`s the result, exactly like any
        // other value-producing `AstExpr`). `then_diverges`/`els_diverges`
        // (`Typed.ty == Never`) are computed here, not at the typelisp
        // level — `AstExpr` carries no per-node type info, and this is
        // exactly the same check `Checker::check_if`'s own `non_never` does
        // when unifying the branches, just read off the already-checked
        // `Typed` tree instead of recomputed.
        Expr::If(c, then, els) => {
            let then_diverges = then.ty == Type::Never;
            let els_diverges = els.ty == Type::Never;
            let c = typed_to_ast(c, interp, group)?;
            let then_ast = typed_to_ast(then, interp, group)?;
            let els_ast = typed_to_ast(els, interp, group)?;
            Some(data(A_IF, vec![c, then_ast, RtValue::Bool(then_diverges), els_ast, RtValue::Bool(els_diverges)]))
        }
        Expr::Assoc { type_name, method, instance: true, args }
            if (*type_name == Path::root("i64") || *type_name == Path::root("f64"))
                && args.len() == 2
                && BINOPS.contains(&method.as_str()) =>
        {
            let lhs = typed_to_ast(&args[0], interp, group)?;
            let rhs = typed_to_ast(&args[1], interp, group)?;
            Some(data(A_BINOP, vec![RtValue::Str(method.clone()), lhs, rhs]))
        }
        // `char`'s `eq`/`lt` (`registry::char_assoc`, Phase 2d) — `char` is
        // stored as a plain `i32`-wide `IntValue` throughout IR construction,
        // so the same `icmp`-lowering `build-op` already does for `i64`'s
        // `=`/`<` works unchanged; only the *method name* differs from the
        // operator string `build-op` expects, so it's translated here rather
        // than teaching `build-op` a second name for the same operation.
        Expr::Assoc { type_name, method, instance: true, args }
            if *type_name == Path::root("char") && args.len() == 2 && (method == "eq" || method == "lt") =>
        {
            let op = if method == "eq" { "=" } else { "<" };
            let lhs = typed_to_ast(&args[0], interp, group)?;
            let rhs = typed_to_ast(&args[1], interp, group)?;
            Some(data(A_BINOP, vec![RtValue::Str(op.to_string()), lhs, rhs]))
        }
        // `let` (CL parallel-binding semantics — every `val` is checked
        // against the *outer* scope, never an earlier sibling binding;
        // `let*` is already desugared to nested single-binding `let`s by
        // the checker, so it needs no separate case here). Bridging fails
        // (`None`) if *any* binding's value or body form does — e.g. a
        // `string`-typed binding — refusing the whole function rather than
        // silently dropping part of it.
        Expr::Let(bindings, body) => {
            let mut names = Vec::with_capacity(bindings.len());
            let mut values = Vec::with_capacity(bindings.len());
            for (name, val) in bindings {
                names.push(RtValue::Str(name.clone()));
                values.push(typed_to_ast(val, interp, group)?);
            }
            let body = body.iter().map(|f| typed_to_ast(f, interp, group)).collect::<Option<Vec<_>>>()?;
            Some(data(A_LET, vec![rt_vector(names), rt_vector(values), rt_vector(body)]))
        }
        // `setf` on a local (`Expr::Set` — `Expr::SetGlobal`, globals, isn't
        // bridged yet). `Sexpr` is allowed now too (Phase 5b,
        // [docs/TODO.md](../../docs/TODO.md)「ステップ5」) — the *same*
        // restriction `is_bridgeable_scalar_type` already applies everywhere
        // else, no longer narrower than it for this one case. A reassigned
        // `Sexpr` pointer gets its *own* `Heap`-root slot kept live for the
        // rest of its binding's scope via `compile-value`/`compile-tail`'s
        // `roots`/`set-root` bookkeeping (`compiler_source.rs`'s doc
        // comment) — Phase 3's rooting discipline only ever rooted a
        // binding's *initial* value once, which is what made this
        // unbridgeable before.
        Expr::Set(name, value) if is_bridgeable_scalar_type(&value.ty) => {
            let v = typed_to_ast(value, interp, group)?;
            Some(data(A_SET, vec![RtValue::Str(name.clone()), v]))
        }
        // `()` used as a value (ループ・分岐構文の整理) — `when`/`unless`/
        // `cond`'s un-taken branch, now that those are `defmacro`s over
        // `if` (see `src/prelude.rs`). The compiled placeholder is never
        // actually read by anything (`Unit` has no representable `TlValue`
        // payload), the same as `ALoop`'s own placeholder when a loop never
        // really produces a value — see `compile-value`'s `AUnit` arm.
        Expr::Unit => Some(data(A_UNIT, vec![])),
        // `panic` (ループ・分岐構文の整理) — the message is deliberately
        // *not* bridged or evaluated (see `compile-value`'s `APanic` arm
        // and `crate::eval::interp::tl_panic`'s doc comment): this exists
        // purely to make a `then_diverges`/`els_diverges` branch (the
        // `Expr::If` arm above) reachable and testable, since `break`/
        // `return` (the other `Never`-typed constructs) only make sense
        // inside a `loop`.
        Expr::Panic(_msg) => Some(data(A_PANIC, vec![])),
        // `loop` (ループ・分岐構文の整理): bridged only when its own result
        // type is `i64`/`bool`/`f64`/`char`/`Unit`/`Never` — see
        // `loop_result_kind`'s doc comment on why `Sexpr` is excluded, the
        // same reasoning as `aset` above. `result_is_bool`/`result_is_f64`/
        // `result_is_char` pick which `LlvmValue` kind seeds the loop's
        // `exit`-block result phi (`compile-value`'s `ALoop` arm) — `Unit`/
        // `Never` fall through to the `i64` defaults since that phi (if
        // ever created at all) is then a placeholder nothing reads, exactly
        // like `Expr::Unit`'s own bridging above.
        Expr::Loop(body) => {
            let (result_is_bool, result_is_f64, result_is_char) = loop_result_kind(&t.ty)?;
            let body = body.iter().map(|f| typed_to_ast(f, interp, group)).collect::<Option<Vec<_>>>()?;
            Some(data(
                A_LOOP,
                vec![
                    rt_vector(body),
                    RtValue::Bool(result_is_bool),
                    RtValue::Bool(result_is_f64),
                    RtValue::Bool(result_is_char),
                ],
            ))
        }
        // `break` (ループ・分岐構文の整理): always contributes `Unit` to its
        // enclosing loop's type (`Checker::check_break`), never a value —
        // no fields to bridge.
        Expr::Break => Some(data(A_BREAK, vec![])),
        // `return` (ループ・分岐構文の整理): `(return)` bridges to `None`
        // (no value, i.e. `Unit` — matching `Checker::check_return`'s own
        // default), `(return value)` bridges `value` and wraps it in
        // `Some`. The enclosing `loop`'s own bridging (above) already
        // restricted the *combined* type of every `break`/`return` reached
        // inside it to the non-`Sexpr` subset (`Checker::contribute_loop_exit`
        // unifies them all via `join_types`), so no separate type check is
        // needed here — if `value`'s type were `Sexpr`, the enclosing
        // `aloop` would already have failed to bridge.
        Expr::Return(value) => {
            let v = match value {
                Some(v) => Some(typed_to_ast(v, interp, group)?),
                None => None,
            };
            Some(data(A_RETURN, vec![option_value(v)]))
        }
        // `not` (ループ・分岐構文の整理): like `cons`/`car`/`cdr`/`null`
        // below, always `Interp::eval_builtin`, never an entry in
        // `Interp.compiled` — needs its own name-based bridging rather than
        // the general `ACall` case's `interp.is_compiled` guard. Translated
        // to `(= x false)` (an existing `ABinOp`) rather than introducing a
        // new unary-negation primitive: `bool` is a plain `i1`-wide
        // `IntValue` throughout IR construction, so the `icmp eq` `build-op`
        // already emits for `=` is exactly `not`'s semantics against a
        // `false` constant — the same "translate to an existing operator
        // string" move `char`'s `eq`/`lt` arm makes below. This is what
        // makes `while`'s macro expansion (`` `(loop (if (not ,test)
        // (break) ()) ,@body) `` — see `src/prelude.rs`) bridgeable at all:
        // `not` wasn't reachable from `compile`-eligible code before
        // `while` became a `defmacro` over `loop`.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "not" && call_args.len() == 1 => {
            let x = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_BINOP, vec![RtValue::Str("=".to_string()), x, data(A_BOOL, vec![RtValue::Bool(false)])]))
        }
        // `cons`/`car`/`cdr` (Phase 3): always `Interp::eval_builtin`
        // (Rust-implemented) free functions, never entries in
        // `Interp.compiled`, so they need their own name-based bridging
        // ahead of the general `ACall` case below rather than its
        // `interp.is_compiled` guard.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "cons" && call_args.len() == 2 => {
            let lhs = typed_to_ast(&call_args[0], interp, group)?;
            let rhs = typed_to_ast(&call_args[1], interp, group)?;
            Some(data(A_CONS, vec![lhs, rhs]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "car" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_CAR, vec![v]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "cdr" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_CDR, vec![v]))
        }
        // `null`/`consp` (Phase 3/5): ordinary typelisp `defun`s in
        // `prelude.rs` (`(match s ((Nil) true) (_ false))` /
        // `(match s ((Cons _ _) true) (_ false))`), each bridged directly
        // onto its own tag-check primitive (`A_NULLP`/`A_CONSP`,
        // `compile-value`'s `build-nullp`/`build-consp`) instead of requiring
        // this bridge to understand `match` at all — and not merely for
        // convenience: since neither `null` nor `consp` could be `compile`d
        // on their own (no `AMatch` `AstExpr` variant exists), the general
        // "call an already-compiled function" `Expr::Call` arm below could
        // never bridge a call to either of them regardless.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "null" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_NULLP, vec![v]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "consp" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_CONSP, vec![v]))
        }
        // `atom` (Phase 5): `prelude.rs`'s `(defun atom ((s Sexpr)) bool (not
        // (consp s)))` — translated straight to `(= (consp s) false)` (the
        // same existing `ABinOp` `not`'s own arm above builds), since `atom`
        // is no more compilable on its own than `consp` is (it calls
        // `consp`, which the general `ACall` arm could only bridge if
        // `consp` were itself already compiled — and `consp` never can be).
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "atom" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            let consp = data(A_CONSP, vec![v]);
            Some(data(A_BINOP, vec![RtValue::Str("=".to_string()), consp, data(A_BOOL, vec![RtValue::Bool(false)])]))
        }
        // `set-car`/`set-cdr` (Phase 5): like `cons`/`car`/`cdr`, always
        // `Interp::eval_builtin` free functions, never entries in
        // `Interp.compiled`.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "set-car" && call_args.len() == 2 => {
            let cell = typed_to_ast(&call_args[0], interp, group)?;
            let val = typed_to_ast(&call_args[1], interp, group)?;
            Some(data(A_SETCAR, vec![cell, val]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "set-cdr" && call_args.len() == 2 => {
            let cell = typed_to_ast(&call_args[0], interp, group)?;
            let val = typed_to_ast(&call_args[1], interp, group)?;
            Some(data(A_SETCDR, vec![cell, val]))
        }
        // A call to another `defun` (Phase 2f, direct calls between compiled
        // functions; Phase 2g, self/mutual recursion): only bridged if `path`
        // is a simple top-level name (the only shape `compile`'s other
        // introspection builtins — `ast-params`/`param-is-bool`/`ret-is-bool`/
        // etc — support) *and* it's either already compiled (`Interp.compiled`,
        // Phase 2f — its `LlvmFunction` lives in some *other*, already-JIT'd
        // module, resolved later by `add_global_mapping`) *or* a member of
        // `group` (Phase 2g — its `LlvmFunction` is a bare declaration already
        // present in *this same* module by construction, resolved by an
        // ordinary intra-module `call` with no JIT/global-mapping step
        // needed at all, regardless of whether that group member's own body
        // has been built yet — see `crate::compile::compiler_source`'s doc
        // comment on `compile-group`). Calling an uncompiled, non-group
        // function is refused by failing the bridge here (`None`), the same
        // way an unsupported-type binding refuses the whole function,
        // **not** by emitting an `ACall` that `compile-value` would have to
        // reject at IR-build time. The reverse direction (compiled code
        // calling back into uncompiled/tree-walked code) needs a trampoline
        // and stays unsupported — see [docs/TODO.md](../../docs/TODO.md)
        // 「ステップ5・Phase 2f」.
        Expr::Call(path, call_args)
            if path.is_simple()
                && (interp.is_compiled(path) || group.contains(path))
                && is_bridgeable_scalar_type(&t.ty) =>
        {
            let args = call_args.iter().map(|a| typed_to_ast(a, interp, group)).collect::<Option<Vec<_>>>()?;
            Some(data(A_CALL, vec![RtValue::Str(path.local().to_string()), rt_vector(args)]))
        }
        _ => None,
    }
}
