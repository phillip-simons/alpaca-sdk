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

Routes the crate deliberately does not implement live in `SKIP` below, with the
reason. Without that list the report can never converge: a route we have decided
against looks exactly like one nobody has got to yet, and "not implemented"
never reaches zero. A gap with an entry in `SKIP` is a decision; a gap without
one is work.

If `specs/reference.json` is present — `just reference` writes it — every route
is annotated with what Alpaca's published reference says about it. The specs
list what exists; only the reference says what is still current.

Usage:
    python3 scripts/coverage.py <specs-dir> [--out COVERAGE.md]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
from collections import defaultdict

METHODS = {"get", "post", "put", "patch", "delete", "head", "options"}

# Spec routes this crate will not implement, and why. Keyed by (method, path)
# exactly as the spec writes it, so a spec revision that moves a path makes the
# entry stop matching and the route reappear as a gap — which is the correct
# failure: the decision was made about a route at a path.
SKIP: dict[tuple[str, str], str] = {
    ("get", "/v1/events/transfers/status"): (
        "Legacy, and closed to new broker partners. The crate calls "
        "`/v2/events/funding/status`, which covers banks and wallets too."
    ),
    ("post", "/v2/wallets/transfers"): (
        "Deprecated 2026-07-09, sunset 2026-10-09, and the reference's own "
        "replacement is the Alpaca web application rather than another route. "
        "The read side of crypto funding is implemented; only the withdrawal "
        "is skipped. The broker equivalent "
        "(`POST /v1/accounts/{account_id}/wallets/transfers`) is not "
        "deprecated and is implemented."
    ),
}

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


def spec_operations(spec: pathlib.Path) -> tuple[list[tuple[str, str]], set[tuple[str, str]]]:
    """Every (method, path) in a document's `paths`, and which are deprecated.

    The deprecation set is the reason this reads the spec rather than only
    listing routes: a route that still exists but is on its way out looks
    exactly like a healthy one from the crate's side.
    """
    operations: list[tuple[str, str]] = []
    deprecated: set[tuple[str, str]] = set()
    in_paths = False
    current: str | None = None
    method: str | None = None

    for line in spec.read_text().splitlines():
        if re.match(r"^paths:\s*$", line):
            in_paths = True
            continue
        if in_paths and re.match(r"^\S", line):
            break  # the next top-level key
        if not in_paths:
            continue

        path_key = re.match(r"^  (/\S*):\s*$", line)
        if path_key:
            current, method = path_key.group(1), None
            continue

        method_key = re.match(r"^    (\w+):\s*$", line)
        if method_key and current and method_key.group(1).lower() in METHODS:
            method = method_key.group(1).lower()
            operations.append((method, current))
            continue

        # `deprecated: true` at operation depth. Deeper ones belong to
        # parameters and are a different question.
        if re.match(r"^      deprecated:\s*true\s*$", line) and current and method:
            deprecated.add((method, current))

    return operations, deprecated


# A route is written one of two ways: with the path inline, or bound to a local
# named `path` first, because it interpolates an id or is reused by two calls.
#
# The binding form is why this scans in source order rather than running each
# pattern over the whole file. `let path = format!("/orders/{id}")` followed by
# `self.rest.patch(&path, ..)` is a PATCH, and reading the binding on its own
# reports it as a GET — which is exactly what this script used to do, hiding
# four implemented routes behind a phantom GET of the same path.
BINDING = re.compile(r"let path = (?:format!\s*\(\s*)?\"([^\"]+)\"")
# `self.rest.get("/x", ..)`, `self.rest.post(&format!("/x/{y}"), ..)`,
# `self.rest.patch(&path, ..)`, and the same split across lines by rustfmt.
#
# `.at_version("v1")` may sit between the two: Alpaca versions routes
# individually, so a client's own version is not always the route's.
REST_CALL = re.compile(
    r"\.rest\s*\.\s*(?:at_version\s*\(\s*\"[^\"]+\"\s*\)\s*\.\s*)?"
    r"(get|post|put|patch|delete)\s*\(\s*"
    r"(?:&?\s*(?:format!\s*\(\s*)?\"(?P<literal>[^\"]+)\"|&?(?P<binding>path)\b)",
    re.S,
)
# `send_void(Method::DELETE, &format!("/x"), ..)` and
# `self.rest.request(Method::PUT, "/x", ..)`, which take the method as a value
# rather than as the name of the call.
VOID_CALL = re.compile(
    r"(?:send_void|\.rest\s*\.\s*request)\s*\(\s*Method::(GET|POST|PUT|PATCH|DELETE)\s*,\s*"
    r"(?:&?\s*(?:format!\s*\(\s*)?\"(?P<literal>[^\"]+)\"|&?(?P<binding>path)\b)",
    re.S,
)
# `self.events(EventVersion::V2, "/events/trades", ..)` and its timestamp-
# windowed sibling `self.event_stream(EventVersion::V2Beta1, "/events/…", ..)`.
EVENT_CALL = re.compile(
    r"\.event(?:s|_stream)\s*\(\s*EventVersion::\w+\s*,\s*\"(?P<literal>[^\"]+)\"",
    re.S,
)
# Market data goes through the pagination helper rather than calling the
# transport directly. All are GETs.
MARKET_DATA = re.compile(
    r"MarketDataRequest::(?:paged|paged_with_limit|latest)\s*\(\s*"
    r"(?:\"(?P<literal>[^\"]+)\"|&(?P<binding>path)\b)",
    re.S,
)
# The routes that do not go through `RestClient` at all: the logo bytes, the
# document download, and the streams whose version segment is not the client's.
# Each reads the `path` binding rather than a literal, and each is a GET.
RAW_GET = re.compile(
    r"(?P<binding>get_bytes\s*\(\s*&path\b"
    r"|crate::sse::subscribe\s*\("
    r"|self\.raw\s*\.\s*get\s*\()"
)

# (pattern, fixed method) — None means the match's first group carries it.
CALL_PATTERNS = [
    (REST_CALL, None),
    (VOID_CALL, None),
    (EVENT_CALL, "get"),
    (MARKET_DATA, "get"),
    (RAW_GET, "get"),
]

# A route Alpaca has flagged should reach a caller as a compiler warning, not
# only as a row in this file. These find the methods that carry one.
DEPRECATED_ATTR = re.compile(r"#\[deprecated\b")
FN_DECL = re.compile(r"\bpub\s+(?:async\s+)?fn\s")


def deprecated_fn_starts(text: str) -> list[int]:
    """The offset of every `pub fn` that a `#[deprecated]` attribute precedes.

    The attribute always sits immediately above its item, so the first function
    declaration after it is the one it applies to.
    """
    fns = [m.start() for m in FN_DECL.finditer(text)]
    marked = []
    for attr in DEPRECATED_ATTR.finditer(text):
        following = [start for start in fns if start > attr.start()]
        if following:
            marked.append(following[0])
    return marked


def crate_routes(
    src: pathlib.Path,
) -> tuple[dict[tuple[str, str], list[str]], set[tuple[str, str]]]:
    """Every route the crate calls, and which are called from deprecated methods.

    Returns `({(method, normalized path): [sources]}, {deprecated routes})`. The
    second is what lets this file check that a route Alpaca has flagged actually
    warns at the call site.
    """
    routes: dict[tuple[str, str], list[str]] = defaultdict(list)
    from_deprecated: set[tuple[str, str]] = set()

    for rs in sorted(src.rglob("*.rs")):
        text = rs.read_text()
        where = str(rs.relative_to(src.parent))
        fn_starts = [m.start() for m in FN_DECL.finditer(text)]
        marked_fns = set(deprecated_fn_starts(text))

        def enclosing_fn_is_deprecated(offset: int) -> bool:
            before = [start for start in fn_starts if start < offset]
            return bool(before) and before[-1] in marked_fns

        # Everything of interest, in source order, so a `path` binding is known
        # by the time the call that uses it is read.
        events: list[tuple[int, str, re.Match[str]]] = [
            (m.start(), "binding", m) for m in BINDING.finditer(text)
        ]
        for index, (pattern, _) in enumerate(CALL_PATTERNS):
            events += [(m.start(), f"call{index}", m) for m in pattern.finditer(text)]
        events.sort(key=lambda item: item[0])

        bound: str | None = None
        bindings: list[str] = []
        seen_here: set[str] = set()
        for offset, kind, match in events:
            if kind == "binding":
                bound = match.group(1)
                bindings.append(bound)
                continue

            _, fixed_method = CALL_PATTERNS[int(kind.removeprefix("call"))]
            groups = match.groupdict()
            if groups.get("literal"):
                path = groups["literal"]
            elif groups.get("binding"):
                if bound is None:
                    continue  # a `path` that came from somewhere this cannot see
                path = bound
            else:
                continue
            method = fixed_method or match.group(1).lower()
            key = (method, normalize(path))
            routes[key].append(where)
            seen_here.add(normalize(path))
            if enclosing_fn_is_deprecated(offset):
                from_deprecated.add(key)

        # A `path` binding no call above accounted for was handed to a private
        # helper — `latest_for_symbol`, say — which this cannot follow. Every
        # such helper in this crate issues a GET, and leaving the path out
        # entirely would report an implemented route as a gap. Recording it is
        # the safer of the two errors, and a wrong one shows up under "called by
        # the crate but not in any spec".
        for path in bindings:
            if normalize(path) not in seen_here:
                routes[("get", normalize(path))].append(where)

    return routes, from_deprecated


def reference_index(path: pathlib.Path) -> dict[tuple[str, str], list[dict]]:
    """What the published reference says, keyed the same way the specs are.

    Absent when `just reference` has not been run; the report degrades to the
    specs alone rather than failing.
    """
    if not path.is_file():
        return {}
    index: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for row in json.loads(path.read_text()):
        index[(row["method"], normalize(row["path"]))].append(row)
    return index


def flagged(reference: dict[tuple[str, str], list[dict]], key: tuple[str, str]) -> str:
    """A short note if the reference has flagged any page for this route."""
    notes = []
    for row in reference.get(key, []):
        if row["sunset"]:
            notes.append(f"sunset {row['sunset']}")
        elif row["deprecated"]:
            notes.append("deprecated")
        elif row["legacy"]:
            notes.append("legacy")
    return f" — reference: {', '.join(sorted(set(notes)))}" if notes else ""


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

    routes, from_deprecated = crate_routes(args.src)
    implemented = set(routes)
    matched: set[tuple[str, str]] = set()
    reference = reference_index(args.specs / "reference.json")

    lines = [
        "# Route coverage",
        "",
        "Generated by `scripts/coverage.py` from the OpenAPI specs `alpaca-java`",
        "vendors. Do not edit by hand — run `just coverage`.",
        "",
        "Paths are compared with the version segment and parameter names removed,",
        "so a ✅ means the route is called, not that it is called at the right",
        "version. The event streams are the known case where that distinction",
        "bit: three of them pointed at routes Alpaca had retired, and every model",
        "behind them was correct.",
        "",
        "**Not implemented** is work outstanding. **Deliberately skipped** is a",
        "decision, recorded with its reason in `SKIP` in the script — a route we",
        "have chosen against must not keep reading as a gap.",
        "",
    ]
    if not reference:
        lines += [
            "> `specs/reference.json` is absent, so nothing here is annotated with",
            "> what Alpaca's published reference says. Run `just reference`.",
            "",
        ]

    totals = []
    rotting: list[tuple[str, str]] = []
    all_skipped: list[tuple[str, str, str]] = []
    for surface in surfaces:
        operations, deprecated = spec_operations(args.specs / f"{surface}.yaml")
        covered, gaps, skipped = [], [], []
        for method, path in sorted(operations, key=lambda o: (o[1], o[0])):
            key = (method, normalize(path))
            if key in implemented:
                matched.add(key)
                covered.append((method, path))
                if (method, path) in deprecated:
                    rotting.append((method, path))
            elif (method, path) in SKIP:
                skipped.append((method, path))
                all_skipped.append((surface, method, path))
            else:
                gaps.append((method, path))

        totals.append((surface, len(covered), len(operations), len(skipped)))
        pct = 100 * len(covered) // len(operations) if operations else 0
        heading = f"## {surface} — {len(covered)}/{len(operations)} ({pct}%)"
        if skipped:
            heading += f", {len(skipped)} deliberately skipped"
        lines += [heading, "", "### Not implemented", ""]
        if gaps:
            group: dict[str, list[str]] = defaultdict(list)
            for method, path in gaps:
                # Group by the first meaningful segment, so related gaps sit together.
                stripped = VERSION.sub("", path)
                group[stripped.split("/")[1] if "/" in stripped[1:] else stripped].append(
                    f"`{method.upper():6}` `{path}`{flagged(reference, (method, normalize(path)))}"
                )
            for head in sorted(group):
                lines.append(f"**{head}**")
                lines += [f"- {row}" for row in group[head]]
                lines.append("")
        else:
            lines += ["Nothing.", ""]

    lines += ["## Deliberately skipped", ""]
    if all_skipped:
        lines += [
            "Routes the spec documents that this crate will not call. Each reason",
            "lives in `SKIP` in `scripts/coverage.py`, so the decision is in the",
            "same place as the check.",
            "",
        ]
        for surface, method, path in all_skipped:
            lines += [f"- `{method.upper():6}` `{path}` ({surface})", f"  - {SKIP[(method, path)]}"]
        lines.append("")
    else:
        lines += ["Nothing.", ""]

    lines += ["## Implemented, and marked deprecated by the spec", ""]
    unwarned: list[tuple[str, str]] = []
    if rotting:
        lines += [
            "Routes this crate calls that Alpaca has flagged. Deprecated is not",
            "gone — but `/v1/events/trades` was flagged before it was switched off,",
            "so each of these wants a replacement found before it is needed.",
            "",
            "**⚠️ warns** means the method carrying the route is `#[deprecated]`, so a",
            "caller finds out from the compiler rather than from this file. A route",
            "Alpaca has flagged and the crate has not is the row to act on.",
            "",
        ]
        for method, path in rotting:
            key = (method, normalize(path))
            warns = key in from_deprecated
            if not warns:
                unwarned.append((method, path))
            mark = "⚠️ warns" if warns else "**no `#[deprecated]` on the method**"
            lines.append(f"- `{method.upper():6}` `{path}` — {mark}{flagged(reference, key)}")
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
    for surface, covered, total, skipped in totals:
        note = f"  ({skipped} skipped)" if skipped else ""
        print(f"{surface:9} {covered:3}/{total:<3} implemented{note}")
    print(f"{'deprecated':9} {len(rotting):3}     implemented routes the spec flags")
    if unwarned:
        # Loud, because the whole point of the attribute is that a caller does
        # not have to read this file to learn a route is going away.
        print(
            f"{'':9} {len(unwarned):3}     of those carry no `#[deprecated]`: "
            + ", ".join(f"{m.upper()} {p}" for m, p in unwarned),
            file=sys.stderr,
        )
    print(f"{'unmatched':9} {len(unmatched):3}     crate routes not found in any spec")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
