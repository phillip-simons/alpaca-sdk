//! The `blocking` feature: the async clients, driven synchronously.
//!
//! The interesting case is not that a call works. It is what happens when the
//! wrapper is used from inside an async context, which is where a naive
//! `block_on` panics.

#![cfg(all(feature = "blocking", feature = "trading"))]

use crate::common::fixture;
use alpaca_sdk::blocking::Blocking;
use alpaca_sdk::trading::TradingClient;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts the account route and hands back the running server.
///
/// The runtime built here is scratch. wiremock runs its server on a thread and
/// a runtime of its own, so this one exists only to drive the async setup calls
/// and is dropped before the blocking client is used — which is the point: the
/// test thread must not be inside a runtime when it calls the façade.
fn account_server() -> MockServer {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
                "trading/test_account_routes__test_get_account__01.json",
            )))
            .expect(1)
            .mount(&server)
            .await;
        server
    })
}

fn blocking_client(server: &MockServer) -> Blocking<TradingClient> {
    let credentials = Credentials::new("key", "secret").unwrap();
    let client = TradingClient::with_config(
        &credentials,
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap();
    Blocking::new(client).unwrap()
}

#[test]
fn a_blocking_call_reaches_the_server_and_decodes() {
    let server = account_server();
    let client = blocking_client(&server);

    let account = client.call(|client| client.get_account()).unwrap();
    assert!(!account.id.is_nil());
}

/// Two calls on one wrapper, because a runtime that had been consumed or shut
/// down by the first would only show up on the second.
#[test]
fn the_runtime_survives_more_than_one_call() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
                "trading/test_account_routes__test_get_account__01.json",
            )))
            .expect(2)
            .mount(&server)
            .await;
        server
    });
    drop(runtime);

    let client = blocking_client(&server);
    let first = client.call(|client| client.get_account()).unwrap();
    let second = client.call(|client| client.get_account()).unwrap();
    assert_eq!(first.id, second.id);
}

/// The trap that needs no call at all. `Runtime::drop` blocks until its threads
/// stop, and blocking is not allowed inside an async context — so a `Blocking`
/// that merely goes out of scope in an async fn used to take the process down.
#[tokio::test]
async fn dropping_inside_a_runtime_does_not_panic() {
    let credentials = Credentials::new("key", "secret").unwrap();
    let client = TradingClient::with_config(
        &credentials,
        RestConfig::new("http://127.0.0.1:1").retry(RetryConfig::none()),
    )
    .unwrap();

    drop(Blocking::new(client).unwrap());
}

/// tokio panics rather than returning an error when a runtime is blocked on from
/// inside another one, and an `#[tokio::main]` fn is exactly where someone tries
/// this first. The failure is reported instead.
#[tokio::test]
async fn calling_from_inside_a_runtime_is_an_error_not_a_panic() {
    let credentials = Credentials::new("key", "secret").unwrap();
    let client = TradingClient::with_config(
        &credentials,
        RestConfig::new("http://127.0.0.1:1").retry(RetryConfig::none()),
    )
    .unwrap();

    // Building the wrapper inside a runtime is fine; only blocking on it is not.
    let blocking = Blocking::new(client).unwrap();

    let error = blocking.call(|client| client.get_account()).unwrap_err();
    match error {
        alpaca_sdk::Error::InvalidRequest(message) => {
            assert!(message.contains("inside an async runtime"), "{message}");
        }
        other => panic!("expected InvalidRequest, got {other:?}"),
    }
}

/// The bridge that used to be refused.
///
/// `spawn_blocking` threads carry an ambient runtime handle, so a check for one
/// rejected them — but they are not driving the reactor, and blocking on them is
/// exactly what they exist for. The call works, and this is how an async program
/// reaches the façade.
#[tokio::test(flavor = "multi_thread")]
async fn a_call_from_spawn_blocking_succeeds() {
    let server = tokio::task::spawn_blocking(account_server).await.unwrap();
    let uri = server.uri();

    let account = tokio::task::spawn_blocking(move || {
        let credentials = Credentials::new("key", "secret").unwrap();
        let client = TradingClient::with_config(
            &credentials,
            RestConfig::new(uri).retry(RetryConfig::none()),
        )
        .unwrap();
        Blocking::new(client)
            .unwrap()
            .call(|client| client.get_account())
    })
    .await
    .unwrap()
    .expect("spawn_blocking is the supported bridge into the blocking façade");

    assert!(!account.id.is_nil());
    drop(server);
}
