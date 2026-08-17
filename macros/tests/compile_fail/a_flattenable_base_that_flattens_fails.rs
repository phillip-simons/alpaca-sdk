//! Flattening does not chain, and the refusal is what says so.
//!
//! `Middle` holds an `Inner` and is itself a base. The helper it emits is built
//! from its own optional fields, so anything flattening `Middle` would get
//! `feed` and not `limit` — a wrapper missing setters, with nothing anywhere to
//! say which. That is the silence `flatten` exists to delete, so the derive
//! refuses rather than generating half of what was asked for.
//!
//! The wrapper that would suffer it is deliberately absent: it would add a
//! second, cascading "cannot find macro" to this file's `.stderr` and nothing
//! to what the case asserts.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Inner {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
#[setters(flattenable)]
pub struct Middle {
    /// The inner base, whose setters would not reach a wrapper of `Middle`.
    #[setters(flatten)]
    pub inner: Inner,
    /// A filter of its own, whose setter would.
    pub feed: Option<u32>,
}

fn main() {}
