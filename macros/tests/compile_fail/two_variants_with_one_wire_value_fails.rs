use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
    /// Also buy, apparently.
    #[wire = "buy"]
    Sell,
}

fn main() {}
