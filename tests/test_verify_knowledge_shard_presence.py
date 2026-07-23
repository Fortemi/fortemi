import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class KnowledgeShardPresenceContractTests(unittest.TestCase):
    def test_inventory_is_fresh_and_canonical_vectors_pass(self) -> None:
        subprocess.run(
            [sys.executable, "scripts/generate-knowledge-shard-field-semantics.py", "--check"],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [sys.executable, "scripts/generate-knowledge-shard-v2-contract.py", "--check"],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [sys.executable, "scripts/ci/verify-knowledge-shard-presence.py"],
            cwd=ROOT,
            check=True,
        )


if __name__ == "__main__":
    unittest.main()
