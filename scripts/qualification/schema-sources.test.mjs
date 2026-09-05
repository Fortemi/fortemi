import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import test from 'node:test';

test('candidate schema snapshot matches its source receipt', () => {
  const root = new URL('../../contracts/dataset-qualification/1.0.0/', import.meta.url);
  const manifest = JSON.parse(fs.readFileSync(new URL('schema-sources.json', root)));
  assert.equal(manifest.status, 'candidate-not-approved');
  assert.deepEqual(manifest.files.map(f => f.path).sort(), ['schemas/authority.schema.json', 'schemas/receipt.schema.json']);
  for (const entry of manifest.files) {
    const bytes = fs.readFileSync(new URL(entry.path, root));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.sha256, entry.path);
  }
});

test('v2 candidate schema bytes match immutable version manifest', () => {
  const root = new URL('../../contracts/dataset-qualification/2.0.0/', import.meta.url);
  const manifest = JSON.parse(fs.readFileSync(new URL('manifest.json', root)));
  assert.equal(manifest.status, 'candidate-not-approved');
  assert.equal(manifest.version, '2.0.0');
  assert.deepEqual(manifest.files.map(f => f.path).sort(), ['schemas/authority.schema.json', 'schemas/receipt.schema.json']);
  for (const entry of manifest.files) {
    assert.equal(createHash('sha256').update(fs.readFileSync(new URL(entry.path, root))).digest('hex'), entry.sha256);
  }
});
