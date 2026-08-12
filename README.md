# alpaca-sdk

[![crates.io](https://img.shields.io/crates/v/alpaca-sdk.svg)](https://crates.io/crates/alpaca-sdk)
[![docs.rs](https://docs.rs/alpaca-sdk/badge.svg)](https://docs.rs/alpaca-sdk)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Unofficial Rust SDK for the [Alpaca](https://alpaca.markets) trading, market data,
and broker APIs — a port of the official Python SDK, [alpaca-py], with the same
feature surface expressed through Rust's type system.

> **Unofficial.** Not affiliated with, endorsed by, or sponsored by Alpaca
> Securities LLC. See [NOTICE](NOTICE).

## Status

Under active development. Ported phase by phase against alpaca-py commit `cc4cb3b`.

| Area | State |
|---|---|
| Transport: auth, retry, errors | ✅ |
| Type vocabulary: 71 enums, `Decimal`, `TimeFrame` | ✅ |
| Trading REST | ✅ |
| Historical market data | 🚧 |
| Live market data streams | ⬜ |
| Trade update stream | ⬜ |
| Broker API | ⬜ |

## What changes from alpaca-py

The port preserves behavior, not API shape. Several Python patterns have no Rust
equivalent, and the replacements are where the strong typing pays off:

- **Money is `rust_decimal::Decimal`**, not `Optional[Union[str, float]]`. Alpaca
  sends order quantities and prices as strings; a custom deserializer accepts both
  strings and numbers. Market-data floats that arrive as JSON numbers stay `f64`.
- **Unknown enum values deserialize into `Unknown`** rather than failing. pydantic
  hard-errors when Alpaca introduces a new order status; this crate does not.
- **Pagination is a `Stream`.** alpaca-py's `PaginationType::{NONE, FULL, ITERATOR}`
  all fall out of one lazy stream plus `.try_collect()`.
- **`raw_data=True` becomes `request_raw`.** A boolean cannot change a function's
  return type in Rust, so the escape hatch is a separate method.
- **Async-first.** A `blocking` feature provides a synchronous façade.
- **`.df` needs the `polars` feature**, off by default so the dependency is opt-in.

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

Apache-2.0, matching [alpaca-py], from which this crate is derived.

[alpaca-py]: https://github.com/alpacahq/alpaca-py
