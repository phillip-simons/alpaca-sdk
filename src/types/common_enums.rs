//! Enums shared by more than one API surface, from `alpaca/common/enums.py`.
//!
//! `PaginationType` is deliberately absent: its three modes are expressed here as
//! a `Stream` plus `try_collect`, so an enum selecting between them would have
//! nothing to select.

use crate::types::wire::wire_enum;

wire_enum! {
    /// Sort direction for endpoints that accept one.
    pub enum Sort {
        /// Oldest first.
        Asc => "asc",
        /// Newest first.
        Desc => "desc",
    }
}

wire_enum! {
    /// Currencies supported for local currency trading.
    ///
    /// See <https://alpaca.markets/support/local-currency-trading-faq>.
    pub enum SupportedCurrencies {
        /// United States dollar.
        Usd => "USD",
        /// Pound sterling.
        Gbp => "GBP",
        /// Swiss franc.
        Chf => "CHF",
        /// Euro.
        Eur => "EUR",
        /// Canadian dollar.
        Cad => "CAD",
        /// Japanese yen.
        Jpy => "JPY",
        /// Turkish lira.
        Try => "TRY",
        /// Australian dollar.
        Aud => "AUD",
        /// Czech koruna.
        Czk => "CZK",
        /// Swedish krona.
        Sek => "SEK",
        /// Danish krone.
        Dkk => "DKK",
        /// Singapore dollar.
        Sgd => "SGD",
        /// Hong Kong dollar.
        Hkd => "HKD",
        /// Hungarian forint.
        Huf => "HUF",
        /// New Zealand dollar.
        Nzd => "NZD",
        /// Norwegian krone.
        Nok => "NOK",
        /// Polish złoty.
        Pln => "PLN",
    }
}
