//! Apple Silicon only: the MCJIT memory manager [`super::jit_engine`] uses, so
//! that a landing pad in JIT-compiled code can be entered on macOS 15.
//!
//! MCJIT's own memory manager hands each FDE to `__register_frame`, and the
//! unwinder does find a frame by it — a panic unwinds *through* compiled code.
//! What fails is entering a landing pad there. The personality routine moves
//! the frame's IP to the pad with `_Unwind_SetIP`, and macOS 15's
//! `unw_set_reg` then reads the `cpusubtype` of the frame's image — the
//! `mach_header` libunwind keeps as the frame's `unw_proc_info_t::extra` — to
//! decide whether to sign the new IP for arm64e. An FDE that came through
//! `__register_frame` has no image, `extra` is 0, and the read is a SIGSEGV at
//! address 0x8. (Seen in macOS 15.7.9's libunwind, `unw_set_reg+344`: `ldr w8,
//! [x22, #0x8]` after `x22 = info.extra`, no null check. macOS 27's does not
//! read it.) No `uwtable`, object format or registration order changes that:
//! `extra` is 0 for every frame found through `__register_frame`.
//!
//! libunwind takes `extra` from somewhere else for frames found through a
//! callback registered with `__unw_add_find_dynamic_unwind_sections` — the
//! interface LLVM's ORC uses for its JIT'd code: the callback's `dso_base`. And
//! it asks those callbacks *before* the FDEs `__register_frame` added. So this
//! memory manager knows where each object's code and `.eh_frame` went, and one
//! process-wide callback answers for that code with that `.eh_frame` and a
//! `mach_header` of its own that says plain arm64.
//!
//! The rest is what LLVM's `SectionMemoryManager` would do: each section gets
//! its own pages, code becomes read+execute and read-only data read-only when
//! MCJIT finalizes, and the pages go when the engine does. `__register_frame`
//! still happens (it is MCJIT's, not the memory manager's), and is what the
//! unwinder falls back to where `__unw_add_find_dynamic_unwind_sections` does
//! not exist.

use std::collections::BTreeMap;
use std::ffi::{c_int, c_void};
use std::sync::{OnceLock, RwLock};

use inkwell::memory_manager::McjitMemoryManager;

extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: c_int, flags: c_int, fd: c_int, offset: i64) -> *mut c_void;
    fn mprotect(addr: *mut c_void, len: usize, prot: c_int) -> c_int;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
    fn getpagesize() -> c_int;
    fn sys_icache_invalidate(start: *mut c_void, len: usize);
    fn dlsym(handle: *mut c_void, symbol: *const u8) -> *mut c_void;
}

const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const PROT_EXEC: c_int = 0x4;
const MAP_PRIVATE: c_int = 0x0002;
const MAP_ANON: c_int = 0x1000;
const MAP_FAILED: *mut c_void = !0usize as *mut c_void;
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

/// libunwind's `struct unw_dynamic_unwind_sections`.
#[repr(C)]
struct DynamicUnwindSections {
    dso_base: usize,
    dwarf_section: usize,
    dwarf_section_length: usize,
    compact_unwind_section: usize,
    compact_unwind_section_length: usize,
}

type FindSections = extern "C" fn(addr: usize, info: *mut DynamicUnwindSections) -> c_int;

/// `struct mach_header_64`, standing in for the image a frame of JIT'd code
/// belongs to. Only `cpusubtype` is read, and only by the `unw_set_reg` check
/// described in this module's doc comment.
#[repr(C)]
struct MachHeader64 {
    magic: u32,
    cputype: i32,
    cpusubtype: i32,
    filetype: u32,
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    reserved: u32,
}

const MH_MAGIC_64: u32 = 0xfeed_facf;
const CPU_TYPE_ARM64: i32 = 0x0100_000c;
const CPU_SUBTYPE_ARM64_ALL: i32 = 0;
const MH_BUNDLE: u32 = 0x8;

/// What the callback answers for an address inside one code section.
#[derive(Clone, Copy)]
struct Entry {
    end: usize,
    /// The `MachHeader64` of the object the code came in. Unique per object
    /// for as long as the object lives, since libunwind also keys its FDE
    /// cache by it.
    dso_base: usize,
    eh_frame: usize,
    eh_frame_len: usize,
}

/// Every live code section of JIT'd code, by start address.
static CODE: RwLock<BTreeMap<usize, Entry>> = RwLock::new(BTreeMap::new());

/// The callback libunwind calls for an address it found in no image.
///
/// Called on whatever thread is unwinding, possibly mid-panic: it takes a
/// read lock and allocates nothing.
extern "C" fn find_sections(addr: usize, info: *mut DynamicUnwindSections) -> c_int {
    let code = CODE.read().unwrap_or_else(|e| e.into_inner());
    match code.range(..=addr).next_back() {
        Some((_, entry)) if addr < entry.end => {
            // SAFETY: libunwind passes a valid, zeroed struct.
            unsafe {
                (*info).dso_base = entry.dso_base;
                (*info).dwarf_section = entry.eh_frame;
                (*info).dwarf_section_length = entry.eh_frame_len;
            }
            1
        }
        _ => 0,
    }
}

/// libunwind's `__unw_remove_dynamic_eh_frame_section`, if it has one: it
/// drops every FDE-cache entry libunwind made under a `dso_base`, which has to
/// happen before that `dso_base` can be freed and its address reused.
type RemoveCached = unsafe extern "C" fn(dso_base: usize);

struct Libunwind {
    remove_cached: Option<RemoveCached>,
}

/// Registers [`find_sections`] once, and says whether it could.
///
/// Looked up rather than linked: the interface is newer than some of the
/// macOS versions this runs on, and without it the code below still does the
/// memory manager's job — only the landing-pad fix is missing.
fn libunwind() -> Option<&'static Libunwind> {
    static LIBUNWIND: OnceLock<Option<Libunwind>> = OnceLock::new();
    LIBUNWIND
        .get_or_init(|| {
            // SAFETY: dlsym with NUL-terminated names; the results are the
            // functions those names declare in libunwind.h / libunwind_ext.h.
            unsafe {
                let add = dlsym(RTLD_DEFAULT, b"__unw_add_find_dynamic_unwind_sections\0".as_ptr());
                if add.is_null() {
                    return None;
                }
                let add: unsafe extern "C" fn(FindSections) -> c_int = std::mem::transmute(add);
                if add(find_sections) != 0 {
                    return None;
                }
                let remove = dlsym(RTLD_DEFAULT, b"__unw_remove_dynamic_eh_frame_section\0".as_ptr());
                let remove_cached = (!remove.is_null()).then(|| std::mem::transmute::<*mut c_void, RemoveCached>(remove));
                Some(Libunwind { remove_cached })
            }
        })
        .as_ref()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Code,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug)]
struct Section {
    ptr: *mut u8,
    /// What LLVM asked for.
    size: usize,
    /// What was mapped: `size` rounded up to whole pages, at least one.
    mapped: usize,
    kind: Kind,
    eh_frame: bool,
}

/// One object's registration with [`CODE`], kept so it can be undone.
#[derive(Debug)]
struct Registered {
    header: *mut MachHeader64,
    code_starts: Vec<usize>,
}

/// See this module's doc comment.
#[derive(Debug, Default)]
pub(super) struct UnwindingMemoryManager {
    sections: Vec<Section>,
    /// How many of `sections` have been finalized; the rest arrived since.
    finalized: usize,
    registered: Vec<Registered>,
}

impl UnwindingMemoryManager {
    fn allocate(&mut self, size: usize, alignment: u32, kind: Kind, eh_frame: bool) -> *mut u8 {
        // SAFETY: getpagesize has no preconditions.
        let page = unsafe { getpagesize() } as usize;
        // mmap returns page-aligned memory; nothing LLVM emits asks for more.
        if alignment as usize > page {
            return std::ptr::null_mut();
        }
        let mapped = size.max(1).div_ceil(page) * page;
        // SAFETY: a fresh anonymous private mapping, owned by this section.
        let ptr = unsafe { mmap(std::ptr::null_mut(), mapped, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0) };
        if ptr == MAP_FAILED {
            return std::ptr::null_mut();
        }
        let ptr = ptr as *mut u8;
        self.sections.push(Section { ptr, size, mapped, kind, eh_frame });
        ptr
    }

    /// Makes the object whose sections are `self.sections[from..]` known to
    /// [`find_sections`]. An object without an `.eh_frame` has nothing to
    /// unwind with, so it is left to the unwinder's other lookups.
    fn register(&mut self, from: usize) {
        if libunwind().is_none() {
            return;
        }
        let new = &self.sections[from..];
        let Some(eh_frame) = new.iter().find(|s| s.eh_frame) else { return };
        let header = Box::into_raw(Box::new(MachHeader64 {
            magic: MH_MAGIC_64,
            cputype: CPU_TYPE_ARM64,
            cpusubtype: CPU_SUBTYPE_ARM64_ALL,
            filetype: MH_BUNDLE,
            ncmds: 0,
            sizeofcmds: 0,
            flags: 0,
            reserved: 0,
        }));
        let mut code = CODE.write().unwrap_or_else(|e| e.into_inner());
        let mut code_starts = Vec::new();
        for section in new.iter().filter(|s| s.kind == Kind::Code) {
            let start = section.ptr as usize;
            code.insert(
                start,
                Entry {
                    end: start + section.size,
                    dso_base: header as usize,
                    eh_frame: eh_frame.ptr as usize,
                    eh_frame_len: eh_frame.size,
                },
            );
            code_starts.push(start);
        }
        self.registered.push(Registered { header, code_starts });
    }
}

impl McjitMemoryManager for UnwindingMemoryManager {
    fn allocate_code_section(&mut self, size: usize, alignment: u32, _section_id: u32, _section_name: &str) -> *mut u8 {
        self.allocate(size, alignment, Kind::Code, false)
    }

    fn allocate_data_section(
        &mut self,
        size: usize,
        alignment: u32,
        _section_id: u32,
        section_name: &str,
        is_read_only: bool,
    ) -> *mut u8 {
        let kind = if is_read_only { Kind::ReadOnly } else { Kind::ReadWrite };
        self.allocate(size, alignment, kind, section_name == ".eh_frame")
    }

    fn finalize_memory(&mut self) -> Result<(), String> {
        let from = self.finalized;
        for section in &self.sections[from..] {
            let prot = match section.kind {
                Kind::Code => PROT_READ | PROT_EXEC,
                Kind::ReadOnly => PROT_READ,
                Kind::ReadWrite => continue,
            };
            // SAFETY: the section's own mapping.
            if unsafe { mprotect(section.ptr as *mut c_void, section.mapped, prot) } != 0 {
                return Err(format!("mprotect failed: {}", std::io::Error::last_os_error()));
            }
            if section.kind == Kind::Code {
                // SAFETY: as above; the code was written through the data
                // side, so the instruction cache has to be told.
                unsafe { sys_icache_invalidate(section.ptr as *mut c_void, section.size) };
            }
        }
        self.finalized = self.sections.len();
        self.register(from);
        Ok(())
    }

    fn destroy(&mut self) {
        // Unregistered before the pages go: an unwinder that asks after this
        // must not be pointed at them.
        {
            let mut code = CODE.write().unwrap_or_else(|e| e.into_inner());
            for registered in &self.registered {
                for start in &registered.code_starts {
                    code.remove(start);
                }
            }
        }
        for registered in self.registered.drain(..) {
            match libunwind().and_then(|l| l.remove_cached) {
                Some(remove_cached) => {
                    // SAFETY: drops libunwind's cache entries under this
                    // header, after which nothing refers to it.
                    unsafe {
                        remove_cached(registered.header as usize);
                        drop(Box::from_raw(registered.header));
                    }
                }
                // Without a way to drop the cache entries, the header stays,
                // so its address is never reused as another object's key.
                None => {}
            }
        }
        for section in self.sections.drain(..) {
            // SAFETY: the section's own mapping, unmapped once.
            unsafe { munmap(section.ptr as *mut c_void, section.mapped) };
        }
        self.finalized = 0;
    }
}
