<!--
Subject line follows Conventional Commits: feat: / fix: / docs: / test: …
A breaking change gets `!` before the colon and a BREAKING CHANGE: footer.
See .github/CONTRIBUTING.md.
-->

## What this changes

<!-- One or two sentences. What can a caller now do, or stop hitting? -->

## Why

<!--
The part worth writing. Why this approach, and why not the obvious
alternative? If a test found the bug, what was it doing at the time?
-->

## What it was verified against

<!--
This crate ranks its sources: a captured response beats a specification, a
specification beats another SDK, and only the published reference says whether
a route is still current. Tick what applies and link it.
-->

- [ ] A captured response from the real API (fixture added under `fixtures/`)
- [ ] The published API reference — <!-- link the page -->
- [ ] The vendored OpenAPI specs
- [ ] Not applicable (refactor, docs, tooling)

## Checklist

- [ ] `just check` passes (fmt, clippy, rustdoc, tests, feature combinations)
- [ ] New behaviour has a test; a new route has a `wiremock` test asserting
      method, version segment and path
- [ ] `just coverage` re-run if a route was added or removed (never hand-edited)
- [ ] `just parameters` / `just enums-drift` re-run if a request struct or wire
      enum changed
- [ ] No credentials, account numbers or personal identifiers in the diff,
      including inside fixtures

## Breaking change?

<!--
Delete if not. Otherwise: what breaks, and what does a caller write instead?
This crate is 0.x, so breaking changes are allowed — but they must be written
down, because `cargo-semver-checks` cannot see them inside 0.x.
-->
