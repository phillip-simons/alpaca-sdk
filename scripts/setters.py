#!/usr/bin/env python3
"""Which request types a caller can only fill in by assignment.

Every request type here is `#[non_exhaustive]`, so nothing is unbuildable — the
fields stay public, and `let mut r = X::default(); r.limit = Some(50);` compiles
from any crate. What it is not is the idiom the rest of the surface reads in.
`GetOrdersRequest` shipped 0.1.0 with fourteen filters and no setter for any of
them, and nothing said so: a missing setter is not a compile error, not a
failing test, and not visible in a diff that adds a field. It is only visible by
reading the struct and the impl side by side and noticing a name in one and not
the other. Nobody does that for every type in this crate — the run prints how
many there are, and no count is kept in this prose, because every count kept in
prose here has gone stale.

# What this checks, and what the compiler checks

`#[derive(Setters)]` reads the real field list, so a type that derives it cannot
have a field without a setter — that half is the compiler's, not this script's,
and it stays true for a field added tomorrow by someone who never read this
file. What is left is the one question a derive cannot ask about itself:
**which types should be deriving it and are not.**

That is the whole job here. It is a much smaller question than the one the first
version of this script answered, and deliberately so: an earlier design listed
every field beside its struct in a macro, and needed this script to diff the two
lists. Reading the fields directly deleted that entire class of drift, and most
of this script with it.

# What counts as a request

The `*Request*` name rule, which is the rule the `#[non_exhaustive]` audit
already applied. `ADDITIONS` carries the types a caller must build that the name
rule cannot see — `Identity` and `Disclosures` are fields of the account
*response* as well as of the account *creation* payload, so no name rule reaches
them. `EXCLUSIONS` carries the one false positive: `TokenizationRequest` is
named like a request and appears only in return position.

**`ADDITIONS` is the weak point, because it fails silently.** A caller-built type
that is not named there is not reported as uncovered; it is not reported at all,
and this script says "every request type derives `Setters`" with a straight
face. That is not hypothetical: `CIPInfo` was listed and the five check types
nested inside it were not, so 68 of its fields sat uncovered under a clean
report.

So the cross-check *runs*. `tests/integration/request_construction.rs` imports
every type a caller has to build — written down for an unrelated reason, since
that test exists to prove they are all still constructible from outside the
crate — and any of them with an optional field that the scope rule does not
reach fails this run. It was a note in this docstring first, and a note would
not have caught the five.

Both maps are claims, not conveniences, and each entry says which. An entry that
stops matching a struct fails the run rather than going quiet, for the same
reason a stale `ALIASES` key fails `enum_drift.py`: an exemption covering
nothing still reads as a decision.

# What it does not check

Fields carrying `#[setters(skip = "…")]`, which fall into three kinds. Either a
constructor already holds the name, and two `pub fn` of one name cannot coexist
in one impl; or the field is only coherent set alongside another, and one setter
writes the group — `OrderAmount`'s `qty`/`notional`, a bracket's class and its
legs, a routing code and its scheme; or the `Option` is there so the field
serializes as *omitted* rather than `null`, and is not a value a caller picks at
all, which is `AccountConfiguration`'s two.

The reason lives in the source next to the field, and the derive refuses a skip
without one, so there is nothing for a list here to add. They are printed on
every run so they stay visible rather than becoming invisible by being handled.

Usage:
    python3 scripts/setters.py [--src src] [--report]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

# `pub struct Name {`, at the start of a line, with an optional generic
# parameter list. No request type is generic today; the alternative was a
# pattern that silently does not match one, which is the failure mode this whole
# script exists to remove.
STRUCT_DECL = re.compile(r"^pub struct (\w+)(?:<[^>]*>)?\s*\{")

# `pub struct PingRequest {}` — braces opened and closed on one line. Matched
# separately because the body scan looks for a line that is exactly `}`, and an
# empty struct has none: the scan ran on into the *next* struct, credited that
# struct's fields to this one, and consumed it so it was never checked at all.
# Latent — no empty-braced struct is in `src/` today, and `cargo fmt` leaves the
# shape alone, so nothing would have caught it arriving.
EMPTY_STRUCT = re.compile(r"^pub struct (\w+)(?:<[^>]*>)?\s*\{\s*\}\s*$")

# Anything that starts a `pub` field, matched before its type is known so that a
# declaration rustfmt wrapped across lines can be rejoined — see `fields`.
FIELD_START = re.compile(r"^    pub ((?:r#)?\w+):")

# A rejoined field of any type. Not used to classify — `OPTION_FIELD` does that
# — but to tell "this is a required field" from "the rejoin produced something
# this script does not understand". Without the distinction the second case
# looks exactly like the first, which is how a field goes missing quietly.
REQUIRED_FIELD = re.compile(r"^pub ((?:r#)?\w+): .+,$")

# A rejoined `pub field: Option<T>,`. Only optional fields: a required field is
# a constructor argument, and the derive leaves it alone.
#
# The path prefix is optional, leading `::` included, because `option_inner` in
# the derive matches on the last segment and only refuses a `qself` — so
# `std::option::Option<u32>` and `::core::option::Option<u32>` are both optional
# fields to the compiler. A pattern that insisted on the bare spelling disagreed
# with the derive about what this script is counting, in the direction where the
# field reads as required and the type stops being checked at all. The leading
# `::` was a second instance of the same bug, found after the first was fixed:
# match what the derive matches, not what the source usually looks like.
OPTION_FIELD = re.compile(r"^pub ((?:r#)?\w+): (?:::)?(?:\w+::)*Option<.+>,$")

# A trailing `// …` on a field, stripped before the field is classified. Without
# this the declaration no longer ends in `,`, so it matches nothing, and a type
# whose every optional field carries a comment is reported as having none —
# which is indistinguishable from being fully covered.
LINE_COMMENT = re.compile(r"\s*//(?!/).*$")

# A tuple or unit struct — `pub struct Codes(HashMap<String, String>);`. It has
# no named fields, so `Setters` has nothing to generate for it and it is out of
# scope whatever its name. Recognised explicitly rather than left to fall
# through, so that the check below can be strict about everything else.
UNNAMED_STRUCT = re.compile(r"^pub struct (\w+)(?:<[^>]*>)?\s*[(;]")

# `pub(crate) struct …`. Out of scope by definition: a type a caller cannot name
# is not a type a caller builds. Matched so the guard below can be strict about
# `pub struct` without failing on the three of these in `src/`.
RESTRICTED_STRUCT = re.compile(r"^\s*pub\([^)]*\) struct\b")

# Lines this script must understand and does not. A `pub struct` it cannot parse
# is a type that silently drops out of the report, which is the one outcome a
# gate must not have — so it stops rather than passing. This fired the first
# time it ran, on the newtype above.
#
# Deliberately *not* anchored to column 0, where `STRUCT_DECL` is. A struct
# indented inside an inline `pub mod` matches neither, and an unanchored guard
# is the difference between "this script does not handle inline modules" being
# an error and being silence.
UNPARSED_STRUCT = re.compile(r"^\s*pub(?:\([^)]*\))? struct\b")

# `Setters` inside a `#[derive(…)]`. Matched against the attribute block above a
# struct, so a derive list rustfmt has split across lines still hits.
DERIVES_SETTERS = re.compile(r"#\[derive\([^)]*\bSetters\b[^)]*\)\]", re.S)

# A line comment anywhere — at the start of a line or trailing one — removed
# before the block is searched for a derive. Without this a struct whose
# *documentation* shows `#[derive(…, Setters)]` in an example is credited with
# deriving it, and that example is the one CONTRIBUTING and the derive's own
# rustdoc both use. A trailing comment inside a rustfmt-split derive list is the
# same hazard one line in.
LINE_COMMENT_ANYWHERE = re.compile(r"//.*$", re.M)

# The same hazard in the other comment syntax. `rustfmt` reformats neither, so
# nothing else in the toolchain would notice a `/* … Setters … */` sitting above
# a struct that derives nothing.
BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.S)

# `#[setters(skip = "why")]`, and the field it sits above.
SKIP_ATTR = re.compile(r'#\[setters\(skip = "(.+?)"\)\]', re.S)

# Types a caller must build that the `*Request*` name rule cannot see. Every one
# is a field of some request, or the body of a POST, and is reachable only by
# construction. Naming a type here is a claim that a caller builds it.
ADDITIONS: dict[str, str] = {
    "UpdatableIdentity": "the PATCH body for an account's identity",
    "UpdatableContact": "the PATCH body for an account's contact details",
    "Contact": "built for account creation; also a field of the account response",
    "Identity": "built for account creation; also a field of the account response",
    "Disclosures": "built for account creation; also a field of the account response",
    "TrustedContact": "built for account creation; also a field of the account response",
    "Agreement": "built for account creation, one per agreement signed",
    "W8BenDocument": "built for a non-US account's tax documentation",
    "Weight": "a leg of a rebalancing portfolio",
    "RebalancingCondition": "a trigger on a rebalancing subscription",
    "CIPInfo": "the CIP payload uploaded for an account",
    # The five checks nested inside `CIPInfo`. `tests/integration/
    # request_construction.rs` was written because these were unbuildable from
    # outside the crate, and its comment names them as what `CIPInfo` "drags in"
    # — so a caller who builds a CIP payload builds these too. `CIPInfo` was
    # here and they were not, which left 68 of its fields uncovered while the
    # report read clean.
    "CIPKycInfo": "the KYC check inside `CIPInfo`",
    "CIPDocument": "the document check inside `CIPInfo`",
    "CIPPhoto": "the photo check inside `CIPInfo`",
    "CIPIdentity": "the identity check inside `CIPInfo`",
    "CIPWatchlist": "the watchlist check inside `CIPInfo`",
    "ManualACHRelationship": "the hand-entered arm of `CreateACHRelationshipRequest`",
    # No optional fields today, so nothing is generated for them and nothing is
    # demanded. Named anyway: they are built by a caller, and the day one grows
    # an optional field is the day this list decides whether anybody notices.
    "PlaidACHRelationship": "the Plaid arm of `CreateACHRelationshipRequest`",
    "BankAddress": "the address on a `CreateBankRequest`",
    "SettlementTransfer": "an element of `CreateInstantFundingSettlementRequest::transfers`",
    "JitSettlementAccount": "an element of `CreateJitSettlementRequest::accounts`",
    "TransmitterInfo": "the travel-rule payload on a crypto transfer",
    "AccountConfiguration": "read-modify-write: fetched, adjusted, sent back",
    "TokenizationMintCallback": "the callback body a caller posts back",
    "KycResults": "built inside `Disclosures` for a manually-approved account",
}

# Named like a request, and not one. Naming a type here is a claim that no
# caller ever builds it.
EXCLUSIONS: dict[str, str] = {
    "TokenizationRequest": (
        "a response record — it appears only in `Result<…>` return position "
        "across both clients, and a setter on it would serve nobody"
    ),
}


def in_scope(name: str) -> bool:
    """Whether a type is one a caller builds."""
    if name in EXCLUSIONS:
        return False
    return "Request" in name or name in ADDITIONS


def unwrap(text: str) -> str:
    """A Rust string literal's line continuations, resolved.

    `"a \\` followed by an indented `b"` is one string with one space in it, and
    printing the raw source instead puts a backslash and a run of spaces in the
    middle of the report.
    """
    return re.sub(r"\\\s*\n\s*", "", text).strip()


def balance(text: str) -> int:
    """How many brackets `text` leaves open.

    Only enough of Rust's grammar to know whether a type is finished. A `->` is
    not a closing angle bracket and would otherwise count as one, which matters
    for a field holding a function pointer.
    """
    depth = 0
    for opening, closing in (("<", ">"), ("(", ")"), ("[", "]")):
        depth += text.count(opening) - text.replace("->", "").count(closing)
    return depth


def fields(lines: list[str], start: int) -> tuple[list[str], list[tuple[str, str]], int]:
    """One struct's optional fields and skips, plus the index of its closing `}`.

    Read downwards, accumulating each field's attributes and clearing them at
    the field they belong to. Walking *up* from a field cannot work: fields are
    not separated by blank lines, so the run above one reaches back through
    every field before it, and a `skip` on the first would be read as sitting on
    all of them.

    A declaration rustfmt wrapped across lines is rejoined before it is
    classified. Matching one line at a time read `pub some_very_long_name:` as a
    *required* field — the type it was looking for was on the next line — so the
    field vanished from the count and its type vanished from the report, in the
    silent direction.
    """
    optional: list[str] = []
    skipped: list[tuple[str, str]] = []
    pending: list[str] = []

    index = start + 1
    while index < len(lines) and lines[index] != "}":
        if not FIELD_START.match(lines[index]):
            pending.append(lines[index])
            index += 1
            continue

        # Join until the brackets balance *and* the declaration terminates.
        # Stopping at the first trailing comma is not enough: rustfmt wraps a
        # long nested generic as `pub filters: Option< HashMap< String,` — which
        # ends in a comma, mid-type, and reads as a complete required field.
        parts = [LINE_COMMENT.sub("", lines[index]).strip()]
        while index + 1 < len(lines) and (
            balance("".join(parts)) > 0 or not parts[-1].endswith(",")
        ):
            index += 1
            if lines[index] == "}":  # an unterminated declaration; stop here
                break
            parts.append(LINE_COMMENT.sub("", lines[index]).strip())
        declaration = " ".join(part for part in parts if part)

        found = OPTION_FIELD.match(declaration)
        if not found and not REQUIRED_FIELD.match(declaration):
            # Neither shape. Silently filing it under "required" is how a field
            # disappears from the count, and every hole this script has had was
            # of that kind: the pattern did not match, nothing said so, and the
            # type stopped being checked. Refuse instead.
            raise SyntaxError(
                f"cannot classify `{declaration}` — this script decides which "
                f"fields need a setter, so a declaration it cannot read is one "
                f"it would pass in silence. Teach it the shape."
            )
        if found:
            optional.append(found.group(1))
            skip = SKIP_ATTR.search("\n".join(pending))
            if skip:
                skipped.append((found.group(1), unwrap(skip.group(1))))
        pending = []
        index += 1

    return optional, skipped, index


def parse(path: pathlib.Path) -> dict[str, tuple[bool, list[str], list[tuple[str, str]]]]:
    """Every struct in one file, as `{name: (derives, optional, skipped)}`.

    `skipped` pairs a field name with the reason recorded on it.

    One downward pass, carrying the attribute lines seen since the last item
    boundary. An earlier version walked *upwards* from each `pub struct` until a
    blank line, on the reasoning that items here are blank-line separated — but
    two that are not merge, and the second is then credited with the first's
    `#[derive(Setters)]`. That is a gate reporting coverage it did not find,
    which is the one way for this script to be worse than not existing.
    """
    found: dict[str, tuple[bool, list[str], list[tuple[str, str]]]] = {}
    lines = path.read_text().splitlines()
    pending: list[str] = []

    index = 0
    while index < len(lines):
        line = lines[index]

        # An item boundary: a blank line, or the `}` that closes the last one.
        if not line.strip() or line == "}":
            pending = []
            index += 1
            continue

        declaration = STRUCT_DECL.match(line)
        if not declaration:
            if RESTRICTED_STRUCT.match(line):
                pending = []
                index += 1
                continue
            if UNNAMED_STRUCT.match(line):
                # A tuple or unit struct is an item boundary like any other. It
                # used to fall through to the accumulator, so its `#[derive(…)]`
                # stayed pending and was credited to whatever struct came next.
                pending = []
                index += 1
                continue
            if UNPARSED_STRUCT.match(line):
                raise SyntaxError(
                    f"{path}:{index + 1}: cannot parse `{line.strip()}` — this "
                    f"script decides which types need `Setters`, so a struct it "
                    f"cannot read is one it would silently pass. Teach it the "
                    f"shape rather than letting it skip."
                )
            pending.append(line)
            index += 1
            continue

        # Comments are removed rather than whole lines dropped. Dropping lines
        # that *begin* with `//` misses one sitting inside a rustfmt-split
        # derive list — `    Clone, // TODO: add Setters` — where the word is in
        # a comment and the derive list is real, so the search hits and the type
        # is credited. Three spellings of the same hazard now; strip the comment
        # text wherever it sits, and there is only one.
        attributes = LINE_COMMENT_ANYWHERE.sub(
            "", BLOCK_COMMENT.sub("", "\n".join(pending))
        )
        derives = bool(DERIVES_SETTERS.search(attributes))

        empty = EMPTY_STRUCT.match(line)
        if empty:
            found[empty.group(1)] = (derives, [], [])
            pending = []
            index += 1
            continue
        optional, skipped, index = fields(lines, index)
        found[declaration.group(1)] = (derives, optional, skipped)
        pending = []

    return found


def buildable(path: pathlib.Path) -> set[str]:
    """The types `request_construction.rs` imports, as the cross-check on scope.

    `ADDITIONS` is where this script fails silently: a caller-built type that is
    not named there is not reported as uncovered, it is not reported at all.
    That test's import list is the same set written down for an unrelated
    reason — it exists to prove every input type is still constructible from
    outside the crate — so disagreeing with it is a question worth asking on
    every run rather than a note in a docstring.

    Returns an empty set if the file has moved, and says so: silently skipping
    the cross-check would restore exactly the blindness it exists to remove.
    """
    if not path.is_file():
        print(
            f"{path} is missing — it is the cross-check on `ADDITIONS`, so "
            f"losing it costs this script the one thing it cannot see itself. "
            f"Point `--buildable` at wherever the buildability test moved to.",
            file=sys.stderr,
        )
        return set()

    imported: set[str] = set()
    for block in re.findall(r"use alpaca_sdk::\w+::\{(.*?)\};", path.read_text(), re.S):
        imported |= {name.strip() for name in block.split(",") if name.strip()}
    return imported


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    parser.add_argument(
        "--buildable",
        type=pathlib.Path,
        default=pathlib.Path("tests/integration/request_construction.rs"),
        help="the test whose import list cross-checks ADDITIONS",
    )
    parser.add_argument(
        "--report",
        action="store_true",
        help="print the gaps and exit 0, rather than failing on them",
    )
    args = parser.parse_args()

    if not args.src.is_dir():
        print(f"{args.src} is missing", file=sys.stderr)
        return 1

    # Keyed by (file, name): four struct names are defined twice, and
    # `OrderRequest` in broker/ has different fields from the trading/ one.
    gaps: dict[str, list[tuple[str, int]]] = {}
    skips: list[tuple[str, str, str]] = []
    seen_additions: set[str] = set()
    seen_exclusions: set[str] = set()
    declared: dict[str, int] = {}
    types = 0
    optional_fields = 0
    covered = 0

    for rs in sorted(args.src.rglob("*.rs")):
        try:
            parsed = parse(rs)
        except SyntaxError as unreadable:
            # A traceback in a CI log reads as "the checker is broken" when what
            # it means is "the checker found source it will not guess about".
            print(unreadable, file=sys.stderr)
            return 1
        for name, (derives, optional, skipped) in parsed.items():
            declared[name] = len(optional)
            if name in EXCLUSIONS:
                seen_exclusions.add(name)
                continue
            if name in ADDITIONS:
                seen_additions.add(name)
            if not in_scope(name):
                continue

            types += 1
            optional_fields += len(optional)
            if derives:
                covered += len(optional) - len(skipped)
                skips.extend((rs.as_posix(), field, why) for field, why in skipped)
            else:
                # Demanded even with no optional fields, so that the report's
                # own summary line is true. It also does the useful thing: a
                # type carrying the derive picks up a setter the day someone
                # adds an optional field to it, with nobody having to notice.
                gaps.setdefault(rs.as_posix(), []).append((name, len(optional)))

    # An entry that no longer matches a struct is worse than a missing one: it
    # reads as a settled decision while covering nothing.
    for label, entries, seen in (
        ("ADDITIONS", ADDITIONS, seen_additions),
        ("EXCLUSIONS", EXCLUSIONS, seen_exclusions),
    ):
        stale = sorted(set(entries) - seen)
        if stale:
            print(
                f"{label} names {', '.join(stale)}, which no struct in "
                f"{args.src} declares — renamed, or the entry is dead",
                file=sys.stderr,
            )
            return 1

    # The cross-check the docstring calls for. A type the buildability test
    # imports is a type a caller has to build; if it has optional fields and the
    # scope rule does not reach it, this script would say nothing about it at
    # all. That is how `CIPInfo`'s five nested check types sat uncovered under a
    # clean report — they were in that test's import list the whole time.
    unreached = sorted(
        name
        for name in buildable(args.buildable)
        if declared.get(name) and not in_scope(name)
    )
    if unreached:
        print(
            f"{', '.join(unreached)} — built by a caller (they are in "
            f"{args.buildable}) and have optional fields, but no name rule "
            f"reaches them. Add each to ADDITIONS, or this script has no "
            f"opinion on whether they derive `Setters`.",
            file=sys.stderr,
        )
        return 1

    print(f"{types} request types, {optional_fields} optional fields")
    print(f"{covered} have a setter, {optional_fields - covered} do not\n")

    if skips:
        print("No setter, by decision — each field says why:\n")
        for path, field, why in sorted(skips):
            print(f"  {path}: {field} — {why}")
        print()

    if not gaps:
        print("Every request type derives `Setters`.")
        return 0

    print("Does not derive `Setters` — reachable only by assignment:\n")
    for path, entries in sorted(gaps.items()):
        subtotal = sum(count for _, count in entries)
        print(f"  {path} ({subtotal})")
        for name, count in sorted(entries):
            print(f"    {name} ({count})")

    if args.report:
        return 0
    # Counted in types, not fields. A type with no optional fields is still
    # required to derive it — so that the day one is added the setter follows —
    # and summarising in fields said "0 optional fields have no setter" while
    # exiting 1, which reads as the script contradicting itself.
    missing = sum(len(entries) for entries in gaps.values())
    print(
        f"\n{missing} request {'type' if missing == 1 else 'types'} "
        f"{'does' if missing == 1 else 'do'} not derive `Setters`. Add it to "
        f"the derive list on each type above.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
