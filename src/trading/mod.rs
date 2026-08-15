//! The trading API: orders, positions, assets, watchlists, and account state.
//!
//! Most of this is checked against captured responses. Four corners of it are
//! not, because no captured payload exists for them — securities lending
//! locates ([`Locate`]), the per-market calendar ([`MarketCalendar`]), tokenized
//! assets ([`TokenizationRequest`]) and crypto funding ([`CryptoWallet`]). Those
//! models follow the published reference, and the first real response is what
//! will confirm them.

pub(crate) mod client;
mod enums;
mod enums_ext;
mod locates;
mod markets;
mod models;
mod requests;
mod stream;
mod tokenization;
mod wallets;

pub use client::TradingClient;
pub use enums::*;
pub use locates::*;
pub use markets::*;
pub use tokenization::*;
pub use wallets::*;
// Lives in `types` so `data` can use it with this feature off; re-exported here
// because upstream owns it and callers expect it at this path.
pub use crate::types::ContractType;
pub use models::*;
pub use requests::*;
pub use stream::{DEFAULT_STABLE_SESSION, TradeStreamMessage, TradingStream};
