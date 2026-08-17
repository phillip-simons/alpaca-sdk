use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// Configuring a setter this field does not get.
    #[setters(flatten, into)]
    pub base: Base,
}

fn main() {}
