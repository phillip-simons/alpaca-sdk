//! What a caller sees when something fails.
//!
//! `Error` is the one type every method in the crate can hand back, so its
//! `Display`, its `source` chain and the two classifiers on it are public API in
//! the strongest sense: they are what a caller writes their own error handling
//! against.

use std::error::Error as _;

use alpaca_sdk::{Error, RestClient, RestConfig, RetryConfig};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> RestClient {
    let credentials = alpaca_sdk::Credentials::new("key", "secret").unwrap();
    RestClient::new(
        &credentials,
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap()
}

/// Serves one status and body on `/v2/thing`.
async fn failing(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/thing"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn error_from(server: &MockServer) -> Error {
    client(server)
        .get::<serde_json::Value, _>("/thing", &())
        .await
        .unwrap_err()
}

// ---------------------------------------------------------------- display

/// The message a caller logs. It has to name the status, the path and Alpaca's
/// own text, because those three are what turns a support conversation into a
/// one-line answer.
#[tokio::test]
async fn an_api_error_reads_as_a_sentence() {
    let server = failing(
        403,
        json!({"code": 40_310_000, "message": "insufficient buying power"}),
    )
    .await;

    let error = error_from(&server).await;
    let text = error.to_string();

    assert!(text.contains("403"), "{text}");
    assert!(text.contains("/thing"), "{text}");
    assert!(text.contains("40310000"), "{text}");
    assert!(text.contains("insufficient buying power"), "{text}");
}

/// `ApiError` is `#[non_exhaustive]` and has no public constructor, so the only
/// way to get one — here or in a caller's own tests — is to make a request fail.
#[tokio::test]
async fn an_api_error_without_a_code_omits_the_parenthetical() {
    let server = failing(500, json!({"message": "boom"})).await;

    let error = error_from(&server).await;
    let text = error.to_string();

    assert!(text.contains("500"), "{text}");
    assert!(text.contains("boom"), "{text}");
    assert!(!text.contains("code"), "{text}");

    match error {
        Error::Api(api) => assert_eq!(api.code, None),
        other => panic!("expected Api, got {other:?}"),
    }
}

// ------------------------------------------------------------- classifiers

#[tokio::test]
async fn status_reports_the_code_for_the_failures_that_have_one() {
    let server = failing(404, json!({"message": "not found"})).await;
    assert_eq!(error_from(&server).await.status(), Some(404));

    // A locally rejected request never reached a server, so it has no status.
    assert_eq!(Error::InvalidRequest("no".to_owned()).status(), None);
    assert_eq!(Error::Stream("dropped".to_owned()).status(), None);
    assert_eq!(Error::Credentials("bad".to_owned()).status(), None);
}

#[tokio::test]
async fn retries_exhausted_reports_the_last_status_and_the_count() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/thing"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"message": "slow down"})))
        .mount(&server)
        .await;

    let credentials = alpaca_sdk::Credentials::new("key", "secret").unwrap();
    let client = RestClient::new(
        &credentials,
        RestConfig::new(server.uri()).retry(RetryConfig::default().wait(std::time::Duration::ZERO)),
    )
    .unwrap();

    let error = client
        .get::<serde_json::Value, _>("/thing", &())
        .await
        .unwrap_err();

    assert_eq!(error.status(), Some(429));
    match error {
        Error::RetriesExhausted { attempts, last } => {
            assert_eq!(attempts, 4);
            assert_eq!(last.message, "slow down");
            // The final failure is the `source`, so `{:#}`-style chains reach it.
            assert!(
                Error::RetriesExhausted { attempts, last }
                    .source()
                    .is_some()
            );
        }
        other => panic!("expected RetriesExhausted, got {other:?}"),
    }
}

#[tokio::test]
async fn is_retryable_agrees_with_the_default_policy() {
    for status in [429u16, 504] {
        let server = failing(status, json!({"message": "later"})).await;
        assert!(
            error_from(&server).await.is_retryable(),
            "{status} should be retryable"
        );
    }

    for status in [400u16, 401, 403, 404, 500] {
        let server = failing(status, json!({"message": "no"})).await;
        assert!(
            !error_from(&server).await.is_retryable(),
            "{status} should not be retryable"
        );
    }

    // Nothing the crate rejected locally is worth retrying.
    assert!(!Error::InvalidRequest("no".to_owned()).is_retryable());
    assert!(!Error::Stream("dropped".to_owned()).is_retryable());
}

// ----------------------------------------------------------------- decode

/// A response that arrives and does not fit keeps the body, so the mismatch can
/// be diagnosed without re-issuing the request — which for a trade is not
/// something a caller can safely do twice.
#[tokio::test]
async fn a_decode_failure_keeps_the_path_and_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/thing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"unexpected": true})))
        .mount(&server)
        .await;

    #[derive(Debug, serde::Deserialize)]
    struct Expected {
        #[allow(dead_code)]
        id: String,
    }

    let error = client(&server)
        .get::<Expected, _>("/thing", &())
        .await
        .unwrap_err();

    match error {
        Error::Decode { path, body, source } => {
            assert_eq!(path, "/thing");
            assert!(body.contains("unexpected"), "{body}");
            assert!(!source.to_string().is_empty());
        }
        other => panic!("expected Decode, got {other:?}"),
    }
}

/// An error body that is not JSON at all — a gateway's HTML — must still reach
/// the caller rather than failing while being reported.
#[tokio::test]
async fn a_non_json_error_body_survives_as_the_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/thing"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"))
        .mount(&server)
        .await;

    match error_from(&server).await {
        Error::Api(api) => {
            assert_eq!(api.code, None);
            assert_eq!(api.message, "<html>bad gateway</html>");
            assert_eq!(api.body, "<html>bad gateway</html>");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

// ------------------------------------------------------------- transport

#[tokio::test]
async fn a_connection_failure_is_a_transport_error_and_is_retryable() {
    let credentials = alpaca_sdk::Credentials::new("key", "secret").unwrap();
    // Port 1 refuses connections everywhere.
    let client = RestClient::new(
        &credentials,
        RestConfig::new("http://127.0.0.1:1").retry(RetryConfig::none()),
    )
    .unwrap();

    let error = client
        .get::<serde_json::Value, _>("/thing", &())
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Transport(_)), "{error:?}");
    assert_eq!(error.status(), None);
    assert!(
        error.is_retryable(),
        "a refused connection is worth retrying"
    );
    assert!(error.source().is_some(), "the reqwest error is the source");
}

// ----------------------------------------------------------- credentials

#[test]
fn structurally_invalid_credentials_are_rejected_before_any_request() {
    let error = alpaca_sdk::Credentials::new("", "secret").unwrap_err();

    assert!(matches!(error, Error::Credentials(_)), "{error:?}");
    assert!(error.to_string().contains("credentials"), "{error}");
}
