//! Deserialization helpers for Alpaca's wire quirks.

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

/// Treats an empty string as an absent value.
///
/// Multi-leg order responses set `asset_id`, `symbol`, `asset_class`, and `side`
/// to `""` rather than omitting them or sending `null`, because those fields
/// describe a single leg and an mleg order has several. alpaca-py rewrites the
/// empty strings to `None` in `Order.__init__` before pydantic sees them; this
/// helper does the same at the field level.
///
/// Works for any target that parses from a string, which covers every field
/// affected: [`uuid::Uuid`], [`String`], and the generated wire enums.
///
/// # Errors
/// Returns an error if a non-empty value fails to parse as `T`.
pub fn empty_string_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let raw = Option::<String>::deserialize(deserializer)?;

    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => T::from_str(value).map(Some).map_err(de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use uuid::Uuid;

    use crate::trading::{AssetClass, OrderSide};

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Leg {
        #[serde(default, deserialize_with = "super::empty_string_as_none")]
        asset_id: Option<Uuid>,
        #[serde(default, deserialize_with = "super::empty_string_as_none")]
        symbol: Option<String>,
        #[serde(default, deserialize_with = "super::empty_string_as_none")]
        asset_class: Option<AssetClass>,
        #[serde(default, deserialize_with = "super::empty_string_as_none")]
        side: Option<OrderSide>,
    }

    #[test]
    fn empty_strings_from_multi_leg_orders_become_none() {
        // The shape of a real mleg parent order response.
        let leg: Leg =
            serde_json::from_str(r#"{"asset_id":"","symbol":"","asset_class":"","side":""}"#)
                .unwrap();

        assert_eq!(
            leg,
            Leg {
                asset_id: None,
                symbol: None,
                asset_class: None,
                side: None,
            }
        );
    }

    #[test]
    fn populated_values_still_parse() {
        let leg: Leg = serde_json::from_str(
            r#"{
                "asset_id":"b0b6dd9d-8b9b-48a9-ba46-b9d54906e415",
                "symbol":"AAPL",
                "asset_class":"us_equity",
                "side":"buy"
            }"#,
        )
        .unwrap();

        assert_eq!(leg.symbol.as_deref(), Some("AAPL"));
        assert_eq!(leg.asset_class, Some(AssetClass::UsEquity));
        assert_eq!(leg.side, Some(OrderSide::Buy));
        assert!(leg.asset_id.is_some());
    }

    #[test]
    fn null_and_missing_are_also_none() {
        let leg: Leg = serde_json::from_str(r#"{"asset_id":null}"#).unwrap();
        assert_eq!(leg.asset_id, None);
        assert_eq!(leg.symbol, None);
    }

    #[test]
    fn whitespace_only_counts_as_empty() {
        let leg: Leg = serde_json::from_str(r#"{"symbol":"   "}"#).unwrap();
        assert_eq!(leg.symbol, None);
    }

    #[test]
    fn a_malformed_uuid_is_still_an_error() {
        // Blanking empty strings must not also swallow genuinely bad values.
        let err = serde_json::from_str::<Leg>(r#"{"asset_id":"not-a-uuid"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid character"), "{err}");
    }

    #[test]
    fn an_unknown_enum_value_survives_rather_than_failing() {
        let leg: Leg = serde_json::from_str(r#"{"side":"short_exempt"}"#).unwrap();
        assert_eq!(
            leg.side,
            Some(OrderSide::Unknown("short_exempt".to_owned()))
        );
    }
}
