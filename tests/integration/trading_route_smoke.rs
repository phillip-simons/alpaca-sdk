//! Every trading route no other test calls, checked for where it goes.
//!
//! Same shape as the broker smoke test: the server answers 404, so this asserts
//! the method, the version segment and the path and nothing else. See that
//! file's header for why routing is worth its own test.
//!
//! It catches one thing beyond routing, and did on the day it was written. A
//! request struct with a bare `Vec` field cannot be serialized into a query
//! string at all — `serde_urlencoded` has no representation for a sequence, so
//! reqwest fails the whole request locally and nothing reaches the wire. A test
//! like this one fails with "the server did not receive any request", which is
//! the only visible symptom that route has.

#![cfg(feature = "trading")]

use crate::common::trading_client as client;
use alpaca_sdk::Credentials;
use alpaca_sdk::trading::{
    ByClientRequestId, CreateLocateRequest, TradingClient, TransferFeeEstimateRequest,
};
use rust_decimal::Decimal;
use std::future::Future;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";

fn id() -> Uuid {
    Uuid::parse_str(ID).unwrap()
}

/// Mounts the one route the call under test must reach, runs the call against a
/// client pointed at it, and returns the requests the mock recorded.
///
/// The call is a closure rather than the server being handed back so that each
/// `MockServer` drops — which is where its `expect(1)` is verified — before the
/// next one starts. A `let server = ...` per route keeps every server in the
/// test alive until it returns, and seventy-odd listeners and runtimes across
/// nineteen concurrent tests exhausts the file descriptor limit of a stock
/// macOS shell. That failed as an unroutable request rather than as `EMFILE`:
/// the mock reported no request arrived, on a different set of routes each run.
/// The broker smoke test was converted away from the returning shape for this
/// reason; `release.yml` runs the suite on macOS, so this file had the same
/// exposure with six servers alive at once in one of its tests.
async fn expect_route<F, Fut, T, E>(
    http_method: &str,
    http_path: &str,
    call: F,
) -> Vec<wiremock::Request>
where
    F: FnOnce(TradingClient) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let server = MockServer::start().await;
    Mock::given(method(http_method))
        .and(path(http_path.to_owned()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        call(client(&server)).await.is_err(),
        "{http_method} {http_path} did not return the 404 the mock answers with"
    );
    server.received_requests().await.unwrap()
}

// ---------------------------------------------------------------- versions

/// The version segment `TradingClient::new` picks for itself.
///
/// The rest of this file leaves `RestConfig::new`'s default in place, which is
/// also `v2` — so it would keep passing if the constructor stopped choosing
/// one. It is read back off the client rather than off a mock because `new`
/// bakes in Alpaca's base URL alongside the version.
#[test]
fn the_trading_client_constructor_picks_v2() {
    let credentials = Credentials::new("key", "secret").unwrap();

    // Both environments: the paper flag picks the base URL and could just as
    // easily have been wired to pick a version.
    for paper in [true, false] {
        let trading = TradingClient::new(&credentials, paper).unwrap();
        assert_eq!(trading.rest().config().api_version, "v2");
    }
}

// -------------------------------------------------------------- watchlists

/// `:by_name` is a colon in the path, not a sub-resource — encoding it would
/// 404 — and the name itself travels in the query string.
#[tokio::test]
async fn the_by_name_watchlist_routes_keep_their_colon() {
    expect_route("GET", "/v2/watchlists:by_name", |trading| async move {
        trading.get_watchlist_by_name("mine").await
    })
    .await;

    expect_route("POST", "/v2/watchlists:by_name", |trading| async move {
        trading.add_asset_to_watchlist_by_name("mine", "AAPL").await
    })
    .await;
}

// ----------------------------------------------------------------- locates

/// Locates are `v1` on a `v2` client — the version belongs to the route, not to
/// the API surface.
#[tokio::test]
async fn a_locate_by_id_is_a_v1_route() {
    expect_route("GET", &format!("/v1/locates/{ID}"), |trading| async move {
        trading.get_locate_by_id(id()).await
    })
    .await;
}

/// The write shares `/v1/locates` with the listing, and the only existing test
/// of it stops at `validate` — a degenerate request that never reaches the
/// network. So the path looked covered and the verb was asserted by nothing.
#[tokio::test]
async fn requesting_a_locate_posts_to_the_path_the_listing_reads() {
    expect_route("POST", "/v1/locates", |trading| async move {
        // A non-positive quantity is refused before it is sent, which would
        // leave the mock never called.
        trading
            .create_locate(&CreateLocateRequest::new("TSLA", 100))
            .await
    })
    .await;
}

// ------------------------------------------------------------ tokenization

#[tokio::test]
async fn the_tokenization_reads_reach_their_paths() {
    expect_route("GET", "/v2/tokenization/requests", |trading| async move {
        trading.get_tokenization_requests(None).await
    })
    .await;

    expect_route(
        "GET",
        &format!("/v2/tokenization/requests/{ID}"),
        |trading| async move { trading.get_tokenization_request(id()).await },
    )
    .await;

    expect_route(
        "GET",
        "/v2/tokenization/requests:by_client_request_id",
        |trading| async move {
            trading
                .get_tokenization_request_by_client_id(&ByClientRequestId::new("req-1"))
                .await
        },
    )
    .await;
}

// ---------------------------------------------------------- crypto wallets

#[tokio::test]
async fn the_crypto_wallet_routes_reach_their_paths() {
    expect_route("GET", "/v2/wallets", |trading| async move {
        trading.get_crypto_wallets(None).await
    })
    .await;

    expect_route("GET", "/v2/wallets/transfers", |trading| async move {
        trading.get_crypto_transfers().await
    })
    .await;

    expect_route(
        "GET",
        "/v2/wallets/transfers/transfer-1",
        |trading| async move { trading.get_crypto_transfer("transfer-1").await },
    )
    .await;

    expect_route("GET", "/v2/wallets/whitelists", |trading| async move {
        trading.get_whitelisted_addresses().await
    })
    .await;

    expect_route(
        "DELETE",
        "/v2/wallets/whitelists/addr-1",
        |trading| async move { trading.delete_whitelisted_address("addr-1").await },
    )
    .await;

    expect_route("GET", "/v2/wallets/fees/estimate", |trading| async move {
        trading
            .estimate_transfer_fee(&TransferFeeEstimateRequest::new(
                "ETH",
                "0xfrom",
                "0xto",
                Decimal::ONE,
            ))
            .await
    })
    .await;
}
