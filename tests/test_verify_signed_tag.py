import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "tools/ci/verify-signed-tag.sh"
RELEASE_FINGERPRINT = "9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33"


class VerifySignedTagTests(unittest.TestCase):
    def test_verification_overrides_configured_signing_adapter(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            fake_bin = temp / "bin"
            fake_bin.mkdir()
            trace = temp / "git-trace"

            self._write_executable(
                fake_bin / "gpg",
                "#!/usr/bin/env bash\nexit 0\n",
            )
            self._write_executable(
                fake_bin / "gpgconf",
                "#!/usr/bin/env bash\nexit 0\n",
            )
            self._write_executable(
                fake_bin / "git",
                f"""#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "${{VERIFY_TAG_TRACE:?}}"
if [[ "$*" == "cat-file -t v2026.7.23" ]]; then
  printf 'tag\\n'
  exit 0
fi
expected='-c gpg.program={fake_bin / "gpg"} verify-tag --raw v2026.7.23'
[[ "$*" == "$expected" ]] || {{
  printf 'unexpected verification command: %s\\n' "$*" >&2
  exit 2
}}
[[ -n "${{GNUPGHOME:-}}" ]] || exit 3
printf '[GNUPG:] VALIDSIG {RELEASE_FINGERPRINT} 2026-08-25 0 4 0 22 8 00 {RELEASE_FINGERPRINT}\\n' >&2
""",
            )

            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{fake_bin}:{env['PATH']}",
                    "RUNNER_TEMP": str(temp),
                    "VERIFY_TAG_TRACE": str(trace),
                }
            )
            result = subprocess.run(
                ["bash", str(VERIFIER), "v2026.7.23"],
                cwd=ROOT,
                env=env,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"Verified v2026.7.23 with release key {RELEASE_FINGERPRINT}", result.stdout)
            self.assertIn(
                f"-c gpg.program={fake_bin / 'gpg'} verify-tag --raw v2026.7.23",
                trace.read_text(),
            )

    @staticmethod
    def _write_executable(path: Path, content: str) -> None:
        path.write_text(content)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)


if __name__ == "__main__":
    unittest.main()
