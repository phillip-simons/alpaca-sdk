//! The shared HTTP transport.
//!
//! Every REST client in this crate wraps a [`RestClient`]. It is public so the
//! raw request methods stay available as an escape hatch for routes this crate
//! does not wrap.
//!
//! [`RestClient::request_raw`] returns the body undecoded, which is the escape
//! hatch for a route this crate has not wrapped or a response whose shape has
//! changed.

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

/// Reads `Retry-After`, in the delta-seconds form only.
///
/// A 429 that carries this header is stating the exact answer the backoff curve
/// is guessing at, so it wins when present.
///
/// RFC 9110 also allows an HTTP-date, which this deliberately does not read:
/// honoring one means trusting that the caller's clock agrees with the server's,
/// and a date honored badly waits either far too long or not at all. Anything
/// that is not a plain count of seconds is treated as absent, which falls back
/// to the curve — the behaviour before this header was read at all.
pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Whether replaying this request could do its work a second time.
///
/// The retry policy is a set of status codes, and a status code alone cannot
/// answer this. A 504 means the gateway gave up waiting for the *answer*, not
/// that nothing happened: Alpaca may well have accepted the order. Replaying it
/// three times on the default policy is four orders, and the caller is handed
/// [`Error::RetriesExhausted`] — so they believe zero were placed.
///
/// A 429 is the exception, and the reason this takes the status as well as the
/// method. It is refused by the rate limiter before it reaches anything that
/// acts on it, so nothing was done and replaying is safe whatever the method.
///
/// Everything else defers to HTTP's own idempotency definition: `GET`, `HEAD`,
/// `PUT`, `DELETE`, `OPTIONS` and `TRACE` may be replayed; `POST` and `PATCH`
/// may not. Alpaca's money-moving routes — `POST /v2/orders`,
/// `POST /v1/journals`, every transfer — are all `POST`.
///
/// Alpaca's reference names the mechanism that would make a `POST` replayable:
/// *"Always supply `Idempotency-Key` when creating journals"*. Until this crate
/// sends one, not replaying is the only safe answer.
fn replay_is_safe(method: &Method, replay: Replay, status: u16) -> bool {
    if status == 429 {
        return true;
    }
    replay == Replay::ByMethod && method.is_idempotent()
}

/// Whether a route may be replayed on the strength of its HTTP method.
///
/// Almost all of them may: the method is what HTTP defines idempotency by, and
/// for `GET`, `PUT` and `DELETE` that definition holds here too.
///
/// The exception is a `DELETE` that is not idempotent in *effect*.
/// `DELETE /v2/positions/{asset}` and `DELETE /v2/positions` do not delete a
/// record — they submit liquidating market orders. If Alpaca accepts the order
/// and the gateway then drops the response, the position is still open when the
/// replay arrives, so the replay submits a second full-size liquidation and
/// sells the caller into a short. That is the same failure `POST` is excluded
/// for, wearing a different verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    /// HTTP's answer is the right one for this route.
    ByMethod,
    /// This route acts, whatever its verb says. Never replay it.
    Never,
}

/// A request with no query parameters or body.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Empty;

/// Configuration shared by every REST client.
///
/// Build one with [`RestConfig::new`] and adjust it with the builder methods.
/// It is `#[non_exhaustive]` so that a new knob can arrive as a field rather
/// than as a breaking change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RestConfig {
    /// The endpoint to target. Overridable for proxies and mock servers.
    pub base_url: String,
    /// The path segment inserted between the base URL and the request path.
    pub api_version: String,
    /// How retryable failures are handled.
    pub retry: RetryConfig,
    /// Per-request timeout, for REST requests only.
    ///
    /// `None` by default, and deliberately: Alpaca publishes no latency
    /// guarantee to pick a number from, and a timeout short enough to be useful
    /// on a quiet endpoint will fire on a slow one. Set it per client if the
    /// caller has a deadline of their own.
    ///
    /// It does **not** reach the event streams. This is a total deadline on a
    /// whole request, and an event stream's response body is never finished, so
    /// applying it there would cap the life of every subscription rather than
    /// bound a slow call.
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
            http: builder.build().map_err(Error::transport)?,
            config,
        })
    }

    /// The configuration this client was built with.
    #[must_use]
    pub fn config(&self) -> &RestConfig {
        &self.config
    }

    /// The same client, targeting a different API version segment.
    ///
    /// Alpaca versions routes individually rather than per API, and it is not
    /// unusual for a client's own version to be wrong for a given route: the
    /// trading API is `v2` but its locate routes are `v1` and its per-market
    /// calendar is `v3`; the broker API is `v1` but funding wallets are
    /// `v1beta` and logos `v1beta1`.
    ///
    /// Writing that at the call site is deliberate. A version buried in a
    /// client constructor is the mistake that shipped three event streams
    /// pointing at routes Alpaca had retired; this way the version sits next to
    /// the path it belongs to.
    ///
    /// Cheap: the underlying HTTP client is shared, not rebuilt.
    #[must_use]
    pub fn at_version(&self, api_version: &str) -> Self {
        Self {
            http: self.http.clone(),
            config: RestConfig {
                api_version: api_version.to_owned(),
                ..self.config.clone()
            },
        }
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
            self.send(
                Method::GET,
                Replay::ByMethod,
                path,
                Some(query),
                None::<&Empty>,
            )
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
            self.send(
                Method::DELETE,
                Replay::ByMethod,
                path,
                Some(query),
                None::<&Empty>,
            )
            .await?,
        )
    }

    /// Issues a `DELETE` that must never be replayed.
    ///
    /// For the routes whose verb says "remove a record" and whose effect is
    /// "submit an order": `DELETE /v2/positions/{asset}` and
    /// `DELETE /v2/positions`, plus their broker twins. HTTP calls a `DELETE`
    /// idempotent and for those four it is not — a replayed liquidation sells
    /// the same quantity twice. A timeout on one is reported, not retried; a 429
    /// still is, because the rate limiter refused it before anything acted.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn delete_effectful<T, Q>(&self, path: &str, query: &Q) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        self.decode(
            path,
            self.send(
                Method::DELETE,
                Replay::Never,
                path,
                Some(query),
                None::<&Empty>,
            )
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
            self.send(
                Method::POST,
                Replay::ByMethod,
                path,
                None::<&Empty>,
                Some(body),
            )
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
            self.send(
                Method::PUT,
                Replay::ByMethod,
                path,
                None::<&Empty>,
                Some(body),
            )
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
            self.send(
                Method::PATCH,
                Replay::ByMethod,
                path,
                None::<&Empty>,
                Some(body),
            )
            .await?,
        )
    }

    /// Issues a request with both a query string and a body.
    ///
    /// `replay` decides what happens on a retryable status. [`Replay::ByMethod`]
    /// is right for almost everything; pass [`Replay::Never`] for a route whose
    /// effect contradicts its verb — one that submits an order or moves money
    /// behind a `PUT` or a `DELETE`.
    ///
    /// The six methods above cover routes that take one or the other, which is
    /// nearly all of them. A handful take both — `PUT /v2/watchlists:by_name`
    /// names the watchlist in the query and carries the update in the body —
    /// and folding the query into the path string would skip its encoding.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn request<T, Q, B>(
        &self,
        method: Method,
        replay: Replay,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        self.decode(path, self.send(method, replay, path, query, body).await?)
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
        self.execute_bytes(&Method::GET, Replay::ByMethod, request, path)
            .await
    }

    /// Issues a request and returns the raw response body without deserializing.
    ///
    /// This is the escape hatch for a route this crate does not wrap, which
    /// makes `replay` load-bearing: the wrapped routes have their answer chosen
    /// for them, and here it is yours. [`Replay::Never`] for anything that acts
    /// — a replayed liquidation sells twice.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn request_raw<Q, B>(
        &self,
        method: Method,
        replay: Replay,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<String>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        self.send(method, replay, path, query, body).await
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
        replay: Replay,
        path: &str,
        query: Option<&Q>,
        body: Option<&B>,
    ) -> Result<String>
    where
        Q: Serialize + ?Sized,
        B: Serialize + ?Sized,
    {
        let mut request = self.http.request(method.clone(), self.url(path));
        if let Some(query) = query {
            request = request.query(query);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        self.execute(&method, replay, request, path).await
    }

    /// Runs the retry loop and reads the successful body as text.
    async fn execute(
        &self,
        method: &Method,
        replay: Replay,
        request: RequestBuilder,
        path: &str,
    ) -> Result<String> {
        self.execute_response(method, replay, request, path)
            .await?
            .text()
            .await
            .map_err(Error::transport)
    }

    /// Runs the retry loop and reads the successful body as bytes.
    async fn execute_bytes(
        &self,
        method: &Method,
        replay: Replay,
        request: RequestBuilder,
        path: &str,
    ) -> Result<Vec<u8>> {
        Ok(self
            .execute_response(method, replay, request, path)
            .await?
            .bytes()
            .await
            .map_err(Error::transport)?
            .to_vec())
    }

    /// The retry loop. Deliberately non-generic so it is compiled once rather
    /// than per request type.
    async fn execute_response(
        &self,
        method: &Method,
        replay: Replay,
        request: RequestBuilder,
        path: &str,
    ) -> Result<reqwest::Response> {
        let retry = &self.config.retry;
        // `attempts` counts retries *after* the first request, so a value of 3
        // means up to 4 requests in total. Saturating because `attempts` is a
        // caller-supplied `u32`: `u32::MAX` would overflow, panicking in debug
        // and wrapping to a zero-iteration loop in release.
        let total_attempts = retry.attempts.saturating_add(1);

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

            let response = current.send().await.map_err(Error::transport)?;
            let status = response.status().as_u16();

            if response.status().is_success() {
                return Ok(response);
            }

            // Read before the body: `text()` consumes the response, headers
            // and all.
            let retry_after = retry_after(response.headers());

            let body = response.text().await.unwrap_or_default();
            let api_error = ApiError::from_body(status, path, body);

            let last_attempt = attempt == total_attempts;
            if !retry.should_retry(status) || !replay_is_safe(method, replay, status) {
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

            // `attempt` is also the number of consecutive failures so far, which
            // is what the backoff curve is indexed by. A `Retry-After` overrides
            // it — but clamped, because the value comes from the other end.
            let delay = retry_after.map_or_else(
                || retry.delay(attempt),
                |after| after.min(retry.retry_after_cap()),
            );
            tracing::debug!(
                path,
                status,
                attempt,
                total_attempts,
                delay_ms = delay.as_millis(),
                honored_retry_after = retry_after.is_some(),
                "retryable response, backing off"
            );
            tokio::time::sleep(delay).await;
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

pub(crate) fn truncate(body: &str) -> String {
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

    fn with_retry_after(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    #[test]
    fn retry_after_reads_delta_seconds() {
        assert_eq!(
            retry_after(&with_retry_after("120")),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            retry_after(&with_retry_after(" 3 ")),
            Some(Duration::from_secs(3))
        );
    }

    #[test]
    fn no_retry_after_header_means_the_curve_applies() {
        assert_eq!(retry_after(&HeaderMap::new()), None);
    }

    /// RFC 9110 allows this form and this crate does not read it: honouring a
    /// date means trusting that the two clocks agree. Reading as absent puts the
    /// caller back on the backoff curve, which is the safe direction.
    #[test]
    fn the_http_date_form_reads_as_absent() {
        assert_eq!(
            retry_after(&with_retry_after("Fri, 31 Dec 1999 23:59:59 GMT")),
            None
        );
    }

    #[test]
    fn a_value_that_is_not_a_count_of_seconds_reads_as_absent() {
        assert_eq!(retry_after(&with_retry_after("soon")), None);
        // Negative and fractional values are not the delta-seconds form either.
        assert_eq!(retry_after(&with_retry_after("-5")), None);
        assert_eq!(retry_after(&with_retry_after("1.5")), None);
    }

    #[test]
    fn url_joins_base_version_and_path() {
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
        // "é" is 2 bytes, so with it MAX_ERROR_BODY (2048) already lands on a
        // boundary and the walk-back loop never runs — a version of `truncate`
        // that dropped the loop entirely would still pass. "€" is 3 bytes, so
        // 2048 falls mid-character, which is the case that actually exercises
        // it: slicing at a non-boundary index panics outright, so this would
        // fail loudly rather than quietly if the loop were broken.
        let body = "€".repeat(1000);
        assert!(
            !body.is_char_boundary(MAX_ERROR_BODY),
            "the test needs a cutoff that is NOT already on a boundary"
        );

        let truncated = truncate(&body);
        assert!(truncated.ends_with("bytes total)"));
        assert!(truncated.len() < body.len());
    }

    /// `RetryConfig::attempts` is public and takes a `u32`, so a caller can pass
    /// `u32::MAX`. The loop bound used to be `attempts + 1`, which overflowed:
    /// a panic in debug, and in release a `total_attempts` of 0, which skips the
    /// loop entirely and falls through to the `unreachable!` below it. Both
    /// failures are on the caller's side of an ordinary builder call, so the
    /// bound saturates instead.
    ///
    /// The request has to actually be issued for this to prove anything —
    /// computing the bound in the test would only re-test the arithmetic.
    #[tokio::test]
    async fn a_saturating_attempt_count_still_sends_one_request() {
        use wiremock::matchers::{method as m, path as p};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(m("GET"))
            .and(p("/v2/account"))
            .respond_with(ResponseTemplate::new(200).set_body_string("7"))
            .expect(1)
            .mount(&server)
            .await;

        let creds = Credentials::new("key", "secret").unwrap();
        let client = RestClient::new(
            &creds,
            RestConfig::new(server.uri()).retry(RetryConfig::default().attempts(u32::MAX)),
        )
        .unwrap();

        // Succeeds on the first attempt, so the retry budget is never spent; the
        // point is that establishing that budget no longer overflows.
        let got: u8 = client.get("/account", &Empty).await.unwrap();
        assert_eq!(got, 7);
    }
}
