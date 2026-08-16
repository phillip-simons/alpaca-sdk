//! The pass case: every attribute the derive accepts, on one struct.
//!
//! Here rather than in the SDK's integration tests because this compiles as its
//! own crate with nothing else in it, so it pins the attribute grammar alone —
//! a change that broke `into` would fail here whatever else was going on.

use alpaca_sdk_macros::Setters;

#[derive(Default, Setters)]
pub struct Request {
    /// Takes its type exactly.
    pub limit: Option<u32>,
    /// Takes anything that converts.
    #[setters(into)]
    pub name: Option<String>,
    /// The field's own documentation is not what this setter should say.
    ///
    /// `into` beside it, so an array works where the field holds a `Vec`.
    #[setters(into, doc = "Restricts the response to `tags`.")]
    pub tags: Option<Vec<String>>,
    /// A required field is a constructor argument, so it gets no setter.
    pub required: u32,
    /// Something else already holds this name.
    #[setters(skip = "`Request::since(NaiveDate)` is a constructor")]
    pub since: Option<u32>,
}

impl Request {
    /// The constructor holding the name `since`.
    pub fn since(_since: u32) -> Self {
        Self::default()
    }
}

fn main() {
    let request = Request::default()
        .limit(50)
        .name("desk-7")
        .tags(["a", "b"].map(String::from));

    assert_eq!(request.limit, Some(50));
    assert_eq!(request.name.as_deref(), Some("desk-7"));
    assert_eq!(request.tags.as_deref(), Some(&["a".to_owned(), "b".to_owned()][..]));
    assert_eq!(request.required, 0);
    assert_eq!(request.since, None);
}
