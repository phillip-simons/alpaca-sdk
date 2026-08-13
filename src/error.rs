//! Error types for every fallible operation in this crate.
//!
//! Alpaca reports a failure as a JSON body carrying `code` and `message`, but
//! not every failure reaches the caller that way: a gateway can answer 502 with
//! HTML. The body is parsed once here, at construction, and a non-JSON body
//! degrades to [`ApiError::body`] with `code` left as `None`.
//!
//! alpaca-py's `APIError` re-parses the body on every access to `code` or
//! `message`, and raises `json.JSONDecodeError` when the body is not JSON.

use std::fmt;

/// The result type returned by every fallible operation in this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Anything that can go wrong while talking to Alpaca.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Alpaca returned a non-success HTTP status.
    #[error(transparent)]
    Api(#[from] ApiError),

    /// The request never completed: DNS, TLS, connection, or timeout failure.
    #[error("http transport error")]
    Transport(#[source] reqwest::Error),

    /// A response arrived but could not be deserialized into the expected type.
    ///
    /// `body` carries the raw payload so the mismatch can be diagnosed without
    /// re-issuing the request.
    #[error("failed to decode response from {path}: {source}")]
    Decode {
        /// The request path whose response failed to decode.
        path: String,
        /// The raw response body, truncated to a reasonable length.
        body: String,
        /// The underlying deserialization error.
        #[source]
        source: serde_json::Error,
    },

    /// The supplied credentials are structurally invalid.
    #[error("invalid credentials: {0}")]
    Credentials(String),

    /// A request was rejected locally, before any network call was made.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// A stream failed: the connection, the handshake, or a frame carried on it.
    ///
    /// This covers both stream transports — the msgpack and JSON websockets and
    /// the SSE event streams — because the failures are the same shape and a
    /// caller matching on them is not usually asking which one broke.
    ///
    /// The line these failures used to be reported on was
    /// [`InvalidRequest`](Self::InvalidRequest), which was a lie in both
    /// directions: nothing about the request was invalid, and the failure
    /// happened long after the request was accepted.
    ///
    /// A stream error is not necessarily the end of the stream. The market data
    /// stream reconnects from most of them and stops on the two that never
    /// resolve by retrying — a subscription the account is not entitled to, and
    /// credentials the server rejects.
    #[error("stream error: {0}")]
    Stream(String),

    /// The configured base URL could not be joined with the request path.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// Every retry attempt returned a retryable status.
    ///
    /// Carries the final [`ApiError`] so the caller still sees what Alpaca said.
    #[error("giving up after {attempts} attempts")]
    RetriesExhausted {
        /// Total number of requests issued, including the first.
        attempts: u32,
        /// The error returned by the final attempt.
        #[source]
        last: ApiError,
    },
}

impl Error {
    /// The HTTP status code, when the failure came from a response.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api(e) => Some(e.status),
            Self::RetriesExhausted { last, .. } => Some(last.status),
            _ => None,
        }
    }

    /// Whether retrying this request could plausibly succeed.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Api(e) => e.is_retryable(),
            Self::Transport(e) => e.is_timeout() || e.is_connect(),
            _ => false,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}

/// A non-success HTTP response from Alpaca.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApiError {
    /// The HTTP status code.
    pub status: u16,
    /// Alpaca's numeric error code, when the body was JSON carrying one.
    pub code: Option<i64>,
    /// The human-readable message, falling back to the raw body.
    pub message: String,
    /// The unparsed response body.
    pub body: String,
    /// The request path that produced this error.
    pub path: String,
}

impl ApiError {
    /// Builds an `ApiError` from a status and raw body, extracting `code` and
    /// `message` when the body is the JSON error object Alpaca normally returns.
    pub(crate) fn from_body(status: u16, path: impl Into<String>, body: String) -> Self {
        #[derive(serde::Deserialize)]
        struct Payload {
            code: Option<i64>,
            message: Option<String>,
        }

        let parsed = serde_json::from_str::<Payload>(&body).ok();
        let (code, message) = match parsed {
            Some(p) => (p.code, p.message),
            None => (None, None),
        };

        Self {
            status,
            code,
            message: message.unwrap_or_else(|| body.clone()),
            body,
            path: path.into(),
        }
    }

    /// Whether this status is one the client retries by default.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        crate::config::DEFAULT_RETRY_STATUS_CODES.contains(&self.status)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "alpaca api error {} on {}", self.status, self.path)?;
        if let Some(code) = self.code {
            write!(f, " (code {code})")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_code_and_message_from_json_body() {
        let err = ApiError::from_body(
            403,
            "/v2/orders",
            r#"{"code":40310000,"message":"insufficient buying power"}"#.to_owned(),
        );

        assert_eq!(err.status, 403);
        assert_eq!(err.code, Some(40_310_000));
        assert_eq!(err.message, "insufficient buying power");
    }

    #[test]
    fn non_json_body_degrades_instead_of_panicking() {
        // alpaca-py's APIError.code raises json.JSONDecodeError on this input.
        let err = ApiError::from_body(502, "/v2/account", "<html>bad gateway</html>".to_owned());

        assert_eq!(err.code, None);
        assert_eq!(err.message, "<html>bad gateway</html>");
        assert_eq!(err.body, "<html>bad gateway</html>");
    }

    #[test]
    fn retryable_statuses_match_alpaca_py_defaults() {
        for status in [429, 504] {
            assert!(ApiError::from_body(status, "/v2/account", String::new()).is_retryable());
        }
        for status in [400, 401, 403, 404, 500] {
            assert!(!ApiError::from_body(status, "/v2/account", String::new()).is_retryable());
        }
    }
}
