#!/usr/bin/env python3
"""Validation-only Docker CLI: label containers and remove only inspected identities.

The real CLI's help supplies option arity, so kind's Docker invocations are covered
as well as shell helpers. Unknown option syntax fails before creating resources.
"""
import json
import os
import re
import shutil
import subprocess
import sys

REAL = os.environ.get('CI_DOCKER_REAL') or shutil.which('docker')
OWNER = os.environ['CI_RESOURCE_SCOPE']
LABEL = f'fortemi.ci.scope={OWNER}'
ATTEMPT = os.environ['CI_RESOURCE_ATTEMPT']
LABELS = ['--label', LABEL, '--label', f'fortemi.ci.attempt={ATTEMPT}']
# Deprecated aliases Docker accepts but hides from `run/create --help`; kind
# still passes --net. Each is checked as the documented flag it maps to.
HIDDEN_ALIASES = {'--net': '--network', '--net-alias': '--network-alias'}


def capture(*args):
    if not REAL:
        raise ValueError('Docker CLI unavailable')
    return subprocess.check_output([REAL, *args], text=True)


def container_args(command, args):
    help_text = capture(command, '--help')
    valued, known = set(), set()
    for line in help_text.splitlines():
        match = re.match(r'^\s+(?:(-\w),?\s+)?(--[\w-]+)(?: (\S+))?', line)
        if match:
            short, long, typ = match.groups()
            for flag in filter(None, [short, long]):
                known.add(flag)
                if typ:
                    valued.add(flag)
    automatic_removal = set()
    i = 0
    while i < len(args) and args[i].startswith('-'):
        arg = args[i]
        if arg == '--':
            i += 1
            break
        key = arg.split('=', 1)[0]
        if HIDDEN_ALIASES.get(key) in known:
            key = HIDDEN_ALIASES[key]
        if key == '--rm':
            automatic_removal.add(i)
        value = arg.split('=', 1)[1] if '=' in arg else None
        if key not in known:
            # Docker/Kind uses combined booleans such as -d and -it.
            if arg.startswith('--') or not all('-' + c in known - valued for c in arg[1:]):
                raise ValueError('unsupported Docker option syntax')
        if key in valued and value is None:
            i += 1
            value = args[i]
        if key in {'-v', '--volume'} and value:
            bits = value.split(':')
            # Destination-only mounts stay anonymous (including kind's /var).
            if len(bits) > 1 and not bits[0].startswith('/'):
                raise ValueError('named volume mount is forbidden')
        if key == '--mount' and value:
            fields = dict(x.split('=', 1) for x in value.split(',') if '=' in x)
            if fields.get('type', 'volume') == 'volume' and fields.get('source', fields.get('src')):
                raise ValueError('named volume mount is forbidden')
        i += 1
    image = args[i]
    try:
        info = json.loads(capture('image', 'inspect', image))[0]
    except subprocess.CalledProcessError:
        subprocess.check_call([REAL, 'pull', image])
        info = json.loads(capture('image', 'inspect', image))[0]
    labels = (info.get('Config') or {}).get('Labels') or {}
    if labels.get('fortemi.ci.scope') == OWNER and labels.get('org.opencontainers.image.revision') != os.environ['CHAIN_SOURCE_SHA']:
        raise ValueError('owned image does not match this run source')
    # Image-declared VOLUMEs remain anonymous too. Docker owns their association
    # with this labelled container; rm -v <ID> is their only deletion path.
    # Keep exited containers for explicit ID-checked rm -v, including foreground
    # docker run --rm callers. The always() final reconciler owns their teardown.
    return [arg for index, arg in enumerate(args[:i]) if index not in automatic_removal] + LABELS + args[i:]


def remove(kind, args):
    targets = []
    for arg in args:
        if arg == '--':
            continue
        if arg.startswith('-'):
            if arg not in {'--force', '--volumes'} and not (arg.startswith('-') and
                    not arg.startswith('--') and all(c in 'fv' for c in arg[1:])):
                raise ValueError('unsupported removal option')
        else:
            targets.append(arg)
    for target in targets:
        if kind == 'volume':
            print(f'::warning::Kept named volume {target}: never delete by name.')
            continue
        try:
            info = json.loads(capture(kind, 'inspect', target))[0]
            identity = info['Id']
            # Resolve aliases, then check ownership on the very ID being removed.
            info = json.loads(capture(kind, 'inspect', identity))[0]
            labels = ((info.get('Config') or {}).get('Labels') if kind in {'container', 'image'}
                      else info.get('Labels')) or {}
            if (info['Id'] != identity or labels.get('fortemi.ci.scope') != OWNER or
                    labels.get('fortemi.ci.attempt') != ATTEMPT):
                raise ValueError('ownership changed')
        except (OSError, subprocess.CalledProcessError, ValueError, KeyError, IndexError, TypeError):
            print(f'::warning::Kept {kind} {target}: ownership/identity unavailable or changed.')
            continue
        command = {'container': ['rm', '-f', '-v'], 'network': ['network', 'rm'],
                   'image': ['image', 'rm', '-f']}[kind]
        subprocess.check_call([REAL, *command, identity])


def main(args):
    if not all(re.fullmatch(r'[1-9][0-9]*', value) for value in [OWNER, ATTEMPT]):
        raise ValueError('invalid resource owner')
    if args[0] in {'rm', 'rmi'}:
        return remove('container' if args[0] == 'rm' else 'image', args[1:])
    if len(args) > 1 and args[0] in {'container', 'network', 'image', 'volume'} and args[1] in {'rm', 'remove'}:
        return remove(args[0], args[2:])
    if 'prune' in args or args[:2] == ['volume', 'create']:
        raise ValueError('prune and named volume creation are forbidden')
    if args[0] in {'run', 'create'}:
        args = [args[0]] + container_args(args[0], args[1:])
    elif args[:2] == ['network', 'create']:
        args = args[:2] + LABELS + args[2:]
    elif args[0] == 'build':
        args = ['build', *LABELS, '--label', 'org.opencontainers.image.revision=' + os.environ['CHAIN_SOURCE_SHA']] + args[1:]
    elif args[:2] == ['buildx', 'rm'] and not {'--keep-daemon', '--keep-state'} <= set(args):
        raise ValueError('buildx removal must retain daemon and state for ID cleanup')
    elif args[:2] == ['buildx', 'build']:
        args = args[:2] + [*LABELS, '--label', 'org.opencontainers.image.revision=' + os.environ['CHAIN_SOURCE_SHA']] + args[2:]
    os.execv(REAL, [REAL, *args])


if __name__ == '__main__':
    try:
        main(sys.argv[1:])
    except (ValueError, IndexError):
        sys.exit('Docker ownership setup refused unsupported resource creation')
