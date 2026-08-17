use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    #[wire = "buy"]
    Buy,
}

fn main() {}
