//! The pass case: everything `wire_enum` accepts, on one enum.
//!
//! Here rather than in the SDK's integration tests because this compiles as its
//! own crate with nothing else in it, so it pins the grammar alone — a change
//! that broke `#[wire = "…"]` would fail here whatever else was going on.
//!
//! The SDK's `wire_tests.rs` covers the *behaviour* of the generated impls
//! against both wire formats. This covers what the attribute will take.
//!
//! `deny(warnings)` is the point of the `#[deprecated]` variant below. The
//! generated `as_str` and `From` arms name every variant, so without an
//! `allow(deprecated)` on those impls the macro's own expansion emits warnings
//! the author never wrote — and `alpaca-sdk` builds under `-D warnings`.

#![deny(warnings)]

use std::str::FromStr as _;

use alpaca_sdk_macros::wire_enum;

/// The tape a trade printed on.
///
/// The wire values are the single-letter codes the data API sends, which is
/// why none of them resembles its variant.
#[wire_enum(sorted)]
pub enum Exchange {
    /// NYSE American.
    ///
    /// A second paragraph, so a multi-line doc comment is exercised rather
    /// than assumed to work.
    #[wire = "A"]
    NyseAmerican,
    /// NASDAQ.
    #[wire = "Q"]
    Nasdaq,
    /// NYSE.
    #[wire = "n"]
    Nyse,
    /// A venue Alpaca has stopped reporting.
    ///
    /// Carried anyway, because deleting a value the API still sends turns a
    /// working match arm into an `Unknown`.
    #[deprecated = "Alpaca no longer reports this tape code"]
    #[wire = "z"]
    Retired,
}

/// `#[cfg]` on a variant, in both directions and doubled.
///
/// The gate is copied from the variant's declaration onto the `WIRE_VALUES`
/// element and the three `match` arms, so all four agree in either state.
/// `all()` is always true and `any()` never is, so both directions compile here
/// without a feature having to be declared for the trybuild crate.
///
/// This is what a wrong gate fails against. Replicating an unrelated gate, or
/// replicating only the first of several, leaves every other test in this
/// workspace green — the assertion in `main` is the only thing that catches it.
#[wire_enum]
pub enum Gated {
    /// Always present.
    #[wire = "kept"]
    Kept,
    /// Present: `all()` is true, and the gate reaches all four sites.
    #[cfg(all())]
    #[wire = "on"]
    On,
    /// Absent: `any()` is never true.
    #[cfg(any())]
    #[wire = "off"]
    Off,
    /// Absent, and only because the *second* gate is read too.
    #[cfg(all())]
    #[cfg(any())]
    #[wire = "both"]
    Both,
}

/// The pattern supporting `#[cfg]` exists to permit: one variant under two
/// opposite gates.
///
/// Both duplicate checks skip gated variants for this. Checking them would
/// refuse this with "`Primary` is already a variant of this enum", which is
/// false in either build — exactly one `Primary` exists in each. A real
/// collision between two variants that are both live is still caught, by rustc:
/// `E0428` for the name, `unreachable_patterns` for the value.
///
/// Without the skip this file does not compile, which is the point of it being
/// here rather than in a unit test.
#[wire_enum]
pub enum Exclusive {
    /// The value this build carries.
    #[cfg(all())]
    #[wire = "primary"]
    Primary,
    /// The value the other build carries, under the same name.
    #[cfg(any())]
    #[wire = "secondary"]
    Primary,
}

/// A `#[deprecated]` on the *enum* rather than on a variant.
///
/// Every one of the seven generated impls names the type, so this needs
/// `#[allow(deprecated)]` on all seven — where the `Retired` variant above only
/// exercises the three that name variants. Without both, `deny(warnings)` at
/// the top of this file fails the case.
#[deprecated = "Alpaca folded this into the tape codes"]
#[wire_enum]
pub enum LegacyVenue {
    /// The only value it ever carried.
    #[wire = "legacy"]
    Legacy,
}

/// Names that collide with the generated code's own identifiers.
///
/// The emitted serde impls introduce a type parameter per method and a visitor
/// struct. Compiled against the old spellings, three of the four broke:
///
/// | Enum named | Was |
/// | --- | --- |
/// | `D` | `E0401: can't use generic parameters from outer item` |
/// | `E` | two `E0277`s (`From<&str>` and `From<String>`), and an `E0308` |
/// | `WireVisitor` | `E0308: mismatched types`, and two `E0277`s |
/// | `S` | fine — `serialize<S>`'s body never names the enum |
///
/// All four are `__`-prefixed anyway: `S` is one edit to that impl away from
/// joining the other three, and a rule with an exception is not a rule. These
/// four enums are what keeps the rename from being reverted as cosmetic —
/// three of them fail without it.
///
/// The prefix narrows the collision rather than closing it: an enum named
/// `__WireVisitor` still breaks. `__` is reserved by convention, which is the
/// whole reason it is the prefix, but nothing here enforces that.
mod collisions {
    use alpaca_sdk_macros::wire_enum;

    /// Shadowed `fn deserialize<D>`.
    #[wire_enum]
    pub enum D {
        /// One.
        #[wire = "d"]
        One,
    }

    /// Shadowed the visitor methods' `E: serde::de::Error`.
    #[wire_enum]
    pub enum E {
        /// One.
        #[wire = "e"]
        One,
    }

    /// Shadowed `fn serialize<S>`.
    #[wire_enum]
    pub enum S {
        /// One.
        #[wire = "s"]
        One,
    }

    /// Shadowed the visitor struct itself.
    #[wire_enum]
    pub enum WireVisitor {
        /// One.
        #[wire = "w"]
        One,
    }
}

/// Proof the generated serde impls exist, without pulling in a format crate to
/// exercise them — `wire_tests.rs` in the SDK does that under JSON and msgpack.
fn assert_serde<T: serde::Serialize + serde::de::DeserializeOwned>() {}

fn main() {
    assert_serde::<Exchange>();

    assert_eq!(Exchange::WIRE_VALUES, &["A", "Q", "n", "z"]);

    // The gate holds in both directions, across all four generated sites.
    assert_eq!(Gated::WIRE_VALUES, &["kept", "on"]);
    assert_eq!(Gated::On.as_str(), "on");
    assert_eq!(Gated::from("on"), Gated::On);
    assert_eq!(Gated::from("off".to_owned()), Gated::Unknown("off".to_owned()));
    assert_eq!(Gated::from("both"), Gated::Unknown("both".to_owned()));

    // One name, two gates: exactly one survives, and the checks let it.
    assert_eq!(Exclusive::WIRE_VALUES, &["primary"]);
    assert_eq!(Exclusive::Primary.as_str(), "primary");
    assert_eq!(Exchange::Nasdaq.as_str(), "Q");
    assert_eq!(Exchange::Nasdaq.to_string(), "Q");
    assert!(!Exchange::Nasdaq.is_unknown());

    assert_eq!(Exchange::from("n"), Exchange::Nyse);
    assert_eq!(Exchange::from("A".to_owned()), Exchange::NyseAmerican);
    assert_eq!(Exchange::from_str("Q").unwrap(), Exchange::Nasdaq);

    let unrecognized = Exchange::from("Z");
    assert!(unrecognized.is_unknown());
    assert_eq!(unrecognized.as_str(), "Z");
}
