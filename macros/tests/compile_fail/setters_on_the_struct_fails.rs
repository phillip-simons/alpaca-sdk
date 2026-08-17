use alpaca_sdk_macros::Setters;

/// `into` written one line too high configures nothing.
///
/// The struct takes exactly one option now — `flattenable` — so the refusal
/// names it rather than saying there is no whole-type option at all. The typo
/// this case is about is still a typo, and still loud.
#[derive(Setters)]
#[setters(into)]
pub struct Request {
    /// A name.
    pub name: Option<String>,
}

fn main() {}
