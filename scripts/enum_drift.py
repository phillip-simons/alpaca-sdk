#!/usr/bin/env python3
"""Diff this crate's wire enums against their spec schemas.

The `wire_enum!` blocks are checked-in source; the specs are Alpaca's own. Where
both describe the same type they should agree. The run prints how many do; no
count is kept here, because every count kept here has gone stale.

Pairing is by name, with `ALIASES` for the types the two spell differently. That
map is not a convenience: an enum whose name does not match is not reported as
drifting, it is not reported at all, and `TradeEvent` reached a release missing
nine documented values inside exactly that silence.

This is a quality report, not a bug report. An unknown value deserializes into
`Unknown(String)` rather than failing, so drift costs a caller a match arm, not
a decode. What it is good for is the opposite direction: a value the spec has
and the crate does not is a value nobody can match on by name.

# What it will not do

It never suggests removing a value the spec lacks. Alpaca serves values it has
stopped documenting, and deleting one turns a working match arm into an
`Unknown`. Extra values are reported as a separate, quieter list for that
reason.

Usage:
    python3 scripts/enum_drift.py [--specs specs] [--src src]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

# `wire_enum! { … pub enum Name { Variant => "wire", … } }`.
ENUM_DECL = re.compile(r"^\s*pub enum (\w+) \{", re.M)
VARIANT = re.compile(r'^\s*(\w+) => "([^"]*)",\s*$')

# Crate enums whose spec schema exists under a different name. Without these the
# pair never meets: the comparison set is an intersection of names, so an enum
# this crate spells differently from Alpaca is not reported as drifting — it is
# not reported at all.
#
# That silence is not hypothetical. `TradeEvent` is Alpaca's
# `TradeUpdateEventType`, the names never matched, and it reached 0.1.0 carrying
# twelve of the twenty-one documented values with this report having no opinion
# on it. The inverse of `NOT_DRIFT`: naming a pair here is a claim that they are
# the same vocabulary under two names.
ALIASES: dict[str, str] = {
    "TradeEvent": "TradeUpdateEventType",
}

# Enums whose same-named spec schema is about something else. Naming a pair
# here is a claim that they are unrelated, not that the difference is fine.
NOT_DRIFT: dict[str, str] = {
    "Exchange": (
        "The spec's schema of this name is venue names; this crate's is the "
        "single-letter tape codes the data API actually sends. Different "
        "vocabularies, same word."
    ),
}

# Values the specs document and this crate deliberately does not carry, with the
# reason. Same purpose as `SKIP` in coverage.py: a value decided against must
# stop reading as a gap, or the report never converges.
DECIDED: dict[tuple[str, str], str] = {
    ("OrderClass", ""): (
        "Alpaca's own schema describes the empty string as a synonym for "
        "`simple`, and `Order::order_class` already maps both it and an absent "
        "field to `Simple` on the way in. A variant for it would only let a "
        "caller send `\"\"` where `simple` says the same thing."
    ),
}

# Differences a diff cannot settle, with what would.
UNRESOLVED: dict[tuple[str, str], str] = {
    ("TaxIdType", "ARG_AR_CUIT"): (
        "the spec spells it ARG_AG_CUIT; one of the two is a typo and only a "
        "live response says which"
    ),
}


def crate_enums(src: pathlib.Path) -> dict[str, list[str]]:
    """Every `wire_enum!` in the crate, as `{name: [wire values]}`.

    Reads the source files, so a variant added or renamed by hand shows up here
    the same way a spec change does.
    """
    enums: dict[str, list[str]] = {}
    for rs in sorted(src.rglob("*enums*.rs")):
        lines = rs.read_text().splitlines()
        current: str | None = None
        for line in lines:
            declaration = ENUM_DECL.match(line)
            if declaration:
                current = declaration.group(1)
                enums[current] = []
                continue
            if current is None:
                continue
            variant = VARIANT.match(line)
            if variant:
                enums[current].append(variant.group(2))
            elif re.match(r"^\s*\}\s*$", line):
                current = None
    return {name: values for name, values in enums.items() if values}


def spec_enums(specs: pathlib.Path) -> dict[str, set[str]]:
    """Every named schema under `components.schemas` that is a string enum.

    Parsed with regexes for the same reason `coverage.py` is: the specs are
    large, the shape needed is shallow, and a YAML dependency for two nested
    keys is not worth carrying.

    Only the schema's own `enum:` counts. A `properties.<field>.enum` is a
    different type that happens to live inside this one, and matching it by the
    outer schema's name would compare unrelated things.
    """
    found: dict[str, set[str]] = {}

    for spec in sorted(specs.glob("*.yaml")):
        lines = spec.read_text().splitlines()
        in_schemas = False
        name: str | None = None
        collecting = False

        for line in lines:
            if re.match(r"^  schemas:\s*$", line):
                in_schemas = True
                continue
            if in_schemas and re.match(r"^  \S", line):
                break  # the next key under `components:`
            if not in_schemas:
                continue

            schema = re.match(r"^    (\w+):\s*$", line)
            if schema:
                name, collecting = schema.group(1), False
                continue

            if name and re.match(r"^      enum:\s*$", line):
                collecting = True
                found.setdefault(name, set())
                continue

            if collecting:
                item = re.match(r"^        - (.*)$", line)
                if item:
                    found[name].add(item.group(1).strip().strip("'\""))
                else:
                    collecting = False

    return {name: values for name, values in found.items() if values}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--specs", type=pathlib.Path, default=pathlib.Path("specs"))
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    args = parser.parse_args()

    if not args.specs.is_dir():
        print(f"{args.specs} is missing — run `just specs` first", file=sys.stderr)
        return 1

    crate = crate_enums(args.src)
    spec = spec_enums(args.specs)

    # An alias that stops resolving on either side silently restores the very
    # blindness the map exists to remove — the pair drops out of the comparison
    # with no comment, which is indistinguishable from never having been aliased.
    for enum, schema in sorted(ALIASES.items()):
        if enum not in crate:
            print(
                f"ALIASES maps {enum}, which is not a wire_enum! in this crate "
                f"— it was probably renamed; update the key or drop the entry",
                file=sys.stderr,
            )
            return 1
        if schema not in spec:
            print(
                f"{enum} is aliased to {schema}, which no spec defines "
                f"— fix the alias or drop it",
                file=sys.stderr,
            )
            return 1

    schema_of = {name: ALIASES.get(name, name) for name in crate}
    shared = sorted(name for name in crate if schema_of[name] in spec)

    print(f"{len(crate)} enums in the crate, {len(spec)} enum schemas in the specs")
    print(f"{len(shared)} have a schema to compare against\n")

    agree: list[str] = []
    missing: dict[str, list[str]] = {}
    extra: dict[str, list[str]] = {}

    for name in shared:
        if name in NOT_DRIFT:
            continue
        ours = set(crate[name])
        theirs = spec[schema_of[name]]
        gap = sorted(value for value in theirs - ours if (name, value) not in DECIDED)
        surplus = sorted(ours - theirs)
        if not gap and not surplus:
            agree.append(name)
            continue
        if gap:
            missing[name] = gap
        if surplus:
            extra[name] = surplus

    print(f"{len(agree)} agree exactly: {', '.join(agree) or 'none'}")
    for name, reason in NOT_DRIFT.items():
        if name in shared:
            print(f"\n{name}: not drift — {reason}")
    for (name, value), reason in DECIDED.items():
        if name in shared:
            shown = value if value else '""'
            print(f"\n{name} {shown}: decided against — {reason}")

    if missing:
        print("\nIn the spec, not in the crate — a value no caller can name:\n")
        for name, values in sorted(missing.items()):
            print(f"  {name}")
            for value in values:
                # The empty string is a real wire value here — Alpaca documents
                # `simple (or "")` for OrderClass — and printing it bare would
                # look like a blank line rather than a finding.
                #
                # Built outside the f-string: a backslash inside an f-string
                # expression is a syntax error before Python 3.12, which made
                # this script unrunnable on the Python current macOS ships.
                shown = value if value else '"" (the empty string)'
                print(f"    {shown}")
            # Keyed by the crate's spelling, since the whole point of an
            # unresolved pair is that the two spellings differ.
            for (enum, ours), note in UNRESOLVED.items():
                if enum == name:
                    print(f"    note: the crate carries {ours} — {note}")

    if extra:
        print("\nIn the crate, not in the spec. **Do not delete these.**")
        print("Alpaca still serves values it has stopped documenting;")
        print("removing one turns a working match arm into an `Unknown`.\n")
        for name, values in sorted(extra.items()):
            print(f"  {name}: {', '.join(values)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
