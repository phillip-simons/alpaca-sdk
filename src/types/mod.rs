//! Shared types used across the trading, market data, and broker APIs.

pub(crate) mod wire;

mod common_enums;
pub mod decimal;
mod ident;
mod logo;
pub(crate) mod path;
pub mod serde_util;
pub mod timestamp;

pub use common_enums::{ContractType, Sort, SupportedCurrencies};
pub use ident::AssetIdent;
pub use logo::LogoRequest;

/// Serde codec for optional [`rust_decimal::Decimal`] fields.
///
/// Re-exported so `#[serde(with = "...")]` paths read symmetrically with
/// [`decimal`].
pub use decimal::option as option_decimal;

/// Serde codec for optional timestamp fields, symmetric with [`timestamp`].
pub use timestamp::option as option_timestamp;

/// Serde codec for integers Alpaca sends as numbers or strings.
pub use serde_util::int;

/// The optional form of [`int`].
pub use serde_util::int::option as option_int;

/// The field-level helpers, re-exported so a `#[serde(...)]` attribute names
/// `alpaca_sdk::types::…` like the codecs above rather than reaching two
/// modules deep.
pub use serde_util::{comma_separated, empty_string_as_none, null_as_default, string_or_list};

#[cfg(test)]
mod wire_tests;
