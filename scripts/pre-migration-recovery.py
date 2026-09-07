#!/usr/bin/env python3
"""Snapshot-bound pre-migration recovery-point reuse/create helper."""

from __future__ import annotations

import fcntl
import glob
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Dict, Iterable, List, Tuple

RESTRICT_KEY = "FortemiPreMigrationStateHash"
FINGERPRINT_VERSION = "fortemi-pre-migration-state-v2"


def fail(message: str, code: int = 1) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(code)


def log(message: str) -> None:
    print(message, flush=True)


def allow_password_auth() -> bool:
    return os.environ.get("FORTEMI_PRE_MIGRATION_RECOVERY_ALLOW_PASSWORD") == "true"


def pg_env() -> Dict[str, str]:
    env = os.environ.copy()
    if allow_password_auth():
        env.setdefault("PGUSER", "postgres")
        env.setdefault("PGHOST", "/var/run/postgresql")
        env.setdefault("PGPORT", "5432")
        env.setdefault("PGDATABASE", env.get("POSTGRES_DB", "matric"))
    else:
        env["PGUSER"] = "postgres"
        env["PGHOST"] = "/var/run/postgresql"
        env["PGPORT"] = "5432"
        env["PGDATABASE"] = env.get("POSTGRES_DB", "matric")
        env.pop("PGPASSWORD", None)
        env.pop("PGPASSFILE", None)
    return env


def run_pg(args: List[str], *, db: str | None = None, input_text: str | None = None, capture: bool = True, snapshot: str | None = None) -> subprocess.CompletedProcess[str]:
    env = pg_env()
    if db:
        env["PGDATABASE"] = db
    prefix = ""
    if snapshot:
        escaped = snapshot.replace("'", "''")
        prefix = f"BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET TRANSACTION SNAPSHOT '{escaped}';\n"
    return subprocess.run(
        args,
        input=(prefix + (input_text or "")) if (input_text is not None or prefix) else None,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        env=env,
        check=False,
    )


def psql_scalar(sql: str, *, db: str | None = None, snapshot: str | None = None) -> str:
    cp = run_pg(["psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1"], db=db, input_text=sql, snapshot=snapshot)
    if cp.returncode != 0:
        fail(f"psql failed while computing recovery metadata: {cp.stderr.strip()}")
    return cp.stdout.strip()


def stream_command_hash(args: List[str], *, env: Dict[str, str] | None = None, input_text: str | None = None) -> str:
    h = hashlib.sha256()
    with tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(
            args,
            stdin=subprocess.PIPE if input_text is not None else None,
            stdout=subprocess.PIPE,
            stderr=err,
            text=False,
            env=env,
        )
        if input_text is not None and proc.stdin is not None:
            proc.stdin.write(input_text.encode())
            proc.stdin.close()
        assert proc.stdout is not None
        for chunk in iter(lambda: proc.stdout.read(1024 * 1024), b""):
            h.update(chunk)
        rc = proc.wait()
        if rc != 0:
            err.seek(0)
            stderr = err.read(8192).decode(errors="replace")
            fail(f"command failed while hashing recovery state: {' '.join(shlex.quote(a) for a in args)}\n{stderr.strip()}")
    return h.hexdigest()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def hash_text(parts: Iterable[str]) -> str:
    h = hashlib.sha256()
    for part in parts:
        h.update(part.encode("utf-8"))
    return h.hexdigest()


def database_identity(db: str) -> str:
    sql = """
SELECT current_setting('server_version_num')
       || E'\t' || (SELECT system_identifier::text FROM pg_control_system())
       || E'\t' || d.oid::text
       || E'\t' || d.datname
FROM pg_database d
WHERE d.datname = current_database();
"""
    return hash_text([psql_scalar(sql, db=db), "\n"])


def migration_manifest() -> str:
    migrations_dir = Path(os.environ.get("FORTEMI_MIGRATIONS_DIR", "/app/migrations"))
    if not migrations_dir.is_dir():
        return hash_text(["no-migrations-dir\n"])
    h = hashlib.sha256()
    for path in sorted(migrations_dir.glob("*.sql"), key=lambda p: str(p)):
        h.update(sha256_file(path).encode())
        h.update(b"  ")
        h.update(str(path.relative_to(migrations_dir)).encode())
        h.update(b"\n")
    return h.hexdigest()


def export_snapshot(db: str) -> Tuple[subprocess.Popen[str], str]:
    env = pg_env()
    env["PGDATABASE"] = db
    proc = subprocess.Popen(
        ["psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\nSELECT pg_export_snapshot();\n")
    proc.stdin.flush()
    snapshot = proc.stdout.readline().strip()
    if not snapshot:
        stderr = proc.stderr.read() if proc.stderr else ""
        proc.kill()
        fail(f"could not export PostgreSQL snapshot: {stderr.strip()}")
    return proc, snapshot


def close_snapshot(proc: subprocess.Popen[str]) -> None:
    try:
        if proc.stdin:
            proc.stdin.write("ROLLBACK;\n\\q\n")
            proc.stdin.flush()
    except BrokenPipeError:
        pass
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def lock_user_relations(exporter: subprocess.Popen[str]) -> None:
    assert exporter.stdin is not None
    sql = r"""
SET LOCAL lock_timeout = '5s';
DO $$
DECLARE
  rel record;
BEGIN
  FOR rel IN
    SELECT n.nspname, c.relname
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r', 'p')
      AND n.nspname NOT LIKE 'pg_%'
      AND n.nspname <> 'information_schema'
    ORDER BY n.nspname COLLATE "C", c.relname COLLATE "C"
  LOOP
    EXECUTE format('LOCK TABLE %I.%I IN ACCESS SHARE MODE', rel.nspname, rel.relname);
  END LOOP;
END $$;
"""
    exporter.stdin.write(sql + "\nSELECT 'locks-acquired';\n")
    exporter.stdin.flush()
    assert exporter.stdout is not None
    marker = exporter.stdout.readline().strip()
    if marker != "locks-acquired":
        stderr = exporter.stderr.read() if exporter.stderr else ""
        fail(f"could not acquire bounded relation locks for recovery fingerprint: {stderr.strip()}")


def schema_hash(db: str, snapshot: str | None = None) -> str:
    env = pg_env()
    env["PGDATABASE"] = db
    args = [
        "pg_dump",
        "--schema-only",
        "--format=plain",
        "--quote-all-identifiers",
        f"--restrict-key={RESTRICT_KEY}",
    ]
    if snapshot:
        args.append(f"--snapshot={snapshot}")
    return stream_command_hash(args, env=env)


def copy_hash(sql: str, *, db: str, snapshot: str | None, label: str) -> str:
    wrapped = f"""COPY ({sql}) TO STDOUT WITH (FORMAT csv, DELIMITER E'\t', QUOTE E'\b');
COMMIT;
"""
    h = hashlib.sha256()
    h.update((label + "\n").encode())
    env = pg_env()
    env["PGDATABASE"] = db
    with tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(
            ["psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=err,
            env=env,
            text=False,
        )
        if snapshot:
            escaped_snapshot = snapshot.replace("'", "''")
            prefix = f"BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET TRANSACTION SNAPSHOT '{escaped_snapshot}';\n"
        else:
            prefix = "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\n"
        assert proc.stdin is not None and proc.stdout is not None
        proc.stdin.write((prefix + wrapped).encode())
        proc.stdin.close()
        for chunk in iter(lambda: proc.stdout.read(1024 * 1024), b""):
            h.update(chunk)
        rc = proc.wait()
        if rc != 0:
            err.seek(0)
            stderr = err.read(8192).decode(errors="replace")
            fail(f"COPY stream failed for {label}: {stderr.strip()}")
    return h.hexdigest()


def relation_names(db: str, snapshot: str | None) -> List[Dict[str, Any]]:
    sql = r"""
SELECT json_build_object(
         'schema', n.nspname,
         'name', c.relname,
         'relkind', c.relkind::text,
         'relispopulated', c.relispopulated
       )::text
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'm')
  AND n.nspname NOT LIKE 'pg_%'
  AND n.nspname <> 'information_schema'
ORDER BY n.nspname COLLATE "C", c.relname COLLATE "C", c.relkind::text COLLATE "C";
COMMIT;
"""
    out = psql_scalar(sql, db=db, snapshot=snapshot)
    names: List[Dict[str, Any]] = []
    for line in out.splitlines():
        if not line:
            continue
        item = json.loads(line)
        names.append(item)
    return names


def quote_ident(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def relations_hash(db: str, snapshot: str | None) -> str:
    h = hashlib.sha256()
    h.update(b"relations-v2\n")
    for item in relation_names(db, snapshot):
        schema = item["schema"]
        table = item["name"]
        relkind = item["relkind"]
        relispopulated = bool(item["relispopulated"])
        relation_key = schema + "." + table
        h.update((relation_key + "\t" + relkind + "\t" + str(relispopulated).lower() + "\t").encode())
        if relkind == "m" and not relispopulated:
            h.update(b"unpopulated-materialized-view\n")
            continue
        rel = f"{quote_ident(schema)}.{quote_ident(table)}"
        literal = relation_key.replace("'", "''")
        sql = f'''
SELECT '{literal}'::text AS relation_key,
       xmin::text AS row_xmin,
       to_jsonb(t)::text AS row_json
FROM {rel} AS t
ORDER BY to_jsonb(t)::text COLLATE "C", xmin::text COLLATE "C"'''
        h.update((copy_hash(sql, db=db, snapshot=snapshot, label=relation_key) + "\n").encode())
    return h.hexdigest()


def sequence_rows(db: str, snapshot: str | None = None) -> List[Dict[str, str]]:
    list_sql = r"""
SELECT json_build_object(
         'schema', schemaname,
         'name', sequencename,
         'start_value', start_value::text,
         'min_value', min_value::text,
         'max_value', max_value::text,
         'increment_by', increment_by::text,
         'cycle', cycle::text,
         'cache_size', cache_size::text
       )::text
FROM pg_sequences
WHERE schemaname NOT LIKE 'pg_%'
  AND schemaname <> 'information_schema'
ORDER BY schemaname COLLATE "C", sequencename COLLATE "C";
"""
    if snapshot:
        list_sql += "COMMIT;\n"
    out = psql_scalar(list_sql, db=db, snapshot=snapshot)
    rows: List[Dict[str, str]] = []
    for line in out.splitlines():
        if line:
            rows.append(json.loads(line))
    return rows


def sequence_state_text(db: str, rows: List[Dict[str, str]] | None = None) -> str:
    rows = rows if rows is not None else sequence_rows(db)
    parts: List[str] = []
    for item in rows:
        schema = item["schema"]
        seq = item["name"]
        rel = f"{quote_ident(schema)}.{quote_ident(seq)}"
        state = psql_scalar(f"SELECT last_value::text || E'\\t' || is_called::text FROM {rel}", db=db)
        attrs = [item[k] for k in ["schema", "name", "start_value", "min_value", "max_value", "increment_by", "cycle", "cache_size"]]
        parts.append("\t".join(attrs + [state]))
    return "\n".join(parts) + "\n"


def sequences_hash(db: str, snapshot: str | None) -> Tuple[str, str]:
    rows = sequence_rows(db, snapshot)
    baseline = sequence_state_text(db, rows)
    if sequence_state_text(db, rows) != baseline:
        fail("sequence moved during recovery fingerprint", code=2)
    return hash_text(["sequences-v2\n", baseline]), baseline


def large_objects_hash(db: str, snapshot: str | None) -> str:
    sql = r"""
SELECT kind, key1, key2, key3, key4
FROM (
  SELECT 'metadata'::text AS kind, oid::text AS key1, xmin::text AS key2,
         lomowner::text AS key3, COALESCE(array_to_string(lomacl, ','), '')::text AS key4
  FROM pg_largeobject_metadata
  UNION ALL
  SELECT 'page'::text AS kind, loid::text AS key1, pageno::text AS key2,
         encode(data, 'hex')::text AS key3, ''::text AS key4
  FROM pg_largeobject
) lo
ORDER BY kind COLLATE "C", key1 COLLATE "C", key2 COLLATE "C", key3 COLLATE "C", key4 COLLATE "C"
"""
    return copy_hash(sql, db=db, snapshot=snapshot, label="large-objects-v2")


def state_hash(db: str, snapshot: str | None) -> Tuple[str, str, str]:
    snapshot_schema_hash = schema_hash(db, snapshot)
    sequence_hash, sequence_state = sequences_hash(db, snapshot)
    parts = [
        FINGERPRINT_VERSION,
        "schema", snapshot_schema_hash,
        "relations", relations_hash(db, snapshot),
        "sequences", sequence_hash,
        "large_objects", large_objects_hash(db, snapshot),
    ]
    return hash_text(part + "\n" for part in parts), sequence_state, snapshot_schema_hash


def read_metadata(path: Path) -> Tuple[Dict[str, str], bool]:
    values: Dict[str, str] = {}
    try:
        for line in path.read_text().splitlines():
            if "=" not in line:
                return values, False
            key, value = line.split("=", 1)
            if not key or key in values:
                return values, False
            values[key] = value
    except OSError:
        return values, False
    return values, True


def metadata_value(path: Path, key: str) -> str:
    values, ok = read_metadata(path)
    if not ok:
        return ""
    return values.get(key, "")


def invalid_reason(meta: Path, expected: Dict[str, str], backup_dest: Path, max_age: int) -> Tuple[bool, str, str]:
    if max_age <= 0:
        return False, "reuse disabled", ""
    if not meta.is_file():
        return False, "metadata missing", ""
    values, metadata_ok = read_metadata(meta)
    if not metadata_ok:
        return False, "metadata invalid", ""
    if values.get("format_version") != "1":
        return False, "metadata version unsupported", ""
    if values.get("verified") != "true":
        return False, "metadata unverified", ""
    artifact_file = values.get("artifact_file", "")
    if not artifact_file or "/" in artifact_file:
        return False, "artifact filename invalid", ""
    artifact = backup_dest / artifact_file
    if not artifact.is_file():
        return False, "artifact missing", ""
    checks = [
        ("from_version", "migration source changed"),
        ("to_version", "migration target changed"),
        ("db_identity_sha256", "database identity changed"),
        ("migration_manifest_sha256", "migration manifest changed"),
        ("state_sha256", "state fingerprint changed"),
    ]
    for key, reason in checks:
        if values.get(key, "") != expected[key]:
            return False, reason, ""
    created = values.get("created_at_epoch", "")
    if not created.isdigit():
        return False, "metadata timestamp invalid", ""
    now = int(time.time())
    if int(created) > now:
        return False, "metadata timestamp invalid", ""
    if now - int(created) > max_age:
        return False, "recovery point stale", ""
    if values.get("artifact_sha256", "") != sha256_file(artifact):
        return False, "artifact checksum changed", ""
    if values.get("artifact_size_bytes", "") != str(artifact.stat().st_size):
        return False, "artifact size changed", ""
    return True, "", artifact_file


def find_reusable(backup_dest: Path, expected: Dict[str, str], max_age: int) -> str | None:
    if max_age <= 0:
        log(">>> Pre-migration backup reuse skipped: reuse disabled")
        return None
    metas = sorted(backup_dest.glob("pre-migration-*.sql*.recovery.meta"))
    if not metas:
        log(">>> Pre-migration backup reuse skipped: no verified recovery metadata found")
        return None
    for meta in metas:
        ok, reason, artifact = invalid_reason(meta, expected, backup_dest, max_age)
        if ok:
            return artifact
        log(f">>> Pre-migration backup reuse skipped: {reason}")
    return None


def publish_recovery_metadata(backup_dest: Path, artifact_file: str, expected: Dict[str, str]) -> None:
    artifact = backup_dest / artifact_file
    if not artifact.is_file():
        fail(f"verified backup artifact is missing before metadata publish: {artifact_file}")
    meta = backup_dest / f"{artifact_file}.recovery.meta"
    tmp = backup_dest / f".{artifact_file}.recovery.meta.tmp.{os.getpid()}"
    lines = [
        "format_version=1",
        "verified=true",
        f"artifact_file={artifact_file}",
        f"artifact_sha256={sha256_file(artifact)}",
        f"artifact_size_bytes={artifact.stat().st_size}",
        f"created_at_epoch={int(time.time())}",
        f"db_identity_sha256={expected['db_identity_sha256']}",
        f"from_version={expected['from_version']}",
        f"to_version={expected['to_version']}",
        f"migration_manifest_sha256={expected['migration_manifest_sha256']}",
        f"state_sha256={expected['state_sha256']}",
        f"reuse_max_age_seconds={os.environ.get('FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS', '86400')}",
    ]
    tmp.write_text("\n".join(lines) + "\n", encoding="utf-8")
    tmp.chmod(0o600)
    os.replace(tmp, meta)
    log(f">>> Recovery metadata published: {meta}")


def choose_staging_dir(db: str, backup_dest: Path) -> Tuple[str, str]:
    staging_dir = Path(os.environ.get("BACKUP_TEMP_DIR", "/dev/shm/fortemi-pre-migration-backup"))
    staging_trusted = os.environ.get("BACKUP_TEMP_TRUSTED_ENCRYPTED", "false")
    db_size_raw = psql_scalar("SELECT pg_database_size(current_database())", db=db)
    try:
        db_size = int(db_size_raw)
    except ValueError:
        db_size = 0
    staging_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    try:
        available = shutil.disk_usage(staging_dir).free
    except OSError:
        available = 0
    if db_size > 0 and available > 0 and available < db_size:
        log(f"!!! WARNING: backup staging dir {staging_dir} has {available} bytes free")
        log(f"!!! but the database is {db_size} bytes; the staged dump may not fit.")
        log(f"!!! Falling back to DISK staging under {backup_dest}/.staging — the dump")
        log("!!! will touch disk unencrypted while the backup runs.")
        log("!!! To keep RAM-backed staging, raise the container shm size or point BACKUP_TEMP_DIR at a larger tmpfs.")
        staging_dir = backup_dest / ".staging"
        staging_trusted = "true"
        staging_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        disk_available = shutil.disk_usage(staging_dir).free
        if disk_available < db_size:
            fail(f"disk staging under {backup_dest} also lacks space ({disk_available} bytes free, database is {db_size} bytes)")
    return str(staging_dir), staging_trusted


def cleanup_old_recovery_points(backup_dest: Path, keep_file: str) -> None:
    retain_raw = os.environ.get("PRE_MIGRATION_BACKUP_RETAIN", os.environ.get("BACKUP_RETAIN", "7"))
    try:
        retain_days = int(retain_raw)
    except ValueError:
        retain_days = 7
    if retain_days < 0:
        retain_days = 0
    cutoff = time.time() - (retain_days * 86400)
    deleted = 0
    for path in backup_dest.glob("pre-migration-*.sql*"):
        if path.name == keep_file or path.name == keep_file + ".recovery.meta":
            continue
        try:
            if path.stat().st_mtime <= cutoff:
                path.unlink()
                deleted += 1
        except FileNotFoundError:
            continue
    if deleted:
        log(f">>> Cleaned up {deleted} old pre-migration recovery artifact(s)")


def remove_created_artifact(backup_dest: Path, artifact_file: str) -> None:
    if not artifact_file or "/" in artifact_file:
        return
    (backup_dest / artifact_file).unlink(missing_ok=True)
    (backup_dest / (artifact_file + ".recovery.meta")).unlink(missing_ok=True)


def validate_final_state(db: str, backup_dest: Path, artifact_file: str, expected_state: Tuple[str, str, str]) -> None:
    state_digest, sequence_state, snapshot_schema_hash = expected_state
    final_exporter: subprocess.Popen[str] | None = None
    try:
        final_exporter, final_snapshot = export_snapshot(db)
        lock_user_relations(final_exporter)
        current_state_digest, current_sequence_state, current_schema_hash = state_hash(db, final_snapshot)
    except BaseException:
        remove_created_artifact(backup_dest, artifact_file)
        raise
    finally:
        if final_exporter is not None:
            close_snapshot(final_exporter)

    if current_sequence_state != sequence_state:
        remove_created_artifact(backup_dest, artifact_file)
        fail("sequence moved during recovery backup creation", code=2)
    if current_schema_hash != snapshot_schema_hash:
        remove_created_artifact(backup_dest, artifact_file)
        fail("database schema changed during recovery backup creation", code=2)
    if current_state_digest != state_digest:
        remove_created_artifact(backup_dest, artifact_file)
        fail("database contents changed during recovery backup creation", code=2)


def create_backup(expected: Dict[str, str], snapshot: str) -> str:
    backup_dest = Path(os.environ["BACKUP_DEST"])
    backup_script = os.environ.get("BACKUP_SCRIPT_PATH", "/app/scripts/backup.sh")
    if not os.access(backup_script, os.X_OK):
        fail(f"pre-migration backup script is not executable: {backup_script}")
    timestamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    basename = f"pre-migration-{expected['from_version']}-{expected['to_version']}-{timestamp}-{os.getpid()}"
    log_file = backup_dest / f".{basename}.log"
    env = pg_env()
    staging_dir, staging_trusted = choose_staging_dir(env["PGDATABASE"], backup_dest)
    for recovery_key in list(env):
        if recovery_key.startswith("BACKUP_RECOVERY_"):
            env.pop(recovery_key, None)
    env.update({
        "BACKUP_DEST": str(backup_dest),
        "BACKUP_BASENAME": basename,
        "BACKUP_CLEANUP_PATTERN": "pre-migration-*.sql*",
        "BACKUP_RECOVERY_META_ENABLED": "false",
        "BACKUP_RETENTION_DEFERRED": "true",
        "BACKUP_RETAIN": os.environ.get("PRE_MIGRATION_BACKUP_RETAIN", os.environ.get("BACKUP_RETAIN", "7")),
        "BACKUP_TEMP_DIR": staging_dir,
        "BACKUP_TEMP_TRUSTED_ENCRYPTED": staging_trusted,
        "BACKUP_COMPRESS": os.environ.get("BACKUP_COMPRESS", "gzip"),
        "BACKUP_PG_DUMP_SNAPSHOT": snapshot,
        "PGUSER": env["PGUSER"],
        "PGHOST": env["PGHOST"],
        "PGPORT": env["PGPORT"],
        "PGDATABASE": env["PGDATABASE"],
        "LOG_FILE": os.environ.get("LOG_FILE", "/var/log/fortemi/backup.log"),
    })
    log(">>> Pending migrations on non-empty database: creating verified pre-migration backup")
    with log_file.open("w", encoding="utf-8") as out:
        proc = subprocess.Popen([backup_script, "-d", "local"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
        assert proc.stdout is not None
        final = ""
        for line in proc.stdout:
            print(line, end="")
            out.write(line)
            final = line.strip() or final
        rc = proc.wait()
    if rc != 0:
        for path in backup_dest.glob(basename + ".sql*"):
            path.unlink(missing_ok=True)
        fail("verified pre-migration backup failed; aborting before checksum repair and migrations")
    if not final:
        fail("verified pre-migration backup completed but did not report an artifact")
    return final


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: pre-migration-recovery.py FROM_VERSION TO_VERSION")
    from_version, to_version = sys.argv[1], sys.argv[2]
    db = os.environ.get("POSTGRES_DB", os.environ.get("PGDATABASE", "matric"))
    backup_dest = Path(os.environ.get("BACKUP_DEST", "/var/backups/matric-memory"))
    backup_dest.mkdir(mode=0o700, parents=True, exist_ok=True)
    try:
        backup_dest.chmod(0o700)
    except OSError:
        pass
    max_age_raw = os.environ.get("FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS", "86400")
    max_age = int(max_age_raw) if max_age_raw.isdigit() else 86400

    lock_path = backup_dest / ".pre-migration-recovery.lock"
    with lock_path.open("a+") as lock_file:
        fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
        exporter, snapshot = export_snapshot(db)
        try:
            lock_user_relations(exporter)
            expected = {
                "from_version": from_version,
                "to_version": to_version,
                "db_identity_sha256": database_identity(db),
                "migration_manifest_sha256": migration_manifest(),
            }
            expected_state = state_hash(db, snapshot)
            state_digest, sequence_state, snapshot_schema_hash = expected_state
            expected["state_sha256"] = state_digest
            reusable = find_reusable(backup_dest, expected, max_age)
            if sequence_state_text(db) != sequence_state:
                fail("sequence moved during recovery-point validation", code=2)
            if schema_hash(db) != snapshot_schema_hash:
                fail("database schema changed during recovery-point validation", code=2)
            if reusable:
                log(f">>> Pre-migration backup ready: {backup_dest}/{reusable} (reusing verified pre-migration backup)")
                return
            final = create_backup(expected, snapshot)
            validate_final_state(db, backup_dest, final, expected_state)
            publish_recovery_metadata(backup_dest, final, expected)
            cleanup_old_recovery_points(backup_dest, final)
            log(f">>> Pre-migration backup ready: {backup_dest}/{final}")
        finally:
            close_snapshot(exporter)


if __name__ == "__main__":
    main()
