//! The C data model of the code being produced: how wide C's `long` and a
//! pointer are, and how far a scalar is aligned.
//!
//! Compiled code and the C it calls must agree on these, and the compiler
//! runs on one machine while the code it produces may run on another: an
//! executable for a Cortex-M board has a 32-bit `long` whatever the machine
//! compiling it has. So the checker (a `def-c-struct`'s layout, the range of
//! a `c-long` literal), the interpreter (what `(as c-long n)` keeps while a
//! program for a target compiles) and the code generator (the FFI thunk, the
//! width casts) all ask [`DataModel::current`], and the AOT driver sets it
//! for the build ([`DataModel::producing`]). Without a build setting it, it
//! is this machine's own ([`DataModel::HOST`]), which is what the JIT and
//! the REPL produce code for.
//!
//! The runtime does not ask: it runs on the machine the code runs on, so
//! `core::ffi::c_long` there is already the answer.

/// How wide C's `long` and a pointer are, and how far anything is aligned:
/// a scalar is aligned as it is wide, up to that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataModel {
    /// `sizeof(void *)`.
    pub pointer_bytes: u32,
    /// `sizeof(long)`.
    pub c_long_bytes: u32,
    /// The widest alignment a scalar has.
    pub max_align: u32,
}

impl DataModel {
    /// The machine this code is compiled for.
    pub const HOST: DataModel = DataModel {
        pointer_bytes: core::mem::size_of::<usize>() as u32,
        c_long_bytes: core::mem::size_of::<core::ffi::c_long>() as u32,
        max_align: core::mem::align_of::<f64>() as u32,
    };

    /// 32-bit `int`, `long` and pointer: wasm32 and the Cortex-M boards
    /// (AAPCS aligns a `double` to 8, and so does wasm32's C ABI).
    pub const ILP32: DataModel = DataModel { pointer_bytes: 4, c_long_bytes: 4, max_align: 8 };

    /// AVR: a 16-bit pointer and a 32-bit `long`, nothing aligned to more than
    /// a byte.
    pub const AVR: DataModel = DataModel { pointer_bytes: 2, c_long_bytes: 4, max_align: 1 };

    /// `long`'s width in bits.
    pub const fn c_long_bits(self) -> u32 {
        self.c_long_bytes * 8
    }

    /// A pointer's width in bits.
    pub const fn pointer_bits(self) -> u32 {
        self.pointer_bytes * 8
    }
}

#[cfg(feature = "std")]
std::thread_local! {
    static CURRENT: core::cell::Cell<DataModel> = const { core::cell::Cell::new(DataModel::HOST) };
}

#[cfg(feature = "std")]
impl DataModel {
    /// The data model of the code being produced on this thread.
    pub fn current() -> DataModel {
        CURRENT.with(|c| c.get())
    }

    /// While the answer lives, code produced on this thread is for `model`.
    pub fn producing(model: DataModel) -> Producing {
        Producing { before: CURRENT.with(|c| c.replace(model)) }
    }
}

/// [`DataModel::producing`]'s guard: puts back the model it replaced.
#[cfg(feature = "std")]
pub struct Producing {
    before: DataModel,
}

#[cfg(feature = "std")]
impl Drop for Producing {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.before));
    }
}
