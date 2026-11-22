//! The hash maps and sets the runtime's crates use: `hashbrown`'s, the
//! table std's are built on, so that the same code builds without std.
//! Only the hash function differs from std's.

pub use hashbrown::{HashMap, HashSet};
