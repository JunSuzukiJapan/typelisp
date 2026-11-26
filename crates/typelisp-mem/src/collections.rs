//! The hash maps and sets the runtime's crates use: `hashbrown`'s, the
//! table std's are built on, so that the same code builds without std.
//! Only the hash function differs from std's.

pub use hashbrown::{HashMap, HashSet};

/// A hasher with no random seed: the same value hashes the same in every
/// run and every build. What a hash the program can see (`sxhash`) is
/// computed with.
pub type FixedHasher = foldhash::fast::FoldHasher;

/// A fresh [`FixedHasher`].
pub fn fixed_hasher() -> FixedHasher {
    use core::hash::BuildHasher;
    foldhash::fast::FixedState::default().build_hasher()
}
