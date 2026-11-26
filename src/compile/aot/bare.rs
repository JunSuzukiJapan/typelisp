//! What a target with no runtime (an AVR) has in its place: the runtime
//! functions that need no heap, written in LLVM's IR, which the program is
//! linked with.
//!
//! There is no `int`: its values have no width, and nowhere to put a large
//! one. An error, with nothing to raise, stops the program
//! ([`super::avr::FAULT`]).

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use inkwell::values::InstructionOpcode;

use typelisp_rt::c_mem;

use super::masking::reached;
use super::settle::{callee, instructions, name_of};

/// The functions whose bodies a macro below does not make.
const BODIES: &str = "
declare void @__typelisp_stop() noreturn
@__bss_end = external global i8

define i16 @rt_int_div_fault(i64 %a, i64 %b) noreturn { call void @__typelisp_stop() unreachable }
define i16 @rt_int_mod_fault(i64 %a, i64 %b) noreturn { call void @__typelisp_stop() unreachable }
define void @bare_check(i1 %ok) alwaysinline {
  br i1 %ok, label %yes, label %no
yes:
  ret void
no:
  call void @__typelisp_stop()
  unreachable
}

define zeroext i1 @rt_f64_fits_f32(double %v) {
  %n = fptrunc double %v to float
  %w = fpext float %n to double
  %x = bitcast double %v to i64
  %y = bitcast double %w to i64
  %r = icmp eq i64 %x, %y
  ret i1 %r
}

define i32 @rt_int_to_char(i64 %n) {
  %ok = call zeroext i1 @rt_int_fits_char(i64 %n)
  call void @bare_check(i1 %ok)
  %c = trunc i64 %n to i32
  ret i32 %c
}

define zeroext i1 @rt_int_fits_char(i64 %n) {
  %unicode = icmp ult i64 %n, 1114112
  %c = trunc i64 %n to i32
  %from = sub i32 %c, 55296
  %plain = icmp uge i32 %from, 2048
  %r = and i1 %unicode, %plain
  ret i1 %r
}

define i64 @rt_float_to_bits(double %f) {
entry:
  %bits = bitcast double %f to i64
  %high = lshr i64 %bits, 52
  %field = and i64 %high, 2047
  %finite = icmp ne i64 %field, 2047
  call void @bare_check(i1 %finite)
  %fraction = and i64 %bits, 4503599627370495
  %normal = icmp ne i64 %field, 0
  %lead = select i1 %normal, i64 4503599627370496, i64 0
  %m = or i64 %fraction, %lead
  %e = trunc i64 %field to i16
  %shift = sub i16 %e, 1075
  %far = icmp sge i16 %shift, 64
  br i1 %far, label %end, label %loop
loop:
  %v = phi i64 [ %m, %entry ], [ %v1, %more ]
  %k = phi i16 [ %shift, %entry ], [ %k1, %more ]
  %done = icmp eq i16 %k, 0
  br i1 %done, label %end, label %more
more:
  %right = icmp slt i16 %k, 0
  %r = lshr i64 %v, 1
  %l = shl i64 %v, 1
  %v1 = select i1 %right, i64 %r, i64 %l
  %up = add i16 %k, 1
  %down = sub i16 %k, 1
  %k1 = select i1 %right, i16 %up, i16 %down
  br label %loop
end:
  %magnitude = phi i64 [ 0, %entry ], [ %v, %loop ]
  %negative = icmp slt i64 %bits, 0
  %minus = sub i64 0, %magnitude
  %answer = select i1 %negative, i64 %minus, i64 %magnitude
  ret i64 %answer
}

define void @bare_stack_check() {
  %at = load volatile i16, ptr inttoptr (i16 93 to ptr), align 1
  %end = ptrtoint ptr @__bss_end to i16
  %room = sub i16 %at, %end
  %ok = icmp sgt i16 %room, STACK_ROOM
  call void @bare_check(i1 %ok)
  ret void
}

define i32 @rt_c_volatile_load(i16 %addr, i16 %kind) alwaysinline {
  switch i16 %kind, label %u32 [
    i16 U8, label %u8
    i16 U16, label %u16
  ]
u8:
  %p8 = inttoptr i16 %addr to ptr
  %v8 = load volatile i8, ptr %p8, align 1
  %w8 = zext i8 %v8 to i32
  ret i32 %w8
u16:
  %low16 = and i16 %addr, 1
  %ok16 = icmp eq i16 %low16, 0
  call void @bare_check(i1 %ok16)
  %p16 = inttoptr i16 %addr to ptr
  %v16 = load volatile i16, ptr %p16, align 2
  %w16 = zext i16 %v16 to i32
  ret i32 %w16
u32:
  %low32 = and i16 %addr, 3
  %ok32 = icmp eq i16 %low32, 0
  call void @bare_check(i1 %ok32)
  %p32 = inttoptr i16 %addr to ptr
  %v32 = load volatile i32, ptr %p32, align 4
  ret i32 %v32
}

define i16 @rt_c_volatile_store(i16 %addr, i16 %kind, i32 %word) alwaysinline {
  switch i16 %kind, label %u32 [
    i16 U8, label %u8
    i16 U16, label %u16
  ]
u8:
  %p8 = inttoptr i16 %addr to ptr
  %v8 = trunc i32 %word to i8
  store volatile i8 %v8, ptr %p8, align 1
  ret i16 0
u16:
  %low16 = and i16 %addr, 1
  %ok16 = icmp eq i16 %low16, 0
  call void @bare_check(i1 %ok16)
  %p16 = inttoptr i16 %addr to ptr
  %v16 = trunc i32 %word to i16
  store volatile i16 %v16, ptr %p16, align 2
  ret i16 0
u32:
  %low32 = and i16 %addr, 3
  %ok32 = icmp eq i16 %low32, 0
  call void @bare_check(i1 %ok32)
  %p32 = inttoptr i16 %addr to ptr
  store volatile i32 %word, ptr %p32, align 4
  ret i16 0
}
";

/// How many bytes of the stack a function has to leave: what the code
/// generator's helper functions and an interrupt's saved registers take.
const STACK_ROOM: u32 = 64;

/// Makes each of the functions `direct` of a program in which a function
/// calls itself, however indirectly, stop the program when it is out of stack.
pub(super) fn check_stack(module: &Module<'static>, direct: &[String]) -> Result<(), String> {
    let functions: Vec<_> = direct.iter().filter_map(|name| module.get_function(name)).filter(|f| f.count_basic_blocks() > 0).collect();
    let recursive = functions.iter().any(|f| {
        let callees: Vec<_> = instructions(*f).into_iter().filter_map(callee).collect();
        reached(&callees).contains(f)
    });
    if !recursive {
        return Ok(());
    }
    let ctx = crate::compile::llvm_context();
    let check = module.add_function("bare_stack_check", ctx.void_type().fn_type(&[], false), None);
    let b = ctx.create_builder();
    for f in functions {
        let first = instructions(f).into_iter().find(|i| i.get_opcode() != InstructionOpcode::Alloca).ok_or("internal error: an empty function")?;
        b.position_before(&first);
        b.build_call(check, &[], "").map_err(|e| format!("the stack check of `{}`: {}", name_of(f), e))?;
    }
    Ok(())
}

/// The functions linked into `module`, in place of the runtime's.
pub(super) fn link_into(ctx: &'static Context, module: &Module<'static>) -> Result<(), String> {
    let mut text = BODIES.replace("U8", &c_mem::KIND_U8.to_string()).replace("U16", &c_mem::KIND_U16.to_string()).replace("STACK_ROOM", &STACK_ROOM.to_string());
    // A buffer of text ends with a NUL.
    text.push('\0');
    let buffer = MemoryBuffer::create_from_memory_range_copy(text.as_bytes(), "bare");
    let bare = ctx.create_module_from_ir(buffer).map_err(|e| format!("internal error: the runtime's replacement: {}", e))?;
    bare.set_triple(&module.get_triple());
    bare.set_data_layout(&module.get_data_layout());
    module.link_in_module(bare).map_err(|e| format!("internal error: the runtime's replacement: {}", e))
}
