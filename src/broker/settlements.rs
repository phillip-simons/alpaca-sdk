//! Settlements, shared by [instant funding](crate::broker::instant_funding) and
//! [JIT](crate::broker::jit).
//!
//! The two settlement families answer with the same object and differ only in
//! what creates one: instant funding settles named transfers, JIT settles named
//! accounts. Modelling them once keeps the wire fact — that these are the same
//! resource under two paths — visible instead of duplicated.
//!
//! Spec-derived, and unverified against a live response.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::wire::wire_enum;

wire_enum! {
    /// Where a settlement stands.
    pub enum SettlementStatus {
        /// Submitted.
        Pending => "PENDING",
        /// Short of funds, and waiting for them.
        AwaitingAdditionalFunds => "AWAITING_ADDITIONAL_FUNDS",
        /// Settled.
        Completed => "COMPLETED",
        /// Failed.
        Failed => "FAILED",
    }
}

wire_enum! {
    /// Which book a settlement belongs to.
    pub enum SettlementAssetClass {
        /// Equities.
        UsEquity => "us_equity",
        /// Crypto.
        Crypto => "crypto",
    }
}

/// Who sent the money, for travel-rule reporting.
///
/// Every field is optional on the wire even though the rule that motivates them
/// is not, so this validates nothing: what a given jurisdiction requires is
/// Alpaca's business, and refusing a request it would have accepted is the
/// worse failure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransmitterInfo {
    /// The sender's full name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_full_name: Option<String>,
    /// The sender's bank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_bank_name: Option<String>,
    /// The sender's account at that bank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_bank_account_number: Option<String>,
    /// Street address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_street_address: Option<String>,
    /// City.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_city: Option<String>,
    /// State or province.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_state: Option<String>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_postal_code: Option<String>,
    /// Country.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_country: Option<String>,
    /// Anything else that identifies the sender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_identifying_information: Option<String>,
}

/// A settlement of money owed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    /// Alpaca's identifier for the settlement.
    pub id: Uuid,
    /// Where it stands.
    pub status: SettlementStatus,
    /// The amount settled.
    pub total_amount: Decimal,
    /// Interest included in that amount.
    #[serde(default)]
    pub interest_amount: Option<Decimal>,
    /// Which book it belongs to.
    #[serde(default)]
    pub asset_class: Option<SettlementAssetClass>,
    /// The currency.
    #[serde(default)]
    pub currency: Option<crate::types::SupportedCurrencies>,
    /// The account the money came from.
    #[serde(default)]
    pub source_account_number: Option<String>,
    /// Why it failed, when it did.
    #[serde(default)]
    pub reason: Option<String>,
    /// Free-form notes the caller attached.
    #[serde(default)]
    pub additional_info: Option<String>,
    /// When it was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
    /// When it completed.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

/// A list of settlements, which arrives under a key rather than bare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlements {
    /// The settlements.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub settlements: Vec<Settlement>,
}

/// Filters for listing settlements.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetSettlementsRequest {
    /// Only settlements in these states, comma-separated.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    pub statuses: Option<Vec<SettlementStatus>>,
}

impl GetSettlementsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only settlements in these states.
    #[must_use]
    pub fn statuses(mut self, statuses: Vec<SettlementStatus>) -> Self {
        self.statuses = Some(statuses);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_statuses_are_upper_case_and_underscored() {
        assert_eq!(
            SettlementStatus::AwaitingAdditionalFunds.as_str(),
            "AWAITING_ADDITIONAL_FUNDS"
        );
    }

    #[test]
    fn statuses_render_as_one_comma_separated_parameter() {
        let request = GetSettlementsRequest::new()
            .statuses(vec![SettlementStatus::Pending, SettlementStatus::Failed]);
        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["statuses"], "PENDING,FAILED");
    }
}
