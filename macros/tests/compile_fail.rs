//! The derive's error paths, asserted on the message each produces.
//!
//! Every refusal in `expand` is a `compile_error!`, which is exactly the half of
//! a derive an ordinary test cannot reach: the failure is that the code does not
//! compile, so the only way to assert on it is to compile a file that should
//! not and diff the output. The unit tests in `src/lib.rs` cover the parsing
//! helpers; this covers what the derive says when it says no.
//!
//! These messages are load-bearing. A derive that refuses without explaining is
//! worse than one that has no opinion, because the caller's next move is to
//! guess — and every one of the things this derive refuses is a case where the
//! obvious guess is wrong. `#[setters(skip)]` on a required field is the
//! sharpest: it reads as a settled decision about a real name collision when
//! the field would never have had a setter either way.
//!
//! There is one case per refusal in `expand` and `parse_options`, which is what
//! makes a refusal added without a case here visible — the count of `*_fails.rs`
//! files and the count of `return Err` sites should move together.
//!
//! # Where this runs, and where it deliberately does not
//!
//! `.stderr` files pin rustc's and syn's diagnostic formatting, neither of
//! which is a stable interface. A toolchain bump can reword a note, re-order a
//! span, or change an underline, and this test goes red for a reason that has
//! nothing to do with the change in front of it.
//!
//! That is an acceptable cost once, on one toolchain, on one platform — CI's
//! `test` job, which is Linux and stable. It is not acceptable in the release
//! pipeline, so `release.yml`'s `cross-platform` job runs `cargo test` without
//! `--workspace` and never reaches this file. A rustfmt-shaped diagnostic
//! change on Windows must not be able to stop a publish.
//!
//! When a toolchain bump does reword something, regenerate rather than hand-edit:
//!
//! ```sh
//! TRYBUILD=overwrite cargo test -p alpaca-sdk-macros --test compile_fail
//! ```
//!
//! Then read the diff. The point of the files is that a *reworded* message is a
//! one-line diff and a *lost* message is an obvious one.

#[test]
fn the_derive_refuses_what_it_cannot_generate() {
    let harness = trybuild::TestCases::new();
    harness.pass("tests/compile_fail/every_attribute_together.rs");
    harness.compile_fail("tests/compile_fail/*_fails.rs");
}
