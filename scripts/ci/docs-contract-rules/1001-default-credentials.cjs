"use strict";

module.exports = {
  id: "issue-1001-default-credentials",
  ownerIssue: "#1001",
  profiles: [
    "local_dev",
    "test_fixture",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: [
    "PGPASSWORD=matric",
    "DATABASE_URL=postgres://matric:matric@localhost/matric",
  ],
  negativeFixtures: ["DATABASE_URL=<DATABASE_URL>"],
  rules: [
    {
      id: "docs-default-pgpassword",
      severity: "high",
      category: "default_database_password",
      detect(line) {
        return /\bPGPASSWORD\s*=\s*matric\b|\bPOSTGRES_PASSWORD\s*=\s*matric\b/.test(line);
      },
      remediation:
        "Use <POSTGRES_PASSWORD>, a secret file, or classify the exact local_dev/test_fixture finding.",
    },
    {
      id: "docs-credential-dsn",
      severity: "high",
      category: "credential_bearing_database_url",
      detect(line) {
        return /\b(?:DATABASE_URL\s*=\s*)?postgres(?:ql)?:\/\/[^/\s"'`:@]+:[^@\s"'`]+@/i.test(
          line
        );
      },
      remediation:
        "Use <DATABASE_URL> or a passwordless placeholder such as postgres://<USER>:<PASSWORD>@<HOST>/<DB>.",
    },
  ],
};
