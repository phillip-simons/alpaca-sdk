use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy, or possibly sell.
    #[wire = "buy"]
    #[wire = "sell"]
    Buy,
}

fn main() {}
