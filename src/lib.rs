//! Unofficial Rust SDK for the [Alpaca](https://alpaca.markets) trading, market
//! data, and broker APIs.
//!
//! Targets the Alpaca API itself, documented at [docs.alpaca.markets]. Where these
//! docs describe what an endpoint does, they describe the API; where they describe
//! how this crate represents it, that is a choice made here. Money is
//! [`rust_decimal::Decimal`] rather than a string-or-float union, unknown API enum
//! values deserialize into an `Unknown` variant instead of failing, and paginated
//! endpoints offer both a single page and a walk.
//!
//! It is not affiliated with or endorsed by Alpaca Securities LLC. See `NOTICE`
//! for the works this one derives from.
//!
//! # What the routes are checked against
//!
//! Alpaca publishes an API reference, vendors `OpenAPI` specs, and ships five
//! SDKs, and they do not always agree. This crate treats them in order of how
//! close each is to the wire: a captured response beats a specification, a
//! specification beats an SDK, and the published reference is what says whether
//! a route is still current at all — three event streams were in the specs,
//! looked healthy, and had been switched off.
//!
//! `just coverage`, `just parameters` and `just enums-drift` diff this crate
//! against those sources, and `COVERAGE.md` is checked in rather than trusted
//! to memory.
//!
//! # Feature flags
//!
//! | Feature | Default | What it enables |
//! |---|---|---|
//! | `trading` | yes | The trading REST client and trade-update stream |
//! | `data` | yes | Historical and live market data |
//! | `broker` | no | The broker API, including its SSE event streams |
//! | `blocking` | no | A synchronous façade over the async clients, via `blocking::Blocking` |
//! | `polars` | no | `DataFrame` conversion for market data collections, via `data::ToFrame`. Implies `data` |
//! | `rustls-tls` | yes | TLS via rustls |
//! | `native-tls` | no | TLS via the platform library |
//!
//! [docs.alpaca.markets]: https://docs.alpaca.markets/us/reference/

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod auth;
pub mod backoff;
#[cfg(feature = "blocking")]
#[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
pub mod blocking;
pub mod config;
pub mod error;
pub mod rest;
pub mod types;

#[cfg(feature = "_sse")]
mod sse;

#[cfg(feature = "broker")]
#[cfg_attr(docsrs, doc(cfg(feature = "broker")))]
pub mod broker;
#[cfg(feature = "data")]
#[cfg_attr(docsrs, doc(cfg(feature = "data")))]
pub mod data;
#[cfg(feature = "trading")]
#[cfg_attr(docsrs, doc(cfg(feature = "trading")))]
pub mod trading;

/// The `polars` this crate was built against.
///
/// Re-exported because [`ToFrame`](crate::data::ToFrame) hands back a
/// `polars::prelude::DataFrame`, and a caller who depends on a different polars
/// version gets two incompatible `DataFrame` types with the same name. Reach for
/// this one, or match the version in `Cargo.toml`.
#[cfg(feature = "polars")]
#[cfg_attr(docsrs, doc(cfg(feature = "polars")))]
pub use polars;

/// The `rust_decimal` this crate was built against.
///
/// Re-exported for the same reason as the `polars` re-export: every price, quantity and
/// balance in this crate is a `rust_decimal::Decimal`, and a caller who depends
/// on a different `rust_decimal` version gets two incompatible `Decimal` types
/// with the same name and an error that does not explain itself. Reach for this
/// one, or match the version in `Cargo.toml`.
///
/// It is also the way to the rest of that crate without declaring the
/// dependency twice — `RoundingStrategy` for the rounding modes, and the
/// conversions:
///
/// ```
/// use alpaca_sdk::Decimal;
///
/// let limit: Decimal = "185.50".parse().unwrap();
/// assert_eq!(limit.to_string(), "185.50");
/// ```
///
/// The `dec!` literal macro is **not** reachable through this re-export: it sits
/// behind `rust_decimal`'s `macros` feature, which is off here because it costs
/// a proc-macro dependency the crate itself does not use. A caller who wants it
/// depends on `rust_decimal` with that feature directly, and cargo unifies the
/// two — the same arrangement `polars` and its `lazy` feature already have.
pub use rust_decimal;

/// The numeric type for every price, quantity and balance this crate exchanges.
///
/// Alpaca sends these as strings; reading one as an `f64` loses precision on the
/// one class of field where it is least acceptable. Market data floats that
/// arrive as JSON numbers stay `f64` and are not this type.
pub use rust_decimal::Decimal;

pub use auth::Credentials;
pub use config::{BaseUrl, RetryBackoff, RetryConfig};
pub use error::{ApiError, Error, Result, TransportError};
pub use rest::{Replay, RestClient, RestConfig};
#[cfg(feature = "_sse")]
pub use sse::{Event as SseEvent, EventStreamRequest};
