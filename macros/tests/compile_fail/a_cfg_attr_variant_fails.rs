use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
    /// Sell, but only sometimes.
    #[cfg_attr(all(), cfg(feature = "selling"))]
    #[wire = "sell"]
    Sell,
}

fn main() {}
