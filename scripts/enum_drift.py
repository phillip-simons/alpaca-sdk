#!/usr/bin/env python3
"""Diff this crate's generated enums against the same-named spec schemas.

The 71 `wire_enum!` blocks come from alpaca-py, via `scripts/gen_enums.py`. The
specs are Alpaca's own. Where both name the same type, they should agree, and
mostly they do not: only 7 of the 19 with a same-named schema match exactly.

This is a quality report, not a bug report. An unknown value deserializes into
`Unknown(String)` rather than failing, so drift costs a caller a match arm, not
a decode. What it is good for is the opposite direction: a value the spec has
and the crate does not is a value nobody can match on by name.

# Why this is not in gen_enums.py

The roadmap asked for it there. It is separate because `gen_enums.py` needs an
alpaca-py checkout to run at all, and this needs only `specs/` and the checked-in
`.rs` files — which are the artifact that actually ships. A drift check that can
only run during a regeneration is a check that runs once a year.

# What it will not do

It never suggests removing a value the spec lacks. alpaca-py carries values
Alpaca still serves and has stopped documenting, and deleting one turns a
working match arm into an `Unknown`. Extra values are reported as a separate,
quieter list for that reason.

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

# Enums whose same-named spec schema is about something else. Naming a pair
# here is a claim that they are unrelated, not that the difference is fine.
NOT_DRIFT: dict[str, str] = {
    "Exchange": (
        "The spec's schema of this name is venue names; alpaca-py's is the "
        "single-letter tape codes the data API actually sends. Different "
        "vocabularies, same word."
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

    Reads the generated files rather than regenerating them, so this reports on
    what is checked in — including a hand edit that should not be there.
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
    shared = sorted(set(crate) & set(spec))

    print(f"{len(crate)} enums in the crate, {len(spec)} enum schemas in the specs")
    print(f"{len(shared)} share a name\n")

    agree: list[str] = []
    missing: dict[str, list[str]] = {}
    extra: dict[str, list[str]] = {}

    for name in shared:
        if name in NOT_DRIFT:
            continue
        ours = set(crate[name])
        theirs = spec[name]
        gap = sorted(theirs - ours)
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

    if missing:
        print("\nIn the spec, not in the crate — a value no caller can name:\n")
        for name, values in sorted(missing.items()):
            print(f"  {name}")
            for value in values:
                # The empty string is a real wire value here — Alpaca documents
                # `simple (or "")` for OrderClass — and printing it bare would
                # look like a blank line rather than a finding.
                print(f"    {value if value else '\"\" (the empty string)'}")
            # Keyed by the crate's spelling, since the whole point of an
            # unresolved pair is that the two spellings differ.
            for (enum, ours), note in UNRESOLVED.items():
                if enum == name:
                    print(f"    note: the crate carries {ours} — {note}")

    if extra:
        print("\nIn the crate, not in the spec. **Do not delete these.**")
        print("alpaca-py carries values Alpaca still serves and no longer documents;")
        print("removing one turns a working match arm into an `Unknown`.\n")
        for name, values in sorted(extra.items()):
            print(f"  {name}: {', '.join(values)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
