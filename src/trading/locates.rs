//! [Securities lending locates](https://docs.alpaca.markets/us/reference/listlocates).
//!
//! Borrowing shares to sell short: ask what a symbol costs to borrow
//! ([`LocateQuote`]), then request a locate ([`CreateLocateRequest`]) and hold
//! it for the trading day.
//!
//! **These routes are `v1`, not the trading client's `v2`.** The client sends
//! them through [`RestClient::at_version`](crate::rest::RestClient::at_version)
//! for that reason.
//!
//! No captured payload exists: everything here follows the published reference,
//! and the first real response is what will confirm it.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::wire::wire_enum;

wire_enum! {
    /// Where a locate is in its lifecycle.
    pub enum LocateStatus {
        /// Located and usable for the trading day.
        Active => "active",
        /// The trading day has rolled and the locate is spent.
        Expired => "expired",
        /// Alpaca could not fill the request.
        Rejected => "rejected",
    }
}

wire_enum! {
    /// Why a symbol has no locate quote.
    pub enum LocateQuoteError {
        /// The symbol is not one Alpaca knows.
        SymbolNotFound => "symbol_not_found",
        /// No locate is needed — the symbol is easy to borrow.
        EasyToBorrow => "easy_to_borrow",
        /// A threshold security, which cannot be located.
        ThresholdSecurity => "threshold_security",
        /// A corporate action is in progress.
        CorporateAction => "corporate_action",
        /// No quote is available right now.
        QuoteUnavailable => "quote_unavailable",
    }
}

/// A locate request and its current status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locate {
    /// Alpaca's identifier for the locate.
    pub id: Uuid,
    /// The symbol located.
    pub symbol: String,
    /// Where the locate is in its lifecycle.
    pub status: LocateStatus,
    /// Shares asked for.
    pub requested_qty: i64,
    /// Shares actually located. Absent when rejected.
    #[serde(default)]
    pub located_qty: Option<i64>,
    /// The fee per share paid. Absent when rejected.
    #[serde(default)]
    pub located_price: Option<Decimal>,
    /// The highest fee per share the request would accept.
    #[serde(default)]
    pub limit_price: Option<Decimal>,
    /// The total fee for the locate.
    #[serde(default)]
    pub total_fee: Option<Decimal>,
    /// Whether the request required the full quantity or nothing.
    #[serde(default)]
    pub all_or_none: bool,
    /// Why the request was rejected, when it was.
    #[serde(default)]
    pub rejection_reason: Option<String>,
    /// When the locate was created.
    pub created_at: DateTime<Utc>,
    /// When an active locate expires. Absent when rejected.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A page of locates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocatesPage {
    /// The locates on this page.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub locates: Vec<Locate>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// What one symbol currently costs to borrow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocateQuote {
    /// The symbol quoted.
    pub symbol: String,
    /// Shares available to borrow.
    pub available_qty: i64,
    /// The fee per share.
    #[serde(default)]
    pub price: Option<Decimal>,
    /// When the quote was taken.
    pub quoted_at: DateTime<Utc>,
}

/// Why a requested symbol has no quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocateQuoteFailure {
    /// The symbol that could not be quoted.
    pub symbol: String,
    /// The machine-readable reason.
    pub code: LocateQuoteError,
    /// The human-readable reason.
    pub message: String,
}

/// Locate quotes, and the symbols that did not get one.
///
/// Partial success is the normal case rather than an error: asking about five
/// symbols where one is easy to borrow returns four quotes and one entry in
/// [`errors`](Self::errors).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocateQuotes {
    /// The symbols that were quoted.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub quotes: Vec<LocateQuote>,
    /// The symbols that were not, and why.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub errors: Vec<LocateQuoteFailure>,
}

/// Filters for listing locates.
///
/// The date window filters on the *locate trading date*, which rolls at 20:00
/// `America/New_York` rather than at midnight UTC, and `end` is exclusive where
/// `start` is inclusive.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetLocatesRequest {
    /// Only locates in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<LocateStatus>,
    /// Only locates for this symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Locates on or after this trading date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// Locates before this trading date, exclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
    /// Maximum results per page. Alpaca defaults to 1,000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetLocatesRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only locates in this state.
    #[must_use]
    pub fn status(mut self, status: LocateStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Only locates for this symbol.
    #[must_use]
    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Restricts the trading-date window.
    #[must_use]
    pub fn between(mut self, start: NaiveDate, end: NaiveDate) -> Self {
        self.start = Some(start);
        self.end = Some(end);
        self
    }
}

/// A request for locate quotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetLocateQuotesRequest {
    /// The symbols to quote, sent as one comma-separated parameter.
    #[serde(serialize_with = "crate::types::serde_util::comma_separated_required")]
    pub symbols: Vec<String>,
}

impl GetLocateQuotesRequest {
    /// Quotes for `symbols`.
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

/// A request for a new locate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateLocateRequest {
    /// The symbol to borrow.
    pub symbol: String,
    /// How many shares.
    pub qty: i64,
    /// The highest fee per share to accept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<Decimal>,
    /// Whether to reject a partial fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_or_none: Option<bool>,
}

impl CreateLocateRequest {
    /// Locate `qty` shares of `symbol`.
    pub fn new(symbol: impl Into<String>, qty: i64) -> Self {
        Self {
            symbol: symbol.into(),
            qty,
            limit_price: None,
            all_or_none: None,
        }
    }

    /// Refuses to pay more than this per share.
    #[must_use]
    pub fn limit_price(mut self, limit_price: Decimal) -> Self {
        self.limit_price = Some(limit_price);
        self
    }

    /// Rejects a partial fill.
    #[must_use]
    pub fn all_or_none(mut self, all_or_none: bool) -> Self {
        self.all_or_none = Some(all_or_none);
        self
    }

    /// The coherence checks a request cannot pass without contradicting itself.
    ///
    /// Alpaca decides everything else — how many shares are available, what a
    /// locate costs, whether the symbol can be located at all — and its answer
    /// says more than a guess here would.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if `qty`
    /// is not positive, or `limit_price` is negative.
    pub fn validate(&self) -> crate::Result<()> {
        if self.qty <= 0 {
            return Err(crate::Error::InvalidRequest(
                "qty must be greater than zero".to_owned(),
            ));
        }
        if self
            .limit_price
            .is_some_and(|price| price.is_sign_negative())
        {
            return Err(crate::Error::InvalidRequest(
                "limit_price cannot be negative".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_symbols_render_as_one_parameter() {
        let request = GetLocateQuotesRequest::new(vec!["TSLA".to_owned(), "GME".to_owned()]);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["symbols"], "TSLA,GME");
    }

    #[test]
    fn a_degenerate_locate_is_refused_before_it_is_sent() {
        assert!(CreateLocateRequest::new("TSLA", 0).validate().is_err());
        assert!(CreateLocateRequest::new("TSLA", -1).validate().is_err());
        assert!(CreateLocateRequest::new("TSLA", 100).validate().is_ok());
    }

    #[test]
    fn a_rejected_locate_omits_every_priced_field() {
        // The reference marks located_qty, located_price, total_fee and
        // expires_at absent on a rejection, so all four have to be optional or
        // the one response that matters most fails to decode.
        let locate: Locate = serde_json::from_value(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "symbol": "TSLA",
            "status": "rejected",
            "requested_qty": 100,
            "rejection_reason": "inventory_unavailable",
            "all_or_none": false,
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();

        assert_eq!(locate.status, LocateStatus::Rejected);
        assert_eq!(locate.located_qty, None);
        assert_eq!(locate.expires_at, None);
    }
}
