from __future__ import annotations

import copy
import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-bundle-exposure.py"
SPEC = importlib.util.spec_from_file_location("verify_bundle_exposure", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def base_config() -> dict:
    return {
        "services": {
            "fortemi": {
                "image": "ghcr.io/fortemi/fortemi:bundle-latest",
                "ports": [
                    {
                        "target": 3000,
                        "published": "3000",
                        "host_ip": "127.0.0.1",
                    },
                    {
                        "target": 3001,
                        "published": "3001",
                        "host_ip": "127.0.0.1",
                    },
                ],
                "environment": {
                    "FORTEMI_EXPOSURE_PROFILE": "local",
                    "REQUIRE_AUTH": "true",
                    "ISSUER_URL": "https://localhost:3000",
                    "MCP_BASE_URL": "https://localhost:3000/mcp",
                    "ALLOWED_ORIGINS": "",
                    "POSTGRES_PASSWORD": "a" * 64,
                },
            },
        }
    }


class BundleExposureTests(unittest.TestCase):
    def test_local_profile_accepts_only_loopback_bindings(self) -> None:
        errors, warnings, report = MODULE.validate(base_config())

        self.assertEqual(errors, [])
        self.assertIn("#990", "\n".join(warnings))
        self.assertNotIn("#937", "\n".join(warnings))
        self.assertEqual(report["api_bind"], "127.0.0.1:3000")
        self.assertEqual(report["docker_socket_profile"], "absent")
        self.assertEqual(
            report["db_secret_source"],
            "generated_or_operator_supplied",
        )

    def test_local_profile_rejects_missing_or_reusable_password(self) -> None:
        for password in ("", "matric", "fortemi-local-dev", "changeme"):
            with self.subTest(password=password):
                config = base_config()
                config["services"]["fortemi"]["environment"][
                    "POSTGRES_PASSWORD"
                ] = password

                errors, _, report = MODULE.validate(config)

                self.assertIn(
                    "generated or operator-supplied POSTGRES_PASSWORD",
                    "\n".join(errors),
                )
                self.assertEqual(
                    report["db_secret_source"],
                    "missing_or_insecure",
                )

    def test_local_profile_rejects_implicit_all_interface_binding(self) -> None:
        config = base_config()
        del config["services"]["fortemi"]["ports"][0]["host_ip"]

        errors, _, _ = MODULE.validate(config)

        self.assertIn(
            "API must publish to a loopback host IP in the local profile",
            errors,
        )

    def test_local_profile_accepts_ipv6_loopback(self) -> None:
        config = base_config()
        for port in config["services"]["fortemi"]["ports"]:
            port["host_ip"] = "::1"

        errors, _, _ = MODULE.validate(config)

        self.assertEqual(errors, [])

    def test_duplicate_target_mapping_fails_closed(self) -> None:
        config = base_config()
        duplicate = copy.deepcopy(config["services"]["fortemi"]["ports"][0])
        duplicate["host_ip"] = "0.0.0.0"
        duplicate["published"] = "3010"
        config["services"]["fortemi"]["ports"].append(duplicate)

        errors, _, _ = MODULE.validate(config)

        self.assertIn("API target port 3000 must have one host mapping", errors)
        self.assertIn(
            "API must publish to a loopback host IP in the local profile",
            errors,
        )

    def test_unknown_profile_fails_closed(self) -> None:
        config = base_config()
        config["services"]["fortemi"]["environment"][
            "FORTEMI_EXPOSURE_PROFILE"
        ] = "public"

        errors, _, _ = MODULE.validate(config)

        self.assertIn(
            "FORTEMI_EXPOSURE_PROFILE must be exactly 'local' or 'shared'",
            errors,
        )

    def test_shared_profile_accepts_complete_explicit_contract(self) -> None:
        config = self.shared_config()

        errors, _, report = MODULE.validate(config)

        self.assertEqual(errors, [])
        self.assertEqual(report["auth_mode"], "required")
        self.assertEqual(report["issuer_mode"], "public_https")
        self.assertEqual(
            report["db_secret_source"],
            "generated_or_operator_supplied",
        )

    def test_shared_profile_rejects_each_missing_security_control(self) -> None:
        cases = {
            "auth": ("REQUIRE_AUTH", "false", "REQUIRE_AUTH=true"),
            "issuer": ("ISSUER_URL", "http://10.0.0.5:3000", "HTTPS ISSUER_URL"),
            "resource": ("MCP_BASE_URL", "", "HTTPS MCP_BASE_URL"),
            "origins": ("ALLOWED_ORIGINS", "", "HTTPS ALLOWED_ORIGINS"),
            "password": (
                "POSTGRES_PASSWORD",
                " MATRIC ",
                "generated or operator-supplied POSTGRES_PASSWORD",
            ),
        }
        for name, (key, value, expected) in cases.items():
            with self.subTest(name=name):
                config = self.shared_config()
                config["services"]["fortemi"]["environment"][key] = value

                errors, _, _ = MODULE.validate(config)

                self.assertIn(expected, "\n".join(errors))

    def test_shared_profile_rejects_wildcard_origin_and_issuer_path(self) -> None:
        config = self.shared_config()
        config["services"]["fortemi"]["environment"].update(
            {
                "ISSUER_URL": "https://memory.example.com/oauth",
                "ALLOWED_ORIGINS": "https://*.example.com",
            }
        )

        errors, _, _ = MODULE.validate(config)

        self.assertIn("HTTPS ISSUER_URL", "\n".join(errors))
        self.assertIn("HTTPS ALLOWED_ORIGINS", "\n".join(errors))

    def test_shared_errors_never_echo_database_secret(self) -> None:
        config = self.shared_config()
        secret = "do-not-log-this-secret"
        config["services"]["fortemi"]["environment"]["POSTGRES_PASSWORD"] = secret
        config["services"]["fortemi"]["environment"]["REQUIRE_AUTH"] = "invalid"

        errors, warnings, report = MODULE.validate(config)

        output = "\n".join((*errors, *warnings, *report.values()))
        self.assertNotIn(secret, output)

    def test_ops_autoheal_profile_accepts_only_reviewed_service(self) -> None:
        config = base_config()
        config["services"]["autoheal"] = self.autoheal_service()

        errors, _, report = MODULE.validate(config, {"edge", "ops-autoheal"})

        self.assertEqual(errors, [])
        self.assertEqual(report["docker_socket_profile"], "ops-autoheal")

    def test_socket_mount_requires_explicit_ops_autoheal_profile(self) -> None:
        config = base_config()
        config["services"]["autoheal"] = self.autoheal_service()

        errors, _, _ = MODULE.validate(config)

        self.assertIn(
            "requires the explicit ops-autoheal profile",
            "\n".join(errors),
        )

    def test_ops_autoheal_profile_requires_service_and_pinned_image(self) -> None:
        missing_errors, _, _ = MODULE.validate(base_config(), {"ops-autoheal"})
        self.assertIn(
            "must render the reviewed autoheal service",
            "\n".join(missing_errors),
        )

        config = base_config()
        config["services"]["autoheal"] = self.autoheal_service()
        config["services"]["autoheal"]["image"] = "willfarrell/autoheal:latest"
        image_errors, _, _ = MODULE.validate(config, {"ops-autoheal"})
        self.assertIn("must be pinned", "\n".join(image_errors))

    def test_other_service_can_never_mount_docker_socket(self) -> None:
        config = base_config()
        config["services"]["autoheal"] = self.autoheal_service()
        config["services"]["unexpected"] = {
            "image": "example.invalid/unexpected:1",
            "volumes": [
                {
                    "type": "bind",
                    "source": "/run/docker.sock",
                    "target": "/run/docker.sock",
                }
            ],
        }

        errors, _, _ = MODULE.validate(config, {"ops-autoheal"})

        self.assertIn(
            "forbidden outside the autoheal service: unexpected",
            "\n".join(errors),
        )

    @staticmethod
    def autoheal_service() -> dict:
        return {
            "image": "willfarrell/autoheal:1.2.0",
            "volumes": [
                {
                    "type": "bind",
                    "source": "/var/run/docker.sock",
                    "target": "/var/run/docker.sock",
                }
            ],
        }

    @staticmethod
    def shared_config() -> dict:
        config = copy.deepcopy(base_config())
        service = config["services"]["fortemi"]
        service["ports"][0]["host_ip"] = "0.0.0.0"
        service["ports"][1]["host_ip"] = "0.0.0.0"
        service["environment"].update(
            {
                "FORTEMI_EXPOSURE_PROFILE": "shared",
                "REQUIRE_AUTH": "true",
                "ISSUER_URL": "https://memory.example.com",
                "MCP_BASE_URL": "https://memory.example.com/mcp",
                "ALLOWED_ORIGINS": "https://memory.example.com",
                "POSTGRES_PASSWORD": "operator-supplied-value-for-bundle",
            }
        )
        return config


if __name__ == "__main__":
    unittest.main()
