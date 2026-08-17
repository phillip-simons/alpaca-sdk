use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base<T> {
    /// The delegates would carry `T` to every wrapper that flattens this.
    pub limit: Option<T>,
}

fn main() {}
