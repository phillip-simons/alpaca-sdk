//! Identifiers that may be either a symbol or a UUID.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

/// An asset identified by ticker symbol or by Alpaca's UUID.
///
/// Several endpoints accept either form in the same path segment. The two cases
/// are variants rather than one string with a runtime check, so the caller says
/// which they meant and the check cannot be skipped.
///
/// ```
/// # use alpaca_sdk::types::AssetIdent;
/// # use uuid::Uuid;
/// let by_symbol: AssetIdent = "AAPL".into();
/// let by_id: AssetIdent = Uuid::nil().into();
///
/// assert_eq!(by_symbol.to_string(), "AAPL");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssetIdent {
    /// Alpaca's UUID for the asset or option contract.
    Id(Uuid),
    /// The ticker symbol, such as `AAPL` or `BTC/USD`.
    Symbol(String),
}

impl AssetIdent {
    /// The identifier percent-encoded as a single request path segment.
    ///
    /// Not the same string as [`Display`](fmt::Display), which stays
    /// human-readable: a crypto pair displays as `BTC/USD` and encodes as
    /// `BTC%2FUSD`, which is the form
    /// [Alpaca's reference asks for](https://docs.alpaca.markets/us/reference/get-v2-assets-symbol_or_asset_id)
    /// and the only form that stays one segment.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] for a symbol that cannot be a
    /// path segment at all — the empty string, `.`, or `..`. Those three are
    /// refused rather than encoded because a URL parser removes a dot segment
    /// whatever spelling it arrives in, so there is no encoding that keeps one
    /// as a literal segment.
    pub fn as_path_segment(&self) -> Result<String> {
        match self {
            // A UUID needs no encoding and cannot be a dot segment, so it skips
            // the check rather than round-tripping through a String.
            Self::Id(id) => Ok(id.to_string()),
            Self::Symbol(symbol) => crate::types::path::segment(symbol),
        }
    }

    /// The UUID, if this identifier is one.
    #[must_use]
    pub fn id(&self) -> Option<Uuid> {
        match self {
            Self::Id(id) => Some(*id),
            Self::Symbol(_) => None,
        }
    }

    /// The symbol, if this identifier is one.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Self::Symbol(symbol) => Some(symbol),
            Self::Id(_) => None,
        }
    }
}

impl fmt::Display for AssetIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(symbol) => f.write_str(symbol),
            Self::Id(id) => write!(f, "{id}"),
        }
    }
}

impl From<Uuid> for AssetIdent {
    fn from(id: Uuid) -> Self {
        Self::Id(id)
    }
}

impl From<String> for AssetIdent {
    /// A string that parses as a UUID becomes [`AssetIdent::Id`]; anything else
    /// becomes [`AssetIdent::Symbol`].
    ///
    /// The sniff is safe in the direction that matters: no Alpaca ticker has the
    /// shape of a UUID, so a symbol cannot be mistaken for an id. Construct the
    /// variant directly to bypass it entirely.
    fn from(value: String) -> Self {
        match Uuid::parse_str(&value) {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Symbol(value),
        }
    }
}

impl From<&str> for AssetIdent {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl FromStr for AssetIdent {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID_STR: &str = "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415";

    #[test]
    fn symbols_stay_symbols() {
        let ident = AssetIdent::from("AAPL");

        assert_eq!(ident, AssetIdent::Symbol("AAPL".to_owned()));
        assert_eq!(ident.symbol(), Some("AAPL"));
        assert_eq!(ident.id(), None);
    }

    #[test]
    fn a_uuid_shaped_string_is_parsed_as_an_id_not_a_symbol() {
        let ident = AssetIdent::from(UUID_STR);

        assert_eq!(ident.id(), Some(Uuid::parse_str(UUID_STR).unwrap()));
        assert_eq!(ident.symbol(), None);
    }

    #[test]
    fn crypto_pairs_are_symbols_not_ids() {
        assert_eq!(
            AssetIdent::from("BTC/USD"),
            AssetIdent::Symbol("BTC/USD".to_owned())
        );
    }

    #[test]
    fn path_segment_renders_both_forms() {
        assert_eq!(AssetIdent::from("AAPL").as_path_segment().unwrap(), "AAPL");
        assert_eq!(
            AssetIdent::from(UUID_STR).as_path_segment().unwrap(),
            UUID_STR
        );
    }

    /// The whole point of the method: `Display` stays readable and the path form
    /// is encoded, so a crypto pair addresses one segment rather than two.
    #[test]
    fn a_crypto_pair_displays_readably_and_encodes_for_the_path() {
        let ident = AssetIdent::from("BTC/USD");

        assert_eq!(ident.to_string(), "BTC/USD");
        assert_eq!(ident.as_path_segment().unwrap(), "BTC%2FUSD");
    }

    #[test]
    fn a_dot_segment_symbol_is_refused_rather_than_sent() {
        assert!(
            AssetIdent::Symbol("..".to_owned())
                .as_path_segment()
                .is_err()
        );
        assert!(AssetIdent::Symbol(String::new()).as_path_segment().is_err());
    }

    #[test]
    fn round_trips_through_json_in_both_forms() {
        for ident in [AssetIdent::from("AAPL"), AssetIdent::from(UUID_STR)] {
            let json = serde_json::to_string(&ident).unwrap();
            let decoded: AssetIdent = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, ident);
        }
    }
}
