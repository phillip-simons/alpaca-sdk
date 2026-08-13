//! The [live market data](https://docs.alpaca.markets/us/docs/streaming-market-data) websocket.
//!
//! # Shape
//!
//! The connection is a [`Stream`] of [`StreamMessage`] rather than a set of
//! registered callbacks. That gives the caller backpressure — a slow consumer
//! slows the read rather than queueing without bound — and lets them dispatch
//! however they like, including into a handler map.

mod messages;
mod streams;

pub use self::StreamConfig as LiveStreamConfig;
pub use messages::{Channel, StreamError, StreamMessage, Subscriptions};
pub use streams::{CryptoDataStream, NewsDataStream, OptionDataStream, StockDataStream};

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use futures_util::{SinkExt as _, Stream, StreamExt as _};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

use crate::auth::Credentials;
use crate::backoff::{DEFAULT_MAX_BACKOFF, DEFAULT_MIN_BACKOFF, reconnect_delay};
use crate::config::user_agent;
use crate::data::models::{
    Bar, News, Orderbook, Quote, Trade, TradeCancel, TradeCorrection, TradingStatus, WithSymbol,
};
use crate::error::{Error, Result};

/// Largest subscribe payload sent in one websocket message.
///
/// A subscription larger than this is split across several subscribe messages
/// rather than fragmented into one. Subscribing is additive, so the server ends
/// in the same state either way, and neither approach puts a frame larger than
/// this on the wire.
const MAX_FRAME_SIZE: usize = 32_768;

/// How long to wait for a frame before re-checking the staleness clock.
///
/// Poll granularity only — never the timeout itself. Feeding this straight to
/// the socket read and treating an elapsed read as staleness is what made
/// `data_timeout` mean a hardcoded five seconds whatever the caller set.
const RECEIVE_POLL: Duration = Duration::from_secs(5);

pub use crate::config::DEFAULT_STABLE_SESSION;

/// Configuration for a market data stream.
///
/// Deliberately *not* `#[non_exhaustive]`: every field is private and reached
/// through a validating builder, so a struct literal is already impossible and
/// the attribute would buy nothing on a type the caller has to construct.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// The websocket endpoint.
    pub endpoint: String,
    /// How long to go without market data before treating the connection as
    /// stale and reconnecting.
    ///
    /// `None` by default. A legitimately quiet subscription — news, or
    /// infrequent bars — would otherwise reconnect on a timer. Set it only for
    /// subscriptions expected to be busy.
    ///
    /// Private, and set through [`StreamConfig::data_timeout`], for the same
    /// reason as the two backoff knobs: a validator that a field assignment can
    /// step around is not a validator. Zero here makes the staleness deadline
    /// elapse on the first pass of every session, so each connection reconnects
    /// immediately.
    data_timeout: Option<Duration>,
    /// Base delay for the first reconnect attempt.
    ///
    /// Private, and set through [`StreamConfig::backoff`], because zero is a
    /// reasonable-looking way to say "reconnect immediately" and is in fact a
    /// hot loop: the curve stays at zero through every doubling, the jitter
    /// branch is skipped, and the client opens thousands of connections a second
    /// against an endpoint that allows one. The sibling knob has been validated
    /// for exactly this since it was added.
    min_backoff: Duration,
    /// Ceiling for the reconnect delay. Set through
    /// [`StreamConfig::backoff`].
    max_backoff: Duration,
    /// How long a session must last to count as healthy. Set through
    /// [`StreamConfig::stable_session`].
    stable_session: Duration,
}

impl StreamConfig {
    /// A configuration targeting `endpoint`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            data_timeout: None,
            min_backoff: DEFAULT_MIN_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            stable_session: DEFAULT_STABLE_SESSION,
        }
    }

    /// Reconnect after this long without market data.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the timeout is not positive.
    pub fn data_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.set_data_timeout(timeout)?;
        Ok(self)
    }

    /// The same, in place.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the timeout is not positive.
    pub fn set_data_timeout(&mut self, timeout: Duration) -> Result<&mut Self> {
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
    /// capped at `max`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if `min` is zero — which would spin —
    /// or if `max` is smaller than `min`.
    pub fn backoff(mut self, min: Duration, max: Duration) -> Result<Self> {
        self.set_backoff(min, max)?;
        Ok(self)
    }

    /// The same, in place.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if `min` is zero, or if `max` is
    /// smaller than `min`.
    pub fn set_backoff(&mut self, min: Duration, max: Duration) -> Result<&mut Self> {
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

    /// How long the stream tolerates silence before reconnecting.
    #[must_use]
    pub fn data_timeout_after(&self) -> Option<Duration> {
        self.data_timeout
    }

    /// The base delay for the first reconnect attempt.
    #[must_use]
    pub fn min_backoff(&self) -> Duration {
        self.min_backoff
    }

    /// The ceiling for the reconnect delay.
    #[must_use]
    pub fn max_backoff(&self) -> Duration {
        self.max_backoff
    }

    /// How long a session must stay up before it clears the failure count.
    ///
    /// A session that delivered market data always clears it; this covers the
    /// legitimately quiet one whose server recycles it. Defaults to
    /// [`DEFAULT_STABLE_SESSION`].
    ///
    /// Note the interaction with [`StreamConfig::data_timeout`]: if the timeout
    /// is the longer of the two, a stream that is up but permanently silent
    /// reconnects on the staleness clock and each of those sessions counts as
    /// healthy, so the backoff never escalates. That is deliberate — the
    /// reconnect cadence is then bounded by `data_timeout` rather than by the
    /// curve — but set `data_timeout` below this value if you would rather a
    /// persistently silent feed backed off instead.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the duration is zero, which would
    /// treat a connection that dropped instantly as healthy and pin the
    /// reconnect delay at its minimum.
    pub fn stable_session(mut self, after: Duration) -> Result<Self> {
        self.set_stable_session(after)?;
        Ok(self)
    }

    /// The same, in place.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the duration is zero.
    pub fn set_stable_session(&mut self, after: Duration) -> Result<&mut Self> {
        if after.is_zero() {
            return Err(Error::InvalidRequest(
                "stable_session must be a positive duration".to_owned(),
            ));
        }
        self.stable_session = after;
        Ok(self)
    }

    /// How long a session must stay up before it clears the failure count.
    #[must_use]
    pub fn stable_session_after(&self) -> Duration {
        self.stable_session
    }
}

/// Which symbols are subscribed on which channels.
#[derive(Debug, Clone, Default)]
pub struct SubscriptionSet {
    channels: BTreeMap<Channel, BTreeSet<String>>,
}

impl SubscriptionSet {
    /// Adds `symbols` to `channel`.
    pub fn add<I, S>(&mut self, channel: Channel, symbols: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let entry = self.channels.entry(channel).or_default();
        for symbol in symbols {
            entry.insert(symbol.into());
        }
    }

    /// Removes `symbols` from `channel`.
    pub fn remove<I, S>(&mut self, channel: Channel, symbols: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if let Some(entry) = self.channels.get_mut(&channel) {
            for symbol in symbols {
                entry.remove(symbol.as_ref());
            }
        }
    }

    /// The symbols subscribed on `channel`.
    #[must_use]
    pub fn symbols(&self, channel: Channel) -> Vec<String> {
        self.channels
            .get(&channel)
            .map(|symbols| symbols.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether anything subscribable is registered.
    ///
    /// Corrections and cancel errors do not count: they arrive with the trades
    /// subscription, so a connection carrying only those would never receive
    /// anything, and the socket should not be opened for it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self
            .channels
            .iter()
            .any(|(channel, symbols)| channel.is_subscribable() && !symbols.is_empty())
    }

    /// The subscribe or unsubscribe payloads to send, split so no single
    /// message exceeds [`MAX_FRAME_SIZE`].
    fn payloads(&self, action: &str) -> Vec<Value> {
        let mut payloads = Vec::new();
        let mut current = serde_json::Map::new();

        for (channel, symbols) in &self.channels {
            if !channel.is_subscribable() || symbols.is_empty() {
                continue;
            }

            let mut batch: Vec<Value> = Vec::new();
            for symbol in symbols {
                batch.push(Value::from(symbol.clone()));

                // Flush when this channel's batch alone approaches the limit.
                if estimated_size(&current) + estimated_batch(&batch) > MAX_FRAME_SIZE {
                    let mut flushed = std::mem::take(&mut batch);
                    let overflow = flushed.pop();
                    if !flushed.is_empty() {
                        current.insert(channel.wire_name().to_owned(), Value::Array(flushed));
                    }
                    if !current.is_empty() {
                        payloads.push(finish(action, std::mem::take(&mut current)));
                    }
                    batch = overflow.into_iter().collect();
                }
            }

            if !batch.is_empty() {
                current.insert(channel.wire_name().to_owned(), Value::Array(batch));
            }
        }

        if !current.is_empty() {
            payloads.push(finish(action, current));
        }
        payloads
    }
}

fn finish(action: &str, mut map: serde_json::Map<String, Value>) -> Value {
    map.insert("action".to_owned(), Value::from(action));
    Value::Object(map)
}

fn estimated_size(map: &serde_json::Map<String, Value>) -> usize {
    map.iter()
        .map(|(key, value)| key.len() + estimated_value(value))
        .sum()
}

fn estimated_value(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.iter().map(estimated_value).sum::<usize>() + 2,
        other => other.to_string().len(),
    }
}

fn estimated_batch(batch: &[Value]) -> usize {
    batch.iter().map(estimated_value).sum::<usize>() + 16
}

/// Why a connection attempt ended.
enum Outcome {
    /// The socket went stale or errored; reconnect.
    Reconnect,
    /// A fatal condition; stop for good.
    Fatal,
}

/// A live market data connection.
///
/// Subscribe before calling [`DataStream::run`]: the socket is not opened until
/// something is subscribed, because Alpaca permits one connection per account
/// and an empty one would hold that slot for nothing.
pub struct DataStream {
    config: StreamConfig,
    credentials: Credentials,
    subscriptions: SubscriptionSet,
}

impl DataStream {
    /// A stream against `config`.
    #[must_use]
    pub fn new(credentials: Credentials, config: StreamConfig) -> Self {
        Self {
            config,
            credentials,
            subscriptions: SubscriptionSet::default(),
        }
    }

    /// The subscriptions registered so far.
    #[must_use]
    pub fn subscriptions(&self) -> &SubscriptionSet {
        &self.subscriptions
    }

    /// The configuration, for the concrete streams to adjust.
    pub(crate) fn config_mut(&mut self) -> &mut StreamConfig {
        &mut self.config
    }

    /// Adds `symbols` to `channel`.
    pub fn subscribe<I, S>(&mut self, channel: Channel, symbols: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.subscriptions.add(channel, symbols);
        self
    }

    /// Removes `symbols` from `channel`.
    pub fn unsubscribe<I, S>(&mut self, channel: Channel, symbols: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.subscriptions.remove(channel, symbols);
        self
    }

    /// Connects and yields frames, reconnecting on failure.
    ///
    /// The stream ends only on a fatal condition: a subscription the account is
    /// not entitled to, or credentials the server rejects. Everything else —
    /// a dropped socket, a stale connection, a transport error — reconnects with
    /// jittered exponential backoff.
    ///
    /// # Errors
    /// Yields an error item for each failed attempt; the stream continues unless
    /// the failure was fatal.
    pub fn run(self) -> impl Stream<Item = Result<StreamMessage>> {
        async_stream::stream! {
            if self.subscriptions.is_empty() {
                yield Err(Error::InvalidRequest(
                    "subscribe to at least one channel before running the stream".to_owned(),
                ));
                return;
            }

            let mut retries: u32 = 0;

            loop {
                let connected = connect(&self.config, &self.credentials, &self.subscriptions).await;
                let mut socket = match connected {
                    Ok(socket) => socket,
                    Err(error) => {
                        let fatal = is_fatal(&error);
                        yield Err(error);
                        if fatal {
                            return;
                        }
                        // A half-open socket from a failed connect or auth keeps
                        // consuming the single connection Alpaca allows, so the
                        // attempt above always drops it before we back off.
                        retries = retries.saturating_add(1);
                        sleep_backoff(&self.config, retries).await;
                        continue;
                    }
                };

                // When this session began, for the health check at the bottom
                // of the loop.
                let session_start = Instant::now();

                // The staleness clock. Starts at connect and is pushed forward
                // by market data, so `data_timeout` measures what it says:
                // elapsed time since the last market data message.
                let mut last_data = Instant::now();
                let mut delivered_data = false;

                let outcome = 'session: loop {
                    let poll = match self.config.data_timeout {
                        Some(timeout) => {
                            let remaining = timeout.saturating_sub(last_data.elapsed());
                            if remaining.is_zero() {
                                tracing::warn!(
                                    endpoint = %self.config.endpoint,
                                    ?timeout,
                                    "no market data within the timeout, reconnecting"
                                );
                                break 'session Outcome::Reconnect;
                            }
                            // Wake at least every `RECEIVE_POLL` so the check
                            // above runs, but never later than the deadline.
                            RECEIVE_POLL.min(remaining)
                        }
                        None => RECEIVE_POLL,
                    };

                    let frame = match tokio::time::timeout(poll, socket.next()).await {
                        // Nothing arrived within the poll window. Whether that
                        // is staleness is decided at the top of the loop, from
                        // the clock rather than from this one read.
                        Err(_) => continue,
                        Ok(None) => break 'session Outcome::Reconnect,
                        Ok(Some(Err(error))) => {
                            tracing::warn!(%error, "websocket error, reconnecting");
                            break 'session Outcome::Reconnect;
                        }
                        Ok(Some(Ok(frame))) => frame,
                    };

                    let payload = match frame {
                        Message::Binary(bytes) => bytes.to_vec(),
                        Message::Text(text) => text.as_bytes().to_vec(),
                        Message::Close(_) => break 'session Outcome::Reconnect,
                        // Ping and pong are handled by the transport.
                        _ => continue,
                    };

                    for message in decode(&payload) {
                        match message {
                            Ok(message) => {
                                if let StreamMessage::Error(error) = &message
                                    && is_fatal_message(&error.message)
                                {
                                    yield Ok(message);
                                    break 'session Outcome::Fatal;
                                }
                                if message.is_market_data() {
                                    last_data = Instant::now();
                                    delivered_data = true;
                                }
                                yield Ok(message);
                            }
                            Err(error) => yield Err(error),
                        }
                    }
                };

                let _ = socket.close(None).await;

                match outcome {
                    Outcome::Fatal => return,
                    Outcome::Reconnect => {
                        // A session that did its job clears the failure count;
                        // one that came up and fell straight over does not.
                        if delivered_data
                            || session_start.elapsed() >= self.config.stable_session
                        {
                            retries = 0;
                        }
                        retries = retries.saturating_add(1);
                        sleep_backoff(&self.config, retries).await;
                    }
                }
            }
        }
    }
}

async fn sleep_backoff(config: &StreamConfig, retries: u32) {
    let delay = reconnect_delay(retries, config.min_backoff, config.max_backoff);
    tracing::debug!(?delay, retries, "backing off before reconnect");
    tokio::time::sleep(delay).await;
}

/// Whether an error should stop the stream rather than trigger a reconnect.
fn is_fatal(error: &Error) -> bool {
    match error {
        Error::Credentials(_) => true,
        Error::Stream(message) => is_fatal_message(message),
        _ => false,
    }
}

/// Retrying an entitlement failure never succeeds and burns the single
/// connection Alpaca allows per account, so these end the stream.
fn is_fatal_message(message: &str) -> bool {
    message.contains("insufficient subscription") || message.contains("auth failed")
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the socket, authenticates, and sends the subscribe payloads.
async fn connect(
    config: &StreamConfig,
    credentials: &Credentials,
    subscriptions: &SubscriptionSet,
) -> Result<Socket> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let mut request = config
        .endpoint
        .as_str()
        .into_client_request()
        .map_err(|e| Error::InvalidUrl(e.to_string()))?;
    request.headers_mut().insert(
        "Content-Type",
        "application/msgpack"
            .parse()
            .map_err(|_| Error::Stream("could not build the content type header".to_owned()))?,
    );
    request.headers_mut().insert(
        "User-Agent",
        user_agent()
            .parse()
            .map_err(|_| Error::Stream("could not build the user agent header".to_owned()))?,
    );

    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| Error::Stream(format!("websocket connect failed: {e}")))?;

    expect_control(&mut socket, "connected").await?;

    let (key, secret) = match credentials {
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
                "the market data stream authenticates with a key pair, not OAuth".to_owned(),
            ));
        }
    };

    send(
        &mut socket,
        &serde_json::json!({"action": "auth", "key": key, "secret": secret}),
    )
    .await?;
    expect_control(&mut socket, "authenticated").await?;

    for payload in subscriptions.payloads("subscribe") {
        send(&mut socket, &payload).await?;
    }

    tracing::info!(endpoint = %config.endpoint, "market data stream connected");
    Ok(socket)
}

async fn send(socket: &mut Socket, payload: &Value) -> Result<()> {
    let encoded = rmp_serde::to_vec_named(payload)
        .map_err(|e| Error::Stream(format!("could not encode a stream message: {e}")))?;

    socket
        .send(Message::Binary(encoded.into()))
        .await
        .map_err(|e| Error::Stream(format!("websocket send failed: {e}")))
}

/// Reads one frame and asserts it is the expected `success` acknowledgement.
async fn expect_control(socket: &mut Socket, expected: &str) -> Result<()> {
    let frame = socket
        .next()
        .await
        .ok_or_else(|| Error::Stream("the stream closed during the handshake".to_owned()))?
        .map_err(|e| Error::Stream(format!("websocket error: {e}")))?;

    let payload = match frame {
        Message::Binary(bytes) => bytes.to_vec(),
        Message::Text(text) => text.as_bytes().to_vec(),
        other => {
            return Err(Error::Stream(format!(
                "expected a data frame during the handshake, got {other:?}"
            )));
        }
    };

    let frames: Vec<Value> = rmp_serde::from_slice(&payload)
        .or_else(|_| serde_json::from_slice(&payload))
        .map_err(|e| Error::Stream(format!("could not decode a handshake frame: {e}")))?;

    let first = frames
        .first()
        .ok_or_else(|| Error::Stream("the server sent an empty handshake frame".to_owned()))?;

    let message_type = first.get("T").and_then(Value::as_str).unwrap_or_default();
    let message = first.get("msg").and_then(Value::as_str).unwrap_or_default();

    if message_type == "error" {
        // Surfaced as-is so `is_fatal` can decide; an entitlement failure must
        // not be retried.
        return Err(Error::Stream(format!(
            "the server rejected the handshake: {message}"
        )));
    }
    if message_type != "success" || message != expected {
        return Err(Error::Stream(format!(
            "expected a {expected:?} acknowledgement, got {first}"
        )));
    }

    Ok(())
}

/// Decodes a batch frame into individual messages.
///
/// Alpaca sends an array of frames per websocket message.
fn decode(payload: &[u8]) -> Vec<Result<StreamMessage>> {
    // msgpack is read through rmpv rather than straight into serde_json::Value.
    // Alpaca sends every timestamp as extension type -1, and serde_json::Value
    // has no representation for an extension, so decoding a real frame that way
    // fails outright — the whole message is lost, not just the timestamp. rmpv
    // carries extensions natively; `normalize` turns them into the RFC 3339
    // strings the models already accept.
    //
    // A mock that encodes timestamps as strings passes either way, which is
    // exactly how this survived until the first live run.
    // JSON is tried first on purpose. `rmpv` does not require consuming the
    // whole input, so a JSON payload starting with `[` (0x5b) parses as the
    // msgpack fixint 91 and "succeeds" with garbage.
    let frames: Option<Vec<Value>> = serde_json::from_slice(payload).ok().or_else(|| {
        rmpv::decode::read_value(&mut &payload[..])
            .ok()
            .map(normalize)
            .map(|value| match value {
                Value::Array(frames) => frames,
                other => vec![other],
            })
    });

    match frames {
        Some(frames) => frames.into_iter().map(decode_frame).collect(),
        None => vec![Err(Error::Stream(
            "could not decode a stream frame as msgpack or JSON".to_owned(),
        ))],
    }
}

/// Converts a msgpack value into JSON, rendering timestamp extensions as
/// RFC 3339 strings so the shared models deserialize them unchanged.
fn normalize(value: rmpv::Value) -> Value {
    use rmpv::Value as Mp;

    match value {
        Mp::Nil => Value::Null,
        Mp::Boolean(flag) => Value::Bool(flag),
        Mp::Integer(number) => number
            .as_i64()
            .map(Value::from)
            .or_else(|| number.as_u64().map(Value::from))
            .unwrap_or(Value::Null),
        Mp::F32(number) => Value::from(f64::from(number)),
        Mp::F64(number) => Value::from(number),
        Mp::String(text) => text.into_str().map_or(Value::Null, Value::from),
        Mp::Binary(bytes) => String::from_utf8(bytes).map_or(Value::Null, Value::from),
        Mp::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
        Mp::Map(pairs) => Value::Object(
            pairs
                .into_iter()
                .map(|(key, value)| {
                    let key = match key {
                        Mp::String(text) => text.into_str().unwrap_or_default(),
                        other => other.to_string(),
                    };
                    (key, normalize(value))
                })
                .collect(),
        ),
        Mp::Ext(tag, bytes) => crate::types::timestamp::from_extension(tag, &bytes)
            .map_or(Value::Null, |timestamp| {
                Value::from(timestamp.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
            }),
    }
}

fn decode_frame(frame: Value) -> Result<StreamMessage> {
    let message_type = frame
        .get("T")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Stream(format!("a stream frame has no type: {frame}")))?
        .to_owned();

    let symbol = frame
        .get("S")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    fn build<T>(frame: &Value, symbol: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned + WithSymbol,
    {
        let mut value: T =
            serde_json::from_value(frame.clone()).map_err(|source| Error::Decode {
                path: "<stream>".to_owned(),
                body: frame.to_string(),
                source,
            })?;
        value.set_symbol(symbol);
        Ok(value)
    }

    Ok(match message_type.as_str() {
        "t" => StreamMessage::Trade(build::<Trade>(&frame, &symbol)?),
        "q" => StreamMessage::Quote(build::<Quote>(&frame, &symbol)?),
        "o" => StreamMessage::Orderbook(build::<Orderbook>(&frame, &symbol)?),
        "b" => StreamMessage::Bar(build::<Bar>(&frame, &symbol)?),
        "u" => StreamMessage::UpdatedBar(build::<Bar>(&frame, &symbol)?),
        "d" => StreamMessage::DailyBar(build::<Bar>(&frame, &symbol)?),
        "s" => StreamMessage::TradingStatus(build::<TradingStatus>(&frame, &symbol)?),
        "c" => StreamMessage::Correction(build::<TradeCorrection>(&frame, &symbol)?),
        "x" => StreamMessage::CancelError(build::<TradeCancel>(&frame, &symbol)?),
        "n" => {
            let news: News =
                serde_json::from_value(frame.clone()).map_err(|source| Error::Decode {
                    path: "<stream>".to_owned(),
                    body: frame.to_string(),
                    source,
                })?;
            StreamMessage::News(news)
        }
        "subscription" => {
            let subs: Subscriptions =
                serde_json::from_value(frame.clone()).map_err(|source| Error::Decode {
                    path: "<stream>".to_owned(),
                    body: frame.to_string(),
                    source,
                })?;
            StreamMessage::Subscription(subs)
        }
        "error" => {
            let error: StreamError =
                serde_json::from_value(frame.clone()).map_err(|source| Error::Decode {
                    path: "<stream>".to_owned(),
                    body: frame.to_string(),
                    source,
                })?;
            StreamMessage::Error(error)
        }
        _ => StreamMessage::Other {
            message_type,
            raw: frame,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_subscription_set_is_empty() {
        assert!(SubscriptionSet::default().is_empty());
    }

    #[test]
    fn corrections_alone_do_not_count_as_subscribed() {
        // They arrive with the trades subscription, so a connection carrying
        // only these would sit silent forever.
        let mut set = SubscriptionSet::default();
        set.add(Channel::Corrections, ["AAPL"]);
        set.add(Channel::CancelErrors, ["AAPL"]);

        assert!(set.is_empty());

        set.add(Channel::Trades, ["AAPL"]);
        assert!(!set.is_empty());
    }

    #[test]
    fn subscribe_payload_omits_corrections_and_cancel_errors() {
        let mut set = SubscriptionSet::default();
        set.add(Channel::Trades, ["AAPL", "MSFT"]);
        set.add(Channel::Corrections, ["AAPL"]);
        set.add(Channel::CancelErrors, ["AAPL"]);

        let payloads = set.payloads("subscribe");
        assert_eq!(payloads.len(), 1);

        let payload = &payloads[0];
        assert_eq!(payload["action"], "subscribe");
        assert_eq!(payload["trades"], serde_json::json!(["AAPL", "MSFT"]));
        assert!(payload.get("corrections").is_none());
        assert!(payload.get("cancelErrors").is_none());
    }

    #[test]
    fn subscribe_payload_uses_the_camel_case_channel_names() {
        let mut set = SubscriptionSet::default();
        set.add(Channel::UpdatedBars, ["AAPL"]);
        set.add(Channel::DailyBars, ["AAPL"]);

        let payload = &set.payloads("subscribe")[0];
        assert_eq!(payload["updatedBars"], serde_json::json!(["AAPL"]));
        assert_eq!(payload["dailyBars"], serde_json::json!(["AAPL"]));
    }

    #[test]
    fn a_large_subscription_splits_into_several_messages() {
        // Splitting into several additive subscribes reaches the same server
        // state without needing fragmented sends, which tokio-tungstenite does
        // not expose.
        let mut set = SubscriptionSet::default();
        let symbols: Vec<String> = (0..8_000).map(|i| format!("SYM{i:05}")).collect();
        set.add(Channel::Trades, symbols.clone());

        let payloads = set.payloads("subscribe");
        assert!(
            payloads.len() > 1,
            "expected a split, got {}",
            payloads.len()
        );

        for payload in &payloads {
            let encoded = rmp_serde::to_vec_named(payload).unwrap();
            assert!(
                encoded.len() <= MAX_FRAME_SIZE,
                "a frame was {} bytes",
                encoded.len()
            );
            assert_eq!(payload["action"], "subscribe");
        }

        // Every symbol still appears exactly once across the messages.
        let mut seen: Vec<String> = payloads
            .iter()
            .filter_map(|p| p.get("trades"))
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
        seen.sort();
        let mut expected = symbols;
        expected.sort();
        assert_eq!(seen, expected);
    }

    #[test]
    fn unsubscribe_removes_only_the_named_symbols() {
        let mut set = SubscriptionSet::default();
        set.add(Channel::Trades, ["AAPL", "MSFT", "SPY"]);
        set.remove(Channel::Trades, ["MSFT"]);

        assert_eq!(set.symbols(Channel::Trades), ["AAPL", "SPY"]);
    }

    #[test]
    fn insufficient_subscription_is_fatal() {
        assert!(is_fatal_message("insufficient subscription for SIP data"));
        assert!(is_fatal_message("auth failed"));
        assert!(!is_fatal_message("symbol not found"));
        assert!(!is_fatal_message("connection reset"));
    }

    #[test]
    fn decodes_a_batch_of_frames() {
        let frames = serde_json::json!([
            {"T": "t", "S": "AAPL", "t": "2022-03-18T14:03:31.960672Z", "p": 170.5, "s": 10.0},
            {"T": "q", "S": "AAPL", "t": "2022-03-18T14:03:31.960672Z",
             "bp": 1.0, "bs": 2.0, "ap": 3.0, "as": 4.0}
        ]);
        let payload = serde_json::to_vec(&frames).unwrap();

        let decoded: Vec<_> = decode(&payload).into_iter().map(Result::unwrap).collect();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].symbol(), Some("AAPL"));
        assert_eq!(decoded[0].channel(), Some(Channel::Trades));
        assert_eq!(decoded[1].channel(), Some(Channel::Quotes));
    }

    #[test]
    fn decodes_a_real_msgpack_frame_with_an_extension_timestamp() {
        // The shape the live server actually sends. Routing this through
        // serde_json::Value loses the whole frame, because JSON cannot hold a
        // msgpack extension — the failure the first live run surfaced.
        let mut buf = vec![0x91]; // one-element array
        buf.push(0x84); // four-entry map
        buf.extend_from_slice(&[0xa1, b'T', 0xa1, b't']);
        buf.extend_from_slice(&[0xa1, b'S', 0xa7]);
        buf.extend_from_slice(b"BTC/USD");
        buf.extend_from_slice(&[0xa1, b'p', 0xcb]);
        buf.extend_from_slice(&63_496.28f64.to_be_bytes());
        // "t" as timestamp32, extension type -1.
        buf.extend_from_slice(&[0xa1, b't', 0xd6, 0xff]);
        buf.extend_from_slice(&1_646_802_000u32.to_be_bytes());
        // The model needs a size; add it via a fifth entry instead.
        buf[1] = 0x85;
        buf.extend_from_slice(&[0xa1, b's', 0xcb]);
        buf.extend_from_slice(&1.5f64.to_be_bytes());

        let decoded: Vec<_> = decode(&buf).into_iter().map(Result::unwrap).collect();

        assert_eq!(decoded.len(), 1);
        match &decoded[0] {
            StreamMessage::Trade(trade) => {
                assert_eq!(trade.symbol, "BTC/USD");
                assert_eq!(trade.price, 63_496.28);
                assert_eq!(trade.timestamp.timestamp(), 1_646_802_000);
            }
            other => panic!("expected a trade, got {other:?}"),
        }
    }

    #[test]
    fn an_unmodeled_frame_keeps_its_payload() {
        // LULD has no captured payload, so the raw frame is what comes back.
        let frames = serde_json::json!([{"T": "l", "S": "AAPL", "u": 10.0, "d": 5.0}]);
        let payload = serde_json::to_vec(&frames).unwrap();

        match &decode(&payload)[0] {
            Ok(StreamMessage::Other { message_type, raw }) => {
                assert_eq!(message_type, "l");
                assert_eq!(raw["u"], 10.0);
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }
}
