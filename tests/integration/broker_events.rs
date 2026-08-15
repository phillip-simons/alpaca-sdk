//! The five broker event streams.
//!
//! These are `text/event-stream` over plain HTTP, not websockets, so what is
//! worth pinning down is the wire contract: five distinct paths, the SSE headers
//! Alpaca's event streams expect, and a subscription that fails loudly instead
//! of handing back a silent empty stream.

#![cfg(feature = "broker")]

use crate::common::broker_client as client;
use alpaca_sdk::broker::GetEventsRequest;
use futures_util::StreamExt as _;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A `text/event-stream` body: one fully-specified event, then one that sends
/// neither an `id` nor an `event` line.
const STREAM: &str = concat!(
    "id: 1\n",
    "event: account_status\n",
    "data: {\"status_to\":\"ACTIVE\",\"account_id\":\"a\"}\n",
    "\n",
    "data: {\"status_to\":\"SUBMITTED\",\"account_id\":\"b\"}\n",
    "\n",
);

fn stream_response() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(STREAM)
}

#[tokio::test]
async fn the_stream_yields_each_event_with_its_id_and_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/events/accounts/status"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    let events: Vec<_> = client(&server)
        .get_account_status_events(None)
        .await
        .unwrap()
        .collect()
        .await;

    assert_eq!(events.len(), 2);

    let first = events[0].as_ref().unwrap();
    assert_eq!(first.id.as_deref(), Some("1"));
    assert_eq!(first.name, "account_status");
    // The payload is JSON the caller types itself; there are no captured
    // payloads for these streams to model from.
    let payload: serde_json::Value = first.json().unwrap();
    assert_eq!(payload["status_to"], "ACTIVE");

    // The second event sends neither field, and the two behave differently —
    // this is the SSE specification, not a quirk of the parser. The last event
    // ID persists until the server changes it, so it is still "1", which is
    // what makes it usable for resumption. The event *type* resets after every
    // dispatch and falls back to the spec default.
    let second = events[1].as_ref().unwrap();
    assert_eq!(second.id.as_deref(), Some("1"));
    assert_eq!(second.name, "message");
}

/// A body that breaks *after* the subscription succeeded, which is the case the
/// error type used to describe as an invalid request.
#[tokio::test]
async fn a_stream_that_breaks_mid_flight_is_a_stream_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/events/accounts/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                // One good event, then bytes that are not UTF-8 at all.
                .set_body_bytes(
                    [
                        b"id: 1\nevent: account_status\ndata: {}\n\n".as_slice(),
                        &[b'd', b'a', b't', b'a', b':', b' ', 0xff, 0xfe, b'\n', b'\n'],
                    ]
                    .concat(),
                ),
        )
        .expect(1)
        .mount(&server)
        .await;

    let events: Vec<_> = client(&server)
        .get_account_status_events(None)
        .await
        .unwrap()
        .collect()
        .await;

    assert!(events[0].is_ok(), "{:?}", events[0]);
    let failure = events
        .iter()
        .find_map(|event| event.as_ref().err())
        .expect("the malformed bytes should have produced an error");
    assert!(
        matches!(failure, alpaca_sdk::Error::Stream(_)),
        "expected Error::Stream, got {failure:?}"
    );
}

/// Mounts a mock for `expected_path` alone, so any other path 404s.
async fn server_for(expected_path: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(expected_path.to_owned()))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn every_stream_has_its_own_path() {
    // Five endpoints across two API versions, and nothing in the response
    // distinguishes them, so a transposed path would be invisible without this.
    // The versions are per stream, not per client: account status and NTA are
    // v1, trades, journals and funding are v2. Each method returns its own
    // opaque stream type, so these cannot share a loop body.
    let server = server_for("/v1/events/accounts/status").await;
    let events: Vec<_> = client(&server)
        .get_account_status_events(None)
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 2);

    let server = server_for("/v2/events/trades").await;
    let events: Vec<_> = client(&server)
        .get_trade_events(None)
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 2);

    let server = server_for("/v2/events/journals/status").await;
    let events: Vec<_> = client(&server)
        .get_journal_events(None)
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 2);

    let server = server_for("/v2/events/funding/status").await;
    let events: Vec<_> = client(&server)
        .get_transfer_events(None)
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 2);

    let server = server_for("/v1/events/nta").await;
    let events: Vec<_> = client(&server)
        .get_non_trading_activity_events(None)
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 2);
}

/// Two filters unique to the non-trading-activity stream. `EventVersion::query`
/// builds the query by hand rather than through serde, so a field added to
/// `GetEventsRequest` without a line there would compile, serialize in
/// isolation, and never reach the wire.
#[tokio::test]
async fn the_nta_stream_sends_its_own_two_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/events/nta"))
        .and(query_param(
            "group_id",
            "9b6c7c1a-9eb2-4d4a-8a3a-1bf4c1d5cbaa",
        ))
        .and(query_param("include_preprocessing", "true"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetEventsRequest::default();
    filter.group_id = Some("9b6c7c1a-9eb2-4d4a-8a3a-1bf4c1d5cbaa".parse().unwrap());
    filter.include_preprocessing = Some(true);

    drop(
        client(&server)
            .get_non_trading_activity_events(Some(&filter))
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn the_subscription_sends_the_sse_headers_and_the_filter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/events/trades"))
        .and(header("accept", "text/event-stream"))
        .and(header("cache-control", "no-cache"))
        .and(header("content-type", "text/event-stream"))
        // Still the broker API, so still authenticated.
        .and(header(
            "authorization",
            "Basic YnJva2VyLWtleTpicm9rZXItc2VjcmV0",
        ))
        .and(query_param("since", "2022-02-01"))
        .and(query_param("until", "2022-02-28"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetEventsRequest::default();
    filter.since = Some("2022-02-01".parse().unwrap());
    filter.until = Some("2022-02-28".parse().unwrap());

    // The mock's `expect(1)` is the assertion; the stream itself is not needed.
    drop(
        client(&server)
            .get_trade_events(Some(&filter))
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn resuming_sends_the_event_id_that_was_last_seen() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/events/journals/status"))
        .and(query_param("id", "42"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    // The id comes off a BrokerEvent, which is why that field is kept —
    // Discarding it would leave a dropped stream with nothing to resume from.
    drop(
        client(&server)
            .get_journal_events(Some(&GetEventsRequest::after_id("42")))
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn the_ulid_cursor_is_spelled_for_the_version_being_called() {
    // On v2 the ULID goes out as since_id. On v1 that name belongs to a
    // deprecated integer form, so the ULID has to go out as since_ulid instead.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/events/trades"))
        .and(query_param("since_id", "01ARZ3NDEKTSV4RRFFQ69G5FAV"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    drop(
        client(&server)
            .get_trade_events(Some(&GetEventsRequest::from_id(
                "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            )))
            .await
            .unwrap(),
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/events/accounts/status"))
        .and(query_param("since_ulid", "01ARZ3NDEKTSV4RRFFQ69G5FAV"))
        .respond_with(stream_response())
        .expect(1)
        .mount(&server)
        .await;

    drop(
        client(&server)
            .get_account_status_events(Some(&GetEventsRequest::from_id(
                "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            )))
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn the_streams_alpaca_retired_are_not_called() {
    // /v1/events/trades is documented as fully deprecated and no longer
    // available, as are the legacy v1 journal and transfer streams. Mounting *only* the retired paths means any request
    // to one fails the test — the client must not go near them.
    for retired in [
        "/v1/events/trades",
        "/v1/events/journals/status",
        "/v1/events/transfers/status",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(retired))
            .respond_with(stream_response())
            .expect(0)
            .mount(&server)
            .await;

        // Every one of these now targets a v2 path, so the mock server answers
        // 404 and the subscription fails rather than silently hitting a dead
        // route.
        let client = client(&server);
        assert!(client.get_trade_events(None).await.is_err(), "{retired}");
        assert!(client.get_journal_events(None).await.is_err(), "{retired}");
        assert!(client.get_transfer_events(None).await.is_err(), "{retired}");
    }
}

#[tokio::test]
async fn a_rejected_subscription_fails_instead_of_returning_an_empty_stream() {
    // Awaiting the response before handing back the stream is what makes this
    // possible: a 403 here would otherwise look like a stream that simply had
    // nothing to say.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/events/nta"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({ "code": 40110000, "message": "forbidden" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    // `unwrap_err` is unavailable here: the Ok side is an opaque stream type
    // that does not implement Debug.
    let Err(error) = client(&server).get_non_trading_activity_events(None).await else {
        panic!("a 403 subscription must not produce a stream");
    };

    assert_eq!(error.status(), Some(403));
}

/// An event stream refuses a redirect.
///
/// This is the credential-leak fix, and it needs pinning because the failure
/// mode is silent: reqwest strips `Authorization`, `Cookie` and
/// `Proxy-Authorization` on a cross-host hop and *nothing else*, so the trading
/// and market-data clients' `APCA-API-*` headers would ride along to wherever a
/// `Location` pointed. Folding the stream client back together with the one that
/// follows redirects — which is what the bug was — would pass CI without this.
#[tokio::test]
async fn an_event_stream_does_not_follow_a_redirect() {
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("data: {}\n\n"))
        // Nothing may reach the redirect target.
        .expect(0)
        .mount(&elsewhere)
        .await;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/events/journals/status"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", elsewhere.uri().as_str()),
        )
        .expect(1)
        .mount(&server)
        .await;

    // The redirect surfaces as the non-success status it is, rather than being
    // followed to another host. The `Ok` side is a `Stream`, which is not
    // `Debug`, so this cannot be `expect_err`.
    match client(&server).get_journal_events(None).await {
        Err(alpaca_sdk::Error::Api(api)) => assert_eq!(api.status, 302),
        Err(other) => panic!("expected the 302 to surface as an API error, got {other:?}"),
        Ok(_) => panic!("a redirect must not be followed"),
    }
}
