//! A timestamp codec that reads both JSON strings and msgpack extension types.
//!
//! The same models serve two wire formats. The REST APIs send RFC 3339 strings;
//! the live market data stream is msgpack and sends timestamps as extension type
//! `-1`.
//!
//! Nothing decodes that extension out of the box — not `DateTime<Utc>`, not
//! `String`, not even `serde_json::Value`; `rmp-serde` surfaces it as a newtype
//! struct and every ordinary target rejects it. So the visitor here handles the
//! extension itself, in all three encodings the msgpack spec defines:
//!
//! | Encoding | Bytes | Layout |
//! |---|---|---|
//! | timestamp32 | 4 | 32-bit seconds |
//! | timestamp64 | 8 | 30-bit nanoseconds, then 34-bit seconds |
//! | timestamp96 | 12 | 32-bit nanoseconds, then 64-bit signed seconds |

use std::fmt;

use chrono::{DateTime, TimeZone as _, Utc};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serializer};

/// The msgpack extension type reserved for timestamps.
const TIMESTAMP_EXT_TYPE: i8 = -1;

struct TimestampVisitor;

/// Decodes a msgpack timestamp extension into a `DateTime`.
///
/// Shared with the live stream, which reads frames through `rmpv` and has to
/// convert extensions itself — the same parsing, one implementation.
///
/// # Errors
/// Returns an error string if the tag is not the timestamp extension or the
/// payload is not 4, 8, or 12 bytes.
#[cfg(feature = "data")]
pub(crate) fn from_extension(tag: i8, bytes: &[u8]) -> Result<DateTime<Utc>, String> {
    TimestampVisitor::from_ext::<serde::de::value::Error>(tag, bytes).map_err(|e| e.to_string())
}

impl TimestampVisitor {
    fn from_parts<E: de::Error>(seconds: i64, nanoseconds: u32) -> Result<DateTime<Utc>, E> {
        Utc.timestamp_opt(seconds, nanoseconds)
            .single()
            .ok_or_else(|| E::custom(format!("{seconds}s {nanoseconds}ns is not a valid time")))
    }

    fn from_ext<E: de::Error>(tag: i8, bytes: &[u8]) -> Result<DateTime<Utc>, E> {
        if tag != TIMESTAMP_EXT_TYPE {
            return Err(E::custom(format!(
                "msgpack extension type {tag}, expected {TIMESTAMP_EXT_TYPE} for a timestamp"
            )));
        }

        match bytes.len() {
            4 => {
                let seconds = u32::from_be_bytes(bytes.try_into().map_err(E::custom)?);
                Self::from_parts(i64::from(seconds), 0)
            }
            8 => {
                let packed = u64::from_be_bytes(bytes.try_into().map_err(E::custom)?);
                // Low 34 bits are seconds, high 30 are nanoseconds.
                let seconds = (packed & 0x0003_ffff_ffff) as i64;
                let nanoseconds = u32::try_from(packed >> 34).map_err(E::custom)?;
                Self::from_parts(seconds, nanoseconds)
            }
            12 => {
                let nanoseconds = u32::from_be_bytes(bytes[..4].try_into().map_err(E::custom)?);
                let seconds = i64::from_be_bytes(bytes[4..].try_into().map_err(E::custom)?);
                Self::from_parts(seconds, nanoseconds)
            }
            other => Err(E::custom(format!(
                "msgpack timestamp is {other} bytes, expected 4, 8, or 12"
            ))),
        }
    }
}

impl<'de> Visitor<'de> for TimestampVisitor {
    type Value = DateTime<Utc>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an RFC 3339 string or a msgpack timestamp extension")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| E::custom(format!("{value:?} is not an RFC 3339 timestamp: {e}")))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.visit_str(&value)
    }

    /// The shape `rmp-serde` hands an extension type over as.
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let (tag, bytes) = <(i8, serde_bytes::ByteBuf)>::deserialize(deserializer)?;
        Self::from_ext(tag, &bytes)
    }

    /// Some encoders send epoch seconds as a bare number.
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Self::from_parts(value, 0)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Self::from_parts(i64::try_from(value).map_err(E::custom)?, 0)
    }
}

/// Deserializes a timestamp from either wire format.
///
/// # Errors
/// Returns an error if the value is neither an RFC 3339 string nor a msgpack
/// timestamp extension, or if it does not name a real instant.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
    deserializer.deserialize_any(TimestampVisitor)
}

/// Serializes a timestamp as an RFC 3339 string.
///
/// # Errors
/// Propagates the serializer's own failures.
pub fn serialize<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
}

/// The same codec for optional timestamps.
pub mod option {
    use super::{DateTime, Deserializer, Serializer, TimestampVisitor, Utc, Visitor, de, fmt};

    struct OptionVisitor;

    impl<'de> Visitor<'de> for OptionVisitor {
        type Value = Option<DateTime<Utc>>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an RFC 3339 string, a msgpack timestamp extension, or null")
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
            deserializer.deserialize_any(TimestampVisitor).map(Some)
        }
    }

    /// Deserializes an optional timestamp from either wire format.
    ///
    /// # Errors
    /// Returns an error if a present value is not a valid timestamp.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DateTime<Utc>>, D::Error> {
        deserializer.deserialize_option(OptionVisitor)
    }

    /// Serializes an optional timestamp as an RFC 3339 string or null.
    ///
    /// # Errors
    /// Propagates the serializer's own failures.
    pub fn serialize<S: Serializer>(
        value: &Option<DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(timestamp) => super::serialize(timestamp, serializer),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Frame {
        #[serde(rename = "t", with = "crate::types::timestamp")]
        timestamp: DateTime<Utc>,
    }

    /// A msgpack map `{"t": <ext -1 payload>}`.
    fn frame(ext: &[u8]) -> Vec<u8> {
        let mut buf = vec![0x81, 0xa1, b't'];
        buf.extend_from_slice(ext);
        buf
    }

    fn timestamp32(seconds: u32) -> Vec<u8> {
        let mut v = vec![0xd6, 0xff];
        v.extend_from_slice(&seconds.to_be_bytes());
        v
    }

    fn timestamp64(seconds: u64, nanoseconds: u32) -> Vec<u8> {
        let packed = (u64::from(nanoseconds) << 34) | seconds;
        let mut v = vec![0xd7, 0xff];
        v.extend_from_slice(&packed.to_be_bytes());
        v
    }

    fn timestamp96(seconds: i64, nanoseconds: u32) -> Vec<u8> {
        let mut v = vec![0xc7, 12, 0xff];
        v.extend_from_slice(&nanoseconds.to_be_bytes());
        v.extend_from_slice(&seconds.to_be_bytes());
        v
    }

    #[test]
    fn reads_an_rfc_3339_string_from_json() {
        let frame: Frame = serde_json::from_str(r#"{"t":"2022-03-09T05:00:00Z"}"#).unwrap();
        assert_eq!(frame.timestamp.to_rfc3339(), "2022-03-09T05:00:00+00:00");
    }

    #[test]
    fn reads_a_fractional_rfc_3339_string() {
        let frame: Frame = serde_json::from_str(r#"{"t":"2022-03-18T14:03:31.960672Z"}"#).unwrap();
        assert_eq!(frame.timestamp.timestamp_subsec_micros(), 960_672);
    }

    #[test]
    fn reads_a_msgpack_timestamp32() {
        // Nothing decodes this by default: DateTime, String and
        // serde_json::Value all reject the newtype struct rmp-serde produces.
        let decoded: Frame = rmp_serde::from_slice(&frame(&timestamp32(1_646_802_000))).unwrap();
        assert_eq!(decoded.timestamp.timestamp(), 1_646_802_000);
        assert_eq!(decoded.timestamp.timestamp_subsec_nanos(), 0);
    }

    #[test]
    fn reads_a_msgpack_timestamp64_with_nanoseconds() {
        let decoded: Frame =
            rmp_serde::from_slice(&frame(&timestamp64(1_646_802_000, 123_456_789))).unwrap();

        assert_eq!(decoded.timestamp.timestamp(), 1_646_802_000);
        assert_eq!(decoded.timestamp.timestamp_subsec_nanos(), 123_456_789);
    }

    #[test]
    fn reads_a_msgpack_timestamp96() {
        let decoded: Frame =
            rmp_serde::from_slice(&frame(&timestamp96(1_646_802_000, 500))).unwrap();

        assert_eq!(decoded.timestamp.timestamp(), 1_646_802_000);
        assert_eq!(decoded.timestamp.timestamp_subsec_nanos(), 500);
    }

    #[test]
    fn rejects_the_wrong_extension_type() {
        // ext type 5 is not a timestamp; silently accepting it would produce a
        // plausible but wrong instant.
        let mut ext = vec![0xd6, 0x05];
        ext.extend_from_slice(&1_646_802_000u32.to_be_bytes());

        assert!(rmp_serde::from_slice::<Frame>(&frame(&ext)).is_err());
    }

    #[test]
    fn rejects_a_malformed_string() {
        assert!(serde_json::from_str::<Frame>(r#"{"t":"not a time"}"#).is_err());
    }

    #[test]
    fn optional_timestamps_accept_null_and_both_formats() {
        #[derive(Debug, Deserialize)]
        struct Maybe {
            #[serde(rename = "t", default, with = "crate::types::timestamp::option")]
            timestamp: Option<DateTime<Utc>>,
        }

        let absent: Maybe = serde_json::from_str(r#"{"t":null}"#).unwrap();
        assert_eq!(absent.timestamp, None);

        let missing: Maybe = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.timestamp, None);

        let json: Maybe = serde_json::from_str(r#"{"t":"2022-03-09T05:00:00Z"}"#).unwrap();
        assert_eq!(json.timestamp.unwrap().timestamp(), 1_646_802_000);

        let packed: Maybe = rmp_serde::from_slice(&frame(&timestamp32(1_646_802_000))).unwrap();
        assert_eq!(packed.timestamp.unwrap().timestamp(), 1_646_802_000);
    }

    #[test]
    fn round_trips_back_out_as_rfc_3339() {
        #[derive(Debug, Serialize)]
        struct Out {
            #[serde(with = "crate::types::timestamp")]
            timestamp: DateTime<Utc>,
        }

        let value = Utc.timestamp_opt(1_646_802_000, 0).unwrap();
        let json = serde_json::to_string(&Out { timestamp: value }).unwrap();

        assert_eq!(json, r#"{"timestamp":"2022-03-09T05:00:00Z"}"#);
    }

    // The three tests below exercise `from_extension` directly rather than
    // through serde, which is how the live stream reaches it. They lived in the
    // integration suite until the function stopped being public.

    /// The three msgpack timestamp encodings, which is what the live market
    /// data stream actually sends.
    #[cfg(feature = "data")]
    #[test]
    fn all_three_msgpack_timestamp_widths_decode() {
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

    #[cfg(feature = "data")]
    #[test]
    fn a_msgpack_extension_that_is_not_a_timestamp_is_rejected() {
        // Right width, wrong extension type.
        let wrong_tag = from_extension(5, &1_646_816_400u32.to_be_bytes()).unwrap_err();
        assert!(wrong_tag.contains("extension type 5"), "{wrong_tag}");

        // Right extension type, a width the msgpack specification does not
        // define.
        let wrong_width = from_extension(-1, &[0, 1, 2]).unwrap_err();
        assert!(!wrong_width.is_empty());
    }

    /// A timestamp before the epoch is a negative seconds value, which only the
    /// 96-bit encoding can carry. Corporate actions reach back decades.
    #[cfg(feature = "data")]
    #[test]
    fn a_pre_epoch_timestamp_decodes() {
        let mut wide = Vec::new();
        wide.extend_from_slice(&0u32.to_be_bytes());
        wide.extend_from_slice(&(-86_400i64).to_be_bytes());

        assert_eq!(from_extension(-1, &wide).unwrap().timestamp(), -86_400);
    }
}
