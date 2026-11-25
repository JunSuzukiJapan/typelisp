//! What a target with no runtime (an AVR) has in its place: the runtime
//! functions that need no heap, written in LLVM's IR, which the program is
//! linked with.
//!
//! An `int` here is a fixnum, and nothing else: there is nowhere to put a
//! larger one. An operation whose answer is none, like any other error
//! there is nothing to raise, stops the program ([`super::avr::FAULT`]).
//!
//! A word is 16 bits, and a fixnum's is its value shifted left by one.

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
define i16 @rt_int_not_fixnum(i16 %x) noreturn { call void @__typelisp_stop() unreachable }
define i16 @rt_largest_fixnum() { ret i16 16383 }

define i32 @bare_value(i16 %x) alwaysinline {
  %v = ashr i16 %x, 1
  %w = sext i16 %v to i32
  ret i32 %w
}

define i16 @bare_fixnum(i32 %n) {
  %m = add i32 %n, 16384
  %fits = icmp ult i32 %m, 32768
  br i1 %fits, label %yes, label %no
yes:
  %t = trunc i32 %n to i16
  %x = shl i16 %t, 1
  ret i16 %x
no:
  call void @__typelisp_stop()
  unreachable
}

define void @bare_check(i1 %ok) alwaysinline {
  br i1 %ok, label %yes, label %no
yes:
  ret void
no:
  call void @__typelisp_stop()
  unreachable
}

define i16 @rt_integer_div(i16 %a, i16 %b) {
  %z = icmp ne i16 %b, 0
  call void @bare_check(i1 %z)
  %x = call i32 @bare_value(i16 %a)
  %y = call i32 @bare_value(i16 %b)
  %q = sdiv i32 %x, %y
  %r = call i16 @bare_fixnum(i32 %q)
  ret i16 %r
}

define i16 @rt_integer_mod(i16 %a, i16 %b) {
  %z = icmp ne i16 %b, 0
  call void @bare_check(i1 %z)
  %x = call i32 @bare_value(i16 %a)
  %y = call i32 @bare_value(i16 %b)
  %r = srem i32 %x, %y
  %some = icmp ne i32 %r, 0
  %s = xor i32 %r, %y
  %differ = icmp slt i32 %s, 0
  %up = and i1 %some, %differ
  %f = add i32 %r, %y
  %m = select i1 %up, i32 %f, i32 %r
  %v = call i16 @bare_fixnum(i32 %m)
  ret i16 %v
}

define i16 @rt_integer_cmp(i16 %a, i16 %b) {
  %g = icmp sgt i16 %a, %b
  %l = icmp slt i16 %a, %b
  %gw = zext i1 %g to i16
  %lw = zext i1 %l to i16
  %r = sub i16 %gw, %lw
  ret i16 %r
}

define zeroext i1 @rt_integer_logtest(i16 %a, i16 %b) {
  %x = and i16 %a, %b
  %r = icmp ne i16 %x, 0
  ret i1 %r
}

define i16 @rt_integer_ash(i16 %n, i16 %count) {
  %v = call i32 @bare_value(i16 %n)
  %left = icmp sgt i16 %count, 0
  br i1 %left, label %up, label %down
up:
  %zero = icmp eq i16 %n, 0
  br i1 %zero, label %nothing, label %some
nothing:
  ret i16 0
some:
  %few = icmp slt i16 %count, 15
  call void @bare_check(i1 %few)
  %c = zext i16 %count to i32
  %s = shl i32 %v, %c
  %r = call i16 @bare_fixnum(i32 %s)
  ret i16 %r
down:
  %far = icmp slt i16 %count, -15
  %neg = sub i16 0, %count
  %by = select i1 %far, i16 15, i16 %neg
  %d = zext i16 %by to i32
  %t = ashr i32 %v, %d
  %q = call i16 @bare_fixnum(i32 %t)
  ret i16 %q
}

define zeroext i1 @rt_integer_logbitp(i16 %n, i16 %pos) {
  %v = ashr i16 %n, 1
  %out = icmp ugt i16 %pos, 15
  %at = select i1 %out, i16 15, i16 %pos
  %s = lshr i16 %v, %at
  %r = trunc i16 %s to i1
  ret i1 %r
}

define i16 @rt_integer_lognot(i16 %x) {
  %r = xor i16 %x, -2
  ret i16 %r
}

define i16 @bare_magnitude(i16 %x) {
  %v = ashr i16 %x, 1
  %s = ashr i16 %v, 15
  %m = xor i16 %v, %s
  ret i16 %m
}

define i16 @rt_integer_logcount(i16 %x) {
  %m = call i16 @bare_magnitude(i16 %x)
  br label %loop
loop:
  %rest = phi i16 [ %m, %0 ], [ %next, %loop ]
  %n = phi i16 [ 0, %0 ], [ %sum, %loop ]
  %bit = and i16 %rest, 1
  %sum = add i16 %n, %bit
  %next = lshr i16 %rest, 1
  %done = icmp eq i16 %next, 0
  br i1 %done, label %end, label %loop
end:
  %r = shl i16 %sum, 1
  ret i16 %r
}

define i16 @rt_integer_integer_length(i16 %x) {
  %m = call i16 @bare_magnitude(i16 %x)
  br label %loop
loop:
  %rest = phi i16 [ %m, %0 ], [ %next, %more ]
  %n = phi i16 [ 0, %0 ], [ %sum, %more ]
  %done = icmp eq i16 %rest, 0
  br i1 %done, label %end, label %more
more:
  %sum = add i16 %n, 1
  %next = lshr i16 %rest, 1
  br label %loop
end:
  %r = shl i16 %n, 1
  ret i16 %r
}

define double @rt_integer_to_float(i16 %x) {
  %v = call i32 @bare_value(i16 %x)
  %f = sitofp i32 %v to double
  ret double %f
}

define i16 @rt_float_to_int(double %f) {
  %low = fcmp ogt double %f, -16385.0
  %high = fcmp olt double %f, 16384.0
  %ok = and i1 %low, %high
  call void @bare_check(i1 %ok)
  %n = fptosi double %f to i32
  %r = call i16 @bare_fixnum(i32 %n)
  ret i16 %r
}

define zeroext i1 @rt_f64_fits_f32(double %v) {
  %n = fptrunc double %v to float
  %w = fpext float %n to double
  %x = bitcast double %v to i64
  %y = bitcast double %w to i64
  %r = icmp eq i64 %x, %y
  ret i1 %r
}

define i32 @rt_integer_to_char(i16 %x) {
  %ok = call zeroext i1 @rt_integer_fits_char(i16 %x)
  call void @bare_check(i1 %ok)
  %v = call i32 @bare_value(i16 %x)
  ret i32 %v
}

define zeroext i1 @rt_integer_fits_char(i16 %x) {
  %r = icmp sge i16 %x, 0
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

define i16 @rt_integer_from_word(i64 %x) {
  %m = add i64 %x, 16384
  %fits = icmp ult i64 %m, 32768
  call void @bare_check(i1 %fits)
  %t = trunc i64 %x to i16
  %r = shl i16 %t, 1
  ret i16 %r
}

define i16 @rt_integer_from_c_ulong(i64 %x) {
  %fits = icmp ult i64 %x, 16384
  call void @bare_check(i1 %fits)
  %t = trunc i64 %x to i16
  %r = shl i16 %t, 1
  ret i16 %r
}

define void @bare_stack_check() {
  %at = load volatile i16, ptr inttoptr (i16 93 to ptr), align 1
  %end = ptrtoint ptr @__bss_end to i16
  %room = sub i16 %at, %end
  %ok = icmp sgt i16 %room, STACK_ROOM
  call void @bare_check(i1 %ok)
  ret void
}

define i16 @rt_c_address(i16 %n) {
  %ok = icmp sge i16 %n, 0
  call void @bare_check(i1 %ok)
  %at = lshr i16 %n, 1
  ret i16 %at
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

/// `rt_integer_NAME`: the instruction `OP` on two `int`s.
const BINARY: &str = "
define i16 @rt_integer_NAME(i16 %a, i16 %b) {
  %x = call i32 @bare_value(i16 %a)
  %y = call i32 @bare_value(i16 %b)
  %v = OP i32 %x, %y
  %r = call i16 @bare_fixnum(i32 %v)
  ret i16 %r
}
";

/// `rt_integer_to_NAME`: the low bits of an `int`, as the type `TY`.
const TO_FIXED: &str = "
define EXT TY @rt_integer_to_NAME(i16 %x) {
  %v = ashr i16 %x, 1
  %w = sext i16 %v to i64
  %r = trunc i64 %w to TY
  ret TY %r
}
";

/// `rt_integer_fits_NAME`: whether an `int` is at least `LOW` and less than
/// `HIGH`.
const FITS_FIXED: &str = "
define zeroext i1 @rt_integer_fits_NAME(i16 %x) {
  %v = call i32 @bare_value(i16 %x)
  %l = icmp sge i32 %v, LOW
  %h = icmp sle i32 %v, HIGH
  %r = and i1 %l, %h
  ret i1 %r
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
    for (name, op) in [("add", "add"), ("sub", "sub"), ("mul", "mul"), ("logand", "and"), ("logior", "or"), ("logxor", "xor")] {
        text.push_str(&BINARY.replace("NAME", name).replace("OP", op));
    }
    for (name, ty, ext) in [
        ("i8", "i8", "signext"),
        ("i16", "i16", "signext"),
        ("i32", "i32", ""),
        ("u8", "i8", "zeroext"),
        ("u16", "i16", "zeroext"),
        ("u32", "i32", ""),
    ] {
        text.push_str(&TO_FIXED.replace("NAME", name).replace("TY", ty).replace("EXT", ext));
    }
    for name in ["c_long", "c_ulong"] {
        text.push_str(&format!(
            "define i64 @rt_integer_to_{}(i16 %x) {{\n  %v = ashr i16 %x, 1\n  %w = sext i16 %v to i64\n  ret i64 %w\n}}\n",
            name
        ));
    }
    for (name, low, high) in [
        ("i8", "-128", "127"),
        ("i16", "-32768", "32767"),
        ("i32", "-2147483648", "2147483647"),
        ("u8", "0", "255"),
        ("u16", "0", "65535"),
        ("u32", "0", "2147483647"),
    ] {
        text.push_str(&FITS_FIXED.replace("NAME", name).replace("LOW", low).replace("HIGH", high));
    }
    // A buffer of text ends with a NUL.
    text.push('\0');
    let buffer = MemoryBuffer::create_from_memory_range_copy(text.as_bytes(), "bare");
    let bare = ctx.create_module_from_ir(buffer).map_err(|e| format!("internal error: the runtime's replacement: {}", e))?;
    bare.set_triple(&module.get_triple());
    bare.set_data_layout(&module.get_data_layout());
    module.link_in_module(bare).map_err(|e| format!("internal error: the runtime's replacement: {}", e))
}
