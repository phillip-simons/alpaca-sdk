use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Asking for a loose setter and for no setter at once.
    #[setters(into, skip = "a constructor holds the name")]
    pub symbol: Option<String>,
}

fn main() {}
