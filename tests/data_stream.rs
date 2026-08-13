//! The live market data stream, against a websocket server that misbehaves on
//! demand.
//!
//! These are the behaviours a rewrite loses: hard-won fixes, invisible in the
//! type signatures and impossible to provoke against the real API. A mock can
//! drop a socket mid-stream, go mute while staying connected, and reject an
//! entitlement — Alpaca will not do those on request.

#![cfg(feature = "data")]

use std::sync::Arc;
use std::time::Duration;

use alpaca_sdk::Credentials;
use alpaca_sdk::data::{Channel, StockDataStream, StreamMessage};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// What the server does after the handshake.
#[derive(Clone)]
enum Script {
    /// Send these frame batches, then hold the socket open.
    Send(Vec<Value>),
    /// Send these, then drop the connection without closing cleanly.
    SendThenDrop(Vec<Value>),
    /// Complete the handshake and then say nothing at all.
    GoMute,
    /// Reject authentication.
    RejectAuth,
    /// Accept auth, then reject the subscription.
    RejectSubscription,
    /// Complete the whole handshake, then drop without ever sending market
    /// data — a connection that came up and fell straight over.
    DropAfterHandshake,
    /// Complete the handshake, stay silent for this long, then drop. A quiet
    /// session that was doing its job and got recycled server-side.
    HoldThenDrop(Duration),
}

/// Everything the server saw a client send, across all connections.
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

/// Starts a server that speaks the Alpaca handshake, and returns its endpoint.
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

                // The server greets first; the client waits for this.
                let connected = encode(&json!([{"T": "success", "msg": "connected"}]));
                if ws.send(Message::Binary(connected.into())).await.is_err() {
                    return;
                }

                // Then the client authenticates.
                let Some(Ok(auth)) = ws.next().await else {
                    return;
                };
                record(&seen, &auth).await;

                if matches!(script, Script::RejectAuth) {
                    let denied =
                        encode(&json!([{"T": "error", "code": 402, "msg": "auth failed"}]));
                    let _ = ws.send(Message::Binary(denied.into())).await;
                    return;
                }

                let ok = encode(&json!([{"T": "success", "msg": "authenticated"}]));
                if ws.send(Message::Binary(ok.into())).await.is_err() {
                    return;
                }

                // Then it subscribes.
                let Some(Ok(subscribe)) = ws.next().await else {
                    return;
                };
                record(&seen, &subscribe).await;

                match script {
                    Script::RejectSubscription => {
                        let denied = encode(&json!([{
                            "T": "error",
                            "code": 409,
                            "msg": "insufficient subscription"
                        }]));
                        let _ = ws.send(Message::Binary(denied.into())).await;
                        // Hold open: the client must stop on its own.
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    Script::GoMute => {
                        // Connected, subscribed, and silent.
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    Script::DropAfterHandshake => drop(ws),
                    Script::HoldThenDrop(hold) => {
                        tokio::time::sleep(hold).await;
                        drop(ws);
                    }
                    Script::Send(ref batches) | Script::SendThenDrop(ref batches) => {
                        let drop_after = matches!(script, Script::SendThenDrop(_));
                        for batch in batches {
                            let encoded = encode(batch);
                            if ws.send(Message::Binary(encoded.into())).await.is_err() {
                                return;
                            }
                        }
                        if drop_after {
                            // Drop without a close frame, the way a network
                            // failure does.
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

fn encode(value: &Value) -> Vec<u8> {
    rmp_serde::to_vec_named(value).unwrap()
}

async fn record(seen: &Received, message: &Message) {
    if let Message::Binary(bytes) = message
        && let Ok(value) = rmp_serde::from_slice::<Value>(bytes)
    {
        seen.lock().await.push(value);
    }
}

fn credentials() -> Credentials {
    Credentials::new("key", "secret").unwrap()
}

fn trade(symbol: &str, price: f64) -> Value {
    json!({
        "T": "t", "S": symbol,
        "t": "2022-03-18T14:03:31.960672Z",
        "p": price, "s": 10.0, "x": "V", "i": 1
    })
}

/// Collects up to `want` messages, giving up after `timeout`.
async fn collect(
    stream: impl futures_util::Stream<Item = alpaca_sdk::Result<StreamMessage>>,
    want: usize,
    timeout: Duration,
) -> Vec<alpaca_sdk::Result<StreamMessage>> {
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

// -------------------------------------------------------------- handshake

#[tokio::test]
async fn authenticates_then_subscribes() {
    let (endpoint, received, _) = serve(Script::Send(vec![json!([trade("AAPL", 170.5)])])).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let messages = collect(stream.run(), 1, Duration::from_secs(5)).await;
    assert_eq!(messages.len(), 1);

    let seen = received.lock().await;
    assert_eq!(seen.len(), 2, "expected an auth then a subscribe");

    // Auth carries the key pair as `key` and `secret`, not nested under data.
    assert_eq!(seen[0]["action"], "auth");
    assert_eq!(seen[0]["key"], "key");
    assert_eq!(seen[0]["secret"], "secret");

    assert_eq!(seen[1]["action"], "subscribe");
    assert_eq!(seen[1]["trades"], json!(["AAPL"]));
}

#[tokio::test]
async fn market_data_arrives_typed_with_its_symbol() {
    let (endpoint, _, _) = serve(Script::Send(vec![json!([
        trade("AAPL", 170.5),
        {"T": "q", "S": "MSFT", "t": "2022-03-18T14:03:31.960672Z",
         "bp": 1.0, "bs": 2.0, "ap": 3.0, "as": 4.0},
        {"T": "b", "S": "SPY", "t": "2022-03-18T14:03:00Z",
         "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 100.0}
    ])]))
    .await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let messages = collect(stream.run(), 3, Duration::from_secs(5)).await;
    assert_eq!(messages.len(), 3);

    match messages[0].as_ref().unwrap() {
        StreamMessage::Trade(t) => {
            // The symbol comes from the frame's S field, not a map key.
            assert_eq!(t.symbol, "AAPL");
            assert_eq!(t.price, 170.5);
        }
        other => panic!("expected a trade, got {other:?}"),
    }
    assert_eq!(
        messages[1].as_ref().unwrap().channel(),
        Some(Channel::Quotes)
    );
    assert_eq!(messages[2].as_ref().unwrap().symbol(), Some("SPY"));
}

#[tokio::test]
async fn the_subscribe_payload_omits_corrections_and_cancel_errors() {
    // They arrive with the trades subscription; naming them is an error.
    let (endpoint, received, _) = serve(Script::Send(vec![json!([trade("AAPL", 1.0)])])).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    stream.register_trade_corrections(["AAPL"]);
    stream.register_trade_cancels(["AAPL"]);

    let _ = collect(stream.run(), 1, Duration::from_secs(5)).await;

    let seen = received.lock().await;
    let subscribe = &seen[1];
    assert_eq!(subscribe["trades"], json!(["AAPL"]));
    assert!(subscribe.get("corrections").is_none(), "{subscribe}");
    assert!(subscribe.get("cancelErrors").is_none(), "{subscribe}");
}

#[tokio::test]
async fn corrections_and_cancels_still_arrive_without_being_subscribed() {
    let (endpoint, _, _) = serve(Script::Send(vec![json!([
        {"T": "c", "S": "AAPL", "t": "2022-03-18T14:03:31.960672Z", "x": "V",
         "op": 1.0, "os": 1.0, "oc": [], "cp": 2.0, "cs": 2.0, "cc": [], "z": "C"},
        {"T": "x", "S": "AAPL", "t": "2022-03-18T14:03:31.960672Z", "x": "V",
         "p": 1.0, "s": 1.0, "z": "C"}
    ])]))
    .await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let messages = collect(stream.run(), 2, Duration::from_secs(5)).await;

    assert!(matches!(
        messages[0].as_ref().unwrap(),
        StreamMessage::Correction(_)
    ));
    assert!(matches!(
        messages[1].as_ref().unwrap(),
        StreamMessage::CancelError(_)
    ));
}

// ------------------------------------------------------- fatal conditions

#[tokio::test]
async fn an_insufficient_subscription_stops_the_stream_for_good() {
    // Retrying an entitlement failure never succeeds and burns the one
    // connection Alpaca allows, so the stream ends rather than reconnecting.
    let (endpoint, _, connections) = serve(Script::RejectSubscription).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let mut messages = Box::pin(stream.run());
    let first = tokio::time::timeout(Duration::from_secs(5), messages.next())
        .await
        .expect("should not hang")
        .expect("should yield the error");

    match first.unwrap() {
        StreamMessage::Error(error) => assert!(error.message.contains("insufficient subscription")),
        other => panic!("expected an error frame, got {other:?}"),
    }

    // The stream must end rather than reconnect.
    let next = tokio::time::timeout(Duration::from_secs(3), messages.next()).await;
    assert!(matches!(next, Ok(None)), "the stream should have ended");
    assert_eq!(
        connections.lock().await.len(),
        1,
        "it must not have reconnected"
    );
}

#[tokio::test]
async fn a_rejected_authentication_stops_the_stream() {
    let (endpoint, _, connections) = serve(Script::RejectAuth).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let mut messages = Box::pin(stream.run());
    let first = tokio::time::timeout(Duration::from_secs(5), messages.next())
        .await
        .expect("should not hang")
        .expect("should yield an error");

    // The variant matters as well as the failure: `is_fatal` reads the message
    // out of `Error::Stream` to decide whether to reconnect, so reporting this
    // one as anything else would turn a permanent rejection into a retry loop.
    match first {
        Err(alpaca_sdk::Error::Stream(message)) => {
            assert!(message.contains("auth failed"), "{message}");
        }
        other => panic!("expected Error::Stream, got {other:?}"),
    }

    let next = tokio::time::timeout(Duration::from_secs(3), messages.next()).await;
    assert!(matches!(next, Ok(None)), "the stream should have ended");
    assert_eq!(connections.lock().await.len(), 1);
}

// ------------------------------------------------------------- reconnects

#[tokio::test]
async fn a_dropped_socket_reconnects() {
    let (endpoint, _, connections) =
        serve(Script::SendThenDrop(vec![json!([trade("AAPL", 170.5)])])).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    // Two trades means the socket dropped and the client came back for more.
    let messages = collect(stream.run(), 2, Duration::from_secs(15)).await;

    assert!(
        messages.len() >= 2,
        "expected a reconnect, got {messages:?}"
    );
    assert!(
        connections.lock().await.len() >= 2,
        "the server should have seen a second connection"
    );
}

#[tokio::test]
async fn a_reconnect_resends_the_subscription() {
    let (endpoint, received, _) =
        serve(Script::SendThenDrop(vec![json!([trade("AAPL", 1.0)])])).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL", "MSFT"]);

    let _ = collect(stream.run(), 2, Duration::from_secs(15)).await;

    let seen = received.lock().await;
    let subscribes: Vec<_> = seen.iter().filter(|m| m["action"] == "subscribe").collect();

    assert!(subscribes.len() >= 2, "the subscription was not resent");
    for subscribe in subscribes {
        assert_eq!(subscribe["trades"], json!(["AAPL", "MSFT"]));
    }
}

#[tokio::test]
async fn a_mute_connection_reconnects_when_a_data_timeout_is_set() {
    // Connected, authenticated, subscribed, and silent. The transport keepalive
    // cannot catch this: the socket is healthy, there is simply no data.
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    stream
        .data_timeout(Duration::from_millis(300))
        .expect("a positive timeout");

    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(8), messages.next()).await;

    assert!(
        connections.lock().await.len() >= 2,
        "a mute stream should have been reconnected, saw {} connection(s)",
        connections.lock().await.len()
    );
}

#[tokio::test]
async fn a_mute_connection_is_left_alone_without_a_data_timeout() {
    // The default. A quiet news or bars subscription must not reconnect on a
    // timer, which is why the timeout is off unless asked for.
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(3), messages.next()).await;

    assert_eq!(
        connections.lock().await.len(),
        1,
        "a quiet stream should not have been reconnected"
    );
}

#[tokio::test]
async fn a_data_timeout_must_be_positive() {
    let mut stream = StockDataStream::with_endpoint(credentials(), "ws://127.0.0.1:1");
    assert!(stream.data_timeout(Duration::ZERO).is_err());
}

// ----------------------------------------------------------- misc framing

#[tokio::test]
async fn running_without_a_subscription_is_an_error() {
    // Spinning until something is subscribed would hang; saying so is more
    // useful.
    let stream = StockDataStream::with_endpoint(credentials(), "ws://127.0.0.1:1");

    let messages = collect(stream.run(), 1, Duration::from_secs(3)).await;
    assert_eq!(messages.len(), 1);
    assert!(messages[0].is_err());
}

#[tokio::test]
async fn a_subscription_acknowledgement_is_reported() {
    let (endpoint, _, _) = serve(Script::Send(vec![json!([
        {"T": "subscription", "trades": ["AAPL"], "quotes": [], "bars": []}
    ])]))
    .await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let messages = collect(stream.run(), 1, Duration::from_secs(5)).await;

    match messages[0].as_ref().unwrap() {
        StreamMessage::Subscription(subs) => assert_eq!(subs.trades, ["AAPL"]),
        other => panic!("expected a subscription frame, got {other:?}"),
    }
}

#[tokio::test]
async fn a_non_fatal_error_frame_does_not_end_the_stream() {
    let (endpoint, _, _) = serve(Script::Send(vec![
        json!([{"T": "error", "code": 405, "msg": "symbol not found"}]),
        json!([trade("AAPL", 170.5)]),
    ]))
    .await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);

    let messages = collect(stream.run(), 2, Duration::from_secs(5)).await;

    assert!(matches!(
        messages[0].as_ref().unwrap(),
        StreamMessage::Error(_)
    ));
    assert!(matches!(
        messages[1].as_ref().unwrap(),
        StreamMessage::Trade(_)
    ));
}

#[tokio::test]
async fn the_stock_stream_rejects_a_feed_without_a_live_socket() {
    // Only iex and sip carry one.
    assert!(
        StockDataStream::new(credentials(), alpaca_sdk::data::DataFeed::Otc).is_err(),
        "otc has no live stream"
    );
    assert!(StockDataStream::new(credentials(), alpaca_sdk::data::DataFeed::Iex).is_ok());
    assert!(StockDataStream::new(credentials(), alpaca_sdk::data::DataFeed::Sip).is_ok());
}

// ------------------------------------------------- the staleness clock itself
//
// Both `data_timeout` tests above use 300ms, which is *below* the 5s internal
// poll interval — so both took the right branch by accident, and neither could
// tell a real clock from one that fired on every poll. These two use a timeout
// above the poll interval, which is the only place the difference shows.

/// A timeout longer than the internal poll interval must be honoured as written.
/// Before the clock existed, a 5s poll elapsing *was* the staleness signal, so
/// any timeout above 5s behaved as exactly 5s: an overnight stock stream
/// reconnected every five seconds against an endpoint allowing one connection
/// per account.
#[tokio::test]
async fn a_timeout_longer_than_the_poll_interval_is_not_fired_early() {
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    // Well above RECEIVE_POLL (5s).
    stream
        .data_timeout(Duration::from_secs(30))
        .expect("a positive timeout");

    let mut messages = Box::pin(stream.run());
    // Long enough for six poll windows to elapse, nowhere near the 30s timeout.
    let _ = tokio::time::timeout(Duration::from_secs(12), messages.next()).await;

    assert_eq!(
        connections.lock().await.len(),
        1,
        "a 30s timeout must not reconnect within 12s; the clock is being read \
         from one poll window rather than from elapsed time"
    );
}

/// And it does still fire once the timeout genuinely elapses.
#[tokio::test]
async fn a_timeout_longer_than_the_poll_interval_still_fires_eventually() {
    let (endpoint, _, connections) = serve(Script::GoMute).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    stream
        .data_timeout(Duration::from_secs(7))
        .expect("a positive timeout");

    let mut messages = Box::pin(stream.run());
    // 7s timeout plus a reconnect, so 12s is ample; the budget is the test's
    // wall-clock cost, and this one dominates `just check`.
    let _ = tokio::time::timeout(Duration::from_secs(12), messages.next()).await;

    assert!(
        connections.lock().await.len() >= 2,
        "a 7s timeout should have reconnected within 12s, saw {} connection(s)",
        connections.lock().await.len()
    );
}

/// A quiet session that stayed up and was then recycled clears the failure
/// count.
///
/// This is the one case where the old loop and the new one differ. The original
/// incremented the counter after a *successful* connect and reset it only on a
/// market data message — so a legitimately silent stream whose server recycles
/// it climbed to the 30s ceiling and stayed there.
///
/// The assertion is on the gap between the last two reconnects, not on how many
/// fit in a window: the gap is set by the client's own timer, so it holds on a
/// slow runner too. With the curve reset each time the delay stays in
/// [0.5s, 1s); without it, by the fifth reconnect it is past 4s.
#[tokio::test]
async fn a_quiet_session_that_stayed_up_clears_the_failure_count() {
    let (endpoint, _, connections) = serve(Script::HoldThenDrop(Duration::from_millis(300))).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    stream
        .stable_session(Duration::from_millis(150))
        .expect("a positive duration");

    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while messages.next().await.is_some() {}
    })
    .await;

    let times = connections.lock().await.clone();
    assert!(
        times.len() >= 3,
        "expected several reconnects, saw {}",
        times.len()
    );
    // Compared against the *first* gap rather than a fixed threshold: under a
    // reset every gap is one `min_backoff` draw, so the ratio is ~1; under a
    // growing curve the last gap is several doublings larger. A ratio is
    // immune to how fast the runner is, where an absolute bound is not.
    let gap = last_gap(&times).expect("at least two connections");
    let first = times[1].duration_since(times[0]);
    assert!(
        gap < first * 3,
        "a session that stayed up past `stable_session` should have cleared the \
         backoff curve, but the last gap was {gap:?} against a first gap of \
         {first:?} — the curve is still growing"
    );
}

/// And the other direction, which resetting on *connect* alone would get wrong:
/// a server that completes the handshake and immediately hangs up has not done
/// its job, however far it got. Treating that as success pinned the delay at
/// ~1s forever — roughly one connection a second at an endpoint that allows one
/// per account.
#[tokio::test]
async fn a_session_that_dropped_immediately_does_advance_the_backoff_curve() {
    let (endpoint, _, connections) = serve(Script::DropAfterHandshake).await;

    let mut stream = StockDataStream::with_endpoint(credentials(), endpoint);
    stream.subscribe_trades(["AAPL"]);
    stream
        .stable_session(Duration::from_millis(150))
        .expect("a positive duration");

    let mut messages = Box::pin(stream.run());
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while messages.next().await.is_some() {}
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
         was only {gap:?} — the curve is pinned at its minimum"
    );
}

#[tokio::test]
async fn a_stable_session_must_be_positive() {
    let mut stream = StockDataStream::with_endpoint(credentials(), "ws://127.0.0.1:1");
    assert!(stream.stable_session(Duration::ZERO).is_err());
}
