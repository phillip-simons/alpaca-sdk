//! The broker API: accounts, funding, journals, documents, and rebalancing.
//!
//! Authenticates with HTTP basic auth rather than the `APCA-*` headers every
//! other client uses, and most routes act on behalf of a specific account.

mod client;
mod enums;
mod events;
mod models;
mod requests;

pub use client::BrokerClient;
pub use enums::*;
pub use events::{BrokerEvent, GetEventsRequest};
pub use models::*;
pub use requests::*;
