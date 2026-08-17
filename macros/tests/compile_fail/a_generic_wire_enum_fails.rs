use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side<T> {
    /// Buy.
    #[wire = "buy"]
    Buy,
}

fn main() {}
