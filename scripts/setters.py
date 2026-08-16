#!/usr/bin/env python3
"""Which request types a caller can only fill in by assignment.

Every request type here is `#[non_exhaustive]`, so nothing is unbuildable — the
fields stay public, and `let mut r = X::default(); r.limit = Some(50);` compiles
from any crate. What it is not is the idiom the rest of the surface reads in.
`GetOrdersRequest` shipped 0.1.0 with fourteen filters and no setter for any of
them, and nothing said so: a missing setter is not a compile error, not a
failing test, and not visible in a diff that adds a field. It is only visible by
reading the struct and the impl side by side and noticing a name in one and not
the other. Nobody does that for eighty-two types.

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
already applied to reach its count of 103. `ADDITIONS` carries the types a
caller must build that the name rule cannot see — `Identity` and `Disclosures`
are fields of the account *response* as well as of the account *creation*
payload, so no name rule reaches them. `EXCLUSIONS` carries the one false
positive: `TokenizationRequest` is named like a request and appears only in
return position.

Both maps are claims, not conveniences, and each entry says which. An entry that
stops matching a struct fails the run rather than going quiet, for the same
reason a stale `ALIASES` key fails `enum_drift.py`: an exemption covering
nothing still reads as a decision.

# What it does not check

Fields carrying `#[setters(skip = "…")]`. Three do, each because a constructor
already holds the name and two `pub fn` of one name cannot coexist in one impl.
The reason lives in the source next to the field, and the derive refuses a skip
without one, so there is nothing for a list here to add. They are printed so
they stay visible rather than becoming invisible by being handled.

Usage:
    python3 scripts/setters.py [--src src] [--report]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

# `pub struct Name {`, at the start of a line.
STRUCT_DECL = re.compile(r"^pub struct (\w+) \{")

# `    pub field: Option<T>,`. Only optional fields: a required field is a
# constructor argument, and the derive leaves it alone.
OPTION_FIELD = re.compile(r"^    pub (\w+): Option<.+>,$")

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


def attribute_block(lines: list[str], start: int) -> str:
    """The contiguous run of attribute and doc lines above `start`.

    Items in this crate are separated by a blank line, so walking up until one
    collects an item's own attributes and never the previous item's. Struct
    *fields* are not separated that way, which is why they are read downwards
    instead — see `parse`.
    """
    above = []
    index = start - 1
    while index >= 0 and lines[index].strip():
        above.append(lines[index])
        index -= 1
    return "\n".join(reversed(above))


def unwrap(text: str) -> str:
    """A Rust string literal's line continuations, resolved.

    `"a \\` followed by an indented `b"` is one string with one space in it, and
    printing the raw source instead puts a backslash and a run of spaces in the
    middle of the report.
    """
    return re.sub(r"\\\s*\n\s*", "", text).strip()


def parse(path: pathlib.Path) -> dict[str, tuple[bool, list[str], list[tuple[str, str]]]]:
    """Every struct in one file, as `{name: (derives, optional, skipped)}`.

    `skipped` pairs a field name with the reason recorded on it.
    """
    found: dict[str, tuple[bool, list[str], list[tuple[str, str]]]] = {}
    lines = path.read_text().splitlines()

    for index, line in enumerate(lines):
        declaration = STRUCT_DECL.match(line)
        if not declaration:
            continue

        name = declaration.group(1)
        derives = bool(DERIVES_SETTERS.search(attribute_block(lines, index)))

        # Read downwards, accumulating each field's attributes and clearing them
        # at the field they belong to. Walking *up* from a field cannot work:
        # fields are not separated by blank lines, so the run above one field
        # reaches back through every field before it, and a `skip` on the first
        # would be read as sitting on all of them.
        optional: list[str] = []
        skipped: list[tuple[str, str]] = []
        pending: list[str] = []
        body = index + 1
        while body < len(lines) and lines[body] != "}":
            field = OPTION_FIELD.match(lines[body])
            if field:
                optional.append(field.group(1))
                skip = SKIP_ATTR.search("\n".join(pending))
                if skip:
                    skipped.append((field.group(1), unwrap(skip.group(1))))
                pending = []
            elif re.match(r"^    pub \w+:", lines[body]):
                pending = []  # a required field, whose attributes end here too
            else:
                pending.append(lines[body])
            body += 1

        found[name] = (derives, optional, skipped)

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
        for name, (derives, optional, skipped) in parse(rs).items():
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
