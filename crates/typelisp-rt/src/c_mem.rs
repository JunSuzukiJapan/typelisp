//! Memory for `def-c-struct`s and typed pointers: what `(c-alloc T n)`
//! allocates, the table of blocks that are still alive, and the loads and
//! stores a typed pointer's fields lower to.
//!
//! # Arenas
//!
//! Every allocation belongs to an **arena**, one per `(unsafe ...)` that
//! allocates (the outermost one in its function — the checker decides which).
//! The arena is opened when that form is entered and closed, freeing every
//! block in it, when the form is left by any path; `unwind-protect` is what
//! guarantees the second half.
//!
//! # The table of live blocks
//!
//! A typed pointer that comes *from C* — a callback's argument, a `defffi`
//! result, a pointer read out of a field — is only accepted if it points into
//! a live block, at a place that holds a value of the pointer's type. C's own
//! memory is refused: nothing here knows how long it lives or what it holds.
//!
//! The table is process-wide rather than per thread, because a task can be
//! resumed on another OS thread than the one that allocated.
//!
//! A block records, for its element type, every `(offset, type)` at which
//! a C object starts — the element itself at offset 0, each field, each field
//! of an embedded struct — so "is this a `point` or an `i32`?" is a lookup.
//!
//! # Words
//!
//! Every function here takes and returns the words compiled code holds: a
//! narrow integer sign- or zero-extended, an `f64` as its bits, an `f32`
//! widened to `f64` bits, a `bool` as 0/1, a pointer as its address. The
//! interpreter converts its values to and from those words around the call.

use std::alloc::Layout;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use typelisp_abi::{fatal, raise};
use typelisp_mem::TypeId;

/// What a load or store reads or writes, as the checker encodes it. One code
/// per C scalar type; a typed pointer is loaded and stored as [`KIND_PTR`].
pub const KIND_I8: i64 = 1;
pub const KIND_I16: i64 = 2;
pub const KIND_I32: i64 = 3;
pub const KIND_U8: i64 = 4;
pub const KIND_U16: i64 = 5;
pub const KIND_U32: i64 = 6;
pub const KIND_C_LONG: i64 = 7;
pub const KIND_C_ULONG: i64 = 8;
pub const KIND_F32: i64 = 9;
pub const KIND_F64: i64 = 10;
pub const KIND_BOOL: i64 = 11;
pub const KIND_PTR: i64 = 12;

struct Block {
    len: usize,
    elem_size: usize,
    layout: Layout,
    /// Every `(offset within an element, type)` a C object starts at.
    positions: Arc<Vec<(usize, TypeId)>>,
}

#[derive(Default)]
struct State {
    next_arena: u64,
    /// Live blocks by base address.
    blocks: BTreeMap<usize, Block>,
    /// Open arenas, and the bases of the blocks each owns.
    arenas: HashMap<u64, Vec<usize>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// Opens an arena and answers its id.
pub fn arena_open() -> u64 {
    with_state(|s| {
        s.next_arena += 1;
        s.arenas.insert(s.next_arena, Vec::new());
        s.next_arena
    })
}

/// Frees every block `arena` owns and forgets the arena. The checker opens
/// and closes each arena exactly once, so an id that is not open is a broken
/// invariant, not an empty arena.
pub fn arena_close(arena: u64) -> Result<(), String> {
    with_state(|s| {
        let bases = s.arenas.remove(&arena).ok_or_else(|| format!("c-arena-close: arena {} is not open", arena))?;
        for base in bases {
            let blk = s.blocks.remove(&base).ok_or_else(|| format!("c-arena-close: block {:#x} of arena {} is not live", base, arena))?;
            unsafe { std::alloc::dealloc(base as *mut u8, blk.layout) };
        }
        Ok(())
    })
}

/// Allocates `count` zeroed elements in `arena`, each `elem_size` bytes
/// aligned to `align`, with a C object of each type of `positions` at its
/// offset.
pub fn alloc(arena: u64, count: i64, elem_size: usize, align: usize, positions: Vec<(usize, TypeId)>) -> Result<usize, String> {
    if count < 1 {
        return Err(format!("c-alloc: the element count must be at least 1, got {}", count));
    }
    let len = elem_size
        .checked_mul(count as usize)
        .ok_or_else(|| format!("c-alloc: {} elements of {} bytes do not fit in memory", count, elem_size))?;
    let layout = Layout::from_size_align(len, align)
        .map_err(|_| format!("c-alloc: {} elements of {} bytes do not fit in memory", count, elem_size))?;
    let base = unsafe { std::alloc::alloc_zeroed(layout) } as usize;
    if base == 0 {
        return Err(format!("c-alloc: out of memory allocating {} bytes", len));
    }
    with_state(|s| {
        let Some(owned) = s.arenas.get_mut(&arena) else {
            unsafe { std::alloc::dealloc(base as *mut u8, layout) };
            return Err("c-alloc: the `unsafe` that owns this allocation has already been left".to_string());
        };
        owned.push(base);
        s.blocks.insert(base, Block { len, elem_size, layout, positions: Arc::new(positions) });
        Ok(base)
    })
}

/// Whether `addr` holds a live C object of type `ty`: inside a live block,
/// at an offset where the block's element type has one.
pub fn check(addr: usize, ty: TypeId) -> Result<(), String> {
    let name = typelisp_print::shared::name_view().type_name(ty);
    if addr == 0 {
        return Err(format!(
            "a `(ptr {})` from C is a null pointer. Declare it as `ptr` if it can be null.",
            name
        ));
    }
    let found = with_state(|s| {
        let (base, blk) = s.blocks.range(..=addr).next_back()?;
        if addr >= base + blk.len {
            return None;
        }
        let rel = (addr - base) % blk.elem_size;
        Some(blk.positions.iter().any(|(off, t)| *off == rel && *t == ty))
    });
    match found {
        Some(true) => Ok(()),
        Some(false) => Err(format!(
            "a `(ptr {})` from C points into memory an `unsafe` allocated, but not at a `{}`",
            name, name
        )),
        None => Err(format!(
            "a `(ptr {})` from C does not point into memory an `unsafe` allocated with `c-alloc` — \
             memory C allocated, or memory already freed, cannot be read through a typed pointer. \
             Declare it as `ptr` to pass it on without reading it.",
            name
        )),
    }
}

/// `(c-ref p i)`: the address of the `i`th `ty` from `addr`, `size` bytes
/// each, which must still be inside the same allocation.
pub fn index(addr: usize, i: i64, size: usize, ty: TypeId) -> Result<usize, String> {
    let target = (addr as i128) + (i as i128) * (size as i128);
    let in_same_block = with_state(|s| {
        let (base, blk) = s.blocks.range(..=addr).next_back()?;
        if addr >= base + blk.len {
            return None;
        }
        Some(target >= *base as i128 && target + size as i128 <= (*base + blk.len) as i128)
    });
    match in_same_block {
        Some(true) => {
            let target = target as usize;
            check(target, ty)?;
            Ok(target)
        }
        Some(false) => Err(format!("c-ref: index {} is outside the allocation", i)),
        None => Err("c-ref: the pointer does not point into a live allocation".to_string()),
    }
}

/// Reads the `kind` at `addr`, as the word compiled code holds.
///
/// # Safety
///
/// `addr` must point to a readable value of that kind — the checker only
/// lowers to this with an address it derived from a checked typed pointer.
pub unsafe fn load(addr: usize, kind: i64) -> i64 {
    let p = addr as *const u8;
    match kind {
        KIND_I8 => p.cast::<i8>().read_unaligned() as i64,
        KIND_I16 => p.cast::<i16>().read_unaligned() as i64,
        KIND_I32 => p.cast::<i32>().read_unaligned() as i64,
        KIND_U8 => p.read_unaligned() as i64,
        KIND_U16 => p.cast::<u16>().read_unaligned() as i64,
        KIND_U32 => p.cast::<u32>().read_unaligned() as i64,
        KIND_C_LONG | KIND_C_ULONG | KIND_PTR => p.cast::<i64>().read_unaligned(),
        KIND_F32 => f64::from(p.cast::<f32>().read_unaligned()).to_bits() as i64,
        KIND_F64 => p.cast::<f64>().read_unaligned().to_bits() as i64,
        KIND_BOOL => i64::from(p.read_unaligned() != 0),
        k => fatal(&format!("c-load: unknown kind {}", k)),
    }
}

/// Writes `word` (as compiled code holds a value of `kind`) to `addr`.
///
/// # Safety
///
/// As [`load`], for a writable value.
pub unsafe fn store(addr: usize, kind: i64, word: i64) {
    let p = addr as *mut u8;
    match kind {
        KIND_I8 | KIND_U8 => p.write_unaligned(word as u8),
        KIND_I16 | KIND_U16 => p.cast::<u16>().write_unaligned(word as u16),
        KIND_I32 | KIND_U32 => p.cast::<u32>().write_unaligned(word as u32),
        KIND_C_LONG | KIND_C_ULONG | KIND_PTR => p.cast::<i64>().write_unaligned(word),
        KIND_F32 => p.cast::<f32>().write_unaligned(f64::from_bits(word as u64) as f32),
        KIND_F64 => p.cast::<f64>().write_unaligned(f64::from_bits(word as u64)),
        KIND_BOOL => p.write_unaligned(u8::from(word != 0)),
        k => fatal(&format!("c-store: unknown kind {}", k)),
    }
}

/// A type argument, as `TypeId::word` writes one.
fn type_arg(word: i64, who: &str) -> TypeId {
    TypeId::from_word(word).unwrap_or_else(|| fatal(&format!("{}: expected a type, got {:#x}", who, word)))
}

unsafe fn args_of<'a>(args: *const i64, argc: u32, want: u32, who: &str) -> &'a [i64] {
    if argc < want {
        fatal(&format!("{}: expected {} arguments", who, want));
    }
    std::slice::from_raw_parts(args, argc as usize)
}

/// `(" c-arena-open")`.
///
/// # Safety
///
/// Callable with any arguments; it reads none.
#[no_mangle]
pub unsafe extern "C" fn rt_c_arena_open(_args: *const i64, _argc: u32) -> i64 {
    arena_open() as i64
}

/// `(" c-arena-close" ARENA)`.
///
/// # Safety
///
/// `args` must point to one readable `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_c_arena_close(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 1, "rt_c_arena_close");
    if let Err(msg) = arena_close(a[0] as u64) {
        fatal(&msg);
    }
    0
}

/// `(" c-alloc" ARENA COUNT SIZE ALIGN OFFSET TYPE ...)`: an element of
/// `SIZE` bytes aligned to `ALIGN`, with a C object of each `TYPE` at its
/// `OFFSET`.
///
/// # Safety
///
/// `args` must point to `argc` readable `i64`s, at least four and an even
/// number, and a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_c_alloc(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 4, "rt_c_alloc");
    if a.len() % 2 != 0 {
        fatal("rt_c_alloc: an offset without its type");
    }
    let positions = a[4..].chunks(2).map(|p| (p[0] as usize, type_arg(p[1], "rt_c_alloc"))).collect();
    match alloc(a[0] as u64, a[1], a[2] as usize, a[3] as usize, positions) {
        Ok(p) => p as i64,
        Err(msg) => raise(msg),
    }
}

/// `(" c-ptr-check" ADDR TYPE)`: `ADDR`, once [`check`] accepts it.
///
/// # Safety
///
/// `args` must point to two readable `i64`s, and a `Heap` must be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_c_ptr_check(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 2, "rt_c_ptr_check");
    match check(a[0] as usize, type_arg(a[1], "rt_c_ptr_check")) {
        Ok(()) => a[0],
        Err(msg) => raise(msg),
    }
}

/// `(" c-index" ADDR I SIZE TYPE)`.
///
/// # Safety
///
/// `args` must point to four readable `i64`s, and a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_c_index(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 4, "rt_c_index");
    match index(a[0] as usize, a[1], a[2] as usize, type_arg(a[3], "rt_c_index")) {
        Ok(p) => p as i64,
        Err(msg) => raise(msg),
    }
}

/// `(" c-offset" ADDR OFFSET)`: the address of a field.
///
/// # Safety
///
/// `args` must point to two readable `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_c_offset(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 2, "rt_c_offset");
    a[0].wrapping_add(a[1])
}

/// `(" c-load" ADDR OFFSET KIND)`.
///
/// # Safety
///
/// `args` must point to three readable `i64`s describing a readable value.
#[no_mangle]
pub unsafe extern "C" fn rt_c_load(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 3, "rt_c_load");
    load(a[0].wrapping_add(a[1]) as usize, a[2])
}

/// `(" c-store" ADDR OFFSET KIND WORD)`.
///
/// # Safety
///
/// `args` must point to four readable `i64`s describing a writable value.
#[no_mangle]
pub unsafe extern "C" fn rt_c_store(args: *const i64, argc: u32) -> i64 {
    let a = args_of(args, argc, 4, "rt_c_store");
    store(a[0].wrapping_add(a[1]) as usize, a[2], a[3]);
    0
}
