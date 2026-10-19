"""Guard post-merge CI triggers and the non-publishing release validation path."""

import json
from pathlib import Path
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / '.gitea/workflows'
RELEASE_JOBS = {
    'publish-release': "startsWith(github.ref, 'refs/tags/v')",
    'publish-github': "startsWith(github.ref, 'refs/tags/v')",
    'sign-release-images': "startsWith(github.ref, 'refs/tags/v') && needs.publish-release.result == 'success' && needs.publish-github.result == 'success'",
    'verify-ghcr-release': "startsWith(github.ref, 'refs/tags/v') && needs.publish-github.result == 'success'",
    'finalize-releases': "startsWith(github.ref, 'refs/tags/v') && needs.publish-release.result == 'success' && needs.publish-github.result == 'success' && needs.verify-ghcr-release.result == 'success' && needs.sign-release-images.result == 'success'",
}


def load(name):
    # BaseLoader preserves YAML 1.2's workflow key 'on' as a string, unlike
    # SafeLoader's YAML 1.1 boolean conversion. Values are strings too.
    return yaml.load((WORKFLOWS / name).read_text(), Loader=yaml.BaseLoader)


class ChangeScopedCITests(unittest.TestCase):
    def test_no_workflow_has_a_pull_request_trigger(self):
        paths = sorted(list(WORKFLOWS.glob('*.yml')) + list(WORKFLOWS.glob('*.yaml')))
        self.assertTrue(paths)
        for path in paths:
            with self.subTest(workflow=path.name):
                events = load(path.name)['on']
                if isinstance(events, str):
                    events = [events]
                self.assertNotIn('pull_request', events)
                self.assertNotIn('pull_request_target', events)

    def test_main_push_and_dispatch_triggers(self):
        events = load('ci-builder.yaml')['on']
        self.assertEqual(events['push']['branches'], ['main'])
        self.assertIn('workflow_dispatch', events)
        self.assertEqual(set(load('test.yml')['on']), {'workflow_dispatch'})

    def test_docsite_build_is_dispatch_only_with_strict_links(self):
        # A push trigger would bypass the capacity-one dispatch chain enforced by
        # scripts/ci/verify-mutsu-build-serialization.sh; release validation and
        # local docs checks dispatch or run it instead.
        self.assertEqual(set(load('docsite-build.yml')['on']), {'workflow_dispatch'})
        tenant = json.loads((ROOT / 'tenants.json').read_text())['tenants'][0]
        self.assertEqual(tenant['source']['path'], './docs')
        self.assertTrue(tenant['strictLinks'])

    def test_publication_signing_and_finalization_stay_strictly_tag_only(self):
        jobs = load('ci-builder.yaml')['jobs']
        for name, guard in RELEASE_JOBS.items():
            with self.subTest(job=name):
                # Exact guard equality rejects an added OR dispatch escape.
                self.assertEqual(jobs[name]['if'], guard)

    def test_release_dependencies_survive_without_pr_trigger(self):
        jobs = load('ci-builder.yaml')['jobs']
        required = {
            'publish-release': {'verify-release-ref', 'sync-github-source',
                                'integration-test', 'test-container', 'audit', 'deny',
                                'auth-consumer', 'knowledge-shard-matrix', 'helm-chart',
                                'hosted-image-smoke', 'pod-security-restricted',
                                'mcp-lockfile-sync', 'mcp-server-tests'},
            'publish-github': {'verify-release-ref', 'publish-release'},
            'sign-release-images': {'publish-release', 'publish-github'},
            'verify-ghcr-release': {'publish-github'},
            'finalize-releases': {'publish-release', 'publish-github',
                                  'verify-ghcr-release', 'sign-release-images'},
        }
        for job, dependencies in required.items():
            with self.subTest(job=job):
                self.assertTrue(dependencies <= set(jobs[job]['needs']))
        finalizer = '\n'.join(s.get('run', '') for s in jobs['finalize-releases']['steps'])
        self.assertIn('ci/vault-fetch.gitea-release.spec', finalizer)
        self.assertIn('https://api.github.com/repos/Fortemi/fortemi/releases', finalizer)

    def test_main_dispatch_runs_validation_and_comprehensive_handoff(self):
        # Release validation is a main dispatch; no dispatch input may add a
        # publication path, and the handoff must run every test category.
        workflow = load('ci-builder.yaml')
        self.assertEqual(set(workflow['on']['workflow_dispatch']['inputs']), {'source_sha'})
        jobs = workflow['jobs']
        self.assertIn("github.ref == 'refs/heads/main'", jobs['main-validation']['if'])
        self.assertIn("needs.main-validation.result == 'success'", jobs['handoff-tests']['if'])
        handoff = '\n'.join(s.get('run', '') for s in jobs['handoff-tests']['steps'])
        self.assertIn('test_category: "all", source_sha: $source_sha', handoff)

if __name__ == '__main__':
    unittest.main()
