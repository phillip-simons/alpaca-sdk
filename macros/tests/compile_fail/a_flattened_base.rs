//! The pass case for flattening: a wrapper that names none of the base's fields.
//!
//! This file is the direct evidence for the claim the derive makes about
//! itself. `Wrapper` writes `#[setters(flatten)]` once and nothing else — no
//! field names, no types, no doc comments — and `main` then calls `start`,
//! `limit` and `name` on it. If those methods exist, they were read off `Base`,
//! because there is nowhere else in this file they could have come from.
//!
//! It also pins the two properties of the base's own attributes that a delegate
//! has to inherit rather than restate: `name` takes `impl Into<String>`, so
//! `"desk-7"` compiles through the wrapper; and `skipped` has a `skip`, so no
//! delegate for it exists to collide with the constructor holding that name.

use alpaca_sdk_macros::Setters;

#[derive(Default, Setters)]
#[setters(flattenable)]
pub struct Base {
    /// Restricts the window to `start` onwards.
    pub start: Option<u32>,
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
    /// Takes anything that converts.
    #[setters(into)]
    pub name: Option<String>,
    /// A required field is a constructor argument, so it gets no setter.
    pub required: u32,
    /// Something else already holds this name.
    #[setters(skip = "`Wrapper::skipped(u32)` is a constructor")]
    pub skipped: Option<u32>,
}

#[derive(Default, Setters)]
pub struct Wrapper {
    /// The shared filters.
    #[setters(flatten)]
    pub base: Base,
    /// A filter of the wrapper's own, to prove the two impls coexist.
    pub feed: Option<u32>,
}

impl Wrapper {
    /// The constructor holding the name `skipped`, which is why no delegate has it.
    pub fn skipped(_skipped: u32) -> Self {
        Self::default()
    }
}

fn main() {
    let wrapper = Wrapper::default()
        .start(1)
        .limit(50)
        .name("desk-7")
        .feed(7);

    assert_eq!(wrapper.base.start, Some(1));
    assert_eq!(wrapper.base.limit, Some(50));
    assert_eq!(wrapper.base.name.as_deref(), Some("desk-7"));
    assert_eq!(wrapper.base.required, 0);
    assert_eq!(wrapper.base.skipped, None);
    assert_eq!(wrapper.feed, Some(7));

    // The base keeps its own setters; flattening adds a second way to reach
    // them rather than moving them.
    assert_eq!(Base::default().limit(1).limit, Some(1));
}
