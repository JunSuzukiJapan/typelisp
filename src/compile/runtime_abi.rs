//! The C signature of each runtime function, read off the Rust function.
//!
//! A runtime function (`rt_*`) is an ordinary C function of its own
//! signature: `rt_cons(car, cdr)` takes two words in two registers. The
//! table of them (`externs::rt_extern_functions`) holds each as a function
//! pointer of its type, written as a cast of the function itself, so the
//! signature the code generator declares is the one the definition has —
//! a table entry that disagrees with its definition does not compile.
//!
//! A function whose arguments are a list of any length (`rt_struct_new`'s
//! fields, a `format` piece's record) takes the list as an array and its
//! length (`*const i64, u32`), which is its signature like any other.

use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, FunctionType};
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

/// A parameter's or result's type, as the C side sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RtTy {
    /// The word ([`crate::compile::carriers::word`]).
    Word,
    /// An array's length.
    U32,
    /// An array's address.
    Ptr,
}

impl RtTy {
    fn llvm(self) -> BasicMetadataTypeEnum<'static> {
        let ctx = crate::compile::llvm_context();
        match self {
            RtTy::Word => crate::compile::carriers::word().into(),
            RtTy::U32 => ctx.i32_type().into(),
            RtTy::Ptr => ctx.ptr_type(AddressSpace::default()).into(),
        }
    }
}

/// A Rust type a runtime function's parameter or result has.
pub(crate) trait RtParam {
    const TY: RtTy;
}

impl RtParam for i64 {
    const TY: RtTy = RtTy::Word;
}

impl RtParam for u32 {
    const TY: RtTy = RtTy::U32;
}

impl RtParam for *const i64 {
    const TY: RtTy = RtTy::Ptr;
}

/// A runtime function's signature: its parameters and its result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sig {
    pub params: Vec<RtTy>,
    pub ret: RtTy,
}

/// As LLVM spells the type (`i64 (ptr, i32)`), written without asking LLVM,
/// whose context every thread shares.
impl std::fmt::Display for Sig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let spelled = |t: &RtTy| match t {
            RtTy::Word => "i64",
            RtTy::U32 => "i32",
            RtTy::Ptr => "ptr",
        };
        let params: Vec<&str> = self.params.iter().map(spelled).collect();
        write!(f, "{} ({})", spelled(&self.ret), params.join(", "))
    }
}

impl Sig {
    /// The LLVM function type of the signature.
    pub(crate) fn fn_type(&self) -> FunctionType<'static> {
        let params: Vec<BasicMetadataTypeEnum<'static>> = self.params.iter().map(|p| p.llvm()).collect();
        match self.ret {
            RtTy::Word => crate::compile::carriers::word().fn_type(&params, false),
            RtTy::U32 => crate::compile::llvm_context().i32_type().fn_type(&params, false),
            RtTy::Ptr => crate::compile::llvm_context().ptr_type(AddressSpace::default()).fn_type(&params, false),
        }
    }

    /// Whether the function takes its arguments as an array and its length.
    pub(crate) fn takes_array(&self) -> bool {
        self.params == [RtTy::Ptr, RtTy::U32]
    }
}

/// A function pointer type a runtime function can have.
pub(crate) trait RtFn: Copy {
    fn sig() -> Sig;
    fn addr(self) -> usize;
}

macro_rules! rt_fn_arity {
    ($($p:ident),*) => {
        impl<R: RtParam, $($p: RtParam),*> RtFn for unsafe extern "C" fn($($p),*) -> R {
            fn sig() -> Sig {
                Sig { params: vec![$($p::TY),*], ret: R::TY }
            }
            fn addr(self) -> usize {
                self as usize
            }
        }
        impl<R: RtParam, $($p: RtParam),*> RtFn for unsafe extern "C-unwind" fn($($p),*) -> R {
            fn sig() -> Sig {
                Sig { params: vec![$($p::TY),*], ret: R::TY }
            }
            fn addr(self) -> usize {
                self as usize
            }
        }
    };
}

rt_fn_arity!();
rt_fn_arity!(A);
rt_fn_arity!(A, B);
rt_fn_arity!(A, B, C);
rt_fn_arity!(A, B, C, D);
rt_fn_arity!(A, B, C, D, E);
rt_fn_arity!(A, B, C, D, E, F);

/// One runtime function: its address and its signature.
#[derive(Clone, Debug)]
pub(crate) struct Shim {
    pub addr: usize,
    pub sig: Sig,
}

/// The table entry for `f`, a runtime function cast to its own type.
pub(crate) fn shim<F: RtFn>(f: F) -> Shim {
    Shim { addr: f.addr(), sig: F::sig() }
}

/// `name` declared in `module` under its own signature, or the declaration
/// already there.
pub(crate) fn declare(module: &Module<'static>, name: &str, sig: &Sig) -> FunctionValue<'static> {
    match module.get_function(name) {
        Some(f) => f,
        None => module.add_function(name, sig.fn_type(), None),
    }
}

/// Calls the runtime function `name` with `args`, the words compiled code
/// holds, converting each to its parameter's type, and answers its result
/// as a word. Raises nothing itself: whoever calls this asks
/// [`crate::compile::raise_check::after_call`] when the callee can raise.
pub(crate) fn call(
    builder: &inkwell::builder::Builder<'static>,
    module: &Module<'static>,
    name: &str,
    args: &[inkwell::values::BasicValueEnum<'static>],
) -> Result<(FunctionValue<'static>, inkwell::values::IntValue<'static>), String> {
    let sig = crate::compile::externs::runtime_sig(name).ok_or_else(|| format!("`{}` is not a runtime function", name))?;
    let f = declare(module, name, sig);
    call_declared(builder, f, sig, args)
}

/// [`call`] with the callee and its signature in hand.
pub(crate) fn call_declared(
    builder: &inkwell::builder::Builder<'static>,
    f: FunctionValue<'static>,
    sig: &Sig,
    args: &[inkwell::values::BasicValueEnum<'static>],
) -> Result<(FunctionValue<'static>, inkwell::values::IntValue<'static>), String> {
    let name = f.get_name().to_string_lossy().into_owned();
    if args.len() != sig.params.len() {
        return Err(format!("`{}` takes {} arguments, given {}", name, sig.params.len(), args.len()));
    }
    let ctx = crate::compile::llvm_context();
    let mut passed: Vec<inkwell::values::BasicMetadataValueEnum<'static>> = Vec::with_capacity(args.len());
    for (a, ty) in args.iter().zip(&sig.params) {
        let v: inkwell::values::BasicMetadataValueEnum<'static> = match (ty, a) {
            (RtTy::Ptr, inkwell::values::BasicValueEnum::PointerValue(p)) => (*p).into(),
            (RtTy::Ptr, inkwell::values::BasicValueEnum::IntValue(w)) => builder
                .build_int_to_ptr(*w, ctx.ptr_type(AddressSpace::default()), "arg_ptr")
                .map_err(|e| format!("{}: {}", name, e))?
                .into(),
            (RtTy::U32, inkwell::values::BasicValueEnum::IntValue(w)) if w.get_type().get_bit_width() == 32 => (*w).into(),
            (RtTy::U32, inkwell::values::BasicValueEnum::IntValue(w)) => builder
                .build_int_truncate(*w, ctx.i32_type(), "arg_u32")
                .map_err(|e| format!("{}: {}", name, e))?
                .into(),
            (RtTy::Word, inkwell::values::BasicValueEnum::IntValue(w)) => (*w).into(),
            (RtTy::Word, inkwell::values::BasicValueEnum::PointerValue(p)) => builder
                .build_ptr_to_int(*p, crate::compile::carriers::word(), "arg_word")
                .map_err(|e| format!("{}: {}", name, e))?
                .into(),
            (ty, other) => return Err(format!("`{}`: a {:?} parameter given {:?}", name, ty, other.get_type())),
        };
        passed.push(v);
    }
    let call = builder.build_call(f, &passed, "rt_result").map_err(|e| format!("{}: {}", name, e))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(inkwell::values::BasicValueEnum::IntValue(v)) => Ok((f, v)),
        _ => Err(format!("`{}` answers no word", name)),
    }
}
