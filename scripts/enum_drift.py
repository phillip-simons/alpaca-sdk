#!/usr/bin/env python3
"""Diff this crate's wire enums against their spec schemas.

The `wire_enum!` blocks are checked-in source; the specs are Alpaca's own. Where
both describe the same type they should agree. The run prints how many do; no
count is kept here, because every count kept here has gone stale. It is several
numbers rather than one, because "has a schema" and "got a verdict" are
different counts and collapsing them is how this report last missed something.

Pairing is by name, with `ALIASES` for the types the two spell differently. That
map is not a convenience — an enum whose name does not match is not reported as
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
# The trailing comma is optional: `wire_enum!` ends its variant list with
# `),+ $(,)?`, so a block whose last variant omits it compiles fine. Requiring
# one here dropped that value silently — as a phantom gap if the spec listed it,
# and as nothing at all if it did not.
VARIANT = re.compile(r'^\s*(\w+) => "([^"]*)",?\s*$')

# A `cfg` attribute on the line before a `mod` — the two shapes it can guard.
CFG_ATTRIBUTE = re.compile(r"^\s*#\[cfg\((?P<predicate>.*)\)\]\s*$")
MOD_FILE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod (\w+)\s*;")
MOD_INLINE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod (\w+)\s*\{")


def _without_negations(predicate: str) -> str:
    """The predicate with every `not(…)` group removed, nesting included.

    Matched by counting parentheses rather than with a regex, because
    `not(all(test, feature = "x"))` holds a pair the regex form stopped at —
    leaving a bare `test` behind and marking a module that ships everywhere
    *except* under test as test-only. That drops shipping code silently, the
    direction this report treats as the worse error.
    """
    kept: list[str] = []
    opener = re.compile(r"\bnot\s*\(")
    at = 0

    while at < len(predicate):
        found = opener.match(predicate, at)
        if not found:
            kept.append(predicate[at])
            at += 1
            continue
        depth, scan = 0, found.end() - 1
        while scan < len(predicate):
            if predicate[scan] == "(":
                depth += 1
            elif predicate[scan] == ")":
                depth -= 1
                if not depth:
                    break
            scan += 1
        at = scan + 1

    return "".join(kept)


def is_test_cfg(line: str) -> bool:
    """Whether a `cfg` attribute compiles its item for tests only.

    Matching the literal `#[cfg(test)]` and nothing else left the exclusion
    half-closed: `types/serde_util.rs` guards its tests with
    `#[cfg(all(test, feature = "trading"))]`, which is just as test-only and was
    read as shipping code.

    Two things must not match. A `feature = "test-utils"` is a feature that
    happens to be spelled that way, so string literals are blanked before
    looking — and `not(test)` means the item exists precisely when tests do not,
    so those groups come out first. Excluding a shipping module would be the
    worse error of the two: an overcount is visible in the headline, an
    undercount is the silence this report exists to break.
    """
    attribute = CFG_ATTRIBUTE.match(line)
    if not attribute:
        return False
    predicate = re.sub(r'"(?:[^"\\]|\\.)*"', '""', attribute.group("predicate"))
    return re.search(r"\btest\b", _without_negations(predicate)) is not None

# Crate enums whose spec schema exists under a different name. Without these the
# pair never meets: `shared` is an intersection of names, so an enum this crate
# spells differently from Alpaca is not reported as drifting, it is not reported
# at all.
#
# That silence is not hypothetical. `TradeEvent` is Alpaca's
# `TradeUpdateEventType`, the names never matched, and it reached 0.1.0 carrying
# twelve of twenty-one documented values — transcribed from another SDK — with
# this report having no opinion on it. The inverse of `NOT_DRIFT`: naming a pair
# here is a claim that they are the same vocabulary under two names.
ALIASES: dict[str, str] = {
    "TradeEvent": "TradeUpdateEventType",
}

# Values this crate carries that the schema's own value list does not, for a
# reason other than "Alpaca stopped documenting it". The surplus list below
# tells the reader not to delete a value because Alpaca still serves undocumented
# ones — true in general, and the wrong reason for these. Recording the actual
# reason here keeps a deliberate choice from reading as unexplained drift on
# every future run.
CRATE_ONLY: dict[tuple[str, str], str] = {
    ("TradeEvent", "restated"): (
        "'sent when the order is manually modified', described in two prose "
        "passages — the schema's description and the trade-events operation's — "
        "and absent from every machine-readable value list. Both passages come "
        "from the same specification, which the published reference republishes, "
        "so this is one source saying it twice rather than two agreeing."
    ),
    ("TradeEvent", "held"): (
        "prose-only like restated, and weaker: it appears under only one of the "
        "two stream descriptions in the specification, and it is also a "
        "documented OrderStatus value, so it may be a status that leaked into "
        "an event list. Carried because an unnamed value is one no caller can "
        "match on, while a variant Alpaca never sends costs a dead match arm."
    ),
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


class BraceImbalance(Exception):
    """A file whose braces did not balance once literals were blanked.

    Raised rather than warned about: the brace count drives the `#[cfg(test)]`
    exclusion, so a file this happens in is one whose test enums may be being
    counted as the crate's, and a headline nobody can trust is worse than no
    headline.
    """

    def __init__(self, path: pathlib.Path, depth: int) -> None:
        super().__init__(
            f"{path}: braces do not balance (ended at depth {depth}). A raw "
            f"string spanning several lines is the likely cause; the "
            f"`#[cfg(test)]` exclusion cannot be trusted in this file."
        )


def _code_only(line: str) -> str:
    """The line with string and char literals blanked and any comment cut.

    Only ever used for counting braces. A test module is full of
    `format!("{}", …)` and `assert!(matches!(x, Y { .. }))`, and a brace inside
    a literal would close the skipped region early — which would start counting
    test declarations again, quietly, exactly the way this whole report fails.

    Raw strings first, and for the same reason: `r#"{"a": "}"}"#` survives the
    ordinary-string pass as unbalanced braces, and `decimal.rs`, `sse.rs` and
    `error.rs` all put raw strings inside `#[cfg(test)]` blocks. One spanning
    several lines still defeats this, which is what the balance check at the end
    of `crate_enums` is for.
    """
    line = re.sub(r'r(#*)".*?"\1', '""', line)
    line = re.sub(r'"(?:[^"\\]|\\.)*"', '""', line)
    line = re.sub(r"'(?:[^'\\]|\\.)'", "''", line)
    return re.sub(r"//.*$", "", line)


def test_only_files(src: pathlib.Path) -> set[pathlib.Path]:
    """Files whose entire module is `#[cfg(test)]`, so nothing in them ships.

    A stand-in declared to exercise the macro is not a wire enum, and counting
    one inflates the headline the same way the old `*enums*.rs` glob deflated
    it — `types/wire_tests.rs` declares a `Side` for exactly that purpose, and
    widening the glob started reporting it as the crate's.

    Resolved from the declaration site rather than the file, because the
    attribute sits on `mod wire_tests;` in the parent and leaves no trace
    inside the file it guards.
    """
    excluded: set[pathlib.Path] = set()

    for rs in sorted(src.rglob("*.rs")):
        lines = rs.read_text().splitlines()
        for previous, line in zip(lines, lines[1:]):
            if not is_test_cfg(previous):
                continue
            declaration = MOD_FILE.match(line)
            if not declaration:
                continue
            name = declaration.group(1)
            # Exactly where Rust would look, and nowhere else. A crate root or
            # a `mod.rs` declares siblings; any other file declares children
            # under a directory named after it. Trying all four regardless
            # meant `src/foo.rs` saying `mod wallets;` — which resolves to
            # `src/foo/wallets.rs` — also excluded a real, shipping
            # `src/wallets.rs`, dropping its enums with no diagnostic and
            # exit 0. Silently reading less of the crate than it claims to is
            # the whole defect this report was rewritten to stop having.
            if rs.name in {"mod.rs", "lib.rs", "main.rs"}:
                candidates = (rs.parent / f"{name}.rs", rs.parent / name / "mod.rs")
            else:
                here = rs.parent / rs.stem
                candidates = (here / f"{name}.rs", here / name / "mod.rs")
            for candidate in candidates:
                if candidate.is_file():
                    excluded.add(candidate)

    return excluded


def crate_enums(
    src: pathlib.Path,
) -> tuple[dict[str, list[str]], list[tuple[str, list[pathlib.Path]]]]:
    """Every `wire_enum!` in the crate, as `{name: [wire values]}`.

    Returns the enums and any name declared by more than one block, since the
    mapping is keyed by name and cannot hold both.

    Reads the source files, so a variant added or renamed by hand shows up here
    the same way a spec change does.

    Every `.rs` file, not just the ones named `*enums*.rs`. Roughly half this
    crate's `wire_enum!` blocks sit beside the models that use them —
    `broker/funding_wallet.rs`, `trading/wallets.rs` and a dozen others — and
    the narrower glob silently excluded them, which meant the report's headline
    counts described a subset while reading as though they described the crate.
    A `pub enum` that is not a `wire_enum!` has no `Variant => "wire"` arms, so
    it collects no values and the filter below drops it.

    Test code is excluded, both whole `#[cfg(test)]` modules and inline ones.
    Widening the glob without that traded an undercount for an overcount: a
    macro stand-in ships to nobody, and a fixture named after a real enum would
    collide with it and cost that enum its verdict.
    """
    excluded = test_only_files(src)
    enums: dict[str, list[str]] = {}
    # Only the declarations that carried wire values. A collision matters when
    # two `wire_enum!`s share a name — each fills the other's gaps, so no
    # verdict is honest. An ordinary valueless `pub enum` that happens to share
    # one is a different thing entirely, and treating it as a collision would
    # suppress a real enum's verdict over a name it merely shares: strictly
    # worse than the narrow glob, which at least compared it.
    declared_in: dict[str, list[pathlib.Path]] = {}

    for rs in sorted(src.rglob("*.rs")):
        if rs in excluded:
            continue
        lines = rs.read_text().splitlines()
        current: str | None = None
        carried = 0
        depth = 0
        skip_below: int | None = None
        pending_cfg_test = False

        for line in lines:
            code = _code_only(line)

            if skip_below is None:
                if is_test_cfg(line):
                    pending_cfg_test = True
                elif pending_cfg_test and MOD_INLINE.match(code):
                    skip_below, pending_cfg_test, current, carried = depth, False, None, 0
                elif line.strip() and not line.lstrip().startswith("#["):
                    pending_cfg_test = False

            if skip_below is None:
                declaration = ENUM_DECL.match(line)
                if declaration:
                    # `setdefault`, never `= []`. Assigning would let a *later*
                    # declaration of the same name wipe an earlier one's values,
                    # and since the glob widened to every `.rs` the names in
                    # scope now include ordinary `pub enum`s — a plain
                    # `enum OrderStatus` anywhere in the tree would silently
                    # erase the wire enum of that name and drop it out of the
                    # report entirely, counts and all, while still exiting 0.
                    # Accumulating instead means a collision can only ever add
                    # values, which is visible and reported below.
                    if current is not None and carried:
                        declared_in.setdefault(current, []).append(rs)
                    current, carried = declaration.group(1), 0
                    enums.setdefault(current, [])
                elif current is not None:
                    variant = VARIANT.match(line)
                    if variant:
                        enums[current].append(variant.group(2))
                        carried += 1
                    # `code`, not `line`: a closing brace with a trailing
                    # comment would otherwise leave the block open, and every
                    # later `Ident => "wire",` in the file — an ordinary match
                    # arm, say — would be filed as one of its values.
                    elif re.match(r"^\s*\}\s*$", code):
                        if carried:
                            declared_in.setdefault(current, []).append(rs)
                        current, carried = None, 0

            depth += code.count("{") - code.count("}")
            if skip_below is not None and depth <= skip_below:
                skip_below = None

        # A block left open at end of file still declared what it declared.
        if current is not None and carried:
            declared_in.setdefault(current, []).append(rs)

        # Every `.rs` file balances its braces, so a non-zero depth here means
        # the blanking above missed a literal — a raw string spanning lines is
        # the way left. That matters more than it looks: `depth` is what ends a
        # `#[cfg(test)]` skip, so a miscount reopens the region early and starts
        # counting test enums as the crate's. Refusing to answer beats answering
        # from a parse known to be wrong, which is the failure this report is
        # named after.
        if depth != 0:
            raise BraceImbalance(rs, depth)

    # No same-file exemption: two declarations in one file, in separate inline
    # modules or behind opposing `cfg`s, merge just as invisibly as two across
    # files, so the same path is recorded twice and still counts.
    collisions = [
        (name, files) for name, files in sorted(declared_in.items()) if len(files) > 1
    ]
    return {name: values for name, values in enums.items() if values}, collisions


def spec_enums(
    specs: pathlib.Path,
) -> tuple[dict[str, set[str]], dict[str, dict[str, int]], set[str]]:
    """Every named schema under `components.schemas` that is a string enum.

    Returns three things: the schemas that carry a value list; any name that
    more than one spec defines differently, as `{name: {spec file: value
    count}}`, so the report can say which surfaces disagreed and by how much;
    and every schema name seen at all, value list or not.

    The third is what keeps the coverage claim honest. A crate enum missing
    from the first has two quite different causes — Alpaca documents no schema
    of that name, or documents one this parser cannot read a value list out of
    — and only the first is fixable by aliasing.

    Parsed with regexes for the same reason `coverage.py` is: the specs are
    large, the shape needed is shallow, and a YAML dependency for two nested
    keys is not worth carrying.

    Only the schema's own `enum:` counts. A `properties.<field>.enum` is a
    different type that happens to live inside this one, and matching it by the
    outer schema's name would compare unrelated things.
    """
    # Every definition separately, not one per file. Two schemas of the same
    # name inside a single spec merge exactly as invisibly as two across specs,
    # and the crate side already refuses that same-file exemption — granting it
    # here would let one `broker.yaml` define `OrderSide` twice and report the
    # union as a clean verdict.
    occurrences: dict[str, list[tuple[str, set[str]]]] = {}
    declared: set[str] = set()

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
                declared.add(name)
                continue

            # Any other key at schema depth ends the current schema, even one
            # this pattern cannot read — a hyphen or a dot in the name, or an
            # inline `Foo: {}`. Leaving `name` pointing at the previous schema
            # would file the next `enum:` under the wrong one, which is worse
            # than not reading it: a value list attributed to a neighbour can
            # manufacture a disagreement that does not exist. The vendored
            # specs have no such key today; this is so that stays harmless.
            if re.match(r"^    \S", line):
                name, collecting = None, False
                continue

            if name and re.match(r"^      enum:\s*$", line):
                collecting = True
                occurrences.setdefault(name, []).append((spec.name, set()))
                continue

            if collecting:
                item = re.match(r"^        - (.*)$", line)
                if item:
                    value = item.group(1).strip().strip("'\"")
                    occurrences[name][-1][1].add(value)
                else:
                    collecting = False

    found = {
        name: set().union(*(values for _, values in seen))
        for name, seen in occurrences.items()
    }

    # A schema name defined more than once with *different* values is a
    # per-surface vocabulary, not one type. Merging them, which is what the
    # return value does, makes the comparison too permissive: a value only
    # `broker.yaml` documents excuses a crate enum built from `trading.yaml`.
    # Gaps survive that (the union can only add values Alpaca documents
    # somewhere) but surplus does not, so the merge quietly suppresses the
    # quieter half of the report. Named here so the verdict can be qualified
    # rather than presented as a clean match.
    merged: dict[str, dict[str, int]] = {}
    for name, seen in occurrences.items():
        defined = [(label, values) for label, values in seen if values]
        if len({frozenset(values) for _, values in defined}) <= 1:
            continue
        labelled: dict[str, int] = {}
        for label, values in sorted(defined, key=lambda pair: pair[0]):
            # Two definitions in one file need distinct labels, or the second
            # would overwrite the first and the report would show one source
            # for a disagreement that has two.
            unique, nth = label, 2
            while unique in labelled:
                unique, nth = f"{label} (definition {nth})", nth + 1
            labelled[unique] = len(values)
        merged[name] = labelled

    return (
        {name: values for name, values in found.items() if values},
        merged,
        declared,
    )


def bucket_total(*buckets: "list[str] | set[str]") -> int:
    """How many enums the buckets below account for between them."""
    return sum(len(bucket) for bucket in buckets)


def reconcile(buckets: int, compared: int) -> str | None:
    """The complaint if the buckets do not account for every compared enum.

    Every compared enum lands in exactly one bucket, so they have to add back
    up to the headline. A verdict that quietly stopped being counted is
    precisely how this report came to describe less than it appeared to, and a
    reconciliation done by hand in a plan file goes stale the first time
    somebody touches these branches.

    Split out from `main` so it can be tested. As the branches stand this
    cannot fire — which is exactly why it needs a test that does not depend on
    being able to provoke it through the report.
    """
    if buckets == compared:
        return None
    return (
        f"the buckets total {buckets} but {compared} enums were compared "
        f"— one is being counted twice or not at all"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--specs", type=pathlib.Path, default=pathlib.Path("specs"))
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    args = parser.parse_args()

    if not args.specs.is_dir():
        print(f"{args.specs} is missing — run `just specs` first", file=sys.stderr)
        return 1

    if not args.src.is_dir():
        print(f"{args.src} is missing — run from the repository root", file=sys.stderr)
        return 1

    try:
        crate, collisions = crate_enums(args.src)
    except BraceImbalance as imbalance:
        print(imbalance, file=sys.stderr)
        return 1
    spec, merged_schemas, declared_schemas = spec_enums(args.specs)

    # An alias that does not resolve on both sides silently restores the very
    # blindness the map exists to remove: the pair stops being compared and the
    # enum drops back into the unchecked list without comment. Both directions
    # fail loudly, because a stale alias is indistinguishable from no alias in
    # the report's output — which is how the original gap lasted a release.
    for enum, schema in sorted(ALIASES.items()):
        if enum not in crate:
            print(
                f"ALIASES maps {enum}, which is not a wire_enum! in this crate "
                f"— it was probably renamed; update the key or drop the entry",
                file=sys.stderr,
            )
            return 1
        if schema not in spec:
            # Two different states, and telling them apart decides what a
            # maintainer should do. A schema that exists but documents its
            # values in prose is not a broken alias, and "drop it" is the one
            # instruction that would restore the original blindness — dropping
            # the TradeEvent pair is precisely how that enum went unchecked for
            # a release.
            if schema in declared_schemas:
                print(
                    f"{enum} is aliased to {schema}, which exists but carries "
                    f"no value list this report can read — the pairing is "
                    f"right, so keep it; the comparison has to be done by hand "
                    f"until the parser can follow a property",
                    file=sys.stderr,
                )
            else:
                print(
                    f"{enum} is aliased to {schema}, which no spec defines "
                    f"— fix the alias or drop it",
                    file=sys.stderr,
                )
            return 1

    # A CRATE_ONLY entry suppresses a value from the surplus list, so a stale one
    # hides a real finding rather than merely going unused.
    for enum, value in sorted(CRATE_ONLY):
        if value not in crate.get(enum, []):
            print(
                f"CRATE_ONLY explains {enum} {value!r}, which this crate no "
                f"longer carries — drop the entry",
                file=sys.stderr,
            )
            return 1

    # DECIDED suppresses a gap, and a gap is the finding this report exists to
    # produce, so an entry naming an enum that no longer exists is the quietest
    # possible way to keep suppressing one. Only the key is checkable here: the
    # value legitimately describes something the crate does *not* carry.
    for enum, _ in sorted(DECIDED):
        if enum not in crate:
            print(
                f"DECIDED names {enum}, which is not a wire_enum! in this "
                f"crate — it was probably renamed; update the key or drop it",
                file=sys.stderr,
            )
            return 1

    # NOT_DRIFT gets no *semantic* guard — no diff confirms that two
    # vocabularies are unrelated — but whether the key still names an enum is a
    # different question, and the answer is checkable. A renamed or misspelled
    # key is silently inert, which is the failure mode every other map here is
    # now protected from.
    for enum in sorted(NOT_DRIFT):
        if enum not in crate:
            print(
                f"NOT_DRIFT names {enum}, which is not a wire_enum! in this "
                f"crate — it was probably renamed; update the key or drop it",
                file=sys.stderr,
            )
            return 1

    # Same rule again, and it was the one map this rework left without it. An
    # UNRESOLVED entry names a spelling the crate carries and the specs do not;
    # once the crate stops carrying it the note describes nothing, and because
    # it prints under whichever enum shares its name, it attaches itself to
    # whatever unrelated gap turns up there next.
    for enum, ours in sorted(UNRESOLVED):
        if ours not in crate.get(enum, []):
            print(
                f"UNRESOLVED notes {enum} {ours!r}, which this crate no longer "
                f"carries — drop the entry",
                file=sys.stderr,
            )
            return 1

    schema_of = {name: ALIASES.get(name, name) for name in crate}
    shared = sorted(name for name in crate if schema_of[name] in spec)
    unchecked = sorted(name for name in crate if schema_of[name] not in spec)

    print(f"{len(crate)} enums in the crate, {len(spec)} enum schemas in the specs")
    # Every enum with a schema is not every enum that gets a verdict: the ones
    # named in NOT_DRIFT and the ones declared twice are deliberately skipped,
    # and folding them into one number is how a partial answer starts reading
    # as a whole one.
    skipped = len(
        [n for n in shared if n in NOT_DRIFT or n in {name for name, _ in collisions}]
    )
    if skipped:
        print(f"{len(shared)} have a schema to compare against, {skipped} of them")
        print(f"skipped for the reasons below, so {len(shared) - skipped} compared\n")
    else:
        # Not "0 of them skipped for the reasons below" — there are no reasons
        # below to point at.
        print(f"{len(shared)} have a schema to compare against, all compared\n")

    if collisions:
        print("Declared more than once, so not compared at all. This report is")
        print("keyed by name and cannot hold two types that share one; comparing")
        print("their combined values would let each cover the other's gaps.")
        print("A verdict comes back when the declarations stop sharing a name.\n")
        for name, files in collisions:
            # Only the ones with a schema were ever going to be compared, so
            # only those are counted as skipped above. Saying so here stops the
            # block reading as a contradiction of that count.
            spare = "" if name in shared else "  <- no spec schema either way"
            print(f"  {name}: {', '.join(str(f) for f in files)}{spare}")
        print()

    agree: list[str] = []
    excepted: list[str] = []
    against_merged: list[str] = []
    merged_compared: list[str] = []
    missing: dict[str, list[str]] = {}
    extra: dict[str, list[str]] = {}

    collided = {name for name, _ in collisions}
    # The names that actually reached a comparison. A note explaining why some
    # value is surplus, or why a gap was left open, is a statement about a
    # verdict — printing it for an enum this run declined to compare asserts a
    # result that was never produced.
    compared = [n for n in shared if n not in NOT_DRIFT and n not in collided]

    for name in shared:
        if name in NOT_DRIFT:
            continue
        if name in collided:
            # Values from every declaration were accumulated under one key, so
            # comparing that union answers a question nobody asked: a value
            # missing from one declaration is supplied by the other and the
            # merged set matches the schema anyway. Verified — deleting a value
            # from one `TransferDirection` left it reported as agreeing exactly.
            # No verdict is the honest output until the names are disambiguated.
            continue
        ours = set(crate[name])
        theirs = spec[schema_of[name]]
        gap = sorted(value for value in theirs - ours if (name, value) not in DECIDED)
        surplus = sorted(
            value for value in ours - theirs if (name, value) not in CRATE_ONLY
        )
        if schema_of[name] in merged_schemas:
            # Compared, because a gap still means a gap — the union only ever
            # adds values Alpaca documents somewhere. But not called a match:
            # surplus is unreliable against a merged vocabulary, so it is
            # dropped rather than reported.
            #
            # Recorded whether or not there is a gap. An enum with both a gap
            # and a suppressed surplus is the case most in need of the caveat
            # — it appears below under a heading that reads like a full
            # verdict, while half of it was never asked. Listing only the
            # gap-free ones would present a partial answer as a whole one,
            # which is the failure this report exists to stop.
            merged_compared.append(name)
            if gap:
                missing[name] = gap
            else:
                against_merged.append(name)
            continue
        if not gap and not surplus:
            # "Exactly" has to mean exactly. An enum whose only differences are
            # ones DECIDED or CRATE_ONLY suppressed does agree with the schema
            # for review purposes, but saying so in the same breath as a genuine
            # value-for-value match is the sort of rounding this report exists
            # to stop — the whole failure was a partial answer reading as a
            # whole one.
            if (ours - theirs) or (theirs - ours):
                excepted.append(name)
            else:
                agree.append(name)
            continue
        if gap:
            missing[name] = gap
        if surplus:
            extra[name] = surplus

    # A NOT_DRIFT entry suppresses an enum from comparison entirely, which is a
    # heavier hammer than DECIDED or CRATE_ONLY and has no staleness guard it
    # can be given — the claim is that two vocabularies are unrelated, which no
    # diff can confirm. What it can notice is the one state that would refute
    # the claim outright.
    for name in NOT_DRIFT:
        if name in shared and set(crate[name]) == spec[schema_of[name]]:
            print(
                f"\n{name}: NOT_DRIFT says this schema is a different "
                f"vocabulary, but the two now match value for value — recheck "
                f"the entry"
            )

    print(f"{len(agree)} agree exactly: {', '.join(agree) or 'none'}")
    if excepted:
        print(
            f"\n{len(excepted)} agree apart from values recorded below: "
            f"{', '.join(excepted)}"
        )

    if against_merged:
        one = len(against_merged) == 1
        print(
            f"\n{len(against_merged)} {'has' if one else 'have'} no gap, but "
            f"{'was' if one else 'were'} compared against a schema name\nthat "
            f"more than one spec defines differently, so the union was used\n"
            f"and any surplus is not trustworthy:\n"
        )
        for name in against_merged:
            sizes = ", ".join(
                f"{f} {n}" for f, n in merged_schemas[schema_of[name]].items()
            )
            print(f"  {name}: {sizes}")
    for name, reason in NOT_DRIFT.items():
        if name in shared:
            print(f"\n{name}: not drift — {reason}")
    for (name, value), reason in CRATE_ONLY.items():
        # Only where it is still surplus. Once Alpaca lists the value, the entry
        # is describing a disagreement that no longer exists, and printing it
        # unconditionally would assert something false on every run.
        if name in compared and value not in spec[schema_of[name]]:
            print(f"\n{name} {value}: carried deliberately — {reason}")
    for (name, value), reason in DECIDED.items():
        # Same staleness rule as CRATE_ONLY above: an entry describes a gap the
        # crate declines to close, so it is only true while the spec still lists
        # the value and the crate still does not.
        if (
            name in compared
            and value in spec[schema_of[name]]
            and value not in crate[name]
        ):
            shown = value if value else '""'
            print(f"\n{name} {shown}: decided against — {reason}")

    if missing:
        print("\nIn the spec, not in the crate — a value no caller can name:\n")
        for name, values in sorted(missing.items()):
            # The gap below is sound either way, but against a merged
            # vocabulary it is only half the comparison: surplus was computed
            # and discarded, so this entry is not the whole answer for it.
            merged = (
                "  <- merged vocabulary; surplus not checked"
                if name in merged_compared
                else ""
            )
            print(f"  {name}{merged}")
            for value in values:
                # The empty string is a real wire value here — Alpaca documents
                # `simple (or "")` for OrderClass — and printing it bare would
                # look like a blank line rather than a finding.
                #
                # Built outside the f-string: a backslash inside an f-string
                # expression is a syntax error before Python 3.12, and this
                # script has to run on whatever the contributor has.
                shown = value if value else '"" (the empty string)'
                print(f"    {shown}")
            # Keyed by the crate's spelling, since the whole point of an
            # unresolved pair is that the two spellings differ. Only while the
            # spelling really is absent from the schema: once Alpaca adopts it
            # the pair is resolved, and the note would otherwise stay bolted to
            # whatever other gap this enum develops later.
            for (enum, ours), note in UNRESOLVED.items():
                if enum == name and ours not in spec[schema_of[name]]:
                    print(f"    note: the crate carries {ours} — {note}")

    if extra:
        print("\nIn the crate, not in the spec. **Do not delete these.**")
        print("Alpaca still serves values it has stopped documenting;")
        print("removing one turns a working match arm into an `Unknown`.\n")
        for name, values in sorted(extra.items()):
            print(f"  {name}: {', '.join(values)}")
            # The heading above is the usual reason a value is crate-only, and
            # it is the wrong one for an UNRESOLVED pair: a suspected typo is
            # not a de-documented value, and leaving it under that sentence is
            # the same mislabelling CRATE_ONLY was added to end for `restated`
            # and `held`.
            for (enum, ours), note in UNRESOLVED.items():
                if enum == name and ours in values:
                    print(f"    {ours} is not a de-documented value — {note}")

    # Two different reasons an enum went uncompared, and only one of them is
    # fixable by aliasing. Telling a reader to "add the pair to ALIASES" for a
    # schema that already exists under exactly that name sends them after a
    # no-op, and states the report's own coverage wrongly in the bargain —
    # which is the defect this whole rework is about, aimed at itself.
    unreadable = [n for n in unchecked if schema_of[n] in declared_schemas]
    absent = [n for n in unchecked if schema_of[n] not in declared_schemas]

    def catalogue(names: list[str]) -> None:
        for name in names:
            # Distinct values: a name declared twice has both lists accumulated
            # under it, and printing the raw length double-counts.
            dup = " — and declared more than once" if name in collided else ""
            print(f"  {name} ({len(set(crate[name]))} values){dup}")

    if absent:
        # Counted, like the compared side. A reader should not have to tally
        # the list to learn how much of the crate this report is silent about,
        # and the CHANGELOG quotes numbers the report itself would not state.
        print(f"\n{len(absent)} with no spec schema found — this report says")
        print("nothing about these. Not a clean bill of health: an enum here is")
        print("unverified, not verified-and-agreeing. If one does have a schema")
        print("under another name, an ALIASES pair starts it being checked.\n")
        catalogue(absent)

    if unreadable:
        one = len(unreadable) == 1
        print(f"\n{len(unreadable)} {'has' if one else 'have'} a schema of that")
        print("name carrying no value list this report can read — the values")
        print("are in its `description` prose, or on one of its properties.")
        print("Also unverified, and aliasing will not help: the name already")
        print("matches. Closing one means comparing it by hand, or teaching")
        print("the parser to follow a property.\n")
        catalogue(unreadable)

    # Every compared enum lands in exactly one bucket, so they have to add back
    # up to the headline. Printing the sum is the cheap half; checking it is the
    # point. A verdict that quietly stopped being counted is precisely how this
    # report came to describe less than it appeared to, and a reconciliation
    # done by hand in a plan file goes stale the first time a branch touches
    # these branches.
    flagged = set(missing) | set(extra)
    print(
        f"\n{len(agree)} exact + {len(excepted)} with exceptions + "
        f"{len(against_merged)} against a merged vocabulary + {len(flagged)} "
        f"flagged = {bucket_total(agree, excepted, against_merged, flagged)} "
        f"compared"
    )
    complaint = reconcile(
        bucket_total(agree, excepted, against_merged, flagged), len(compared)
    )
    if complaint:
        print(complaint, file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
