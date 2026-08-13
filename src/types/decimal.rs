//! Serde support for the money fields Alpaca sends as strings.
//!
//! Alpaca sends order quantities and prices as JSON *strings*, and accepts them
//! that way in request bodies. They are [`Decimal`] here, deserialized from
//! either a string or a number and serialized back as a string — the only form
//! that survives a round trip exactly. Reading them as `f64` loses precision on
//! fractional-share quantities.
//!
//! Market data floats (bar OHLCV, vwap) stay `f64`: they arrive as JSON numbers
//! and are already approximate on the wire, so `Decimal` would add cost without
//! adding accuracy.
//!
//! ```
//! # use serde::{Deserialize, Serialize};
//! # use rust_decimal::Decimal;
//! #[derive(Serialize, Deserialize)]
//! struct Position {
//!     #[serde(with = "alpaca_sdk::types::decimal")]
//!     qty: Decimal,
//!     #[serde(with = "alpaca_sdk::types::option_decimal", default)]
//!     limit_price: Option<Decimal>,
//! }
//! ```

use std::fmt;

use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive as _;
use serde::de::{self, Unexpected, Visitor};
use serde::{Deserializer, Serializer};

struct DecimalVisitor;

impl Visitor<'_> for DecimalVisitor {
    type Value = Decimal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a decimal as a string or number")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        value
            .trim()
            .parse::<Decimal>()
            .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        // `from_f64`, not `from_f64_retain`: the latter keeps the float's full
        // binary expansion, so a wire value of 183.42 becomes
        // 183.41999999999998749444785057. `from_f64` recovers the shortest
        // decimal that round-trips, which is the number the wire meant.
        Decimal::from_f64(value).ok_or_else(|| E::invalid_value(Unexpected::Float(value), &self))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Decimal::from(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Decimal::from(value))
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Self::Value, E> {
        Decimal::from_i128(value).ok_or_else(|| E::custom("i128 out of range for a decimal"))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Self::Value, E> {
        Decimal::from_u128(value).ok_or_else(|| E::custom("u128 out of range for a decimal"))
    }
}

/// Deserializes a [`Decimal`] from a string or a number.
///
/// # Errors
/// Returns an error if the value is neither, or does not parse as a decimal.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    deserializer.deserialize_any(DecimalVisitor)
}

/// Serializes a [`Decimal`] as a string, which is what Alpaca's request bodies
/// expect for quantity and price fields.
///
/// # Errors
/// Propagates the serializer's own failures.
pub fn serialize<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

/// The same codec for optional fields, where `null` and `""` both mean absent.
pub mod option {
    use super::{Decimal, DecimalVisitor, Deserializer, Serializer, Visitor, de, fmt};

    struct OptionVisitor;

    impl<'de> Visitor<'de> for OptionVisitor {
        type Value = Option<Decimal>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a decimal as a string or number, or null")
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            // Alpaca sends "" rather than null for several absent numeric
            // fields, notably on multi-leg order legs.
            struct MaybeEmpty;

            impl Visitor<'_> for MaybeEmpty {
                type Value = Option<Decimal>;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a decimal as a string or number, or an empty string")
                }

                fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                    if value.trim().is_empty() {
                        return Ok(None);
                    }
                    DecimalVisitor.visit_str(value).map(Some)
                }

                fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                    DecimalVisitor.visit_f64(value).map(Some)
                }

                fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                    DecimalVisitor.visit_i64(value).map(Some)
                }

                fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                    DecimalVisitor.visit_u64(value).map(Some)
                }

                fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }

                fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }
            }

            deserializer.deserialize_any(MaybeEmpty)
        }
    }

    /// Deserializes an optional [`Decimal`], mapping `null` and `""` to `None`.
    ///
    /// # Errors
    /// Returns an error if a present value does not parse as a decimal.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Decimal>, D::Error> {
        deserializer.deserialize_option(OptionVisitor)
    }

    /// Serializes an optional [`Decimal`] as a string or `null`.
    ///
    /// # Errors
    /// Propagates the serializer's own failures.
    pub fn serialize<S: Serializer>(
        value: &Option<Decimal>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(decimal) => serializer.serialize_str(&decimal.to_string()),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Order {
        #[serde(with = "crate::types::decimal")]
        qty: Decimal,
        #[serde(with = "crate::types::option_decimal", default)]
        limit_price: Option<Decimal>,
    }

    fn parse(json: &str) -> Order {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn accepts_a_string_which_is_what_alpaca_sends() {
        let order = parse(r#"{"qty":"1.5","limit_price":"183.42"}"#);

        assert_eq!(order.qty, Decimal::new(15, 1));
        assert_eq!(order.limit_price, Some(Decimal::new(18_342, 2)));
    }

    #[test]
    fn accepts_a_number_because_some_endpoints_send_one() {
        let order = parse(r#"{"qty":3,"limit_price":183.42}"#);

        assert_eq!(order.qty, Decimal::from(3));
        assert_eq!(order.limit_price, Some(Decimal::new(18_342, 2)));
    }

    #[test]
    fn null_and_empty_string_both_mean_absent() {
        assert_eq!(parse(r#"{"qty":"1","limit_price":null}"#).limit_price, None);
        assert_eq!(parse(r#"{"qty":"1","limit_price":""}"#).limit_price, None);
        assert_eq!(parse(r#"{"qty":"1"}"#).limit_price, None);
    }

    #[test]
    fn serializes_as_a_string_to_preserve_precision() {
        let json = serde_json::to_string(&Order {
            qty: Decimal::new(15, 1),
            limit_price: Some(Decimal::new(18_342, 2)),
        })
        .unwrap();

        assert_eq!(json, r#"{"qty":"1.5","limit_price":"183.42"}"#);
    }

    #[test]
    fn absent_optional_serializes_as_null() {
        let json = serde_json::to_string(&Order {
            qty: Decimal::from(1),
            limit_price: None,
        })
        .unwrap();

        assert_eq!(json, r#"{"qty":"1","limit_price":null}"#);
    }

    #[test]
    fn fractional_shares_keep_all_nine_decimal_places() {
        // Alpaca documents up to 9 decimal places on qty. This is the case an
        // f64 round trip corrupts, and the reason for Decimal.
        let order = parse(r#"{"qty":"0.123456789"}"#);

        assert_eq!(order.qty.to_string(), "0.123456789");
        assert_eq!(
            serde_json::to_string(&order).unwrap(),
            r#"{"qty":"0.123456789","limit_price":null}"#
        );
    }

    #[test]
    fn a_json_number_does_not_pick_up_float_noise() {
        // `Decimal::from_f64_retain` yields 183.41999999999998749444785057 here,
        // which is precisely the corruption this type exists to avoid.
        let order = parse(r#"{"qty":"1","limit_price":183.42}"#);

        assert_eq!(order.limit_price.unwrap().to_string(), "183.42");
    }

    #[test]
    fn large_notional_values_survive() {
        let order = parse(r#"{"qty":"123456789.987654321"}"#);
        assert_eq!(order.qty.to_string(), "123456789.987654321");
    }

    #[test]
    fn round_trips_through_msgpack() {
        let original = Order {
            qty: Decimal::new(15, 1),
            limit_price: None,
        };

        let encoded = rmp_serde::to_vec_named(&original).unwrap();
        let decoded: Order = rmp_serde::from_slice(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    /// `DecimalVisitor` implements `visit_i128`/`visit_u128` and the optional
    /// path's `MaybeEmpty` does not, which looks like an oversight and is not:
    /// `serde_json` does not hand a bare integer to `visit_i128` unless it
    /// overflows `u64`, and even then the value still arrives through a visitor
    /// both codecs implement. Both paths are asserted here so the asymmetry is
    /// not "fixed" into a difference in behaviour.
    #[test]
    fn a_number_beyond_u64_decodes_the_same_required_or_optional() {
        // 1e20: past u64::MAX (~1.8e19), inside Decimal's range (~7.9e28).
        let huge = "100000000000000000000";

        let required = parse(&format!(r#"{{"qty":{huge}}}"#));
        assert_eq!(required.qty.to_string(), huge);

        let optional = parse(&format!(r#"{{"qty":"1","limit_price":{huge}}}"#));
        assert_eq!(optional.limit_price.unwrap().to_string(), huge);
    }

    /// A value past `Decimal`'s own range has to fail rather than saturate — a
    /// silently clamped quantity is worse than a decode error.
    #[test]
    fn a_number_beyond_decimals_range_is_an_error() {
        // Decimal tops out near 7.9e28.
        let beyond = "1".to_owned() + &"0".repeat(40);
        assert!(serde_json::from_str::<Order>(&format!(r#"{{"qty":{beyond}}}"#)).is_err());
    }

    #[test]
    fn rejects_a_value_that_is_not_a_number() {
        let err = serde_json::from_str::<Order>(r#"{"qty":"abc"}"#).unwrap_err();
        assert!(err.to_string().contains("abc"), "{err}");
    }
}
