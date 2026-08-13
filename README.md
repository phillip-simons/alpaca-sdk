# alpaca-sdk

[![crates.io](https://img.shields.io/crates/v/alpaca-sdk.svg)](https://crates.io/crates/alpaca-sdk)
[![docs.rs](https://docs.rs/alpaca-sdk/badge.svg)](https://docs.rs/alpaca-sdk)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](#minimum-supported-rust-version)

Unofficial Rust SDK for the [Alpaca](https://alpaca.markets) trading, market data,
and broker APIs, targeting [the API itself][docs] rather than any one SDK's idea
of it.

> **Unofficial.** Not affiliated with, endorsed by, or sponsored by Alpaca
> Securities LLC. See [NOTICE](NOTICE).

## Status

Under active development. Built phase by phase, verified against captured API
responses at every step.

| Area | State |
|---|---|
| Transport: auth, retry, errors | ✅ |
| Type vocabulary: 71 enums, `Decimal`, `TimeFrame` | ✅ |
| Trading REST | ✅ |
| Historical market data | ✅ |
| Live market data streams | ✅ |
| Trade update stream | ✅ |
| Broker API | ✅ |

Route coverage is tracked in [COVERAGE.md](COVERAGE.md): 251 of the 253 routes
the specs document, with two deliberate skips, each recorded with its reason.

**What is not verified against a live server.** The broker routes have never met
a real response — this account has no broker sandbox key — and forex, indices and
logos answer `403 insufficient grants` on a plan that reaches SIP, so they are
per-product entitlements. Those models follow the published reference and the
vendored specs. Treat a decode failure on a first real payload there as expected
work rather than a regression.

Release notes are in [CHANGELOG.md](CHANGELOG.md).

## What the types look like

- **Money is `rust_decimal::Decimal`.** Alpaca sends order quantities and prices
  as strings and market data as JSON numbers, so the deserializer accepts both
  and the market-data floats stay `f64` — reading a string price as a float
  loses precision.
- **Unknown enum values deserialize into `Unknown`** rather than failing. Alpaca
  adds values without warning, and a new order status should cost a caller a
  match arm rather than a decode.
- **Paginated endpoints offer two methods.** `get_x` fetches one page; `get_all_x`
  walks every page with an optional cap.
- **Request structs are `#[non_exhaustive]`.** Build one with `new` or
  `default` and set fields on it — `let mut r = GetOrdersRequest::default();
  r.limit = Some(50);`. Alpaca documents new query parameters regularly, and
  this is what lets one arrive as a new field rather than as a breaking change.
- **`request_raw` is the escape hatch** for routes this crate does not wrap: the
  transport is public, and returns the body undecoded.
- **Async-first.** The `blocking` feature wraps any client in a runtime of its
  own; the `polars` feature adds `DataFrame` conversion for the market data
  collections.

## Examples

`examples/` has three runnable programs, each needing paper credentials in
`APCA_API_KEY_ID` and `APCA_API_SECRET_KEY`:

```sh
cargo run --example account          # balances, positions, open orders
cargo run --example historical_bars  # daily bars for a couple of symbols
cargo run --example crypto_stream    # live trades over the websocket
```

## How it is verified

Alpaca publishes an API reference, vendors OpenAPI specs, and ships five SDKs,
and they do not always agree. Sources are ranked by how close each is to the
wire: a captured response beats a specification, a specification beats an SDK,
and only the published reference says whether a route is still current — three
event streams were in the specs, looked healthy, and had been switched off.

`just coverage`, `just parameters` and `just enums-drift` diff this crate
against those sources; the route results live in [COVERAGE.md](COVERAGE.md).

## Minimum supported Rust version

1.88. Enabling `polars` raises it to 1.95, which is why that feature is
off by default.

## Feature flags

| Feature | Default | Enables |
|---|---|---|
| `trading` | ✅ | Trading REST client and trade-update stream |
| `data` | ✅ | Historical and live market data |
| `broker` | | Broker API, including its SSE event streams |
| `blocking` | | Synchronous façade over the async clients |
| `polars` | | `DataFrame` conversion for market data collections |
| `rustls-tls` | ✅ | TLS via rustls |
| `native-tls` | | TLS via the platform library |

## License

Apache-2.0. This crate derives from Apache-2.0 works; see [NOTICE](NOTICE).

[docs]: https://docs.alpaca.markets/us/reference/
