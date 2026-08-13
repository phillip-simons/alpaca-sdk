//! The trade update websocket.
//!
//! Same reconnect discipline as the market data stream, different protocol. Four
//! things differ on the wire, and all four are easy to get wrong by assuming the
//! streams match:
//!
//! | | Market data | Trade updates |
//! |---|---|---|
//! | Encoding | msgpack | JSON |
//! | Greeting | waits for `{"T":"success","msg":"connected"}` | none — authenticates immediately |
//! | Auth | `{"action":"auth","key":…,"secret":…}` | `{"action":"authenticate","data":{"key_id":…,"secret_key":…}}` |
//! | Auth reply | `{"T":"success","msg":"authenticated"}` | `{"data":{"status":"authorized"}}` |
//!
//! Subscribing is `{"action":"listen","data":{"streams":["trade_updates"]}}`, and
//! every update arrives as `{"stream":"trade_updates","data":{…}}`.

use std::time::{Duration, Instant};

use futures_util::{SinkExt as _, Stream, StreamExt as _};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

use crate::auth::Credentials;
use crate::backoff::{DEFAULT_MAX_BACKOFF, DEFAULT_MIN_BACKOFF, reconnect_delay};
use crate::config::{BaseUrl, user_agent};
use crate::error::{Error, Result};
use crate::trading::models::TradeUpdate;

/// The one channel this stream carries.
const TRADE_UPDATES: &str = "trade_updates";

/// How long to wait for a frame before re-checking the staleness clock.
const RECEIVE_POLL: Duration = Duration::from_secs(5);

pub use crate::config::DEFAULT_STABLE_SESSION;

/// A frame from the trade update stream.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TradeStreamMessage {
    /// Something happened to one of the account's orders.
    TradeUpdate(Box<TradeUpdate>),
    /// A frame this crate does not model, kept intact.
    Other {
        /// The `stream` value identifying the frame.
        stream: String,
        /// The frame as sent.
        raw: Value,
    },
}

impl TradeStreamMessage {
    /// Whether this frame is an account update rather than a control frame.
    ///
    /// Only these reset the staleness clock, for the same reason as the market
    /// data stream: a subscribed-but-silent connection must not look healthy.
    #[must_use]
    pub fn is_trade_update(&self) -> bool {
        matches!(self, Self::TradeUpdate(_))
    }
}

/// Live updates for the account's orders.
///
/// ```no_run
/// # use alpaca_sdk::{Credentials, trading::{TradeStreamMessage, TradingStream}};
/// # use futures_util::StreamExt as _;
/// # async fn example() -> alpaca_sdk::Result<()> {
/// let stream = TradingStream::new(Credentials::from_env()?, true);
///
/// let mut updates = Box::pin(stream.run());
/// while let Some(update) = updates.next().await {
///     if let Ok(TradeStreamMessage::TradeUpdate(update)) = update {
///         println!("{:?} {:?}", update.event, update.order.status);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct TradingStream {
    endpoint: String,
    credentials: Credentials,
    min_backoff: Duration,
    max_backoff: Duration,
    data_timeout: Option<Duration>,
    stable_session: Duration,
}

impl TradingStream {
    /// A stream against the paper or live trading environment.
    #[must_use]
    pub fn new(credentials: Credentials, paper: bool) -> Self {
        Self::with_endpoint(credentials, BaseUrl::trading_stream(paper).as_str())
    }

    /// A stream against a custom endpoint, for proxies and tests.
    #[must_use]
    pub fn with_endpoint(credentials: Credentials, endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            credentials,
            min_backoff: DEFAULT_MIN_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            data_timeout: None,
            stable_session: DEFAULT_STABLE_SESSION,
        }
    }

    /// Reconnect after this long without a trade update.
    ///
    /// Off by default. An account that simply is not trading
    /// is silent for good reasons, so reconnecting on a timer would be wrong;
    /// set this only if updates are expected continuously.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the timeout is not positive.
    pub fn data_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(Error::InvalidRequest(
                "data_timeout must be a positive duration".to_owned(),
            ));
        }
        self.data_timeout = Some(timeout);
        Ok(self)
    }

    /// The reconnect backoff window.
    ///
    /// The delay starts at `min`, doubles on each consecutive failure, and is
    /// capped at `max`. Mirrors
    /// [`StreamConfig::backoff`](crate::data::StreamConfig::backoff), so the two
    /// streams are configured the same way.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if `min` is zero — which would spin —
    /// or if `max` is smaller than `min`.
    pub fn backoff(mut self, min: Duration, max: Duration) -> Result<Self> {
        if min.is_zero() {
            return Err(Error::InvalidRequest(
                "min_backoff must be a positive duration; zero reconnects \
                 continuously rather than immediately"
                    .to_owned(),
            ));
        }
        if max < min {
            return Err(Error::InvalidRequest(
                "max_backoff must be at least min_backoff".to_owned(),
            ));
        }
        self.min_backoff = min;
        self.max_backoff = max;
        Ok(self)
    }

    /// How long a session must stay up before it clears the reconnect failure
    /// count.
    ///
    /// A session that delivered a trade update always clears it; this covers the
    /// silent account, which this stream's own documentation calls normal.
    /// Defaults to [`DEFAULT_STABLE_SESSION`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the duration is zero, which would
    /// treat a connection that dropped instantly as healthy.
    pub fn stable_session(mut self, after: Duration) -> Result<Self> {
        if after.is_zero() {
            return Err(Error::InvalidRequest(
                "stable_session must be a positive duration".to_owned(),
            ));
        }
        self.stable_session = after;
        Ok(self)
    }

    /// Connects and yields trade updates, reconnecting on failure.
    ///
    /// The stream ends only when the server rejects the credentials; every other
    /// failure reconnects with jittered exponential backoff.
    ///
    /// # Errors
    /// Yields an error item per failed attempt; the stream continues unless the
    /// failure was fatal.
    pub fn run(self) -> impl Stream<Item = Result<TradeStreamMessage>> {
        async_stream::stream! {
            let mut retries: u32 = 0;

            loop {
                let mut socket = match connect(&self.endpoint, &self.credentials).await {
                    Ok(socket) => socket,
                    Err(error) => {
                        let fatal = matches!(error, Error::Credentials(_));
                        yield Err(error);
                        if fatal {
                            return;
                        }
                        retries = retries.saturating_add(1);
                        let delay = reconnect_delay(retries, self.min_backoff, self.max_backoff);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                };

                // When this session began, for the health check at the bottom
                // of the loop. The old reset fired only on a trade update, and a
                // silent account is this stream's documented normal state — so
                // it never ran, and a few server-side recycles left every
                // reconnect waiting the maximum. There is no replay on this
                // socket, so a fill in that window is gone. Resetting on
                // connect alone would swing too far the other way — see
                // `DEFAULT_STABLE_SESSION`.
                let session_start = Instant::now();

                // Reset by trade updates, so `data_timeout` measures elapsed
                // time since the last one rather than the length of one read.
                let mut last_update = Instant::now();
                let mut delivered_update = false;

                loop {
                    let poll = match self.data_timeout {
                        Some(timeout) => {
                            let remaining = timeout.saturating_sub(last_update.elapsed());
                            if remaining.is_zero() {
                                tracing::warn!(
                                    endpoint = %self.endpoint,
                                    ?timeout,
                                    "no trade updates within the timeout, reconnecting"
                                );
                                break;
                            }
                            RECEIVE_POLL.min(remaining)
                        }
                        None => RECEIVE_POLL,
                    };

                    let frame = match tokio::time::timeout(poll, socket.next()).await {
                        // Staleness is decided at the top of the loop, from the
                        // clock rather than from this one read.
                        Err(_) => continue,
                        Ok(None) => break,
                        Ok(Some(Err(error))) => {
                            tracing::warn!(%error, "trade update stream error, reconnecting");
                            break;
                        }
                        Ok(Some(Ok(frame))) => frame,
                    };

                    let payload = match frame {
                        Message::Text(text) => text.as_bytes().to_vec(),
                        Message::Binary(bytes) => bytes.to_vec(),
                        Message::Close(_) => break,
                        _ => continue,
                    };

                    match decode(&payload) {
                        Ok(Some(message)) => {
                            if message.is_trade_update() {
                                last_update = Instant::now();
                                delivered_update = true;
                            }
                            yield Ok(message);
                        }
                        // A frame with no `stream` key, such as a listen
                        // acknowledgement; nothing to hand the caller.
                        Ok(None) => {}
                        Err(error) => yield Err(error),
                    }
                }

                let _ = socket.close(None).await;
                // A session that did its job clears the failure count; one that
                // came up and fell straight over does not.
                if delivered_update || session_start.elapsed() >= self.stable_session {
                    retries = 0;
                }
                retries = retries.saturating_add(1);
                let delay = reconnect_delay(retries, self.min_backoff, self.max_backoff);
                tracing::debug!(?delay, retries, "backing off before reconnect");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the socket, authenticates, and starts listening.
async fn connect(endpoint: &str, credentials: &Credentials) -> Result<Socket> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let mut request = endpoint
        .into_client_request()
        .map_err(|e| Error::InvalidUrl(e.to_string()))?;
    request.headers_mut().insert(
        "User-Agent",
        user_agent()
            .parse()
            .map_err(|_| Error::Stream("could not build the user agent header".to_owned()))?,
    );

    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| Error::Stream(format!("websocket connect failed: {e}")))?;

    let (key_id, secret_key) = match credentials {
        Credentials::KeyPair {
            api_key,
            secret_key,
        }
        | Credentials::Basic {
            api_key,
            secret_key,
        } => (api_key.clone(), secret_key.clone()),
        Credentials::OAuth { .. } => {
            return Err(Error::Credentials(
                "the trade update stream authenticates with a key pair, not OAuth".to_owned(),
            ));
        }
    };

    // No greeting frame here, unlike the market data stream: this endpoint
    // expects the authenticate message immediately on connect.
    send(
        &mut socket,
        &serde_json::json!({
            "action": "authenticate",
            "data": {"key_id": key_id, "secret_key": secret_key},
        }),
    )
    .await?;

    let reply = receive(&mut socket).await?;
    let status = reply
        .get("data")
        .and_then(|data| data.get("status"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if status != "authorized" {
        return Err(Error::Credentials(format!(
            "the server did not authorize the connection: {reply}"
        )));
    }

    send(
        &mut socket,
        &serde_json::json!({
            "action": "listen",
            "data": {"streams": [TRADE_UPDATES]},
        }),
    )
    .await?;

    tracing::info!(endpoint, "trade update stream connected");
    Ok(socket)
}

async fn send(socket: &mut Socket, payload: &Value) -> Result<()> {
    socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .map_err(|e| Error::Stream(format!("websocket send failed: {e}")))
}

async fn receive(socket: &mut Socket) -> Result<Value> {
    let frame = socket
        .next()
        .await
        .ok_or_else(|| Error::Stream("the stream closed during the handshake".to_owned()))?
        .map_err(|e| Error::Stream(format!("websocket error: {e}")))?;

    let payload = match frame {
        Message::Text(text) => text.as_bytes().to_vec(),
        Message::Binary(bytes) => bytes.to_vec(),
        other => {
            return Err(Error::Stream(format!(
                "expected a data frame during the handshake, got {other:?}"
            )));
        }
    };

    serde_json::from_slice(&payload)
        .map_err(|e| Error::Stream(format!("could not decode a handshake frame: {e}")))
}

/// Decodes one frame, returning `None` for frames that carry no stream.
fn decode(payload: &[u8]) -> Result<Option<TradeStreamMessage>> {
    let frame: Value = serde_json::from_slice(payload)
        .map_err(|e| Error::Stream(format!("could not decode a stream frame: {e}")))?;

    let Some(stream) = frame.get("stream").and_then(Value::as_str) else {
        return Ok(None);
    };

    if stream != TRADE_UPDATES {
        return Ok(Some(TradeStreamMessage::Other {
            stream: stream.to_owned(),
            raw: frame,
        }));
    }

    let data = frame
        .get("data")
        .cloned()
        .ok_or_else(|| Error::Stream(format!("a trade update frame has no data: {frame}")))?;

    let update: TradeUpdate = serde_json::from_value(data).map_err(|source| Error::Decode {
        path: "<trade_updates>".to_owned(),
        body: frame.to_string(),
        source,
    })?;

    Ok(Some(TradeStreamMessage::TradeUpdate(Box::new(update))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> Result<Option<TradeStreamMessage>> {
        decode(json.as_bytes())
    }

    const ORDER: &str = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "x",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "order_class": "simple",
        "time_in_force": "day",
        "status": "filled",
        "extended_hours": false,
        "symbol": "AAPL"
    }"#;

    #[test]
    fn decodes_a_trade_update() {
        let json = format!(
            r#"{{"stream":"trade_updates","data":{{"event":"fill","timestamp":"2021-03-16T18:38:01.942282Z","order":{ORDER},"price":"170.5","qty":"10"}}}}"#
        );

        match frame(&json).unwrap().unwrap() {
            TradeStreamMessage::TradeUpdate(update) => {
                assert_eq!(update.order.symbol.as_deref(), Some("AAPL"));
                assert_eq!(update.price, Some(rust_decimal::Decimal::new(1705, 1)));
            }
            other => panic!("expected a trade update, got {other:?}"),
        }
    }

    #[test]
    fn a_listen_acknowledgement_yields_nothing() {
        // The server echoes the subscription without a `stream` key; there is
        // nothing useful to hand the caller.
        let decoded = frame(r#"{"data":{"streams":["trade_updates"]}}"#).unwrap();
        assert!(decoded.is_none());
    }

    #[test]
    fn an_unknown_stream_keeps_its_payload() {
        match frame(r#"{"stream":"account_updates","data":{"x":1}}"#)
            .unwrap()
            .unwrap()
        {
            TradeStreamMessage::Other { stream, raw } => {
                assert_eq!(stream, "account_updates");
                assert_eq!(raw["data"]["x"], 1);
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn only_trade_updates_reset_the_staleness_clock() {
        let other = TradeStreamMessage::Other {
            stream: "account_updates".to_owned(),
            raw: Value::Null,
        };
        assert!(!other.is_trade_update());
    }

    #[test]
    fn a_malformed_frame_is_an_error_not_a_panic() {
        assert!(frame("not json").is_err());
        assert!(frame(r#"{"stream":"trade_updates"}"#).is_err());
    }

    #[test]
    fn a_data_timeout_must_be_positive() {
        let credentials = Credentials::new("key", "secret").unwrap();
        let stream = TradingStream::with_endpoint(credentials, "ws://127.0.0.1:1");
        assert!(stream.data_timeout(Duration::ZERO).is_err());
    }

    #[test]
    fn paper_and_live_use_different_endpoints() {
        let credentials = Credentials::new("key", "secret").unwrap();

        let paper = TradingStream::new(credentials.clone(), true);
        let live = TradingStream::new(credentials, false);

        assert_eq!(paper.endpoint, "wss://paper-api.alpaca.markets/stream");
        assert_eq!(live.endpoint, "wss://api.alpaca.markets/stream");
    }
}
