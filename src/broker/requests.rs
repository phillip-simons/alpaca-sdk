//! Request bodies and filters unique to the broker API, ported from
//! `alpaca/broker/requests.py`.
//!
//! Routes that act on behalf of an account reuse the trading API's request
//! types — an order submitted through `/trading/accounts/{id}/orders` takes the
//! same body as one submitted directly — exactly as alpaca-py's broker module
//! imports them from `alpaca.trading`. Only the types with no trading
//! equivalent live here.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::broker::enums::{
    BankAccountType, FeePaymentMethod, IdentifierType, TransferDirection, TransferTiming,
    TransferType,
};
use crate::error::{Error, Result};
use crate::trading::OrderType;
use crate::types::SupportedCurrencies;

/// An order submitted on behalf of a brokerage account.
///
/// The trading API's [`crate::trading::OrderRequest`] plus the two fields only a
/// correspondent may set: the commission to charge the end user, and the
/// currency to settle in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub commission: Option<Decimal>,
    /// The settlement currency. Unset means USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    /// Charges `commission` dollars to the end user.
    #[must_use]
    pub fn commission(mut self, commission: Decimal) -> Self {
        self.commission = Some(commission);
        self
    }

    /// Settles the order in a currency other than USD.
    #[must_use]
    pub fn currency(mut self, currency: SupportedCurrencies) -> Self {
        self.currency = Some(currency);
        self
    }

    /// Checks the rules Alpaca enforces on the order before it is sent.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the wrapped order is invalid, or if
    /// a non-USD order is anything other than a market order — local currency
    /// trading supports market orders only.
    pub fn validate(&self) -> Result<()> {
        self.order.validate()?;

        let local_currency = self
            .currency
            .as_ref()
            .is_some_and(|currency| *currency != SupportedCurrencies::Usd);
        if local_currency && self.order.order_type != OrderType::Market {
            return Err(Error::InvalidRequest(
                "orders in a local currency must be market orders".to_owned(),
            ));
        }

        Ok(())
    }
}

/// The body that opens an ACH relationship.
///
/// Alpaca accepts two shapes here: bank details entered by hand, or a Plaid
/// processor token. They are one Rust type rather than two so the client method
/// takes one parameter, which is what alpaca-py's runtime `isinstance` check
/// approximates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CreateACHRelationshipRequest {
    /// Bank details entered by the account holder.
    Manual(ManualACHRelationship),
    /// A processor token from a completed Plaid link.
    Plaid(PlaidACHRelationship),
}

/// Bank details for an ACH relationship opened by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub nickname: Option<String>,
}

/// The processor token from a completed Plaid link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaidACHRelationship {
    /// The Alpaca-specific processor token Plaid returned.
    pub processor_token: String,
}

/// The body that connects a recipient bank for wires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub country: Option<String>,
    /// The bank's state or province. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_province: Option<String>,
    /// The bank's postal code. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    /// The bank's city. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// The bank's street address. International banks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    /// Checks the address rules Alpaca enforces on bank connections.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if a domestic (ABA) bank carries any
    /// address field, or an international (BIC) bank is missing one. alpaca-py
    /// enforces both directions in a model validator.
    pub fn validate(&self) -> Result<()> {
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
            IdentifierType::Bic => {
                for (field, value) in address {
                    if value.is_none() {
                        return Err(Error::InvalidRequest(format!(
                            "{field} is required for international bank accounts"
                        )));
                    }
                }
            }
            // A value Alpaca added that predates this rule; let the API judge it.
            IdentifierType::Unknown(_) => {}
        }

        Ok(())
    }
}

/// Where an international bank is located. Every field is required.
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// alpaca-py has two classes here, one per transfer type, each pinning
/// `transfer_type` with a validator that rejects the other value. An enum makes
/// the same guarantee without a runtime check: an ACH transfer cannot carry a
/// `bank_id`, and a wire cannot carry a `relationship_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CreateTransferRequest {
    /// Money moving over an ACH relationship.
    Ach(CreateACHTransferRequest),
    /// Money moving by wire to a connected bank.
    Wire(CreateBankTransferRequest),
}

impl CreateTransferRequest {
    /// Checks the amount is positive.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the amount is zero or negative.
    pub fn validate(&self) -> Result<()> {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// The body of an option exercise request.
///
/// Both fields of alpaca-py's `CreateOptionExerciseRequest` are optional, and
/// it drops unset ones, so an exercise with no commission posts `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOptionExerciseRequest {
    /// The commission to charge the end user, in dollars.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub commission: Option<Decimal>,
}

impl CreateOptionExerciseRequest {
    /// An exercise request that charges no commission.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Charges `commission` dollars to the end user.
    #[must_use]
    pub fn commission(mut self, commission: Decimal) -> Self {
        self.commission = Some(commission);
        self
    }
}
