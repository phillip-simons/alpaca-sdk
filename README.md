# alpaca-sdk

[![crates.io](https://img.shields.io/crates/v/alpaca-sdk.svg)](https://crates.io/crates/alpaca-sdk)
[![docs.rs](https://docs.rs/alpaca-sdk/badge.svg)](https://docs.rs/alpaca-sdk)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/phillip-simons/alpaca-sdk/blob/main/LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](#minimum-supported-rust-version)

Unofficial Rust SDK for the [Alpaca](https://alpaca.markets) trading, market
data, and broker APIs — **251 of the 253 routes** the published specs document,
across REST, WebSocket and server-sent events.

It targets [the API itself][docs] rather than any one SDK's reading of it. That
distinction is the point of the crate, and [what it means in practice](#how-this-is-verified)
is written down below.

> **Unofficial.** Not affiliated with, endorsed by, or sponsored by Alpaca
> Securities LLC. See [NOTICE](https://github.com/phillip-simons/alpaca-sdk/blob/main/NOTICE).

## Install

```sh
cargo add alpaca-sdk
```

Trading and market data are on by default. The broker API, a synchronous
wrapper, and `polars` conversion are [feature flags](#feature-flags).

## Quick start

Credentials come from `APCA_API_KEY_ID` and `APCA_API_SECRET_KEY`. The `true`
selects Alpaca's paper environment; pass `false` for a live account.

```rust,no_run
use alpaca_sdk::trading::TradingClient;
use alpaca_sdk::{Credentials, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = TradingClient::new(&Credentials::from_env()?, true)?;

    let account = client.get_account().await?;
    println!("equity: {:?}", account.equity);

    for position in client.get_all_positions().await? {
        println!("{} {} @ {:?}", position.symbol, position.qty, position.current_price);
    }

    Ok(())
}
```

## Placing an order

Every order shape has a constructor — `market`, `limit`, `stop`, `stop_limit`,
`trailing_stop`, `multi_leg`, and the `bracket` / `oco` / `oto` classes on top
of them.

```rust,no_run
use alpaca_sdk::trading::{OrderAmount, OrderRequest, OrderSide, TimeInForce, TradingClient};
use alpaca_sdk::{Credentials, Decimal, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = TradingClient::new(&Credentials::from_env()?, true)?;

    // Prices and quantities are `Decimal`, never `f64` — Alpaca sends them as
    // strings, and a float would quietly round the order you meant to place.
    let order = OrderRequest::limit(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::ONE),
        TimeInForce::Day,
        Decimal::new(18550, 2), // 185.50
    );

    let placed = client.submit_order(&order).await?;
    println!("{} is {:?}", placed.id, placed.status);

    Ok(())
}
```

## Market data

Market data is a separate API with its own client and its own entitlements: the
free plan reaches IEX, and SIP needs a paid one.

```rust,no_run
use alpaca_sdk::data::{DataFeed, StockBarsRequest, StockHistoricalDataClient, TimeFrame};
use alpaca_sdk::{Credentials, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = StockHistoricalDataClient::new(&Credentials::from_env()?)?;

    // `limit` caps the total across every page, not the page size — the
    // pagination cursor is walked for you.
    let request = StockBarsRequest::new(["AAPL", "MSFT"], TimeFrame::day())
        .feed(DataFeed::Iex)
        .limit(5);

    for (symbol, bars) in &client.get_stock_bars(&request).await? {
        println!("{symbol}: {} bars, last close {}", bars.len(), bars[bars.len() - 1].close);
    }

    Ok(())
}
```

## Live streams

Subscriptions are declared before the socket opens, and replayed on every
reconnect — a dropped connection resubscribes itself.

```rust,no_run
use alpaca_sdk::data::{CryptoDataStream, CryptoFeed, StreamMessage};
use alpaca_sdk::{Credentials, Result};
use futures_util::StreamExt as _;

#[tokio::main]
async fn main() -> Result<()> {
    let mut stream = CryptoDataStream::new(Credentials::from_env()?, CryptoFeed::Us)?;
    stream.subscribe_trades(["BTC/USD"]);

    let mut messages = Box::pin(stream.run());
    while let Some(message) = messages.next().await {
        if let Ok(StreamMessage::Trade(trade)) = message {
            println!("{} @ {}", trade.symbol, trade.price);
        }
    }

    Ok(())
}
```

`examples/` has these as runnable programs — `cargo run --example account`,
`historical_bars`, or `crypto_stream`.

## Feature flags

| Feature | Default | Enables |
|---|---|---|
| `trading` | ✅ | Trading REST client and the trade-update stream |
| `data` | ✅ | Historical and live market data |
| `broker` | | Broker API, including its nine SSE event streams |
| `blocking` | | Synchronous wrapper over any async client |
| `polars` | | `DataFrame` conversion for market data collections |
| `rustls-tls` | ✅ | TLS via rustls |
| `native-tls` | | TLS via the platform library |

Streams stay async even under `blocking`: a blocking iterator over a live feed
deadlocks as soon as the caller is slower than the socket's read buffer.

## How the types behave

The decisions a caller actually runs into, and why each one is the way it is.

- **Money that crosses the wire as a string is `Decimal`.** Market data floats
  that arrive as JSON numbers stay `f64`. Reading a string price as a float
  loses precision on the one field where it matters. The crate re-exports both
  the type and `rust_decimal` itself, as `alpaca_sdk::Decimal` and
  `alpaca_sdk::rust_decimal` — reach for those rather than adding the dependency
  separately, or a version mismatch gives you two incompatible `Decimal` types
  with the same name.
- **Unknown enum values deserialize into `Unknown` rather than failing.** Alpaca
  adds values without warning, and a new order status should cost you a match
  arm rather than a decode error in production.
- **Unknown response fields are ignored**, for the same reason.
- **Paginated endpoints offer two methods.** `get_x` fetches one page;
  `get_all_x` walks every page, with an optional cap.
- **Request structs are `#[non_exhaustive]`.** Build one with `new` or
  `default` and assign fields:

  ```rust,no_run
  use alpaca_sdk::trading::GetOrdersRequest;

  let mut filter = GetOrdersRequest::default();
  filter.limit = Some(50);
  ```

  Alpaca documents new query parameters regularly. This is what lets one arrive
  as a new field instead of as a breaking change.
- **Retries follow [Alpaca's own guidance][rate-limits]:** 429 and 504, three
  attempts after the first, ~1s doubling to a 30s ceiling with jitter. A
  response carrying `Retry-After` overrides that curve, clamped to the same
  ceiling.

  **A request that acts is never replayed.** A 504 means the gateway stopped
  waiting for the answer, not that nothing happened — so `submit_order`,
  `create_journal`, and the position-closing routes are reported rather than
  retried, whatever the status list says. Only a 429 is replayed regardless,
  because the rate limiter refuses the request before anything acts on it. If
  you re-issue one of these yourself, that is the same hazard in your own hands.
- **`request_raw` is the escape hatch** for routes this crate does not wrap. The
  transport is public and hands back the body undecoded.

## How this is verified

Alpaca publishes an API reference, vendors `OpenAPI` specs, and ships five SDKs —
and they do not always agree. This crate ranks its sources by how close each is
to the wire:

1. a **captured response** beats a specification;
2. a **specification** beats another SDK;
3. only the **published reference** says whether a route is still *current*.

That order is not academic. Three event streams were in the specs, looked
healthy from the crate's side, and had been switched off; the reference was the
only source that said so.

`fixtures/` holds 227 real API responses, and every model is checked against
them. `just coverage`, `just parameters` and `just enums-drift` diff this crate
against the specs and the reference by machine, because reading does not scale
to 251 routes — each of the three found something a careful read had missed.
Route results are checked in at [COVERAGE.md](COVERAGE.md).

### What is *not* verified against a live server

Honest limits, and properties of what could be checked rather than of what was
built:

- **The broker routes have never met a real response.** This account has no
  broker sandbox key, so all 153 are derived from captured payloads, the
  reference and the specs.
- **The `CIP*` models are probably unverifiable** — the sandbox answers 404 for
  those routes.
- **Forex, indices and logos** answer `403 insufficient grants` even on a plan
  that reaches SIP; they are per-product entitlements.

Treat a decode failure on a first real payload in those areas as expected work
rather than a regression — and please [report it](https://github.com/phillip-simons/alpaca-sdk/blob/main/.github/CONTRIBUTING.md#reporting-a-bug)
with the raw body.

## Minimum supported Rust version

**1.88.** Enabling `polars` raises it to 1.95, which is why that feature is off
by default — a convenience feature does not get to set the crate's floor.

## Contributing

See [CONTRIBUTING.md](https://github.com/phillip-simons/alpaca-sdk/blob/main/.github/CONTRIBUTING.md). The most useful contribution is
usually a captured API response or a precise bug report rather than a large
patch. Security issues: [SECURITY.md](https://github.com/phillip-simons/alpaca-sdk/blob/main/.github/SECURITY.md).

Release notes are in [CHANGELOG.md](CHANGELOG.md).

## License

Apache-2.0. This crate derives from Apache-2.0 works; see [NOTICE](https://github.com/phillip-simons/alpaca-sdk/blob/main/NOTICE).

[docs]: https://docs.alpaca.markets/us/reference/
[rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits
