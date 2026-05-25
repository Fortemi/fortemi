# Per-Workstream Test Plan — Construction Phase

**Issue**: fortemi/fortemi#736
**Phase**: Phase 3 — SDLC Corpus Generation
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

This document inventories test cases per construction workstream (WS-1 through WS-10), names the fixture each case requires, and assigns each case to the appropriate CI job. It is the construction-phase test-engineering ledger — a checklist for "have we covered this workstream's surface?"

For framework, pyramid, and coverage targets, see `test-strategy.md`. For the security suite detail, see `tenant-isolation-regression-suite.md`.

---

## Master Test-Case Inventory

| Test ID | Workstream | Test Case (short name) | Category | Fixtures | CI Job |
|---|---|---|---|---|---|
| WS1-U-1 | WS-1 | referenced_backend_read_returns_correct_bytes | unit | small-repo | test.yml (unit-tests) |
| WS1-U-2 | WS-1 | referenced_backend_write_returns_not_supported_error | unit | (none) | test.yml (unit-tests) |
| WS1-U-3 | WS-1 | referenced_backend_delete_returns_not_supported_error | unit | (none) | test.yml (unit-tests) |
| WS1-U-4 | WS-1 | referenced_backend_exists_returns_true_for_real_path | unit | small-repo | test.yml (unit-tests) |
| WS1-U-5 | WS-1 | referenced_backend_exists_returns_false_for_nonexistent | unit | (none) | test.yml (unit-tests) |
| WS1-U-6 | WS-1 | referenced_backend_resolve_path_returns_literal_path | unit | (none) | test.yml (unit-tests) |
| WS1-U-7 | WS-1 | streaming_blake3_matches_full_read_blake3 | unit | with-large-files | test.yml (unit-tests) |
| WS1-U-8 | WS-1 | streaming_blake3_handles_empty_file | unit | (none) | test.yml (unit-tests) |
| WS1-U-9 | WS-1 | pg_file_storage_repository_dispatches_referenced | integration (tx) | small-repo | test.yml (integration) |
| WS1-U-10 | WS-1 | file_source_referenced_variant_serializes_round_trip | unit | (none) | test.yml (unit-tests) |
| WS2-I-1 | WS-2 | migration_adds_storage_mode_column | integration (non-tx) | (none) | test.yml (integration) |
| WS2-I-2 | WS-2 | migration_adds_source_path_column | integration (non-tx) | (none) | test.yml (integration) |
| WS2-I-3 | WS-2 | migration_adds_scan_config_column | integration (non-tx) | (none) | test.yml (integration) |
| WS2-I-4 | WS-2 | migration_adds_last_scan_at_and_scan_status | integration (non-tx) | (none) | test.yml (integration) |
| WS2-I-5 | WS-2 | migration_preserves_existing_archives_as_managed | integration (non-tx) | (none) | test.yml (integration) |
| WS2-I-6 | WS-2 | create_referenced_archive_repository_method | integration (tx) | small-repo | test.yml (integration) |
| WS2-I-7 | WS-2 | archive_info_struct_includes_storage_mode | unit | (none) | test.yml (unit-tests) |
| WS2-I-8 | WS-2 | archive_context_carries_storage_mode | integration (tx) | small-repo | test.yml (integration) |
| WS2-I-9 | WS-2 | default_archive_cache_handles_referenced_field | integration (tx) | small-repo | test.yml (integration) |
| WS2-I-10 | WS-2 | clone_archive_schema_rejects_referenced_to_referenced_without_path | integration (tx) | small-repo | test.yml (integration) |
| WS2-I-11 | WS-2 | drop_archive_schema_referenced_does_not_delete_source | integration (tx) | small-repo | test.yml (integration) |
| WS3-U-1 | WS-3 | scan_walker_respects_gitignore | unit | with-gitignore | test.yml (unit-tests) |
| WS3-U-2 | WS-3 | scan_walker_skips_node_modules | unit | small-repo (extended) | test.yml (unit-tests) |
| WS3-U-3 | WS-3 | scan_walker_skips_target_dir | unit | small-repo (extended) | test.yml (unit-tests) |
| WS3-U-4 | WS-3 | scan_walker_skips_git_dir | unit | small-repo | test.yml (unit-tests) |
| WS3-U-5 | WS-3 | scan_walker_respects_file_size_cap | unit | with-large-files | test.yml (unit-tests) |
| WS3-U-6 | WS-3 | scan_walker_handles_permission_denied_warns_continues | unit | with-permission-denied | test.yml (unit-tests) |
| WS3-U-7 | WS-3 | scan_walker_handles_symlink_loop | unit | with-symlink-loop | test.yml (unit-tests) |
| WS3-U-8 | WS-3 | scan_walker_handles_non_utf8_paths | unit | with-non-utf8-paths | test.yml (unit-tests) |
| WS3-U-9 | WS-3 | scan_walker_parallel_threads_correct | unit | with-many-small-files | test.yml (unit-tests) |
| WS3-U-10 | WS-3 | scan_walker_additional_ignores_extends_defaults | unit | small-repo | test.yml (unit-tests) |
| WS3-U-11 | WS-3 | scan_walker_disable_default_ignores_flag_works | unit | small-repo | test.yml (unit-tests) |
| WS3-S-1 | WS-3 | secret_detector_pem_rsa_private_key | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-2 | WS-3 | secret_detector_pem_ec_private_key | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-3 | WS-3 | secret_detector_pem_openssh_private_key | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-4 | WS-3 | secret_detector_aws_access_key | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-5 | WS-3 | secret_detector_github_pat_classic | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-6 | WS-3 | secret_detector_github_pat_fine_grained | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-7 | WS-3 | secret_detector_jwt_with_confidence_threshold | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-8 | WS-3 | path_denylist_dotenv | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-9 | WS-3 | path_denylist_id_rsa_family | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-10 | WS-3 | path_denylist_ssh_dir | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-11 | WS-3 | path_denylist_aws_credentials | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-12 | WS-3 | path_denylist_kube_config | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-13 | WS-3 | path_denylist_pem_pfx_p12_jks | unit | with-secrets | test.yml (unit-tests) |
| WS3-S-14 | WS-3 | quarantine_event_records_path_and_reason | unit | with-secrets | test.yml (unit-tests) |
| WS4-I-1 | WS-4 | directory_scan_handler_completes_small_repo | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-2 | WS-4 | directory_scan_handler_idempotent | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-3 | WS-4 | directory_scan_handler_re_ingests_on_content_change | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-4 | WS-4 | scan_status_lifecycle_idle_scanning_idle | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-5 | WS-4 | scan_status_lifecycle_idle_scanning_error | integration (tx) | with-permission-denied | test.yml (integration) |
| WS4-I-6 | WS-4 | extraction_handler_path_access_gate_includes_referenced | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-7 | WS-4 | referenced_source_blob_storage_backend_is_referenced | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-8 | WS-4 | derived_artifact_storage_backend_is_filesystem | integration (tx) | with-mixed-media | test.yml (integration) |
| WS4-I-9 | WS-4 | scan_dedups_identical_files_within_archive | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-10 | WS-4 | scan_queues_extraction_jobs_per_file | integration (tx) | small-repo | test.yml (integration) |
| WS4-I-11 | WS-4 | job_type_directory_scan_variant_round_trips | unit | (none) | test.yml (unit-tests) |
| WS4-I-12 | WS-4 | scan_handler_logs_quarantine_events_to_db | integration (tx) | with-secrets | test.yml (integration) |
| WS6-I-1 | WS-6 | derived_storage_path_default_under_file_storage_path | unit | (none) | test.yml (unit-tests) |
| WS6-I-2 | WS-6 | derived_storage_path_env_override_respected | unit | (none) | test.yml (unit-tests) |
| WS6-I-3 | WS-6 | derived_subdir_created_per_archive_on_first_ingest | integration (tx) | with-mixed-media | test.yml (integration) |
| WS6-I-4 | WS-6 | drop_archive_removes_derived_subdir_only | integration (tx) | with-mixed-media | test.yml (integration) |
| WS6-I-5 | WS-6 | managed_archive_derived_artifacts_unchanged_behavior | integration (tx) | with-mixed-media | test.yml (integration) |
| WS6-I-6 | WS-6 | store_derived_attachment_routes_to_companion_for_referenced | integration (tx) | with-mixed-media | test.yml (integration) |
| WS7-I-1 | WS-7 | post_archives_referenced_succeeds_with_valid_path | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-2 | WS-7 | post_archives_referenced_rejects_nonexistent_path | integration (tx) | (none) | test.yml (integration) |
| WS7-I-3 | WS-7 | post_archives_referenced_rejects_path_outside_allowlist | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-4 | WS-7 | post_archives_referenced_no_allowlist_allows_any_path | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-5 | WS-7 | post_archives_referenced_canonicalizes_relative_path | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-6 | WS-7 | post_archives_referenced_rejects_traversal | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-7 | WS-7 | post_archives_rescan_returns_202_and_job_id | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-8 | WS-7 | get_archives_scan_status_returns_idle_after_completion | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-9 | WS-7 | get_archives_quarantined_files_returns_skipped_secrets | integration (tx) | with-secrets | test.yml (integration) |
| WS7-I-10 | WS-7 | referenced_archive_post_notes_returns_403 | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-11 | WS-7 | referenced_archive_put_attachment_returns_403 | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-12 | WS-7 | referenced_archive_delete_note_returns_403 | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-13 | WS-7 | referenced_archive_rescan_endpoint_allowed_under_write_gate | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-14 | WS-7 | referenced_archive_get_notes_returns_200 | integration (tx) | small-repo | test.yml (integration) |
| WS7-I-15 | WS-7 | referenced_archive_search_returns_200 | integration (tx) | small-repo | test.yml (integration) |
| WS7-E-1 | WS-7 | e2e_create_referenced_archive_and_search | e2e | small-repo | test.yml (e2e) |
| WS7-E-2 | WS-7 | e2e_rescan_picks_up_new_files | e2e | small-repo | test.yml (e2e) |
| WS7-E-3 | WS-7 | e2e_quarantine_workflow_end_to_end | e2e | with-secrets | test.yml (e2e) |
| WS8-I-1 | WS-8 | mcp_manage_archives_create_accepts_storage_mode_param | integration (mcp) | small-repo | test.yml (mcp-tests) |
| WS8-I-2 | WS-8 | mcp_manage_archives_create_accepts_source_path_param | integration (mcp) | small-repo | test.yml (mcp-tests) |
| WS8-I-3 | WS-8 | mcp_manage_archives_backward_compat_without_new_params | integration (mcp) | (none) | test.yml (mcp-tests) |
| WS8-I-4 | WS-8 | mcp_rescan_archive_tool_returns_job_id | integration (mcp) | small-repo | test.yml (mcp-tests) |
| WS8-I-5 | WS-8 | mcp_rescan_archive_tool_poll_completes | integration (mcp) | small-repo | test.yml (mcp-tests) |
| WS8-I-6 | WS-8 | mcp_tool_schema_validates_in_inspector | unit (node) | (none) | test.yml (mcp-tests) |
| WS8-I-7 | WS-8 | mcp_get_documentation_surfaces_new_capabilities | unit (node) | (none) | test.yml (mcp-tests) |
| WS9-S-1 | WS-9 | TI-EXTSTORAGE-1 cross-tenant archive listing | security regression | small-repo | test.yml (security) |
| WS9-S-2 | WS-9 | TI-EXTSTORAGE-2 path-traversal via API read | security regression | with-secrets, small-repo | test.yml (security) |
| WS9-S-3 | WS-9 | TI-EXTSTORAGE-3 scan job schema-context isolation | security regression | small-repo | test.yml (security) |
| WS9-S-4 | WS-9 | TI-EXTSTORAGE-4 symlink escape skipped+logged | security regression | with-symlink-escape | test.yml (security) |
| WS9-S-5 | WS-9 | TI-EXTSTORAGE-5 mount-disappearance degraded status | security regression | small-repo + runtime tmpfs | test.yml (security) |
| WS9-S-6 | WS-9 | TI-EXTSTORAGE-6 secret quarantine tenant-scoped | security regression | with-secrets | test.yml (security) |
| WS9-S-7 | WS-9 | TI-EXTSTORAGE-7 path-traversal canonicalization at create | security regression | small-repo | test.yml (security) |
| WS9-S-8 | WS-9 | TI-EXTSTORAGE-8 cross-tenant dedup independence | security regression | small-repo (×2) | test.yml (security) |
| WS9-S-9 | WS-9 | TI-EXTSTORAGE-9 drop-archive source preservation | security regression | small-repo | test.yml (security) |
| WS9-S-10 | WS-9 | TI-EXTSTORAGE-10 MCP cross-session protection | security regression | small-repo | test.yml (security) |
| WS10-D-1 | WS-10 | doc_sync_no_broken_mentions_in_referenced_storage_md | doc validation | (none) | docs.yml (lint) |
| WS10-D-2 | WS-10 | doc_sync_claude_md_referenced_section_present | doc validation | (none) | docs.yml (lint) |
| WS10-D-3 | WS-10 | api_reference_includes_new_endpoints | doc validation | (none) | docs.yml (lint) |
| BENCH-1 | WS-4 | bench_scan_10k_files_with_embedding | performance smoke | with-many-small-files | nightly-benchmarks.yml |
| BENCH-2 | WS-3 | bench_scan_walker_throughput | performance smoke | with-many-small-files, small-repo, with-large-files | nightly-benchmarks.yml |

---

## Per-Workstream Test Plan Summary

| Workstream | Test Cases | Unit | Integration | E2E | Security | Coverage Floor |
|---|---|---|---|---|---|---|
| WS-1 (Storage Backend Abstraction) | 10 | 8 | 2 | 0 | 0 | 90% |
| WS-2 (Schema and Registry) | 11 | 1 | 10 | 0 | 0 | 85% |
| WS-3 (Walker + Ignore + Secret-Scan) | 25 | 25 | 0 | 0 | 0 | 90% |
| WS-4 (Scan-and-Ingest Pipeline) | 12 | 1 | 11 | 0 | 0 | 80% |
| WS-6 (Derived Artifact Companion) | 6 | 2 | 4 | 0 | 0 | 85% |
| WS-7 (API Surface) | 18 | 0 | 15 | 3 | 0 | 85% |
| WS-8 (MCP Tool Surface) | 7 | 2 | 5 | 0 | 0 | 80% |
| WS-9 (Multi-Tenant Security) | 10 | 0 | 0 | 0 | 10 | 100% of TI-EXTSTORAGE-* |
| WS-10 (Docs) | 3 | 0 | 0 | 0 | 0 | n/a (doc-sync) |
| **Performance smoke** | 2 | n/a | n/a | n/a | n/a | informational only |
| **TOTAL** | **104** | 39 | 47 | 3 | 10 | aggregate 85% |

WS-5 (Live Update Detection) is deferred per synthesis Decision 4 and therefore has zero test cases in this plan.

---

## Notes on CI Job Mapping

- **test.yml (unit-tests)**: Runs `cargo test --workspace --lib` — fast, transaction-free, no DB tests.
- **test.yml (integration)**: Runs `cargo test --workspace --test '*'` against PG container. Most tests use `#[sqlx::test]` transactional rollback; the WS-2 migration tests use `#[tokio::test]` with manual pool per CLAUDE.md.
- **test.yml (e2e)**: Runs the small e2e subset against a locally-launched Fortemi instance.
- **test.yml (security)**: Dedicated job for WS9-S-* tests. Multi-tenant tests need two distinct schemas and OAuth clients; this isolation is cheaper as a separate job than mixed in with integration.
- **test.yml (mcp-tests)**: Node.js test runner in `mcp-server/`. Existing pattern from current MCP tool test coverage.
- **docs.yml (lint)**: doc-sync skill validates @-mentions resolve. New `docs/referenced-storage.md` must exist and not contain broken references.
- **nightly-benchmarks.yml** (new): Runs BENCH-* on schedule (nightly), records to a baseline file, alerts on ≥20% regression. Does not block PRs.

---

## Fixture Requirements Cross-Reference

| Fixture | Used by | Notes |
|---|---|---|
| `small-repo/` | 38 tests | Baseline; most-used fixture |
| `with-secrets/` | 17 tests | Critical for WS-3 secret tests + WS-7/WS-9 quarantine flows |
| `with-symlink-escape/` | 1 test (TI-EXTSTORAGE-4) | Symlink target must be filesystem-portable |
| `with-symlink-loop/` | 1 test (WS3-U-7) | `ignore` crate's built-in protection should pass |
| `with-large-files/` | 3 tests | Streaming hash + file-size cap |
| `with-non-utf8-paths/` | 1 test (WS3-U-8) | Gate on filesystem capability detection |
| `with-gitignore/` | 1 test (WS3-U-1) | Self-contained fixture with .gitignore |
| `with-permission-denied/` | 2 tests | Subdir mode 0; test setup script must chmod |
| `with-many-small-files/` | 3 tests (BENCH-1, BENCH-2, WS3-U-9) | Generated via script, committed (~1MB) |
| `with-overlapping-paths/` | 0 tests in this plan | Reserved for Q-6 follow-up; not required for v1 |
| `with-mixed-media/` | 4 tests | Tests WS-6 derived-artifact routing |
| `empty-dir/` | 0 tests in this plan | Optional edge case; add if construction discovers issue |
| `single-file-only/` | 0 tests in this plan | Optional edge case; add if construction discovers issue |

---

## Gating

Per `test-strategy.md` §6: WS-9 must reach 100% of TI-EXTSTORAGE-* before WS-9 is signed off. WS-1, WS-3, WS-6 must reach ≥85% line coverage of new code. Aggregate coverage of new code lines must be ≥85% before the epic is considered complete.

Performance smoke benchmarks (BENCH-1, BENCH-2) are informational. A 20%+ regression triggers a warning to the engineering team but does not block PRs.

## References

- @.aiwg/working/issue-planner-storage/synthesis.md — §3 Decisions, §4 Workstreams, §5 Risks
- @.aiwg/working/issue-planner-storage/testing/test-strategy.md — pyramid, fixtures, CI
- @.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md — TI-EXTSTORAGE-* detail
- @CLAUDE.md — Testing Standards, PostgreSQL Migration Compatibility
- @.claude/rules/anti-laziness.md — never skip/delete tests, never use `#[ignore]`
- @.claude/rules/vague-discretion.md — measurable completion criteria
