//! [Fully-paid securities lending](https://docs.alpaca.markets/us/reference/get-v1-list-fpsl-loans):
//! the loans, the revenue split, and the per-account analytics.
//!
//! An account lends out shares it owns outright and takes a cut of the borrow
//! fee; [`FpslTier`] is how that cut is set, and [`FpslLoan`] is one day of one
//! symbol on loan.
//!
//! Spec-derived, and unverified against a live response.
//!
//! Interest and market values are `f64` here rather than [`Decimal`], because
//! this family sends them as JSON numbers — the same rule the market data
//! models follow, applied to what the wire actually does rather than to what
//! the field means.
//!
//! [`Decimal`]: rust_decimal::Decimal

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a loan's interest is split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpslInterest {
    /// The account holder's share.
    pub customer: f64,
    /// The correspondent's share.
    pub partner: f64,
}

/// One symbol on loan from one account for one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpslLoan {
    /// The lending account.
    pub account_id: String,
    /// That account's number.
    pub account_number: String,
    /// The correspondent.
    pub correspondent: String,
    /// The symbol lent.
    pub symbol: String,
    /// How many shares.
    pub quantity: i64,
    /// What they were worth.
    pub market_value: f64,
    /// Collateral posted against them.
    pub collateral: f64,
    /// The day.
    pub date: NaiveDate,
    /// How the fee was split.
    #[serde(default)]
    pub interest: Option<FpslInterest>,
    /// When the record last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A page of loans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpslLoansPage {
    /// The loans.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub loans: Vec<FpslLoan>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// A revenue-split tier.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FpslTier {
    /// Alpaca's identifier for the tier.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Its name.
    #[serde(default)]
    pub tier_name: Option<String>,
    /// Which market it applies to.
    #[serde(default)]
    pub market: Option<String>,
    /// The account holder's share.
    #[serde(default)]
    pub customer_split: Option<f64>,
    /// The correspondent's share.
    #[serde(default)]
    pub partner_split: Option<f64>,
    /// When the tier was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// One account's lending activity over a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpslAnalytics {
    /// The account.
    pub account_number: String,
    /// Loans opened over the window.
    pub total_lending_activities: i64,
    /// Loans still open.
    pub in_progress_lending_activities: i64,
    /// How the fees were split.
    #[serde(default)]
    pub interest: Option<FpslInterest>,
}

/// Filters for listing loans.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetFpslLoansRequest {
    /// Only loans from this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// The first day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// The last day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
    /// How many to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetFpslLoansRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only loans from this account.
    #[must_use]
    pub fn account_id(mut self, account_id: Uuid) -> Self {
        self.account_id = Some(account_id);
        self
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

/// A window over one account's lending analytics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetFpslAnalyticsRequest {
    /// The first day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// The last day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loan_without_an_interest_split_decodes() {
        let loan: FpslLoan = serde_json::from_value(serde_json::json!({
            "account_id": "550e8400-e29b-41d4-a716-446655440000",
            "account_number": "123",
            "correspondent": "TEST",
            "symbol": "AAPL",
            "quantity": 100,
            "market_value": 18500.0,
            "collateral": 18500.0,
            "date": "2026-01-02",
        }))
        .unwrap();

        assert_eq!(loan.interest, None);
        assert_eq!(loan.quantity, 100);
    }

    #[test]
    fn a_backwards_loan_window_is_refused() {
        let start: NaiveDate = "2026-02-01".parse().unwrap();
        let end: NaiveDate = "2026-01-01".parse().unwrap();
        assert!(GetFpslLoansRequest::new().between(start, end).is_err());
    }
}
