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
import subprocess
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
            '#[cfg(all(test, feature = "trading"))]\nmod wire_tests;\n',
        )
        self.write("src/types/wire_tests.rs", wire_enum("Side", {"Buy": "buy"}))
        self.write("src/lib.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_an_any_cfg_ships_and_is_not_excluded(self) -> None:
        """`any(test, feature = "x")` compiles under x, so its enums ship.

        Tests are sufficient there, not necessary — the opposite of
        `all(test, …)`, and the same `test` token in both. Treating it as
        test-only drops a real wire enum with no diagnostic and exit 0.
        """
        self.write(
            "src/lib.rs",
            '#[cfg(any(test, feature = "test-utils"))]\nmod helpers;\n',
        )
        self.write(
            "src/lib/helpers.rs", wire_enum("ShipsUnderFeature", {"Live": "live"})
        )

        self.assertEqual(sorted(self.enums()), ["ShipsUnderFeature"])

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

    def test_a_nested_negation_is_not_excluded(self) -> None:
        """`not(all(test, …))` holds a paren pair a flat regex stopped at.

        It left a bare `test` behind and marked a module that ships everywhere
        *except* under test as test-only — dropping shipping code silently.
        """
        self.write(
            "src/lib.rs",
            '#[cfg(not(all(test, feature = "x")))]\nmod shipping {\n'
            + wire_enum("Shipping", {"Live": "live"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_visibility_qualified_test_module_is_excluded(self) -> None:
        """`pub mod` and `pub(crate) mod` are accepted shapes; pin them."""
        for declaration in ("pub mod", "pub(crate) mod"):
            with self.subTest(declaration=declaration):
                self.setUp()
                self.write(
                    "src/lib.rs",
                    wire_enum("Shipping", {"Live": "live"})
                    + f"\n#[cfg(test)]\n{declaration} tests {{\n"
                    + wire_enum("TestOnly", {"A": "a"})
                    + "}\n",
                )

                self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_visibility_qualified_test_file_module_is_excluded(self) -> None:
        self.write("src/types/mod.rs", "#[cfg(test)]\npub(crate) mod wire_tests;\n")
        self.write("src/types/wire_tests.rs", wire_enum("Side", {"Buy": "buy"}))
        self.write("src/lib.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_is_test_cfg_classifies_each_shape(self) -> None:
        """Asserted directly, so a shape with no fixture is still pinned.

        The question is whether the item can exist with `test` off, not whether
        the word appears: `all(test, …)` needs tests, `any(test, …)` merely
        accepts them and ships under its other arm.
        """
        for attribute, expected in (
            ("#[cfg(test)]", True),
            ('#[cfg(all(test, feature = "trading"))]', True),
            ("#[cfg(all(test, any(unix, windows)))]", True),
            ('#[cfg(any(test, feature = "x"))]', False),
            ("#[cfg(not(test))]", False),
            ('#[cfg(not(all(test, feature = "x")))]', False),
            ('#[cfg(all(any(test, unix), feature = "x"))]', False),
            ('#[cfg(feature = "test-utils")]', False),
            ('#[cfg(feature = "trading")]', False),
            # Blanked before parsing, so punctuation inside a feature name
            # cannot be read as predicate structure.
            ('#[cfg(feature = "a(test)b")]', False),
            ('#[cfg(feature = "a,test")]', False),
        ):
            with self.subTest(attribute=attribute):
                self.assertEqual(self.script.is_test_cfg(attribute), expected)

    def test_a_comment_between_the_attribute_and_the_mod_does_not_disarm_it(
        self,
    ) -> None:
        """Blank lines and comments there are ordinary formatting.

        Reading the two as strictly adjacent let a single comment hand a
        test-only enum back to the headline.
        """
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + "\n#[cfg(test)]\n// why these live here\n\nmod tests {\n"
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_comment_before_a_test_file_module_does_not_disarm_it(self) -> None:
        self.write(
            "src/types/mod.rs",
            "#[cfg(test)]\n// the macro's own behaviour\nmod wire_tests;\n",
        )
        self.write("src/types/wire_tests.rs", wire_enum("Side", {"Buy": "buy"}))
        self.write("src/lib.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_test_directory_module_takes_its_children_with_it(self) -> None:
        """"Nothing in them ships" has to mean the whole directory."""
        self.write("src/lib.rs", "#[cfg(test)]\nmod fixtures;\n")
        self.write("src/fixtures/mod.rs", "mod helper;\n")
        self.write("src/fixtures/helper.rs", wire_enum("TestOnlyDeep", {"A": "a"}))
        self.write("src/real.rs", wire_enum("Shipping", {"Live": "live"}))

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_the_skip_ends_when_the_test_module_closes(self) -> None:
        """A skip that never ends is an undercount, and reads as a clean run.

        The raw-string case pins that the region must not end *early*; this
        pins that it ends at all. Not live only because test modules sit at
        the end of a file by convention.
        """
        self.write(
            "src/lib.rs",
            "#[cfg(test)]\nmod tests {\n"
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n\n"
            + wire_enum("Shipping", {"Live": "live"}),
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_cfg_test_on_something_else_does_not_reach_a_later_mod(self) -> None:
        """The attribute applies to the next item, not to the next `mod`.

        Without a reset, a `#[cfg(test)]` on any other item would arm the skip
        and swallow whatever module came next — an undercount, the direction
        this script treats as the worse error.
        """
        self.write(
            "src/lib.rs",
            "#[cfg(test)]\nfn helper() {}\n\nmod shipping {\n"
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

    def test_a_one_line_valueless_enum_is_not_a_collision(self) -> None:
        """The multi-line decoy resets through the closing-brace path.

        A one-line `pub enum Foo { Bar }` never reaches it, so it exercises the
        `carried` guard on the end-of-file path instead — where a manufactured
        collision would cost a real wire enum its verdict.
        """
        self.write("src/a.rs", wire_enum("OrderStatus", {"New": "new"}))
        self.write("src/b.rs", "pub enum OrderStatus { Whatever }\n")

        self.assertEqual(self.collisions(), [])
        self.assertEqual(sorted(self.enums()), ["OrderStatus"])

    def test_a_valueless_enum_closing_beside_content_is_not_a_collision(self) -> None:
        """Reaches the `carried` guard on the end-of-file path.

        The brace shares a line with a variant name, so it matches neither the
        bare-closing-brace reset nor the same-line reset — `current` is still
        set at end of file with nothing carried, which is the state the guard
        is for.
        """
        self.write("src/a.rs", wire_enum("OrderStatus", {"New": "new"}))
        self.write("src/b.rs", "pub enum OrderStatus {\n    Whatever }\n")

        self.assertEqual(self.collisions(), [])
        self.assertEqual(sorted(self.enums()), ["OrderStatus"])

    def test_two_such_enums_are_not_a_collision_either(self) -> None:
        """The same state, reached at the *declaration* path instead.

        A second declaration arrives while the first is still open and empty.
        """
        self.write("src/a.rs", wire_enum("OrderStatus", {"New": "new"}))
        self.write(
            "src/b.rs",
            "pub enum OrderStatus {\n    Whatever }\n"
            "pub enum OrderStatus {\n    Another }\n",
        )

        self.assertEqual(self.collisions(), [])
        self.assertEqual(sorted(self.enums()), ["OrderStatus"])

    def test_an_ordinary_pub_enum_is_not_in_the_report_at_all(self) -> None:
        """A `pub enum` with no `Variant => "wire"` arms is not a wire enum.

        Deliberately under a name no `wire_enum!` shares: where the two collide
        the filter's effect is invisible, since the name is in the report
        either way. Without it the real crate's headline reads 137 rather than
        118, and the uncompared catalogue fills with types that never cross the
        wire — several of which share a name with a real spec schema, so one
        gaining an `enum:` list would report a non-wire type as missing every
        value Alpaca documents.
        """
        self.write("src/a.rs", wire_enum("Shipping", {"Live": "live"}))
        self.write(
            "src/b.rs",
            "pub enum Credentials {\n    Key(String),\n    None,\n}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_two_declarations_in_one_file_still_collide(self) -> None:
        """No same-file exemption: they merge just as invisibly."""
        self.write(
            "src/a.rs",
            wire_enum("Shared", {"A": "a"}) + wire_enum("Shared", {"B": "b"}),
        )

        self.assertEqual([name for name, _ in self.collisions()], ["Shared"])

    def test_a_collided_enum_gets_no_verdict_in_the_report(self) -> None:
        """The branch's central decision, asserted where a reader would see it.

        Each declaration fills the other's gaps, so comparing the union answers
        a question nobody asked: the reproduction on this branch was deleting a
        value from one `TransferDirection` and watching it still report as
        agreeing exactly. Checking this at `crate_enums` alone left every
        mutation of the *reporting* path green.
        """
        self.write("src/lib.rs", self.satisfy_guards())
        self.write("src/a.rs", wire_enum("Shared", {"A": "a"}))
        self.write("src/b.rs", wire_enum("Shared", {"B": "b"}))
        self.write_spec(
            "trading.yaml", self.guarded_spec() + schema("Shared", ["a", "b"])
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        # Named as skipped, and named nowhere that implies a verdict.
        self.assertIn("Declared more than once", out)
        self.assertIn("1 of them", out)
        self.assertNotIn("Shared,", out)
        self.assertNotIn(" Shared\n", out)
        for verdict in ("agree exactly: ", "agree apart from"):
            line = next((l for l in out.splitlines() if verdict in l), "")
            self.assertNotIn("Shared", line)


class TestVariantParsing(TreeTest):
    def test_a_brace_in_an_ordinary_string_does_not_break_the_count(self) -> None:
        """The plain-string sibling of the raw-string case.

        This one fails loudly rather than miscounting — an unbalanced depth
        raises — but the exclusion is only as good as the blanking under it.
        """
        self.write(
            "src/lib.rs",
            wire_enum("Shipping", {"Live": "live"})
            + '\n#[cfg(test)]\nmod tests {\n    let s = "a lone { brace";\n'
            + wire_enum("TestOnly", {"A": "a"})
            + "}\n",
        )

        self.assertEqual(sorted(self.enums()), ["Shipping"])

    def test_a_closing_brace_with_a_comment_still_closes_the_block(self) -> None:
        """Otherwise every later `Ident => "wire",` is filed as one of its values.

        An ordinary match arm elsewhere in the file has that shape.
        """
        self.write(
            "src/a.rs",
            """
            wire_enum! {
                pub enum Shipping {
                    Live => "live",
                } // closes the enum
                fn route() {
                    match x {
                        Sneaky => "sneaky",
                    }
                }
            }
            """,
        )

        # Not `["live", "sneaky"]`: the match arm is not one of its values.
        self.assertEqual(self.enums()["Shipping"], ["live"])

    def test_an_empty_enum_does_not_adopt_later_match_arms(self) -> None:
        """`pub enum Never {}` opens and closes on one line.

        It never meets the closing-brace reset, so it stayed open and filed
        every later `Ident => "wire",` in the file as one of its values — a
        phantom enum with fabricated values, which would then be compared
        against a schema of that name. rustfmt leaves the shape alone.
        """
        self.write(
            "src/a.rs",
            "pub enum Never {}\n\n"
            "fn route(x: Thing) -> &str {\n    match x {\n"
            '        Alpha => "alpha",\n        Beta => "beta",\n    }\n}\n',
        )

        self.assertEqual(self.enums(), {})

    def test_a_block_closed_on_one_line_still_declares_its_enum(self) -> None:
        """`    }}` never matches the closing-brace reset, so the block ends at EOF."""
        self.write(
            "src/a.rs",
            'wire_enum! {\n    pub enum Shared {\n        A => "a",\n    }}\n',
        )
        self.write(
            "src/b.rs",
            'wire_enum! {\n    pub enum Shared {\n        B => "b",\n    }}\n',
        )

        self.assertEqual([name for name, _ in self.collisions()], ["Shared"])

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

    def test_an_unreadable_schema_key_does_not_capture_the_next_value_list(
        self,
    ) -> None:
        """A key this parser cannot name must not lend its `enum:` to a neighbour.

        Attributing a value list to the wrong schema is worse than skipping it:
        it can manufacture a disagreement that does not exist.
        """
        self.write_spec(
            "broker.yaml",
            schema("Colour", ["red"])
            + "    Odd-Name:\n      enum:\n        - green\n        - blue\n",
        )

        found, merged, _ = self.script.spec_enums(self.root / "specs")

        self.assertEqual(found["Colour"], {"red"})
        self.assertEqual(merged, {})

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


class TestNotesStopPrinting(TreeTest):
    """The quieter half of the staleness contract.

    A guard that fails the run is easy to notice. These entries instead stop
    printing once the state they describe no longer holds — and a note that
    keeps printing asserts, on every run, something that has stopped being
    true.
    """

    def test_a_crate_only_note_prints_while_the_value_is_still_surplus(self) -> None:
        self.write("src/lib.rs", self.satisfy_guards())
        self.write_spec("trading.yaml", self.guarded_spec())

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("TradeEvent restated: carried deliberately", out)

    def test_a_crate_only_note_stops_once_the_spec_lists_the_value(self) -> None:
        """Alpaca documenting `restated` ends the disagreement the note describes."""
        self.write("src/lib.rs", self.satisfy_guards())
        self.write_spec(
            "trading.yaml",
            schema("TradeUpdateEventType", ["fill", "restated"])
            + schema("TaxIdType", ["ARG_AG_CUIT"])
            + schema("OrderClass", ["simple"]),
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertNotIn("restated: carried deliberately", out)

    def test_a_decided_note_stops_once_the_crate_carries_the_value(self) -> None:
        self.write(
            "src/lib.rs",
            wire_enum(
                "TradeEvent",
                {"Restated": "restated", "Held": "held", "Fill": "fill"},
            )
            + wire_enum("TaxIdType", {"ArgCuit": "ARG_AR_CUIT"})
            + wire_enum("Exchange", {"Nyse": "N"})
            + wire_enum("OrderClass", {"Simple": "simple", "Empty": ""}),
        )
        self.write_spec(
            "trading.yaml",
            schema("TradeUpdateEventType", ["fill"])
            + schema("TaxIdType", ["ARG_AG_CUIT"])
            + schema("OrderClass", ["simple", '""']),
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertNotIn("decided against", out)

    def test_an_unresolved_note_prints_against_the_gap_it_explains(self) -> None:
        self.write("src/lib.rs", self.satisfy_guards())
        self.write_spec("trading.yaml", self.guarded_spec())

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("note: the crate carries ARG_AR_CUIT", out)
        self.assertIn("ARG_AR_CUIT is not a de-documented value", out)

    def test_an_unresolved_note_stops_once_the_spec_adopts_the_spelling(self) -> None:
        """Then the pair is resolved and there is nothing left to explain.

        The enum keeps a *different* gap, so it still reaches the missing list
        and the note still has somewhere to print. Dropping the gap instead
        would take the whole entry out of the report and the guard would go
        unexercised — the note would be absent either way.
        """
        self.write("src/lib.rs", self.satisfy_guards())
        self.write_spec(
            "trading.yaml",
            schema("TradeUpdateEventType", ["fill"])
            + schema("TaxIdType", ["ARG_AR_CUIT", "SOMETHING_ELSE"])
            + schema("OrderClass", ["simple"]),
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        # Still reported, so the note had a gap to attach itself to.
        self.assertIn("SOMETHING_ELSE", out)
        self.assertNotIn("note: the crate carries", out)
        self.assertNotIn("is not a de-documented value", out)

    def test_the_surplus_side_note_stops_on_its_own_terms(self) -> None:
        """The `extra` block's guard, which the missing-side test cannot reach.

        Adopting the spelling empties the surplus too, so that test proves
        nothing here. `TaxIdType` keeps a second, unrelated surplus value so
        the block still prints and the note still has somewhere to go.
        """
        self.write(
            "src/lib.rs",
            wire_enum(
                "TradeEvent",
                {"Restated": "restated", "Held": "held", "Fill": "fill"},
            )
            + wire_enum("TaxIdType", {"ArgCuit": "ARG_AR_CUIT", "Other": "OTHER"})
            + wire_enum("Exchange", {"Nyse": "N"})
            + wire_enum("OrderClass", {"Simple": "simple"}),
        )
        self.write_spec(
            "trading.yaml",
            schema("TradeUpdateEventType", ["fill"])
            + schema("TaxIdType", ["ARG_AR_CUIT"])
            + schema("OrderClass", ["simple"]),
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        # The block prints, for the value that really is still surplus.
        self.assertIn("TaxIdType: OTHER", out)
        self.assertNotIn("is not a de-documented value", out)


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
        # The marker itself, not the substring "merged vocabulary" — that also
        # occurs in the "N against a merged vocabulary" summary line, which is
        # printed on every run and would satisfy the assertion regardless.
        self.assertIn("Colour  <- merged vocabulary; surplus not checked", out)
        # The untrustworthy half is suppressed, not reported as a finding.
        self.assertNotIn("teal", out)

    def test_a_decided_value_is_suppressed_from_the_gap_list(self) -> None:
        """The map's entire purpose, and the fixture had been hiding it.

        `guarded_spec` gives `OrderClass` exactly what the crate carries, so no
        `DECIDED` value was ever both in the spec and absent from the crate —
        the one state the suppression exists for. With the empty string in the
        schema, `OrderClass` must stay out of the gap list and be counted as
        agreeing apart from values recorded below.
        """
        self.write("src/lib.rs", self.base)
        self.write_spec(
            "trading.yaml",
            schema("TradeUpdateEventType", ["fill"])
            + schema("TaxIdType", ["ARG_AG_CUIT"])
            + '    OrderClass:\n      enum:\n        - simple\n        - ""\n',
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("agree apart from values recorded below: OrderClass", out)
        self.assertIn('OrderClass "": decided against', out)
        gaps = out.split("In the spec, not in the crate")[-1]
        self.assertNotIn("OrderClass", gaps)

    def test_an_empty_string_gap_is_named_rather_than_printed_bare(self) -> None:
        """Alpaca documents `simple (or "")` for OrderClass, so it is a real value.

        Printed bare it looks like a blank line rather than a finding. Under a
        different enum here, because `DECIDED` suppresses the `OrderClass` one
        before it reaches the missing list.
        """
        self.write("src/lib.rs", self.base + wire_enum("Shape", {"Round": "round"}))
        self.write_spec(
            "trading.yaml",
            self.guarded_spec()
            + '    Shape:\n      enum:\n        - round\n        - ""\n',
        )

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn('"" (the empty string)', out)

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
        self.assertIn("1 has a schema of that", out)
        self.assertIn("no value list this report can read", out)
        self.assertIn("CIPProvider (1 values)", out)

    def test_an_enum_with_no_schema_is_catalogued_and_counted(self) -> None:
        """The larger half of the coverage statement, and it had no assertion.

        "The report now lists what it could not check" is the headline claim;
        the block making it was deletable with the suite still green.
        """
        self.write(
            "src/lib.rs",
            self.base
            + wire_enum("Unknowable", {"A": "a", "B": "b"})
            + wire_enum("AlsoUnknowable", {"C": "c"}),
        )
        self.write_spec("trading.yaml", self.guarded_spec())

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("3 with no spec schema found", out)
        self.assertIn("Unknowable (2 values)", out)
        self.assertIn("AlsoUnknowable (1 values)", out)
        self.assertIn("Exchange (1 values)", out)

    def test_a_collided_name_is_counted_by_distinct_values(self) -> None:
        """Both declarations accumulate under one key, so a raw length doubles.

        A wrong count, in a report whose subject is counts being wrong.
        """
        self.write("src/lib.rs", self.base)
        self.write("src/a.rs", wire_enum("Shared", {"A": "a", "B": "b"}))
        self.write("src/b.rs", wire_enum("Shared", {"A2": "a", "C": "c"}))
        self.write_spec("trading.yaml", self.guarded_spec())

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        # Three distinct wire values across the two declarations, not four.
        self.assertIn("Shared (3 values) — and declared more than once", out)

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


class TestTheRunner(unittest.TestCase):
    """The guard that exists because `unittest discover` passes on nothing.

    Tested by running it, since its whole subject is what the exit code says
    when there is nothing to say.
    """

    RUNNER = pathlib.Path(__file__).resolve().parent / "run.py"

    def test_an_empty_directory_is_a_failure_not_a_pass(self) -> None:
        with tempfile.TemporaryDirectory() as empty:
            copied = pathlib.Path(empty) / "run.py"
            copied.write_bytes(self.RUNNER.read_bytes())

            finished = subprocess.run(
                [sys.executable, str(copied)], capture_output=True, text=True
            )

        self.assertEqual(finished.returncode, 1)
        self.assertIn("no tests discovered", finished.stderr)

    def test_a_directory_with_tests_in_it_runs_them(self) -> None:
        """Deliberately a throwaway suite, not the real one.

        Pointing the runner at this directory would have it discover the test
        you are reading and recurse.
        """
        with tempfile.TemporaryDirectory() as somewhere:
            here = pathlib.Path(somewhere)
            (here / "run.py").write_bytes(self.RUNNER.read_bytes())
            (here / "test_stub.py").write_text(
                "import unittest\n\n\n"
                "class Stub(unittest.TestCase):\n"
                "    def test_passes(self):\n"
                "        self.assertTrue(True)\n"
            )

            finished = subprocess.run(
                [sys.executable, str(here / "run.py")],
                capture_output=True,
                text=True,
            )

        self.assertEqual(finished.returncode, 0, finished.stderr)
        self.assertIn("Ran 1 test", finished.stderr)


class TestMissingDirectories(TreeTest):
    """Neither path is a crash, and neither is a silent pass."""

    def test_a_missing_specs_directory_says_to_fetch_them(self) -> None:
        self.write("src/lib.rs", self.satisfy_guards())
        (self.root / "specs").rmdir()

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("just specs", err)

    def test_a_missing_src_directory_says_where_to_run_from(self) -> None:
        self.write_spec("trading.yaml", self.guarded_spec())
        (self.root / "src").rmdir()

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("repository root", err)


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
