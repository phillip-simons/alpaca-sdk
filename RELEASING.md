# Releasing

Publishing goes through `.github/workflows/release.yml` using crates.io
[trusted publishing]: the workflow proves its identity over OIDC and receives a
token that lasts 30 minutes and is revoked when the job ends. No API token is
stored in GitHub, in this repo, or on your machine.

[trusted publishing]: https://crates.io/docs/trusted-publishing

## One-time setup

Both steps are manual, and the second cannot be done before the first.

### 1. The GitHub environment

Repository **Settings → Environments → New environment**, named exactly
`release`. Add yourself under **Required reviewers**.

This is what makes the publish job stop and wait for a human. Verification runs
*before* the pause, so by the time you are asked to approve, the gate is already
green.

### 2. The crates.io trusted publisher

crates.io → the crate → **Settings → Trusted Publishing → Add publisher**:

| Field | Value |
|---|---|
| Repository owner | `phillip-simons` |
| Repository name | `alpaca-sdk` |
| Workflow filename | `release.yml` |
| Environment | `release` |

A trusted publisher can only be added to a crate that has been published at
least once. `alpaca-sdk 0.0.0` was published manually on 2026-08-12 for exactly
this reason, so the prerequisite is already met.

**The workflow filename is part of the trust configuration.** Renaming
`release.yml` breaks publishing until the configuration is changed to match, and
the failure appears at publish time rather than at rename time.

## Releasing a version

```sh
# 1. Set the version. There is one place.
$EDITOR Cargo.toml

# 2. Prove it locally first. This is what CI will run again.
just publish-dry

# 3. Commit, tag, push. The tag must match Cargo.toml or the job fails
#    before publishing anything.
git add Cargo.toml Cargo.lock
git commit -m "release 0.1.0"
git push origin main
git tag v0.1.0
git push origin v0.1.0
```

Then approve the deployment when GitHub asks. Watch the `verify` job first —
its **What would be uploaded** step prints the exact file list, so the contents
of the tarball are on the record next to the approval rather than something to
check afterwards.

## Before the first real version

Rehearse on a prerelease. `0.1.0-alpha.1` exercises OIDC, the approval gate, the
packaging and the docs.rs build on a version nobody will depend on.

This matters more than it sounds: **a published version can never be replaced.**
`cargo yank` hides a version from new dependents; it does not remove it, and it
does not let you reuse the number. A docs.rs build that fails is equally
permanent for that version.

## What the pipeline checks

`verify` runs `just publish-dry`, which is `just ci` — fmt, clippy, rustdoc,
tests, feature combinations, MSRV, cargo-deny — plus `cargo-semver-checks` and a
packaging dry run. Then:

- the tag matches `Cargo.toml`;
- the tarball's file list is printed for review.

`publish` runs only after `verify` passes and you approve.

## Notes

- No OIDC token is issued to `pull_request` runs or to forks, so the release
  path cannot be exercised from a pull request. The prerelease rehearsal is the
  only full test of it.
- `id-token: write` is set on the publish job, not on the workflow. Workflow-level
  would hand the OIDC capability to every job.
- `fixtures/` ships in the tarball on purpose: `tests/` ships too, and every one
  of those tests reads a captured payload out of `fixtures/` at runtime.
  Excluding one without the other would publish tests that cannot pass.
- `polars` needs Rust 1.95 while the crate declares 1.88. It is off by default,
  so it does not set the floor, and `cargo publish` verifies with default
  features.
