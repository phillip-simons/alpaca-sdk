//! The serde codecs every model is built on: money, time, and the four wire
//! quirks Alpaca sends that a derived `Deserialize` would reject.
//!
//! These are the highest-leverage tests in the suite. A route test proves one
//! route works; a codec bug is wrong on every route that carries a price, a
//! timestamp, or an optional field — and wrong quietly, because the failure mode
//! is a rounded number rather than an error.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------- money

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Money {
    #[serde(with = "alpaca_sdk::types::decimal")]
    required: Decimal,
    #[serde(with = "alpaca_sdk::types::option_decimal", default)]
    optional: Option<Decimal>,
}

fn money(json: &str) -> Money {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"))
}

/// Alpaca sends the same field as a string on one route and a number on
/// another, so both have to decode into the same value.
#[test]
fn a_price_decodes_from_a_string_or_a_number() {
    assert_eq!(
        money(r#"{"required":"1.5","optional":"2.25"}"#),
        money(r#"{"required":1.5,"optional":2.25}"#)
    );
}

/// The reason this is `from_f64` and not `from_f64_retain`: the retaining
/// conversion keeps the float's full binary expansion, so 183.42 decodes as
/// 183.41999999999998749444785057 and every downstream comparison fails.
#[test]
fn a_float_decodes_to_the_number_the_wire_meant() {
    assert_eq!(
        money(r#"{"required":183.42}"#).required.to_string(),
        "183.42"
    );
}

#[test]
fn integers_decode_without_going_through_a_float() {
    assert_eq!(money(r#"{"required":7}"#).required, Decimal::from(7));
    assert_eq!(money(r#"{"required":-7}"#).required, Decimal::from(-7));
    // Larger than f64 can represent exactly; a float path would lose the tail.
    assert_eq!(
        money(r#"{"required":9007199254740993}"#)
            .required
            .to_string(),
        "9007199254740993"
    );
}

/// Whitespace is trimmed rather than rejected: a padded string is still a
/// number, and failing the whole response over one is the worse outcome.
#[test]
fn a_padded_string_still_parses() {
    assert_eq!(
        money(r#"{"required":"  1.5  "}"#).required,
        Decimal::new(15, 1)
    );
}

#[test]
fn an_absent_or_null_optional_is_none() {
    assert_eq!(money(r#"{"required":"1"}"#).optional, None);
    assert_eq!(money(r#"{"required":"1","optional":null}"#).optional, None);
}

#[test]
fn a_price_that_is_not_a_number_is_an_error_not_a_zero() {
    let err = serde_json::from_str::<Money>(r#"{"required":"string"}"#).unwrap_err();
    assert!(err.to_string().contains("string"), "{err}");
    assert!(serde_json::from_str::<Money>(r#"{"required":true}"#).is_err());
}

/// Money always goes back out as a string. Alpaca accepts both, but a string
/// round-trips exactly and a JSON number does not.
#[test]
fn money_serializes_as_a_string_in_both_shapes() {
    let value = serde_json::to_value(Money {
        required: Decimal::new(15, 1),
        optional: Some(Decimal::new(-25, 2)),
    })
    .unwrap();

    assert_eq!(value["required"], "1.5");
    assert_eq!(value["optional"], "-0.25");
}

#[test]
fn a_none_price_serializes_as_null_not_as_a_missing_key() {
    let value = serde_json::to_value(Money {
        required: Decimal::ONE,
        optional: None,
    })
    .unwrap();

    assert!(value.get("optional").is_some());
    assert_eq!(value["optional"], serde_json::Value::Null);
}

// -------------------------------------------------------------------- time

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Times {
    #[serde(with = "alpaca_sdk::types::timestamp")]
    at: chrono::DateTime<chrono::Utc>,
    #[serde(with = "alpaca_sdk::types::option_timestamp", default)]
    maybe: Option<chrono::DateTime<chrono::Utc>>,
}

#[test]
fn a_timestamp_decodes_from_an_rfc_3339_string() {
    let times: Times = serde_json::from_str(
        r#"{"at":"2022-03-09T09:00:00.000059Z","maybe":"2022-03-09T09:00:00Z"}"#,
    )
    .unwrap();

    assert_eq!(times.at.timestamp(), 1_646_816_400);
    assert_eq!(times.at.timestamp_subsec_nanos(), 59_000);
    assert!(times.maybe.is_some());
}

#[test]
fn an_absent_or_null_timestamp_is_none() {
    let times: Times = serde_json::from_str(r#"{"at":"2022-03-09T09:00:00Z"}"#).unwrap();
    assert_eq!(times.maybe, None);

    let times: Times =
        serde_json::from_str(r#"{"at":"2022-03-09T09:00:00Z","maybe":null}"#).unwrap();
    assert_eq!(times.maybe, None);
}

#[test]
fn a_timestamp_round_trips_through_json() {
    let original: Times =
        serde_json::from_str(r#"{"at":"2022-03-09T09:00:00.000059Z","maybe":null}"#).unwrap();
    let text = serde_json::to_string(&original).unwrap();
    let again: Times = serde_json::from_str(&text).unwrap();

    assert_eq!(original, again);
}

/// The three msgpack timestamp encodings, which is what the live market data
/// stream actually sends. Nothing decodes extension type -1 out of the box —
/// not `DateTime`, not `String`, not `serde_json::Value` — so this codec is the
/// only thing standing between the stream and a decode failure on every frame.
#[test]
fn all_three_msgpack_timestamp_widths_decode() {
    use alpaca_sdk::types::timestamp::from_extension;

    // timestamp32: 4 bytes of seconds.
    let seconds = 1_646_816_400u32;
    let decoded = from_extension(-1, &seconds.to_be_bytes()).unwrap();
    assert_eq!(decoded.timestamp(), 1_646_816_400);
    assert_eq!(decoded.timestamp_subsec_nanos(), 0);

    // timestamp64: 30 bits of nanoseconds, then 34 bits of seconds.
    let packed = (59_000u64 << 34) | 1_646_816_400u64;
    let decoded = from_extension(-1, &packed.to_be_bytes()).unwrap();
    assert_eq!(decoded.timestamp(), 1_646_816_400);
    assert_eq!(decoded.timestamp_subsec_nanos(), 59_000);

    // timestamp96: 32 bits of nanoseconds, then 64 signed bits of seconds.
    let mut wide = Vec::new();
    wide.extend_from_slice(&59_000u32.to_be_bytes());
    wide.extend_from_slice(&1_646_816_400i64.to_be_bytes());
    let decoded = from_extension(-1, &wide).unwrap();
    assert_eq!(decoded.timestamp(), 1_646_816_400);
    assert_eq!(decoded.timestamp_subsec_nanos(), 59_000);
}

#[test]
fn a_msgpack_extension_that_is_not_a_timestamp_is_rejected() {
    use alpaca_sdk::types::timestamp::from_extension;

    // Right width, wrong extension type.
    let wrong_tag = from_extension(5, &1_646_816_400u32.to_be_bytes()).unwrap_err();
    assert!(wrong_tag.contains("extension type 5"), "{wrong_tag}");

    // Right extension type, a width the msgpack specification does not define.
    let wrong_width = from_extension(-1, &[0, 1, 2]).unwrap_err();
    assert!(!wrong_width.is_empty());
}

/// A timestamp before the epoch is a negative seconds value, which only the
/// 96-bit encoding can carry. Corporate actions reach back decades.
#[test]
fn a_pre_epoch_timestamp_decodes() {
    use alpaca_sdk::types::timestamp::from_extension;

    let mut wide = Vec::new();
    wide.extend_from_slice(&0u32.to_be_bytes());
    wide.extend_from_slice(&(-86_400i64).to_be_bytes());

    assert_eq!(from_extension(-1, &wide).unwrap().timestamp(), -86_400);
}

// ------------------------------------------------------------- wire quirks

/// Multi-leg order responses set `symbol`, `side` and four other fields to `""`
/// rather than omitting them, because those describe a single leg and an mleg
/// order has several. An empty string is not a value.
#[test]
fn an_empty_string_reads_as_absent() {
    #[derive(Deserialize)]
    struct Leg {
        #[serde(default, deserialize_with = "alpaca_sdk::types::empty_string_as_none")]
        symbol: Option<String>,
        #[serde(default, deserialize_with = "alpaca_sdk::types::empty_string_as_none")]
        id: Option<uuid::Uuid>,
    }

    let blank: Leg = serde_json::from_str(r#"{"symbol":"","id":""}"#).unwrap();
    assert_eq!(blank.symbol, None);
    assert_eq!(blank.id, None);

    let set: Leg =
        serde_json::from_str(r#"{"symbol":"AAPL","id":"61e69015-8549-4bfd-b9c3-01e75843f47d"}"#)
            .unwrap();
    assert_eq!(set.symbol.as_deref(), Some("AAPL"));
    assert!(set.id.is_some());

    // A malformed non-empty value is still an error: only `""` means absent.
    assert!(serde_json::from_str::<Leg>(r#"{"id":"not-a-uuid"}"#).is_err());
}

/// The trading account endpoint returns `"options_approved_level": "1"` and
/// `"daytrade_count": 0` in the same payload.
#[test]
fn an_integer_decodes_from_a_string_or_a_number() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Counts {
        #[serde(with = "alpaca_sdk::types::int")]
        required: i64,
        #[serde(with = "alpaca_sdk::types::option_int", default)]
        optional: Option<i64>,
    }

    let as_strings: Counts = serde_json::from_str(r#"{"required":"1","optional":"2"}"#).unwrap();
    let as_numbers: Counts = serde_json::from_str(r#"{"required":1,"optional":2}"#).unwrap();
    assert_eq!(as_strings, as_numbers);

    let absent: Counts = serde_json::from_str(r#"{"required":0}"#).unwrap();
    assert_eq!(absent.optional, None);

    // Integers go back out as numbers, not strings.
    let value = serde_json::to_value(as_numbers).unwrap();
    assert_eq!(value["required"], 1);

    assert!(serde_json::from_str::<Counts>(r#"{"required":"twelve"}"#).is_err());
}

/// A field the API declares as a list and sends as `null` when it is empty.
/// `#[serde(default)]` covers an *absent* field, not a present-and-null one —
/// the distinction that broke nine `Vec` fields at once.
#[test]
fn a_null_list_reads_as_an_empty_one() {
    #[derive(Deserialize)]
    struct Holder {
        #[serde(default, deserialize_with = "alpaca_sdk::types::null_as_default")]
        items: Vec<String>,
    }

    assert!(
        serde_json::from_str::<Holder>(r#"{"items":null}"#)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        serde_json::from_str::<Holder>("{}")
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        serde_json::from_str::<Holder>(r#"{"items":["a"]}"#)
            .unwrap()
            .items
            .len(),
        1
    );
}

/// Stocks send condition codes as a list and crypto sends a bare string.
#[test]
fn condition_codes_normalize_whether_they_arrive_as_a_string_or_a_list() {
    #[derive(Deserialize)]
    struct Conditions {
        #[serde(default, deserialize_with = "alpaca_sdk::types::string_or_list")]
        c: Option<Vec<String>>,
    }

    let one: Conditions = serde_json::from_str(r#"{"c":"R"}"#).unwrap();
    assert_eq!(one.c, Some(vec!["R".to_owned()]));

    let many: Conditions = serde_json::from_str(r#"{"c":["R","T"]}"#).unwrap();
    assert_eq!(many.c.unwrap().len(), 2);

    let absent: Conditions = serde_json::from_str("{}").unwrap();
    assert_eq!(absent.c, None);

    let null: Conditions = serde_json::from_str(r#"{"c":null}"#).unwrap();
    assert_eq!(null.c, None);
}
