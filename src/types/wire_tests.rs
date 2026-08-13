//! Behavior of the `wire_enum!` macro, pinned against both wire formats.
//!
//! The live market data stream is msgpack and the REST APIs are JSON, so every
//! generated enum has to behave identically under `serde_json` and `rmp-serde`.

use std::str::FromStr as _;

use serde::{Deserialize, Serialize};

use crate::types::wire::wire_enum;

wire_enum! {
    /// A stand-in with the same shape as the generated enums.
    pub enum Side {
        /// Buy.
        Buy => "buy",
        /// Sell.
        Sell => "sell",
    }
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

#[test]
fn enums_are_usable_as_map_keys() {
    // Hash + Eq are derived; handler maps in the streaming layer depend on it.
    let mut counts = std::collections::HashMap::new();
    counts.insert(Side::Buy, 1);
    counts.insert(Side::Unknown("x".to_owned()), 2);

    assert_eq!(counts.get(&Side::Buy), Some(&1));
    assert_eq!(counts.get(&Side::from("x")), Some(&2));
}
