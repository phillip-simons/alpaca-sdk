//! The market data API: historical bars, quotes, trades, and live streams.

mod corporate_actions;
mod enums;
mod events;
#[cfg(feature = "polars")]
mod frame;
mod historical;
pub mod live;
mod meta;
mod models;
mod pagination;
mod requests;
mod timeframe;

pub use corporate_actions::*;
pub use enums::*;
pub use events::{CorporateActionEventType, CorporateActionEventsRequest, CorporateActionRegion};
#[cfg(feature = "polars")]
#[cfg_attr(docsrs, doc(cfg(feature = "polars")))]
pub use frame::ToFrame;
pub use historical::{
    CorporateActionsClient, CryptoHistoricalDataClient, ForexDataClient, LogoClient, NewsClient,
    OptionHistoricalDataClient, ScreenerClient, StockHistoricalDataClient,
};
pub use live::{
    Channel, CryptoDataStream, DEFAULT_STABLE_SESSION, DataStream, NewsDataStream,
    OptionDataStream, StockDataStream, StreamConfig, StreamError, StreamMessage, SubscriptionSet,
    Subscriptions,
};
pub use meta::{Codes, Tape, TickType};
// Shared with the broker API, which documents the same route, so it lives in
// `types` and is re-exported here where market data callers look for it.
pub use crate::types::LogoRequest;
pub use models::*;
pub use requests::*;
pub use timeframe::{TimeFrame, TimeFrameUnit};
