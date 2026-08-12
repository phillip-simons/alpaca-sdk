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
| 6 — Broker | ✅ | 75 routes, 20 models, 4 pagination schemes, 5 SSE streams |
| **6.5 — Exceed alpaca-py** | **⬜** | API gaps; see below |
| 7 — Polish | ⬜ | polars, blocking façade, docs, migration guide, 1.0 |

Ported against alpaca-py `cc4cb3b`. `just pinned` reports drift against a local
alpaca-py checkout.

## Phase 6, as built

All 76 of alpaca-py's broker routes are ported, bar one: `delete_account`, which
alpaca-py deprecates and forwards to `close_account`. One route, one method.

The broker spec has **154 operations**; alpaca-py has 76. The remaining 78 are
Phase 6.5's business.

**Count the routes, not the sections.** This section once claimed accounts and
trading-on-behalf were finished while 16 order, asset, announcement and
account-lifecycle routes were missing. The check that would have caught it, and
the one to re-run before believing this file:

```sh
diff <(grep -oE '^    def [a-z_0-9]+' ../alpaca-py/alpaca/broker/client.py \
        | sed 's/    def //' | grep -v '^_' | sort) \
     <(grep -oE 'pub (async )?fn [a-z_0-9]+' src/broker/client.rs \
        | sed 's/.*fn //' | sort -u)
```

### Still open from Phase 6 (nothing route-shaped)

- `create_account`, `update_account` and `list_accounts` take a generic
  `Serialize` body rather than a request type, so `CreateAccountRequest`,
  `UpdateAccountRequest` and `ListAccountsRequest` do not exist here — including
  the last one's `entities` comma-join, which currently falls on the caller.
- `Weight::percent` is rounded to 2dp by the two constructors only. alpaca-py
  rounds in a field validator, so it also rounds percentages *Alpaca sent*;
  this port does not, on the grounds that editing a server's own numbers on the
  way in is worse than the divergence. A `percent` assigned directly to the
  field is likewise not rounded.
- `Error` has no variant for a stream that breaks mid-flight. Both the SSE
  streams and the websocket code report those as `InvalidRequest`, which is
  wrong in the same way for both. Worth one variant covering the two.

### The broker's models are not the trading models

Three of them extend their trading counterparts, and returning the trading type
silently drops the extra fields:

- `broker::Order` = `trading::Order` + `commission`
- `broker::TradeAccount` = `trading::TradeAccount` + 12 fields (`cash_withdrawable`,
  the `last_*` previous-session values, `clearing_broker`)
- `broker::OrderRequest` = `trading::OrderRequest` + `commission` + `currency`,
  and a non-USD order must be a market order
- `BatchJournalResponse` = `Journal` + `error_message`

They use `#[serde(flatten)]` over the trading struct rather than transcribing it,
which is the closest thing to alpaca-py's subclassing. Check for this whenever a
broker route appears to reuse a trading model.

### Three pagination schemes, not one

The broker API pages three different ways, and the fixtures disagree with each
other about which:

- **Offset**, no envelope, empty array to stop: transfers.
- **Page token**, envelope (`{"subscriptions": [...], "next_page_token": …}`),
  absent token to stop: rebalancing subscriptions and runs.
- **Cursor**, no envelope, where the cursor is the last item's own `id`: account
  activities. Nothing in the response says there is more; an empty array stops it.
- **None at all**, bare array: rebalancing portfolios.

### The event streams are not the websocket streams

The five SSE endpoints are plain HTTP streams of `text/event-stream`: no
subscribe message, no auth handshake, no reconnect machine. They live in
`broker/events.rs` and share nothing with `data/live`.

Two details the SSE specification decides, not Alpaca:

- **The last event id persists.** An event that sends no `id` line keeps the
  previous one, which is what makes `GetEventsRequest::after_id` a usable way to
  resume a dropped stream.
- **The event type does not.** It resets every dispatch and falls back to the
  spec default `"message"`, so `BrokerEvent::name` always has a value and that
  value is often meaningless.

The subscription is awaited before the stream is handed back, so a rejected one
is an error rather than a stream that mysteriously says nothing.

Each paginated route has both a single-page method and a `get_all_*` that walks,
which covers alpaca-py's `PaginationType.NONE` and `.FULL`. The lazy `.ITERATOR`
mode is not ported; a `Stream` is its Rust equivalent if a caller ever wants one.
Every walk stops on an empty page even when a token says otherwise — a token
pointing at an empty page would otherwise loop forever.

`commission` arrives as a JSON *number* on order responses and as a *string* in
the spec's trade-update events; `Decimal` reads both and writes a string. If a
live broker sandbox ever rejects a commission on an order request, that is the
thing to look at first.

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
- **CIP is spec-derived and unverified.** alpaca-py's two CIP methods are empty
  stubs — its own comment says the sandbox 404s them — so the six `CIP*` models
  have never met a real response. They follow `models/cip.py` and the broker
  spec. First real payload wins; treat a decode failure there as expected work,
  not a regression. Note `CIPPhoto.face_comparison` reads Alpaca's
  `face_comparision`, which is a typo on the wire and so is load-bearing.
- **`BrokerClient` carries a second `reqwest::Client`**, only for the document
  download: that route answers `301` to a presigned storage URL, and
  `RestClient` refuses redirects on purpose. The second client follows them and
  sheds the credentials when one crosses hosts. That shedding is reqwest's
  behaviour, not ours, so `broker_documents.rs` asserts it against a pair of
  mock servers rather than trusting it to stay true across upgrades.

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
