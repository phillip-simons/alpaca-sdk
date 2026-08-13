#!/usr/bin/env python3
"""Generate `wire_enum!` blocks from alpaca-py's `enums.py` modules.

The 74 string enums are mechanical to port and easy to get subtly wrong by hand,
so they are generated. Docstrings — including the per-member `Attributes:` block
alpaca-py uses — are carried across as rustdoc.

Generated files are overwritten on every run. Hand-written `impl` blocks belong
in the sibling `enums_ext.rs`, which this script never touches.

These enums come from alpaca-py, which is not the API. `scripts/enum_drift.py`
diffs the generated files against the same-named schemas in `specs/` and reports
where the two disagree; it is a separate script rather than a step here because
this one needs an alpaca-py checkout and that one does not, and a check that can
only run during a regeneration is a check that runs once a year.

KNOWN GAP: nothing currently proves the checked-in `enums.rs` files still match
what this script produces — a hand edit to a generated file would survive, since
`tests/enum_parity.rs` is generated from the same parse. Closing it needs an
alpaca-py checkout in CI (pinned to the revision in the file headers) plus a
`git diff --exit-code` after a regeneration run.

Usage:
    python3 scripts/gen_enums.py /path/to/alpaca-py
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

# Python module -> (Rust output path, module doc)
MODULES = {
    "alpaca/trading/enums.py": (
        "src/trading/enums.rs",
        "Enums for the trading API, from `alpaca/trading/enums.py`.",
    ),
    "alpaca/data/enums.py": (
        "src/data/enums.rs",
        "Enums for the market data API, from `alpaca/data/enums.py`.",
    ),
    "alpaca/broker/enums.py": (
        "src/broker/enums.rs",
        "Enums for the broker API, from `alpaca/broker/enums.py`.",
    ),
}

# Ported by hand elsewhere: BaseURL and Sort/SupportedCurrencies live in
# config.rs and types/common_enums.rs, and PaginationType is replaced by Stream.
SKIP_ENUMS = {"BaseURL", "PaginationType", "Sort", "SupportedCurrencies"}

# Enums that more than one feature-gated module needs. They live in `types` so
# a module can use one without depending on the feature that happens to own it
# upstream -- `data` needs ContractType, but must build with `trading` off.
# Each is re-exported from its original module so the public path is unchanged.
SHARED_ENUMS = {"ContractType"}
SHARED_OUTPUT = (
    "src/types/shared_enums.rs",
    "Enums shared by more than one API surface.",
)

ATTR_LINE = re.compile(r"^\s*(\w+)\s*\((?:str|int)\)\s*:\s*(.*)$")
BARE_URL = re.compile(r"(?<![<(\w])(https?://[^\s,)\]]+)")

# PascalCase names that collide with Rust keywords. `Self` cannot even be written
# as a raw identifier, so it needs a real name rather than an escape.
RENAMES = {
    ("ClearingBroker", "Self"): "SelfClearing",
}

# Reserved words a PascalCase conversion can produce. Anything landing here
# without a RENAMES entry is a hard error, never a silently broken variant.
RESERVED = {"Self", "Super", "Crate"}


@dataclass
class Variant:
    name: str
    wire: str
    docs: list[str] = field(default_factory=list)


@dataclass
class WireEnum:
    name: str
    docs: list[str]
    variants: list[Variant]
    has_methods: bool


def pascal_case(name: str) -> str:
    """USA_SSN -> UsaSsn, GTC -> Gtc, Z -> Z."""
    parts = [p for p in name.split("_") if p]
    return "".join(p[:1].upper() + p[1:].lower() for p in parts)


# Spans that must be passed through untouched: existing code spans and the
# angle-bracketed URLs produced above.
PROTECTED = re.compile(r"(`[^`]*`|<[^>]*>)")
WORD = re.compile(r"[A-Za-z][A-Za-z0-9_]*")


def _needs_backticks(word: str) -> bool:
    """Mirror clippy's `doc_markdown` heuristic for identifier-looking words."""
    if "_" in word:
        return True
    # An uppercase letter anywhere but the front, alongside a lowercase one:
    # `CIPInfo`, `TradeDocument`, `dtbpCheck`.
    return any(c.isupper() for c in word[1:]) and any(c.islower() for c in word)


def _backtick_identifiers(text: str) -> str:
    return WORD.sub(
        lambda m: f"`{m.group(0)}`" if _needs_backticks(m.group(0)) else m.group(0),
        text,
    )


def clean_doc_line(line: str) -> str:
    """Make a Python docstring line safe and useful as rustdoc."""
    line = line.rstrip()
    # rustdoc warns on bare URLs and treats [x] as an intra-doc link.
    line = BARE_URL.sub(r"<\1>", line)
    line = line.replace("[", "\\[").replace("]", "\\]")

    # Wrap bare identifiers so `cargo clippy -D warnings` stays clean, leaving
    # existing code spans and URLs alone.
    return "".join(
        part if PROTECTED.fullmatch(part) else _backtick_identifiers(part)
        for part in PROTECTED.split(line)
    )


def split_docstring(raw: str | None) -> tuple[list[str], dict[str, list[str]]]:
    """Split a class docstring into the summary and the `Attributes:` entries."""
    if not raw:
        return [], {}

    lines = [line.rstrip() for line in raw.strip("\n").split("\n")]
    summary: list[str] = []
    attrs: dict[str, list[str]] = {}

    in_attrs = False
    current: str | None = None

    for line in lines:
        stripped = line.strip()
        if stripped in {"Attributes:", "Args:"}:
            in_attrs = True
            current = None
            continue

        if not in_attrs:
            summary.append(clean_doc_line(stripped))
            continue

        match = ATTR_LINE.match(line)
        if match:
            current = match.group(1)
            attrs[current] = [clean_doc_line(match.group(2).strip())]
        elif stripped and current:
            # A wrapped continuation of the previous attribute description.
            attrs[current].append(clean_doc_line(stripped))
        elif not stripped:
            current = None

    while summary and not summary[-1]:
        summary.pop()
    while summary and not summary[0]:
        summary.pop(0)

    return summary, attrs


def parse_module(path: Path) -> list[WireEnum]:
    tree = ast.parse(path.read_text())
    enums: list[WireEnum] = []

    for node in tree.body:
        if not isinstance(node, ast.ClassDef) or node.name in SKIP_ENUMS:
            continue
        if not any(
            isinstance(base, ast.Name) and base.id == "Enum" for base in node.bases
        ):
            continue

        summary, attrs = split_docstring(ast.get_docstring(node))
        has_methods = any(
            isinstance(b, (ast.FunctionDef, ast.AsyncFunctionDef)) for b in node.body
        )

        variants: list[Variant] = []
        for stmt in node.body:
            if not isinstance(stmt, ast.Assign) or len(stmt.targets) != 1:
                continue
            target = stmt.targets[0]
            if not isinstance(target, ast.Name):
                continue
            if not isinstance(stmt.value, ast.Constant) or not isinstance(
                stmt.value.value, str
            ):
                continue

            py_name = target.id
            wire = stmt.value.value
            # `DocumentType` carries a member whose wire value is the empty
            # string; `` renders as unbalanced backticks.
            fallback = f"`{wire}`" if wire else "The empty value."
            docs = attrs.get(py_name) or [fallback]
            rust_name = RENAMES.get(
                (node.name, py_name),
                pascal_case(py_name),
            )
            variants.append(
                Variant(name=rust_name, wire=stmt.value.value, docs=docs)
            )

        if variants:
            enums.append(
                WireEnum(
                    name=node.name,
                    docs=summary,
                    variants=variants,
                    has_methods=has_methods,
                )
            )

    return enums


def validate(enum: WireEnum) -> list[str]:
    """Catch the two ways a faithful-looking port silently loses information."""
    problems: list[str] = []

    for variant in enum.variants:
        if variant.name in RESERVED:
            problems.append(
                f"{enum.name}: variant {variant.name!r} (wire {variant.wire!r}) is a "
                f"Rust keyword; add a RENAMES entry"
            )
        if not variant.name[:1].isalpha():
            problems.append(
                f"{enum.name}: variant {variant.name!r} is not a valid Rust identifier"
            )

    seen_names: dict[str, str] = {}
    for variant in enum.variants:
        if variant.name in seen_names:
            problems.append(
                f"{enum.name}: variant name {variant.name!r} collides "
                f"(wire {seen_names[variant.name]!r} and {variant.wire!r})"
            )
        seen_names[variant.name] = variant.wire

    seen_wire: dict[str, str] = {}
    for variant in enum.variants:
        if variant.wire in seen_wire:
            # Python enums allow aliases; Rust match arms do not.
            problems.append(
                f"{enum.name}: duplicate wire value {variant.wire!r} on "
                f"{seen_wire[variant.wire]} and {variant.name}"
            )
        seen_wire[variant.wire] = variant.name

    return problems


def render(enum: WireEnum) -> str:
    out: list[str] = ["wire_enum! {"]

    docs = enum.docs or [f"The `{enum.name}` values accepted by the API."]
    for line in docs:
        out.append(f"    /// {line}".rstrip())
    out.append(f"    pub enum {enum.name} {{")

    for variant in enum.variants:
        for line in variant.docs:
            out.append(f"        /// {line}".rstrip())
        out.append(f'        {variant.name} => "{variant.wire}",')

    out.append("    }")
    out.append("}")
    return "\n".join(out)


def render_module(doc: str, source: str, revision: str, enums: list[WireEnum]) -> str:
    header = [
        f"//! {doc}",
        "//!",
        f"//! Generated by `scripts/gen_enums.py` from `{source}`",
        f"//! at alpaca-py revision `{revision}`. Do not edit by hand — hand-written",
        "//! `impl` blocks belong in the sibling `enums_ext.rs`.",
        "",
        "use crate::types::wire::wire_enum;",
        "",
    ]
    return "\n".join(header) + "\n\n".join(render(e) for e in enums) + "\n"


def render_parity_test(revision: str, by_module: dict[str, list[WireEnum]]) -> str:
    """Emit a test asserting every enum's wire values match the Python source.

    Generated from the same parse as the enums themselves, so it does not prove
    the generator is correct — it proves the checked-in Rust has not drifted from
    the alpaca-py revision it was generated at, which is what breaks silently.
    """
    out = [
        "//! Wire values for every generated enum, checked against alpaca-py.",
        "//!",
        f"//! Generated by `scripts/gen_enums.py` at alpaca-py revision `{revision}`.",
        "//! Regenerate after bumping the upstream revision; a diff here is a wire",
        "//! change that needs reviewing, not a test to silence.",
        "",
        "#![allow(clippy::too_many_lines)]",
        "",
    ]

    for module, enums in by_module.items():
        if not enums:
            continue
        names = ", ".join(sorted(e.name for e in enums))
        out.append(f"use alpaca_sdk::{module}::{{{names}}};")
    out.append("")

    out.append("/// Asserts the known wire values and that each one round-trips.")
    out.append("macro_rules! assert_wire_values {")
    out.append("    ($ty:ty, $values:expr) => {{")
    out.append("        assert_eq!(<$ty>::WIRE_VALUES, $values, stringify!($ty));")
    out.append("        for value in <$ty>::WIRE_VALUES {")
    out.append("            let parsed = <$ty>::from(*value);")
    out.append("            assert!(!parsed.is_unknown(), \"{}: {value}\", stringify!($ty));")
    out.append("            assert_eq!(parsed.as_str(), *value, stringify!($ty));")
    out.append("        }")
    out.append("    }};")
    out.append("}")
    out.append("")

    for module, enums in by_module.items():
        for enum in enums:
            out.append("#[test]")
            out.append(f"fn {module}_{camel_to_snake(enum.name)}_wire_values() {{")
            values = ", ".join(f'"{v.wire}"' for v in enum.variants)
            out.append(f"    assert_wire_values!({enum.name}, [{values}]);")
            out.append("}")
            out.append("")

    return "\n".join(out)


def camel_to_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2

    source_root = Path(sys.argv[1]).resolve()
    out_root = Path(__file__).resolve().parent.parent

    revision = subprocess.run(
        ["git", "-C", str(source_root), "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()

    total = 0
    problems: list[str] = []
    needs_methods: list[str] = []
    by_module: dict[str, list[WireEnum]] = {}

    shared: list[WireEnum] = []
    shared_sources: list[str] = []

    for rel_source, (rel_out, doc) in MODULES.items():
        parsed = parse_module(source_root / rel_source)

        # Route the shared ones out of their owning module.
        owned = [e for e in parsed if e.name not in SHARED_ENUMS]
        for enum in parsed:
            if enum.name in SHARED_ENUMS:
                shared.append(enum)
                shared_sources.append(rel_source)

        by_module[Path(rel_out).parent.name] = owned
        for enum in parsed:
            problems.extend(validate(enum))
            if enum.has_methods:
                needs_methods.append(f"{rel_source}::{enum.name}")

        out_path = out_root / rel_out
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(render_module(doc, rel_source, revision, owned))

        members = sum(len(e.variants) for e in owned)
        total += len(owned)
        print(f"{rel_out}: {len(owned)} enums, {members} variants")

    if shared:
        rel_out, doc = SHARED_OUTPUT
        out_path = out_root / rel_out
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(
            render_module(doc, ", ".join(sorted(set(shared_sources))), revision, shared)
        )
        by_module["types"] = shared
        total += len(shared)
        members = sum(len(e.variants) for e in shared)
        print(f"{rel_out}: {len(shared)} enums, {members} variants")

    parity_path = out_root / "tests" / "enum_parity.rs"
    parity_path.parent.mkdir(parents=True, exist_ok=True)
    parity_path.write_text(render_parity_test(revision, by_module))
    print(f"tests/enum_parity.rs: {sum(len(e) for e in by_module.values())} tests")

    if problems:
        print("\nPROBLEMS:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1

    print(f"\n{total} enums generated from alpaca-py @ {revision}")

    if needs_methods:
        # These carry behavior the macro cannot generate. Losing them silently is
        # exactly the failure mode a mechanical port is prone to.
        print("\nHand-port the methods on these into enums_ext.rs:")
        for name in needs_methods:
            print(f"  {name}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
