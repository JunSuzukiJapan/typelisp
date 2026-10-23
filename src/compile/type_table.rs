//! A module's type table: where compiled code reads the types it hands the
//! runtime from.
//!
//! A type's id is a number this process gave out (`typelisp_mem::TypeId`), so
//! code that leaves the process cannot carry one. Code generation therefore
//! does not write ids into the code. It reads each type out of a table of the
//! module's own ([`load`]), and the table is filled when the module's
//! destination is known:
//!
//! - linked into this process ([`settle_in_process`], which
//!   [`super::jit_engine`] runs): the table is a constant holding the ids, so
//!   the optimizer folds each read into the id itself;
//! - written out, as bitcode or as an executable ([`settle_for_export`]): the
//!   module carries the types' shapes (`typelisp_mem::type_words`) and a
//!   function that interns them into the table (`rt_types_define`), which
//!   whoever loads the module runs before any of its code
//!   ([`init_functions`]).
//!
//! Until then, each type the code reads is a declared global of its own whose
//! name holds the type's place in this process's table. Settling replaces
//! them all, so a module that has been settled, or loaded from bitcode, has
//! none.

use inkwell::builder::Builder;
use inkwell::module::{Linkage, Module};
use inkwell::values::{GlobalValue, IntValue};
use inkwell::AddressSpace;

use typelisp_mem::type_words::{TypeWords, Word};
use typelisp_mem::TypeId;

/// The prefix of a type's global before its module's table is settled; the
/// rest of the name is the type's [`TypeId::index`].
const UNSETTLED: &str = "typelisp.type.";

/// The prefix of the function an exported module interns its types with; the
/// rest of the name is the module's, so two modules linked into one
/// executable do not define the same function.
const INIT: &str = "typelisp_types_init.";

/// Reads `ty`, as its [`TypeId::word`], out of `module`'s table.
pub fn load(builder: &Builder<'static>, module: &Module<'static>, ty: TypeId) -> Result<IntValue<'static>, String> {
    let i64_ty = super::llvm_context().i64_type();
    let name = format!("{}{}", UNSETTLED, ty.index());
    let slot = module.get_global(&name).unwrap_or_else(|| module.add_global(i64_ty, None, &name));
    Ok(builder
        .build_load(i64_ty, slot.as_pointer_value(), "type")
        .map_err(|e| format!("failed to load a type from the type table: {}", e))?
        .into_int_value())
}

/// The globals [`load`] declared in `module`, with their types, in the
/// module's order.
fn unsettled(module: &Module<'static>) -> Result<Vec<(GlobalValue<'static>, TypeId)>, String> {
    module
        .get_globals()
        .filter_map(|g| {
            let name = g.get_name().to_str().ok()?.strip_prefix(UNSETTLED)?.to_string();
            Some((g, name))
        })
        .map(|(g, index)| {
            let ty = index.parse::<u32>().ok().and_then(TypeId::from_index);
            ty.map(|ty| (g, ty)).ok_or_else(|| format!("internal error: `{}{}` names no type", UNSETTLED, index))
        })
        .collect()
}

/// Points every global in `slots` at its word of `table`, and removes it.
fn redirect(table: GlobalValue<'static>, len: u32, slots: Vec<(GlobalValue<'static>, usize)>) {
    let i64_ty = super::llvm_context().i64_type();
    let array_ty = i64_ty.array_type(len);
    for (slot, pos) in slots {
        // SAFETY: `pos` is below `len`, the table's length.
        let word = unsafe {
            table.as_pointer_value().const_gep(array_ty, &[i64_ty.const_zero(), i64_ty.const_int(pos as u64, false)])
        };
        slot.as_pointer_value().replace_all_uses_with(word);
        // SAFETY: every use was just replaced.
        unsafe { slot.delete() };
    }
}

/// Fills `module`'s table with this process's ids, for a module linked here.
pub fn settle_in_process(module: &Module<'static>) -> Result<(), String> {
    let slots = unsettled(module)?;
    if slots.is_empty() {
        return Ok(());
    }
    let i64_ty = super::llvm_context().i64_type();
    let len = slots.len() as u32;
    let words: Vec<IntValue<'static>> = slots.iter().map(|(_, ty)| i64_ty.const_int(ty.word() as u64, true)).collect();
    let table = module.add_global(i64_ty.array_type(len), None, "typelisp.types");
    table.set_initializer(&i64_ty.const_array(&words));
    table.set_constant(true);
    table.set_linkage(Linkage::Private);
    redirect(table, len, slots.into_iter().enumerate().map(|(pos, (g, _))| (g, pos)).collect());
    Ok(())
}

/// Gives `module` the shapes of the types its code reads and the function
/// that interns them, for a module that leaves this process.
pub fn settle_for_export(module: &Module<'static>) -> Result<(), String> {
    let slots = unsettled(module)?;
    if slots.is_empty() {
        return Ok(());
    }
    let ctx = super::llvm_context();
    let i64_ty = ctx.i64_type();
    let mut types = TypeWords::default();
    let slots: Vec<(GlobalValue<'static>, usize)> = slots.into_iter().map(|(g, ty)| (g, types.add(ty))).collect();

    let (shapes, nwords) = constant_words(module, "typelisp.types.shapes", types.words());
    let len = types.len() as u32;
    let table = module.add_global(i64_ty.array_type(len), None, "typelisp.types");
    table.set_initializer(&i64_ty.array_type(len).const_zero());
    table.set_linkage(Linkage::Private);
    redirect(table, len, slots);

    let module_name = module.get_name().to_str().map_err(|e| format!("a module's name is not UTF-8: {}", e))?.to_string();
    let init = module.add_function(&format!("{}{}", INIT, module_name), ctx.void_type().fn_type(&[], false), None);
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(init, "entry"));
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let define = match module.get_function("rt_types_define") {
        Some(f) => f,
        None => module.add_function("rt_types_define", i64_ty.fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false), None),
    };
    let args = builder.build_alloca(i64_ty.array_type(3), "args").map_err(|e| e.to_string())?;
    let arg_words = [
        shapes.as_pointer_value().const_to_int(i64_ty),
        i64_ty.const_int(nwords as u64, false),
        table.as_pointer_value().const_to_int(i64_ty),
    ];
    for (i, w) in arg_words.into_iter().enumerate() {
        // SAFETY: `i` is below the array's length, 3.
        let slot = unsafe { builder.build_gep(i64_ty, args, &[i64_ty.const_int(i as u64, false)], "arg") }.map_err(|e| e.to_string())?;
        builder.build_store(slot, w).map_err(|e| e.to_string())?;
    }
    builder
        .build_call(define, &[args.into(), ctx.i32_type().const_int(3, false).into()], "")
        .map_err(|e| e.to_string())?;
    builder.build_return(None).map_err(|e| e.to_string())?;
    Ok(())
}

/// `words` as a constant array of `module` named `name`, each string written
/// as the address of a constant of its own and its length; with the array's
/// length in words.
pub fn constant_words(module: &Module<'static>, name: &str, words: &[Word]) -> (GlobalValue<'static>, usize) {
    let ctx = super::llvm_context();
    let i64_ty = ctx.i64_type();
    let mut strings = std::collections::HashMap::new();
    let mut placed = Vec::new();
    for w in words {
        match w {
            Word::Int(n) => placed.push(i64_ty.const_int(*n as u64, true)),
            Word::Str(text) => {
                let address = *strings.entry(*text).or_insert_with(|| {
                    let g = module.add_global(ctx.i8_type().array_type(text.len() as u32), None, &format!("{}.string", name));
                    g.set_initializer(&ctx.const_string(text.as_bytes(), false));
                    g.set_constant(true);
                    g.set_linkage(Linkage::Private);
                    g.as_pointer_value().const_to_int(i64_ty)
                });
                placed.push(address);
                placed.push(i64_ty.const_int(text.len() as u64, false));
            }
        }
    }
    let array = module.add_global(i64_ty.array_type(placed.len() as u32), None, name);
    array.set_initializer(&i64_ty.const_array(&placed));
    array.set_constant(true);
    array.set_linkage(Linkage::Private);
    (array, placed.len())
}

/// The functions that intern the types of the exported modules `module` was
/// made from — one per module, run before any code of `module` runs.
pub fn init_functions(module: &Module<'static>) -> Vec<String> {
    module
        .get_functions()
        .filter(|f| f.count_basic_blocks() > 0)
        .filter_map(|f| f.get_name().to_str().ok().filter(|n| n.starts_with(INIT)).map(str::to_string))
        .collect()
}

/// `module` as bitcode, its table settled for the process that loads it —
/// the one way a module is written out as bitcode.
pub fn write_bitcode(module: &Module<'static>) -> Result<Vec<u8>, String> {
    settle_for_export(module)?;
    // `as_slice` deliberately includes LLVM's guaranteed trailing NUL
    // (inkwell's `MemoryBuffer::get_size` is `LLVMGetBufferSize() + 1`), so
    // the artifact is one byte longer than the bitstream. **Do not trim it**:
    // the loader is `MemoryBuffer::create_from_memory_range_copy`
    // (`driver::install_compiled_library`), which asserts that last byte is
    // NUL and passes `len - 1` as the real size. Trimming would not even fail
    // the assert — the bitstream's own last byte is padding zero — it would
    // hand LLVM a buffer one byte short instead.
    //
    // The visible cost is that `llvm-dis`/`llvm-bcanalyzer` reject the file as
    // written ("Bitcode stream should be a multiple of 4 bytes in length"; the
    // artifact is 4n+1). To inspect one, drop the last byte into a scratch
    // copy.
    Ok(module.write_bitcode_to_memory().as_slice().to_vec())
}
