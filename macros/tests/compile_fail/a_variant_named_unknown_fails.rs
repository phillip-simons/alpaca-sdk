use alpaca_sdk_macros::wire_enum;

#[wire_enum]
pub enum Side {
    /// Not a side this SDK names.
    #[wire = "unknown"]
    Unknown,
}

fn main() {}
