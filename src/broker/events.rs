//! The broker API's [server-sent event streams](https://docs.alpaca.markets/us/docs/sse-events).
//!
//! Nine endpoints push events as they happen: account status changes, trades,
//! journal status, funding status, non-trading activity, account activities,
//! admin actions, IPO events and system events. Four of them appear only in the
//! published reference, and three of the older five had been switched off at the
//! routes other clients still call.
//!
//! The transport itself is [`crate::sse`], which the trading and market data
//! streams share. What lives here is what is specific to the broker's streams:
//! the filter type, and the fact that Alpaca versions each stream individually.
//!
//! Each event's `data` is JSON whose shape depends on the endpoint. No captured
//! payloads exist to model these from, so [`BrokerEvent::json`] deserializes it
//! into a type of the caller's choosing rather than into a guess.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One event from a broker event stream.
///
/// The same type every Alpaca event stream yields; the alias is here so the
/// broker module reads in its own vocabulary.
pub type BrokerEvent = crate::sse::Event;

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
#[non_exhaustive]
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
    /// Only activities sharing this sibling-relationship id.
    ///
    /// **Documented on the non-trading-activity stream only.** The other four
    /// streams do not list it, so it is sent whenever it is set and ignored by
    /// whatever receives it — setting it for another stream is neither
    /// meaningful nor an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<Uuid>,
    /// Whether to include activities that are still being preprocessed.
    ///
    /// Non-trading-activity stream only, like [`group_id`](Self::group_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_preprocessing: Option<bool>,
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

    /// Events from this ULID cursor onwards, which is how a dropped stream is
    /// resumed.
    ///
    /// Deduplicate on the event id: whether the cursor event is itself
    /// redelivered varies by stream, the same way it does for
    /// [`EventStreamRequest::since_id`](crate::EventStreamRequest::since_id).
    #[must_use]
    pub fn from_id(since_id: impl Into<String>) -> Self {
        Self {
            since_id: Some(since_id.into()),
            ..Self::default()
        }
    }
}

/// Which version of the events API a stream lives on.
///
/// Alpaca versions these per stream rather than per API, so this is not the
/// client's `api_version`: account status and non-trading activity are v1,
/// trades, journals, funding, admin actions, IPOs and system events are v2, and
/// account activities is `v2beta1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventVersion {
    /// `/v1/events/*`.
    V1,
    /// `/v2/events/*`.
    V2,
    /// `/v2beta1/events/*`.
    V2Beta1,
}

impl EventVersion {
    /// The path segment.
    pub(crate) fn segment(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V2Beta1 => "v2beta1",
        }
    }

    /// What this version calls the ULID cursor bounds.
    fn cursor_params(self) -> (&'static str, &'static str) {
        match self {
            // The v1 `since_id`/`until_id` are the deprecated integer form.
            Self::V1 => ("since_ulid", "until_ulid"),
            Self::V2 | Self::V2Beta1 => ("since_id", "until_id"),
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

        // NTA-only, and this method does not know which stream it is building
        // for — only which version. Both v1 streams pass through here. Sending
        // an unrecognised query parameter is harmless; silently dropping one the
        // caller set would not be.
        if let Some(group_id) = filter.group_id {
            query.push(("group_id", group_id.to_string()));
        }
        if let Some(include_preprocessing) = filter.include_preprocessing {
            query.push(("include_preprocessing", include_preprocessing.to_string()));
        }
        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
