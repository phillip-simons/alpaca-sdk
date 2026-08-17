//! rustc's error, and the third way flattening can fail.
//!
//! A wrapper's own optional field sharing a name with one of the base's asks
//! for two `pub fn limit` in one inherent impl. The derive cannot see it — it
//! reads one struct at a time and does not know what the base's fields are
//! called — so this is `E0592`, spanned at the two `#[derive(Setters)]`
//! attributes rather than at either field.
//!
//! Pinned for the reason the other two rustc-owned cases are: the message is
//! what a caller meets, and "duplicate definitions with name `limit`" pointing
//! at a derive is not obviously "your wrapper and its base both have a `limit`".
//! Adding a field to a wrapper of `TimeseriesRequest` is the way to reach it.

use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// The shared filters, which already carry a `limit`.
    #[setters(flatten)]
    pub base: Base,
    /// A filter of the wrapper's own, sharing that name.
    pub limit: Option<u32>,
}

fn main() {}
