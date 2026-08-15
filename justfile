# alpaca-sdk task runner. `just check` is the gate — run it before every commit.
# `just ci` additionally runs the slower jobs GitHub Actions does.

# Set for every recipe, matching the workflow-level `RUSTFLAGS` in ci.yml.
#
# Without it the gate was weaker than the CI it stands in for, and in exactly
# the place that matters: a helper used only behind one feature is a *warning*
# in a build without that feature, not an error. `just check` passed, and CI
# failed on `dead_code` in `--features trading` alone. A gate that misses what
# CI catches is not a gate.
export RUSTFLAGS := "-D warnings"

# Where the alpaca-py checkout lives. Only the fixture extractor reads it:
# its test suite is a source of captured API responses, and nothing else here
# depends on that project. Override with ALPACA_PY=/path/to/alpaca-py.
alpaca_py := env_var_or_default("ALPACA_PY", "../alpaca-py")

default: check

# What is here catches something on an ordinary edit. What is in `ci` catches
# something rarer, or something only a second toolchain can see.
#
# `features` used to be in this list, on the argument that a missing cfg gate is
# invisible to every other recipe and "costs half a second warm". That was true
# when the test suite was 33 separate binaries and cargo could reuse almost all
# of the work; it is not true now. Measured after a one-line edit to `src/`, it
# was 98.9s of a 305s gate — a third of the wall clock to catch a bug class you
# can only introduce while adding feature-specific code. CI runs it on every
# push and pull request, and branch protection means a miss costs a fixup push
# rather than a broken `main`.
#
# The gate. Run before every commit.
check: fmt-check clippy doc test

# Rewrite formatting in place.
fmt:
    cargo fmt --all

# Fail if anything is unformatted.
fmt-check:
    cargo fmt --all -- --check

# Lint every target and feature, warnings denied.
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Build the docs with rustdoc lints denied.
doc:
    # These lints only fire here — clippy does not run rustdoc, so without this
    # recipe `missing_docs` and the intra-doc link lints are decoration. This one
    # is in `check` because any doc edit can trip it.
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked

# An intra-doc link that crosses a feature boundary resolves under
# --all-features and dangles everywhere else, so the all-features build alone
# cannot see it. Anyone running `cargo doc --features trading` can.
#
# In `ci` rather than `check`: it only fires on a link written across a feature
# boundary, which is rare, and the `docs` job runs it on every push and pull
# request. It was ~two thirds of the four rustdoc invocations `just doc` used
# to make.
#
# Build the docs once per surface, which the all-features build cannot check.
doc-surfaces:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --no-default-features --features trading,rustls-tls
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --no-default-features --features data,rustls-tls
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --no-default-features --features broker,rustls-tls

# Build and open the docs, to review a module's public surface.
doc-open:
    cargo doc --no-deps --all-features --open

# Run the full test suite, including doctests.
test:
    # Deliberately no `--all-targets`: adding it silently drops the Doc-tests
    # pass — the doctests still compile, and stop ever being run.
    cargo test --all-features --locked

# Run a subset by name, e.g. `just test-one mleg` or `just test-one decimal`.
test-one *args:
    cargo test --all-features {{ args }}

# Auto-fix what is mechanically fixable, then show what is left.
fix:
    cargo fmt --all
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    just check

# Check that each API surface still builds on its own.
features:
    # This crate is heavily cfg-gated, and a missing `#[cfg(feature = ...)]`
    # compiles fine under --all-features and only fails here.
    #
    # `--all-targets` because plain `cargo check` builds only the library: an
    # unguarded *test* file is just as broken under a reduced feature set, and
    # without this the matrix cannot see it. `tests/enum_parity.rs` was in
    # exactly that state.
    cargo check --all-targets --no-default-features --features rustls-tls
    cargo check --all-targets --no-default-features --features trading,rustls-tls
    cargo check --all-targets --no-default-features --features data,rustls-tls
    cargo check --all-targets --no-default-features --features broker,rustls-tls
    # `polars` alone, to pin the implication: it enables `data`, because the
    # frame conversion is for the market data collections and the feature would
    # otherwise compile all of polars and expose nothing.
    cargo check --all-targets --no-default-features --features polars,rustls-tls
    # `blocking` alone. It is generic over the client rather than a mirrored API,
    # so it compiles without any surface enabled — which is worth knowing stays
    # true, since a mirrored one would not.
    cargo check --all-targets --no-default-features --features blocking,rustls-tls
    cargo check --all-targets --no-default-features --features trading,data,broker,blocking,polars,native-tls

# Build against the MSRV. Needs `rustup toolchain install 1.88.0`.
msrv:
    # Every feature except polars, which drags in sysinfo and needs 1.95. An
    # off-by-default convenience feature should not set the crate's floor, so
    # the declared rust-version covers everything else.
    cargo +1.88.0 check --no-default-features \
        --features trading,data,broker,blocking,rustls-tls --locked

# License and advisory audit. Needs `cargo install cargo-deny`.
deny:
    cargo deny check

# Check public API compatibility. Needs `cargo install cargo-semver-checks`.
semver:
    cargo semver-checks check-release

# Reproduce CI's nightly `docs` job. Needs `rustup toolchain install nightly`.
#
# `src/lib.rs` gates `feature(doc_cfg)` behind `--cfg docsrs`, which only the
# workflow set, so a malformed `doc(cfg(...))` attribute compiled under every
# other recipe in this file and failed in CI. That was the whole of the gap
# between `just ci` and the workflow, and it was load-bearing in a second
# place: `release.yml` gates publishing on `just publish-dry`, so the same
# blind spot sat in front of a release.
#
# Stable cannot stand in for nightly here. `--cfg docsrs` is what turns the
# feature on, and the attribute it enables is unstable, so the check does not
# exist on a stable toolchain rather than merely being weaker there.
doc-docsrs:
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +nightly doc --all-features --no-deps

# Everything CI runs.
ci: check doc-surfaces doc-docsrs features msrv deny

# Install the repo's git hooks (once per clone).
hooks:
    git config core.hooksPath .githooks
    @echo "pre-commit hook installed (core.hooksPath=.githooks)"

# Fast inner-loop compile check. Needs `cargo install cargo-watch`.
watch:
    cargo watch -x 'clippy --all-targets --all-features -- -D warnings'

# ---------------------------------------------------------------------------
# Fixtures
#
# Captured API responses, harvested from other SDKs' test suites. Payloads are
# the one thing worth taking from another implementation: a real response is a
# fact about the API, where another project's types are only its reading of it.
# ---------------------------------------------------------------------------

# Re-extract the captured API responses from alpaca-py's test suite.
fixtures source=alpaca_py:
    python3 scripts/extract_fixtures.py {{ source }}

# Capture payloads for the routes no SDK's tests cover.
#
# Read-only GETs against live market data. Records refusals as well as
# successes: several of these routes are plan-gated, and a 403 is a finding.
# Needs credentials, same as `just live`.
capture:
    cargo test --all-features --test live_capture -- --ignored --nocapture

# Harvest response payloads from the Go SDK's tests into fixtures/go.
#
# The Go suite is the only one of the other four worth reading: it pastes raw
# JSON into backtick literals, so wire quirks survive. C# and TypeScript build
# their payloads through their own types, which normalizes those quirks away.
harvest go="../alpaca-trade-api-go":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d "{{ go }}" ]; then
        echo "clone it first: git clone --depth 1 https://github.com/alpacahq/alpaca-trade-api-go {{ go }}"
        exit 1
    fi
    python3 scripts/harvest_go_fixtures.py "{{ go }}"

# Download the OpenAPI specs the coverage check diffs against.
#
# These come from alpacahq/alpaca-java, which generates itself from them and
# runs a drift job against upstream — so they are the closest machine-readable
# statement of what the API is. Not vendored: they are 1.2MB of YAML that
# changes on Alpaca's schedule, not ours.
specs:
    mkdir -p specs
    for surface in broker data trading; do \
        curl -fsSL -o "specs/$surface.yaml" \
            "https://raw.githubusercontent.com/alpacahq/alpaca-java/main/specs/$surface/openapi.yaml"; \
    done
    @echo "specs downloaded to specs/"

# Index Alpaca's published API reference into specs/reference.json.
#
# The specs say what exists; only the reference says what is still current, and
# it is the source that caught three event streams pointing at retired routes.
# Every reference page has a `.md` twin embedding its own OpenAPI fragment.
#
# ~250 pages, so it takes a minute. Cached under specs/reference/.
reference:
    python3 scripts/reference.py

# Regenerate COVERAGE.md: which documented routes this crate implements.
#
# Reads specs/reference.json when it is there, to annotate each route with what
# the reference says about it. Run `just reference` first, or accept the specs
# alone — the report says which it got.
coverage: specs
    python3 scripts/coverage.py specs --out COVERAGE.md

# Where this crate's wire enums and the same-named spec schemas disagree.
#
# A quality report, not a gate: an unknown value deserializes into
# `Unknown(String)` rather than failing. Needs `just specs`.
enums-drift:
    python3 scripts/enum_drift.py

# Line and function coverage. Needs `cargo install cargo-llvm-cov`.
#
# The number is a map, not a target: it says which code no test has ever run,
# which is where to look next. Route methods dominate the uncovered set, and
# each one is a wiremock test nobody has written yet.
cov:
    cargo llvm-cov --all-features --summary-only

# The same, as a browsable report.
cov-open:
    cargo llvm-cov --all-features --open

# Which documented query parameters this crate never sends.
#
# `just coverage` compares paths and methods; a route can be implemented,
# counted, and still be missing half of what it accepts. Needs the parameters
# recorded by `just reference`, so run that first.
parameters:
    python3 scripts/parameters.py


# ---------------------------------------------------------------------------
# Live API
# ---------------------------------------------------------------------------

# Run the tests that hit real paper endpoints, which `just test` skips.
#
# WARNING: this rewrites tracked files. `--ignored` selects every ignored test
# in every target, which includes the capture in tests/live_capture.rs, so this
# re-runs `just capture` as a side effect and overwrites fixtures/live/*.json
# and fixtures/live/index.json. Commit or stash before running it, and diff
# fixtures/live/ afterwards. To run only the smoke tests, select that target
# directly rather than reaching for this recipe.
live:
    # They are #[ignore]d so a normal run never spends network time or
    # credentials. Needs APCA_API_KEY_ID and APCA_API_SECRET_KEY set.
    #
    # The login shell does not export them; they come from .envrc via direnv.
    # If this fails with "APCA_API_KEY_ID is not set", run:
    #     direnv exec . just live
    cargo test --all-features --locked -- --ignored --test-threads=1

# ---------------------------------------------------------------------------
# Release
# ---------------------------------------------------------------------------

# List exactly what `cargo publish` would upload.
package:
    # Checks the `exclude` list in Cargo.toml before a release rather than
    # after. --allow-dirty because this is for inspection mid-change; the real
    # publish path goes through publish-dry, which does not pass it.
    cargo package --list --all-features --allow-dirty

# Full pre-release verification, without publishing.
publish-dry: ci semver
    cargo publish --dry-run --locked
