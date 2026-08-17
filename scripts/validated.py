#!/usr/bin/env python3
"""Which request types could still be sent without their rules being checked.

`RestClient` bounds every body and every query by `Validated` and calls it
before building the HTTP request, so the ordinary way of skipping validation —
adding a route and forgetting the `request.validate()?` line — is now a compile
error. That is the compiler's half, and it is the larger half.

# What this checks, and what the compiler checks

The compiler catches a type with no `Validated` impl *at the call site that
sends it*. Four things sit outside that:

1. **A request type that nothing sends yet.** It compiles, ships, and the
   omission surfaces only when someone wires it to a route — and at that point
   the fix looks like an unrelated change to a file they were not editing.
   Requiring the impl at declaration time keeps the cost where the decision is.

2. **Deriving and hand-implementing.** rustc catches this as `E0119`, and this
   script says it in the vocabulary of the rule rather than of coherence. It is
   here because a reader of the report should see the whole rule in one place.

3. **An infallible `to_query`.** This one the compiler cannot see at all, and it
   is the reason the script is a gate rather than a report. Flattening a request
   into `Vec<(&str, String)>` produces a value that satisfies the bound and
   carries no rules, so the request's own rules are skipped and everything still
   compiles. `GetCorporateAnnouncementsRequest` was in exactly that shape: a
   90-day window checked by a `validate` the transport would never reach.
   A type that hand-implements `Validated` must therefore both return a
   `Result` from `to_query` and propagate its own `validate` inside it. Neither
   half is enough alone: `Ok(query)` satisfies the signature and asks nothing,
   and `let _ = self.validate();` asks and throws the answer away. What is
   matched is the literal `validate()?`, which is a check on the *shape* of the
   code rather than a proof about it — a deliberate limit, and the reason
   `GetCorporateAnnouncementsRequest` also has a wiremock test that fails if its
   window stops being enforced. `ClosePositionRequest`, the only other type with
   a `to_query`, has no rules to test.

4. **A parent that does not ask a field whose type has rules.** Either half of
   the trait can be at fault. A derived no-op says "this type has no rules",
   which is false if one of its fields is a type that does; and a hand-written
   impl can simply not mention the field. Either way the transport checks the
   parent and the parent checks nothing.

   `CreateAccountRequest` was in the second of those — it hand-implements, and
   carried `UploadDocument`s it never asked, so `create_account` would send a
   document `upload_documents_to_account` refuses. It was found by reading,
   which is why the rule exists. The derived half is prospective: no type is in
   it today, and `CorporateActionEventsRequest` enters it the moment
   `EventStreamRequest` gains a validator.

# What counts as a request

The `*Request*` name rule, as in `setters.py`, with the same kind of `ADDITIONS`
and `EXCLUSIONS` maps for what the name rule cannot see. Both are claims, and an
entry that stops matching a struct fails the run rather than going quiet.

`EXEMPT` is this script's own map, and the interesting one: a type with real
rules that deliberately does **not** implement `Validated`, because it is never
sent on its own and deriving the no-op would let it be. The entry is checked for
staleness the same way — the type must still exist and must still declare the
inherent `validate` the exemption is about. What no script can check is that the
parent still calls it; that lives in a comment beside the method.

Usage:
    python3 scripts/validated.py [--src src] [--report]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

# `pub struct Name` / `pub enum Name`, at any `pub` visibility and any
# indentation.
#
# Wider than what `src/` contains today: a request type declared inside an
# inline `mod` block, or as `pub(super)`, is one nobody would think to write —
# and is exactly the shape that would slip past a tighter pattern in silence,
# which is the only direction of error this script cannot afford.
#
# A module-private `struct FooRequest` is still invisible, and that is the
# floor rather than an oversight: the transport is public, so anything it can
# be handed is reachable from outside this file.
DECL = re.compile(r"^\s*pub(?:\([^)]*\))? (?:struct|enum) (\w+)\b")

# `Validated` inside a `#[derive(…)]`. Matched against the attribute block above
# the declaration with its doc comments stripped, so a derive list rustfmt has
# split across lines still hits and a `#[derive(…)]` written inside a doc
# *example* does not. That second half is not hypothetical: a documented type
# whose own doc shows a derive list would otherwise be counted as deriving
# whatever the example says, which is a false pass on the one question this
# script exists to answer.
DERIVES_VALIDATED = re.compile(r"#\[derive\([^)]*\bValidated\b[^)]*\)\]", re.S)

# `impl Validated for Name`, with or without generics on either side.
#
# The generic list is `<.*>` — greedy, so it runs to the *last* `>` on the line
# — rather than `<[^>]*>`, which stops at the first and so misses
# `impl<T: Into<String>> Validated for Foo<T>`. Not balanced matching, which a
# regex cannot do; greedy is enough because the trait name that follows pins
# where the list has to end. Missing one is not silent — the type is reported as
# implementing neither half — but the remedy it then suggests is adding a
# derive, which would be `E0119`.
HAND_IMPL = re.compile(r"^impl(?:<.*>)? Validated for (\w+)")

# `impl Name {`, opening an inherent block. Greedy in both generic positions
# for the reason `HAND_IMPL` is: `<[^>]*>` stops at the first `>`, so it misses
# `impl<T: Into<String>> Foo<T> {` — and every `to_query` inside such a block
# with it.
#
# The brace has to be on the signature line, which is a real limit: an `impl`
# header wrapped over two lines hides the whole block, `to_query` included.
# `just check` runs `cargo fmt --check` before this script and rustfmt does not
# produce that shape, so the gate is standing behind a formatter rather than
# behind its own pattern. Worth knowing if the two ever come apart.
INHERENT_IMPL = re.compile(r"^impl(?:<.*>)? (\w+)(?:<.*>)? \{$")

# `fn to_query(…) -> …` at any visibility, inside an inherent block. Matched
# against the block's text with newlines collapsed, so a signature rustfmt has
# wrapped over several lines — which happens as soon as the return type grows —
# is still seen. Matching line-by-line missed exactly that case.
#
# Visibility is not part of the pattern because it is not part of the hazard: a
# `pub(crate) fn to_query` feeding a client method leaks the request's rules
# just as thoroughly as a public one, and is if anything likelier, since an
# internal helper gets less scrutiny.
#
# The event streams' flatteners are named `query` and are deliberately **not**
# matched here. They are not a way past the bound: `sse::subscribe` takes the
# filter itself and validates before flattening, so their output is produced
# after the check rather than in place of it. Adding the name would demand a
# `Result` from three methods that cannot skip anything.
#
# One name, then, and the rule's limit is wider than that name suggests. A
# flattener called `into_query` is invisible to it; so is one inlined at the
# call site — `rest.get(path, &vec![("start", filter.start.to_string())])`
# satisfies the bound through the `Vec` impl with no method to match at all, and
# is the likelier next instance of the hazard. What this rule enforces is a
# convention: flattening lives in a method called `to_query`, and that method
# asks. Worth stating, because the alternative reading — that any flattening is
# caught — is what would let the next one through.
TO_QUERY = re.compile(
    r"\b(?:pub(?:\([^)]*\))? )?fn to_query\s*\([^)]*\)\s*->\s*([^{]+?)\s*\{"
)

# `    pub field: Type,` — a struct field and the whole of its type text.
FIELD = re.compile(r"^\s+pub(?:\([^)]*\))? \w+: (.+),$")

# The identifiers inside a field's type, so `Option<Vec<UploadDocument>>` yields
# `Option`, `Vec` and `UploadDocument`. Crude on purpose: it only has to find
# the names, and a name that is not a request type matches nothing.
TYPE_NAMES = re.compile(r"\b([A-Z]\w*)")

# `fn validate(&self) -> …`, inside an inherent block, at any visibility.
INHERENT_VALIDATE = re.compile(r"\b(?:pub(?:\([^)]*\))? )?fn validate\s*\(\s*&self\s*\)")

# Types the `*Request*` name rule cannot see that the transport is nevertheless
# handed. Naming a type here is a claim about that, and not quite the claim
# `setters.py`'s map of the same name makes: that one asks which types a
# *caller* constructs, which is why this one has two entries it does not.
# `TapeQuery` is `pub(crate)` — no caller can build one — and `UploadDocument`
# is reached as a slice element rather than by name. Both are sent, which is the
# question here.
ADDITIONS: dict[str, str] = {
    "UpdatableIdentity": "the PATCH body for an account's identity",
    "UpdatableContact": "the PATCH body for an account's contact details",
    "Contact": "built for account creation; also a field of the account response",
    "Identity": "built for account creation; also a field of the account response",
    "Disclosures": "built for account creation; also a field of the account response",
    "TrustedContact": "built for account creation; also a field of the account response",
    "Agreement": "built for account creation, one per agreement signed",
    "W8BenDocument": "built for a non-US account's tax documentation",
    "Weight": "a leg of a rebalancing portfolio",
    "RebalancingCondition": "a trigger on a rebalancing subscription",
    "CIPInfo": "the CIP payload uploaded for an account",
    "TransmitterInfo": "the travel-rule payload on a crypto transfer",
    "AccountConfiguration": "read-modify-write: fetched, adjusted, sent back",
    "TokenizationMintCallback": "the callback body a caller posts back",
    "KycResults": "built inside `Disclosures` for a manually-approved account",
    "TapeQuery": "the `tape` parameter the stock condition route requires",
    "UploadDocument": "one entry of the `&[UploadDocument]` slice sent as a body",
}

# Named like a request, and not one. Naming a type here is a claim that no
# caller ever builds it.
EXCLUSIONS: dict[str, str] = {
    "MarketDataRequest": (
        "not a request body — it is the internal description of a paginated "
        "route (path, page limit, how to unwrap the payload) that "
        "`get_marketdata` reads, and never itself serialized"
    ),
    "OptionsApprovalRequester": (
        "not a request at all — a `wire_enum!` naming *who* asked for an options "
        "level, and only ever a field of the approval response. It matches the "
        "name rule on the substring in `Request`er"
    ),
    "TokenizationRequest": (
        "a response record — it appears only in `Result<…>` return position "
        "across both clients, and a setter on it would serve nobody"
    ),
}

# Types with real rules that deliberately implement neither half of the trait.
#
# Each is nested inside a request that carries it to the wire, and that parent's
# own `Validated` impl calls this one's inherent `validate`. Deriving the no-op
# here would be worse than saying nothing: it would let the type be passed to
# the transport directly and checked by nothing, which is the failure the trait
# exists to remove. Implementing it by hand would work, but the parent would
# then have two plausible ways to reach the same rules.
# Where the container and infrastructure impls live — `Vec<T>`, `[T]`, `&T`,
# `Raw<T>`, `Empty`, `()`. They are hand-written in the same syntactic shape as
# a real validator, so the rule about nested rules has to know not to count
# them; without this every request holding a `Vec` of anything is reported.
#
# Matched on the trailing path rather than the whole of it, so that `--src`
# pointed anywhere — an absolute path, another checkout — still finds it. As a
# fixed string it worked for `--src src` and silently reported twenty-two false
# positives for everything else.
BLANKET_IMPLS = "types/validated.rs"

EXEMPT: dict[str, str] = {
    "W8BenDocument": (
        "reaches the wire only inside `UploadW8BenDocumentRequest`, whose "
        "`Validated` impl calls this one's inherent `validate`"
    ),
    "Weight": (
        "a line of a portfolio, reaching the wire only inside "
        "`CreatePortfolioRequest`, `UpdatePortfolioRequest` and "
        "`CreateRunRequest`, each of which validates every weight it carries"
    ),
}


def in_scope(name: str) -> bool:
    """Whether a type is one a caller builds and the transport may be handed."""
    if name in EXCLUSIONS:
        return False
    return "Request" in name or name in ADDITIONS


# A line that cannot be part of the attribute block above a declaration: it
# either closes the previous item (`}`), ends it (`;`), or opens a body (`{`).
# A multi-line `#[derive(` list ends in `)]` and its inner lines end in a comma
# or nothing, so none of them trip this.
ENDS_PREVIOUS_ITEM = ("{", "}", ";")


# `/* … */` on a single line, which the boundary test has to see through for
# the same reason it sees through `//`.
BLOCK_COMMENT = re.compile(r"/\*.*?\*/")


def strip_trailing_comment(line: str) -> str:
    """`line` with any comment removed, ignoring `//` inside a string.

    The boundary test below is on the last character, so `pub struct Empty;`
    stops the walk and `pub struct Empty;  // no query or body` did not — the
    attribute block then ran on upwards into *that* item's `#[derive(…)]` and
    the next declaration inherited it. A false pass, and reachable: a unit
    struct with a trailing note is an ordinary thing to write.

    Both syntaxes, because closing one and not the other leaves the same hole
    wearing a different spelling: `pub struct ARequest; /* a note */` did not
    end the walk either.

    The string check matters because a URL in a code line contains `//`;
    truncating there could only hide text, which turns a pass into a failure
    rather than the reverse, but a gate that cries wolf gets ignored.
    """
    line = BLOCK_COMMENT.sub("", line)
    quotes = 0
    for index in range(len(line) - 1):
        if line[index] == '"' and (index == 0 or line[index - 1] != "\\"):
            quotes += 1
        elif line[index : index + 2] == "//" and quotes % 2 == 0:
            return line[:index]
    return line


def attribute_block(lines: list[str], start: int) -> str:
    """The run of attribute lines above `start`, doc comments removed.

    Two things this must not do, both of which are false *passes* — a real
    violation reported as fine, which is the only direction of error this script
    cannot afford.

    It must not read the *previous* item's attributes. Items in this crate are
    separated by a blank line, and the first version relied on that alone: two
    adjacent declarations with no blank line between them meant the second
    inherited the first's `#[derive(…)]`. `rustfmt` does not insert the blank
    line, so nothing else would have said so. The walk therefore also stops at
    any line that closes, ends or opens an item.

    It must not read doc comments. A type documented with a fenced Rust example
    showing a `#[derive(…)]` list would otherwise be read as carrying whatever
    the example says.
    """
    above = []
    index = start - 1
    while index >= 0:
        raw = lines[index].strip()
        # Comment lines are stepped over, not stopped at. Stopping would also
        # work for the ordinary layout — docs sit above the attributes — but it
        # would truncate the block at a comment written *between* two
        # attributes, losing a derive that is really there.
        if raw.startswith(("///", "//!", "//")):
            index -= 1
            continue
        stripped = strip_trailing_comment(lines[index]).strip()
        if not stripped or stripped.endswith(ENDS_PREVIOUS_ITEM):
            break
        above.append(lines[index])
        index -= 1
    return "\n".join(reversed(above))


def block_body(lines: list[str], start: int) -> list[str]:
    """The lines of the braced block opening at `start`."""
    depth = 0
    body = []
    for index in range(start, len(lines)):
        depth += lines[index].count("{") - lines[index].count("}")
        body.append(lines[index])
        if depth == 0 and index > start:
            break
    return body


class Found:
    """Everything one file says about its request types."""

    def __init__(self) -> None:
        self.declared: dict[str, bool] = {}  # name -> derives Validated
        self.hand: set[str] = set()
        # name -> whether its `impl Validated` body calls `validate` on anything
        self.hand_delegates: dict[str, bool] = {}
        # name -> (the return type is a `Result`, the body calls `validate`)
        self.to_query: dict[str, tuple[bool, bool]] = {}
        self.inherent_validate: set[str] = set()
        self.fields: dict[str, set[str]] = {}  # name -> type names of its fields


def fn_body(text: str, start: int) -> str:
    """The braced body beginning just after the `{` at `start`.

    `text` is a whole `impl` block joined onto one line, so a plain brace count
    finds the end of one method within it. Needed because the `to_query` rule
    has to look at that method specifically: an `impl` block usually holds
    others, and a `validate()` call in a neighbour would answer the question
    with the wrong method's body.
    """
    depth = 1
    for index in range(start, len(text)):
        depth += (text[index] == "{") - (text[index] == "}")
        if depth == 0:
            return text[start:index]
    return text[start:]


def field_type_names(lines: list[str], start: int, declared: str) -> set[str]:
    """Every capitalised identifier appearing in the field types of one item.

    Read from the declaration line down to the closing brace, which is enough
    for the one question asked of it: does this type hold another type that has
    rules? Enum variant payloads are picked up the same way, since the pattern
    is on the type text rather than on the `pub` that precedes a struct field —
    for a variant the names come from the whole line.

    The declaration line is included, and it has to be: a newtype puts its whole
    field there — `pub struct WrappedOrderRequest(pub OrderRequest);` — and
    rustfmt keeps it on one line, so starting below it made that shape a
    permanent blind spot rather than an unlikely one.

    What is dropped from that line is the *declaration itself*, by cutting at
    the item's own name, rather than every later occurrence of that name.
    Discarding the name outright was wrong in the one place it matters: the
    broker's `OrderRequest` holds a `crate::trading::OrderRequest`, so the
    parent and the child are the same string, and dropping it hid the only
    same-named nesting in the crate.
    """
    names: set[str] = set()
    attribute_depth = 0
    for index, line in enumerate(lines[start:]):
        if line.startswith(("}", ")")) or (index and DECL.match(line)):
            break
        stripped = line.strip()
        # Doc comments and attributes are prose and configuration, not types.
        # Reading them mined every capitalised word in a field's documentation
        # and in its `#[setters(skip = "…")]` reason — and those reasons name
        # types, so `EventStreamRequest` was recorded as holding itself and the
        # gate demanded a delegation that cannot be written. Six types were in
        # that shape and every one would have failed the run on correct code.
        #
        # An attribute is tracked to its closing bracket rather than matched by
        # its first line, because a skip reason runs to several lines and the
        # continuations start with a bare word.
        if attribute_depth:
            attribute_depth += stripped.count("[") - stripped.count("]")
            continue
        if stripped.startswith(("///", "//!", "//")):
            continue
        if stripped.startswith("#["):
            attribute_depth = stripped.count("[") - stripped.count("]")
            continue
        if index == 0:
            # Everything after `struct Name` / `enum Name`, which for a newtype
            # is the field and for anything else is `{` or `;`.
            head = line.split(declared, 1)
            text = head[1] if len(head) == 2 else ""
        else:
            field = FIELD.match(line)
            text = field.group(1) if field else line
        names.update(TYPE_NAMES.findall(text))
    return names


def parse(path: pathlib.Path) -> Found:
    found = Found()
    lines = path.read_text().splitlines()

    for index, line in enumerate(lines):
        declaration = DECL.match(line)
        if declaration:
            name = declaration.group(1)
            found.declared[name] = bool(
                DERIVES_VALIDATED.search(attribute_block(lines, index))
            )
            found.fields[name] = field_type_names(lines, index, name)
            continue

        hand = HAND_IMPL.match(line)
        if hand:
            found.hand.add(hand.group(1))
            found.hand_delegates[hand.group(1)] = ".validate()" in " ".join(
                part.strip()
                for part in block_body(lines, index)
                if not part.lstrip().startswith(("///", "//"))
            )
            continue

        inherent = INHERENT_IMPL.match(line)
        if inherent:
            name = inherent.group(1)
            # Joined and whitespace-collapsed, so a signature rustfmt has
            # wrapped reads the same as a one-line one. Comments go first, both
            # kinds: `to_query`'s own documentation discusses the `Result` it
            # returns, and a commented-out `// self.validate()?;` would satisfy
            # the call check while doing nothing. Either would answer the
            # question with prose rather than with code.
            body = " ".join(
                part.strip()
                for part in block_body(lines, index)
                if not part.lstrip().startswith(("///", "//"))
            )
            query = TO_QUERY.search(body)
            if query:
                found.to_query[name] = (
                    query.group(1).startswith("Result<"),
                    "validate()?" in fn_body(body, query.end()),
                )
            if INHERENT_VALIDATE.search(body):
                found.inherent_validate.add(name)

    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    parser.add_argument(
        "--report",
        action="store_true",
        help="print the gaps and exit 0, rather than failing on them",
    )
    args = parser.parse_args()

    if not args.src.is_dir():
        print(f"{args.src} is missing", file=sys.stderr)
        return 1

    missing: list[tuple[str, str]] = []
    both: list[tuple[str, str]] = []
    unasked: list[tuple[str, str, str]] = []
    broken_exemption: list[tuple[str, str]] = []
    leaky: list[tuple[str, str]] = []
    exempt_seen: dict[str, bool] = {}
    seen_additions: set[str] = set()
    seen_exclusions: set[str] = set()
    derived = 0
    handwritten = 0

    # Read every file before judging any of it. An `impl Validated for T` does
    # not have to sit in the file that declares `T` — nothing in the language
    # says so and nothing in this crate enforces it — and a per-file pass would
    # report such a type as implementing neither half while simultaneously
    # failing to apply the `to_query` rule to it. Both errors at once, in
    # opposite directions.
    parsed = {rs.as_posix(): parse(rs) for rs in sorted(args.src.rglob("*.rs"))}
    delegates: dict[str, bool] = {
        name: asks
        for found in parsed.values()
        for name, asks in found.hand_delegates.items()
    }
    # Keyed by bare name, not by module. `OrderRequest` and
    # `GetAccountActivitiesRequest` each exist twice — trading and broker — and
    # each copy hand-implements, so today the two readings agree. A third of
    # either name that implemented neither half would be reported as covered on
    # the strength of its namesakes. Resolving modules is more machinery than a
    # text scan should have; the compiler catches that type at its call site,
    # and this script's unique job is the type nothing calls yet.
    all_hand: set[str] = {name for found in parsed.values() for name in found.hand}
    # Types whose rules a parent has to ask for, because they will not be asked
    # any other way: every hand-written impl, and the two that implement neither
    # half precisely because a parent asks them.
    #
    # The blanket impls in `types/validated.rs` are excluded *by file*, not by
    # the name rule. Filtering with `in_scope` was the obvious spelling and it
    # is too narrow: a nested type with rules and a plain noun for a name — a
    # `Beneficiary`, say — would be invisible, and the two this rule already
    # catches are only in scope because someone hand-listed them in
    # `ADDITIONS`. Nothing would force the next one to be listed, and the whole
    # point of the rule is the case nobody thought about.
    has_rules: set[str] = set(EXEMPT) | {
        name
        for where, found in parsed.items()
        if not where.endswith(BLANKET_IMPLS)
        for name in found.hand
    }

    for where, found in parsed.items():
        for name, derives in found.declared.items():
            if name in EXCLUSIONS:
                seen_exclusions.add(name)
                continue
            if name in ADDITIONS:
                seen_additions.add(name)
            if not in_scope(name):
                continue

            hand = name in all_hand

            if name in EXEMPT:
                # The exemption is a claim about a type that has rules and
                # implements neither half of the trait. Every part of that has
                # to still be true, and each failure gets its own message —
                # reporting a derive on an exempt type as `E0119` would name an
                # error rustc does not emit and prescribe dropping a
                # hand-written impl that does not exist.
                exempt_seen[name] = name in found.inherent_validate
                if derives or hand:
                    broken_exemption.append((where, name))
                continue

            if derives and hand:
                both.append((where, name))
            elif derives:
                derived += 1
                # A derived no-op says "this type has no rules". If one of its
                # fields is a type that does, that is false and the field's
                # rules are asked by nobody: the transport checks the parent,
                # the parent checks nothing.
                for field in sorted(found.fields.get(name, set()) & has_rules):
                    unasked.append((where, name, field))
            elif hand:
                handwritten += 1
                # The same question of a parent that *does* implement the
                # trait. Being hand-written is not the same as delegating, and
                # the one real instance of this shape —
                # `CreateAccountRequest` carrying `UploadDocument`s, which it
                # never asked — was in exactly this arm. Checking only the
                # derived arm would have left the bug this branch fixed
                # invisible to the rule written because of it.
                #
                # A shape check, like the `to_query` one: the body has to call
                # `.validate()` on something. It cannot tell which field, so it
                # catches the line being deleted rather than proving the line
                # is right.
                # The same file first, then anywhere. `OrderRequest` exists
                # twice — the broker one wraps the trading one — and the two
                # answer this differently: the wrapper delegates, the wrapped
                # one is the leaf that holds the rules. A flat name lookup gave
                # both whichever answer was parsed last, and reported the
                # delegating one as not delegating.
                asks = found.hand_delegates.get(name, delegates.get(name, False))
                if found.fields.get(name, set()) & has_rules and not asks:
                    for field in sorted(found.fields.get(name, set()) & has_rules):
                        unasked.append((where, name, field))
            else:
                missing.append((where, name))

        # A hand-written validator plus a `to_query` that cannot report it is
        # the one skip the transport's bound cannot see. Both halves matter: a
        # `Result` return that never calls `validate` satisfies the signature
        # and asks nothing, which is a gate passing on the shape of a fix
        # rather than on the fix.
        for name, (returns_result, asks) in found.to_query.items():
            if name in all_hand and not (returns_result and asks):
                leaky.append((where, name))

    # An entry that no longer matches a struct is worse than a missing one: it
    # reads as a settled decision while covering nothing.
    for label, entries, seen in (
        ("ADDITIONS", ADDITIONS, seen_additions),
        ("EXCLUSIONS", EXCLUSIONS, seen_exclusions),
        ("EXEMPT", EXEMPT, set(exempt_seen)),
    ):
        stale = sorted(set(entries) - seen)
        if stale:
            print(
                f"{label} names {', '.join(stale)}, which no struct in "
                f"{args.src} declares — renamed, or the entry is dead",
                file=sys.stderr,
            )
            return 1

    toothless = sorted(name for name, has_rules in exempt_seen.items() if not has_rules)
    if toothless:
        print(
            f"EXEMPT names {', '.join(toothless)}, which no longer declares an "
            "inherent `validate` — the exemption was for a type with rules, so "
            "either the rules moved or the entry should go",
            file=sys.stderr,
        )
        return 1

    total = derived + handwritten
    print(f"{total} request types reach the transport")
    print(f"{derived} derive `Validated`, {handwritten} implement it by hand\n")

    # Listed only while the exemption still holds. Printing "neither, by
    # decision" for a type that has just been found deriving the trait would
    # contradict the failure two paragraphs below it.
    intact = sorted(set(EXEMPT) - {name for _, name in broken_exemption})
    if intact:
        print("Neither, by decision — nested types their parents validate:\n")
        for name in intact:
            print(f"  {name} — {EXEMPT[name]}")
        print()

    if not (missing or both or broken_exemption or leaky or unasked):
        print("Every request type is checked before it can be sent.")
        return 0

    if missing:
        print("Implements neither half of `Validated`:\n")
        for where, name in sorted(missing):
            print(f"  {where}: {name}")
        print(
            "\nAdd `Validated` to the derive list, or write "
            "`impl Validated for T` if it has rules.\n"
        )

    if both:
        print("Derives `Validated` *and* implements it — rustc calls this E0119:\n")
        for where, name in sorted(both):
            print(f"  {where}: {name}")
        print("\nKeep the hand-written impl and drop the derive.\n")

    if unasked:
        print("Holds a field whose type has rules, and does not ask it:\n")
        for where, name, field in sorted(unasked):
            print(f"  {where}: {name} holds {field}")
        print(
            "\nThe transport asks the outer type, and the outer type either "
            "derives the no-op — saying it has no rules, which is false — or "
            "implements the trait without mentioning the field. Either way the "
            "inner rules never run. Call the field's `validate` from the "
            "parent's impl; the slice impl walks a `Vec` or a slice of them "
            "for you.\n"
        )

    if broken_exemption:
        print("Exempt from `Validated`, and implementing it anyway:\n")
        for where, name in sorted(broken_exemption):
            print(f"  {where}: {name} — {EXEMPT[name]}")
        print(
            "\nThese two are exempt because they carry real rules and are only "
            "ever sent nested inside a parent that calls them. Deriving the "
            "no-op undoes that: it lets the type be handed to the transport "
            "directly and checked by nothing, which is the failure the trait "
            "exists to remove. Drop the impl, or drop the EXEMPT entry and say "
            "why here instead.\n"
        )

    if leaky:
        print("Has rules, and a `to_query` that cannot report them:\n")
        for where, name in sorted(leaky):
            print(f"  {where}: {name}")
        print(
            "\nFlattening to query pairs loses the type, and with it the bound "
            "that would have asked. `to_query` must both return `Result<…>` "
            "and call `self.validate()?` first — the signature alone proves "
            "nothing.\n"
        )

    if args.report:
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
