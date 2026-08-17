//! A base carrying generic arguments has a name, and still cannot be flattened.
//!
//! The sibling case uses a tuple, which has no name at all. This one does — the
//! helper is named after the last path segment — but the helper takes a bare
//! `$wrapper:ident` and transcribes the base's field types verbatim, so there is
//! nowhere for `<u32>` to go. Both shapes share a refusal, and this is the one
//! that makes its wording earn the "with no generic arguments" clause.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Base<T> {
    /// Caps the total number of items returned.
    pub limit: Option<T>,
}

#[derive(Setters)]
pub struct Request {
    /// A base whose parameters the helper could not carry.
    #[setters(flatten)]
    pub base: Base<u32>,
}

fn main() {}
