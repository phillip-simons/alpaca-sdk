use alpaca_sdk_macros::Setters;

#[derive(Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}

#[derive(Setters)]
pub struct Request<T> {
    /// The helper macro takes a bare `$wrapper:ident`, which loses the `<T>`.
    #[setters(flatten)]
    pub base: Base,
    /// Something for the parameter to be used by.
    pub extra: Option<T>,
}

fn main() {}
