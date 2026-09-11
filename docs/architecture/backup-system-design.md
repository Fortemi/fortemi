# Backup System Architecture Design

Native auto membership uses the triggering note's schema and tenant for set
selection, criteria evaluation and inserted membership. Migration209 replaces
the public trigger function used by public and archive tables, including future
archives cloned from public. A shared invoker-privilege evaluator preserves tag,
collection, FTS, date, include-all and archive-exclusion rules without changing
the caller's search path or granting access to another tenant. Cross-tenant
composite FKs and FORCE RLS remain mandatory. No existing rows are backfilled.

Qualified archive note writes now enroll matching sets instead of silently
consulting public. Matching native memberships adopt only their own bootstrap
sets; imported notes and tombstones still suppress enrollment. The clone test
promotes an asserted automatic source membership to explicit membership before
copying it, retaining strict clone data-conflict behavior. This runtime fix does
not change shard wire/profile authority or qualify unreleased artifacts. Full
Lane B acceptance, exact-head CI and the cleanup/release sweep remain; NO-GO.

Cycle20 stages changed nullable concept URI and scheme/notation coordinates before
retained-ID replacement. Preview/apply reject a coordinate owned by an unselected
live concept; selected concepts may exchange coordinates without deleting IDs or
rewriting unselected children/replacement references. Apply acquires the shared
concept writer guard before row locks; preview does not stage or lock rows.
Both wire orders, repeat, Skip/Merge, observable deferred rollback and protected
references are tested. Native note-tag promotion also rejects over-capacity commit;
available capacity passes explicit final-constraint validation before test rollback.
No wire/profile/migration change. Full original Lane B and release gates remain;
suite NO-GO, exact2.0.0/full-v1 candidate evidence only.

Explicit full-wipe preview omits concept-coordinate destination checks because
apply deletes those selected roots first. Complete-archive validation remains;
ordinary replacement continues rejecting an unselected owner at the same key.
Tests preserve read-only preview, repeated wipe and actual post-wipe exports.
General partial-wipe foreign-key preview behavior is a separate acceptance scope.

Cycle19 extends the relation-writer guard to concept INSERT/UPDATE/DELETE and
adds deferred final-status breadth validation. A concept-only import must project
retained, unselected narrower relations: promoting a candidate cannot commit201
approved children. Skip/Merge conflicts retain their destination snapshot rather
than being overwritten by the final concept restoration pass. Balanced status
swaps validate at transaction end without changing relation identities or values.
Migration208 preserves existing relation validator/coordination function bodies,
adds no portable state and does not revise the shard schema or profile.

Native/import rejection and48 mixed-writer combinations pass, covering both writer
orders, three isolation modes, native/restore contexts, public/registered schemas
and first-writer commit/rollback. These scoped tests do not establish full native
writer coverage or released/platform qualification. Full original Lane B acceptance,
publication/pins, exact-head CI, delivery and cleanup/release gates remain open.
Suite NO-GO; exact2.0.0/full-v1 candidate evidence only.

Archive clone permits an already-created tenant-only coordination row when copying
`skos_relation_write_guard`, since preceding concept writes can create it. This
specific primary-key conflict is harmless; all data-table conflicts remain strict.
Clean clone tests check logical guard equality and both target concept triggers.

Cycle18 corrects two actual concurrent imports both committing invalid final
capacity/cycle graphs. An unconditional pre-statement relation-DML trigger uses a
physical no-op UPSERT of a tenant-keyed guard in each archive. Writers serialize
until commit/rollback; Read Committed sees the prior writer, while stale Repeatable
Read/Serializable writers abort40001 and require a whole-transaction retry.
The forced-RLS guard is local coordination, not a portable component or changed
declared record. Other tenants/archives remain independent. Ordinary immediate and
deferred final-state validators remain active and unchanged.

Actual handler barriers now show one waiting writer and one commit, exact retained
references/unselected snapshots, winner replay and loser-preview rejection.
Server146/migration17/clean archive17/required-live tenant23 pass, including24
isolation/context/schema/commit combinations and restricted-role guard checks.
Twenty-two actual exports pass44 pristine installed consumers with exact component,
key-presence and mandatory-byte comparison. Cycle18 receipts bind this evidence. Non-relation
writers, partial owner/status changes, narrower-only/inverse consistency, general
alternate-key permutations and performance remain unqualified. Full original Lane B
acceptance, publication/pins, runtime/platform/released checks, CI/delivery and
cleanup/releases remain. Suite NO-GO; exact2.0.0/full-v1 candidate evidence only.

Cycle17 corrects actual valid-batch preview/apply disagreement: intermediate
cycles and full-capacity parent moves previously rejected despite valid final
graphs. Migration206 preserves ordinary immediate guards and adds unconditional
deferred final-state validation to public/registered archives. Restore permits
intermediate states but cannot commit invalid final cardinality/hierarchy, even
after changing the restore flag or caller search path. Current retained rows are
resolved in the triggering schema; stale/deleted queued events do not validate
transient graphs. Existing native validator functions/OIDs remain unchanged.

Protected identities/references, both wire orders from the original graph, repeat,
read-only preview, exact unselected snapshots and reached deferred rollback pass.
Server144/migration16/clean archive17/required-live tenant22 and API/DB Clippy,
format/diff pass. Twenty actual exports pass40 pristine installed consumers with
exact components, key presence and mandatory bytes, bound by cycle17 receipts.
No deletion/recreation workaround, row backfill or schema/profile expansion.
Concurrency, partial owner/status, narrower-only/inverse consistency, general
alternate-key permutations and performance remain unqualified, along with full
original Lane B acceptance, publication/pins, CI/delivery and cleanup/releases.
Suite NO-GO; exact2.0.0/full-v1 candidate evidence only.

Cycle16 corrects successful shard apply of an invalid broader cycle and a root
extension that pushes an unselected descendant past depth five. Forward migration
20260911020500 retains the broader function/trigger identity but validates the
prospective graph, not cached target depth or the old pre-insert graph. Read-only
preview and apply planning use the same resulting-edge projection and include
affected descendants. Bounded, deduplicated traversal stops at six edges, rejecting
cycles and paths beyond five without recursive overflow or snapshot backfill.

Broader edges define this native hierarchy. Related edges and narrower-only
records are not inferred into additional broader identities. Narrower cardinality
validation is unchanged; unselected concept snapshots remain untouched. Native
and restore writes both enforce the new guard, preserving legitimate parent limits.

Two handler cases pass rejection in replace/skip/merge preview/apply, native
controls, protected references, exact native-state preservation, valid single-edge
reparenting/repeat and reached deferred rollback. Public/old-archive migration
tests cover bounded cycles/depth/self-loops, valid reparent/upsert, related controls,
bindings and rollback. Server142/migration15/archive17/required-live tenant22 and
API/DB Clippy/format/diff pass. Eighteen actual producer exports pass36 clean
installed-consumer destinations with exact components, key presence and bytes.
Multi-edge ordering, narrower-only/inverse consistency, concurrency
and partial owner/status changes remain separate gates, as do the full original
Lane B acceptance, publication/pins, CI/delivery, platform/released verification
and cleanup/releases. No schema/profile or historical-row repair; suite NO-GO.

The cycle15 retained-relation correction uses the effective native SKOS limits:
three broader parents and200 approved narrower children. Forward migration
20260911020400 excludes the retained relation before counting its resulting state,
preserving function identities and public/registered-archive trigger bindings.
Candidate children do not consume approved capacity; a retained edge retargeted
to an approved child does. Existing depth/cycle calls and numerical bounds remain
active in native and restore contexts. No stored row or prior migration is edited.

Actual replay-at-capacity tests reproduced two apply failures. Separate over-limit
archive tests reproduced false-success dry runs. A shared read-only projection now
checks resulting relation cardinality before preview/apply mutations, including
retained rows, selected-owner omissions, skip/merge conflicts and effective concept
statuses. Native triggers still enforce write-time limits. Tests retain protected
references and compare whole native snapshots for repeat/preview/rejection and
reached deferred-commit rollback. Ordinary native update and overflow controls pass.

Server140/migration14/pristine archive17/required-live tenant22 and API/DB Clippy,
format/diff pass. Sixteen actual exports pass32 clean installed-Core destinations,
including both boundary graphs, with all-component and mandatory-byte comparison.
Recursive cycle formation, reparenting/order at capacity, partial
child/status changes and concurrent writers are not qualified by these tests.
No schema/profile or consumer change; full original Lane B acceptance, shared
publication/pins, authenticated/platform/released checks, CI/delivery and lane
cleanup/releases remain. Suite NO-GO is unchanged.

The note-tag restore correction guards native note-count/lifecycle authoring in
shard context with forward migration20260911020300. Inserting a note assignment
must not auto-promote an unselected concept; selected-note omission must not
rewrite that concept's stored snapshot. Unselected concept snapshots remain
destination-owned; selected concepts use their declared source records. Actual
handler insertion/omission regressions now pass with preview, deferred rollback,
repeat and independent reference preservation. Ordinary native tagging still
performs count updates and candidate promotion. Original native function identity
and public/old-archive migration behavior are preserved. Partial owner changes,
native validator limits, concurrency and full original acceptance/publication/
pins/delivery/release qualification remain open. Suite NO-GO.

Cycle13 cross-runtime verification was red:11 scenarios passed across22 clean
destinations, but the new omission artifact is rejected before import because
exported provenance references revisions hidden by partial-note restore's
schema-2 presence reset. Local server136/migration13/archive17/tenant22 passing
does not qualify that artifact. Preserve the rejected bytes; fix unselected
history/current state and visibility rather than dropping provenance or relaxing
consumer validation. The historical failure remains preserved.

Cycle14 separates schema-2 note projections from native history authoring.
Notes-only and note/tag replay retain unselected rich original hashes/metadata,
revision identities, current pointers, presence flags and provenance. Existing
unselected rows are not hidden; new required scaffolding remains absent from rich
component export unless declared. Content changes do not create native revisions.
When flat content conflicts with an unselected declared original/current snapshot,
preview and apply reject before writes. Selecting the corresponding rich component
allows the coherent change; skip/merge owners remain untouched. Flat-only notes
without declared rich snapshots can still be edited and replayed.

Server138, migration13, clean archive17 and required-live tenant22 pass with exact
history/provenance repeat checks, read-only preview and reached deferred-commit
rollback. Local attachment policy scan timestamps are not immutable history and
are excluded from successful-repeat history assertions, but whole-native-table
snapshots remain exact for preview and failed apply. All14 actual producer exports
pass28 clean installed-Core destinations with all-component and mandatory-byte
comparison, including the former omission failure. No consumer validator or provenance
is removed, no schema/migration is changed, and no prior corruption is inferred or
backfilled. Full original acceptance, delivery and release gates remain open;
these candidate results do not change suite NO-GO.

The next unreleased SKOS correction separates relation restore from native
authoring. A third forward migration guards hierarchy/reciprocal triggers during
restore, preventing undeclared reciprocal UUIDs and changes to unselected concept
snapshots during relation-only replay. Ordinary native writes still create
reciprocals and update hierarchy counters/timestamps, with rollback preserved.
Actual-handler regressions2, broad server134, migration12, clean archive17 and
required-live tenant22 pass. Two actual producer exports pass four pristine
installed-Core destinations with all-component/mandatory-byte comparison.
Original fixture comparison allows omitted optional empty migration history;
consumer-hop manifest presence remains exact. Remaining note-count/lifecycle,
partial-selection/owner-change/native-validator/concurrency and full Lane B
acceptance/publication/pins/delivery/release gates remain open. Suite NO-GO.

The unreleased SKOS ownership correction now distinguishes initializer-created
scaffolding from live schemes using tenant-qualified creation custody. A forward
migration adds the identity relation without inferring custody for existing rows.
Native edits, imported updates and references retire custody transactionally;
last-reference deletion never restores it. Export omits untouched scaffolding,
not live roots. Only conflicting recorded unused defaults can be removed; live
identity conflicts reject in apply and read-only preview. Selected notation/URI
swaps preserve identities and references. A second forward migration guards the
collection timestamp trigger during restore while retaining native edit behavior.
No historical migration or wire profile changes. Current server132/migration11,
clean archive17 and required-live tenant22 checks pass. Eight producer exports
pass16 pristine installed-consumer destinations, including a live default scheme
also verified through a clean server round trip. Full original acceptance,
publication/pins, CI/platform/delivery and release gates remain; suite NO-GO.

Current Knowledge Shard policy is governed by ADR-102, not the historical draft
below. Ordinary `full-v1` replacement must preserve independent graph roots and
external references while reconciling selected-owner children. #1147's unreleased
correction updates retained notes/attachments in place and reconciles selected
graph children, tags, attachment omissions and outgoing links explicitly. Skip
filters owned descendants by actual selected-owner conflicts, and presence/count
writes are limited to applied notes/collections. New independent roots may still
reference existing records. Its
local HTTP regression preserves unrelated graph rows, incoming links and grants;
this is not complete cross-family or released-artifact qualification. SKOS,
embedding omissions and producer/consumer receipt gates remain.
Selected graph-edge and community-assignment omissions now exclude retained
primary keys, preserving native references and creation metadata. Assignments
move before nested-community deletion. Remaining assignments block omission;
community-only replacement checks unselected assignments during both read-only
planning and apply. Arbitrary external FK/trigger preview and concurrent-state
reservation are not implied. The fixed React relationship corpus is shared local
evidence, not published or released-platform qualification.
Schema-2 configuration roots now use archive-local identity declarations rather
than a global presence reset. Values remain in the shared live registry; export
adds selected-set dependencies. Migration preserves existing visible declarations,
native writes track the active schema, and tenant-qualified FKs/forced RLS guard
the relation. New archives do not inherit unrelated explicit config roots.
Typed value comparison rejects shard changes to configurations declared or
referenced by another archive, while identical reuse and sole-owner changes stay
valid. Membership omission cleanup requires both selected notes and selected
sets, preserving retained coordinates and either excluded endpoint. This corrects
the earlier unreleased single-set interpretation; it does not grant cleanup
authority from a shared set dependency declaration or delete referenced notes.
Vector omission also requires both non-null endpoints selected. Null/excluded
endpoints survive, retained vector IDs update in place, and changed incoming
coordinates use transaction-local nullable staging for swaps. Native uniqueness
and retained token references remain intact; unselected coordinate collisions
reject. Consumer native member pointers are preserved separately from the wire
projection. These changes do not qualify concurrency or complete conflict preview.
Ordinary vector replacement now rejects unselected coordinate occupants in both
dry-run and actual apply, as detailed below. Retained vector note changes also
detach optional token chunk pointers that would cross note ownership. Token IDs,
owners, payloads and timestamps survive; matching pointers remain, dry-run does
not mutate, and failure rolls back pointer changes. Later imports do not invent
missing associations. React preserves membership ownership, detaches invalid
native vector pointers and marks affected referenced-vector materializations
stale. This is not native adjunct wire transfer or comprehensive invalidation
for all writers and newly matching vectors.

The coordinate guard rejects unselected occupants consistently in both
read-only preview and actual apply, allowing selected IDs to vacate their keys.
Migration20260911010000 refreshes both old/new set statistics on vector movement,
including null-set transitions, using the triggering schema and tenant. Existing
trigger bindings survive the forward correction. Declared selected-set snapshots
still restore after apply; historical snapshot counts are not backfilled.
Same-note reparenting and rollback are tested. The note-owner association
correction above adds bounded native token/member/materialized checks;
comprehensive invalidation, concurrency and explicit-wipe preview remain open.
Set cleanup now requires
identity-only bootstrap creation custody; existing rows are never classified from
their names/system flags. Fresh/repair archive creation and the fresh public seed
runner record custody. Native edits and references adopt live ownership, while
validated restore suppresses automatic membership generation and explicitly adopts
declared parents. Set/name collisions with unmarked or adopted identities fail
even in dry-run. Only unused recorded seeds may be hidden or retired. Migration
interruption loses cleanup authority conservatively rather than inferring it.
Explicit wipe clears local configuration declarations instead of deleting the
shared registry, and still rejects changes to another archive's live values.
Repeated wipe, dry-run and late-template rollback preserve sibling native rows
and exports. The internal on-disk reader with production regeneration options
preserves old jobs and queues only target-schema work. Authenticated swap and
actual worker execution remain unqualified. Selected embedding sets may exchange
names/slugs in a batch while retaining identities and references. Ordered row
locks precede custody checks; temporary unique keys exist only inside the apply
transaction and roll back on failure. Unselected live collisions still reject.
Partial-selection/skip/repeat HTTP tests use an injected archive context, not
authenticated routing. Concurrent writers and authenticated/public archive
qualification remain open; this is not complete shared-registry qualification.
History originals and revisions update in place. Omission cleanup requires
explicit note owners; retained pointers and activities move before omitted
revisions are removed. Reference conflicts reject the apply transaction.
Provenance edge cleanup uses revision owners, activity cleanup uses note owners,
and unified records use note/attachment owners while preserving retained IDs.
Named locations, locations and devices are independent roots, not global-delete
targets during ordinary replacement. Forward restore-trigger guards preserve
authoritative history, edit flags and timestamps without disabling native edits
after commit. Local partial/full/repeat/empty/late-rollback tests have bounded
scope. Ordinary replace dry-run projects post-import references using read-only
queries and rejects retained activity/revision references consistently with apply,
including partial selections and unrelated current pointers. Actual cleanup still
checks current references after writes. This does not qualify concurrent writers,
arbitrary trigger errors, explicit-wipe preview, alternate-key exchanges or consumer
alignment.
Destructive swap remains a separately explicit operation;
the suite-wide NO-GO boundary is unchanged.

## Document Information

| Field | Value |
|-------|-------|
| Status | Draft |
| Created | 2026-01-17 |
| Author | Architecture Designer |
| Version | 1.0.0 |

---

## 1. System Overview

### 1.1 Purpose

Design a comprehensive backup and restore system for matric-memory that provides:
- Bulk export of all notes to portable shard formats (JSON/Markdown)
- Import/restore capability from knowledge shards
- Automated scheduled backups with rotation and retention
- Multiple destination support (local, network, S3-compatible)
- MCP integration for AI-assisted backup operations

### 1.2 High-Level Architecture

```
+------------------------------------------------------------------+
|                        MATRIC-MEMORY                              |
+------------------------------------------------------------------+
|                                                                   |
|  +-------------------+     +-------------------+                  |
|  |   matric-api      |     |   mcp-server      |                  |
|  |                   |     |                   |                  |
|  | /backup/export    |<--->| export_all_notes  |                  |
|  | /backup/import    |     | import_notes      |                  |
|  | /backup/status    |     | backup_now        |                  |
|  +--------+----------+     | backup_status     |                  |
|           |                +-------------------+                  |
|           v                                                       |
|  +-------------------+                                            |
|  | matric-backup     |  (New Crate)                               |
|  |                   |                                            |
|  | - Archive Builder |                                            |
|  | - Archive Reader  |                                            |
|  | - Progress Track  |                                            |
|  | - Encryption      |                                            |
|  +--------+----------+                                            |
|           |                                                       |
+-----------|-------------------------------------------------------+
            |
            v
+------------------------------------------------------------------+
|                     BACKUP DESTINATIONS                           |
+------------------------------------------------------------------+
|                                                                   |
|  +-------------+  +-------------+  +-------------+  +----------+  |
|  |   Local     |  |   rsync     |  |     S3      |  |   SMB/   |  |
|  |   Disk      |  |   Target    |  | Compatible  |  |   NFS    |  |
|  +-------------+  +-------------+  +-------------+  +----------+  |
|                                                                   |
+------------------------------------------------------------------+
            |
            v
+------------------------------------------------------------------+
|                     AUTOMATION LAYER                              |
+------------------------------------------------------------------+
|                                                                   |
|  +---------------------------+  +-----------------------------+   |
|  |  matric-backup.sh         |  |  matric-backup.timer        |   |
|  |                           |  |  matric-backup.service      |   |
|  |  - CLI interface          |  |                             |   |
|  |  - Rotation policy        |  |  - Systemd scheduling       |   |
|  |  - Notification hooks     |  |  - Failure alerting         |   |
|  +---------------------------+  +-----------------------------+   |
|                                                                   |
+------------------------------------------------------------------+
```

### 1.3 Data Flow Diagram

```
EXPORT FLOW:
============

  [API Request]
       |
       v
  [Validate Auth + Scope]
       |
       v
  [Query All Notes]----> [Query Collections]----> [Query Tags]
       |                        |                      |
       +------------------------+----------------------+
       |
       v
  [Build Archive Structure]
       |
       +---> [notes/]
       |        +---> {id}.json (metadata + content)
       |        +---> {id}.md (optional markdown export)
       |
       +---> [collections/]
       |        +---> collections.json
       |
       +---> [tags/]
       |        +---> tags.json
       |
       +---> [templates/]
       |        +---> templates.json
       |
       +---> manifest.json (version, timestamp, counts)
       |
       v
  [Compress Archive] (gzip/zstd)
       |
       v
  [Optional Encryption] (age/GPG)
       |
       v
  [Stream to Destination]


IMPORT FLOW:
============

  [Upload/Stream Archive]
       |
       v
  [Decrypt if Encrypted]
       |
       v
  [Decompress]
       |
       v
  [Validate Manifest]
       |
       v
  [Conflict Resolution Strategy]
       |
       +---> skip_existing
       +---> overwrite
       +---> merge
       +---> rename_new
       |
       v
  [Import Collections First]
       |
       v
  [Import Notes (batch)]
       |
       v
  [Import Tags]
       |
       v
  [Queue NLP Pipeline Jobs]
       |
       v
  [Return Import Summary]
```

---

## 2. Component Breakdown

### 2.1 New Crate: `matric-backup`

```
crates/matric-backup/
  Cargo.toml
  src/
    lib.rs
    archive/
      mod.rs
      builder.rs      # Archive creation
      reader.rs       # Archive parsing
      format.rs       # Shard format definitions
    compression/
      mod.rs
      gzip.rs
      zstd.rs
    encryption/
      mod.rs
      age.rs          # age encryption (modern, simple)
    progress/
      mod.rs
      tracker.rs      # Progress reporting
    destinations/
      mod.rs
      local.rs        # Local filesystem
      s3.rs           # S3-compatible storage
      rsync.rs        # rsync protocol
    error.rs
```

### 2.2 Archive Format Specification

```json
// manifest.json
{
  "version": "1.0.0",
  "format": "matric-backup",
  "created_at": "2026-01-17T12:00:00Z",
  "created_by": "matric-api/0.2.0",
  "source_instance": "memory.integrolabs.net",
  "encryption": null | { "method": "age", "recipient_count": 1 },
  "compression": "zstd",
  "counts": {
    "notes": 1234,
    "collections": 15,
    "tags": 89,
    "templates": 5,
    "links": 3456,
    "embeddings_excluded": true
  },
  "options": {
    "include_original": true,
    "include_revised": true,
    "include_embeddings": false,
    "include_links": true,
    "include_revision_history": false
  },
  "checksum": "sha256:abc123..."
}
```

```json
// notes/{uuid}.json
{
  "id": "uuid",
  "collection_id": "uuid | null",
  "format": "markdown",
  "source": "api",
  "created_at_utc": "ISO8601",
  "updated_at_utc": "ISO8601",
  "starred": false,
  "archived": false,
  "title": "Note Title",
  "metadata": {},
  "original": {
    "content": "...",
    "hash": "sha256:...",
    "user_created_at": "ISO8601"
  },
  "revised": {
    "content": "...",
    "ai_metadata": {},
    "model": "llama3.3:70b"
  },
  "tags": ["tag1", "tag2"],
  "links": [
    {
      "to_note_id": "uuid",
      "kind": "semantic",
      "score": 0.85
    }
  ]
}
```

### 2.3 Configuration Schema

```yaml
# /etc/matric-memory/backup.yaml or ~/.config/matric-memory/backup.yaml

backup:
  # Shard format options
  format:
    include_original: true
    include_revised: true
    include_embeddings: false  # Large, can be regenerated
    include_revision_history: false
    include_links: true
    content_format: "both"  # "json" | "markdown" | "both"

  # Compression
  compression:
    algorithm: "zstd"  # "gzip" | "zstd" | "none"
    level: 3           # 1-19 for zstd, 1-9 for gzip

  # Encryption (optional)
  encryption:
    enabled: false
    method: "age"      # "age" | "gpg"
    recipients:        # age public keys or GPG key IDs
      - "age1..."

  # Destinations (can specify multiple)
  destinations:
    - name: "local-primary"
      type: "local"
      path: "/var/backups/matric-memory"
      enabled: true

    - name: "s3-offsite"
      type: "s3"
      endpoint: "https://s3.amazonaws.com"  # or MinIO URL
      bucket: "matric-backups"
      prefix: "prod/"
      region: "us-east-1"
      # Credentials from env: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
      enabled: true

    - name: "rsync-nas"
      type: "rsync"
      target: "backup@nas.local:/backups/matric"
      ssh_key: "/root/.ssh/backup_key"
      enabled: false

    - name: "smb-share"
      type: "smb"
      share: "//fileserver/backups"
      mount_point: "/mnt/backups"
      credentials_file: "/etc/matric-memory/smb.creds"
      enabled: false

  # Retention policy
  retention:
    # Keep backups by age
    keep_daily: 7      # Keep daily backups for 7 days
    keep_weekly: 4     # Keep weekly backups for 4 weeks
    keep_monthly: 12   # Keep monthly backups for 12 months
    keep_yearly: 3     # Keep yearly backups for 3 years

    # Size limits (per destination)
    max_total_size_gb: 100

    # Minimum retention regardless of policy
    min_backups: 3

  # Scheduling (for systemd timer)
  schedule:
    full_backup: "daily"    # "hourly" | "daily" | "weekly"
    time: "02:00"           # Time for daily backup
    day_of_week: "sunday"   # For weekly backups

  # Notifications
  notifications:
    on_success: false
    on_failure: true
    webhook_url: "https://hooks.slack.com/..."
    email: "admin@example.com"
```

---

## 3. API Endpoint Specifications

### 3.1 Export Endpoints

#### `POST /api/v1/backup/export`

Start a new backup export job.

**Request:**
```json
{
  "format": "archive",        // "archive" | "markdown-zip"
  "compression": "zstd",      // "gzip" | "zstd" | "none"
  "include_original": true,
  "include_revised": true,
  "include_embeddings": false,
  "include_links": true,
  "include_templates": true,
  "filter": {                 // Optional filters
    "collection_ids": ["uuid1", "uuid2"],
    "tags": ["important"],
    "created_after": "2025-01-01T00:00:00Z",
    "created_before": null,
    "starred_only": false
  }
}
```

**Response:**
```json
{
  "job_id": "uuid",
  "status": "pending",
  "estimated_notes": 1234,
  "message": "Export job queued"
}
```

#### `GET /api/v1/backup/export/:job_id`

Check export job status.

**Response:**
```json
{
  "job_id": "uuid",
  "status": "processing",      // "pending" | "processing" | "completed" | "failed"
  "progress_percent": 45,
  "notes_processed": 556,
  "notes_total": 1234,
  "current_phase": "notes",    // "notes" | "collections" | "compressing" | "encrypting"
  "started_at": "ISO8601",
  "estimated_completion": "ISO8601",
  "error": null
}
```

#### `GET /api/v1/backup/export/:job_id/download`

Download completed export archive.

**Response:** Binary stream with headers:
```
Content-Type: application/octet-stream
Content-Disposition: attachment; filename="matric-backup-2026-01-17T120000Z.tar.zst"
Content-Length: 12345678
X-Backup-Checksum: sha256:abc123...
```

#### `GET /api/v1/backup/exports`

List recent export jobs.

**Response:**
```json
{
  "exports": [
    {
      "job_id": "uuid",
      "status": "completed",
      "created_at": "ISO8601",
      "completed_at": "ISO8601",
      "size_bytes": 12345678,
      "notes_count": 1234,
      "download_expires_at": "ISO8601"
    }
  ]
}
```

### 3.2 Import Endpoints

#### `POST /api/v1/backup/import`

Start a new import job.

**Request:** Multipart form data with:
- `archive`: The knowledge shard file
- `options`: JSON string:
```json
{
  "conflict_strategy": "skip_existing",  // "skip_existing" | "overwrite" | "merge" | "rename_new"
  "reprocess_nlp": true,                 // Queue NLP jobs for imported notes
  "preserve_ids": false,                 // Keep original UUIDs if possible
  "target_collection_id": null,          // Import all to specific collection
  "dry_run": false                       // Validate without importing
}
```

**Response:**
```json
{
  "job_id": "uuid",
  "status": "pending",
  "message": "Import job queued",
  "archive_info": {
    "version": "1.0.0",
    "notes_count": 1234,
    "created_at": "ISO8601",
    "source_instance": "memory.integrolabs.net"
  }
}
```

#### `GET /api/v1/backup/import/:job_id`

Check import job status.

**Response:**
```json
{
  "job_id": "uuid",
  "status": "processing",
  "progress_percent": 30,
  "current_phase": "notes",
  "stats": {
    "notes_processed": 370,
    "notes_total": 1234,
    "notes_imported": 350,
    "notes_skipped": 20,
    "notes_failed": 0,
    "collections_imported": 15,
    "tags_imported": 45,
    "nlp_jobs_queued": 350
  },
  "errors": [],
  "started_at": "ISO8601"
}
```

### 3.3 Status Endpoints

#### `GET /api/v1/backup/status`

Get overall backup system status.

**Response:**
```json
{
  "last_backup": {
    "timestamp": "ISO8601",
    "type": "full",
    "destination": "local-primary",
    "notes_count": 1234,
    "size_bytes": 12345678,
    "duration_seconds": 45
  },
  "next_scheduled": {
    "timestamp": "ISO8601",
    "type": "full"
  },
  "destinations": [
    {
      "name": "local-primary",
      "type": "local",
      "status": "healthy",
      "space_available_gb": 450,
      "backups_stored": 12
    },
    {
      "name": "s3-offsite",
      "type": "s3",
      "status": "healthy",
      "backups_stored": 30
    }
  ],
  "retention_status": {
    "total_backups": 42,
    "total_size_gb": 15.6,
    "oldest_backup": "ISO8601",
    "next_cleanup": "ISO8601"
  }
}
```

---

## 4. MCP Tool Specifications

### 4.1 export_all_notes

```javascript
{
  name: "export_all_notes",
  description: `Export all notes from the knowledge base to a portable archive.

Creates a compressed archive containing:
- All notes with original and revised content
- Collections and hierarchy
- Tags and tag associations
- Templates
- Semantic links between notes

Options:
- format: Shard format ("archive" for .tar.zst, "markdown-zip" for .zip of .md files)
- include_embeddings: Include vector embeddings (large, usually false)
- filter: Optional filters (collection_ids, tags, date range)

Returns a job_id to track progress. Use backup_status to monitor.
Download the shard when complete via the API.`,
  inputSchema: {
    type: "object",
    properties: {
      format: {
        type: "string",
        enum: ["archive", "markdown-zip"],
        description: "Export format",
        default: "archive"
      },
      include_embeddings: {
        type: "boolean",
        description: "Include vector embeddings (large)",
        default: false
      },
      filter: {
        type: "object",
        description: "Optional filters",
        properties: {
          collection_ids: { type: "array", items: { type: "string" } },
          tags: { type: "array", items: { type: "string" } },
          created_after: { type: "string" },
          created_before: { type: "string" },
          starred_only: { type: "boolean" }
        }
      }
    }
  }
}
```

### 4.2 import_notes

```javascript
{
  name: "import_notes",
  description: `Import notes from a knowledge shard.

Restores notes from a previously exported archive. Handles:
- Notes with original and revised content
- Collections (creates if not exist)
- Tags
- Templates

Conflict strategies:
- skip_existing: Skip notes that already exist (by ID)
- overwrite: Replace existing notes
- merge: Keep existing, add new only
- rename_new: Import with new IDs

After import, notes are queued for NLP processing unless disabled.`,
  inputSchema: {
    type: "object",
    properties: {
      archive_url: {
        type: "string",
        description: "URL to fetch archive from (or use upload endpoint)"
      },
      conflict_strategy: {
        type: "string",
        enum: ["skip_existing", "overwrite", "merge", "rename_new"],
        default: "skip_existing"
      },
      reprocess_nlp: {
        type: "boolean",
        description: "Queue NLP jobs for imported notes",
        default: true
      },
      dry_run: {
        type: "boolean",
        description: "Validate without importing",
        default: false
      }
    },
    required: ["archive_url"]
  }
}
```

### 4.3 backup_now

```javascript
{
  name: "backup_now",
  description: `Trigger an immediate backup to configured destinations.

Runs the backup script with current configuration, creating a new backup
and shipping to all enabled destinations.

Returns immediately with job_id. Use backup_status to monitor progress.`,
  inputSchema: {
    type: "object",
    properties: {
      destinations: {
        type: "array",
        items: { type: "string" },
        description: "Specific destinations (default: all enabled)"
      },
      type: {
        type: "string",
        enum: ["full", "incremental"],
        description: "Backup type (incremental not yet implemented)",
        default: "full"
      }
    }
  }
}
```

### 4.4 backup_status

```javascript
{
  name: "backup_status",
  description: `Get the status of the backup system.

Returns:
- Last successful backup details
- Next scheduled backup
- Destination health status
- Retention policy status
- Active backup/restore job progress`,
  inputSchema: {
    type: "object",
    properties: {
      job_id: {
        type: "string",
        description: "Specific job ID to check (optional)"
      }
    }
  }
}
```

---

## 5. Backup Shell Script

### 5.1 Script: `/usr/local/bin/matric-backup.sh`

```bash
#!/usr/bin/env bash
#
# matric-backup.sh - Automated backup script for matric-memory
#
# Usage: matric-backup.sh [options]
#   -c, --config FILE    Configuration file (default: /etc/matric-memory/backup.yaml)
#   -d, --destination    Specific destination name
#   -t, --type           Backup type: full (default), incremental
#   -n, --dry-run        Show what would be done
#   -v, --verbose        Verbose output
#   -h, --help           Show this help
#

set -euo pipefail

# Default configuration
CONFIG_FILE="${MATRIC_BACKUP_CONFIG:-/etc/matric-memory/backup.yaml}"
API_BASE="${MATRIC_MEMORY_URL:-http://localhost:3000}"
API_KEY="${MATRIC_MEMORY_API_KEY:-}"
BACKUP_TYPE="full"
DRY_RUN=false
VERBOSE=false
SPECIFIC_DEST=""

# Logging
log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }
log_verbose() { $VERBOSE && log "$*" || true; }
log_error() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" >&2; }

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    -c|--config) CONFIG_FILE="$2"; shift 2 ;;
    -d|--destination) SPECIFIC_DEST="$2"; shift 2 ;;
    -t|--type) BACKUP_TYPE="$2"; shift 2 ;;
    -n|--dry-run) DRY_RUN=true; shift ;;
    -v|--verbose) VERBOSE=true; shift ;;
    -h|--help) show_help; exit 0 ;;
    *) log_error "Unknown option: $1"; exit 1 ;;
  esac
done

# Load configuration
load_config() {
  if [[ ! -f "$CONFIG_FILE" ]]; then
    log_error "Config file not found: $CONFIG_FILE"
    exit 1
  fi
  # Parse YAML config (requires yq)
  COMPRESSION=$(yq -r '.backup.compression.algorithm // "zstd"' "$CONFIG_FILE")
  COMPRESSION_LEVEL=$(yq -r '.backup.compression.level // 3' "$CONFIG_FILE")
  ENCRYPT_ENABLED=$(yq -r '.backup.encryption.enabled // false' "$CONFIG_FILE")
  LOCAL_PATH=$(yq -r '.backup.destinations[] | select(.type == "local" and .enabled == true) | .path // empty' "$CONFIG_FILE" | head -1)
  KEEP_DAILY=$(yq -r '.backup.retention.keep_daily // 7' "$CONFIG_FILE")
  KEEP_WEEKLY=$(yq -r '.backup.retention.keep_weekly // 4' "$CONFIG_FILE")
  KEEP_MONTHLY=$(yq -r '.backup.retention.keep_monthly // 12' "$CONFIG_FILE")
}

# Create backup via API
create_backup() {
  log "Starting $BACKUP_TYPE backup..."

  # Request export
  RESPONSE=$(curl -s -X POST \
    -H "Authorization: Bearer $API_KEY" \
    -H "Content-Type: application/json" \
    -d '{
      "format": "archive",
      "compression": "'"$COMPRESSION"'",
      "include_original": true,
      "include_revised": true,
      "include_embeddings": false,
      "include_links": true
    }' \
    "$API_BASE/api/v1/backup/export")

  JOB_ID=$(echo "$RESPONSE" | jq -r '.job_id')
  if [[ -z "$JOB_ID" || "$JOB_ID" == "null" ]]; then
    log_error "Failed to start backup: $RESPONSE"
    exit 1
  fi

  log "Export job started: $JOB_ID"

  # Poll for completion
  while true; do
    sleep 5
    STATUS_RESPONSE=$(curl -s -H "Authorization: Bearer $API_KEY" \
      "$API_BASE/api/v1/backup/export/$JOB_ID")

    STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')
    PROGRESS=$(echo "$STATUS_RESPONSE" | jq -r '.progress_percent')

    log_verbose "Status: $STATUS ($PROGRESS%)"

    case "$STATUS" in
      completed)
        log "Export completed"
        break
        ;;
      failed)
        ERROR=$(echo "$STATUS_RESPONSE" | jq -r '.error')
        log_error "Export failed: $ERROR"
        exit 1
        ;;
      pending|processing)
        continue
        ;;
      *)
        log_error "Unknown status: $STATUS"
        exit 1
        ;;
    esac
  done

  echo "$JOB_ID"
}

# Download knowledge shard
download_backup() {
  local job_id="$1"
  local output_dir="$2"
  local timestamp=$(date '+%Y%m%d_%H%M%S')
  local filename="matric-backup-${timestamp}.tar.${COMPRESSION}"

  log "Downloading backup to $output_dir/$filename..."

  curl -s -H "Authorization: Bearer $API_KEY" \
    -o "$output_dir/$filename" \
    "$API_BASE/api/v1/backup/export/$job_id/download"

  if [[ ! -f "$output_dir/$filename" ]]; then
    log_error "Download failed"
    exit 1
  fi

  log "Downloaded: $output_dir/$filename ($(stat -c%s "$output_dir/$filename") bytes)"
  echo "$output_dir/$filename"
}

# Apply retention policy
apply_retention() {
  local backup_dir="$1"

  log "Applying retention policy..."

  # Find and categorize backups
  local now=$(date +%s)
  local day=$((24 * 60 * 60))

  # Keep daily for KEEP_DAILY days
  # Keep weekly for KEEP_WEEKLY weeks (Sunday backups)
  # Keep monthly for KEEP_MONTHLY months (1st of month backups)

  find "$backup_dir" -name "matric-backup-*.tar.*" -type f -mtime "+$KEEP_DAILY" | while read -r file; do
    local file_date=$(stat -c%Y "$file")
    local age_days=$(( (now - file_date) / day ))
    local file_dow=$(date -d "@$file_date" +%u)  # Day of week (7=Sunday)
    local file_dom=$(date -d "@$file_date" +%d)  # Day of month

    # Keep weekly (Sunday) for KEEP_WEEKLY weeks
    if [[ "$file_dow" == "7" && "$age_days" -le $((KEEP_WEEKLY * 7)) ]]; then
      log_verbose "Keeping weekly: $file"
      continue
    fi

    # Keep monthly (1st) for KEEP_MONTHLY months
    if [[ "$file_dom" == "01" && "$age_days" -le $((KEEP_MONTHLY * 30)) ]]; then
      log_verbose "Keeping monthly: $file"
      continue
    fi

    # Delete if past retention
    if $DRY_RUN; then
      log "Would delete: $file"
    else
      log "Deleting: $file"
      rm -f "$file"
    fi
  done
}

# Ship to S3
ship_to_s3() {
  local file="$1"
  local bucket=$(yq -r '.backup.destinations[] | select(.type == "s3") | .bucket' "$CONFIG_FILE")
  local prefix=$(yq -r '.backup.destinations[] | select(.type == "s3") | .prefix // ""' "$CONFIG_FILE")
  local endpoint=$(yq -r '.backup.destinations[] | select(.type == "s3") | .endpoint // ""' "$CONFIG_FILE")

  local filename=$(basename "$file")
  local s3_path="s3://${bucket}/${prefix}${filename}"

  log "Uploading to S3: $s3_path"

  local aws_opts=""
  [[ -n "$endpoint" ]] && aws_opts="--endpoint-url $endpoint"

  if $DRY_RUN; then
    log "Would upload: $file -> $s3_path"
  else
    aws $aws_opts s3 cp "$file" "$s3_path"
    log "Uploaded to S3"
  fi
}

# Ship via rsync
ship_via_rsync() {
  local file="$1"
  local target=$(yq -r '.backup.destinations[] | select(.type == "rsync") | .target' "$CONFIG_FILE")
  local ssh_key=$(yq -r '.backup.destinations[] | select(.type == "rsync") | .ssh_key // ""' "$CONFIG_FILE")

  local rsync_opts="-avz"
  [[ -n "$ssh_key" ]] && rsync_opts="$rsync_opts -e 'ssh -i $ssh_key'"

  log "Syncing to: $target"

  if $DRY_RUN; then
    log "Would rsync: $file -> $target"
  else
    rsync $rsync_opts "$file" "$target/"
    log "Rsync complete"
  fi
}

# Send notification
send_notification() {
  local status="$1"
  local message="$2"

  local webhook=$(yq -r '.backup.notifications.webhook_url // ""' "$CONFIG_FILE")
  local notify_success=$(yq -r '.backup.notifications.on_success // false' "$CONFIG_FILE")
  local notify_failure=$(yq -r '.backup.notifications.on_failure // true' "$CONFIG_FILE")

  if [[ "$status" == "success" && "$notify_success" != "true" ]]; then
    return
  fi
  if [[ "$status" == "failure" && "$notify_failure" != "true" ]]; then
    return
  fi

  if [[ -n "$webhook" ]]; then
    curl -s -X POST -H "Content-Type: application/json" \
      -d '{"text": "Matric Memory Backup: '"$message"'"}' \
      "$webhook"
  fi
}

# Main
main() {
  load_config

  # Create backup
  JOB_ID=$(create_backup)

  # Download to local
  if [[ -n "$LOCAL_PATH" ]]; then
    mkdir -p "$LOCAL_PATH"
    BACKUP_FILE=$(download_backup "$JOB_ID" "$LOCAL_PATH")
    apply_retention "$LOCAL_PATH"
  else
    # Download to temp
    BACKUP_FILE=$(download_backup "$JOB_ID" "/tmp")
  fi

  # Ship to other destinations
  for dest_type in $(yq -r '.backup.destinations[] | select(.enabled == true) | .type' "$CONFIG_FILE"); do
    case "$dest_type" in
      s3) ship_to_s3 "$BACKUP_FILE" ;;
      rsync) ship_via_rsync "$BACKUP_FILE" ;;
    esac
  done

  # Cleanup temp if used
  if [[ -z "$LOCAL_PATH" ]]; then
    rm -f "$BACKUP_FILE"
  fi

  log "Backup complete"
  send_notification "success" "Backup completed successfully"
}

# Run
main "$@"
```

### 5.2 Systemd Timer

```ini
# /etc/systemd/system/matric-backup.timer
[Unit]
Description=Matric Memory Backup Timer
Documentation=https://github.com/integrolabs/matric-memory

[Timer]
OnCalendar=*-*-* 02:00:00
Persistent=true
RandomizedDelaySec=300

[Install]
WantedBy=timers.target
```

```ini
# /etc/systemd/system/matric-backup.service
[Unit]
Description=Matric Memory Backup
Documentation=https://github.com/integrolabs/matric-memory
After=network.target matric-api.service

[Service]
Type=oneshot
ExecStart=/usr/local/bin/matric-backup.sh -v
User=matric
Group=matric
Environment=MATRIC_BACKUP_CONFIG=/etc/matric-memory/backup.yaml
Environment=MATRIC_MEMORY_API_KEY=<API_KEY>

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=matric-backup

# Security hardening
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/var/backups/matric-memory

[Install]
WantedBy=multi-user.target
```

---

## 6. Incremental Backup Strategy

### 6.1 Change Tracking

For future incremental backup support, track changes using:

```sql
-- Add to initial schema migration
ALTER TABLE note ADD COLUMN backup_sequence BIGINT DEFAULT 0;
CREATE INDEX idx_note_backup_sequence ON note(backup_sequence);

-- Trigger to increment on any change
CREATE OR REPLACE FUNCTION update_backup_sequence()
RETURNS TRIGGER AS $$
BEGIN
  NEW.backup_sequence = (SELECT COALESCE(MAX(backup_sequence), 0) + 1 FROM note);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER note_backup_sequence_trigger
BEFORE INSERT OR UPDATE ON note
FOR EACH ROW
EXECUTE FUNCTION update_backup_sequence();
```

### 6.2 Incremental Export

```json
// Request incremental export
{
  "type": "incremental",
  "since_sequence": 12345,
  "since_timestamp": "2026-01-16T02:00:00Z"
}

// Response includes only changed notes
{
  "manifest": {
    "type": "incremental",
    "base_sequence": 12345,
    "new_sequence": 12890,
    "changes": {
      "created": 50,
      "modified": 123,
      "deleted": 5
    }
  }
}
```

---

## 7. Security Considerations

### 7.1 Encryption

**Age Encryption (Recommended)**
- Modern, audited encryption
- Simple key management
- Streaming support

```bash
# Generate recipient key
age-keygen -o backup-key.txt

# Archive manifest specifies encryption
{
  "encryption": {
    "method": "age",
    "recipients": ["age1...public-key..."]
  }
}
```

**GPG Encryption (Alternative)**
- Broader tool support
- Hardware key support (YubiKey)

### 7.2 Access Control

- Backup API endpoints require `admin` or `backup` scope
- API keys for automated backup should have minimal scope:
  ```json
  {
    "scope": "backup:read backup:write"
  }
  ```

### 7.3 Secrets Management

- Never store encryption keys in knowledge shards
- Use environment variables or secrets manager for credentials
- Rotate API keys used for backup automation regularly

---

## 8. Error Handling and Recovery

### 8.1 Export Errors

| Error | Cause | Recovery |
|-------|-------|----------|
| `EXPORT_DB_CONNECTION` | Database unavailable | Retry after delay |
| `EXPORT_DISK_FULL` | Output location full | Clean up or use different destination |
| `EXPORT_TIMEOUT` | Very large export | Increase timeout, use filters |
| `EXPORT_ENCRYPTION_FAILED` | Missing/invalid key | Verify encryption config |

### 8.2 Import Errors

| Error | Cause | Recovery |
|-------|-------|----------|
| `IMPORT_INVALID_ARCHIVE` | Corrupt/wrong format | Verify archive, try different backup |
| `IMPORT_VERSION_MISMATCH` | Future archive version | Upgrade matric-memory |
| `IMPORT_CONFLICT` | ID collision | Use rename_new strategy |
| `IMPORT_DECRYPTION_FAILED` | Wrong key | Verify encryption key |

### 8.3 Retry Logic

```rust
// Exponential backoff for transient failures
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
        }
    }
}
```

---

## 9. Implementation Plan

### Phase 1: Core Export/Import (Week 1-2)

**Tasks:**
1. Create `matric-backup` crate structure
2. Implement shard builder (JSON + tar)
3. Implement shard reader
4. Add compression support (gzip, zstd)
5. Add export API endpoints to matric-api
6. Add import API endpoints to matric-api
7. Unit tests for shard format

**Deliverables:**
- Working `/api/v1/backup/export` endpoint
- Working `/api/v1/backup/import` endpoint
- Shard format v1.0.0 specification

### Phase 2: Destinations & Automation (Week 3)

**Tasks:**
1. Implement local filesystem destination
2. Implement S3-compatible destination
3. Implement rsync destination
4. Create backup shell script
5. Create systemd timer/service
6. Add retention policy logic

**Deliverables:**
- `matric-backup.sh` script
- Systemd service files
- Configuration file format

### Phase 3: Encryption & MCP (Week 4)

**Tasks:**
1. Implement age encryption
2. Add encryption options to API
3. Create MCP tools (export_all_notes, import_notes, backup_now, backup_status)
4. Add progress reporting via SSE/WebSocket
5. Integration tests

**Deliverables:**
- Encrypted backup support
- MCP tool integration
- End-to-end tests

### Phase 4: Polish & Documentation (Week 5)

**Tasks:**
1. Add markdown-zip export format
2. Implement backup status endpoint
3. Add notification webhooks
4. Write user documentation
5. Create backup/restore guide
6. Performance testing

**Deliverables:**
- Complete documentation
- Performance benchmarks
- Release notes

---

## 10. Architectural Decision Records

### ADR-001: Archive Format Selection

**Status:** Accepted

**Context:** Need a portable, efficient shard format for backup/restore.

**Decision:** Use tar archives with JSON metadata + optional markdown files.
- tar provides streaming, wide tool support
- JSON for programmatic parsing
- Optional markdown for human readability

**Consequences:**
- Pro: Standard format, easy debugging
- Pro: Streaming support for large backups
- Con: Slightly larger than binary formats

**Alternatives Considered:**
- SQLite dump: Not portable enough
- Custom binary: Complex, hard to debug
- Plain JSON: Can't include binary efficiently

### ADR-002: Compression Algorithm

**Status:** Accepted

**Context:** Need fast compression with good ratios for knowledge shards.

**Decision:** Support both zstd (default) and gzip.
- zstd: Better compression ratio, faster
- gzip: Universal compatibility

**Consequences:**
- Pro: Best-of-both-worlds with option
- Con: Two implementations to maintain

### ADR-003: Embedding Exclusion Default

**Status:** Accepted

**Context:** Embeddings are large (768 floats per chunk) but regenerable.

**Decision:** Exclude embeddings by default, optional inclusion.

**Consequences:**
- Pro: Smaller backups (10-100x)
- Pro: Faster backup/restore
- Con: Restore requires NLP reprocessing (time)
- Con: Different embeddings if model changed

### ADR-004: Encryption Library

**Status:** Accepted

**Context:** Need optional encryption for knowledge shards.

**Decision:** Use `age` as primary, GPG as alternative.
- age: Modern, simple, secure
- GPG: Broad compatibility, hardware key support

**Consequences:**
- Pro: Simple key management with age
- Pro: Enterprise compatibility with GPG
- Con: Two libraries to support

---

## 11. Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_roundtrip() {
        let notes = vec![create_test_note()];
        let archive = ShardBuilder::new()
            .add_notes(notes.clone())
            .build();

        let reader = ShardReader::new(&archive);
        let restored = reader.read_notes().unwrap();

        assert_eq!(notes.len(), restored.len());
        assert_eq!(notes[0].id, restored[0].id);
    }

    #[test]
    fn test_compression_zstd() {
        let data = b"test data repeated ".repeat(1000);
        let compressed = compress_zstd(&data, 3);
        let decompressed = decompress_zstd(&compressed);
        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_export_import_cycle() {
    let client = TestClient::new().await;

    // Create test notes
    let note_ids: Vec<Uuid> = (0..10)
        .map(|_| client.create_note("Test content").await)
        .collect();

    // Export
    let job_id = client.start_export().await;
    client.wait_for_job(job_id).await;
    let archive = client.download_export(job_id).await;

    // Clear database
    client.purge_all_notes().await;

    // Import
    let import_job = client.start_import(&archive).await;
    client.wait_for_job(import_job).await;

    // Verify
    for id in note_ids {
        let note = client.get_note(id).await;
        assert!(note.is_ok());
    }
}
```

---

## 12. Monitoring and Observability

### Metrics

```rust
// Prometheus metrics
pub static BACKUP_DURATION: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "matric_backup_duration_seconds",
        "Time taken for backup operations"
    ).unwrap()
});

pub static BACKUP_SIZE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "matric_backup_size_bytes",
        "Size of last knowledge shard"
    ).unwrap()
});

pub static BACKUP_NOTES_COUNT: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "matric_backup_notes_count",
        "Number of notes in last backup"
    ).unwrap()
});
```

### Logging

```rust
// Structured logging for backup operations
tracing::info!(
    job_id = %job_id,
    notes_count = notes.len(),
    size_bytes = archive.len(),
    duration_ms = duration.as_millis(),
    "Backup completed"
);
```

### Health Checks

```json
// GET /health includes backup status
{
  "status": "healthy",
  "components": {
    "database": "healthy",
    "backup": {
      "last_success": "2026-01-17T02:00:00Z",
      "last_duration_seconds": 45,
      "destinations_healthy": 2,
      "destinations_total": 2
    }
  }
}
```

---

## 13. Future Enhancements

### 13.1 Incremental Backups (v2.0)
- Track changes via sequence numbers
- Delta archives for faster backups
- Point-in-time recovery

### 13.2 Backup Verification (v2.0)
- Automatic restore testing
- Checksum verification
- Data integrity reports

### 13.3 Multi-Instance Sync (v3.0)
- Bidirectional sync between instances
- Conflict resolution for concurrent edits
- Federated knowledge base

### 13.4 Backup Browser UI (v2.0)
- Web UI for backup management
- Archive exploration
- Selective restore

---

## Appendix A: File Structure After Implementation

```
matric-memory/
  crates/
    matric-backup/
      Cargo.toml
      src/
        lib.rs
        archive/
        compression/
        encryption/
        destinations/
        progress/
        error.rs
    matric-api/
      src/
        handlers/
          backup.rs  # New
        main.rs      # Updated routes

  mcp-server/
    index.js         # Updated with backup tools

  scripts/
    matric-backup.sh

  systemd/
    matric-backup.service
    matric-backup.timer

  config/
    backup.yaml.example

  docs/
    backup-guide.md
```

---

## Appendix B: Dependencies

### matric-backup Cargo.toml

```toml
[package]
name = "matric-backup"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"

# Archive
tar = "0.4"
flate2 = "1"  # gzip
zstd = "0.13"

# Encryption
age = "0.10"

# S3
aws-sdk-s3 = "1"
aws-config = "1"

# Async
futures = "0.3"
async-trait = "0.1"

# Progress
indicatif = "0.17"

# Internal
matric-core = { path = "../matric-core" }
```
