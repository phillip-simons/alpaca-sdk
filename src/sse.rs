//! The shared [server-sent event](https://docs.alpaca.markets/us/docs/sse-events) transport.
//!
//! Alpaca pushes events on three surfaces, not one: the broker API's account,
//! trade, journal, funding, activity, admin, IPO and system streams; the trading
//! API's activity stream; and the market data API's corporate-actions stream.
//! All of them are the same thing — a plain HTTP response of
//! `text/event-stream`, read incrementally.
//!
//! None of the [`crate::data::live`] websocket machinery applies. There is no
//! subscribe message, no authentication handshake, and no reconnect state
//! machine. Resuming is the caller's job, with the cursor on the last event
//! they handled — see [`EventStreamRequest::from_id`].
//!
//! Resuming is genuinely the caller's job, and it is not optional: these are
//! push streams, so a stream that dies has *not* already delivered its events,
//! and whatever the server emitted while nobody was connected is not replayed
//! unless it is asked for by id. [`Event`] documents the two things that make
//! this sharper than it looks — a dropped-message notice the caller never sees,
//! and an end-of-stream that does not distinguish "finished" from "failed".
//!
//! This module was the broker client's private plumbing until the reference
//! sweep found seven more streams outside the broker API.

use chrono::{DateTime, Utc};
use futures_util::{Stream, StreamExt as _};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::auth::Credentials;
use crate::error::{ApiError, Error, Result};

/// The window an event stream replays before going live.
///
/// **Not interchangeable with the broker's
/// [`GetEventsRequest`](crate::broker::GetEventsRequest)**, and the difference
/// is not cosmetic: the five older streams bound their window by *date*, while
/// every stream found in the reference sweep — admin actions, IPO events, system
/// events, account activities, corporate actions — bounds it by *timestamp*. Sending a bare date to a route that parses RFC-3339 is how a
/// filter silently stops filtering.
///
/// The cursor pair is a [ULID](https://github.com/ulid/spec), and `since_id`
/// here means the ULID everywhere it is accepted — unlike the v1 broker
/// streams, where that name belongs to a deprecated integer form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EventStreamRequest {
    /// Replay events emitted at or after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// Close the connection after the last event at or before this time.
    ///
    /// Alpaca requires `since` whenever this is set, and rejects a future value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Replay events from this ULID onwards.
    ///
    /// **Whether the event with this id is itself redelivered depends on the
    /// stream**, and Alpaca's reference states it both ways: the
    /// corporate-actions stream documents it as inclusive, while the IPO-events
    /// stream says "the first event returned will be the one immediately after
    /// `since_id`". One filter type serves every stream, so this cannot promise
    /// either.
    ///
    /// Treat the first event after a resume as possibly-duplicate and
    /// deduplicate on `id`: that is correct under both readings, where assuming
    /// exclusivity drops an event on every reconnect if the stream turns out to
    /// be inclusive.
    ///
    /// Mutually exclusive with `since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_id: Option<String>,
    /// Close the connection once this ULID has been delivered, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_id: Option<String>,
}

impl EventStreamRequest {
    /// Events from `since` onwards.
    #[must_use]
    pub fn since(since: DateTime<Utc>) -> Self {
        Self {
            since: Some(since),
            ..Self::default()
        }
    }

    /// Events from this ULID onwards, which is how a dropped stream is resumed.
    ///
    /// Named `from_id` rather than `after_id` because "after" asserts an
    /// exclusivity that only some of these streams have — see
    /// [`EventStreamRequest::since_id`], and deduplicate on `id`.
    #[must_use]
    pub fn from_id(since_id: impl Into<String>) -> Self {
        Self {
            since_id: Some(since_id.into()),
            ..Self::default()
        }
    }

    /// The filter as query parameters.
    #[must_use]
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(since) = self.since {
            query.push(("since", since.to_rfc3339()));
        }
        if let Some(until) = self.until {
            query.push(("until", until.to_rfc3339()));
        }
        if let Some(since_id) = &self.since_id {
            query.push(("since_id", since_id.clone()));
        }
        if let Some(until_id) = &self.until_id {
            query.push(("until_id", until_id.clone()));
        }
        query
    }
}

/// One event from an Alpaca event stream.
///
/// # What this does not carry
///
/// SSE comment lines are not surfaced. Alpaca uses them for two things the
/// caller would want to know about — `: you are reading too slowly, dropped
/// 10000 messages`, and `: internal server error` — and the parser underneath
/// discards them before this crate sees them. Two consequences worth planning
/// around:
///
/// - A slow consumer can lose events silently. The `id` values are ULIDs and a
///   gap is not detectable from them, so a consumer that must not miss a fill
///   should reconcile against the REST API rather than trust the stream alone.
/// - The end of a stream is ambiguous. A server-side error followed by a clean
///   close is indistinguishable from the stream ending because `until_id` was
///   reached: both are `Poll::Ready(None)`. **`None` does not mean "you have
///   everything"** — a supervisor that treats it as completion will stop
///   resubscribing.
///
/// `#[non_exhaustive]` so that a comment carrier can be added without a
/// breaking change once the parser can surface one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Event {
    /// The last event id the server sent.
    ///
    /// Per the SSE specification this persists: an event that sends no `id`
    /// line keeps the previous one, so this is `None` only until the server has
    /// sent a first id. That is exactly what makes it usable to resume a
    /// dropped stream, so it is surfaced rather than discarded.
    pub id: Option<String>,
    /// The event's type.
    ///
    /// Unlike the id, this resets after every dispatch, and an event that sends
    /// no `event` line gets the SSE specification's default of `"message"` —
    /// so this always has a value, and that value is not always meaningful.
    pub name: String,
    /// The payload, as it arrived.
    pub data: String,
}

impl Event {
    /// Deserializes the payload.
    ///
    /// Most of these streams have no captured payloads to model from, so the
    /// type is the caller's to choose — [`serde_json::Value`] to look first, a
    /// struct once the shape is known.
    ///
    /// # Errors
    /// Returns [`Error::Decode`] if the payload is not valid JSON for `T`.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.data).map_err(|source| Error::Decode {
            path: "event stream".to_owned(),
            body: self.data.clone(),
            source,
        })
    }
}

impl From<eventsource_stream::Event> for Event {
    fn from(event: eventsource_stream::Event) -> Self {
        Self {
            // The parser reports "no id yet" as the empty string, which is not
            // the same thing as an id of "". Once the server has sent one it
            // persists, so this is only ever None at the head of a stream.
            id: (!event.id.is_empty()).then_some(event.id),
            name: event.event,
            data: event.data,
        }
    }
}

/// Translates a stream-level failure into this crate's error type.
///
/// All three arms are [`Error::Stream`], which is the variant the websocket
/// paths use too. The distinction the caller usually wants — did the transport
/// drop, or did the server send something unparseable — is in the message rather
/// than in the type, because it does not change what a caller can do about it.
pub(crate) fn stream_error(error: &eventsource_stream::EventStreamError<reqwest::Error>) -> Error {
    match error {
        eventsource_stream::EventStreamError::Transport(source) => {
            Error::Stream(format!("event stream failed: {source}"))
        }
        eventsource_stream::EventStreamError::Utf8(source) => {
            Error::Stream(format!("event stream was not valid utf-8: {source}"))
        }
        eventsource_stream::EventStreamError::Parser(source) => {
            Error::Stream(format!("event stream was malformed: {source}"))
        }
    }
}

/// Builds the HTTP client an event stream is read through.
///
/// Separate from [`crate::rest::RestClient`] because that one decodes a whole
/// body and these are read incrementally. Redirects are refused, as they are
/// there.
///
/// # No timeout
///
/// [`RestConfig::timeout`] is deliberately *not* applied here. reqwest's
/// `ClientBuilder::timeout` is a total deadline on the whole request, "until
/// the response body has finished" — and the body of a `text/event-stream`
/// never finishes. Applying it would give every subscription a fixed lifespan:
/// events until the deadline, then a timeout error, forever. Since
/// [`crate::Error::is_transient`] would report the reconnect as worthwhile, the obvious
/// recovery loop would reconnect on that same clock and drop whatever arrived
/// in the gap.
///
/// A stall detector belongs on the *reads*, not on the stream, so if one is
/// wanted later it is `read_timeout`, which resets per chunk.
///
/// # Errors
/// Returns an error if the credentials cannot be encoded as headers.
pub(crate) fn streaming_client(credentials: &Credentials) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    credentials.apply(&mut headers)?;
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(crate::config::user_agent()),
    );

    Ok(reqwest::Client::builder()
        .default_headers(headers)
        // No event stream redirects, so following one has no upside and some
        // downside: reqwest strips `Authorization` on a cross-host hop but not
        // the `APCA-API-*` headers the trading and data clients send, and this
        // is a long-lived credentialed connection. The one route that does
        // redirect — the broker document download — has its own client below.
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// Builds the HTTP client the broker document download uses.
///
/// Separate from [`streaming_client`] for one reason: this one *does* apply
/// [`RestConfig::timeout`]. A document download is an ordinary request/response
/// call with a body that ends, so a total deadline is exactly the right shape
/// for it — the argument for withholding one from an event stream does not
/// transfer, and folding the two together silently dropped the caller's
/// deadline from the two download routes.
///
/// Redirects are followed: the route answers `301` to a presigned storage URL.
/// Safe because the broker client converts its credentials to basic auth, which
/// reqwest strips on a cross-host hop.
///
/// # Errors
/// Returns an error if the credentials cannot be encoded as headers.
#[cfg(feature = "broker")]
pub(crate) fn download_client(
    credentials: &Credentials,
    timeout: Option<std::time::Duration>,
) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    credentials.apply(&mut headers)?;
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(crate::config::user_agent()),
    );

    let mut builder = reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10));
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    Ok(builder.build()?)
}

/// Opens an event stream at `url`.
///
/// The subscription itself is awaited so a rejected one — bad credentials, a
/// filter the server dislikes — surfaces as an error here rather than as a
/// single item on an otherwise empty stream.
///
/// `path` is only used to label errors; `url` is the absolute address, because
/// Alpaca versions these endpoints individually and the version is therefore
/// not the client's.
///
/// # Errors
/// Propagates transport failures and any non-success status the server answers
/// the subscription with.
pub(crate) async fn subscribe(
    http: &reqwest::Client,
    url: &str,
    path: &str,
    query: &[(&'static str, String)],
) -> Result<impl Stream<Item = Result<Event>> + use<>> {
    use eventsource_stream::Eventsource as _;

    let mut request = http
        .get(url)
        // The four headers Alpaca's event streams expect on a subscription.
        .header(reqwest::header::CONNECTION, "keep-alive")
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .header(reqwest::header::CONTENT_TYPE, "text/event-stream")
        .header(reqwest::header::ACCEPT, "text/event-stream");
    if !query.is_empty() {
        request = request.query(query);
    }

    let response = request.send().await.map_err(Error::from)?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(Error::Api(ApiError::from_body(status.as_u16(), path, body)));
    }

    // Nothing is retried past this point.
    Ok(response
        .bytes_stream()
        .eventsource()
        .map(|event| match event {
            Ok(event) => Ok(Event::from(event)),
            Err(error) => Err(stream_error(&error)),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_id_yet_is_none_rather_than_an_empty_string() {
        // The parser signals "the server has not sent an id" with an empty
        // string. An id of "" and no id at all are different things, and only
        // the second one is safe to resume from.
        let event = Event::from(eventsource_stream::Event {
            event: "message".to_owned(),
            data: "{}".to_owned(),
            id: String::new(),
            retry: None,
        });

        assert_eq!(event.id, None);
        assert_eq!(event.name, "message");
    }

    #[test]
    fn the_timestamp_window_renders_as_rfc_3339_rather_than_a_date() {
        // The whole reason this type exists beside the broker's date-bounded
        // filter: these streams parse RFC-3339, and a bare date sent to them is
        // a filter that quietly stops filtering.
        let filter = EventStreamRequest::since("2026-03-20T12:24:58Z".parse().unwrap());
        let query = filter.query();

        assert_eq!(query.len(), 1);
        assert_eq!(query[0].0, "since");
        assert!(query[0].1.starts_with("2026-03-20T12:24:58"));
    }

    #[test]
    fn an_empty_filter_sends_nothing() {
        assert!(EventStreamRequest::default().query().is_empty());
    }

    /// `path` labels the error, not `url` — the two differ deliberately (see
    /// [`subscribe`]'s doc comment), so a wrong implementation that reached for
    /// the request's own path instead would not be caught by asserting they
    /// happen to match.
    #[tokio::test]
    async fn a_rejected_subscription_is_labeled_with_the_given_path() {
        use wiremock::matchers::{method, path as path_matcher};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/v1/events/status"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let url = format!("{}/v1/events/status", server.uri());
        // The `Ok` side is a `Stream`, which is not `Debug`, so this cannot be
        // `unwrap_err()`.
        match subscribe(&http, &url, "/status", &[]).await {
            Err(Error::Api(api)) => {
                assert_eq!(api.status, 403);
                assert_eq!(api.path, "/status");
                assert_eq!(api.message, "forbidden");
            }
            Err(other) => panic!("expected Api, got {other:?}"),
            Ok(_) => panic!("expected the rejected subscription to error"),
        }
    }

    #[test]
    fn the_payload_deserializes_into_a_caller_chosen_type() {
        let event = Event {
            id: Some("1".to_owned()),
            name: "message".to_owned(),
            data: r#"{"status_to":"ACTIVE"}"#.to_owned(),
        };

        let value: serde_json::Value = event.json().unwrap();
        assert_eq!(value["status_to"], "ACTIVE");

        // And a mismatch is a decode error carrying the payload, not a panic.
        let error = event.json::<Vec<u8>>().unwrap_err();
        assert!(matches!(error, Error::Decode { .. }));
    }
}
