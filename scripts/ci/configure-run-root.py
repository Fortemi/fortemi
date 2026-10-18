#!/usr/bin/env python3
"""Validate ownership before writes; choose a fresh registered root on mismatch."""
import importlib.util
import json
import os
from pathlib import Path
import secrets
import sys
import time

spec = importlib.util.spec_from_file_location('resources', Path(__file__).with_name('reconcile-run-resources.py'))
resources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resources)


def mkdirs(path):
    """Create/open an absolute directory chain without traversing symlinks."""
    fd = os.open('/', resources.DIRECTORY)
    try:
        for part in os.path.abspath(path).split('/')[1:]:
            try:
                os.mkdir(part, 0o700, dir_fd=fd)
            except FileExistsError:
                pass
            child = os.open(part, resources.DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def configure():
    owner, attempt = os.environ['CI_RESOURCE_SCOPE'], os.environ['CI_RESOURCE_ATTEMPT']
    if not all(resources.numeric(value) for value in [owner, attempt]):
        raise ValueError('invalid owner')
    base = os.path.abspath(os.environ['CI_RUNS_ROOT'])
    original = os.path.join(base, f'{owner}-{attempt}')
    try:
        parent = mkdirs(base)
    except OSError:
        print('::warning::Kept unusable runs ancestor; using fresh fallback root.', file=sys.stderr)
        base = f'/tmp/fortemi-ci-runs-{os.geteuid()}'
        parent = mkdirs(base)
    try:
        name = f'{owner}-{attempt}'
        fresh = base != os.path.dirname(original)
        while True:
            if fresh:
                name = f'{owner}-{attempt}.{secrets.token_hex(8)}'
            try:
                os.mkdir(name, 0o700, dir_fd=parent)
                created = True
            except FileExistsError:
                created = False
            if fresh and not created:
                continue
            try:
                with resources.open_child(parent, name) as root:
                    expected = resources.identity(os.fstat(root))
                    if created:
                        info = {resources.LABEL: owner, resources.ATTEMPT: attempt,
                                'created': time.time(), 'identity': expected}
                        descriptor = os.open('.owner.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                             0o600, dir_fd=root)
                        with os.fdopen(descriptor, 'w') as stream:
                            json.dump(info, stream)
                            stream.flush()
                            os.fsync(stream.fileno())
                    else:
                        info = resources.marker(root)
                        if (info[resources.LABEL] != owner or info[resources.ATTEMPT] != attempt or
                                tuple(info['identity']) != expected):
                            raise ValueError('ownership/device/inode mismatch')
                break
            except (OSError, ValueError, KeyError, TypeError):
                if created:
                    # Registration failure is not an ownership collision. Keep
                    # the incomplete root and fail before any workload writes.
                    raise
                print('::warning::Kept existing root: ownership/device/inode unavailable or changed; using fresh root.',
                      file=sys.stderr)
                fresh = True
        chosen = os.path.join(base, name)
        updates = {'CI_RUNS_ROOT': base}
        for key in ['CARGO_TARGET_DIR', 'CARGO_INSTALL_ROOT', 'DOCKER_CONFIG']:
            value = os.environ.get(key)
            if value:
                if not value.startswith(original + '/'):
                    raise ValueError('configured path is outside run root')
                updates[key] = chosen + value[len(original):]
        updates['TMPDIR'] = chosen + '/tmp'
        # Reject pre-existing symlink descendants before installing any wrapper.
        for path in [chosen + '/bin', *[updates[k] for k in ['TMPDIR', 'DOCKER_CONFIG'] if k in updates]]:
            fd = mkdirs(path)
            os.close(fd)
        with open(os.environ['GITHUB_ENV'], 'a') as stream:
            for key, value in updates.items():
                stream.write(f'{key}={value}\n')
        return chosen
    finally:
        os.close(parent)


if __name__ == '__main__':
    print(configure())
