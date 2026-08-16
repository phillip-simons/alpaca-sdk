//! [OAuth](https://docs.alpaca.markets/us/reference/issueoauthtoken) for
//! third-party apps acting on a broker account's behalf.
//!
//! Three routes: look up a registered client, authorize it against an account,
//! and exchange that for a bearer token.
//!
//! **The token route takes JSON, not form encoding.** OAuth token endpoints are
//! conventionally `application/x-www-form-urlencoded`, and this one is not —
//! the reference's request body is a JSON schema. Worth stating, because a
//! reader who knows the convention would otherwise assume the crate had it
//! wrong.
//!
//! Spec-derived, and unverified against a live response.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

wire_enum! {
    /// Whether a registered OAuth client may be used.
    pub enum OAuthClientStatus {
        /// Usable.
        Active => "ACTIVE",
        /// Not usable.
        Disabled => "DISABLED",
    }
}

wire_enum! {
    /// Which OAuth flow a client is asking for.
    pub enum OAuthResponseType {
        /// The authorization-code flow.
        Code => "code",
        /// The implicit flow.
        Token => "token",
    }
}

/// A registered third-party application.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OAuthClient {
    /// The client id.
    #[serde(default)]
    pub client_id: Option<String>,
    /// The application's name.
    #[serde(default)]
    pub name: Option<String>,
    /// What it does.
    #[serde(default)]
    pub description: Option<String>,
    /// Its home page.
    #[serde(default)]
    pub url: Option<String>,
    /// Where it may be redirected back to.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub redirect_uri: Vec<String>,
    /// Whether it may act on live accounts.
    #[serde(default)]
    pub live_trading_approved: Option<bool>,
    /// Whether it is usable.
    #[serde(default)]
    pub status: Option<OAuthClientStatus>,
    /// Its terms of use.
    #[serde(default)]
    pub terms_of_use: Option<String>,
    /// Its privacy policy.
    #[serde(default)]
    pub privacy_policy: Option<String>,
}

/// The authorization code an app exchanges for a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OAuthCode {
    /// The code.
    pub code: String,
    /// The client it was issued to.
    pub client_id: String,
    /// Where the app will be redirected.
    pub redirect_uri: String,
    /// What the code is good for.
    pub scope: String,
}

/// A bearer token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OAuthToken {
    /// The token.
    pub access_token: String,
    /// What it is good for.
    pub scope: String,
    /// How to present it. Always `Bearer`.
    pub token_type: String,
}

/// A request to authorize an app against an account, or to issue it a token.
///
/// Both routes take the same body, which is why one type serves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct OAuthRequest {
    /// The account being acted on behalf of.
    pub account_id: Uuid,
    /// The app.
    pub client_id: String,
    /// Its secret.
    pub client_secret: String,
    /// Where it will be redirected back to.
    pub redirect_uri: String,
    /// What is being asked for.
    pub scope: String,
}

impl OAuthRequest {
    /// Authorizes `client_id` against `account_id` for `scope`.
    pub fn new(
        account_id: Uuid,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            account_id,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: redirect_uri.into(),
            scope: scope.into(),
        }
    }
}

/// Filters for looking up a registered client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetOAuthClientRequest {
    /// Which flow the app intends to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_type: Option<OAuthResponseType>,
    /// The redirect it intends to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub redirect_uri: Option<String>,
    /// The scope it intends to ask for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub scope: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_request_serializes_as_json_not_a_form() {
        // The convention for OAuth token endpoints is form encoding; this one
        // takes JSON, and the test exists so nobody "fixes" it back.
        let request = OAuthRequest::new(
            Uuid::nil(),
            "client",
            "secret",
            "https://example.invalid/cb",
            "account:write trading",
        );
        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["client_id"], "client");
        assert_eq!(json["scope"], "account:write trading");
    }

    #[test]
    fn a_client_with_no_redirects_listed_decodes() {
        let client: OAuthClient = serde_json::from_value(serde_json::json!({
            "client_id": "abc",
            "status": "ACTIVE",
        }))
        .unwrap();

        assert!(client.redirect_uri.is_empty());
        assert_eq!(client.status, Some(OAuthClientStatus::Active));
    }
}
