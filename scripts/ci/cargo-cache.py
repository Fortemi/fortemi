#!/usr/bin/env python3
"""Owned Cargo target reuse. No network, tool installation, or host cache purge."""
import contextlib
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time

SCHEMA = 'fortemi-cargo-cache-v1'
MAX_BYTES = 24 * 1024**3
CI_MAIN_MAX_BYTES = 64 * 1024**3
MAX_ENTRIES = 6
QUARANTINE = '.evict-quarantine'
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def private_dir(path):
    """Never traverse symlinks; private leaf must belong to this uid."""
    path = Path(os.path.abspath(path))
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in path.parts[1:]:
            try:
                os.mkdir(part, 0o700, dir_fd=fd)
            except FileExistsError:
                pass
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        info = os.fstat(fd)
        if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) & 0o077:
            raise ValueError('cache leaf must be private and owned')
    finally:
        os.close(fd)
    return path


@contextlib.contextmanager
def open_directory(path):
    """Pin every component without following symlinks (including ancestors)."""
    fd = os.open('/', DIRECTORY)
    try:
        for part in Path(os.path.abspath(path)).parts[1:]:
            child = os.open(part, DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def node_identity(info):
    return info.st_dev, info.st_ino


def owned(info, device, directory=False):
    if (info.st_uid != os.geteuid() or info.st_dev != device or
            (directory and (not stat.S_ISDIR(info.st_mode) or stat.S_IMODE(info.st_mode) & 0o077))):
        raise ValueError('cache ownership/device/mode changed')


def verify_lock(root, guard):
    parent, lock = guard
    info = os.fstat(parent)
    owned(info, info.st_dev, directory=True)
    with open_directory(root) as live:
        if node_identity(os.fstat(live)) != node_identity(info):
            raise ValueError('cache root moved')
    lock_info = os.fstat(lock)
    owned(lock_info, info.st_dev)
    if (not stat.S_ISREG(lock_info.st_mode) or stat.S_IMODE(lock_info.st_mode) & 0o077 or
            node_identity(os.stat('.lock', dir_fd=parent, follow_symlinks=False)) != node_identity(lock_info)):
        raise ValueError('cache lock changed')


@contextlib.contextmanager
def locked(root):
    if not sys.platform.startswith('linux'):
        raise OSError('optional cache quarantine requires Linux')
    try:
        ctypes.CDLL(None, use_errno=True).renameat2
    except AttributeError as error:
        raise OSError('optional cache quarantine unavailable') from error
    with open_directory(root) as parent:
        info = os.fstat(parent)
        owned(info, info.st_dev, directory=True)
        fd = os.open('.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600, dir_fd=parent)
        try:
            guard = (parent, fd)
            verify_lock(root, guard)
            fcntl.flock(fd, fcntl.LOCK_EX)
            verify_lock(root, guard)
            yield guard
        finally:
            os.close(fd)


def cache_marker(fd):
    marker = os.open('.owner.json', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
    with os.fdopen(marker) as stream:
        info = os.fstat(stream.fileno())
        owned(info, os.fstat(fd).st_dev)
        if not stat.S_ISREG(info.st_mode):
            raise ValueError('invalid entry marker')
        return json.loads(stream.read(4096))


@contextlib.contextmanager
def quarantine_directory(parent, create=False):
    if create:
        try:
            os.mkdir(QUARANTINE, 0o700, dir_fd=parent)
        except FileExistsError:
            pass
    fd = os.open(QUARANTINE, DIRECTORY, dir_fd=parent)
    try:
        owned(os.fstat(fd), os.fstat(parent).st_dev, directory=True)
        yield fd
    finally:
        os.close(fd)


def no_pending_quarantine(root):
    # Even an unproven/partially cleared tree blocks ALL later cache writes.
    # One fixed slot bounds interrupted storage; never allocate another slot.
    with open_directory(root) as parent:
        try:
            os.stat(QUARANTINE, dir_fd=parent, follow_symlinks=False)
        except FileNotFoundError:
            return
        with quarantine_directory(parent) as pending:
            if os.listdir(pending):
                raise ValueError('incomplete cache quarantine; reconciliation required')


def rename_noreplace(source_fd, source, destination_fd, destination):
    """Linux atomic claim, same facility as reconcile-run-resources.py."""
    if not sys.platform.startswith('linux'):
        raise OSError('cache quarantine requires Linux')
    try:
        rename = ctypes.CDLL(None, use_errno=True).renameat2
    except AttributeError as error:
        raise OSError('cache quarantine unavailable') from error
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(source_fd, os.fsencode(source), destination_fd, os.fsencode(destination), 1):
        raise OSError(ctypes.get_errno(), 'cache quarantine rename refused')  # RENAME_NOREPLACE


def quarantine_evict(root, guard, name, expected):
    """Audited quarantine-only recursive deletion; root lock spans every step.

    Preflight the entire owned tree before rename. After the atomic claim, pin
    and recheck root/lock/quarantine/tree/ancestor identities before each unlink.
    Keep the marker until the final empty-tree transition. Any interruption
    leaves the single quarantine slot blocking reuse/writes, including invoke.
    """
    if not re.fullmatch(r'[0-9a-f]{64}', name):
        raise ValueError('invalid eviction name')
    verify_lock(root, guard)
    parent, _ = guard
    with quarantine_directory(parent, create=True) as pending:
        if os.listdir(pending):
            raise ValueError('incomplete cache quarantine')
        pending_identity = node_identity(os.fstat(pending))
        source = os.open(name, DIRECTORY, dir_fd=parent)
        try:
            record = [SCHEMA, os.geteuid(), *expected]

            def verify_source():
                verify_lock(root, guard)
                owned(os.fstat(source), os.fstat(parent).st_dev, directory=True)
                if (node_identity(os.fstat(source)) != expected or
                        node_identity(os.stat(name, dir_fd=parent, follow_symlinks=False)) != expected or
                        cache_marker(source) != record):
                    raise ValueError('cache source identity/marker changed')

            def preflight(directory):
                for child in os.listdir(directory):
                    before = os.stat(child, dir_fd=directory, follow_symlinks=False)
                    owned(before, expected[0])
                    if stat.S_ISDIR(before.st_mode):
                        fd = os.open(child, DIRECTORY, dir_fd=directory)
                        try:
                            if node_identity(os.fstat(fd)) != node_identity(before):
                                raise ValueError('cache descendant changed')
                            preflight(fd)
                        finally:
                            os.close(fd)

            verify_source()
            preflight(source)
            verify_source()
            if node_identity(os.stat(QUARANTINE, dir_fd=parent, follow_symlinks=False)) != pending_identity:
                raise ValueError('cache quarantine moved')
            rename_noreplace(parent, name, pending, 'tree')
            os.fsync(parent)
            os.fsync(pending)

            def verify():
                verify_lock(root, guard)
                owned(os.fstat(source), expected[0], directory=True)
                owned(os.fstat(pending), expected[0], directory=True)
                if (node_identity(os.stat(QUARANTINE, dir_fd=parent, follow_symlinks=False)) != pending_identity or
                        node_identity(os.fstat(source)) != expected or
                        node_identity(os.stat('tree', dir_fd=pending, follow_symlinks=False)) != expected or
                        cache_marker(source) != record):
                    raise ValueError('quarantined cache identity/marker changed')

            def clear_quarantined(directory, ancestors=()):
                def anchored():
                    verify()
                    for ancestor, child, identity in ancestors:
                        info = os.stat(child, dir_fd=ancestor, follow_symlinks=False)
                        owned(info, expected[0])
                        if node_identity(info) != identity:
                            raise ValueError('quarantined ancestor changed')
                anchored()
                for child in os.listdir(directory):
                    if directory == source and child == '.owner.json':
                        continue
                    anchored()
                    before = os.stat(child, dir_fd=directory, follow_symlinks=False)
                    owned(before, expected[0])
                    if stat.S_ISDIR(before.st_mode):
                        fd = os.open(child, DIRECTORY, dir_fd=directory)
                        try:
                            if node_identity(os.fstat(fd)) != node_identity(before):
                                raise ValueError('quarantined child changed')
                            clear_quarantined(fd, (*ancestors, (directory, child, node_identity(before))))
                        finally:
                            os.close(fd)
                        anchored()
                        after = os.stat(child, dir_fd=directory, follow_symlinks=False)
                        owned(after, expected[0])
                        if node_identity(after) != node_identity(before):
                            raise ValueError('quarantined child changed')
                        os.rmdir(child, dir_fd=directory)
                    else:
                        anchored()
                        after = os.stat(child, dir_fd=directory, follow_symlinks=False)
                        owned(after, expected[0])
                        if node_identity(after) != node_identity(before):
                            raise ValueError('quarantined artifact changed')
                        os.unlink(child, dir_fd=directory)  # symlink itself, never its target

            verify()
            preflight(source)  # reject a foreign descendant introduced at rename
            clear_quarantined(source)
            verify()
            if os.listdir(source) != ['.owner.json']:
                raise ValueError('quarantined cache not empty')
            os.unlink('.owner.json', dir_fd=source)
            # Marker absence is permitted only in this final empty transition.
            verify_lock(root, guard)
            owned(os.fstat(source), expected[0], directory=True)
            owned(os.fstat(pending), expected[0], directory=True)
            if (os.listdir(source) or node_identity(os.fstat(source)) != expected or
                    node_identity(os.stat('tree', dir_fd=pending, follow_symlinks=False)) != expected or
                    node_identity(os.stat(QUARANTINE, dir_fd=parent, follow_symlinks=False)) != pending_identity):
                raise ValueError('empty quarantine changed')
            os.rmdir('tree', dir_fd=pending)
            os.fsync(pending)
        finally:
            os.close(source)


def trust(env):
    # These fields are supplied by workflow YAML, never by the candidate source.
    if env.get('FORTEMI_CACHE_CI') == '1':
        event, ref = env.get('FORTEMI_CACHE_EVENT'), env.get('FORTEMI_CACHE_REF', '')
        if event not in {'push', 'workflow_dispatch'}:
            return None
        if ref == 'refs/heads/main':
            return 'main'
        if ref.startswith('refs/tags/v'):
            return 'release'
        return None
    if env.get('CI') or env.get('GITHUB_ACTIONS') or env.get('GITEA_ACTIONS'):
        return None
    return 'local'


def repository_identity(root):
    """Share linked worktrees, never infer repository equivalence from a remote.

    The common Git directory identifies one local repository even when its
    worktrees have different absolute source paths. Independent clones stay
    isolated. Non-Git trees (or unavailable Git) retain directory isolation.
    """
    root = root.resolve()
    try:
        result = subprocess.run(
            ['git', '-C', str(root), 'rev-parse', '--path-format=absolute',
             '--show-toplevel', '--git-common-dir'],
            # Caller Git overrides must not identify an unrelated repository.
            env={k: v for k, v in os.environ.items() if not k.startswith('GIT_')},
            capture_output=True, text=True, timeout=5, check=True)
        top, common = result.stdout.strip().splitlines()
        if Path(top).resolve() == root and Path(common).is_dir():
            return ['git-common-dir', str(Path(common).resolve())]
    except (OSError, ValueError, subprocess.SubprocessError):
        pass
    return ['directory', str(root)]


def ci_repository_authority(root, env):
    """Validate the existing workflow context and registered tooling checkout.

    These are runner/workflow assertions, not proof against hostile same-uid
    code. A matching remote alone never admits a local clone to CI reuse.
    """
    if (env.get('FORTEMI_CACHE_CI') != '1' or trust(env) not in {'main', 'release'} or
            env.get('GITHUB_ACTIONS') != 'true' or
            env.get('GITHUB_SERVER_URL') != 'https://git.integrolabs.net' or
            env.get('GITHUB_REPOSITORY', '').lower() != 'fortemi/fortemi' or
            env.get('GITHUB_EVENT_NAME') != env.get('FORTEMI_CACHE_EVENT') or
            env.get('GITHUB_REF') != env.get('FORTEMI_CACHE_REF')):
        raise ValueError('CI repository context unavailable')
    owner, attempt = env.get('CI_RESOURCE_SCOPE', ''), env.get('CI_RESOURCE_ATTEMPT', '')
    if not all(re.fullmatch(r'[1-9][0-9]*', v) for v in (owner, attempt)):
        raise ValueError('CI ownership context unavailable')
    tooling = Path(env.get('CI_SAFETY_TOOLS', ''))
    base = root / 'target/.ci-tools'
    if (not tooling.is_absolute() or tooling.parent != base or
            not re.fullmatch(re.escape(owner + '-' + attempt) + r'\.[0-9a-f]{24}', tooling.name)):
        raise ValueError('CI tooling registration unavailable')
    with open_directory(tooling) as fd:
        info = os.fstat(fd)
        owned(info, info.st_dev, directory=True)
        marker_fd = os.open('.owner.json', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        with os.fdopen(marker_fd) as stream:
            marker_info = os.fstat(stream.fileno())
            owned(marker_info, info.st_dev)
            if not stat.S_ISREG(marker_info.st_mode) or stat.S_IMODE(marker_info.st_mode) & 0o077:
                raise ValueError('invalid CI registration marker')
            record = json.loads(stream.read(4096))
        if (not isinstance(record, dict) or record.get('fortemi.ci.scope') != owner or
                record.get('fortemi.ci.attempt') != attempt or
                record.get('identity') != list(node_identity(info))):
            raise ValueError('CI registration identity changed')
    # Read the literal local configuration, not get-url (which applies insteadOf).
    # Never let inherited Git overrides redirect either probe.
    git_env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
    for checkout in (root, tooling):
        top = subprocess.run(['git', '-C', str(checkout), 'rev-parse', '--show-toplevel'],
                             env=git_env, capture_output=True, text=True, timeout=5, check=True)
        origin = subprocess.run(['git', '-C', str(checkout), 'config', '--local', '--get-all', 'remote.origin.url'],
                                env=git_env, capture_output=True, text=True, timeout=5, check=True)
        if (Path(top.stdout.strip()).resolve() != checkout or
                origin.stdout.strip().lower() != 'https://git.integrolabs.net/fortemi/fortemi.git'):
            raise ValueError('CI checkout authority mismatch')
    return ['trusted-ci-repository', 'https://git.integrolabs.net', 'fortemi/fortemi']


def cache_repository_identity(root, env):
    if env.get('FORTEMI_CACHE_CI') == '1':
        return ci_repository_authority(root, env)
    return repository_identity(root)


def identity(root, compiler, env, variant):
    # Source stays under Cargo fingerprint control. Hash dependency/configuration
    # inputs here, so an ordinary source edit can reuse compiled dependencies.
    files = []
    for directory, children, names in os.walk(root):
        children[:] = sorted(c for c in children if c not in {'target', '.git', 'node_modules', '.aiwg', '.claude', '.codex'} and not Path(directory, c).is_symlink())
        files.extend(Path(directory, name) for name in sorted(names) if name in {'Cargo.toml', 'Cargo.lock'})
    files += sorted((root / '.cargo').glob('*.toml'))
    for directory in ['.sqlx', 'migrations']:
        files += sorted(p for p in (root / directory).rglob('*') if p.is_file())
    files += [p for p in [root / 'rust-toolchain', root / 'rust-toolchain.toml',
                          root / '.gitea/workflows/ci-builder.yaml', root / '.gitea/workflows/test.yml'] if p.is_file()]
    inputs = [(str(p.relative_to(root)), hashlib.sha256(p.read_bytes()).hexdigest()) for p in files]
    flags = {k: v for k, v in env.items() if k.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_', 'CC_', 'CXX_', 'CFLAGS_', 'PKG_CONFIG_', 'OPENSSL_')) or
             k in {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS',
                   'CARGO_BUILD_TARGET', 'CARGO_INCREMENTAL', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                   'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN', 'CC', 'CXX', 'AR', 'LD', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS',
                   'SQLX_OFFLINE'}}
    flags.pop('CARGO_TARGET_DIR', None)
    # Stamps are runtime values in Core today. Cargo tracks env!/option_env!
    # and declared build-script env inputs if a crate embeds them in future.
    return digest([SCHEMA, cache_repository_identity(root, env), compiler, variant, inputs, flags])


def entries(root):
    no_pending_quarantine(root)
    found = []
    for path in root.iterdir():
        if not re.fullmatch(r'[0-9a-f]{64}', path.name):
            continue
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) & 0o077:
            raise ValueError('invalid cache entry')
        marker = path / '.owner.json'
        fd = os.open(marker, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd) as stream:
            marker_info = os.fstat(stream.fileno())
            if not stat.S_ISREG(marker_info.st_mode) or marker_info.st_uid != os.geteuid():
                raise ValueError('invalid entry marker')
            record = json.loads(stream.read(4096))
        if record != [SCHEMA, os.geteuid(), info.st_dev, info.st_ino]:
            raise ValueError('cache entry ownership changed')
        size = 0
        for directory, children, names in os.walk(path, followlinks=False):
            for name in children:
                item = Path(directory, name).lstat()
                if not stat.S_ISLNK(item.st_mode) and item.st_uid != os.geteuid():
                    raise ValueError('foreign cache descendant')
            for name in names:
                item = Path(directory, name).lstat()
                if stat.S_ISREG(item.st_mode):
                    if item.st_uid != os.geteuid():
                        raise ValueError('foreign cache artifact')
                    size += item.st_size
        found.append((path.stat().st_mtime, path, size))
    return sorted(found)


def retention_bytes(env, lane):
    # No caller-configurable budget. Only the workflow-trusted main lane grows.
    return CI_MAIN_MAX_BYTES if lane == 'main' and trust(env) == 'main' else MAX_BYTES


def prune(root, keep=None, reserve=0, *, guard, max_bytes=None,
          selected=None, phase='direct'):
    # Resolve at call time: local/default callers and small-budget fixtures keep
    # honoring MAX_BYTES. Never bind the constant as a default argument.
    limit = MAX_BYTES if max_bytes is None else max_bytes
    verify_lock(root, guard)
    current = entries(root)
    count, size = len(current), sum(e[2] for e in current)
    selected = keep if selected is None else selected
    count_pressure, byte_pressure = count + reserve > MAX_ENTRIES, size > limit
    reason = 'both' if count_pressure and byte_pressure else (
        'count' if count_pressure else 'bytes' if byte_pressure else 'none')
    summary = {'cache_retention': 'before', 'phase': phase,
               'before_entries': count, 'before_bytes': size,
               'max_entries': MAX_ENTRIES, 'max_bytes': limit,
               'reserve_entries': reserve, 'largest_entry_bytes': max((e[2] for e in current), default=0),
               'reason': reason,
               'oversized_entries_before': sum(e[2] > limit for e in current),
               'selected_oversized': any(p == selected and w > limit for _, p, w in current)}
    print(json.dumps(summary), file=sys.stderr)
    removed = removed_bytes = 0
    retained = list(current)
    try:
        for entry in current:
            _, path, weight = entry
            if count + reserve <= MAX_ENTRIES and size <= limit:
                break
            if path == keep:
                continue
            expected = node_identity(path.lstat())
            quarantine_evict(root, guard, path.name, expected)
            count -= 1
            size -= weight
            removed += 1
            removed_bytes += weight
            retained = [candidate for candidate in retained if candidate != entry]
        if count + reserve > MAX_ENTRIES or size > limit:
            raise ValueError('cache budget unavailable')
    except (OSError, ValueError, KeyError, NotImplementedError, RecursionError):
        # A partial quarantine failure cannot support post-prune totals.
        print(json.dumps(dict(summary, cache_retention='unavailable',
                              retained_entries=None, retained_bytes=None,
                              evicted_entries=None, evicted_bytes=None,
                              oversized_entries_after=None, selected_retained=None)), file=sys.stderr)
        raise
    result = dict(summary, cache_retention='complete', retained_bytes=size,
                  retained_entries=count, evicted_entries=removed, evicted_bytes=removed_bytes,
                  oversized_entries_after=sum(w > limit for _, _, w in retained),
                  selected_retained=any(p == selected for _, p, _ in retained))
    print(json.dumps(result), file=sys.stderr)
    return result


def warning(operation):
    # Never print paths, exception payloads, arguments or environment values.
    print(f'::warning::Optional Cargo cache {operation}; unsafe entries were retained. See cache state telemetry.', file=sys.stderr)


def fresh_target(env, reason):
    # Independent of the optional cache root, including symlinks/foreign entries.
    target = tempfile.mkdtemp(prefix='fortemi-cargo-fresh-', dir=env.get('TMPDIR'))
    print(json.dumps({'cache_schema': SCHEMA, 'cache_enabled': False,
                      'cache_present': False, 'cache_state': reason}), file=sys.stderr)
    return {'CARGO_TARGET_DIR': target, 'FORTEMI_CARGO_CACHE_ENTRY': '',
            'FORTEMI_CARGO_CACHE_LANE': ''}


def setup(env):
    try:
        return select_cache(env)
    except (OSError, ValueError, subprocess.SubprocessError, KeyError, NotImplementedError, RecursionError):
        warning('unavailable or invalid')
        return fresh_target(env, 'unavailable')


def select_cache(env):
    root = Path(env.get('GITHUB_WORKSPACE', os.getcwd())).resolve()
    lane = trust(env)
    if lane is None or env.get('FORTEMI_CACHE_DISABLE') == '1':
        print(json.dumps({'cache_schema': SCHEMA, 'cache_enabled': False,
                          'cache_present': False, 'cache_state': 'disabled' if lane else 'untrusted'}), file=sys.stderr)
        return {}  # preserve the original cold/run-local target and Cargo failure
    if env.get('FORTEMI_CACHE_CI') == '1' and not env.get('FORTEMI_CACHE_VARIANT'):
        warning('job identity unavailable')
        return fresh_target(env, 'missing-job-identity')
    # Admit CI authority before touching the persistent root or probing tools.
    cache_repository_identity(root, env)
    cache = private_dir(Path(env.get('FORTEMI_CARGO_CACHE_ROOT', root / 'target/.cargo-reuse-v1')) / lane)
    compiler = subprocess.check_output(['rustc', '-vV'], text=True).strip()
    if shutil.which('dpkg-query'):
        compiler += '\npackages:' + digest(subprocess.check_output(['dpkg-query', '-W', '-f=${Package}=${Version}\n'], text=True))
    variant = env.get('FORTEMI_CACHE_VARIANT', 'local-dev-default')
    key = identity(root, compiler, env, variant)
    with locked(cache) as guard:
        target = cache / key
        hit = target.exists()
        prune(cache, keep=target if hit else None, reserve=0 if hit else 1, guard=guard,
              max_bytes=retention_bytes(env, lane), selected=target, phase='setup')
        if not hit:
            private_dir(target)
            info = target.stat()
            (target / '.owner.json').write_text(json.dumps([SCHEMA, os.geteuid(), info.st_dev, info.st_ino]))
            # The marker itself is charged; admitting it must also fit the cap.
            prune(cache, keep=target, guard=guard, max_bytes=retention_bytes(env, lane),
                  selected=target, phase='setup-created')
        else:
            entries(cache)  # validate before allowing Cargo to write
        os.utime(target, None)
        retained = entries(cache)  # actual setup outcome includes the new marker
    print(json.dumps({'cache_schema': SCHEMA, 'cache_enabled': True,
                      'cache_state': 'present' if hit else 'new', 'cache_present': hit,
                      'retained_entries': len(retained), 'retained_bytes': sum(w for _, _, w in retained),
                      'max_entries': MAX_ENTRIES, 'max_bytes': retention_bytes(env, lane)}), file=sys.stderr)
    return {'CARGO_TARGET_DIR': str(target / 'target'), 'FORTEMI_CARGO_CACHE_ENTRY': str(target),
            'FORTEMI_CARGO_CACHE_LANE': lane}


def finalize(env):
    if not env.get('FORTEMI_CARGO_CACHE_ENTRY'):
        print(json.dumps({'cache_finalization': 'skipped', 'selected_retained': False,
                          'workload_status_preserved': True}), file=sys.stderr)
        return 0
    try:
        root = private_dir(Path(env['FORTEMI_CARGO_CACHE_ENTRY']).parent)
        with locked(root) as guard:
            result = prune(root, guard=guard, max_bytes=retention_bytes(env, root.name),
                           selected=Path(env['FORTEMI_CARGO_CACHE_ENTRY']), phase='finalize')
            print(json.dumps({'cache_finalization': 'complete',
                              'selected_retained': result['selected_retained'],
                              'selected_oversized': result['selected_oversized'],
                              'workload_status_preserved': True}), file=sys.stderr)
    except (OSError, ValueError, KeyError, NotImplementedError, RecursionError):
        warning('finalization unavailable or invalid')
        print(json.dumps({'cache_finalization': 'unavailable', 'selected_retained': None,
                          'retained_entries': None, 'retained_bytes': None,
                          'workload_status_preserved': True}), file=sys.stderr)
    return 0


def proof(env):
    entry = env.get('FORTEMI_CARGO_CACHE_ENTRY')
    matched = bool(entry) and env.get('CARGO_TARGET_DIR') == str(Path(entry) / 'target')
    print(json.dumps({'cargo_cache_workflow_proof': 1, 'cache_enabled': bool(entry),
                      'target_matches_entry': matched,
                      'job_identity_present': bool(env.get('FORTEMI_CACHE_VARIANT'))}))
    if entry and not matched:
        warning('target environment was overridden')
        return 1
    return 0


def invoke(args, env):
    real = env.get('FORTEMI_REAL_CARGO') or shutil.which('cargo')
    if not real or Path(real).resolve() == Path(__file__).resolve():
        raise ValueError('real Cargo unavailable')
    entry = env.get('FORTEMI_CARGO_CACHE_ENTRY')
    with contextlib.ExitStack() as guard:
        if entry and env.get('FORTEMI_CACHE_LOCK_HELD') != entry:
            try:
                guard.enter_context(locked(private_dir(Path(entry).parent)))
                # The entry may have disappeared/changed after setup.
                valid = {str(path) for _, path, _ in entries(Path(entry).parent)}
                if entry not in valid:
                    raise ValueError('selected entry disappeared')
            except (OSError, ValueError, NotImplementedError, RecursionError):
                guard.close()
                warning('invocation entry unavailable or invalid')
                env.update(fresh_target(env, 'unavailable'))
                entry = None
        if entry:
            env['FORTEMI_CACHE_LOCK_HELD'] = entry
        start = time.monotonic()
        command = args[0] if args else ''
        # Do not alter explicit structured output or third-party Cargo commands.
        instrument = bool(entry) and command in {'build', 'check', 'clippy', 'test', 'run'} and not any(
            a.startswith('--message-format') for a in args)
        launched = list(args)
        if instrument:
            end = launched.index('--') if '--' in launched else len(launched)
            launched.insert(end, '--message-format=json-render-diagnostics')
        fresh = artifacts = 0
        success = None
        process = subprocess.Popen([real, *launched], env=env, stdout=subprocess.PIPE)
        try:
            for line in process.stdout:
                try:
                    message = json.loads(line)
                except (ValueError, UnicodeError):
                    message = None
                compiler_output = success is None
                if compiler_output and isinstance(message, dict) and message.get('reason') == 'compiler-artifact':
                    artifacts += 1
                    fresh += message.get('fresh') is True
                if isinstance(message, dict) and message.get('reason') == 'build-finished':
                    success = message.get('success')
                if instrument and compiler_output and isinstance(message, dict) and 'reason' in message:
                    rendered = message.get('message', {}).get('rendered') if message.get('reason') == 'compiler-message' else None
                    if rendered:
                        sys.stderr.write(rendered)
                else:
                    sys.stdout.buffer.write(line)
                    sys.stdout.buffer.flush()
            code = process.wait()
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait()
        # No arguments, paths, environment values, or compiler payloads in telemetry.
        print(json.dumps({'cargo_cache_telemetry': 1, 'command': command if re.fullmatch(r'[a-z-]+', command) else 'other',
                          'elapsed_seconds': round(time.monotonic() - start, 3), 'exit_code': code,
                          'compiler_artifacts': artifacts if instrument else None,
                          'fresh': fresh if instrument else None, 'nonfresh': artifacts - fresh if instrument else None,
                          'build_finished': success, 'cache_enabled': bool(entry)}), file=sys.stderr)
        return code if code >= 0 else 128 - code


def main(args):
    env = dict(os.environ)
    if args == ['setup']:
        updates = setup(env)
        with open(env['GITHUB_ENV'], 'a') as stream:
            for key, value in updates.items():
                if '\n' in value or '\r' in value:
                    raise ValueError('invalid environment path')
                stream.write(f'{key}={value}\n')
        return 0
    if args == ['finalize']:
        return finalize(env)
    if args == ['proof']:
        return proof(env)
    if args and args[0] == 'local':
        selection = args[1:args.index('--')] if '--' in args[1:] else args[1:]
        env['FORTEMI_CACHE_VARIANT'] = digest(selection)
        env.update(setup(env))
        args = args[1:]
    if args and args[0] == '--':
        args = args[1:]
    try:
        return invoke(args, env)
    finally:
        if os.environ.get('FORTEMI_CACHE_CI') != '1' and env.get('FORTEMI_CARGO_CACHE_ENTRY'):
            finalize(env)


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, ValueError, subprocess.CalledProcessError, KeyError):
        sys.exit('Cargo cache setup/invocation failed; use FORTEMI_CACHE_DISABLE=1 with a fresh owned target')
