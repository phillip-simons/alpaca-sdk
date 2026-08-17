use alpaca_sdk_macros::wire_enum;

#[wire_enum]
#[wire = "side"]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
}

fn main() {}
