import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/create-explicit-docker-network.sh"


FAKE_DOCKER = r"""#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_DOCKER_STATE:?}"
log="${FAKE_DOCKER_LOG:?}"
printf '%s\n' "$*" >> "$log"

case "${1:-} ${2:-}" in
  "network inspect")
    if [[ ! -f "$state" ]]; then exit 1; fi
    if [[ "$*" == *"--format"* ]]; then cat "$state"; else printf '{}\n'; fi
    ;;
  "network create")
    count_file="${state}.count"
    count=0
    if [[ -f "$count_file" ]]; then count="$(cat "$count_file")"; fi
    count=$((count + 1))
    printf '%s\n' "$count" > "$count_file"
    if (( count <= ${FAKE_DOCKER_CREATE_FAILURES:-0} )); then exit 1; fi
    shift 2
    subnet=''
    while (( $# )); do
      case "$1" in
        --driver) shift 2 ;;
        --subnet) subnet="$2"; shift 2 ;;
        *) shift ;;
      esac
    done
    if [[ "${FAKE_DOCKER_MISMATCH:-false}" == true ]]; then
      printf '10.239.255.0/24\n' > "$state"
    else
      printf '%s\n' "$subnet" > "$state"
    fi
    ;;
  "network rm")
    rm -f "$state"
    ;;
  *)
    exit 64
    ;;
esac
"""


class ExplicitDockerNetworkTests(unittest.TestCase):
    def setUp(self):
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.bin_dir = self.root / "bin"
        self.bin_dir.mkdir()
        self.docker = self.bin_dir / "docker"
        self.docker.write_text(FAKE_DOCKER, encoding="utf-8")
        self.docker.chmod(self.docker.stat().st_mode | stat.S_IXUSR)
        self.state = self.root / "network.state"
        self.log = self.root / "docker.log"

    def tearDown(self):
        self.tempdir.cleanup()

    def run_script(self, name="fortemi-ci-test", seed="48076", **extra_env):
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.bin_dir}:{env['PATH']}",
                "FAKE_DOCKER_STATE": str(self.state),
                "FAKE_DOCKER_LOG": str(self.log),
                **extra_env,
            }
        )
        return subprocess.run(
            ["bash", str(SCRIPT), name, seed],
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_retries_collisions_and_verifies_selected_subnet(self):
        result = self.run_script(FAKE_DOCKER_CREATE_FAILURES="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "10.251.206.0/24")
        self.assertEqual(self.state.read_text(encoding="utf-8").strip(), "10.251.206.0/24")
        calls = self.log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(sum("network create" in call for call in calls), 3)
        self.assertTrue(any("network inspect --format" in call for call in calls))

    def test_rejects_existing_network_without_mutation(self):
        self.state.write_text("10.250.1.0/24\n", encoding="utf-8")
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already exists", result.stderr)
        self.assertEqual(self.state.read_text(encoding="utf-8").strip(), "10.250.1.0/24")

    def test_rejects_invalid_name_and_seed(self):
        invalid_name = self.run_script(name="bad network")
        self.assertEqual(invalid_name.returncode, 2)
        invalid_seed = self.run_script(seed="not-numeric")
        self.assertEqual(invalid_seed.returncode, 2)

    def test_removes_network_when_subnet_verification_fails(self):
        result = self.run_script(FAKE_DOCKER_MISMATCH="true")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verification failed", result.stderr)
        self.assertFalse(self.state.exists())
        self.assertIn("network rm", self.log.read_text(encoding="utf-8"))

    def test_fails_after_bounded_candidate_exhaustion(self):
        result = self.run_script(FAKE_DOCKER_CREATE_FAILURES="300")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no collision-free explicit subnet", result.stderr)
        calls = self.log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(sum("network create" in call for call in calls), 256)


if __name__ == "__main__":
    unittest.main()
