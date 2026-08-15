//! Stream live crypto trades over the market data websocket.
//!
//! ```text
//! APCA_API_KEY_ID=... APCA_API_SECRET_KEY=... cargo run --example crypto_stream
//! ```
//!
//! Crypto is used rather than stocks so the example prints something whatever
//! time it is run. The stock stream is the same shape — `StockDataStream`, and
//! a feed the account is entitled to.
//!
//! Stops after five trades or thirty seconds, whichever comes first.

use std::time::Duration;

use alpaca_sdk::data::{CryptoDataStream, CryptoFeed, StreamMessage};
use alpaca_sdk::{Credentials, Result};
use futures_util::StreamExt as _;

#[tokio::main]
async fn main() -> Result<()> {
    let mut stream = CryptoDataStream::new(Credentials::from_env()?, CryptoFeed::Us)?;

    // Subscriptions are declared before the socket opens; the stream replays
    // them on every reconnect, so a dropped connection resubscribes itself.
    stream.subscribe_trades(["BTC/USD", "ETH/USD"]);

    let mut messages = Box::pin(stream.run());
    let mut seen = 0_usize;

    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(message) = messages.next().await {
            match message {
                Ok(StreamMessage::Subscription(subs)) => {
                    println!("subscribed to trades: {:?}", subs.trades);
                }
                Ok(StreamMessage::Trade(trade)) => {
                    println!(
                        "{} {} @ {} size={}",
                        trade.timestamp, trade.symbol, trade.price, trade.size
                    );
                    seen += 1;
                    if seen >= 5 {
                        return;
                    }
                }
                // An error frame from the server, as opposed to a broken socket.
                Ok(StreamMessage::Error(error)) => println!("server said: {error:?}"),
                // The enum is #[non_exhaustive]; new frame types stay handled.
                Ok(_) => {}
                Err(error) => {
                    println!("stream error: {error}");
                    return;
                }
            }
        }
    })
    .await;

    println!("\n{seen} trade(s); timed out: {}", outcome.is_err());
    Ok(())
}
