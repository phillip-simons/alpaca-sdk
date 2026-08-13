//! The trade update stream, against a websocket server speaking the trading
//! protocol — which is not the market data protocol, in four separate ways.

#![cfg(feature = "trading")]

use std::sync::Arc;
use std::time::Duration;

use alpaca_sdk::Credentials;
use alpaca_sdk::trading::{TradeStreamMessage, TradingStream};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
enum Script {
    /// Authorize, then send these frames and hold the socket open.
    Send(Vec<Value>),
    /// Authorize, send these, then drop without a close frame.
    SendThenDrop(Vec<Value>),
    /// Refuse authorization.
    RejectAuth,
    /// Authorize, then say nothing.
    GoMute,
    /// Authorize, stay silent for this long, then drop. A quiet session that
    /// was doing its job and got recycled server-side — which on this stream is
    /// the *normal* shape, because a silent account is normal.
    HoldThenDrop(Duration),
}

type Received = Arc<Mutex<Vec<Value>>>;

/// When each connection was accepted.
///
/// Timestamps rather than a count: the property the backoff curve controls is
/// the *gap* between reconnects, and asserting on a count inside a fixed window
/// makes the test a race against the runner. The gap is set by the client's own
/// timer, so it holds on a slow machine too.
type Connections = Arc<Mutex<Vec<std::time::Instant>>>;

/// The delay between the last two connections.
///
/// `None` if fewer than two were made, which is itself a failure for these
/// tests and is reported as such by the caller.
fn last_gap(times: &[std::time::Instant]) -> Option<std::time::Duration> {
    match times {
        [.., previous, last] => Some(last.duration_since(*previous)),
        _ => None,
    }
}

async fn serve(script: Script) -> (String, Received, Connections) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());

    let received: Received = Arc::new(Mutex::new(Vec::new()));
    let connections: Connections = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&received);
    let counter = Arc::clone(&connections);

    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let seen = Arc::clone(&seen);
            let counter = Arc::clone(&counter);
            let script = script.clone();

            tokio::spawn(async move {
                counter.lock().await.push(std::time::Instant::now());
                let Ok(mut ws) = tokio_tungstenite::accept_async(socket).await else {
                    return;
                };

                // No greeting: this endpoint expects `authenticate` immediately.
                let Some(Ok(auth)) = ws.next().await else {
                    return;
                };
                record(&seen, &auth).await;

                if matches!(script, Script::RejectAuth) {
                    let denied = json!({"stream": "authorization",
                                        "data": {"status": "unauthorized", "action": "authenticate"}});
                    let _ = ws.send(Message::Text(denied.to_string().into())).await;
                    return;
                }

                let ok = json!({"stream": "authorization",
                                "data": {"status": "authorized", "action": "authenticate"}});
                if ws.send(Message::Text(ok.to_string().into())).await.is_err() {
                    return;
                }

                // Then it listens.
                let Some(Ok(listen)) = ws.next().await else {
                    return;
                };
                record(&seen, &listen).await;

                match script {
                    Script::GoMute => tokio::time::sleep(Duration::from_secs(30)).await,
                    Script::HoldThenDrop(hold) => {
                        tokio::time::sleep(hold).await;
                        drop(ws);
                    }
                    Script::Send(ref frames) | Script::SendThenDrop(ref frames) => {
                        let drop_after = matches!(script, Script::SendThenDrop(_));
                        for frame in frames {
                            if ws
                                .send(Message::Text(frame.to_string().into()))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        if drop_after {
                            drop(ws);
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    Script::RejectAuth => unreachable!("handled above"),
                }
            });
        }
    });

    (endpoint, received, connections)
}

async fn record(seen: &Received, message: &Message) {
    if let Message::Text(text) = message
        && let Ok(value) = serde_json::from_str::<Value>(text)
    {
        seen.lock().await.push(value);
    }
}

fn credentials() -> Credentials {
    Credentials::new("my-key", "my-secret").unwrap()
}

fn order() -> Value {
    json!({
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
    })
}

fn update(event: &str) -> Value {
    json!({
        "stream": "trade_updates",
        "data": {
            "event": event,
            "timestamp": "2021-03-16T18:38:01.942282Z",
            "order": order(),
            "price": "170.5",
            "qty": "10"
        }
    })
}

async fn collect(
    stream: impl futures_util::Stream<Item = alpaca_sdk::Result<TradeStreamMessage>>,
    want: usize,
    timeout: Duration,
) -> Vec<alpaca_sdk::Result<TradeStreamMessage>> {
    let mut messages = Box::pin(stream);
    let mut collected = Vec::new();

    let _ = tokio::time::timeout(timeout, async {
        while collected.len() < want {
            match messages.next().await {
                Some(message) => collected.push(message),
                None => break,
            }
        }
    })
    .await;

    collected
}

// ------------------------------------------------------------- handshake

#[tokio::test]
async fn authenticates_with_the_trading_envelope_then_listens() {
    // This is the shape that differs from market data, and the reason this test
    // asserts the payload rather than just the outcome.
    let (endpoint, received, _) = serve(Script::Send(vec![update("fill")])).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let messages = collect(stream.run(), 1, Duration::from_secs(5)).await;
    assert_eq!(messages.len(), 1);

    let seen = received.lock().await;
    assert_eq!(seen.len(), 2, "expected an authenticate then a listen");

    // Nested under `data`, and named key_id/secret_key — not key/secret.
    assert_eq!(seen[0]["action"], "authenticate");
    assert_eq!(seen[0]["data"]["key_id"], "my-key");
    assert_eq!(seen[0]["data"]["secret_key"], "my-secret");
    assert!(seen[0].get("key").is_none(), "market data shape leaked in");

    assert_eq!(seen[1]["action"], "listen");
    assert_eq!(seen[1]["data"]["streams"], json!(["trade_updates"]));
}

#[tokio::test]
async fn sends_authenticate_without_waiting_for_a_greeting() {
    // The market data stream blocks on {"T":"success","msg":"connected"}. This
    // one must not, or it would hang forever against a real server.
    let (endpoint, received, _) = serve(Script::Send(vec![update("fill")])).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let _ = collect(stream.run(), 1, Duration::from_secs(5)).await;

    assert_eq!(
        received.lock().await[0]["action"],
        "authenticate",
        "the client should speak first"
    );
}

#[tokio::test]
async fn frames_are_json_not_msgpack() {
    let (endpoint, received, _) = serve(Script::Send(vec![update("fill")])).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let _ = collect(stream.run(), 1, Duration::from_secs(5)).await;

    // The server only records frames it could parse as JSON text; msgpack
    // binary frames would never have landed here.
    assert_eq!(received.lock().await.len(), 2);
}

// -------------------------------------------------------------- messages

#[tokio::test]
async fn trade_updates_arrive_typed() {
    let (endpoint, _, _) = serve(Script::Send(vec![
        update("new"),
        update("fill"),
        update("canceled"),
    ]))
    .await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let messages = collect(stream.run(), 3, Duration::from_secs(5)).await;

    assert_eq!(messages.len(), 3);
    match messages[1].as_ref().unwrap() {
        TradeStreamMessage::TradeUpdate(update) => {
            assert_eq!(update.order.symbol.as_deref(), Some("AAPL"));
            assert_eq!(update.price, Some(rust_decimal::Decimal::new(1705, 1)));
            assert_eq!(
                update.order.status,
                alpaca_sdk::trading::OrderStatus::Filled
            );
        }
        other => panic!("expected a trade update, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unknown_event_does_not_break_the_stream() {
    // A stream that failed on an unfamiliar event would break the moment
    // Alpaca added one; the catch-all keeps it usable.
    let (endpoint, _, _) = serve(Script::Send(vec![update("some_new_event")])).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let messages = collect(stream.run(), 1, Duration::from_secs(5)).await;

    match messages[0].as_ref().unwrap() {
        TradeStreamMessage::TradeUpdate(update) => assert_eq!(
            update.event,
            alpaca_sdk::trading::TradeEvent::Unknown("some_new_event".to_owned())
        ),
        other => panic!("expected a trade update, got {other:?}"),
    }
}

// ------------------------------------------------------------ reconnects

#[tokio::test]
async fn a_rejected_authorization_stops_the_stream() {
    let (endpoint, _, connections) = serve(Script::RejectAuth).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let mut messages = Box::pin(stream.run());

    let first = tokio::time::timeout(Duration::from_secs(5), messages.next())
        .await
        .expect("should not hang")
        .expect("should yield an error");
    // `Credentials`, not `Stream`: the socket and the handshake both worked, and
    // the server said no. A stream failure would reconnect; this must not.
    assert!(
        matches!(first, Err(alpaca_sdk::Error::Credentials(_))),
        "expected Error::Credentials, got {first:?}"
    );

    let next = tokio::time::timeout(Duration::from_secs(3), messages.next()).await;
    assert!(matches!(next, Ok(None)), "the stream should have ended");
    assert_eq!(
        connections.lock().await.len(),
        1,
        "bad credentials must not be retried"
    );
}

#[tokio::test]
async fn a_dropped_socket_reconnects_and_relistens() {
    let (endpoint, received, connections) = serve(Script::SendThenDrop(vec![update("fill")])).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let messages = collect(stream.run(), 2, Duration::from_secs(15)).await;

    assert!(
        messages.len() >= 2,
        "expected a reconnect, got {messages:?}"
    );
    assert!(connections.lock().await.len() >= 2);

    let seen = received.lock().await;
    let listens = seen.iter().filter(|m| m["action"] == "listen").count();
    assert!(listens >= 2, "the listen was not resent after reconnecting");
}

#[tokio::test]
async fn a_mute_stream_is_left_alone_by_default() {
    // An account that simply is not trading is silent for good reasons.
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint);
    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(3), messages.next()).await;

    assert_eq!(connections.lock().await.len(), 1);
}

#[tokio::test]
async fn a_mute_stream_reconnects_when_a_data_timeout_is_set() {
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint)
        .data_timeout(Duration::from_millis(300))
        .expect("a positive timeout");

    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(8), messages.next()).await;

    assert!(
        connections.lock().await.len() >= 2,
        "a mute stream should have been reconnected, saw {}",
        connections.lock().await.len()
    );
}

// ------------------------------------------- the staleness clock and backoff
//
// This stream carries fills, and it received the same two fixes as the market
// data stream with no test of its own. The `data_timeout` test below used 300ms,
// under the 5s internal poll interval, so it could not tell a real clock from
// one that fired on every poll.

/// A timeout longer than the internal poll interval must be honoured as written.
/// Before the clock existed, one elapsed 5s read *was* the staleness signal, so
/// any timeout above 5s behaved as exactly 5s.
#[tokio::test]
async fn a_timeout_longer_than_the_poll_interval_is_not_fired_early() {
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint)
        .data_timeout(Duration::from_secs(30))
        .expect("a positive timeout");

    let mut updates = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(12), updates.next()).await;

    assert_eq!(
        connections.lock().await.len(),
        1,
        "a 30s timeout must not reconnect within 12s; the clock is being read \
         from one poll window rather than from elapsed time"
    );
}

/// A quiet session that stayed up and was then recycled clears the failure
/// count. This is the case the old loop got wrong on *this* stream in
/// particular: it reset only on a trade update, and a silent account never
/// sends one — so a few server-side recycles left every reconnect waiting the
/// 30s maximum, and there is no replay here, so a fill in that window is gone.
///
/// Asserted on the gap between the last two reconnects rather than a count in a
/// window, so a slow runner cannot fail a correct implementation.
#[tokio::test]
async fn a_quiet_session_that_stayed_up_clears_the_failure_count() {
    let (endpoint, _, connections) = serve(Script::HoldThenDrop(Duration::from_millis(300))).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint)
        .stable_session(Duration::from_millis(150))
        .expect("a positive duration");

    let mut updates = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while updates.next().await.is_some() {}
    })
    .await;

    let times = connections.lock().await.clone();
    assert!(
        times.len() >= 3,
        "expected several reconnects, saw {}",
        times.len()
    );
    let gap = last_gap(&times).expect("at least two connections");
    assert!(
        gap < Duration::from_secs(2),
        "a session that stayed up past `stable_session` should have cleared the \
         backoff curve, but the last gap was {gap:?}"
    );
}

/// And a server that authorizes and immediately hangs up is still a failure.
#[tokio::test]
async fn a_session_that_dropped_immediately_does_advance_the_backoff_curve() {
    let (endpoint, _, connections) = serve(Script::SendThenDrop(Vec::new())).await;

    let stream = TradingStream::with_endpoint(credentials(), endpoint)
        .stable_session(Duration::from_millis(150))
        .expect("a positive duration");

    let mut updates = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while updates.next().await.is_some() {}
    })
    .await;

    let times = connections.lock().await.clone();
    assert!(
        times.len() >= 3,
        "expected several reconnects, saw {}",
        times.len()
    );
    let gap = last_gap(&times).expect("at least two connections");
    assert!(
        gap >= Duration::from_secs(1),
        "a connect-then-drop server should be backed off from, but the last gap \
         was only {gap:?}"
    );
}

#[tokio::test]
async fn a_stable_session_must_be_positive() {
    let stream = TradingStream::with_endpoint(credentials(), "ws://127.0.0.1:1");
    assert!(stream.stable_session(Duration::ZERO).is_err());
}
