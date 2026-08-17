//! The market data API's [corporate actions event stream](https://docs.alpaca.markets/us/reference/subscribetocorporateactionseventssse).
//!
//! One long-lived `text/event-stream` connection carrying every corporate-action
//! mutation — `insert`, `update` and `delete` — across all fifteen action types.
//! The same data is available on demand from
//! [`CorporateActionsClient::get_corporate_actions`](crate::data::CorporateActionsClient::get_corporate_actions);
//! this is the push side of it.
//!
//! No SDK ports it, and there is no captured payload, so the event body stays
//! JSON for the caller to deserialize: every event shares an envelope
//! (`event_id`, `at`, `action`, `region`, `event_type`) and `event_type` selects
//! which of fifteen shapes the `ca` field takes. Claiming a Rust type for all
//! fifteen from the spec alone would be a guess with a struct around it.

use serde::{Deserialize, Serialize};

use crate::sse::EventStreamRequest;
use crate::types::Validated;
use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

/// Which markets a corporate actions stream should carry.
#[wire_enum]
pub enum CorporateActionRegion {
    /// Every event, whichever market.
    #[wire = "all"]
    All,
    /// US-listed or US-regulated actions only.
    #[wire = "us"]
    Us,
    /// Everything else.
    #[wire = "non_us"]
    NonUs,
}

/// The `event_type` discriminator on a corporate actions event.
///
/// Also the filter: naming a subset here narrows the stream to those types.
/// The values are not the same strings as
/// [`CorporateActionsType`](crate::data::CorporateActionsType), which the
/// polled route uses — these carry a `_corporateaction_event` suffix, and
/// the two lists do not even hold the same members.
#[wire_enum(sorted)]
pub enum CorporateActionEventType {
    /// A cash dividend.
    #[wire = "cash_dividend_corporateaction_event"]
    CashDividend,
    /// A cash merger.
    #[wire = "cash_merger_corporateaction_event"]
    CashMerger,
    /// A partial call of an equity.
    #[wire = "equity_partial_call_corporateaction_event"]
    EquityPartialCall,
    /// A forward split.
    #[wire = "forward_split_corporateaction_event"]
    ForwardSplit,
    /// A name change.
    #[wire = "name_change_corporateaction_event"]
    NameChange,
    /// A redemption.
    #[wire = "redemption_corporateaction_event"]
    Redemption,
    /// A reorganization.
    #[wire = "reorganization_corporateaction_event"]
    Reorganization,
    /// A reverse split.
    #[wire = "reverse_split_corporateaction_event"]
    ReverseSplit,
    /// A rights distribution.
    #[wire = "rights_distribution_corporateaction_event"]
    RightsDistribution,
    /// A spin off.
    #[wire = "spin_off_corporateaction_event"]
    SpinOff,
    /// A merger paid in stock and cash.
    #[wire = "stock_and_cash_merger_corporateaction_event"]
    StockAndCashMerger,
    /// A stock dividend.
    #[wire = "stock_dividend_corporateaction_event"]
    StockDividend,
    /// A stock merger.
    #[wire = "stock_merger_corporateaction_event"]
    StockMerger,
    /// A unit split.
    #[wire = "unit_split_corporateaction_event"]
    UnitSplit,
    /// A worthless removal.
    #[wire = "worthless_removal_corporateaction_event"]
    WorthlessRemoval,
}

/// Filters for the corporate actions event stream.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CorporateActionEventsRequest {
    /// The replay window and cursor, shared with every other Alpaca stream
    /// found in the reference sweep.
    #[serde(flatten)]
    pub window: EventStreamRequest,
    /// Only these event types. Sent as one comma-separated `type` parameter.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    #[setters(into, doc = "Only these event types.")]
    pub types: Option<Vec<CorporateActionEventType>>,
    /// Which markets to receive events for. Alpaca defaults this to `all`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Only this region.")]
    pub region: Option<CorporateActionRegion>,
}

impl CorporateActionEventsRequest {
    /// A stream of everything, live from now.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replays from `window` before going live.
    #[must_use]
    pub fn window(mut self, window: EventStreamRequest) -> Self {
        self.window = window;
        self
    }

    /// The filter as query parameters.
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        let mut query = self.window.query();
        if let Some(types) = &self.types {
            query.push((
                "type",
                types
                    .iter()
                    .map(CorporateActionEventType::as_str)
                    .collect::<Vec<_>>()
                    .join(","),
            ));
        }
        if let Some(region) = &self.region {
            query.push(("region", region.as_str().to_owned()));
        }
        query
    }
}

impl Validated for CorporateActionEventsRequest {
    /// Asks the replay window it wraps.
    ///
    /// Three filter types serve the eleven event-stream routes, and this is
    /// the only one that *contains* another. Five routes hand
    /// `sse::subscribe` a `GetEventsRequest`, which carries its own flat
    /// window fields; five hand it an [`EventStreamRequest`] directly, and the
    /// bound checks it. Here it arrives one level down, behind a
    /// `#[serde(flatten)]`, so the bound lands on the wrapper — and a derived
    /// no-op here would swallow it. A rule added to [`EventStreamRequest`]
    /// would then hold for those five routes and silently not for this one.
    ///
    /// It is hand-written rather than derived because that is what the two
    /// spellings mean: the derive says "this type has no rules", and this one
    /// has its window's.
    ///
    /// **No test asserts this, and none can yet.** The window has no rules, so
    /// a test would compare `Ok(())` with `Ok(())` and pass whether the line
    /// below were here or not — the shape of test that reads as coverage and
    /// is not, so one was written and then declined. What holds it instead is
    /// `just validated`: its fourth rule refuses a type that derives the no-op
    /// while holding a field whose type has rules, so on the day
    /// `EventStreamRequest` gains a validator, reverting this impl to a derive
    /// fails the gate. That is the same day the delegation starts mattering.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if the
    /// replay window is not one Alpaca accepts.
    fn validate(&self) -> crate::Result<()> {
        self.window.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_render_as_one_comma_separated_parameter() {
        let request = CorporateActionEventsRequest::new()
            .types(vec![
                CorporateActionEventType::CashDividend,
                CorporateActionEventType::StockMerger,
            ])
            .region(CorporateActionRegion::Us);

        assert_eq!(
            request.query(),
            vec![
                (
                    "type",
                    "cash_dividend_corporateaction_event,stock_merger_corporateaction_event"
                        .to_owned()
                ),
                ("region", "us".to_owned()),
            ]
        );
    }

    #[test]
    fn the_event_types_are_not_the_polled_routes_types() {
        // Two lists, two spellings, and not even the same members — mapping one
        // onto the other would send a filter the stream ignores.
        assert_eq!(
            CorporateActionEventType::CashDividend.as_str(),
            "cash_dividend_corporateaction_event"
        );
        assert_eq!(
            crate::data::CorporateActionsType::CashDividend.as_str(),
            "cash_dividend"
        );
    }

    #[test]
    fn an_empty_filter_sends_nothing() {
        assert!(CorporateActionEventsRequest::new().query().is_empty());
    }
}
