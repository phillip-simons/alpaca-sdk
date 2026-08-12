#!/usr/bin/env python3
"""Diff this crate's routes against Alpaca's OpenAPI specs.

The specs are the ones `alpacahq/alpaca-java` vendors and drift-checks against
upstream, which makes them the closest machine-readable statement of what the
API is. Fetch them with `just specs`, then run this.

The output is a review document, not a gate. Path matching ignores the version
segment and the names of path parameters, so `/v1/accounts/{account_id}` and a
crate literal of `/accounts/{id}` match. That is deliberate — the versions this
crate targets are set per client and per stream, and reconstructing them
statically would report noise. Both sides of every match are printed so a
version mismatch is visible rather than assumed away.

Usage:
    python3 scripts/coverage.py <specs-dir> [--out COVERAGE.md]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
from collections import defaultdict

METHODS = {"get", "post", "put", "patch", "delete", "head", "options"}

# Version segments Alpaca uses. Stripped from both sides before matching.
VERSION = re.compile(r"^/(v\d+(?:beta\d*)?)(?=/)")

# Path parameters are matched by position, not by name: the crate calls it
# `{account_id}` in one place and `{id}` in another, and the spec has its own
# spelling again.
PARAM = re.compile(r"\{[^}]*\}")


def normalize(path: str) -> str:
    """A path reduced to what is worth comparing."""
    path = VERSION.sub("", path)
    path = PARAM.sub("{}", path)
    return path.rstrip("/") or "/"


def spec_operations(spec: pathlib.Path) -> list[tuple[str, str]]:
    """Every (method, path) in an OpenAPI document's `paths` section."""
    operations: list[tuple[str, str]] = []
    in_paths = False
    current: str | None = None

    for line in spec.read_text().splitlines():
        if re.match(r"^paths:\s*$", line):
            in_paths = True
            continue
        if in_paths and re.match(r"^\S", line):
            break  # the next top-level key
        if not in_paths:
            continue

        path = re.match(r"^  (/\S*):\s*$", line)
        if path:
            current = path.group(1)
            continue

        method = re.match(r"^    (\w+):\s*$", line)
        if method and current and method.group(1).lower() in METHODS:
            operations.append((method.group(1).lower(), current))

    return operations


# `self.rest.get("/x", ..)`, `self.rest.post(&format!("/x/{y}"), ..)`, and the
# same split across lines by rustfmt.
REST_CALL = re.compile(
    r"\.rest\s*\.\s*(get|post|put|patch|delete)\s*\(\s*&?\s*(?:format!\s*\(\s*)?\"([^\"]+)\"",
    re.S,
)
# `send_void(Method::DELETE, &format!("/x"), ..)` and the events helper.
VOID_CALL = re.compile(
    r"send_void\s*\(\s*Method::(GET|POST|PUT|PATCH|DELETE)\s*,\s*&?\s*(?:format!\s*\(\s*)?\"([^\"]+)\"",
    re.S,
)
EVENT_CALL = re.compile(
    r"\.events\s*\(\s*EventVersion::(V\d)\s*,\s*\"([^\"]+)\"",
    re.S,
)
# The document download builds its URL by hand.
RAW_GET = re.compile(r"let path = format!\(\s*\"([^\"]+)\"", re.S)
# Market data goes through the pagination helper rather than calling the
# transport directly: `MarketDataRequest::paged("/stocks/bars")`. All are GETs.
MARKET_DATA = re.compile(
    r"MarketDataRequest::(?:paged|paged_with_limit|latest)\s*\(\s*\"([^\"]+)\"",
    re.S,
)


def crate_routes(src: pathlib.Path) -> dict[tuple[str, str], list[str]]:
    """Every route the crate calls, as {(method, normalized path): [sources]}."""
    routes: dict[tuple[str, str], list[str]] = defaultdict(list)

    for rs in sorted(src.rglob("*.rs")):
        text = rs.read_text()
        where = str(rs.relative_to(src.parent))

        # (pattern, method) where a method of None means "the first group is it".
        for pattern, fixed_method in (
            (REST_CALL, None),
            (VOID_CALL, None),
            (EVENT_CALL, "get"),
            (RAW_GET, "get"),
            (MARKET_DATA, "get"),
        ):
            for match in pattern.finditer(text):
                groups = match.groups()
                path = groups[-1]
                method = fixed_method or groups[0].lower()
                routes[(method, normalize(path))].append(where)

    return routes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("specs", type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("COVERAGE.md"))
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    args = parser.parse_args()

    surfaces = ["trading", "data", "broker"]
    missing_specs = [s for s in surfaces if not (args.specs / f"{s}.yaml").is_file()]
    if missing_specs:
        print(
            f"missing specs: {', '.join(missing_specs)} — run `just specs` first",
            file=sys.stderr,
        )
        return 1

    routes = crate_routes(args.src)
    implemented = set(routes)
    matched: set[tuple[str, str]] = set()

    lines = [
        "# Route coverage",
        "",
        "Generated by `scripts/coverage.py` from the OpenAPI specs `alpaca-java`",
        "vendors. Do not edit by hand — run `just coverage`.",
        "",
        "Paths are compared with the version segment and parameter names removed,",
        "so a ✅ means the route is called, not that it is called at the right",
        "version. The event streams are the known case where that distinction bit:",
        "see ROADMAP.md.",
        "",
    ]

    totals = []
    for surface in surfaces:
        operations = spec_operations(args.specs / f"{surface}.yaml")
        covered, gaps = [], []
        for method, path in sorted(operations, key=lambda o: (o[1], o[0])):
            key = (method, normalize(path))
            if key in implemented:
                matched.add(key)
                covered.append((method, path))
            else:
                gaps.append((method, path))

        totals.append((surface, len(covered), len(operations)))
        pct = 100 * len(covered) // len(operations) if operations else 0
        lines += [
            f"## {surface} — {len(covered)}/{len(operations)} ({pct}%)",
            "",
            "### Not implemented",
            "",
        ]
        if gaps:
            group: dict[str, list[str]] = defaultdict(list)
            for method, path in gaps:
                # Group by the first meaningful segment, so related gaps sit together.
                stripped = VERSION.sub("", path)
                group[stripped.split("/")[1] if "/" in stripped[1:] else stripped].append(
                    f"`{method.upper():6}` `{path}`"
                )
            for head in sorted(group):
                lines.append(f"**{head}**")
                lines += [f"- {row}" for row in group[head]]
                lines.append("")
        else:
            lines += ["Nothing.", ""]

    lines += ["## Called by the crate but not in any spec", ""]
    unmatched = sorted(set(implemented) - matched)
    if unmatched:
        lines += [
            "Each of these is one of: a route the specs have not caught up with, a",
            "path built somewhere this script cannot see, or a mistake. Worth",
            "reading every time it changes.",
            "",
        ]
        for method, path in unmatched:
            lines.append(f"- `{method.upper():6}` `{path}` — {', '.join(sorted(set(routes[(method, path)])))}")
        lines.append("")
    else:
        lines += ["Nothing.", ""]

    args.out.write_text("\n".join(lines))
    for surface, covered, total in totals:
        print(f"{surface:9} {covered:3}/{total:<3} implemented")
    print(f"{'unmatched':9} {len(unmatched):3}     crate routes not found in any spec")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
