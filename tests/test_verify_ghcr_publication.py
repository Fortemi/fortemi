import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/verify-ghcr-publication.sh"
REVISION = "a" * 40


class VerifyGhcrPublicationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "commands.log"
        self.output = self.root / "evidence"

        self.write_executable(
            "python3",
            """
            #!/usr/bin/env bash
            set -euo pipefail
            printf 'python3 %s\n' "$*" >> "$FAKE_COMMAND_LOG"
            while (( $# )); do
                if [[ "$1" == "--output" ]]; then
                    mkdir -p "$(dirname "$2")"
                    printf '{}\n' > "$2"
                    exit 0
                fi
                shift
            done
            exit 1
            """,
        )
        self.write_executable(
            "docker",
            """
            #!/usr/bin/env bash
            set -euo pipefail
            printf 'docker %s config=%s\n' "$*" "${DOCKER_CONFIG:-unset}" >> "$FAKE_COMMAND_LOG"
            [[ -n "${DOCKER_CONFIG:-}" && -d "$DOCKER_CONFIG" ]]
            case "$1 $2" in
                "pull --quiet") exit 0 ;;
                "image inspect")
                    if [[ "$4" == *revision* ]]; then
                        printf '%s\n' "${FAKE_REVISION:-$GITHUB_SHA}"
                    else
                        printf '%s\n' "${FAKE_VERSION:-$VERSION}"
                    fi
                    ;;
                *) exit 2 ;;
            esac
            """,
        )

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write_executable(self, name: str, body: str) -> None:
        path = self.bin / name
        path.write_text(textwrap.dedent(body).lstrip())
        path.chmod(0o755)

    def run_verifier(self, **overrides: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.bin}:{env['PATH']}",
                "FAKE_COMMAND_LOG": str(self.log),
                "TARGET_IMAGE": "ghcr.io/fortemi/fortemi",
                "VERSION": "2026.7.19",
                "GITHUB_SHA": REVISION,
                "OUTPUT_DIR": str(self.output),
            }
        )
        env.update(overrides)
        return subprocess.run(
            ["bash", str(SCRIPT)],
            cwd=ROOT,
            env=env,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def test_verifies_both_families_with_anonymous_client(self) -> None:
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = self.log.read_text()
        self.assertIn("ghcr-api-public-release.json", commands)
        self.assertIn("ghcr-bundle-public-release.json", commands)
        self.assertEqual(commands.count("docker pull --quiet"), 2)
        configs = {
            line.rsplit(" config=", 1)[1]
            for line in commands.splitlines()
            if line.startswith("docker ")
        }
        self.assertEqual(len(configs), 1)
        self.assertNotIn("unset", configs)

    def test_rejects_stale_source_revision_label(self) -> None:
        result = self.run_verifier(FAKE_REVISION="b" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match", result.stderr)

    def test_rejects_prefixed_version(self) -> None:
        result = self.run_verifier(VERSION="v2026.7.19")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unprefixed Fortemi release version", result.stderr)
        self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
