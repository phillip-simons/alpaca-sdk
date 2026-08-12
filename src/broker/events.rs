//! The broker API's [server-sent event streams](https://docs.alpaca.markets/us/docs/sse-events).
//!
//! Five endpoints push events as they happen: account status changes, trades,
//! journal status, transfer status, and non-trading activity. They are plain
//! HTTP streams of `text/event-stream`, not websockets, so none of the
//! [`crate::data::live`] machinery applies — there is no subscribe message, no
//! authentication handshake, and no reconnect state machine. alpaca-py iterates
//! the stream until it ends and so does this.
//!
//! Each event's `data` is JSON whose shape depends on the endpoint. alpaca-py
//! yields it as a raw string; [`BrokerEvent::json`] deserializes it into a type
//! of the caller's choosing, since no captured payloads exist to model these
//! from.

use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Filters for an event stream.
///
/// `since` and `until` bound the window by date; [`since_id`](Self::since_id)
/// and [`until_id`](Self::until_id) bound it by ULID cursor, which is the
/// precise form and the one to resume a dropped stream with.
///
/// **The cursor parameter is named differently on each API version**, and the
/// client renders it for the stream being subscribed to. The v1 streams take
/// `since_ulid`/`until_ulid` — their `since_id`/`until_id` are a legacy integer
/// form, deprecated since 2023-08-01 with a sunset of 2027-02-15, which this
/// crate does not expose. The v2 streams take `since_id`/`until_id` and those
/// *are* the ULIDs. Same names, different meanings, opposite versions; naming
/// the field after the concept rather than either spelling is the only way out
/// that does not mislead half the time.
///
/// The derived `Serialize` uses the v2 spelling. The client does not use it —
/// it builds the query itself, because it knows which stream it is calling.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEventsRequest {
    /// Resume after this event id.
    ///
    /// Accepted by the account status, non-trading activity, and journal
    /// streams. The trade and funding streams take only the cursor pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Only events on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<NaiveDate>,
    /// Only events on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<NaiveDate>,
    /// Only events after this ULID.
    ///
    /// Sent as `since_ulid` to the v1 streams and `since_id` to the v2 ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_id: Option<String>,
    /// Only events up to this ULID.
    ///
    /// Sent as `until_ulid` to the v1 streams and `until_id` to the v2 ones.
    /// Alpaca requires the lower bound whenever this is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_id: Option<String>,
}

impl GetEventsRequest {
    /// Events from `since` onwards.
    #[must_use]
    pub fn since(since: NaiveDate) -> Self {
        Self {
            since: Some(since),
            ..Self::default()
        }
    }

    /// Events after the one with this id.
    #[must_use]
    pub fn after_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            ..Self::default()
        }
    }

    /// Events after this ULID cursor, which is how a dropped stream is resumed.
    #[must_use]
    pub fn after_ulid(since_id: impl Into<String>) -> Self {
        Self {
            since_id: Some(since_id.into()),
            ..Self::default()
        }
    }
}

/// Which version of the events API a stream lives on.
///
/// Alpaca versions these per stream rather than per API, so this is not the
/// client's `api_version`: account status and non-trading activity are v1, and
/// trades, journals and funding are v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventVersion {
    /// `/v1/events/*`.
    V1,
    /// `/v2/events/*`.
    V2,
}

impl EventVersion {
    /// The path segment.
    pub(crate) fn segment(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
        }
    }

    /// What this version calls the ULID cursor bounds.
    fn cursor_params(self) -> (&'static str, &'static str) {
        match self {
            // The v1 `since_id`/`until_id` are the deprecated integer form.
            Self::V1 => ("since_ulid", "until_ulid"),
            Self::V2 => ("since_id", "until_id"),
        }
    }

    /// Renders a filter as query parameters for this version.
    pub(crate) fn query(self, filter: &GetEventsRequest) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(id) = &filter.id {
            query.push(("id", id.clone()));
        }
        if let Some(since) = filter.since {
            query.push(("since", since.to_string()));
        }
        if let Some(until) = filter.until {
            query.push(("until", until.to_string()));
        }

        let (since_key, until_key) = self.cursor_params();
        if let Some(since_id) = &filter.since_id {
            query.push((since_key, since_id.clone()));
        }
        if let Some(until_id) = &filter.until_id {
            query.push((until_key, until_id.clone()));
        }
        query
    }
}

/// One event from a broker event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerEvent {
    /// The last event id the server sent.
    ///
    /// Per the SSE specification this persists: an event that sends no `id`
    /// line keeps the previous one, so this is `None` only until the server has
    /// sent a first id. That is exactly what makes it usable with
    /// [`GetEventsRequest::after_id`] to resume a dropped stream — alpaca-py
    /// discards it and yields only the data.
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

impl BrokerEvent {
    /// Deserializes the payload.
    ///
    /// These streams have no captured payloads to model from, so the type is the
    /// caller's to choose — [`serde_json::Value`] to look first, a struct once
    /// the shape is known.
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

impl From<eventsource_stream::Event> for BrokerEvent {
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
/// A malformed stream is not an invalid request, but [`Error`] has no variant
/// for a broken stream and the websocket code already reports its failures this
/// way. Consistent and imperfect beats inconsistent and imperfect; the roadmap
/// carries the note to add a variant for both at once.
pub(crate) fn stream_error(error: &eventsource_stream::EventStreamError<reqwest::Error>) -> Error {
    match error {
        eventsource_stream::EventStreamError::Transport(source) => {
            Error::InvalidRequest(format!("event stream failed: {source}"))
        }
        eventsource_stream::EventStreamError::Utf8(source) => {
            Error::InvalidRequest(format!("event stream was not valid utf-8: {source}"))
        }
        eventsource_stream::EventStreamError::Parser(source) => {
            Error::InvalidRequest(format!("event stream was malformed: {source}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_id_yet_is_none_rather_than_an_empty_string() {
        // The parser signals "the server has not sent an id" with an empty
        // string. An id of "" and no id at all are different things, and only
        // the second one is safe to resume from.
        let event = BrokerEvent::from(eventsource_stream::Event {
            event: "message".to_owned(),
            data: "{}".to_owned(),
            id: String::new(),
            retry: None,
        });

        assert_eq!(event.id, None);
        assert_eq!(event.name, "message");
    }

    #[test]
    fn the_cursor_is_named_for_the_version_it_is_sent_to() {
        // The same concept, two spellings. On v1 the `since_id` name belongs to
        // a deprecated integer form, so sending the ULID under it would be
        // wrong rather than merely old.
        let filter = GetEventsRequest {
            since_id: Some("01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned()),
            until_id: Some("01ARZ3NDEKTSV4RRFFQ69G5FBW".to_owned()),
            ..GetEventsRequest::default()
        };

        assert_eq!(
            EventVersion::V1.query(&filter),
            vec![
                ("since_ulid", "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned()),
                ("until_ulid", "01ARZ3NDEKTSV4RRFFQ69G5FBW".to_owned()),
            ]
        );
        assert_eq!(
            EventVersion::V2.query(&filter),
            vec![
                ("since_id", "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned()),
                ("until_id", "01ARZ3NDEKTSV4RRFFQ69G5FBW".to_owned()),
            ]
        );
    }

    #[test]
    fn an_empty_filter_sends_nothing() {
        let filter = GetEventsRequest::default();
        assert!(EventVersion::V1.query(&filter).is_empty());
        assert!(EventVersion::V2.query(&filter).is_empty());
    }

    #[test]
    fn dates_render_as_the_wire_format() {
        let filter = GetEventsRequest {
            since: Some("2022-02-01".parse().unwrap()),
            until: Some("2022-02-28".parse().unwrap()),
            ..GetEventsRequest::default()
        };

        assert_eq!(
            EventVersion::V2.query(&filter),
            vec![
                ("since", "2022-02-01".to_owned()),
                ("until", "2022-02-28".to_owned()),
            ]
        );
    }

    #[test]
    fn the_payload_deserializes_into_a_caller_chosen_type() {
        let event = BrokerEvent {
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
