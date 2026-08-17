use alpaca_sdk_macros::Setters;

/// `flatten` configures a field; the struct-level word is `flattenable`.
#[derive(Setters)]
#[setters(flatten)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

fn main() {}
