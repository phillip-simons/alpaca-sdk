use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Buy.
    #[serde(rename = "b")]
    #[wire = "buy"]
    Buy,
}

fn main() {}
