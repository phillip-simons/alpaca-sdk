use alpaca_sdk_macros::wire_enum;

#[wire_enum(sorted)]
pub enum Side {
    /// Sell.
    #[wire = "sell"]
    Sell,
    /// Buy.
    #[wire = "buy"]
    Buy,
}

fn main() {}
