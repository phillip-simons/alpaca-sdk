//! [Tokenized assets](https://docs.alpaca.markets/us/reference/posttokenizationmint):
//! minting a position onto a chain, and tracking the request.
//!
//! No captured payload exists for any of these routes, so the models follow the
//! published reference and are unverified against a live response.
//!
//! The broker API carries the same four routes under an account id; see
//! `broker::BrokerClient`. The models are shared.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::wire::wire_enum;

wire_enum! {
    /// Who issues the token.
    pub enum TokenizationIssuer {
        /// xStocks.
        Xstocks => "xstocks",
        /// St0x.
        St0x => "st0x",
    }
}

wire_enum! {
    /// The chain a token lives on.
    pub enum TokenizationNetwork {
        /// Solana.
        Solana => "solana",
        /// Arbitrum.
        Arbitrum => "arbitrum",
        /// Ethereum.
        Ethereum => "ethereum",
        /// BNB Chain.
        Binance => "binance",
        /// Base.
        Base => "base",
        /// TON.
        Ton => "ton",
        /// Tron.
        Tron => "tron",
        /// Mantle.
        Mantle => "mantle",
        /// Cronos.
        Cronos => "cronos",
        /// `HyperEVM`.
        HyperEvm => "hyperevm",
    }
}

wire_enum! {
    /// Where a tokenization request stands.
    pub enum TokenizationStatus {
        /// Submitted and not yet settled.
        Pending => "pending",
        /// Refused.
        Rejected => "rejected",
        /// Settled.
        Completed => "completed",
    }
}

wire_enum! {
    /// Which direction a tokenization request goes.
    pub enum TokenizationType {
        /// Position to token.
        Mint => "mint",
        /// Token back to position.
        Redeem => "redeem",
    }
}

/// A tokenization request and its status.
///
/// The response to a mint carries a narrower set of these fields than a listed
/// request does, so all but the core are optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenizationRequest {
    /// Alpaca's identifier for the request.
    pub tokenization_request_id: String,
    /// Whether this mints or redeems. Absent on a mint response, which is one.
    #[serde(rename = "type", default)]
    pub request_type: Option<TokenizationType>,
    /// Where the request stands.
    pub status: TokenizationStatus,
    /// The position's symbol.
    pub underlying_symbol: String,
    /// The token's symbol.
    pub token_symbol: String,
    /// How many units.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// Who issues the token.
    pub issuer: TokenizationIssuer,
    /// The chain.
    pub network: TokenizationNetwork,
    /// The wallet the token goes to.
    #[serde(default)]
    pub wallet_address: Option<String>,
    /// Fees charged.
    #[serde(default, with = "crate::types::option_decimal")]
    pub fees: Option<Decimal>,
    /// The on-chain transaction, once there is one.
    #[serde(default)]
    pub tx_hash: Option<String>,
    /// The caller's own identifier for the request.
    #[serde(default)]
    pub client_request_id: Option<String>,
    /// The issuer's identifier for the request.
    #[serde(default)]
    pub issuer_request_id: Option<String>,
    /// The account, for broker callers.
    #[serde(default)]
    pub account: Option<String>,
    /// The account id, for broker callers.
    #[serde(default)]
    pub client_account_id: Option<Uuid>,
    /// The correspondent's own account identifier.
    #[serde(default)]
    pub client_external_account_id: Option<String>,
    /// The issuer's account.
    #[serde(default)]
    pub issuer_account: Option<String>,
    /// When the request was made.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A request to mint a tokenized asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MintTokenRequest {
    /// The position to tokenize.
    pub underlying_symbol: String,
    /// How many units.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// Who issues the token.
    pub issuer: TokenizationIssuer,
    /// The chain to mint on.
    pub network: TokenizationNetwork,
    /// The wallet to mint into.
    pub wallet_address: String,
}

impl MintTokenRequest {
    /// Mints `qty` of `underlying_symbol` to `wallet_address`.
    #[must_use]
    pub fn new(
        underlying_symbol: impl Into<String>,
        qty: Decimal,
        issuer: TokenizationIssuer,
        network: TokenizationNetwork,
        wallet_address: impl Into<String>,
    ) -> Self {
        Self {
            underlying_symbol: underlying_symbol.into(),
            qty,
            issuer,
            network,
            wallet_address: wallet_address.into(),
        }
    }

    /// The one check a request cannot pass without contradicting itself.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if `qty`
    /// is not positive.
    pub fn validate(&self) -> crate::Result<()> {
        if self.qty <= Decimal::ZERO {
            return Err(crate::Error::InvalidRequest(
                "qty must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for listing tokenization requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GetTokenizationRequestsRequest {
    /// Only mints, or only redemptions.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub request_type: Option<TokenizationType>,
    /// Only requests in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TokenizationStatus>,
    /// Only requests for this position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlying_symbol: Option<String>,
    /// Only requests through this issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<TokenizationIssuer>,
    /// Only requests on this chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<TokenizationNetwork>,
    /// Only requests made after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Only requests made before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<DateTime<Utc>>,
}

impl GetTokenizationRequestsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only mints, or only redemptions.
    #[must_use]
    pub fn request_type(mut self, request_type: TokenizationType) -> Self {
        self.request_type = Some(request_type);
        self
    }

    /// Only requests in this state.
    #[must_use]
    pub fn status(mut self, status: TokenizationStatus) -> Self {
        self.status = Some(status);
        self
    }
}

/// A lookup by the caller's own request id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ByClientRequestId {
    /// The identifier the caller sent with the request.
    pub client_request_id: String,
}

impl ByClientRequestId {
    /// A lookup for `client_request_id`.
    #[must_use]
    pub fn new(client_request_id: impl Into<String>) -> Self {
        Self {
            client_request_id: client_request_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mint_response_decodes_without_the_listing_only_fields() {
        // The mint response is a strict subset of the listed request, and
        // requiring the wider shape would fail on the narrower one.
        let request: TokenizationRequest = serde_json::from_value(serde_json::json!({
            "tokenization_request_id": "abc",
            "status": "pending",
            "underlying_symbol": "AAPL",
            "token_symbol": "AAPLx",
            "qty": "1.5",
            "issuer": "xstocks",
            "network": "solana",
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();

        assert_eq!(request.status, TokenizationStatus::Pending);
        assert_eq!(request.request_type, None);
        assert_eq!(request.wallet_address, None);
    }

    #[test]
    fn the_direction_stays_on_the_wire_as_type() {
        // `request_type` is a Rust-side name only: Alpaca sends and expects
        // `type`, and a rename that leaked would silently stop decoding it.
        let request: TokenizationRequest = serde_json::from_value(serde_json::json!({
            "tokenization_request_id": "abc",
            "type": "redeem",
            "status": "pending",
            "underlying_symbol": "AAPL",
            "token_symbol": "AAPLx",
            "qty": "1.5",
            "issuer": "xstocks",
            "network": "solana",
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();
        assert_eq!(request.request_type, Some(TokenizationType::Redeem));

        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["type"], "redeem");
        assert!(encoded.get("request_type").is_none(), "{encoded}");
    }

    #[test]
    fn a_zero_mint_is_refused_before_it_is_sent() {
        let request = MintTokenRequest::new(
            "AAPL",
            Decimal::ZERO,
            TokenizationIssuer::Xstocks,
            TokenizationNetwork::Solana,
            "wallet",
        );
        assert!(request.validate().is_err());
    }
}
