//! Broker API models.
//!
//! Ported from `alpaca/broker/models/`.
//!
//! Grounded in the payloads alpaca-py captured, then cross-checked against
//! `broker-api.json`. Where the two disagree the fixture wins: the spec has
//! already been wrong about field optionality elsewhere in this port.

use std::collections::HashMap;
use std::net::IpAddr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::broker::enums::{
    ACHRelationshipStatus, AccountType, AgreementType, BankAccountType, BankStatus,
    CIPApprovalStatus, CIPProvider, CIPResult, CIPStatus, CalendarSubType, ClearingBroker,
    DocumentType, DriftBandSubType, FeePaymentMethod, FundingSource, IdentifierType,
    JournalEntryType, JournalStatus, PortfolioStatus, RebalancingConditionsType, RunInitiatedFrom,
    RunStatus, RunType, TaxIdType, TradeDocumentSubType, TradeDocumentType, TransferDirection,
    TransferStatus, TransferType, VisaType, WeightType,
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

/// A link between an account and a bank account, for ACH transfers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ACHRelationship {
    /// Alpaca's id for the relationship.
    pub id: Uuid,
    /// The account the relationship belongs to.
    pub account_id: Uuid,
    /// When the relationship was created.
    pub created_at: DateTime<Utc>,
    /// When the relationship last changed.
    pub updated_at: DateTime<Utc>,
    /// Where the relationship is in the approval process.
    pub status: ACHRelationshipStatus,
    /// The name on the bank account.
    #[serde(default)]
    pub account_owner_name: String,
    /// Whether the bank account is checking or savings.
    pub bank_account_type: BankAccountType,
    /// The bank account number.
    #[serde(default)]
    pub bank_account_number: String,
    /// The bank's routing number.
    #[serde(default)]
    pub bank_routing_number: String,
    /// A caller-supplied name for the relationship.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub nickname: Option<String>,
    /// The Plaid processor token, when the relationship was created through Plaid.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub processor_token: Option<String>,
}

/// A bank an account may wire money to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bank {
    /// Alpaca's id for the bank connection.
    pub id: Uuid,
    /// The account the connection belongs to.
    pub account_id: Uuid,
    /// When the connection was created.
    pub created_at: DateTime<Utc>,
    /// When the connection last changed.
    pub updated_at: DateTime<Utc>,
    /// The bank's name.
    #[serde(default)]
    pub name: String,
    /// Where the connection is in the approval process.
    pub status: BankStatus,
    /// The bank's country. Empty for domestic banks.
    #[serde(default)]
    pub country: String,
    /// The bank's state or province. Empty for domestic banks.
    #[serde(default)]
    pub state_province: String,
    /// The bank's postal code. Empty for domestic banks.
    #[serde(default)]
    pub postal_code: String,
    /// The bank's city. Empty for domestic banks.
    #[serde(default)]
    pub city: String,
    /// The bank's street address. Empty for domestic banks.
    #[serde(default)]
    pub street_address: String,
    /// The bank account number.
    #[serde(default)]
    pub account_number: String,
    /// The routing number or BIC.
    #[serde(default)]
    pub bank_code: String,
    /// Which of the two `bank_code` is.
    pub bank_code_type: IdentifierType,
}

/// Money moving into or out of an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    /// Alpaca's id for the transfer.
    pub id: Uuid,
    /// The account the money moves for.
    pub account_id: Uuid,
    /// When the transfer was created.
    pub created_at: DateTime<Utc>,
    /// When the transfer last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// When the transfer expires if it has not settled.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// The ACH relationship the money moves over, for ACH transfers.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub relationship_id: Option<Uuid>,
    /// The bank the money moves to, for wires.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub bank_id: Option<Uuid>,
    /// What the recipient receives, after fees.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Whether this is an ACH transfer or a wire.
    #[serde(rename = "type")]
    pub transfer_type: TransferType,
    /// Where the transfer is in its lifecycle.
    pub status: TransferStatus,
    /// Whether money is coming in or going out.
    pub direction: TransferDirection,
    /// Why the transfer is in its current status.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub reason: Option<String>,
    /// What was asked for, before fees.
    #[serde(default, with = "crate::types::option_decimal")]
    pub requested_amount: Option<Decimal>,
    /// Fees applied to the transfer.
    #[serde(default, with = "crate::types::option_decimal")]
    pub fee: Option<Decimal>,
    /// How the fees are paid.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub fee_payment_method: Option<FeePaymentMethod>,
    /// Free-text detail carried on wires.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub additional_information: Option<String>,
}

/// Cash or securities moving from one account to another.
///
/// `net_amount`, `qty` and `price` are declared `float` in alpaca-py but arrive
/// as strings — `"115.5"` in the captured payload — so they are [`Decimal`]
/// here. Reading them as floats is the precision loss this port exists to avoid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// Alpaca's id for the journal.
    pub id: Uuid,
    /// The account the money or securities came from.
    pub from_account: Uuid,
    /// The account they went to.
    pub to_account: Uuid,
    /// Whether this journal moves cash or securities.
    pub entry_type: JournalEntryType,
    /// Where the journal is in its lifecycle.
    pub status: JournalStatus,
    /// The cash amount, for cash journals.
    #[serde(default, with = "crate::types::option_decimal")]
    pub net_amount: Option<Decimal>,
    /// The security journaled, for security journals.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub symbol: Option<String>,
    /// How much of the security moved.
    #[serde(default, with = "crate::types::option_decimal")]
    pub qty: Option<Decimal>,
    /// The price the security was journaled at.
    #[serde(default, with = "crate::types::option_decimal")]
    pub price: Option<Decimal>,
    /// Free-text description.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub description: Option<String>,
    /// When the journal settles.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub settle_date: Option<NaiveDate>,
    /// The system date the journal belongs to.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub system_date: Option<NaiveDate>,
    /// Travel rule: the transmitter's name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub transmitter_name: Option<String>,
    /// Travel rule: the transmitter's account number.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub transmitter_account_number: Option<String>,
    /// Travel rule: the transmitter's address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub transmitter_address: Option<String>,
    /// Travel rule: the transmitter's financial institution.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub transmitter_financial_institution: Option<String>,
    /// Travel rule: when the transfer was transmitted.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub transmitter_timestamp: Option<String>,
    /// The currency the journal settles in.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub currency: Option<crate::types::SupportedCurrencies>,
}

/// One journal's outcome within a batch.
///
/// A batch request answers with one of these per entry, and a failed entry
/// carries its reason rather than failing the whole request — so
/// `error_message` has to be read, not assumed empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchJournalResponse {
    /// The journal itself.
    #[serde(flatten)]
    pub journal: Journal,
    /// Why this entry failed, when it did.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub error_message: Option<String>,
}

/// A statement, confirmation, or tax form belonging to a trading account.
///
/// Distinct from [`AccountDocument`], which is the identity paperwork attached
/// to the brokerage account itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeDocument {
    /// Alpaca's id for the document.
    pub id: Uuid,
    /// The document's name. Often empty.
    #[serde(default)]
    pub name: String,
    /// What kind of document this is.
    #[serde(rename = "type")]
    pub document_type: TradeDocumentType,
    /// A more specific classification.
    ///
    /// Alpaca sends `""` when there is none, which reads as absent here — the
    /// same transformation alpaca-py makes in its constructor.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub sub_type: Option<TradeDocumentSubType>,
    /// The date the document covers.
    pub date: NaiveDate,
}

/// A W-8BEN form filled in field by field rather than uploaded as a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct W8BenDocument {
    /// The country the applicant is a citizen of.
    pub country_citizen: String,
    /// The date the form was signed.
    pub date: NaiveDate,
    /// The applicant's date of birth.
    pub date_of_birth: NaiveDate,
    /// The applicant's full name.
    pub full_name: String,
    /// The IP address the form was signed from.
    pub ip_address: IpAddr,
    /// Permanent address: city and state.
    pub permanent_address_city_state: String,
    /// Permanent address: country.
    pub permanent_address_country: String,
    /// Permanent address: street.
    pub permanent_address_street: String,
    /// The revision of the form.
    pub revision: String,
    /// The full name of the signer.
    pub signer_full_name: String,
    /// When the form data was gathered.
    pub timestamp: DateTime<Utc>,
    /// Any additional conditions claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_conditions: Option<String>,
    /// The applicant's tax id in their home country.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_tax_id: Option<String>,
    /// Set when neither tax id is supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ftin_not_required: Option<bool>,
    /// The type of income claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub income_type: Option<String>,
    /// Mailing address: city and state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailing_address_city_state: Option<String>,
    /// Mailing address: country.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailing_address_country: Option<String>,
    /// Mailing address: street.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailing_address_street: Option<String>,
    /// The treaty paragraph claimed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph_number: Option<String>,
    /// The withholding rate claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_rate_withholding: Option<String>,
    /// A reference number for the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_number: Option<String>,
    /// The applicant's country of residency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residency: Option<String>,
    /// The applicant's US tax id or SSN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id_ssn: Option<String>,
}

impl W8BenDocument {
    /// Checks that the form identifies the applicant for tax purposes.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if none of `foreign_tax_id`,
    /// `tax_id_ssn` and `ftin_not_required` is set. alpaca-py enforces the same
    /// rule in a model validator.
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.foreign_tax_id.is_none()
            && self.tax_id_ssn.is_none()
            && self.ftin_not_required.is_none()
        {
            return Err(crate::Error::InvalidRequest(
                "ftin_not_required must be set when neither foreign_tax_id nor tax_id_ssn is"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// One line of a portfolio's target allocation.
///
/// alpaca-py rounds `percent` to two decimal places in a field validator, which
/// fires on responses as well as requests. Here the two constructors round and
/// nothing else does: a percentage Alpaca sends back is kept exactly as sent,
/// and a `percent` assigned directly to the field is the caller's to round.
/// Rounding a value on the way *in* would be the port quietly editing Alpaca's
/// own numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weight {
    /// Whether this line is cash or a security.
    #[serde(rename = "type")]
    pub weight_type: WeightType,
    /// The security, for asset lines.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub symbol: Option<String>,
    /// The share of the portfolio, as a percentage.
    ///
    /// Declared `float` in alpaca-py; the wire carries `"35"`, a string.
    #[serde(with = "crate::types::decimal")]
    pub percent: Decimal,
}

impl Weight {
    /// A cash line holding `percent` of the portfolio.
    ///
    /// `percent` is rounded to two decimal places, which is what Alpaca accepts
    /// and what alpaca-py's validator does.
    #[must_use]
    pub fn cash(percent: Decimal) -> Self {
        Self {
            weight_type: WeightType::Cash,
            symbol: None,
            percent: percent.round_dp(2),
        }
    }

    /// An asset line holding `percent` of the portfolio in `symbol`.
    ///
    /// `percent` is rounded to two decimal places, as for
    /// [`cash`](Self::cash).
    #[must_use]
    pub fn asset(symbol: impl Into<String>, percent: Decimal) -> Self {
        Self {
            weight_type: WeightType::Asset,
            symbol: Some(symbol.into()),
            percent: percent.round_dp(2),
        }
    }

    /// Checks the line is one Alpaca will accept.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the percentage is not
    /// positive, or an asset line names no symbol.
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.percent <= Decimal::ZERO {
            return Err(crate::Error::InvalidRequest(
                "a weight's percent must be greater than zero".to_owned(),
            ));
        }
        if self.weight_type == WeightType::Asset && self.symbol.is_none() {
            return Err(crate::Error::InvalidRequest(
                "an asset weight must name a symbol".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Which sub type a rebalancing condition carries.
///
/// The `type` field decides which enum `sub_type` belongs to. alpaca-py models
/// this as `Union[DriftBandSubType, CalendarSubType]`, which pydantic resolves
/// by trying each in turn — a scheme that cannot work here, because every
/// generated enum accepts any string into `Unknown` and so would always match
/// first. The two value sets are disjoint, so the wire value alone decides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RebalancingSubType {
    /// A drift band condition's sub type.
    DriftBand(DriftBandSubType),
    /// A calendar condition's sub type.
    Calendar(CalendarSubType),
    /// A value belonging to neither set.
    Unknown(String),
}

impl RebalancingSubType {
    /// The value as it appears on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::DriftBand(sub_type) => sub_type.as_str(),
            Self::Calendar(sub_type) => sub_type.as_str(),
            Self::Unknown(value) => value,
        }
    }
}

impl From<&str> for RebalancingSubType {
    fn from(value: &str) -> Self {
        if DriftBandSubType::WIRE_VALUES.contains(&value) {
            Self::DriftBand(DriftBandSubType::from(value))
        } else if CalendarSubType::WIRE_VALUES.contains(&value) {
            Self::Calendar(CalendarSubType::from(value))
        } else {
            Self::Unknown(value.to_owned())
        }
    }
}

impl std::fmt::Display for RebalancingSubType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for RebalancingSubType {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RebalancingSubType {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}

/// When a portfolio should be rebalanced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebalancingCondition {
    /// Whether the trigger is drift or the calendar.
    #[serde(rename = "type")]
    pub condition_type: RebalancingConditionsType,
    /// The specific trigger, from the set the type implies.
    pub sub_type: RebalancingSubType,
    /// The drift threshold, for drift band conditions.
    #[serde(default, with = "crate::types::option_decimal")]
    pub percent: Option<Decimal>,
    /// The day the calendar condition fires on.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub day: Option<String>,
}

/// A target allocation that accounts can subscribe to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portfolio {
    /// Alpaca's id for the portfolio.
    pub id: Uuid,
    /// The portfolio's name.
    pub name: String,
    /// What the portfolio is for.
    #[serde(default)]
    pub description: String,
    /// Whether the portfolio can still be subscribed to.
    pub status: PortfolioStatus,
    /// Days to wait after a rebalance before rebalancing again.
    #[serde(default, with = "crate::types::serde_util::int::option")]
    pub cooldown_days: Option<i64>,
    /// When the portfolio was created.
    pub created_at: DateTime<Utc>,
    /// When the portfolio last changed.
    pub updated_at: DateTime<Utc>,
    /// The target allocation.
    #[serde(default)]
    pub weights: Vec<Weight>,
    /// When to rebalance towards it.
    #[serde(default)]
    pub rebalance_conditions: Option<Vec<RebalancingCondition>>,
}

/// An account's subscription to a portfolio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// Alpaca's id for the subscription.
    pub id: Uuid,
    /// The subscribed account.
    pub account_id: Uuid,
    /// The portfolio it tracks.
    pub portfolio_id: Uuid,
    /// When the subscription was created.
    pub created_at: DateTime<Utc>,
    /// When the account was last rebalanced.
    #[serde(default)]
    pub last_rebalanced_at: Option<DateTime<Utc>>,
}

/// An order a rebalancing run chose not to place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedOrder {
    /// The security the order would have been for.
    pub symbol: String,
    /// Which way it would have gone.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub side: Option<crate::trading::OrderSide>,
    /// The dollar value it would have been for.
    #[serde(default, with = "crate::types::option_decimal")]
    pub notional: Option<Decimal>,
    /// The currency of that value.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub currency: Option<String>,
    /// Why it was skipped.
    #[serde(default)]
    pub reason: String,
    /// The detail behind the reason.
    #[serde(default)]
    pub reason_details: String,
}

/// One attempt to move an account towards its portfolio's weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RebalancingRun {
    /// Alpaca's id for the run.
    pub id: Uuid,
    /// The account being rebalanced.
    pub account_id: Uuid,
    /// The portfolio being rebalanced towards.
    pub portfolio_id: Uuid,
    /// Whether this is a full rebalance or a cash investment.
    #[serde(rename = "type")]
    pub run_type: RunType,
    /// The cash being invested, for `invest_cash` runs.
    #[serde(default, with = "crate::types::option_decimal")]
    pub amount: Option<Decimal>,
    /// The weights the run targets.
    #[serde(default)]
    pub weights: Vec<Weight>,
    /// Whether Alpaca or the correspondent started the run.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub initiated_from: Option<RunInitiatedFrom>,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// When the run last changed.
    pub updated_at: DateTime<Utc>,
    /// When the run finished.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// When the run was cancelled.
    #[serde(default)]
    pub canceled_at: Option<DateTime<Utc>>,
    /// Where the run is in its lifecycle.
    pub status: RunStatus,
    /// Why the run is in that status.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub reason: Option<String>,
    /// The orders the run placed.
    #[serde(default)]
    pub orders: Option<Vec<Order>>,
    /// The orders that were rejected.
    #[serde(default)]
    pub failed_orders: Option<Vec<Order>>,
    /// The orders the run declined to place.
    #[serde(default)]
    pub skipped_orders: Option<Vec<SkippedOrder>>,
}

/// One page of subscriptions.
///
/// Unlike the portfolio list, which is a bare array, this route wraps its
/// results and pages by token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionsPage {
    /// The subscriptions on this page.
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    /// The token that fetches the next page, when there is one.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub next_page_token: Option<String>,
}

/// One page of rebalancing runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunsPage {
    /// The runs on this page.
    #[serde(default)]
    pub runs: Vec<RebalancingRun>,
    /// The token that fetches the next page, when there is one.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub next_page_token: Option<String>,
}

/// The KYC provider's verdict on the account holder.
///
/// Every field is optional: which of them a provider fills in varies, and
/// alpaca-py declares them all optional for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPKycInfo {
    /// The provider's id for this check.
    pub id: String,
    /// The risk score assigned.
    #[serde(default, with = "crate::types::serde_util::int::option")]
    pub risk_score: Option<i64>,
    /// The risk level assigned.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub risk_level: Option<String>,
    /// Which risk categories applied.
    #[serde(default)]
    pub risk_categories: Option<Vec<String>>,
    /// The applicant's name.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub applicant_name: Option<String>,
    /// The applicant's email address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub email_address: Option<String>,
    /// The applicant's nationality.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub nationality: Option<String>,
    /// The applicant's date of birth.
    #[serde(default)]
    pub date_of_birth: Option<DateTime<Utc>>,
    /// The applicant's address.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub address: Option<String>,
    /// The applicant's postal code.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub postal_code: Option<String>,
    /// The applicant's country of residency.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub country_of_residency: Option<String>,
    /// When KYC finished.
    #[serde(default)]
    pub kyc_completed_at: Option<DateTime<Utc>>,
    /// The IP address the applicant used.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ip_address: Option<String>,
    /// When the check started.
    #[serde(default)]
    pub check_initiated_at: Option<DateTime<Utc>>,
    /// When the check finished.
    #[serde(default)]
    pub check_completed_at: Option<DateTime<Utc>>,
    /// Whether the applicant was approved.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub approval_status: Option<CIPApprovalStatus>,
    /// Who approved them.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub approved_by: Option<String>,
    /// Why.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub approved_reason: Option<String>,
    /// When.
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
}

/// The provider's checks on an identity document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPDocument {
    /// The provider's id for this check.
    pub id: String,
    /// The overall result.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub result: Option<CIPResult>,
    /// Where the check is in its lifecycle.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status: Option<CIPStatus>,
    /// When the check was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// The date of birth on the document.
    #[serde(default)]
    pub date_of_birth: Option<DateTime<Utc>>,
    /// When the document expires.
    #[serde(default)]
    pub date_of_expiry: Option<DateTime<Utc>>,
    /// The numbers printed on the document.
    #[serde(default)]
    pub document_numbers: Option<Vec<String>>,
    /// What kind of document it is.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub document_type: Option<String>,
    /// The first name on the document.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub first_name: Option<String>,
    /// The last name on the document.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub last_name: Option<String>,
    /// The gender on the document.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub gender: Option<String>,
    /// The country that issued it.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub issuing_country: Option<String>,
    /// The nationality on the document.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub nationality: Option<String>,
    /// Whether the age checks out.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub age_validation: Option<CIPResult>,
    /// Whether the document is known to be compromised.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub compromised_document: Option<CIPResult>,
    /// Whether there is a police record against it.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub police_record: Option<CIPStatus>,
    /// Whether the data matches what was submitted.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub data_comparison: Option<CIPResult>,
    /// The detail behind that comparison.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub data_comparison_breakdown: Option<String>,
    /// Whether the image is intact.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub image_integrity: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub image_integrity_breakdown: Option<String>,
    /// Whether the document looks genuine.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub visual_authenticity: Option<String>,
}

/// The provider's checks on a submitted photo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPPhoto {
    /// The provider's id for this check.
    pub id: String,
    /// The overall result.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub result: Option<CIPResult>,
    /// Where the check is in its lifecycle.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status: Option<CIPStatus>,
    /// When the check was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Whether the face matches the document.
    ///
    /// Spelled `face_comparision` on the wire. The typo is Alpaca's, and it is
    /// what the field is actually called, so it is what is read here.
    #[serde(
        default,
        rename = "face_comparision",
        deserialize_with = "empty_string_as_none"
    )]
    pub face_comparison: Option<CIPResult>,
    /// The detail behind that comparison.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub face_comparison_breakdown: Option<String>,
    /// Whether the image is intact.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub image_integrity: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub image_integrity_breakdown: Option<String>,
    /// Whether the photo looks genuine.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub visual_authenticity: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub visual_authenticity_breakdown: Option<String>,
}

/// The provider's checks against identity databases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPIdentity {
    /// The provider's id for this check.
    pub id: String,
    /// The overall result.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub result: Option<CIPResult>,
    /// Where the check is in its lifecycle.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status: Option<CIPStatus>,
    /// When the check was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Whether the address matched.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub matched_address: Option<CIPResult>,
    /// Which addresses matched.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub matched_addresses: Option<String>,
    /// Whether the sources agreed.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub sources: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub sources_breakdown: Option<String>,
    /// Whether the address checks out.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub address: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub address_breakdown: Option<String>,
    /// Whether the date of birth checks out.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub date_of_birth: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub date_of_birth_breakdown: Option<String>,
    /// Whether the tax id checks out.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub tax_id: Option<CIPResult>,
    /// The detail behind that.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub tax_id_breakdown: Option<String>,
}

/// The provider's checks against sanctions and watchlists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPWatchlist {
    /// The provider's id for this check.
    pub id: String,
    /// The overall result.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub result: Option<CIPResult>,
    /// Where the check is in its lifecycle.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status: Option<CIPStatus>,
    /// When the check was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// The records that matched.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub records: Option<String>,
    /// Whether the applicant is politically exposed.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub politically_exposed_person: Option<CIPResult>,
    /// Whether they appear on a sanctions list.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub sanction: Option<CIPResult>,
    /// Whether there is adverse media about them.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub adverse_media: Option<CIPResult>,
    /// Whether they appear on a monitored list.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub monitored_lists: Option<CIPResult>,
}

/// A Customer Identification Program record for an account.
///
/// Correspondents that run their own KYC submit these; Alpaca stores them as
/// the regulatory record of who was checked and how.
///
/// **Unverified against a live response.** alpaca-py's two CIP methods are
/// empty stubs — its own comment says the sandbox answers 404 for these routes
/// — so no fixture exists and none of these models has ever met a real payload.
/// They follow `alpaca/broker/models/cip.py` and the broker spec. Treat a
/// decode failure here as a bug report rather than a surprise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CIPInfo {
    /// Alpaca's id for the record.
    pub id: Uuid,
    /// The account it belongs to.
    pub account_id: Uuid,
    /// Which KYC providers the information came from.
    #[serde(default)]
    pub provider_name: Vec<CIPProvider>,
    /// When the record was first uploaded.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
    /// The KYC verdict.
    #[serde(default)]
    pub kyc: Option<Box<CIPKycInfo>>,
    /// The document checks.
    #[serde(default)]
    pub document: Option<Box<CIPDocument>>,
    /// The photo checks.
    #[serde(default)]
    pub photo: Option<Box<CIPPhoto>>,
    /// The identity database checks.
    #[serde(default)]
    pub identity: Option<Box<CIPIdentity>>,
    /// The watchlist checks.
    #[serde(default)]
    pub watchlist: Option<Box<CIPWatchlist>>,
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
