//! [Crypto funding](https://docs.alpaca.markets/us/reference/listcryptofundingwallets):
//! deposit wallets, on-chain transfers, and the withdrawal allowlist.
//!
//! No captured payload exists, so these models follow the published reference
//! and are unverified against a live response.
//!
//! **The withdrawal route is missing on purpose.** `POST /v2/wallets/transfers`
//! is deprecated as of 2026-07-09 with a sunset of 2026-10-09, and the
//! reference's replacement is the Alpaca web application rather than another
//! route — so there is nothing to point a method at. The reason is recorded in
//! `SKIP` in `scripts/coverage.py`. Everything on the read side is here, and the
//! broker API's own equivalent of the withdrawal is *not* deprecated and *is*
//! implemented.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::wire::wire_enum;

wire_enum! {
    /// The chain a wallet or address is on.
    pub enum CryptoChain {
        /// Solana.
        Sol => "SOL",
        /// Ethereum.
        Eth => "ETH",
        /// Bitcoin.
        Btc => "BTC",
        /// XRP Ledger.
        Xrp => "XRP",
        /// Arbitrum.
        Arb => "ARB",
    }
}

wire_enum! {
    /// The network a wallet is on, where it differs from the chain.
    pub enum CryptoNetwork {
        /// Ethereum.
        Ethereum => "ethereum",
        /// Solana.
        Solana => "solana",
    }
}

wire_enum! {
    /// Which way a transfer moves.
    pub enum TransferDirection {
        /// Into the Alpaca account.
        Incoming => "INCOMING",
        /// Out of it.
        Outgoing => "OUTGOING",
    }
}

wire_enum! {
    /// Where a crypto transfer stands.
    ///
    /// Upper-case on the wire, unlike the fiat transfer statuses.
    pub enum CryptoTransferStatus {
        /// In flight.
        Processing => "PROCESSING",
        /// Failed.
        Failed => "FAILED",
        /// Settled.
        Complete => "COMPLETE",
    }
}

wire_enum! {
    /// Whether an allowlisted address may be withdrawn to yet.
    pub enum WhitelistStatus {
        /// Usable.
        Approved => "APPROVED",
        /// Still in its cooling-off period.
        Pending => "PENDING",
    }
}

/// A deposit wallet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoWallet {
    /// The on-chain address to deposit to.
    #[serde(default)]
    pub address: Option<String>,
    /// The chain it is on.
    #[serde(default)]
    pub chain: Option<String>,
    /// When Alpaca created it.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// An on-chain transfer into or out of an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoTransfer {
    /// Alpaca's identifier for the transfer.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The asset moved.
    #[serde(default)]
    pub asset: Option<String>,
    /// How much.
    #[serde(default)]
    pub amount: Option<Decimal>,
    /// What it was worth in USD.
    #[serde(default)]
    pub usd_value: Option<Decimal>,
    /// The chain.
    #[serde(default)]
    pub chain: Option<String>,
    /// Which way it moved.
    #[serde(default)]
    pub direction: Option<TransferDirection>,
    /// Where it stands.
    #[serde(default)]
    pub status: Option<CryptoTransferStatus>,
    /// The sending address.
    #[serde(default)]
    pub from_address: Option<String>,
    /// The receiving address.
    #[serde(default)]
    pub to_address: Option<String>,
    /// Alpaca's fee.
    #[serde(default)]
    pub fees: Option<Decimal>,
    /// The chain's own fee.
    #[serde(default)]
    pub network_fee: Option<Decimal>,
    /// The on-chain transaction.
    #[serde(default)]
    pub tx_hash: Option<String>,
    /// When the transfer was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// An address a withdrawal may be sent to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhitelistedAddress {
    /// Alpaca's identifier for the entry.
    #[serde(default)]
    pub id: Option<String>,
    /// The address.
    #[serde(default)]
    pub address: Option<String>,
    /// The asset it may receive.
    #[serde(default)]
    pub asset: Option<String>,
    /// The chain.
    #[serde(default)]
    pub chain: Option<String>,
    /// Whether it is usable yet.
    #[serde(default)]
    pub status: Option<WhitelistStatus>,
    /// When it was added.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// What a proposed transfer would cost in gas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferFeeEstimate {
    /// Alpaca's fee.
    #[serde(default)]
    pub fee: Option<Decimal>,
    /// The chain's own fee.
    #[serde(default)]
    pub network_fee: Option<Decimal>,
}

/// Filters for listing deposit wallets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetCryptoWalletsRequest {
    /// Only wallets for this asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Only wallets on this chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<CryptoChain>,
    /// Only wallets on this network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<CryptoNetwork>,
}

impl GetCryptoWalletsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only wallets for this asset.
    #[must_use]
    pub fn asset(mut self, asset: impl Into<String>) -> Self {
        self.asset = Some(asset.into());
        self
    }

    /// Only wallets on this chain.
    #[must_use]
    pub fn chain(mut self, chain: CryptoChain) -> Self {
        self.chain = Some(chain);
        self
    }
}

/// A request to allowlist a withdrawal address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateWhitelistedAddressRequest {
    /// The address to allow.
    pub address: String,
    /// The asset it may receive.
    pub asset: String,
    /// The chain it is on.
    pub chain: CryptoChain,
}

impl CreateWhitelistedAddressRequest {
    /// Allowlists `address` for `asset` on `chain`.
    pub fn new(address: impl Into<String>, asset: impl Into<String>, chain: CryptoChain) -> Self {
        Self {
            address: address.into(),
            asset: asset.into(),
            chain,
        }
    }
}

/// A request for a transfer's estimated gas fee.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransferFeeEstimateRequest {
    /// The asset to move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// The sending address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_address: Option<String>,
    /// The receiving address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_address: Option<String>,
    /// How much to move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Decimal>,
}

impl TransferFeeEstimateRequest {
    /// An estimate for moving `amount` of `asset` between two addresses.
    pub fn new(
        asset: impl Into<String>,
        from_address: impl Into<String>,
        to_address: impl Into<String>,
        amount: Decimal,
    ) -> Self {
        Self {
            asset: Some(asset.into()),
            from_address: Some(from_address.into()),
            to_address: Some(to_address.into()),
            amount: Some(amount),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_transfer_statuses_are_upper_case_on_the_wire() {
        // Unlike the fiat transfer statuses, which are lower-case. Two enums,
        // two conventions, and the wire decides.
        assert_eq!(CryptoTransferStatus::Complete.as_str(), "COMPLETE");
        assert_eq!(TransferDirection::Incoming.as_str(), "INCOMING");
    }

    #[test]
    fn money_fields_are_decimals_because_they_cross_the_wire_as_strings() {
        let transfer: CryptoTransfer = serde_json::from_value(serde_json::json!({
            "amount": "0.5",
            "usd_value": "1234.56",
            "network_fee": "0.0001",
            "status": "COMPLETE",
        }))
        .unwrap();

        assert_eq!(transfer.amount, Some(Decimal::new(5, 1)));
        assert_eq!(transfer.usd_value, Some(Decimal::new(123_456, 2)));
    }
}
