# Core build reuse

CI installs `scripts/ci/cargo-cache.py` as a Cargo wrapper from the workflow
revision, alongside existing run ownership tooling. Credentials, Docker config,
install prefixes, temporary files and raw LLVM profiles stay run-scoped. Main
and version-tag compilation use separate private cache namespaces. Pull requests,
other refs and unknown CI contexts keep cold run-local targets; they never restore
or populate these persistent targets. The release BuildKit builder is a new,
separate instance; PR caches are never imported into it.

Compiler `rustc -vV` (including host), dependency manifests/locks, Cargo configuration,
workflow bytes (the complete profile/feature selection), job identity and relevant
compiler environment form the target key. Ordinary Rust edits rely on Cargo's
fingerprint invalidation, preserving dependencies between runs. Each job gets its
own stable target; lint, tests, coverage, audit tool compilation and auth-consumer
jobs retain their distinct matrices. Cargo still executes every requested check.
The existing capacity-one runner serializes jobs; a private cache-root flock also
serializes Cargo payloads and retention. Nested Cargo subcommands inherit the lock.
Do not point another runner at this root; the lock is not a cross-host job lease.

Local repository identity uses the resolved Git common directory. Linked
worktrees share only through the same explicit private cache root and command
selection; independent local clones remain isolated even with identical remotes.
Non-Git trees and failed local Git probes keep directory isolation. Caller Git
overrides cannot redirect these probes.

Trusted CI uses a stable authority identity for `https://git.integrolabs.net`,
`Fortemi/fortemi`, because the native runner creates a new Git common directory
inside each ephemeral checkout. Admission still requires the existing workflow
main/version-tag push or dispatch lane, `GITHUB_ACTIONS=true`, exact server and
repository context, matching event/ref assertions, positive run/attempt IDs, and
the existing workflow tooling registration at
`target/.ci-tools/<run>-<attempt>.<24-hex>`. Its private owned directory and regular
private marker must match run, attempt, device and inode; symlink components are
rejected. Both source and tooling must be Git repository roots with exactly one
literal local origin matching the canonical HTTPS repository URL. Repository path
case is normalized; server, protocol and URL shape are fixed. Missing or mismatched
context fails cold before touching persistent storage. Remote URLs alone never
merge local clones. These assertions rely on the existing trusted workflow/runner
boundary; they do not authenticate hostile same-uid code or locally fabricated CI
environments. PRs, forks, wrong servers and unknown CI cannot borrow main targets.
Run IDs, SHAs and paths validate admission but do not enter the stable authority
key. All previous compiler/native inputs, manifests, locks, configuration,
workflows, features/flags/profile and variant invalidations remain. Main, release
and local lanes, permission rules and capacity limits are unchanged. Actual warm
reuse still needs subsequent CI evidence.

A cache-present event is not a build hit. Wrapper stderr telemetry reports actual
Cargo compiler-artifact totals, fresh/nonfresh counts, build-finished state, exit
code and elapsed seconds. Explicit message formats and external subcommands are
preserved; their artifact counts are null unless a nested supported Cargo command
reports them. Counts measure messages, not unique crates. No arguments or environment
values are written to Cargo invocation telemetry. The workflow proof step prints
only boolean target-match, cache-enabled and job-identity-presence results. Cargo diagnostics and test
output remain visible. Cold/disabled/fallback invocations preserve Cargo's normal
output format and report null artifact counts. Coverage subcommands still require
a real acceptance run to establish their full log/lcov behavior.

On Titan the actual Core runner is a native host runner, capacity one, with
`titan:host` / `matric-builder:host` labels. Its enabled Actions cache server uses
`/srv/gitea-runner-fortemi/cache`. These workflows explicitly select the private
`fortemi-cargo-cache-v1` child there for Cargo targets; this implementation uses
local disk and flock, not the cache server protocol. The service's cache server
being enabled alone does not populate these targets. The runner UID must be able
to create/access the private child, and the admitted disk must persist between
jobs. No pre-existing cache environment key is assumed. An operator must admit a
different root explicitly if that configured location changes; this source pass
does not change runner services/configuration or provision host permissions.

The persistent root is outside `GITHUB_WORKSPACE`: native runner checkout cleanup
can remove workspace files between tasks. Run-local credentials/tools/profiles
remain beneath registered workspace run roots and retain their existing cleanup
rules. Local use below still requires the bounded runner's admitted cache path.

The setup step evaluates job identity with `${{ github.job }}`, rather than the
unproven `gitea.job` alias, and writes it to `GITHUB_ENV`. Workflow/job environments
declare only `FORTEMI_CARGO_RUN_TARGET_DIR` as the cold-target input. Setup uses
that input to register the run root, then writes the selected `CARGO_TARGET_DIR`
to `GITHUB_ENV`; later steps have no competing workflow/job target value. A
separate `proof` step reports job-identity presence and target matching and fails if
an enabled entry does not match the effective target. Static workflow fixtures
enforce this wiring. A real main run must still establish nonempty identity,
`target_matches_entry: true`, and later Cargo `cache_enabled: true` telemetry;
source checks do not establish runner expression evaluation or a cache hit.

Private marked cache entries use fixed LRU retention limits. After the existing
workflow and repository authority checks admit CI main, that namespace has a
provisional 256 GiB aggregate byte ceiling and 16 entries. Release and local
namespaces retain their existing 24 GiB and six-entry ceilings. Finalization
revalidates CI authority before touching the persistent root. No environment
variable configures either budget. This increases the former trusted-main
64 GiB retention ceiling by 192 GiB within existing disk capacity; it provisions
no storage. These limits charge regular-file logical sizes, including
entry markers; symlink targets are never charged or followed.
Setup and each job's always-finalizer enforce retention;
compilation can temporarily exceed this retention ceiling. This is not a filesystem
quota. Setup validates entries under the root lock before pruning. A missing or
corrupt marker, changed identity/UID, unsafe symlink, inaccessible root or interrupted
entry selects a fresh target under the registered run's temporary directory. It
warns and reports `cache_enabled: false`, `cache_present: false`, and
`cache_state: unavailable`. Existing unsafe entries stay untouched; repeated runs
remain able to validate without cache reuse.

Before pruning, bounded public telemetry reports entry count/bytes, count/byte
limits, largest entry bytes, reservation, count/bytes/both/none pressure, and
oversized-generation state. Completion reports evicted count/bytes and retained
count/bytes plus whether the selected generation survived. Setup also reports its
actual post-creation inventory; finalization reports complete, skipped, or
unavailable. An unavailable/partial quarantine result has unknown post totals and
cannot establish useful retention. No cache paths, raw job identities, environment
values or private payloads are printed by these cache-state/proof records.
A single generation larger than its lane ceiling is evicted by finalization,
including the just-used generation: it cannot be warm on the next run. There is
no overage exception. Setup still protects a current hit; if that prevents meeting
the ceiling, setup fails closed to the existing fresh run-local target.
The observed main BuildUnit footprint is 59,281,679,902 bytes; comprehensive Fast
is 18,764,973,840 bytes and comprehensive Integration is 27,351,784,817 bytes.
Their sum, 105,398,438,559 bytes, already exceeds the former main 64 GiB ceiling
(68,719,476,736 bytes). Integration terminated on a required fixture flag; its
forward source correction does not turn that run into a successful receipt.
Eight genuine compiler profiles exist in source; coverage and slow profile
sizes remain unknown. The provisional 256 GiB/16 policy provides bounded
headroom, but configuration alone does not establish retention adequacy or
performance. Require actual complete-cycle inventories and finalization totals
across all compiler profiles, then unchanged-warm receipts with successful
workload status and compiler freshness before claiming production main reuse
or speedup. Local 24 GiB adequacy and full-suite performance remain unproven.
OpenAPI/source/contract failures remain independent.
Finalization is optional and warns on bookkeeping errors while preserving the Cargo/workload status (including
failures). Local finalization has the same rule. There is no automatic purge or
claim that an invalid retained entry has been repaired. An operator can inspect
the exact retained entry's ownership before any separately authorized recovery;
the disabled path below requires no deletion.
Do not finalize while external processes consume target files; the CI runner's
existing capacity-one policy is required. Same-uid hostile code is outside this
filesystem boundary; release runners/builders must remain trusted-only.

## Titan local use

Use the existing detached bounded runner, one job at a time. Prepare an owned,
private directory under `/var/tmp` through the parent's approved setup route, then:

```sh
RUNNER=/home/roctinam/dev/fortemi-suite/.aiwg/testing/scripts/local-test-runner.mjs
node "$RUNNER" start --cache-dir /var/tmp/fortemi-core-cargo-cache -- env \
  FORTEMI_CARGO_CACHE_ROOT=/var/tmp/fortemi-core-cargo-cache \
  CARGO_BUILD_JOBS=1 python3 scripts/ci/cargo-cache.py local \
  build --offline --locked -j1 -p matric-api --features hosted-auth,kms-vault
```

Poll the exact returned unit with runner `status` and `logs`; admission is not
completion. Use the same arguments for a warm comparison. Local keys include the
Cargo argument selection; features, profile and target changes select another
entry. Linked worktrees can select the same entry; Cargo may rebuild path-dependent
workspace crates while reusing registry dependencies. Missing
offline dependencies remain failures; this helper never fetches or stages them.
It does not change runner quotas, network policy, shared services or global Cargo
configuration. Preserve approved fixtures and required gates.

For a clean comparison use a new owned target and disable reuse:

```sh
node "$RUNNER" start --cache-dir /var/tmp/fortemi-core-cargo-cache -- env \
  FORTEMI_CACHE_DISABLE=1 CARGO_BUILD_JOBS=1 \
  CARGO_TARGET_DIR=/var/tmp/fortemi-core-cargo-cache/clean-target \
  python3 scripts/ci/cargo-cache.py local build --offline --locked -j1 \
  -p matric-api --features hosted-auth,kms-vault
```

Use a fresh clean-target name each baseline. This disables reuse without deleting
shared data. Retain receipts from cold, unchanged warm and source-edit invocations,
including successful exit, expected binary existence/hash and aggregate telemetry.
No speedup has yet been measured for this implementation.

## Docker and releases

Rust dependency mounts are locked, architecture/family-separated and keyed inside
the mount by compiler, system package versions, all Cargo manifests/locks,
available SQLx metadata, migration bytes, release profile, features, PKE selection,
and SQLx offline mode. Stamp-only changes keep the target generation. API and
bundle source builds copy the newly resolved binary out of the
mount; a missing binary fails the image build. Mount target retention is one input
generation and 12 GiB per family after output copying. Release builds pass the
explicit `api`, `hosted` or `bundle` family into the mount identity, retaining each
family through the sequential release builds and next release. Feature lists
stay inside the hashed key, since their commas are mount-option delimiters.
Default standalone Docker builds use `api`/`bundle`; custom feature families
should pass a distinct `FORTEMI_CACHE_FAMILY` token to retain both variants.
Only owned directory children with known 64-hex generation names are eligible for
retention removal; symlinks and unrelated children are left alone.
The dedicated release BuildKit builder
has a 24 GB GC retention policy; GC and compilation are not a hard disk quota.
Release `--no-cache` remains: APT signature/TLS, runtime freshness, immutable base
pins, source/tag checks, smoke tests, signed publication and release receipts are
unchanged. Full layer bypass can coexist with Cargo mount reuse. `--no-cache`
alone does not clear mounts; `--build-arg FORTEMI_CACHE_DISABLE=1` selects fresh
non-mounted Cargo home and target. No host-wide pruning is needed. Plain BuildKit
progress records actual cached steps; release logs also report elapsed seconds.
Version arguments moved after toolchain setup; bundle npm dependencies precede MCP
source copying. These source changes enable reuse, but do not establish its benefit.

Stamp audit: `matric-api/src/main.rs` reads `MATRIC_GIT_SHA` and
`MATRIC_BUILD_DATE` with `std::env::var` at startup/state construction. The API
version uses `env!("CARGO_PKG_VERSION")`, tracked through the manifests/lockfiles;
the Rust application does not consume `MATRIC_VERSION`. The only workspace
`build.rs`, in `matric-db`, declares `rerun-if-changed=../../migrations` and no
stamp environment inputs. Docker retains stamp ENV values in its builder and
runtime stages. The native helper passes them unchanged to Cargo. Neither helper
uses a cache-present event to skip compilation: Cargo always runs, fingerprints
ordinary source and `env!`/`option_env!` inputs, and honors build scripts' declared
`rerun-if-env-changed` inputs. A future build script that reads an environment
variable must declare that dependency. A real version bump that changes a manifest
or lockfile still selects a new generation; this correction only removes stamp
overkeying. Docker copies outputs only after successful Cargo completion and
fails if an expected binary is absent.

The release evidence policy in `docker/container-release-evidence-policy.json`
explicitly reviews three public cache build arguments alongside the existing
version, revision, date and feature arguments. They must never carry credentials
or other secrets. The review covers the `publish-release` wrapper in
`.gitea/workflows/ci-builder.yaml`:

| Argument | Docker default (API/hosted and bundle) | Release value | Meaning |
|---|---|---|---|
| `FORTEMI_CACHE_DISABLE` | `0` | `${FORTEMI_CACHE_DISABLE:-0}` | Exactly `1` uses non-mounted Cargo home and target directories and still compiles; `0` permits mount reuse. Other values do not disable reuse; the helper does not validate a boolean. |
| `FORTEMI_CACHE_FAMILY` | `api` in `Dockerfile`; `bundle` in `Dockerfile.bundle` | The literal wrapper label: `api`, `hosted`, `bundle`, or `mcp` | Separates compiled Rust target mounts. Hosted must override the API default to `hosted`. Registry mounts remain shared within a namespace and architecture. |
| `FORTEMI_CACHE_NAMESPACE` | `local-v1` | `release-v1` | Selects registry and target mount identities on the separate release builder. A namespace is not an authorization boundary against code with access to that builder. |

The wrapper also passes all three arguments to standalone MCP. `Dockerfile.mcp`
does not declare them, compile Rust, or use Cargo mounts. MCP's policy association
records the shared invocation and the verifier's whole-workflow scan; it does
not establish compiled-cache use. GLiNER, pyannote, builder and testdb have separate
workflow associations and are outside this cache-argument review.

The scoped review does not renew the policy's global review date or change its
pending signature, provenance, SBOM or license gates. These entries document
reviewed source behavior; they do not enforce argument values. The verifier's
explicit allowlist now names exactly these seven reviewed arguments. Manifest
equality and rejection of unknown workflow arguments remain required. Fresh
validation of this source integration is pending; no cache hit, Docker/CI
acceptance or speedup follows from this review.

For parent validation, run the Python cache/policy fixtures first in the existing
bounded runner, then a separate tiny real Rust 1.92 fixture with one already cached,
pinned registry dependency. Use two linked worktrees, one private cache root and
identical helper arguments. Record a cold build in A, unchanged build in A, then
build in B: the key/target must match and the registry artifact must be fresh in B.
Check successful exit and execute/hash the actual binary after every build. Edit
B's source and require changed output plus a rebuilt application artifact while
the registry artifact stays fresh. Change an embedded `option_env!("MATRIC_VERSION")`
stamp and require new output and a rebuilt application artifact with the same key
and fresh registry dependency. Also cover a fixture build script using
`rerun-if-env-changed=MATRIC_GIT_SHA` and verify its generated output changes.
Delete only the fixture binary and require Cargo to recreate it successfully.
Missing offline dependencies must fail; do not fetch them or substitute a Core
rebuild. Use explicit Cargo JSON output for per-package freshness evidence and
preserve all prior receipts. This is helper correctness proof, not a Core benchmark;
actual CI/Docker reuse and workload performance remain separate acceptance work.

`sccache` is not installed on the inspected PATH and is not declared in the builder
Dockerfile. It is intentionally not enabled or installed. Its Rust support requires
incremental compilation disabled and excludes several linker outputs; a future
approved integration needs actual installed-tool and workload evidence.

Sources: [Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html),
[Docker cache optimization](https://docs.docker.com/build/cache/optimize/),
[Docker invalidation](https://docs.docker.com/build/cache/invalidation/),
[BuildKit GC](https://docs.docker.com/build/cache/garbage-collection/),
[cargo-llvm-cov environment](https://github.com/taiki-e/cargo-llvm-cov#environment-variables),
[sccache Rust limits](https://github.com/mozilla/sccache/blob/main/docs/Rust.md).
