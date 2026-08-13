#!/usr/bin/env python3
"""Extract response payloads from alpaca-trade-api-go's test suite.

Why only Go, out of the four other SDKs: its tests embed responses as raw JSON
strings pasted whole into backtick literals, so the wire's quirks survive —
numbers as strings, nulls, empty strings, the odd misspelled field. The C# and
TypeScript suites build their payloads *through the SDK's own types*
(`JObject`, `JSON.stringify(obj)`), which normalizes away precisely the quirks a
fixture exists to catch. A payload that has been through a model is evidence
about the model, not about the API.

Each `func Test*` gets treated as one unit: the JSON literals in its body belong
to the route asserted in the same body, and a test that switches on a page token
yields a numbered sequence rather than one payload.

Usage:
    python3 scripts/harvest_go_fixtures.py <path-to-alpaca-trade-api-go> [--out fixtures/go]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys

# `func TestGetAuctions(t *testing.T) {`
TEST_FN = re.compile(r"^func (Test\w+)\(", re.M)
# Backtick literals. Go has no escaping inside them, so this is exact.
BACKTICK = re.compile(r"`([^`]*)`", re.S)
# `assert.Equal(t, "/v2/stocks/auctions", req.URL.Path)`
ROUTE = re.compile(r'assert\.Equal\(\s*t,\s*"(/[^"]*)"\s*,\s*req\.URL\.Path\s*\)')
# `assert.Equal(t, "AAPL", req.URL.Query().Get("symbols"))`
QUERY = re.compile(
    r'assert\.Equal\(\s*t,\s*"([^"]*)"\s*,\s*req\.URL\.Query\(\)\.Get\("([^"]+)"\)\s*\)'
)
# Many tests use `c.do = mockResp(`{...}`)` and assert nothing about the URL,
# leaving the SDK method as the only thing that says which route the payload
# belongs to: `got, err := c.GetLatestCryptoPerpBar(...)`. Less precise than a
# path, and the reason the index distinguishes the two.
CALL = re.compile(r"\bc\.([A-Z]\w+)\(")


def test_blocks(source: str) -> list[tuple[str, str]]:
    """Split a file into (test name, body) at `func Test*` boundaries."""
    starts = [(m.group(1), m.start()) for m in TEST_FN.finditer(source)]
    blocks = []
    for i, (name, start) in enumerate(starts):
        end = starts[i + 1][1] if i + 1 < len(starts) else len(source)
        blocks.append((name, source[start:end]))
    return blocks


def payloads(body: str) -> list[str]:
    """Backtick literals in `body` that are JSON objects or arrays.

    Go tests use backticks for other things too — regexes, SQL, prose in
    assertion messages — so the parse is the filter.
    """
    found = []
    for match in BACKTICK.finditer(body):
        text = match.group(1).strip()
        if not text or text[0] not in "{[":
            continue
        try:
            json.loads(text)
        except ValueError:
            continue
        found.append(text)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo", type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("fixtures/go"))
    args = parser.parse_args()

    if not args.repo.is_dir():
        print(f"not a directory: {args.repo}", file=sys.stderr)
        return 1

    revision = subprocess.run(
        ["git", "-C", str(args.repo), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()

    args.out.mkdir(parents=True, exist_ok=True)
    for stale in args.out.glob("*.json"):
        stale.unlink()

    index: dict[str, dict] = {}
    written = skipped_no_route = 0

    for go in sorted(args.repo.rglob("*_test.go")):
        package = go.parent.name
        source = go.read_text()

        for name, body in test_blocks(source):
            found = payloads(body)
            if not found:
                continue

            routes = ROUTE.findall(body)
            calls = sorted(set(CALL.findall(body)))
            if not routes and not calls:
                # Nothing says where this payload came from, and a fixture
                # nobody can place is a fixture nobody will use.
                skipped_no_route += 1
                continue

            query = {key: value for value, key in QUERY.findall(body)}
            # Split before a capital that follows a lowercase, and before the
            # last capital of a run — so GetUSCorporates becomes
            # get_us_corporates rather than get_u_s_corporates.
            snake = re.sub(
                r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", "_", name
            ).lower()

            for n, payload in enumerate(found, start=1):
                stem = f"{package}__{snake}__{n:02}"
                (args.out / f"{stem}.json").write_text(
                    json.dumps(json.loads(payload), indent=2, sort_keys=True) + "\n"
                )
                index[f"{stem}.json"] = {
                    # `route` is asserted by the test. `sdk_method` is inferred
                    # from the call, for the tests that assert no path — good
                    # enough to place the payload, not proof of the route.
                    "route": routes[0] if routes else None,
                    "routes": sorted(set(routes)) if len(set(routes)) > 1 else None,
                    "sdk_method": None if routes else calls,
                    "query": query or None,
                    "source": f"{go.relative_to(args.repo)}:{name}",
                    "page": n if len(found) > 1 else None,
                }
                written += 1

    index_path = args.out / "index.json"
    index_path.write_text(
        json.dumps(
            {
                "source": {
                    "repository": "https://github.com/alpacahq/alpaca-trade-api-go",
                    "revision": revision,
                    "license": "Apache-2.0",
                },
                "note": (
                    "Response payloads lifted from the Go SDK's tests, where they "
                    "are raw JSON pasted into backtick literals. Routes come from "
                    "the req.URL.Path assertion in the same test function."
                ),
                "fixtures": {
                    k: {kk: vv for kk, vv in v.items() if vv is not None}
                    for k, v in sorted(index.items())
                },
            },
            indent=2,
        )
        + "\n"
    )

    routes = sorted({v["route"] for v in index.values() if v["route"]})
    by_method = sorted(
        {m for v in index.values() if v["sdk_method"] for m in v["sdk_method"]}
    )
    print(f"wrote {written} payloads to {args.out}")
    print(f"  {len(routes)} routes asserted by a test")
    print(f"  {len(by_method)} more placed by SDK method only")
    print(f"skipped {skipped_no_route} blocks with nothing to place them by")
    for route in routes:
        print(f"  route  {route}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
