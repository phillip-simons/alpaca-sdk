use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// Documenting a setter that is one method per field of the base.
    #[setters(flatten, doc = "Sets the shared filters.")]
    pub base: Base,
}

fn main() {}
