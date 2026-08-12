//! Identifiers that may be either a symbol or a UUID.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An asset identified by ticker symbol or by Alpaca's UUID.
///
/// Several endpoints accept either form in the same path segment. alpaca-py types
/// this as `Union[UUID, str]` and checks it at runtime in
/// `validate_symbol_or_asset_id` and `validate_symbol_or_contract_id`; here the
/// two cases are variants, so the check happens at the call site and cannot be
/// skipped.
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
    /// The identifier as it appears in a request path.
    #[must_use]
    pub fn as_path_segment(&self) -> String {
        self.to_string()
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
    /// A string that parses as a UUID becomes [`AssetIdent::Id`], matching
    /// a string that parses as a UUID is treated as an id, not a symbol.
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
    fn uuid_strings_are_upcast_like_alpaca_py_does() {
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
        assert_eq!(AssetIdent::from("AAPL").as_path_segment(), "AAPL");
        assert_eq!(AssetIdent::from(UUID_STR).as_path_segment(), UUID_STR);
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
