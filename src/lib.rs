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
//! It is not affiliated with or endorsed by Alpaca Securities LLC.
//!
//! # Coming from alpaca-py
//!
//! This crate began as a port of the official Python SDK, [alpaca-py], and is a
//! derivative work of it (see `NOTICE`). It no longer tracks that SDK: alpaca-py
//! is the least complete of Alpaca's official SDKs, and in at least one place it
//! still calls an endpoint Alpaca has retired. Where the two disagree, this crate
//! follows the API.
//!
//! The shape is close enough to migrate mechanically. The differences that need a
//! decision rather than a rename are collected in `ROADMAP.md`.
//!
//! # Feature flags
//!
//! | Feature | Default | What it enables |
//! |---|---|---|
//! | `trading` | yes | The trading REST client and trade-update stream |
//! | `data` | yes | Historical and live market data |
//! | `broker` | no | The broker API, including its SSE event streams |
//! | `blocking` | no | A synchronous façade over the async clients |
//! | `polars` | no | `DataFrame` conversion for market data collections, via [`data::ToFrame`]. Implies `data` |
//! | `rustls-tls` | yes | TLS via rustls |
//! | `native-tls` | no | TLS via the platform library |
//!
//! [alpaca-py]: https://github.com/alpacahq/alpaca-py
//! [docs.alpaca.markets]: https://docs.alpaca.markets/us/reference/

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod auth;
pub mod backoff;
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

pub use auth::Credentials;
pub use config::{BaseUrl, RetryBackoff, RetryConfig};
pub use error::{ApiError, Error, Result};
pub use rest::{RestClient, RestConfig};
#[cfg(feature = "_sse")]
pub use sse::{Event as SseEvent, EventStreamRequest};
