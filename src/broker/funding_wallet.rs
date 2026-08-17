//! [Funding wallets](https://docs.alpaca.markets/us/reference/getfundingwallet):
//! per-account fiat deposit rails, recipient banks, and withdrawals.
//!
//! An account gets its own set of banking details to be funded into, in a
//! currency of its own; money arriving there is converted and credited. This is
//! the international counterpart to the ACH relationships that
//! [`CreateACHRelationshipRequest`](crate::broker::CreateACHRelationshipRequest)
//! opens.
//!
//! **Every route here is `v1beta`, not the broker client's `v1`.** They go
//! through [`RestClient::at_version`](crate::rest::RestClient::at_version) for
//! that reason — the version is the thing this crate has been bitten by before.
//!
//! Spec-derived, and unverified against a live response.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::SupportedCurrencies;
use crate::types::Validated;
use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

/// Whether a funding wallet may be used.
#[wire_enum]
pub enum FundingWalletStatus {
    /// Usable.
    #[wire = "active"]
    Active,
    /// Being set up.
    #[wire = "pending"]
    Pending,
    /// Switched off.
    #[wire = "disabled"]
    Disabled,
}

/// Which rail money moves on.
#[wire_enum]
pub enum PaymentType {
    /// International wire.
    #[wire = "swift_wire"]
    SwiftWire,
    /// A domestic scheme in the destination country.
    #[wire = "local_rails"]
    LocalRails,
}

/// Which way a funding wallet transfer moves.
///
/// Lower-case here, where the crypto wallet's equivalent is upper-case.
/// Two families, two conventions.
#[wire_enum(sorted)]
pub enum FundingDirection {
    /// Into the account.
    #[wire = "incoming"]
    Incoming,
    /// Out of it.
    #[wire = "outgoing"]
    Outgoing,
}

/// Where a funding wallet transfer stands.
#[wire_enum]
pub enum FundingTransferStatus {
    /// Submitted.
    #[wire = "PENDING"]
    Pending,
    /// Withdrawn.
    #[wire = "CANCELED"]
    Canceled,
    /// Sent.
    #[wire = "EXECUTED"]
    Executed,
    /// Failed.
    #[wire = "FAILED"]
    Failed,
    /// Settled.
    ///
    /// `COMPLETE`, not `COMPLETED` — the instant funding family spells the
    /// same idea the other way.
    #[wire = "COMPLETE"]
    Complete,
}

/// What a fee on a funding wallet transfer is for.
#[wire_enum]
pub enum FundingFeeType {
    /// Sending money out.
    #[wire = "withdrawal_fee"]
    WithdrawalFee,
    /// Converting currency.
    #[wire = "fx_fee"]
    FxFee,
    /// The network's own charge.
    #[wire = "network_fee"]
    NetworkFee,
    /// Taking money in.
    #[wire = "deposit_fee"]
    DepositFee,
    /// A returned ACH.
    #[wire = "ach_return_fee"]
    AchReturnFee,
    /// The correspondent's cut.
    ///
    /// Spelled `parnter_fee` on the wire. That is Alpaca's typo, and
    /// matching it is what makes the value decode.
    #[wire = "parnter_fee"]
    PartnerFee,
    /// Alpaca's cut.
    #[wire = "alpaca_fee"]
    AlpacaFee,
}

/// Which national scheme a routing code belongs to.
#[wire_enum]
pub enum RoutingCodeType {
    /// UK sort code.
    #[wire = "sort_code"]
    SortCode,
    /// US ABA routing number.
    #[wire = "aba"]
    Aba,
    /// Australian BSB.
    #[wire = "bsb_code"]
    BsbCode,
    /// Canadian institution number.
    #[wire = "institution_no"]
    InstitutionNo,
    /// A bank code.
    #[wire = "bank_code"]
    BankCode,
    /// A branch code.
    #[wire = "branch_code"]
    BranchCode,
    /// Mexican CLABE.
    #[wire = "clabe"]
    Clabe,
    /// Chinese CNAPS.
    #[wire = "cnaps"]
    Cnaps,
    /// Indian IFSC.
    #[wire = "ifsc"]
    Ifsc,
}

/// What kind of bank account a recipient bank is.
///
/// Lower-case here, where the ACH relationship's
/// [`BankAccountType`](crate::broker::BankAccountType) is upper-case and
/// carries an empty variant besides. Same idea, two wire vocabularies, so
/// two types.
#[wire_enum(sorted)]
pub enum RecipientAccountType {
    /// Checking.
    #[wire = "checking"]
    Checking,
    /// Savings.
    #[wire = "savings"]
    Savings,
}

/// An account's funding wallet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FundingWallet {
    /// The account it belongs to.
    pub account_id: Uuid,
    /// Whether it may be used.
    pub status: FundingWalletStatus,
    /// When it was opened.
    pub created_at: DateTime<Utc>,
}

/// The wallets a batch create opened.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FundingWallets {
    /// The wallets.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub funding_wallets: Vec<FundingWallet>,
}

/// A fee on a funding wallet transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FundingFee {
    /// What it is for.
    #[serde(rename = "type")]
    pub fee_type: FundingFeeType,
    /// How much.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// In what currency.
    pub currency: crate::types::SupportedCurrencies,
    /// How it is charged.
    pub payment_type: String,
}

/// The USD leg of a transfer denominated in something else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsdAmount {
    /// How much, in USD.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
}

/// Money in or out of a funding wallet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FundingWalletTransfer {
    /// Alpaca's identifier for the transfer.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The account.
    #[serde(default)]
    pub account_id: Option<Uuid>,
    /// Which way it moved.
    #[serde(default)]
    pub direction: Option<FundingDirection>,
    /// Where it stands.
    #[serde(default)]
    pub status: Option<FundingTransferStatus>,
    /// Which rail it took.
    #[serde(default)]
    pub payment_type: Option<PaymentType>,
    /// What was asked for.
    #[serde(default, with = "crate::types::option_decimal")]
    pub requested_amount: Option<Decimal>,
    /// What arrived, before conversion.
    #[serde(default, with = "crate::types::option_decimal")]
    pub original_amount: Option<Decimal>,
    /// In what currency.
    #[serde(default)]
    pub original_currency: Option<SupportedCurrencies>,
    /// The USD leg.
    #[serde(default)]
    pub usd: Option<UsdAmount>,
    /// What was charged.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub fees: Vec<FundingFee>,
    /// When it was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A page of funding wallet transfers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FundingWalletTransfers {
    /// The transfers.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub transfers: Vec<FundingWalletTransfer>,
}

/// A bank a withdrawal may be sent to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RecipientBank {
    /// Alpaca's identifier for the bank.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The account number.
    #[serde(default)]
    pub account_number: Option<String>,
    /// The IBAN, where the country uses one.
    #[serde(default)]
    pub iban: Option<String>,
    /// The BIC or SWIFT code.
    #[serde(default)]
    pub bic_swift: Option<String>,
    /// The national routing code.
    #[serde(default)]
    pub routing_code: Option<String>,
    /// Which scheme that code belongs to.
    #[serde(default)]
    pub routing_code_type: Option<RoutingCodeType>,
    /// The holder's first name, for an individual.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Their last name.
    #[serde(default)]
    pub last_name: Option<String>,
    /// The holder's name, for a company.
    #[serde(default)]
    pub company_name: Option<String>,
    /// Street address.
    #[serde(default)]
    pub street_address: Option<String>,
    /// City.
    #[serde(default)]
    pub city: Option<String>,
    /// State or province.
    #[serde(default)]
    pub state_or_province: Option<String>,
    /// Postal code.
    #[serde(default)]
    pub postal_code: Option<String>,
    /// Country.
    #[serde(default)]
    pub country: Option<String>,
    /// The currency it takes.
    #[serde(default)]
    pub currency: Option<crate::types::SupportedCurrencies>,
    /// Which rails may reach it.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub payment_types: Vec<PaymentType>,
    /// When it was added.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A request to open funding wallets for several accounts at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct BatchCreateFundingWalletsRequest {
    /// The accounts to open wallets for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub account_ids: Option<Vec<Uuid>>,
}

impl BatchCreateFundingWalletsRequest {
    /// Opens wallets for `account_ids`.
    #[must_use]
    pub fn new(account_ids: Vec<Uuid>) -> Self {
        Self {
            account_ids: Some(account_ids),
        }
    }
}

/// Filters for an account's funding details.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetFundingDetailsRequest {
    /// Only details for this rail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_type: Option<PaymentType>,
    /// Only details for this currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<crate::types::SupportedCurrencies>,
}

/// A request to register a bank a withdrawal may be sent to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateRecipientBankRequest {
    /// The account number.
    pub account_number: String,
    /// The bank's name.
    pub bank_name: String,
    /// Which country it is in.
    pub bank_country: String,
    /// The currency it takes.
    pub currency: crate::types::SupportedCurrencies,
    /// Street address.
    pub street_address: String,
    /// City.
    pub city: String,
    /// State or province.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub state_or_province: Option<String>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub postal_code: Option<String>,
    /// The holder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub bank_account_holder_name: Option<String>,
    /// What kind of account it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_type: Option<RecipientAccountType>,
    /// The IBAN, where the country uses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into, doc = "Sets the IBAN.")]
    pub iban: Option<String>,
    /// The BIC or SWIFT code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into, doc = "Sets the BIC or SWIFT code.")]
    pub bic_swift: Option<String>,
    /// The national routing code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(skip = "`routing_code(code, code_type)` sets this and \
                      `routing_code_type` together, which is the point: a \
                      routing code without its scheme is ambiguous")]
    pub routing_code: Option<String>,
    /// Which scheme that code belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(skip = "set with the code it describes, by \
                      `routing_code(code, code_type)` — a scheme naming no \
                      code is not a state worth being able to reach")]
    pub routing_code_type: Option<RoutingCodeType>,
}

impl CreateRecipientBankRequest {
    /// The six fields the route requires, and nothing assumed beyond them.
    ///
    /// Which of `iban`, `bic_swift` and `routing_code` a given country needs is
    /// Alpaca's business: the reference marks all three optional, and enforcing
    /// a combination here would refuse requests it accepts. That is the rule
    /// throughout this crate — a documented constraint is checked locally, an
    /// undocumented one is left to the server — and the same reasoning behind
    /// [`CreateBankRequest`](crate::broker::CreateBankRequest)'s
    /// [`Validated::validate`] applies to international bank addresses.
    #[must_use]
    pub fn new(
        account_number: impl Into<String>,
        bank_name: impl Into<String>,
        bank_country: impl Into<String>,
        currency: SupportedCurrencies,
        street_address: impl Into<String>,
        city: impl Into<String>,
    ) -> Self {
        Self {
            account_number: account_number.into(),
            bank_name: bank_name.into(),
            bank_country: bank_country.into(),
            currency,
            street_address: street_address.into(),
            city: city.into(),
            state_or_province: None,
            postal_code: None,
            bank_account_holder_name: None,
            account_type: None,
            iban: None,
            bic_swift: None,
            routing_code: None,
            routing_code_type: None,
        }
    }

    /// Sets the national routing code and its scheme.
    #[must_use]
    pub fn routing_code(mut self, code: impl Into<String>, code_type: RoutingCodeType) -> Self {
        self.routing_code = Some(code.into());
        self.routing_code_type = Some(code_type);
        self
    }
}

/// A request to send money out of a funding wallet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateWithdrawalRequest {
    /// How much to send, in USD.
    ///
    /// Encoded explicitly through this crate's decimal codec — a string on the
    /// wire — rather than relying on `rust_decimal`'s own `Serialize`, which is
    /// what every other money field here does and which a dependency bump could
    /// otherwise change underneath a withdrawal.
    #[serde(
        default,
        with = "crate::types::option_decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub usd_amount: Option<Decimal>,
    /// What to convert it to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_currency: Option<SupportedCurrencies>,
    /// Which rail to send it on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Sets the rail.")]
    pub payment_type: Option<PaymentType>,
}

impl CreateWithdrawalRequest {
    /// Sends `usd_amount` out as `desired_currency`.
    #[must_use]
    pub fn new(usd_amount: Decimal, desired_currency: SupportedCurrencies) -> Self {
        Self {
            usd_amount: Some(usd_amount),
            desired_currency: Some(desired_currency),
            payment_type: None,
        }
    }
}

impl Validated for CreateWithdrawalRequest {
    /// The one check a withdrawal cannot pass without contradicting itself.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if the
    /// amount is set and not positive.
    fn validate(&self) -> crate::Result<()> {
        if self
            .usd_amount
            .is_some_and(|amount| amount <= Decimal::ZERO)
        {
            return Err(crate::Error::InvalidRequest(
                "usd_amount must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A sandbox-only deposit, for testing the funding path end to end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct DemoFundingRequest {
    /// How much to deposit.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub amount: Option<Decimal>,
    /// In what currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<crate::types::SupportedCurrencies>,
    /// The account number to credit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub receiver_account_number: Option<String>,
    /// Its routing code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub receiver_routing_code: Option<String>,
}

impl DemoFundingRequest {
    /// Deposits `amount` of `currency` into `receiver_account_number`.
    #[must_use]
    pub fn new(
        amount: Decimal,
        currency: SupportedCurrencies,
        receiver_account_number: impl Into<String>,
    ) -> Self {
        Self {
            amount: Some(amount),
            currency: Some(currency),
            receiver_account_number: Some(receiver_account_number.into()),
            receiver_routing_code: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_partner_fee_keeps_alpacas_typo() {
        // `parnter_fee` is what the wire sends. Spelling it correctly here
        // would make the value fail to match and fall into `Unknown`.
        assert_eq!(FundingFeeType::PartnerFee.as_str(), "parnter_fee");
    }

    #[test]
    fn complete_is_spelled_differently_here_than_in_instant_funding() {
        // COMPLETE against instant funding's COMPLETED. One character, two
        // families, and no way to share the enum.
        assert_eq!(FundingTransferStatus::Complete.as_str(), "COMPLETE");
        assert_eq!(
            crate::broker::InstantFundingStatus::Completed.as_str(),
            "COMPLETED"
        );
    }

    #[test]
    fn the_direction_is_lower_case_here_and_upper_case_on_crypto_wallets() {
        assert_eq!(FundingDirection::Incoming.as_str(), "incoming");
        assert_eq!(
            crate::trading::TransferDirection::Incoming.as_str(),
            "INCOMING"
        );
    }

    #[test]
    fn a_recipient_bank_needs_only_what_the_reference_requires() {
        // No enforced combination of iban / bic_swift / routing_code: the
        // reference marks all three optional, and guessing would refuse
        // requests Alpaca accepts.
        let bank = CreateRecipientBankRequest::new(
            "12345678",
            "Example Bank",
            "GB",
            SupportedCurrencies::Gbp,
            "1 Example Street",
            "London",
        );
        let json = serde_json::to_value(&bank).unwrap();

        assert!(json.get("iban").is_none());
        assert!(json.get("bic_swift").is_none());
        assert!(json.get("routing_code").is_none());
    }

    #[test]
    fn a_zero_withdrawal_is_refused_before_it_is_sent() {
        let request = CreateWithdrawalRequest::new(Decimal::ZERO, SupportedCurrencies::Gbp);
        assert!(request.validate().is_err());
    }
}
