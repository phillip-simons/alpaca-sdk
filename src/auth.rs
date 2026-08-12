//! Credentials and the request headers they produce.
//!
//! Alpaca accepts three credential forms — a key pair, HTTP basic auth, and an
//! OAuth token — documented at
//! <https://docs.alpaca.markets/us/docs/authentication>. alpaca-py takes an
//! `(api_key, secret_key, oauth_token)` triple and validates it at runtime, and
//! raises `ValueError` on the three invalid combinations. Modelling the same thing
//! as an enum makes those states unrepresentable, so the checks disappear entirely.

use std::fmt;

use base64::prelude::{BASE64_STANDARD, Engine as _};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::error::{Error, Result};

/// Environment variable alpaca reads for the API key ID.
pub const API_KEY_ENV: &str = "APCA_API_KEY_ID";
/// Environment variable alpaca reads for the API secret key.
pub const SECRET_KEY_ENV: &str = "APCA_API_SECRET_KEY";

const KEY_HEADER: &str = "APCA-API-KEY-ID";
const SECRET_HEADER: &str = "APCA-API-SECRET-KEY";

/// How a client authenticates with Alpaca.
///
/// `Debug` is implemented by hand so keys never reach a log line.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Credentials {
    /// The default: `APCA-API-KEY-ID` and `APCA-API-SECRET-KEY` headers.
    KeyPair {
        /// The API key ID.
        api_key: String,
        /// The API secret key.
        secret_key: String,
    },
    /// HTTP basic auth over the same key pair. Used by the broker API.
    Basic {
        /// The API key ID.
        api_key: String,
        /// The API secret key.
        secret_key: String,
    },
    /// An OAuth bearer token, used instead of a key pair.
    OAuth {
        /// The bearer token.
        token: String,
    },
}

impl Credentials {
    /// Key-pair credentials sent as `APCA-*` headers.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if either value is empty.
    pub fn new(api_key: impl Into<String>, secret_key: impl Into<String>) -> Result<Self> {
        let (api_key, secret_key) = (api_key.into(), secret_key.into());
        Self::reject_empty(&api_key, "api_key")?;
        Self::reject_empty(&secret_key, "secret_key")?;
        Ok(Self::KeyPair {
            api_key,
            secret_key,
        })
    }

    /// Key-pair credentials sent as an HTTP basic `Authorization` header.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if either value is empty.
    pub fn basic(api_key: impl Into<String>, secret_key: impl Into<String>) -> Result<Self> {
        let (api_key, secret_key) = (api_key.into(), secret_key.into());
        Self::reject_empty(&api_key, "api_key")?;
        Self::reject_empty(&secret_key, "secret_key")?;
        Ok(Self::Basic {
            api_key,
            secret_key,
        })
    }

    /// OAuth bearer token credentials.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if the token is empty.
    pub fn oauth(token: impl Into<String>) -> Result<Self> {
        let token = token.into();
        Self::reject_empty(&token, "oauth token")?;
        Ok(Self::OAuth { token })
    }

    /// Reads [`API_KEY_ENV`] and [`SECRET_KEY_ENV`] from the environment.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if either variable is unset or empty.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var(API_KEY_ENV)
            .map_err(|_| Error::Credentials(format!("{API_KEY_ENV} is not set")))?;
        let secret_key = std::env::var(SECRET_KEY_ENV)
            .map_err(|_| Error::Credentials(format!("{SECRET_KEY_ENV} is not set")))?;
        Self::new(api_key, secret_key)
    }

    /// Switches key-pair credentials to basic auth, leaving other variants alone.
    ///
    /// The broker API authenticates this way; the trading and data APIs do not.
    #[must_use]
    pub fn into_basic(self) -> Self {
        match self {
            Self::KeyPair {
                api_key,
                secret_key,
            } => Self::Basic {
                api_key,
                secret_key,
            },
            other => other,
        }
    }

    /// Writes the auth headers for these credentials into `headers`.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if a value cannot be encoded as a header,
    /// which means it contains bytes outside the visible ASCII range.
    pub(crate) fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        match self {
            Self::KeyPair {
                api_key,
                secret_key,
            } => {
                headers.insert(KEY_HEADER, Self::header_value(api_key, "api_key")?);
                headers.insert(SECRET_HEADER, Self::header_value(secret_key, "secret_key")?);
            }
            Self::Basic {
                api_key,
                secret_key,
            } => {
                let encoded = BASE64_STANDARD.encode(format!("{api_key}:{secret_key}"));
                headers.insert(
                    AUTHORIZATION,
                    Self::header_value(&format!("Basic {encoded}"), "basic auth")?,
                );
            }
            Self::OAuth { token } => {
                headers.insert(
                    AUTHORIZATION,
                    Self::header_value(&format!("Bearer {token}"), "oauth token")?,
                );
            }
        }
        Ok(())
    }

    fn header_value(value: &str, name: &str) -> Result<HeaderValue> {
        let mut header = HeaderValue::from_str(value)
            .map_err(|_| Error::Credentials(format!("{name} contains invalid header bytes")))?;
        header.set_sensitive(true);
        Ok(header)
    }

    fn reject_empty(value: &str, name: &str) -> Result<()> {
        if value.is_empty() {
            return Err(Error::Credentials(format!("{name} must not be empty")));
        }
        Ok(())
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self {
            Self::KeyPair { .. } => "KeyPair",
            Self::Basic { .. } => "Basic",
            Self::OAuth { .. } => "OAuth",
        };
        f.debug_struct("Credentials")
            .field("kind", &variant)
            .field("secrets", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_pair_sets_apca_headers() {
        let mut headers = HeaderMap::new();
        Credentials::new("key", "secret")
            .unwrap()
            .apply(&mut headers)
            .unwrap();

        assert_eq!(headers[KEY_HEADER], "key");
        assert_eq!(headers[SECRET_HEADER], "secret");
        assert!(!headers.contains_key(AUTHORIZATION));
    }

    #[test]
    fn basic_auth_matches_alpaca_py_encoding() {
        let mut headers = HeaderMap::new();
        Credentials::basic("key", "secret")
            .unwrap()
            .apply(&mut headers)
            .unwrap();

        // base64("key:secret")
        assert_eq!(headers[AUTHORIZATION], "Basic a2V5OnNlY3JldA==");
    }

    #[test]
    fn oauth_sets_bearer_and_omits_key_headers() {
        let mut headers = HeaderMap::new();
        Credentials::oauth("tok")
            .unwrap()
            .apply(&mut headers)
            .unwrap();

        assert_eq!(headers[AUTHORIZATION], "Bearer tok");
        assert!(!headers.contains_key(KEY_HEADER));
    }

    #[test]
    fn empty_values_are_rejected() {
        assert!(Credentials::new("", "secret").is_err());
        assert!(Credentials::new("key", "").is_err());
        assert!(Credentials::oauth("").is_err());
    }

    #[test]
    fn debug_does_not_leak_secrets() {
        let creds = Credentials::new("AKREALKEY", "supersecret").unwrap();
        let rendered = format!("{creds:?}");

        assert!(!rendered.contains("AKREALKEY"));
        assert!(!rendered.contains("supersecret"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn auth_headers_are_marked_sensitive() {
        let mut headers = HeaderMap::new();
        Credentials::new("key", "secret")
            .unwrap()
            .apply(&mut headers)
            .unwrap();

        assert!(headers[KEY_HEADER].is_sensitive());
        assert!(headers[SECRET_HEADER].is_sensitive());
    }

    #[test]
    fn into_basic_converts_key_pair_only() {
        let pair = Credentials::new("key", "secret").unwrap();
        assert!(matches!(pair.into_basic(), Credentials::Basic { .. }));

        let oauth = Credentials::oauth("tok").unwrap();
        assert!(matches!(oauth.into_basic(), Credentials::OAuth { .. }));
    }
}
