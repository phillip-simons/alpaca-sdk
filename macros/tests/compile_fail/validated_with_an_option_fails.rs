//! `#[validated(…)]` is refused rather than ignored.
//!
//! The attribute is declared by the derive purely so this message is the one a
//! caller sees. Ignoring it would be the worse outcome: `#[validated(nested)]`
//! reads as configuration, and a caller who believes it walked their nested
//! request types would have exactly the silent skip the trait exists to remove.

use alpaca_sdk_macros::Validated;

// Stands in for `alpaca_sdk::Validated`, which this crate cannot depend on: it
// is the crate that depends on this one. The derive emits an unqualified
// `impl Validated for T {}`, so any trait of that name with a defaulted method
// satisfies it — which is also what pins that the expansion stays unqualified.
trait Validated {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Validated)]
#[validated(nested)]
pub struct Request {
    pub contact: Option<u32>,
}

fn main() {}
