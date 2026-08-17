use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// `into` is a flag; the type it converts to is the field's own.
    #[setters(into = "String")]
    pub symbol: Option<String>,
}

fn main() {}
