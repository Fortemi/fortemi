#!/usr/bin/env python3
"""Reap explicitly owned attempts only; uncertainty always retains resources.

Threat model: the runner and its Docker daemon are single-tenant and
non-adversarial. Label, age and identity rules protect against our own CI's
mistakes (cancels, reruns, crashed jobs), not a hostile process racing deletion.
The residual windows -- a tree moved mid-delete; a volume swapped by name --
exist only against a hostile actor already on the runner. Fortemi/fortemi#1236
tracks those windows. Named volumes are retained; anonymous data is removed
only with its labelled container's immutable ID, using docker rm -v.
"""
import argparse
from contextlib import contextmanager
import ctypes
import datetime as dt
import fcntl
import email.utils
import json
import math
import os
import re
import stat
import subprocess
import time
import urllib.request

LABEL = 'fortemi.ci.scope'
ATTEMPT = 'fortemi.ci.attempt'
TERMINAL = {'success', 'failure', 'cancelled', 'skipped'}
MIN_AGE = 1800
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
QUARANTINE = '.reconcile-quarantine'


def numeric(value):
    return isinstance(value, str) and re.fullmatch(r'[1-9][0-9]*', value) is not None


def timeout(limit):
    remaining = globals().get('deadline', float('inf')) - time.monotonic()
    if remaining <= 0:
        raise TimeoutError('deadline')
    return min(limit, remaining)


def docker(*args):
    return subprocess.check_output([os.environ.get('CI_DOCKER_REAL', 'docker'), *args],
                                   text=True, timeout=timeout(20)).strip()


def api(path):
    # Existing job secret, read at call time, never on argv or in diagnostics.
    token = os.environ.get('CI_ACTIONS_TOKEN', '')
    repo = os.environ.get('GITHUB_REPOSITORY', '')
    if not token or not re.fullmatch(r'[\w.-]+/[\w.-]+', repo):
        raise ValueError('missing authentication or repository')
    base = os.environ.get('GITHUB_API_URL', 'https://git.integrolabs.net/api/v1')
    request = urllib.request.Request(f'{base}/repos/{repo}/actions/{path}',
                                     headers={'Authorization': f'token {token}'})
    with urllib.request.urlopen(request, timeout=timeout(15)) as response:
        # The server clock is an independent age bound; a host clock jump alone
        # cannot turn a recently completed run into a deletion candidate.
        server_now = email.utils.parsedate_to_datetime(response.headers['Date']).timestamp()
        if not math.isfinite(server_now):
            raise ValueError('invalid server date')
        return json.load(response), server_now


def status(value):
    state = value.get('status')
    if state == 'completed':
        state = value.get('conclusion')
    if state not in TERMINAL | {'queued', 'in_progress', 'waiting'}:
        raise ValueError('unusable status')
    return state


def timestamp(value):
    parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('timestamp has no timezone')
    result = parsed.timestamp()
    if not math.isfinite(result):
        raise ValueError('non-finite timestamp')
    return result


def old(created, server_now):
    return math.isfinite(created) and MIN_AGE <= min(time.time(), server_now) - created


def run_state(run_id):
    value, server_now = api(f'runs/{run_id}')
    if str(value.get('id')) != run_id or not numeric(str(value.get('run_attempt'))):
        raise ValueError('unusable run response')
    state = status(value)
    # v1.27.2 exposes completed_at (latest attempt), not updated_at.
    completed = timestamp(value['completed_at']) if state in TERMINAL else None
    if completed is not None and completed > min(time.time(), server_now):
        raise ValueError('future completion')
    return state, str(value['run_attempt']), completed, server_now


def eligible(resource, state, current):
    owner, attempt, created = resource[:3]
    if current:
        # Explicit final teardown is confined to this exact attempt. Still check
        # the latest attempt so a delayed cleanup cannot affect a rerun.
        return (owner == os.environ['CI_RESOURCE_SCOPE'] and
                attempt == os.environ['CI_RESOURCE_ATTEMPT'] == state[1])
    return state[0] in TERMINAL and old(state[2], state[3]) and old(created, state[3])


def labels(info, kind):
    return ((info.get('Config') or {}).get('Labels') if kind in {'image', 'container'}
            else info.get('Labels')) or {}


def inspect(kind, identity):
    value = json.loads(docker(kind, 'inspect', identity))
    if not isinstance(value, list) or len(value) != 1:
        raise ValueError('unusable Docker inspection')
    return value[0]


def docker_record(kind, identity):
    info = inspect(kind, identity)
    tags = labels(info, kind)
    owner, attempt = tags.get(LABEL), tags.get(ATTEMPT)
    # Every scope-labelled named volume is reported, even legacy/malformed labels.
    if kind == 'volume':
        return (owner, attempt, 0, kind, info['Name'], None) if LABEL in tags else None
    if not numeric(owner) or not numeric(attempt):
        return None
    # Never remove containers/networks/images by a reusable name or tag.
    immutable = info['Id']
    created_text = info['Created']
    return owner, attempt, timestamp(created_text), kind, immutable, (created_text, tags)


def inventory(keep=None):
    resources = []
    for kind, command in [('container', ['ps', '-aq', '--no-trunc']),
                          ('network', ['network', 'ls', '-q', '--no-trunc']),
                          ('volume', ['volume', 'ls', '-q']),
                          ('image', ['image', 'ls', '-q', '--no-trunc'])]:
        for identity in set(docker(*command, '--filter', f'label={LABEL}').split()):
            record = docker_record(kind, identity)
            if record:
                resources.append(record)
            elif keep and LABEL in labels(inspect(kind, identity), kind):
                # Scope-labelled but with unusable ownership metadata: never
                # removed, but always visible in the kept count.
                keep(kind, identity, 'scope label present but run/attempt labels unusable')
    return resources


@contextmanager
def open_directory(path):
    """Open every absolute path component without following any symlink."""
    path = os.path.abspath(path)
    fd = os.open('/', DIRECTORY)
    try:
        for part in path.split('/')[1:]:
            if not part:
                continue
            next_fd = os.open(part, DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        yield fd
    finally:
        os.close(fd)


def identity(value):
    return value.st_dev, value.st_ino


def marker(fd):
    descriptor = os.open('.owner.json', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
    with os.fdopen(descriptor) as stream:
        if (not stat.S_ISREG(os.fstat(stream.fileno()).st_mode) or
                os.fstat(stream.fileno()).st_dev != os.fstat(fd).st_dev):
            raise ValueError('marker is not regular')
        value = json.load(stream)
    if not numeric(value.get(LABEL)) or not numeric(value.get(ATTEMPT)):
        raise ValueError('invalid marker ownership')
    created = float(value['created'])
    if not math.isfinite(created) or created > time.time():
        raise ValueError('invalid marker timestamp')
    return value


def owned_roots(path, keep=lambda *args: None):
    result = []
    with open_directory(path) as parent:
        with os.scandir(parent) as entries:
            for entry in entries:
                if not re.fullmatch(r'[1-9][0-9]*-[1-9][0-9]*(?:\.[\w-]+)?', entry.name):
                    continue
                try:
                    fd = os.open(entry.name, DIRECTORY, dir_fd=parent)
                    try:
                        info = marker(fd)
                        if entry.name.split('.', 1)[0] != f'{info[LABEL]}-{info[ATTEMPT]}':
                            raise ValueError('root name/marker mismatch')
                        if tuple(info['identity']) != identity(os.fstat(fd)):
                            raise ValueError('root marker device/inode mismatch')
                        result.append((info[LABEL], info[ATTEMPT], float(info['created']),
                                       'root', entry.name, (identity(os.fstat(fd)), info)))
                    finally:
                        os.close(fd)
                except (OSError, ValueError, KeyError, TypeError):
                    keep('root', entry.name, 'unusable root identity/marker')
    return result


def rename_noreplace(source_fd, source, destination_fd, destination):
    """Linux atomic claim: absence of renameat2 is a keep, never a fallback."""
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(source_fd, os.fsencode(source), destination_fd, os.fsencode(destination), 1):
        raise OSError(ctypes.get_errno(), 'quarantine rename refused')  # RENAME_NOREPLACE


@contextmanager
def quarantine(parent, create=False):
    if create:
        try:
            os.mkdir(QUARANTINE, 0o700, dir_fd=parent)
        except FileExistsError:
            pass
    fd = os.open(QUARANTINE, DIRECTORY, dir_fd=parent)
    try:
        info = os.fstat(fd)
        if (info.st_dev != os.fstat(parent).st_dev or info.st_uid != os.geteuid() or
                stat.S_IMODE(info.st_mode) != 0o700):
            raise ValueError('quarantine device/ownership/mode changed')
        # Serialize our own reconcilers; never race another cleanup's traversal.
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield fd
    finally:
        os.close(fd)


def quarantine_name(record):
    dev, inode = record[5][0]
    return f'{record[0]}-{record[1]}.{dev}.{inode}'


def quarantined_roots(path, keep):
    result = []
    with open_directory(path) as parent:
        if not os.path.lexists(os.path.join(path, QUARANTINE)):
            return result
        with quarantine(parent) as pending:
            for name in os.listdir(pending):
                try:
                    with open_child(pending, name) as slot:
                        # A crash between ticket unlink and slot rmdir leaves only
                        # empty reconciler metadata. Recheck its own inode on retry.
                        match = re.fullmatch(r'([1-9][0-9]*)-([1-9][0-9]*)\.[0-9]+\.[0-9]+', name)
                        if match and not os.listdir(slot):
                            info = os.fstat(slot)
                            result.append((match[1], match[2], info.st_mtime, 'quarantine-empty',
                                           name, (identity(info), None)))
                            continue
                        info = marker(slot)  # durable ticket survives removal of the tree marker
                        expected = tuple(info['identity'])
                        record = (info[LABEL], info[ATTEMPT], float(info['created']),
                                  'quarantine', name, (expected, info['ownership']))
                        if quarantine_name(record) != name:
                            raise ValueError('quarantine ticket/name mismatch')
                        # A crash after tree rmdir leaves only the durable ticket.
                        try:
                            tree = os.stat('tree', dir_fd=slot, follow_symlinks=False)
                        except FileNotFoundError:
                            tree = None
                        if tree is not None and identity(tree) != expected:
                            raise ValueError('quarantined tree changed')
                        result.append(record)
                except (OSError, ValueError, KeyError, TypeError):
                    keep('quarantine', name, 'unusable quarantine identity/ticket')
    return result


@contextmanager
def open_child(parent, name):
    fd = os.open(name, DIRECTORY, dir_fd=parent)
    try:
        if os.fstat(fd).st_dev != os.fstat(parent).st_dev:
            raise ValueError('directory crosses filesystem')
        yield fd
    finally:
        os.close(fd)


def remove_root(path, record):
    """Claim a root atomically, then delete only the pinned quarantine tree."""
    expected, ownership = record[5]
    with open_directory(path) as parent, quarantine(parent, create=True) as pending:
        if record[3] == 'quarantine-empty':
            name = record[4]
            with open_child(pending, name) as slot:
                if (identity(os.fstat(slot)) != expected or os.listdir(slot) or
                        identity(os.stat(name, dir_fd=pending, follow_symlinks=False)) != expected):
                    raise ValueError('empty quarantine slot changed')
                os.rmdir(name, dir_fd=pending)
            return
        name = quarantine_name(record)
        if record[3] == 'root':
            with open_child(parent, record[4]) as source:
                if (identity(os.fstat(source)) != expected or
                        identity(os.stat(record[4], dir_fd=parent, follow_symlinks=False)) != expected or
                        marker(source) != ownership or expected[0] != os.fstat(pending).st_dev):
                    raise ValueError('root identity/marker/device changed before quarantine')
                # mkdir cannot replace an existing slot. A collision is retained.
                os.mkdir(name, 0o700, dir_fd=pending)
                with open_child(pending, name) as slot:
                    ticket = dict(ownership, identity=expected, ownership=ownership)
                    descriptor = os.open('.owner.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                         0o600, dir_fd=slot)
                    with os.fdopen(descriptor, 'w') as stream:
                        json.dump(ticket, stream)
                        stream.flush()
                        os.fsync(stream.fileno())
                    os.fsync(slot)
                    rename_noreplace(parent, record[4], slot, 'tree')
                    # Check BOTH descriptors and the destination entry after rename.
                    if (identity(os.fstat(source)) != expected or
                            identity(os.stat('tree', dir_fd=slot, follow_symlinks=False)) != expected):
                        raise ValueError('root changed during quarantine rename')
                    os.fsync(parent)
                    os.fsync(slot)
        with open_child(pending, name) as slot:
            slot_identity = identity(os.fstat(slot))
            ticket = dict(ownership, identity=list(expected), ownership=ownership)

            def verify_slot():
                with open_directory(path) as live_parent:
                    if identity(os.fstat(live_parent)) != identity(os.fstat(parent)):
                        raise ValueError('parent moved')
                if (identity(os.stat(QUARANTINE, dir_fd=parent, follow_symlinks=False)) != identity(os.fstat(pending)) or
                        identity(os.stat(name, dir_fd=pending, follow_symlinks=False)) != slot_identity or
                        marker(slot) != ticket):
                    raise ValueError('quarantine moved or ticket changed')

            verify_slot()
            try:
                fd = os.open('tree', DIRECTORY, dir_fd=slot)
            except FileNotFoundError:
                fd = None  # recovery after tree rmdir, before ticket removal
            if fd is not None:
                try:
                    def verify():
                        verify_slot()
                        if (identity(os.fstat(fd)) != expected or
                                identity(os.stat('tree', dir_fd=slot, follow_symlinks=False)) != expected):
                            raise ValueError('quarantined tree moved')
                        try:
                            if marker(fd) != ownership:
                                raise ValueError('quarantined marker changed')
                        except FileNotFoundError:
                            # Only our final empty-tree transition may lack its marker.
                            if os.listdir(fd):
                                raise ValueError('quarantined marker missing from nonempty tree')

                    def clear(directory, ancestors=()):
                        def anchored():
                            verify()
                            for parent_fd, child_name, expected_child in ancestors:
                                if identity(os.stat(child_name, dir_fd=parent_fd, follow_symlinks=False)) != expected_child:
                                    raise ValueError('descendant moved')
                        anchored()
                        for child in os.listdir(directory):
                            if directory == fd and child == '.owner.json':
                                continue
                            before = os.stat(child, dir_fd=directory, follow_symlinks=False)
                            if before.st_dev != expected[0]:
                                raise ValueError('entry crosses filesystem')
                            if stat.S_ISDIR(before.st_mode):
                                with open_child(directory, child) as child_fd:
                                    if identity(os.fstat(child_fd)) != identity(before):
                                        raise ValueError('child changed')
                                    clear(child_fd, (*ancestors, (directory, child, identity(before))))
                                    anchored()
                                    if identity(os.stat(child, dir_fd=directory, follow_symlinks=False)) != identity(before):
                                        raise ValueError('child changed')
                                    os.rmdir(child, dir_fd=directory)
                            else:
                                anchored()
                                if identity(os.stat(child, dir_fd=directory, follow_symlinks=False)) != identity(before):
                                    raise ValueError('entry changed')
                                os.unlink(child, dir_fd=directory)  # unlink symlinks themselves
                    verify()
                    clear(fd)
                    verify()
                    if '.owner.json' in os.listdir(fd):
                        os.unlink('.owner.json', dir_fd=fd)
                    verify()
                    os.rmdir('tree', dir_fd=slot)
                finally:
                    os.close(fd)
            verify_slot()
            os.unlink('.owner.json', dir_fd=slot)
            os.rmdir(name, dir_fd=pending)


def reconcile(current=False):
    removed = kept = 0

    def keep(kind, target, reason):
        nonlocal kept
        kept += 1
        print(f'::warning::Kept {kind} {target}: {reason}.')

    try:
        path = os.environ['CI_RUNS_ROOT']
        resources = inventory(keep)
        kept += len(resources)
        for record in resources:
            if record[3] == 'volume':
                print(f'::warning::Kept labelled named volume {record[4]}: no identity-safe removal; never delete by name.')
        parents = {}
        # Workflow tooling is owned before Git writes into it. Recover crashed
        # bootstrap checkouts by exactly the same age/status/quarantine rules.
        paths = {path, os.environ.get('CI_TOOLS_ROOT', os.path.join(os.path.dirname(path), '.ci-tools')),
                 f'/tmp/fortemi-ci-runs-{os.geteuid()}'}
        for root_path in sorted(paths):
            if os.path.lexists(root_path):
                for discover in [owned_roots, quarantined_roots]:
                    found = discover(root_path, keep)
                    kept += len(found)
                    resources += found
                    parents.update({id(record): root_path for record in found})
        candidates = [r for r in resources if r[3] != 'volume' and
                      (r[:2] == (os.environ['CI_RESOURCE_SCOPE'], os.environ['CI_RESOURCE_ATTEMPT'])
                       if current else r[0] != os.environ.get('CI_RESOURCE_SCOPE') and old(r[2], time.time()))]
        # Preflight ALL active-run lookups before any quarantine or Docker writes.
        states = {owner: run_state(owner) for owner in sorted({r[0] for r in candidates})}
        for record in candidates:
            owner, _, _, kind, target, _ = record
            if not eligible(record, states[owner], current):
                continue
            # Never rely on the inventory-time terminal snapshot. Lookup failures
            # stop the entire sweep, while individual identity/path failures keep.
            if not eligible(record, run_state(owner), current):
                continue
            try:
                if kind in {'root', 'quarantine', 'quarantine-empty'}:
                    remove_root(parents[id(record)], record)
                else:
                    # Inspect the SAME immutable ID immediately before its removal.
                    if docker_record(kind, target) != record:
                        raise ValueError('Docker identity/labels changed')
                    command = {'container': ['rm', '-f', '-v'], 'network': ['network', 'rm'],
                               'image': ['image', 'rm']}[kind]
                    docker(*command, target)
            except Exception:
                print(f'::warning::Kept {kind} {target}: identity/data/path unavailable or changed.')
                continue
            removed += 1
            kept -= 1
            print(f'Removed owned {kind} {target} (run {owner})')
    except Exception:
        print('::warning::Reconciliation lookup/data/path unavailable; keeping remaining resources.')
    finally:
        print(f'reconcile: removed={removed} kept={kept}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--current', action='store_true')
    reconcile(parser.parse_args().current)
