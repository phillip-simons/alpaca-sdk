use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Documenting a setter and declaring there is none.
    #[setters(skip = "a constructor holds the name", doc = "Sets the symbol.")]
    pub symbol: Option<String>,
}

fn main() {}
