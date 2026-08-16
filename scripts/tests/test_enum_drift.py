"""What `enum_drift.py` reads, and what it refuses to answer.

Every test here drives a synthetic `src/` and `specs/` tree built in a
temporary directory. That is the whole reason this file can exist: running the
*report* needs Alpaca's `specs/`, which is gitignored and fetched over the
network, but exercising the *parser* needs a dozen lines of YAML. So this suite
joins `just check` even though `just enums-drift` cannot.

Each case here is a defect the script actually had. The script's job is to
refuse to present a partial answer as a whole one, and every one of these was a
way it did exactly that while exiting 0 — which is also why so many assert on
the absence of something rather than its presence.

Written against `unittest` rather than pytest deliberately: this repository
carries no Python dependencies and a verification script's tests should not be
the thing that introduces one.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile
import textwrap
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "enum_drift.py"


def load_script():
    """A fresh module object, so a test that patches a map cannot leak."""
    spec = importlib.util.spec_from_file_location("enum_drift", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wire_enum(name: str, values: dict[str, str], trailing_comma: bool = True) -> str:
    """A `wire_enum!` block, spelled the way the macro accepts it."""
    arms = [f'        {variant} => "{wire}",' for variant, wire in values.items()]
    if not trailing_comma and arms:
        arms[-1] = arms[-1].rstrip(",")
    body = "\n".join(arms)
    return f"wire_enum! {{\n    pub enum {name} {{\n{body}\n    }}\n}}\n"


def schema(name: str, values: list[str]) -> str:
    """One `components.schemas` entry carrying a value list."""
    listed = "\n".join(f"        - {value}" for value in values)
    return f"    {name}:\n      enum:\n{listed}\n"


class TreeTest(unittest.TestCase):
    """Builds a `src/` and `specs/` pair and runs the script over it."""

    def setUp(self) -> None:
        self.script = load_script()
        self._tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self._tmp.name)
        (self.root / "src").mkdir()
        (self.root / "specs").mkdir()
        self.addCleanup(self._tmp.cleanup)

    def write(self, relative: str, content: str) -> pathlib.Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(content))
        return path

    def write_spec(self, filename: str, entries: str) -> None:
        self.write(f"specs/{filename}", f"components:\n  schemas:\n{entries}")

    def enums(self) -> dict[str, list[str]]:
        found, _ = self.script.crate_enums(self.root / "src")
        return found

    def collisions(self):
        _, found = self.script.crate_enums(self.root / "src")
        return found

    def satisfy_guards(self) -> str:
        """The enums every suppression map names, so a guard is not what fires.

        The maps are keyed by real enum names, so a synthetic tree without them
        trips a staleness guard before reaching the code under test. Any test
        that calls `main` needs this; one calling `crate_enums` directly does
        not.
        """
        return (
            wire_enum(
                "TradeEvent",
                {"Restated": "restated", "Held": "held", "Fill": "fill"},
            )
            + wire_enum("TaxIdType", {"ArgCuit": "ARG_AR_CUIT"})
            + wire_enum("Exchange", {"Nyse": "N"})
            + wire_enum("OrderClass", {"Simple": "simple"})
        )

    def guarded_spec(self) -> str:
        return (
            schema("TradeUpdateEventType", ["fill"])
            + schema("TaxIdType", ["ARG_AG_CUIT"])
            + schema("OrderClass", ["simple"])
        )

    def run_main(self) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        argv = [
            "enum_drift.py",
            "--specs",
            str(self.root / "specs"),
            "--src",
            str(self.root / "src"),
        ]
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            saved, sys.argv = sys.argv, argv
            try:
                code = self.script.main()
            finally:
                sys.argv = saved
        return code, out.getvalue(), err.getvalue()


class TestCfgTestExclusion(TreeTest):
    """Test code is not the crate's.

    Widening the glob from `*enums*.rs` to every `.rs` fixed an undercount and
    introduced an overcount: `wire_tests.rs` declares a `Side` to exercise the
    macro, and it ships to nobody.
    """

    def test_file_level_test_module_is_excluded(self) -> None:
        self.write("src/types/mod.rs", "#[cfg(test)]\nmod wire_tests;\n")
        self.write("src/types/wire_tests.rs", wire_enum("Side", {"Buy": "buy"}))
        self.write("src/lib.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_inline_test_module_is_excluded(self) -> None:
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + "\n#[cfg(test)]\nmod tests {\n"
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_compound_cfg_is_still_test_only(self) -> None:
        """`serde_util.rs` guards its tests with `all(test, feature = …)`.

        Matching the bare `#[cfg(test)]` and nothing else left the exclusion
        half-closed against a form already in this crate.
        """
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + '\n#[cfg(all(test, feature = "trading"))]\nmod tests {\n'
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_compound_cfg_on_a_file_module_is_still_test_only(self) -> None:
        self.write(
            "src/types/mod.rs",
            '#[cfg(any(test, feature = "trading"))]\nmod wire_tests;\n',
        )
        self.write("src/types/wire_tests.rs", wire_enum("Side", {"Buy": "buy"}))
        self.write("src/lib.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_feature_merely_named_test_is_not_excluded(self) -> None:
        """`feature = "test-utils"` is a feature, not a test cfg.

        Excluding a shipping module is the worse of the two errors: an
        overcount shows up in the headline, an undercount is silence.
        """
        self.write(
            "src/lib.rs",
            '#[cfg(feature = "test-utils")]\nmod helpers {\n'
            + wire_enum("Shipping", {"Live": "live"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_not_test_is_not_excluded(self) -> None:
        """`not(test)` compiles precisely when tests do not."""
        self.write(
            "src/lib.rs",
            "#[cfg(not(test))]\nmod shipping {\n"
            + wire_enum("Shipping", {"Live": "live"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_sibling_is_not_excluded_by_a_child_test_module(self) -> None:
        """`src/foo.rs` declaring `mod wallets;` means `src/foo/wallets.rs`.

        Trying every candidate path regardless of the declaring file also
        excluded a real, shipping `src/wallets.rs` — an undercount with no
        diagnostic, which is the failure this script is named after.
        """
        self.write("src/decoy.rs", "#[cfg(test)]\nmod wallets;\n")
        self.write("src/decoy/wallets.rs", wire_enum("TestOnly", {"A": "a"}))
        self.write("src/wallets.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_raw_string_does_not_reopen_the_skip(self) -> None:
        """`r#"{"a": "}"}"#` survives ordinary-string blanking as loose braces.

        Those close the skipped region early and put test enums back in the
        headline. `decimal.rs`, `sse.rs` and `error.rs` all hold raw strings
        inside test modules, so this was live ammunition.
        """
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + '\n#[cfg(test)]\nmod tests {\n    let s = r#"{"a": "}"}"#;\n'
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_unbalanced_braces_stop_the_run(self) -> None:
        """A raw string across lines defeats a line-at-a-time parse.

        There is no cheap fix for that, so the script refuses to answer rather
        than answering from a parse it knows is wrong.
        """
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + '\n#[cfg(test)]\nmod tests {\n    const J: &str = r#"\n        {\n    "#;\n}\n',
        )

        with self.assertRaises(self.script.BraceImbalance):
            self.script.crate_enums(self.root / "src")

    def test_the_imbalance_is_an_exit_code_not_a_traceback(self) -> None:
        self.write(
            "src/lib.rs",
            self.satisfy_guards()
            + '\n#[cfg(test)]\nmod tests {\n    const J: &str = r#"\n        {\n    "#;\n}\n',
        )
        self.write_spec("trading.yaml", self.guarded_spec())

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("braces do not balance", err)


class TestCollisions(TreeTest):
    """Two declarations of one name, and what that does and does not mean."""

    def test_two_wire_enums_of_one_name_collide(self) -> None:
        self.write("src/a.rs", wire_enum("Shared", {"A": "a"}))
        self.write("src/b.rs", wire_enum("Shared", {"B": "b"}))

        self.assertEqual([name for name, _ in self.collisions()], ["Shared"])

    def test_values_accumulate_rather_than_overwrite(self) -> None:
        """A collision may only ever add, which is visible and reported.

        Assigning would let the later declaration erase the earlier one's
        values and drop it from the report entirely, still at exit 0.
        """
        self.write("src/a.rs", wire_enum("Shared", {"A": "a"}))
        self.write("src/b.rs", wire_enum("Shared", {"B": "b"}))

        self.assertEqual(sorted(self.enums()["Shared"]), ["a", "b"])

    def test_a_valueless_pub_enum_is_not_a_collision(self) -> None:
        """Sharing a name with an ordinary enum must not cost a verdict.

        Treating it as a collision suppressed a real wire enum's comparison
        entirely — strictly worse than the narrow glob, which compared it.
        """
        self.write("src/a.rs", wire_enum("OrderStatus", {"New": "new"}))
        self.write("src/b.rs", "pub enum OrderStatus {\n    Whatever,\n}\n")

        self.assertEqual(self.collisions(), [])
        self.assertEqual(sorted(self.enums()), ["OrderStatus"])

    def test_two_declarations_in_one_file_still_collide(self) -> None:
        """No same-file exemption: they merge just as invisibly."""
        self.write(
            "src/a.rs",
            wire_enum("Shared", {"A": "a"}) + wire_enum("Shared", {"B": "b"}),
        )

        self.assertEqual([name for name, _ in self.collisions()], ["Shared"])


class TestVariantParsing(TreeTest):
    def test_a_final_variant_without_a_comma_is_read(self) -> None:
        """`wire_enum!` ends its list with `),+ $(,)?`, so the comma is optional.

        Requiring one dropped that value silently — a phantom gap if the spec
        listed it, and nothing at all if it did not.
        """
        self.write(
            "src/a.rs",
            wire_enum("Colour", {"Red": "red", "Blue": "blue"}, trailing_comma=False),
        )

        self.assertEqual(sorted(self.enums()["Colour"]), ["blue", "red"])


class TestSpecMerging(TreeTest):
    """A name two specs define differently is a per-surface vocabulary."""

    def test_differing_definitions_across_files_are_flagged(self) -> None:
        self.write_spec("broker.yaml", schema("OrderSide", ["buy", "sell", "short"]))
        self.write_spec("trading.yaml", schema("OrderSide", ["buy", "sell"]))

        _, merged, _ = self.script.spec_enums(self.root / "specs")

        self.assertIn("OrderSide", merged)
        self.assertEqual(
            merged["OrderSide"], {"broker.yaml": 3, "trading.yaml": 2}
        )

    def test_identical_definitions_are_not_flagged(self) -> None:
        self.write_spec("broker.yaml", schema("OrderSide", ["buy", "sell"]))
        self.write_spec("trading.yaml", schema("OrderSide", ["buy", "sell"]))

        _, merged, _ = self.script.spec_enums(self.root / "specs")

        self.assertEqual(merged, {})

    def test_two_definitions_in_one_spec_are_flagged(self) -> None:
        """The crate side refuses a same-file exemption; the spec side must too."""
        self.write_spec(
            "broker.yaml",
            schema("OrderSide", ["buy", "sell"]) + schema("OrderSide", ["buy", "short"]),
        )

        _, merged, _ = self.script.spec_enums(self.root / "specs")

        self.assertIn("OrderSide", merged)
        self.assertEqual(len(merged["OrderSide"]), 2)

    def test_the_merged_values_are_the_union(self) -> None:
        self.write_spec("broker.yaml", schema("OrderSide", ["buy", "short"]))
        self.write_spec("trading.yaml", schema("OrderSide", ["buy", "sell"]))

        found, _, _ = self.script.spec_enums(self.root / "specs")

        self.assertEqual(found["OrderSide"], {"buy", "sell", "short"})

    def test_a_schema_without_a_value_list_is_still_declared(self) -> None:
        """The distinction that keeps the coverage claim honest.

        A schema documenting its values in prose is not the same as no schema,
        and only one of the two is fixable by aliasing.
        """
        self.write_spec(
            "broker.yaml",
            "    CIPProvider:\n      description: alloy, trulioo\n      type: string\n",
        )

        found, _, declared = self.script.spec_enums(self.root / "specs")

        self.assertNotIn("CIPProvider", found)
        self.assertIn("CIPProvider", declared)


class TestStalenessGuards(TreeTest):
    """A stale entry that hides a real finding looks like a clean result."""

    def setUp(self) -> None:
        super().setUp()
        self.write("src/lib.rs", self.satisfy_guards())
        self.write_spec("trading.yaml", self.guarded_spec())

    def test_a_clean_tree_passes(self) -> None:
        code, _, err = self.run_main()

        self.assertEqual(code, 0, err)

    def test_an_alias_key_that_names_no_enum_fails(self) -> None:
        self.script.ALIASES["Missing"] = "TradeUpdateEventType"

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("Missing", err)

    def test_an_alias_target_no_spec_defines_fails(self) -> None:
        self.script.ALIASES["TradeEvent"] = "NotASchema"

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("no spec defines", err)

    def test_an_alias_target_that_exists_but_is_unreadable_says_keep_it(self) -> None:
        """Dropping the TradeEvent pair is how that enum went unchecked.

        So the one instruction this case must not give is "drop it".
        """
        self.write_spec(
            "trading.yaml",
            "    TradeUpdateEventType:\n      description: prose only\n      type: string\n"
            + schema("TaxIdType", ["ARG_AG_CUIT"]),
        )

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("keep it", err)
        self.assertNotIn("drop it", err)

    def test_a_crate_only_entry_for_a_dropped_value_fails(self) -> None:
        self.write(
            "src/lib.rs",
            wire_enum("TradeEvent", {"Held": "held", "Fill": "fill"})
            + wire_enum("TaxIdType", {"ArgCuit": "ARG_AR_CUIT"})
            + wire_enum("Exchange", {"Nyse": "N"})
            + wire_enum("OrderClass", {"Simple": "simple"}),
        )

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("CRATE_ONLY", err)
        self.assertIn("restated", err)

    def test_an_unresolved_entry_for_a_dropped_value_fails(self) -> None:
        """Otherwise the note prints attached to an unrelated gap."""
        self.write(
            "src/lib.rs",
            wire_enum(
                "TradeEvent",
                {"Restated": "restated", "Held": "held", "Fill": "fill"},
            )
            + wire_enum("TaxIdType", {"Other": "OTHER"})
            + wire_enum("Exchange", {"Nyse": "N"})
            + wire_enum("OrderClass", {"Simple": "simple"}),
        )

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("UNRESOLVED", err)

    def test_a_not_drift_key_that_names_no_enum_fails(self) -> None:
        """Its exemption is from a semantic guard, not an existence check."""
        self.script.NOT_DRIFT["Renamed"] = "a reason"

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("Renamed", err)

    def test_a_decided_key_that_names_no_enum_fails(self) -> None:
        """DECIDED suppresses a gap, which is the finding worth having.

        Only the key is checkable: the value names something the crate
        deliberately does not carry, so its absence is the entry working.
        """
        self.script.DECIDED[("Renamed", "value")] = "a reason"

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("Renamed", err)

    def test_a_not_drift_pair_that_now_matches_asks_to_be_rechecked(self) -> None:
        """The one state that would refute the claim outright."""
        self.write_spec(
            "trading.yaml", self.guarded_spec() + schema("Exchange", ["N"])
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("recheck", out)


class TestReportShape(TreeTest):
    """What the report says about its own coverage."""

    def setUp(self) -> None:
        super().setUp()
        self.base = self.satisfy_guards()

    def test_a_merged_vocabulary_with_a_gap_carries_the_caveat(self) -> None:
        """Surplus is dropped against a union, so the gap is only half an answer.

        Listing it bare, under a heading that reads like a full verdict, is the
        shape this whole report exists to remove.
        """
        self.write(
            "src/lib.rs",
            self.base + wire_enum("Colour", {"Red": "red", "Teal": "teal"}),
        )
        self.write_spec(
            "trading.yaml", self.guarded_spec() + schema("Colour", ["red", "blue"])
        )
        self.write_spec("broker.yaml", schema("Colour", ["red", "green"]))

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("merged vocabulary", out)
        # The untrustworthy half is suppressed, not reported as a finding.
        self.assertNotIn("teal", out)

    def test_an_unreadable_schema_is_not_called_missing(self) -> None:
        """"Add the pair to ALIASES" is a no-op when the name already matches."""
        self.write("src/lib.rs", self.base + wire_enum("CIPProvider", {"A": "alloy"}))
        self.write_spec(
            "trading.yaml",
            self.guarded_spec()
            + "    CIPProvider:\n      description: alloy\n      type: string\n",
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("carries no value list", out)

    def test_the_printed_buckets_are_the_actual_bucket_sizes(self) -> None:
        """Asserted as numbers, not as the presence of the word "compared".

        Four enums reach a comparison. `OrderClass` matches value for value;
        `TradeEvent` differs only by the two `CRATE_ONLY` values, so it is
        excepted rather than exact; `TaxIdType` has both a gap and a surplus
        and `Colour` a surplus, so both are flagged and the union counts them
        once each. `Exchange` has no schema here and is not compared at all.
        """
        self.write(
            "src/lib.rs", self.base + wire_enum("Colour", {"Red": "red", "Teal": "teal"})
        )
        self.write_spec(
            "trading.yaml", self.guarded_spec() + schema("Colour", ["red"])
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn(
            "1 exact + 1 with exceptions + 0 against a merged vocabulary + "
            "2 flagged = 4 compared",
            out,
        )


class TestReconciliation(unittest.TestCase):
    """The buckets have to account for every compared enum.

    Tested on the helper rather than through a report, because no input can
    currently make the report's buckets disagree — the branches partition the
    compared set. A test that could only drive it through `main` would assert
    nothing, which is what the first version of this test did.
    """

    def setUp(self) -> None:
        self.script = load_script()

    def test_matching_totals_produce_no_complaint(self) -> None:
        self.assertIsNone(self.script.reconcile(28, 28))

    def test_a_short_total_is_a_complaint(self) -> None:
        complaint = self.script.reconcile(27, 28)

        self.assertIsNotNone(complaint)
        self.assertIn("27", complaint)
        self.assertIn("28", complaint)

    def test_a_long_total_is_a_complaint(self) -> None:
        """Double-counting is as wrong as dropping one, and less obvious."""
        self.assertIsNotNone(self.script.reconcile(29, 28))

    def test_the_total_counts_every_bucket(self) -> None:
        self.assertEqual(
            self.script.bucket_total(["a"], ["b", "c"], [], {"d"}), 4
        )


if __name__ == "__main__":
    unittest.main()
