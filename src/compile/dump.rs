//! The dump *file*: one unit's checked state and one unit's bitcode, in one
//! artifact, for as many units as the dump holds.
//!
//! The front end owns what a unit means ([`typelisp_front::dump`]); this owns
//! how it is written down and how the native half gets installed.
//!
//! **Why a header rather than appending to the `.bc`.** Adding bytes after a
//! bitstream and handing the whole file to LLVM is not something the format
//! promises to tolerate — `llvm-dis` already rejects our artifacts over a
//! single trailing NUL ("Bitcode stream should be a multiple of 4 bytes in
//! length"). With a directory of offsets the loader slices out exactly the
//! bytes `Module::parse_bitcode_from_buffer` was always given, and the file
//! stays one file.

use std::convert::TryInto;

use typelisp_front::dump::{apply_types, UnitItem, UnitState};

use crate::compile::symbols::CompiledItem;
use crate::{Checker, Heap, Interp};

/// Identifies the file and, with [`FORMAT_VERSION`], what is in it.
pub const MAGIC: &[u8; 6] = b"TYPLD\0";

/// The container's own version, distinct from
/// [`typelisp_front::dump::FORMAT_VERSION`] (the unit payload's): a change to
/// the directory layout and a change to what a unit records are different
/// events, and either one alone should be able to reject an old file.
pub const FORMAT_VERSION: u32 = 1;

const HEADER: usize = 6 + 4 + 4;
const DIRECTORY_ENTRY: usize = 8 * 4;

/// One unit as it sits in a dump: the bytes of its checked state, and the
/// bitcode holding the bodies that state describes.
///
/// Borrowed, not owned: the prelude's and island's dumps are `include_bytes!`
/// statics, and the bitcode is handed straight to LLVM, which copies it into
/// its own `MemoryBuffer` anyway.
pub struct UnitRef<'a> {
    pub types: &'a [u8],
    pub bitcode: &'a [u8],
}

/// Lays out `units` as a dump.
pub fn write(units: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());

    let mut offset = (HEADER + DIRECTORY_ENTRY * units.len()) as u64;
    for (types, bitcode) in units {
        for section in [types, bitcode] {
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(section.len() as u64).to_le_bytes());
            offset += section.len() as u64;
        }
    }
    for (types, bitcode) in units {
        out.extend_from_slice(types);
        out.extend_from_slice(bitcode);
    }
    out
}

/// Reads a dump's directory, without touching the payloads.
pub fn parse<'a>(bytes: &'a [u8], label: &str) -> Result<Vec<UnitRef<'a>>, String> {
    if bytes.len() < HEADER || &bytes[..6] != MAGIC {
        return Err(format!("{}: not a typelisp dump", label));
    }
    let version = u32::from_le_bytes(bytes[6..10].try_into().expect("4 bytes"));
    if version != FORMAT_VERSION {
        return Err(format!(
            "{}: dump container version {}, expected {} — the file and this build are from \
             different sources",
            label, version, FORMAT_VERSION
        ));
    }
    let count = u32::from_le_bytes(bytes[10..14].try_into().expect("4 bytes")) as usize;
    let directory_end = HEADER + DIRECTORY_ENTRY * count;
    if bytes.len() < directory_end {
        return Err(format!("{}: dump claims {} units but ends inside its directory", label, count));
    }

    let word = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes")) as usize;
    let mut units = Vec::with_capacity(count);
    for i in 0..count {
        let base = HEADER + DIRECTORY_ENTRY * i;
        let section = |half: usize| -> Result<&[u8], String> {
            let (off, len) = (word(base + half * 16), word(base + half * 16 + 8));
            bytes
                .get(off..off + len)
                .ok_or_else(|| format!("{}: dump unit {} points past the end of the file", label, i))
        };
        units.push(UnitRef { types: section(0)?, bitcode: section(1)? });
    }
    Ok(units)
}

/// Reads one unit's checked state, without applying it.
///
/// Split out from [`load_unit`] because a caller with a source to check
/// against (`prelude_bootstrap::load` and the island's) has to see
/// `source_digest` *before* anything is applied.
pub fn read_types(unit: &UnitRef, label: &str) -> Result<UnitState, String> {
    typelisp_front::dump::read_state(unit.types, label)
}

/// Applies one unit: its checker state and definitions, then the native bodies
/// its bitcode holds.
///
/// `chk`/`interp` must be the pair every previously applied unit went into —
/// a unit records what its own compilation added, so applying one out of order
/// leaves it referring to names that are not there yet.
pub fn load_unit(
    heap: &mut Heap,
    chk: &mut Checker,
    interp: &mut Interp,
    state: UnitState,
    bitcode: &[u8],
    regen_script: &str,
) -> Result<(), String> {
    let label = state.label.clone();
    let globals = state.globals.clone();
    let items: Vec<CompiledItem> = state
        .items
        .iter()
        .map(|i| match i {
            UnitItem::Fn(path) => CompiledItem::Fn(path.clone()),
            UnitItem::Method(path, name) => CompiledItem::Method(path.clone(), name.clone()),
        })
        .collect();

    apply_types(heap, chk, interp, state)?;

    // Storage for the unit's globals, created here and in the recorded order:
    // `typelisp_rt`'s table hands ids out sequentially from zero per `Interp`,
    // and the bitcode addresses them by the id it was compiled against. The
    // check is not paranoia — `Interp::promote_global` is also called *lazily*
    // by the compile driver the first time a body mentions a global, so a load
    // that did anything before this point could already have consumed an id.
    for (name, id) in &globals {
        let path = typelisp_front::dump::parse_path(name);
        let got = interp
            .promote_global(heap, &path)
            .map_err(|e| format!("{}: creating storage for global `{}`: {}", label, path, e))?;
        if got != *id {
            return Err(format!(
                "{}: global `{}` was compiled against slot {} but this load put it in slot {} — \
                 something claimed a compiled-global id before the dump was applied",
                label, path, id, got
            ));
        }
    }

    // A unit with nothing compiled — a session that never called `(compile)` —
    // has no bitcode section at all, and asking LLVM to parse zero bytes is a
    // parse error rather than an empty module.
    if bitcode.is_empty() {
        return Ok(());
    }
    crate::compile::driver::install_compiled_library(
        interp,
        crate::compile::CompiledLibrary {
            label: &label,
            regen_script,
            bitcode,
            items: &items,
            // The unit and its bitcode were written by one pass into one file,
            // so there is no second artifact to have drifted from. What *can*
            // drift is the source both were built from, and that is checked
            // against `UnitState::source_digest` before this is reached.
            expected_hash: None,
        },
    )
}
