## Summary

Describe the problem and resulting behaviour. Link the issue.

## Verification

PRs do not run CI. Build locally and test the touched crates/files plus their
direct neighbours, with the CI-equivalent features and environment. Include
`hosted-auth,kms-vault` for the relevant API tests and `REDIS_URL` for SSE tests;
use a disposable migrated database and serialize shared-database tests.

- Scope and direct neighbours tested (and why):
- Exact commands, features and environment (omit credentials):
- Build result; test counts (passed / failed / ignored):
- Unavailable checks or remaining limitations:

After merge, record the main ci-builder and comprehensive test.yml run IDs and
results. `delivery.require_ci_green` applies to those runs. Fix red main forward
on a new branch immediately.

For releases: record the exact candidate SHA and both green release validation
run IDs (`ci-builder` dispatch, its comprehensive `test.yml` handoff, and the
`docsite-build` dispatch) before running `tools/release/cut-tag.sh`.
