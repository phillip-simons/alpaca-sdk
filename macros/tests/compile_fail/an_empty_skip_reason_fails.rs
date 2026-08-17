use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// A skip with nothing to say for itself.
    #[setters(skip = "   ")]
    pub limit: Option<u32>,
}

fn main() {}
