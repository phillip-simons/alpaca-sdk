use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    /// Caps the response.
    #[setters(rename = "max")]
    pub limit: Option<u32>,
}

fn main() {}
