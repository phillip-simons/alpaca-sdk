//! Read a trading account: balances, open positions, open orders.
//!
//! ```text
//! APCA_API_KEY_ID=... APCA_API_SECRET_KEY=... cargo run --example account
//! ```
//!
//! Strictly read-only. `TradingClient::new`'s second argument selects the paper
//! environment; pass `false` and the same code reads a live account instead.

use alpaca_sdk::trading::TradingClient;
use alpaca_sdk::{Credentials, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Reads APCA_API_KEY_ID and APCA_API_SECRET_KEY, and reports which one is
    // missing rather than failing at the first request.
    let credentials = Credentials::from_env()?;
    let client = TradingClient::new(&credentials, true)?;

    let account = client.get_account().await?;
    println!("account {}", account.account_number);
    println!("  status   {:?}", account.status);
    println!("  currency {:?}", account.currency);
    // Money crosses the wire as a string and arrives as a `Decimal`, so this is
    // exact rather than the nearest float to it.
    println!("  equity   {:?}", account.equity);
    println!("  buying power {:?}", account.buying_power);

    let positions = client.get_all_positions().await?;
    println!("\n{} open position(s)", positions.len());
    for position in &positions {
        println!(
            "  {:<8} qty={} market_value={:?} unrealized_pl={:?}",
            position.symbol, position.qty, position.market_value, position.unrealized_pl
        );
    }

    // `None` asks for the default filter — open orders. A `GetOrdersRequest`
    // narrows it by status, side, symbol or window.
    let orders = client.get_orders(None).await?;
    println!("\n{} open order(s)", orders.len());
    for order in &orders {
        // Almost every field on an order is optional: Alpaca omits `symbol` on
        // a multi-leg order, where the legs carry it instead.
        println!(
            "  {:<8} {:?} {:?} qty={:?} status={:?}",
            order.symbol.as_deref().unwrap_or("—"),
            order.side,
            order.order_type,
            order.qty,
            order.status
        );
    }

    Ok(())
}
