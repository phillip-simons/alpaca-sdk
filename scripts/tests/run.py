"""Run the script tests, and fail if there were not any to run.

`python3 -m unittest discover` prints "Ran 0 tests ... OK" and exits 0 when it
finds nothing. Renaming a test file out of the `test*.py` pattern, or moving
this directory, would leave `just check` and the CI job green while executing
none of the suite — a check that passes by finding nothing, which is the exact
shape of failure the code under test exists to stop reporting.

So discovery is counted before it is run, and a suite that shrank to nothing is
an error rather than a pass. The floor is deliberately zero-versus-nonzero
rather than a pinned number: an expected-count constant is one more thing to
forget to update, and it would fail for the wrong reason every time somebody
legitimately deletes a test.
"""

from __future__ import annotations

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent


def main() -> int:
    suite = unittest.TestLoader().discover(str(HERE), top_level_dir=str(HERE))
    found = suite.countTestCases()

    if not found:
        print(
            f"no tests discovered under {HERE} — a renamed file or a moved "
            f"directory would look exactly like this, and like a pass",
            file=sys.stderr,
        )
        return 1

    result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
