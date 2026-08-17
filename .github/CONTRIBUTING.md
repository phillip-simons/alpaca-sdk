# Contributing

Thanks for looking. This is an unofficial SDK maintained by one person, so the
most useful contribution is usually a captured API response or a precise bug
report rather than a large patch.

## The one thing to know first

**This crate targets the Alpaca API, not any SDK's idea of it.** Sources are
ranked by how close each is to the wire:

1. a **captured response** from the real API beats everything;
2. a **specification** beats another SDK;
3. only the **published reference** says whether a route is still current.

That order is not decoration. Three event streams were in the vendored specs,
looked healthy, and had been switched off — the reference was the only source
that said so. A change justified by "alpaca-py does it this way" will be asked
for a better source.

## Getting set up

```sh
git clone https://github.com/phillip-simons/alpaca-sdk
cd alpaca-sdk
just hooks     # installs the pre-commit credential guard, once per clone
just check     # fmt, clippy, rustdoc, tests, script tests
```

`just check` is the gate. Run it before every commit. It holds what fires on an
ordinary edit; CI holds the rest and runs both.

It needs `python3` on your machine as well as a Rust toolchain. The last step is
`just test-scripts`, which tests the Python under `scripts/` — `enum_drift.py`
decides which of this crate's wire enums get compared against Alpaca's specs at
all, and that logic went several defects deep before it had any tests. Stdlib
`unittest`, no packages to install. `just enums-drift` itself stays out of the
gate, because running the report needs `specs/` over the network while testing
the parser needs only the synthetic trees the tests write for themselves.

`just ci` adds what CI checks and `just check` does not: the feature
combinations, the per-surface rustdoc builds, the nightly `docsrs` build, the
MSRV build and `cargo-deny`. Run it before opening a pull request, or just let
CI do it.

That nightly build is `just doc-docsrs`, and it needs a nightly toolchain
(`rustup toolchain install nightly`) that the other recipes do not. It is worth
knowing why it exists: CI's `docs` job builds with `--cfg docsrs`, which is what
turns on `feature(doc_cfg)` in `src/lib.rs`, so a malformed `doc(cfg(...))`
attribute compiles under a stable toolchain and fails there. It used to have no
local equivalent at all, which also put it in front of a release — `release.yml`
gates publishing on `just publish-dry`, and that runs `just ci`.

```sh
just doc-docsrs   # RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +nightly doc …
```

On a change that touches no Rust — documentation, issue templates, this file —
CI skips those jobs. They still report as skipped, which counts as satisfied, so
the pull request is not left waiting on checks that will never run. Editing
anything under `src/`, `macros/`, `tests/`, `examples/`, `fixtures/`,
`Cargo.toml`, `Cargo.lock`, `build.rs`, `deny.toml` or `ci.yml` brings the whole
matrix back. `macros/` counts because the macros it holds are compiled into
every request type and every wire enum, so a change there is a change to the
library even though nothing under `src/` moved. `scripts/` has its own job on its own filter, so a change to
the Python runs the script tests without dragging the Rust matrix along with
it.

The minimum supported Rust version is **1.88**. Enabling `polars` raises it to
1.95, which is why that feature is off by default — a convenience feature does
not get to set the crate's floor.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), with a
subject that says what changed rather than which files moved.

```
<type>[optional scope]: <description>

[optional body explaining why]
```

Types used here:

| Type | For |
|---|---|
| `feat` | A new capability a caller can reach |
| `fix` | A defect corrected |
| `test` | Tests added or reworked, with no behaviour change |
| `docs` | Documentation, comments, README |
| `refactor` | Behaviour-preserving restructuring |
| `perf` | A performance change |
| `build` | `Cargo.toml`, dependencies, the `justfile` |
| `ci` | Anything under `.github/` |
| `chore` | Everything else |

A breaking change gets a `!` before the colon — `feat!:` — and a
`BREAKING CHANGE:` footer explaining what a caller has to do differently.

**Write the body for someone who has to understand the change in a year.** The
subject says what; the body should say why, and especially why the obvious
alternative was not taken. If a bug was found by a test, say what the test was
doing when it found it.

## Pull requests

- **One concern per pull request.** A bug fix and a refactor in the same diff
  take twice as long to review and cannot be reverted independently.
- **`just check` must pass**, and CI will confirm it on Linux, across every
  feature combination and the MSRV. macOS and Windows are tested in the release
  workflow rather than per commit, so a platform-specific failure surfaces at
  release time — worth knowing if you are touching anything filesystem- or
  time-related.
- **New behaviour needs a test.** For a route, that means a `wiremock` test
  asserting the method, the version segment and the path — routing is the
  failure this crate has actually shipped.
- **Never `git add -A`.** Stage explicit paths. An unrelated `secrets.env` was
  swept into a commit once; `just hooks` installs a guard against a repeat.

## Adding or changing a route

1. Check `COVERAGE.md` — regenerate it with `just coverage`, never by hand. CI
   regenerates it too and fails if your commit does not match, so a route added
   without rerunning it will not merge.
2. Verify the route against the published reference, not only the spec. The
   spec says what exists; the reference says what is still current.
3. Add a test. If you have a real captured payload, add it under `fixtures/`
   and parse it — a fixture nothing reads proves nothing.
4. Re-run `just parameters`, `just setters`, `just validated` and
   `just enums-drift` if you touched a request struct or a wire enum. The first
   says whether the crate can send a documented parameter at all, the second
   whether a caller can set it without an assignment, the third whether the
   request's own rules can still be skipped, the fourth whether a wire enum
   still matches its schema.

## Conventions worth knowing

- **Money that crosses the wire as a string is `Decimal`.** Market data floats
  that arrive as JSON numbers stay `f64`. Reading a string price as a float
  loses precision.
- **Unknown enum values must degrade to `Unknown`, never fail.** Alpaca adds
  values without warning, and a new order status should cost a caller a match
  arm rather than a decode.
- **Unknown response fields are ignored.** Alpaca sends fields no model
  declares.
- **Request structs are `#[non_exhaustive]`.** Build with the constructor the
  type provides, then chain a setter per optional field. That is usually `new`
  or `default`, but a type whose valid fields depend on a choice offers named
  constructors instead — `OrderRequest::limit`, `CreateJournalRequest::cash`,
  `CreateBankRequest::domestic` — and `AccountConfiguration` offers neither,
  because it is a read-modify-write and a constructor would invite resetting
  every setting the caller did not name. Give a new request struct the
  constructor its shape justifies. This is what lets a newly documented
  parameter arrive as a field rather than as a breaking change.

  ```rust
  GetOrdersRequest::default()
      .status(QueryOrderStatus::Open)
      .limit(50)
      .symbols(vec!["AAPL".to_owned()])
  ```

  **Every request type derives `Setters`**, which generates one consuming
  setter per `Option` field. A field with no setter is reachable only by
  assignment, which still compiles and is now the fallback rather than the
  idiom.

  Because the derive reads the real field list, a field added tomorrow has a
  setter today — there is nothing to keep in step by hand. What needs saying is
  only the exceptions:

  - `#[setters(into)]` on `String` and `Vec<T>` fields, so `.subtag("desk-7")`
    works without a `to_owned()`. Everything else takes its type exactly.
  - `#[setters(doc = "…")]` where the field's own doc comment reads as a noun
    and the method should read as an action. The derive uses the field's
    documentation otherwise, and refuses to generate a setter for a field with
    none.
  - `#[setters(skip = "why")]` where a setter should not exist. Three kinds: a
    constructor already holds the name; the field is only coherent set
    alongside another and one setter writes the group — `OrderAmount`'s
    `qty`/`notional`, a bracket's class and its legs; or the `Option` exists so
    the field serializes as *omitted* rather than `null` and is not a value a
    caller picks at all, which is what `AccountConfiguration`'s `dtbp_check` and
    `pdt_check` are. The reason is required, so a skip is never mistakable for
    an oversight, and `just setters` prints them all on every run.

    The test for the second kind is not "could a caller misuse this" — the
    fields are public, so they always could. It is whether the incoherent state
    is one the API *offers*, in a documented method a reader would take as
    blessed. `OrderRequest::validate` does not reject `qty` and `notional`
    together, because `OrderAmount` made that unreachable; a setter for each
    would quietly make it reachable again.

  - `#[setters(flatten)]` on a field holding a shared base, with
    `#[setters(flattenable)]` on the base itself. **A request that wraps a
    shared base flattens it rather than restating it.** The five market data
    requests holding a `TimeseriesRequest` offer its filters as their own, so a
    caller writes `.limit(50)` rather than `.base.limit(50)`, and the delegates
    are read off the base — no wrapper names a field of it. One rule comes with
    it: `macro_rules!` is textually scoped, so the base has to be declared
    before its wrappers, in the same module or an ancestor. Violating it is a
    compile error naming the missing helper, not a silently absent setter.

    That rule is also why two types wrap a base and do not flatten it —
    `CorporateActionEventsRequest::window` and `broker::OrderRequest::order`,
    both of whose bases live in another module. `src/types/setters.rs` says so
    beside the convention; neither restates its base's fields, so neither is the
    drift this exists to delete.

  `just setters` names request types that do not derive it, and fields holding a
  flattenable base without flattening it, and **fails** on either. Unlike `just
  parameters` and `just enums-drift`, which report a difference with Alpaca that
  may be Alpaca's to resolve, this checks a rule this repository sets for itself
  and can satisfy — with one coupling written down in `scripts/setters.py`: a
  base marked `flattenable` for one wrapper's sake makes the gate demand
  `flatten` of *every* wrapper, including any whose module cannot reach the
  helper.

  **Every request type also implements `Validated`**, and there are exactly two
  ways to do it. A type with no rules adds `Validated` to its derive list; a
  type with rules writes `impl Validated for T { fn validate(&self) … }` by
  hand. Doing both is `E0119`, and doing neither is a compile error at any call
  site that sends the type. There are three such places, and each calls
  `validate` itself so no route has to:

  - `RestClient`, on every body and every query, before the request is built.
  - `sse::subscribe`, on every event stream filter, before it is flattened into
    query pairs.
  - `get_marketdata`, on every market data request, before it is flattened into
    a parameter map. This one is easy to forget when adding a data route: what
    reaches `RestClient` on that surface is a `Raw`-wrapped map, so the
    transport's own bound never sees that surface's request types at all.

  The compiler cannot see a request type that nothing sends *yet* — that one is
  the gate script's, below.

  That bound replaced roughly thirty hand-written `request.validate()?` lines,
  each of which a new route could silently omit. Do not add one back: validation
  happens once, in the transport, before a socket is opened.

  There is deliberately no `#[validated(…)]` attribute. An attribute switching
  the derive between "no rules" and "defer to a hand-written body" would
  recreate the failure exactly one level up — write the validator, forget the
  attribute, and it never runs while everything still compiles. Coherence cannot
  be forgotten.

  `just validated` covers the four cases the bound cannot: a request type
  nothing sends yet; a type that both derives and implements; a type that
  derives the no-op while holding a field whose type *does* have rules, so the
  transport asks the parent and the parent asks nobody; and a type with rules
  whose `to_query` flattens it into query pairs — which satisfy the bound and
  carry no rules of their own. That last one is why the gate exists;
  `GetCorporateAnnouncementsRequest` was in exactly that shape, with a 90-day
  window checked by a `validate` the transport would never have reached. A type
  that hand-implements `Validated` must therefore both return a `Result` from
  `to_query` and call `self.validate()?` inside it. The signature alone is not
  the rule — `Ok(query)` satisfies it and asks nothing — and the gate checks for
  both.

  Two types implement neither half on purpose — `W8BenDocument` and `Weight`.
  Both carry real rules, both are only ever sent nested inside a parent that
  calls them, and deriving the no-op would let one be passed to the transport
  directly and checked by nothing. `scripts/validated.py` records the exemption
  and fails if either stops declaring the `validate` it is about.

  All three live in `macros/`, a second published crate, because a procedural
  macro cannot live in the crate that uses it. See `RELEASING.md` — the two
  publish together.

  Its refusals are covered by trybuild compile-fail tests in
  `macros/tests/compile_fail/`. If you change one of those messages, or a
  toolchain bump rewords a diagnostic, regenerate the expectations rather than
  editing them by hand, then read the diff:

  ```sh
  TRYBUILD=overwrite cargo test -p alpaca-sdk-macros --test compile_fail
  ```
- **Alpaca's typos are load-bearing.** `face_comparision` and `parnter_fee` are
  spelled that way on the wire. Do not "fix" them.

## Tests against the real API

`just live` runs the `#[ignore]`d tests against the real paper API. They need
`APCA_API_KEY_ID` and `APCA_API_SECRET_KEY` in the environment and refuse to run
against a key that is not `PK`-prefixed.

**Use paper keys.** Never put credentials in a file the repository tracks.

## Personal data in fixtures

No credentials, account numbers or personal identifiers belong in a diff,
fixtures included. One narrow exemption: the payloads `just fixtures` extracts
from [alpaca-py](https://github.com/alpacahq/alpaca-py)'s test suite carry that
project's own synthetic values in `account_number`, `email_address`,
`phone_number`, `street_address` and `date_of_birth`. Those are kept as
extracted, because editing a captured payload stops it being evidence of what
the wire sends. The exemption reaches those fields, in fixtures traceable to
that suite, and nothing further — **data belonging to a real person or a real
account is forbidden without exception**, in any field, and a credential is
never exempt in any file. If you are unsure which kind you are holding, it is
the real kind: redact it.

## Reporting a bug

A decode failure is the most useful bug report this crate can get, and the most
useful form of it is **the raw response body** — with account numbers and
identifiers redacted. That payload is a fact about the API; a description of it
is not.

Security issues: see [SECURITY.md](SECURITY.md). Do not open a public issue.
