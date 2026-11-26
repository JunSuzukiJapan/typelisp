//! Accesses of the static globals that an interrupt handler must not come
//! between the bytes of, on a machine whose instructions access a byte (an
//! AVR): each is made with interrupts disabled.
//!
//! Which accesses those are is read off the settled functions. A handler
//! comes between what the program reads only where a handler writes it, and
//! between what the program writes where a handler reads or writes it. What
//! only runs in handlers, which do not interrupt one another, or before
//! interrupts are enabled, is left alone.

use std::collections::{BTreeMap, HashMap, HashSet};

use inkwell::module::Module;
use inkwell::values::{AsValueRef, BasicValueEnum, FunctionValue, InstructionOpcode, InstructionValue, Operand};

use super::settle::{callee, instructions, name_of};
use super::avr;
use crate::compile::carriers;

/// A global whose accesses disable interrupts.
pub(super) struct Masked {
    pub bytes: u32,
    pub read: bool,
    pub written: bool,
}

/// An access of a static global that can be assigned.
struct Access {
    variable: String,
    write: bool,
    bytes: u32,
    at: InstructionValue<'static>,
}

/// Disables interrupts around the accesses of the globals `variables` that the
/// program, entered at `entry`, makes and that one of `handlers` could come
/// between the bytes of. Answers the globals that have one, by variable.
pub(super) fn mask(
    module: &Module<'static>,
    variables: &[String],
    entry: FunctionValue<'static>,
    handlers: &[FunctionValue<'static>],
) -> Result<BTreeMap<String, Masked>, String> {
    let globals: HashMap<usize, &String> = variables
        .iter()
        .filter_map(|v| module.get_global(v).map(|g| (g.as_pointer_value().as_value_ref() as usize, v)))
        .collect();
    let (mut read, mut written): (HashSet<String>, HashSet<String>) = Default::default();
    for f in reached(handlers) {
        for access in accesses(f, &globals)? {
            if access.write { &mut written } else { &mut read }.insert(access.variable);
        }
    }
    let mut masked: BTreeMap<String, Masked> = BTreeMap::new();
    let b = crate::compile::llvm_context().create_builder();
    for f in reached(&[entry]) {
        for access in accesses(f, &globals)? {
            let between = written.contains(&access.variable) || access.write && read.contains(&access.variable);
            if access.bytes == 1 || !between {
                continue;
            }
            b.position_before(&access.at);
            let status = avr::status(&b)?;
            avr::set_status(&b, carriers::int(8).const_zero())?;
            match access.at.get_next_instruction() {
                Some(next) => b.position_before(&next),
                None => return Err(format!("internal error: `{}` ends with an access", name_of(f))),
            }
            avr::set_status(&b, status)?;
            let m = masked.entry(access.variable).or_insert(Masked { bytes: access.bytes, read: false, written: false });
            if access.write { m.written = true } else { m.read = true }
        }
    }
    Ok(masked)
}

/// `entered`, and every function they come to call.
pub(super) fn reached(entered: &[FunctionValue<'static>]) -> Vec<FunctionValue<'static>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut found = Vec::new();
    let mut pending = entered.to_vec();
    while let Some(f) = pending.pop() {
        if seen.insert(name_of(f)) {
            pending.extend(instructions(f).into_iter().filter_map(callee));
            found.push(f);
        }
    }
    found
}

/// The accesses of `globals` in `f`: volatile, as any of a global that can be
/// assigned is.
fn accesses(f: FunctionValue<'static>, globals: &HashMap<usize, &String>) -> Result<Vec<Access>, String> {
    let mut found = Vec::new();
    for at in instructions(f) {
        let write = match at.get_opcode() {
            InstructionOpcode::Load => false,
            InstructionOpcode::Store => true,
            _ => continue,
        };
        let Some(Operand::Value(BasicValueEnum::PointerValue(p))) = at.get_operand(u32::from(write)) else { continue };
        let Some(variable) = globals.get(&(p.as_value_ref() as usize)) else { continue };
        if !at.get_volatile().map_err(|e| format!("internal error: {:?}", e))? {
            continue;
        }
        let ty = match (write, at.get_operand(0)) {
            (true, Some(Operand::Value(v))) => v.get_type(),
            _ => inkwell::types::BasicTypeEnum::try_from(at.get_type()).map_err(|()| "internal error: a load answers a value".to_string())?,
        };
        found.push(Access { variable: variable.to_string(), write, bytes: carriers::bits_of(ty).div_ceil(8), at });
    }
    Ok(found)
}
