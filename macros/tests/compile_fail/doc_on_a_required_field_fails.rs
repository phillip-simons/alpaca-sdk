use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Documents a setter that is never generated.
    #[setters(doc = "Sets the symbol.")]
    pub symbol: String,
}

fn main() {}
