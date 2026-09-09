//! The tagged-`i64` representation compiled code uses for a [`Value`].
//!
//! **Why this lives in `typelisp-mem` and not in `typelisp-abi`.** It was in
//! `typelisp-abi` until the compiled-CPS work (Phase C): the encoding is
//! about `Value`, which is defined here, and the *collector* needs it. A
//! compiled frame ([`crate::value::BoxedObj::Frame`]) holds tagged words
//! rather than `Value`s — compiled code loads and stores its locals through
//! a raw pointer, so they have to be machine words — and the mark phase has
//! to be able to say which of those words reference the heap. `typelisp-abi`
//! depends on *this* crate, so the decoder could not have been read from
//! there. Re-deriving the scheme here instead would be two tables that must
//! agree about all eight tags.
//!
//! `typelisp-abi` re-exports [`encode`] and [`decode`], so every existing
//! caller still names them where it always did.

use crate::value::{BoxId, PathId, StrId, Value};
use crate::{ConsRef, SymRef};

pub const TAG_BITS: i64 = 3;
pub const TAG_MASK: i64 = 0b111;

// Stage 2's tag table (`docs/dev/implementation-log.md, "Sexpr表現 +
// Match/Construct/共有Rustライブラリ 実装計画"`): 8 tags in the low 3 bits.
// `Nil`/`Bool` share one "immediate constant" tag (`TAG_IMMEDIATE`) since
// `Value` has 9 variants but only 8 tag slots — see that doc for the full
// rationale (why this needs no more than 3 bits, the alignment argument for
// `Cons` pointers, etc.). `TAG_BOXED` (formerly `TAG_FLOAT`, reclaimed by the
// `Sexpr`/`RtValue` unification plan — see `BoxedObj`'s doc comment):
// `Value::Float` used to claim this tag directly and was never actually
// representable in compiled code (`encode`/`decode` both failed on it); an
// `f64` doesn't fit losslessly in the remaining bits alongside a tag anyway,
// so this tag now means "payload is a `BoxId` into the heap's boxed-object
// store" instead of trying to pack an immediate float.
pub const TAG_FIXNUM: i64 = 0b000;
pub const TAG_CONS: i64 = 0b001;
pub const TAG_SYMBOL: i64 = 0b010;
pub const TAG_STR: i64 = 0b011;
pub const TAG_CHAR: i64 = 0b100;
pub const TAG_PATH: i64 = 0b101;
pub const TAG_IMMEDIATE: i64 = 0b110;
pub const TAG_BOXED: i64 = 0b111;

pub const IMMEDIATE_NIL: i64 = 0;
pub const IMMEDIATE_FALSE: i64 = 1;
pub const IMMEDIATE_TRUE: i64 = 2;

/// Prints `msg` to stderr and aborts — a corrupt tagged word means the
/// *runtime* is broken, not the program, so there is nothing to unwind to.
///
/// The same division `typelisp_abi::fatal` documents at length (SBCL's
/// `lose()` versus signalling a condition); duplicated as a private helper
/// rather than depended on, because `typelisp-abi` is the crate above this
/// one.
fn corrupt(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// Encodes a `Value` into the tagged `i64` representation compiled code
/// uses for a `Sexpr`. `Value::Boxed` needs no allocation here — unlike a
/// hypothetical unboxed `Float` payload, a `BoxId` is already just a small
/// integer index, exactly like `Symbol`/`Str`/`Path`; the caller must have
/// already allocated the box (via e.g. `Heap::alloc_f64`) the same way a
/// `Value::Cons` must already be a live heap cell before reaching this
/// function.
pub fn encode(v: Value) -> i64 {
    match v {
        Value::Int(n) => (n << TAG_BITS) | TAG_FIXNUM,
        Value::Cons(c) => (c.addr() as i64) | TAG_CONS,
        // The address itself, `Cons`-style: a `Symbol` header is 8-byte
        // aligned, so the low 3 bits are the tag's to use.
        Value::Symbol(s) => (s.addr() as i64) | TAG_SYMBOL,
        Value::Str(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_STR,
        Value::Char(c) => ((c as i64) << TAG_BITS) | TAG_CHAR,
        Value::Path(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_PATH,
        Value::Empty => (IMMEDIATE_NIL << TAG_BITS) | TAG_IMMEDIATE,
        Value::Bool(false) => (IMMEDIATE_FALSE << TAG_BITS) | TAG_IMMEDIATE,
        Value::Bool(true) => (IMMEDIATE_TRUE << TAG_BITS) | TAG_IMMEDIATE,
        Value::Boxed(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_BOXED,
    }
}

/// The inverse of [`encode`].
pub fn decode(tagged: i64) -> Value {
    match tagged & TAG_MASK {
        TAG_FIXNUM => Value::Int(tagged >> TAG_BITS),
        TAG_CONS => Value::Cons(unsafe { ConsRef::from_addr((tagged & !TAG_MASK) as usize) }),
        TAG_SYMBOL => Value::Symbol(unsafe { SymRef::from_addr((tagged & !TAG_MASK) as usize) }),
        TAG_STR => Value::Str(StrId::from_u32((tagged >> TAG_BITS) as u32)),
        TAG_CHAR => {
            let scalar = (tagged >> TAG_BITS) as u32;
            Value::Char(char::from_u32(scalar).unwrap_or_else(|| corrupt("decode: invalid char scalar value")))
        }
        TAG_PATH => Value::Path(PathId::from_u32((tagged >> TAG_BITS) as u32)),
        TAG_IMMEDIATE => match tagged >> TAG_BITS {
            IMMEDIATE_NIL => Value::Empty,
            IMMEDIATE_FALSE => Value::Bool(false),
            IMMEDIATE_TRUE => Value::Bool(true),
            other => corrupt(&format!("decode: unknown immediate tag payload {}", other)),
        },
        TAG_BOXED => Value::Boxed(BoxId::from_u32((tagged >> TAG_BITS) as u32)),
        _ => unreachable!("a 3-bit mask is always one of the 8 arms above"),
    }
}

/// Whether a tagged word points at something the collector owns.
///
/// The mark phase's question about a compiled frame slot, asked without
/// building a `Value` first: a fixnum, a char and the three immediates
/// reference nothing, so a frame full of raw `i32` locals costs the collector
/// one mask test and one tag test per slot and no more.
pub fn references_heap(tagged: i64) -> bool {
    matches!(tagged & TAG_MASK, TAG_CONS | TAG_SYMBOL | TAG_STR | TAG_PATH | TAG_BOXED)
}
