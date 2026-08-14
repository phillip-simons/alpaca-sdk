//! [End-of-day reporting](https://docs.alpaca.markets/us/reference/get-v1-reporting-eod-positions)
//! and the [cash interest tiers](https://docs.alpaca.markets/us/reference/get-v1-list-apr-tiers)
//! behind one of the reports.
//!
//! Positions as of a close, the same positions aggregated across every account,
//! and the interest each account earned on its idle cash.
//!
//! Spec-derived, and unverified against a live response.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::trading::Position;
use crate::types::serde_util::comma_separated;

/// Positions across accounts as of one close.
///
/// Keyed by account id. Each value is the same [`Position`] the trading API
/// returns, so an end-of-day position and a live one read identically.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EodPositions {
    /// The close these positions are as of.
    #[serde(default)]
    pub asof: Option<NaiveDate>,
    /// Positions, keyed by account id.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub positions: HashMap<String, Vec<Position>>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One symbol's position summed across every account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AggregatePosition {
    /// The symbol.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Its CUSIP.
    #[serde(default)]
    pub cusip: Option<String>,
    /// Which book it is on.
    #[serde(default)]
    pub asset_type: Option<String>,
    /// How many accounts hold it.
    #[serde(default)]
    pub num_accounts: Option<i64>,
    /// Shares held long.
    #[serde(default, with = "crate::types::option_decimal")]
    pub long_qty: Option<Decimal>,
    /// What those are worth.
    #[serde(default, with = "crate::types::option_decimal")]
    pub long_market_value: Option<Decimal>,
    /// Shares held short.
    #[serde(default, with = "crate::types::option_decimal")]
    pub short_qty: Option<Decimal>,
    /// What those are worth.
    #[serde(default, with = "crate::types::option_decimal")]
    pub short_market_value: Option<Decimal>,
    /// The closing price used.
    #[serde(default, with = "crate::types::option_decimal")]
    pub closing_price: Option<Decimal>,
}

/// One account's interest on idle cash for one day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CashInterestDetail {
    /// The account.
    #[serde(default)]
    pub account_id: Option<Uuid>,
    /// The day.
    #[serde(default)]
    pub date: Option<NaiveDate>,
    /// The currency.
    #[serde(default)]
    pub currency: Option<crate::types::SupportedCurrencies>,
    /// The cash it was earned on.
    #[serde(default, with = "crate::types::option_decimal")]
    pub cash_balance: Option<Decimal>,
    /// What the account earned.
    #[serde(default, with = "crate::types::option_decimal")]
    pub account_accrued_interest: Option<Decimal>,
    /// At what rate, in basis points.
    #[serde(default)]
    pub account_rate_bps: Option<i64>,
    /// What the correspondent took.
    #[serde(default, with = "crate::types::option_decimal")]
    pub correspondent_fee: Option<Decimal>,
    /// At what rate, in basis points.
    #[serde(default)]
    pub correspondent_rate_bps: Option<i64>,
    /// The tier that set those rates.
    #[serde(default)]
    pub apr_tier_id: Option<Uuid>,
    /// That tier's name.
    #[serde(default)]
    pub apr_tier_name: Option<String>,
}

/// A page of cash interest details.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CashInterestReport {
    /// The details.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub interest: Vec<CashInterestDetail>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// How many accounts sit in a tier, and how much they hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AprTierDetails {
    /// When the count was taken.
    #[serde(default)]
    pub as_of: Option<NaiveDate>,
    /// How many accounts.
    #[serde(default)]
    pub total_accounts: Option<i64>,
    /// How much cash between them.
    #[serde(default, with = "crate::types::option_decimal")]
    pub total_balance: Option<Decimal>,
}

/// A cash interest rate tier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AprTier {
    /// Alpaca's identifier for the tier.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Its name.
    #[serde(default)]
    pub name: Option<String>,
    /// The currency it applies to.
    #[serde(default)]
    pub currency: Option<crate::types::SupportedCurrencies>,
    /// What the account earns, in basis points.
    #[serde(default)]
    pub account_rate_bps: Option<i64>,
    /// What the correspondent takes, in basis points.
    #[serde(default)]
    pub correspondent_rate_bps: Option<i64>,
    /// Whether accounts land here unless placed elsewhere.
    #[serde(default)]
    pub is_default: Option<bool>,
    /// How many accounts sit in it.
    #[serde(default)]
    pub details: Option<AprTierDetails>,
    /// When the tier was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// The tier list, which arrives under a key rather than bare.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AprTiers {
    /// The tiers.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub apr_tiers: Vec<AprTier>,
}

/// Filters for the end-of-day positions report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetEodPositionsRequest {
    /// Only this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// Only this symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// The close to report as of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asof: Option<NaiveDate>,
    /// How many to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetEodPositionsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The close to report as of.
    #[must_use]
    pub fn asof(mut self, asof: NaiveDate) -> Self {
        self.asof = Some(asof);
        self
    }
}

/// Filters for the aggregate positions report.
///
/// `date` is required, so it is a constructor argument rather than a builder
/// step: the route has no default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetAggregatePositionsRequest {
    /// The close to report as of.
    pub date: NaiveDate,
    /// Only these symbols.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub symbols: Option<Vec<String>>,
    /// Whether to include firm accounts in the aggregate.
    ///
    /// A flag, not a list — despite sitting next to `symbols`, which is one.
    /// Alpaca's reference: *"Defaults to True which includes firm accounts.
    /// Passing False will exclude all firm accounts."* Sending a comma-separated
    /// list of account ids here got parsed as a boolean, and the report came back
    /// silently missing the firm inventory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firm_accounts: Option<bool>,
}

impl GetAggregatePositionsRequest {
    /// Aggregate positions as of `date`.
    #[must_use]
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            symbols: None,
            firm_accounts: None,
        }
    }

    /// Only these symbols.
    #[must_use]
    pub fn symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = Some(symbols);
        self
    }

    /// Whether to include firm accounts. Alpaca includes them by default.
    #[must_use]
    pub fn firm_accounts(mut self, include: bool) -> Self {
        self.firm_accounts = Some(include);
        self
    }
}

/// Filters for the cash interest report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetCashInterestRequest {
    /// Only this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// One day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    /// Days after this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<NaiveDate>,
    /// Days before this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<NaiveDate>,
    /// Which way to sort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<crate::types::Sort>,
    /// How many per page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetCashInterestRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only this account.
    #[must_use]
    pub fn account_id(mut self, account_id: Uuid) -> Self {
        self.account_id = Some(account_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eod_positions_are_keyed_by_account_and_read_as_trading_positions() {
        // The report reuses the trading Position model, which `fixtures/`
        // already verifies — so an end-of-day position and a live one decode
        // through the same code.
        let report: EodPositions = serde_json::from_value(serde_json::json!({
            "asof": "2026-01-02",
            "next_page_token": null,
            "positions": {
                "550e8400-e29b-41d4-a716-446655440000": [{
                    "asset_id": "550e8400-e29b-41d4-a716-446655440001",
                    "symbol": "AAPL",
                    "exchange": "NASDAQ",
                    "asset_class": "us_equity",
                    "avg_entry_price": "180.00",
                    "qty": "10",
                    "qty_available": "10",
                    "side": "long",
                    "market_value": "1850.00",
                    "cost_basis": "1800.00",
                    "unrealized_pl": "50.00",
                    "unrealized_plpc": "0.0277",
                    "unrealized_intraday_pl": "5.00",
                    "unrealized_intraday_plpc": "0.0027",
                    "current_price": "185.00",
                    "lastday_price": "184.50",
                    "change_today": "0.0027",
                }],
            },
        }))
        .unwrap();

        assert_eq!(report.positions.len(), 1);
        assert_eq!(
            report.positions["550e8400-e29b-41d4-a716-446655440000"][0].symbol,
            "AAPL"
        );
    }

    #[test]
    fn the_aggregate_report_requires_a_date() {
        // No default on the route, so it cannot be a builder step that a caller
        // forgets.
        let request = GetAggregatePositionsRequest::new("2026-01-02".parse().unwrap());
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["date"], "2026-01-02");
    }
}
