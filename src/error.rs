//! Error types for every fallible operation in this crate.
//!
//! Alpaca reports a failure as a JSON body carrying `code` and `message`, but
//! not every failure reaches the caller that way: a gateway can answer 502 with
//! HTML. The body is parsed once here, at construction, and a non-JSON body
//! degrades to [`ApiError::body`] with `code` left as `None`.
//!
//! Parsing once matters for the degenerate case: a gateway's HTML is not JSON,
//! and an error type that fails while reporting an error is the worst place to
//! fail.

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
    Transport(#[source] TransportError),

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
    /// The boundary against [`InvalidRequest`](Self::InvalidRequest) is *where*
    /// the failure happened, not what caused it. Anything the crate determines
    /// locally before a socket is opened — an empty subscription set, a
    /// non-positive timeout, an unrecognised feed — stays `InvalidRequest`.
    /// Everything that fails on the wire is this.
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
    /// A response that parsed as JSON but is not the document the route returns.
    ///
    /// The distinction from [`Error::InvalidRequest`] is *where* the failure is.
    /// Nothing about the request was wrong and the response arrived intact; it
    /// simply does not have the shape this crate expects — a market data payload
    /// under no known key, or a `latest` response missing the very field it is
    /// named for. That is a decode failure, and reporting it as an invalid
    /// request sends the caller to look at their own parameters.
    ///
    /// `serde_json` never saw these, because they are found after a successful
    /// parse, so the source is synthesized to carry the reason.
    ///
    /// Gated on `data`: the market data payloads are the only shapes checked
    /// this way, and an ungated helper is dead code in a `trading`-only build.
    #[cfg(feature = "data")]
    pub(crate) fn decode_shape(path: &str, body: &str, reason: impl fmt::Display) -> Self {
        use serde::de::Error as _;
        Self::Decode {
            path: path.to_owned(),
            body: crate::rest::truncate(body),
            source: serde_json::Error::custom(reason),
        }
    }

    /// The HTTP status code, when the failure came from a response.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api(e) => Some(e.status),
            Self::RetriesExhausted { last, .. } => Some(last.status),
            _ => None,
        }
    }

    /// Whether this failure is transient — worth trying again *if the request
    /// is safe to repeat*.
    ///
    /// Named for what it can actually tell you. The obvious reading of a method
    /// called `is_retryable` is "it is safe to send this again", and that is not
    /// something an error alone can answer — which is why it is not called that.
    ///
    /// **Transient does not mean safe to replay.** A timed-out `POST /v2/orders`
    /// and a 504 on the same request are both transient, and both are
    /// indistinguishable from an order Alpaca accepted whose response was lost;
    /// replaying either places a second order. The one transport failure that
    /// *is* safe is a connect error, because nothing was sent.
    ///
    /// The safety question belongs to the request method, and the client answers
    /// it internally: `GET`, `PUT` and `DELETE` are replayed, `POST` and `PATCH`
    /// are not, except on a 429. If you are deciding whether to re-issue a call
    /// yourself, that is the rule to apply — this predicate only tells you
    /// whether trying again could plausibly work.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Api(e) => e.is_retried_by_default(),
            Self::Transport(e) => e.is_timeout() || e.is_connect(),
            _ => false,
        }
    }
}

impl Error {
    /// Wraps a transport failure.
    ///
    /// An inherent constructor rather than `impl From<reqwest::Error> for
    /// Error`, and for the same reason [`TransportError`] exists at all: a
    /// `From` impl names `reqwest::Error` in this crate's *public* API, so a
    /// `0.13 → 0.14` bump would still be a breaking change here — which is
    /// exactly what the newtype is meant to prevent. Keeping the conversion
    /// crate-private finishes the job.
    pub(crate) fn transport(error: reqwest::Error) -> Self {
        Self::Transport(TransportError(error))
    }
}

/// An HTTP transport failure.
///
/// An opaque wrapper rather than `reqwest::Error` itself, and that is the whole
/// point of it. reqwest is a `0.x` crate, so under cargo's rules `0.13 → 0.14`
/// is a breaking change — and a `reqwest::Error` in this crate's public API
/// would make every such bump a breaking change *here*, for a dependency that
/// has nothing to do with Alpaca. Callers on `0.1.x` would be pinned to
/// reqwest 0.13 for the life of the line.
///
/// This crate already re-exports `polars` and [`crate::rust_decimal`]
/// with two paragraphs each explaining why
/// a version-skewed type is worth avoiding. reqwest got neither; now it does not
/// need one.
///
/// The `reqwest::Error` is deliberately **not** reachable — an accessor would
/// put the type back in the public API and undo the point of the wrapper. What a
/// caller actually asks of a transport error is answered by the predicates
/// below, and [`std::error::Error::source`] continues the chain past this layer
/// into reqwest's own cause (the `hyper` or TLS error underneath), which is the
/// part that carries information this type does not already forward.
///
/// If you need a predicate that is not here, open an issue rather than reaching
/// through: adding one is cheap and keeps the dependency contained.
#[derive(Debug)]
pub struct TransportError(reqwest::Error);

impl TransportError {
    /// Whether the failure was a timeout.
    ///
    /// A timeout says nothing about whether the server acted on the request:
    /// see [`Error::is_transient`] before using this to decide on a retry.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.0.is_timeout()
    }

    /// Whether the connection was never established.
    ///
    /// The one transport failure that guarantees the request was not processed.
    #[must_use]
    pub fn is_connect(&self) -> bool {
        self.0.is_connect()
    }

    /// Whether the failure happened while reading or writing the body.
    #[must_use]
    pub fn is_body(&self) -> bool {
        self.0.is_body()
    }

    /// Whether the failure was in decoding the response.
    #[must_use]
    pub fn is_decode(&self) -> bool {
        self.0.is_decode()
    }

    /// The HTTP status, when the failure carried one.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        self.0.status().map(|status| status.as_u16())
    }

    /// The URL the request was for, if reqwest recorded one.
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        self.0.url().map(url::Url::as_str)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for TransportError {
    /// The inner error's *cause*, not the inner error itself.
    ///
    /// `Display` already delegates to `reqwest::Error`, so returning it here as
    /// well would print the same sentence twice in any formatter that walks the
    /// chain — which is most of them. Skipping a link that carries no new text
    /// keeps the chain informative.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.0)
    }
}

/// A non-success HTTP response from Alpaca.
///
/// Every field is public to read. Building one goes through
/// [`from_body`](Self::from_body), which is also how the transport builds the
/// ones a caller receives.
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
    /// Builds an `ApiError` from a status, a path, and the raw response body.
    ///
    /// This is the only way to construct one, here and in a caller's own code:
    /// the struct is `#[non_exhaustive]`, so a field-by-field literal is not
    /// available outside the crate. That is deliberate rather than an oversight
    /// — [`code`](Self::code) and [`message`](Self::message) are *read out of*
    /// [`body`](Self::body), and a constructor taking all three separately would
    /// let a caller build an error whose fields contradict each other, which is
    /// a state no response can produce.
    ///
    /// It is public so a caller can build one to test their own error handling
    /// against, and what they get behaves exactly like a real failure — the same
    /// parse, including the degradation below:
    ///
    /// ```
    /// use alpaca_sdk::ApiError;
    ///
    /// let error = ApiError::from_body(
    ///     403,
    ///     "/v2/orders",
    ///     r#"{"code":40310000,"message":"insufficient buying power"}"#,
    /// );
    ///
    /// assert_eq!(error.code, Some(40_310_000));
    /// assert_eq!(error.message, "insufficient buying power");
    /// assert!(!error.is_retried_by_default());
    /// ```
    ///
    /// A body that is not the JSON object Alpaca normally sends — a gateway's
    /// HTML, say — leaves `code` as `None` and becomes the message verbatim,
    /// rather than failing. An error type that errors while reporting an error
    /// is the worst place to be strict.
    ///
    /// ```
    /// # use alpaca_sdk::ApiError;
    /// let error = ApiError::from_body(502, "/v2/account", "<html>bad gateway</html>");
    ///
    /// assert_eq!(error.code, None);
    /// assert_eq!(error.message, "<html>bad gateway</html>");
    /// ```
    #[must_use]
    pub fn from_body(status: u16, path: impl Into<String>, body: impl Into<String>) -> Self {
        #[derive(serde::Deserialize)]
        struct Payload {
            code: Option<i64>,
            message: Option<String>,
        }

        let body = body.into();
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

    /// Whether this status is in the crate's **default** retry set.
    ///
    /// The default set, not the policy the client that produced this error was
    /// built with — an [`ApiError`] does not carry one. A client configured with
    /// `RetryConfig::status_codes([500, 502])` gets `false` here for the 500 it
    /// actually retried, and `true` for the 429 it did not. The name says
    /// "by default" so that gap is visible at the call site rather than
    /// surprising.
    #[must_use]
    pub fn is_retried_by_default(&self) -> bool {
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
        // A 502 from a gateway is HTML, and this is the path where an error
        // type must not itself error.
        let err = ApiError::from_body(502, "/v2/account", "<html>bad gateway</html>".to_owned());

        assert_eq!(err.code, None);
        assert_eq!(err.message, "<html>bad gateway</html>");
        assert_eq!(err.body, "<html>bad gateway</html>");
    }

    #[test]
    fn only_429_and_504_are_retryable() {
        for status in [429, 504] {
            assert!(
                ApiError::from_body(status, "/v2/account", String::new()).is_retried_by_default()
            );
        }
        for status in [400, 401, 403, 404, 500] {
            assert!(
                !ApiError::from_body(status, "/v2/account", String::new()).is_retried_by_default()
            );
        }
    }
}
