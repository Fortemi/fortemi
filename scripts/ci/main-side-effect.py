#!/usr/bin/env python3
"""Bounded daemon mutex with definitive, attempt-specific holder recovery."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import time

spec = importlib.util.spec_from_file_location('resources', Path(__file__).with_name('reconcile-run-resources.py'))
resources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resources)
LOCK = 'fortemi-main-side-effects'
JOB = 'fortemi.ci.job'
IMAGE = 'node:20@sha256:8f693eaa7e0a8e71560c9a82b55fd54c2ae920a2ba5d2cde28bac7d1c01c9ba5'


def job_identity(value, run, attempt, job_id=None):
    if (str(value.get('run_id')) != run or str(value.get('run_attempt')) != attempt or
            not resources.numeric(str(value.get('id'))) or
            (job_id is not None and str(value['id']) != job_id)):
        raise ValueError('unusable holder job')
    return resources.status(value)


def current_job(run, attempt):
    # Display name is explicitly supplied by each non-matrix caller. Paginate
    # until exhaustion; ambiguity, omissions and malformed data fail closed.
    matches, seen = [], set()
    total = None
    for page in range(1, 101):
        value, _ = resources.api(f'runs/{run}/attempts/{attempt}/jobs?limit=50&page={page}')
        jobs, count = value['jobs'], value['total_count']
        if (not isinstance(jobs, list) or type(count) is not int or count < 0 or
                (total is not None and total != count)):
            raise ValueError('unusable jobs list')
        total = count
        for job in jobs:
            job_identity(job, run, attempt)
            if job['id'] in seen:
                raise ValueError('duplicate job')
            seen.add(job['id'])
            if job.get('name') == os.environ['CI_MUTEX_JOB_NAME']:
                if resources.status(job) != 'in_progress':
                    raise ValueError('caller job is not running')
                matches.append(str(job['id']))
        if len(seen) == total:
            if len(matches) != 1:
                raise ValueError('caller job is ambiguous or missing')
            return matches[0]
        if not jobs or len(seen) > total:
            raise ValueError('incomplete jobs list')
    raise ValueError('jobs pagination incomplete')


def recover():
    """Recover only an old, definitively terminal holder JOB, not its whole run."""
    try:
        info = resources.inspect('container', LOCK)
        labels = resources.labels(info, 'container')
        owner, attempt, job_id = (labels.get(key) for key in [resources.LABEL, resources.ATTEMPT, JOB])
        if not all(resources.numeric(v) for v in [owner, attempt, job_id]):
            return
        value, server_now = resources.api(f'jobs/{job_id}')
        if (job_identity(value, owner, attempt, job_id) not in resources.TERMINAL or
                not resources.old(resources.timestamp(info['Created']), server_now)):
            return
        # Job IDs belong to one attempt in v1.27.2. Reinspect the immutable
        # container ID and recheck the holder directly before removal.
        if resources.inspect('container', info['Id']) != info:
            return
        value, _ = resources.api(f'jobs/{job_id}')
        if job_identity(value, owner, attempt, job_id) in resources.TERMINAL:
            resources.docker('rm', '-v', info['Id'])
    except Exception:
        print('::warning::Mutex holder lookup unavailable; retaining lock.', flush=True)


def main(args):
    run, attempt = os.environ['GITHUB_RUN_ID'], os.environ['CI_RESOURCE_ATTEMPT']
    if not all(resources.numeric(v) for v in [run, attempt]):
        raise ValueError('invalid caller identity')
    held = None
    deadline = time.monotonic() + 2160
    resources.deadline = deadline

    def bounded(command, **kwargs):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('mutex deadline')
        return subprocess.run(command, timeout=min(remaining, 20), **kwargs)

    def fresh():
        result = bounded(['bash', 'scripts/ci/main-tip-guard.sh'])
        if result.returncode not in {0, 10}:
            raise ValueError('unknown freshness')
        return result.returncode == 0

    try:
        if not fresh():
            return 0
        job_id = current_job(run, attempt)
        if bounded(['docker', 'image', 'inspect', IMAGE], stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL).returncode:
            bounded(['docker', 'pull', IMAGE], check=True, stdout=subprocess.DEVNULL)
        while time.monotonic() < deadline:
            created = bounded(['docker', 'create', '--label', f'{resources.LABEL}={run}',
                               '--label', f'{resources.ATTEMPT}={attempt}', '--label', f'{JOB}={job_id}',
                               '--name', LOCK, '--network', 'none', '--entrypoint', '/bin/true', IMAGE],
                              text=True, capture_output=True)
            if created.returncode == 0:
                held = created.stdout.strip()
                if not held or any(c not in '0123456789abcdef' for c in held):
                    raise ValueError('invalid lock container ID')
                break
            if not fresh():
                return 0
            recover()
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('mutex deadline')
            time.sleep(min(5, remaining))
        if held is None or time.monotonic() >= deadline:
            raise TimeoutError('mutex deadline')
        if not fresh():
            return 0
        # Freshness is deliberately checked after obtaining the shared fence.
        if args[0] == 'sync':
            result = subprocess.run(['git', 'push', 'github', 'HEAD:refs/heads/main'],
                                    env=dict(os.environ, GIT_TERMINAL_PROMPT='0'))
            if result.returncode and not fresh():
                return 0
            return result.returncode
        if args[0] == 'dispatch':
            return subprocess.run([sys.executable, 'scripts/ci/dispatch-main.py', *args[1:]]).returncode
        raise ValueError('invalid side effect')
    finally:
        # The wait deadline must not suppress best-effort release of our own ID.
        del resources.deadline
        if held:
            # Never release by reusable name, even if another waiter recovered it.
            try:
                info = resources.inspect('container', held)
                labels = resources.labels(info, 'container')
                if (info['Id'] != held or labels.get(resources.LABEL) != run or
                        labels.get(resources.ATTEMPT) != attempt):
                    raise ValueError('own mutex identity/labels changed')
                resources.docker('rm', '-v', held)
            except Exception:
                print('::warning::Could not release own mutex container.', flush=True)


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except Exception:
        print('::error::Main side effect refused: freshness/holder unknown or monotonic mutex deadline exceeded.',
              file=sys.stderr)
        sys.exit(1)
