//! Behavior of the `wire_enum` attribute, pinned against both wire formats.
//!
//! The live market data stream is msgpack and the REST APIs are JSON, so every
//! generated enum has to behave identically under `serde_json` and `rmp-serde`.
//!
//! The attribute's *refusals* are covered by the compile-fail suite in
//! `macros/tests/`, which is the only place they can be: each one is a
//! `compile_error!`. This file covers what the generated code does once it
//! compiles, and it is also what keeps the grammar honest — a change to the
//! attribute that broke `#[wire = "…"]` fails here.

use std::str::FromStr as _;

use serde::{Deserialize, Serialize};

use crate::types::wire::wire_enum;

/// A stand-in with the same shape as the generated enums.
#[wire_enum]
pub enum Side {
    /// Buy.
    #[wire = "buy"]
    Buy,
    /// Sell.
    #[wire = "sell"]
    Sell,
}

/// A stand-in for the enums whose order is deliberate rather than alphabetical.
///
/// `WIRE_VALUES` is the wire vocabulary as written, not a sorted view of it,
/// and this is where that is pinned. `ActivityType` in the real crate leads
/// with `Fill` for the same reason.
#[wire_enum]
pub enum Activity {
    /// A fill, first because it is the one anybody reads for.
    #[wire = "FILL"]
    Fill,
    /// A dividend.
    #[wire = "DIV"]
    Dividend,
    /// An ACH transfer.
    #[wire = "ACH"]
    Ach,
}

/// A stand-in for the enums that opt into `sorted`, shaped like the
/// single-letter tape codes the data API sends.
///
/// Byte order, not case-insensitive order — the uppercase codes sort before the
/// lowercase one, and an enum that claimed otherwise would not compile.
#[wire_enum(sorted)]
pub enum TapeCode {
    /// NYSE American.
    #[wire = "A"]
    NyseAmerican,
    /// NASDAQ.
    #[wire = "Q"]
    Nasdaq,
    /// NYSE.
    #[wire = "n"]
    Nyse,
}

/// A stand-in for the three enums that carry an empty wire value.
///
/// `DocumentType`, `BankAccountType` and `AssetExchange` each have one,
/// because Alpaca's schemas list the empty string as an enum value. This is
/// a stand-in shaped like them rather than any one of them.
/// The attribute deliberately does not refuse it, and this is what says so at
/// runtime: `""` is a known value, not an `Unknown`.
#[wire_enum]
pub enum AccountKind {
    /// `CHECKING`
    #[wire = "CHECKING"]
    Checking,
    /// The empty value.
    #[wire = ""]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Envelope {
    side: Side,
}

#[test]
fn known_values_round_trip_through_json() {
    let json = r#"{"side":"buy"}"#;
    let decoded: Envelope = serde_json::from_str(json).unwrap();

    assert_eq!(decoded.side, Side::Buy);
    assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
}

#[test]
fn unknown_values_deserialize_instead_of_failing() {
    // Alpaca adds enum values without warning, so a payload carrying one must
    // stay usable rather than failing the whole decode.
    let decoded: Envelope = serde_json::from_str(r#"{"side":"short_exempt"}"#).unwrap();

    assert_eq!(decoded.side, Side::Unknown("short_exempt".to_owned()));
    assert!(decoded.side.is_unknown());
    assert_eq!(decoded.side.as_str(), "short_exempt");
}

#[test]
fn unknown_values_re_serialize_to_the_original_string() {
    let json = r#"{"side":"short_exempt"}"#;
    let decoded: Envelope = serde_json::from_str(json).unwrap();

    // Round-tripping must not corrupt a value we did not recognize.
    assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
}

#[test]
fn known_values_round_trip_through_msgpack() {
    let encoded = rmp_serde::to_vec_named(&Envelope { side: Side::Sell }).unwrap();
    let decoded: Envelope = rmp_serde::from_slice(&encoded).unwrap();

    assert_eq!(decoded.side, Side::Sell);
}

#[test]
fn unknown_values_round_trip_through_msgpack() {
    let original = Envelope {
        side: Side::Unknown("auction".to_owned()),
    };

    let encoded = rmp_serde::to_vec_named(&original).unwrap();
    let decoded: Envelope = rmp_serde::from_slice(&encoded).unwrap();

    assert_eq!(decoded, original);
}

#[test]
fn msgpack_compact_encoding_also_decodes() {
    // The market data stream sends compact (non-named) msgpack maps.
    let encoded = rmp_serde::to_vec(&Envelope { side: Side::Buy }).unwrap();
    let decoded: Envelope = rmp_serde::from_slice(&encoded).unwrap();

    assert_eq!(decoded.side, Side::Buy);
}

#[test]
fn conversions_cover_str_string_and_fromstr() {
    assert_eq!(Side::from("buy"), Side::Buy);
    assert_eq!(Side::from("buy".to_owned()), Side::Buy);
    assert_eq!(Side::from_str("sell").unwrap(), Side::Sell);
    assert_eq!(
        Side::from("nope".to_owned()),
        Side::Unknown("nope".to_owned())
    );
}

#[test]
fn display_matches_the_wire_value() {
    assert_eq!(Side::Buy.to_string(), "buy");
    assert_eq!(Side::Unknown("weird".to_owned()).to_string(), "weird");
}

#[test]
fn wire_values_lists_only_known_variants() {
    assert_eq!(Side::WIRE_VALUES, &["buy", "sell"]);
}

/// `sorted` is a claim about the source, not a transform of the output. An
/// enum ordered by significance keeps that order in `WIRE_VALUES`.
#[test]
fn wire_values_keeps_declaration_order_rather_than_sorting() {
    assert_eq!(Activity::WIRE_VALUES, &["FILL", "DIV", "ACH"]);
}

/// The opt-in path, exercised at runtime rather than only in trybuild. An enum
/// that carries `sorted` is otherwise an ordinary wire enum.
#[test]
fn a_sorted_enum_is_an_ordinary_wire_enum() {
    assert_eq!(TapeCode::WIRE_VALUES, &["A", "Q", "n"]);
    assert_eq!(TapeCode::from("n"), TapeCode::Nyse);
    assert_eq!(TapeCode::NyseAmerican.as_str(), "A");
    assert_eq!(serde_json::to_string(&TapeCode::Nasdaq).unwrap(), r#""Q""#);
    assert_eq!(
        serde_json::from_str::<TapeCode>(r#""Z""#).unwrap(),
        TapeCode::Unknown("Z".to_owned())
    );
}

/// The empty string is a value Alpaca sends, so it round-trips as one. Were
/// this to start resolving to `Unknown("")`, three enums would quietly lose a
/// variant — which is why the attribute has no "non-empty" check to trip over.
#[test]
fn an_empty_wire_value_is_a_value_and_not_an_unknown() {
    let decoded: AccountKind = serde_json::from_str(r#""""#).unwrap();

    assert_eq!(decoded, AccountKind::None);
    assert!(!decoded.is_unknown());
    assert_eq!(decoded.as_str(), "");
    assert_eq!(serde_json::to_string(&decoded).unwrap(), r#""""#);
    assert_eq!(AccountKind::from(String::new()), AccountKind::None);
}

#[test]
fn enums_are_usable_as_map_keys() {
    // Hash + Eq are derived; handler maps in the streaming layer depend on it.
    let mut counts = std::collections::HashMap::new();
    counts.insert(Side::Buy, 1);
    counts.insert(Side::Unknown("x".to_owned()), 2);

    assert_eq!(counts.get(&Side::Buy), Some(&1));
    assert_eq!(counts.get(&Side::from("x")), Some(&2));
}
