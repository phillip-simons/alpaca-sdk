//! Transport-level behavior that the rest of the SDK depends on: retry counts,
//! which statuses retry, header wiring, and body handling.
//!
//! Each assertion pins a behavior ported from `alpaca/common/rest.py`.

use std::time::Duration;

use alpaca_sdk::{Credentials, Error, RestClient, RestConfig, RetryConfig};
use serde::Deserialize;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Account {
    id: String,
}

/// A client with retries that do not sleep, so retry tests stay fast.
fn client(server: &MockServer, retry: RetryConfig) -> RestClient {
    let creds = Credentials::new("test-key", "test-secret").unwrap();
    RestClient::new(&creds, RestConfig::new(server.uri()).retry(retry)).unwrap()
}

fn instant_retry() -> RetryConfig {
    RetryConfig::default().wait(Duration::ZERO)
}

#[tokio::test]
async fn sends_auth_and_user_agent_headers() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .and(header("APCA-API-KEY-ID", "test-key"))
        .and(header("APCA-API-SECRET-KEY", "test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "abc"})))
        .expect(1)
        .mount(&server)
        .await;

    let account: Account = client(&server, RetryConfig::none())
        .get("/account", &())
        .await
        .unwrap();

    assert_eq!(account.id, "abc");

    let request = &server.received_requests().await.unwrap()[0];
    let user_agent = request.headers.get("user-agent").unwrap().to_str().unwrap();
    assert!(user_agent.starts_with("APCA-RS/"), "{user_agent}");
    assert!(user_agent.contains(" Rust/"), "{user_agent}");
}

#[tokio::test]
async fn get_sends_parameters_as_query_string() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("status", "open"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let _: Vec<Account> = client(&server, RetryConfig::none())
        .get("/orders", &[("status", "open"), ("limit", "50")])
        .await
        .unwrap();
}

#[tokio::test]
async fn post_sends_a_json_body() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .and(body_json(json!({"symbol": "AAPL", "qty": "1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "order-1"})))
        .expect(1)
        .mount(&server)
        .await;

    let order: Account = client(&server, RetryConfig::none())
        .post("/orders", &json!({"symbol": "AAPL", "qty": "1"}))
        .await
        .unwrap();

    assert_eq!(order.id, "order-1");
}

#[tokio::test]
async fn delete_sends_parameters_as_query_string_not_body() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v2/positions"))
        .and(query_param("cancel_orders", "true"))
        .respond_with(ResponseTemplate::new(207).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let _: Vec<Account> = client(&server, RetryConfig::none())
        .delete("/positions", &[("cancel_orders", "true")])
        .await
        .unwrap();
}

#[tokio::test]
async fn no_content_response_decodes_as_unit() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v2/orders/abc"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let () = client(&server, RetryConfig::none())
        .delete("/orders/abc", &())
        .await
        .unwrap();
}

#[tokio::test]
async fn retries_429_then_succeeds() {
    let server = MockServer::start().await;

    // wiremock matches mounted mocks in order, first match wins, so the
    // exhaustible 429 must be mounted ahead of the 200 that follows it.
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "abc"})))
        .expect(1)
        .mount(&server)
        .await;

    let account: Account = client(&server, instant_retry())
        .get("/account", &())
        .await
        .unwrap();

    assert_eq!(account.id, "abc");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn issues_four_requests_before_giving_up() {
    let server = MockServer::start().await;

    // alpaca-py's loop performs DEFAULT_RETRY_ATTEMPTS retries *after* the
    // initial request, so the default policy makes four requests in total.
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "code": 42_910_000,
            "message": "rate limit exceeded"
        })))
        .expect(4)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    match err {
        Error::RetriesExhausted { attempts, last } => {
            assert_eq!(attempts, 4);
            assert_eq!(last.status, 429);
            assert_eq!(last.code, Some(42_910_000));
            assert_eq!(last.message, "rate limit exceeded");
        }
        other => panic!("expected RetriesExhausted, got {other:?}"),
    }
}

#[tokio::test]
async fn retries_504_as_well_as_429() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(504))
        .expect(4)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    assert!(matches!(err, Error::RetriesExhausted { .. }));
}

#[tokio::test]
async fn does_not_retry_other_error_statuses() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "code": 40_110_000,
            "message": "forbidden"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    match err {
        Error::Api(ref api) => {
            assert_eq!(api.status, 403);
            assert_eq!(api.code, Some(40_110_000));
            assert_eq!(api.path, "/account");
        }
        ref other => panic!("expected Api, got {other:?}"),
    }
    assert_eq!(err.status(), Some(403));
}

#[tokio::test]
async fn server_error_body_that_is_not_json_still_surfaces() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(500).set_body_string("<html>oops</html>"))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    match err {
        Error::Api(api) => {
            assert_eq!(api.code, None);
            assert_eq!(api.message, "<html>oops</html>");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[tokio::test]
async fn redirects_are_not_followed() {
    // A base URL that redirects must fail rather than replay the request — and
    // its auth headers — against the redirect target.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(
            ResponseTemplate::new(301).insert_header("location", "https://evil.example/v2/account"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, RetryConfig::none())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    assert_eq!(err.status(), Some(301));
}

#[tokio::test]
async fn decode_error_reports_the_path_and_body() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"unexpected": true})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, RetryConfig::none())
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();

    match err {
        Error::Decode { path, body, .. } => {
            assert_eq!(path, "/account");
            assert!(body.contains("unexpected"), "{body}");
        }
        other => panic!("expected Decode, got {other:?}"),
    }
}
