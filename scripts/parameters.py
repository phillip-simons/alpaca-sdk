#!/usr/bin/env python3
"""Diff the query parameters Alpaca documents against the ones this crate sends.

`scripts/coverage.py` compares paths and methods. A route can be implemented,
counted, and still be missing half of what it accepts — which is not a
hypothetical: hand-checking three routes in Phase 6.5 turned up four missing
parameters, `asset_class`, `before_order_id` and `after_order_id` on
`GET /v2/orders` and `show_deliverables` on the option contracts route. Three
routes out of 251 is not a sample, and reading does not scale to the rest.

Run `just reference` first: this reads the `parameters` recorded in
`specs/reference.json`, which is the published reference rather than the
vendored specs. A reference.json written before that field existed is reported
as such rather than silently passing.

# What "the crate sends this" means here

There is no mechanical path from a route to the request struct that serializes
its query string, so this does not try to find one. For each route it takes the
crate files that call it, widens to their module — `src/trading/client.rs`
becomes `src/trading/` — and asks whether the parameter's name appears anywhere
in that module as a field name or a `#[serde(rename)]`. That is the vocabulary a
query string can be built from.

So the check is one-directional and deliberately loose: a name it does not find
is definitely not sent, and a name it does find might belong to a different
struct in the same module. It is a list of things to look at, in the same spirit
as COVERAGE.md — false positives cost a minute, and a false negative costs a
parameter nobody notices for a year.

Parameters the crate deliberately omits go in `SKIP` with the reason, for the
same reason `coverage.py` has one: without it the report never converges,
because a parameter we decided against looks exactly like one nobody has got to.

Usage:
    python3 scripts/parameters.py [--reference specs/reference.json] [--src src]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
from collections import defaultdict

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from coverage import crate_routes, normalize  # noqa: E402

# Parameters this crate knowingly does not send, keyed by
# `(method, normalized path, parameter)`.
SKIP: dict[tuple[str, str, str], str] = {}

# A struct field, a function parameter, or anything else written `name:` — all
# of which are names this module could serialize under.
FIELD = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*[A-Za-z&<(\[]", re.M)

# `#[serde(rename = "x")]`, which is how most of the wire names in this crate
# are spelled, and the only place a name that is not a legal Rust identifier can
# appear.
RENAME = re.compile(r'rename\s*=\s*"([^"]+)"')

# A string literal that reads like a wire name. Not every query parameter goes
# through a struct: the broker event streams build theirs as `("since_ulid", …)`
# pairs, because the same field is spelled differently per API version. Matching
# these costs some precision — "status" appears in prose too — and the
# alternative is reporting a parameter that is demonstrably sent.
LITERAL = re.compile(r'"([a-z][a-z0-9_]*)"')


def module_of(source: str) -> pathlib.PurePosixPath:
    """The directory a crate file belongs to, as the unit to search.

    `src/trading/client.rs` becomes `src/trading`. A route's parameters are
    built by a request struct that lives beside the client calling it rather
    than in the same file, so the file alone is too narrow a place to look.
    """
    return pathlib.PurePosixPath(source).parent


def vocabulary(module: pathlib.PurePosixPath) -> set[str]:
    """Every name the files under `module` could serialize a parameter under.

    Always includes `src/types/`, which holds the request pieces shared across
    surfaces — a parameter defined there is sent by whichever client uses it.

    `src/broker` also gets `src/trading`, because the broker API acts on behalf
    of trading accounts and reuses their request types wholesale; that is why
    `broker` implies `trading` as a cargo feature. Without this every parameter
    of every broker trading route reads as missing.
    """
    directories = {pathlib.Path(module), pathlib.Path("src/types")}
    if pathlib.PurePosixPath(module).name == "broker":
        directories.add(pathlib.Path("src/trading"))

    names: set[str] = set()
    for directory in directories:
        if not directory.is_dir():
            continue
        for rs in directory.rglob("*.rs"):
            text = rs.read_text()
            names.update(FIELD.findall(text))
            names.update(RENAME.findall(text))
            names.update(LITERAL.findall(text))
    return names


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=pathlib.Path, default=pathlib.Path("specs/reference.json"))
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    args = parser.parse_args()

    if not args.reference.is_file():
        print(f"{args.reference} is missing — run `just reference` first", file=sys.stderr)
        return 1

    rows = json.loads(args.reference.read_text())
    if not any("parameters" in row for row in rows):
        print(
            f"{args.reference} predates the `parameters` field — rerun `just reference`",
            file=sys.stderr,
        )
        return 1

    routes, _ = crate_routes(args.src)

    documented = 0
    missing: dict[tuple[str, str], list[tuple[str, str]]] = defaultdict(list)
    skipped = 0
    unmatched_routes = 0
    caches: dict[str, set[str]] = {}

    for row in rows:
        key = (row["method"], normalize(row["path"]))
        sources = routes.get(key)
        if not sources:
            # Not implemented, or implemented somewhere this cannot see. Either
            # way it is coverage.py's question, not this one.
            unmatched_routes += 1
            continue

        for parameter in row.get("parameters", []):
            # Path parameters are interpolated into the URL under whatever name
            # the crate likes, and header parameters are the transport's job.
            if parameter["in"] != "query":
                continue
            documented += 1

            name = parameter["name"]
            if (row["method"], normalize(row["path"]), name) in SKIP:
                skipped += 1
                continue

            found = False
            for source in sorted(set(sources)):
                module = str(module_of(source))
                if module not in caches:
                    caches[module] = vocabulary(module_of(source))
                if name in caches[module]:
                    found = True
                    break

            if not found:
                required = "required" if parameter["required"] else "optional"
                missing[(row["method"], row["path"])].append((name, required))

    print(f"{documented} documented query parameters on routes this crate implements")
    print(f"{unmatched_routes} reference operations not matched to a crate route (see COVERAGE.md)")
    if skipped:
        print(f"{skipped} skipped by decision (see SKIP in this file)")

    if not missing:
        print("no gaps")
        return 0

    total = sum(len(names) for names in missing.values())
    print(f"\n{total} parameters not found in the implementing module:\n")
    for (method, path), names in sorted(missing.items(), key=lambda item: item[0][1]):
        print(f"  {method.upper():6} {path}")
        for name, required in sorted(names):
            print(f"           {name} ({required})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
