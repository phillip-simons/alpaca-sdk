use alpaca_sdk_macros::Setters;

/// `into` written one line too high applies to nothing.
#[derive(Setters)]
#[setters(into)]
pub struct Request {
    /// A name.
    pub name: Option<String>,
}

fn main() {}
