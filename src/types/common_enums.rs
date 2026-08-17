//! Enums shared by more than one API surface.
//!
//! They live here so a module can use one without depending on the feature that
//! would otherwise own it: `data` needs `ContractType` and must build with
//! `trading` off. Each is re-exported from the surface it belongs to, so the
//! public path is the one a caller would guess.
//!
//! `PaginationType` is deliberately absent: its three modes are expressed here as
//! a `Stream` plus `try_collect`, so an enum selecting between them would have
//! nothing to select.

use crate::types::wire::wire_enum;

/// Sort direction for endpoints that accept one.
#[wire_enum(sorted)]
pub enum Sort {
    /// Oldest first.
    #[wire = "asc"]
    Asc,
    /// Newest first.
    #[wire = "desc"]
    Desc,
}

/// Currencies supported for local currency trading.
///
/// See <https://docs.alpaca.markets/us/docs/local-currency-trading-lct>.
#[wire_enum]
pub enum SupportedCurrencies {
    /// United States dollar.
    #[wire = "USD"]
    Usd,
    /// Pound sterling.
    #[wire = "GBP"]
    Gbp,
    /// Swiss franc.
    #[wire = "CHF"]
    Chf,
    /// Euro.
    #[wire = "EUR"]
    Eur,
    /// Canadian dollar.
    #[wire = "CAD"]
    Cad,
    /// Japanese yen.
    #[wire = "JPY"]
    Jpy,
    /// Turkish lira.
    #[wire = "TRY"]
    Try,
    /// Australian dollar.
    #[wire = "AUD"]
    Aud,
    /// Czech koruna.
    #[wire = "CZK"]
    Czk,
    /// Swedish krona.
    #[wire = "SEK"]
    Sek,
    /// Danish krone.
    #[wire = "DKK"]
    Dkk,
    /// Singapore dollar.
    #[wire = "SGD"]
    Sgd,
    /// Hong Kong dollar.
    #[wire = "HKD"]
    Hkd,
    /// Hungarian forint.
    #[wire = "HUF"]
    Huf,
    /// New Zealand dollar.
    #[wire = "NZD"]
    Nzd,
    /// Norwegian krone.
    #[wire = "NOK"]
    Nok,
    /// Polish złoty.
    #[wire = "PLN"]
    Pln,
}

/// Whether an options contract is a call or a put.
#[wire_enum(sorted)]
pub enum ContractType {
    /// The right to buy the underlying.
    #[wire = "call"]
    Call,
    /// The right to sell the underlying.
    #[wire = "put"]
    Put,
}
