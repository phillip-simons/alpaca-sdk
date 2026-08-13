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

use alpaca_sdk::trading::{ByClientRequestId, TradingClient, TransferFeeEstimateRequest};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ID: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";

fn client(server: &MockServer) -> TradingClient {
    let credentials = Credentials::new("key", "secret").unwrap();
    TradingClient::with_config(
        &credentials,
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap()
}

fn id() -> Uuid {
    Uuid::parse_str(ID).unwrap()
}

async fn expect_route(http_method: &str, http_path: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(http_method))
        .and(path(http_path.to_owned()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    server
}

// -------------------------------------------------------------- watchlists

/// `:by_name` is a colon in the path, not a sub-resource — encoding it would
/// 404 — and the name itself travels in the query string.
#[tokio::test]
async fn the_by_name_watchlist_routes_keep_their_colon() {
    let server = expect_route("GET", "/v2/watchlists:by_name").await;
    assert!(client(&server).get_watchlist_by_name("mine").await.is_err());

    let server = expect_route("POST", "/v2/watchlists:by_name").await;
    assert!(
        client(&server)
            .add_asset_to_watchlist_by_name("mine", "AAPL")
            .await
            .is_err()
    );
}

// ----------------------------------------------------------------- locates

/// Locates are `v1` on a `v2` client — the version belongs to the route, not to
/// the API surface.
#[tokio::test]
async fn a_locate_by_id_is_a_v1_route() {
    let server = expect_route("GET", &format!("/v1/locates/{ID}")).await;
    assert!(client(&server).get_locate_by_id(id()).await.is_err());
}

// ------------------------------------------------------------ tokenization

#[tokio::test]
async fn the_tokenization_reads_reach_their_paths() {
    let server = expect_route("GET", "/v2/tokenization/requests").await;
    assert!(
        client(&server)
            .get_tokenization_requests(None)
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v2/tokenization/requests/{ID}")).await;
    assert!(
        client(&server)
            .get_tokenization_request(id())
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v2/tokenization/requests:by_client_request_id").await;
    assert!(
        client(&server)
            .get_tokenization_request_by_client_id(&ByClientRequestId::new("req-1"))
            .await
            .is_err()
    );
}

// ---------------------------------------------------------- crypto wallets

#[tokio::test]
async fn the_crypto_wallet_routes_reach_their_paths() {
    let server = expect_route("GET", "/v2/wallets").await;
    assert!(client(&server).get_crypto_wallets(None).await.is_err());

    let server = expect_route("GET", "/v2/wallets/transfers").await;
    assert!(client(&server).get_crypto_transfers().await.is_err());

    let server = expect_route("GET", "/v2/wallets/transfers/transfer-1").await;
    assert!(
        client(&server)
            .get_crypto_transfer("transfer-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v2/wallets/whitelists").await;
    assert!(client(&server).get_whitelisted_addresses().await.is_err());

    let server = expect_route("DELETE", "/v2/wallets/whitelists/addr-1").await;
    assert!(
        client(&server)
            .delete_whitelisted_address("addr-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v2/wallets/fees/estimate").await;
    assert!(
        client(&server)
            .estimate_transfer_fee(&TransferFeeEstimateRequest::new(
                "ETH",
                "0xfrom",
                "0xto",
                Decimal::ONE
            ))
            .await
            .is_err()
    );
}
