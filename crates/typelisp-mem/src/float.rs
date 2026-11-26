//! What `max` and `min` mean on a float, written once for both widths.
//!
//! The rest of float arithmetic is IEEE 754's at the type's own width. These
//! two are not IEEE operations, so the choice is written here: the first
//! operand unless the second is strictly greater (smaller). `(max 0.0 -0.0)`
//! is therefore `0.0` and `(max -0.0 0.0)` is `-0.0`, and a `NaN` first
//! operand is the answer while a `NaN` second one is not.

use num_traits::Float;

pub fn max<F: Float>(a: F, b: F) -> F {
    if b > a { b } else { a }
}

pub fn min<F: Float>(a: F, b: F) -> F {
    if b < a { b } else { a }
}
