use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// A base the delegates would have nothing to write through to.
    #[setters(flatten)]
    pub base: Option<Base>,
}

fn main() {}
