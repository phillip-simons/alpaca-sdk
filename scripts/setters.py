#!/usr/bin/env python3
"""Which request types a caller can only fill in by assignment.

Every request type here is `#[non_exhaustive]`, so nothing is unbuildable — the
fields stay public, and `let mut r = X::default(); r.limit = Some(50);` compiles
from any crate. What it is not is the idiom the rest of the surface reads in.
`GetOrdersRequest` shipped 0.1.0 with fourteen filters and no setter for any of
them, and nothing said so: a missing setter is not a compile error, not a
failing test, and not visible in a diff that adds a field. It is only visible by
reading the struct and the impl side by side and noticing a name in one and not
the other. Nobody does that for a hundred and twenty types.

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

**`ADDITIONS` is the weak point, and it fails silently.** A caller-built type
that is not named there is not reported as uncovered; it is not reported at all,
and this script says "every request type derives `Setters`" with a straight
face. That is not hypothetical: `CIPInfo` was listed and the five check types
nested inside it were not, so 68 of its fields sat uncovered under a clean
report. `tests/integration/request_construction.rs` is the cross-check — its
import list is every type a caller has to build, written down for a different
reason, and anything there with an optional field belongs here.

Both maps are claims, not conveniences, and each entry says which. An entry that
stops matching a struct fails the run rather than going quiet, for the same
reason a stale `ALIASES` key fails `enum_drift.py`: an exemption covering
nothing still reads as a decision.

# What it does not check

Fields carrying `#[setters(skip = "…")]`, which fall into two kinds. Either a
constructor already holds the name, and two `pub fn` of one name cannot coexist
in one impl; or the field is only coherent set alongside another, and one setter
writes the group — `OrderAmount`'s `qty`/`notional`, a bracket's class and its
legs, a routing code and its scheme.

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

# Anything that starts a `pub` field, matched before its type is known so that a
# declaration rustfmt wrapped across lines can be rejoined — see `fields`.
FIELD_START = re.compile(r"^    pub (\w+):")

# A rejoined `pub field: Option<T>,`. Only optional fields: a required field is
# a constructor argument, and the derive leaves it alone.
#
# The path prefix is optional because `option_inner` in the derive matches on the
# last segment, so `std::option::Option<u32>` is an optional field to the
# compiler. A pattern that insisted on the bare spelling disagreed with the
# derive about what this script is counting, in the direction where the field
# reads as required and the type stops being checked at all.
OPTION_FIELD = re.compile(r"^pub (\w+): (?:\w+::)*Option<.+>,$")

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

# Lines this script must understand and does not. A `pub struct` it cannot parse
# is a type that silently drops out of the report, which is the one outcome a
# gate must not have — so it stops rather than passing. This fired the first
# time it ran, on the newtype above.
#
# Deliberately *not* anchored to column 0, where `STRUCT_DECL` is. A struct
# indented inside an inline `pub mod` matches neither, and an unanchored guard
# is the difference between "this script does not handle inline modules" being
# an error and being silence.
UNPARSED_STRUCT = re.compile(r"^\s*pub struct\b")

# `Setters` inside a `#[derive(…)]`. Matched against the whole attribute block
# above a struct, so a derive list rustfmt has split across lines still hits.
DERIVES_SETTERS = re.compile(r"#\[derive\([^)]*\bSetters\b[^)]*\)\]", re.S)

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

        parts = [LINE_COMMENT.sub("", lines[index]).strip()]
        while not parts[-1].endswith(",") and index + 1 < len(lines):
            index += 1
            if lines[index] == "}":  # an unterminated declaration; stop here
                break
            parts.append(LINE_COMMENT.sub("", lines[index]).strip())
        declaration = " ".join(part for part in parts if part)

        found = OPTION_FIELD.match(declaration)
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
            if UNPARSED_STRUCT.match(line) and not UNNAMED_STRUCT.match(line):
                raise SyntaxError(
                    f"{path}:{index + 1}: cannot parse `{line.strip()}` — this "
                    f"script decides which types need `Setters`, so a struct it "
                    f"cannot read is one it would silently pass. Teach it the "
                    f"shape rather than letting it skip."
                )
            pending.append(line)
            index += 1
            continue

        derives = bool(DERIVES_SETTERS.search("\n".join(pending)))
        optional, skipped, index = fields(lines, index)
        found[declaration.group(1)] = (derives, optional, skipped)
        pending = []

    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
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
    types = 0
    fields = 0
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
            if name in EXCLUSIONS:
                seen_exclusions.add(name)
                continue
            if name in ADDITIONS:
                seen_additions.add(name)
            if not in_scope(name):
                continue

            types += 1
            fields += len(optional)
            if derives:
                covered += len(optional) - len(skipped)
                skips.extend((rs.as_posix(), field, why) for field, why in skipped)
            elif optional:
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

    print(f"{types} request types, {fields} optional fields")
    print(f"{covered} have a setter, {fields - covered} do not\n")

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
    print(
        f"\n{sum(c for e in gaps.values() for _, c in e)} optional fields have "
        f"no setter. Add `Setters` to the derive list on each type above.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
