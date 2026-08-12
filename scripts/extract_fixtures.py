#!/usr/bin/env python3
"""Extract alpaca-py's mocked HTTP payloads into JSON fixtures.

alpaca-py's tests stub `requests_mock` with inline response bodies that are real
captured API responses. They are the highest-value asset in that repo for a port:
asserting the Rust models deserialize *these* is what makes "every feature
preserved" a verified claim rather than an asserted one.

Each `reqmock.<method>(url, text="...")` call becomes a fixture file plus an
index entry recording the method, URL, and originating test.

Usage:
    python3 scripts/extract_fixtures.py /path/to/alpaca-py
"""

from __future__ import annotations

import ast
import json
import re
import subprocess
import sys
from pathlib import Path

HTTP_METHODS = {"get", "post", "put", "patch", "delete", "head", "options"}

# alpaca/common/enums.py BaseURL, so f-string URLs render to real endpoints.
BASE_URLS = {
    "BROKER_SANDBOX": "https://broker-api.sandbox.alpaca.markets",
    "BROKER_PRODUCTION": "https://broker-api.alpaca.markets",
    "TRADING_PAPER": "https://paper-api.alpaca.markets",
    "TRADING_LIVE": "https://api.alpaca.markets",
    "DATA": "https://data.alpaca.markets",
    "DATA_SANDBOX": "https://data.sandbox.alpaca.markets",
    "MARKET_DATA_STREAM": "wss://stream.data.alpaca.markets",
    "OPTION_DATA_STREAM": "wss://stream.data.alpaca.markets",
    "TRADING_STREAM_PAPER": "wss://paper-api.alpaca.markets/stream",
    "TRADING_STREAM_LIVE": "wss://api.alpaca.markets/stream",
}

# tests/<area>/... -> fixtures/<area>/
AREAS = {
    "trading": "trading",
    "broker": "broker",
    "data": "data",
    "common": "common",
}


def render_url(node: ast.expr) -> str:
    """Best-effort rendering of a URL expression to a concrete string.

    Interpolations that cannot be resolved statically (an account id bound to a
    local) are left as `{name}` placeholders, which is enough for a route to be
    recognizable in the index.
    """
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value

    if isinstance(node, ast.JoinedStr):
        return "".join(render_url(part) for part in node.values)

    if isinstance(node, ast.FormattedValue):
        return render_url(node.value)

    # BaseURL.TRADING_PAPER.value -> the endpoint; anything else -> {expr}
    if isinstance(node, ast.Attribute):
        if node.attr == "value":
            return render_url(node.value)
        if (
            isinstance(node.value, ast.Name)
            and node.value.id == "BaseURL"
            and node.attr in BASE_URLS
        ):
            return BASE_URLS[node.attr]
        return "{" + node.attr + "}"

    if isinstance(node, ast.Name):
        return "{" + node.id + "}"

    if isinstance(node, ast.Call):
        return "{call}"

    return "{expr}"


def path_of(url: str) -> str:
    """The path portion, with the scheme and host stripped."""
    match = re.match(r"^[a-z]+://[^/]+(/.*)$", url)
    return match.group(1) if match else url


def area_for(rel_path: Path) -> str:
    parts = rel_path.parts
    if len(parts) > 1 and parts[1] in AREAS:
        return AREAS[parts[1]]
    return "other"


def slugify(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")


def extract(source_root: Path, out_root: Path) -> tuple[int, int, list[str]]:
    written = 0
    skipped: list[str] = []
    index: list[dict[str, str]] = []

    for py_file in sorted((source_root / "tests").rglob("test_*.py")):
        rel = py_file.relative_to(source_root)
        tree = ast.parse(py_file.read_text())

        # Map each call node to the test function enclosing it.
        enclosing: dict[int, str] = {}
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                for child in ast.walk(node):
                    enclosing.setdefault(id(child), node.name)

        seen_names: dict[str, int] = {}

        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            func = node.func
            if not (
                isinstance(func, ast.Attribute)
                and isinstance(func.value, ast.Name)
                and func.value.id == "reqmock"
                and func.attr in HTTP_METHODS
            ):
                continue

            body = next(
                (
                    kw.value.value
                    for kw in node.keywords
                    if kw.arg == "text"
                    and isinstance(kw.value, ast.Constant)
                    and isinstance(kw.value.value, str)
                ),
                None,
            )
            if body is None:
                continue

            test_name = enclosing.get(id(node), "module")
            try:
                payload = json.loads(body)
            except json.JSONDecodeError:
                # A couple of routes return a bare string rather than JSON.
                skipped.append(f"{rel}::{test_name} (body is not JSON)")
                continue

            url = render_url(node.args[0]) if node.args else "{unknown}"
            area = area_for(rel)

            stem = f"{slugify(py_file.stem)}__{slugify(test_name)}"
            seen_names[stem] = seen_names.get(stem, 0) + 1
            suffix = seen_names[stem]
            name = f"{stem}__{suffix:02d}.json"

            out_path = out_root / area / name
            out_path.parent.mkdir(parents=True, exist_ok=True)
            out_path.write_text(json.dumps(payload, indent=2) + "\n")
            written += 1

            index.append(
                {
                    "fixture": f"{area}/{name}",
                    "method": func.attr.upper(),
                    "url": url,
                    "path": path_of(url),
                    "source": f"{rel}::{test_name}",
                }
            )

    index.sort(key=lambda entry: entry["fixture"])
    (out_root / "index.json").write_text(json.dumps(index, indent=2) + "\n")

    return written, len(index), skipped


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2

    source_root = Path(sys.argv[1]).resolve()
    out_root = Path(__file__).resolve().parent.parent / "fixtures"

    revision = subprocess.run(
        ["git", "-C", str(source_root), "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()

    written, indexed, skipped = extract(source_root, out_root)

    (out_root / "SOURCE").write_text(
        f"Captured API responses extracted from alpaca-py @ {revision}\n"
        f"by scripts/extract_fixtures.py. Do not edit by hand.\n"
    )

    print(f"{written} fixtures written, {indexed} indexed, from alpaca-py @ {revision}")
    for entry in skipped:
        print(f"  skipped: {entry}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
