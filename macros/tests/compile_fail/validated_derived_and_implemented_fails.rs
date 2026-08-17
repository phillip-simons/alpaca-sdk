//! Deriving `Validated` and hand-writing it are mutually exclusive.
//!
//! This is the load-bearing property of the whole design, and it is not
//! something the derive enforces — coherence does. The derive emits
//! `impl Validated for T {}`, so a second impl is `E0119`, and a type therefore
//! cannot have both a real validator and a no-op that shadows it.
//!
//! Pinned here because the property is invisible in the derive's own source: it
//! comes from what the expansion *is*, and a change that made the derive emit
//! something else — an inherent method, say — would lose it without touching a
//! line that mentions it.

use alpaca_sdk_macros::Validated;

trait Validated {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Validated)]
pub struct Request {
    pub qty: Option<u32>,
}

impl Validated for Request {
    fn validate(&self) -> Result<(), String> {
        Err("nope".to_owned())
    }
}

fn main() {}
