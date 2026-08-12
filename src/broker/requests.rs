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
