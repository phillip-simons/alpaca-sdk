//! Account-level odds and ends the reference sweep turned up: options approval,
//! the Onfido identity-verification handoff, country risk ratings, IRA excess
//! contributions, trading limits, and order estimation.
//!
//! Nothing here is in alpaca-py. All of it is spec-derived and unverified
//! against a live response.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::wire::wire_enum;

wire_enum! {
    /// Which options level an account is approved for, or asking for.
    ///
    /// Zero is a real level — it means options trading is switched off — and it
    /// appears on the approved side only. A request cannot ask for it.
    pub enum OptionsLevel {
        /// No options trading.
        Zero => "0",
        /// Covered calls and cash-secured puts.
        One => "1",
        /// Long calls and puts.
        Two => "2",
        /// Spreads.
        Three => "3",
    }
}

wire_enum! {
    /// Where an options approval request stands.
    pub enum OptionsApprovalStatus {
        /// Under review.
        Pending => "PENDING",
        /// Granted at the level asked for.
        Approved => "APPROVED",
        /// Granted, but at a lower level than asked for.
        LowerLevelApproved => "LOWER_LEVEL_APPROVED",
        /// Refused.
        Rejected => "REJECTED",
    }
}

wire_enum! {
    /// Who asked for an options level.
    pub enum OptionsApprovalRequester {
        /// The correspondent.
        Correspondent => "CORRESPONDENT",
        /// Alpaca, on review.
        AlpacaAdmin => "ALPACA_ADMIN",
    }
}

wire_enum! {
    /// How risky Alpaca considers a country.
    pub enum RiskRating {
        /// Low risk.
        Low => "low",
        /// Medium risk.
        Medium => "medium",
        /// High risk.
        High => "high",
        /// Not served at all.
        Prohibited => "prohibited",
    }
}

/// An options level approval request and its outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsApproval {
    /// Alpaca's identifier for the request.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The account.
    #[serde(default)]
    pub account_id: Option<Uuid>,
    /// What was asked for.
    #[serde(default)]
    pub requested_level: Option<OptionsLevel>,
    /// What was granted.
    ///
    /// Not always what was asked for: `LOWER_LEVEL_APPROVED` is a real outcome.
    #[serde(default)]
    pub approved_level: Option<OptionsLevel>,
    /// Where the request stands.
    #[serde(default)]
    pub status: Option<OptionsApprovalStatus>,
    /// Who asked.
    #[serde(default)]
    pub requester: Option<OptionsApprovalRequester>,
    /// When it was made.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A page of options approval requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsApprovalsPage {
    /// The requests.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub options_approvals: Vec<OptionsApproval>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// A request for a given options level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestOptionsApprovalRequest {
    /// The level to ask for.
    pub level: OptionsLevel,
}

impl RequestOptionsApprovalRequest {
    /// Asks for `level`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) for
    /// level zero, which the approved side reports and the request side does
    /// not accept — asking to be approved for no options is not a thing the
    /// route can do.
    pub fn new(level: OptionsLevel) -> crate::Result<Self> {
        if level == OptionsLevel::Zero {
            return Err(crate::Error::InvalidRequest(
                "options level 0 is an outcome, not a level that can be requested".to_owned(),
            ));
        }
        Ok(Self { level })
    }
}

/// Filters for listing options approval requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOptionsApprovalsRequest {
    /// Only this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// Only requests for this level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_level: Option<OptionsLevel>,
    /// Only requests granted at this level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_level: Option<OptionsLevel>,
    /// Only requests in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<OptionsApprovalStatus>,
    /// How many per page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetOptionsApprovalsRequest {
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

/// A token for Onfido's client-side identity-verification SDK.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnfidoToken {
    /// The token to hand the SDK.
    #[serde(default)]
    pub token: Option<String>,
}

/// Filters for an Onfido SDK token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOnfidoTokenRequest {
    /// The origin the SDK will run on, which Onfido checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    /// Which platform the SDK is running on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// What Onfido's SDK concluded, reported back to Alpaca.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateOnfidoOutcomeRequest {
    /// The token the verification ran under.
    pub token: String,
    /// What it concluded.
    pub outcome: String,
    /// Why, where there is a reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl UpdateOnfidoOutcomeRequest {
    /// Reports `outcome` for the verification run under `token`.
    pub fn new(token: impl Into<String>, outcome: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            outcome: outcome.into(),
            reason: None,
        }
    }

    /// Adds a reason.
    #[must_use]
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// What Alpaca will serve in one country.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountryInfo {
    /// The country's name.
    pub full_name: String,
    /// How risky securities trading there is considered.
    pub securities_risk_rating: RiskRating,
    /// How risky crypto trading there is considered.
    pub crypto_risk_rating: RiskRating,
    /// Which of its states crypto is served in, where that varies.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub crypto_supported_states: Vec<String>,
}

/// An over-contribution to an IRA, which has to be returned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IraExcessContribution {
    /// The account.
    #[serde(default)]
    pub account_id: Option<String>,
    /// The tax year it applies to.
    #[serde(default)]
    pub tax_year: Option<f64>,
    /// How much was contributed.
    #[serde(default)]
    pub total_contribution_amount: Option<f64>,
}

/// The USD leg of an account's trading limits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingLimitsUsd {
    /// The ceiling for the day.
    #[serde(default)]
    pub daily_net_limit: Option<Decimal>,
    /// How much is committed.
    #[serde(default)]
    pub used: Option<Decimal>,
    /// How much is left.
    #[serde(default)]
    pub available: Option<Decimal>,
    /// How much is held against open orders.
    #[serde(default)]
    pub held: Option<Decimal>,
}

/// What an account may still trade today.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingLimits {
    /// The ceiling for the day.
    #[serde(default)]
    pub daily_net_limit: Option<Decimal>,
    /// How much is committed.
    #[serde(default)]
    pub used: Option<Decimal>,
    /// How much is left.
    #[serde(default)]
    pub available: Option<Decimal>,
    /// How much is held against open orders.
    #[serde(default)]
    pub held: Option<Decimal>,
    /// The rate used to convert, on a non-USD account.
    #[serde(default)]
    pub swap_rate: Option<Decimal>,
    /// The same figures in USD, on a non-USD account.
    #[serde(default)]
    pub usd: Option<TradingLimitsUsd>,
}

/// A hypothetical order, to be costed rather than placed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstimateOrderRequest {
    /// The symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// How much to spend, rather than how many shares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notional: Option<Decimal>,
    /// Which side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<crate::trading::OrderSide>,
    /// What kind of order.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub order_type: Option<crate::trading::OrderType>,
    /// How long it stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<crate::trading::TimeInForce>,
    /// The currency conversion spread, in basis points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_fee_bps: Option<i64>,
}

impl EstimateOrderRequest {
    /// Costs a notional order in `symbol`.
    pub fn notional(
        symbol: impl Into<String>,
        notional: Decimal,
        side: crate::trading::OrderSide,
    ) -> Self {
        Self {
            symbol: Some(symbol.into()),
            notional: Some(notional),
            side: Some(side),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_zero_cannot_be_asked_for() {
        // It is an outcome the approved side reports, not a level a request
        // may name.
        assert!(RequestOptionsApprovalRequest::new(OptionsLevel::Zero).is_err());
        assert!(RequestOptionsApprovalRequest::new(OptionsLevel::Two).is_ok());
    }

    #[test]
    fn options_levels_are_numeric_strings_on_the_wire() {
        assert_eq!(OptionsLevel::Three.as_str(), "3");
    }

    #[test]
    fn a_country_with_no_state_carve_outs_decodes() {
        let country: CountryInfo = serde_json::from_value(serde_json::json!({
            "full_name": "United Kingdom",
            "securities_risk_rating": "low",
            "crypto_risk_rating": "medium",
        }))
        .unwrap();

        assert!(country.crypto_supported_states.is_empty());
        assert_eq!(country.crypto_risk_rating, RiskRating::Medium);
    }
}
