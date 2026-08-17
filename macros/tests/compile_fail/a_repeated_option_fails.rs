use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Two reasons for one absence, and only the last would have been used.
    #[setters(skip = "a constructor holds the name")]
    #[setters(skip = "set with the field beside it")]
    pub symbol: Option<String>,
}

fn main() {}
