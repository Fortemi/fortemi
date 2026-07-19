from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-docker-socket-guidance.py"


class DockerSocketGuidanceTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        result = self.run_check(ROOT)

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_world_writable_socket_is_rejected(self) -> None:
        result = self.run_fixture(
            {"docs/runner.md": "sudo chmod 666 /var/run/docker.sock\n"}
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("must never be made world-writable", result.stderr)

    def test_other_world_writable_modes_are_rejected(self) -> None:
        for mode in ("0777", "04777", "o+w", "o=rw", "a+rw"):
            with self.subTest(mode=mode):
                result = self.run_fixture(
                    {"docs/runner.md": f"sudo chmod {mode} /run/docker.sock\n"}
                )

                self.assertEqual(result.returncode, 1)
                self.assertIn("must never be made world-writable", result.stderr)

    def test_unreviewed_socket_mount_is_rejected(self) -> None:
        result = self.run_fixture(
            {
                "docs/runner.md": (
                    "docker run -v /var/run/docker.sock:/var/run/docker.sock image\n"
                )
            }
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("outside the reviewed", result.stderr)

    def test_approved_mount_requires_threat_markers(self) -> None:
        result = self.run_fixture(
            {
                "build/README.md": (
                    "docker run -v /var/run/docker.sock:/var/run/docker.sock image\n"
                )
            }
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("missing nearby threat markers", result.stderr)

    def test_alternate_target_and_long_mount_syntax_are_rejected(self) -> None:
        examples = (
            "docker run -v /var/run/docker.sock:/tmp/docker.sock image\n",
            (
                "docker run --mount "
                "type=bind,source=/var/run/docker.sock,target=/tmp/docker.sock image\n"
            ),
        )
        for example in examples:
            with self.subTest(example=example):
                result = self.run_fixture({"docs/runner.md": example})

                self.assertEqual(result.returncode, 1)
                self.assertIn("outside the reviewed", result.stderr)

    def test_reviewed_opt_in_mount_is_allowed(self) -> None:
        result = self.run_fixture(
            {
                "build/README.md": (
                    "Trusted opt-in builder. Docker access is root-equivalent "
                    "host control. A :ro mount is not a security boundary.\n"
                    "docker run -v /var/run/docker.sock:/var/run/docker.sock image\n"
                )
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unauthenticated_docker_tcp_is_rejected(self) -> None:
        result = self.run_fixture(
            {"docs/runner.md": "export DOCKER_HOST=tcp://builder.example:2375\n"}
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("must require mutually authenticated TLS", result.stderr)

    def test_unauthenticated_docker_host_flag_is_rejected(self) -> None:
        result = self.run_fixture(
            {"docs/runner.md": "docker --host=tcp://builder.example:2375 info\n"}
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("must require mutually authenticated TLS", result.stderr)

    def test_mutually_authenticated_docker_tcp_is_allowed(self) -> None:
        result = self.run_fixture(
            {
                "docs/runner.md": (
                    "Use mutually authenticated TLS with a client certificate.\n"
                    "export DOCKER_HOST=tcp://builder.example:2376\n"
                    "export DOCKER_TLS_VERIFY=1\n"
                )
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def run_fixture(self, files: dict[str, str]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for relative, content in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            return self.run_check(root)

    @staticmethod
    def run_check(root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), str(root)],
            check=False,
            capture_output=True,
            text=True,
        )


if __name__ == "__main__":
    unittest.main()
