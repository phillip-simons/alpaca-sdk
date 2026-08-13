# alpaca-sdk task runner. `just check` is the gate — run it before every commit.
# `just ci` additionally runs the slower jobs GitHub Actions does.

# Where the alpaca-py checkout lives. The codegen recipes read from it.
# Override with ALPACA_PY=/path/to/alpaca-py, or `just gen-enums /some/path`.
alpaca_py := env_var_or_default("ALPACA_PY", "../alpaca-py")

default: check

# The gate. Run before every commit.
#
# `features` is in here rather than only in `ci` because a missing cfg gate is
# invisible to every other recipe — it compiles fine under --all-features and
# fails only when a surface is built alone. It costs half a second warm.
check: fmt-check clippy doc test features

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
    # recipe `missing_docs` and the intra-doc link lints are decoration.
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked

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
    cargo check --no-default-features --features rustls-tls
    cargo check --no-default-features --features trading,rustls-tls
    cargo check --no-default-features --features data,rustls-tls
    cargo check --no-default-features --features broker,rustls-tls
    # `polars` alone, to pin the implication: it enables `data`, because the
    # frame conversion is for the market data collections and the feature would
    # otherwise compile all of polars and expose nothing.
    cargo check --no-default-features --features polars,rustls-tls
    cargo check --no-default-features --features trading,data,broker,blocking,polars,native-tls

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

# Everything CI runs.
ci: check msrv deny

# Install the repo's git hooks (once per clone).
hooks:
    git config core.hooksPath .githooks
    @echo "pre-commit hook installed (core.hooksPath=.githooks)"

# Fast inner-loop compile check. Needs `cargo install cargo-watch`.
watch:
    cargo watch -x 'clippy --all-targets --all-features -- -D warnings'

# ---------------------------------------------------------------------------
# Porting from alpaca-py
#
# Both generators overwrite their output wholesale. Hand-written code never
# lives in generated files: enum methods belong in the `enums_ext.rs` next
# door, and fixtures are captured API responses that should only change when
# the upstream revision does.
# ---------------------------------------------------------------------------

# Regenerate the wire enums and their parity test.
gen-enums source=alpaca_py:
    python3 scripts/gen_enums.py {{ source }}
    cargo fmt --all

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

# Regenerate everything, then verify nothing broke.
regen source=alpaca_py: (gen-enums source) (fixtures source)
    just check

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

# Compare the pinned upstream revision against the local alpaca-py checkout.
pinned source=alpaca_py:
    #!/usr/bin/env bash
    set -euo pipefail
    generated=$(grep -o 'revision `[^`]*`' src/trading/enums.rs | head -1 | tr -d '`' | cut -d' ' -f2)
    upstream=$(git -C "{{ source }}" rev-parse --short HEAD 2>/dev/null || echo "unavailable")
    echo "generated from: ${generated}"
    echo "alpaca-py HEAD: ${upstream}"
    if [ "${generated}" = "${upstream}" ]; then
        echo "up to date"
    else
        echo "MISMATCH — run \`just regen\`"
    fi

# ---------------------------------------------------------------------------
# Live API
# ---------------------------------------------------------------------------

# Run the tests that hit real paper endpoints, which `just test` skips.
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
