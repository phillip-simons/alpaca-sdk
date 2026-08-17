//! Request bodies and filters unique to the broker API.
//!
//! Routes that act on behalf of an account reuse the trading API's request
//! types — an order submitted through `/trading/accounts/{id}/orders` takes the
//! same body as one submitted directly. Only the types with no trading
//! equivalent live here.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::broker::enums::{
    AccountEntities, AccountSubType, AccountType, BankAccountType, DocumentType, FeePaymentMethod,
    FundingSource, IdentifierType, JournalEntryType, JournalStatus, PortfolioStatus, RunType,
    TaxIdType, TradeDocumentType, TransferDirection, TransferTiming, TransferType,
    UploadDocumentMimeType, UploadDocumentSubType, VisaType,
};
use crate::broker::models::{
    Agreement, Contact, Disclosures, Identity, RebalancingCondition, TrustedContact, W8BenDocument,
    Weight,
};
use crate::broker::onboarding::ActivityCategory;
use crate::error::{Error, Result};
use crate::trading::ActivityType;
use crate::trading::{AccountStatus, AssetClass};
use crate::types::Sort;
use crate::types::SupportedCurrencies;
use crate::types::Validated;
use crate::types::setters::Setters;

/// An order submitted on behalf of a brokerage account.
///
/// The trading API's [`crate::trading::OrderRequest`] plus the two fields only a
/// correspondent may set: the commission to charge the end user, and the
/// currency to settle in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct OrderRequest {
    /// Every field the trading API also accepts.
    #[serde(flatten)]
    pub order: crate::trading::OrderRequest,
    /// The dollar value commission to charge the end user.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    #[setters(doc = "Charges `commission` dollars to the end user.")]
    pub commission: Option<Decimal>,
    /// The settlement currency. Unset means USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Settles the order in a currency other than USD.")]
    pub currency: Option<SupportedCurrencies>,
}

impl OrderRequest {
    /// Wraps a trading order request so it can be sent on behalf of an account.
    #[must_use]
    pub fn new(order: crate::trading::OrderRequest) -> Self {
        Self {
            order,
            commission: None,
            currency: None,
        }
    }
}

impl Validated for OrderRequest {
    /// Checks the rules Alpaca enforces on the order before it is sent.
    ///
    /// Local currency orders are **not** restricted to market orders here, on
    /// the strength of [the LCT documentation][lct]: "Alpaca currently supports
    /// LCT trading for market,
    /// limit, stop & stop limit orders with a time in force=Day". That page
    /// also names the time-in-force constraint, which is deliberately not
    /// enforced either — it is a statement of what is supported today, and
    /// enforcing it would recreate the same stale-rule bug one field over.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the wrapped order is invalid.
    ///
    /// [lct]: https://docs.alpaca.markets/us/docs/local-currency-trading-lct
    fn validate(&self) -> Result<()> {
        self.order.validate()
    }
}

/// The body that opens a brokerage account.
///
/// The four required fields are the ones [the reference][createaccount] marks
/// required; [`validate`](Self::validate) additionally checks the sub-fields it
/// lists, since a rejection at that depth is otherwise a round trip away.
///
/// [createaccount]: https://docs.alpaca.markets/us/reference/createaccount
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateAccountRequest {
    /// How to reach the account holder.
    pub contact: Contact,
    /// Who the account holder is.
    pub identity: Identity,
    /// The holder's regulatory disclosures.
    pub disclosures: Disclosures,
    /// The agreements the holder has signed. At least one is required.
    pub agreements: Vec<Agreement>,
    /// Whether this is a trading, custodial or other account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_type: Option<AccountType>,
    /// The IRA sub type, for IRA accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_sub_type: Option<AccountSubType>,
    /// Identity documents submitted with the application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub documents: Option<Vec<UploadDocument>>,
    /// A secondary contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_contact: Option<TrustedContact>,
    /// The settlement currency. Unset means USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<SupportedCurrencies>,
    /// Which asset classes the account may trade. Unset means Alpaca decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub enabled_assets: Option<Vec<AssetClass>>,
    /// The existing holder to attach this account to, for multi-live accounts.
    ///
    /// When it is set, Alpaca takes the holder's details from that account
    /// instead of `contact` and `identity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub primary_account_holder_id: Option<String>,
}

impl CreateAccountRequest {
    /// An application for `contact`, signed off by `agreements`.
    #[must_use]
    pub fn new(
        contact: Contact,
        identity: Identity,
        disclosures: Disclosures,
        agreements: Vec<Agreement>,
    ) -> Self {
        Self {
            contact,
            identity,
            disclosures,
            agreements,
            account_type: None,
            account_sub_type: None,
            documents: None,
            trusted_contact: None,
            currency: None,
            enabled_assets: None,
            primary_account_holder_id: None,
        }
    }
}

impl Validated for CreateAccountRequest {
    /// Checks the sub-fields Alpaca requires on a new account.
    ///
    /// The required set is [the reference's][createaccount]. It is worth saying
    /// where it came from: the obvious alternative source, another SDK's
    /// validator, requires a field the reference does not and misses six that it
    /// does.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] naming the first missing field.
    ///
    /// [createaccount]: https://docs.alpaca.markets/us/reference/createaccount
    fn validate(&self) -> Result<()> {
        fn require(present: bool, field: &str) -> Result<()> {
            if present {
                Ok(())
            } else {
                Err(Error::InvalidRequest(format!(
                    "{field} is required to open an account"
                )))
            }
        }

        require(
            !self.contact.email_address.is_empty(),
            "contact.email_address",
        )?;
        require(
            !self.contact.street_address.is_empty(),
            "contact.street_address",
        )?;
        require(self.contact.city.is_some(), "contact.city")?;

        require(!self.identity.given_name.is_empty(), "identity.given_name")?;
        require(
            !self.identity.family_name.is_empty(),
            "identity.family_name",
        )?;
        require(
            self.identity.date_of_birth.is_some(),
            "identity.date_of_birth",
        )?;
        require(self.identity.tax_id_type.is_some(), "identity.tax_id_type")?;
        require(
            self.identity.country_of_tax_residence.is_some(),
            "identity.country_of_tax_residence",
        )?;
        require(
            !self.identity.funding_source.is_empty(),
            "identity.funding_source",
        )?;

        require(
            self.disclosures.is_control_person.is_some(),
            "disclosures.is_control_person",
        )?;
        require(
            self.disclosures.is_affiliated_exchange_or_finra.is_some(),
            "disclosures.is_affiliated_exchange_or_finra",
        )?;
        require(
            self.disclosures.is_politically_exposed.is_some(),
            "disclosures.is_politically_exposed",
        )?;
        require(
            self.disclosures.immediate_family_exposed.is_some(),
            "disclosures.immediate_family_exposed",
        )?;

        require(!self.agreements.is_empty(), "agreements")?;
        for agreement in &self.agreements {
            require(agreement.ip_address.is_some(), "agreements[].ip_address")?;
        }

        // The documents submitted with the application are the same
        // `UploadDocument` the upload route takes, and they carry the same
        // rules — so a W-8BEN sent as a general document has to be refused
        // here for the same reason it is refused there. It was not: this
        // request could put on the wire the exact value
        // `upload_documents_to_account` rejects.
        //
        // The slice impl of `Validated` does the walking.
        self.documents.as_deref().unwrap_or_default().validate()?;

        Ok(())
    }
}

/// Contact details on an account update, where every field is optional.
///
/// The response [`Contact`] requires an email address; an update that only
/// changes a postal code should not have to restate one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct UpdatableContact {
    /// Primary email address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub email_address: Option<String>,
    /// Primary phone number, including the country code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub phone_number: Option<String>,
    /// Street address lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub street_address: Option<Vec<String>>,
    /// Unit or apartment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub unit: Option<String>,
    /// City.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub city: Option<String>,
    /// State or province. Required when the country is `USA`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub state: Option<String>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub postal_code: Option<String>,
    /// Country, as a three-letter code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub country: Option<String>,
}

/// Identity details on an account update.
///
/// The field list is [the reference's][patchaccount], which includes `tax_id`,
/// `tax_id_type` and the `country_of_*` fields.
///
/// Documented as updatable but not yet modelled, because they need enums this
/// crate does not generate: `marital_status`,
/// `investment_experience_with_options`, `investment_experience_with_stocks`.
///
/// [patchaccount]: https://docs.alpaca.markets/us/reference/patchaccount
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct UpdatableIdentity {
    /// Given name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub given_name: Option<String>,
    /// Middle name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub middle_name: Option<String>,
    /// Family name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub family_name: Option<String>,
    /// Date of birth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_of_birth: Option<NaiveDate>,
    /// Tax identification number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub tax_id: Option<String>,
    /// Which national scheme the tax id belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id_type: Option<TaxIdType>,
    /// Country of citizenship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub country_of_citizenship: Option<String>,
    /// Country of birth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub country_of_birth: Option<String>,
    /// Country of tax residence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub country_of_tax_residence: Option<String>,
    /// Where the account's funds come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub funding_source: Option<Vec<FundingSource>>,
    /// Visa category, for non-permanent residents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visa_type: Option<VisaType>,
    /// When the visa expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visa_expiration_date: Option<NaiveDate>,
    /// Intended date of departure from the USA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_of_departure_from_usa: Option<NaiveDate>,
    /// Whether the holder is a permanent resident.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent_resident: Option<bool>,
    /// How many dependents the holder has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_of_dependents: Option<u32>,
    /// Annual income, lower bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub annual_income_min: Option<Decimal>,
    /// Annual income, upper bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub annual_income_max: Option<Decimal>,
    /// Liquid net worth, lower bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub liquid_net_worth_min: Option<Decimal>,
    /// Liquid net worth, upper bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub liquid_net_worth_max: Option<Decimal>,
    /// Total net worth, lower bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub total_net_worth_min: Option<Decimal>,
    /// Total net worth, upper bound.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub total_net_worth_max: Option<Decimal>,
}

/// The body that changes an existing account.
///
/// Every field is optional and unset fields are not sent, so an update touches
/// only what it names.
///
/// [The reference][patchaccount] lists four more top-level fields that this does
/// not carry — `beneficiaries`, `cash_interest`, `fpsl` and `allow_instant_ach`
/// — because they belong to broker features this crate has not modelled yet.
///
/// [patchaccount]: https://docs.alpaca.markets/us/reference/patchaccount
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct UpdateAccountRequest {
    /// New contact details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<UpdatableContact>,
    /// New identity details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<UpdatableIdentity>,
    /// New disclosures. Every field is already optional on this type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disclosures: Option<Disclosures>,
    /// New secondary contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_contact: Option<TrustedContact>,
    /// Further agreements the holder has signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub agreements: Option<Vec<Agreement>>,
    /// The holder this account is attached to, for multi-live accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub primary_account_holder_id: Option<String>,
}

/// Filters for listing accounts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct ListAccountsRequest {
    /// Space-delimited tokens, matched against the account number, phone
    /// number, name and email address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub query: Option<String>,
    /// Only accounts created at or after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_after: Option<DateTime<Utc>>,
    /// Only accounts created at or before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_before: Option<DateTime<Utc>>,
    /// Only accounts in these statuses.
    ///
    /// Sent as one comma-separated parameter. The reference types this as a
    /// single string rather than a list, so more than one value is untested
    /// against the live API.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    #[setters(into)]
    pub status: Option<Vec<AccountStatus>>,
    /// Chronological ordering. Alpaca defaults to descending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    /// Which extra records to fill in on each account.
    ///
    /// The list route omits most of an account's detail to keep the response
    /// small; naming entities here fills them back in. Sent as one
    /// comma-separated parameter, which the reference is explicit about:
    /// "comma-delimited entity names to include in the response".
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    #[setters(into)]
    pub entities: Option<Vec<AccountEntities>>,
}

/// The body that opens an ACH relationship.
///
/// Alpaca accepts two shapes here: bank details entered by hand, or a Plaid
/// processor token. They are one Rust type rather than two so the client method
/// takes one parameter and the choice is checked at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validated)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CreateACHRelationshipRequest {
    /// Bank details entered by the account holder.
    Manual(ManualACHRelationship),
    /// A processor token from a completed Plaid link.
    Plaid(PlaidACHRelationship),
}

/// Bank details for an ACH relationship opened by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct ManualACHRelationship {
    /// The name on the bank account.
    pub account_owner_name: String,
    /// Whether the bank account is checking or savings.
    pub bank_account_type: BankAccountType,
    /// The bank account number.
    pub bank_account_number: String,
    /// The bank's routing number.
    pub bank_routing_number: String,
    /// A name for the relationship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into, doc = "Names the relationship.")]
    pub nickname: Option<String>,
}

/// The processor token from a completed Plaid link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct PlaidACHRelationship {
    /// The Alpaca-specific processor token Plaid returned.
    pub processor_token: String,
}

impl ManualACHRelationship {
    /// The four details Alpaca requires, with the nickname left unset.
    #[must_use]
    pub fn new(
        account_owner_name: impl Into<String>,
        bank_account_type: BankAccountType,
        bank_account_number: impl Into<String>,
        bank_routing_number: impl Into<String>,
    ) -> Self {
        Self {
            account_owner_name: account_owner_name.into(),
            bank_account_type,
            bank_account_number: bank_account_number.into(),
            bank_routing_number: bank_routing_number.into(),
            nickname: None,
        }
    }
}

impl PlaidACHRelationship {
    /// Wraps the processor token Plaid returned.
    #[must_use]
    pub fn new(processor_token: impl Into<String>) -> Self {
        Self {
            processor_token: processor_token.into(),
        }
    }
}

/// The body that connects a recipient bank for wires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateBankRequest {
    /// The bank's name.
    pub name: String,
    /// Whether `bank_code` is a routing number or a BIC.
    pub bank_code_type: IdentifierType,
    /// The 9-digit ABA routing number, or the international BIC.
    pub bank_code: String,
    /// The bank account number.
    pub account_number: String,
    /// The bank's country. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub country: Option<String>,
    /// The bank's state or province. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub state_province: Option<String>,
    /// The bank's postal code. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub postal_code: Option<String>,
    /// The bank's city. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub city: Option<String>,
    /// The bank's street address. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub street_address: Option<String>,
}

impl CreateBankRequest {
    /// A domestic bank, identified by its ABA routing number.
    #[must_use]
    pub fn domestic(
        name: impl Into<String>,
        routing_number: impl Into<String>,
        account_number: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            bank_code_type: IdentifierType::Aba,
            bank_code: routing_number.into(),
            account_number: account_number.into(),
            country: None,
            state_province: None,
            postal_code: None,
            city: None,
            street_address: None,
        }
    }

    /// An international bank, identified by its BIC. Every address field is
    /// required for these; [`validate`](Self::validate) enforces that.
    #[must_use]
    pub fn international(
        name: impl Into<String>,
        bic: impl Into<String>,
        account_number: impl Into<String>,
        address: BankAddress,
    ) -> Self {
        Self {
            name: name.into(),
            bank_code_type: IdentifierType::Bic,
            bank_code: bic.into(),
            account_number: account_number.into(),
            country: Some(address.country),
            state_province: Some(address.state_province),
            postal_code: Some(address.postal_code),
            city: Some(address.city),
            street_address: Some(address.street_address),
        }
    }
}

impl Validated for CreateBankRequest {
    /// Checks the address rules Alpaca documents for bank connections.
    ///
    /// Only one direction is enforced. [The reference][createrecipientbank]
    /// marks the address fields "Only for international banks, ie if
    /// `bank_code_type` = BIC", so setting them on a domestic bank is an error.
    /// It also marks all five *optional*, so an international bank missing one
    /// is **not** rejected here: requiring them would refuse a request Alpaca
    /// accepts.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if a domestic (ABA) bank carries any
    /// address field.
    ///
    /// [createrecipientbank]: https://docs.alpaca.markets/us/reference/createrecipientbank
    fn validate(&self) -> Result<()> {
        let address = [
            ("country", &self.country),
            ("state_province", &self.state_province),
            ("postal_code", &self.postal_code),
            ("city", &self.city),
            ("street_address", &self.street_address),
        ];

        match self.bank_code_type {
            IdentifierType::Aba => {
                for (field, value) in address {
                    if value.is_some() {
                        return Err(Error::InvalidRequest(format!(
                            "{field} may only be set for international bank accounts"
                        )));
                    }
                }
            }
            // The reference marks every address field optional, so an
            // incomplete international bank is Alpaca's to reject, not ours.
            IdentifierType::Bic | IdentifierType::Unknown(_) => {}
        }

        Ok(())
    }
}

/// Where an international bank is located. Every field is required.
///
/// Built from [`Default`] and then assigned field by field: all five are
/// `String`, so a positional constructor would let a city and a postal code
/// swap places with nothing to catch it. Named assignment cannot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Setters)]
#[non_exhaustive]
pub struct BankAddress {
    /// The bank's country.
    pub country: String,
    /// The bank's state or province.
    pub state_province: String,
    /// The bank's postal code.
    pub postal_code: String,
    /// The bank's city.
    pub city: String,
    /// The bank's street address.
    pub street_address: String,
}

/// The body that moves money into or out of an account.
///
/// The two transfer types take disjoint fields, and an enum makes that a
/// compile-time guarantee rather than a runtime check: an ACH transfer cannot
/// carry a `bank_id`, and a wire cannot carry a `relationship_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CreateTransferRequest {
    /// Money moving over an ACH relationship.
    Ach(CreateACHTransferRequest),
    /// Money moving by wire to a connected bank.
    Wire(CreateBankTransferRequest),
}

impl Validated for CreateTransferRequest {
    /// Checks the amount is positive.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the amount is zero or negative.
    fn validate(&self) -> Result<()> {
        let amount = match self {
            Self::Ach(request) => request.amount,
            Self::Wire(request) => request.amount,
        };
        if amount <= Decimal::ZERO {
            return Err(Error::InvalidRequest(
                "transfer amount must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Money moving over an ACH relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateACHTransferRequest {
    /// How much to move. Fees are deducted from this.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Whether the money comes in or goes out.
    pub direction: TransferDirection,
    /// How quickly the transfer should settle.
    pub timing: TransferTiming,
    /// The relationship to move the money over.
    pub relationship_id: Uuid,
    /// Always [`TransferType::Ach`]; Alpaca requires it in the body.
    pub transfer_type: TransferType,
    /// How any fees are paid. Defaults to invoice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_payment_method: Option<FeePaymentMethod>,
}

impl CreateACHTransferRequest {
    /// An ACH transfer over `relationship_id`.
    #[must_use]
    pub fn new(
        amount: Decimal,
        direction: TransferDirection,
        timing: TransferTiming,
        relationship_id: Uuid,
    ) -> Self {
        Self {
            amount,
            direction,
            timing,
            relationship_id,
            transfer_type: TransferType::Ach,
            fee_payment_method: None,
        }
    }
}

/// Money moving by wire to a connected bank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateBankTransferRequest {
    /// How much to move. Fees are deducted from this.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Whether the money comes in or goes out.
    pub direction: TransferDirection,
    /// How quickly the transfer should settle.
    pub timing: TransferTiming,
    /// The bank to wire to.
    pub bank_id: Uuid,
    /// Always [`TransferType::Wire`]; Alpaca requires it in the body.
    pub transfer_type: TransferType,
    /// How any fees are paid. Defaults to invoice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_payment_method: Option<FeePaymentMethod>,
    /// Detail carried with the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub additional_information: Option<String>,
}

impl CreateBankTransferRequest {
    /// A wire transfer to `bank_id`.
    #[must_use]
    pub fn new(
        amount: Decimal,
        direction: TransferDirection,
        timing: TransferTiming,
        bank_id: Uuid,
    ) -> Self {
        Self {
            amount,
            direction,
            timing,
            bank_id,
            transfer_type: TransferType::Wire,
            fee_payment_method: None,
            additional_information: None,
        }
    }
}

/// Filters for listing an account's transfers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetTransfersRequest {
    /// Only transfers moving this way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<TransferDirection>,
    /// Maximum number of transfers per page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// How many transfers to skip.
    ///
    /// Set by the paginating helper on each pass; a value set here is the
    /// starting offset for a single-page request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// The body that opens a journal between two accounts.
///
/// Cash and security journals share a shape but not a set of fields: a cash
/// journal carries an `amount` and no `symbol`/`qty`, a security journal the
/// reverse. [`validate`](Self::validate) enforces that. Build one with
/// [`cash`](Self::cash) or
/// [`security`](Self::security) and the right fields are set for you.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateJournalRequest {
    /// The account the money or securities come from.
    pub from_account: Uuid,
    /// The account they go to.
    pub to_account: Uuid,
    /// Whether this journal moves cash or securities.
    pub entry_type: JournalEntryType,
    /// The cash amount. Cash journals only.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub amount: Option<Decimal>,
    /// The security to move. Security journals only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub symbol: Option<String>,
    /// How much of the security to move. Security journals only.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub qty: Option<Decimal>,
    /// Free-text description. Sandbox reads fixture directives from this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub description: Option<String>,
    /// Travel rule: the transmitter's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_name: Option<String>,
    /// Travel rule: the transmitter's account number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_account_number: Option<String>,
    /// Travel rule: the transmitter's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_address: Option<String>,
    /// Travel rule: the transmitter's financial institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_financial_institution: Option<String>,
    /// Travel rule: when the transfer was transmitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_timestamp: Option<String>,
    /// The settlement currency. Unset means USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<SupportedCurrencies>,
}

impl CreateJournalRequest {
    fn base(from_account: Uuid, to_account: Uuid, entry_type: JournalEntryType) -> Self {
        Self {
            from_account,
            to_account,
            entry_type,
            amount: None,
            symbol: None,
            qty: None,
            description: None,
            transmitter_name: None,
            transmitter_account_number: None,
            transmitter_address: None,
            transmitter_financial_institution: None,
            transmitter_timestamp: None,
            currency: None,
        }
    }

    /// A cash journal moving `amount` between two accounts.
    #[must_use]
    pub fn cash(from_account: Uuid, to_account: Uuid, amount: Decimal) -> Self {
        let mut request = Self::base(from_account, to_account, JournalEntryType::Cash);
        request.amount = Some(amount);
        request
    }

    /// A security journal moving `qty` of `symbol` between two accounts.
    #[must_use]
    pub fn security(
        from_account: Uuid,
        to_account: Uuid,
        symbol: impl Into<String>,
        qty: Decimal,
    ) -> Self {
        let mut request = Self::base(from_account, to_account, JournalEntryType::Security);
        request.symbol = Some(symbol.into());
        request.qty = Some(qty);
        request
    }
}

impl Validated for CreateJournalRequest {
    /// Checks that the fields set match the entry type.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if a cash journal carries a symbol or
    /// quantity or no amount, or a security journal carries an amount or is
    /// missing its symbol or quantity.
    fn validate(&self) -> Result<()> {
        match self.entry_type {
            JournalEntryType::Cash => {
                if self.symbol.is_some() || self.qty.is_some() {
                    return Err(Error::InvalidRequest(
                        "symbol and qty are reserved for security journals".to_owned(),
                    ));
                }
                if self.amount.is_none() {
                    return Err(Error::InvalidRequest(
                        "cash journals must carry an amount".to_owned(),
                    ));
                }
            }
            JournalEntryType::Security => {
                if self.amount.is_some() {
                    return Err(Error::InvalidRequest(
                        "amount is reserved for cash journals".to_owned(),
                    ));
                }
                if self.symbol.is_none() || self.qty.is_none() {
                    return Err(Error::InvalidRequest(
                        "security journals must carry a symbol and a qty".to_owned(),
                    ));
                }
            }
            // A value Alpaca added since; let the API judge it.
            JournalEntryType::Unknown(_) => {}
        }

        Ok(())
    }
}

/// One destination in a batch journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct BatchJournalRequestEntry {
    /// The account to fund.
    pub to_account: Uuid,
    /// How much cash to send.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub description: Option<String>,
    /// Travel rule: the transmitter's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_name: Option<String>,
    /// Travel rule: the transmitter's account number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_account_number: Option<String>,
    /// Travel rule: the transmitter's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_address: Option<String>,
    /// Travel rule: the transmitter's financial institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_financial_institution: Option<String>,
    /// Travel rule: when the transfer was transmitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_timestamp: Option<String>,
}

impl BatchJournalRequestEntry {
    /// Sends `amount` to `to_account`.
    #[must_use]
    pub fn new(to_account: Uuid, amount: Decimal) -> Self {
        Self {
            to_account,
            amount,
            description: None,
            transmitter_name: None,
            transmitter_account_number: None,
            transmitter_address: None,
            transmitter_financial_institution: None,
            transmitter_timestamp: None,
        }
    }
}

/// One source in a reverse batch journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct ReverseBatchJournalRequestEntry {
    /// The account to draw from.
    pub from_account: Uuid,
    /// How much cash to draw.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub description: Option<String>,
    /// Travel rule: the transmitter's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_name: Option<String>,
    /// Travel rule: the transmitter's account number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_account_number: Option<String>,
    /// Travel rule: the transmitter's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_address: Option<String>,
    /// Travel rule: the transmitter's financial institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_financial_institution: Option<String>,
    /// Travel rule: when the transfer was transmitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub transmitter_timestamp: Option<String>,
}

impl ReverseBatchJournalRequestEntry {
    /// Draws `amount` from `from_account`.
    #[must_use]
    pub fn new(from_account: Uuid, amount: Decimal) -> Self {
        Self {
            from_account,
            amount,
            description: None,
            transmitter_name: None,
            transmitter_account_number: None,
            transmitter_address: None,
            transmitter_financial_institution: None,
            transmitter_timestamp: None,
        }
    }
}

/// Cash moving out of one account into many.
///
/// Only cash batch journals are supported, so `entry_type` is always
/// [`JournalEntryType::Cash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateBatchJournalRequest {
    /// Always [`JournalEntryType::Cash`].
    pub entry_type: JournalEntryType,
    /// The account the money comes from, usually the sweep firm account.
    pub from_account: Uuid,
    /// Where the money goes.
    pub entries: Vec<BatchJournalRequestEntry>,
}

impl CreateBatchJournalRequest {
    /// A batch paying every entry out of `from_account`.
    #[must_use]
    pub fn new(from_account: Uuid, entries: Vec<BatchJournalRequestEntry>) -> Self {
        Self {
            entry_type: JournalEntryType::Cash,
            from_account,
            entries,
        }
    }
}

/// Cash moving into one account out of many.
///
/// Only cash reverse batch journals are supported, so `entry_type` is always
/// [`JournalEntryType::Cash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateReverseBatchJournalRequest {
    /// Always [`JournalEntryType::Cash`].
    pub entry_type: JournalEntryType,
    /// The account the money goes to, usually the sweep firm account.
    pub to_account: Uuid,
    /// Where the money comes from.
    pub entries: Vec<ReverseBatchJournalRequestEntry>,
}

impl CreateReverseBatchJournalRequest {
    /// A reverse batch collecting every entry into `to_account`.
    #[must_use]
    pub fn new(to_account: Uuid, entries: Vec<ReverseBatchJournalRequestEntry>) -> Self {
        Self {
            entry_type: JournalEntryType::Cash,
            to_account,
            entries,
        }
    }
}

/// Filters for listing journals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetJournalsRequest {
    /// Only journals created on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<NaiveDate>,
    /// Only journals created on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<NaiveDate>,
    /// Only journals in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<JournalStatus>,
    /// Only cash or only security journals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<JournalEntryType>,
    /// Only journals into this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_account: Option<Uuid>,
    /// Only journals out of this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_account: Option<Uuid>,
}

/// Filters for listing an account's trade documents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetTradeDocumentsRequest {
    /// Only documents dated on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// Only documents dated on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
    /// Only documents of this kind.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub document_type: Option<TradeDocumentType>,
}

impl Validated for GetTradeDocumentsRequest {
    /// Checks the date window is the right way round.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if `start` is after `end`.
    fn validate(&self) -> Result<()> {
        if let (Some(start), Some(end)) = (self.start, self.end)
            && start > end
        {
            return Err(Error::InvalidRequest(
                "start must not be after end".to_owned(),
            ));
        }
        Ok(())
    }
}

/// One document in an upload.
///
/// W-8BEN forms take a different shape from every other document — they may be
/// sent as structured fields rather than an encoded file. The enum makes using
/// one shape for the other's document type unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum UploadDocument {
    /// Any document other than a W-8BEN.
    Document(UploadDocumentRequest),
    /// A W-8BEN, as an encoded file or as fields.
    W8Ben(UploadW8BenDocumentRequest),
}

impl Validated for UploadDocument {
    /// Checks the document is internally consistent.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if a general upload claims to be a
    /// W-8BEN, or a W-8BEN upload sets neither or both of its content fields.
    fn validate(&self) -> Result<()> {
        match self {
            Self::Document(document) => document.validate(),
            Self::W8Ben(document) => document.validate(),
        }
    }
}

/// A document uploaded as base64-encoded content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct UploadDocumentRequest {
    /// What kind of document this is.
    pub document_type: DocumentType,
    /// A more specific classification, where the type supports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_sub_type: Option<UploadDocumentSubType>,
    /// The document itself, base64-encoded.
    pub content: String,
    /// The format of the encoded content.
    pub mime_type: UploadDocumentMimeType,
}

impl UploadDocumentRequest {
    /// A document of `document_type` carrying base64 `content`.
    #[must_use]
    pub fn new(
        document_type: DocumentType,
        content: impl Into<String>,
        mime_type: UploadDocumentMimeType,
    ) -> Self {
        Self {
            document_type,
            document_sub_type: None,
            content: content.into(),
            mime_type,
        }
    }
}

impl Validated for UploadDocumentRequest {
    /// Checks this is not a W-8BEN in disguise.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the type or sub type says W-8BEN;
    /// those go through [`UploadW8BenDocumentRequest`].
    fn validate(&self) -> Result<()> {
        if self.document_type == DocumentType::W8ben
            || self.document_sub_type == Some(UploadDocumentSubType::FormW8Ben)
        {
            return Err(Error::InvalidRequest(
                "use UploadW8BenDocumentRequest to upload a W-8BEN".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A W-8BEN upload, as an encoded file or as structured fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct UploadW8BenDocumentRequest {
    /// Always [`DocumentType::W8ben`]; Alpaca requires it in the body.
    pub document_type: DocumentType,
    /// Always [`UploadDocumentSubType::FormW8Ben`].
    pub document_sub_type: UploadDocumentSubType,
    /// The form as base64-encoded content. Set this or `content_data`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub content: Option<String>,
    /// The form as fields. Set this or `content`.
    ///
    /// Boxed because it dwarfs every other field here, and this type is one
    /// variant of [`UploadDocument`] — an unboxed form would make every upload
    /// in a batch as large as the largest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_data: Option<Box<W8BenDocument>>,
    /// The format of the content. Always JSON when `content_data` is set.
    pub mime_type: UploadDocumentMimeType,
}

impl UploadW8BenDocumentRequest {
    /// A W-8BEN uploaded as an encoded file.
    #[must_use]
    pub fn from_content(content: impl Into<String>, mime_type: UploadDocumentMimeType) -> Self {
        Self {
            document_type: DocumentType::W8ben,
            document_sub_type: UploadDocumentSubType::FormW8Ben,
            content: Some(content.into()),
            content_data: None,
            mime_type,
        }
    }

    /// A W-8BEN filled in field by field. Always sent as JSON.
    #[must_use]
    pub fn from_fields(document: W8BenDocument) -> Self {
        Self {
            document_type: DocumentType::W8ben,
            document_sub_type: UploadDocumentSubType::FormW8Ben,
            content: None,
            content_data: Some(Box::new(document)),
            mime_type: UploadDocumentMimeType::Json,
        }
    }
}

impl Validated for UploadW8BenDocumentRequest {
    /// Checks exactly one content form is set, and that the pieces agree.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if neither or both of `content` and
    /// `content_data` are set, if the type or sub type has been changed, if
    /// `content_data` is paired with a mime type other than JSON, or if the
    /// form itself fails [`W8BenDocument::validate`].
    fn validate(&self) -> Result<()> {
        match (&self.content, &self.content_data) {
            (None, None) => {
                return Err(Error::InvalidRequest(
                    "a W-8BEN upload needs either content or content_data".to_owned(),
                ));
            }
            (Some(_), Some(_)) => {
                return Err(Error::InvalidRequest(
                    "a W-8BEN upload takes content or content_data, not both".to_owned(),
                ));
            }
            (Some(_), None) => {}
            (None, Some(document)) => {
                if self.mime_type != UploadDocumentMimeType::Json {
                    return Err(Error::InvalidRequest(
                        "content_data must be sent as application/json".to_owned(),
                    ));
                }
                document.validate()?;
            }
        }

        if self.document_type != DocumentType::W8ben
            || self.document_sub_type != UploadDocumentSubType::FormW8Ben
        {
            return Err(Error::InvalidRequest(
                "a W-8BEN upload must keep the W8BEN document type and sub type".to_owned(),
            ));
        }

        Ok(())
    }
}

/// The body that creates a portfolio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreatePortfolioRequest {
    /// A name for the portfolio.
    pub name: String,
    /// What the portfolio is for.
    pub description: String,
    /// The target allocation. Percentages should total 100.
    pub weights: Vec<Weight>,
    /// Days to wait after a rebalance before rebalancing again.
    pub cooldown_days: u32,
    /// When to rebalance towards the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub rebalance_conditions: Option<Vec<RebalancingCondition>>,
}

impl CreatePortfolioRequest {
    /// A portfolio targeting `weights`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        weights: Vec<Weight>,
        cooldown_days: u32,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            weights,
            cooldown_days,
            rebalance_conditions: None,
        }
    }
}

impl Validated for CreatePortfolioRequest {
    /// Checks every weight.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if any weight is not positive or an
    /// asset weight names no symbol.
    fn validate(&self) -> Result<()> {
        for weight in &self.weights {
            weight.validate()?;
        }
        Ok(())
    }
}

/// The body that changes a portfolio.
///
/// Changing the weights or the conditions re-evaluates every subscribed account
/// at the next opportunity, subject to the cooldown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct UpdatePortfolioRequest {
    /// A new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub name: Option<String>,
    /// A new description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub description: Option<String>,
    /// A new target allocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub weights: Option<Vec<Weight>>,
    /// A new cooldown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_days: Option<u32>,
    /// New rebalancing conditions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub rebalance_conditions: Option<Vec<RebalancingCondition>>,
}

impl Validated for UpdatePortfolioRequest {
    /// Checks every weight the update sets.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if any weight is not positive or an
    /// asset weight names no symbol.
    fn validate(&self) -> Result<()> {
        for weight in self.weights.iter().flatten() {
            weight.validate()?;
        }
        Ok(())
    }
}

/// Filters for listing portfolios.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetPortfoliosRequest {
    /// Only portfolios with this name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub name: Option<String>,
    /// Only portfolios with this description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub description: Option<String>,
    /// Only portfolios holding this security.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub symbol: Option<String>,
    /// Only this portfolio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portfolio_id: Option<Uuid>,
    /// Only portfolios in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PortfolioStatus>,
}

/// The body that subscribes an account to a portfolio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateSubscriptionRequest {
    /// The account to rebalance.
    pub account_id: Uuid,
    /// The portfolio to rebalance it towards.
    pub portfolio_id: Uuid,
}

impl CreateSubscriptionRequest {
    /// Subscribes `account_id` to `portfolio_id`.
    #[must_use]
    pub fn new(account_id: Uuid, portfolio_id: Uuid) -> Self {
        Self {
            account_id,
            portfolio_id,
        }
    }
}

/// Filters for listing subscriptions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetSubscriptionsRequest {
    /// Only this account's subscription.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// Only subscriptions to this portfolio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portfolio_id: Option<Uuid>,
    /// Page size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The page to fetch. Set by the paginating helper on each pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub page_token: Option<String>,
}

/// The body that starts a rebalancing run by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateRunRequest {
    /// The account to rebalance.
    pub account_id: Uuid,
    /// Whether to rebalance fully or invest cash.
    #[serde(rename = "type")]
    pub run_type: RunType,
    /// The weights to rebalance towards.
    pub weights: Vec<Weight>,
}

impl CreateRunRequest {
    /// A run moving `account_id` towards `weights`.
    #[must_use]
    pub fn new(account_id: Uuid, run_type: RunType, weights: Vec<Weight>) -> Self {
        Self {
            account_id,
            run_type,
            weights,
        }
    }
}

impl Validated for CreateRunRequest {
    /// Checks every weight.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if any weight is not positive or an
    /// asset weight names no symbol.
    fn validate(&self) -> Result<()> {
        for weight in &self.weights {
            weight.validate()?;
        }
        Ok(())
    }
}

/// Filters for listing rebalancing runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetRunsRequest {
    /// Only this account's runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// Only runs of this type.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub run_type: Option<RunType>,
    /// Page size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The page to fetch.
    ///
    /// The reference does not list it on this route, but the route pages, and
    /// without it a caller cannot reach the second page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub page_token: Option<String>,
}

/// Filters for listing account activities across accounts.
///
/// One documented exclusivity: [`category`](Self::category) and
/// [`activity_types`](Self::activity_types) cannot both be set, which
/// [`validate`](Self::validate) enforces.
///
/// `date` alongside `after` or `until` is **not** rejected. It is a plausible
/// rule and nothing in the reference or the spec states it, and this crate does
/// not refuse requests on a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetAccountActivitiesRequest {
    /// Only this account's activities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    /// Only activities of these kinds.
    ///
    /// Sent as one comma-separated parameter.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    #[setters(into)]
    pub activity_types: Option<Vec<ActivityType>>,
    /// Only trade activities, or only non-trade ones.
    ///
    /// The coarse counterpart to [`activity_types`](Self::activity_types), and
    /// **mutually exclusive with it** — the reference says so in as many words:
    /// "Cannot be used with `activity_types` parameter". [`validate`] enforces
    /// that.
    ///
    /// [`validate`]: Self::validate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Only activities in this category.")]
    pub category: Option<ActivityCategory>,
    /// Only activities belonging to one order.
    ///
    /// The way to fetch the fills that made up a completed order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Only activities belonging to this order.")]
    pub order_id: Option<Uuid>,
    /// Only activities on this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    /// Only activities before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Only activities after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Which way to sort. Defaults to descending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Sort>,
    /// How many activities to return per page.
    ///
    /// Defaults to 100, and is capped there — unless `date` is set, in which
    /// case Alpaca may return everything in one response and ignore paging
    /// altogether.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// Where to resume from: the `id` of the last activity already seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub page_token: Option<String>,
}

impl Validated for GetAccountActivitiesRequest {
    /// Rejects the one combination the reference forbids.
    ///
    /// `category` and `activity_types` cannot be sent together: "Cannot be used
    /// with `activity_types` parameter". That is a documented rule, so it is
    /// enforced.
    ///
    /// `date` combined with `after` or `until` is a plausible conflict that the
    /// reference documents no rule against, so it is not enforced — refusing a
    /// request Alpaca would accept is the worse of the two failures. That is the
    /// rule throughout: a documented constraint is enforced here, an undocumented
    /// one is left to the server.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if both
    /// `category` and `activity_types` are set.
    fn validate(&self) -> Result<()> {
        if self.category.is_some() && self.activity_types.is_some() {
            return Err(crate::Error::InvalidRequest(
                "category cannot be combined with activity_types".to_owned(),
            ));
        }
        Ok(())
    }
}

/// The body of an option exercise request.
///
/// Both fields are optional and unset ones are dropped, so an exercise with no
/// commission posts `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct CreateOptionExerciseRequest {
    /// The commission to charge the end user, in dollars.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    #[setters(doc = "Charges `commission` dollars to the end user.")]
    pub commission: Option<Decimal>,
}

impl CreateOptionExerciseRequest {
    /// An exercise request that charges no commission.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every parent that carries a [`W8BenDocument`] or a [`Weight`] must call
    /// that type's inherent `validate` from its own [`Validated`] impl.
    ///
    /// This is the whole basis of the exemption those two types have. Neither
    /// derives `Validated` and neither implements it, deliberately: both carry
    /// real rules and are only ever sent nested, so a no-op impl would let one
    /// be handed to the transport and checked by nothing.
    /// `scripts/validated.py` records that decision and fails if either stops
    /// declaring the `validate` it is about — but no script can check that the
    /// *parent* still calls it, and until these tests existed nothing did.
    /// Each of the delegations below could be deleted with the whole suite
    /// still green.
    ///
    /// A nested type's rules are only worth having if the enclosing request
    /// asks for them, so the assertion is on the parent, not on the child.
    mod nested_rules_reach_their_parent {
        use super::*;

        /// A W-8BEN naming none of `foreign_tax_id`, `tax_id_ssn` or
        /// `ftin_not_required` — the one combination `W8BenDocument::validate`
        /// refuses. Every required field is filled so that the only thing wrong
        /// with it is the thing under test.
        fn unidentified_w8ben() -> crate::broker::W8BenDocument {
            let mut document = crate::broker::W8BenDocument {
                country_citizen: "DE".to_owned(),
                date: "2024-01-01".parse().unwrap(),
                date_of_birth: "1980-01-01".parse().unwrap(),
                full_name: "A Person".to_owned(),
                ip_address: "203.0.113.1".parse().unwrap(),
                permanent_address_city_state: "Berlin".to_owned(),
                permanent_address_country: "DE".to_owned(),
                permanent_address_street: "1 Some Street".to_owned(),
                revision: "October 2021".to_owned(),
                signer_full_name: "A Person".to_owned(),
                timestamp: "2024-01-01T00:00:00Z".parse().unwrap(),
                additional_conditions: None,
                foreign_tax_id: None,
                ftin_not_required: None,
                income_type: None,
                mailing_address_city_state: None,
                mailing_address_country: None,
                mailing_address_street: None,
                paragraph_number: None,
                percent_rate_withholding: None,
                reference_number: None,
                residency: None,
                tax_id_ssn: None,
            };
            // Belt and braces: the three fields the rule is about are already
            // `None` above, and saying so here means a future field addition
            // that defaults one of them cannot quietly make this document valid.
            document.foreign_tax_id = None;
            document.tax_id_ssn = None;
            document.ftin_not_required = None;
            document
        }

        /// An asset weight naming no symbol, which `Weight::validate` refuses.
        /// Built through the field rather than `Weight::asset`, because that
        /// constructor cannot produce this state — which is the point of it.
        fn symbolless_asset_weight() -> crate::broker::Weight {
            let mut weight = crate::broker::Weight::asset("AAPL", Decimal::from(100));
            weight.symbol = None;
            weight
        }

        #[test]
        fn the_w8ben_upload_asks_its_document() {
            let request = UploadW8BenDocumentRequest::from_fields(unidentified_w8ben());
            assert!(request.validate().is_err());
        }

        /// And through the enum the upload route actually takes, which is one
        /// delegation further out.
        ///
        /// Both arms, because they are separate delegations: covering only the
        /// `W8Ben` one leaves `Document` free to return `Ok(())` with the suite
        /// still green, and a general upload claiming to be a W-8BEN is exactly
        /// what `UploadDocumentRequest`'s rule exists to refuse.
        #[test]
        fn the_upload_enum_asks_the_w8ben_variant_it_holds() {
            let document = UploadDocument::W8Ben(UploadW8BenDocumentRequest::from_fields(
                unidentified_w8ben(),
            ));
            assert!(document.validate().is_err());
        }

        /// An account application with every field the reference requires, so
        /// that the only thing left to refuse is whatever is added to it.
        fn otherwise_valid_application() -> CreateAccountRequest {
            let contact = Contact {
                email_address: "someone@example.com".to_owned(),
                street_address: vec!["1 Some Street".to_owned()],
                city: Some("Berlin".to_owned()),
                ..Default::default()
            };

            let identity = Identity {
                given_name: "A".to_owned(),
                family_name: "Person".to_owned(),
                date_of_birth: Some("1980-01-01".parse().unwrap()),
                tax_id_type: Some(TaxIdType::UsaSsn),
                country_of_tax_residence: Some("USA".to_owned()),
                funding_source: vec![FundingSource::EmploymentIncome],
                ..Default::default()
            };

            let disclosures = Disclosures {
                is_control_person: Some(false),
                is_affiliated_exchange_or_finra: Some(false),
                is_politically_exposed: Some(false),
                immediate_family_exposed: Some(false),
                ..Default::default()
            };

            let mut agreement = Agreement::new(
                crate::broker::AgreementType::Customer,
                "2024-01-01T00:00:00Z".parse().unwrap(),
            );
            agreement.ip_address = Some("203.0.113.1".to_owned());

            CreateAccountRequest::new(contact, identity, disclosures, vec![agreement])
        }

        /// The application itself has to be sound, or this test would pass on
        /// the wrong error.
        #[test]
        fn the_baseline_application_is_accepted() {
            otherwise_valid_application().validate().unwrap();
        }

        /// The same documents, reached through the field
        /// `CreateAccountRequest` carries them in. This was the live gap:
        /// `create_account` would send a W-8BEN disguised as a general document
        /// — the exact value `upload_documents_to_account` refuses — because
        /// the parent never asked.
        #[test]
        fn creating_an_account_asks_the_documents_it_carries() {
            let mut request = otherwise_valid_application();
            request.documents = Some(vec![UploadDocument::Document(UploadDocumentRequest::new(
                DocumentType::W8ben,
                "QQ==",
                UploadDocumentMimeType::Pdf,
            ))]);

            let error = request.validate().unwrap_err();
            assert!(
                format!("{error}").contains("UploadW8BenDocumentRequest"),
                "the document's own rule should be what fires, got: {error}"
            );
        }

        #[test]
        fn the_upload_enum_asks_the_document_variant_it_holds() {
            let document = UploadDocument::Document(UploadDocumentRequest::new(
                DocumentType::W8ben,
                "QQ==",
                UploadDocumentMimeType::Pdf,
            ));
            assert!(document.validate().is_err());
        }

        #[test]
        fn creating_a_portfolio_asks_every_weight() {
            let request = CreatePortfolioRequest::new(
                "Growth",
                "Mostly equities",
                vec![symbolless_asset_weight()],
                7,
            );
            assert!(request.validate().is_err());
        }

        #[test]
        fn updating_a_portfolio_asks_every_weight() {
            let request =
                UpdatePortfolioRequest::default().weights(vec![symbolless_asset_weight()]);
            assert!(request.validate().is_err());
        }

        #[test]
        fn creating_a_run_asks_every_weight() {
            let request = CreateRunRequest::new(
                Uuid::nil(),
                RunType::FullRebalance,
                vec![symbolless_asset_weight()],
            );
            assert!(request.validate().is_err());
        }

        /// The broker's `OrderRequest` is a thin wrapper around the trading
        /// one, and its `Validated` impl is a single delegating line. Deleting
        /// it left every order route unchecked with the suite still green.
        #[test]
        fn the_broker_order_wrapper_asks_the_order_it_wraps() {
            let mut order = crate::trading::OrderRequest::market(
                "AAPL",
                crate::trading::OrderSide::Buy,
                crate::trading::OrderAmount::Qty(Decimal::from(1)),
                crate::trading::TimeInForce::Day,
            );
            // A bracket order with neither exit leg.
            order.order_class = Some(crate::trading::OrderClass::Bracket);

            assert!(OrderRequest::new(order).validate().is_err());
        }
    }
}
