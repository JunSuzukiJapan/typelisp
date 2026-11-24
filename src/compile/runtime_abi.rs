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
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;

use crate::compile::carriers::Carrier;

/// A parameter's or result's type, as the C side sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RtTy {
    /// A value carried at its own type.
    Value(Carrier),
    /// An array's address.
    Ptr,
}

/// The length that goes with an array.
const LENGTH: RtTy = RtTy::Value(Carrier::Int { bits: 32, signed: false });

impl RtTy {
    pub(crate) fn llvm(self) -> BasicTypeEnum<'static> {
        match self {
            RtTy::Value(c) => c.llvm().unwrap_or_else(|| unreachable!("a runtime function takes and answers no unit")),
            RtTy::Ptr => crate::compile::llvm_context().ptr_type(AddressSpace::default()).into(),
        }
    }

    /// The C ABI's extension of an integer narrower than a register's
    /// smallest width: which attribute says the caller of a parameter, or
    /// the callee for a result, has done it.
    fn extension(self) -> Option<&'static str> {
        match self {
            RtTy::Value(Carrier::Bool) => Some("zeroext"),
            RtTy::Value(Carrier::Int { bits: 8 | 16, signed }) => Some(if signed { "signext" } else { "zeroext" }),
            _ => None,
        }
    }
}

/// A Rust type a runtime function's parameter or result has.
pub(crate) trait RtParam {
    const TY: RtTy;
}

macro_rules! rt_params {
    ($($t:ty => $c:expr,)*) => {$(
        impl RtParam for $t {
            const TY: RtTy = RtTy::Value($c);
        }
    )*};
}
rt_params! {
    typelisp_mem::Word => Carrier::Word,
    i8 => Carrier::Int { bits: 8, signed: true },
    i16 => Carrier::Int { bits: 16, signed: true },
    i32 => Carrier::Int { bits: 32, signed: true },
    i64 => Carrier::Int { bits: 64, signed: true },
    u8 => Carrier::Int { bits: 8, signed: false },
    u16 => Carrier::Int { bits: 16, signed: false },
    u32 => Carrier::Int { bits: 32, signed: false },
    u64 => Carrier::Int { bits: 64, signed: false },
    f32 => Carrier::F32,
    f64 => Carrier::F64,
    bool => Carrier::Bool,
}

impl RtParam for *const typelisp_mem::Word {
    const TY: RtTy = RtTy::Ptr;
}

/// A runtime function's signature: its parameters and its result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sig {
    pub params: Vec<RtTy>,
    pub ret: RtTy,
}

/// Written without asking LLVM, whose context every thread shares.
impl std::fmt::Display for Sig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let spelled = |t: &RtTy| match t {
            RtTy::Value(Carrier::Word) => "word".to_string(),
            RtTy::Value(Carrier::Int { bits, signed }) => format!("{}{}", if *signed { "i" } else { "u" }, bits),
            RtTy::Value(Carrier::F32) => "float".to_string(),
            RtTy::Value(Carrier::F64) => "double".to_string(),
            RtTy::Value(Carrier::Bool) => "bool".to_string(),
            RtTy::Value(Carrier::Unit) => unreachable!("a runtime function takes and answers no unit"),
            RtTy::Ptr => "ptr".to_string(),
        };
        let params: Vec<String> = self.params.iter().map(spelled).collect();
        write!(f, "{} ({})", spelled(&self.ret), params.join(", "))
    }
}

impl Sig {
    /// The LLVM function type of the signature.
    pub(crate) fn fn_type(&self) -> FunctionType<'static> {
        let params: Vec<BasicMetadataTypeEnum<'static>> = self.params.iter().map(|p| p.llvm().into()).collect();
        self.ret.llvm().fn_type(&params, false)
    }

    /// Where the C ABI extends a narrow integer, and how.
    fn extensions(&self) -> Vec<(AttributeLoc, Attribute)> {
        let ctx = crate::compile::llvm_context();
        let attr = |name: &str| ctx.create_enum_attribute(Attribute::get_named_enum_kind_id(name), 0);
        let params = self.params.iter().enumerate().filter_map(|(i, p)| Some((AttributeLoc::Param(i as u32), attr(p.extension()?))));
        params.chain(self.ret.extension().map(|e| (AttributeLoc::Return, attr(e)))).collect()
    }

    /// Whether the function takes its arguments as an array and its length.
    pub(crate) fn takes_array(&self) -> bool {
        self.params == [RtTy::Ptr, LENGTH]
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
        None => {
            let f = module.add_function(name, sig.fn_type(), None);
            for (at, attr) in sig.extensions() {
                f.add_attribute(at, attr);
            }
            f
        }
    }
}

/// Calls the runtime function `name` with `args`, converting each to its
/// parameter's type, and answers its result. Raises nothing itself: whoever
/// calls this asks [`crate::compile::raise_check::after_call`] when the
/// callee can raise.
pub(crate) fn call(
    builder: &inkwell::builder::Builder<'static>,
    module: &Module<'static>,
    name: &str,
    args: &[BasicValueEnum<'static>],
) -> Result<(FunctionValue<'static>, BasicValueEnum<'static>), String> {
    let sig = crate::compile::externs::runtime_sig(name).ok_or_else(|| format!("`{}` is not a runtime function", name))?;
    let f = declare(module, name, sig);
    call_declared(builder, f, sig, args)
}

/// [`call`] of a function whose result is a word.
pub(crate) fn call_word(
    builder: &inkwell::builder::Builder<'static>,
    module: &Module<'static>,
    name: &str,
    args: &[BasicValueEnum<'static>],
) -> Result<(FunctionValue<'static>, inkwell::values::IntValue<'static>), String> {
    let (f, v) = call(builder, module, name, args)?;
    Ok((f, v.into_int_value()))
}

/// `v` as the parameter type `ty`, which it has unless one of the two is
/// wider: a word for a narrower integer, an integer for a wider one (the
/// callee reads the low bits its value has), an `f32` for a `double`, or a
/// float where the callee takes any value's bits.
fn argument(
    b: &inkwell::builder::Builder<'static>,
    v: BasicValueEnum<'static>,
    ty: RtTy,
) -> Result<BasicValueEnum<'static>, String> {
    let ctx = crate::compile::llvm_context();
    let e = |e: inkwell::builder::BuilderError| e.to_string();
    let to = ty.llvm();
    if v.get_type() == to {
        return Ok(v);
    }
    Ok(match (v, to) {
        (BasicValueEnum::IntValue(w), BasicTypeEnum::PointerType(p)) => b.build_int_to_ptr(w, p, "arg").map_err(e)?.into(),
        (BasicValueEnum::PointerValue(p), BasicTypeEnum::IntType(t)) => b.build_ptr_to_int(p, t, "arg").map_err(e)?.into(),
        (BasicValueEnum::IntValue(w), BasicTypeEnum::IntType(t)) if w.get_type().get_bit_width() > t.get_bit_width() => {
            b.build_int_truncate(w, t, "arg").map_err(e)?.into()
        }
        (BasicValueEnum::IntValue(w), BasicTypeEnum::IntType(t)) => b.build_int_z_extend(w, t, "arg").map_err(e)?.into(),
        (BasicValueEnum::IntValue(_), BasicTypeEnum::FloatType(t)) => {
            let c = if t == ctx.f32_type() { Carrier::F32 } else { Carrier::F64 };
            c.coerce(b, v)?
        }
        (BasicValueEnum::FloatValue(f), BasicTypeEnum::FloatType(t)) if t == ctx.f64_type() => b.build_float_ext(f, t, "arg").map_err(e)?.into(),
        (BasicValueEnum::FloatValue(_), BasicTypeEnum::IntType(_)) => crate::compile::carriers::to_word(b, v)?.into(),
        _ => return Err(format!("a {:?} parameter given a {}", ty, v.get_type().print_to_string())),
    })
}

/// [`call`] with the callee and its signature in hand.
pub(crate) fn call_declared(
    builder: &inkwell::builder::Builder<'static>,
    f: FunctionValue<'static>,
    sig: &Sig,
    args: &[BasicValueEnum<'static>],
) -> Result<(FunctionValue<'static>, BasicValueEnum<'static>), String> {
    let name = f.get_name().to_string_lossy().into_owned();
    if args.len() != sig.params.len() {
        return Err(format!("`{}` takes {} arguments, given {}", name, sig.params.len(), args.len()));
    }
    let err = |e: String| format!("{}: {}", name, e);
    let mut passed: Vec<inkwell::values::BasicMetadataValueEnum<'static>> = Vec::with_capacity(args.len());
    for (a, ty) in args.iter().zip(&sig.params) {
        passed.push(argument(builder, *a, *ty).map_err(err)?.into());
    }
    let call = builder.build_call(f, &passed, "rt_result").map_err(|e| err(e.to_string()))?;
    for (at, attr) in sig.extensions() {
        call.add_attribute(at, attr);
    }
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok((f, v)),
        inkwell::values::ValueKind::Instruction(_) => Err(err("answers no value".to_string())),
    }
}
