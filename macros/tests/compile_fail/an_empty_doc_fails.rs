use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Documented on the field, and blanked on the setter.
    #[setters(doc = "  ")]
    pub limit: Option<u32>,
}

fn main() {}
