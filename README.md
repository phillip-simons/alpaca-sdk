# alpaca-sdk

[![crates.io](https://img.shields.io/crates/v/alpaca-sdk.svg)](https://crates.io/crates/alpaca-sdk)
[![docs.rs](https://docs.rs/alpaca-sdk/badge.svg)](https://docs.rs/alpaca-sdk)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

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

See [ROADMAP.md](ROADMAP.md) for what is left, how the port is verified, and the
conventions that keep it honest.

## Coming from alpaca-py

This crate began as a port of [alpaca-py] and is a derivative work of it. It no
longer tracks that SDK — alpaca-py is the least complete of Alpaca's official
SDKs, and in at least one place still calls an endpoint Alpaca has retired — so
where the two disagree, this crate follows the API.

The shape is close enough to migrate mechanically. What needs a decision rather
than a rename:

- **Money is `rust_decimal::Decimal`**, not `Optional[Union[str, float]]`. Alpaca
  sends order quantities and prices as strings; a custom deserializer accepts both
  strings and numbers. Market-data floats that arrive as JSON numbers stay `f64`.
  Several fields alpaca-py declares `float` arrive as strings on the wire, and
  reading them as floats loses precision.
- **Unknown enum values deserialize into `Unknown`** rather than failing. Alpaca
  adds values without warning; pydantic hard-errors on a new order status, and
  this crate keeps the raw string instead.
- **Paginated endpoints offer two methods, not a mode flag.** `get_x` fetches one
  page; `get_all_x` walks every page with an optional cap. That covers alpaca-py's
  `PaginationType::{NONE, FULL}`; the lazy `ITERATOR` mode has no equivalent yet.
- **`raw_data=True` becomes `request_raw`.** A boolean cannot change a function's
  return type in Rust, so the escape hatch is a separate method.
- **Async-first.** A `blocking` feature provides a synchronous façade.
- **`.df` needs the `polars` feature**, off by default so the dependency is opt-in.

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

Apache-2.0, matching [alpaca-py], from which this crate is derived. See
[NOTICE](NOTICE).

[alpaca-py]: https://github.com/alpacahq/alpaca-py
[docs]: https://docs.alpaca.markets/us/reference/
