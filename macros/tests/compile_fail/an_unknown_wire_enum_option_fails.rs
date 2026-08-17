use alpaca_sdk_macros::wire_enum;

#[wire_enum(ordered)]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
}

fn main() {}
