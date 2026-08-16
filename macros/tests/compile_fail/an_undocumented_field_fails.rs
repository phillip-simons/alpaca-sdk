use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request {
    pub limit: Option<u32>,
}

fn main() {}
