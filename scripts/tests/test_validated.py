"""What `validated.py` sees, and — mostly — what it fails to see.

Every test here drives a synthetic `src/` tree in a temporary directory, the
same way `test_setters.py` and `test_enum_drift.py` do. Exercising the parser
needs a dozen lines of Rust rather than the real crate, so the suite costs
nothing and joins `just check`.

# Why this file is longer than the script deserves

`validated.py` is a regex scanner over Rust, and its whole job is to answer one
question in the safe direction. A false *positive* is a nuisance: someone reads
the message and adds a derive that was not needed. A false *negative* is the
script printing "every request type is checked before it can be sent" while one
is not — which is indistinguishable from the state the `Validated` bound exists
to make impossible, and is worse than having no script, because it is an
assurance.

Eleven rounds of review found eight distinct false-pass shapes in this scanner:
a `#[derive(…)]` written inside a doc example, attribute bleed between two
declarations with no blank line, trailing `//` and `/* */` comments hiding an
item boundary, a rustfmt-wrapped `to_query` signature, `pub(crate)` visibility,
a newtype's field sitting on the declaration line, and two same-named types
sharing an answer. Each was found by hand, against a scratch copy of `src/`, and
then thrown away. This file is those probes made permanent — which is the same
argument the change under test makes about `request.validate()?`: a check that
depends on somebody remembering to run it is not a check.

So most of these assert on the *absence* of a report, and each says which shape
it is about.

`unittest` rather than pytest, matching the neighbouring suites: this repository
carries no Python dependencies and a verification script's tests should not
introduce the first.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "validated.py"


def load_script():
    """A fresh module object, so a test that patches a map cannot leak."""
    spec = importlib.util.spec_from_file_location("validated", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TreeTest(unittest.TestCase):
    """A synthetic `src/` tree, run through the real `main`."""

    def setUp(self) -> None:
        self.validated = load_script()
        self.validated.ADDITIONS = {}
        self.validated.EXCLUSIONS = {}
        self.validated.EXEMPT = {}
        self.root = pathlib.Path(tempfile.mkdtemp())
        self.src = self.root / "src"
        self.src.mkdir()
        # The blanket container impls live here in the real tree, and the rule
        # about nested rules excludes this path so that `Vec` is not read as a
        # type with rules of its own.
        (self.src / "types").mkdir()
        (self.src / "types" / "validated.rs").write_text(
            "impl<T: Validated> Validated for Vec<T> {\n"
            "    fn validate(&self) -> Result<()> {\n"
            "        Ok(())\n"
            "    }\n"
            "}\n"
        )

    def write(self, name: str, source: str) -> None:
        (self.src / name).write_text(source)

    def run_gate(self) -> tuple[int, str]:
        """The script's exit code and everything it printed.

        `ADDITIONS`, `EXCLUSIONS` and `EXEMPT` name types from the real crate,
        and each is checked for staleness — an entry matching no struct fails
        the run. A synthetic tree has none of them, so all three start empty
        here and a test that is about one of the maps sets it itself. The module
        is reloaded per test, so nothing leaks between them.
        """
        out, err = io.StringIO(), io.StringIO()
        saved = sys.argv
        sys.argv = ["validated.py", "--src", str(self.src)]
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = self.validated.main()
        finally:
            sys.argv = saved
        return code, out.getvalue() + err.getvalue()

    def assertReported(self, source: str, name: str, name_file: str = "requests.rs"):
        self.write(name_file, source)
        code, output = self.run_gate()
        self.assertEqual(code, 1, f"expected a failure, got a pass:\n{output}")
        self.assertIn(name, output)
        return output

    def assertClean(self, source: str, name_file: str = "requests.rs"):
        self.write(name_file, source)
        code, output = self.run_gate()
        self.assertEqual(code, 0, f"expected a pass, got a failure:\n{output}")
        return output


DERIVED = """\
/// A request with no rules.
#[derive(Debug, Clone, Serialize, Setters, Validated)]
pub struct GetThingRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""

WITH_RULES = """\
/// A request with rules.
#[derive(Debug, Clone, Serialize, Setters)]
pub struct CreateThingRequest {
    /// A name.
    pub name: String,
}

impl Validated for CreateThingRequest {
    fn validate(&self) -> Result<()> {
        Err(Error::InvalidRequest("no".to_owned()))
    }
}
"""


class TestTheBaseline(TreeTest):
    def test_a_derived_type_passes(self) -> None:
        self.assertClean(DERIVED)

    def test_a_hand_implemented_type_passes(self) -> None:
        self.assertClean(WITH_RULES)


class TestImplementsNeitherHalf(TreeTest):
    """The first of the four cases: a request type nothing sends yet.

    The compiler catches this at a call site that sends the type. Until there is
    one, only this does.
    """

    def test_neither_half_is_reported(self) -> None:
        source = DERIVED.replace(", Validated)]", ")]")
        self.assertReported(source, "GetThingRequest")

    def test_deriving_and_implementing_is_reported(self) -> None:
        source = WITH_RULES.replace("Setters)]", "Setters, Validated)]")
        output = self.assertReported(source, "CreateThingRequest")
        self.assertIn("E0119", output)


class TestFalsePassesFoundByReview(TreeTest):
    """One test per shape that once made the gate print a clean bill.

    Every one of these was a live false negative at some point during review,
    and each is here because it was found by hand rather than by anything that
    runs.
    """

    def test_a_derive_inside_a_doc_example_does_not_count(self) -> None:
        source = """\
/// A request whose documentation shows a derive list.
///
/// ```ignore
/// #[derive(Debug, Serialize, Validated)]
/// pub struct Example;
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct GetThingRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""
        self.assertReported(source, "GetThingRequest")

    def test_attributes_do_not_bleed_across_a_missing_blank_line(self) -> None:
        source = DERIVED + """\
#[derive(Debug, Clone, Serialize)]
pub struct GetOtherRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""
        self.assertReported(source, "GetOtherRequest")

    def test_a_trailing_line_comment_does_not_hide_an_item_boundary(self) -> None:
        source = """\
/// A unit request with a note beside it.
#[derive(Debug, Clone, Validated)]
pub struct GetFirstRequest; // nothing to send
#[derive(Debug, Clone, Serialize)]
pub struct GetSecondRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""
        self.assertReported(source, "GetSecondRequest")

    def test_a_trailing_block_comment_does_not_hide_an_item_boundary(self) -> None:
        source = """\
/// A unit request with a note beside it.
#[derive(Debug, Clone, Validated)]
pub struct GetFirstRequest; /* nothing to send */
#[derive(Debug, Clone, Serialize)]
pub struct GetSecondRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""
        self.assertReported(source, "GetSecondRequest")

    def test_a_comment_between_attributes_does_not_hide_the_derive(self) -> None:
        """The other direction: stopping at a comment loses a real derive.

        Skipping and stopping both close the doc-example hole; only skipping
        leaves a derive written below a comment visible.
        """
        source = """\
/// A request with no rules.
#[derive(Debug, Clone, Serialize, Validated)]
// an ordinary comment between attributes
#[non_exhaustive]
pub struct GetThingRequest {
    /// A filter.
    pub limit: Option<u32>,
}
"""
        self.assertClean(source)

    def test_a_declaration_inside_an_inline_mod_is_seen(self) -> None:
        source = """\
mod inner {
    /// A request nobody would think to declare here.
    #[derive(Debug, Clone, Serialize)]
    pub struct GetInnerRequest {
        /// A filter.
        pub limit: Option<u32>,
    }
}
"""
        self.assertReported(source, "GetInnerRequest")

    def test_a_hand_impl_in_another_file_is_found(self) -> None:
        """An `impl` does not have to sit beside its declaration.

        A per-file pass reported such a type as implementing neither half *and*
        exempted it from the `to_query` rule — two errors at once, in opposite
        directions.
        """
        self.write("requests.rs", WITH_RULES.split("impl Validated")[0])
        self.write(
            "elsewhere.rs",
            "impl Validated for CreateThingRequest {\n"
            "    fn validate(&self) -> Result<()> {\n"
            "        Ok(())\n"
            "    }\n"
            "}\n",
        )
        code, output = self.run_gate()
        self.assertEqual(code, 0, output)

    def test_a_nested_generic_bound_still_reads_as_a_hand_impl(self) -> None:
        source = """\
/// A generic request.
#[derive(Debug, Clone)]
pub struct BoundedRequest<T> {
    /// A name.
    pub name: T,
}

impl<T: Into<String> + Clone> Validated for BoundedRequest<T> {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}
"""
        self.assertClean(source)


class TestTheFlatteningRule(TreeTest):
    """The one case the compiler cannot see at all.

    Flattening a request into `Vec<(&str, String)>` hands the transport a value
    that satisfies the bound and carries no rules, so the request's own rules
    are skipped and everything still compiles. A type with rules must therefore
    both return a `Result` from `to_query` and propagate its own `validate`.
    """

    def with_to_query(self, signature: str, body: str) -> str:
        return WITH_RULES + f"""
impl CreateThingRequest {{
    {signature} {{
{body}
        Ok(query)
    }}
}}
"""

    GOOD = ("pub fn to_query(&self) -> Result<Vec<(&'static str, String)>>",
            "        self.validate()?;\n        let query = Vec::new();")

    def test_the_correct_shape_passes(self) -> None:
        self.assertClean(self.with_to_query(*self.GOOD))

    def test_an_infallible_to_query_is_reported(self) -> None:
        source = self.with_to_query(
            "pub fn to_query(&self) -> Vec<(&'static str, String)>",
            "        self.validate()?;\n        let query = Vec::new();",
        ).replace("        Ok(query)", "        query")
        self.assertReported(source, "CreateThingRequest")

    def test_a_result_that_never_asks_is_reported(self) -> None:
        """The signature alone proves nothing — `Ok(query)` satisfies it."""
        self.assertReported(
            self.with_to_query(self.GOOD[0], "        let query = Vec::new();"),
            "CreateThingRequest",
        )

    def test_a_discarded_validator_is_reported(self) -> None:
        self.assertReported(
            self.with_to_query(
                self.GOOD[0],
                "        let _ = self.validate();\n        let query = Vec::new();",
            ),
            "CreateThingRequest",
        )

    def test_a_commented_out_call_is_reported(self) -> None:
        self.assertReported(
            self.with_to_query(
                self.GOOD[0],
                "        // self.validate()?;\n        let query = Vec::new();",
            ),
            "CreateThingRequest",
        )

    def test_a_wrapped_signature_is_still_read(self) -> None:
        """rustfmt wraps this signature as soon as the return type grows."""
        source = WITH_RULES + """
impl CreateThingRequest {
    pub fn to_query(
        &self,
    ) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
"""
        self.assertReported(source, "CreateThingRequest")

    def test_visibility_is_not_part_of_the_hazard(self) -> None:
        """A `pub(crate)` flattener leaks the rules just as thoroughly."""
        source = WITH_RULES + """
impl CreateThingRequest {
    pub(crate) fn to_query(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
"""
        self.assertReported(source, "CreateThingRequest")


class TestTheNestedRule(TreeTest):
    """A parent that holds a type with rules and does not ask it.

    Both halves of the trait can be at fault, and the one live instance in the
    crate — `CreateAccountRequest` carrying `UploadDocument`s — was in the
    hand-written half, so checking only the derived half would have missed the
    bug the rule was written for.
    """

    def parent(self, derive: str, impl_body: str | None) -> str:
        source = WITH_RULES + f"""
/// A parent.
#[derive({derive})]
pub struct SendThingRequest {{
    /// The children.
    pub things: Vec<CreateThingRequest>,
}}
"""
        if impl_body is not None:
            source += f"""
impl Validated for SendThingRequest {{
    fn validate(&self) -> Result<()> {{
{impl_body}
    }}
}}
"""
        return source

    def test_a_derived_parent_is_reported(self) -> None:
        self.assertReported(
            self.parent("Debug, Clone, Serialize, Validated", None),
            "SendThingRequest",
        )

    def test_a_hand_written_parent_that_never_asks_is_reported(self) -> None:
        self.assertReported(
            self.parent("Debug, Clone, Serialize", "        Ok(())"),
            "SendThingRequest",
        )

    def test_a_hand_written_parent_that_asks_passes(self) -> None:
        self.assertClean(
            self.parent(
                "Debug, Clone, Serialize", "        self.things.validate()"
            )
        )

    def test_a_newtype_field_on_the_declaration_line_is_seen(self) -> None:
        """rustfmt keeps a tuple struct on one line, so this shape is stable."""
        source = WITH_RULES + """
/// A newtype parent.
#[derive(Debug, Clone, Serialize, Validated)]
pub struct WrappedThingRequest(pub CreateThingRequest);
"""
        self.assertReported(source, "WrappedThingRequest")

    def test_prose_is_not_mined_for_type_names(self) -> None:
        """A doc comment and a skip reason both name types. Neither is a field.

        `EventStreamRequest`'s own `#[setters(skip = "…")]` reason names the
        type, and reading it made the gate believe the struct held itself —
        which no struct can do by value, and which demands a delegation that
        cannot be written. Six real types were in that shape. The skip reason
        runs to several lines, so the attribute is tracked to its closing
        bracket rather than matched by its first line.
        """
        source = WITH_RULES + """
/// A parent whose prose mentions `CreateThingRequest` at length.
///
/// See `CreateThingRequest` for what the rules are.
#[derive(Debug, Clone, Serialize, Validated)]
pub struct SendThingRequest {
    /// A filter. Not a `CreateThingRequest`, whatever this sentence says.
    #[setters(skip = "a constructor already holds this name — see \\
                      `CreateThingRequest::new`, and two `pub fn` of one \\
                      name cannot share an impl")]
    pub limit: Option<u32>,
}
"""
        self.assertClean(source)

    def test_a_field_of_a_rules_free_type_is_not_reported(self) -> None:
        self.assertClean(
            DERIVED + """
/// A parent holding something with no rules.
#[derive(Debug, Clone, Serialize, Validated)]
pub struct SendThingRequest {
    /// The children.
    pub things: Vec<GetThingRequest>,
}
"""
        )

    def test_a_container_impl_is_not_read_as_a_type_with_rules(self) -> None:
        """`Vec` hand-implements the trait in `types/validated.rs`.

        Excluding those by the name rule was too narrow — a nested type with a
        plain noun for a name would have been invisible — so they are excluded
        by file. If that stopped working, every request holding a `Vec` of
        anything would be reported.
        """
        self.assertClean(DERIVED)


class TestTheExemptionsAreClaims(TreeTest):
    """`EXEMPT` names a type with rules that implements neither half.

    An entry covering nothing still reads as a settled decision, so both halves
    of the claim are checked: the type exists, and it still declares the
    inherent `validate` the exemption is about.
    """

    NESTED = """\
/// A nested type nothing sends on its own.
#[derive(Debug, Clone, Serialize, Setters)]
pub struct NestedThing {
    /// A share.
    pub percent: u32,
}

impl NestedThing {
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}
"""

    def exempt(self, source: str) -> tuple[int, str]:
        """`NestedThing` in scope and exempt, which is the real arrangement."""
        self.write("requests.rs", source)
        self.validated.ADDITIONS = {"NestedThing": "a nested thing"}
        self.validated.EXEMPT = {"NestedThing": "sent only inside a parent"}
        return self.run_gate()

    def test_an_intact_exemption_passes(self) -> None:
        code, output = self.exempt(DERIVED + self.NESTED)
        self.assertEqual(code, 0, output)

    def test_an_exemption_whose_type_lost_its_validate_fails(self) -> None:
        code, output = self.exempt(
            DERIVED + self.NESTED.replace("pub fn validate", "pub fn check")
        )
        self.assertEqual(code, 1, output)
        self.assertIn("no longer declares an inherent `validate`", output)

    def test_an_exemption_naming_no_type_fails(self) -> None:
        self.write("requests.rs", DERIVED)
        self.validated.EXEMPT = {"NoSuchThing": "gone"}
        code, output = self.run_gate()
        self.assertEqual(code, 1, output)
        self.assertIn("NoSuchThing", output)


class TestStripTrailingComment(unittest.TestCase):
    """The helper the item-boundary test depends on."""

    def setUp(self) -> None:
        self.strip = load_script().strip_trailing_comment

    def test_a_line_comment_is_removed(self) -> None:
        self.assertEqual(self.strip("pub struct A; // note").strip(), "pub struct A;")

    def test_a_block_comment_is_removed(self) -> None:
        self.assertEqual(self.strip("pub struct A; /* note */").strip(), "pub struct A;")

    def test_a_url_inside_a_string_survives(self) -> None:
        """Truncating here would hide code, which only ever costs a false alarm
        — but a gate that cries wolf gets ignored."""
        line = '    let base = "https://example.com/v2";'
        self.assertEqual(self.strip(line), line)


if __name__ == "__main__":
    unittest.main()
