//! Fetch daily bars for a few symbols from the historical market data API.
//!
//! ```text
//! APCA_API_KEY_ID=... APCA_API_SECRET_KEY=... cargo run --example historical_bars
//! ```
//!
//! Market data is a separate API from trading, with its own client and its own
//! entitlements: the free plan reaches IEX, and SIP needs a paid one. This asks
//! for IEX so it works on any key.

use alpaca_sdk::data::{DataFeed, StockBarsRequest, StockHistoricalDataClient, TimeFrame};
use alpaca_sdk::{Credentials, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = StockHistoricalDataClient::new(&Credentials::from_env()?)?;

    // `limit` caps the total across every page, not the page size — the client
    // walks the pagination cursor for you and stops when the cap is reached.
    let request = StockBarsRequest::new(["AAPL", "MSFT"], TimeFrame::day())
        .feed(DataFeed::Iex)
        .limit(5);

    let bars = client.get_stock_bars(&request).await?;

    // A `BarSet` is keyed by symbol. A symbol with no data in the window is
    // absent rather than present-and-empty.
    for (symbol, series) in &bars {
        println!("{symbol}: {} bar(s)", series.len());
        for bar in series {
            println!(
                "  {}  o={} h={} l={} c={} v={}",
                bar.timestamp.date_naive(),
                bar.open,
                bar.high,
                bar.low,
                bar.close,
                bar.volume
            );
        }
    }

    Ok(())
}
