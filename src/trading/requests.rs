//! Request types for the trading API.
//!
//! See [orders at Alpaca](https://docs.alpaca.markets/us/docs/orders-at-alpaca)
//! for what the order classes and time-in-force values mean.
//!
//! Each order shape is a constructor rather than one struct with every field
//! optional, so several combinations Alpaca would reject cannot be built at
//! all:
//!
//! - "exactly one of `qty` or `notional`" is [`OrderAmount`]
//! - "exactly one of `trail_price` or `trail_percent`" is [`Trail`]
//! - "`limit_price` is not supported for market orders" is enforced by there
//!   being no way to set it on one
//!
//! What remains — the bracket/OCO/OTO leg requirements and the multi-leg rules —
//! is checked in [`OrderRequest::validate`], which the client calls before
//! sending.
//!
//! Optional fields are skipped when absent rather than sent as null: Alpaca
//! treats an explicit null differently from an omitted key on several routes.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::trading::enums::{
    ActivityCategory, ActivityType, AssetClass, AssetExchange, AssetStatus,
    CorporateActionDateType, CorporateActionType, ExerciseStyle, OrderClass, OrderSide, OrderType,
    PositionIntent, QueryOrderStatus, TimeInForce,
};
use crate::types::{ContractType, Sort};

/// How much of an asset to trade.
///
/// Alpaca accepts a share quantity or a dollar amount, never both, so this is
/// an enum rather than two optional fields and a runtime check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderAmount {
    /// A number of shares. Fractional quantities are supported for stocks with
    /// market orders and for crypto.
    Qty(Decimal),
    /// A dollar amount. Stocks only, and only with market orders.
    Notional(Decimal),
}

/// How far a trailing stop trails the high water mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trail {
    /// A dollar amount away from the high water mark.
    Price(Decimal),
    /// A percentage away from the high water mark.
    Percent(Decimal),
}

/// The profit-taking leg of a bracket, OCO, or OTO order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TakeProfitRequest {
    /// The limit price to exit a profitable trade at.
    #[serde(with = "crate::types::decimal")]
    pub limit_price: Decimal,
}

impl TakeProfitRequest {
    /// A take-profit leg at `limit_price`.
    #[must_use]
    pub fn new(limit_price: Decimal) -> Self {
        Self { limit_price }
    }
}

/// The loss-limiting leg of a bracket, OCO, or OTO order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopLossRequest {
    /// The stop price to exit a losing trade at.
    #[serde(with = "crate::types::decimal")]
    pub stop_price: Decimal,
    /// An optional limit price, making the exit a stop-limit rather than a stop.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub limit_price: Option<Decimal>,
}

impl StopLossRequest {
    /// A stop-loss leg at `stop_price`.
    #[must_use]
    pub fn new(stop_price: Decimal) -> Self {
        Self {
            stop_price,
            limit_price: None,
        }
    }

    /// Adds a limit price, making this a stop-limit exit.
    #[must_use]
    pub fn limit_price(mut self, limit_price: Decimal) -> Self {
        self.limit_price = Some(limit_price);
        self
    }
}

/// One leg of a multi-leg option order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionLegRequest {
    /// The option contract symbol.
    pub symbol: String,
    /// This leg's proportional quantity within the order.
    #[serde(with = "crate::types::decimal")]
    pub ratio_qty: Decimal,
    /// Whether this leg buys or sells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// The desired position strategy for this leg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_intent: Option<PositionIntent>,
}

impl OptionLegRequest {
    /// A leg identified by side.
    #[must_use]
    pub fn new(symbol: impl Into<String>, ratio_qty: Decimal, side: OrderSide) -> Self {
        Self {
            symbol: symbol.into(),
            ratio_qty,
            side: Some(side),
            position_intent: None,
        }
    }

    /// A leg identified by position intent instead of side.
    ///
    /// Alpaca requires a side or a position intent. This constructor and
    /// [`OptionLegRequest::new`] are the only ways to build a leg, so neither
    /// can be missing and there is nothing to check at runtime.
    #[must_use]
    pub fn with_position_intent(
        symbol: impl Into<String>,
        ratio_qty: Decimal,
        position_intent: PositionIntent,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            ratio_qty,
            side: None,
            position_intent: Some(position_intent),
        }
    }

    /// Sets the position intent alongside the side.
    #[must_use]
    pub fn position_intent(mut self, position_intent: PositionIntent) -> Self {
        self.position_intent = Some(position_intent);
        self
    }
}

/// An order to submit.
///
/// Build one with [`OrderRequest::market`], [`OrderRequest::limit`],
/// [`OrderRequest::stop`], [`OrderRequest::stop_limit`],
/// [`OrderRequest::trailing_stop`], or [`OrderRequest::multi_leg`], then chain
/// the optional setters.
///
/// ```
/// # use alpaca_sdk::trading::{OrderAmount, OrderRequest, OrderSide, TimeInForce};
/// # use rust_decimal::Decimal;
/// let order = OrderRequest::market(
///     "AAPL",
///     OrderSide::Buy,
///     OrderAmount::Qty(Decimal::from(1)),
///     TimeInForce::Day,
/// )
/// .client_order_id("my-order-1")
/// .extended_hours(true);
///
/// assert!(order.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRequest {
    /// The symbol being traded. Required for every order class except mleg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Number of shares to trade.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub qty: Option<Decimal>,
    /// Dollar value to trade.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub notional: Option<Decimal>,
    /// Whether the order buys or sells. Required for every class except mleg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// The execution logic of the order.
    #[serde(rename = "type")]
    pub order_type: OrderType,
    /// How long the order stays in force.
    pub time_in_force: TimeInForce,
    /// The order class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_class: Option<OrderClass>,
    /// Whether the order may execute outside regular trading hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extended_hours: Option<bool>,
    /// A caller-supplied identifier for the order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    /// The legs of a multi-leg option order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legs: Option<Vec<OptionLegRequest>>,
    /// The profit-taking exit, for bracket, OCO, and OTO orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<TakeProfitRequest>,
    /// The loss-limiting exit, for bracket, OCO, and OTO orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<StopLossRequest>,
    /// The desired position strategy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_intent: Option<PositionIntent>,
    /// Limit price, for limit and stop-limit orders.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub limit_price: Option<Decimal>,
    /// Stop price, for stop and stop-limit orders.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub stop_price: Option<Decimal>,
    /// Dollar trail, for trailing stop orders.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub trail_price: Option<Decimal>,
    /// Percentage trail, for trailing stop orders.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub trail_percent: Option<Decimal>,
}

impl OrderRequest {
    fn base(order_type: OrderType, time_in_force: TimeInForce) -> Self {
        Self {
            symbol: None,
            qty: None,
            notional: None,
            side: None,
            order_type,
            time_in_force,
            order_class: None,
            extended_hours: None,
            client_order_id: None,
            legs: None,
            take_profit: None,
            stop_loss: None,
            position_intent: None,
            limit_price: None,
            stop_price: None,
            trail_price: None,
            trail_percent: None,
        }
    }

    fn single_leg(
        order_type: OrderType,
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
    ) -> Self {
        let mut request = Self::base(order_type, time_in_force);
        request.symbol = Some(symbol.into());
        request.side = Some(side);
        match amount {
            OrderAmount::Qty(qty) => request.qty = Some(qty),
            OrderAmount::Notional(notional) => request.notional = Some(notional),
        }
        request
    }

    /// A market order.
    #[must_use]
    pub fn market(
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
    ) -> Self {
        Self::single_leg(OrderType::Market, symbol, side, amount, time_in_force)
    }

    /// A limit order.
    #[must_use]
    pub fn limit(
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
        limit_price: Decimal,
    ) -> Self {
        let mut request = Self::single_leg(OrderType::Limit, symbol, side, amount, time_in_force);
        request.limit_price = Some(limit_price);
        request
    }

    /// A stop order.
    #[must_use]
    pub fn stop(
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
        stop_price: Decimal,
    ) -> Self {
        let mut request = Self::single_leg(OrderType::Stop, symbol, side, amount, time_in_force);
        request.stop_price = Some(stop_price);
        request
    }

    /// A stop-limit order.
    #[must_use]
    pub fn stop_limit(
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
        stop_price: Decimal,
        limit_price: Decimal,
    ) -> Self {
        let mut request =
            Self::single_leg(OrderType::StopLimit, symbol, side, amount, time_in_force);
        request.stop_price = Some(stop_price);
        request.limit_price = Some(limit_price);
        request
    }

    /// A trailing stop order.
    #[must_use]
    pub fn trailing_stop(
        symbol: impl Into<String>,
        side: OrderSide,
        amount: OrderAmount,
        time_in_force: TimeInForce,
        trail: Trail,
    ) -> Self {
        let mut request =
            Self::single_leg(OrderType::TrailingStop, symbol, side, amount, time_in_force);
        match trail {
            Trail::Price(price) => request.trail_price = Some(price),
            Trail::Percent(percent) => request.trail_percent = Some(percent),
        }
        request
    }

    /// A multi-leg option order.
    ///
    /// Only market and limit orders are supported for this class; pass
    /// `limit_price` to make it a limit order.
    #[must_use]
    pub fn multi_leg(
        qty: Decimal,
        time_in_force: TimeInForce,
        legs: Vec<OptionLegRequest>,
        limit_price: Option<Decimal>,
    ) -> Self {
        let order_type = if limit_price.is_some() {
            OrderType::Limit
        } else {
            OrderType::Market
        };
        let mut request = Self::base(order_type, time_in_force);
        request.order_class = Some(OrderClass::Mleg);
        request.qty = Some(qty);
        request.legs = Some(legs);
        request.limit_price = limit_price;
        request
    }

    /// Makes this a bracket order with both exits.
    #[must_use]
    pub fn bracket(mut self, take_profit: TakeProfitRequest, stop_loss: StopLossRequest) -> Self {
        self.order_class = Some(OrderClass::Bracket);
        self.take_profit = Some(take_profit);
        self.stop_loss = Some(stop_loss);
        self
    }

    /// Makes this a one-cancels-other order with both exits.
    #[must_use]
    pub fn oco(mut self, take_profit: TakeProfitRequest, stop_loss: StopLossRequest) -> Self {
        self.order_class = Some(OrderClass::Oco);
        self.take_profit = Some(take_profit);
        self.stop_loss = Some(stop_loss);
        self
    }

    /// Makes this a one-triggers-other order with a take-profit exit.
    #[must_use]
    pub fn oto_take_profit(mut self, take_profit: TakeProfitRequest) -> Self {
        self.order_class = Some(OrderClass::Oto);
        self.take_profit = Some(take_profit);
        self
    }

    /// Makes this a one-triggers-other order with a stop-loss exit.
    #[must_use]
    pub fn oto_stop_loss(mut self, stop_loss: StopLossRequest) -> Self {
        self.order_class = Some(OrderClass::Oto);
        self.stop_loss = Some(stop_loss);
        self
    }

    /// Sets whether the order may execute outside regular trading hours.
    #[must_use]
    pub fn extended_hours(mut self, extended_hours: bool) -> Self {
        self.extended_hours = Some(extended_hours);
        self
    }

    /// Sets a caller-supplied identifier for the order.
    #[must_use]
    pub fn client_order_id(mut self, client_order_id: impl Into<String>) -> Self {
        self.client_order_id = Some(client_order_id.into());
        self
    }

    /// Sets the desired position strategy.
    #[must_use]
    pub fn position_intent(mut self, position_intent: PositionIntent) -> Self {
        self.position_intent = Some(position_intent);
        self
    }

    /// Checks the combinations Alpaca rejects, before the request is sent.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] when a bracket or OCO order is missing
    /// an exit, an OTO order has neither, or the multi-leg rules are violated:
    /// wrong order type, missing or duplicate legs, or a leg count outside 2 to 4.
    pub fn validate(&self) -> Result<()> {
        let invalid = |reason: String| Err(Error::InvalidRequest(reason));

        match self.order_class {
            Some(OrderClass::Bracket | OrderClass::Oco) => {
                let class = self.order_class.as_ref().map_or("", OrderClass::as_str);
                if self.take_profit.is_none() {
                    return invalid(format!("{class} orders require take_profit.limit_price"));
                }
                if self.stop_loss.is_none() {
                    return invalid(format!("{class} orders require stop_loss.stop_price"));
                }
            }
            Some(OrderClass::Oto) if self.take_profit.is_none() && self.stop_loss.is_none() => {
                return invalid("oto orders require either take_profit or stop_loss".to_owned());
            }
            _ => {}
        }

        if self.order_class == Some(OrderClass::Mleg) {
            if !matches!(self.order_type, OrderType::Market | OrderType::Limit) {
                return invalid(
                    "mleg order class only supports market and limit orders".to_owned(),
                );
            }
            if self.qty.is_none() {
                return invalid("qty is required for the mleg order class".to_owned());
            }

            let legs = self.legs.as_deref().unwrap_or_default();
            if legs.is_empty() {
                return invalid("legs is required for the mleg order class".to_owned());
            }
            if legs.len() > 4 {
                return invalid("at most 4 legs are allowed for the mleg order class".to_owned());
            }
            if legs.len() < 2 {
                return invalid("at least 2 legs are required for the mleg order class".to_owned());
            }

            let mut symbols: Vec<&str> = legs.iter().map(|leg| leg.symbol.as_str()).collect();
            symbols.sort_unstable();
            symbols.dedup();
            if symbols.len() != legs.len() {
                return invalid("all legs must have unique symbols".to_owned());
            }
        } else {
            if self.symbol.is_none() {
                return invalid(
                    "symbol is required for all order classes other than mleg".to_owned(),
                );
            }
            if self.side.is_none() {
                return invalid(
                    "side is required for all order classes other than mleg".to_owned(),
                );
            }
        }

        Ok(())
    }
}

/// Changes to apply to an existing order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceOrderRequest {
    /// The new number of shares.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub qty: Option<Decimal>,
    /// The new expiration logic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    /// The new limit price. Required when replacing a limit or stop-limit order.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub limit_price: Option<Decimal>,
    /// The new stop price. Required when replacing a stop or stop-limit order.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub stop_price: Option<Decimal>,
    /// The new trail value, for trailing stop orders.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub trail: Option<Decimal>,
    /// A new caller-supplied identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
}

impl ReplaceOrderRequest {
    /// An empty replacement, changing nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the new quantity.
    #[must_use]
    pub fn qty(mut self, qty: Decimal) -> Self {
        self.qty = Some(qty);
        self
    }

    /// Sets the new time in force.
    #[must_use]
    pub fn time_in_force(mut self, time_in_force: TimeInForce) -> Self {
        self.time_in_force = Some(time_in_force);
        self
    }

    /// Sets the new limit price.
    #[must_use]
    pub fn limit_price(mut self, limit_price: Decimal) -> Self {
        self.limit_price = Some(limit_price);
        self
    }

    /// Sets the new stop price.
    #[must_use]
    pub fn stop_price(mut self, stop_price: Decimal) -> Self {
        self.stop_price = Some(stop_price);
        self
    }

    /// Sets the new trail value.
    #[must_use]
    pub fn trail(mut self, trail: Decimal) -> Self {
        self.trail = Some(trail);
        self
    }

    /// Sets the new client order id.
    #[must_use]
    pub fn client_order_id(mut self, client_order_id: impl Into<String>) -> Self {
        self.client_order_id = Some(client_order_id.into());
        self
    }

    /// Checks that the supplied values are positive.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if `qty`, `stop_price`, or `trail` is
    /// zero or negative.
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("qty", self.qty),
            ("stop_price", self.stop_price),
            ("trail", self.trail),
        ] {
            if let Some(value) = value
                && value <= Decimal::ZERO
            {
                return Err(Error::InvalidRequest(format!(
                    "{name} must be greater than 0"
                )));
            }
        }
        Ok(())
    }
}

/// How much of a position to close.
///
/// A quantity or a percentage, never both and never neither — an enum rather
/// than two optional fields and a runtime check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosePositionRequest {
    /// Close this number of shares.
    Qty(Decimal),
    /// Close this percentage of the position, between 0 and 100.
    Percentage(Decimal),
}

impl ClosePositionRequest {
    /// The query parameters for this request.
    #[must_use]
    pub fn to_query(self) -> Vec<(&'static str, String)> {
        match self {
            Self::Qty(qty) => vec![("qty", qty.to_string())],
            Self::Percentage(percentage) => vec![("percentage", percentage.to_string())],
        }
    }
}

/// Filters for listing orders.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrdersRequest {
    /// Which orders to return: open, closed, or all. Defaults to open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<QueryOrderStatus>,
    /// Maximum number of orders to return. Defaults to 50, maximum 500.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Only orders submitted after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Only orders submitted until this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Sort>,
    /// Whether to roll multi-leg orders up under their parent's `legs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nested: Option<bool>,
    /// Only orders on this side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<OrderSide>,
    /// Only orders for these symbols.
    ///
    /// Sent as one comma-separated parameter, which is what Alpaca expects.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    pub symbols: Option<Vec<String>>,
    /// Only orders in these asset classes.
    ///
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    pub asset_class: Option<Vec<AssetClass>>,
    /// Only orders placed before this one.
    ///
    /// The id-based cursor, which is steadier than
    /// [`until`](Self::until) when several orders share a timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_order_id: Option<Uuid>,
    /// Only orders placed after this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_order_id: Option<Uuid>,
    /// Only orders for more than this quantity.
    ///
    /// **Documented on the broker route only** —
    /// `GET /v1/trading/accounts/{account_id}/orders`. The trading API's own
    /// reference page does not list it, so sending it to
    /// [`TradingClient::get_orders`](crate::trading::TradingClient::get_orders)
    /// is asking for undefined behaviour rather than a documented filter. It
    /// lives here because the broker route takes this same request type.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub qty_above: Option<Decimal>,
    /// Only orders for less than this quantity. Broker route only, like
    /// [`qty_above`](Self::qty_above).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub qty_below: Option<Decimal>,
    /// Only orders carrying this subtag. Broker route only, like
    /// [`qty_above`](Self::qty_above).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtag: Option<String>,
}

/// Filters for listing account activities.
///
/// The broker API documents the same filters plus an `account_id`, and carries
/// [its own copy](crate::broker::GetAccountActivitiesRequest) for that reason.
/// The two cannot share a struct the way `broker::Order` shares
/// `trading::Order`: that works by `#[serde(flatten)]`, and a flattened struct
/// cannot be serialized into a query string — `serde_urlencoded` rejects it at
/// runtime, with no compile error to warn anyone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountActivitiesRequest {
    /// Only activities of these kinds.
    ///
    /// Sent as one comma-separated parameter.
    ///
    /// Not accepted by
    /// [`get_account_activities_by_type`](crate::trading::TradingClient::get_account_activities_by_type),
    /// where the one type being asked for is in the path instead.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
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
    pub category: Option<ActivityCategory>,
    /// Only activities belonging to one order.
    ///
    /// The way to fetch the fills that made up a completed order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Uuid>,
    /// Only activities created on this date.
    ///
    /// The reference is specific that this is `created_at` and not the
    /// settlement date: a fee for a Monday trade is typically created on the
    /// Tuesday, in UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    /// Only activities created before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Only activities created after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Which way to sort. Defaults to descending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Sort>,
    /// How many activities to return per page. Defaults to 100, and capped
    /// there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// Where to resume from: the `id` of the last activity already seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetAccountActivitiesRequest {
    /// Rejects the one combination the reference forbids.
    ///
    /// `category` and `activity_types` cannot be sent together: "Cannot be used
    /// with `activity_types` parameter". That is a documented rule, so it is
    /// enforced — the same rule, and the same enforcement, as the broker's copy.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if both are set.
    pub fn validate(&self) -> Result<()> {
        if self.category.is_some() && self.activity_types.is_some() {
            return Err(Error::InvalidRequest(
                "activity_types and category cannot be combined".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for fetching one order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrderByIdRequest {
    /// Whether to roll multi-leg orders up under their parent's `legs`.
    pub nested: bool,
}

/// Filters for listing assets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAssetsRequest {
    /// Only assets with this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AssetStatus>,
    /// Only assets in this class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_class: Option<AssetClass>,
    /// Only assets on this exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<AssetExchange>,
    /// Comma-separated attributes to filter on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<String>,
}

/// Filters for the market calendar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCalendarRequest {
    /// The first day to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// The last day to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
}

/// Filters for portfolio history.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPortfolioHistoryRequest {
    /// Duration of the data, as a number and unit such as `1D`, `1W`, `1M`, `1A`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Resolution of each window: `1Min`, `5Min`, `15Min`, `1H`, or `1D`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<String>,
    /// How intraday data is reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intraday_reporting: Option<String>,
    /// Start of the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// How profit and loss is reset between windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pnl_reset: Option<String>,
    /// End of the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// End date of the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_end: Option<NaiveDate>,
    /// Whether to include extended hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extended_hours: Option<bool>,
    /// Comma-separated cash flow activity types to include.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cashflow_types: Option<String>,
}

/// A new watchlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWatchlistRequest {
    /// The watchlist name, up to 64 characters.
    pub name: String,
    /// The symbols to track.
    pub symbols: Vec<String>,
}

impl CreateWatchlistRequest {
    /// A watchlist named `name` tracking `symbols`.
    #[must_use]
    pub fn new(name: impl Into<String>, symbols: Vec<String>) -> Self {
        Self {
            name: name.into(),
            symbols,
        }
    }
}

/// Changes to apply to a watchlist.
///
/// At least one field must be set; [`UpdateWatchlistRequest::validate`] checks
/// it, because a `PATCH` with an empty body changes nothing and Alpaca's answer
/// to one is not documented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateWatchlistRequest {
    /// A new name for the watchlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A new set of symbols, replacing the existing ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<String>>,
}

impl UpdateWatchlistRequest {
    /// An empty update, changing nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a new name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets a new symbol list.
    #[must_use]
    pub fn symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = Some(symbols);
        self
    }

    /// Checks that at least one field is set.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if neither `name` nor `symbols` is set.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_none() && self.symbols.is_none() {
            return Err(Error::InvalidRequest(
                "one of name or symbols must be defined".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for corporate action announcements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCorporateAnnouncementsRequest {
    /// The action types to return. Alpaca allows at most 20.
    pub ca_types: Vec<CorporateActionType>,
    /// The earliest date to return, at most 90 days before `until`.
    pub since: NaiveDate,
    /// The latest date to return.
    pub until: NaiveDate,
    /// Only announcements for this symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Only announcements for this CUSIP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cusip: Option<String>,
    /// Which date field `since` and `until` filter on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_type: Option<CorporateActionDateType>,
}

impl GetCorporateAnnouncementsRequest {
    /// Announcements of `ca_types` between `since` and `until`.
    #[must_use]
    pub fn new(ca_types: Vec<CorporateActionType>, since: NaiveDate, until: NaiveDate) -> Self {
        Self {
            ca_types,
            since,
            until,
            symbol: None,
            cusip: None,
            date_type: None,
        }
    }

    /// Only announcements for this symbol.
    #[must_use]
    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Only announcements for this CUSIP.
    #[must_use]
    pub fn cusip(mut self, cusip: impl Into<String>) -> Self {
        self.cusip = Some(cusip.into());
        self
    }

    /// Which date field the window filters on.
    #[must_use]
    pub fn date_type(mut self, date_type: CorporateActionDateType) -> Self {
        self.date_type = Some(date_type);
        self
    }

    /// The query parameters for this request.
    ///
    /// `ca_types` is emitted once per value — `?ca_types=x&ca_types=y` — rather
    /// than comma-separated, which is what this route expects. It cannot go
    /// through the normal query serializer at all: `serde_urlencoded` has no
    /// representation for a sequence and fails the whole request.
    #[must_use]
    pub fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = self
            .ca_types
            .iter()
            .map(|ca_type| ("ca_types", ca_type.to_string()))
            .collect();

        query.push(("since", self.since.to_string()));
        query.push(("until", self.until.to_string()));

        if let Some(symbol) = &self.symbol {
            query.push(("symbol", symbol.clone()));
        }
        if let Some(cusip) = &self.cusip {
            query.push(("cusip", cusip.clone()));
        }
        if let Some(date_type) = &self.date_type {
            query.push(("date_type", date_type.to_string()));
        }

        query
    }

    /// Checks the range Alpaca accepts.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the window exceeds 90 days.
    pub fn validate(&self) -> Result<()> {
        if (self.until - self.since).num_days() > 90 {
            return Err(Error::InvalidRequest(
                "the date range between since and until must be no more than 90 days".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for listing option contracts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOptionContractsRequest {
    /// Only contracts on these underlying symbols.
    ///
    /// Sent as one comma-separated parameter, which is what Alpaca expects.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::types::serde_util::comma_separated"
    )]
    pub underlying_symbols: Option<Vec<String>>,
    /// Only contracts with this status.
    ///
    /// Leaving it unset is not the same as asking for everything: the reference
    /// says "by default only active contracts are returned".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AssetStatus>,
    /// Only contracts expiring on this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<NaiveDate>,
    /// Only contracts expiring on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date_gte: Option<NaiveDate>,
    /// Only contracts expiring on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date_lte: Option<NaiveDate>,
    /// Only contracts with this root symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_symbol: Option<String>,
    /// Only calls or only puts.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub contract_type: Option<ContractType>,
    /// Only contracts with this exercise style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ExerciseStyle>,
    /// Whether to include each contract's deliverables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_deliverables: Option<bool>,
    /// Only contracts in — or only contracts outside — the Penny Program.
    ///
    /// The Penny Program Indicator: `true` selects contracts eligible for penny
    /// price increments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppind: Option<bool>,
    /// Only contracts struck at or above this price.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub strike_price_gte: Option<Decimal>,
    /// Only contracts struck at or below this price.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::types::option_decimal"
    )]
    pub strike_price_lte: Option<Decimal>,
    /// Maximum number of contracts to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Token for fetching the next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl GetOptionContractsRequest {
    /// Contracts on `underlying_symbols`, asking for active ones explicitly.
    ///
    /// That matches what the route does when `status` is unset, spelled out
    /// rather than relied upon.
    #[must_use]
    pub fn new(underlying_symbols: Vec<String>) -> Self {
        Self {
            underlying_symbols: Some(underlying_symbols),
            status: Some(AssetStatus::Active),
            ..Self::default()
        }
    }
}

/// The outcome of cancelling one order in a bulk cancel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelOrderResponse {
    /// Id of the order.
    pub id: Uuid,
    /// Status code for this order's cancellation.
    #[serde(with = "crate::types::serde_util::int")]
    pub status: i64,
    /// Any additional detail returned for the cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qty(value: i64) -> OrderAmount {
        OrderAmount::Qty(Decimal::from(value))
    }

    fn market() -> OrderRequest {
        OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
    }

    #[test]
    fn market_order_serializes_only_the_fields_that_are_set() {
        // An unset field must be absent from the body, not sent as null:
        // Alpaca distinguishes the two on several routes.
        let json = serde_json::to_value(market()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "symbol": "AAPL",
                "qty": "1",
                "side": "buy",
                "type": "market",
                "time_in_force": "day",
            })
        );
    }

    #[test]
    fn notional_orders_send_notional_instead_of_qty() {
        let order = OrderRequest::market(
            "AAPL",
            OrderSide::Buy,
            OrderAmount::Notional(Decimal::new(15_050, 2)),
            TimeInForce::Day,
        );
        let json = serde_json::to_value(&order).unwrap();

        assert_eq!(json["notional"], "150.50");
        assert!(json.get("qty").is_none());
    }

    #[test]
    fn limit_order_carries_its_price() {
        let order = OrderRequest::limit(
            "AAPL",
            OrderSide::Buy,
            qty(1),
            TimeInForce::Day,
            Decimal::new(1835, 1),
        );

        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(
            serde_json::to_value(&order).unwrap()["limit_price"],
            "183.5"
        );
    }

    #[test]
    fn trailing_stop_sends_exactly_one_trail_field() {
        let by_price = OrderRequest::trailing_stop(
            "AAPL",
            OrderSide::Sell,
            qty(1),
            TimeInForce::Day,
            Trail::Price(Decimal::from(5)),
        );
        let by_percent = OrderRequest::trailing_stop(
            "AAPL",
            OrderSide::Sell,
            qty(1),
            TimeInForce::Day,
            Trail::Percent(Decimal::from(2)),
        );

        let price_json = serde_json::to_value(&by_price).unwrap();
        assert_eq!(price_json["trail_price"], "5");
        assert!(price_json.get("trail_percent").is_none());

        let percent_json = serde_json::to_value(&by_percent).unwrap();
        assert_eq!(percent_json["trail_percent"], "2");
        assert!(percent_json.get("trail_price").is_none());
    }

    #[test]
    fn bracket_orders_require_both_exits() {
        let take_profit = TakeProfitRequest::new(Decimal::from(200));
        let stop_loss = StopLossRequest::new(Decimal::from(150));

        assert!(
            market()
                .bracket(take_profit.clone(), stop_loss)
                .validate()
                .is_ok()
        );

        // Constructed by hand, since `bracket` sets both.
        let mut missing_stop = market();
        missing_stop.order_class = Some(OrderClass::Bracket);
        missing_stop.take_profit = Some(take_profit);
        assert!(missing_stop.validate().is_err());
    }

    #[test]
    fn oto_orders_require_at_least_one_exit() {
        assert!(
            market()
                .oto_take_profit(TakeProfitRequest::new(Decimal::from(200)))
                .validate()
                .is_ok()
        );
        assert!(
            market()
                .oto_stop_loss(StopLossRequest::new(Decimal::from(150)))
                .validate()
                .is_ok()
        );

        let mut neither = market();
        neither.order_class = Some(OrderClass::Oto);
        assert!(neither.validate().is_err());
    }

    #[test]
    fn multi_leg_orders_need_between_two_and_four_unique_legs() {
        let leg = |symbol: &str| OptionLegRequest::new(symbol, Decimal::from(1), OrderSide::Buy);

        let valid = OrderRequest::multi_leg(
            Decimal::from(1),
            TimeInForce::Day,
            vec![leg("AAPL240119C00150000"), leg("AAPL240119P00150000")],
            None,
        );
        assert!(valid.validate().is_ok());

        let too_few =
            OrderRequest::multi_leg(Decimal::from(1), TimeInForce::Day, vec![leg("A")], None);
        assert!(too_few.validate().is_err());

        let too_many = OrderRequest::multi_leg(
            Decimal::from(1),
            TimeInForce::Day,
            vec![leg("A"), leg("B"), leg("C"), leg("D"), leg("E")],
            None,
        );
        assert!(too_many.validate().is_err());

        let duplicate_symbols = OrderRequest::multi_leg(
            Decimal::from(1),
            TimeInForce::Day,
            vec![leg("A"), leg("A")],
            None,
        );
        assert!(duplicate_symbols.validate().is_err());
    }

    #[test]
    fn multi_leg_orders_reject_unsupported_order_types() {
        let mut order = OrderRequest::multi_leg(
            Decimal::from(1),
            TimeInForce::Day,
            vec![
                OptionLegRequest::new("A", Decimal::from(1), OrderSide::Buy),
                OptionLegRequest::new("B", Decimal::from(1), OrderSide::Sell),
            ],
            None,
        );
        order.order_type = OrderType::Stop;

        assert!(order.validate().is_err());
    }

    #[test]
    fn multi_leg_orders_omit_the_top_level_symbol_and_side() {
        let order = OrderRequest::multi_leg(
            Decimal::from(1),
            TimeInForce::Day,
            vec![
                OptionLegRequest::new("A", Decimal::from(1), OrderSide::Buy),
                OptionLegRequest::new("B", Decimal::from(1), OrderSide::Sell),
            ],
            None,
        );
        let json = serde_json::to_value(&order).unwrap();

        assert!(json.get("symbol").is_none());
        assert!(json.get("side").is_none());
        assert_eq!(json["order_class"], "mleg");
        assert_eq!(json["legs"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn single_leg_orders_require_a_symbol_and_side() {
        let mut order = market();
        order.symbol = None;
        assert!(order.validate().is_err());

        let mut order = market();
        order.side = None;
        assert!(order.validate().is_err());
    }

    #[test]
    fn replace_order_rejects_non_positive_values() {
        assert!(
            ReplaceOrderRequest::new()
                .qty(Decimal::from(1))
                .validate()
                .is_ok()
        );
        assert!(
            ReplaceOrderRequest::new()
                .qty(Decimal::ZERO)
                .validate()
                .is_err()
        );
        assert!(
            ReplaceOrderRequest::new()
                .trail(Decimal::from(-1))
                .validate()
                .is_err()
        );
    }

    #[test]
    fn watchlist_update_requires_at_least_one_field() {
        assert!(UpdateWatchlistRequest::new().validate().is_err());
        assert!(UpdateWatchlistRequest::new().name("x").validate().is_ok());
        assert!(
            UpdateWatchlistRequest::new()
                .symbols(vec!["AAPL".to_owned()])
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn close_position_renders_one_query_parameter() {
        assert_eq!(
            ClosePositionRequest::Qty(Decimal::new(15, 1)).to_query(),
            vec![("qty", "1.5".to_owned())]
        );
        assert_eq!(
            ClosePositionRequest::Percentage(Decimal::from(50)).to_query(),
            vec![("percentage", "50".to_owned())]
        );
    }

    #[test]
    fn corporate_announcement_window_is_capped_at_ninety_days() {
        let since = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();

        let ok = GetCorporateAnnouncementsRequest::new(
            vec![CorporateActionType::Dividend],
            since,
            NaiveDate::from_ymd_opt(2022, 3, 1).unwrap(),
        );
        assert!(ok.validate().is_ok());

        let too_wide = GetCorporateAnnouncementsRequest::new(
            vec![CorporateActionType::Dividend],
            since,
            NaiveDate::from_ymd_opt(2022, 12, 1).unwrap(),
        );
        assert!(too_wide.validate().is_err());
    }

    #[test]
    fn option_contracts_request_defaults_to_active() {
        let request = GetOptionContractsRequest::new(vec!["AAPL".to_owned()]);
        assert_eq!(request.status, Some(AssetStatus::Active));
    }
}
