use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy, at a price.
    #[wire = "buy"]
    Buy(String),
}

fn main() {}
