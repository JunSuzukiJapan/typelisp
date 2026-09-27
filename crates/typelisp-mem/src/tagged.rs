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
//! agree about every class.
//!
//! `typelisp-abi` re-exports [`encode`] and [`decode`], so every existing
//! caller still names them where it always did.
//!
//! # The layout
//!
//! A variable-length low tag, SBCL's shape (its `fixnum-tag-bits` is 1 and
//! its `lowtag` is wider): a fixnum spends **one** bit, everything else
//! pays more, because the fixnum is the one class whose payload wants every
//! bit it can get and the one class arithmetic touches directly.
//!
//! ```text
//! ....0      fixnum      n << 1, 63-bit signed
//! ..001      cons        address | 1     (a `Cell` is 8-byte aligned)
//! ..011      symbol      address | 3     (a `Symbol` header is 8-byte aligned)
//! ..101      boxed       (BoxId << 3) | 5
//! ..111      small       bits 3-5 pick the class, the payload sits above bit 6:
//!    000 immediate   0 = `()`, 1 = false, 2 = true   → the words 7, 71, 135
//!    001 char        scalar << 6 | 15
//!    010 str         StrId  << 6 | 23
//!    011 path        PathId << 6 | 31
//!    1xx reserved    `decode` aborts
//! ```
//!
//! Why the low bits and not the high ones: the low three bits of an aligned
//! address are the allocator's to give away, while the high bits belong to
//! the architecture (48/57-bit virtual addresses, arm64's top-byte and
//! pointer-authentication schemes). And with the fixnum tag being zero,
//! `a + b` on two tagged fixnums is the tagged sum, comparison is comparison
//! of the words, and the collector's question "does this slot reference the
//! heap?" is answered for a fixnum by one bit.
//!
//! Why a fixnum is 63 bits and not 64: some bit has to say "not a pointer".
//! The language has no 64-bit integer type for exactly this reason
//! (`docs/ja/reference/functions/numbers.md` §1); the arbitrary-precision `int` promotes to a
//! bignum box above this range instead of dropping bits, and [`encode`]
//! aborts rather than truncate if handed a wider `Value::Int` — a `Value::Int`
//! is also how raw C words travel through the *interpreter*, and those must
//! never reach a tagged word.

use crate::value::{BoxId, PathId, StrId, Value};
use crate::{ConsRef, SymRef};

/// Which tag layout this runtime encodes and decodes. Recorded in every dump
/// beside the body ABI (`UnitState::body_layout` / `emits_layout`), for the
/// same reason the ABI is: a compiled body has the layout baked into its
/// inline tag tests and shifts, the bitcode does not say which, and running
/// a body under the wrong layout is a wrong answer, not an error.
///
/// Changing the layout is a generation-shifted changeover across the
/// self-hosting chain, exactly like an ABI change (see
/// `crate::compiler::ISLAND_DUMP_BODY_LAYOUT` and its two siblings): the
/// committed island's bodies are under the old layout while the SOURCE it
/// compiles emits the new one, and only the bootstrap binary may run in
/// that state.
pub const LAYOUT: u8 = LAYOUT_OPTION_NICHE;

/// Every word carried a 3-bit tag in its low bits; a fixnum kept 61 bits.
/// Retired 2026-09-15; the number stays taken so a dump that records it is
/// refused by name rather than misread.
pub const LAYOUT_THREE_BIT: u8 = 0;
/// The layout above: a fixnum spends one bit, every other class three or six.
/// Retired 2026-09-17 with the `Option<T>` niche; the number stays taken.
pub const LAYOUT_FIXNUM_ONE_BIT: u8 = 1;
/// [`LAYOUT_FIXNUM_ONE_BIT`]'s words, plus the `Option<T>` niche: an
/// `Option` whose payload can be told from the empty-list immediate is
/// *that payload's tagged word* for `some` and [`NIL_WORD`] for `none`,
/// with no box (`crate::option`). A body compiled under the previous
/// layout builds and expects boxes for those same types, so it cannot run
/// under this one.
pub const LAYOUT_OPTION_NICHE: u8 = 2;

/// A fixnum is `n << FIXNUM_SHIFT` with the low bit clear.
pub const FIXNUM_SHIFT: i64 = 1;
pub const FIXNUM_MIN: i64 = -(1 << 62);
pub const FIXNUM_MAX: i64 = (1 << 62) - 1;

/// The three-bit classes. A fixnum is any word with bit 0 clear, so it has
/// no entry here: `FIXNUM_MASK`/`FIXNUM_TAG` are its test.
pub const FIXNUM_MASK: i64 = 0b1;
pub const FIXNUM_TAG: i64 = 0b0;
pub const LOW_MASK: i64 = 0b111;
pub const TAG_CONS: i64 = 0b001;
pub const TAG_SYMBOL: i64 = 0b011;
pub const TAG_BOXED: i64 = 0b101;
pub const TAG_SMALL: i64 = 0b111;

/// The small classes: `LOW_MASK` bits are `TAG_SMALL`, bits 3-5 are one of
/// these, and the payload starts at `SMALL_SHIFT`.
pub const SMALL_MASK: i64 = 0b111_111;
pub const SMALL_SHIFT: i64 = 6;
pub const TAG_IMMEDIATE: i64 = (0 << 3) | TAG_SMALL;
pub const TAG_CHAR: i64 = (1 << 3) | TAG_SMALL;
pub const TAG_STR: i64 = (2 << 3) | TAG_SMALL;
pub const TAG_PATH: i64 = (3 << 3) | TAG_SMALL;

/// `BoxId` sits above the three-bit class.
pub const BOXED_SHIFT: i64 = 3;

pub const IMMEDIATE_NIL: i64 = 0;
pub const IMMEDIATE_FALSE: i64 = 1;
pub const IMMEDIATE_TRUE: i64 = 2;

/// The empty list as a word — `Value::Empty`'s encoding, and `Option<T>`'s
/// `none` under the niche. The island writes the same constant.
pub const NIL_WORD: i64 = (IMMEDIATE_NIL << SMALL_SHIFT) | TAG_IMMEDIATE;

/// Whether `n` survives `encode` as a fixnum.
pub fn fixnum_fits(n: i64) -> bool {
    (FIXNUM_MIN..=FIXNUM_MAX).contains(&n)
}

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
/// uses for a `Sexpr`. `Value::Boxed` needs no allocation here — a `BoxId`
/// is already just a small integer index, exactly like `Str`/`Path`; the
/// caller must have already allocated the box (via e.g. `Heap::alloc_f64`)
/// the same way a `Value::Cons` must already be a live heap cell before
/// reaching this function.
///
/// Aborts on a `Value::Int` outside the fixnum range: that word has no
/// encoding, and the producers that can make one (the reader, `int`
/// arithmetic) are required to have boxed it as a bignum already
/// (`Heap::canonical_int`). Truncating here would be the silent loss of
/// bits the 63-bit layout exists to make impossible.
pub fn encode(v: Value) -> i64 {
    match try_encode(v) {
        Ok(w) => w,
        Err(n) => corrupt(&format!("encode: {} does not fit a 63-bit fixnum and was not boxed", n)),
    }
}

/// [`encode`] as a question: `Err(n)` for the one value with no encoding, a
/// `Value::Int` outside the fixnum range. For the producers that have to
/// decide between a fixnum and a bignum box before a word exists, and for
/// tests — `encode` itself aborts, as a runtime-corruption condition must.
pub fn try_encode(v: Value) -> Result<i64, i64> {
    Ok(match v {
        Value::Int(n) => {
            if !fixnum_fits(n) {
                return Err(n);
            }
            n << FIXNUM_SHIFT
        }
        Value::Cons(c) => (c.addr() as i64) | TAG_CONS,
        // The address itself, `Cons`-style: a `Symbol` header is 8-byte
        // aligned, so the low 3 bits are the tag's to use.
        Value::Symbol(s) => (s.addr() as i64) | TAG_SYMBOL,
        Value::Str(id) => ((id.as_u32() as i64) << SMALL_SHIFT) | TAG_STR,
        Value::Char(c) => ((c as i64) << SMALL_SHIFT) | TAG_CHAR,
        Value::Path(id) => ((id.as_u32() as i64) << SMALL_SHIFT) | TAG_PATH,
        Value::Empty => NIL_WORD,
        Value::Bool(false) => (IMMEDIATE_FALSE << SMALL_SHIFT) | TAG_IMMEDIATE,
        Value::Bool(true) => (IMMEDIATE_TRUE << SMALL_SHIFT) | TAG_IMMEDIATE,
        Value::Boxed(id) => ((id.as_u32() as i64) << BOXED_SHIFT) | TAG_BOXED,
    })
}

/// The inverse of [`encode`].
pub fn decode(tagged: i64) -> Value {
    if tagged & FIXNUM_MASK == FIXNUM_TAG {
        return Value::Int(tagged >> FIXNUM_SHIFT);
    }
    match tagged & LOW_MASK {
        TAG_CONS => Value::Cons(unsafe { ConsRef::from_addr((tagged & !LOW_MASK) as usize) }),
        TAG_SYMBOL => Value::Symbol(unsafe { SymRef::from_addr((tagged & !LOW_MASK) as usize) }),
        TAG_BOXED => Value::Boxed(BoxId::from_u32((tagged >> BOXED_SHIFT) as u32)),
        TAG_SMALL => match tagged & SMALL_MASK {
            TAG_IMMEDIATE => match tagged >> SMALL_SHIFT {
                IMMEDIATE_NIL => Value::Empty,
                IMMEDIATE_FALSE => Value::Bool(false),
                IMMEDIATE_TRUE => Value::Bool(true),
                other => corrupt(&format!("decode: unknown immediate tag payload {}", other)),
            },
            TAG_CHAR => {
                let scalar = (tagged >> SMALL_SHIFT) as u32;
                Value::Char(char::from_u32(scalar).unwrap_or_else(|| corrupt("decode: invalid char scalar value")))
            }
            TAG_STR => Value::Str(StrId::from_u32((tagged >> SMALL_SHIFT) as u32)),
            TAG_PATH => Value::Path(PathId::from_u32((tagged >> SMALL_SHIFT) as u32)),
            other => corrupt(&format!("decode: reserved small tag {:#b}", other)),
        },
        _ => unreachable!("an odd word's low three bits are one of the four odd classes above"),
    }
}

/// Whether a tagged word points at something the collector owns.
///
/// The mark phase's question about a compiled frame slot, asked without
/// building a `Value` first. Two levels, because the layout is: a fixnum
/// answers on bit 0 alone, a pointer class on the low three bits, and only
/// the small class has to look at its sub-tag (a `Str` and a `Path` are
/// interned in the heap and swept; a char and the immediates reference
/// nothing).
pub fn references_heap(tagged: i64) -> bool {
    if tagged & FIXNUM_MASK == FIXNUM_TAG {
        return false;
    }
    match tagged & LOW_MASK {
        TAG_CONS | TAG_SYMBOL | TAG_BOXED => true,
        TAG_SMALL => matches!(tagged & SMALL_MASK, TAG_STR | TAG_PATH),
        _ => unreachable!("an odd word's low three bits are one of the four odd classes above"),
    }
}
