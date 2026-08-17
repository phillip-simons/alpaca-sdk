//! rustc's error, not the derive's: the helper macro was never emitted.
//!
//! The message names the macro the base would have produced, and points at the
//! field asking for it. Pinned here so the two failures a caller can actually
//! hit are reviewed rather than merely emitted.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// `Base` is not marked `#[setters(flattenable)]`.
    #[setters(flatten)]
    pub base: Base,
}

fn main() {}
