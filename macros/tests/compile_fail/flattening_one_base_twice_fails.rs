//! Two fields flattening one base ask for two of every one of its setters.
//!
//! This is the only delegate collision the derive can see for itself: both
//! fields are in the struct in front of it. Left to rustc it is `E0592` spanned
//! at `Base`'s own `#[derive(Setters)]` — the one item in the program that is
//! correct — so the derive refuses first, at the second `flatten`.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// The primary window.
    #[setters(flatten)]
    pub base: Base,
    /// A second one, which would want a second `limit`.
    #[setters(flatten)]
    pub comparison: Base,
}

fn main() {}
