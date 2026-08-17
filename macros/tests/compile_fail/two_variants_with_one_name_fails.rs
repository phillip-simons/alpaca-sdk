use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
    /// Buy again, apparently.
    #[wire = "sell"]
    Buy,
}

fn main() {}
