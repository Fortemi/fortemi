import importlib.util
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts/ci/verify-ollama-exposure-guidance.py"
SPEC = importlib.util.spec_from_file_location("verify_ollama_exposure_guidance", VERIFIER)
assert SPEC and SPEC.loader
verify_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify_module)


class OllamaExposureGuidanceTests(unittest.TestCase):
    def test_repository_guidance_passes(self) -> None:
        self.assertEqual(verify_module.validate(ROOT), [])

    def test_forbidden_guidance_is_detected(self) -> None:
        for unsafe in ("OLLAMA_HOST=0.0.0.0", "172.17.0.1:11434"):
            with self.subTest(unsafe=unsafe), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "unsafe.md").write_text(f"{unsafe}\n")
                with mock.patch.object(
                    verify_module, "tracked_guidance", return_value=[Path("unsafe.md")]
                ):
                    failures = verify_module.validate(root)
            self.assertTrue(
                any("forbidden quiet exposure" in item for item in failures)
            )

    def test_installer_warns_without_mutating_host(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bindir = Path(directory)
            commands = {
                "curl": "#!/bin/sh\nexit 0\n",
                "ollama": "#!/bin/sh\nexit 0\n",
                "uname": "#!/bin/sh\necho Linux\n",
                "systemctl": "#!/bin/sh\nexit 0\n",
                "ss": (
                    "#!/bin/sh\n"
                    "echo 'LISTEN 0 4096 127.0.0.1:11434 0.0.0.0:*'\n"
                ),
            }
            for name, body in commands.items():
                path = bindir / name
                path.write_text(body)
                path.chmod(path.stat().st_mode | stat.S_IXUSR)
            env = {
                **os.environ,
                "PATH": f"{bindir}:{os.environ['PATH']}",
                "INSTALL_DIR": directory,
            }
            result = subprocess.run(
                ["bash", "installer/scripts/pull-models.sh"],
                cwd=ROOT,
                env=env,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
        self.assertIn("No host settings were changed.", result.stdout)
        self.assertIn("HOST_GATEWAY_IP", result.stdout)
        self.assertIn("Review the printed address", result.stdout)
        self.assertNotIn("OLLAMA_HOST=0.0.0.0", result.stdout)


if __name__ == "__main__":
    unittest.main()
