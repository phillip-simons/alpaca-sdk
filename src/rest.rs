//! The shared HTTP transport.
//!
//! Every REST client in this crate wraps a [`RestClient`]. It is public so the
//! raw request methods stay available as an escape hatch for routes this crate
//! does not wrap.
//!
//! Migrating from alpaca-py: this is where `raw_data=True` went. A boolean
//! cannot change a function's return type in Rust, so the escape hatch is a
//! method — [`RestClient::request_raw`] — rather than a constructor flag.
//!
//! Ported from `alpaca/common/rest.py`.

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Method, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::auth::Credentials;
use crate::config::{BaseUrl, RetryConfig, user_agent};
use crate::error::{ApiError, Error, Result};

/// Response bodies longer than this are truncated in [`Error::Decode`].
const MAX_ERROR_BODY: usize = 2048;

/// A request with no query parameters or body.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Empty;

/// Configuration shared by every REST client.
#[derive(Debug, Clone)]
pub struct RestConfig {
    /// The endpoint to target. Overridable for proxies and mock servers.
    pub base_url: String,
    /// The path segment inserted between the base URL and the request path.
    pub api_version: String,
    /// How retryable failures are handled.
    pub retry: RetryConfig,
    /// Per-request timeout.
    ///
    /// `None` by default, and deliberately: Alpaca publishes no latency
    /// guarantee to pick a number from, and a timeout short enough to be useful
    /// on a quiet endpoint will fire on a slow one. Set it per client if the
    /// caller has a deadline of their own.
    pub timeout: Option<Duration>,
}

impl RestConfig {
    /// A configuration targeting `base_url` at API version `v2`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_version: "v2".to_owned(),
            retry: RetryConfig::default(),
            timeout: None,
        }
    }

    /// Overrides the API version segment.
    #[must_use]
    pub fn api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self
    }

    /// Overrides the retry policy.
    #[must_use]
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Sets a per-request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl From<BaseUrl> for RestConfig {
    fn from(url: BaseUrl) -> Self {
        Self::new(url.as_str())
    }
}

/// An authenticated HTTP client for one Alpaca endpoint.
#[derive(Debug, Clone)]
pub struct RestClient {
    http: reqwest::Client,
    config: RestConfig,
}

impl RestClient {
    /// Builds a client from credentials and configuration.
    ///
    /// Redirects are disabled deliberately. Because the base URL is overridable,
    /// an `http://` endpoint that redirects to `https://` would replay a POST body
    /// — and its auth headers — over cleartext, so the request must fail loudly
    /// instead. The one route that legitimately redirects, the broker document
    /// download, uses its own client.
    ///
    /// # Errors
    /// Returns [`Error::Credentials`] if the credentials cannot be encoded as
    /// headers, or [`Error::Transport`] if the underlying HTTP client fails to build.
    pub fn new(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Self::build(Some(credentials), config)
    }

    /// Builds a client that sends no authentication headers.
    ///
    /// Alpaca's crypto market data endpoints serve unauthenticated requests.
    /// Every other endpoint requires credentials.
    ///
    /// # Errors
    /// Returns [`Error::Transport`] if the underlying HTTP client fails to build.
    pub fn unauthenticated(config: RestConfig) -> Result<Self> {
        Self::build(None, config)
    }

    fn build(credentials: Option<&Credentials>, config: RestConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(credentials) = credentials {
            credentials.apply(&mut headers)?;
        }
        headers.insert(USER_AGENT, HeaderValue::from_static(user_agent()));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none());

        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }

        Ok(Self {
            http: builder.build()?,
            config,
        })
    }

    /// The configuration this client was built with.
    #[must_use]
    pub fn config(&self) -> &RestConfig {
        &self.config
    }

    /// Issues a `GET`, sending `query` as URL parameters.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get<T, Q>(&self, path: &str, query: &Q) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(Method::GET, path, Some(query), None::<&Empty>)
                .await?,
        )
    }

    /// Issues a `DELETE`, sending `query` as URL parameters.
    ///
    /// `DELETE` parameters go in the query string, not the body — which is what
    /// the endpoints taking them (`cancel_orders`, `close_position`) expect.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn delete<T, Q>(&self, path: &str, query: &Q) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(Method::DELETE, path, Some(query), None::<&Empty>)
                .await?,
        )
    }

    /// Issues a `POST` with a JSON body.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(Method::POST, path, None::<&Empty>, Some(body))
                .await?,
        )
    }

    /// Issues a `PUT` with a JSON body.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn put<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(Method::PUT, path, None::<&Empty>, Some(body))
                .await?,
        )
    }

    /// Issues a `PATCH` with a JSON body.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn patch<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(Method::PATCH, path, None::<&Empty>, Some(body))
                .await?,
        )
    }

    /// Issues a `GET` and returns the response body as bytes.
    ///
    /// For the routes that answer with something other than JSON. The logo
    /// endpoint is the only one in this crate: it serves `image/png`, and
    /// putting it through [`RestClient::get`] would try to parse a PNG as JSON.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn get_bytes<Q>(&self, path: &str, query: &Q) -> Result<Vec<u8>>
    where
        Q: Serialize + ?Sized,
    {
        let request = self.http.get(self.url(path)).query(query);
        self.execute_bytes(request, path).await
    }

    /// Issues a request and returns the raw response body without deserializing.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn request_raw<Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<String>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        self.send(method, path, query, body).await
    }

    /// Builds the absolute URL for `path`.
    ///
    /// Plain concatenation of `base_url`, the API version segment, and the path.
    /// Not `Url::join`, which would treat a path as relative and drop segments.
    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}{path}",
            self.config.base_url.trim_end_matches('/'),
            self.config.api_version
        )
    }

    /// Runs the request, retrying retryable statuses, and returns the body text.
    async fn send<Q, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<String>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let mut request = self.http.request(method, self.url(path));
        if let Some(query) = query {
            request = request.query(query);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        self.execute(request, path).await
    }

    /// Runs the retry loop and reads the successful body as text.
    async fn execute(&self, request: RequestBuilder, path: &str) -> Result<String> {
        self.execute_response(request, path)
            .await?
            .text()
            .await
            .map_err(Error::Transport)
    }

    /// Runs the retry loop and reads the successful body as bytes.
    async fn execute_bytes(&self, request: RequestBuilder, path: &str) -> Result<Vec<u8>> {
        Ok(self
            .execute_response(request, path)
            .await?
            .bytes()
            .await
            .map_err(Error::Transport)?
            .to_vec())
    }

    /// The retry loop. Deliberately non-generic so it is compiled once rather
    /// than per request type.
    async fn execute_response(
        &self,
        request: RequestBuilder,
        path: &str,
    ) -> Result<reqwest::Response> {
        let retry = &self.config.retry;
        // `attempts` counts retries *after* the first request, so a value of 3
        // means up to 4 requests in total.
        let total_attempts = retry.attempts + 1;

        // The original builder is sent first and a copy is kept for the next
        // attempt. Cloning up front instead would swallow a builder-level error
        // — a query string that failed to serialize, say — behind a `None` from
        // `try_clone`, reporting it as an unretryable body.
        let mut current = request;

        for attempt in 1..=total_attempts {
            let next = if attempt < total_attempts {
                current.try_clone()
            } else {
                None
            };

            let response = current.send().await.map_err(Error::Transport)?;
            let status = response.status().as_u16();

            if response.status().is_success() {
                return Ok(response);
            }

            let body = response.text().await.unwrap_or_default();
            let api_error = ApiError::from_body(status, path, body);

            let last_attempt = attempt == total_attempts;
            if !retry.should_retry(status) {
                return Err(Error::Api(api_error));
            }
            if last_attempt {
                return Err(Error::RetriesExhausted {
                    attempts: total_attempts,
                    last: api_error,
                });
            }

            let Some(next) = next else {
                // Only reachable with a streaming body, which this crate never
                // sends; keep the retry loop honest rather than silently
                // returning the last error as if it were final.
                return Err(Error::InvalidRequest(
                    "request cannot be retried because its body is a stream".to_owned(),
                ));
            };
            current = next;

            tracing::debug!(
                path,
                status,
                attempt,
                total_attempts,
                "retryable response, backing off"
            );
            tokio::time::sleep(retry.wait).await;
        }

        // `total_attempts` is at least 1, so the loop always returns.
        unreachable!("retry loop exited without returning")
    }

    /// Deserializes a body, treating an empty body as `null`.
    ///
    /// Alpaca answers several endpoints (notably `DELETE`) with `204 No Content`.
    /// `T` is typically `()` or an `Option` there, both of which deserialize
    /// from `null`.
    fn decode<T: DeserializeOwned>(&self, path: &str, body: String) -> Result<T> {
        let source = if body.trim().is_empty() {
            "null"
        } else {
            &body
        };

        serde_json::from_str(source).map_err(|source| Error::Decode {
            path: path.to_owned(),
            body: truncate(&body),
            source,
        })
    }
}

fn truncate(body: &str) -> String {
    if body.len() <= MAX_ERROR_BODY {
        return body.to_owned();
    }
    let mut end = MAX_ERROR_BODY;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &body[..end], body.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(base_url: &str) -> RestClient {
        let creds = Credentials::new("key", "secret").unwrap();
        RestClient::new(&creds, RestConfig::new(base_url)).unwrap()
    }

    #[test]
    fn url_matches_alpaca_py_concatenation() {
        let client = client("https://paper-api.alpaca.markets");
        assert_eq!(
            client.url("/orders"),
            "https://paper-api.alpaca.markets/v2/orders"
        );
    }

    #[test]
    fn trailing_slash_on_base_url_does_not_double_up() {
        let client = client("https://paper-api.alpaca.markets/");
        assert_eq!(
            client.url("/orders"),
            "https://paper-api.alpaca.markets/v2/orders"
        );
    }

    #[test]
    fn api_version_is_overridable() {
        let creds = Credentials::new("key", "secret").unwrap();
        let client = RestClient::new(
            &creds,
            RestConfig::new("https://broker-api.sandbox.alpaca.markets").api_version("v1"),
        )
        .unwrap();

        assert_eq!(
            client.url("/accounts"),
            "https://broker-api.sandbox.alpaca.markets/v1/accounts"
        );
    }

    #[test]
    fn empty_body_decodes_as_unit() {
        let client = client("https://paper-api.alpaca.markets");
        client.decode::<()>("/orders/1", String::new()).unwrap();
        client.decode::<()>("/orders/1", "   ".to_owned()).unwrap();
    }

    #[test]
    fn empty_body_decodes_as_none() {
        let client = client("https://paper-api.alpaca.markets");
        let decoded: Option<u8> = client.decode("/orders/1", String::new()).unwrap();
        assert_eq!(decoded, None);
    }

    #[test]
    fn decode_failure_carries_the_body() {
        let client = client("https://paper-api.alpaca.markets");
        let err = client
            .decode::<u8>("/account", "not json".to_owned())
            .unwrap_err();

        match err {
            Error::Decode { path, body, .. } => {
                assert_eq!(path, "/account");
                assert_eq!(body, "not json");
            }
            other => panic!("expected Decode, got {other:?}"),
        }
    }

    #[test]
    fn long_bodies_are_truncated_on_a_char_boundary() {
        let body = "é".repeat(MAX_ERROR_BODY);
        let truncated = truncate(&body);
        assert!(truncated.ends_with("bytes total)"));
        assert!(truncated.len() < body.len());
    }
}
