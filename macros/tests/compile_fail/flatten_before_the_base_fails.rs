//! rustc's error again, and the likelier of the two: `macro_rules!` is
//! textually scoped, so a base declared *after* the wrapper is out of scope at
//! the wrapper even though the two are in one module.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// `Base` is declared below, which is too late.
    #[setters(flatten)]
    pub base: Base,
}

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

fn main() {}
