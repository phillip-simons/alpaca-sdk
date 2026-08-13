//! Response models for the trading API.
//!
//! Field-for-field with what the API sends, with two systematic changes: money
//! that arrives as a string is [`Decimal`] rather than `str`, and integers
//! Alpaca sends inconsistently go through [`serde_util::int`].
//!
//! Unknown fields are ignored rather than rejected. Alpaca adds fields without
//! warning — `Asset` grew `last_price` and `last_close_pct_change`, orders carry
//! a `commission` on the broker API — and failing on an unrecognised key would
//! break every caller each time the API grows.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::HashMap;
use uuid::Uuid;

use crate::trading::enums::{
    AccountStatus, ActivityType, AssetClass, AssetExchange, AssetStatus, CorporateActionSubType,
    CorporateActionType, DTBPCheck, ExerciseStyle, NonTradeActivityStatus, OrderClass, OrderSide,
    OrderStatus, OrderType, PDTCheck, PositionIntent, PositionSide, TimeInForce, TradeActivityType,
    TradeConfirmationEmail, TradeEvent,
};
use crate::types::ContractType;
use crate::types::serde_util::{self, empty_string_as_none};

/// A tradable security.
///
/// Some assets are not tradable with Alpaca; those carry `tradable = false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Alpaca's unique id for the asset.
    pub id: Uuid,
    /// The asset class.
    ///
    /// Sent on the wire as `class`, hence the rename.
    #[serde(rename = "class")]
    pub asset_class: AssetClass,
    /// The exchange the asset trades on.
    pub exchange: AssetExchange,
    /// The ticker symbol.
    pub symbol: String,
    /// The asset's full name.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the asset is active.
    pub status: AssetStatus,
    /// Whether the asset can be traded.
    pub tradable: bool,
    /// Whether the asset can be traded on margin.
    pub marginable: bool,
    /// Whether the asset can be shorted.
    pub shortable: bool,
    /// Whether the asset is easy to borrow when shorting.
    pub easy_to_borrow: bool,
    /// Whether fractional shares are available.
    pub fractionable: bool,
    /// The minimum order size, for crypto.
    #[serde(default, with = "crate::types::option_decimal")]
    pub min_order_size: Option<Decimal>,
    /// The minimum trade increment, for crypto.
    #[serde(default, with = "crate::types::option_decimal")]
    pub min_trade_increment: Option<Decimal>,
    /// The price increment, for crypto.
    #[serde(default, with = "crate::types::option_decimal")]
    pub price_increment: Option<Decimal>,
    /// The maintenance margin requirement, as a percentage.
    #[serde(default, with = "crate::types::option_decimal")]
    pub maintenance_margin_requirement: Option<Decimal>,
    /// Unique characteristics of the asset, such as `ptp_no_exception`.
    #[serde(default)]
    pub attributes: Option<Vec<String>>,
}

/// A position's values expressed in USD, for local currency trading accounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsdPositionValues {
    /// The average entry price of the position.
    #[serde(with = "crate::types::decimal")]
    pub avg_entry_price: Decimal,
    /// Total dollar amount of the position.
    #[serde(with = "crate::types::decimal")]
    pub market_value: Decimal,
    /// Total cost basis in dollars.
    #[serde(with = "crate::types::decimal")]
    pub cost_basis: Decimal,
    /// Unrealized profit or loss in dollars.
    #[serde(with = "crate::types::decimal")]
    pub unrealized_pl: Decimal,
    /// Unrealized profit or loss as a fraction.
    #[serde(with = "crate::types::decimal")]
    pub unrealized_plpc: Decimal,
    /// Unrealized intraday profit or loss in dollars.
    #[serde(with = "crate::types::decimal")]
    pub unrealized_intraday_pl: Decimal,
    /// Unrealized intraday profit or loss as a fraction.
    #[serde(with = "crate::types::decimal")]
    pub unrealized_intraday_plpc: Decimal,
    /// The current price per share.
    #[serde(with = "crate::types::decimal")]
    pub current_price: Decimal,
    /// The previous trading day's closing price per share.
    #[serde(with = "crate::types::decimal")]
    pub lastday_price: Decimal,
    /// Fractional change from the previous day's price.
    #[serde(with = "crate::types::decimal")]
    pub change_today: Decimal,
}

/// An open long or short holding in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Id of the asset held.
    pub asset_id: Uuid,
    /// Ticker symbol of the asset held.
    pub symbol: String,
    /// Exchange the asset trades on.
    pub exchange: AssetExchange,
    /// Asset class of the asset held.
    pub asset_class: AssetClass,
    /// Whether the asset is marginable.
    #[serde(default)]
    pub asset_marginable: Option<bool>,
    /// The average price at which the position was entered.
    #[serde(with = "crate::types::decimal")]
    pub avg_entry_price: Decimal,
    /// The number of shares held.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// Whether the position is long or short.
    pub side: PositionSide,
    /// Total dollar amount of the position.
    #[serde(default, with = "crate::types::option_decimal")]
    pub market_value: Option<Decimal>,
    /// Total cost basis in dollars.
    #[serde(with = "crate::types::decimal")]
    pub cost_basis: Decimal,
    /// Unrealized profit or loss in dollars.
    #[serde(default, with = "crate::types::option_decimal")]
    pub unrealized_pl: Option<Decimal>,
    /// Unrealized profit or loss as a fraction.
    #[serde(default, with = "crate::types::option_decimal")]
    pub unrealized_plpc: Option<Decimal>,
    /// Unrealized intraday profit or loss in dollars.
    #[serde(default, with = "crate::types::option_decimal")]
    pub unrealized_intraday_pl: Option<Decimal>,
    /// Unrealized intraday profit or loss as a fraction.
    #[serde(default, with = "crate::types::option_decimal")]
    pub unrealized_intraday_plpc: Option<Decimal>,
    /// The current price per share.
    #[serde(default, with = "crate::types::option_decimal")]
    pub current_price: Option<Decimal>,
    /// The previous trading day's closing price per share.
    #[serde(default, with = "crate::types::option_decimal")]
    pub lastday_price: Option<Decimal>,
    /// Fractional change from the previous day's price.
    #[serde(default, with = "crate::types::option_decimal")]
    pub change_today: Option<Decimal>,
    /// Exchange rate used to convert the price into the local currency.
    #[serde(default, with = "crate::types::option_decimal")]
    pub swap_rate: Option<Decimal>,
    /// Exchange rate at which the entry price was converted.
    #[serde(default, with = "crate::types::option_decimal")]
    pub avg_entry_swap_rate: Option<Decimal>,
    /// The same values expressed in USD, for local currency trading.
    #[serde(default)]
    pub usd: Option<UsdPositionValues>,
    /// Shares available after subtracting open orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub qty_available: Option<Decimal>,
}

/// Every account's positions as of the last market close, keyed by account id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllAccountsPositions {
    /// When the positions were captured.
    pub as_of: DateTime<Utc>,
    /// Positions held, keyed by account id.
    pub positions: HashMap<String, Vec<Position>>,
}

/// A request to buy or sell an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Alpaca's id for the order.
    pub id: Uuid,
    /// The client-supplied unique id.
    pub client_order_id: String,
    /// When the order was created.
    pub created_at: DateTime<Utc>,
    /// When the order was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the order was submitted.
    pub submitted_at: DateTime<Utc>,
    /// When the order was filled.
    #[serde(default)]
    pub filled_at: Option<DateTime<Utc>>,
    /// When the order expired.
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    /// When an auto-cancel will be triggered.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// When the order was canceled.
    #[serde(default)]
    pub canceled_at: Option<DateTime<Utc>>,
    /// When the order failed.
    #[serde(default)]
    pub failed_at: Option<DateTime<Utc>>,
    /// When the order was replaced.
    #[serde(default)]
    pub replaced_at: Option<DateTime<Utc>>,
    /// Id of the order that replaced this one.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub replaced_by: Option<Uuid>,
    /// Id of the order this one replaces.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub replaces: Option<Uuid>,
    /// Id of the asset. Absent at the top level of a multi-leg order.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub asset_id: Option<Uuid>,
    /// Ticker symbol. Absent at the top level of a multi-leg order.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub symbol: Option<String>,
    /// Asset class. Absent at the top level of a multi-leg order.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub asset_class: Option<AssetClass>,
    /// Ordered notional amount, mutually exclusive with `qty`.
    #[serde(default, with = "crate::types::option_decimal")]
    pub notional: Option<Decimal>,
    /// Ordered quantity, mutually exclusive with `notional`.
    #[serde(default, with = "crate::types::option_decimal")]
    pub qty: Option<Decimal>,
    /// Quantity filled so far.
    #[serde(default, with = "crate::types::option_decimal")]
    pub filled_qty: Option<Decimal>,
    /// Average fill price. May be zero until the order is processed.
    #[serde(default, with = "crate::types::option_decimal")]
    pub filled_avg_price: Option<Decimal>,
    /// The order class.
    ///
    /// Alpaca omits this or sends `""` on some responses, and both mean
    /// [`OrderClass::Simple`] — the schema's own description says
    /// `simple (or "")`.
    #[serde(default = "order_class_default", deserialize_with = "order_class")]
    pub order_class: OrderClass,
    /// Deprecated alias for [`Order::order_type`].
    #[serde(
        rename = "order_type",
        default,
        deserialize_with = "empty_string_as_none"
    )]
    pub order_type_deprecated: Option<OrderType>,
    /// The order type. Absent from the legs of a multi-leg order.
    #[serde(rename = "type", default, deserialize_with = "empty_string_as_none")]
    pub order_type: Option<OrderType>,
    /// Buy or sell. Absent at the top level of a multi-leg order.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub side: Option<OrderSide>,
    /// How long the order stays in force.
    pub time_in_force: TimeInForce,
    /// Limit price, for limit and stop-limit orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub limit_price: Option<Decimal>,
    /// Stop price, for stop and stop-limit orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub stop_price: Option<Decimal>,
    /// Current status of the order.
    pub status: OrderStatus,
    /// Whether the order is eligible to execute outside regular hours.
    pub extended_hours: bool,
    /// Child orders, when querying a non-simple order class in nested style.
    #[serde(default)]
    pub legs: Option<Vec<Order>>,
    /// Percent away from the high water mark, for trailing stop orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub trail_percent: Option<Decimal>,
    /// Dollar amount away from the high water mark, for trailing stop orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub trail_price: Option<Decimal>,
    /// Highest or lowest price seen since a trailing stop was submitted.
    #[serde(default, with = "crate::types::option_decimal")]
    pub hwm: Option<Decimal>,
    /// The desired position strategy.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub position_intent: Option<PositionIntent>,
    /// This leg's proportional quantity within a multi-leg order.
    #[serde(default, with = "crate::types::option_decimal")]
    pub ratio_qty: Option<Decimal>,
}

fn order_class_default() -> OrderClass {
    OrderClass::Simple
}

/// Maps a missing or empty `order_class` to [`OrderClass::Simple`].
fn order_class<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OrderClass, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(match raw.as_deref().map(str::trim) {
        None | Some("") => OrderClass::Simple,
        Some(value) => OrderClass::from(value),
    })
}

/// Why a position could not be closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedClosePositionDetails {
    /// The status code for the failure.
    #[serde(with = "serde_util::int")]
    pub code: i64,
    /// A description of the failure.
    pub message: String,
    /// Quantity available to close.
    #[serde(default, with = "crate::types::option_decimal")]
    pub available: Option<Decimal>,
    /// Total quantity held in the account.
    #[serde(default, with = "crate::types::option_decimal")]
    pub existing_qty: Option<Decimal>,
    /// Quantity locked up in existing orders.
    #[serde(default, with = "crate::types::option_decimal")]
    pub held_for_orders: Option<Decimal>,
    /// Symbol the request applied to.
    #[serde(default)]
    pub symbol: Option<String>,
}

/// The outcome of closing one position, whether or not it succeeded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClosePositionBody {
    /// The liquidating order that was created.
    Order(Box<Order>),
    /// Why the position could not be closed.
    Failed(FailedClosePositionDetails),
}

/// One entry in the response to closing all positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosePositionResponse {
    /// Id of the order created to liquidate the position.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub order_id: Option<Uuid>,
    /// Status code for this position's liquidation.
    #[serde(default, with = "serde_util::int::option")]
    pub status: Option<i64>,
    /// Symbol of the position being closed.
    #[serde(default)]
    pub symbol: Option<String>,
    /// The order, or the reason the close failed.
    pub body: ClosePositionBody,
}

/// The value of a portfolio over time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioHistory {
    /// Left-labeled start of each time window, as Unix seconds.
    pub timestamp: Vec<i64>,
    /// Account equity at the end of each window.
    pub equity: Vec<f64>,
    /// Profit or loss in dollars from the base value.
    pub profit_loss: Vec<f64>,
    /// Profit or loss as a fraction of the base value.
    pub profit_loss_pct: Vec<Option<f64>>,
    /// Basis in dollars for the profit and loss calculation.
    #[serde(default)]
    pub base_value: Option<f64>,
    /// Size of each time window.
    pub timeframe: String,
    /// Cash flow amounts per activity type.
    #[serde(default)]
    pub cashflow: HashMap<ActivityType, Vec<f64>>,
}

/// An ordered list of assets an account is tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchlist {
    /// Alpaca's id for the watchlist.
    pub id: Uuid,
    /// Id of the account the watchlist belongs to.
    pub account_id: Uuid,
    /// An arbitrary name of up to 64 characters.
    pub name: String,
    /// When the watchlist was created.
    pub created_at: DateTime<Utc>,
    /// When the watchlist was last updated.
    pub updated_at: DateTime<Utc>,
    /// The assets in the watchlist. Not returned by every endpoint.
    #[serde(default)]
    pub assets: Option<Vec<Asset>>,
}

/// The market clock for US equity markets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    /// The current time.
    pub timestamp: DateTime<Utc>,
    /// Whether the market is open right now.
    pub is_open: bool,
    /// When the market next opens.
    pub next_open: DateTime<Utc>,
    /// When the market next closes.
    pub next_close: DateTime<Utc>,
}

/// Market hours for a single trading day.
///
/// The API sends every time as a bare string in eastern time. They are combined
/// with `date` into datetimes here, so the fields are usable without the caller
/// re-parsing them.
///
/// The two session fields and `settlement_date` appear in real responses and are
/// optional here, because older responses — and the captured fixtures — do not
/// carry them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Calendar {
    /// The trading day.
    pub date: NaiveDate,
    /// When the regular session opens, as a naive eastern-time datetime.
    pub open: NaiveDateTime,
    /// When the regular session closes, as a naive eastern-time datetime.
    pub close: NaiveDateTime,
    /// When the extended-hours session opens, typically 04:00.
    pub session_open: Option<NaiveDateTime>,
    /// When the extended-hours session closes, typically 20:00.
    pub session_close: Option<NaiveDateTime>,
    /// When trades executed on this day settle.
    pub settlement_date: Option<NaiveDate>,
}

impl<'de> Deserialize<'de> for Calendar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            date: NaiveDate,
            open: String,
            close: String,
            #[serde(default)]
            session_open: Option<String>,
            #[serde(default)]
            session_close: Option<String>,
            #[serde(default)]
            settlement_date: Option<NaiveDate>,
        }

        let raw = Raw::deserialize(deserializer)?;

        let combine = |time: &str, format: &str, field: &str| {
            NaiveDateTime::parse_from_str(&format!("{} {time}", raw.date), format)
                .map_err(|e| de::Error::custom(format!("{field}: {e}")))
        };

        // `open` and `close` are `HH:MM`; the session times are `HHMM`, with no
        // separator. Reusing one format for both silently fails to parse.
        let session = |time: Option<&String>, field: &str| match time {
            Some(time) => combine(time, "%Y-%m-%d %H%M", field).map(Some),
            None => Ok(None),
        };

        Ok(Self {
            open: combine(&raw.open, "%Y-%m-%d %H:%M", "open")?,
            close: combine(&raw.close, "%Y-%m-%d %H:%M", "close")?,
            session_open: session(raw.session_open.as_ref(), "session_open")?,
            session_close: session(raw.session_close.as_ref(), "session_close")?,
            settlement_date: raw.settlement_date,
            date: raw.date,
        })
    }
}

/// An account activity that is not a trade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonTradeActivity {
    /// Unique id, formatted as `<date>::<uuid>`.
    pub id: String,
    /// Id of the account the activity belongs to.
    pub account_id: Uuid,
    /// What kind of activity this is.
    pub activity_type: ActivityType,
    /// When the activity occurred or its transaction settled.
    pub date: NaiveDate,
    /// Net amount of money associated with the activity.
    #[serde(with = "crate::types::decimal")]
    pub net_amount: Decimal,
    /// Extra description, which may be the empty string.
    pub description: String,
    /// Status of the activity. Not present for all types.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status: Option<NonTradeActivityStatus>,
    /// Symbol of the security involved. Not present for all types.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub symbol: Option<String>,
    /// Shares that contributed to a dividend payment.
    #[serde(default, with = "crate::types::option_decimal")]
    pub qty: Option<Decimal>,
    /// Price associated with the activity.
    #[serde(default, with = "crate::types::option_decimal")]
    pub price: Option<Decimal>,
    /// Average amount paid per share, for dividends.
    #[serde(default, with = "crate::types::option_decimal")]
    pub per_share_amount: Option<Decimal>,
}

/// An account activity representing a fill or partial fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeActivity {
    /// Unique id, formatted as `<date>::<uuid>`.
    pub id: String,
    /// Id of the account the activity belongs to.
    pub account_id: Uuid,
    /// What kind of activity this is. Always [`ActivityType::Fill`].
    pub activity_type: ActivityType,
    /// When the trade was processed.
    pub transaction_time: DateTime<Utc>,
    /// Whether this was a fill or a partial fill.
    #[serde(rename = "type")]
    pub trade_type: TradeActivityType,
    /// Price per share the trade executed at.
    #[serde(with = "crate::types::decimal")]
    pub price: Decimal,
    /// Number of shares involved in this execution.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// Which side the trade was on.
    pub side: OrderSide,
    /// Symbol of the asset traded.
    pub symbol: String,
    /// Shares left to fill, zero unless partially filled.
    #[serde(with = "crate::types::decimal")]
    pub leaves_qty: Decimal,
    /// Id of the order that filled.
    pub order_id: Uuid,
    /// Cumulative shares executed on the order.
    #[serde(with = "crate::types::decimal")]
    pub cum_qty: Decimal,
    /// Status of the order that executed the trade.
    pub order_status: OrderStatus,
}

/// An account activity, which is either a trade or something else.
///
/// The account-activities endpoint returns a heterogeneous array whose element
/// type is decided by `activity_type`. This enum makes that distinction visible
/// in the type rather than leaving it to a runtime branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Activity {
    /// A fill or partial fill.
    Trade(TradeActivity),
    /// Anything else: dividends, fees, transfers, and so on.
    NonTrade(NonTradeActivity),
}

/// Trading account information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeAccount {
    /// Alpaca's id for the account.
    pub id: Uuid,
    /// The account number.
    pub account_number: String,
    /// Current status of the account.
    pub status: AccountStatus,
    /// Status of the account for crypto trading, when enabled.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub crypto_status: Option<AccountStatus>,
    /// Account currency, currently always `USD`.
    #[serde(default)]
    pub currency: Option<String>,
    /// Current available cash buying power.
    #[serde(default, with = "crate::types::option_decimal")]
    pub buying_power: Option<Decimal>,
    /// Buying power under Regulation T.
    #[serde(default, with = "crate::types::option_decimal")]
    pub regt_buying_power: Option<Decimal>,
    /// Day trade buying power.
    ///
    /// Removed from Alpaca responses on 2026-07-06 in the FINRA intraday-margin
    /// migration, so this is now absent in practice.
    #[serde(default, with = "crate::types::option_decimal")]
    pub daytrading_buying_power: Option<Decimal>,
    /// Non-marginable buying power.
    #[serde(default, with = "crate::types::option_decimal")]
    pub non_marginable_buying_power: Option<Decimal>,
    /// Cash balance.
    #[serde(default, with = "crate::types::option_decimal")]
    pub cash: Option<Decimal>,
    /// Fees accrued on the account.
    #[serde(default, with = "crate::types::option_decimal")]
    pub accrued_fees: Option<Decimal>,
    /// Cash pending transfer out.
    #[serde(default, with = "crate::types::option_decimal")]
    pub pending_transfer_out: Option<Decimal>,
    /// Cash pending transfer in.
    #[serde(default, with = "crate::types::option_decimal")]
    pub pending_transfer_in: Option<Decimal>,
    /// Total value of cash plus holdings. Deprecated alias for `equity`.
    #[serde(default, with = "crate::types::option_decimal")]
    pub portfolio_value: Option<Decimal>,
    /// Whether the account is flagged as a pattern day trader.
    ///
    /// Removed from Alpaca responses on 2026-07-06.
    #[serde(default)]
    pub pattern_day_trader: Option<bool>,
    /// Whether the account is blocked from placing orders.
    #[serde(default)]
    pub trading_blocked: Option<bool>,
    /// Whether the account is blocked from money transfers.
    #[serde(default)]
    pub transfers_blocked: Option<bool>,
    /// Whether account activity by the user is prohibited.
    #[serde(default)]
    pub account_blocked: Option<bool>,
    /// When the account was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Whether the user has suspended their own trading.
    #[serde(default)]
    pub trade_suspended_by_user: Option<bool>,
    /// Margin multiplier for the account.
    #[serde(default, with = "crate::types::option_decimal")]
    pub multiplier: Option<Decimal>,
    /// Whether the account may short.
    #[serde(default)]
    pub shorting_enabled: Option<bool>,
    /// Cash plus long market value plus short market value.
    #[serde(default, with = "crate::types::option_decimal")]
    pub equity: Option<Decimal>,
    /// Equity as of 16:00 ET on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_equity: Option<Decimal>,
    /// Mark-to-market value of all long positions.
    #[serde(default, with = "crate::types::option_decimal")]
    pub long_market_value: Option<Decimal>,
    /// Mark-to-market value of all short positions.
    #[serde(default, with = "crate::types::option_decimal")]
    pub short_market_value: Option<Decimal>,
    /// Regulation T initial margin requirement.
    #[serde(default, with = "crate::types::option_decimal")]
    pub initial_margin: Option<Decimal>,
    /// Maintenance margin requirement.
    #[serde(default, with = "crate::types::option_decimal")]
    pub maintenance_margin: Option<Decimal>,
    /// Maintenance margin requirement on the previous trading day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub last_maintenance_margin: Option<Decimal>,
    /// Value of the Special Memorandum Account.
    #[serde(default, with = "crate::types::option_decimal")]
    pub sma: Option<Decimal>,
    /// Day trades made in the last 5 trading days, inclusive of today.
    ///
    /// Removed from Alpaca responses on 2026-07-06.
    #[serde(default, with = "serde_util::int::option")]
    pub daytrade_count: Option<i64>,
    /// Buying power for options trading.
    #[serde(default, with = "crate::types::option_decimal")]
    pub options_buying_power: Option<Decimal>,
    /// Approved options trading level: 0 disabled through 3 spreads.
    #[serde(default, with = "serde_util::int::option")]
    pub options_approved_level: Option<i64>,
    /// Effective options trading level, the lower of approved and configured.
    #[serde(default, with = "serde_util::int::option")]
    pub options_trading_level: Option<i64>,
}

/// Configuration options for a trading account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountConfiguration {
    /// Day Trade Buying Power check.
    ///
    /// Removed from Alpaca responses on 2026-07-06.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub dtbp_check: Option<DTBPCheck>,
    /// Whether the account may trade fractional shares.
    pub fractional_trading: bool,
    /// Maximum margin multiplier, between 1 and 4.
    pub max_margin_multiplier: String,
    /// Whether the account is restricted to long-only.
    pub no_shorting: bool,
    /// Pattern Day Trader check.
    ///
    /// Removed from Alpaca responses on 2026-07-06.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub pdt_check: Option<PDTCheck>,
    /// Whether the account is blocked from submitting new orders.
    pub suspend_trade: bool,
    /// Whether trade confirmation emails are sent.
    pub trade_confirm_email: TradeConfirmationEmail,
    /// Whether to accept orders for PTP symbols with no exception.
    pub ptp_no_exception_entry: bool,
    /// Desired maximum options trading level.
    #[serde(default, with = "serde_util::int::option")]
    pub max_options_trading_level: Option<i64>,
}

/// An announced corporate action, such as a dividend, merger, or split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorporateActionAnnouncement {
    /// Unique id for this announcement.
    pub id: Uuid,
    /// Id shared by every announcement for the same corporate action.
    pub corporate_action_id: String,
    /// The type of corporate action announced.
    pub ca_type: CorporateActionType,
    /// The specific subtype announced.
    pub ca_sub_type: CorporateActionSubType,
    /// Symbol of the company initiating the action.
    pub initiating_symbol: String,
    /// CUSIP of the company initiating the action.
    pub initiating_original_cusip: String,
    /// Symbol of the child company involved.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub target_symbol: Option<String>,
    /// CUSIP of the child company involved.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub target_original_cusip: Option<String>,
    /// When the action or a terms update was announced.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub declaration_date: Option<NaiveDate>,
    /// First date on which buying does not confer entitlement.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ex_date: Option<NaiveDate>,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub record_date: Option<NaiveDate>,
    /// Date the announcement takes effect.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub payable_date: Option<NaiveDate>,
    /// Cash paid per share held on the record date.
    #[serde(with = "crate::types::decimal")]
    pub cash: Decimal,
    /// Denominator of any quantity change ratio.
    #[serde(with = "crate::types::decimal")]
    pub old_rate: Decimal,
    /// Numerator of any quantity change ratio.
    #[serde(with = "crate::types::decimal")]
    pub new_rate: Decimal,
}

/// A trade update pushed over the trading websocket stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeUpdate {
    /// What happened to the order.
    pub event: TradeEvent,
    /// Id of the execution, for fill events.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub execution_id: Option<Uuid>,
    /// The order the update concerns.
    pub order: Order,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Position quantity after the event.
    #[serde(default, with = "crate::types::option_decimal")]
    pub position_qty: Option<Decimal>,
    /// Price of the fill.
    #[serde(default, with = "crate::types::option_decimal")]
    pub price: Option<Decimal>,
    /// Quantity of the fill.
    #[serde(default, with = "crate::types::option_decimal")]
    pub qty: Option<Decimal>,
}

/// An option contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionContract {
    /// Unique id of the contract.
    pub id: String,
    /// The contract's OCC symbol.
    pub symbol: String,
    /// Human-readable name of the contract.
    pub name: String,
    /// Whether the contract is active.
    pub status: AssetStatus,
    /// Whether the contract can be traded.
    pub tradable: bool,
    /// When the contract expires.
    pub expiration_date: NaiveDate,
    /// The contract's root symbol.
    pub root_symbol: String,
    /// Symbol of the underlying asset.
    pub underlying_symbol: String,
    /// Id of the underlying asset.
    pub underlying_asset_id: Uuid,
    /// Call or put.
    #[serde(rename = "type")]
    pub contract_type: ContractType,
    /// American or European exercise style.
    pub style: ExerciseStyle,
    /// Strike price of the contract.
    #[serde(with = "crate::types::decimal")]
    pub strike_price: Decimal,
    /// Contract size, usually 100.
    pub size: String,
    /// Open interest in the contract.
    #[serde(default, with = "crate::types::option_decimal")]
    pub open_interest: Option<Decimal>,
    /// Date the open interest figure is from.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub open_interest_date: Option<NaiveDate>,
    /// Closing price of the contract.
    #[serde(default, with = "crate::types::option_decimal")]
    pub close_price: Option<Decimal>,
    /// Date the closing price is from.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub close_price_date: Option<NaiveDate>,
}

/// A page of option contracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionContractsResponse {
    /// The contracts on this page.
    #[serde(default)]
    pub option_contracts: Option<Vec<OptionContract>>,
    /// Token for fetching the next page.
    #[serde(default)]
    pub next_page_token: Option<String>,
}
