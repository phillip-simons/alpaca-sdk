//! The broker API: accounts, funding, journals, documents, and rebalancing.
//!
//! Authenticates with HTTP basic auth rather than the `APCA-*` headers every
//! other client uses, and most routes act on behalf of a specific account.
//!
//! # What is verified, and what is not
//!
//! The core of this API — accounts, funding, journals, documents, rebalancing,
//! trading on behalf of an account — is checked against captured payloads.
//!
//! Instant funding, JIT, FPSL, funding wallets, IPOs, reporting, OAuth,
//! tokenization and the crypto wallets are not. Those came from the published
//! reference and the vendored specs, and have never met a real response, because
//! this account has no broker sandbox key.
//!
//! That is not a reason to distrust them so much as a reason to expect the first
//! live payload to correct something. Treat a decode failure on one of these as
//! expected work rather than a regression, exactly as the `CIP*` models are
//! treated. The one family with real payloads behind it is
//! [`fixed_income`], harvested from the Go SDK's tests.

mod client;
mod enums;
mod events;
pub mod fixed_income;
pub mod fpsl;
pub mod funding_wallet;
pub mod instant_funding;
pub mod ipos;
pub mod jit;
mod models;
pub mod oauth;
pub mod onboarding;
pub mod reporting;
mod requests;
pub mod settlements;

pub use client::{BrokerClient, DOCUMENT_UPLOAD_LIMIT};
pub use enums::*;
pub use events::{BrokerEvent, GetEventsRequest};
pub use fixed_income::*;
pub use fpsl::*;
pub use funding_wallet::*;
pub use instant_funding::*;
pub use ipos::*;
pub use jit::*;
pub use models::*;
pub use oauth::*;
pub use onboarding::*;
pub use reporting::*;
pub use requests::*;
pub use settlements::*;
