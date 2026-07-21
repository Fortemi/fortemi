#!/usr/bin/env node

import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import {
  MigrationRunner,
  allMigrations,
  createPGliteInstance,
  exportShardWithReport,
  importShard,
  packTarGz,
  unpackTarGz,
  validateCoreV1ShardArchive,
} from '@fortemi/core'

const FIXED_NOW = Date.parse('2026-07-21T12:00:00Z')
const output = resolve(process.argv[2] ?? '../../fixtures/shards/pglite-core-v1-2026.7.11.shard')
const verify = process.argv.includes('--verify')
const legacyRoot = resolve('../../fixtures/shards/core-v1-valid')
const decoder = new TextDecoder()

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

async function createDatabase() {
  const db = await createPGliteInstance('memory')
  await new MigrationRunner(db).apply(allMigrations)
  return db
}

async function legacyCoreV1Archive() {
  const files = new Map()
  for (const name of [
    'notes.jsonl',
    'collections.json',
    'tags.json',
    'templates.json',
    'links.jsonl',
    'manifest.json',
  ]) {
    files.set(name, new Uint8Array(await readFile(resolve(legacyRoot, name))))
  }
  return packTarGz(files)
}

async function seed(db) {
  await db.exec(`
    INSERT INTO collection
      (id, name, description, parent_id, position, created_at, updated_at, deleted_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e701', 'Parent', NULL, NULL, 0,
       '2026-07-21T10:00:00Z', '2026-07-21T10:00:00Z', NULL),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e702', 'Child', 'Nested collection',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e701', 1,
       '2026-07-21T10:01:00Z', '2026-07-21T10:02:00Z', NULL);

    INSERT INTO note
      (id, title, format, source, is_starred, is_archived,
       created_at, updated_at, deleted_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e711', 'Published package note',
       'markdown', 'pglite-conformance', true, false,
       '2026-07-21T10:10:00Z', '2026-07-21T10:11:00Z', NULL),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e712', 'Published package tombstone',
       'markdown', 'pglite-conformance', false, true,
       '2026-07-21T10:12:00Z', '2026-07-21T10:14:00Z',
       '2026-07-21T10:14:00Z');

    INSERT INTO note_original (id, note_id, content, content_hash, created_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e721',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e711',
       '# Original', 'fixture-original-active', '2026-07-21T10:10:00Z'),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e722',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e712',
       '# Deleted original', 'fixture-original-deleted', '2026-07-21T10:12:00Z');

    INSERT INTO note_revised_current
      (note_id, content, ai_metadata, generation_count, model,
       is_user_edited, updated_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e711', '# Revised',
       '{"conformance":{"producer":"@fortemi/core","version":"2026.7.11"}}',
       1, 'fixture', false, '2026-07-21T10:11:00Z'),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e712', NULL, NULL,
       0, NULL, false, '2026-07-21T10:14:00Z');

    INSERT INTO collection_note (collection_id, note_id, position, added_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e702',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e711', 0, '2026-07-21T10:10:30Z'),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e701',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e712', 0, '2026-07-21T10:12:30Z');

    INSERT INTO note_tag (id, note_id, tag, created_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e731',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e711', 'portable', '2026-07-21T10:10:00Z'),
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e732',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e712', 'tombstone', '2026-07-21T10:12:00Z');

    INSERT INTO template
      (id, name, description, content, format, default_tags,
       collection_id, created_at, updated_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e741', 'Published package template',
       NULL, '# Template', 'markdown', '["portable"]',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e701',
       '2026-07-21T10:20:00Z', '2026-07-21T10:21:00Z');

    INSERT INTO link
      (id, source_note_id, target_note_id, link_type, confidence,
       created_at, updated_at, deleted_at)
    VALUES
      ('018f2d2d-bc00-7cc8-8ad2-f147d6a2e751',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e711',
       '018f2d2d-bc00-7cc8-8ad2-f147d6a2e712',
       'related', NULL, '2026-07-21T10:30:00Z',
       '2026-07-21T10:31:00Z', NULL);
  `)
}

async function main() {
  const NativeDate = Date
  class FrozenDate extends NativeDate {
    constructor(...args) {
      if (args.length === 0) {
        super(FIXED_NOW)
      } else {
        super(...args)
      }
    }

    static now() {
      return FIXED_NOW
    }
  }
  globalThis.Date = FrozenDate
  let source
  let destination
  try {
    source = await createDatabase()
    await seed(source)
    const exported = await exportShardWithReport(source, { profile: 'core-v1' })
    assert.equal(exported.success, true, exported.errors.join('; '))
    assert.ok(exported.archive)
    assert.equal(exported.capability_report.portable, true)
    assert.deepEqual(exported.capability_report.losses, [
      {
        code: 'null-revision-normalized',
        component: 'notes',
        count: 1,
        message: '1 null revised-content value(s) were normalized to original content as required by core-v1.',
      },
    ])

    const files = unpackTarGz(exported.archive)
    assert.deepEqual(await validateCoreV1ShardArchive(files), { valid: true, errors: [] })
    const manifest = JSON.parse(decoder.decode(files.get('manifest.json')))
    assert.deepEqual(manifest.counts, {
      notes: 2,
      collections: 2,
      tags: 2,
      templates: 1,
      links: 1,
      embedding_sets: 0,
      embedding_set_members: 0,
      embeddings: 0,
      embedding_configs: 0,
    })

    destination = await createDatabase()
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

    const imported = await importShard(destination, exported.archive, {
      conflictStrategy: 'replace',
    })
    assert.equal(imported.success, true, imported.errors.join('; '))
    const semantic = await destination.query(`
      SELECT
        (SELECT parent_id FROM collection
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e702') AS parent_id,
        (SELECT to_char(
           deleted_at AT TIME ZONE 'UTC',
           'YYYY-MM-DD"T"HH24:MI:SS"Z"'
         ) FROM note
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e712') AS deleted_at,
        (SELECT confidence FROM link
         WHERE id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e751') AS confidence,
        (SELECT ai_metadata FROM note_revised_current
         WHERE note_id = '018f2d2d-bc00-7cc8-8ad2-f147d6a2e711') AS metadata
    `)
    assert.equal(semantic.rows[0].parent_id, '018f2d2d-bc00-7cc8-8ad2-f147d6a2e701')
    assert.equal(semantic.rows[0].deleted_at, '2026-07-21T10:14:00Z')
    assert.equal(semantic.rows[0].confidence, null)
    assert.deepEqual(semantic.rows[0].metadata, {
      conformance: { producer: '@fortemi/core', version: '2026.7.11' },
    })

    const reexported = await exportShardWithReport(destination, { profile: 'core-v1' })
    assert.equal(reexported.success, true, reexported.errors.join('; '))
    assert.ok(reexported.archive)
    assert.deepEqual(
      await validateCoreV1ShardArchive(unpackTarGz(reexported.archive)),
      { valid: true, errors: [] },
    )

    const legacyDestination = await createDatabase()
    try {
      const legacy = await importShard(legacyDestination, await legacyCoreV1Archive(), {
        conflictStrategy: 'replace',
      })
      assert.equal(legacy.success, true, legacy.errors.join('; '))
      assert.equal(legacy.capability_report.requested_profile, 'core-v1')
    } finally {
      await legacyDestination.close()
    }

    if (verify) {
      const expected = await readFile(output)
      assert.equal(
        Buffer.compare(Buffer.from(exported.archive), expected),
        0,
        `fixture drift: expected ${sha256(expected)}, got ${sha256(exported.archive)}`,
      )
    } else {
      await writeFile(output, exported.archive)
    }
    process.stdout.write(`${sha256(exported.archive)}  ${output}\n`)
  } finally {
    await destination?.close()
    await source?.close()
    globalThis.Date = NativeDate
  }
}

await main()
