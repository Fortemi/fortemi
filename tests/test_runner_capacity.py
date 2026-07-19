from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "check-runner-capacity.sh"


class RunnerCapacityTests(unittest.TestCase):
    def test_every_matric_builder_workflow_invokes_capacity_guard(self) -> None:
        workflow_dir = ROOT / ".gitea" / "workflows"
        workflows = sorted((*workflow_dir.glob("*.yml"), *workflow_dir.glob("*.yaml")))
        matric_builder_workflows = [
            path for path in workflows if "runs-on: matric-builder" in path.read_text()
        ]

        self.assertGreater(len(matric_builder_workflows), 0)
        for path in matric_builder_workflows:
            with self.subTest(workflow=path.name):
                self.assertIn(
                    "scripts/ci/check-runner-capacity.sh",
                    path.read_text(),
                )

    def test_capacity_passes_above_both_floors(self) -> None:
        result = self.run_check(free_kib=50_000_000, free_inodes=2_000_000)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("runner capacity preflight passed", result.stdout)
        self.assertIn("free_kib=50000000", result.stdout)
        self.assertIn("free_inodes=2000000", result.stdout)

    def test_capacity_fails_below_byte_floor(self) -> None:
        result = self.run_check(free_kib=1_000, free_inodes=2_000_000)

        self.assertEqual(result.returncode, 1)
        self.assertIn("free space is below", result.stderr)
        self.assertIn("Runner infrastructure capacity is insufficient", result.stderr)

    def test_capacity_fails_below_inode_floor(self) -> None:
        result = self.run_check(free_kib=50_000_000, free_inodes=10)

        self.assertEqual(result.returncode, 1)
        self.assertIn("free inodes are below", result.stderr)

    def test_invalid_threshold_fails_closed(self) -> None:
        result = self.run_check(
            free_kib=50_000_000,
            free_inodes=2_000_000,
            min_free_kib="forty-gib",
        )

        self.assertEqual(result.returncode, 2)
        self.assertIn("must be a bounded positive integer", result.stderr)

    @staticmethod
    def run_check(
        *,
        free_kib: int,
        free_inodes: int,
        min_free_kib: str = "41943040",
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_df = root / "df"
            fake_df.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    if [[ "$1" == "-Pk" ]]; then
                      printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\\n'
                      printf '/dev/test 100000000 1 {free_kib} 1%% /runner\\n'
                    elif [[ "$1" == "-Pi" ]]; then
                      printf 'Filesystem Inodes IUsed IFree IUse%% Mounted on\\n'
                      printf '/dev/test 3000000 1 {free_inodes} 1%% /runner\\n'
                    else
                      exit 2
                    fi
                    """
                )
            )
            fake_df.chmod(fake_df.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env.pop("RUNNER_MIN_FREE_KIB", None)
            env.pop("RUNNER_MIN_FREE_INODES", None)
            return subprocess.run(
                [
                    "bash",
                    str(SCRIPT),
                    "--path",
                    str(root),
                    "--min-free-kib",
                    min_free_kib,
                    "--min-free-inodes",
                    "1000000",
                    "--df-bin",
                    str(fake_df),
                ],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )


if __name__ == "__main__":
    unittest.main()
