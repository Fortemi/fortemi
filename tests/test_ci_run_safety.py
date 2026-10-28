"""No-daemon regression tests for Option B's ownership, admission and retry paths."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts/ci' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


reaper = load('reaper', 'reconcile-run-resources.py')


class ReconcilerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'runs'
        self.root.mkdir()
        self.env = patch.dict(os.environ, {'CI_RUNS_ROOT': str(self.root), 'CI_RESOURCE_SCOPE': '99', 'CI_RESOURCE_ATTEMPT': '1'})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.removed = []
        self.objects = {}
        for kind in ['container', 'network', 'volume', 'image']:
            for owner in ['11', '22', '33', '44', '']:
                identity = f'{kind}-{owner or "unlabelled"}'
                created = time.time() - (30 if owner == '33' else 4000)
                labels = {'fortemi.ci.scope': owner, 'fortemi.ci.attempt': '1'} if owner else {}
                date = reaper.dt.datetime.fromtimestamp(created, reaper.dt.timezone.utc).isoformat()
                self.objects[identity] = {'Id': identity, 'Labels': labels, 'Config': {'Labels': labels},
                                          'Name': identity, 'Created': date, 'CreatedAt': date}
        for owner in ['11', '22', '33', '44', '55']:
            directory = self.root / (owner + '-1')
            directory.mkdir()
            (directory / 'payload').write_text('keep unless explicitly terminal and old')
            if owner != '55':
                (directory / '.owner.json').write_text(json.dumps({'fortemi.ci.scope': owner, 'fortemi.ci.attempt': '1',
                                         'created': time.time() - (30 if owner == '33' else 4000),
                                         'identity': [directory.stat().st_dev, directory.stat().st_ino]}))
        outside = Path(self.tmp.name) / 'outside'
        outside.mkdir()
        (self.root / '66-1').symlink_to(outside, target_is_directory=True)

    @staticmethod
    def date(value):
        return reaper.dt.datetime.fromtimestamp(value, reaper.dt.timezone.utc).isoformat()

    @staticmethod
    def state(status='cancelled', attempt='1', age=4000):
        return status, attempt, time.time() - age, time.time()

    def docker(self, *args):
        if 'inspect' in args:
            return json.dumps([self.objects[args[-1]]])
        if args[0] == 'ps' or 'ls' in args:
            kind = 'container' if args[0] == 'ps' else args[0]
            # Deliberately return an unlabelled object too; inspection must filter it.
            return '\n'.join(x for x in self.objects if x.startswith(kind + '-'))
        if args[0] == 'rm':
            self.assertIn('-v', args)  # anonymous data follows the labelled container ID
        self.assertNotIn('unlabelled', args[-1])
        self.assertNotEqual(args[:2], ('volume', 'rm'))
        self.removed.append(args[-1])
        return ''

    def execute(self, state, current=False):
        with patch.object(reaper, 'docker', self.docker), patch.object(reaper, 'run_state', state), contextlib.redirect_stdout(io.StringIO()) as output:
            reaper.reconcile(current)
        return output.getvalue()

    def test_terminal_old_only_active_young_and_unlabelled_survive(self):
        self.execute(lambda owner: self.state({'11': 'in_progress', '22': 'cancelled', '44': 'waiting'}[owner]))
        self.assertEqual(set(self.removed), {f'{k}-22' for k in ['container', 'network', 'image']})
        self.assertFalse((self.root / '22-1').exists())
        for owner in ['11', '33', '44', '55', '66']:
            self.assertTrue((self.root / (owner + '-1')).exists())

    def test_api_failure_deletes_nothing_including_current_cleanup(self):
        for current in [False, True]:
            with patch.dict(os.environ, {'CI_RESOURCE_SCOPE': '22' if current else '99'}):
                text = self.execute(lambda _: (_ for _ in ()).throw(ValueError()), current)
            self.assertIn('warning', text)
            self.assertEqual(self.removed, [])
            self.assertTrue((self.root / '22-1').exists())

    def test_mixed_lookup_failure_is_atomic_before_deletions(self):
        def states(owner):
            if owner == '44':
                raise ValueError('unusable response')
            return self.state('cancelled')
        self.execute(states)
        self.assertEqual(self.removed, [])

    def test_current_final_cleanup_only_owned_current(self):
        with patch.dict(os.environ, {'CI_RESOURCE_SCOPE': '22', 'CI_RESOURCE_ATTEMPT': '1'}):
            self.execute(lambda _: self.state('in_progress'), True)
        self.assertEqual(set(self.removed), {f'{k}-22' for k in ['container', 'network', 'image']})

    def test_api_uses_job_secret_at_call_time_and_validates_response(self):
        response = unittest.mock.MagicMock()
        response.__enter__.return_value = io.StringIO(json.dumps({'id':22,'run_attempt':1,'status':'completed','conclusion':'cancelled','completed_at':self.date(time.time()-4000)}))
        response.__enter__.return_value.headers = {'Date': reaper.email.utils.formatdate(time.time(), usegmt=True)}
        with patch.dict(os.environ, {'CI_ACTIONS_TOKEN': 'test-only-secret', 'GITHUB_REPOSITORY': 'Fortemi/fortemi'}), patch.object(reaper.urllib.request, 'urlopen', return_value=response) as api:
            self.assertEqual(reaper.run_state('22')[0], 'cancelled')
            self.assertEqual(api.call_args.args[0].get_header('Authorization'), 'token test-only-secret')
            self.assertEqual(api.call_args.kwargs['timeout'], 15)
        response.__enter__.return_value = io.StringIO('{}')
        response.__enter__.return_value.headers = {'Date': reaper.email.utils.formatdate(time.time(), usegmt=True)}
        with patch.dict(os.environ, {'CI_ACTIONS_TOKEN': 'test-only-secret', 'GITHUB_REPOSITORY': 'Fortemi/fortemi'}), patch.object(reaper.urllib.request, 'urlopen', return_value=response):
            with self.assertRaises(ValueError):
                reaper.run_state('22')


    def test_symlinked_ancestor_aborts_every_deletion(self):
        linked = Path(self.tmp.name) / 'linked'
        linked.symlink_to(self.tmp.name, target_is_directory=True)
        with patch.dict(os.environ, {'CI_RUNS_ROOT': str(linked / 'runs')}):
            output = self.execute(lambda _: self.state())
        self.assertIn('warning', output)
        self.assertEqual(self.removed, [])
        self.assertTrue((self.root / '22-1' / 'payload').exists())

    def test_root_directory_swap_preserves_both_trees(self):
        record = next(r for r in reaper.owned_roots(str(self.root)) if r[0] == '22')
        original = self.root / '22-1'
        saved = self.root / 'saved'
        original.rename(saved)
        original.mkdir()
        (original / '.owner.json').write_text((saved / '.owner.json').read_text())
        (original / 'replacement').write_text('keep')
        with self.assertRaises(ValueError):
            reaper.remove_root(str(self.root), record)
        self.assertTrue((saved / 'payload').exists())
        self.assertTrue((original / 'replacement').exists())

    def test_directory_swap_during_open_is_refused(self):
        record = next(r for r in reaper.owned_roots(str(self.root)) if r[0] == '22')
        root = self.root / '22-1'
        nested = root / 'nested'
        nested.mkdir()
        (nested / 'keep').write_text('keep')
        real_open = reaper.os.open
        def swapped(name, *args, **kwargs):
            descriptor = real_open(name, *args, **kwargs)
            if name == 'nested':
                # The source is already quarantined; move it through its parent FD.
                parent = kwargs['dir_fd']
                reaper.os.rename('nested', 'moved', src_dir_fd=parent, dst_dir_fd=parent)
                reaper.os.mkdir('nested', dir_fd=parent)
            return descriptor
        with patch.object(reaper.os, 'open', side_effect=swapped), self.assertRaises(ValueError):
            reaper.remove_root(str(self.root), record)
        pending = self.root / reaper.QUARANTINE / reaper.quarantine_name(record) / 'tree'
        self.assertTrue((pending / 'moved' / 'keep').exists())

    def test_symlinks_inside_are_unlinked_without_following(self):
        root = self.root / '22-1'
        outside = Path(self.tmp.name) / 'outside'
        (outside / 'keep').write_text('keep')
        (root / 'link').symlink_to(outside, target_is_directory=True)
        record = next(r for r in reaper.owned_roots(str(self.root)) if r[0] == '22')
        reaper.remove_root(str(self.root), record)
        self.assertTrue((outside / 'keep').exists())
        self.assertFalse(root.exists())

    def test_changed_marker_refuses_deletion(self):
        record = next(r for r in reaper.owned_roots(str(self.root)) if r[0] == '22')
        (self.root / '22-1' / '.owner.json').write_text('{}')
        with self.assertRaises(ValueError):
            reaper.remove_root(str(self.root), record)
        self.assertTrue((self.root / '22-1' / 'payload').exists())

    def test_restarted_run_is_rechecked_before_any_resource_deletion(self):
        calls = {}
        def state(owner):
            calls[owner] = calls.get(owner, 0) + 1
            return self.state() if calls[owner] == 1 else self.state('in_progress', '2')
        self.execute(state)
        self.assertEqual(self.removed, [])
        self.assertTrue((self.root / '22-1' / 'payload').exists())

    def test_recent_completion_and_server_clock_both_retain_old_resources(self):
        self.execute(lambda _: self.state(age=60))
        self.assertEqual(self.removed, [])
        # A forward host-clock jump alone cannot satisfy the server age bound.
        now = time.time()
        with patch.object(reaper.time, 'time', return_value=now + 10000):
            self.execute(lambda _: ('cancelled', '1', now - 60, now))
        self.assertEqual(self.removed, [])

    def test_nan_infinite_and_future_marker_times_are_not_candidates(self):
        for value in [float('nan'), float('inf'), time.time() + 10000]:
            marker = self.root / '22-1' / '.owner.json'
            marker.write_text(json.dumps({reaper.LABEL: '22', reaper.ATTEMPT: '1', 'created': value}))
            self.assertNotIn('22', [r[0] for r in reaper.owned_roots(str(self.root))])

    def test_labelled_named_volume_is_kept_and_reported_without_failing(self):
        outside = Path(self.tmp.name) / 'outside' / 'keep'
        outside.write_text('keep')
        output = self.execute(lambda _: self.state())
        self.assertIn('Kept labelled named volume volume-22', output)
        self.assertIn('reconcile: removed=12 kept=10', output)
        self.assertNotIn('volume-22', self.removed)
        self.assertTrue(outside.exists())
        self.assertTrue((self.root / '55-1' / 'payload').exists())
        self.assertFalse(any('unlabelled' in item for item in self.removed))

    def test_legacy_labelled_named_volumes_are_also_reported_and_kept(self):
        self.objects['volume-legacy'] = {'Name': 'volume-legacy', 'Labels': {reaper.LABEL: 'legacy'}}
        output = self.execute(lambda _: self.state())
        self.assertIn('Kept labelled named volume volume-legacy', output)
        self.assertNotIn('volume-legacy', self.removed)

    def test_scope_labelled_object_with_unusable_attempt_is_counted_as_kept(self):
        baseline = self.execute(lambda _: self.state())
        kept_before = int(baseline.rsplit('kept=', 1)[1])
        self.removed = []
        self.setUp()
        broken = {reaper.LABEL: '22'}  # attempt label missing
        self.objects['container-broken'] = {'Id': 'container-broken', 'Labels': broken,
                                            'Config': {'Labels': broken}, 'Name': 'container-broken',
                                            'Created': self.date(time.time() - 4000)}
        output = self.execute(lambda _: self.state())
        self.assertIn('::warning::Kept container container-broken', output)
        self.assertEqual(int(output.rsplit('kept=', 1)[1]), kept_before + 1)
        self.assertNotIn('container-broken', self.removed)

    def test_current_cleanup_cannot_remove_a_new_attempt(self):
        with patch.dict(os.environ, {'CI_RESOURCE_SCOPE': '22'}):
            self.execute(lambda _: self.state('in_progress', '2'), True)
        self.assertEqual(self.removed, [])
        self.assertTrue((self.root / '22-1').exists())

    def test_deletion_uses_inspected_immutable_id_not_inventory_alias(self):
        self.objects = {'container-alias': self.objects['container-22']}
        self.objects['container-alias']['Id'] = 'immutable-id'
        self.objects['immutable-id'] = self.objects['container-alias']
        def fake(*args):
            if args[0] == 'ps':
                return 'container-alias'
            if 'ls' in args:
                return ''
            return self.docker(*args)
        with patch.object(reaper, 'docker', fake), patch.object(reaper, 'run_state', lambda _: self.state()):
            reaper.reconcile()
        self.assertEqual(self.removed, ['immutable-id'])


    def assert_safe_retention(self, output):
        self.assertIn('::warning::Kept', output)
        self.assertRegex(output, r'reconcile: removed=\d+ kept=[1-9]\d*\n$')
        self.assertTrue((self.root / '55-1' / 'payload').exists())
        self.assertFalse(any('unlabelled' in item or item.startswith('volume-') for item in self.removed))
        self.assertTrue((Path(self.tmp.name) / 'outside' / 'keep').exists())

    def test_tree_swapped_before_quarantine_rename_is_kept_reported_and_successful(self):
        outside = Path(self.tmp.name) / 'outside'
        (outside / 'keep').write_text('keep')
        source = self.root / '22-1'
        original_rename = reaper.rename_noreplace
        def swapped(source_fd, name, destination_fd, destination):
            if name == '22-1':
                source.rename(outside / 'moved-original')
                source.mkdir()
                (source / 'unlabelled').write_text('keep replacement')
            original_rename(source_fd, name, destination_fd, destination)
        with patch.object(reaper, 'rename_noreplace', side_effect=swapped):
            output = self.execute(lambda _: self.state())
        self.assertIn('Kept root 22-1', output)
        self.assertTrue((outside / 'moved-original' / 'payload').exists())
        self.assertEqual(len(list((self.root / reaper.QUARANTINE).glob('22-1.*/tree/unlabelled'))), 1)
        # The mismatching tree remains on subsequent runs, too.
        output += self.execute(lambda _: self.state())
        self.assertIn('unusable quarantine identity/ticket', output)
        self.assert_safe_retention(output)

    def test_quarantine_destination_never_replaces_an_existing_tree(self):
        (Path(self.tmp.name) / 'outside' / 'keep').write_text('keep')
        original_rename = reaper.rename_noreplace
        def occupied(source_fd, name, destination_fd, destination):
            if name == '22-1':
                reaper.os.mkdir(destination, dir_fd=destination_fd)
                descriptor = reaper.os.open(destination, reaper.DIRECTORY, dir_fd=destination_fd)
                try:
                    file = reaper.os.open('unlabelled', os.O_WRONLY | os.O_CREAT, 0o600, dir_fd=descriptor)
                    os.close(file)
                finally:
                    os.close(descriptor)
            original_rename(source_fd, name, destination_fd, destination)
        with patch.object(reaper, 'rename_noreplace', side_effect=occupied):
            output = self.execute(lambda _: self.state())
        self.assertTrue((self.root / '22-1' / 'payload').exists())
        self.assertEqual(len(list((self.root / reaper.QUARANTINE).glob('22-1.*/tree/unlabelled'))), 1)
        self.assert_safe_retention(output)

    def test_unsupported_atomic_rename_is_kept_and_reported(self):
        (Path(self.tmp.name) / 'outside' / 'keep').write_text('keep')
        with patch.object(reaper, 'rename_noreplace', side_effect=OSError('unsupported')):
            output = self.execute(lambda _: self.state())
        self.assertTrue((self.root / '22-1' / 'payload').exists())
        self.assert_safe_retention(output)

    def test_cross_device_entry_is_kept_and_reported(self):
        (Path(self.tmp.name) / 'outside' / 'keep').write_text('keep')
        real_stat = reaper.os.stat
        def foreign(path, *args, **kwargs):
            info = real_stat(path, *args, **kwargs)
            if path == 'payload' and 'dir_fd' in kwargs:
                parts = list(info)
                parts[2] += 1  # st_dev, without mounting anything
                return os.stat_result(parts)
            return info
        with patch.object(reaper.os, 'stat', side_effect=foreign):
            output = self.execute(lambda _: self.state())
        self.assertEqual(len(list((self.root / reaper.QUARANTINE).glob('22-1.*/tree/payload'))), 1)
        self.assert_safe_retention(output)

    def test_quarantine_on_another_device_is_kept_and_reported(self):
        (Path(self.tmp.name) / 'outside' / 'keep').write_text('keep')
        real_fstat = reaper.os.fstat
        def foreign(fd):
            info = real_fstat(fd)
            if Path(os.readlink(f'/proc/self/fd/{fd}')).name == reaper.QUARANTINE:
                parts = list(info)
                parts[2] += 1
                return os.stat_result(parts)
            return info
        with patch.object(reaper.os, 'fstat', side_effect=foreign):
            output = self.execute(lambda _: self.state())
        self.assertTrue((self.root / '22-1' / 'payload').exists())
        self.assert_safe_retention(output)

    def test_interrupted_quarantine_is_finished_by_next_run(self):
        original_unlink = reaper.os.unlink
        def interrupted(path, *args, **kwargs):
            if path == 'payload':
                raise OSError('interrupted cleanup')
            return original_unlink(path, *args, **kwargs)
        with patch.object(reaper.os, 'unlink', side_effect=interrupted):
            output = self.execute(lambda _: self.state())
        self.assertIn('Kept root 22-1', output)
        self.assertFalse((self.root / '22-1').exists())
        self.assertEqual(len(list((self.root / reaper.QUARANTINE).glob('22-1.*/tree/payload'))), 1)
        output = self.execute(lambda _: self.state())
        self.assertIn('Removed owned quarantine 22-1.', output)
        self.assertEqual(list((self.root / reaper.QUARANTINE).iterdir()), [])

    def test_crashes_during_final_quarantine_steps_recover(self):
        for phase in ['tree', 'ticket', 'slot']:
            with self.subTest(phase=phase):
                run_root = self.root / '22-1'
                run_root.mkdir(exist_ok=True)
                (run_root / '.owner.json').write_text(json.dumps({reaper.LABEL: '22', reaper.ATTEMPT: '1', 'created': time.time()-4000,
                    'identity': [run_root.stat().st_dev, run_root.stat().st_ino]}))
                record = next(r for r in reaper.owned_roots(str(self.root)) if r[0] == '22')
                original_rmdir, original_unlink = reaper.os.rmdir, reaper.os.unlink
                def rmdir(path, *args, **kwargs):
                    if (phase == 'tree' and path == 'tree') or (phase == 'slot' and path == reaper.quarantine_name(record)):
                        raise OSError('interrupted final rmdir')
                    return original_rmdir(path, *args, **kwargs)
                def unlink(path, *args, **kwargs):
                    if phase == 'ticket' and path == '.owner.json' and Path(os.readlink(f'/proc/self/fd/{kwargs["dir_fd"]}')).name != 'tree':
                        raise OSError('interrupted ticket unlink')
                    return original_unlink(path, *args, **kwargs)
                with patch.object(reaper.os, 'rmdir', side_effect=rmdir), patch.object(reaper.os, 'unlink', side_effect=unlink):
                    with self.assertRaises(OSError):
                        reaper.remove_root(str(self.root), record)
                # Own final teardown may finish immediately; orphan empty metadata
                # still obeys the normal 30-minute age bound.
                with patch.dict(os.environ, {'CI_RESOURCE_SCOPE': '22'}):
                    output = self.execute(lambda _: self.state(), current=True)
                self.assertIn('Removed owned quarantine', output)
                self.assertEqual(list((self.root / reaper.QUARANTINE).iterdir()), [])

    def test_late_active_lookup_failure_keeps_all_remaining_resources(self):
        calls = 0
        def state(owner):
            nonlocal calls
            calls += 1
            if calls > 3:
                raise ValueError('lookup unavailable after preflight')
            return self.state()
        output = self.execute(state)
        self.assertIn('lookup/data/path unavailable', output)
        self.assertEqual(self.removed, [])
        self.assertTrue((self.root / '22-1' / 'payload').exists())

    def test_changed_labels_on_same_id_are_kept_successfully(self):
        original = self.docker
        inspected = set()
        def changed(*args):
            if args[:2] == ('container', 'inspect'):
                if args[-1] in inspected:
                    self.objects[args[-1]]['Config']['Labels'] = {}
                inspected.add(args[-1])
            return original(*args)
        with patch.object(reaper, 'docker', side_effect=changed), patch.object(reaper, 'run_state', lambda _: self.state()), contextlib.redirect_stdout(io.StringIO()) as output:
            reaper.reconcile()
        self.assertIn('Kept container container-22', output.getvalue())
        self.assertFalse(any(item.startswith('container-') for item in self.removed))


class ShellTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        self.env = dict(os.environ, PATH=f'{self.path}:{os.environ["PATH"]}',
                        CHAIN_SOURCE_SHA='a' * 40, GITHUB_REF='refs/heads/main', GITHUB_RUN_ID='99',
                        CI_RESOURCE_SCOPE='99', CI_RESOURCE_ATTEMPT='1', CI_TIP_RETRY_DELAY='0', TEST_API_IMAGE='matric-memory:test-99',
                        TEST_TMP=str(self.path))
        # CI exports CI_DOCKER_REAL (setup-run-resources.sh); inherited, it routes
        # resources.docker() to the real daemon instead of the PATH mock.
        self.env.pop('CI_DOCKER_REAL', None)
        self.fake('git', '''#!/usr/bin/env python3
import os,sys,pathlib
p=pathlib.Path(os.environ['TEST_TMP']);a=sys.argv[1:]
with (p/'calls').open('a') as f:f.write('git '+repr(a)+'\\n')
if a[0]=='ls-remote':
 if (p/'failure').exists():sys.exit(1)
 tip=(p/'tip').read_text() if (p/'tip').exists() else 'a'*40
 print(tip+'\\trefs/heads/main')
elif a[0]=='rev-parse':print('a'*40)
elif a[0]=='push' and (p/'reject').exists():
 if (p/'advance').exists():(p/'tip').write_text('b'*40)
 sys.exit(1)
''')
        self.fake('docker', '''#!/usr/bin/env python3
import os,sys,pathlib,json
p=pathlib.Path(os.environ['TEST_TMP']);a=sys.argv[1:]
with (p/'calls').open('a') as f:f.write('docker '+repr(a)+'\\n')
if a[0]=='create':
 if (p/'advance_on_lock').exists():(p/'tip').write_text('b'*40)
 if (p/'contend').exists():
  (p/'tip').write_text('b'*40)
  sys.exit(1)
 print('abcdef1234')
elif a[:2]==['container','inspect']:print(json.dumps([{'Id':a[-1],'Config':{'Labels':{'fortemi.ci.scope':os.environ['GITHUB_RUN_ID'],'fortemi.ci.attempt':os.environ['CI_RESOURCE_ATTEMPT']}}}]))
elif a[:2]==['image','inspect']:
 if not (p/'image').exists():sys.exit(1)
 print((p/'image').read_text())
elif a[0]=='build':(p/'image').write_text(os.environ['CHAIN_SOURCE_SHA'])
''')

        import sys
        self.fake('python3', '#!' + sys.executable + '\n' + r'''import importlib.util,os,sys
from pathlib import Path
if sys.argv[1] == 'scripts/ci/dispatch-main.py':
 with (Path(os.environ['TEST_TMP'])/'calls').open('a') as f:f.write('dispatch '+sys.argv[2]+'\n')
 sys.exit(0)
if sys.argv[1] == 'scripts/ci/main-side-effect.py':
 spec=importlib.util.spec_from_file_location('mutex',sys.argv[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
 def api(path):
  if 'page=1' in path:return {'total_count':1,'jobs':[{'id':123,'name':'Test Holder','run_id':99,'run_attempt':1,'status':'in_progress'}]},0
  return {'jobs':[]},0
 m.resources.api=api
 os.environ['CI_MUTEX_JOB_NAME']='Test Holder'
 sys.executable=str(Path(os.environ['TEST_TMP'])/'python3')
 try:sys.exit(m.main(sys.argv[2:]))
 except Exception:sys.exit(1)
os.execv(sys.executable,[sys.executable,*sys.argv[1:]])
''')

    def fake(self, name, text):
        target = self.path / name
        target.write_text(text)
        target.chmod(0o755)

    def shell(self, script, *args):
        return subprocess.run(['bash', str(ROOT / 'scripts/ci' / script), *args], env=self.env,
                              cwd=ROOT, text=True, capture_output=True, timeout=10)

    def calls(self):
        return (self.path / 'calls').read_text()

    def test_tip_current_superseded_and_unknown(self):
        self.assertEqual(self.shell('main-tip-guard.sh').returncode, 0)
        (self.path / 'tip').write_text('b' * 40)
        result = self.shell('main-tip-guard.sh')
        self.assertEqual(result.returncode, 10)
        self.assertIn('::notice::', result.stdout)
        (self.path / 'failure').touch()
        (self.path / 'calls').unlink()
        result = self.shell('main-tip-guard.sh')
        self.assertEqual(result.returncode, 1)
        self.assertIn('::error::', result.stderr)
        self.assertEqual(self.calls().count('ls-remote'), 3)

    def test_actual_downstream_admission_step_marks_stale_successfully(self):
        import yaml
        workflow = yaml.safe_load((ROOT / '.gitea/workflows/test.yml').read_text())
        script = next(step['run'] for step in workflow['jobs']['admit-source']['steps']
                      if step.get('id') == 'freshness')
        output = self.path / 'output'
        env = dict(self.env, GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_OUTPUT=str(output),
                   CI_SAFETY_TOOLS=str(ROOT))
        (self.path / 'tip').write_text('b' * 40)
        result = subprocess.run(['bash', '-c', script], cwd=ROOT, env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output.read_text().strip(), 'current=false')
        output.unlink()
        (self.path / 'tip').write_text('a' * 40)
        result = subprocess.run(['bash', '-c', script], cwd=ROOT, env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output.read_text().strip(), 'current=true')
        output.unlink()
        (self.path / 'failure').touch()
        result = subprocess.run(['bash', '-c', script], cwd=ROOT, env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())

    def test_superseded_sync_skips_successfully_and_current_pushes(self):
        (self.path / 'tip').write_text('b' * 40)
        result = self.shell('main-side-effect.sh', 'sync')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("'push'", self.calls())
        (self.path / 'tip').write_text('a' * 40)
        self.assertEqual(self.shell('main-side-effect.sh', 'sync').returncode, 0)
        self.assertIn("'push'", self.calls())

    def test_unknown_never_pushes(self):
        (self.path / 'failure').touch()
        result = self.shell('main-side-effect.sh', 'sync')
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("'push'", self.calls())
        self.assertNotIn("'rm', '-v', 'abcdef1234'", self.calls())

    def test_rejected_push_after_advance_is_success(self):
        (self.path / 'reject').touch()
        (self.path / 'advance').touch()
        result = self.shell('main-side-effect.sh', 'sync')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::notice::', result.stdout)

    def test_rejected_push_while_current_is_failure(self):
        (self.path / 'reject').touch()
        self.assertEqual(self.shell('main-side-effect.sh', 'sync').returncode, 1)

    def test_dispatch_current_superseded_and_unknown(self):
        result = self.shell('main-side-effect.sh', 'dispatch', 'test.yml', 'dispatch.json')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('dispatch test.yml', self.calls())
        (self.path / 'calls').unlink()
        (self.path / 'tip').write_text('b' * 40)
        result = self.shell('main-side-effect.sh', 'dispatch', 'test.yml', 'dispatch.json')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('dispatch test.yml', self.calls())
        (self.path / 'failure').touch()
        result = self.shell('main-side-effect.sh', 'dispatch', 'test.yml', 'dispatch.json')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn('dispatch test.yml', self.calls())

    def test_waiting_older_main_is_successfully_superseded_without_taking_lock(self):
        (self.path / 'contend').touch()
        result = self.shell('main-side-effect.sh', 'sync')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::notice::', result.stdout)
        self.assertNotIn("'push'", self.calls())
        self.assertNotIn("'rm', '-v', 'abcdef1234'", self.calls())

    def test_tip_is_rechecked_after_mutex_acquisition(self):
        (self.path / 'advance_on_lock').touch()
        result = self.shell('main-side-effect.sh', 'sync')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("'push'", self.calls())
        self.assertIn("'rm', '-v', 'abcdef1234'", self.calls())

    def test_partial_retry_rebuilds_run_source_and_checks_label(self):
        result = self.shell('ensure-run-api-image.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('GIT_SHA=' + 'a' * 40, self.calls())
        self.assertNotIn('ls-remote', self.calls())
        self.assertIn('org.opencontainers.image.revision', self.calls())
        (self.path / 'image').unlink()  # attempt N final cleanup; attempt N+1 rebuilds
        self.assertEqual(self.shell('ensure-run-api-image.sh').returncode, 0)
        (self.path / 'calls').unlink()
        self.assertEqual(self.shell('ensure-run-api-image.sh').returncode, 0)
        self.assertNotIn("'build'", self.calls())
        (self.path / 'image').write_text('b' * 40)
        self.assertEqual(self.shell('ensure-run-api-image.sh').returncode, 1)


    def test_runtime_role_restart_reuses_container_and_checks_only_new_logs(self):
        script = (ROOT / 'scripts/ci/test-bundle-runtime-role.sh').read_text()
        start = script.split('start() {', 1)[1].split('\nsql() {', 1)[0]
        log_has = script.split('container_log_has() {', 1)[1].split('\ncommon_env=', 1)[0]
        calls = self.path / 'runtime-calls'
        env = dict(self.env, RUNTIME_CALLS=str(calls))
        probe = r'''
set -euo pipefail
NAME=labelled-container
BUNDLE_IMAGE=pinned-image
TIMEOUT=10
fail() { exit 1; }
remove_docker() { docker "$@"; }
docker() {
    printf '%s\n' "$*" >> "$RUNTIME_CALLS"
    case "$1" in
        inspect) printf running ;;
        exec) return 0 ;;
        logs)
            [[ "$2" == --since && "$3" == "$STARTED_AT" ]] || return 1
            # Prior-start success must not be accepted in this startup window.
            if [[ "$STARTED_AT" == old-start ]]; then printf 'MCP credentials valid'; fi ;;
    esac
}
'''
        probe += '\nstart() {' + start + '\ncontainer_log_has() {' + log_has
        probe += r'''
start ready -v /var/lib/postgresql/data
start ready --restart
! container_log_has 'MCP credentials valid'
STARTED_AT=old-start
container_log_has 'MCP credentials valid'
'''
        result = subprocess.run(['bash', '-c', probe], env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = calls.read_text().splitlines()
        self.assertEqual(sum(line.startswith('run ') for line in lines), 1)
        self.assertEqual(sum(line.startswith('restart ') for line in lines), 1)
        self.assertEqual(sum(line.startswith('rm ') for line in lines), 1)
        self.assertTrue(any('-v /var/lib/postgresql/data' in line for line in lines))
        self.assertFalse(any('volume create' in line or 'volume rm' in line for line in lines))


class OwnershipTests(unittest.TestCase):
    def setUp(self):
        with patch.dict(os.environ, {'CI_DOCKER_REAL': '/mock/docker', 'CI_RESOURCE_SCOPE': '99', 'CI_RESOURCE_ATTEMPT': '1'}):
            self.module = load('owned_docker', 'owned-docker.py')

    def test_kind_and_image_volumes_remain_anonymous_on_labelled_containers(self):
        help_text = """  -d, --detach                           Run in background
  -v, --volume list                      Bind mount a volume
      --name string                      Container name
      --mount mount                      Mount
      --network network                  Network
"""
        def capture(*args):
            if args[-1] == '--help': return help_text
            return '[{"Config":{"Volumes":{"/var/lib/postgresql/data":{},"/var":{}}}}]'
        with patch.object(self.module, 'capture', capture), patch.object(self.module.subprocess, 'check_call') as create:
            args = self.module.container_args('run', ['-d', '--name', 'kind', '--network', 'kind-net', '--volume=/var', 'node:pin'])
        self.assertIn('fortemi.ci.scope=99', args)
        self.assertIn('fortemi.ci.attempt=1', args)
        self.assertIn('--volume=/var', args)
        create.assert_not_called()
        self.assertEqual(args[-1], 'node:pin')
        with patch.object(self.module, 'capture', capture):
            self.assertIn('type=volume,target=/var/lib/buildkit', self.module.container_args(
                'create', ['--mount', 'type=volume,target=/var/lib/buildkit', 'buildkit:pin']))
            for mount in [['-v', 'named:/var'], ['--mount', 'type=volume,source=named,target=/var']]:
                with self.assertRaises(ValueError):
                    self.module.container_args('create', [*mount, 'image'])

    def test_kind_node_run_with_hidden_net_alias_is_labelled(self):
        # Exact kind v0.31 control-plane invocation from ci-builder run 85870 job 205080;
        # help text is the real Docker 29.1.3 CLI, which hides the --net alias.
        help_text = (ROOT / 'tests/fixtures/docker/run-help-29.1.3.txt').read_text()
        def capture(*args):
            if args[-1] == '--help': return help_text
            return '[{"Config":{"Volumes":{"/var":{}}}}]'
        kind = ['--name', 'fortemi-psa-1-control-plane', '--hostname', 'fortemi-psa-1-control-plane',
                '--label', 'io.x-k8s.kind.role=control-plane', '--privileged',
                '--security-opt', 'seccomp=unconfined', '--security-opt', 'apparmor=unconfined',
                '--tmpfs', '/tmp', '--tmpfs', '/run', '--volume', '/var',
                '--volume', '/lib/modules:/lib/modules:ro', '-e', 'KIND_EXPERIMENTAL_CONTAINERD_SNAPSHOTTER',
                '--detach', '--tty', '--label', 'io.x-k8s.kind.cluster=fortemi-psa-1',
                '--net', 'fortemi-psa-1-net', '--restart=on-failure:1', '--init=false', '--cgroupns=private',
                '--publish=127.0.0.1:43003:6443/TCP', '-e', 'KUBECONFIG=/etc/kubernetes/admin.conf',
                'kindest/node:pin']
        with patch.object(self.module, 'capture', capture), patch.object(self.module.subprocess, 'check_call') as pull:
            args = self.module.container_args('run', kind)
        pull.assert_not_called()
        self.assertEqual(args[:-5], kind[:-1])
        self.assertEqual(args[-5:], [*self.module.LABELS, 'kindest/node:pin'])
        with patch.object(self.module, 'capture', capture):
            for bad in [['--net', 'n', '--bogus-flag', 'image'], ['--net', 'n', '-v', 'named:/var', 'image']]:
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    self.module.container_args('run', bad)

    def test_hidden_network_aliases_map_only_to_documented_flags(self):
        real_help = (ROOT / 'tests/fixtures/docker/run-help-29.1.3.txt').read_text()
        no_network_help = '\n'.join(line for line in real_help.splitlines() if '--network' not in line)
        def capture_with(help_text):
            def capture(*args):
                if args[-1] == '--help': return help_text
                return '[{"Config":{"Volumes":{}}}]'
            return capture
        accepted = [
            ['--net', 'n1', 'image'], ['--net=n1', 'image'],
            ['--net-alias', 'a1', '--net', 'n1', 'image'], ['--net-alias=a1', 'image'],
        ]
        for args in accepted:
            with self.subTest(args=args), patch.object(self.module, 'capture', capture_with(real_help)):
                out = self.module.container_args('run', args)
                self.assertEqual(out[-1], 'image')
                self.assertEqual(out[:len(args) - 1], args[:-1])
                self.assertEqual(out[-5:-1], self.module.LABELS)
        refused = [
            # Canonical flag absent from help: the alias is unknown too.
            (no_network_help, ['--net', 'n1', 'image']),
            (no_network_help, ['--net-alias=a1', 'image']),
            # Refusals after an alias still apply.
            (real_help, ['--net', 'n1', '--mount', 'type=volume,source=named,target=/var', 'image']),
            (real_help, ['--net=n1', '-v', 'named:/var', 'image']),
            (real_help, ['--net-alias', 'a1', '--netx', 'n1', 'image']),
        ]
        for help_text, args in refused:
            with self.subTest(args=args), patch.object(self.module, 'capture', capture_with(help_text)):
                with self.assertRaises(ValueError):
                    self.module.container_args('run', args)

    def test_owned_image_with_wrong_source_is_refused_before_use(self):
        def capture(*args):
            if args[-1] == '--help': return ''
            return json.dumps([{'Config': {'Labels': {'fortemi.ci.scope': '99',
                                  'org.opencontainers.image.revision': 'b' * 40}}}])
        with patch.dict(os.environ, {'CHAIN_SOURCE_SHA': 'a' * 40}), patch.object(self.module, 'capture', capture):
            with self.assertRaises(ValueError):
                self.module.container_args('run', ['api:99'])

    def test_builds_always_have_run_and_source_labels(self):
        for command in [['build', '-t', 'api:99', '.'], ['buildx', 'build', '--load', '.']]:
            with patch.dict(os.environ, {'CHAIN_SOURCE_SHA': 'a' * 40}), patch.object(self.module.os, 'execv') as execute:
                self.module.main(command)
            self.assertIn('fortemi.ci.scope=99', execute.call_args.args[1])
            self.assertIn('fortemi.ci.attempt=1', execute.call_args.args[1])
            self.assertIn('org.opencontainers.image.revision=' + 'a' * 40, execute.call_args.args[1])

    def test_every_removal_resolves_and_rechecks_labelled_immutable_id(self):
        for kind, command in [('container', ['rm', '-fv']), ('container', ['container', 'rm', '-v']),
                              ('network', ['network', 'rm']), ('image', ['rmi']), ('image', ['image', 'rm'])]:
            with self.subTest(kind=kind, command=command):
                calls = []
                def capture(*args):
                    calls.append(args)
                    return json.dumps([{'Id': 'immutable-id', 'Labels': {reaper.LABEL: '99', reaper.ATTEMPT: '1'},
                        'Config': {'Labels': {reaper.LABEL: '99', reaper.ATTEMPT: '1'}}}])
                with patch.object(self.module, 'capture', side_effect=capture), patch.object(self.module.subprocess, 'check_call') as remove:
                    self.module.main([*command, 'alias-or-tag'])
                self.assertEqual(calls, [(kind, 'inspect', 'alias-or-tag'), (kind, 'inspect', 'immutable-id')])
                self.assertEqual(remove.call_args.args[0][-1], 'immutable-id')
                self.assertNotIn('alias-or-tag', remove.call_args.args[0])
                if kind == 'container':
                    self.assertIn('-v', remove.call_args.args[0])

    def test_unlabelled_replacement_and_new_attempt_are_kept_without_failure(self):
        for tags in [{}, {reaper.LABEL: '88', reaper.ATTEMPT: '1'}, {reaper.LABEL: '99', reaper.ATTEMPT: '2'}]:
            with self.subTest(tags=tags):
                first = {'Id': 'immutable-id', 'Config': {'Labels': {reaper.LABEL: '99', reaper.ATTEMPT: '1'}}}
                final = {'Id': 'immutable-id', 'Config': {'Labels': tags}}
                with patch.object(self.module, 'capture', side_effect=[json.dumps([first]), json.dumps([final])]), \
                     patch.object(self.module.subprocess, 'check_call') as remove, contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertIsNone(self.module.main(['rm', '-fv', 'reused-name']))
                remove.assert_not_called()
                self.assertIn('Kept container reused-name', output.getvalue())

    def test_named_volume_removal_reports_success_without_calling_docker(self):
        with patch.object(self.module, 'capture') as inspect, patch.object(self.module.subprocess, 'check_call') as remove, \
             contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertIsNone(self.module.main(['volume', 'rm', 'legacy-labelled-volume']))
        inspect.assert_not_called()
        remove.assert_not_called()
        self.assertIn('Kept named volume legacy-labelled-volume', output.getvalue())

    def test_no_named_volume_creation_prune_or_plugin_deletion_bypass(self):
        for command in [['volume', 'create', 'named'], ['system', 'prune'], ['buildx', 'rm', 'builder']]:
            with self.subTest(command=command), patch.object(self.module.os, 'execv') as execute:
                with self.assertRaises(ValueError):
                    self.module.main(command)
                execute.assert_not_called()
        with patch.object(self.module.os, 'execv') as execute:
            self.module.main(['buildx', 'rm', '--keep-daemon', '--keep-state', 'builder'])
        self.assertIn('--keep-daemon', execute.call_args.args[1])

    def test_automatic_container_deletion_is_deferred_to_id_checked_cleanup(self):
        def capture(*args):
            if args[-1] == '--help':
                return '      --rm                              Automatically remove\n'
            return '[{"Config":{"Volumes":{"/var":{}}}}]'
        for flag in ['--rm', '--rm=true']:
            with patch.object(self.module, 'capture', capture):
                args = self.module.container_args('run', [flag, 'image'])
            self.assertNotIn(flag, args)
            self.assertIn('fortemi.ci.scope=99', args)


class MutexTests(unittest.TestCase):
    def setUp(self):
        self.module = load('mutex', 'main-side-effect.py')
        self.resources = self.module.resources
        self.info = {'Id': 'immutable-lock', 'Created': ReconcilerTests.date(time.time()-4000),
                     'Config': {'Labels': {reaper.LABEL: '99', reaper.ATTEMPT: '1',
                                           self.module.JOB: '123'}}}
        self.job = {'id': 123, 'run_id': 99, 'run_attempt': 1,
                    'status': 'completed', 'conclusion': 'cancelled'}

    def recover(self, job=None):
        value = self.job if job is None else job
        with patch.object(self.resources, 'inspect', return_value=self.info), \
             patch.object(self.resources, 'api', return_value=(value, time.time())) as api, \
             patch.object(self.resources, 'docker') as docker:
            self.module.recover()
        return api, docker

    def test_abandoned_terminal_previous_attempt_of_current_run_is_recovered(self):
        with patch.dict(os.environ, {'GITHUB_RUN_ID': '99', 'CI_RESOURCE_ATTEMPT': '2'}):
            api, docker = self.recover()
        self.assertEqual(api.call_count, 2)
        api.assert_called_with('jobs/123')
        docker.assert_called_once_with('rm', '-v', 'immutable-lock')
        # No whole-run lookup: queued cleanup can keep that run nonterminal.
        self.assertTrue(all(call.args == ('jobs/123',) for call in api.call_args_list))

    def test_live_holder_is_never_stolen(self):
        for state in ['in_progress', 'queued', 'waiting']:
            with self.subTest(state=state):
                _, docker = self.recover(dict(self.job, status=state))
                docker.assert_not_called()

    def test_unknown_or_mismatched_holder_is_retained(self):
        for value in [{}, dict(self.job, run_attempt=2), dict(self.job, run_id=77),
                      dict(self.job, id=999), dict(self.job, conclusion='unknown')]:
            _, docker = self.recover(value)
            docker.assert_not_called()

    def test_holder_rechecked_before_removal_and_replacement_id_preserved(self):
        with patch.object(self.resources, 'inspect', return_value=self.info), \
             patch.object(self.resources, 'api', side_effect=[(self.job, time.time()),
                        (dict(self.job, status='in_progress'), time.time())]), \
             patch.object(self.resources, 'docker') as docker:
            self.module.recover()
        docker.assert_not_called()
        with patch.object(self.resources, 'inspect', side_effect=[self.info, dict(self.info, Id='replacement')]), \
             patch.object(self.resources, 'api', return_value=(self.job, time.time())), \
             patch.object(self.resources, 'docker') as docker:
            self.module.recover()
        docker.assert_not_called()

    def test_young_lock_and_server_age_protect_clock_jump(self):
        now = time.time()
        self.info['Created'] = ReconcilerTests.date(now - 60)
        with patch.object(self.resources.time, 'time', return_value=now+10000), \
             patch.object(self.resources, 'inspect', return_value=self.info), \
             patch.object(self.resources, 'api', return_value=(self.job, now)), \
             patch.object(self.resources, 'docker') as docker:
            self.module.recover()
        docker.assert_not_called()

    def test_monotonic_deadline_counts_docker_duration(self):
        clock = [0]
        commands = []
        def command(args, **kwargs):
            commands.append(args)
            if args[:2] == ['docker', 'create']:
                clock[0] = 2161  # command time counts, even with no polling sleep
                return subprocess.CompletedProcess(args, 1, '')
            return subprocess.CompletedProcess(args, 0, '')
        with patch.dict(os.environ, {'GITHUB_RUN_ID': '99', 'CI_RESOURCE_ATTEMPT': '2'}), \
             patch.object(self.module, 'current_job', return_value='456'), \
             patch.object(self.module.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(self.module.time, 'sleep') as sleep, \
             patch.object(self.module.subprocess, 'run', side_effect=command):
            with self.assertRaises(TimeoutError):
                self.module.main(['sync'])
        sleep.assert_not_called()
        self.assertFalse(any(args[:2] == ['git', 'push'] for args in commands))
        create = next(args for args in commands if args[:2] == ['docker', 'create'])
        for label in ['fortemi.ci.scope=99', 'fortemi.ci.attempt=2', 'fortemi.ci.job=456']:
            self.assertIn(label, create)

    def test_own_release_uses_id_even_when_acquisition_deadline_elapsed(self):
        clock = [0]
        def command(args, **kwargs):
            if args[:2] == ['docker', 'create']:
                clock[0] = 2161
                return subprocess.CompletedProcess(args, 0, 'abcdef1234')
            return subprocess.CompletedProcess(args, 0, '')
        with patch.dict(os.environ, {'GITHUB_RUN_ID': '99', 'CI_RESOURCE_ATTEMPT': '2'}), \
             patch.object(self.module, 'current_job', return_value='456'), \
             patch.object(self.module.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(self.module.subprocess, 'run', side_effect=command), \
             patch.object(self.resources, 'inspect', return_value=dict(self.info, Id='abcdef1234',
                 Config={'Labels': {reaper.LABEL: '99', reaper.ATTEMPT: '2'}})), \
             patch.object(self.resources, 'docker') as docker:
            with self.assertRaises(TimeoutError):
                self.module.main(['sync'])
        docker.assert_called_once_with('rm', '-v', 'abcdef1234')
        self.assertFalse(hasattr(self.resources, 'deadline'))

    def test_paginated_holder_lookup_rejects_ambiguity(self):
        job = dict(self.job, status='in_progress', name='Holder')
        other = dict(job, id=321, name='Other job')
        with patch.dict(os.environ, {'CI_MUTEX_JOB_NAME':'Holder'}), \
             patch.object(self.resources, 'api', side_effect=[
                 ({'total_count': 2, 'jobs': [other]}, 0),
                 ({'total_count': 2, 'jobs': [job]}, 0)]) as api:
            self.assertEqual(self.module.current_job('99', '1'), '123')
            self.assertEqual(api.call_count, 2)
        with patch.dict(os.environ, {'CI_MUTEX_JOB_NAME':'Holder'}), \
             patch.object(self.resources, 'api', return_value=(
                 {'total_count': 2, 'jobs': [job, dict(job, id=456)]}, 0)):
            with self.assertRaises(ValueError):
                self.module.current_job('99', '1')


class WorkflowToolingTests(unittest.TestCase):
    """Execute the workflow shells with separate workflow/payload trees and mocked I/O."""

    def setUp(self):
        import sys
        import yaml
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        # A historical payload has neither safety helper. Bootstrap must obtain
        # them from the workflow tree, never from this working directory.
        self.payload = self.path / 'historical-payload'
        self.payload.mkdir()
        self.bin = self.path / 'bin'
        self.bin.mkdir()
        self.workflow_sha = 'b' * 40
        self.historical_sha = '2f2232b6030484381c0145d5a5f141732176e7eb'
        self.jobs = yaml.safe_load((ROOT / '.gitea/workflows/test.yml').read_text())['jobs']
        self.env = dict(os.environ, PATH=f'{self.bin}:{os.environ["PATH"]}',
                        CI_WORKFLOW_SHA=self.workflow_sha, CHAIN_SOURCE_SHA=self.historical_sha,
                        GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_REF='refs/heads/main',
                        GITHUB_REPOSITORY='Fortemi/fortemi', GITHUB_RUN_ID='99',
                        CI_RESOURCE_SCOPE='99', CI_RESOURCE_ATTEMPT='2',
                        GITHUB_OUTPUT=str(self.path / 'output'), GITHUB_ENV=str(self.path / 'env'), CI_TIP_RETRY_DELAY='0',
                        CI_TOOLS_ROOT=str(self.path / 'tools'),
                        CI_RUNS_ROOT=str(self.path / 'runs'), TEST_WORKFLOW_TREE=str(ROOT),
                        TEST_IO=str(self.path / 'io'), TEST_MAIN_TIP=self.workflow_sha)
        self.fake('git', '#!' + sys.executable + '\n' + r'''import json,os,sys,shutil
from pathlib import Path
a=sys.argv[1:];cwd=Path.cwd()
with open(os.environ['TEST_IO'],'a') as f:f.write(json.dumps(['git',str(cwd),a])+'\n')
if a[:1]==['-C']:cwd=Path(a[1]);a=a[2:]
if a[:2]==['init','-q']:
 Path(a[2]).mkdir(parents=True,exist_ok=True)
elif a[:2]==['remote','add'] or a[:2]==['sparse-checkout','set']:pass
elif a[:1]==['fetch']:(cwd/'fetched-sha').write_text(a[-1])
elif a[:1]==['checkout']:
 # Only the workflow revision carries the new helpers.
 if (cwd/'fetched-sha').read_text()==os.environ['CI_WORKFLOW_SHA']:
  target=cwd/'scripts/ci';target.mkdir(parents=True)
  for name in ['main-tip-guard.sh','reconcile-run-resources.py','owned-docker.py','configure-run-root.py','setup-run-resources.sh']:
   shutil.copyfile(Path(os.environ['TEST_WORKFLOW_TREE'])/'scripts/ci'/name,target/name)
elif a[:1]==['rev-parse']:print((cwd/'fetched-sha').read_text())
elif a[:1]==['ls-remote']:
 assert (cwd/'scripts/ci/main-tip-guard.sh').is_file()
 print(os.environ['TEST_MAIN_TIP']+'\trefs/heads/main')
else:sys.exit('Unexpected Git call: '+repr(a))
''')
        self.fake('python3', '#!' + sys.executable + '\n' + r'''import importlib.util,json,os,sys,time
from pathlib import Path
path=Path(sys.argv[1])
if path.name!='reconcile-run-resources.py':os.execv(sys.executable,[sys.executable,*sys.argv[1:]])
assert path.name=='reconcile-run-resources.py' and path.is_file()
assert path.is_relative_to(Path(os.environ['CI_SAFETY_TOOLS']))
with open(os.environ['TEST_IO'],'a') as f:f.write(json.dumps(['cleanup',sys.argv[1:]])+'\n')
spec=importlib.util.spec_from_file_location('cleanup',path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
# Exercise real filesystem cleanup; daemon and API I/O remain mocked.
m.docker=lambda *args:''
m.run_state=lambda owner:('success',os.environ['CI_RESOURCE_ATTEMPT'],time.time()-4000,time.time())
m.reconcile('--current' in sys.argv[2:])
''')

    def fake(self, name, text):
        target = self.bin / name
        target.write_text(text)
        target.chmod(0o755)

    def run_step(self, step):
        result = subprocess.run(['bash', '-e', '-c', step['run']], cwd=self.payload,
                                env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        envfile = Path(self.env['GITHUB_ENV'])
        if envfile.exists():
            self.env.update(line.split('=', 1) for line in envfile.read_text().splitlines())
        return result

    def bootstrap(self, job):
        config = self.jobs[job]
        self.assertEqual(config['env']['CI_WORKFLOW_SHA'], '${{ gitea.sha }}')
        tools = config['env']['CI_SAFETY_TOOLS']
        self.assertIn('${{ gitea.run_id }}-${{ gitea.run_attempt }}', tools)
        self.env['CI_SAFETY_TOOLS'] = str(self.path / 'tools/99-2')
        self.assertFalse((self.payload / 'scripts/ci/main-tip-guard.sh').exists())
        self.assertFalse((self.payload / 'scripts/ci/reconcile-run-resources.py').exists())
        self.run_step(config['steps'][0])
        calls = [json.loads(line) for line in (self.path / 'io').read_text().splitlines()]
        fetches = [call[2] for call in calls if call[0] == 'git' and 'fetch' in call[2]]
        self.assertEqual(len(fetches), 1)
        self.assertEqual(fetches[0][-1], self.workflow_sha)
        self.assertTrue(any('sparse-checkout' in call[2] and 'scripts/ci' in call[2]
                            for call in calls if call[0] == 'git'))

    def admission(self):
        self.bootstrap('admit-source')
        return self.run_step(next(step for step in self.jobs['admit-source']['steps']
                                  if step.get('id') == 'freshness'))

    def test_historical_stale_dispatch_succeeds_without_payload_helpers(self):
        result = self.admission()
        self.assertEqual(Path(self.env['GITHUB_OUTPUT']).read_text(), 'current=false\n')
        self.assertIn('::notice::Superseded source ' + self.historical_sha, result.stdout)
        # All build/test work has a false admission conjunct, including always() overlay.
        for name in ['build-testdb', 'fast-tests', 'integration-tests', 'coverage',
                     'slow-tests', 'validate-intel-overlay']:
            self.assertTrue(self.jobs[name]['if'].startswith("needs.admit-source.outputs.current == 'true'"))
        summary = self.jobs['test-summary']['steps'][0]
        script = summary['run'].replace('${{ needs.admit-source.outputs.current }}', 'false')
        result = self.run_step(dict(summary, run=script))
        self.assertIn('::notice::', result.stdout)
        self.run_step(self.jobs['admit-source']['steps'][-1])
        self.assertTrue(Path(self.env['CI_SAFETY_TOOLS']).exists())

    def test_cleanup_uses_workflow_tooling_when_payload_has_no_helpers(self):
        self.bootstrap('cleanup-run-images')
        runs = Path(self.env['CI_RUNS_ROOT'])
        for owner in ['99', '88']:
            root = runs / f'{owner}-2'
            root.mkdir(parents=True)
            (root / '.owner.json').write_text(json.dumps({
                'fortemi.ci.scope': owner, 'fortemi.ci.attempt': '2', 'created': time.time()-4000,
                'identity': [root.stat().st_dev, root.stat().st_ino]}))
            (root / 'payload').write_text('owned temporary resource')
        steps = self.jobs['cleanup-run-images']['steps']
        sweep = next(step for step in steps if step['name'] == 'Reconcile inactive runs')
        self.assertIn("needs.admit-source.outputs.current == 'true'", sweep['if'])
        teardown = next(step for step in steps if '--current' in step.get('run', ''))
        self.assertEqual(teardown['if'], 'always()')
        result = self.run_step(teardown)
        self.assertIn('Removed owned root 99-2', result.stdout)
        self.assertNotIn('::warning::', result.stdout)
        self.assertFalse((runs / '99-2').exists())
        self.assertTrue((runs / '88-2/payload').is_file())
        self.run_step(steps[-1])
        self.assertFalse(Path(self.env['CI_SAFETY_TOOLS']).exists())

    def test_current_dispatch_is_still_admitted(self):
        self.env['CHAIN_SOURCE_SHA'] = self.workflow_sha
        result = self.admission()
        self.assertEqual(Path(self.env['GITHUB_OUTPUT']).read_text(), 'current=true\n')
        self.assertNotIn('Superseded', result.stdout)


class AdmissionTests(unittest.TestCase):
    def test_stale_admission_skips_overlay_and_all_heavy_jobs(self):
        import yaml
        jobs = yaml.safe_load((ROOT / '.gitea/workflows/test.yml').read_text())['jobs']
        for name in ['build-testdb', 'fast-tests', 'integration-tests', 'coverage',
                     'slow-tests', 'validate-intel-overlay']:
            with self.subTest(job=name):
                self.assertIn('admit-source', jobs[name]['needs'])
                self.assertIn("needs.admit-source.outputs.current == 'true'", jobs[name]['if'])
                # The first conjunct is false for rejected admission regardless
                # of skipped dependencies or the always() override.
                self.assertTrue(jobs[name]['if'].startswith("needs.admit-source.outputs.current == 'true'"))
        cleanup = jobs['cleanup-run-images']['steps']
        sweep = next(step for step in cleanup if step['name'] == 'Reconcile inactive runs')
        self.assertIn("needs.admit-source.outputs.current == 'true'", sweep['if'])
        teardown = next(step for step in cleanup if '--current' in step.get('run', ''))
        self.assertEqual(teardown['if'], 'always()')
        summary = jobs['test-summary']['steps'][0]['run']
        self.assertIn('needs.admit-source.outputs.current', summary)
        self.assertIn('exit 0', summary.split('echo "Test Summary:', 1)[0])



class RoundFourFilesystemTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        self.runs = self.path / 'runs'
        self.root = self.runs / '99-2'
        self.root.mkdir(parents=True)
        self.env = self.fixture_environment()
        info = self.root.stat()
        self.marker = {reaper.LABEL: '99', reaper.ATTEMPT: '2', 'created': time.time()-4000,
                       'identity': [info.st_dev, info.st_ino]}
        (self.root / 'sentinel').write_text('original retained')

    def fixture_environment(self):
        env = dict(os.environ, CI_RUNS_ROOT=str(self.runs), CI_RESOURCE_SCOPE='99',
                   CI_RESOURCE_ATTEMPT='2', CI_SAFETY_TOOLS=str(ROOT),
                   CARGO_TARGET_DIR=str(self.root / 'target'),
                   CARGO_INSTALL_ROOT=str(self.root / 'tools'),
                   DOCKER_CONFIG=str(self.root / 'docker'),
                   GITHUB_ENV=str(self.path / 'env'), GITHUB_PATH=str(self.path / 'path'))
        # Live CI's bootstrap target overrides CARGO_TARGET_DIR during setup.
        # These ownership fixtures use their own paths and never select a cache.
        env.pop('FORTEMI_CARGO_RUN_TARGET_DIR', None)
        env.pop('FORTEMI_CACHE_CI', None)
        return env

    def setup(self, marker):
        (self.root / '.owner.json').write_text(json.dumps(marker))
        result = subprocess.run(['bash', str(ROOT / 'scripts/ci/setup-run-resources.sh')],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        values = dict(line.split('=', 1) for line in (self.path / 'env').read_text().splitlines())
        return result, Path(values['TMPDIR']).parent, values

    def test_inherited_ci_targets_do_not_escape_fixture_run_root(self):
        inherited = {
            'CARGO_TARGET_DIR': str(self.path / 'persistent-cache/entry/target'),
            'FORTEMI_CARGO_RUN_TARGET_DIR': str(self.path / 'live-runs/93445-1/target'),
            'FORTEMI_CACHE_CI': '1',
            'FORTEMI_CARGO_CACHE_ROOT': str(self.path / 'persistent-cache'),
        }
        with patch.dict(os.environ, inherited):
            self.env = self.fixture_environment()
        self.assertNotIn('FORTEMI_CARGO_RUN_TARGET_DIR', self.env)
        self.assertNotIn('FORTEMI_CACHE_CI', self.env)
        result, chosen, values = self.setup(self.marker)
        self.assertEqual(chosen, self.root)
        self.assertNotIn('::warning::', result.stderr)
        for key, child in [('CARGO_TARGET_DIR', 'target'), ('CARGO_INSTALL_ROOT', 'tools'),
                           ('DOCKER_CONFIG', 'docker'), ('TMPDIR', 'tmp')]:
            self.assertEqual(values[key], str(self.root / child))
        self.assertNotIn('FORTEMI_REAL_CARGO', values)
        self.assertFalse((self.path / 'persistent-cache').exists())
        self.assertFalse((self.path / 'live-runs').exists())

    def test_existing_marker_requires_owner_attempt_device_and_inode(self):
        bad_markers = [dict(self.marker, **{reaper.LABEL:'88'}),
                       dict(self.marker, **{reaper.ATTEMPT:'1'}),
                       dict(self.marker, identity=[0, self.root.stat().st_ino]),
                       dict(self.marker, identity=[self.root.stat().st_dev, 0]),
                       {k:v for k,v in self.marker.items() if k != 'identity'}]
        for marker in bad_markers:
            with self.subTest(marker=marker):
                result, chosen, values = self.setup(marker)
                self.assertNotEqual(chosen, self.root)
                self.assertIn('::warning::Kept existing root', result.stderr)
                self.assertEqual((self.root / 'sentinel').read_text(), 'original retained')
                registered = json.loads((chosen / '.owner.json').read_text())
                self.assertEqual(registered['identity'], [chosen.stat().st_dev, chosen.stat().st_ino])
                self.assertEqual(registered[reaper.LABEL], '99')
                for key in ['DOCKER_CONFIG', 'CARGO_TARGET_DIR', 'CARGO_INSTALL_ROOT']:
                    self.assertTrue(Path(values[key]).is_relative_to(chosen))

    def test_valid_existing_marker_is_validated_and_reused(self):
        result, chosen, _ = self.setup(self.marker)
        self.assertEqual(chosen, self.root)
        self.assertNotIn('::warning::', result.stderr)

    def test_marker_symlink_is_retained_and_replaced_with_fresh_root(self):
        other = self.path / 'other-marker'
        other.write_text(json.dumps(self.marker))
        (self.root / '.owner.json').symlink_to(other)
        result = subprocess.run(['bash', str(ROOT / 'scripts/ci/setup-run-resources.sh')],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::warning::Kept existing root', result.stderr)
        self.assertTrue((self.root / '.owner.json').is_symlink())
        self.assertEqual(json.loads(other.read_text()), self.marker)

    def test_symlinked_runs_ancestor_uses_registered_fallback_without_outside_writes(self):
        outside = self.path / 'outside'
        outside.mkdir()
        link = self.path / 'linked'
        link.symlink_to(outside, target_is_directory=True)
        nominal = link / 'runs/99-2'
        env = dict(self.env, CI_RUNS_ROOT=str(link / 'runs'),
                   DOCKER_CONFIG=str(nominal / 'docker'), CARGO_TARGET_DIR=str(nominal / 'target'),
                   CARGO_INSTALL_ROOT=str(nominal / 'tools'))
        result = subprocess.run(['bash', str(ROOT / 'scripts/ci/setup-run-resources.sh')],
                                env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::warning::Kept unusable runs ancestor', result.stderr)
        self.assertEqual(list(outside.iterdir()), [])
        values = dict(line.split('=', 1) for line in (self.path / 'env').read_text().splitlines())
        chosen = Path(values['TMPDIR']).parent
        fallback = Path(f'/tmp/fortemi-ci-runs-{os.geteuid()}')
        self.assertEqual(chosen.parent, fallback)
        record = next(r for r in reaper.owned_roots(str(fallback)) if r[4] == chosen.name)
        reaper.remove_root(str(fallback), record)
        self.assertFalse(chosen.exists())

    def test_job_cleanup_does_not_follow_static_symlink_ancestor(self):
        import yaml
        outside = self.path / 'outside'
        outside.mkdir()
        sentinel = outside / 'payload'
        sentinel.write_text('must survive')
        link = self.path / 'link'
        link.symlink_to(outside, target_is_directory=True)
        env = dict(self.env, CARGO_TARGET_DIR=str(link), CARGO_INSTALL_ROOT=str(link),
                   DOCKER_CONFIG=str(link), CI_SAFETY_TOOLS=str(link))
        count = 0
        for filename in ['ci-builder.yaml', 'test.yml']:
            jobs = yaml.safe_load((ROOT / '.gitea/workflows' / filename).read_text())['jobs']
            for job in jobs.values():
                for step in job.get('steps', []):
                    if step.get('name') in ['Cleanup run-local build and Docker state',
                                           'Cleanup admission tooling', 'Cleanup safety tooling']:
                        result = subprocess.run(['bash', '-e', '-c', step['run']], env=env,
                                                capture_output=True, text=True, timeout=10)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn('Retained', result.stdout)
                        self.assertTrue(sentinel.is_file())
                        count += 1
        self.assertEqual(count, 20)  # 17 job directories plus admission/final tooling notices

    def test_ci_temporary_cleanup_retains_every_path(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/ci/cleanup-local-temp.sh'), str(self.root)],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0)
        self.assertIn('Retained', result.stdout)
        self.assertTrue((self.root / 'sentinel').is_file())


class RoundFourTeardownTests(unittest.TestCase):
    def test_actual_always_teardown_with_no_wrapper_or_helper_never_removes_by_name(self):
        import sys
        import yaml
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            binary = path / 'bin'
            binary.mkdir()
            log = path / 'calls'
            docker = binary / 'docker'
            docker.write_text('#!' + sys.executable + '\n' + r'''import json,os,sys
from pathlib import Path
args=sys.argv[1:]
with open(os.environ['MOCK_DOCKER_LOG'],'a') as f:f.write(json.dumps(args)+'\n')
if 'inspect' not in args:sys.exit('Unverified CLI operation: '+repr(args))
print(json.dumps([{'Id':'alien-id','Config':{'Labels':{'fortemi.ci.scope':'88','fortemi.ci.attempt':'1'}},'Labels':{}}]))
''')
            docker.chmod(0o755)
            steps = []
            for filename in ['ci-builder.yaml', 'test.yml']:
                jobs = yaml.safe_load((ROOT / '.gitea/workflows' / filename).read_text())['jobs']
                for job in jobs.values():
                    if 'CI_SAFETY_TOOLS' not in job.get('env', {}):
                        continue
                    for step in job.get('steps', []):
                        if step.get('if') == 'always()' and 'owned-docker.py' in step.get('run', ''):
                            steps.append(step)
            self.assertGreaterEqual(len(steps), 10)
            steps.append({'name': 'Helper cleanup without wrapper',
                          'run': 'bash "$TEST_CLEANUP_ENTRY" rm -fv helper-container'})
            for tooling in [path / 'missing', ROOT]:
                env = dict(os.environ, PATH=f'{binary}:{os.environ["PATH"]}',
                           CI_SAFETY_TOOLS=str(tooling), CI_RESOURCE_SCOPE='99', CI_RESOURCE_ATTEMPT='2',
                           FORTEMI_BUILDX_BUILDER='fortemi-ci-99-2', MOCK_DOCKER_LOG=str(log),
                           TEST_CLEANUP_ENTRY=str(ROOT / 'scripts/ci/remove-run-docker.sh'))
                env.pop('CI_DOCKER_REAL', None)  # PATH contains the real CLI mock, no ownership wrapper.
                for step in steps:
                    result = subprocess.run(['bash', '-e', '-c', step['run']], env=env,
                                            capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, step['name'] + result.stderr)
                    self.assertIn('::warning::Kept', result.stdout)
                if tooling == path / 'missing':
                    self.assertFalse(log.exists())
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertTrue(calls)
            self.assertTrue(all(call[1] == 'inspect' for call in calls), calls)
            self.assertTrue(any(call[-1] == 'alien-id' for call in calls))


class WorkflowToolRecoveryTests(unittest.TestCase):
    date = staticmethod(ReconcilerTests.date)
    state = staticmethod(ReconcilerTests.state)
    docker = ReconcilerTests.docker
    execute = ReconcilerTests.execute
    # Reuse the daemon/API fixtures and add tooling to inventory.
    def setUp(self):
        ReconcilerTests.setUp(self)
        self.tools = Path(self.tmp.name) / 'tools'
        self.tools.mkdir()
        self.env_tools = patch.dict(os.environ, {'CI_TOOLS_ROOT': str(self.tools)})
        self.env_tools.start()
        self.addCleanup(self.env_tools.stop)
        for owner in ['11', '22', '33', '44']:
            root = self.tools / f'{owner}-1.dead-checkout'
            root.mkdir()
            (root / 'git-payload').write_text('partial checkout')
            info = root.stat()
            (root / '.owner.json').write_text(json.dumps({reaper.LABEL:owner, reaper.ATTEMPT:'1',
                'created':time.time()-(30 if owner == '33' else 4000),
                'identity':[info.st_dev,info.st_ino]}))

    def test_old_terminal_tool_checkouts_recover_but_active_young_unknown_are_retained(self):
        output = self.execute(lambda owner: self.state({'11':'in_progress','22':'cancelled','44':'waiting'}[owner]))
        self.assertFalse((self.tools / '22-1.dead-checkout').exists())
        for owner in ['11','33','44']:
            self.assertTrue((self.tools / f'{owner}-1.dead-checkout/git-payload').is_file())
        self.assertIn('Removed owned root 22-1.dead-checkout', output)

    def test_tool_api_failure_keeps_all_run_and_tool_resources(self):
        output = self.execute(lambda _: (_ for _ in ()).throw(ValueError('unavailable')))
        self.assertEqual(self.removed, [])
        self.assertIn('::warning::', output)
        self.assertTrue((self.tools / '22-1.dead-checkout/git-payload').is_file())

    def test_missing_or_mismatched_tool_identity_is_kept_and_reported(self):
        root = self.tools / '22-1.dead-checkout'
        path = root / '.owner.json'
        original = json.loads(path.read_text())
        for value in [{k:v for k,v in original.items() if k != 'identity'},
                      dict(original, identity=[original['identity'][0], 0])]:
            path.write_text(json.dumps(value))
            output = self.execute(lambda _: self.state())
            self.assertIn('Kept root 22-1.dead-checkout: unusable root identity/marker', output)
            self.assertTrue((root / 'git-payload').is_file())

    def test_crashed_tool_quarantine_recovered_on_next_sweep(self):
        real_unlink = reaper.os.unlink
        def interrupted(name, *args, **kwargs):
            if name == 'git-payload':
                raise OSError('interrupted')
            return real_unlink(name, *args, **kwargs)
        with patch.object(reaper.os, 'unlink', side_effect=interrupted):
            output = self.execute(lambda _: self.state())
        self.assertIn('Kept root 22-1.dead-checkout', output)
        self.assertFalse((self.tools / '22-1.dead-checkout').exists())
        output = self.execute(lambda _: self.state())
        self.assertIn('Removed owned quarantine', output)
        self.assertFalse(list((self.tools / reaper.QUARANTINE).iterdir()))


class CIWorkflowToolingTests(unittest.TestCase):
    setUp = WorkflowToolingTests.setUp
    fake = WorkflowToolingTests.fake
    run_step = WorkflowToolingTests.run_step
    bootstrap = WorkflowToolingTests.bootstrap
    admission = WorkflowToolingTests.admission

    def use_ci(self):
        import yaml
        self.jobs = yaml.safe_load((ROOT / '.gitea/workflows/ci-builder.yaml').read_text())['jobs']

    def test_historical_ci_dispatch_is_successfully_skipped_before_any_payload_work(self):
        self.use_ci()
        result = self.admission()
        self.assertEqual(Path(self.env['GITHUB_OUTPUT']).read_text(), 'current=false\n')
        self.assertIn('::notice::Superseded source ' + self.historical_sha, result.stdout)
        protected = {'runner-capacity','verify-release-ref','publish-release','publish-github',
                     'sign-release-images','verify-ghcr-release','finalize-releases'}
        for name, job in self.jobs.items():
            if name in protected | {'admit-source','cleanup-run-images'}:
                continue
            self.assertIn('admit-source', job['needs'], name)
            self.assertTrue(job['if'].startswith("needs.admit-source.outputs.current == 'true'"), name)
        for name in protected - {'runner-capacity','verify-release-ref'}:
            self.assertIn("startsWith(github.ref, 'refs/tags/v')", self.jobs[name]['if'])
        self.assertFalse(any('checkout' in call[2] and '-C' not in call[2]
                             for call in map(json.loads, (self.path/'io').read_text().splitlines())))
        marker = json.loads((Path(self.env['CI_SAFETY_TOOLS']) / '.owner.json').read_text())
        info = Path(self.env['CI_SAFETY_TOOLS']).stat()
        self.assertEqual(marker['identity'], [info.st_dev,info.st_ino])
        self.assertEqual(marker[reaper.LABEL], '99')

    def test_current_ci_dispatch_is_admitted(self):
        self.use_ci()
        self.env['CHAIN_SOURCE_SHA'] = self.workflow_sha
        self.admission()
        self.assertEqual(Path(self.env['GITHUB_OUTPUT']).read_text(), 'current=true\n')

    def test_ci_final_cleanup_uses_workflow_revision_even_without_payload_helpers(self):
        self.use_ci()
        WorkflowToolingTests.test_cleanup_uses_workflow_tooling_when_payload_has_no_helpers(self)

    def test_bootstrap_partial_fetch_failure_remains_registered_and_teardown_keeps(self):
        self.use_ci()
        self.fake('git', '#!/bin/sh\nexit 1\n')
        self.env['CI_SAFETY_TOOLS'] = str(self.path / 'tools/99-2')
        step = self.jobs['cleanup-run-images']['steps'][0]
        result = subprocess.run(['bash','-e','-c',step['run']], env=self.env,
                                cwd=self.payload,text=True,capture_output=True,timeout=10)
        self.assertNotEqual(result.returncode, 0)
        values = dict(line.split('=',1) for line in Path(self.env['GITHUB_ENV']).read_text().splitlines())
        self.env.update(values)
        tools = Path(values['CI_SAFETY_TOOLS'])
        marker = json.loads((tools / '.owner.json').read_text())
        self.assertEqual(marker['identity'], [tools.stat().st_dev, tools.stat().st_ino])
        step = next(s for s in self.jobs['cleanup-run-images']['steps'] if '--current' in s.get('run',''))
        result = self.run_step(step)
        self.assertIn('::warning::Kept', result.stdout)
        self.assertTrue(tools.is_dir())

    def test_tool_bootstrap_rejects_static_symlink_ancestor_before_git_writes(self):
        self.use_ci()
        outside = self.path / 'outside'
        outside.mkdir()
        sentinel = outside / 'sentinel'
        sentinel.write_text('keep')
        link = self.path / 'linked-tools'
        link.symlink_to(outside, target_is_directory=True)
        self.env['CI_SAFETY_TOOLS'] = str(link / '99-2')
        step = self.jobs['cleanup-run-images']['steps'][0]
        result = subprocess.run(['bash','-e','-c',step['run']], env=self.env,
                                cwd=self.payload,text=True,capture_output=True,timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(outside.iterdir()), [sentinel])
        self.assertFalse((self.path / 'io').exists())


class CleanupStaticTests(unittest.TestCase):
    def assert_cache_quarantine_policy(self, code):
        """Recognize one audited FD-recursive primitive, never whitelist a file.

        Keep the shell/rmtree prohibition unchanged. Pin the AST of the complete
        deletion/ownership/rename/lock/budget boundary so removing or reordering
        a guard requires explicit re-review. Comments/formatting do not matter.
        Behavior fixtures in test_cargo_cache exercise the pinned semantics.
        """
        import ast
        import hashlib
        audited = {'open_directory', 'node_identity', 'owned', 'verify_lock', 'locked',
                   'cache_marker', 'quarantine_directory', 'no_pending_quarantine',
                   'rename_noreplace', 'quarantine_evict', 'entries', 'prune'}
        constants = {'SCHEMA', 'MAX_BYTES', 'MAX_ENTRIES', 'QUARANTINE', 'DIRECTORY'}
        tree = ast.parse(code)
        selected = [node for node in tree.body if
                    isinstance(node, ast.FunctionDef) and node.name in audited or
                    isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id in constants for t in node.targets)]
        self.assertEqual({n.name for n in selected if isinstance(n, ast.FunctionDef)}, audited)
        payload = ast.dump(ast.Module(body=selected, type_ignores=[]), include_attributes=False)
        self.assertEqual(hashlib.sha256(payload.encode()).hexdigest(), '35b531562eb62de88cb44423816c28b1fbc7112e70f98b6616e306aba2438160')
        primitive = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'quarantine_evict')
        approved = {id(n) for n in ast.walk(primitive)}
        deletes = []
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            if isinstance(node.func, ast.Attribute) and node.func.attr in {'unlink', 'remove', 'rmdir', 'rmtree'}:
                self.assertIn(id(node), approved, 'deletion outside audited cache quarantine')
                self.assertIsInstance(node.func.value, ast.Name)
                self.assertEqual(node.func.value.id, 'os')
                self.assertIn(node.func.attr, {'unlink', 'rmdir'})
                self.assertTrue(any(k.arg == 'dir_fd' for k in node.keywords))
                deletes.append(node)
        self.assertEqual(len(deletes), 4)
        # The explicit recursive call must stay confined to the audited body.
        calls = [n for n in ast.walk(tree) if isinstance(n, ast.Call) and
                 isinstance(n.func, ast.Name) and n.func.id == 'clear_quarantined']
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(id(n) in approved for n in calls))

    def test_cache_quarantine_guard_mutations_and_extra_deletes_are_rejected(self):
        code = (ROOT / 'scripts/ci/cargo-cache.py').read_text()
        self.assert_cache_quarantine_policy(code)
        mutations = [
            code.replace('rename_noreplace(parent, name, pending, \'tree\')', 'pass'),
            code.replace('cache_marker(source) != record', 'False'),
            code.replace('owned(before, expected[0])', 'pass'),
            code.replace('\n    verify_lock(root, guard)\n', '\n    pass\n'),
            code.replace('dir_fd=directory', 'dir_fd=None'),
            code.replace('\n    no_pending_quarantine(root)\n', '\n    pass\n'),
            code + '\ndef unsafe(path):\n    os.unlink(path)\n',
            code + '\nclear_quarantined(123)\n',
        ]
        for number, mutation in enumerate(mutations):
            with self.subTest(mutation=number), self.assertRaises((AssertionError, SyntaxError)):
                self.assert_cache_quarantine_policy(mutation)

    def test_no_recursive_run_path_deletion_outside_quarantine(self):
        import re
        import yaml
        # All PR/main jobs that configure run paths, and their script closure.
        # Protected publisher/native-runner jobs retain their separate contract.
        pending = set()
        scanned = set()
        bootstrap = None
        for filename in ['ci-builder.yaml','test.yml']:
            jobs = yaml.safe_load((ROOT / '.gitea/workflows' / filename).read_text())['jobs']
            for name, job in jobs.items():
                if 'CI_SAFETY_TOOLS' not in job.get('env', {}):
                    continue
                self.assertEqual(job['env']['CI_WORKFLOW_SHA'], '${{ gitea.sha }}')
                initial = job['steps'][0]['run']
                if bootstrap is None:
                    bootstrap = initial
                self.assertEqual(initial, bootstrap, (filename, name))
                self.assertLess(initial.index("os.open('.owner.json'"), initial.index('git init -q'))
                for step in job['steps']:
                    code = step.get('run','')
                    self.assertNotRegex(code, r'\brm\s+(?:[^\n]*?\s)?(?:-[a-zA-Z]*[rR]|--recursive)\b|\brmtree\s*\(')
                    executable = re.sub(r'^.*(?:bash -n|python3 -m unittest|verify-mutsu-build-serialization|verify-pre-migration-backup-gate).*$', '', code, flags=re.M)
                    pending.update(re.findall(r'(?:scripts/ci/|scripts/)[\w/-]+\.(?:sh|py)',executable))
        while pending:
            name = pending.pop()
            if name in scanned or not (ROOT/name).is_file():
                continue
            scanned.add(name)
            code = (ROOT/name).read_text()
            if name == 'scripts/ci/cleanup-local-temp.sh':
                # The only local recursive delete is unreachable with CI ownership.
                self.assertIn('if [[ -n "${CI_RESOURCE_SCOPE:-}" ]]; then', code)
                self.assertIn('else\n  rm -rf -- "$@"\nfi', code)
                continue
            # The in-container apt cache is outside runner run directories.
            code = re.sub(r'^RUN apt-get.*$', '', code, flags=re.M)
            self.assertNotRegex(code, r'\brm\s+(?:[^\n]*?\s)?(?:-[a-zA-Z]*[rR]|--recursive)\b|\brmtree\s*\(',name)
            if name == 'scripts/ci/cargo-cache.py':
                self.assert_cache_quarantine_policy(code)
            if name.endswith('.sh'):
                pending.update(re.findall(r'(?:scripts/ci/|scripts/)[\w/-]+\.(?:sh|py)',code))
        self.assertIn('scripts/ci/smoke-hosted-image.sh', scanned)
        self.assertIn('scripts/test-keycloak-reference-realm.sh', scanned)
        self.assertIn('scripts/ci/cleanup-local-temp.sh', scanned)
        self.assertIn('scripts/ci/cargo-cache.py', scanned)

    def test_all_always_docker_teardown_is_explicit_and_never_falls_back_to_cli(self):
        import re
        import yaml
        for filename in ['ci-builder.yaml','test.yml']:
            jobs = yaml.safe_load((ROOT / '.gitea/workflows' / filename).read_text())['jobs']
            for name, job in jobs.items():
                if 'CI_SAFETY_TOOLS' not in job.get('env',{}):
                    continue
                for step in job['steps']:
                    if step.get('if') != 'always()':
                        continue
                    code = step.get('run','')
                    self.assertNotRegex(code, r'\bdocker\s+(?:rm|rmi|stop|(?:container|network|image|volume|buildx)\s+rm)',name)
                    if 'owned-docker.py' in code:
                        self.assertIn('if [[ -f "${CI_SAFETY_TOOLS}/scripts/ci/owned-docker.py" ]]', code)
                        self.assertIn('::warning::Kept', code)
        for name in ['ci/smoke-hosted-image.sh','ci/smoke-pre-migration-recovery-realpg.sh',
                     'ci/test-bundle-runtime-role.sh','ci/test-pod-security-restricted.sh',
                     'ci/test-redis.sh','ci/create-explicit-docker-network.sh',
                     'test-keycloak-reference-realm.sh','test-mcp-external-issuer.sh','test-otel-collector.sh']:
            code = (ROOT / 'scripts' / name).read_text()
            self.assertNotRegex(code, r'\bdocker\s+(?:rm|rmi|stop|(?:container|network|image|volume|buildx)\s+rm)',name)
            self.assertNotRegex(code, r'remove_docker[^\n]*>/dev/null',name)
            self.assertIn('remove-run-docker.sh', code, name)
        entry = (ROOT / 'scripts/ci/remove-run-docker.sh').read_text()
        self.assertIn('if [[ -n "${CI_RESOURCE_SCOPE:-}" ]]; then', entry)
        self.assertIn('python3 "$helper" "$@"', entry)
        self.assertIn('::warning::Kept', entry)


if __name__ == '__main__':
    unittest.main()
