//! The string enums the market data API accepts and sends.
//!
//! `Exchange` here is the single-letter tape codes the data API sends, which is
//! not what the specs' schema of that name holds — those are venue names. Same
//! word, different vocabulary; `just enums-drift` records it as a decision so
//! the two are not "reconciled" into one wrong list.
//!
//! Hand-written `impl` blocks belong in the sibling `enums_ext.rs`.

use crate::types::wire::wire_enum;
wire_enum! {
    /// The exchanges that provide data feeds to Alpaca.
    pub enum Exchange {
        /// Cboe BZ
        Z => "Z",
        /// International Securities Exchange
        I => "I",
        /// Chicago Stock Exchange
        M => "M",
        /// Members Exchange
        U => "U",
        /// Long Term Stock Exchange
        L => "L",
        /// CBOE
        W => "W",
        /// NASDAQ OMX PSX
        X => "X",
        /// NASDAQ OMX BX
        B => "B",
        /// FINRA ADF
        D => "D",
        /// Cboe EDGA
        J => "J",
        /// NYSE Arca
        P => "P",
        /// NASDAQ OMX
        Q => "Q",
        /// NASDAQ Small Cap
        S => "S",
        /// IEX
        V => "V",
        /// NYSE American (AMEX)
        A => "A",
        /// Market Independent
        E => "E",
        /// New York Stock Exchange
        N => "N",
        /// NASDAQ Int
        T => "T",
        /// Cboe BYX
        Y => "Y",
        /// National Stock Exchange
        C => "C",
        /// MIAX
        H => "H",
        /// Cboe EDGX
        K => "K",
    }
}

wire_enum! {
    /// Equity market data feeds. OTC and SIP are available with premium data subscriptions.
    pub enum DataFeed {
        /// Investor's exchange data feed
        Iex => "iex",
        /// Securities Information Processor feed
        Sip => "sip",
        /// SIP data with a 15 minute delay
        DelayedSip => "delayed_sip",
        /// Over the counter feed
        Otc => "otc",
        /// Blue Ocean, overnight US trading data
        Boats => "boats",
        /// derived overnight US trading data
        Overnight => "overnight",
    }
}

wire_enum! {
    /// Data normalization based on types of corporate actions.
    pub enum Adjustment {
        /// Unadjusted data
        Raw => "raw",
        /// Stock-split adjusted data
        Split => "split",
        /// Dividend adjusted data
        Dividend => "dividend",
        /// Data adjusted for all corporate actions
        All => "all",
    }
}

wire_enum! {
    /// Crypto location
    pub enum CryptoFeed {
        /// United States crypto feed
        Us => "us",
    }
}

wire_enum! {
    /// The source feed of the data.
    /// `opra` requires subscription
    pub enum OptionsFeed {
        /// Options Price Reporting Authority
        Opra => "opra",
        /// Indicative data
        Indicative => "indicative",
    }
}

wire_enum! {
    /// Most actives possible filters.
    pub enum MostActivesBy {
        /// `volume`
        Volume => "volume",
        /// `trades`
        Trades => "trades",
    }
}

wire_enum! {
    /// Most actives possible filters.
    pub enum MarketType {
        /// `stocks`
        Stocks => "stocks",
        /// `crypto`
        Crypto => "crypto",
    }
}

wire_enum! {
    /// The `NewsImageSize` values accepted by the API.
    pub enum NewsImageSize {
        /// `thumb`
        Thumb => "thumb",
        /// `small`
        Small => "small",
        /// `large`
        Large => "large",
    }
}

wire_enum! {
    /// The type of corporate action.
    /// ref. <https://docs.alpaca.markets/reference/corporateactions-1>
    pub enum CorporateActionsType {
        /// Reverse split
        ReverseSplit => "reverse_split",
        /// Forward split
        ForwardSplit => "forward_split",
        /// Unit split
        UnitSplit => "unit_split",
        /// Cash dividend
        CashDividend => "cash_dividend",
        /// Stock dividend
        StockDividend => "stock_dividend",
        /// Spin off
        SpinOff => "spin_off",
        /// Cash merger
        CashMerger => "cash_merger",
        /// Stock merger
        StockMerger => "stock_merger",
        /// Stock and cash merger
        StockAndCashMerger => "stock_and_cash_merger",
        /// Redemption
        Redemption => "redemption",
        /// Name change
        NameChange => "name_change",
        /// Worthless removal
        WorthlessRemoval => "worthless_removal",
        /// Rights distribution
        RightsDistribution => "rights_distribution",
    }
}
