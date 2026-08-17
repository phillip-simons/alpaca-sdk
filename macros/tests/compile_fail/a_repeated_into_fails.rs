use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Twice is not more into than once, and last-wins would hide the typo.
    #[setters(into, into)]
    pub symbol: Option<String>,
}

fn main() {}
