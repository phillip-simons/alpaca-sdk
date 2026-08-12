//! The broker API: accounts, funding, journals, documents, and rebalancing.
//!
//! Authenticates with HTTP basic auth rather than the `APCA-*` headers every
//! other client uses, and most routes act on behalf of a specific account.

mod client;
mod enums;
mod models;

pub use client::BrokerClient;
pub use enums::*;
pub use models::*;
