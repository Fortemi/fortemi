from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-bundle-db-secrets.py"
SPEC = importlib.util.spec_from_file_location("verify_bundle_db_secrets", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BundleDbSecretVerifierTests(unittest.TestCase):
    def make_root(
        self,
        *,
        dockerfile: str = "ENV POSTGRES_USER=matric\n",
        compose: str = (
            "services:\n  fortemi:\n    environment:\n"
            "      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-}\n"
        ),
        gitignore: str = ".env\n.env.bak\n",
        entrypoint: str = (
            "require_postgres_password() { :; }\n"
            "require_postgres_password\n"
            'if [ -z "${DATABASE_URL:-}" ]; then :; fi\n'
        ),
    ) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        (root / "docker").mkdir()
        (root / "Dockerfile.bundle").write_text(dockerfile, encoding="utf-8")
        (root / "docker-compose.bundle.yml").write_text(
            compose,
            encoding="utf-8",
        )
        (root / ".gitignore").write_text(gitignore, encoding="utf-8")
        (root / "docker" / "bundle-entrypoint.sh").write_text(
            entrypoint,
            encoding="utf-8",
        )
        return temporary, root

    def test_accepts_fail_closed_runtime_contract(self) -> None:
        temporary, root = self.make_root()
        with temporary:
            self.assertEqual(MODULE.verify(root), [])

    def test_rejects_image_password_and_credential_url(self) -> None:
        temporary, root = self.make_root(
            dockerfile=(
                "ENV POSTGRES_PASSWORD=matric\n"
                "ENV DATABASE_URL=postgres://matric:matric@localhost/matric\n"
            )
        )
        with temporary:
            errors = "\n".join(MODULE.verify(root))
            self.assertIn("must not bake POSTGRES_PASSWORD", errors)
            self.assertIn("credential-bearing URL", errors)

    def test_rejects_compose_default_and_entrypoint_fallback(self) -> None:
        temporary, root = self.make_root(
            compose=(
                "services:\n  fortemi:\n    environment:\n"
                "      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-fortemi-local-dev}\n"
            ),
            entrypoint=(
                'POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-fortemi-local-dev}"\n'
                'if [ -z "${DATABASE_URL:-}" ]; then :; fi\n'
            ),
        )
        with temporary:
            errors = "\n".join(MODULE.verify(root))
            self.assertIn("reusable DB password", errors)
            self.assertIn("must not default POSTGRES_PASSWORD", errors)
            self.assertIn("must validate POSTGRES_PASSWORD", errors)

    def test_rejects_duplicate_or_arbitrary_fixed_compose_assignment(self) -> None:
        temporary, root = self.make_root(
            compose=(
                "services:\n  fortemi:\n    environment:\n"
                "      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-}\n"
                "      - POSTGRES_PASSWORD=fixed-but-not-known\n"
            )
        )
        with temporary:
            errors = "\n".join(MODULE.verify(root))
            self.assertIn("exactly one empty fail-closed", errors)

    def test_rejects_unignored_environment_backup(self) -> None:
        temporary, root = self.make_root(gitignore=".env\n")
        with temporary:
            errors = "\n".join(MODULE.verify(root))
            self.assertIn(".env.bak files must both be ignored", errors)


if __name__ == "__main__":
    unittest.main()
