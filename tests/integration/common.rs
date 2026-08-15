//! The helpers every other module in this binary was defining for itself.
//!
//! `fixture` was written out verbatim in fifteen files, the broker client
//! builder in ten and `credentials` in six. That is not just repetition: the
//! copies had already drifted. One `fixture` dropped the path from its panic
//! message, so a typo in a fixture name failed as a bare "No such file or
//! directory" with no clue which name was wrong; another returns the file
//! unparsed, because the module it lives in asserts on the raw text.
//!
//! The drift is kept rather than flattened — [`fixture_str`] is the raw-text
//! variant under its own name — because forcing one shape on both would mean
//! re-parsing a payload the caller is about to compare as a string.
//!
//! Two things stay local to their modules on purpose:
//!
//! - `trading_stream` and `live_smoke` build their own credentials. The first
//!   asserts the key back out of the websocket auth frame, so its values are
//!   part of the test; the second reads the environment.
//! - `error_surface` and `rest_transport` build a bare [`RestClient`], not a
//!   surface client, and parameterise it differently again.

// Which helpers are reachable depends on the feature set, and a build with no
// surface enabled at all reaches none of them.
#![allow(dead_code)]

use alpaca_sdk::Credentials;
#[cfg(any(feature = "broker", feature = "trading"))]
use alpaca_sdk::{RestConfig, RetryConfig};
#[cfg(any(feature = "broker", feature = "trading"))]
use wiremock::MockServer;

/// Reads `fixtures/<name>` and parses it.
///
/// The panic names the path it was reading: the failure this catches is a
/// mistyped fixture name, and `std::io::Error` alone does not say which one.
pub(crate) fn fixture(name: &str) -> serde_json::Value {
    let body = fixture_str(name);
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("parsing fixtures/{name}: {e}\n\n{body}"))
}

/// Reads `fixtures/<name>` without parsing it.
///
/// For the modules that assert against the raw text — whether a field is
/// present at all, or what a number looks like before serde sees it.
pub(crate) fn fixture_str(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The key pair the mock-server tests authenticate with.
pub(crate) fn credentials() -> Credentials {
    Credentials::new("key", "secret").unwrap()
}

/// A broker client pointed at `server`.
///
/// The key pair is not arbitrary: `broker_accounts` asserts the basic-auth
/// header this produces, base64 of `broker-key:broker-secret`.
#[cfg(feature = "broker")]
pub(crate) fn broker_client(server: &MockServer) -> alpaca_sdk::broker::BrokerClient {
    let credentials = Credentials::new("broker-key", "broker-secret").unwrap();
    alpaca_sdk::broker::BrokerClient::with_config(
        &credentials,
        RestConfig::new(server.uri())
            .api_version("v1")
            .retry(RetryConfig::none()),
    )
    .unwrap()
}

/// A trading client pointed at `server`, on `RestConfig`'s default `v2`.
#[cfg(feature = "trading")]
pub(crate) fn trading_client(server: &MockServer) -> alpaca_sdk::trading::TradingClient {
    alpaca_sdk::trading::TradingClient::with_config(
        &credentials(),
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap()
}
