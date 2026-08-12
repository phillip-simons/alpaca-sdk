# Roadmap and working notes

Where the port stands, what is left, and the things that are easy to get wrong.
Written to be picked up cold.

## Status

| Phase | State | Contents |
|---|---|---|
| 0 — Foundation | ✅ | Transport, auth, retry, errors, backoff |
| 1 — Type vocabulary | ✅ | 71 generated enums, `Decimal`, `TimeFrame`, `AssetIdent` |
| 2 — Trading REST | ✅ | 32 routes, 20 models, request builders |
| 3 — Historical data | ✅ | 6 clients, 26 methods, the pagination loop |
| 4 — Live market data | ✅ | msgpack websocket, 4 streams, reconnect machine |
| 5 — Trade updates | ✅ | JSON websocket, single channel |
| **6 — Broker** | **🚧** | accounts + trading-on-behalf done; see below |
| 6.5 — Exceed alpaca-py | ⬜ | API gaps; see below |
| 7 — Polish | ⬜ | polars, blocking façade, docs, migration guide, 1.0 |

Ported against alpaca-py `cc4cb3b`. `just pinned` reports drift against a local
alpaca-py checkout.

## Remaining in Phase 6

Captured fixtures exist for all of these under `fixtures/broker/`.

- Funding: ACH relationships, recipient banks, transfers
- Journals: create, batch, reverse batch, list, cancel
- Documents: list, download, and the base64 upload (10-document limit)
- Rebalancing: portfolios, subscriptions, runs
- CIP / KYC submission
- Account activities
- The five SSE event streams (`reqwest` byte stream + `eventsource-stream`)

The broker spec has **154 operations**; alpaca-py has **76**. Phase 6 targets the
76. The rest is Phase 6.5.

## Phase 6.5 — the API gaps

Scope changed on 2026-08-12 from *alpaca-py parity* to *API coverage*. alpaca-py
is the least complete of Alpaca's five official SDKs for market data. Evidence:
the OpenAPI specs carry 18 trading routes it lacks, and diffing the C#, Node, Go
and Java clients found ~25 more non-broker gaps.

**Market data** — all confirmed present in `market-data-api.json`:

- Auctions: `/v2/stocks/auctions` — the only route all four other SDKs carry and
  we do not
- Stock meta: `/v2/stocks/meta/exchanges`, `/v2/stocks/meta/conditions/{tick_type}`
- Option meta: `/v1beta1/options/meta/conditions/{tick_type}`
- Forex: `/v1beta1/forex/rates`, `/v1beta1/forex/latest/rates`
- Fixed income: `/v1beta1/fixed_income/latest/{prices,quotes}`, and
  `/v2/assets/fixed_income/us_{corporates,treasuries}`
- Logos: `/v1beta1/logos/{symbol}`

**Not in the spec — verify against the live API before implementing:**

- Indices: `/v1beta1/indices/{values,latest/values}` (Node only)
- Crypto perpetuals: `/v1beta1/crypto-perps/{feed}/latest/*` (C#, Go, Node)

**Trading** — confirmed in `trading-api.json`:

- `/v2/positions/{id}/do-not-exercise`
- `/v2/account/activities/{activity_type}`
- `/v2/watchlists:by_name` (GET, PUT, DELETE)
- `/v3/calendar/{market}`
- `/v1/locates`, `/locates/quotes`, `/locates/{id}`
- `/v2/wallets/*` (7 routes), `/v2/tokenization/*` (4 routes)
- `/v2beta1/events/activities`

**Follow-on worth doing properly:** `meta/conditions` and `meta/exchanges` are the
official decoder for the single-letter exchange codes and the opaque
`conditions: Vec<String>` on trades and quotes. Worth a lookup helper, not just
a raw map.

## Also open

- **Enum drift.** Of 71 generated enums, only 7 of the 19 with a same-named spec
  schema agree exactly. Missing values land in `Unknown(String)` — degrading, not
  breaking — but should become real variants. Add a spec cross-check to
  `scripts/gen_enums.py`. Do **not** remove values we have that the spec lacks;
  they may be deprecated but still served. `Exchange` is not drift: the spec's
  same-named schema is venue names, alpaca-py's is tape codes. Verify
  `TaxIdType::ARG_AR_CUIT` (ours) against the spec's `ARG_AG_CUIT` on a live
  response — one is a typo.
- **docs.rs build** for 0.0.0 has not been checked.

## How this port is built

The method that has actually found bugs, in order of value:

1. **Captured fixtures beat schemas.** `fixtures/` holds 135 real API responses
   extracted from alpaca-py's test suite by `scripts/extract_fixtures.py`. Every
   model is verified against them. Three bugs came from this that no schema would
   have shown: a string where an `int` was declared, money-as-strings in `float`
   fields, and a pagination loop that never terminated.
2. **The live smoke test beats mocks.** `tests/live_smoke.rs` found a bug all 15
   stream tests had passed — the mocks encoded msgpack timestamps as strings, a
   form the real server never sends.
3. **Generators for the mechanical parts.** `scripts/gen_enums.py` produces the
   71 wire enums and their parity test from alpaca-py's AST.

## Running things

```sh
just              # = just check: fmt, clippy, rustdoc, test, feature combos
just ci           # + msrv, cargo-deny
just live         # the #[ignore]d tests against the real paper API
just regen        # re-run both generators against ../alpaca-py
just pinned       # is the generated code stale vs. the local alpaca-py?
just hooks        # install the pre-commit credential guard (once per clone)
```

**`just live` needs credentials in the environment.** They are not exported by
the login shell; they come from `.envrc` (`dotenv_if_exists secrets.env`) via
direnv. Either allow direnv for the directory, or scope it to one command:

```sh
direnv exec . just live
```

Use **paper** keys. The tests refuse to run against a key that is not `PK`-prefixed.

## Conventions worth keeping

- **Never `git add -A`.** An unrelated `secrets.env` was swept into a commit and
  pushed that way once. Stage explicit paths; `just hooks` installs a guard, and
  there is a second one at the Claude Code tool boundary.
- **Money that crosses the wire as a string is `Decimal`**; market-data floats
  that arrive as JSON numbers stay `f64`.
- **Unknown enum values must degrade**, never fail — Alpaca adds them.
- **Unknown response fields are ignored.** Alpaca sends fields no model declares
  (`Asset.last_price`, order `commission`, the calendar session fields).
- **Generated files are never hand-edited.** Enum methods go in `enums_ext.rs`.
