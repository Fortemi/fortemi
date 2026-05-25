#!/usr/bin/env python3
"""
Phase 6 issue filer for fortemi/fortemi#736 epic.

Parses issue-backlog.md, files all issues to fortemi/fortemi on Gitea in
dependency-aware order, records ID mapping, and updates #736 epic body.

Usage:
  GITEA_TOKEN=$(cat ~/.config/gitea/token) python3 file-issues.py
  GITEA_TOKEN=$(cat ~/.config/gitea/token) python3 file-issues.py --dry-run
"""
import json
import os
import re
import sys
import time
import urllib.request
import urllib.error
from pathlib import Path

GITEA = "https://git.integrolabs.net"
REPO = "fortemi/fortemi"
BACKLOG = Path(__file__).parent / "issue-backlog.md"
ADDENDUM_PATH = Path(__file__).parent / "issue-backlog.md"  # same file, §7
MAP_FILE = Path(__file__).parent / "filing-map.json"
DRY = "--dry-run" in sys.argv

TOKEN = os.environ.get("GITEA_TOKEN", "").strip()
if not TOKEN:
    print("ERROR: GITEA_TOKEN env var not set", file=sys.stderr)
    sys.exit(1)

# Label name → ID mapping (fetched from fortemi/fortemi)
LABELS = {
    "P0": 223, "P1": 201, "P2": 202, "P3": 203,
    "epic": 197, "feat": 204, "enhancement": 199, "bug": 198,
    "test": 238, "testing": 238, "docs": 194, "documentation": 194,
    "jobs": 209, "matric-jobs": 209,
    "matric-api": 224, "rest-api": 224,
    "matric-db": 231, "crud": 231,
    "matric-core": 239, "infrastructure": 239, "core": 239,
    "mcp-server": 206, "mcp-api": 206,
    "security": 196,
    "observability": 239,  # no perfect match; use infrastructure
    "deps": 239,
    "archives": 229,
    "extraction": 228,
}


def map_label(name):
    """Map backlog label string to ID, with fallbacks."""
    n = name.strip().lower()
    n = re.sub(r"^(priority|type|scope|phase):\s*", "", n)
    n = n.replace(" ", "-")
    if n in ("construction", "transition", "deferred"):
        return None  # no phase labels in fortemi
    return LABELS.get(n)


# Always-applied labels
EPIC_AREA = [229, 228]  # area/archives, area/extraction


def gitea(method, path, body=None):
    """HTTP helper."""
    url = f"{GITEA}/api/v1{path}"
    headers = {
        "Authorization": f"token {TOKEN}",
        "Content-Type": "application/json",
    }
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code} on {method} {path}: {e.read()[:500]}", file=sys.stderr)
        raise


def create_issue(title, body, labels):
    """Create a Gitea issue. Returns issue dict with .number."""
    payload = {
        "title": title,
        "body": body,
        "labels": [l for l in labels if l is not None],
    }
    if DRY:
        print(f"[DRY] would create: {title[:80]}")
        print(f"      labels: {payload['labels']}")
        return {"number": 0, "title": title}
    result = gitea("POST", f"/repos/{REPO}/issues", payload)
    print(f"  #{result['number']} — {title[:80]}")
    time.sleep(0.3)  # gentle rate-limit pause
    return result


def parse_backlog():
    """Parse backlog.md into structured issue specs.

    Returns list of dicts: {id, title, labels, body, blocked_by, blocks, wave}
    """
    text = BACKLOG.read_text()
    issues = []

    # Match #### WS-X-Y... or ##### WS-X-Y... headers
    # Each issue block runs from its header to the next #### or ### or ## header
    pattern = re.compile(
        r"^(#####?)\s+(WS-\d+(?:-[A-Z0-9]+)?(?:-PARENT|-TEST|-DOC[A-Z0-9-]*)?|TEST-WS-\d+|CROSS-\d+|WS-10-DOC-[A-Z0-9-]+)\s+—\s+`([^`]+)`",
        re.MULTILINE
    )
    matches = list(pattern.finditer(text))

    for i, m in enumerate(matches):
        header_level, ws_id, title_code = m.group(1), m.group(2), m.group(3)
        # title_code is like `epic(matric-db): storage backend abstraction extension`
        title = title_code

        # Body spans from this header to next header (or EOF)
        start = m.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        block = text[start:end].strip()

        # Stop body at next major section divider like "---\n\n#### " or "## 2."
        # The regex above already handles this since we match the next header.
        # But strip trailing "---" separators
        block = re.sub(r"\n+---\s*$", "", block)

        # Extract labels line
        labels = []
        lbl_match = re.search(r"\*\*Labels\*\*:\s*(.+?)(?:\n|$)", block)
        if lbl_match:
            for lbl in re.findall(r"`([^`]+)`", lbl_match.group(1)):
                lid = map_label(lbl)
                if lid:
                    labels.append(lid)
        # Always add area/archives + area/extraction to scope
        labels.extend(EPIC_AREA)
        # Dedup
        labels = list(dict.fromkeys(labels))

        # Build body markdown: keep the original block as the issue body but
        # replace the backlog's "Dep declaration" line with a resolved version later.
        body_md = block

        # Extract dep declaration
        blocked_by = []
        blocks = []
        dep_match = re.search(
            r"\*\*?Dep(?:\s+declaration)?\*\*?:\s*`BLOCKS:\s*\[([^\]]*)\]`;\s*`BLOCKED-BY:\s*\[([^\]]*)\]`",
            block
        )
        if dep_match:
            blocks_raw, blocked_raw = dep_match.group(1), dep_match.group(2)
            blocks = [s.strip().lstrip("#") for s in blocks_raw.split(",") if s.strip()]
            blocked_by = [s.strip().lstrip("#") for s in blocked_raw.split(",") if s.strip()]

        # Wave: parent-level (4 #) vs atomic (5 #)
        wave = "parent" if header_level == "####" else "atomic"

        issues.append({
            "ws_id": ws_id,
            "title": title,
            "labels": labels,
            "body": body_md,
            "blocked_by": blocked_by,
            "blocks": blocks,
            "wave": wave,
        })

    return issues


def main():
    print(f"Parsing backlog…")
    issues = parse_backlog()
    print(f"Found {len(issues)} issues in backlog")

    # File ordering: parents first, then atomics. WS-5 deferred section will be
    # represented but the addendum's WS-5-A..F are separate atomic issues we
    # encode below (not yet in §2.3 since addendum added them).
    parents = [i for i in issues if i["wave"] == "parent"]
    atomics = [i for i in issues if i["wave"] == "atomic"]

    # New WS-5 children per Option D addendum (§7.1)
    ws5_children = [
        {
            "ws_id": "WS-5-A",
            "title": "feat(matric-core): mount-kind detection via /proc/self/mountinfo + platform equivalents",
            "labels": [201, 204, 239, 229, 228],  # P1, new-feature, infrastructure, archives, extraction
            "body": (
                "## Context\n\nPer Phase 5 Q-1 decision (Option D — detection-driven hybrid). "
                "Detect mount kind to route between `RecommendedWatcher` and `PollWatcher`.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] Returns enum: `native_*` | `nfs|cifs|smb|sshfs|fuse` | `docker_bind_macos_arm` | `wsl2_drvfs` | `tmpfs|overlay` | `unknown`\n"
                "- [ ] Linux impl parses `/proc/self/mountinfo`\n"
                "- [ ] macOS impl uses `statfs`\n"
                "- [ ] Windows impl uses `GetVolumeInformation`\n"
                "- [ ] Unknown mount type defaults to `unknown` (safe fallback to polling)\n"
                "- [ ] Unit tests cover 6 fixture mount types\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-PARENT; Blocks: WS-5-B\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md (§1 trigger logic)\n"
                "- @.aiwg/working/issue-planner-storage/issue-backlog.md §7.1\n\n"
                "## Files\n\n`crates/matric-core/src/mount_detect.rs` (new)"
            ),
            "blocked_by": ["WS-5-PARENT"], "blocks": ["WS-5-B"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-B",
            "title": "feat(matric-jobs): unified watcher event channel + backend selector",
            "labels": [201, 204, 209, 229, 228],
            "body": (
                "## Context\n\nPer Phase 5 Q-1 (Option D). Single `WatcherEvent` enum + Tokio channel; "
                "selector chooses `RecommendedWatcher` or `PollWatcher` per mount kind from WS-5-A.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] `WatcherEvent::{Create, Modify, Delete, Rescan}` enum defined\n"
                "- [ ] Backend selector function takes mount-kind + override env var\n"
                "- [ ] `FORTEMI_WATCHER_MODE=auto|poll|notify|off` env var honored (auto = detection-driven)\n"
                "- [ ] Returns trait-object backend with unified event channel\n"
                "- [ ] Unit tests cover selector decision matrix\n\n"
                "## Dependencies\n\nBlocked-by: WS-2-PARENT, WS-5-A; Blocks: WS-5-C, WS-5-D, WS-5-E\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md"
            ),
            "blocked_by": ["WS-2-PARENT", "WS-5-A"], "blocks": ["WS-5-C", "WS-5-D", "WS-5-E"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-C",
            "title": "feat(matric-jobs): PollWatcher pipeline integration",
            "labels": [201, 204, 209, 229, 228],
            "body": (
                "## Context\n\nnotify-rs `PollWatcher` with `compare_contents`, configurable interval, "
                "content-hash dedup pass before ingest. Used for NFS / Docker macOS / WSL / unknown mounts.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] PollWatcher instantiated with configurable interval (default 60s)\n"
                "- [ ] `compare_contents: true` option enabled\n"
                "- [ ] Interval scales with file count when archive >10k files (formula: `max(60s, file_count / 1000)`)\n"
                "- [ ] Events normalized to `WatcherEvent` and pushed to unified channel\n"
                "- [ ] Content-hash dedup prevents re-ingest of unchanged files\n"
                "- [ ] Integration test against fixture NFS mount\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-B; Blocks: WS-5-TEST\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md (§3 notify-rs deep-dive)"
            ),
            "blocked_by": ["WS-5-B"], "blocks": ["WS-5-TEST"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-D",
            "title": "feat(matric-jobs): RecommendedWatcher pipeline integration",
            "labels": [201, 204, 209, 229, 228],
            "body": (
                "## Context\n\nnotify-rs `RecommendedWatcher` wiring for native mounts (inotify/FSEvents/RDCW). "
                "Event normalization to unified channel from WS-5-B, debounce window for rapid edit bursts.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] RecommendedWatcher instantiated for native mount kinds\n"
                "- [ ] Events normalized to `WatcherEvent` shape\n"
                "- [ ] Debounce window (default 500ms) coalesces rapid edits\n"
                "- [ ] Graceful degradation if RecommendedWatcher fails to start (log + fallback to PollWatcher)\n"
                "- [ ] Integration test on native ext4 + tmpfs fixture\n"
                "- [ ] Per-platform CI matrix: linux-inotify, macos-fsevents, windows-rdcw\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-B; Blocks: WS-5-TEST\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md"
            ),
            "blocked_by": ["WS-5-B"], "blocks": ["WS-5-TEST"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-E",
            "title": "feat(matric-api): FORTEMI_WATCHER_MODE operator override env var",
            "labels": [201, 204, 224, 229, 228],
            "body": (
                "## Context\n\nOperator override for live-watching mode. Values: `auto` (default, detection-driven) "
                "| `poll` (force PollWatcher) | `notify` (force RecommendedWatcher) | `off` (no watcher).\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] Env var read at archive create + on per-archive restart\n"
                "- [ ] Documented in `deployment-environment-template`\n"
                "- [ ] Invalid value logs warning and falls back to `auto`\n"
                "- [ ] Per-archive override via `scan_config.watcher_mode` (optional; env var is default)\n"
                "- [ ] Integration test for all 4 modes\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-B; Blocks: WS-5-TEST, WS-10-DOC-EXT-005\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md (§1 override logic)"
            ),
            "blocked_by": ["WS-5-B"], "blocks": ["WS-5-TEST", "WS-10-DOC-EXT-005"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-F",
            "title": "feat(observability): emit watcher_kind gauge + watcher_event_rate counter per archive",
            "labels": [202, 204, 239, 229, 228],
            "body": (
                "## Context\n\nPrometheus-style metrics for which backend each archive is using and event throughput. "
                "Surfaces silent fallback to operator monitoring.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] `fortemi_archive_watcher_kind{archive_id, kind}` gauge (1 per active watcher)\n"
                "- [ ] `fortemi_archive_watcher_event_rate{archive_id, event_type}` counter\n"
                "- [ ] Metrics exposed via existing `/metrics` endpoint\n"
                "- [ ] Documented in operator runbook (links to WS-10-DOC-EXT-005)\n"
                "- [ ] Unit test for metric emission on event\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-B, WS-5-C, WS-5-D; Blocks: WS-5-TEST\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md"
            ),
            "blocked_by": ["WS-5-B", "WS-5-C", "WS-5-D"], "blocks": ["WS-5-TEST"], "wave": "atomic",
        },
        {
            "ws_id": "WS-5-TEST",
            "title": "test(matric-jobs): live-update detection + watcher backend test suite",
            "labels": [201, 238, 209, 196, 229, 228],
            "body": (
                "## Context\n\nTracker for 8 new test cases per Phase 5 Q-1 (Option D) addendum §7.2.\n\n"
                "## Test cases\n\n"
                "- [ ] Mount-kind detection per platform (linux, macos, windows fixtures)\n"
                "- [ ] PollWatcher fallback behavior on NFS fixture\n"
                "- [ ] PollWatcher fallback on Docker bind mount (macOS M1 emulation)\n"
                "- [ ] FORTEMI_WATCHER_MODE=poll forces PollWatcher on native mount\n"
                "- [ ] FORTEMI_WATCHER_MODE=off yields no watcher (control case)\n"
                "- [ ] Observability surface emits correct watcher_kind gauge\n"
                "- [ ] Docker bind-mount soak test (1hr run, 100 edit events expected, count match)\n"
                "- [ ] Watcher restart on source mount disappear → reappear\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-C, WS-5-D, WS-5-E, WS-5-F; Blocks: WS-5-PARENT closure\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md"
            ),
            "blocked_by": ["WS-5-C", "WS-5-D", "WS-5-E", "WS-5-F"], "blocks": ["WS-5-PARENT"], "wave": "atomic",
        },
        {
            "ws_id": "WS-10-DOC-EXT-005",
            "title": "docs(referenced-storage): live-update behavior — choosing mode, troubleshooting, tuning polling interval",
            "labels": [202, 194, 229, 228],
            "body": (
                "## Context\n\nOperator-facing documentation for Option D live-update detection. Covers: "
                "how detection works, when to force a specific mode, how to tune polling interval, troubleshooting "
                "Docker / WSL / NFS edge cases.\n\n"
                "## Acceptance criteria\n\n"
                "- [ ] New section in `docs/content/referenced-storage.md` (or new file) covering watcher modes\n"
                "- [ ] Decision tree: which mode to use for which deployment\n"
                "- [ ] Troubleshooting: events not firing, polling slow, watcher restarts\n"
                "- [ ] References Prometheus metrics from WS-5-F\n"
                "- [ ] Links to upstream notify-rs docs\n"
                "- [ ] `doc-sync` skill validates no broken @-mentions\n\n"
                "## Dependencies\n\nBlocked-by: WS-5-E, WS-5-F; Blocks: WS-10-PARENT closure\n\n"
                "## References\n\n- @.aiwg/working/issue-planner-storage/research-q1-live-updates.md\n"
                "- @https://docs.rs/notify/latest/notify/"
            ),
            "blocked_by": ["WS-5-E", "WS-5-F"], "blocks": ["WS-10-PARENT"], "wave": "atomic",
        },
    ]

    print(f"\nParent issues: {len(parents)}")
    print(f"Atomic issues from backlog: {len(atomics)}")
    print(f"Atomic issues from addendum (WS-5 + extras): {len(ws5_children)}")
    print(f"Grand total to file: {len(parents) + len(atomics) + len(ws5_children)}")

    if DRY:
        print(f"\n[DRY-RUN] Inspecting first 5 parents and 5 atomics:")
        for issue in (parents[:5] + atomics[:5]):
            print(f"\n--- {issue['ws_id']} ---")
            print(f"Title: {issue['title']}")
            print(f"Labels: {issue['labels']}")
            print(f"Blocked-by: {issue['blocked_by']}")
            print(f"Blocks: {issue['blocks']}")
            print(f"Body length: {len(issue['body'])} chars")

    # ID mapping: ws_id → Gitea issue number
    id_map = {}
    if MAP_FILE.exists():
        id_map = json.loads(MAP_FILE.read_text())
        print(f"Loaded existing id-map with {len(id_map)} entries (resume mode)")

    def file_one(issue):
        if issue["ws_id"] in id_map:
            print(f"  [skip] {issue['ws_id']} already filed as #{id_map[issue['ws_id']]}")
            return
        result = create_issue(issue["title"], issue["body"], issue["labels"])
        id_map[issue["ws_id"]] = result["number"]
        if not DRY:
            MAP_FILE.write_text(json.dumps(id_map, indent=2))

    print(f"\n=== Wave 1: Parents (10) ===")
    for issue in parents:
        file_one(issue)

    print(f"\n=== Wave 2: Atomic impl from backlog ({len(atomics)}) ===")
    for issue in atomics:
        file_one(issue)

    print(f"\n=== Wave 3: WS-5 + extras from addendum ({len(ws5_children)}) ===")
    for issue in ws5_children:
        file_one(issue)

    print(f"\n=== Done. Issues filed: {len([v for v in id_map.values() if v])} ===")
    print(f"ID map saved to {MAP_FILE}")
    print(f"\nNote: #736 epic update is a separate step (run with --update-736 after review).")


if __name__ == "__main__":
    main()
