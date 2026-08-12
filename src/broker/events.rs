//! The broker API's server-sent event streams.
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
/// `since` and `until` bound the window; `id` resumes from an event already
/// seen, which is what [`BrokerEvent::id`] is for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEventsRequest {
    /// Resume after this event id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Only events on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<NaiveDate>,
    /// Only events on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<NaiveDate>,
}

impl GetEventsRequest {
    /// Events from `since` onwards.
    #[must_use]
    pub fn since(since: NaiveDate) -> Self {
        Self {
            id: None,
            since: Some(since),
            until: None,
        }
    }

    /// Events after the one with this id.
    #[must_use]
    pub fn after_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            since: None,
            until: None,
        }
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
