use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// A type with no plain name, so no helper macro to look for.
    #[setters(flatten)]
    pub base: (Base, u32),
}

fn main() {}
