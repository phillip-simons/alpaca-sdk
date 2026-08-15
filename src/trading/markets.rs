//! The [per-market calendar](https://docs.alpaca.markets/us/reference/calendar-2).
//!
//! `GET /v3/calendar/{market}` answers with a named market's sessions rather
//! than the US equities calendar `GET /v2/calendar` returns, and it splits the
//! day into pre-market, core, lunch and post-market rather than into an open and
//! a close.
//!
//! **This route is `v3`.** The trading client is `v2` and the broker's
//! equivalent is `v2`, so all three versions of the same idea are live at once —
//! which is exactly why the version is written at the call site.
//!
//! No captured payload exists for this route.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::types::wire::wire_enum;

wire_enum! {
    /// A market, by its ISO 10383 code or Alpaca's acronym for it.
    ///
    /// Alpaca's list mixes the two conventions — `NASDAQ` and `XNAS` are both
    /// accepted and are the same venue — so this reproduces the list rather
    /// than tidying it.
    pub enum Market {
        /// Before market open, US.
        Bmo => "BMO",
        /// BNY Mellon.
        Bnym => "BNYM",
        /// Blue Ocean overnight.
        Boats => "BOATS",
        /// Cboe Europe.
        Ceux => "CEUX",
        /// Cboe Chi-X.
        Chix => "CHIX",
        /// Hong Kong Exchanges.
        Hkex => "HKEX",
        /// Investors Exchange.
        Iex => "IEX",
        /// Investors Exchange, MIC.
        Iexg => "IEXG",
        /// International Securities Exchange.
        Ise => "ISE",
        /// London Stock Exchange.
        Lse => "LSE",
        /// Borsa Italiana.
        Mta => "MTA",
        /// Borsa Italiana, MIC.
        Mtaa => "MTAA",
        /// Nasdaq.
        Nasdaq => "NASDAQ",
        /// New York Stock Exchange.
        Nyse => "NYSE",
        /// Oceanview.
        Ocea => "OCEA",
        /// Options Price Reporting Authority.
        Opra => "OPRA",
        /// Over the counter.
        Otc => "OTC",
        /// OTC Markets.
        Otcm => "OTCM",
        /// SIFMA, for the bond market's holiday schedule.
        Sifma => "SIFMA",
        /// Saudi Exchange.
        Tadawul => "TADAWUL",
        /// Euronext Amsterdam.
        Xams => "XAMS",
        /// Euronext Brussels.
        Xbru => "XBRU",
        /// Euronext Dublin.
        Xdub => "XDUB",
        /// Deutsche Börse Xetra.
        Xetr => "XETR",
        /// Deutsche Börse Xetra, alternate spelling.
        Xetra => "XETRA",
        /// Hong Kong Exchanges, MIC.
        Xhkg => "XHKG",
        /// Euronext Lisbon.
        Xlis => "XLIS",
        /// London Stock Exchange, MIC.
        Xlon => "XLON",
        /// Nasdaq, MIC.
        Xnas => "XNAS",
        /// New York Stock Exchange, MIC.
        Xnys => "XNYS",
        /// Euronext Paris.
        Xpar => "XPAR",
        /// Saudi Exchange, MIC.
        Xsau => "XSAU",
    }
}

/// One trading day on a named market.
///
/// Every timestamp is a full RFC-3339 instant, unlike
/// [`Calendar`](crate::trading::Calendar), whose `open` and `close` are naive
/// eastern-time datetimes. Two calendars, two time representations — which is
/// the sort of thing that only shows up when both are used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketSession {
    /// The trading day.
    pub date: NaiveDate,
    /// When the core session opens.
    pub core_start: DateTime<Utc>,
    /// When the core session closes.
    pub core_end: DateTime<Utc>,
    /// When the pre-market session opens, on markets that have one.
    #[serde(default)]
    pub pre_start: Option<DateTime<Utc>>,
    /// When the pre-market session closes.
    #[serde(default)]
    pub pre_end: Option<DateTime<Utc>>,
    /// When the post-market session opens.
    #[serde(default)]
    pub post_start: Option<DateTime<Utc>>,
    /// When the post-market session closes.
    #[serde(default)]
    pub post_end: Option<DateTime<Utc>>,
    /// When the lunch break starts, on markets that take one.
    #[serde(default)]
    pub lunch_start: Option<DateTime<Utc>>,
    /// When the lunch break ends.
    #[serde(default)]
    pub lunch_end: Option<DateTime<Utc>>,
    /// When trades executed on this day settle.
    #[serde(default)]
    pub settlement_date: Option<NaiveDate>,
}

/// Which market a calendar describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketInfo {
    /// Alpaca's short name for it.
    pub acronym: String,
    /// Its full name.
    pub name: String,
    /// The IANA timezone its sessions are quoted in.
    pub timezone: String,
    /// Its ISO 10383 market identifier code.
    #[serde(default)]
    pub mic: Option<String>,
    /// Its bank identifier code.
    #[serde(default)]
    pub bic: Option<String>,
}

/// A named market's calendar.
///
/// `GET /v3/calendar/{market}` answers with a named market's sessions rather
/// than the US equities calendar `GET /v2/calendar` returns, and it splits the
/// day into pre-market, core, lunch and post-market rather than into an open and
/// a close. For the latter, see [`Calendar`](crate::trading::Calendar).
///
/// **This route is `v3`.** The trading client is `v2` and the broker's
/// equivalent is `v2`, so all three versions of the same idea are live at once —
/// which is why the version is written at the call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketCalendar {
    /// The market described.
    pub market: MarketInfo,
    /// Its trading days.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub calendar: Vec<MarketSession>,
}

/// Filters for a market calendar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetMarketCalendarRequest {
    /// The first day to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// The last day to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
    /// The timezone to quote sessions in. Alpaca accepts only `UTC`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl GetMarketCalendarRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts the window.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if `end`
    /// is before `start`.
    pub fn between(mut self, start: NaiveDate, end: NaiveDate) -> crate::Result<Self> {
        if end < start {
            return Err(crate::Error::InvalidRequest(format!(
                "end ({end}) is before start ({start})"
            )));
        }
        self.start = Some(start);
        self.end = Some(end);
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_market_session_is_instants_not_naive_datetimes() {
        // The v2 calendar's `open` and `close` are naive eastern-time; these
        // are absolute. Reading one as the other is an off-by-four-hours bug
        // that only shows up outside New York.
        let session: MarketSession = serde_json::from_value(serde_json::json!({
            "date": "2026-01-02",
            "core_start": "2026-01-02T14:30:00Z",
            "core_end": "2026-01-02T21:00:00Z",
        }))
        .unwrap();

        assert_eq!(session.core_start.to_rfc3339(), "2026-01-02T14:30:00+00:00");
        assert_eq!(session.lunch_start, None);
    }

    #[test]
    fn a_backwards_window_is_refused() {
        let start: NaiveDate = "2026-01-10".parse().unwrap();
        let end: NaiveDate = "2026-01-01".parse().unwrap();
        assert!(GetMarketCalendarRequest::new().between(start, end).is_err());
    }

    #[test]
    fn both_spellings_of_a_venue_are_accepted() {
        // Alpaca's own list carries NASDAQ and XNAS, and they are the same
        // venue. Normalizing them here would send a value the API did not list.
        assert_eq!(Market::Nasdaq.as_str(), "NASDAQ");
        assert_eq!(Market::Xnas.as_str(), "XNAS");
    }
}
