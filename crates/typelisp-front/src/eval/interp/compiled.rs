//! Calls across the boundary between interpreted and compiled code, in both
//! directions: encoding arguments and decoding results, entering a compiled
//! body, and the hooks compiled code calls back into the interpreter through.

use typelisp_mem::Word;
use std::rc::Rc;
use typelisp_mem::RootScope;
use crate::{BoxId, Heap, Value};
use crate::check::repr::Repr;
use crate::eval::value::EvalError;
use super::{backend, ACTIVE_INTERP, CompiledBody, FnBody, Interp};

/// Runs a coroutine-ABI body to completion on a driver of its own.
///
/// `Interp::decode_compiled_return`, which this is, for a caller with no
/// interpreter at hand — an interpreted `thread`'s task, reading its callee's
/// answer on the thread it ran on. Nothing here needs one.
pub(crate) fn decode_crossing_return(heap: &mut Heap, raw: i64, ret: &Repr) -> Result<Value, EvalError> {
    Ok(match ret {
        // A raw registry index, never a heap pointer. For an LLVM object
        // that is already the interpreter's own representation, so it just
        // *is* the result.
        Repr::Handle => match backend("decoding an llvm-handle return")? {
            b if (b.handle_is_live)(raw) => Value::Int(raw),
            _ => {
                return Err(EvalError::Internal(format!(
                    "compiled call returned dangling llvm handle {}",
                    raw
                )))
            }
        },
        // The word's low bits, at the type's width: what fills the rest of
        // the word is no part of the value.
        Repr::Narrow(t) => Value::Int(typelisp_mem::with_fixed!(*t, T => {
            use typelisp_mem::fixed::Fixed;
            T::from_word(raw).to_word()
        })),
        // Also raw, and — unlike `Handle` above — with no registry to
        // check it against. Whatever C answered is the answer; the FFI
        // declaration is what claimed it would be a pointer.
        Repr::RawWord => Value::Int(raw),
        // A `Unit`-typed body compiles to a plain `0` (`compile-unit`) —
        // decode it back to the real unit value rather than surfacing the
        // raw word as a bogus `Int(0)`, so a `Unit`-returning compiled
        // function interoperates with interpreted code exactly like an
        // interpreted one.
        Repr::Unit => Value::Empty,
        // Compiled code represents a `bool` as a raw 0/1 `i64` (LLVM
        // `icmp` results, zero-extended).
        Repr::Bool => Value::Bool(raw != 0),
        // A compiled `char` is a raw `i64` Unicode scalar value — the
        // exact inverse of the `*c as i64` a `char` argument crosses as. A
        // compiled `char` only ever holds a value that was a valid `char`
        // on the way in, so a decode failure is an internal-invariant
        // break, not a user-reachable error.
        Repr::Char => match char::from_u32(raw as u32) {
            Some(c) => Value::Char(c),
            None => {
                return Err(EvalError::Internal(format!(
                    "compiled call returned {} for a `char` result, which is not a valid Unicode scalar value",
                    raw
                )))
            }
        },
        // Raw `f64::to_bits` in the return register
        // (`llvm_builder_build_float_op`'s final `bitcast`).
        // The width the *declared return type* named decides the box, which
        // is the whole reason the two are separate `Repr`s: the word coming
        // back says nothing of it.
        Repr::F64 => heap.alloc_f64(f64::from_bits(raw as u64)),
        Repr::F32 => heap.alloc_f32(f32::from_bits(raw as u32)),
        // A tagged word whose decode must land on the shape the
        // declaration promised — a mismatch here means the compiled side
        // and this side disagree about the ABI, which is an internal
        // error rather than anything a program can cause.
        Repr::Str => match typelisp_rt::decode(raw as Word) {
            v @ Value::Str(_) => v,
            other => return Err(crossing_mismatch("a string", other)),
        },
        Repr::Sym => match typelisp_rt::decode(raw as Word) {
            v @ Value::Symbol(_) => v,
            other => return Err(crossing_mismatch("a Symbol", other)),
        },
        Repr::Ratio => match typelisp_rt::decode(raw as Word) {
            v @ Value::Boxed(id) if heap.is_ratio(id) => v,
            other => return Err(crossing_mismatch("a ratio", other)),
        },
        // A `rational` is the word of the `int` or `ratio` it is.
        Repr::Rational => match typelisp_rt::decode(raw as Word) {
            v @ Value::Int(_) => v,
            v @ Value::Boxed(id) if heap.is_bignum(id) || heap.is_ratio(id) => v,
            other => return Err(crossing_mismatch("a rational (int or ratio)", other)),
        },
        // An `int` is its tagged word: a fixnum, or a bignum box for the
        // values a fixnum cannot hold — and never a box for one it can
        // (`Heap::canonical_int`'s invariant, checked here so that a
        // compiled body that boxed a small value is caught at the
        // boundary rather than the next `eq`).
        Repr::Int => match typelisp_rt::decode(raw as Word) {
            v @ Value::Int(_) => v,
            v @ Value::Boxed(id) if heap.is_bignum(id) => {
                if heap.bignum_fits_fixnum(id) {
                    return Err(EvalError::Internal(format!(
                        "compiled call returned an `int` boxed as a bignum that fits a fixnum ({}) — not canonical",
                        heap.bignum_value(id)
                    )));
                }
                v
            }
            other => return Err(crossing_mismatch("an int (fixnum or bignum)", other)),
        },
        Repr::RandomState => match typelisp_rt::decode(raw as Word) {
            v @ Value::Boxed(id) if heap.is_random_state(id) => v,
            other => return Err(crossing_mismatch("a random-state", other)),
        },
        // Every remaining representation is a heap value crossing as the
        // tagged `i64` the `rt_*` shims read. A niched `Option<T>` is one
        // too: the empty-list immediate for `None`, and `T`'s *field*
        // encoding for `Some` — a fixnum for a narrow payload, a float
        // box for a float one — which is exactly what `decode` reads back
        // as the `Value` the interpreter represents that `Option` by.
        Repr::Sexpr
        | Repr::Niche(_)
        | Repr::Struct
        | Repr::Enum
        | Repr::Dyn
        | Repr::Fn
        | Repr::Scope(_)
        | Repr::Vector(_)
        | Repr::HashTable(..) => typelisp_rt::decode(raw as Word),
        // A still-generic type variable. Nothing compiled can return one:
        // a generic body is monomorphized before it is compiled, so
        // reaching here is a bug in whatever produced the signature.
        Repr::None => {
            return Err(EvalError::Internal(
                "compiled call has no declared return representation — a generic body was compiled unmonomorphized"
                    .to_string(),
            ))
        }
    })
}

/// How many bits a value of `repr` takes where it crosses: a float's or a
/// fixed-width integer's own width, a word otherwise. A value wider than a
/// word is the words it takes, in order (`typelisp_mem::value_words`).
pub(crate) fn crossing_bits(repr: &Repr) -> u32 {
    use typelisp_mem::fixed::FixedType;
    match repr {
        Repr::F64 => 64,
        Repr::Narrow(FixedType::I32 | FixedType::U32) | Repr::Char | Repr::F32 => 32,
        Repr::RawWord => Word::BITS.max(std::ffi::c_long::BITS),
        _ => Word::BITS,
    }
}

/// The values in `words`, the arguments a call with these parameter
/// representations left: each raw, as [`decode_crossing_return`] takes it.
pub(crate) fn crossing_values(words: &[Word], reprs: &[Repr]) -> Result<Vec<i64>, EvalError> {
    let mut rest = words;
    let mut values = Vec::with_capacity(reprs.len());
    for r in reprs {
        let n = typelisp_mem::words_for(crossing_bits(r));
        if rest.len() < n {
            return Err(arity_mismatch(words.len(), reprs.len()));
        }
        let (mine, others) = rest.split_at(n);
        values.push(typelisp_mem::raw_value(mine));
        rest = others;
    }
    if !rest.is_empty() {
        return Err(arity_mismatch(words.len(), reprs.len()));
    }
    Ok(values)
}

fn arity_mismatch(words: usize, params: usize) -> EvalError {
    EvalError::Internal(format!("arity mismatch: the closure takes {} argument(s), given {} words", params, words))
}

/// One value's worth of `Interp::encode_crossing_args` — see
/// `Interp::encode_crossing_value`, which this is, for a caller with no
/// interpreter (a worker thread's printer): nothing here needs one.
pub(crate) fn encode_crossing_value(heap: &Heap, v: &Value, repr: &Repr) -> Result<i64, EvalError> {
    // A built-in used as a function value is a box like any other, so it
    // would encode as an ordinary tagged word — and then the apply
    // boundary would find a callee whose arguments it has no way to
    // decode: a
    // built-in carries only its name, and the apply site does not carry
    // the representations at runtime (an interpreted closure does, which
    // is why *it* crosses fine). Rejected here, where it is still an
    // ordinary catchable error rather than an abort from inside a
    // compiled frame.
    if let Value::Boxed(id) = v {
        if heap.is_builtin_fn(*id) {
            return Err(EvalError::Internal(format!(
                "compiled call: the built-in \"{}\" cannot be passed as a function value to compiled code",
                heap.builtin_fn_name(*id)
            )));
        }
    }
    match repr {
        // Raw machine words, by the encodings compiled code uses
        // internally.
        Repr::Narrow(_) => match v {
            Value::Int(n) => Ok(*n),
            other => Err(EvalError::Internal(format!("compiled call: expected an integer argument, got {:?}", other))),
        },
        // An `int` crosses as the tagged word it is — the catch-all's
        // `encode` — but only the two shapes it can have are let through:
        // `encode` would abort on a `Value::Int` past the fixnum range
        // and happily tag any other box, and neither is an `int`.
        Repr::Int => match v {
            Value::Int(n) if typelisp_mem::fixnum_fits(*n) => Ok(typelisp_rt::encode(*v) as i64),
            Value::Boxed(id) if heap.is_bignum(*id) => Ok(typelisp_rt::encode(*v) as i64),
            other => Err(EvalError::Internal(format!("compiled call: expected an int argument (fixnum or bignum), got {:?}", other))),
        },
        Repr::Bool => match v {
            Value::Bool(b) => Ok(i64::from(*b)),
            other => Err(EvalError::Internal(format!("compiled call: expected a bool argument, got {:?}", other))),
        },
        Repr::Char => match v {
            Value::Char(c) => Ok(*c as i64),
            other => Err(EvalError::Internal(format!("compiled call: expected a char argument, got {:?}", other))),
        },
        // `()` crosses as the plain `0` `compile-unit` compiles a
        // `Unit`-typed body tail to. A unit value carries no information,
        // so the word is a placeholder the callee never reads; it just has
        // to be the one both sides agree on.
        Repr::Unit => Ok(0),
        // A float is its raw `f64::to_bits` pattern in an `i64`
        // (`compile-float`/`llvm_builder_build_float_op`), the inverse of
        // `decode_compiled_return`'s float arms, and an `f32` its own 32
        // bits. Each width is read out of its own box, rather than one arm
        // accepting whichever box turns up.
        Repr::F64 => match v {
            Value::Boxed(id) if heap.is_f64(*id) => Ok(heap.f64_value(*id).to_bits() as i64),
            other => Err(EvalError::Internal(format!("compiled call: expected an f64 argument, got {:?}", other))),
        },
        Repr::F32 => match v {
            Value::Boxed(id) if heap.is_f32(*id) => Ok(i64::from(heap.f32_value(*id).to_bits())),
            other => Err(EvalError::Internal(format!("compiled call: expected an f32 argument, got {:?}", other))),
        },
        // An LLVM handle crosses as the *raw* registry index — never
        // tagged. Without its own arm it would fall into the tagged
        // catch-all below and the callee would read `handle << 3` as a
        // handle: passing the island its own module as handle 1 made it
        // look up handle 8 and abort with "dangling llvm handle 8". Must
        // stay ahead of the catch-all, exactly as the matching check in
        // `decode_compiled_return` does.
        Repr::Handle => match v {
            Value::Int(h) => Ok(*h),
            other => Err(EvalError::Internal(format!("compiled call: expected an llvm handle argument, got {:?}", other))),
        },
        // A `ptr`/`c-long`/`c-ulong` crosses raw, for the same reason a
        // handle does and with more at stake: all 64 bits are meaningful,
        // so the tagged catch-all would not merely misread it — it would
        // discard the top three. Must stay ahead of that catch-all.
        Repr::RawWord => match v {
            Value::Int(n) => Ok(*n),
            other => Err(EvalError::Internal(format!("compiled call: expected a raw word argument, got {:?}", other))),
        },
        // Every remaining representation — `Sexpr`, `string`,
        // `bignum`/`ratio`, structs, enums, closures, trait objects, and a
        // `Scope<V>` (one heap object since Phase 1a) — is already a heap
        // `Value`, and crosses as the tagged `i64` the `rt_*` shims read.
        // A niched `Option<T>` too: its `Some` is the `Value` of a `T`
        // as a field holds it (`Value::Int` for a narrow payload, which
        // `encode` tags as a fixnum — the field encoding), its `None`
        // the empty list.
        _ => Ok(typelisp_rt::encode(*v) as i64),
    }
}

/// Calls the compiled body at `address`, built under `abi`, with `args`
/// already in the compiled representation — [`CompiledBody::call`] for a
/// caller that holds the two facts rather than the body (a worker thread's
/// printer, which cannot hold the interpreter's `Rc`).
pub(crate) fn call_body(address: usize, abi: u8, args: &[Word]) -> i64 {
    if abi == typelisp_abi::BODY_ABI_COROUTINE {
        return call_coroutine(address, args);
    }
    // SAFETY: `address` comes from a symbol the backend resolved out of a
    // module that defines it, so it points at a function built under the
    // shared ABI. `CompiledFn::new`/`new_multi` are the only producers.
    let f: unsafe extern "C-unwind" fn(*const Word, u32) -> Word =
        unsafe { std::mem::transmute::<usize, unsafe extern "C-unwind" fn(*const Word, u32) -> Word>(address) };
    unsafe { f(args.as_ptr(), args.len() as u32) as i64 }
}

/// This is the boundary an *interpreted* caller sits at: the interpreter is
/// waiting on a Rust stack frame for an answer, so a task that would have to
/// wait underneath has nobody to hand control back to. What the body
/// suspends for is resolved by `run_to_end` itself — answered on the spot
/// through the driving scheduler when it stops for nothing, refused into
/// the chain as a panic when it would wait — so nothing about suspension
/// comes out of here. The interpreter's own nested evaluation
/// (`Interp::run_to_completion`) draws the same line for the same reason.
fn call_coroutine(address: usize, args: &[Word]) -> i64 {
    // SAFETY: same provenance as the classic branch — a symbol resolved out of
    // a module that defines it, declared under `coroutine_fn_type`.
    let f: typelisp_rt::coroutine::CoroutineFn =
        unsafe { std::mem::transmute::<usize, typelisp_rt::coroutine::CoroutineFn>(address) };
    let heap = unsafe { typelisp_abi::active_heap() };
    let mut stack = typelisp_rt::coroutine::FrameStack::new();
    // `run_to_end`, not `run`: an `apply` of an interpreted function value
    // underneath has to happen on *this* Rust frame, because this frame is
    // what the caller is waiting on. See `FrameStack::run_to_end`.
    let applied = |closure: Word, argv: &[Word]| unsafe {
        typelisp_rt::apply_on_this_frame(closure, argv, "a compiled function reached from the interpreter")
    };
    match stack.run_to_end(heap, f, args, &[], applied) {
        Ok(word) => word,
        // Resolved by `run_to_end` above, so reaching here is a protocol
        // break rather than a limitation.
        Err(typelisp_rt::coroutine::Paused::Applying { .. } | typelisp_rt::coroutine::Paused::ApplyingDyn { .. }) => typelisp_abi::raise(
            "a compiled function applied an interpreted value that the drive did not resolve".to_string(),
        )
        .into_zero(),
        // Resolved by `run_to_end` too — answered or refused into the chain.
        Err(typelisp_rt::coroutine::Paused::Suspended) => typelisp_abi::fatal(
            "a compiled function suspended and the drive did not resolve it — a driver on a machine frame must use `run_to_end`",
        ),
        // The unwind was travelling towards a `catch` that is not in this
        // chain; raising it again puts it back on the path it was on, and
        // the caller's `catch_compiled_panic` takes it.
        Err(typelisp_rt::coroutine::Paused::Unwinding) => typelisp_rt::coroutine::resume_unwinding().into_zero(),
    }
}

impl Interp {
    /// Runs already-evaluated `argv` (`param_tys`-typed, `ret_ty`-returning)
    /// through `compiled` instead of tree-walking — shared by `call`
    /// and `assoc`, the two places eval can reach a `(compile
    /// "name")`d body from. Every parameter/return type is either `Sexpr`
    /// (`compile::runtime::encode`/`decode`, Stage 5 of the
    /// Sexpr-representation plan) or assumed already representable as a
    /// plain `i64` at the compiled ABI level — true for an `i64`/`i32`
    /// itself, and for a general-ADT pointer (`Option`/`defstruct`, Stage 6)
    /// that *already* crossed this same boundary once (so it's sitting in
    /// `argv` as a `Value::Int`, not decoded into a boxed struct/enum/etc.
    /// — general-ADT bridging at this boundary isn't implemented, a
    /// pre-existing, separate gap). Calling a compiled method on a receiver
    /// built by *pure* interpretation (a real boxed struct,
    /// never touched by compiled code) hits the same wall an analogous top-level
    /// `call` already would for a general-ADT parameter — a clear
    /// internal error here, not a silent misread of unrelated bits.
    pub fn call_compiled(
        &self,
        heap: &mut Heap,
        compiled: &dyn CompiledBody,
        argv: &[Value],
        param_reprs: &[Repr],
        ret: &Repr,
    ) -> Result<Value, EvalError> {
        // Recorded before anything is pushed, so the unwind path below can
        // discard the crossing roots and whatever compiled code pushed on top
        // of them in one shot — an unwinding `(panic ...)` runs none of the
        // pops that normally balance either.
        let (int_args, crossing_roots, roots_on_entry) = self.begin_compiled(heap, argv, param_reprs)?;
        let raw = match crate::eval::crossing::catch_compiled_panic(|| compiled.call(&int_args)) {
            Ok(raw) => raw,
            Err(e) => {
                heap.truncate_roots(roots_on_entry);
                return Err(e);
            }
        };
        self.finish_compiled(heap, raw, ret, crossing_roots)
    }

    /// [`Self::call_compiled`]'s marshaling half, for a caller that **drives**
    /// the body instead of calling it.
    ///
    /// A coroutine-ABI body reached from a task is not a call: the task owns
    /// the frame chain, so the crossing has to be split in two around however
    /// many times the body stops and starts again. Everything between the two
    /// halves — the encoded words, how many roots were pushed, where the root
    /// stack stood — travels in the task's state (`core_cps`'s `DriveCtx`)
    /// rather than in Rust locals, which is exactly what makes the suspension
    /// in the middle survivable.
    ///
    /// Returns the encoded arguments, how many crossing roots were pushed (the
    /// caller pops exactly that many through [`Self::finish_compiled`]), and
    /// the root count from before any of it, for the unwind path to truncate
    /// to — an unwinding `(panic ...)` runs none of the pops that balance.
    pub(crate) fn begin_compiled(
        &self,
        heap: &mut Heap,
        argv: &[Value],
        param_reprs: &[Repr],
    ) -> Result<(Vec<Word>, usize, usize), EvalError> {
        let roots_on_entry = heap.root_count();
        // A `defun`/`defmethod`'s `sig` lists only its fixed parameters; a
        // `&rest` one that reached compilation would land in the "no declared
        // representation" error above rather than be guessed at.
        let (int_args, crossing_roots) = self.encode_crossing_args(heap, argv, param_reprs, false)?;
        self.enter_compiled(heap);
        Ok((int_args, crossing_roots, roots_on_entry))
    }

    /// The other half of [`Self::begin_compiled`]: discard the crossing roots
    /// and read the returned word back as a value.
    pub(crate) fn finish_compiled(
        &self,
        heap: &mut Heap,
        raw: i64,
        ret: &Repr,
        crossing_roots: usize,
    ) -> Result<Value, EvalError> {
        for _ in 0..crossing_roots {
            heap.pop_root();
        }
        self.decode_compiled_return(heap, raw, ret)
    }

    /// [`Self::call_compiled`]'s argument-marshaling half, factored out so
    /// [`Self::eval`]'s `apply` arm can reuse it when the callee is a
    /// `BoxedObj::CompiledClosure` rather than a top-level `(compile ...)`d
    /// function — both cross the exact same interpreter -> compiled-ABI
    /// `i64` boundary. Returns the encoded `int_args` plus how many roots it
    /// pushed (over `argv`'s heap-backed elements only); the caller must pop
    /// exactly that many once the call this feeds into has returned. See
    /// `call_compiled`'s (pre-refactor) doc comment for why encoding reads
    /// `argv`'s own runtime shape rather than each parameter's static type,
    /// and why every heap-backed argument is rooted for the marshaling+call
    /// window.
    /// `param_reprs` are the callee's *declared* parameter representations, and
    /// they are required rather than advisory: since the scalar unification an
    /// `f64` and a `Sexpr` holding a float are the same runtime shape
    /// (`Value::Boxed(float)`), and they cross the boundary differently — an
    /// `f64` parameter takes raw `f64::to_bits`, a `Sexpr` parameter takes the
    /// tagged box pointer. The value cannot say which; only the declaration
    /// can. Reading it off the value instead made `(which (Float 1.5))` miss
    /// its `(float _)` arm in compiled code while the interpreted call still
    /// matched.
    ///
    /// A `Repr` rather than a `Type`, since Stage C: the type is gone from the
    /// IR by the time a definition is registered, and every distinction this
    /// function draws is one a `Repr` already makes. That is what a `Repr` is
    /// — the residue of the type system the boundary needs.
    pub(super) fn encode_crossing_args(
        &self,
        heap: &mut Heap,
        argv: &[Value],
        param_reprs: &[Repr],
        rest: bool,
    ) -> Result<(Vec<Word>, usize), EvalError> {
        let mut crossing_roots = 0usize;
        let mut int_args: Vec<Word> = Vec::with_capacity(argv.len());
        // Root every argument up front, before the encode loop below
        // allocates anything. (Rooting a scalar is a no-op for the collector,
        // so the pass does not bother telling them apart.) A later argument's
        // encoding can allocate — a `Str`/`Bignum`/`Ratio` argument copies
        // itself onto the GC heap — and that allocation can trigger a GC;
        // without this pre-pass, an *earlier-in-`argv`* alloc (e.g. a
        // leading `Str` argument) would collect a not-yet-rooted `Sexpr`
        // argument sitting later in `argv`, whose freed string/cons slots
        // then get recycled under it (observed compiling the self-hosted
        // island: a `defun`'s `name` string argument's `alloc_string`
        // reclaimed its own body AST, so a `(var "x")` node read back as the
        // `name`). Rooting order doesn't matter for protection — only that
        // every heap arg is rooted before the first allocation — so this
        // separate pass is the whole fix.
        for v in argv {
            heap.push_root(*v);
            crossing_roots += 1;
        }
        for (i, v) in argv.iter().enumerate() {
            // Past the fixed parameters there is exactly one more argument:
            // the `&rest` list. The checker packs every surplus argument into
            // that single `Sexpr` at *check* time
            // (`wrap_rest_elem`/`cons_rest_list`), so what crosses here is the
            // whole list, and it crosses tagged like any other `Sexpr`.
            //
            // `Repr::Sexpr`, not the `&rest` *element*'s representation:
            // using the element here asked a list to be an `i32` ("expected
            // an integer argument, got Cons"). The two are only
            // distinguishable by the declaration, since a packed list and an
            // ordinary `Sexpr` argument are the same kind of value.
            let param_repr = match param_reprs.get(i) {
                Some(r) => Some(r),
                None if rest => Some(&Repr::Sexpr),
                None => None,
            };
            let encoded = match param_repr {
                Some(r) => self.encode_crossing_value(heap, v, r),
                None => Err(EvalError::Internal(format!(
                    "compiled call: no declared representation for argument {} — cannot tell a raw scalar from a tagged heap word",
                    i
                ))),
            };
            match encoded {
                Ok(n) => int_args.extend(typelisp_mem::value_words(n, param_repr.map_or(Word::BITS, crossing_bits))),
                Err(e) => {
                    for _ in 0..crossing_roots {
                        heap.pop_root();
                    }
                    return Err(e);
                }
            }
        }
        Ok((int_args, crossing_roots))
    }

    /// One value's worth of [`Self::encode_crossing_args`] — the exact
    /// inverse of [`Self::decode_compiled_return`], and shared with it by
    /// [`Self::apply_interpreted`], which crosses the same boundary in the
    /// other direction (compiled code's argument words in, the interpreted
    /// result's word out).
    ///
    /// The dispatch is on the *declared* representation, never on the value's
    /// shape. It used to read the value: an `i32` argument was `RtValue::Int`
    /// and crossed as a raw machine word, while a `Sexpr` argument holding an
    /// integer was `RtValue::Sexpr(Value::Int)` and crossed as a tagged word,
    /// and the two variants told them apart. With one value universe both are
    /// `Value::Int(n)` and the value says nothing — exactly the ambiguity
    /// that made `(which (Float 1.5))` miss its `(float _)` arm when `f64`
    /// unified (`compile_match_distinguishes_float_bignum_and_ratio_boxes`).
    /// A caller with no declared representation to consult must raise an
    /// error rather than guess: either encoding would corrupt the call
    /// silently.
    ///
    /// Encoding allocates nothing, so no rooting happens (or is needed) here.
    fn encode_crossing_value(&self, heap: &Heap, v: &Value, repr: &Repr) -> Result<i64, EvalError> {
        encode_crossing_value(heap, v, repr)
    }

    /// [`Self::call_compiled`]'s return-value half, factored out for the same
    /// reason as [`Self::encode_crossing_args`] — a direct `apply` on a
    /// `BoxedObj::CompiledClosure` decodes its raw `i64` result exactly like a
    /// top-level compiled call's, by the callee's declared return
    /// representation.
    ///
    /// A `Repr` rather than a `Type` since Stage C, which collapsed the old
    /// `is_boxed_sexpr_type` predicate into the classification itself: which
    /// `Type::Named`s cross as a tagged word (a `defstruct`, an enum, a
    /// `Scope<V>`, a trait object, a closure) was a rule spelled out at that
    /// predicate; a `Repr` already *is* that rule's answer.
    pub(super) fn decode_compiled_return(&self, heap: &mut Heap, raw: i64, ret: &Repr) -> Result<Value, EvalError> {
        decode_crossing_return(heap, raw, ret)
    }

    /// Invokes a **classic-ABI** `BoxedObj::CompiledClosure` directly from
    /// interp Rust code — the `apply` counterpart of a top-level
    /// `(compile ...)`d function call, for a callee produced by compiled
    /// code (returned across the boundary, or built and threaded through a
    /// chain of `apply`s the interpreter is itself driving). `args` are
    /// already-encoded raw i64s ([`Self::encode_crossing_args`]'s output);
    /// the caller must already have called `compile::runtime::set_active_heap`
    /// (the closure's own body may call back into the `rt_*` runtime,
    /// directly or transitively through another compiled function).
    /// Marshals the closure's captured environment into the fixed
    /// `compiled_fn_type_with_env` ABI (`args_ptr, argc, env_ptr, env_len`)
    /// every closure-boxed compiled function shares — the exact inverse of
    /// `rt_closure_env_get`'s per-slot re-encode, done here in one pass
    /// since the whole env crosses at once rather than one slot per call.
    ///
    /// A **coroutine**-ABI closure never reaches here: `finish_apply` routes
    /// it through `State::CompiledEnter`/`DriveCallee::Closure` instead, so
    /// it is driven by the task and can suspend — the same reason a *named*
    /// coroutine-ABI body is driven rather than called. This function's own
    /// caller only ever names a classic one.
    pub(super) fn call_closure_box(heap: &Heap, id: BoxId, args: &[Word]) -> i64 {
        debug_assert!(
            heap.compiled_closure_body_abi(id) != typelisp_abi::BODY_ABI_COROUTINE,
            "call_closure_box: a coroutine-ABI closure must be driven through CompiledEnter, not called"
        );
        let env_len = heap.compiled_closure_env_len(id);
        let mask = heap.compiled_closure_mask(id);
        let env: Vec<Word> = (0..env_len)
            .map(|i| {
                let v = heap.compiled_closure_env_get(id, i);
                if mask & (1 << i) != 0 {
                    typelisp_rt::encode(v)
                } else {
                    match v {
                        Value::Int(raw) => raw as Word,
                        other => unreachable!("compiled closure env slot {} holds a non-raw value {:?} for an unmasked slot", i, other),
                    }
                }
            })
            .collect();
        let fn_ptr = heap.compiled_closure_fnptr(id);
        // SAFETY: every `BoxedObj::CompiledClosure` in the heap was built by
        // `rt_closure_new` from a real LLVM function pointer compiled under
        // `compiled_fn_type_with_env`'s exact signature (`build-make-closure`
        // in `compiler.rs`'s `compile-lambda`/`resolve-value` is its only
        // producer) — there is no other way to construct one, so `fn_ptr`
        // always points at a function with this signature.
        // A `(panic ...)` in this closure's body comes back as a raise
        // waiting on the thread, for the caller's `catch_compiled_panic`.
        let f: unsafe extern "C-unwind" fn(*const Word, u32, *const Word, u32) -> Word = unsafe { std::mem::transmute(fn_ptr) };
        unsafe { f(args.as_ptr(), args.len() as u32, env.as_ptr(), env.len() as u32) as i64 }
    }

    /// Registers this thread's `Heap` and `Interp` for the compiled code
    /// about to run, and wires the interpreter re-entry hook
    /// the apply boundary calls when it finds an interpreted callee.
    ///
    /// Called immediately before *every* crossing into compiled code rather
    /// than once per session: it is three pointer stores, and there is no
    /// cheaper place to decide ahead of time whether a given callee might
    /// transitively touch the heap or apply a function value. The heap
    /// registration is what makes `rt-cons`/`rt-car`/... resolve against the
    /// right heap — see `compile::runtime::set_active_heap`'s doc comment.
    ///
    /// AOT has no counterpart: an AOT-compiled executable is its own process
    /// with no interpreter in it, so `rt_heap_init` registers the heap alone
    /// and an interpreted callee there is an invariant break (see
    /// `typelisp_rt::apply_on_this_frame`).
    pub(super) fn enter_compiled(&self, heap: &mut Heap) {
        typelisp_rt::set_active_heap(heap as *mut Heap);
        self.install_print_hooks();
        typelisp_rt::set_apply_interpreted(Some(rt_apply_interpreted));
        typelisp_rt::set_dyn_slot_closure(Some(rt_dyn_slot_closure));
    }

    /// The address of the entry C calls for the callback `sig` — made by the
    /// backend the first time it is asked for, and again whenever the
    /// function has since been redefined or recompiled.
    pub(crate) fn callback_address(&self, heap: &mut Heap, sig: crate::ffi_callback::CallbackSig) -> Result<usize, EvalError> {
        self.make_callback_entry(heap, sig).map_err(EvalError::Panic)
    }

    /// [`Self::callback_address`], failing as a message — the shape the
    /// runtime's factory answers in.
    pub(super) fn make_callback_entry(&self, heap: &mut Heap, sig: crate::ffi_callback::CallbackSig) -> Result<usize, String> {
        let current = self.root.borrow().get_fn(&sig.function).and_then(|f| f.compiled.borrow().clone());
        if let (Some((addr, body)), Some(now)) = (self.callback_entries.borrow().get(&sig), &current) {
            if Rc::ptr_eq(body, now) {
                return Ok(*addr);
            }
        }
        let make = backend("a C callback").map_err(|e| e.to_string())?.callback_entry;
        let (addr, body) = make(self, heap, sig)?;
        if let Some((_, old)) = self.callback_entries.borrow_mut().insert(sig, (addr, body)) {
            self.retired_callback_bodies.borrow_mut().push(old);
        }
        Ok(addr)
    }
}

impl Interp {
    /// The closure an unfilled vtable slot's method reifies to —
    /// [`rt_dyn_slot_closure`]'s body, and the `:dyn` counterpart of
    /// [`Self::apply_interpreted`]'s closure argument.
    ///
    /// The slot names a `(type path, method)` pair (`Self::vtables`, filled
    /// by the boxing site from the checker's own `TraitDef::method_order`),
    /// and reifying it produces exactly the closure an interpreted
    /// `(method-ref ...)` would — carrying the parameter and return
    /// representations the caller decodes by.
    pub(super) fn dyn_slot_closure(&self, heap: &mut Heap, vtable: u32, slot: u32) -> Result<Word, EvalError> {
        let target = self
            .vtables
            .borrow()
            .get(vtable as usize)
            .and_then(|slots| slots.get(slot as usize).cloned())
            .ok_or_else(|| EvalError::Internal(format!("vtable {} has no slot {}", vtable, slot)))?;
        let (type_path, method) = target;
        let f = self
            .root
            .borrow()
            .get_method(&type_path, method)
            .ok_or_else(|| EvalError::Internal(format!("vtable slot names `{}::{}`, which is not registered", type_path, method.name())))?;
        let v = self.reify(heap, &f)?;
        Ok(typelisp_rt::encode(v))
    }

    /// Applies an *interpreted* closure that compiled code is calling —
    /// [`rt_apply_interpreted`]'s body, and the compiled -> interpreted
    /// direction of the boundary [`Self::call_compiled`] crosses the other
    /// way. `closure` and `argv` are raw compiled-side words.
    ///
    /// The representations to decode and encode by come from the closure box
    /// itself ([`typelisp_mem::BoxedObj::Closure`]'s `params`/`ret`), because
    /// nothing else at this point has them: the apply site knows the callee's
    /// `Fn` type statically but carries none of it into the emitted call, and
    /// The words are exactly as ambiguous here as they are anywhere else on
    /// this boundary (a raw `f64` bit pattern and a tagged box pointer are
    /// both just `i64`s). A built-in used as a function value has no
    /// signature to read, which is why it is rejected before it can ever
    /// cross — see [`Self::encode_crossing_value`].
    fn apply_interpreted(&self, heap: &mut Heap, closure: Word, argv: &[Word]) -> Result<i64, EvalError> {
        let f = typelisp_rt::decode(closure);
        let id = match f {
            Value::Boxed(id) if heap.is_closure(id) => id,
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                return Err(EvalError::Internal(format!(
                    "the built-in \"{}\" cannot be applied from compiled code: its argument representations are not carried at the apply site",
                    heap.builtin_fn_name(id)
                )))
            }
            other => return Err(EvalError::Internal(format!("the callee is not an interpreted closure: {:?}", other))),
        };
        let (params, _, _) = heap.closure_parts(id);
        let param_reprs = super::core_eval::param_reprs(heap, params)?;
        let ret = Repr::read(heap, heap.closure_ret(id))
            .ok_or_else(|| EvalError::Internal("the closure has no declared return representation".to_string()))?;
        let raws = crossing_values(argv, &param_reprs)?;

        let mut s = RootScope::new(heap);
        // The closure box is reachable from the caller's compiled frame,
        // which the collector cannot see; the arguments the caller passed are
        // rooted on its side, but their *decoded* forms (a float
        // argument allocates a box) are new objects reachable from nothing.
        s.push_root(f);
        let mut args = Vec::with_capacity(raws.len());
        for (raw, r) in raws.iter().zip(&param_reprs) {
            let v = self.decode_compiled_return(&mut s, *raw, r)?;
            s.push_root(v);
            args.push(v);
        }
        let v = self.call_interpreted_closure(&mut s, id, args)?;
        s.push_root(v);
        self.encode_crossing_value(&s, &v, &ret)
    }

    /// Whether `target` currently has a compiled body installed, so calls to it
    /// run native rather than being tree-walked.
    ///
    /// Public because "did the precompiled prelude actually take effect?" is
    /// otherwise unanswerable from outside the crate: every observable of a
    /// compiled call is identical to the interpreted one *except* how fast it
    /// is, which is exactly the wrong thing to assert on. `false` for a target
    /// with no definition, since an absent definition has no compiled body
    /// either.
    pub fn is_compiled(&self, target: &crate::CompileTarget) -> bool {
        self.target_def(target).is_some_and(|f| f.compiled.borrow().is_some())
    }

    /// Whether `target` is a `(defffi ...)` declaration.
    ///
    /// For the tools, which otherwise cannot tell its empty body from an
    /// ordinary one: [`Self::compiled_fn_body`] answers `(params, [])` for
    /// both, and building a module out of no forms produces a function that
    /// is not the one the name reaches.
    pub fn fn_is_ffi(&self, target: &crate::CompileTarget) -> bool {
        self.target_def(target).is_some_and(|f| f.ffi)
    }

    /// Looks up `target`'s registered body, enforcing the one constraint every
    /// entry point into the compiler shares: a real type signature (so an LLVM
    /// function type can be built — never set for a `defmacro`). For an
    /// instance method, `params`/`sig.0` already carry the receiver as element
    /// `0` (`typeck::defmethod` pushes the receiver's own type onto
    /// `sig_params` before the method's declared parameters) — so it flows
    /// through exactly like any other parameter here, no special-casing
    /// needed. Shared by the backend's `compile::driver::add_compiled_function`
    /// (the actual bridge step) and `compile::driver::compile_function`/
    /// `call_graph_edges` (which need the body slightly earlier — to collect
    /// `call` targets — before `add_compiled_function` ever runs).
    pub fn compiled_fn_body(&self, target: &crate::CompileTarget) -> Result<FnBody, EvalError> {
        let f = self.target_def(target).ok_or_else(|| EvalError::NoSuchFunction(target.link_name()))?;
        let sig = f.sig.as_ref().ok_or_else(|| {
            EvalError::Panic(format!("compile: \"{}\" has no signature (is it a defmacro?)", target.link_name()))
        })?;
        let params = f.params.iter().cloned().zip(sig.0.iter().cloned()).collect();
        // The body's forms, not one spliced expression: `core_bridge` takes a
        // sequence, so there is nothing for `single_body_expr`'s old
        // wrap-in-a-`progn` to do.
        Ok((params, f.body.clone()))
    }

    /// The representation of what `target` answers with.
    pub fn compiled_fn_ret(&self, target: &crate::CompileTarget) -> Result<Repr, EvalError> {
        let f = self.target_def(target).ok_or_else(|| EvalError::NoSuchFunction(target.link_name()))?;
        let sig = f.sig.as_ref().ok_or_else(|| {
            EvalError::Panic(format!("compile: \"{}\" has no signature (is it a defmacro?)", target.link_name()))
        })?;
        Ok(sig.1.clone())
    }
}

/// A compiled call handed back a word that does not decode to the shape its
/// declared representation promised — the two sides disagree about the ABI, so
/// this is an internal error rather than anything a program can cause.
fn crossing_mismatch(what: &str, got: Value) -> EvalError {
    EvalError::Internal(format!("compiled call returned {:?} for {} result", got, what))
}

/// The interpreter half of applying a function value from compiled code:
/// the callee is *not* a compiled closure, so the call has to finish in the
/// evaluator.
///
/// **The last resort, not the usual path.** A compiled `apply` names its
/// callee to the caller's own driver, which hands an interpreted one to the
/// task's continuation stack (`Paused::Applying`) — where it can suspend.
/// This is what a driver standing on a *machine* frame does instead
/// (`FrameStack::run_to_end`), because that frame is waiting for an answer.
///
/// An error from the callee *unwinds* rather than returning: there is a
/// compiled frame between here and any Rust caller that could handle a
/// `Result`, and it has no way to carry one. Unwinding is how the error gets
/// past that frame to [`crate::eval::crossing::catch_compiled_panic`], which puts it
/// back together — so a `(panic ...)` in an interpreted callback reached from
/// compiled code is as recoverable as one anywhere else.
///
/// A *missing interpreter* still aborts: that is a registration bug in
/// [`Interp::enter_compiled`], not something a program can provoke.
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s, and both a `Heap`
/// ([`typelisp_rt::set_active_heap`]) and an `Interp`
/// ([`Interp::enter_compiled`]) must be registered on this thread.
unsafe extern "C-unwind" fn rt_apply_interpreted(closure: Word, args: *const Word, argc: u32) -> i64 {
    let interp = ACTIVE_INTERP.with(|cell| cell.get());
    if interp.is_null() {
        typelisp_rt::fatal("applying a function value from compiled code: no interpreter is registered on this thread");
    }
    let interp = &*interp;
    let heap = typelisp_rt::active_heap();
    let argv = std::slice::from_raw_parts(args, argc as usize);
    match interp.apply_interpreted(heap, closure, argv) {
        Ok(w) => w,
        Err(e) => crate::eval::crossing::unwind_interpreted_failure(e).into_zero(),
    }
}

/// The interpreter half of a `:dyn` call whose vtable slot is empty: the
/// implementation behind it is an ordinary interpreted method. Hands back the
/// closure that method reifies to, which then takes the same path any other
/// interpreted callee takes out of compiled code — the driver's
/// `Paused::Applying`, or [`rt_apply_interpreted`] when the driver is
/// standing on a machine frame.
///
/// Most failures here really are compiler bugs — a slot with no
/// `(type, method)` behind it, or a method that no longer resolves — but
/// reifying the closure allocates, so heap exhaustion reaches this arm too,
/// and that a program can provoke. So errors are raised rather than abort,
/// the same way [`rt_apply_interpreted`]'s are; an internal one still surfaces
/// intact as `EvalError::Internal` rather than being flattened.
///
/// # Safety
///
/// Both a `Heap` and an `Interp` must be registered on this thread.
unsafe extern "C-unwind" fn rt_dyn_slot_closure(vtable: u32, slot: u32) -> Word {
    let interp = ACTIVE_INTERP.with(|cell| cell.get());
    if interp.is_null() {
        typelisp_rt::fatal("a :dyn call with an interpreted implementation: no interpreter is registered on this thread");
    }
    let interp = &*interp;
    let heap = typelisp_rt::active_heap();
    match interp.dyn_slot_closure(heap, vtable, slot) {
        Ok(w) => w,
        Err(e) => crate::eval::crossing::unwind_interpreted_failure(e).into_word(),
    }
}
