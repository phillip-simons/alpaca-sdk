//! Broker API models, ported from `alpaca/broker/models/`.
//!
//! Grounded in the payloads alpaca-py captured, then cross-checked against
//! `broker-api.json`. Where the two disagree the fixture wins: the spec has
//! already been wrong about field optionality elsewhere in this port.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::broker::enums::{
    AccountType, AgreementType, ClearingBroker, DocumentType, FundingSource, TaxIdType, VisaType,
};
use crate::trading::AccountStatus;
use crate::types::serde_util::empty_string_as_none;

/// How to reach the account holder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Primary email address.
    pub email_address: String,
    /// Primary phone number.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub phone_number: Option<String>,
    /// Street address lines.
    #[serde(default)]
    pub street_address: Vec<String>,
    /// Unit or apartment.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub unit: Option<String>,
    /// City.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub city: Option<String>,
    /// State or province.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub state: Option<String>,
    /// Postal code.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub postal_code: Option<String>,
    /// Country.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country: Option<String>,
}

/// Who the account holder is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Given name.
    pub given_name: String,
    /// Middle name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub middle_name: Option<String>,
    /// Family name.
    pub family_name: String,
    /// Date of birth.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub date_of_birth: Option<NaiveDate>,
    /// Tax identification number.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub tax_id: Option<String>,
    /// Which national scheme the tax id belongs to.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub tax_id_type: Option<TaxIdType>,
    /// Country of citizenship.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country_of_citizenship: Option<String>,
    /// Country of birth.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country_of_birth: Option<String>,
    /// Country of tax residence.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country_of_tax_residence: Option<String>,
    /// Where the account's funds come from.
    #[serde(default)]
    pub funding_source: Vec<FundingSource>,
    /// Annual income, lower bound.
    #[serde(default)]
    pub annual_income_min: Option<Decimal>,
    /// Annual income, upper bound.
    #[serde(default)]
    pub annual_income_max: Option<Decimal>,
    /// Liquid net worth, lower bound.
    #[serde(default)]
    pub liquid_net_worth_min: Option<Decimal>,
    /// Liquid net worth, upper bound.
    #[serde(default)]
    pub liquid_net_worth_max: Option<Decimal>,
    /// Total net worth, lower bound.
    #[serde(default)]
    pub total_net_worth_min: Option<Decimal>,
    /// Total net worth, upper bound.
    #[serde(default)]
    pub total_net_worth_max: Option<Decimal>,
    /// Visa category, for non-permanent residents.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub visa_type: Option<VisaType>,
    /// When the visa expires.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub visa_expiration_date: Option<NaiveDate>,
    /// Intended date of departure from the USA.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub date_of_departure_from_usa: Option<NaiveDate>,
    /// Whether the holder is a permanent resident.
    #[serde(default)]
    pub permanent_resident: Option<bool>,
}

/// Regulatory disclosures about the account holder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disclosures {
    /// Whether the holder controls a public company.
    #[serde(default)]
    pub is_control_person: Option<bool>,
    /// Whether the holder is affiliated with an exchange or FINRA.
    #[serde(default)]
    pub is_affiliated_exchange_or_finra: Option<bool>,
    /// Whether the holder is affiliated with an exchange or FINRA member.
    #[serde(default)]
    pub is_affiliated_exchange_or_iiroc: Option<bool>,
    /// Whether the holder is a politically exposed person.
    #[serde(default)]
    pub is_politically_exposed: Option<bool>,
    /// Whether an immediate family member is exposed.
    #[serde(default)]
    pub immediate_family_exposed: Option<bool>,
    /// Whether the account is discretionary.
    #[serde(default)]
    pub is_discretionary: Option<bool>,
    /// Employment status.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub employment_status: Option<String>,
    /// Employer name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub employer_name: Option<String>,
    /// Employer address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub employer_address: Option<String>,
    /// Employment position.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub employment_position: Option<String>,
}

/// An agreement the account holder signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreement {
    /// Which agreement was signed.
    pub agreement: AgreementType,
    /// When it was signed.
    pub signed_at: DateTime<Utc>,
    /// The IP address it was signed from.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ip_address: Option<String>,
    /// The agreement revision.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub revision: Option<String>,
}

/// A document attached to an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountDocument {
    /// Alpaca's id for the document.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub id: Option<Uuid>,
    /// What kind of document this is.
    pub document_type: DocumentType,
    /// A more specific classification.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub document_sub_type: Option<String>,
    /// Where the document can be fetched.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub content: Option<String>,
    /// When the document was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// Someone to contact about the account other than the holder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedContact {
    /// Given name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub given_name: Option<String>,
    /// Family name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub family_name: Option<String>,
    /// Email address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub email_address: Option<String>,
    /// Phone number.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub phone_number: Option<String>,
    /// Street address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub street_address: Option<String>,
    /// City.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub city: Option<String>,
    /// State.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub state: Option<String>,
    /// Postal code.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub postal_code: Option<String>,
    /// Country.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country: Option<String>,
}

/// The outcome of identity verification.
///
/// The per-check payloads vary by provider and are not modelled; they are kept
/// as raw JSON rather than guessed at, which is how alpaca-py treats them too.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KycResults {
    /// Checks that rejected.
    #[serde(default)]
    pub reject: Option<Value>,
    /// Checks that accepted.
    #[serde(default)]
    pub accept: Option<Value>,
    /// Checks that were inconclusive.
    #[serde(default)]
    pub indeterminate: Option<Value>,
    /// Free-text detail.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub additional_information: Option<String>,
    /// The overall verdict.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub summary: Option<String>,
}

/// A brokerage account opened through the broker API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Alpaca's id for the account.
    pub id: Uuid,
    /// The human-readable account number.
    pub account_number: String,
    /// Current status of the account.
    pub status: AccountStatus,
    /// Status of the account for crypto trading.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub crypto_status: Option<AccountStatus>,
    /// Account currency.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub currency: Option<String>,
    /// Equity as of the previous trading day's close.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_equity: Option<Decimal>,
    /// When the account was created.
    pub created_at: DateTime<Utc>,
    /// Which clearing broker the account is assigned to.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub clearing_broker: Option<ClearingBroker>,
    /// Contact details.
    #[serde(default)]
    pub contact: Option<Contact>,
    /// Identity details.
    #[serde(default)]
    pub identity: Option<Identity>,
    /// Regulatory disclosures.
    #[serde(default)]
    pub disclosures: Option<Disclosures>,
    /// Agreements the holder signed.
    #[serde(default)]
    pub agreements: Option<Vec<Agreement>>,
    /// Documents attached to the account.
    #[serde(default)]
    pub documents: Option<Vec<AccountDocument>>,
    /// A secondary contact.
    #[serde(default)]
    pub trusted_contact: Option<TrustedContact>,
    /// Whether this is a trading, custodial, IRA, or other account.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub account_type: Option<AccountType>,
    /// Per-account trading configuration.
    #[serde(default)]
    pub trading_configurations: Option<Value>,
    /// Identity verification results.
    #[serde(default)]
    pub kyc_results: Option<KycResults>,
}

/// The trading view of a brokerage account.
///
/// The broker API answers `/trading/accounts/{id}/account` with everything the
/// trading API's [`crate::trading::TradeAccount`] carries plus the fields below,
/// so that record is flattened in rather than transcribed — alpaca-py subclasses
/// it for the same reason. Reach the shared fields through
/// [`account`](Self::account).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeAccount {
    /// Every field the trading API also returns.
    #[serde(flatten)]
    pub account: crate::trading::TradeAccount,
    /// Cash available to withdraw from the account.
    #[serde(default, with = "crate::types::option_decimal")]
    pub cash_withdrawable: Option<Decimal>,
    /// Cash available to transfer out by journal.
    #[serde(default, with = "crate::types::option_decimal")]
    pub cash_transferable: Option<Decimal>,
    /// When the previous session closed.
    #[serde(default)]
    pub previous_close: Option<DateTime<Utc>>,
    /// Long market value at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_long_market_value: Option<Decimal>,
    /// Short market value at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_short_market_value: Option<Decimal>,
    /// Cash at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_cash: Option<Decimal>,
    /// Initial margin at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_initial_margin: Option<Decimal>,
    /// Regulation T buying power at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_regt_buying_power: Option<Decimal>,
    /// Day trade buying power at 16:00 ET on the previous trading day.
    ///
    /// Removed from Alpaca responses on 2026-07-06 in the FINRA intraday-margin
    /// migration, so this is now absent in practice.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_daytrading_buying_power: Option<Decimal>,
    /// Day trade count at 16:00 ET on the previous trading day.
    ///
    /// Removed from Alpaca responses on 2026-07-06.
    #[serde(default, with = "crate::types::serde_util::int::option")]
    pub last_daytrade_count: Option<i64>,
    /// Buying power at 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_buying_power: Option<Decimal>,
    /// The clearing broker this account is assigned to.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub clearing_broker: Option<ClearingBroker>,
}

/// An order placed on behalf of a brokerage account.
///
/// Identical to the trading API's [`crate::trading::Order`] but for the
/// commission the correspondent charged, which only the broker API reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    /// Every field the trading API also returns.
    #[serde(flatten)]
    pub order: crate::trading::Order,
    /// The commission charged to the end user, in dollars.
    ///
    /// Arrives as a JSON number on order responses and as a string on trade
    /// update events; [`Decimal`] reads both.
    #[serde(default, with = "crate::types::option_decimal")]
    pub commission: Option<Decimal>,
}

/// Positions held across every account, as of the last market close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllAccountsPositions {
    /// When the snapshot was taken.
    pub as_of: DateTime<Utc>,
    /// Positions keyed by account id.
    #[serde(default)]
    pub positions: HashMap<String, Vec<crate::trading::Position>>,
}
