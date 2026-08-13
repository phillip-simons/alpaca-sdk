#!/usr/bin/env python3
"""Build a machine-readable index of Alpaca's published API reference.

The reference at <https://docs.alpaca.markets/us/reference/> is a JavaScript
application, but every page has a `.md` twin at the same slug, and that twin
embeds a one-operation OpenAPI document: the versioned path, the operationId,
which API it belongs to, and — the reason this script exists — whether Alpaca
has flagged the route deprecated, legacy, or given it a sunset date.

That is the one thing the vendored specs cannot tell you. `/v1/events/trades`
was in the spec, looked healthy from the crate's side, and had been switched
off; the reference said so. See ROADMAP.md.

Usage:
    python3 scripts/reference.py [--out specs/reference.json] [--cache specs/reference]
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import re
import subprocess
import sys

INDEX = "https://docs.alpaca.markets/us/llms.txt"
METHODS = {"get", "post", "put", "patch", "delete", "head", "options"}
SLUG = re.compile(r"https://docs\.alpaca\.markets/us/reference/[a-z0-9_.-]+\.md")

# The docs CDN answers 403 to urllib's default user agent, so everything goes
# through curl, which it does answer.
def fetch(url: str, into: pathlib.Path | None = None) -> str | None:
    if into is not None and into.exists() and into.stat().st_size > 512:
        return into.read_text(errors="replace")
    argv = ["curl", "-sfL", "--max-time", "180", url]
    if into is not None:
        argv += ["-o", str(into)]
    result = subprocess.run(argv, capture_output=into is None, text=True)
    if result.returncode != 0:
        return None
    return into.read_text(errors="replace") if into else result.stdout


def parameters(declared: list) -> list[dict]:
    """The parameters of one operation, deduplicated and sorted.

    `$ref`s are dropped rather than resolved: these are one-operation documents
    and a reference into `components` is rare, so following them would be more
    machinery than it earns. A dropped parameter is invisible to the check
    rather than reported wrongly.
    """
    seen: dict[tuple[str, str], dict] = {}
    for parameter in declared:
        if not isinstance(parameter, dict) or "$ref" in parameter:
            continue
        name = parameter.get("name")
        where = parameter.get("in")
        if not name or not where:
            continue
        seen[(where, name)] = {
            "name": name,
            "in": where,
            "required": bool(parameter.get("required")),
        }
    return [seen[key] for key in sorted(seen)]


def parse(slug: str, text: str) -> list[dict]:
    """Every operation a reference page documents."""
    marker = text.find("# OpenAPI definition")
    head = text[:marker] if marker >= 0 else text
    title = ""
    if m := re.search(r"^# (.+)$", head, re.M):
        title = m.group(1).strip()
    if marker < 0:
        return []
    start = text.find("```json", marker)
    if start < 0:
        return []
    end = text.find("\n```", start)
    try:
        doc = json.loads(text[start + len("```json") : end])
    except json.JSONDecodeError:
        return []

    api = doc.get("info", {}).get("title", "")
    rows = []
    for path, item in doc.get("paths", {}).items():
        # Parameters may be declared once for the path and inherited by every
        # operation on it, or per operation, or both. The union is what a caller
        # may send.
        shared = item.get("parameters", []) if isinstance(item, dict) else []
        for method, operation in item.items():
            if method.lower() not in METHODS:
                continue
            rows.append(
                {
                    "slug": slug,
                    "title": title,
                    "api": api,
                    "method": method.lower(),
                    "path": path,
                    "operation_id": operation.get("operationId", ""),
                    # Every parameter the operation accepts, which is what
                    # `scripts/parameters.py` diffs against the crate. Route
                    # coverage and parameter coverage are different questions,
                    # and hand-checking three routes for the second one found
                    # four missing parameters.
                    "parameters": parameters(shared + operation.get("parameters", [])),
                    # Three independent ways Alpaca marks a route as on its way
                    # out, and pages use different ones.
                    "deprecated": bool(operation.get("deprecated")),
                    "legacy": "legacy" in title.lower(),
                    "sunset": (operation.get("x-deprecation") or {}).get("sunset"),
                    "deprecation_reason": (operation.get("x-deprecation") or {}).get("reason"),
                }
            )
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("specs/reference.json"))
    parser.add_argument("--cache", type=pathlib.Path, default=pathlib.Path("specs/reference"))
    args = parser.parse_args()

    args.cache.mkdir(parents=True, exist_ok=True)
    index = fetch(INDEX)
    if index is None:
        print(f"could not fetch {INDEX}", file=sys.stderr)
        return 1
    urls = sorted(set(SLUG.findall(index)))
    print(f"{len(urls)} reference pages")

    def one(url: str) -> tuple[str, str | None]:
        name = url.rsplit("/", 1)[-1]
        return name, fetch(url, args.cache / name)

    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        pages = list(pool.map(one, urls))

    failed = [name for name, text in pages if text is None]
    rows: list[dict] = []
    for name, text in pages:
        if text is not None:
            rows.extend(parse(name.removesuffix(".md"), text))

    args.out.write_text(json.dumps(rows, indent=1, sort_keys=True) + "\n")
    flagged = [r for r in rows if r["deprecated"] or r["legacy"] or r["sunset"]]
    print(f"{len(rows)} operations, {len(flagged)} flagged deprecated/legacy")
    for row in sorted(flagged, key=lambda r: (r["api"], r["path"])):
        sunset = f", sunset {row['sunset']}" if row["sunset"] else ""
        print(f"  {row['method'].upper():6} {row['path']}{sunset}")
    if failed:
        print(f"{len(failed)} pages failed: {', '.join(failed)}", file=sys.stderr)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
