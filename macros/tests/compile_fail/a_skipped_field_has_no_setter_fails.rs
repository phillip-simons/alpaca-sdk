//! `skip` must generate *nothing*, not a setter that is merely undocumented.
//!
//! This is the assertion behind every `#[setters(skip = "…")]` in the SDK: the
//! ones on `OrderRequest::qty` and `notional` exist so that "both at once" — a
//! state `OrderAmount` was designed to make unrepresentable, and that
//! `validate` therefore does not check — cannot be reached through a method the
//! API offers. If `skip` ever silently generated a setter anyway, every one of
//! those decisions would be undone at once and nothing else would notice.

use alpaca_sdk_macros::Setters;

#[derive(Default, Setters)]
pub struct Request {
    /// Set with the field it only makes sense beside.
    #[setters(skip = "written by `pair(a, b)`, which sets both")]
    pub qty: Option<u32>,
    /// The other half.
    #[setters(skip = "the sibling of `qty`")]
    pub notional: Option<u32>,
}

fn main() {
    let _ = Request::default().qty(1).notional(2);
}
