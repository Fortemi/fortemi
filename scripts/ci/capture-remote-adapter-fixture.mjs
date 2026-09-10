import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash, randomUUID } from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'

const [baseArgument, container, outputArgument] = process.argv.slice(2)
assert.ok(baseArgument && container && outputArgument, 'usage: capture-remote-adapter-fixture.mjs <loopback-url> <lane-container> <output.json>')
const base = new URL(baseArgument)
assert.equal(base.hostname, '127.0.0.1')
const image = 'ghcr.io/fortemi/fortemi@sha256:f24c621176dd17ff69896a78c3c9ba0f96f6b0a7ed15024dfb42343401d99cb1'
const identity = JSON.parse(execFileSync('docker', ['inspect', '--format', '{{json .}}', container], { encoding: 'utf8' }))
assert.equal(identity.Config.Labels['fortemi.lane'], 'b')
assert.equal(identity.Config.Labels['fortemi.task'], 'react-417-421')
assert.equal(identity.Config.Image, image)
assert.equal(identity.State.Running, true)
assert.ok(identity.NetworkSettings.Ports['3000/tcp'].some((binding) => binding.HostIp === '127.0.0.1' && binding.HostPort === base.port))
assert.equal(execFileSync('docker', ['exec', '--user', 'postgres', container, 'psql', '-d', 'matric', '-Atc', 'SELECT count(*) FROM note'], { encoding: 'utf8' }).trim(), '0', 'destination must contain no native note rows, including deleted rows')

const cases = {}
const owned = []
async function request(name, method, path, body, expected = 200) {
  const response = await fetch(new URL(path, base), {
    method, headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(30000),
  })
  const text = await response.text()
  let value = null
  if (text) {
    try { value = JSON.parse(text) } catch { value = text }
  }
  cases[name] = { request: { method, path, ...(body ? { body } : {}) }, response: {
    status: response.status, contentType: response.headers.get('content-type'), body: value,
  } }
  assert.equal(response.status, expected, `${name} status`)
  return value
}

const health = await request('health', 'GET', '/health')
assert.equal(health.git_sha, 'e91c595a896275f835cb7ed1aef173cb26056206')
assert.equal((await request('list_empty', 'GET', '/api/v1/notes?limit=10')).total, 0, 'destination must be empty')
delete cases.health
try {
  for (const name of ['first', 'second']) {
    const result = await request(`create_${name}`, 'POST', '/api/v1/notes', {
      content: `REMOTE CONTRACT NEEDLE ${name}`, title: `Remote fixture ${name}`,
      tags: ['lane-b-remote', name], revision_mode: 'none', pipeline: [], source: 'remote-contract-fixture',
    }, 201)
    assert.match(result.id, /^[0-9a-f-]{36}$/)
    owned.push(result.id)
  }
  const [first, second] = owned
  await request('links_empty', 'GET', `/api/v1/notes/${first}/links`)
  await request('link_create', 'POST', `/api/v1/notes/${first}/links`, { to_note_id: second, kind: 'explicit', score: 0.75 }, 201)
  await request('list_nonempty', 'GET', '/api/v1/notes?limit=10&offset=0')
  await request('detail_first', 'GET', `/api/v1/notes/${first}`)
  await request('detail_second', 'GET', `/api/v1/notes/${second}`)
  await request('links_outgoing', 'GET', `/api/v1/notes/${first}/links`)
  await request('links_incoming', 'GET', `/api/v1/notes/${second}/links`)
  await request('concepts_nonempty', 'GET', `/api/v1/notes/${first}/concepts`)
  await request('provenance_empty', 'GET', `/api/v1/notes/${first}/provenance`)

  const revision = randomUUID()
  execFileSync('docker', ['exec', '-i', '--user', 'postgres', container, 'psql', '-v', 'ON_ERROR_STOP=1', '-d', 'matric'], {
    encoding: 'utf8', input: `BEGIN;
SELECT set_config('app.current_tenant', (SELECT tenant_id::text FROM note WHERE id = '${first}'), true);
INSERT INTO note_revision (id, note_id, revision_number, content, created_at_utc)
VALUES ('${revision}', '${first}', 1, 'Synthetic provenance content', '2026-09-10T00:00:00Z');
INSERT INTO provenance_activity (id, note_id, revision_id, activity_type, model_name, started_at, ended_at, metadata)
VALUES ('${randomUUID()}', '${first}', '${revision}', 'ai_revision', 'synthetic-fixture-model', '2026-09-10T00:00:00Z', '2026-09-10T00:00:01Z', '{"synthetic":true}');
INSERT INTO provenance_edge (id, revision_id, source_note_id, relation, created_at_utc)
VALUES ('${randomUUID()}', '${revision}', '${second}', 'wasDerivedFrom', '2026-09-10T00:00:01Z');
UPDATE note_revised_current SET last_revision_id = '${revision}', content = 'Synthetic current revised content' WHERE note_id = '${first}';
COMMIT;
`, stdio: ['pipe', 'pipe', 'pipe'],
  })
  const graph = await request('provenance_nonempty', 'GET', `/api/v1/notes/${first}/provenance`)
  assert.equal(graph.all_activities.length, 1)
  assert.equal(graph.all_edges.length, 1)
  await request('detail_revised', 'GET', `/api/v1/notes/${first}`)
  await request('search_nonempty', 'GET', '/api/v1/search?q=NEEDLE&mode=fts&limit=10')
  await request('search_empty', 'GET', '/api/v1/search?q=NO_MATCH_TERM_1146&mode=fts&limit=10')
  await request('search_tags', 'GET', '/api/v1/search?q=NEEDLE&mode=fts&limit=10&tags=lane-b-remote%2Csecond')
  await request('search_missing_q', 'GET', '/api/v1/search?query=NEEDLE', undefined, 400)
  await request('not_found', 'GET', `/api/v1/notes/${randomUUID()}`, undefined, 404)
  await request('star', 'PATCH', `/api/v1/notes/${second}`, { starred: true })
  await request('detail_starred', 'GET', `/api/v1/notes/${second}`)
  await request('unstar', 'PATCH', `/api/v1/notes/${second}`, { starred: false })
  await request('search_limit', 'GET', '/api/v1/search?q=NEEDLE&mode=fts&limit=1')
  await request('search_tags_no_match', 'GET', '/api/v1/search?q=NEEDLE&mode=fts&limit=10&tags=first%2Csecond')
  for (const mode of ['semantic', 'hybrid']) {
    const result = await request(`search_${mode}_degraded`, 'GET', `/api/v1/search?q=NEEDLE&mode=${mode}&limit=10`)
    assert.equal(result.degraded, true, 'unavailable inference must be reported as degraded')
    assert.equal(result.degradation.effective_mode, 'fts')
  }
  await request('update_content', 'PATCH', `/api/v1/notes/${second}`, { content: 'REMOTE CONTRACT UPDATED', revision_mode: 'none' })
  await request('update_tags', 'PATCH', `/api/v1/notes/${second}`, { tags: ['lane-b-remote', 'updated'] })
  await request('archive', 'PATCH', `/api/v1/notes/${second}`, { archived: true })
  await request('unarchive', 'PATCH', `/api/v1/notes/${second}`, { archived: false })
  await request('delete', 'DELETE', `/api/v1/notes/${second}`, undefined, 204)
  await request('deleted_not_found', 'GET', `/api/v1/notes/${second}`, undefined, 404)
  const restored = await request('restore', 'POST', `/api/v1/notes/${second}/restore?revision_mode=none`)
  assert.equal(restored.restored, true)
  assert.equal(restored.id, second)
  await request('detail_restored', 'GET', `/api/v1/notes/${second}`)
} finally {
  for (const [index, id] of owned.entries()) await request(`cleanup_delete_${index}`, 'DELETE', `/api/v1/notes/${id}`, undefined, 204)
}
assert.equal((await request('cleanup_list', 'GET', '/api/v1/notes?limit=10')).total, 0)
const fixture = {
  schemaVersion: 'fortemi.remote-adapter-fixture.v1',
  producer: { image, commit: health.git_sha, version: health.version },
  boundary: 'Actual HTTP responses from a disposable Linux AMD64 server with synthetic records. Provenance rows are seeded, not generated by inference. No auth or released React qualification.',
  cases,
  cleanup: { ownedNoteIds: owned, deleted: owned.length, remainingVisibleNotes: 0, containerRemovalRequired: true },
}
const bytes = `${JSON.stringify(fixture, null, 2)}\n`
const output = resolve(outputArgument)
mkdirSync(dirname(output), { recursive: true })
writeFileSync(output, bytes)
console.log(JSON.stringify({ output, sha256: createHash('sha256').update(bytes).digest('hex'), cases: Object.keys(cases).length }))
