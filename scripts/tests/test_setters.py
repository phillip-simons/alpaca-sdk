"""What `setters.py` sees when a request wraps a shared base.

Every test here drives a synthetic `src/` tree in a temporary directory, for the
same reason `test_enum_drift.py` does: exercising the parser needs a dozen lines
of Rust, not the real crate, so this suite costs nothing and joins `just check`.

The subject is the half of the report the compiler cannot do. `#[derive(Setters)]`
reads a struct's real fields, so a type that derives it cannot have a field
without a setter. What no derive can see is a *wrapper* that holds a flattenable
base and never flattens it: it compiles, it serializes correctly, and it just
makes the caller reach through `.base`. Nothing says so, so this script does.

Most of these assert on the absence of something — a gap that should have been
reported and was not — because that is the direction this gate fails in. A false
positive is a nuisance; a false negative is the script saying "every base one
holds is flattened" while one is not.

`unittest` rather than pytest, deliberately: this repository carries no Python
dependencies and a verification script's tests should not introduce the first.
"""

from __future__ import annotations

import importlib.util
import pathlib
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "setters.py"


def load_script():
    """A fresh module object, so a test that patches a map cannot leak."""
    spec = importlib.util.spec_from_file_location("setters", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BASE = """\
/// Fields shared by every historical time series request.
#[derive(Debug, Serialize, Setters)]
#[setters(flattenable)]
pub struct TimeseriesRequest {
    /// The symbols to query.
    pub symbol_or_symbols: Symbols,
    /// Caps the total number of items returned.
    pub limit: Option<u32>,
}
"""


class TreeTest(unittest.TestCase):
    """A synthetic `src/` tree, parsed by the real script."""

    def setUp(self) -> None:
        self.setters = load_script()
        self.root = pathlib.Path(tempfile.mkdtemp())

    def parse(self, source: str) -> dict[str, tuple]:
        path = self.root / "requests.rs"
        path.write_text(source)
        return self.setters.parse(path)

    def named_of(self, source: str, struct: str) -> list[tuple[str, str, bool]]:
        """The `(field, type, flattens)` triples the parser recorded."""
        return self.parse(source)[struct][4]

    def flattenable_of(self, source: str, struct: str) -> bool:
        return self.parse(source)[struct][1]


class TestFlattenableIsRecognised(TreeTest):
    def test_the_attribute_makes_a_struct_a_base(self) -> None:
        self.assertTrue(self.flattenable_of(BASE, "TimeseriesRequest"))

    def test_a_struct_without_it_is_not_a_base(self) -> None:
        source = BASE.replace("#[setters(flattenable)]\n", "")
        self.assertFalse(self.flattenable_of(source, "TimeseriesRequest"))

    def test_a_doc_comment_mentioning_it_does_not_make_a_base(self) -> None:
        """Prose is not configuration.

        This repository writes the reasoning next to the thing it applies to, so
        a doc comment discussing `#[setters(flattenable)]` is ordinary here —
        and reading one as the attribute would invent a base, then report every
        wrapper of it as a gap that does not exist.
        """
        source = BASE.replace(
            "#[setters(flattenable)]\n",
            "/// One day this should carry `#[setters(flattenable)]`.\n",
        )
        self.assertFalse(self.flattenable_of(source, "TimeseriesRequest"))


class TestFlattenedFields(TreeTest):
    """Which required fields the parser records as candidate bases."""

    def wrapper(self, field: str) -> str:
        return BASE + f"""
/// Historical bars for stocks.
#[derive(Debug, Serialize, Setters)]
pub struct StockBarsRequest {{
{field}
    /// The bar interval.
    pub timeframe: TimeFrame,
}}
"""

    def test_a_flattened_base_is_recorded_as_flattened(self) -> None:
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    #[serde(flatten)]\n"
            "    #[setters(flatten)]\n"
            "    pub base: TimeseriesRequest,"
        )
        self.assertIn(
            ("base", "TimeseriesRequest", True),
            self.named_of(source, "StockBarsRequest"),
        )

    def test_an_unflattened_base_is_recorded_as_not(self) -> None:
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    #[serde(flatten)]\n"
            "    pub base: TimeseriesRequest,"
        )
        self.assertIn(
            ("base", "TimeseriesRequest", False),
            self.named_of(source, "StockBarsRequest"),
        )

    def test_serde_flatten_alone_does_not_satisfy_it(self) -> None:
        """`#[serde(flatten)]` sits on every one of these fields already.

        The two attributes answer different questions — one is about the wire,
        one about the setters — and a pattern that could not tell them apart
        would report every wrapper in the crate as already done.
        """
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    #[serde(flatten)]\n"
            "    pub base: TimeseriesRequest,"
        )
        (recorded,) = [
            entry for entry in self.named_of(source, "StockBarsRequest") if entry[0] == "base"
        ]
        self.assertFalse(recorded[2])

    def test_a_qualified_base_resolves_to_its_last_segment(self) -> None:
        """`base_ident` in the derive resolves on the last path segment.

        So a wrapper spelling its base `crate::data::TimeseriesRequest` finds
        the same helper — and a pattern here that insisted on the bare spelling
        would not record the field at all, letting a missing `flatten` through.
        """
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    pub base: crate::data::TimeseriesRequest,"
        )
        self.assertIn(
            ("base", "TimeseriesRequest", False),
            self.named_of(source, "StockBarsRequest"),
        )

    def test_a_comment_mentioning_flatten_does_not_satisfy_it(self) -> None:
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    ///\n"
            "    /// Give this `#[setters(flatten)]` one day.\n"
            "    pub base: TimeseriesRequest,"
        )
        self.assertIn(
            ("base", "TimeseriesRequest", False),
            self.named_of(source, "StockBarsRequest"),
        )

    def test_an_optional_field_is_not_a_candidate_base(self) -> None:
        """The derive refuses `flatten` on an `Option`, so this never asks.

        A flattened base is one the wrapper always holds; an absent one has
        nothing for the delegates to write through to.
        """
        source = self.wrapper(
            "    /// The shared filters.\n"
            "    pub base: Option<TimeseriesRequest>,"
        )
        self.assertEqual(
            [entry for entry in self.named_of(source, "StockBarsRequest") if entry[0] == "base"],
            [],
        )


class TestScopeOfBases(TreeTest):
    """A base is whatever carries the attribute, wherever it is."""

    def test_a_base_is_recorded_even_with_a_non_request_name(self) -> None:
        """The derive consults no name rule, so neither can this.

        Scoping the base set to `*Request*` would make a wrapper's `flatten`
        look unnecessary because its base had dropped out of the set.
        """
        source = BASE.replace("TimeseriesRequest", "SharedFilters")
        self.assertTrue(self.flattenable_of(source, "SharedFilters"))


if __name__ == "__main__":
    unittest.main()
