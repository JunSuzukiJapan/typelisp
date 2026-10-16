//! The optimization level — `typl -O0` to `-O3`.
//!
//! One number for the whole process. The command line only records it;
//! whatever optimizes asks it, at the time it does its work, and decides
//! there what that level means for it (today: the compiler island's IR
//! passes — `compile::driver::run_compile_function` — and LLVM's pass
//! pipeline and code generator — `compile::optimize`). `0`, the default,
//! optimizes nothing: the machine code is the IR the island emitted.
//!
//! The interpreter does not ask: what it runs is not compiled.

use std::sync::atomic::{AtomicU8, Ordering};

/// The highest level there is.
pub const MAX: u8 = 3;

static LEVEL: AtomicU8 = AtomicU8::new(0);

pub fn opt_level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

/// Records `level` for the rest of the process. Past [`MAX`] is the command
/// line's mistake, which [`parse_flag`] refuses before it gets here.
pub fn set_opt_level(level: u8) {
    assert!(level <= MAX, "optimization level {} is past {}", level, MAX);
    LEVEL.store(level, Ordering::Relaxed);
}

/// `-O0` .. `-O3` as the level it names. `None` when `arg` is not an `-O`
/// flag at all; `Some(Err)` when it is one naming no level.
pub fn parse_flag(arg: &str) -> Option<Result<u8, String>> {
    let rest = arg.strip_prefix("-O")?;
    Some(match rest.parse::<u8>() {
        Ok(level) if level <= MAX => Ok(level),
        _ => Err(format!("{}: the optimization level is one of -O0, -O1, -O2 and -O3", arg)),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_flag;

    #[test]
    fn the_four_levels_parse_and_nothing_else_does() {
        for level in 0..=3u8 {
            assert_eq!(parse_flag(&format!("-O{}", level)), Some(Ok(level)));
        }
        for bad in ["-O", "-O4", "-Os", "-O-1", "-O01x"] {
            assert!(matches!(parse_flag(bad), Some(Err(_))), "{}", bad);
        }
        assert_eq!(parse_flag("-o"), None);
        assert_eq!(parse_flag("--release"), None);
    }
}
