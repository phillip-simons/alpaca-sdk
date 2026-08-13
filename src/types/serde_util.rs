//! Deserialization helpers for Alpaca's wire quirks.

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, de};

/// Treats an empty string as an absent value.
///
/// Multi-leg order responses set `asset_id`, `symbol`, `asset_class`, and `side`
/// to `""` rather than omitting them or sending `null`, because those fields
/// describe a single leg and an mleg order has several. An empty string is not a
/// value, so this reads it as absent.
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

/// Serde codec for integers Alpaca sends inconsistently as numbers or strings.
///
/// The trading account endpoint returns `"options_approved_level": "1"` but
/// `"daytrade_count": 0` in the same payload. A plain `i64` rejects the response
/// outright, so both forms are accepted.
pub mod int {
    use std::fmt;

    use serde::de::{self, Unexpected, Visitor};
    use serde::{Deserializer, Serializer};

    struct IntVisitor;

    impl Visitor<'_> for IntVisitor {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer as a number or string")
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
            i64::try_from(value).map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
        }

        fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
            if value.fract() == 0.0 {
                // Alpaca sends whole numbers as floats in a few places.
                #[allow(clippy::cast_possible_truncation)]
                return Ok(value as i64);
            }
            Err(E::invalid_value(Unexpected::Float(value), &self))
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            value
                .trim()
                .parse()
                .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
        }
    }

    /// Deserializes an integer from a number or string.
    ///
    /// # Errors
    /// Returns an error if the value is neither, or does not parse.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        deserializer.deserialize_any(IntVisitor)
    }

    /// Serializes an integer as a number.
    ///
    /// # Errors
    /// Propagates the serializer's own failures.
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(*value)
    }

    /// The same codec for optional fields, where `null` and `""` mean absent.
    pub mod option {
        use super::{Deserializer, IntVisitor, Serializer, Visitor, de, fmt};

        struct OptionVisitor;

        impl<'de> Visitor<'de> for OptionVisitor {
            type Value = Option<i64>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an integer as a number or string, or null")
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
                struct MaybeEmpty;

                impl Visitor<'_> for MaybeEmpty {
                    type Value = Option<i64>;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("an integer as a number or string")
                    }

                    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                        IntVisitor.visit_i64(value).map(Some)
                    }

                    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                        IntVisitor.visit_u64(value).map(Some)
                    }

                    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                        IntVisitor.visit_f64(value).map(Some)
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        if value.trim().is_empty() {
                            return Ok(None);
                        }
                        IntVisitor.visit_str(value).map(Some)
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

        /// Deserializes an optional integer from a number, string, or null.
        ///
        /// # Errors
        /// Returns an error if a present value does not parse as an integer.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<i64>, D::Error> {
            deserializer.deserialize_option(OptionVisitor)
        }

        /// Serializes an optional integer as a number or null.
        ///
        /// # Errors
        /// Propagates the serializer's own failures.
        pub fn serialize<S: Serializer>(
            value: &Option<i64>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match value {
                Some(int) => serializer.serialize_i64(*int),
                None => serializer.serialize_none(),
            }
        }
    }
}

/// Deserializes an explicit `null` as the type's default.
///
/// `#[serde(default)]` alone covers a field that is *absent*, not one that is
/// present and null. Alpaca sends both — `"funding_source": null` appears in the
/// same list response as a populated one — and the difference is invisible until
/// a payload with the null form shows up, at which point the whole response
/// fails to decode.
///
/// Pair it with `default` so both forms are handled:
/// `#[serde(default, deserialize_with = "null_as_default")]`.
///
/// # Errors
/// Propagates the deserializer's own failures.
pub fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Serializes a list as a single comma-separated query parameter.
///
/// Alpaca expects `symbols=AAPL,SPY` rather than a repeated parameter.
///
/// # Errors
/// Propagates the serializer's own failures.
pub fn comma_separated<S, T>(values: &Option<Vec<T>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Display,
{
    match values {
        Some(values) => comma_joined(values, serializer),
        None => serializer.serialize_none(),
    }
}

/// The same, for a list Alpaca requires rather than one it accepts.
///
/// A required list cannot be `Option`, so it needs its own serializer. It needs
/// one at all for a reason worth stating plainly: **a `Vec` in a query struct
/// does not serialize.** `serde_urlencoded` has no representation for a
/// sequence, so reqwest's query builder fails the whole request with
/// `Builder: unsupported value` — locally, before anything is sent, with no
/// status and nothing on the wire to look at. A route whose only parameter is a
/// bare `Vec` can never be called at all.
///
/// # Errors
/// Propagates the serializer's own failure.
pub fn comma_separated_required<S, T>(values: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Display,
{
    comma_joined(values, serializer)
}

fn comma_joined<S, T>(values: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Display,
{
    let joined = values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    serializer.serialize_str(&joined)
}

/// Deserializes a field that may be a single string or a list of them.
///
/// Trade and quote condition codes come back as a list for stocks and as a bare
/// string for crypto. This normalizes both to a list, rather than leaving
/// the caller to branch; normalizing to a list here means they do not have to.
///
/// # Errors
/// Returns an error if the value is neither a string nor a list of strings.
pub fn string_or_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrList;

    impl<'de> serde::de::Visitor<'de> for StringOrList {
        type Value = Option<Vec<String>>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string, a list of strings, or null")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Some(vec![value.to_owned()]))
        }

        fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
            Ok(Some(vec![value]))
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
            deserializer.deserialize_any(self)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(Some(values))
        }
    }

    deserializer.deserialize_option(StringOrList)
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
