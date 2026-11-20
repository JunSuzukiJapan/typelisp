//! What the executable can run that it may not: refusing, at compile time, a
//! program that reaches a runtime shim it is not allowed to use —
//! `typl -c --no-cons` the cons arena, `--target wasm32-wasip1` what
//! WebAssembly has not got.
//!
//! The question is asked of the executable's whole module — the program, the
//! prelude bodies linked into it, and the `main` that starts them — because
//! that is where every function the executable can run is. Whatever `main`
//! refers to can run, and so can whatever those refer to: a direct call, and
//! equally a function whose address is an operand (a Lisp call is a driver
//! round trip that hands the callee over as a word, a closure carries its
//! code, a vtable or `print-object` registration names its method). The walk
//! follows every such reference, through constant expressions and global
//! initialisers too. What it reaches that is only declared is a runtime shim
//! or a C function, and the caller says which of those are refused.

use std::collections::{HashMap, VecDeque};

use inkwell::llvm_sys::core::{
    LLVMGetInitializer, LLVMGetNumOperands, LLVMGetOperand, LLVMGetValueName2, LLVMIsAConstant, LLVMIsAFunction,
    LLVMIsAGlobalVariable,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::module::Module;
use inkwell::values::{AsValueRef, FunctionValue};

/// One refused function the program reaches: the functions from `main` to
/// it, by their symbols, and what it is, as the error says it.
pub(super) struct Use {
    pub chain: Vec<String>,
    pub what: String,
}

/// Every function `entry` reaches in `module` that `refused` names (by
/// answering what it is), each with the first chain of references
/// (shortest, breadth first) that reaches it. Empty when the program reaches
/// none.
pub(super) fn uses(module: &Module<'static>, entry: FunctionValue<'static>, refused: impl Fn(&str) -> Option<String>) -> Vec<Use> {
    // Each function reached, and the one that reached it first.
    let mut parent: HashMap<LLVMValueRef, Option<LLVMValueRef>> = HashMap::new();
    let mut queue = VecDeque::new();
    let start = entry.as_value_ref();
    parent.insert(start, None);
    queue.push_back(start);
    let mut found = Vec::new();
    while let Some(func) = queue.pop_front() {
        let name = value_name(func);
        if let Some(what) = refused(&name) {
            found.push(Use { chain: chain_to(&parent, func), what });
            continue;
        }
        let Some(f) = module.get_function(&name) else { continue };
        let mut referenced = Vec::new();
        for block in f.get_basic_blocks() {
            let mut instr = block.get_first_instruction();
            while let Some(i) = instr {
                let v = i.as_value_ref();
                for k in 0..unsafe { LLVMGetNumOperands(v) } {
                    let operand = unsafe { LLVMGetOperand(v, k as u32) };
                    functions_in(operand, &mut referenced, &mut Vec::new());
                }
                instr = i.get_next_instruction();
            }
        }
        for callee in referenced {
            if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(callee) {
                e.insert(Some(func));
                queue.push_back(callee);
            }
        }
    }
    found
}

/// The functions `v` names, looking through constant expressions and into
/// the initialisers of the globals it names. `seen` is the globals and
/// constants already looked through, so a self-referencing initialiser ends.
fn functions_in(v: LLVMValueRef, out: &mut Vec<LLVMValueRef>, seen: &mut Vec<LLVMValueRef>) {
    if v.is_null() {
        return;
    }
    unsafe {
        if !LLVMIsAFunction(v).is_null() {
            out.push(v);
            return;
        }
        // An instruction operand that is itself an instruction, an argument
        // or a block is walked where it is defined, if at all.
        if LLVMIsAConstant(v).is_null() || seen.contains(&v) {
            return;
        }
        seen.push(v);
        if !LLVMIsAGlobalVariable(v).is_null() {
            functions_in(LLVMGetInitializer(v), out, seen);
            return;
        }
        for k in 0..LLVMGetNumOperands(v) {
            functions_in(LLVMGetOperand(v, k as u32), out, seen);
        }
    }
}

fn chain_to(parent: &HashMap<LLVMValueRef, Option<LLVMValueRef>>, mut at: LLVMValueRef) -> Vec<String> {
    let mut chain = vec![value_name(at)];
    while let Some(Some(p)) = parent.get(&at) {
        chain.push(value_name(*p));
        at = *p;
    }
    chain.reverse();
    chain
}

fn value_name(v: LLVMValueRef) -> String {
    let mut len = 0;
    let ptr = unsafe { LLVMGetValueName2(v, &mut len) };
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// The error compiling stops with: `header`, then each use, its chain
/// spelled with the program's own names.
pub(super) fn report(header: &str, uses: &[Use]) -> String {
    let mut msg = String::from(header);
    for u in uses {
        let chain: Vec<&str> = u
            .chain
            .iter()
            .take(u.chain.len() - 1)
            .map(|s| typelisp_mem::ConcreteNames::link_name_of(s).unwrap_or(s))
            .collect();
        msg.push_str(&format!("\n  {} (reached by {})", u.what, chain.join(" -> ")));
    }
    msg
}
