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
just check     # fmt, clippy, rustdoc, tests, feature combinations
```

`just check` is the gate. Run it before every commit; CI runs it again along
with the MSRV build and `cargo-deny`.

On a change that touches no Rust — documentation, issue templates, this file —
CI skips those jobs. They still report as skipped, which counts as satisfied, so
the pull request is not left waiting on checks that will never run. Editing
anything under `src/`, `tests/`, `examples/`, `fixtures/`, `Cargo.toml`,
`Cargo.lock`, `build.rs`, `deny.toml` or `ci.yml` brings the whole matrix back.

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

1. Check `COVERAGE.md` — regenerate it with `just coverage`, never by hand.
2. Verify the route against the published reference, not only the spec. The
   spec says what exists; the reference says what is still current.
3. Add a test. If you have a real captured payload, add it under `fixtures/`
   and parse it — a fixture nothing reads proves nothing.
4. Re-run `just parameters` and `just enums-drift` if you touched a request
   struct or a wire enum.

## Conventions worth knowing

- **Money that crosses the wire as a string is `Decimal`.** Market data floats
  that arrive as JSON numbers stay `f64`. Reading a string price as a float
  loses precision.
- **Unknown enum values must degrade to `Unknown`, never fail.** Alpaca adds
  values without warning, and a new order status should cost a caller a match
  arm rather than a decode.
- **Unknown response fields are ignored.** Alpaca sends fields no model
  declares.
- **Request structs are `#[non_exhaustive]`.** Build with `new` or `default`
  and assign fields. This is what lets a newly documented parameter arrive as a
  field rather than as a breaking change.
- **Alpaca's typos are load-bearing.** `face_comparision` and `parnter_fee` are
  spelled that way on the wire. Do not "fix" them.

## Tests against the real API

`just live` runs the `#[ignore]`d tests against the real paper API. They need
`APCA_API_KEY_ID` and `APCA_API_SECRET_KEY` in the environment and refuse to run
against a key that is not `PK`-prefixed.

**Use paper keys.** Never put credentials in a file the repository tracks.

## Reporting a bug

A decode failure is the most useful bug report this crate can get, and the most
useful form of it is **the raw response body** — with account numbers and
identifiers redacted. That payload is a fact about the API; a description of it
is not.

Security issues: see [SECURITY.md](SECURITY.md). Do not open a public issue.
