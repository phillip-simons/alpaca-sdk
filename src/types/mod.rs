//! Shared types used across the trading, market data, and broker APIs.

pub(crate) mod wire;

mod common_enums;
pub mod decimal;
mod ident;
pub mod serde_util;
mod shared_enums;
pub mod timestamp;

pub use common_enums::{Sort, SupportedCurrencies};
pub use ident::AssetIdent;
pub use shared_enums::*;

/// Serde codec for optional [`rust_decimal::Decimal`] fields.
///
/// Re-exported so `#[serde(with = "...")]` paths read symmetrically with
/// [`decimal`].
pub use decimal::option as option_decimal;

#[cfg(test)]
mod wire_tests;
