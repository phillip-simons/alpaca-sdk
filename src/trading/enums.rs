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
wire_enum! {
    /// Represents what kind of Activity an instance of `TradeActivity` or `NonTradeActivity` is.
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#enumactivitytype>.
    pub enum ActivityType {
        /// Order fills (both partial and full fills)
        Fill => "FILL",
        /// ACATS IN/OUT (Cash)
        Acatc => "ACATC",
        /// ACATS IN/OUT (Securities)
        Acats => "ACATS",
        /// Crypto fee
        Cfee => "CFEE",
        /// Capital gain distribution
        Cgd => "CGD",
        /// Cash in lieu of stock
        Cil => "CIL",
        /// Cash deposit(+)
        Csd => "CSD",
        /// Cash withdrawal(-)
        Csw => "CSW",
        /// Dividends
        Div => "DIV",
        /// Dividend (capital gain long term)
        Divcgl => "DIVCGL",
        /// Dividend (capital gain short term)
        Divcgs => "DIVCGS",
        /// Dividend fee
        Divfee => "DIVFEE",
        /// Dividend adjusted (Foreign Tax Withheld)
        Divft => "DIVFT",
        /// Dividend adjusted (NRA Withheld)
        Divnra => "DIVNRA",
        /// Dividend return of capital
        Divroc => "DIVROC",
        /// Dividend adjusted (Tefra Withheld)
        Divtw => "DIVTW",
        /// Dividend (tax exempt)
        Divtxex => "DIVTXEX",
        /// `DIVWH`. The specs do not describe this value.
        Divwh => "DIVWH",
        /// `EXTRD`. The specs do not describe this value.
        Extrd => "EXTRD",
        /// Fee denominated in USD
        Fee => "FEE",
        /// Free of Payment Transfers
        Fopt => "FOPT",
        /// `FXTRD`. The specs do not describe this value.
        Fxtrd => "FXTRD",
        /// Interest (credit/margin)
        Int => "INT",
        /// Interest adjusted (NRA Withheld)
        Intnra => "INTNRA",
        /// `INTPNL`. The specs do not describe this value.
        Intpnl => "INTPNL",
        /// Interest adjusted (Tefra Withheld)
        Inttw => "INTTW",
        /// Journal entry
        Jnl => "JNL",
        /// Journal entry (cash)
        Jnlc => "JNLC",
        /// Journal entry (stock)
        Jnls => "JNLS",
        /// Merger/Acquisition
        Ma => "MA",
        /// `MEM`. The specs do not describe this value.
        Mem => "MEM",
        /// Miscellaneous or rarely used activity types (All types except those in TRANS, DIV, or FILL)
        Misc => "MISC",
        /// Name change
        Nc => "NC",
        /// On chain transactions (blockchain deposits/withdrawals)
        Oct => "OCT",
        /// Option assignment
        Opasn => "OPASN",
        /// Option corporate action
        Opca => "OPCA",
        /// Option cash deliverable for non-standard contracts
        Opcsh => "OPCSH",
        /// Option exercise
        Opexc => "OPEXC",
        /// Option expiration
        Opexp => "OPEXP",
        /// Option trade
        Optrd => "OPTRD",
        /// Pass Thru Charge
        Ptc => "PTC",
        /// Pass Thru Rebate
        Ptr => "PTR",
        /// Reorg CA
        Reorg => "REORG",
        /// Stock spinoff
        Spin => "SPIN",
        /// Stock split
        Split => "SPLIT",
        /// `SWP`. The specs do not describe this value.
        Swp => "SWP",
        /// Cash transactions (both CSD and CSW)
        Trans => "TRANS",
        /// `VOF`. The specs do not describe this value.
        Vof => "VOF",
        /// `WH`. The specs do not describe this value.
        Wh => "WH",
    }
}

wire_enum! {
    /// Represents the type of `TradeActivity`.
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#attributes>.
    pub enum TradeActivityType {
        /// `partial_fill`
        PartialFill => "partial_fill",
        /// `fill`
        Fill => "fill",
    }
}

wire_enum! {
    /// Represents the status of a `NonTradeActivity`.
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/account-activities/#enumaccountactivity>.
    pub enum NonTradeActivityStatus {
        /// `executed`
        Executed => "executed",
        /// `correct`
        Correct => "correct",
        /// `canceled`
        Canceled => "canceled",
    }
}

wire_enum! {
    /// Represents what class of order this is.
    ///
    /// The order classes supported by Alpaca vary based on the order's security type.
    /// The following provides a comprehensive breakdown of the supported order classes for each category:
    /// - Equity trading: simple (or ""), oco, oto, bracket.
    /// - Options trading: simple (or ""), mleg (required for multi-leg complex options strategies).
    /// - Crypto trading: simple (or "").
    pub enum OrderClass {
        /// `simple`
        Simple => "simple",
        /// `mleg`
        Mleg => "mleg",
        /// `bracket`
        Bracket => "bracket",
        /// `oco`
        Oco => "oco",
        /// `oto`
        Oto => "oto",
    }
}

wire_enum! {
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
    pub enum OrderType {
        /// `market`
        Market => "market",
        /// `limit`
        Limit => "limit",
        /// `stop`
        Stop => "stop",
        /// `stop_limit`
        StopLimit => "stop_limit",
        /// `trailing_stop`
        TrailingStop => "trailing_stop",
    }
}

wire_enum! {
    /// Represents what side this order was executed on.
    pub enum OrderSide {
        /// Buy.
        Buy => "buy",
        /// Sell.
        Sell => "sell",
        /// Buy at or below the last sale price.
        BuyMinus => "buy_minus",
        /// Sell at or above the last sale price.
        SellPlus => "sell_plus",
        /// Sell short.
        SellShort => "sell_short",
        /// Sell short, exempt from the price test.
        SellShortExempt => "sell_short_exempt",
        /// Side not disclosed.
        Undisclosed => "undisclosed",
        /// A cross, where one broker is on both sides.
        Cross => "cross",
        /// A cross where the sell side is short.
        CrossShort => "cross_short",
    }
}

wire_enum! {
    /// Represents the various states an Order can be in.
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/orders/#order-status>.
    pub enum OrderStatus {
        /// `new`
        New => "new",
        /// `partially_filled`
        PartiallyFilled => "partially_filled",
        /// `filled`
        Filled => "filled",
        /// `done_for_day`
        DoneForDay => "done_for_day",
        /// `canceled`
        Canceled => "canceled",
        /// `expired`
        Expired => "expired",
        /// `replaced`
        Replaced => "replaced",
        /// `pending_cancel`
        PendingCancel => "pending_cancel",
        /// `pending_replace`
        PendingReplace => "pending_replace",
        /// `pending_review`
        PendingReview => "pending_review",
        /// `accepted`
        Accepted => "accepted",
        /// `pending_new`
        PendingNew => "pending_new",
        /// `accepted_for_bidding`
        AcceptedForBidding => "accepted_for_bidding",
        /// `stopped`
        Stopped => "stopped",
        /// `rejected`
        Rejected => "rejected",
        /// `suspended`
        Suspended => "suspended",
        /// `calculated`
        Calculated => "calculated",
        /// `held`
        Held => "held",
    }
}

wire_enum! {
    /// This represents the category to which the asset belongs to.
    /// It serves to identify the nature of the financial instrument, with options
    /// including "`us_equity`" for U.S. equities, "`us_option`" for U.S. options,
    /// and "crypto" for cryptocurrencies.
    pub enum AssetClass {
        /// A US-listed equity.
        UsEquity => "us_equity",
        /// A US-listed option contract.
        UsOption => "us_option",
        /// A cryptocurrency pair.
        Crypto => "crypto",
        /// A perpetual crypto future.
        CryptoPerp => "crypto_perp",
        /// The option chain on a US-listed equity.
        UsEquityChain => "us_equity_chain",
        /// A US market index.
        UsIndex => "us_index",
        /// An equity listed outside the US.
        GlobalEquity => "global_equity",
        /// A US treasury instrument.
        Treasury => "treasury",
        /// A US corporate bond.
        Corporate => "corporate",
        /// An indication of interest in an IPO.
        ///
        /// Distinct from the assets API's `attributes: ["ipo"]` flag, which marks
        /// an ordinary equity as IPO-eligible; this is the asset class itself.
        Ipo => "ipo",
    }
}

wire_enum! {
    /// Represents the various states for an Asset's lifecycle
    pub enum AssetStatus {
        /// `active`
        Active => "active",
        /// `inactive`
        Inactive => "inactive",
    }
}

wire_enum! {
    /// Represents the current exchanges Alpaca supports.
    pub enum AssetExchange {
        /// `AMEX`
        Amex => "AMEX",
        /// `ARCA`
        Arca => "ARCA",
        /// `ASCX`
        Ascx => "ASCX",
        /// `BATS`
        Bats => "BATS",
        /// `NYSE`
        Nyse => "NYSE",
        /// `NASDAQ`
        Nasdaq => "NASDAQ",
        /// `NYSEARCA`
        Nysearca => "NYSEARCA",
        /// `FTXU`
        Ftxu => "FTXU",
        /// `CBSE`
        Cbse => "CBSE",
        /// `GNSS`
        Gnss => "GNSS",
        /// `ERSX`
        Ersx => "ERSX",
        /// `OTC`
        Otc => "OTC",
        /// `CRYPTO`
        Crypto => "CRYPTO",
        /// The empty value.
        Empty => "",
    }
}

wire_enum! {
    /// Represents what side this position is.
    pub enum PositionSide {
        /// `short`
        Short => "short",
        /// `long`
        Long => "long",
    }
}

wire_enum! {
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
    pub enum TimeInForce {
        /// `day`
        Day => "day",
        /// `gtc`
        Gtc => "gtc",
        /// `opg`
        Opg => "opg",
        /// `cls`
        Cls => "cls",
        /// `ioc`
        Ioc => "ioc",
        /// `fok`
        Fok => "fok",
    }
}

wire_enum! {
    /// The general types of corporate action events.
    ///
    /// See <https://docs.alpaca.markets/docs/corporate-actions>.
    pub enum CorporateActionType {
        /// `dividend`
        Dividend => "dividend",
        /// `merger`
        Merger => "merger",
        /// `spinoff`
        Spinoff => "spinoff",
        /// `split`
        Split => "split",
    }
}

wire_enum! {
    /// The specific types of corporate actions. Each subtype is related to `CorporateActionType`.
    ///
    /// See <https://docs.alpaca.markets/docs/corporate-actions>.
    pub enum CorporateActionSubType {
        /// `cash`
        Cash => "cash",
        /// `stock`
        Stock => "stock",
        /// `merger_update`
        MergerUpdate => "merger_update",
        /// `merger_completion`
        MergerCompletion => "merger_completion",
        /// `spinoff`
        Spinoff => "spinoff",
        /// `stock_split`
        StockSplit => "stock_split",
        /// `unit_split`
        UnitSplit => "unit_split",
        /// `reverse_split`
        ReverseSplit => "reverse_split",
        /// `recapitalization`
        Recapitalization => "recapitalization",
    }
}

wire_enum! {
    /// The various statuses each brokerage account can take during its lifetime
    ///
    /// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#account-status>.
    pub enum AccountStatus {
        /// The account is closed.
        AccountClosed => "ACCOUNT_CLOSED",
        /// The account close is in progress.
        ///
        /// Listed by the trading spec, which describes no other value of this
        /// name; the reading here is the name's, not the spec's.
        AccountClosedPending => "ACCOUNT_CLOSED_PENDING",
        /// The account information is being updated.
        AccountUpdated => "ACCOUNT_UPDATED",
        /// The application requires manual action.
        ActionRequired => "ACTION_REQUIRED",
        /// The account is active for trading.
        Active => "ACTIVE",
        /// `AML_REVIEW`. The specs do not describe this value.
        AmlReview => "AML_REVIEW",
        /// The final account approval is pending.
        ApprovalPending => "APPROVAL_PENDING",
        /// The account application has been approved, and is waiting to become
        /// active.
        Approved => "APPROVED",
        /// `DISABLED`. The specs do not describe this value.
        Disabled => "DISABLED",
        /// `DISABLE_PENDING`. The specs do not describe this value.
        DisablePending => "DISABLE_PENDING",
        /// `EDITED`. The specs do not describe this value.
        Edited => "EDITED",
        /// The account is not set to trade the asset in question.
        Inactive => "INACTIVE",
        /// `KYC_SUBMITTED`. The specs do not describe this value.
        KycSubmitted => "KYC_SUBMITTED",
        /// `LIMITED`. The specs do not describe this value.
        Limited => "LIMITED",
        /// The account is onboarding.
        Onboarding => "ONBOARDING",
        /// `PAPER_ONLY`. The specs do not describe this value.
        PaperOnly => "PAPER_ONLY",
        /// `REAPPROVAL_PENDING`. The specs do not describe this value.
        ReapprovalPending => "REAPPROVAL_PENDING",
        /// The account application has been rejected.
        Rejected => "REJECTED",
        /// `RESUBMITTED`. The specs do not describe this value.
        Resubmitted => "RESUBMITTED",
        /// `SIGNED_UP`. The specs do not describe this value.
        SignedUp => "SIGNED_UP",
        /// The account application submission failed for some reason.
        SubmissionFailed => "SUBMISSION_FAILED",
        /// The account application has been submitted for review.
        Submitted => "SUBMITTED",
    }
}

wire_enum! {
    /// The `CorporateActionDateType` values accepted by the API.
    pub enum CorporateActionDateType {
        /// `declaration_date`
        DeclarationDate => "declaration_date",
        /// `ex_date`
        ExDate => "ex_date",
        /// `record_date`
        RecordDate => "record_date",
        /// `payable_date`
        PayableDate => "payable_date",
    }
}

wire_enum! {
    /// The `TradeEvent` values accepted by the API.
    pub enum TradeEvent {
        /// `accepted`
        Accepted => "accepted",
        /// `canceled`
        Canceled => "canceled",
        /// `expired`
        Expired => "expired",
        /// `fill`
        Fill => "fill",
        /// `new`
        New => "new",
        /// `partial_fill`
        PartialFill => "partial_fill",
        /// `pending_cancel`
        PendingCancel => "pending_cancel",
        /// `pending_new`
        PendingNew => "pending_new",
        /// `pending_replace`
        PendingReplace => "pending_replace",
        /// `rejected`
        Rejected => "rejected",
        /// `replaced`
        Replaced => "replaced",
        /// `restated`
        Restated => "restated",
    }
}

wire_enum! {
    /// The `QueryOrderStatus` values accepted by the API.
    pub enum QueryOrderStatus {
        /// `open`
        Open => "open",
        /// `closed`
        Closed => "closed",
        /// `all`
        All => "all",
    }
}

wire_enum! {
    /// Specifies when to run a DTBP check for an account.
    ///
    /// NOTE: These values are currently the same as `PDTCheck` however they are not guaranteed to be in sync the future
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/trading-configurations/#attributes>.
    pub enum DTBPCheck {
        /// `both`
        Both => "both",
        /// `entry`
        Entry => "entry",
        /// `exit`
        Exit => "exit",
    }
}

wire_enum! {
    /// Specifies when to run a PDT check for an account.
    ///
    /// NOTE: These values are currently the same as `DTBPCheck` however they are not guaranteed to be in sync the future
    ///
    /// See <https://docs.alpaca.markets/docs/api-references/broker-api/trading/trading-configurations/#attributes>.
    pub enum PDTCheck {
        /// `both`
        Both => "both",
        /// `entry`
        Entry => "entry",
        /// `exit`
        Exit => "exit",
    }
}

wire_enum! {
    /// Used for controlling when an Account will receive a trade confirmation email.
    ///
    /// See <https://docs.alpaca.markets/reference/getaccountconfig>.
    pub enum TradeConfirmationEmail {
        /// `all`
        All => "all",
        /// `none`
        None => "none",
    }
}

wire_enum! {
    /// Represents the exercise style of options
    pub enum ExerciseStyle {
        /// `american`
        American => "american",
        /// `european`
        European => "european",
    }
}

wire_enum! {
    /// Represents the category of an Activity
    pub enum ActivityCategory {
        /// `trade_activity`
        TradeActivity => "trade_activity",
        /// `non_trade_activity`
        NonTradeActivity => "non_trade_activity",
    }
}

wire_enum! {
    /// Represents what side this order was executed on.
    pub enum PositionIntent {
        /// `buy_to_open`
        BuyToOpen => "buy_to_open",
        /// `buy_to_close`
        BuyToClose => "buy_to_close",
        /// `sell_to_open`
        SellToOpen => "sell_to_open",
        /// `sell_to_close`
        SellToClose => "sell_to_close",
    }
}
