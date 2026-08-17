//! The string enums the trading API accepts and sends.
//!
//! Every one decodes an unrecognised value into `Unknown(String)` rather than
//! failing, so a value Alpaca starts sending costs a caller a match arm and not
//! a decode. That is why removing a value is more dangerous than keeping a stale
//! one, and why `just enums-drift` reports the two directions separately.
//!
//! Hand-written `impl` blocks belong in the sibling `enums_ext.rs`, which keeps
//! the wire vocabulary in one file and the behaviour in another.

use crate::types::wire::wire_enum;

/// Represents what kind of Activity an instance of `TradeActivity` or `NonTradeActivity` is.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#enumactivitytype>.
#[wire_enum]
pub enum ActivityType {
    /// Order fills (both partial and full fills)
    #[wire = "FILL"]
    Fill,
    /// ACATS IN/OUT (Cash)
    #[wire = "ACATC"]
    Acatc,
    /// ACATS IN/OUT (Securities)
    #[wire = "ACATS"]
    Acats,
    /// Crypto fee
    #[wire = "CFEE"]
    Cfee,
    /// Capital gain distribution
    #[wire = "CGD"]
    Cgd,
    /// Cash in lieu of stock
    #[wire = "CIL"]
    Cil,
    /// Cash deposit(+)
    #[wire = "CSD"]
    Csd,
    /// Cash withdrawal(-)
    #[wire = "CSW"]
    Csw,
    /// Dividends
    #[wire = "DIV"]
    Div,
    /// Dividend (capital gain long term)
    #[wire = "DIVCGL"]
    Divcgl,
    /// Dividend (capital gain short term)
    #[wire = "DIVCGS"]
    Divcgs,
    /// Dividend fee
    #[wire = "DIVFEE"]
    Divfee,
    /// Dividend adjusted (Foreign Tax Withheld)
    #[wire = "DIVFT"]
    Divft,
    /// Dividend adjusted (NRA Withheld)
    #[wire = "DIVNRA"]
    Divnra,
    /// Dividend return of capital
    #[wire = "DIVROC"]
    Divroc,
    /// Dividend adjusted (Tefra Withheld)
    #[wire = "DIVTW"]
    Divtw,
    /// Dividend (tax exempt)
    #[wire = "DIVTXEX"]
    Divtxex,
    /// `DIVWH`. The specs do not describe this value.
    #[wire = "DIVWH"]
    Divwh,
    /// `EXTRD`. The specs do not describe this value.
    #[wire = "EXTRD"]
    Extrd,
    /// Fee denominated in USD
    #[wire = "FEE"]
    Fee,
    /// Free of Payment Transfers
    #[wire = "FOPT"]
    Fopt,
    /// `FXTRD`. The specs do not describe this value.
    #[wire = "FXTRD"]
    Fxtrd,
    /// Interest (credit/margin)
    #[wire = "INT"]
    Int,
    /// Interest adjusted (NRA Withheld)
    #[wire = "INTNRA"]
    Intnra,
    /// `INTPNL`. The specs do not describe this value.
    #[wire = "INTPNL"]
    Intpnl,
    /// Interest adjusted (Tefra Withheld)
    #[wire = "INTTW"]
    Inttw,
    /// Journal entry
    #[wire = "JNL"]
    Jnl,
    /// Journal entry (cash)
    #[wire = "JNLC"]
    Jnlc,
    /// Journal entry (stock)
    #[wire = "JNLS"]
    Jnls,
    /// Merger/Acquisition
    #[wire = "MA"]
    Ma,
    /// `MEM`. The specs do not describe this value.
    #[wire = "MEM"]
    Mem,
    /// Miscellaneous or rarely used activity types (All types except those in TRANS, DIV, or FILL)
    #[wire = "MISC"]
    Misc,
    /// Name change
    #[wire = "NC"]
    Nc,
    /// On chain transactions (blockchain deposits/withdrawals)
    #[wire = "OCT"]
    Oct,
    /// Option assignment
    #[wire = "OPASN"]
    Opasn,
    /// Option corporate action
    #[wire = "OPCA"]
    Opca,
    /// Option cash deliverable for non-standard contracts
    #[wire = "OPCSH"]
    Opcsh,
    /// Option exercise
    #[wire = "OPEXC"]
    Opexc,
    /// Option expiration
    #[wire = "OPEXP"]
    Opexp,
    /// Option trade
    #[wire = "OPTRD"]
    Optrd,
    /// Pass Thru Charge
    #[wire = "PTC"]
    Ptc,
    /// Pass Thru Rebate
    #[wire = "PTR"]
    Ptr,
    /// Reorg CA
    #[wire = "REORG"]
    Reorg,
    /// Stock spinoff
    #[wire = "SPIN"]
    Spin,
    /// Stock split
    #[wire = "SPLIT"]
    Split,
    /// `SWP`. The specs do not describe this value.
    #[wire = "SWP"]
    Swp,
    /// Cash transactions (both CSD and CSW)
    #[wire = "TRANS"]
    Trans,
    /// `VOF`. The specs do not describe this value.
    #[wire = "VOF"]
    Vof,
    /// `WH`. The specs do not describe this value.
    #[wire = "WH"]
    Wh,
}

/// Represents the type of `TradeActivity`.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#attributes>.
#[wire_enum]
pub enum TradeActivityType {
    /// `partial_fill`
    #[wire = "partial_fill"]
    PartialFill,
    /// `fill`
    #[wire = "fill"]
    Fill,
}

/// Represents the status of a `NonTradeActivity`.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#enumaccountactivity>.
#[wire_enum]
pub enum NonTradeActivityStatus {
    /// `executed`
    #[wire = "executed"]
    Executed,
    /// `correct`
    #[wire = "correct"]
    Correct,
    /// `canceled`
    #[wire = "canceled"]
    Canceled,
}

/// Represents what class of order this is.
///
/// The order classes supported by Alpaca vary based on the order's security type.
/// The following provides a comprehensive breakdown of the supported order classes for each category:
/// - Equity trading: simple (or ""), oco, oto, bracket.
/// - Options trading: simple (or ""), mleg (required for multi-leg complex options strategies).
/// - Crypto trading: simple (or "").
#[wire_enum]
pub enum OrderClass {
    /// `simple`
    #[wire = "simple"]
    Simple,
    /// `mleg`
    #[wire = "mleg"]
    Mleg,
    /// `bracket`
    #[wire = "bracket"]
    Bracket,
    /// `oco`
    #[wire = "oco"]
    Oco,
    /// `oto`
    #[wire = "oto"]
    Oto,
}

/// How an order is priced and triggered.
///
/// Which types are accepted depends on what is being traded, and sending an
/// unsupported one is rejected by Alpaca rather than by this crate:
///
/// | Asset class | Accepted |
/// |---|---|
/// | Equities | `market`, `limit`, `stop`, `stop_limit`, `trailing_stop` |
/// | Options | `market`, `limit`, `stop`, `stop_limit` |
/// | Crypto | `market`, `limit`, `stop_limit` |
///
/// Each has a constructor on
/// [`OrderRequest`](crate::trading::OrderRequest) that takes the fields that
/// type requires, so an order cannot be built missing its own stop or limit
/// price.
#[wire_enum]
pub enum OrderType {
    /// `market`
    #[wire = "market"]
    Market,
    /// `limit`
    #[wire = "limit"]
    Limit,
    /// `stop`
    #[wire = "stop"]
    Stop,
    /// `stop_limit`
    #[wire = "stop_limit"]
    StopLimit,
    /// `trailing_stop`
    #[wire = "trailing_stop"]
    TrailingStop,
}

/// Represents what side this order was executed on.
#[wire_enum]
pub enum OrderSide {
    /// Buy.
    #[wire = "buy"]
    Buy,
    /// Sell.
    #[wire = "sell"]
    Sell,
    /// Buy at or below the last sale price.
    #[wire = "buy_minus"]
    BuyMinus,
    /// Sell at or above the last sale price.
    #[wire = "sell_plus"]
    SellPlus,
    /// Sell short.
    #[wire = "sell_short"]
    SellShort,
    /// Sell short, exempt from the price test.
    #[wire = "sell_short_exempt"]
    SellShortExempt,
    /// Side not disclosed.
    #[wire = "undisclosed"]
    Undisclosed,
    /// A cross, where one broker is on both sides.
    #[wire = "cross"]
    Cross,
    /// A cross where the sell side is short.
    #[wire = "cross_short"]
    CrossShort,
}

/// Represents the various states an Order can be in.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/orders/#order-status>.
#[wire_enum]
pub enum OrderStatus {
    /// `new`
    #[wire = "new"]
    New,
    /// `partially_filled`
    #[wire = "partially_filled"]
    PartiallyFilled,
    /// `filled`
    #[wire = "filled"]
    Filled,
    /// `done_for_day`
    #[wire = "done_for_day"]
    DoneForDay,
    /// `canceled`
    #[wire = "canceled"]
    Canceled,
    /// `expired`
    #[wire = "expired"]
    Expired,
    /// `replaced`
    #[wire = "replaced"]
    Replaced,
    /// `pending_cancel`
    #[wire = "pending_cancel"]
    PendingCancel,
    /// `pending_replace`
    #[wire = "pending_replace"]
    PendingReplace,
    /// `pending_review`
    #[wire = "pending_review"]
    PendingReview,
    /// `accepted`
    #[wire = "accepted"]
    Accepted,
    /// `pending_new`
    #[wire = "pending_new"]
    PendingNew,
    /// `accepted_for_bidding`
    #[wire = "accepted_for_bidding"]
    AcceptedForBidding,
    /// `stopped`
    #[wire = "stopped"]
    Stopped,
    /// `rejected`
    #[wire = "rejected"]
    Rejected,
    /// `suspended`
    #[wire = "suspended"]
    Suspended,
    /// `calculated`
    #[wire = "calculated"]
    Calculated,
    /// `held`
    #[wire = "held"]
    Held,
}

/// This represents the category to which the asset belongs to.
/// It serves to identify the nature of the financial instrument, with options
/// including "`us_equity`" for U.S. equities, "`us_option`" for U.S. options,
/// and "crypto" for cryptocurrencies.
#[wire_enum]
pub enum AssetClass {
    /// A US-listed equity.
    #[wire = "us_equity"]
    UsEquity,
    /// A US-listed option contract.
    #[wire = "us_option"]
    UsOption,
    /// A cryptocurrency pair.
    #[wire = "crypto"]
    Crypto,
    /// A perpetual crypto future.
    #[wire = "crypto_perp"]
    CryptoPerp,
    /// The option chain on a US-listed equity.
    #[wire = "us_equity_chain"]
    UsEquityChain,
    /// A US market index.
    #[wire = "us_index"]
    UsIndex,
    /// An equity listed outside the US.
    #[wire = "global_equity"]
    GlobalEquity,
    /// A US treasury instrument.
    #[wire = "treasury"]
    Treasury,
    /// A US corporate bond.
    #[wire = "corporate"]
    Corporate,
    /// An indication of interest in an IPO.
    ///
    /// Distinct from the assets API's `attributes: ["ipo"]` flag, which marks
    /// an ordinary equity as IPO-eligible; this is the asset class itself.
    #[wire = "ipo"]
    Ipo,
}

/// Represents the various states for an Asset's lifecycle
#[wire_enum(sorted)]
pub enum AssetStatus {
    /// `active`
    #[wire = "active"]
    Active,
    /// `inactive`
    #[wire = "inactive"]
    Inactive,
}

/// Represents the current exchanges Alpaca supports.
#[wire_enum]
pub enum AssetExchange {
    /// `AMEX`
    #[wire = "AMEX"]
    Amex,
    /// `ARCA`
    #[wire = "ARCA"]
    Arca,
    /// `ASCX`
    #[wire = "ASCX"]
    Ascx,
    /// `BATS`
    #[wire = "BATS"]
    Bats,
    /// `NYSE`
    #[wire = "NYSE"]
    Nyse,
    /// `NASDAQ`
    #[wire = "NASDAQ"]
    Nasdaq,
    /// `NYSEARCA`
    #[wire = "NYSEARCA"]
    Nysearca,
    /// `FTXU`
    #[wire = "FTXU"]
    Ftxu,
    /// `CBSE`
    #[wire = "CBSE"]
    Cbse,
    /// `GNSS`
    #[wire = "GNSS"]
    Gnss,
    /// `ERSX`
    #[wire = "ERSX"]
    Ersx,
    /// `OTC`
    #[wire = "OTC"]
    Otc,
    /// `CRYPTO`
    #[wire = "CRYPTO"]
    Crypto,
    /// The empty value.
    #[wire = ""]
    Empty,
}

/// Represents what side this position is.
#[wire_enum]
pub enum PositionSide {
    /// `short`
    #[wire = "short"]
    Short,
    /// `long`
    #[wire = "long"]
    Long,
}

/// Represents the various time in force options for an Order.
///
/// The Time-In-Force values supported by Alpaca vary based on the order's security type. Here is a breakdown of the supported `TIFs` for each specific security type:
/// - Equity trading: day, gtc, opg, cls, ioc, fok.
/// - Options trading: day.
/// - Crypto trading: gtc, ioc.
/// Below are the descriptions of each TIF:
/// - day: A day order is eligible for execution only on the day it is live. By default, the order is only valid during Regular Trading Hours (9:30am - 4:00pm ET). If unfilled after the closing auction, it is automatically canceled. If submitted after the close, it is queued and submitted the following trading day. However, if marked as eligible for extended hours, the order can also execute during supported extended hours.
/// - gtc: The order is good until canceled. Non-marketable GTC limit orders are subject to price adjustments to offset corporate actions affecting the issue. We do not currently support Do Not Reduce(DNR) orders to opt out of such price adjustments.
/// - opg: Use this TIF with a market/limit order type to submit “market on open” (MOO) and “limit on open” (LOO) orders. This order is eligible to execute only in the market opening auction. Any unfilled orders after the open will be cancelled. OPG orders submitted after 9:28am but before 7:00pm ET will be rejected. OPG orders submitted after 7:00pm will be queued and routed to the following day’s opening auction. On open/on close orders are routed to the primary exchange. Such orders do not necessarily execute exactly at 9:30am / 4:00pm ET but execute per the exchange’s auction rules.
/// - cls: Use this TIF with a market/limit order type to submit “market on close” (MOC) and “limit on close” (LOC) orders. This order is eligible to execute only in the market closing auction. Any unfilled orders after the close will be cancelled. CLS orders submitted after 3:50pm but before 7:00pm ET will be rejected. CLS orders submitted after 7:00pm will be queued and routed to the following day’s closing auction. Only available with API v2.
/// - ioc: An Immediate Or Cancel (IOC) order requires all or part of the order to be executed immediately. Any unfilled portion of the order is canceled. Only available with API v2. Most market makers who receive IOC orders will attempt to fill the order on a principal basis only, and cancel any unfilled balance. On occasion, this can result in the entire order being cancelled if the market maker does not have any existing inventory of the security in question.
/// - fok: A Fill or Kill (FOK) order is only executed if the entire order quantity can be filled, otherwise the order is canceled. Only available with API v2.
#[wire_enum]
pub enum TimeInForce {
    /// `day`
    #[wire = "day"]
    Day,
    /// `gtc`
    #[wire = "gtc"]
    Gtc,
    /// `opg`
    #[wire = "opg"]
    Opg,
    /// `cls`
    #[wire = "cls"]
    Cls,
    /// `ioc`
    #[wire = "ioc"]
    Ioc,
    /// `fok`
    #[wire = "fok"]
    Fok,
}

/// The general types of corporate action events.
///
/// See <https://docs.alpaca.markets/docs/corporate-actions>.
#[wire_enum(sorted)]
pub enum CorporateActionType {
    /// `dividend`
    #[wire = "dividend"]
    Dividend,
    /// `merger`
    #[wire = "merger"]
    Merger,
    /// `spinoff`
    #[wire = "spinoff"]
    Spinoff,
    /// `split`
    #[wire = "split"]
    Split,
}

/// The specific types of corporate actions. Each subtype is related to `CorporateActionType`.
///
/// See <https://docs.alpaca.markets/docs/corporate-actions>.
#[wire_enum]
pub enum CorporateActionSubType {
    /// `cash`
    #[wire = "cash"]
    Cash,
    /// `stock`
    #[wire = "stock"]
    Stock,
    /// `merger_update`
    #[wire = "merger_update"]
    MergerUpdate,
    /// `merger_completion`
    #[wire = "merger_completion"]
    MergerCompletion,
    /// `spinoff`
    #[wire = "spinoff"]
    Spinoff,
    /// `stock_split`
    #[wire = "stock_split"]
    StockSplit,
    /// `unit_split`
    #[wire = "unit_split"]
    UnitSplit,
    /// `reverse_split`
    #[wire = "reverse_split"]
    ReverseSplit,
    /// `recapitalization`
    #[wire = "recapitalization"]
    Recapitalization,
}

/// The various statuses each brokerage account can take during its lifetime
///
/// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#account-status>.
#[wire_enum(sorted)]
pub enum AccountStatus {
    /// The account is closed.
    #[wire = "ACCOUNT_CLOSED"]
    AccountClosed,
    /// The account close is in progress.
    ///
    /// Listed by the trading spec, which describes no other value of this
    /// name; the reading here is the name's, not the spec's.
    #[wire = "ACCOUNT_CLOSED_PENDING"]
    AccountClosedPending,
    /// The account information is being updated.
    #[wire = "ACCOUNT_UPDATED"]
    AccountUpdated,
    /// The application requires manual action.
    #[wire = "ACTION_REQUIRED"]
    ActionRequired,
    /// The account is active for trading.
    #[wire = "ACTIVE"]
    Active,
    /// `AML_REVIEW`. The specs do not describe this value.
    #[wire = "AML_REVIEW"]
    AmlReview,
    /// The final account approval is pending.
    #[wire = "APPROVAL_PENDING"]
    ApprovalPending,
    /// The account application has been approved, and is waiting to become
    /// active.
    #[wire = "APPROVED"]
    Approved,
    /// `DISABLED`. The specs do not describe this value.
    #[wire = "DISABLED"]
    Disabled,
    /// `DISABLE_PENDING`. The specs do not describe this value.
    #[wire = "DISABLE_PENDING"]
    DisablePending,
    /// `EDITED`. The specs do not describe this value.
    #[wire = "EDITED"]
    Edited,
    /// The account is not set to trade the asset in question.
    #[wire = "INACTIVE"]
    Inactive,
    /// `KYC_SUBMITTED`. The specs do not describe this value.
    #[wire = "KYC_SUBMITTED"]
    KycSubmitted,
    /// `LIMITED`. The specs do not describe this value.
    #[wire = "LIMITED"]
    Limited,
    /// The account is onboarding.
    #[wire = "ONBOARDING"]
    Onboarding,
    /// `PAPER_ONLY`. The specs do not describe this value.
    #[wire = "PAPER_ONLY"]
    PaperOnly,
    /// `REAPPROVAL_PENDING`. The specs do not describe this value.
    #[wire = "REAPPROVAL_PENDING"]
    ReapprovalPending,
    /// The account application has been rejected.
    #[wire = "REJECTED"]
    Rejected,
    /// `RESUBMITTED`. The specs do not describe this value.
    #[wire = "RESUBMITTED"]
    Resubmitted,
    /// `SIGNED_UP`. The specs do not describe this value.
    #[wire = "SIGNED_UP"]
    SignedUp,
    /// The account application submission failed for some reason.
    #[wire = "SUBMISSION_FAILED"]
    SubmissionFailed,
    /// The account application has been submitted for review.
    #[wire = "SUBMITTED"]
    Submitted,
}

/// The `CorporateActionDateType` values accepted by the API.
#[wire_enum]
pub enum CorporateActionDateType {
    /// `declaration_date`
    #[wire = "declaration_date"]
    DeclarationDate,
    /// `ex_date`
    #[wire = "ex_date"]
    ExDate,
    /// `record_date`
    #[wire = "record_date"]
    RecordDate,
    /// `payable_date`
    #[wire = "payable_date"]
    PayableDate,
}

/// What happened to an order, as reported by the `trade_updates` stream.
///
/// This is the set of events the stream *emits*, which is not the set of
/// states an order can be *in*. The two vocabularies overlap heavily —
/// fourteen of these values are also [`OrderStatus`] values — but neither
/// list contains the other, and the differences are where the mistakes
/// live: an execution is the `fill` event and the [`OrderStatus::Filled`]
/// status, spelled differently, and `filled` is never an event.
///
/// The variants follow the order of Alpaca's `TradeUpdateEventType` schema,
/// as [`OrderStatus`] follows its own. That order is broadly the lifecycle
/// one, but it is not the "common events, then rarer events" split the
/// prose uses: `accepted` is documented as common in every passage yet sits
/// tenth, behind `rejected` and `pending_new`. Do not read the position of a
/// variant as a claim about how often it arrives — and note the passages do
/// not agree with each other either, `pending_new` being common in the
/// operation's description and rare in the schema's.
///
/// Reconciled against the `TradeUpdateEventType` schema in Alpaca's broker
/// specification, which lists nineteen values. The published reference for
/// the trade events stream lists the same nineteen, but it is a later
/// revision of that same specification rather than an independent account,
/// so treat the two as one source. [`Self::Restated`] and [`Self::Held`]
/// are in neither list and are described only in surrounding prose — see
/// their own documentation, which says which prose and how much of it.
///
/// That schema belongs to the server-sent trade events endpoint, and no
/// vendored source enumerates the websocket stream's vocabulary separately.
/// The two are taken to be one vocabulary here, which is what Alpaca's
/// documentation implies by describing the same events for both, but it is
/// an assumption rather than something a source states.
///
/// See <https://docs.alpaca.markets/us/docs/websocket-streaming> for the
/// stream itself, and <https://docs.alpaca.markets/reference/subscribetotradev2sse>
/// for the reference this was reconciled against — the former describes the
/// websocket and does not list every value below.
#[wire_enum]
pub enum TradeEvent {
    /// Sent when an order has been routed to exchanges for execution.
    #[wire = "new"]
    New,
    /// Sent when the order has been completely filled.
    ///
    /// `timestamp` is the time at which the order was filled.
    #[wire = "fill"]
    Fill,
    /// Sent when fewer shares than the total remaining quantity on the
    /// order have been filled.
    ///
    /// `timestamp` is the time at which the shares were filled.
    #[wire = "partial_fill"]
    PartialFill,
    /// Sent when the order transitions to the canceled state.
    ///
    /// Not only in response to a cancel request. Alpaca also cancels as part
    /// of automated processing — corporate-action sweeps, aged-GTC
    /// expiration, the overnight-session lifecycle — and an upstream venue
    /// can cancel too. Do not read this event as "the cancel I asked for
    /// went through".
    ///
    /// Alpaca exposes a machine-readable cause for it on a `reason` field,
    /// but documents that field only on the *server-sent* form of these
    /// events, which this crate does not yet model. No source says the
    /// websocket frame this type arrives on carries one.
    ///
    /// `timestamp` is the time at which the order was canceled.
    #[wire = "canceled"]
    Canceled,
    /// Sent when an order has reached the end of its lifespan, as
    /// determined by the order's time in force.
    ///
    /// `timestamp` is the time at which the order expired.
    #[wire = "expired"]
    Expired,
    /// Sent when the order is done executing for the day, and will not
    /// receive further updates until the next trading day.
    #[wire = "done_for_day"]
    DoneForDay,
    /// Sent when a requested replacement of an order is processed.
    ///
    /// `timestamp` is the time at which the order was replaced.
    #[wire = "replaced"]
    Replaced,
    /// Sent when the order has been rejected.
    ///
    /// `timestamp` is the time at which the rejection occurred.
    #[wire = "rejected"]
    Rejected,
    /// Sent when the order has been received by Alpaca and routed to the
    /// exchanges, but has not yet been accepted for execution.
    #[wire = "pending_new"]
    PendingNew,
    /// Sent when an order is received and accepted by Alpaca.
    #[wire = "accepted"]
    Accepted,
    /// Sent when the order has been stopped: a trade is guaranteed for the
    /// order, usually at a stated price or better, but has not yet
    /// occurred.
    #[wire = "stopped"]
    Stopped,
    /// Sent when the order is awaiting cancellation.
    ///
    /// Most cancellations occur without the order entering this state.
    #[wire = "pending_cancel"]
    PendingCancel,
    /// Sent when the order is awaiting replacement.
    #[wire = "pending_replace"]
    PendingReplace,
    /// Sent when the order has been completed for the day — it is either
    /// filled or done for the day — but remaining settlement calculations
    /// are still pending.
    #[wire = "calculated"]
    Calculated,
    /// Sent when the order has been suspended and is not eligible for
    /// trading.
    #[wire = "suspended"]
    Suspended,
    /// Sent when the order replace has been rejected.
    ///
    /// Note the `order_` prefix: the wire value is `order_replace_rejected`
    /// rather than `replace_rejected`. This is an ordinary event for
    /// anything that reprices a resting limit order, despite sitting under
    /// Alpaca's "rarer events" heading.
    #[wire = "order_replace_rejected"]
    OrderReplaceRejected,
    /// Sent when the order cancel has been rejected.
    ///
    /// Prefixed like [`Self::OrderReplaceRejected`], and ordinary for the
    /// same reason: a cancel loses the race against a fill routinely.
    #[wire = "order_cancel_rejected"]
    OrderCancelRejected,
    /// Sent when a previously reported execution has been canceled
    /// ("busted") by the upstream exchange.
    #[wire = "trade_bust"]
    TradeBust,
    /// Sent when a previously reported trade has been corrected — the
    /// exchange may have updated the price, quantity or another execution
    /// parameter after the trade was initially reported.
    #[wire = "trade_correct"]
    TradeCorrect,
    /// Sent when the order is manually modified.
    ///
    /// Described in prose in two places — the schema's own description and
    /// the trade-events operation's — and absent from every machine-readable
    /// value list. Both passages come from the same specification, which
    /// the published reference republishes — a later revision of it, not an
    /// independent account — so treat this as one source saying it twice
    /// rather than two agreeing. Carried on that prose; the value lists
    /// alone would drop it.
    #[wire = "restated"]
    Restated,
    /// For multi-leg orders, the state the secondary orders (stop loss,
    /// take profit) enter while waiting to be triggered.
    ///
    /// Prose-only in the same way [`Self::Restated`] is, and weaker still:
    /// where `restated` is described in two prose passages, this appears in
    /// exactly one — the trade-events operation description — and nowhere in
    /// the schema's own. It is also an [`OrderStatus`] value, the only one
    /// of these two that is, so it may be a status that leaked into an event
    /// list rather than an event in its own right. This is the single
    /// thinnest claim in the enum. Carried because an unnamed value is one
    /// no caller can match on, while a variant Alpaca never sends costs a
    /// dead match arm.
    #[wire = "held"]
    Held,
}

/// The `QueryOrderStatus` values accepted by the API.
#[wire_enum]
pub enum QueryOrderStatus {
    /// `open`
    #[wire = "open"]
    Open,
    /// `closed`
    #[wire = "closed"]
    Closed,
    /// `all`
    #[wire = "all"]
    All,
}

/// Specifies when to run a DTBP check for an account.
///
/// NOTE: These values are currently the same as `PDTCheck` however they are not guaranteed to be in sync the future
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/trading-configurations/#attributes>.
#[wire_enum(sorted)]
pub enum DTBPCheck {
    /// `both`
    #[wire = "both"]
    Both,
    /// `entry`
    #[wire = "entry"]
    Entry,
    /// `exit`
    #[wire = "exit"]
    Exit,
}

/// Specifies when to run a PDT check for an account.
///
/// NOTE: These values are currently the same as `DTBPCheck` however they are not guaranteed to be in sync the future
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/trading-configurations/#attributes>.
#[wire_enum(sorted)]
pub enum PDTCheck {
    /// `both`
    #[wire = "both"]
    Both,
    /// `entry`
    #[wire = "entry"]
    Entry,
    /// `exit`
    #[wire = "exit"]
    Exit,
}

/// Used for controlling when an Account will receive a trade confirmation email.
///
/// See <https://docs.alpaca.markets/reference/getaccountconfig>.
#[wire_enum(sorted)]
pub enum TradeConfirmationEmail {
    /// `all`
    #[wire = "all"]
    All,
    /// `none`
    #[wire = "none"]
    None,
}

/// Represents the exercise style of options
#[wire_enum(sorted)]
pub enum ExerciseStyle {
    /// `american`
    #[wire = "american"]
    American,
    /// `european`
    #[wire = "european"]
    European,
}

/// Represents the category of an Activity
#[wire_enum]
pub enum ActivityCategory {
    /// `trade_activity`
    #[wire = "trade_activity"]
    TradeActivity,
    /// `non_trade_activity`
    #[wire = "non_trade_activity"]
    NonTradeActivity,
}

/// Represents what side this order was executed on.
#[wire_enum]
pub enum PositionIntent {
    /// `buy_to_open`
    #[wire = "buy_to_open"]
    BuyToOpen,
    /// `buy_to_close`
    #[wire = "buy_to_close"]
    BuyToClose,
    /// `sell_to_open`
    #[wire = "sell_to_open"]
    SellToOpen,
    /// `sell_to_close`
    #[wire = "sell_to_close"]
    SellToClose,
}
