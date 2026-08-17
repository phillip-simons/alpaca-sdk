use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy = 3,
}

fn main() {}
