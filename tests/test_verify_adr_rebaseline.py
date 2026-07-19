from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-adr-rebaseline.py"
SPEC = importlib.util.spec_from_file_location("verify_adr_rebaseline", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class AdrRebaselineContractTests(unittest.TestCase):
    def test_repository_rebaseline_contract(self) -> None:
        self.assertEqual(MODULE.verify_repository(ROOT), [])

    def test_checkpoint_rejects_stale_owner_and_missing_date(self) -> None:
        text = """
## July 2026 checkpoint rebaseline

- **Decision status:** Proposed.
- **Implementation phase:** Construction.
- **Phase owner:** `Fortemi/fortemi#1016`.
""".lstrip()

        failures = MODULE.verify_checkpoint(
            text,
            "090",
            "Fortemi/fortemi#733",
        )

        self.assertTrue(any("Accepted target architecture" in item for item in failures))
        self.assertTrue(any("missing phase owner" in item for item in failures))
        self.assertTrue(any("Checkpoint decision date" in item for item in failures))

    def test_checkpoint_rejects_duplicate_sections(self) -> None:
        text = f"{MODULE.CHECKPOINT_HEADING}\n\n{MODULE.CHECKPOINT_HEADING}\n"
        self.assertEqual(
            MODULE.verify_checkpoint(text, "088", "Fortemi/fortemi#712"),
            ["ADR-088: expected exactly one checkpoint section"],
        )

    def test_checkpoint_accepts_implemented_adr_status(self) -> None:
        text = f"""
{MODULE.CHECKPOINT_HEADING}

- **Decision status:** Accepted; core contract implemented.
- **Implementation phase:** Runtime recorder integration.
- **Phase owner:** `Fortemi/fortemi#713`.
- **Checkpoint decision date:** 2026-07-14.
""".lstrip()

        self.assertEqual(
            MODULE.verify_checkpoint(text, "092", "Fortemi/fortemi#713"),
            [],
        )


if __name__ == "__main__":
    unittest.main()
