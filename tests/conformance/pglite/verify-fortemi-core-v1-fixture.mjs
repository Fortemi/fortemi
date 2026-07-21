#!/usr/bin/env node

import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import {
  MigrationRunner,
  allMigrations,
  createPGliteInstance,
  exportShardWithReport,
  importShard,
  unpackTarGz,
  validateCoreV1ShardArchive,
} from '@fortemi/core'

const fixture = resolve(
  process.argv[2] ?? '../../fixtures/shards/fortemi-core-v1-2026.7.1.shard',
)

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

function normalizeValue(value) {
  if (Array.isArray(value)) return value.map(normalizeValue)
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [key, normalizeValue(nested)]),
    )
  }
  if (typeof value === 'string') return value.replace(/\.000Z$/, 'Z')
  return value
}

function componentRecords(files, component) {
  const filename = {
    collections: 'collections.json',
    links: 'links.jsonl',
    notes: 'notes.jsonl',
    tags: 'tags.json',
    templates: 'templates.json',
  }[component]
  const text = new TextDecoder().decode(files.get(filename))
  const records = filename.endsWith('.jsonl')
    ? text.trim().split('\n').filter(Boolean).map((line) => JSON.parse(line))
    : JSON.parse(text)
  const identity = component === 'tags' ? 'name' : 'id'
  return normalizeValue(records).sort((left, right) =>
    left[identity].localeCompare(right[identity]),
  )
}

async function createDatabase() {
  const db = await createPGliteInstance('memory')
  await new MigrationRunner(db).apply(allMigrations)
  return db
}

async function main() {
  const archive = new Uint8Array(await readFile(fixture))
  const sourceFiles = unpackTarGz(archive)
  assert.deepEqual(
    await validateCoreV1ShardArchive(sourceFiles),
    { valid: true, errors: [] },
  )

  const destination = await createDatabase()
  try {
    const rejected = await importShard(
      destination,
      new Uint8Array([0x6e, 0x6f, 0x74, 0x2d, 0x61, 0x2d, 0x73, 0x68, 0x61, 0x72, 0x64]),
      { conflictStrategy: 'replace' },
    )
    assert.equal(rejected.success, false)
    const afterRejected = await destination.query(`
      SELECT
        (SELECT COUNT(*)::int FROM collection) AS collections,
        (SELECT COUNT(*)::int FROM note) AS notes,
        (SELECT COUNT(*)::int FROM template) AS templates,
        (SELECT COUNT(*)::int FROM link) AS links
    `)
    assert.deepEqual(afterRejected.rows[0], {
      collections: 0,
      notes: 0,
      templates: 0,
      links: 0,
    })

    const imported = await importShard(destination, archive, {
      conflictStrategy: 'replace',
    })
    assert.equal(imported.success, true, imported.errors.join('; '))
    assert.deepEqual(
      {
        collections: imported.counts.collections,
        notes: imported.counts.notes,
        templates: imported.counts.templates,
        links: imported.counts.links,
      },
      { collections: 2, notes: 2, templates: 1, links: 1 },
    )

    const semantic = await destination.query(`
      SELECT
        (SELECT parent_id FROM collection
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e779') AS parent_id,
        (SELECT to_char(
           deleted_at AT TIME ZONE 'UTC',
           'YYYY-MM-DD"T"HH24:MI:SS"Z"'
         ) FROM note
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e778') AS deleted_at,
        (SELECT ai_metadata FROM note_revised_current
         WHERE note_id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e77a') AS metadata,
        (SELECT content FROM note_revised_current
         WHERE note_id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e77a') AS content,
        (SELECT confidence FROM link
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e77e') AS score
    `)
    assert.deepEqual(semantic.rows[0], {
      parent_id: '018f2d2d-bc00-7cc8-8ad2-f147d6a2e77b',
      deleted_at: '2026-07-17T12:30:00Z',
      metadata: null,
      content: 'Revised fixture content',
      score: 1,
    })

    const reexported = await exportShardWithReport(destination, { profile: 'core-v1' })
    assert.equal(reexported.success, true, reexported.errors.join('; '))
    assert.ok(reexported.archive)
    assert.equal(reexported.capability_report.portable, true)
    const reexportedFiles = unpackTarGz(reexported.archive)
    assert.deepEqual(await validateCoreV1ShardArchive(reexportedFiles), {
      valid: true,
      errors: [],
    })

    const reexportedManifest = JSON.parse(
      new TextDecoder().decode(reexportedFiles.get('manifest.json')),
    )
    assert.deepEqual(reexportedManifest.counts, {
      notes: 2,
      collections: 2,
      tags: 1,
      templates: 1,
      links: 1,
      embedding_sets: 0,
      embedding_set_members: 0,
      embeddings: 0,
      embedding_configs: 0,
    })
    for (const component of ['collections', 'notes', 'tags', 'templates', 'links']) {
      assert.deepEqual(
        componentRecords(reexportedFiles, component),
        componentRecords(sourceFiles, component),
        `${component} changed across the PGlite semantic re-export`,
      )
    }

    process.stdout.write(`${sha256(archive)}  ${fixture}\n`)
  } finally {
    await destination.close()
  }
}

await main()
