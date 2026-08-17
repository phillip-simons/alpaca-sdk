use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request {
    /// Asking to delegate the base's setters and for no setters at once.
    #[setters(flatten, skip = "a constructor holds the name")]
    pub base: Base,
}

fn main() {}
