//! Unofficial Rust SDK for the [Alpaca](https://alpaca.markets) trading, market
//! data, and broker APIs.
//!
//! This is a port of the official Python SDK, [alpaca-py], with the same feature
//! surface expressed through Rust's type system: money is [`rust_decimal::Decimal`]
//! rather than a string-or-float union, unknown API enum values deserialize into an
//! `Unknown` variant instead of failing, and pagination is a `Stream` rather than a
//! three-way mode flag.
//!
//! It is not affiliated with or endorsed by Alpaca Securities LLC.
//!
//! # Feature flags
//!
//! | Feature | Default | What it enables |
//! |---|---|---|
//! | `trading` | yes | The trading REST client and trade-update stream |
//! | `data` | yes | Historical and live market data |
//! | `broker` | no | The broker API, including its SSE event streams |
//! | `blocking` | no | A synchronous façade over the async clients |
//! | `polars` | no | `DataFrame` conversion for market data collections |
//! | `rustls-tls` | yes | TLS via rustls |
//! | `native-tls` | no | TLS via the platform library |
//!
//! [alpaca-py]: https://github.com/alpacahq/alpaca-py

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod auth;
pub mod backoff;
pub mod config;
pub mod error;
pub mod rest;
pub mod types;

#[cfg(feature = "broker")]
#[cfg_attr(docsrs, doc(cfg(feature = "broker")))]
pub mod broker;
#[cfg(feature = "data")]
#[cfg_attr(docsrs, doc(cfg(feature = "data")))]
pub mod data;
#[cfg(feature = "trading")]
#[cfg_attr(docsrs, doc(cfg(feature = "trading")))]
pub mod trading;

pub use auth::Credentials;
pub use config::{BaseUrl, RetryConfig};
pub use error::{ApiError, Error, Result};
pub use rest::{RestClient, RestConfig};
