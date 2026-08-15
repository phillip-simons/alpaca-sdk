//! Targets the Alpaca API itself, documented at [docs.alpaca.markets]. Where these
//! docs describe what an endpoint does, they describe the API; where they describe
//! how this crate represents it, that is a choice made here.
//!
//! Each API surface is its own feature-gated module — `trading`, `data` and
//! `broker`. The `blocking` feature adds `blocking::Blocking`, a synchronous
//! façade over any of them; `polars` adds `data::ToFrame`, and implies `data`.
//!
//! The README is the rest of this page: what the crate is, the feature table,
//! the quick-start examples, how the types behave, how the routes are verified,
//! and the minimum supported Rust version.
//!
//! [docs.alpaca.markets]: https://docs.alpaca.markets/us/reference/

#![cfg_attr(docsrs, feature(doc_cfg))]
// The README is included so that its five examples compile as doctests. They
// are the crate's front door and nothing was checking them, so five batches of
// breaking changes went past without anything noticing whether they still
// built. They did — but only because nobody had needed to find out.
#![doc = include_str!("../README.md")]

// `reqwest` is depended on with `default-features = false`, and none of
// `trading`, `data` or `broker` implies a TLS backend — only the two features
// below reach `reqwest/rustls` and `reqwest/native-tls`. So
// `default-features = false, features = ["broker"]` used to compile cleanly and
// then fail *every* HTTPS request at runtime, because the client in
// `rest::Rest` was built with no backend to negotiate with. Failing the build
// is the only place that mistake is cheap to find.
#[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
compile_error!(
    "alpaca-sdk requires a TLS backend: enable exactly one of the `rustls-tls` \
     or `native-tls` features. Disabling default features turns off \
     `rustls-tls`, and none of `trading`, `data` or `broker` enables a backend \
     on its own, so every HTTPS request would fail at runtime."
);

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
///
/// Because the version is part of this crate's public API, **a major or minor
/// bump of `polars` is treated as a breaking change of `alpaca-sdk`** and gets
/// one here too. `polars` is pre-1.0 and moves quickly, where a minor bump is
/// the breaking one — so in practice its release cadence, not this crate's,
/// drives how often `alpaca-sdk`'s own version has to break.
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
/// Because the version is part of this crate's public API, **a major or minor
/// bump of `rust_decimal` is treated as a breaking change of `alpaca-sdk`** and
/// gets one here too.
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
