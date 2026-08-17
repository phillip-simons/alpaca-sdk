//! Shared types used across the trading, market data, and broker APIs.

pub(crate) mod wire;

mod common_enums;
pub mod decimal;
mod ident;
mod logo;
pub(crate) mod path;
pub mod serde_util;
pub(crate) mod setters;
pub mod timestamp;
pub(crate) mod validated;

pub use common_enums::{ContractType, Sort, SupportedCurrencies};
pub use ident::AssetIdent;
pub use logo::LogoRequest;
pub use validated::Validated;

/// The `Validated` derive, which emits the trait's defaulted no-op.
///
/// Crate-internal, like [`Setters`](setters), and re-exported from the same
/// path as the trait so that one `use crate::types::Validated;` brings both. A
/// downstream caller satisfying the bound writes the one-line impl by hand, or
/// reaches for [`Raw`](crate::rest::Raw).
///
/// # Why the crate root re-exports `types::validated::Validated`
///
/// Because *this* line makes `types::Validated` name two things — the trait in
/// the type namespace, this macro in the macro namespace. `pub use
/// types::Validated;` at the root compiles either way; the difference is only
/// what a downstream `#[derive(alpaca_sdk::Validated)]` is told. Through the
/// short path it is "derive macro `Validated` is private", which reads like a
/// feature flag is missing; through the module path it is "cannot find
/// `Validated` in `alpaca_sdk`", which is the truth.
///
/// Neither is a barrier, and this should not pretend otherwise: the
/// private-derive message still appears at `alpaca_sdk::types::Validated`,
/// and *that* one carries `help: import Validated directly →
/// alpaca_sdk_macros::Validated`, which is a published crate. The root path
/// offers no such help, which is the whole of the difference. Keeping the
/// derive crate-internal is about not promising a stable expansion rather than
/// about preventing access — and the expansion is unqualified, so a downstream
/// `Validated` of someone else's would be what it implemented.
pub(crate) use alpaca_sdk_macros::Validated;

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
pub use serde_util::{
    comma_separated, comma_separated_required, empty_string_as_none, null_as_default,
    string_or_list,
};

#[cfg(test)]
mod wire_tests;
