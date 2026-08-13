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

use crate::types::wire::wire_enum;

wire_enum! {
    /// Whether a funding wallet may be used.
    pub enum FundingWalletStatus {
        /// Usable.
        Active => "active",
        /// Being set up.
        Pending => "pending",
        /// Switched off.
        Disabled => "disabled",
    }
}

wire_enum! {
    /// Which rail money moves on.
    pub enum PaymentType {
        /// International wire.
        SwiftWire => "swift_wire",
        /// A domestic scheme in the destination country.
        LocalRails => "local_rails",
    }
}

wire_enum! {
    /// Which way a funding wallet transfer moves.
    ///
    /// Lower-case here, where the crypto wallet's equivalent is upper-case.
    /// Two families, two conventions.
    pub enum FundingDirection {
        /// Into the account.
        Incoming => "incoming",
        /// Out of it.
        Outgoing => "outgoing",
    }
}

wire_enum! {
    /// Where a funding wallet transfer stands.
    pub enum FundingTransferStatus {
        /// Submitted.
        Pending => "PENDING",
        /// Withdrawn.
        Canceled => "CANCELED",
        /// Sent.
        Executed => "EXECUTED",
        /// Failed.
        Failed => "FAILED",
        /// Settled.
        ///
        /// `COMPLETE`, not `COMPLETED` — the instant funding family spells the
        /// same idea the other way.
        Complete => "COMPLETE",
    }
}

wire_enum! {
    /// What a fee on a funding wallet transfer is for.
    pub enum FundingFeeType {
        /// Sending money out.
        WithdrawalFee => "withdrawal_fee",
        /// Converting currency.
        FxFee => "fx_fee",
        /// The network's own charge.
        NetworkFee => "network_fee",
        /// Taking money in.
        DepositFee => "deposit_fee",
        /// A returned ACH.
        AchReturnFee => "ach_return_fee",
        /// The correspondent's cut.
        ///
        /// Spelled `parnter_fee` on the wire. That is Alpaca's typo, and
        /// matching it is what makes the value decode.
        PartnerFee => "parnter_fee",
        /// Alpaca's cut.
        AlpacaFee => "alpaca_fee",
    }
}

wire_enum! {
    /// Which national scheme a routing code belongs to.
    pub enum RoutingCodeType {
        /// UK sort code.
        SortCode => "sort_code",
        /// US ABA routing number.
        Aba => "aba",
        /// Australian BSB.
        BsbCode => "bsb_code",
        /// Canadian institution number.
        InstitutionNo => "institution_no",
        /// A bank code.
        BankCode => "bank_code",
        /// A branch code.
        BranchCode => "branch_code",
        /// Mexican CLABE.
        Clabe => "clabe",
        /// Chinese CNAPS.
        Cnaps => "cnaps",
        /// Indian IFSC.
        Ifsc => "ifsc",
    }
}

wire_enum! {
    /// What kind of bank account a recipient bank is.
    ///
    /// Lower-case here, where the ACH relationship's
    /// [`BankAccountType`](crate::broker::BankAccountType) is upper-case and
    /// carries an empty variant besides. Same idea, two wire vocabularies, so
    /// two types.
    pub enum RecipientAccountType {
        /// Checking.
        Checking => "checking",
        /// Savings.
        Savings => "savings",
    }
}

/// An account's funding wallet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct FundingFee {
    /// What it is for.
    #[serde(rename = "type")]
    pub fee_type: FundingFeeType,
    /// How much.
    pub amount: Decimal,
    /// In what currency.
    pub currency: String,
    /// How it is charged.
    pub payment_type: String,
}

/// The USD leg of a transfer denominated in something else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsdAmount {
    /// How much, in USD.
    pub amount: Decimal,
}

/// Money in or out of a funding wallet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub requested_amount: Option<Decimal>,
    /// What arrived, before conversion.
    #[serde(default)]
    pub original_amount: Option<Decimal>,
    /// In what currency.
    #[serde(default)]
    pub original_currency: Option<String>,
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
    pub routing_code_type: Option<String>,
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
    pub currency: Option<String>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BatchCreateFundingWalletsRequest {
    /// The accounts to open wallets for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetFundingDetailsRequest {
    /// Only details for this rail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_type: Option<PaymentType>,
    /// Only details for this currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// A request to register a bank a withdrawal may be sent to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateRecipientBankRequest {
    /// The account number.
    pub account_number: String,
    /// The bank's name.
    pub bank_name: String,
    /// Which country it is in.
    pub bank_country: String,
    /// The currency it takes.
    pub currency: String,
    /// Street address.
    pub street_address: String,
    /// City.
    pub city: String,
    /// State or province.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_or_province: Option<String>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    /// The holder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_account_holder_name: Option<String>,
    /// What kind of account it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_type: Option<RecipientAccountType>,
    /// The IBAN, where the country uses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iban: Option<String>,
    /// The BIC or SWIFT code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bic_swift: Option<String>,
    /// The national routing code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_code: Option<String>,
    /// Which scheme that code belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_code_type: Option<RoutingCodeType>,
}

impl CreateRecipientBankRequest {
    /// The five fields the route requires, and nothing assumed beyond them.
    ///
    /// Which of `iban`, `bic_swift` and `routing_code` a given country needs is
    /// Alpaca's business: the reference marks all three optional, and enforcing
    /// a combination here would refuse requests it accepts. That is the same
    /// finding the phase 6.5 audit recorded about international bank addresses.
    pub fn new(
        account_number: impl Into<String>,
        bank_name: impl Into<String>,
        bank_country: impl Into<String>,
        currency: impl Into<String>,
        street_address: impl Into<String>,
        city: impl Into<String>,
    ) -> Self {
        Self {
            account_number: account_number.into(),
            bank_name: bank_name.into(),
            bank_country: bank_country.into(),
            currency: currency.into(),
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

    /// Sets the IBAN.
    #[must_use]
    pub fn iban(mut self, iban: impl Into<String>) -> Self {
        self.iban = Some(iban.into());
        self
    }

    /// Sets the BIC or SWIFT code.
    #[must_use]
    pub fn bic_swift(mut self, bic_swift: impl Into<String>) -> Self {
        self.bic_swift = Some(bic_swift.into());
        self
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateWithdrawalRequest {
    /// How much to send, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd_amount: Option<Decimal>,
    /// What to convert it to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_currency: Option<String>,
    /// Which rail to send it on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_type: Option<PaymentType>,
}

impl CreateWithdrawalRequest {
    /// Sends `usd_amount` out as `desired_currency`.
    #[must_use]
    pub fn new(usd_amount: Decimal, desired_currency: impl Into<String>) -> Self {
        Self {
            usd_amount: Some(usd_amount),
            desired_currency: Some(desired_currency.into()),
            payment_type: None,
        }
    }

    /// Sets the rail.
    #[must_use]
    pub fn payment_type(mut self, payment_type: PaymentType) -> Self {
        self.payment_type = Some(payment_type);
        self
    }

    /// The one check a withdrawal cannot pass without contradicting itself.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if the
    /// amount is set and not positive.
    pub fn validate(&self) -> crate::Result<()> {
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DemoFundingRequest {
    /// How much to deposit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Decimal>,
    /// In what currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// The account number to credit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_account_number: Option<String>,
    /// Its routing code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_routing_code: Option<String>,
}

impl DemoFundingRequest {
    /// Deposits `amount` of `currency` into `receiver_account_number`.
    #[must_use]
    pub fn new(
        amount: Decimal,
        currency: impl Into<String>,
        receiver_account_number: impl Into<String>,
    ) -> Self {
        Self {
            amount: Some(amount),
            currency: Some(currency.into()),
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
            "GBP",
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
        let request = CreateWithdrawalRequest::new(Decimal::ZERO, "GBP");
        assert!(request.validate().is_err());
    }
}
