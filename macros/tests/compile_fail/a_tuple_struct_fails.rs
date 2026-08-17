use alpaca_sdk_macros::Setters;

#[derive(Setters)]
pub struct Request(Option<u32>, Option<String>);

fn main() {}
