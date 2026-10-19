from __future__ import annotations

import os
import re
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "init-bundle-env.sh"
CONFIGURE = ROOT / "installer" / "scripts" / "configure.sh"
DEPLOY = ROOT / "installer" / "scripts" / "deploy.sh"


class InitBundleEnvTests(unittest.TestCase):
    def run_script(
        self,
        env_file: Path,
        *,
        password: str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("POSTGRES_PASSWORD", None)
        if password is not None:
            env["POSTGRES_PASSWORD"] = password
        return subprocess.run(
            ["bash", str(SCRIPT), str(env_file)],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )

    def test_generates_secret_with_restrictive_mode_without_printing_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"

            result = self.run_script(env_file)

            self.assertEqual(result.returncode, 0, result.stderr)
            value = env_file.read_text(encoding="utf-8").split("=", 1)[1].strip()
            self.assertRegex(value, re.compile(r"^[0-9a-f]{64}$"))
            self.assertNotIn(value, result.stdout + result.stderr)
            mode = stat.S_IMODE(env_file.stat().st_mode)
            self.assertEqual(mode, 0o600)

    def test_replaces_known_default_and_collapses_duplicate_assignments(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"
            env_file.write_text(
                "KEEP=this\nPOSTGRES_PASSWORD=matric\n"
                "POSTGRES_PASSWORD=fortemi-local-dev\n",
                encoding="utf-8",
            )

            result = self.run_script(env_file)

            self.assertEqual(result.returncode, 0, result.stderr)
            content = env_file.read_text(encoding="utf-8")
            self.assertIn("KEEP=this", content)
            self.assertEqual(content.count("POSTGRES_PASSWORD="), 1)
            self.assertNotIn("POSTGRES_PASSWORD=matric", content)
            self.assertNotIn("POSTGRES_PASSWORD=fortemi-local-dev", content)

    def test_preserves_existing_nondefault_secret_idempotently(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"
            content = "POSTGRES_PASSWORD=existing-install-specific-value\n"
            env_file.write_text(content, encoding="utf-8")
            env_file.chmod(0o644)

            first = self.run_script(env_file)
            second = self.run_script(env_file)

            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(env_file.read_text(encoding="utf-8"), content)
            self.assertEqual(stat.S_IMODE(env_file.stat().st_mode), 0o600)

    def test_accepts_operator_value_but_never_prints_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"
            secret = "operator-install-specific-value"

            result = self.run_script(env_file, password=secret)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"POSTGRES_PASSWORD={secret}", env_file.read_text())
            self.assertNotIn(secret, result.stdout + result.stderr)

    def test_rejects_operator_default_and_symlink_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env_file = root / ".env"
            weak = self.run_script(env_file, password="matric")
            self.assertNotEqual(weak.returncode, 0)
            self.assertFalse(env_file.exists())

            target = root / "target"
            target.write_text("KEEP=this\n", encoding="utf-8")
            env_file.symlink_to(target)
            linked = self.run_script(env_file)
            self.assertNotEqual(linked.returncode, 0)
            self.assertEqual(target.read_text(encoding="utf-8"), "KEEP=this\n")

    def test_installer_reconfiguration_preserves_generated_secret(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            install_dir = Path(directory)
            scripts_dir = install_dir / "scripts"
            scripts_dir.mkdir()
            bootstrap = scripts_dir / "init-bundle-env.sh"
            bootstrap.write_bytes(SCRIPT.read_bytes())
            bootstrap.chmod(0o755)
            env = os.environ.copy()
            env.update(
                {
                    "INSTALL_DIR": str(install_dir),
                    "DATA_DIR": str(install_dir / "data"),
                    "DOMAIN": "localhost",
                    "FORTEMI_EXPOSURE_PROFILE": "local",
                    "FORTEMI_INSTALL_MODE": "secure",
                }
            )
            env.pop("POSTGRES_PASSWORD", None)

            first = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )
            first_content = (install_dir / ".env").read_text(encoding="utf-8")
            backup = install_dir / ".env.bak"
            backup.write_text("legacy-backup\n", encoding="utf-8")
            backup.chmod(0o644)
            second = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )
            second_content = (install_dir / ".env").read_text(encoding="utf-8")

            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertEqual(second.returncode, 0, second.stderr)
            pattern = re.compile(r"^POSTGRES_PASSWORD=(.+)$", re.MULTILINE)
            first_secret = pattern.search(first_content)
            second_secret = pattern.search(second_content)
            assert first_secret is not None and second_secret is not None
            self.assertEqual(first_secret.group(1), second_secret.group(1))
            self.assertIn(
                f"POSTGRES_PASSWORD={first_secret.group(1)}",
                backup.read_text(encoding="utf-8"),
            )
            self.assertEqual(stat.S_IMODE(backup.stat().st_mode), 0o600)
            output = first.stdout + first.stderr + second.stdout + second.stderr
            self.assertNotIn(first_secret.group(1), output)

    def test_installer_refuses_symlinked_environment_backup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            install_dir = Path(directory)
            scripts_dir = install_dir / "scripts"
            scripts_dir.mkdir()
            bootstrap = scripts_dir / "init-bundle-env.sh"
            bootstrap.write_bytes(SCRIPT.read_bytes())
            bootstrap.chmod(0o755)
            env_file = install_dir / ".env"
            env_file.write_text(
                "POSTGRES_PASSWORD=existing-install-specific-value\n",
                encoding="utf-8",
            )
            target = install_dir / "do-not-overwrite"
            target.write_text("preserve\n", encoding="utf-8")
            (install_dir / ".env.bak").symlink_to(target)
            env = os.environ.copy()
            env.update(
                {
                    "INSTALL_DIR": str(install_dir),
                    "DATA_DIR": str(install_dir / "data"),
                }
            )

            result = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("symlinked backup", result.stderr)
            self.assertEqual(target.read_text(encoding="utf-8"), "preserve\n")

    def test_installer_disables_autoheal_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            install_dir = Path(directory)
            scripts_dir = install_dir / "scripts"
            scripts_dir.mkdir()
            bootstrap = scripts_dir / "init-bundle-env.sh"
            bootstrap.write_bytes(SCRIPT.read_bytes())
            bootstrap.chmod(0o755)
            env = os.environ.copy()
            env.update(
                {
                    "INSTALL_DIR": str(install_dir),
                    "DATA_DIR": str(install_dir / "data"),
                }
            )
            env.pop("FORTEMI_AUTOHEAL_MODE", None)

            result = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            content = (install_dir / ".env").read_text(encoding="utf-8")
            self.assertIn("COMPOSE_PROFILES=edge\n", content)
            self.assertIn("FORTEMI_AUTOHEAL_MODE=disabled\n", content)
            self.assertNotIn("COMPOSE_PROFILES=edge,ops-autoheal", content)
            self.assertIn("no Docker socket mount", result.stdout)

    def test_installer_rejects_explicit_autoheal_mode_before_writes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            install_dir = Path(directory)
            scripts_dir = install_dir / "scripts"
            scripts_dir.mkdir()
            bootstrap = scripts_dir / "init-bundle-env.sh"
            bootstrap.write_bytes(SCRIPT.read_bytes())
            bootstrap.chmod(0o755)
            env = os.environ.copy()
            env.update(
                {
                    "INSTALL_DIR": str(install_dir),
                    "DATA_DIR": str(install_dir / "data"),
                    "FORTEMI_AUTOHEAL_MODE": "ops-autoheal",
                }
            )

            result = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

            self.assertEqual(result.returncode, 1)
            self.assertIn("autoheal is disabled because willfarrell/autoheal 1.2.0", result.stderr)
            self.assertIn("end-of-life Alpine 3.13", result.stderr)
            self.assertIn("unpatched HIGH/CRITICAL advisories", result.stderr)
            self.assertIn("Fortemi/fortemi#1240", result.stderr)
            self.assertFalse((install_dir / ".env").exists())
            self.assertFalse((install_dir / "data").exists())

    def test_installer_rejects_conflicting_autoheal_profile(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env = os.environ.copy()
            env.update(
                {
                    "INSTALL_DIR": directory,
                    "COMPOSE_PROFILES": "edge,ops-autoheal",
                    "FORTEMI_AUTOHEAL_MODE": "disabled",
                }
            )

            result = subprocess.run(
                ["bash", str(CONFIGURE)],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("autoheal is disabled", result.stderr)
            self.assertIn("Fortemi/fortemi#1240", result.stderr)
            self.assertFalse((Path(directory) / ".env").exists())

    def test_installer_rejects_autoheal_profiles_and_preserves_existing_env(self) -> None:
        for mode in ("disabled", "ops-autoheal"):
            for profiles in ("ops-autoheal", "ops-autoheal,edge", "edge, ops-autoheal "):
                with self.subTest(mode=mode, profiles=profiles), tempfile.TemporaryDirectory() as directory:
                    install_dir = Path(directory)
                    env_file = install_dir / ".env"
                    content = "KEEP=operator-config\nPOSTGRES_PASSWORD=existing-secret\n"
                    env_file.write_text(content, encoding="utf-8")
                    env = os.environ.copy()
                    env.update({
                        "INSTALL_DIR": directory,
                        "COMPOSE_PROFILES": profiles,
                        "FORTEMI_AUTOHEAL_MODE": mode,
                    })

                    result = subprocess.run(
                        ["bash", str(CONFIGURE)], check=False,
                        capture_output=True, text=True, env=env,
                    )

                    self.assertEqual(result.returncode, 1)
                    self.assertIn("autoheal is disabled", result.stderr)
                    self.assertIn("Fortemi/fortemi#1240", result.stderr)
                    self.assertEqual(env_file.read_text(encoding="utf-8"), content)
                    self.assertFalse((install_dir / ".env.bak").exists())
                    self.assertFalse((install_dir / "data").exists())


class DeployAutohealRetirementTests(unittest.TestCase):
    def deploy(self, root, *, project=None, model_name="", names="", inspect_status=0):
        install_dir = root / "__Fortemi.Bundle-TEST"
        scripts_dir = install_dir / "scripts"
        scripts_dir.mkdir(parents=True)
        (scripts_dir / "validate-bundle-exposure.sh").write_text("exit 0\n")
        bin_dir = root / "bin"
        bin_dir.mkdir()
        docker = bin_dir / "docker"
        docker.write_text(
            '#!/bin/bash\nprintf "%s\\n" "$*" >> "$DOCKER_CALLS"\n'
            'if [[ "$1 $4 $5 $6" == "compose config --format json" ]]; then\n'
            '  printf \'{"name": "%s"}\\n\' "$MODEL_NAME"\n'
            '  exit 0\n'
            'fi\n'
            'if [[ "$1 $2" == "ps -a" ]]; then\n'
            '  printf "%s" "$AUTOHEAL_NAMES"\n'
            '  exit "$INSPECT_STATUS"\n'
            'fi\n'
            '[[ "$1" == compose && ( "$4" == pull || "$4 $5" == "up -d" ) ]]\n'
        )
        docker.chmod(0o755)
        curl = bin_dir / "curl"
        curl.write_text("#!/bin/bash\nexit 0\n")
        curl.chmod(0o755)
        env = os.environ.copy()
        env.pop("COMPOSE_PROJECT_NAME", None)
        env.update({
            "PATH": f"{bin_dir}:{env['PATH']}",
            "INSTALL_DIR": str(install_dir),
            "AUTOHEAL_NAMES": names,
            "MODEL_NAME": model_name,
            "INSPECT_STATUS": str(inspect_status),
            "DOCKER_CALLS": str(root / "docker-calls"),
        })
        if project is not None:
            env["COMPOSE_PROJECT_NAME"] = project
        result = subprocess.run(
            ["bash", str(DEPLOY)], capture_output=True, text=True, env=env,
        )
        return result, (root / "docker-calls").read_text().splitlines()

    def test_refuses_retired_containers_with_exact_command_and_project_scope(self):
        for project, expected in ((None, "fortemibundle-test"), ("__Custom.Project-1", "customproject-1")):
            with self.subTest(project=project), tempfile.TemporaryDirectory() as directory:
                result, calls = self.deploy(
                    Path(directory), project=project,
                    names="old-autoheal-1\nold-autoheal-2\n",
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("Docker socket access", result.stderr)
                self.assertIn("docker rm -f old-autoheal-1 old-autoheal-2\n", result.stderr)
                self.assertIn("docs/content/operators-guide.md#upgrade-retire-existing-autoheal-containers", result.stderr)
                expected_calls = [] if project else ["compose -f docker-compose.bundle.yml config --format json"]
                self.assertEqual(calls, expected_calls + [
                    f"ps -a --filter label=com.docker.compose.project={expected} "
                    "--filter label=com.docker.compose.service=autoheal --format {{.Names}}"
                ])

    def test_proceeds_when_no_retired_container_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            result, calls = self.deploy(Path(directory))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(calls[-2:], [
                "compose -f docker-compose.bundle.yml pull",
                "compose -f docker-compose.bundle.yml up -d",
            ])

    def test_stops_when_container_inspection_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            result, calls = self.deploy(Path(directory), inspect_status=1)
            self.assertEqual(result.returncode, 1)
            self.assertIn("cannot check for retired autoheal containers", result.stderr)
            self.assertEqual(len(calls), 2)

    def test_uses_compose_resolved_project_name_from_environment_file(self):
        with tempfile.TemporaryDirectory() as directory:
            result, calls = self.deploy(
                Path(directory), model_name="tenant-from-dotenv", names="old-autoheal\n",
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("label=com.docker.compose.project=tenant-from-dotenv", calls[-1])
            self.assertIn("docker rm -f old-autoheal", result.stderr)


if __name__ == "__main__":
    unittest.main()
