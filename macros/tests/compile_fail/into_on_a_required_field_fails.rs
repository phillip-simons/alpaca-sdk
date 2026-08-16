use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Configures a setter that is never generated.
    #[setters(into)]
    pub symbol: String,
}

fn main() {}
