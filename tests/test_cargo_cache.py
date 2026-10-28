"""Policy/helper fixtures; no real compiler, builds, Docker or network."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HELPER = Path(__file__).resolve().parents[1] / 'scripts/ci/cargo-cache.py'
spec = importlib.util.spec_from_file_location('cargo_cache', HELPER)
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


class CacheTests(unittest.TestCase):
    def git_fixture(self, parent):
        """Real Git metadata, no network/compiler or changes outside the fixture."""
        repo, linked = parent / 'repo', parent / 'linked'
        repo.mkdir()
        (repo / 'Cargo.toml').write_text('[workspace]\n')
        (repo / 'Cargo.lock').write_text('lock1\n')
        env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
        def git(*args):
            return subprocess.run(['git', '-C', str(repo), '-c', 'core.hooksPath=/dev/null',
                                   '-c', 'commit.gpgsign=false', '-c', 'user.name=Cache Fixture',
                                   '-c', 'user.email=fixture@example.invalid', *args],
                                  env=env, capture_output=True, text=True, check=True, timeout=10)
        git('init', '-q')
        git('add', 'Cargo.toml', 'Cargo.lock')
        git('commit', '-qm', 'cache fixture')
        git('worktree', 'add', '--detach', str(linked), 'HEAD')
        return repo, linked

    def test_linked_worktrees_share_identity_but_unrelated_repositories_do_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            repo, linked = self.git_fixture(parent)
            key = cache.identity(repo, 'rust1 host-a packages-a', {}, 'build-default')
            self.assertNotEqual(repo, linked)
            self.assertEqual(cache.repository_identity(repo), cache.repository_identity(linked))
            self.assertEqual(key, cache.identity(linked, 'rust1 host-a packages-a', {}, 'build-default'))
            # Matching manifests and even a matching remote are insufficient:
            # independent repositories must never share a target namespace.
            other_parent = parent / 'unrelated'
            other_parent.mkdir()
            other, _ = self.git_fixture(other_parent)
            for tree in [repo, other]:
                subprocess.run(['git', '-C', str(tree), 'remote', 'add', 'origin',
                                'https://example.invalid/same/repo.git'], check=True, timeout=10)
            self.assertNotEqual(key, cache.identity(other, 'rust1 host-a packages-a', {}, 'build-default'))
            with patch.dict(os.environ, {'GIT_DIR': str(other / '.git'), 'GIT_WORK_TREE': str(other)}):
                self.assertEqual(key, cache.identity(linked, 'rust1 host-a packages-a', {}, 'build-default'))
            child = repo / 'subdirectory'
            child.mkdir()
            self.assertEqual(cache.repository_identity(child), ['directory', str(child)])
            env = dict(self.selection_env(repo), FORTEMI_CARGO_CACHE_ROOT=str(parent / 'cache'))
            with patch.object(cache.subprocess, 'check_output', return_value='compiler'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                selected = cache.setup(env)
                linked_selection = cache.setup(dict(env, GITHUB_WORKSPACE=str(linked)))
                release = cache.setup(dict(env, FORTEMI_CACHE_REF='refs/tags/v1'))
                pr = cache.setup(dict(env, FORTEMI_CACHE_EVENT='pull_request'))
            self.assertEqual(selected, linked_selection)
            self.assertNotEqual(selected['CARGO_TARGET_DIR'], release['CARGO_TARGET_DIR'])
            self.assertEqual(pr, {})
            for compiler, env, variant in [
                    ('rust2 host-a packages-a', {}, 'build-default'),
                    ('rust1 host-b packages-a', {}, 'build-default'),
                    ('rust1 host-a packages-b', {}, 'build-default'),
                    ('rust1 host-a packages-a', {'CC': 'other-cc'}, 'build-default'),
                    ('rust1 host-a packages-a', {'CARGO_PROFILE_RELEASE_LTO': 'fat'}, 'build-default'),
                    ('rust1 host-a packages-a', {}, 'build-features-hosted-auth')]:
                self.assertNotEqual(key, cache.identity(linked, compiler, env, variant))
            (linked / 'Cargo.lock').write_text('lock2\n')
            self.assertNotEqual(key, cache.identity(linked, 'rust1 host-a packages-a', {}, 'build-default'))
            (linked / 'Cargo.lock').write_text('lock1\n')
            (linked / 'Cargo.toml').write_text('[workspace]\nresolver="2"\n')
            self.assertNotEqual(key, cache.identity(linked, 'rust1 host-a packages-a', {}, 'build-default'))

    def test_repository_probe_failure_and_non_git_trees_keep_directory_isolation(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / 'a', Path(tmp) / 'b'
            a.mkdir()
            b.mkdir()
            for failure in [FileNotFoundError(), subprocess.TimeoutExpired('git', 5)]:
                with patch.object(cache.subprocess, 'run', side_effect=failure):
                    self.assertEqual(cache.repository_identity(a), ['directory', str(a)])
                    self.assertNotEqual(cache.identity(a, 'rust', {}, 'dev'), cache.identity(b, 'rust', {}, 'dev'))
            self.assertNotEqual(cache.identity(a, 'rust', {}, 'dev'), cache.identity(b, 'rust', {}, 'dev'))

    def test_trust_never_restores_for_pr_or_unknown_ci(self):
        base = {'FORTEMI_CACHE_CI': '1', 'FORTEMI_CACHE_EVENT': 'pull_request', 'FORTEMI_CACHE_REF': 'refs/heads/main'}
        self.assertIsNone(cache.trust(base))
        self.assertIsNone(cache.trust({'CI': 'true'}))
        self.assertEqual(cache.trust(dict(base, FORTEMI_CACHE_EVENT='push')), 'main')
        self.assertEqual(cache.trust(dict(base, FORTEMI_CACHE_EVENT='push', FORTEMI_CACHE_REF='refs/tags/v1')), 'release')
        self.assertIsNone(cache.trust(dict(base, FORTEMI_CACHE_EVENT='workflow_dispatch', FORTEMI_CACHE_REF='refs/heads/feature')))

    def test_inputs_invalidate_but_source_edits_preserve_dependencies(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'Cargo.lock').write_text('lock1')
            (root / 'Cargo.toml').write_text('[workspace]')
            (root / 'source.rs').write_text('before')
            key = cache.identity(root, 'rust1 host-a', {}, 'dev-default')
            (root / 'source.rs').write_text('after')
            self.assertEqual(key, cache.identity(root, 'rust1 host-a', {}, 'dev-default'))
            for compiler, env, variant in [('rust2 host-a', {}, 'dev-default'), ('rust1 host-b', {}, 'dev-default'),
                                          ('rust1 host-a', {'RUSTFLAGS': '-Cinstrument-coverage'}, 'dev-default'),
                                          ('rust1 host-a', {}, 'release-hosted-auth,kms-vault')]:
                self.assertNotEqual(key, cache.identity(root, compiler, env, variant))
            (root / 'Cargo.lock').write_text('lock2')
            self.assertNotEqual(key, cache.identity(root, 'rust1 host-a', {}, 'dev-default'))

    def test_symlink_cache_refused_without_touching_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'real').mkdir()
            (root / 'link').symlink_to(root / 'real', target_is_directory=True)
            with self.assertRaises(OSError):
                cache.private_dir(root / 'link' / 'child')
            self.assertFalse((root / 'real' / 'child').exists())

    def test_cold_disabled_and_pr_do_not_create_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = {'GITHUB_WORKSPACE': tmp, 'FORTEMI_CACHE_CI': '1', 'FORTEMI_CACHE_EVENT': 'pull_request'}
            self.assertEqual(cache.setup(env), {})
            self.assertEqual(cache.setup(dict(env, FORTEMI_CACHE_DISABLE='1')), {})
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_bounded_retention_removes_only_marked_entries(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            unrelated = root / 'unrelated'
            unrelated.write_text('keep')
            for number in range(3):
                entry = cache.private_dir(root / f'{number:064x}')
                info = entry.stat()
                (entry / '.owner.json').write_text(json.dumps([cache.SCHEMA, os.geteuid(), info.st_dev, info.st_ino]))
                (entry / 'artifact').write_bytes(b'x' * 1024)
            with cache.locked(root) as guard, patch.object(cache, 'MAX_ENTRIES', 1):
                result = cache.prune(root, guard=guard)
            self.assertEqual(result['retained_entries'], 1)
            self.assertEqual(unrelated.read_text(), 'keep')

    def owned_entry(self, root, number=1, size=1024):
        entry = cache.private_dir(root / f'{number:064x}')
        info = entry.stat()
        (entry / '.owner.json').write_text(json.dumps([cache.SCHEMA, os.geteuid(), info.st_dev, info.st_ino]))
        (entry / 'nested').mkdir()
        (entry / 'nested/artifact').write_bytes(b'x' * size)
        return entry

    def test_quarantine_rename_precedes_every_delete_and_symlink_target_survives(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            outside = Path(tmp) / 'outside'
            outside.mkdir()
            (outside / 'sentinel').write_text('untouched')
            (entry / 'nested/link').symlink_to(outside, target_is_directory=True)
            expected = cache.node_identity(entry.stat())
            rename, unlink, rmdir = cache.rename_noreplace, os.unlink, os.rmdir
            events = []

            def claim(*args):
                self.assertTrue(entry.is_dir())
                rename(*args)
                events.append('rename')
                self.assertFalse(entry.exists())
                self.assertEqual(cache.node_identity((root / cache.QUARANTINE / 'tree').stat()), expected)

            def delete(operation, name, *args, **kwargs):
                self.assertEqual(events[0], 'rename')
                self.assertFalse(entry.exists())
                self.assertIn('dir_fd', kwargs)
                events.append(str(name))
                return operation(name, *args, **kwargs)

            with cache.locked(root) as guard, patch.object(cache, 'rename_noreplace', claim), patch.object(cache.os, 'unlink', lambda *a, **k: delete(unlink, *a, **k)), patch.object(cache.os, 'rmdir', lambda *a, **k: delete(rmdir, *a, **k)):
                cache.quarantine_evict(root, guard, entry.name, expected)
            self.assertGreater(len(events), 3)
            self.assertEqual((outside / 'sentinel').read_text(), 'untouched')
            self.assertEqual(list((root / cache.QUARANTINE).iterdir()), [])

    def test_quarantine_rejects_unproven_source_without_rename(self):
        for failure in ['entry-symlink', 'marker-symlink', 'corrupt-marker', 'missing-marker',
                        'changed-inode', 'foreign-uid', 'foreign-device']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as tmp:
                root = cache.private_dir(Path(tmp) / 'cache')
                entry = self.owned_entry(root)
                expected = cache.node_identity(entry.stat())
                marker = entry / '.owner.json'
                saved = root / 'saved'
                if failure == 'entry-symlink':
                    entry.rename(saved)
                    entry.symlink_to(saved, target_is_directory=True)
                elif failure == 'marker-symlink':
                    marker.rename(entry / 'saved-marker')
                    marker.symlink_to(entry / 'saved-marker')
                elif failure == 'missing-marker':
                    marker.unlink()
                elif failure == 'corrupt-marker':
                    marker.write_text('{interrupted')
                elif failure == 'changed-inode':
                    entry.rename(saved)
                    entry.mkdir(mode=0o700)
                    (entry / '.owner.json').write_text((saved / '.owner.json').read_text())
                    (entry / 'sentinel').write_text('untouched')
                original = os.fstat

                def changed(fd):
                    info = original(fd)
                    if cache.node_identity(info) == expected and failure in {'foreign-uid', 'foreign-device'}:
                        fields = list(info)
                        fields[4 if failure == 'foreign-uid' else 2] += 1
                        return os.stat_result(fields)
                    return info

                with cache.locked(root) as guard, patch.object(cache.os, 'fstat', changed), patch.object(cache, 'rename_noreplace') as rename:
                    with self.assertRaises((OSError, ValueError)):
                        cache.quarantine_evict(root, guard, entry.name, expected)
                    rename.assert_not_called()
                original_entry = saved if saved.exists() else entry
                self.assertEqual((original_entry / 'nested/artifact').read_bytes(), b'x' * 1024)
                if failure == 'changed-inode':
                    self.assertEqual((entry / 'sentinel').read_text(), 'untouched')

    def test_foreign_descendant_is_preserved_before_and_after_claim(self):
        for after_claim in [False, True]:
            with self.subTest(after_claim=after_claim), tempfile.TemporaryDirectory() as tmp:
                root = cache.private_dir(Path(tmp) / 'cache')
                entry = self.owned_entry(root)
                expected = cache.node_identity(entry.stat())
                original_stat, original_rename = os.stat, cache.rename_noreplace
                claimed = False

                def rename(*args):
                    nonlocal claimed
                    original_rename(*args)
                    claimed = True

                def foreign(name, *args, **kwargs):
                    info = original_stat(name, *args, **kwargs)
                    if name == 'artifact' and (claimed or not after_claim):
                        fields = list(info)
                        fields[4] += 1
                        return os.stat_result(fields)
                    return info

                with cache.locked(root) as guard, patch.object(cache.os, 'stat', foreign), patch.object(cache, 'rename_noreplace', rename), patch.object(cache.os, 'unlink') as unlink:
                    with self.assertRaises(ValueError):
                        cache.quarantine_evict(root, guard, entry.name, expected)
                    unlink.assert_not_called()
                retained = root / cache.QUARANTINE / 'tree' if claimed else entry
                self.assertEqual((retained / 'nested/artifact').read_bytes(), b'x' * 1024)
                self.assertEqual(claimed, after_claim)

    def test_source_swap_during_rename_preserves_both_trees(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            expected = cache.node_identity(entry.stat())
            saved = root / 'saved'
            original = cache.rename_noreplace

            def swapped(source_fd, name, pending, destination):
                entry.rename(saved)
                entry.mkdir(mode=0o700)
                (entry / '.owner.json').write_text((saved / '.owner.json').read_text())
                (entry / 'sentinel').write_text('replacement')
                original(source_fd, name, pending, destination)

            with cache.locked(root) as guard, patch.object(cache, 'rename_noreplace', swapped), patch.object(cache.os, 'unlink') as unlink:
                with self.assertRaises(ValueError):
                    cache.quarantine_evict(root, guard, entry.name, expected)
                unlink.assert_not_called()
            self.assertEqual((saved / 'nested/artifact').read_bytes(), b'x' * 1024)
            self.assertEqual((root / cache.QUARANTINE / 'tree/sentinel').read_text(), 'replacement')

    def test_quarantine_marker_change_after_rename_preserves_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            expected = cache.node_identity(entry.stat())
            original = cache.rename_noreplace

            def claim(*args):
                original(*args)
                (root / cache.QUARANTINE / 'tree/.owner.json').write_text('[]')

            with cache.locked(root) as guard, patch.object(cache, 'rename_noreplace', claim), patch.object(cache.os, 'unlink') as unlink:
                with self.assertRaises(ValueError):
                    cache.quarantine_evict(root, guard, entry.name, expected)
                unlink.assert_not_called()
            self.assertEqual((root / cache.QUARANTINE / 'tree/nested/artifact').read_bytes(), b'x' * 1024)

    def test_partial_delete_blocks_repeated_setup_and_invoke_and_preserves_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            root = cache.private_dir(workspace / 'cache/main')
            entry = self.owned_entry(root)
            (entry / 'second').write_text('retained')
            original = os.unlink
            deleted = []

            def partial(name, *args, **kwargs):
                if deleted:
                    raise OSError('injected partial failure')
                original(name, *args, **kwargs)
                deleted.append(name)

            output = io.StringIO()
            with patch.object(cache, 'MAX_ENTRIES', 0), patch.object(cache.os, 'unlink', partial), contextlib.redirect_stderr(output):
                self.assertEqual(cache.finalize({'FORTEMI_CARGO_CACHE_ENTRY': str(entry)}), 0)
            self.assertEqual(len(deleted), 1)
            self.assertIn('::warning::', output.getvalue())
            pending = root / cache.QUARANTINE
            before = sorted(str(p.relative_to(pending)) for p in pending.rglob('*'))
            active = self.owned_entry(root, 2)
            for _ in range(3):
                with patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                    selected = cache.setup(self.selection_env(workspace))
                self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
                self.assertFalse(Path(selected['CARGO_TARGET_DIR']).is_relative_to(root))
                self.assertEqual(sorted(str(p.relative_to(pending)) for p in pending.rglob('*')), before)
                self.assertEqual(len(list(root.glob('[0-9a-f]' * 64))), 1)
            # Existing entries cannot acquire new writes while quarantine remains.
            fake = workspace / 'cargo-fixture'
            fake.write_text('#!/usr/bin/env python3\nimport os,sys\nprint(os.environ["CARGO_TARGET_DIR"])\nsys.exit(int(os.environ["FIXTURE_STATUS"]))\n')
            fake.chmod(0o700)
            for status in [0, 101]:
                env = dict(os.environ, FORTEMI_REAL_CARGO=str(fake), TMPDIR=tmp,
                           FORTEMI_CACHE_LOCK_HELD='', FORTEMI_CARGO_CACHE_ENTRY=str(active),
                           CARGO_TARGET_DIR=str(active / 'target'), FIXTURE_STATUS=str(status))
                result = subprocess.run(['python3', str(HELPER), '--', 'test'], env=env,
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, status, result.stderr)
                self.assertIn('::warning::', result.stderr)
                self.assertFalse(Path(result.stdout.strip()).is_relative_to(root))
            self.assertEqual((active / 'nested/artifact').read_bytes(), b'x' * 1024)

    def test_retention_reserves_new_slot_protects_keep_and_enforces_byte_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            old = self.owned_entry(root, 1)
            keep = self.owned_entry(root, 2)
            os.utime(old, (1, 1))
            os.utime(keep, (2, 2))
            with cache.locked(root) as guard, patch.object(cache, 'MAX_ENTRIES', 2):
                result = cache.prune(root, keep=keep, reserve=1, guard=guard)
            self.assertEqual(result['retained_entries'], 1)
            self.assertFalse(old.exists())
            self.assertTrue(keep.exists())
            with cache.locked(root) as guard, patch.object(cache, 'MAX_BYTES', 1):
                with self.assertRaises(ValueError):
                    cache.prune(root, keep=keep, guard=guard)
            self.assertTrue(keep.exists())
            with cache.locked(root) as guard, patch.object(cache, 'MAX_BYTES', 1):
                result = cache.prune(root, guard=guard)
            self.assertEqual(result['retained_entries'], 0)
            self.assertEqual(result['retained_bytes'], 0)

    def test_retention_pressure_totals_and_age_ordering(self):
        # Charge marker bytes too; no synthetic inventory or mocked deletion.
        for reason, sizes, cap_count, byte_extra, survivors in [
                ('none', [100, 100], 6, 1000, [1, 2]),
                ('count', [100, 100, 100], 2, 1000, [2, 3]),
                ('bytes', [100, 100, 100], 6, -1, [2, 3]),
                ('both', [100, 100, 100], 2, -1, [2, 3])]:
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as tmp:
                root = cache.private_dir(Path(tmp) / 'cache')
                paths = [self.owned_entry(root, i, size) for i, size in enumerate(sizes, 1)]
                for i, path in enumerate(paths, 1):
                    os.utime(path, (i, i))
                weights = [(p / '.owner.json').stat().st_size + size for p, size in zip(paths, sizes)]
                total = sum(weights)
                output = io.StringIO()
                with cache.locked(root) as guard, patch.object(cache, 'MAX_ENTRIES', cap_count), patch.object(cache, 'MAX_BYTES', total + byte_extra), contextlib.redirect_stderr(output):
                    result = cache.prune(root, keep=paths[-1], guard=guard)
                self.assertEqual(result['reason'], reason)
                self.assertEqual(result['before_entries'], len(paths))
                self.assertEqual(result['before_bytes'], total)
                self.assertEqual(result['largest_entry_bytes'], max(weights))
                self.assertEqual(result['max_bytes'], total + byte_extra)
                self.assertEqual(result['max_entries'], cap_count)
                self.assertEqual([i for i, p in enumerate(paths, 1) if p.exists()], survivors)
                self.assertEqual(result['retained_entries'], len(survivors))
                self.assertEqual(result['retained_bytes'], sum(weights[i-1] for i in survivors))
                self.assertEqual(result['evicted_entries'], len(paths) - len(survivors))
                self.assertEqual(result['evicted_bytes'], total - result['retained_bytes'])
                self.assertTrue(result['selected_retained'])
                self.assertEqual(result['oversized_entries_before'], 0)
                self.assertEqual(result['oversized_entries_after'], 0)
                records = [json.loads(line) for line in output.getvalue().splitlines()]
                self.assertEqual([r['cache_retention'] for r in records], ['before', 'complete'])
                self.assertNotIn(tmp, output.getvalue())

    def test_main_budget_isolation_setup_and_finalizer_hard_ceiling(self):
        # Every context gets a real selection/finalize with tiny explicit caps.
        # No budget environment variable can admit a PR or enlarge local/release.
        for lane, ci, ref, expected_cap in [
                ('main', True, 'refs/heads/main', 3000),
                ('release', True, 'refs/tags/v1', 1000),
                ('local', False, '', 1000)]:
            with self.subTest(lane=lane), tempfile.TemporaryDirectory() as tmp:
                workspace = Path(tmp)
                env = self.selection_env(workspace) if ci else {
                    'GITHUB_WORKSPACE': tmp, 'FORTEMI_CARGO_CACHE_ROOT': str(workspace / 'cache'), 'TMPDIR': tmp}
                if ci:
                    env['FORTEMI_CACHE_REF'] = ref
                env['FORTEMI_CACHE_MAX_BYTES'] = str(999999999)
                output = io.StringIO()
                with patch.object(cache, 'MAX_BYTES', 1000), patch.object(cache, 'CI_MAIN_MAX_BYTES', 3000), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(output):
                    selected = cache.setup(env)
                    entry = Path(selected['FORTEMI_CARGO_CACHE_ENTRY'])
                    (entry / 'artifact').write_bytes(b'x' * 1500)
                    self.assertEqual(cache.finalize(dict(env, **selected)), 0)
                    records = [json.loads(line) for line in output.getvalue().splitlines()]
                    final = [r for r in records if r.get('phase') == 'finalize' and r.get('cache_retention') == 'complete'][-1]
                    self.assertEqual(final['max_bytes'], expected_cap)
                    self.assertEqual(final['reason'], 'none' if lane == 'main' else 'bytes')
                    self.assertEqual(final['selected_retained'], lane == 'main')
                    self.assertEqual(final['selected_oversized'], lane != 'main')
                    self.assertEqual(entry.exists(), lane == 'main')
                    self.assertEqual(final['oversized_entries_after'], 0)
                    self.assertEqual(final['evicted_entries'], 0 if lane == 'main' else 1)
                    self.assertEqual(final['retained_entries'], 1 if lane == 'main' else 0)
                    if lane == 'main':
                        self.assertEqual(cache.setup(env), selected)
                        # Even main loses a single oversized generation: no soft overage.
                        (entry / 'artifact').write_bytes(b'x' * 3100)
                        self.assertEqual(cache.finalize(dict(env, **selected)), 0)
                        self.assertFalse(entry.exists())
                        records = [json.loads(line) for line in output.getvalue().splitlines()]
                        oversized = [r for r in records if r.get('phase') == 'finalize' and r.get('cache_retention') == 'complete'][-1]
                        self.assertTrue(oversized['selected_oversized'])
                        self.assertFalse(oversized['selected_retained'])
                        self.assertEqual(oversized['retained_bytes'], 0)
                        self.assertEqual(oversized['evicted_entries'], 1)
                    again = cache.setup(env)
                    self.assertTrue(Path(again['FORTEMI_CARGO_CACHE_ENTRY']).is_dir())
                    # Recreated markers alone are within the hard ceiling.
                    self.assertLessEqual(sum(w for _, _, w in cache.entries(entry.parent)), expected_cap)
                self.assertNotIn(tmp, output.getvalue())
        self.assertEqual(cache.MAX_BYTES, 24 * 1024**3)
        self.assertEqual(cache.CI_MAIN_MAX_BYTES, 64 * 1024**3)
        self.assertEqual(cache.MAX_ENTRIES, 6)
        self.assertEqual(cache.retention_bytes({'CI': 'true'}, 'main'), cache.MAX_BYTES)
        self.assertEqual(cache.retention_bytes({'FORTEMI_CACHE_CI': '1', 'FORTEMI_CACHE_EVENT': 'pull_request', 'FORTEMI_CACHE_REF': 'refs/heads/main'}, 'main'), cache.MAX_BYTES)

    def test_sibling_lanes_do_not_borrow_budget_or_evict_each_other(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            main_env = self.selection_env(workspace)
            release_env = dict(main_env, FORTEMI_CACHE_REF='refs/tags/v1')
            local_env = {'GITHUB_WORKSPACE': tmp, 'FORTEMI_CARGO_CACHE_ROOT': str(workspace / 'cache'), 'TMPDIR': tmp}
            with patch.object(cache, 'MAX_BYTES', 1000), patch.object(cache, 'CI_MAIN_MAX_BYTES', 3000), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                selections = [cache.setup(env) for env in [main_env, release_env, local_env]]
                paths = [Path(s['FORTEMI_CARGO_CACHE_ENTRY']) for s in selections]
                self.assertEqual([p.parent.name for p in paths], ['main', 'release', 'local'])
                for entry in paths:
                    (entry / 'artifact').write_bytes(b'x' * 1500)
                cache.finalize(dict(release_env, **selections[1]))
                self.assertTrue(paths[0].exists())
                self.assertFalse(paths[1].exists())
                self.assertTrue(paths[2].exists())
                cache.finalize(dict(local_env, **selections[2]))
                self.assertTrue(paths[0].exists())
                self.assertFalse(paths[2].exists())
                cache.finalize(dict(main_env, **selections[0]))
                self.assertTrue(paths[0].exists())
                self.assertEqual(cache.setup(dict(main_env, FORTEMI_CACHE_EVENT='pull_request')), {})
                self.assertEqual(cache.setup({'CI': 'true', 'GITHUB_WORKSPACE': tmp}), {})

    def test_pressure_preserves_unproven_entries_and_never_follows_links(self):
        for kind in ['missing-marker', 'entry-symlink', 'marker-symlink']:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp:
                root = cache.private_dir(Path(tmp) / 'cache')
                valid = self.owned_entry(root, 1)
                unsafe = self.owned_entry(root, 2)
                outside = Path(tmp) / 'outside'
                outside.mkdir()
                (outside / 'sentinel').write_text('untouched')
                if kind == 'entry-symlink':
                    unsafe.rename(root / 'saved')
                    unsafe.symlink_to(outside, target_is_directory=True)
                else:
                    (unsafe / '.owner.json').unlink()
                    if kind == 'marker-symlink':
                        (unsafe / '.owner.json').symlink_to(outside / 'sentinel')
                with cache.locked(root) as guard, patch.object(cache, 'MAX_BYTES', 1), patch.object(cache, 'quarantine_evict') as evict:
                    with self.assertRaises((OSError, ValueError)):
                        cache.prune(root, guard=guard)
                    evict.assert_not_called()
                self.assertTrue(valid.exists())
                self.assertTrue(unsafe.exists())
                self.assertEqual((outside / 'sentinel').read_text(), 'untouched')
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root, size=10)
            outside = Path(tmp) / 'outside'
            outside.mkdir()
            (outside / 'sentinel').write_bytes(b'y' * 10000)
            (entry / 'nested/link').symlink_to(outside, target_is_directory=True)
            charged = (entry / '.owner.json').stat().st_size + 10
            with cache.locked(root) as guard, patch.object(cache, 'MAX_BYTES', 1), contextlib.redirect_stderr(io.StringIO()):
                result = cache.prune(root, selected=entry, guard=guard)
            self.assertEqual(result['before_bytes'], charged)
            self.assertEqual(result['evicted_bytes'], charged)
            self.assertTrue(result['selected_oversized'])
            self.assertFalse(result['selected_retained'])
            self.assertEqual(result['retained_entries'], 0)
            self.assertEqual(result['retained_bytes'], 0)
            self.assertEqual((outside / 'sentinel').read_bytes(), b'y' * 10000)

    def test_partial_retention_failure_reports_unknown_post_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            output = io.StringIO()
            with patch.object(cache, 'MAX_BYTES', 1), patch.object(cache, 'quarantine_evict', side_effect=OSError('private-payload')), contextlib.redirect_stderr(output):
                self.assertEqual(cache.finalize({'FORTEMI_CARGO_CACHE_ENTRY': str(entry)}), 0)
            records = [json.loads(line) for line in output.getvalue().splitlines() if line.startswith('{')]
            self.assertEqual(records[-1]['cache_finalization'], 'unavailable')
            self.assertIsNone(records[-1]['selected_retained'])
            self.assertIsNone(records[-1]['retained_bytes'])
            self.assertFalse(any(r.get('cache_retention') == 'complete' for r in records))
            self.assertNotIn('private-payload', output.getvalue())
            self.assertNotIn(tmp, output.getvalue())
            self.assertTrue(entry.exists())

    def test_unsupported_atomic_claim_falls_back_without_deletion(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            root = cache.private_dir(workspace / 'cache/main')
            entry = self.owned_entry(root)
            with patch.object(cache, 'MAX_ENTRIES', 0), patch.object(cache, 'rename_noreplace', side_effect=OSError('unsupported')), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()) as output:
                selected = cache.setup(self.selection_env(workspace))
            self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
            self.assertIn('::warning::', output.getvalue())
            self.assertEqual((entry / 'nested/artifact').read_bytes(), b'x' * 1024)

    def test_root_lock_and_quarantine_swaps_stop_deletion(self):
        for changed in ['root', 'lock', 'quarantine']:
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as tmp:
                root = cache.private_dir(Path(tmp) / 'cache')
                entry = self.owned_entry(root)
                expected = cache.node_identity(entry.stat())
                original = cache.rename_noreplace
                saved = Path(tmp) / 'saved'

                def claim(*args):
                    original(*args)
                    if changed == 'root':
                        root.rename(saved)
                        root.mkdir(mode=0o700)
                        (root / 'sentinel').write_text('replacement')
                    elif changed == 'lock':
                        (root / '.lock').rename(root / 'saved-lock')
                        (root / '.lock').write_text('replacement')
                    else:
                        (root / cache.QUARANTINE).rename(saved)
                        (root / cache.QUARANTINE).mkdir(mode=0o700)
                        (root / cache.QUARANTINE / 'sentinel').write_text('replacement')

                with cache.locked(root) as guard, patch.object(cache, 'rename_noreplace', claim), patch.object(cache.os, 'unlink') as unlink:
                    with self.assertRaises(ValueError):
                        cache.quarantine_evict(root, guard, entry.name, expected)
                    unlink.assert_not_called()
                tree = (saved / cache.QUARANTINE / 'tree' if changed == 'root' else
                        saved / 'tree' if changed == 'quarantine' else root / cache.QUARANTINE / 'tree')
                self.assertEqual((tree / 'nested/artifact').read_bytes(), b'x' * 1024)

    def test_descendant_symlink_swap_during_clear_preserves_both_destinations(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            expected = cache.node_identity(entry.stat())
            outside = Path(tmp) / 'outside'
            outside.mkdir()
            (outside / 'sentinel').write_text('untouched')
            saved = Path(tmp) / 'saved-child'
            original = os.open
            opens = 0

            def swapped(name, *args, **kwargs):
                nonlocal opens
                if name == 'nested':
                    opens += 1
                    if opens == 3:  # preflight before/after rename, then clear
                        nested = root / cache.QUARANTINE / 'tree/nested'
                        nested.rename(saved)
                        nested.symlink_to(outside, target_is_directory=True)
                return original(name, *args, **kwargs)

            with cache.locked(root) as guard, patch.object(cache.os, 'open', swapped), patch.object(cache.os, 'unlink') as unlink:
                with self.assertRaises(OSError):
                    cache.quarantine_evict(root, guard, entry.name, expected)
                unlink.assert_not_called()
            self.assertEqual((saved / 'artifact').read_bytes(), b'x' * 1024)
            self.assertEqual((outside / 'sentinel').read_text(), 'untouched')

    def test_marker_unlink_then_tree_rmdir_failure_blocks_new_writes(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            root = cache.private_dir(workspace / 'cache/main')
            entry = self.owned_entry(root)
            original = os.rmdir

            def interrupted(name, *args, **kwargs):
                if name == 'tree':
                    raise OSError('injected final transition failure')
                return original(name, *args, **kwargs)

            with patch.object(cache, 'MAX_ENTRIES', 0), patch.object(cache.os, 'rmdir', interrupted), contextlib.redirect_stderr(io.StringIO()) as output:
                self.assertEqual(cache.finalize({'FORTEMI_CARGO_CACHE_ENTRY': str(entry)}), 0)
            self.assertIn('::warning::', output.getvalue())
            self.assertEqual(list((root / cache.QUARANTINE / 'tree').iterdir()), [])
            with patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                selected = cache.setup(self.selection_env(workspace))
            self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
            self.assertEqual(list((root / cache.QUARANTINE).iterdir()), [root / cache.QUARANTINE / 'tree'])

    def test_unsupported_platform_and_missing_rename_symbol_use_fresh_target(self):
        for platform in ['darwin', 'linux']:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as tmp:
                workspace = Path(tmp)
                # libc with no renameat2 symbol; never try a copying fallback.
                with patch.object(cache.sys, 'platform', platform), patch.object(cache.ctypes, 'CDLL', return_value=object()), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()) as output:
                    selected = cache.setup(self.selection_env(workspace))
                self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
                self.assertIn('::warning::', output.getvalue())
                self.assertEqual(list((workspace / 'cache/main').glob('[0-9a-f]' * 64)), [])

    def test_setup_reserves_entry_before_allowing_new_cache_writes(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            env = self.selection_env(workspace)
            with patch.object(cache, 'MAX_ENTRIES', 2), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                first = cache.setup(dict(env, FORTEMI_CACHE_VARIANT='first'))
                second = cache.setup(dict(env, FORTEMI_CACHE_VARIANT='second'))
                os.utime(first['FORTEMI_CARGO_CACHE_ENTRY'], (1, 1))
                os.utime(second['FORTEMI_CARGO_CACHE_ENTRY'], (2, 2))
                third = cache.setup(dict(env, FORTEMI_CACHE_VARIANT='third'))
            lane = workspace / 'cache/main'
            self.assertEqual(len(cache.entries(lane)), 2)
            self.assertFalse(Path(first['FORTEMI_CARGO_CACHE_ENTRY']).exists())
            self.assertTrue(Path(second['FORTEMI_CARGO_CACHE_ENTRY']).exists())
            self.assertTrue(Path(third['FORTEMI_CARGO_CACHE_ENTRY']).exists())

    def test_quarantine_destination_collision_never_replaces_unrelated_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = self.owned_entry(root)
            expected = cache.node_identity(entry.stat())
            original = cache.rename_noreplace

            def collision(source, name, pending, destination):
                os.mkdir('tree', 0o700, dir_fd=pending)
                (root / cache.QUARANTINE / 'tree/sentinel').write_text('unrelated')
                original(source, name, pending, destination)

            with cache.locked(root) as guard, patch.object(cache, 'rename_noreplace', collision), patch.object(cache.os, 'unlink') as unlink:
                with self.assertRaises(OSError):
                    cache.quarantine_evict(root, guard, entry.name, expected)
                unlink.assert_not_called()
            self.assertEqual((entry / 'nested/artifact').read_bytes(), b'x' * 1024)
            self.assertEqual((root / cache.QUARANTINE / 'tree/sentinel').read_text(), 'unrelated')

    def test_unproven_quarantine_directory_is_retained_and_blocks_setup(self):
        for kind in ['pending', 'symlink', 'foreign-uid', 'foreign-device']:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp:
                workspace = Path(tmp)
                root = cache.private_dir(workspace / 'cache/main')
                entry = self.owned_entry(root)
                outside = workspace / 'outside'
                outside.mkdir()
                (outside / 'sentinel').write_text('untouched')
                pending = root / cache.QUARANTINE
                if kind == 'symlink':
                    pending.symlink_to(outside, target_is_directory=True)
                    expected = None
                else:
                    pending.mkdir(mode=0o700)
                    (pending / 'sentinel').write_text('retained')
                    expected = cache.node_identity(pending.stat())
                original = os.fstat

                def changed(fd):
                    info = original(fd)
                    if expected == cache.node_identity(info) and kind in {'foreign-uid', 'foreign-device'}:
                        fields = list(info)
                        fields[4 if kind == 'foreign-uid' else 2] += 1
                        return os.stat_result(fields)
                    return info

                with patch.object(cache.os, 'fstat', changed), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()) as output:
                    selected = cache.setup(self.selection_env(workspace))
                self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
                self.assertIn('::warning::', output.getvalue())
                self.assertEqual((entry / 'nested/artifact').read_bytes(), b'x' * 1024)
                self.assertEqual((outside / 'sentinel').read_text(), 'untouched')
                if kind != 'symlink':
                    self.assertEqual((pending / 'sentinel').read_text(), 'retained')

    def test_offline_miss_failure_and_structured_telemetry_propagate(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / 'cargo-fixture'
            fake.write_text('#!/usr/bin/env python3\nimport sys\nprint(\'offline fixture: dependency absent\', file=sys.stderr)\nsys.exit(101)\n')
            fake.chmod(0o700)
            result = subprocess.run(['python3', str(HELPER), '--', 'build', '--locked', '--offline'],
                                    env=dict(os.environ, FORTEMI_REAL_CARGO=str(fake), FORTEMI_CARGO_CACHE_ENTRY='',
                                             FORTEMI_CACHE_LOCK_HELD=''), capture_output=True, text=True)
            self.assertEqual(result.returncode, 101)
            record = json.loads(result.stderr.splitlines()[-1])
            self.assertEqual(record['exit_code'], 101)
            self.assertIsNone(record['compiler_artifacts'])
            self.assertIsNone(record['build_finished'])
            self.assertNotIn('CARGO_HOME', record)

    def test_enabled_cache_reports_artifact_freshness_and_preserves_test_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lane = cache.private_dir(root / 'cache')
            entry = cache.private_dir(lane / ('e' * 64))
            info = entry.stat()
            (entry / '.owner.json').write_text(json.dumps([cache.SCHEMA, os.geteuid(), info.st_dev, info.st_ino]))
            fake = root / 'cargo-fixture'
            fake.write_text('#!/usr/bin/env python3\nimport json,sys\nassert "--message-format=json-render-diagnostics" in sys.argv\nfor fresh in [True,False]:\n print(json.dumps({"reason":"compiler-artifact","fresh":fresh}))\nprint(json.dumps({"reason":"build-finished","success":True}))\nprint("test fixture ... ok")\n')
            fake.chmod(0o700)
            result = subprocess.run(['python3', str(HELPER), '--', 'test'],
                                    env=dict(os.environ, FORTEMI_REAL_CARGO=str(fake),
                                             FORTEMI_CACHE_CI='1', FORTEMI_CACHE_LOCK_HELD='',
                                             FORTEMI_CARGO_CACHE_ENTRY=str(entry)),
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), 'test fixture ... ok')
            record = json.loads(result.stderr.splitlines()[-1])
            self.assertTrue(record['cache_enabled'])
            self.assertEqual(record['compiler_artifacts'], 2)
            self.assertEqual(record['fresh'], 1)
            self.assertEqual(record['nonfresh'], 1)
            self.assertTrue(record['build_finished'])

    def test_workflows_install_helper_and_preserve_release_bypass(self):
        root = HELPER.parents[2]
        for name in ['ci-builder.yaml', 'test.yml']:
            text = (root / '.gitea/workflows' / name).read_text()
            self.assertIn('FORTEMI_CACHE_CI:', text)
            self.assertIn('setup-run-resources.sh', text)
            self.assertIn('cargo-cache.py" finalize', text)
        text = (root / '.gitea/workflows/ci-builder.yaml').read_text()
        for family in ['api', 'hosted', 'mcp', 'bundle']:
            self.assertIn(f'build_and_push {family} --no-cache', text)


    def selection_env(self, root):
        return {'GITHUB_WORKSPACE': str(root), 'FORTEMI_CARGO_CACHE_ROOT': str(root / 'cache'),
                'FORTEMI_CACHE_CI': '1', 'FORTEMI_CACHE_EVENT': 'push',
                'FORTEMI_CACHE_REF': 'refs/heads/main', 'FORTEMI_CACHE_VARIANT': 'lint',
                'TMPDIR': str(root)}

    def test_invalid_entries_select_fresh_target_and_leave_bytes_untouched(self):
        for failure in ['missing-marker', 'corrupt-marker', 'changed-inode', 'foreign-uid', 'entry-symlink', 'marker-symlink']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                lane = cache.private_dir(root / 'cache/main')
                entry = lane / ('a' * 64)
                sentinel = root / 'outside'
                sentinel.mkdir()
                (sentinel / 'data').write_text('untouched')
                if failure == 'entry-symlink':
                    entry.symlink_to(sentinel, target_is_directory=True)
                else:
                    entry.mkdir(mode=0o700)
                    (entry / 'partial-artifact').write_text('interrupted')
                    marker = entry / '.owner.json'
                    if failure == 'corrupt-marker':
                        marker.write_text('{interrupted')
                    elif failure in {'changed-inode', 'foreign-uid'}:
                        info = entry.stat()
                        marker.write_text(json.dumps([cache.SCHEMA, os.geteuid() + 1 if failure == 'foreign-uid' else os.geteuid(), info.st_dev if failure == 'foreign-uid' else 0, info.st_ino if failure == 'foreign-uid' else 0]))
                    elif failure == 'marker-symlink':
                        marker.symlink_to(sentinel / 'data')
                for _ in range(2):  # an unchanged interrupted entry cannot wedge later jobs
                    output = io.StringIO()
                    with contextlib.redirect_stderr(output), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None):
                        selected = cache.setup(self.selection_env(root))
                    self.assertEqual(selected['FORTEMI_CARGO_CACHE_ENTRY'], '')
                    self.assertTrue(Path(selected['CARGO_TARGET_DIR']).is_relative_to(root))
                    self.assertFalse(Path(selected['CARGO_TARGET_DIR']).is_relative_to(lane))
                    record = json.loads(output.getvalue().splitlines()[-1])
                    self.assertFalse(record['cache_present'])
                    self.assertFalse(record['cache_enabled'])
                    self.assertIn('::warning::', output.getvalue())
                    with contextlib.redirect_stderr(io.StringIO()) as final_output:
                        self.assertEqual(cache.finalize({'FORTEMI_CARGO_CACHE_ENTRY': str(entry)}), 0)
                    self.assertIn('::warning::', final_output.getvalue())
                    self.assertTrue(entry.exists())
                    self.assertEqual((sentinel / 'data').read_text(), 'untouched')
                    if not entry.is_symlink():
                        self.assertEqual((entry / 'partial-artifact').read_text(), 'interrupted')

    def test_foreign_directory_uid_is_rejected_before_marker_access(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lane = cache.private_dir(root / 'cache/main')
            entry = cache.private_dir(lane / ('d' * 64))
            (entry / 'sentinel').write_text('retained')
            original = Path.lstat

            def foreign_uid(path, *args, **kwargs):
                info = original(path, *args, **kwargs)
                if path == entry:
                    fields = list(info)
                    fields[4] = os.geteuid() + 1
                    return os.stat_result(fields)
                return info

            with patch.object(Path, 'lstat', foreign_uid), patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None), contextlib.redirect_stderr(io.StringIO()):
                selected = cache.setup(self.selection_env(root))
            self.assertFalse(selected['FORTEMI_CARGO_CACHE_ENTRY'])
            self.assertEqual((entry / 'sentinel').read_text(), 'retained')
            self.assertFalse((entry / '.owner.json').exists())

    def test_valid_setup_reuses_target_and_job_variant_separates_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.selection_env(root)
            with patch.object(cache.subprocess, 'check_output', return_value='fixture'), patch.object(cache.shutil, 'which', return_value=None):
                first = cache.setup(env)
                (Path(first['FORTEMI_CARGO_CACHE_ENTRY']) / 'artifact').write_text('compiled')
                second = cache.setup(env)
                other = cache.setup(dict(env, FORTEMI_CACHE_VARIANT='coverage'))
            self.assertEqual(first, second)
            self.assertNotEqual(first['CARGO_TARGET_DIR'], other['CARGO_TARGET_DIR'])
            self.assertEqual((Path(second['FORTEMI_CARGO_CACHE_ENTRY']) / 'artifact').read_text(), 'compiled')

    def test_finalizer_failure_is_warning_and_preserves_workload_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = {'FORTEMI_CARGO_CACHE_ENTRY': str(Path(tmp) / 'entry')}
            # A finalization failure after a successful command must return zero.
            for error in [OSError('fixture'), ValueError('fixture')]:
                output = io.StringIO()
                with contextlib.redirect_stderr(output), patch.object(cache, 'private_dir', side_effect=error):
                    self.assertEqual(cache.finalize(env), 0)
                self.assertIn('::warning::', output.getvalue())
            for status in [0, 101]:
                with patch.dict(os.environ, {'FORTEMI_CARGO_CACHE_ENTRY': str(Path(tmp) / 'entry')}, clear=True), patch.object(cache, 'invoke', return_value=status), patch.object(cache, 'prune', side_effect=ValueError('fixture')), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(cache.main(['--', 'test']), status)

    def test_corrupt_entry_finalizer_returns_success_without_deletion(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = cache.private_dir(Path(tmp) / 'cache')
            entry = cache.private_dir(root / ('b' * 64))
            (entry / 'partial').write_text('interrupted')
            with contextlib.redirect_stderr(io.StringIO()) as output:
                self.assertEqual(cache.finalize({'FORTEMI_CARGO_CACHE_ENTRY': str(entry)}), 0)
            self.assertIn('::warning::', output.getvalue())
            self.assertEqual((entry / 'partial').read_text(), 'interrupted')

    def test_entry_lost_after_setup_preserves_real_cargo_exit_and_uses_fresh_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lane = cache.private_dir(root / 'cache')
            fake = root / 'cargo-fixture'
            fake.write_text('#!/usr/bin/env python3\nimport os,sys\nprint(os.environ["CARGO_TARGET_DIR"])\nsys.exit(int(os.environ["FIXTURE_STATUS"]))\n')
            fake.chmod(0o700)
            for status in [0, 101]:
                env = dict(os.environ, FORTEMI_REAL_CARGO=str(fake), TMPDIR=tmp,
                           FORTEMI_CARGO_CACHE_ENTRY=str(lane / ('c' * 64)),
                           CARGO_TARGET_DIR=str(lane / 'missing/target'), FIXTURE_STATUS=str(status))
                result = subprocess.run(['python3', str(HELPER), '--', 'test'], env=env,
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, status, result.stderr)
                chosen = Path(result.stdout.strip())
                self.assertTrue(chosen.is_relative_to(root))
                self.assertFalse(chosen.is_relative_to(lane))
                record = json.loads(result.stderr.splitlines()[-1])
                self.assertFalse(record['cache_enabled'])
                self.assertEqual(record['exit_code'], status)

    def test_workflow_proof_detects_target_override(self):
        env = {'FORTEMI_CARGO_CACHE_ENTRY': '/owned/key', 'CARGO_TARGET_DIR': '/run/target',
               'FORTEMI_CACHE_VARIANT': 'lint'}
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(cache.proof(env), 1)
            self.assertEqual(cache.proof(dict(env, CARGO_TARGET_DIR='/owned/key/target')), 0)
            self.assertEqual(cache.proof({}), 0)

    def test_sqlx_migration_changes_invalidate_but_stamps_retain_generation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            initial = cache.identity(root, 'compiler', {}, 'lint')
            for directory in ['.sqlx', 'migrations']:
                (root / directory).mkdir()
                source = root / directory / 'input'
                source.write_text('first')
                changed = cache.identity(root, 'compiler', {}, 'lint')
                self.assertNotEqual(initial, changed)
                source.write_text('second')
                self.assertNotEqual(changed, cache.identity(root, 'compiler', {}, 'lint'))
                initial = cache.identity(root, 'compiler', {}, 'lint')
            self.assertNotEqual(initial, cache.identity(root, 'compiler', {'SQLX_OFFLINE': 'true'}, 'lint'))
            for name in ['MATRIC_VERSION', 'MATRIC_GIT_SHA', 'MATRIC_BUILD_DATE']:
                self.assertEqual(initial, cache.identity(root, 'compiler', {name: 'changed'}, 'lint'))
                self.assertEqual(initial, cache.identity(root, 'compiler', {name: 'changed-again'}, 'lint'))

    def test_local_passes_stamps_and_always_invokes_cargo_for_source_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / 'source.rs'
            stamps = {'MATRIC_VERSION': 'v-test', 'MATRIC_GIT_SHA': 'sha-test', 'MATRIC_BUILD_DATE': 'date-test'}
            selection = {'CARGO_TARGET_DIR': str(Path(tmp) / 'target'), 'FORTEMI_CARGO_CACHE_ENTRY': ''}
            with patch.dict(os.environ, stamps, clear=True), patch.object(cache, 'setup', return_value=selection), patch.object(cache, 'invoke', return_value=0) as invoke:
                for content in ['before', 'after']:
                    source.write_text(content)
                    self.assertEqual(cache.main(['local', 'build', '--offline', '--locked']), 0)
                self.assertEqual(invoke.call_count, 2)
                for call in invoke.call_args_list:
                    args, env = call.args
                    self.assertEqual(args, ['build', '--offline', '--locked'])
                    for name, value in stamps.items():
                        self.assertEqual(env[name], value)
                    self.assertEqual(env['CARGO_TARGET_DIR'], selection['CARGO_TARGET_DIR'])

    def test_workflow_target_has_no_job_env_override_and_proof_follows_setup(self):
        import yaml
        root = HELPER.parents[2]
        for name in ['ci-builder.yaml', 'test.yml']:
            workflow = yaml.safe_load((root / '.gitea/workflows' / name).read_text())
            self.assertNotIn('CARGO_TARGET_DIR', workflow.get('env', {}))
            self.assertNotIn('FORTEMI_CACHE_VARIANT', workflow['env'])
            self.assertTrue(workflow['env']['FORTEMI_CARGO_CACHE_ROOT'].startswith('/srv/gitea-runner-fortemi/cache/'))
            for job in workflow['jobs'].values():
                self.assertNotIn('CARGO_TARGET_DIR', job.get('env', {}))
                steps = job.get('steps', [])
                for index, step in enumerate(steps):
                    if 'setup-run-resources.sh' in step.get('run', ''):
                        self.assertEqual(step['env']['FORTEMI_CACHE_VARIANT'], '${{ github.job }}')
                        self.assertIn('cargo-cache.py" proof', steps[index + 1]['run'])

    def test_docker_generation_reuses_stamps_and_requires_successful_output(self):
        # Execute the real shell flow with tool stubs and all mount/image paths
        # redirected into this disposable fixture. This does not prove rustc
        # fingerprinting; the parent's real compiler smoke supplies that proof.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tools = root / 'bin'
            tools.mkdir()
            image = root / 'image'
            image.mkdir()
            mount = root / 'mount'
            mount.mkdir()
            (root / 'Cargo.toml').write_text('[workspace]\n')
            lock = root / 'Cargo.lock'
            lock.write_text('lock1')
            source = root / 'source.rs'
            source.write_text('before')
            for name, content in {
                    'rustc': '#!/bin/sh\nprintf "%s\\n" "${FIXTURE_COMPILER:-rust1 host-a}"\n',
                    'dpkg-query': '#!/bin/sh\nprintf "%s\\n" "${FIXTURE_NATIVE:-native1}"\n',
                    'rust-cross-build': '''#!/usr/bin/env python3
import json, os, pathlib, sys
if sys.argv[1:] == ['triple']:
    print('fixture-triple')
    sys.exit(0)
assert sys.argv[1:3] == ['cargo', 'build']
assert '--locked' in sys.argv and '--release' in sys.argv
target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
with open('calls.jsonl', 'a') as stream:
    stream.write(json.dumps({'target': str(target), 'stamp': os.environ['MATRIC_VERSION']}) + '\\n')
if os.environ.get('FIXTURE_MODE') == 'failure':
    sys.exit(101)
if os.environ.get('FIXTURE_MODE') != 'missing':
    out = target / 'fixture-triple/release/matric-api'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(pathlib.Path('source.rs').read_text() + ':' + os.environ['MATRIC_VERSION'])
'''}.items():
                executable = tools / name
                executable.write_text(content)
                executable.chmod(0o700)
            helper = (HELPER.parents[2] / 'docker/rust-cache-build.sh').read_text()
            helper = helper.replace('/cargo-target', str(mount)).replace('/app/', str(image) + '/')
            script = root / 'build.sh'
            script.write_text(helper)
            env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
                       TARGETARCH='amd64', FORTEMI_API_FEATURES='hosted-auth,otel',
                       CARGO_BUILD_JOBS='1', MATRIC_VERSION='v1', MATRIC_GIT_SHA='sha1',
                       MATRIC_BUILD_DATE='date1', FORTEMI_CACHE_BUILD_PKE='0',
                       FORTEMI_CACHE_DISABLE='0', SQLX_OFFLINE='true')
            def run(**changes):
                return subprocess.run(['sh', str(script)], cwd=root, env=dict(env, **changes),
                                      capture_output=True, text=True, timeout=10)
            def calls():
                return [json.loads(line) for line in (root / 'calls.jsonl').read_text().splitlines()]
            first = run()
            self.assertEqual(first.returncode, 0, first.stderr)
            output = image / 'matric-api'
            self.assertEqual(output.read_text(), 'before:v1')
            first_target = calls()[-1]['target']
            source.write_text('after')
            changed = run(MATRIC_VERSION='v2', MATRIC_GIT_SHA='sha2', MATRIC_BUILD_DATE='date2')
            self.assertEqual(changed.returncode, 0, changed.stderr)
            self.assertEqual(calls()[-1], {'target': first_target, 'stamp': 'v2'})
            self.assertEqual(output.read_text(), 'after:v2')
            for change in [{'FIXTURE_COMPILER': 'rust2 host-a'}, {'FIXTURE_NATIVE': 'native2'},
                           {'TARGETARCH': 'arm64'}, {'FORTEMI_API_FEATURES': 'hosted-auth,kms-vault'},
                           {'SQLX_OFFLINE': 'false'}]:
                result = run(**change)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotEqual(calls()[-1]['target'], first_target)
            lock.write_text('lock2')
            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(calls()[-1]['target'], first_target)
            # A retained binary cannot mask Cargo failure or missing output.
            self.assertEqual(run(FIXTURE_MODE='failure').returncode, 101)
            binary = Path(calls()[-1]['target']) / 'fixture-triple/release/matric-api'
            binary.unlink()
            self.assertNotEqual(run(FIXTURE_MODE='missing').returncode, 0)

    def test_docker_family_mounts_preserve_namespace_arch_and_family(self):
        root = HELPER.parents[2]
        for name, pke in [('Dockerfile', '0'), ('Dockerfile.bundle', '1')]:
            source = (root / name).read_text()
            self.assertIn(f'ARG FORTEMI_CACHE_BUILD_PKE={pke}', source)
            mount = next(line for line in source.splitlines() if 'target=/cargo-target' in line)
            for value in ['FORTEMI_CACHE_NAMESPACE', 'TARGETARCH', 'FORTEMI_CACHE_FAMILY']:
                self.assertIn('${' + value + '}', mount)
            self.assertIn('sharing=locked', mount)
            # api/hosted/bundle share an arch and trust namespace but retain
            # different mount identities; comma-bearing feature lists stay keys.
            ids = [mount.replace('${FORTEMI_CACHE_FAMILY}', family)
                   for family in ['api', 'hosted', 'bundle']]
            self.assertEqual(len(set(ids)), 3)
            self.assertNotIn('${FORTEMI_API_FEATURES}', mount)
        workflow = (root / '.gitea/workflows/ci-builder.yaml').read_text()
        self.assertIn('--build-arg FORTEMI_CACHE_FAMILY="${label}"', workflow)
        helper = (root / 'docker/rust-cache-build.sh').read_text()
        self.assertIn('for inputs in .sqlx migrations', helper)
        self.assertNotIn('${MATRIC_VERSION', helper)
        self.assertNotIn('${MATRIC_GIT_SHA', helper)
        self.assertNotIn('${MATRIC_BUILD_DATE', helper)
        self.assertIn('[ ! -L "$old" ]', helper)
        self.assertNotIn('find /cargo-target -mindepth', helper)


if __name__ == '__main__':
    unittest.main()
