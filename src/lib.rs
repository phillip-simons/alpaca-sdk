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
//! The shape is close enough to migrate mechanically. What follows is the part
//! that is not: where a rename is not enough, or where this crate deliberately
//! answers a question differently.
//!
//! | In alpaca-py | Here |
//! |---|---|
//! | `raw_data=True` on the constructor | [`RestClient::request_raw`]. A boolean cannot change a function's return type in Rust, so the escape hatch is a method rather than a flag |
//! | `.df` on `BarSet`, `QuoteSet`, `TradeSet` | [`data::ToFrame::df`], behind the `polars` feature. Those types are `HashMap` aliases here, and an alias cannot take an inherent method, so it arrives with a `use` |
//! | Using the SDK synchronously | [`blocking::Blocking`], behind the `blocking` feature: one wrapper over any client rather than a synchronous copy of every method |
//! | `set[symbol]` lookup on a result | Plain `HashMap` indexing — the collections *are* maps |
//! | A callback registered per symbol per channel | A [`Stream`](futures_util::Stream) of messages, which the caller dispatches however they like |
//! | `PaginationType.FULL` and `max_items_limit` | A `get_all_…` method per paginated route, taking `max_items` |
//! | `APIError.code`, re-parsed on every access | [`ApiError`], parsed once at construction; a non-JSON error body degrades instead of raising |
//! | A stream failure raised as a connection error | [`Error::Stream`], distinct from [`Error::InvalidRequest`], which now means only a request this crate rejected before sending it |
//! | An enum value the SDK does not know | An `Unknown(String)` variant, so a new value Alpaca starts sending does not break decoding |
//! | Money as a string or a float, depending | [`rust_decimal::Decimal`] |
//! | Retries: a flat 3-second wait | Exponential backoff from 1 second with jitter, which is what Alpaca's rate-limit page asks for. [`RetryBackoff::Flat`] with `wait` set to 3 seconds restores the old behaviour |
//! | `delete_account` | `close_account`. alpaca-py deprecates the first and forwards it to the second |
//!
//! Where the two disagree about the API rather than about Rust, this crate
//! follows the API; `ROADMAP.md` records each case and what settled it.
//!
//! # Feature flags
//!
//! | Feature | Default | What it enables |
//! |---|---|---|
//! | `trading` | yes | The trading REST client and trade-update stream |
//! | `data` | yes | Historical and live market data |
//! | `broker` | no | The broker API, including its SSE event streams |
//! | `blocking` | no | A synchronous façade over the async clients, via [`blocking::Blocking`] |
//! | `polars` | no | `DataFrame` conversion for market data collections, via [`data::ToFrame`]. Implies `data` |
//! | `rustls-tls` | yes | TLS via rustls |
//! | `native-tls` | no | TLS via the platform library |
//!
//! [alpaca-py]: https://github.com/alpacahq/alpaca-py
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

pub use auth::Credentials;
pub use config::{BaseUrl, RetryBackoff, RetryConfig};
pub use error::{ApiError, Error, Result};
pub use rest::{RestClient, RestConfig};
#[cfg(feature = "_sse")]
pub use sse::{Event as SseEvent, EventStreamRequest};
