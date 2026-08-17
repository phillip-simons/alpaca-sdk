use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub struct Side {
    /// Buy.
    pub buy: bool,
}

fn main() {}
