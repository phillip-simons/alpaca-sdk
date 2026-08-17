//! The string enums the market data API accepts and sends.
//!
//! `Exchange` here is the single-letter tape codes the data API sends, which is
//! not what the specs' schema of that name holds — those are venue names. Same
//! word, different vocabulary; `just enums-drift` records it as a decision so
//! the two are not "reconciled" into one wrong list.
//!
//! Hand-written `impl` blocks belong in the sibling `enums_ext.rs`.

use crate::types::wire::wire_enum;

/// The exchanges that provide data feeds to Alpaca.
#[wire_enum]
pub enum Exchange {
    /// Cboe BZ
    #[wire = "Z"]
    Z,
    /// International Securities Exchange
    #[wire = "I"]
    I,
    /// Chicago Stock Exchange
    #[wire = "M"]
    M,
    /// Members Exchange
    #[wire = "U"]
    U,
    /// Long Term Stock Exchange
    #[wire = "L"]
    L,
    /// CBOE
    #[wire = "W"]
    W,
    /// NASDAQ OMX PSX
    #[wire = "X"]
    X,
    /// NASDAQ OMX BX
    #[wire = "B"]
    B,
    /// FINRA ADF
    #[wire = "D"]
    D,
    /// Cboe EDGA
    #[wire = "J"]
    J,
    /// NYSE Arca
    #[wire = "P"]
    P,
    /// NASDAQ OMX
    #[wire = "Q"]
    Q,
    /// NASDAQ Small Cap
    #[wire = "S"]
    S,
    /// IEX
    #[wire = "V"]
    V,
    /// NYSE American (AMEX)
    #[wire = "A"]
    A,
    /// Market Independent
    #[wire = "E"]
    E,
    /// New York Stock Exchange
    #[wire = "N"]
    N,
    /// NASDAQ Int
    #[wire = "T"]
    T,
    /// Cboe BYX
    #[wire = "Y"]
    Y,
    /// National Stock Exchange
    #[wire = "C"]
    C,
    /// MIAX
    #[wire = "H"]
    H,
    /// Cboe EDGX
    #[wire = "K"]
    K,
}

/// Equity market data feeds. OTC and SIP are available with premium data subscriptions.
#[wire_enum]
pub enum DataFeed {
    /// Investor's exchange data feed
    #[wire = "iex"]
    Iex,
    /// Securities Information Processor feed
    #[wire = "sip"]
    Sip,
    /// SIP data with a 15 minute delay
    #[wire = "delayed_sip"]
    DelayedSip,
    /// Over the counter feed
    #[wire = "otc"]
    Otc,
    /// Blue Ocean, overnight US trading data
    #[wire = "boats"]
    Boats,
    /// derived overnight US trading data
    #[wire = "overnight"]
    Overnight,
}

/// Data normalization based on types of corporate actions.
#[wire_enum]
pub enum Adjustment {
    /// Unadjusted data
    #[wire = "raw"]
    Raw,
    /// Stock-split adjusted data
    #[wire = "split"]
    Split,
    /// Dividend adjusted data
    #[wire = "dividend"]
    Dividend,
    /// Data adjusted for all corporate actions
    #[wire = "all"]
    All,
}

/// Crypto location
#[wire_enum(sorted)]
pub enum CryptoFeed {
    /// United States crypto feed
    #[wire = "us"]
    Us,
}

/// The source feed of the data.
/// `opra` requires subscription
#[wire_enum]
pub enum OptionsFeed {
    /// Options Price Reporting Authority
    #[wire = "opra"]
    Opra,
    /// Indicative data
    #[wire = "indicative"]
    Indicative,
}

/// Most actives possible filters.
#[wire_enum]
pub enum MostActivesBy {
    /// `volume`
    #[wire = "volume"]
    Volume,
    /// `trades`
    #[wire = "trades"]
    Trades,
}

/// Most actives possible filters.
#[wire_enum]
pub enum MarketType {
    /// `stocks`
    #[wire = "stocks"]
    Stocks,
    /// `crypto`
    #[wire = "crypto"]
    Crypto,
}

/// The `NewsImageSize` values accepted by the API.
#[wire_enum]
pub enum NewsImageSize {
    /// `thumb`
    #[wire = "thumb"]
    Thumb,
    /// `small`
    #[wire = "small"]
    Small,
    /// `large`
    #[wire = "large"]
    Large,
}

/// The type of corporate action.
/// ref. <https://docs.alpaca.markets/reference/corporateactions-1>
#[wire_enum]
pub enum CorporateActionsType {
    /// Reverse split
    #[wire = "reverse_split"]
    ReverseSplit,
    /// Forward split
    #[wire = "forward_split"]
    ForwardSplit,
    /// Unit split
    #[wire = "unit_split"]
    UnitSplit,
    /// Cash dividend
    #[wire = "cash_dividend"]
    CashDividend,
    /// Stock dividend
    #[wire = "stock_dividend"]
    StockDividend,
    /// Spin off
    #[wire = "spin_off"]
    SpinOff,
    /// Cash merger
    #[wire = "cash_merger"]
    CashMerger,
    /// Stock merger
    #[wire = "stock_merger"]
    StockMerger,
    /// Stock and cash merger
    #[wire = "stock_and_cash_merger"]
    StockAndCashMerger,
    /// Redemption
    #[wire = "redemption"]
    Redemption,
    /// Name change
    #[wire = "name_change"]
    NameChange,
    /// Worthless removal
    #[wire = "worthless_removal"]
    WorthlessRemoval,
    /// Rights distribution
    #[wire = "rights_distribution"]
    RightsDistribution,
}
