//! Where the code being built keeps a value between making it and using it.
//!
//! Three kinds of place, one interface (`slot-set`, `slot-get`):
//!
//! * the arguments of a call (`arg-list`): not storage at all. The values are
//!   recorded and the call takes them, so nothing may stop between the two;
//! * a temporary (`temp-slot`, `scratch-slot`): stack storage for a value
//!   that meets others at the end of an `if` or a `match`. The machine frame
//!   goes away when the function stops, so nothing may stop here either;
//! * a slot of the frame (`frame-slot`, `frame-slots`): what a local and a
//!   call's argument are kept in, and what is still there when the function
//!   is entered again.
//!
//! A value wider than a word takes as many slots of the frame as it is words.
//!
//! A place's type is the type of the first value put in it, and every other
//! value put there has that type. So the island says which value waits where
//! and whether the collector follows it, and nothing about how wide it is.
//!
//! A value of an expression that does not come back — a `panic`, a `throw`, a
//! `return` — is no value ([`NativeHandle::Never`]). Putting one in a place
//! does nothing, and reading a place nothing was put in answers one: the code
//! that would read it is never reached.

use std::cell::RefCell;
use std::rc::Rc;

use inkwell::builder::Builder;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, PointerValue};
use typelisp_mem::Value;

use super::coroutine::{builder_key, new_frame_slot, FRAME_CTXS};
use super::handles::{expect_handle, handle_value, NativeHandle};
use super::{entry_alloca, expect_int, expect_llvm_builder};
use crate::compile::carriers;
use crate::eval::interp::EvalError;

/// A value, or none where the code that makes it does not come back.
pub(crate) type Operand = Option<BasicValueEnum<'static>>;

/// Storage of one value, and the type it has.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    ptr: PointerValue<'static>,
    ty: BasicTypeEnum<'static>,
}

/// A slot of the frame: made when the first value is put in it.
#[derive(Clone, Copy)]
pub(crate) struct FrameSlot {
    /// Whether the collector follows what is in it.
    rooted: bool,
    place: Option<Place>,
}

pub(crate) enum Slots {
    Args(Vec<Operand>),
    Temp(Option<Place>),
    Frame(Vec<FrameSlot>),
}

type Shared = Rc<RefCell<Slots>>;

pub(super) fn slots_value(s: Slots) -> Value {
    handle_value(NativeHandle::Slots(Rc::new(RefCell::new(s))))
}

pub(super) fn expect_slots(v: &Value) -> Result<Shared, EvalError> {
    match expect_handle(v, "place")? {
        NativeHandle::Slots(s) => Ok(s),
        _ => Err(EvalError::Internal(format!("expected places for values, got {:?}", v))),
    }
}

/// The value `v` is a handle for, or `None` for no value.
pub(super) fn operand(v: &Value) -> Result<Operand, EvalError> {
    match expect_handle(v, "LlvmValue")? {
        NativeHandle::Value(x) => Ok(Some(x)),
        NativeHandle::Never => Ok(None),
        _ => Err(EvalError::Internal(format!("expected an LlvmValue, got {:?}", v))),
    }
}

pub(super) fn operand_value(v: Operand) -> Value {
    handle_value(v.map_or(NativeHandle::Never, NativeHandle::Value))
}

fn index(v: &Value) -> Result<usize, EvalError> {
    let n = expect_int(v)?;
    usize::try_from(n).map_err(|_| EvalError::Internal(format!("{} is not the number of a slot", n)))
}

/// `(no-value)` — the value of code that does not come back.
pub(super) fn no_value() -> Result<Value, EvalError> {
    Ok(operand_value(None))
}

/// `(arg-list builder n)` — the `n` arguments of a call about to be built.
pub(super) fn arg_list(args: &[Value]) -> Result<Value, EvalError> {
    Ok(slots_value(Slots::Args(vec![None; index(&args[1])?])))
}

/// `(temp-slot builder)` — a temporary.
pub(super) fn temp_slot() -> Result<Value, EvalError> {
    Ok(slots_value(Slots::Temp(None)))
}

/// `(scratch-slot builder role)` — the temporary this function uses for
/// `role`, the same one every time it is asked for.
///
/// A temporary of its own per request is storage of its own in the machine
/// frame, so a body with a merge per arithmetic operation would have a frame
/// as long as the body: `(+ 1 (+ 1 ... a))` a hundred thousand deep needed
/// 8.3MB. What the island asks for here instead is one per role, on its word
/// that no two uses of one role are ever in progress at once — true of a
/// merge, which is filled on each branch and read where they meet before
/// anything else asks for that role — and that every use is of one type.
///
/// Only inside a frame (`coroutine-begin` .. `coroutine-end`), which is what
/// keeps the answer to one function.
pub(super) fn scratch_slot(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let role = expect_int(&args[1])?;
    FRAME_CTXS.with(|c| {
        let mut c = c.borrow_mut();
        let f = c
            .get_mut(&builder_key(&builder))
            .ok_or_else(|| EvalError::Internal("scratch-slot: no frame is open on this builder".to_string()))?;
        let slot = f.scratch.entry(role).or_insert_with(|| Rc::new(RefCell::new(Slots::Temp(None))));
        Ok(handle_value(NativeHandle::Slots(slot.clone())))
    })
}

/// `(frame-slots builder)` — slots of the frame, none yet; `add-frame-slot`
/// adds them.
pub(super) fn frame_slots() -> Result<Value, EvalError> {
    Ok(slots_value(Slots::Frame(Vec::new())))
}

/// `(frame-slot builder)`, `(frame-slot-rooted builder)` — one slot of the
/// frame, the second followed by the collector.
pub(super) fn frame_slot(rooted: bool) -> Result<Value, EvalError> {
    Ok(slots_value(Slots::Frame(vec![FrameSlot { rooted, place: None }])))
}

/// `(add-frame-slot builder slots)`, `(add-frame-slot-rooted builder slots)`
/// — one more slot at the end of `slots`.
pub(super) fn add_frame_slot(args: &[Value], rooted: bool) -> Result<Value, EvalError> {
    match &mut *expect_slots(&args[1])?.borrow_mut() {
        Slots::Frame(all) => all.push(FrameSlot { rooted, place: None }),
        _ => return Err(EvalError::Internal("add-frame-slot: not the frame's slots".to_string())),
    }
    Ok(Value::Empty)
}

/// `(slot-set builder slots i v)` — puts `v` in slot `i`.
pub(super) fn slot_set(args: &[Value]) -> Result<Value, EvalError> {
    let slots = expect_slots(&args[1])?;
    let i = index(&args[2])?;
    let v = operand(&args[3])?;
    let builder = expect_llvm_builder(&args[0])?;
    put(&builder, &slots, i, v)?;
    Ok(Value::Empty)
}

pub(super) fn put(builder: &Rc<RefCell<Builder<'static>>>, slots: &Shared, i: usize, v: Operand) -> Result<(), EvalError> {
    let err = |e: String| EvalError::Internal(format!("slot-set: {}", e));
    let mut slots = slots.borrow_mut();
    let place = match (&mut *slots, v) {
        (Slots::Args(all), v) => {
            let n = all.len();
            *all.get_mut(i).ok_or_else(|| err(format!("argument {} of {}", i, n)))? = v;
            return Ok(());
        }
        (_, None) => return Ok(()),
        (Slots::Temp(place), Some(v)) => match place {
            Some(p) => *p,
            None => {
                let ty = v.get_type();
                let ptr = entry_alloca(&builder.borrow(), ty, "temp").map_err(err)?;
                *place.insert(Place { ptr, ty })
            }
        },
        (Slots::Frame(all), Some(v)) => {
            let n = all.len();
            let slot = all.get_mut(i).ok_or_else(|| err(format!("frame slot {} of {}", i, n)))?;
            match slot.place {
                Some(p) => p,
                None => {
                    let ty = v.get_type();
                    if slot.rooted && ty != carriers::word().as_basic_type_enum() {
                        return Err(err(format!("the collector cannot follow a {}", ty.print_to_string())));
                    }
                    let ptr = new_frame_slot(builder, slot.rooted, carriers::words_of(ty))?;
                    *slot.place.insert(Place { ptr, ty })
                }
            }
        }
    };
    let Some(v) = v else { return Ok(()) };
    if v.get_type() != place.ty {
        return Err(err(format!("a {} put where a {} is", v.get_type().print_to_string(), place.ty.print_to_string())));
    }
    carriers::store(&builder.borrow(), place.ptr, v).map_err(err)?;
    Ok(())
}

/// `(slot-get builder slots i)` — the value in slot `i`.
pub(super) fn slot_get(args: &[Value]) -> Result<Value, EvalError> {
    let slots = expect_slots(&args[1])?;
    let builder = expect_llvm_builder(&args[0])?;
    let i = index(&args[2])?;
    let v = get(&builder.borrow(), &slots.borrow(), i)?;
    Ok(operand_value(v))
}

fn get(b: &Builder<'static>, slots: &Slots, i: usize) -> Result<Operand, EvalError> {
    let err = |e: String| EvalError::Internal(format!("slot-get: {}", e));
    let place = match slots {
        Slots::Args(all) => return all.get(i).copied().ok_or_else(|| err(format!("argument {} of {}", i, all.len()))),
        Slots::Temp(place) => *place,
        Slots::Frame(all) => all.get(i).ok_or_else(|| err(format!("frame slot {} of {}", i, all.len())))?.place,
    };
    let Some(place) = place else { return Ok(None) };
    Ok(Some(carriers::load(b, place.ty, place.ptr, "slot").map_err(err)?))
}

/// Every value in `slots`, in order: what a call takes.
pub(super) fn all_of(b: &Builder<'static>, slots: &Shared) -> Result<Vec<Operand>, EvalError> {
    let slots = slots.borrow();
    let n = match &*slots {
        Slots::Args(all) => all.len(),
        Slots::Temp(_) => 1,
        Slots::Frame(all) => all.len(),
    };
    (0..n).map(|i| get(b, &slots, i)).collect()
}
