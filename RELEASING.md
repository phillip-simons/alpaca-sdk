# Releasing

Publishing goes through `.github/workflows/release.yml` using crates.io
[trusted publishing]: the workflow proves its identity over OIDC and receives a
token that lasts 30 minutes and is revoked when the job ends. No API token is
stored in GitHub, in this repo, or on your machine.

[trusted publishing]: https://crates.io/docs/trusted-publishing

## Two crates, one release

This is a workspace. `alpaca-sdk-macros` holds the `Setters` derive, and exists
only because a procedural macro cannot live in the crate that uses it. It is not
a crate anyone depends on directly, and its documentation says so.

`alpaca-sdk` pins it with `=`, following the precedent serde sets for
`serde_derive`: the derive's output has to match what the SDK's own source
expects of it, and a caret range would let cargo pair a version of one with a
version of the other that was never built together. `release.yml` checks the pin
names the version being published, because a stale pin resolves against
crates.io instead of the sibling and hands a caller a derive built from
different source than the SDK was tested against.

**One `cargo publish --workspace` does both**, in dependency order, from a single
invocation. Two separate `cargo publish` calls would not work: the second needs
the first to be live and indexed, and crates.io does not index synchronously.
The same command with `--dry-run` — which is what `just publish-dry` runs —
packages both and resolves the not-yet-published sibling out of a temporary
registry, so the whole path is exercised locally before a tag exists.

## One-time setup

Both steps are manual, and the second cannot be done before the first.

### 1. The GitHub environment

Repository **Settings → Environments → New environment**, named exactly
`release`. Add yourself under **Required reviewers**.

This is what makes the publish job stop and wait for a human. Verification runs
*before* the pause, so by the time you are asked to approve, the gate is already
green.

### 2. The crates.io trusted publishers

**Two of them, one per crate.** crates.io → the crate → **Settings → Trusted
Publishing → Add publisher**, with identical values for both:

| Field | Value |
|---|---|
| Repository owner | `phillip-simons` |
| Repository name | `alpaca-sdk` |
| Workflow filename | `release.yml` |
| Environment | `release` |

A trusted publisher can only be added to a crate that has been published at
least once. `alpaca-sdk 0.0.0` was published manually on 2026-08-12 for exactly
this reason, so that side is already met.

**`alpaca-sdk-macros` has not been published yet, and needs the same treatment
before the next release can run.** Publish it once by hand, then add its trusted
publisher:

```sh
cargo publish -p alpaca-sdk-macros   # with a scratch API token, once
```

Until that is done, the `publish` job will fail at the macros crate — after the
approval prompt, and after `alpaca-sdk` has been packaged but not uploaded.
Nothing is left half-published: cargo uploads in dependency order and stops at
the first failure, so the failure mode is "nothing was published", not "the
macros crate shipped without the SDK".

**The workflow filename is part of the trust configuration.** Renaming
`release.yml` breaks publishing until the configuration is changed to match, and
the failure appears at publish time rather than at rename time.

## Releasing a version

```sh
# 1. Set the version. Two places, and only the first is checked by the
#    pipeline — the tag is compared against Cargo.toml, and nothing compares
#    either against CHANGELOG.md.
$EDITOR Cargo.toml
$EDITOR CHANGELOG.md   # promote the heading to `## [0.1.0] — 2026-08-13`

# 1b. Bump the macros crate to the same version, and the `=` pin with it.
#     Lockstep every release, even when `macros/` did not change — serde does
#     the same with serde_derive, and for the same reason: it makes "which
#     macros version goes with this SDK" a question with one answer instead of
#     a lookup. It also sidesteps `cargo publish --workspace` meeting a member
#     whose version is already on crates.io.
$EDITOR macros/Cargo.toml   # version = "0.1.1"
$EDITOR Cargo.toml          # alpaca-sdk-macros = { version = "=0.1.1", … }

# 2. Prove it locally first. This is what CI will run again.
just publish-dry

# 3. The version bump goes through a pull request like anything else.
#    `main` is protected and admins are not exempt, so this cannot be
#    pushed directly.
git checkout -b release-0.1.0
git add Cargo.toml macros/Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "release 0.1.0"
git push -u origin release-0.1.0
gh pr create --fill
gh pr merge --rebase          # after `all checks` goes green

# 4. Tag the merged commit on main. Tags are not covered by branch
#    protection, and this is what triggers the release workflow. The tag
#    must match Cargo.toml or the job fails before publishing anything.
git checkout main && git pull
git tag v0.1.0
git push origin v0.1.0
```

**Merge with `--rebase` or `--squash`, not a merge commit.** `main` requires
linear history, and merge commits are disabled on the repository to stop the UI
offering a button that protection would then reject.

**Write the notes as the change is made, not at tag time.** Inside `0.x`,
`cargo-semver-checks` has nothing to assert — every bump is permitted to break —
so CHANGELOG.md is the only mechanism that communicates a breaking change, and
the ones without a compile error attached are exactly the ones nobody remembers
a week later.

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
packaging dry run, all on Linux. Then:

- the tag matches `Cargo.toml`;
- the `=` pin on `alpaca-sdk-macros` names the version being published;
- the tarball's file list is printed for review.

`cross-platform` runs the test suite on macOS and Windows, in parallel with
`verify`. **This is the only place those two platforms are tested.** Routine CI
runs Linux alone, because Windows was most of the wall-clock of every commit and
was catching a class of bug that has not appeared yet; paying it once per release
keeps the coverage without the loop. A failure there stops the release before
the approval prompt, not after.

`publish` runs only after both pass and you approve.

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
- **Branch protection does not cover tags.** `main` requires a pull request and
  nine green checks, admins included, but `git push origin v0.1.0` goes straight
  through — which is what makes step 4 work. It also means the tag is the one
  unguarded step in the release, and the reason `verify` re-checks that the tag
  matches `Cargo.toml` before anything is published.
- **In a genuine emergency**, protection can be lifted at Settings → Branches
  rather than worked around. Turning it off deliberately and turning it back on
  leaves a record; a permanent admin exemption does not.
