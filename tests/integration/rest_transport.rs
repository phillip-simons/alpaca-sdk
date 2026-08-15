//! Transport-level behavior that the rest of the SDK depends on: retry counts,
//! which statuses retry, header wiring, and body handling.
//!
//! Each assertion pins a behavior ported from `alpaca/common/rest.py`.

use std::time::{Duration, Instant};

use alpaca_sdk::{Credentials, Error, RestClient, RestConfig, RetryBackoff, RetryConfig};
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

    // `attempts` counts retries *after* the initial request, so the default
    // policy makes four requests in total.
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

/// The one test here that actually sleeps, because it is the only way to tell a
/// growing delay from a flat one from outside the crate.
///
/// Four retries at a 40ms base: exponential waits at least
/// `20 + 40 + 80 + 160 = 300ms` — the jitter window is `[capped / 2, capped]`,
/// so that is the floor, not the expectation — where a flat 40ms would total
/// 160ms. The two ranges do not overlap, so the assertion cannot pass under the
/// old behaviour.
#[tokio::test]
async fn the_delay_between_retries_grows() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(429))
        .expect(5)
        .mount(&server)
        .await;

    let retry = RetryConfig::default()
        .attempts(4)
        .wait(Duration::from_millis(40))
        .backoff(RetryBackoff::Exponential {
            max: Duration::from_secs(30),
        });

    let started = Instant::now();
    let err = client(&server, retry)
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert!(matches!(err, Error::RetriesExhausted { .. }), "{err:?}");
    assert!(
        elapsed >= Duration::from_millis(280),
        "waited {elapsed:?}, which is flat-wait territory, not exponential"
    );
}

/// A 429 carrying `Retry-After` is stating the answer the curve is guessing at,
/// so it wins. The assertion is on the wall clock rather than on a delay value:
/// with the header ignored, a 10-second base doubling three times takes over a
/// minute, so the two outcomes cannot be confused.
///
/// The bound is 5s rather than the 1s it was, because 1s was measuring the
/// runner as much as the client — four round trips against a loaded CI box can
/// take that long with the header honoured perfectly. 5s is still nowhere near
/// the 70s the curve would cost, so the discrimination is untouched.
#[tokio::test]
async fn retry_after_overrides_the_backoff_curve() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(4)
        .mount(&server)
        .await;

    let retry = RetryConfig::default().wait(Duration::from_secs(10));

    let started = Instant::now();
    let err = client(&server, retry)
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert!(matches!(err, Error::RetriesExhausted { .. }), "{err:?}");
    assert!(
        elapsed < Duration::from_secs(5),
        "waited {elapsed:?}; the curve was used and the header ignored"
    );
}

/// The value comes from the other end, so it is clamped rather than trusted.
/// Without the clamp this test would take an hour.
#[tokio::test]
async fn an_absurd_retry_after_is_clamped_to_the_ceiling() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "3600"))
        .expect(2)
        .mount(&server)
        .await;

    let retry = RetryConfig::default()
        .attempts(1)
        .backoff(RetryBackoff::Exponential {
            max: Duration::from_millis(50),
        });

    let started = Instant::now();
    let err = client(&server, retry)
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert!(matches!(err, Error::RetriesExhausted { .. }), "{err:?}");
    assert!(
        elapsed < Duration::from_secs(5),
        "waited {elapsed:?}; the ceiling was not applied"
    );
}

/// A header this crate does not read must not become a zero-length wait. The
/// HTTP-date form falls back to the curve, which is the pre-existing behaviour.
#[tokio::test]
async fn an_http_date_retry_after_falls_back_to_the_curve() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT"),
        )
        .expect(3)
        .mount(&server)
        .await;

    let retry = RetryConfig::default()
        .attempts(2)
        .wait(Duration::from_millis(60))
        .backoff(RetryBackoff::Exponential {
            max: Duration::from_secs(30),
        });

    let started = Instant::now();
    let err = client(&server, retry)
        .get::<Account, _>("/account", &())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert!(matches!(err, Error::RetriesExhausted { .. }), "{err:?}");
    // Two waits, jittered from a 60ms base: the floor is half of 60 plus half
    // of 120. A parsed-as-zero header would total nothing.
    assert!(
        elapsed >= Duration::from_millis(90),
        "waited {elapsed:?}; the unparsed header appears to have been read as zero"
    );
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

// ---------------------------------------------------------------------------
// Which requests may be replayed
//
// The retry policy is a set of status codes, and a status code alone cannot say
// whether replaying is safe. Every test above this line mounts a `GET`, which is
// why a 504 replaying a `POST` went unnoticed: on the default policy one
// `submit_order` against a gateway timeout placed four orders, and the caller
// was handed `RetriesExhausted` — so they believed none had been placed.
// ---------------------------------------------------------------------------

/// The regression test for that. A `POST` that 504s is reported, not replayed.
#[tokio::test]
async fn a_post_is_not_replayed_on_a_gateway_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(504).set_body_string("gateway timeout"))
        // The whole point: exactly one request reaches the server.
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .post::<Account, _>("/orders", &json!({"symbol": "AAPL"}))
        .await
        .unwrap_err();

    // And it surfaces as the API error it is, rather than as `RetriesExhausted`
    // — which would have claimed four attempts were made.
    match err {
        Error::Api(api) => assert_eq!(api.status, 504),
        other => panic!("expected Api(504), got {other:?}"),
    }
}

/// `PATCH` is not idempotent either, and Alpaca patches account configuration.
#[tokio::test]
async fn a_patch_is_not_replayed_on_a_gateway_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("PATCH"))
        .and(path("/v2/account/configurations"))
        .respond_with(ResponseTemplate::new(504))
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .patch::<Account, _>("/account/configurations", &json!({}))
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Api(api) if api.status == 504));
}

/// A 429 is the exception, and the reason the rule reads the status as well as
/// the method: the rate limiter refuses the request before anything acts on it,
/// so nothing was done and replaying a `POST` is safe.
#[tokio::test]
async fn a_post_is_replayed_on_a_rate_limit() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "abc"})))
        .expect(1)
        .mount(&server)
        .await;

    let account: Account = client(&server, instant_retry())
        .post("/orders", &json!({"symbol": "AAPL"}))
        .await
        .unwrap();

    assert_eq!(account.id, "abc");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

/// `DELETE` is idempotent in HTTP's sense, so a 504 on one may be replayed —
/// cancelling an order twice cancels it once.
#[tokio::test]
async fn an_idempotent_method_is_still_replayed_on_a_gateway_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v2/orders/abc"))
        .respond_with(ResponseTemplate::new(504))
        .expect(4)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .delete::<Account, _>("/orders/abc", &())
        .await
        .unwrap_err();

    assert!(matches!(err, Error::RetriesExhausted { attempts: 4, .. }));
}

/// But HTTP's sense is not the only one. `DELETE /v2/positions/{asset}` does not
/// remove a record — it submits a liquidating market order. If Alpaca accepts
/// that order and the gateway drops the response, the position is still open
/// when the replay arrives, so the replay sells the same quantity again and puts
/// the caller short. That is the `POST` failure wearing a different verb, and
/// deferring to `Method::is_idempotent` alone would replay it.
#[tokio::test]
async fn a_delete_that_submits_an_order_is_not_replayed() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v2/positions/AAPL"))
        .respond_with(ResponseTemplate::new(504).set_body_string("gateway timeout"))
        // Exactly one liquidation reaches the server.
        .expect(1)
        .mount(&server)
        .await;

    let err = client(&server, instant_retry())
        .delete_effectful::<Account, _>("/positions/AAPL", &())
        .await
        .unwrap_err();

    match err {
        Error::Api(api) => assert_eq!(api.status, 504),
        other => panic!("expected Api(504), got {other:?}"),
    }
}

/// And a 429 still replays it, for the same reason it replays a `POST`: the rate
/// limiter refused the request before anything acted on it.
#[tokio::test]
async fn a_rate_limited_effectful_delete_is_still_replayed() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v2/positions/AAPL"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/v2/positions/AAPL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "abc"})))
        .expect(1)
        .mount(&server)
        .await;

    let account: Account = client(&server, instant_retry())
        .delete_effectful("/positions/AAPL", &())
        .await
        .unwrap();

    assert_eq!(account.id, "abc");
}
