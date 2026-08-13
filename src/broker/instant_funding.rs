//! [Instant funding](https://docs.alpaca.markets/us/reference/get-v1-instant-funding-list):
//! trading against money that has not landed yet, and settling the debt.
//!
//! A correspondent fronts an account cash before the deposit clears; the
//! account trades on it immediately, and the correspondent owes Alpaca until the
//! [deadline](InstantFunding::deadline). Interest accrues past it.
//!
//! Spec-derived, and unverified against a live response.
//! Settlements are shared with [JIT](crate::broker::jit); see
//! [`crate::broker::settlements`].

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::broker::settlements::TransmitterInfo;
use crate::types::wire::wire_enum;

wire_enum! {
    /// Where an instant funding request stands.
    pub enum InstantFundingStatus {
        /// Submitted.
        Pending => "PENDING",
        /// Withdrawn before execution.
        Canceled => "CANCELED",
        /// The cash has been advanced.
        Executed => "EXECUTED",
        /// It could not be advanced.
        Failed => "FAILED",
        /// Advanced and settled.
        Completed => "COMPLETED",
    }
}

wire_enum! {
    /// Who charges a fee on an instant funding request.
    pub enum InstantFundingFeeType {
        /// The correspondent.
        Partner => "partner",
        /// Alpaca.
        Alpaca => "alpaca",
    }
}

wire_enum! {
    /// What an instant funding list is sorted by.
    pub enum InstantFundingSortBy {
        /// When the request was made.
        CreatedAt => "created_at",
        /// How much was advanced.
        Amount => "amount",
        /// When it must be settled.
        Deadline => "deadline",
    }
}

wire_enum! {
    /// Which way a sort runs.
    ///
    /// Upper-case here, unlike the lower-case `asc`/`desc` every other route in
    /// this crate takes — which is why this is its own enum rather than
    /// [`Sort`](crate::types::Sort).
    pub enum SortOrder {
        /// Ascending.
        Asc => "ASC",
        /// Descending.
        Desc => "DESC",
    }
}

/// A fee charged on an instant funding request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantFundingFee {
    /// Alpaca's identifier for the fee.
    pub id: Uuid,
    /// Who charges it.
    #[serde(rename = "type")]
    pub fee_type: InstantFundingFeeType,
    /// How much.
    pub amount: Decimal,
}

/// A day's interest on an unsettled advance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantFundingInterest {
    /// Alpaca's identifier for the charge.
    pub id: Uuid,
    /// The day it accrued for.
    pub date: NaiveDate,
    /// How much.
    pub amount: Decimal,
    /// Where the charge stands.
    pub status: InstantFundingStatus,
    /// When it was raised.
    pub created_at: DateTime<Utc>,
    /// When it was reconciled.
    #[serde(default)]
    pub reconciled_at: Option<DateTime<Utc>>,
}

/// An advance of cash against a deposit that has not cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantFunding {
    /// Alpaca's identifier for the advance.
    pub id: Uuid,
    /// The account credited.
    pub account_no: String,
    /// The account the money is owed from.
    pub source_account_no: String,
    /// How much was advanced.
    pub amount: Decimal,
    /// How much is still owed.
    pub remaining_payable: Decimal,
    /// Interest accrued so far.
    pub total_interest: Decimal,
    /// Where the advance stands.
    pub status: InstantFundingStatus,
    /// The business day it was booked on.
    pub system_date: NaiveDate,
    /// The day it must be settled by.
    pub deadline: NaiveDate,
    /// Fees charged.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub fees: Vec<InstantFundingFee>,
    /// Interest charges raised.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub interests: Vec<InstantFundingInterest>,
    /// When the advance was requested.
    pub created_at: DateTime<Utc>,
}

/// How much instant funding a correspondent may have outstanding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantFundingLimits {
    /// The ceiling.
    #[serde(default)]
    pub amount_limit: Option<Decimal>,
    /// How much of it is committed.
    #[serde(default)]
    pub amount_in_use: Option<Decimal>,
    /// How much is left.
    #[serde(default)]
    pub amount_available: Option<Decimal>,
}

/// One account's share of the correspondent's limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInstantFundingLimits {
    /// The account.
    pub account_no: String,
    /// Its ceiling.
    pub amount_limit: Decimal,
    /// How much of it is committed.
    pub amount_in_use: Decimal,
    /// How much is left.
    pub amount_available: Decimal,
}

/// A day's instant funding position for one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantFundingReport {
    /// The account.
    pub account_no: String,
    /// The business day.
    pub system_date: NaiveDate,
    /// The settlement deadline.
    pub deadline: NaiveDate,
    /// Everything owed on that day.
    pub total_amount_owed: Decimal,
    /// Interest charged for missing the deadline.
    pub total_interest_penalty: Decimal,
    /// The advances behind the total.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub instant_funding_transfers: Vec<InstantFunding>,
}

/// Filters for listing instant funding requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetInstantFundingRequest {
    /// Only requests in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<InstantFundingStatus>,
    /// Only requests booked on this business day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_date: Option<NaiveDate>,
    /// Only requests due on this day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<NaiveDate>,
    /// Only requests made before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<DateTime<Utc>>,
    /// What to sort by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<InstantFundingSortBy>,
    /// Which way to sort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<SortOrder>,
    /// How many to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// How many to skip. This family pages by offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

impl GetInstantFundingRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only requests in this state.
    #[must_use]
    pub fn status(mut self, status: InstantFundingStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Sorts the results.
    #[must_use]
    pub fn sort(mut self, sort_by: InstantFundingSortBy, order: SortOrder) -> Self {
        self.sort_by = Some(sort_by);
        self.sort_order = Some(order);
        self
    }
}

/// A request to advance cash against an uncleared deposit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateInstantFundingRequest {
    /// The account to credit.
    pub account_no: String,
    /// The account the money is owed from.
    pub source_account_no: String,
    /// How much to advance.
    pub amount: Decimal,
}

impl CreateInstantFundingRequest {
    /// Advances `amount` to `account_no` from `source_account_no`.
    pub fn new(
        account_no: impl Into<String>,
        source_account_no: impl Into<String>,
        amount: Decimal,
    ) -> Self {
        Self {
            account_no: account_no.into(),
            source_account_no: source_account_no.into(),
            amount,
        }
    }

    /// The one check a request cannot pass without contradicting itself.
    ///
    /// This guards a money-movement route, so it catches a sign error before it
    /// becomes an advance.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if
    /// `amount` is not positive.
    pub fn validate(&self) -> crate::Result<()> {
        if self.amount <= Decimal::ZERO {
            return Err(crate::Error::InvalidRequest(
                "amount must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// One advance to settle, and who sent the money.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementTransfer {
    /// The advance being settled.
    pub instant_transfer_id: Uuid,
    /// Who sent the money, for travel-rule reporting.
    pub transmitter_info: TransmitterInfo,
}

/// A request to settle one or more advances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateInstantFundingSettlementRequest {
    /// The advances to settle.
    pub transfers: Vec<SettlementTransfer>,
    /// Free-form notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<String>,
}

impl CreateInstantFundingSettlementRequest {
    /// Settles `transfers`.
    #[must_use]
    pub fn new(transfers: Vec<SettlementTransfer>) -> Self {
        Self {
            transfers,
            additional_info: None,
        }
    }

    /// Rejects a settlement that would settle nothing.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if no
    /// transfers are named.
    pub fn validate(&self) -> crate::Result<()> {
        if self.transfers.is_empty() {
            return Err(crate::Error::InvalidRequest(
                "a settlement must name at least one transfer".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for the instant funding report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetInstantFundingReportRequest {
    /// Which report to run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_type: Option<String>,
    /// The business day to report on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_date: Option<NaiveDate>,
}

/// A request for several accounts' instant funding limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetAccountLimitsRequest {
    /// The accounts to ask about, sent as one comma-separated parameter.
    ///
    /// The spec draws this as an array, which reads as a repeated parameter;
    /// the reference says "comma-separated account numbers" in as many words,
    /// and it is the reference that describes the live route. It also has to be
    /// joined to be sent at all — a bare `Vec` in a query struct fails the
    /// request before it leaves the process.
    #[serde(serialize_with = "crate::types::serde_util::comma_separated_required")]
    pub account_numbers: Vec<String>,
}

impl GetAccountLimitsRequest {
    /// Limits for `account_numbers`.
    pub fn new(account_numbers: Vec<String>) -> Self {
        Self { account_numbers }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_advance_is_refused_before_it_is_sent() {
        let request = CreateInstantFundingRequest::new("acct", "source", Decimal::ZERO);
        assert!(request.validate().is_err());

        let request = CreateInstantFundingRequest::new("acct", "source", Decimal::ONE);
        assert!(request.validate().is_ok());
    }

    #[test]
    fn a_settlement_must_settle_something() {
        assert!(
            CreateInstantFundingSettlementRequest::new(Vec::new())
                .validate()
                .is_err()
        );
    }

    #[test]
    fn the_sort_order_is_upper_case_on_this_family_alone() {
        // Every other route in this crate takes `asc`/`desc`.
        assert_eq!(SortOrder::Asc.as_str(), "ASC");
        assert_eq!(crate::types::Sort::Asc.as_str(), "asc");
    }

    #[test]
    fn an_advance_with_no_fees_or_interest_yet_decodes() {
        let funding: InstantFunding = serde_json::from_value(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "account_no": "123",
            "source_account_no": "456",
            "amount": "1000.00",
            "remaining_payable": "1000.00",
            "total_interest": "0",
            "status": "EXECUTED",
            "system_date": "2026-01-02",
            "deadline": "2026-01-05",
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();

        assert!(funding.fees.is_empty());
        assert!(funding.interests.is_empty());
    }
}
