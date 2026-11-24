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

/// What [`reached`] found.
pub(super) struct Reached {
    /// The names of the functions reached, defined or only declared (a shim).
    pub functions: std::collections::HashSet<String>,
    /// The globals the code reached refers to.
    pub globals: std::collections::HashSet<LLVMValueRef>,
}

/// Everything `roots` reach in `module`, walked as [`uses`] walks, except
/// that the global `opaque` is not looked into.
pub(super) fn reached(module: &Module<'static>, roots: &[FunctionValue<'static>], opaque: Option<LLVMValueRef>) -> Reached {
    let mut seen_fns: std::collections::HashSet<LLVMValueRef> = roots.iter().map(|f| f.as_value_ref()).collect();
    let mut queue: VecDeque<LLVMValueRef> = seen_fns.iter().copied().collect();
    let mut found = Reached { functions: Default::default(), globals: Default::default() };
    while let Some(func) = queue.pop_front() {
        let name = value_name(func);
        let Some(f) = module.get_function(&name) else { continue };
        found.functions.insert(name);
        let mut referenced = Vec::new();
        for block in f.get_basic_blocks() {
            let mut instr = block.get_first_instruction();
            while let Some(i) = instr {
                let v = i.as_value_ref();
                for k in 0..unsafe { LLVMGetNumOperands(v) } {
                    let operand = unsafe { LLVMGetOperand(v, k as u32) };
                    let mut seen: Vec<LLVMValueRef> = opaque.into_iter().collect();
                    functions_in(operand, &mut referenced, &mut seen);
                    found.globals.extend(seen.into_iter().filter(|g| Some(*g) != opaque && unsafe { !LLVMIsAGlobalVariable(*g).is_null() }));
                }
                instr = i.get_next_instruction();
            }
        }
        for callee in referenced {
            if seen_fns.insert(callee) {
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

/// The types the code a walk has reached reads out of a type table — the
/// types whose values that code can make, since making a value hands the
/// runtime its type.
///
/// Walked like [`uses`]: from a function to everything it refers to, through
/// constants and global initialisers. Two kinds of function are not walked
/// into. One only declared has no code of its own. A module's type-table
/// init function ([`crate::compile::type_table::is_init_function`]) reads
/// every type of its table, which says nothing about which of them the code
/// makes.
pub(super) struct TypeReach {
    /// Each exported table ([`crate::compile::type_table::exported_tables`])
    /// with the type at each of its places.
    tables: HashMap<LLVMValueRef, Vec<typelisp_mem::TypeId>>,
    /// Every function walked, or queued to be.
    reached: std::collections::HashSet<LLVMValueRef>,
    /// Every constant looked through, so each is looked through once.
    seen: std::collections::HashSet<LLVMValueRef>,
    pub types: std::collections::HashSet<typelisp_mem::TypeId>,
}

impl TypeReach {
    pub fn new(module: &Module<'static>) -> Result<Self, String> {
        let tables = crate::compile::type_table::exported_tables(module)?
            .into_iter()
            .map(|(table, types)| (table.as_value_ref(), types))
            .collect();
        Ok(TypeReach { tables, reached: Default::default(), seen: Default::default(), types: Default::default() })
    }

    /// Whether the walk has reached the function `name`, defined in the
    /// module or only declared (a shim).
    pub fn reaches(&self, module: &Module<'static>, name: &str) -> bool {
        module.get_function(name).is_some_and(|f| self.reached.contains(&f.as_value_ref()))
    }

    /// Walks from `from`, adding what it reaches to what the walk has.
    pub fn walk(&mut self, module: &Module<'static>, from: FunctionValue<'static>) -> Result<(), String> {
        let mut queue = VecDeque::new();
        if self.reached.insert(from.as_value_ref()) {
            queue.push_back(from.as_value_ref());
        }
        while let Some(func) = queue.pop_front() {
            let name = value_name(func);
            if crate::compile::type_table::is_init_function(&name) {
                continue;
            }
            let Some(f) = module.get_function(&name) else { continue };
            for block in f.get_basic_blocks() {
                let mut instr = block.get_first_instruction();
                while let Some(i) = instr {
                    let v = i.as_value_ref();
                    for k in 0..unsafe { LLVMGetNumOperands(v) } {
                        self.operand(unsafe { LLVMGetOperand(v, k as u32) }, &mut queue)?;
                    }
                    instr = i.get_next_instruction();
                }
            }
        }
        Ok(())
    }

    fn operand(&mut self, v: LLVMValueRef, queue: &mut VecDeque<LLVMValueRef>) -> Result<(), String> {
        use inkwell::llvm_sys::core::{LLVMConstIntGetSExtValue, LLVMGetConstOpcode, LLVMGetGEPSourceElementType, LLVMIsAConstantExpr, LLVMIsAConstantInt};
        use inkwell::llvm_sys::LLVMOpcode;
        if v.is_null() {
            return Ok(());
        }
        unsafe {
            if !LLVMIsAFunction(v).is_null() {
                if self.reached.insert(v) {
                    queue.push_back(v);
                }
                return Ok(());
            }
            if LLVMIsAConstant(v).is_null() || !self.seen.insert(v) {
                return Ok(());
            }
            if !LLVMIsAGlobalVariable(v).is_null() {
                if let Some(ty) = crate::compile::type_table::unsettled_type(&value_name(v)) {
                    self.types.insert(ty);
                } else if let Some(types) = self.tables.get(&v) {
                    // The table itself is the address of its first place.
                    self.types.insert(types[0]);
                } else {
                    self.operand(LLVMGetInitializer(v), queue)?;
                }
                return Ok(());
            }
            if !LLVMIsAConstantExpr(v).is_null() && LLVMGetConstOpcode(v) == LLVMOpcode::LLVMGetElementPtr {
                if let Some(types) = self.tables.get(&LLVMGetOperand(v, 0)) {
                    // `type_table::redirect`'s `[N x i64]` with `0, place`, or
                    // the same address as the optimizer may write it: in
                    // words, or in bytes.
                    let indices: Vec<i64> = (1..LLVMGetNumOperands(v))
                        .map(|k| {
                            let index = LLVMGetOperand(v, k as u32);
                            (!LLVMIsAConstantInt(index).is_null()).then(|| LLVMConstIntGetSExtValue(index))
                        })
                        .collect::<Option<_>>()
                        .ok_or_else(|| format!("internal error: a type table is indexed by a non-constant in `{}`", value_name(v)))?;
                    let source = LLVMGetGEPSourceElementType(v);
                    let place = match (inkwell::llvm_sys::core::LLVMGetTypeKind(source), &indices[..]) {
                        (inkwell::llvm_sys::LLVMTypeKind::LLVMArrayTypeKind, [0, place]) => *place,
                        (inkwell::llvm_sys::LLVMTypeKind::LLVMIntegerTypeKind, [offset]) => {
                            match inkwell::llvm_sys::core::LLVMGetIntTypeWidth(source) {
                                64 => *offset,
                                8 if offset % 8 == 0 => offset / 8,
                                width => return Err(format!("internal error: a type table is addressed in {}-bit units", width)),
                            }
                        }
                        _ => return Err("internal error: a type table is addressed in a shape this walk does not read".to_string()),
                    };
                    let ty = usize::try_from(place)
                        .ok()
                        .and_then(|p| types.get(p))
                        .ok_or_else(|| format!("internal error: place {} is outside its type table", place))?;
                    self.types.insert(*ty);
                    return Ok(());
                }
            }
            for k in 0..LLVMGetNumOperands(v) {
                self.operand(LLVMGetOperand(v, k as u32), queue)?;
            }
        }
        Ok(())
    }
}
