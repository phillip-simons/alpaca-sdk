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

use crate::error::{Error, Result};
use crate::types::Validated;
use crate::types::setters::Setters;
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
///
/// The broker API carries the same four routes under an account id; see
/// `broker::BrokerClient`. The models are shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenizationRequest {
    /// Alpaca's identifier for the request.
    ///
    /// A `String`, not a `Uuid`: the response schema calls this `type: string`
    /// with no format, and a `Uuid` here would turn a value that is not one
    /// into [`Error::Decode`](crate::Error::Decode) for the whole response.
    /// The callback body and the path parameter do say `format: uuid`, and the
    /// caller supplies those, so [`TokenizationMintCallback`] parses one. The
    /// split matches the upstream schemas and is meant to stay.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
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
}

impl Validated for MintTokenRequest {
    /// The one check a request cannot pass without contradicting itself.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if `qty`
    /// is not positive.
    fn validate(&self) -> Result<()> {
        if self.qty <= Decimal::ZERO {
            return Err(Error::InvalidRequest(
                "qty must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Filters for listing tokenization requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
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
    #[setters(into)]
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
}

/// The exactly-one-of check both callback bodies carry.
///
/// The two types are separate because their wire shapes are, but the rule on
/// the account identifiers is one rule and reads better written once.
fn exactly_one_account(has_account_id: bool, has_external_id: bool) -> Result<()> {
    match (has_account_id, has_external_id) {
        (false, false) => Err(Error::InvalidRequest(
            "one of client_account_id and client_external_account_id must be set".to_owned(),
        )),
        (true, true) => Err(Error::InvalidRequest(
            "client_account_id and client_external_account_id are mutually exclusive".to_owned(),
        )),
        _ => Ok(()),
    }
}

/// An issuer's confirmation that a mint settled on chain.
///
/// The body of `POST /v1/accounts/{account_id}/tokenization/callback/mint`, on
/// `broker::BrokerClient::tokenization_mint_callback`. Sent *to* Alpaca by the
/// issuer, not by the Authorized Participant — which is why the account is
/// identified in the body rather than taken from the caller's credentials.
///
/// Exactly one of [`client_account_id`](Self::client_account_id) and
/// [`client_external_account_id`](Self::client_external_account_id) must be
/// set, and it must match the identifier used on the original mint request.
///
/// The redeem callback is a **different shape**, not a mirror of this one —
/// see [`TokenizationRedeemRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct TokenizationMintCallback {
    /// Alpaca's identifier for the request being confirmed.
    pub tokenization_request_id: Uuid,
    /// Transaction hash of the completed request on the blockchain.
    pub tx_hash: String,
    /// Alpaca account id of the Authorized Participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_account_id: Option<Uuid>,
    /// The customer's identifier on the issuer's platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub client_external_account_id: Option<String>,
    /// Alpaca's older alias for
    /// [`client_external_account_id`](Self::client_external_account_id).
    ///
    /// Deprecated since 2026-07-15 and sunsetting 2026-10-15. Still accepted
    /// until then, so the field is here; set the newer one instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub client_id: Option<String>,
    /// The chain the mint settled on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<TokenizationNetwork>,
    /// The wallet address that received the tokenized asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub wallet_address: Option<String>,
}

impl TokenizationMintCallback {
    /// Confirms `tokenization_request_id` settled as `tx_hash`.
    #[must_use]
    pub fn new(tokenization_request_id: Uuid, tx_hash: impl Into<String>) -> Self {
        Self {
            tokenization_request_id,
            tx_hash: tx_hash.into(),
            client_account_id: None,
            client_external_account_id: None,
            client_id: None,
            network: None,
            wallet_address: None,
        }
    }
}

impl Validated for TokenizationMintCallback {
    /// Checks that the body names exactly one account.
    ///
    /// [`new`](Self::new) sets neither identifier, because the schema does not
    /// say which one a given issuer uses; a body that still carries neither
    /// would go out with the account unnamed.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if
    /// neither `client_account_id` nor `client_external_account_id` is set, or
    /// if both are. The deprecated [`client_id`](Self::client_id) counts as
    /// `client_external_account_id`, which is what Alpaca still accepts it as.
    fn validate(&self) -> Result<()> {
        exactly_one_account(
            self.client_account_id.is_some(),
            self.client_external_account_id.is_some() || self.client_id.is_some(),
        )
    }
}

/// An issuer's request to redeem tokens back into the underlying asset.
///
/// The body of `POST /v1/accounts/{account_id}/tokenization/callback/redeem`,
/// on `broker::BrokerClient::tokenization_redeem_callback`. Alpaca's spec names
/// this schema `TokenizationRedeemRequest` rather than a "callback", and it
/// carries seven required fields against the mint callback's two — the two
/// routes share a URL prefix and nothing else, so they get two types.
///
/// Alpaca journals the underlying asset into the Authorized Participant's
/// account in response, so a redeem that ran twice has moved the asset twice.
/// This crate never replays a `POST`, so a timeout is reported rather than
/// retried underneath the caller — but it cannot send an `Idempotency-Key`
/// header either, so a caller who retries by hand has no guard against a
/// double redeem. `issuer_request_id` is the caller's own: the by-issuer-id
/// lookup on `broker::BrokerClient` says whether the first attempt landed.
///
/// Exactly one of [`client_account_id`](Self::client_account_id) and
/// [`client_external_account_id`](Self::client_external_account_id) must be
/// set, identifying whose account receives the underlying asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct TokenizationRedeemRequest {
    /// The issuer's own identifier for the redemption.
    pub issuer_request_id: String,
    /// The underlying asset's symbol.
    pub underlying_symbol: String,
    /// The tokenized asset's symbol.
    pub token_symbol: String,
    /// How much to convert back. May be fractional.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// The chain the tokens were held on.
    pub network: TokenizationNetwork,
    /// The address the redeemed tokens were originally held at.
    pub wallet_address: String,
    /// Transaction hash of the completed request on the blockchain.
    pub tx_hash: String,
    /// Alpaca account id of the Authorized Participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_account_id: Option<Uuid>,
    /// The customer's identifier on the issuer's platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub client_external_account_id: Option<String>,
    /// Alpaca's older alias for
    /// [`client_external_account_id`](Self::client_external_account_id).
    ///
    /// Deprecated since 2026-07-15 and sunsetting 2026-10-15. Still accepted
    /// until then, so the field is here; set the newer one instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub client_id: Option<String>,
}

impl TokenizationRedeemRequest {
    /// Redeems `qty` of `token_symbol` back into `underlying_symbol`.
    ///
    /// Every parameter is required by the schema; there is no shorter form.
    #[must_use]
    pub fn new(
        issuer_request_id: impl Into<String>,
        underlying_symbol: impl Into<String>,
        token_symbol: impl Into<String>,
        qty: Decimal,
        network: TokenizationNetwork,
        wallet_address: impl Into<String>,
        tx_hash: impl Into<String>,
    ) -> Self {
        Self {
            issuer_request_id: issuer_request_id.into(),
            underlying_symbol: underlying_symbol.into(),
            token_symbol: token_symbol.into(),
            qty,
            network,
            wallet_address: wallet_address.into(),
            tx_hash: tx_hash.into(),
            client_account_id: None,
            client_external_account_id: None,
            client_id: None,
        }
    }
}

impl Validated for TokenizationRedeemRequest {
    /// Checks that the body names exactly one account.
    ///
    /// [`new`](Self::new) sets neither identifier, because the schema does not
    /// say which one a given issuer uses; a body that still carries neither
    /// would go out without naming the account the underlying asset is
    /// journaled into.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if
    /// neither `client_account_id` nor `client_external_account_id` is set, or
    /// if both are. The deprecated [`client_id`](Self::client_id) counts as
    /// `client_external_account_id`, which is what Alpaca still accepts it as.
    fn validate(&self) -> Result<()> {
        exactly_one_account(
            self.client_account_id.is_some(),
            self.client_external_account_id.is_some() || self.client_id.is_some(),
        )
    }
}

/// What the redeem callback answers with.
///
/// The 200 body of `POST /v1/accounts/{account_id}/tokenization/callback/redeem`,
/// on `broker::BrokerClient::tokenization_redeem_callback`. The mint callback
/// answers with the ordinary [`TokenizationRequest`] instead; only this one
/// needs a type of its own.
///
/// No captured payload exists for this route — paper trading answers 404 on
/// this surface — so the shape is `specs/broker.yaml`'s and is unverified
/// against a live response. That is why almost every field that the spec calls
/// required is still `Option` here: an incomplete model costs nothing, because
/// unknown fields are ignored, but a field wrongly declared required would turn
/// a working route into [`Error::Decode`](crate::Error::Decode). The fields
/// left non-optional are the ones [`TokenizationRequest`] already requires
/// across the six broker routes that answer with one, so requiring them here
/// adds no failure that is not already in the crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenizationRedeemResponse {
    /// Alpaca's identifier for the request.
    pub tokenization_request_id: String,
    /// Where the request stands.
    pub status: TokenizationStatus,
    /// The underlying asset's symbol.
    pub underlying_symbol: String,
    /// The tokenized asset's symbol.
    pub token_symbol: String,
    /// How much was converted back. May be fractional.
    #[serde(with = "crate::types::decimal")]
    pub qty: Decimal,
    /// Who issues the token.
    pub issuer: TokenizationIssuer,
    /// The chain the tokens were held on.
    pub network: TokenizationNetwork,
    /// When the request was made.
    pub created_at: DateTime<Utc>,
    /// Whether this mints or redeems — `redeem`, on this route.
    ///
    /// The spec calls it required and [`TokenizationRequest`] does not, because
    /// a mint response omits it. Optional here for the same reason.
    #[serde(rename = "type", default)]
    pub request_type: Option<TokenizationType>,
    /// The issuer's own identifier for the redemption.
    #[serde(default)]
    pub issuer_request_id: Option<String>,
    /// The address the redeemed tokens were originally held at.
    #[serde(default)]
    pub wallet_address: Option<String>,
    /// The on-chain transaction.
    #[serde(default)]
    pub tx_hash: Option<String>,
}

/// A lookup by the caller's own request id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
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
    fn a_redeem_response_keeps_type_on_the_wire_and_tolerates_the_over_required() {
        // `specs/broker.yaml` calls all twelve fields required. Four of them —
        // `type`, `issuer_request_id`, `wallet_address`, `tx_hash` — are
        // optional on `TokenizationRequest` for reasons the spec does not
        // record, so requiring them here would make the newer model stricter
        // than the one already shipping against the same wire family. A body
        // without them must decode.
        let response: TokenizationRedeemResponse = serde_json::from_value(serde_json::json!({
            "tokenization_request_id": "abc",
            "status": "completed",
            "underlying_symbol": "AAPL",
            "token_symbol": "AAPLx",
            "qty": "1.5",
            "issuer": "xstocks",
            "network": "solana",
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();

        assert_eq!(response.request_type, None);
        assert_eq!(response.issuer_request_id, None);
        assert_eq!(response.wallet_address, None);
        assert_eq!(response.tx_hash, None);
        assert_eq!(response.qty, Decimal::new(15, 1));

        // `request_type` is a Rust-side name only, as on `TokenizationRequest`.
        let full: TokenizationRedeemResponse = serde_json::from_value(serde_json::json!({
            "tokenization_request_id": "abc",
            "type": "redeem",
            "status": "completed",
            "underlying_symbol": "AAPL",
            "token_symbol": "AAPLx",
            "qty": "1.5",
            "issuer": "xstocks",
            "network": "solana",
            "created_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();
        assert_eq!(full.request_type, Some(TokenizationType::Redeem));

        let encoded = serde_json::to_value(&full).unwrap();
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

    #[test]
    fn a_mint_callback_naming_no_account_or_two_is_refused() {
        // What `new` leaves behind serialises to a body with neither field,
        // which the schema calls invalid and the wire would not say so kindly.
        let mut callback = TokenizationMintCallback::new(Uuid::nil(), "0xdead");
        assert!(callback.validate().is_err());
        let encoded = serde_json::to_value(&callback).unwrap();
        assert!(encoded.get("client_account_id").is_none(), "{encoded}");
        assert!(
            encoded.get("client_external_account_id").is_none(),
            "{encoded}"
        );

        callback.client_account_id = Some(Uuid::nil());
        assert!(callback.validate().is_ok());

        callback.client_external_account_id = Some("cust-1".to_owned());
        assert!(callback.validate().is_err());

        callback.client_account_id = None;
        assert!(callback.validate().is_ok());

        // The deprecated alias names the same account as the newer field, so
        // it satisfies the rule and collides with `client_account_id` too.
        let mut aliased = TokenizationMintCallback::new(Uuid::nil(), "0xdead");
        aliased.client_id = Some("cust-1".to_owned());
        assert!(aliased.validate().is_ok());
        aliased.client_account_id = Some(Uuid::nil());
        assert!(aliased.validate().is_err());
    }

    #[test]
    fn a_redeem_naming_no_account_or_two_is_refused() {
        let mut request = TokenizationRedeemRequest::new(
            "iss-1",
            "AAPL",
            "AAPLx",
            Decimal::ONE,
            TokenizationNetwork::Solana,
            "wallet",
            "0xdead",
        );
        assert!(request.validate().is_err());

        request.client_external_account_id = Some("cust-1".to_owned());
        assert!(request.validate().is_ok());

        request.client_account_id = Some(Uuid::nil());
        assert!(request.validate().is_err());

        request.client_external_account_id = None;
        assert!(request.validate().is_ok());
    }
}
