use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// A required field has no setter to skip.
    #[setters(skip = "a constructor holds the name")]
    pub symbol: String,
}

fn main() {}
