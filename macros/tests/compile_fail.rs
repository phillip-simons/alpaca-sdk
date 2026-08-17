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
//! There is at least one case per refusal in `expand`, `parse_options` and
//! `parse_container_options`, which is what makes a refusal added without a case
//! here visible: the two counts do not match and are not meant to — several
//! refusals are reachable by more than one shape, and some cases below are not
//! refusals at all — but a `return Err` added without a case is a delta in one
//! and not the other. Beyond the refusals there is
//! `a_skipped_field_has_no_setter_fails.rs`, which is not a refusal at all — it
//! fails to compile because the method genuinely is not there, which is the
//! assertion every `#[setters(skip)]` in the SDK rests on.
//!
//! # Three of these failures are rustc's
//!
//! Three ways flattening can go wrong land on rustc rather than on the derive,
//! for one reason: the derive reads one struct at a time and knows nothing about
//! what other items exist, what they are called, or in what order they appear.
//!
//! - `flatten_without_flattenable_fails` — the base emitted no helper macro.
//! - `flatten_before_the_base_fails` — it emitted one, but *after* the wrapper,
//!   and `macro_rules!` is textually scoped.
//! - `a_flattened_name_collision_fails` — the wrapper has an optional field of
//!   its own sharing a name with one of the base's, so two `pub fn` of that name
//!   land in one inherent impl.
//!
//! These three are the ones a caller is likely to reach. Two further shapes fail
//! at rustc and are deliberately not pinned: a wrapper flattening *two different*
//! bases that happen to share a field name, which is the same `E0592` one step
//! further out; and the base's field types failing to resolve at the wrapper,
//! which is the type-resolution rule the derive documents as its sharp edge.
//! Neither has a message worth reviewing that the three below do not already
//! show, and both need a multi-module fixture to reach.
//!
//! `flattening_one_base_twice_fails` is the neighbouring case the derive *can*
//! see — both fields are in the struct in front of it — so it is a refusal here
//! rather than an `E0592` pointing at the base.
//!
//! They are pinned deliberately. Each is invisible until it is violated, so the
//! message a caller meets when they violate it is worth reviewing rather than
//! merely emitting — and none of the three reads as obviously as it might.
//!
//! The first two have identical `error:` lines and differ only in their trailing
//! help: `flatten_before_the_base_fails.stderr` carries rustc's `have you added
//! the #[macro_use]` suggestion — because a macro of that name does exist,
//! further down — and `flatten_without_flattenable_fails.stderr` carries none,
//! because none does. The suggestion is not the right advice in either case. The
//! third points at two `#[derive(Setters)]` attributes and never mentions the
//! field, which is the whole reason it is worth having a file that shows it.
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
    harness.pass("tests/compile_fail/a_flattened_base.rs");
    harness.compile_fail("tests/compile_fail/*_fails.rs");
}
