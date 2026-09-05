import fs from 'node:fs';
// Synthetic inputs test validators only. They are not approved authority instances.
const schema = JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-qualification/1.0.0/schemas/authority.schema.json', import.meta.url)));
const digest = `sha256:${'a'.repeat(64)}`;
const component = name => ({ name, repository: 'test/fixture', revision: 'a'.repeat(40) });
export function authority(type = 'tenant') {
  const tuple = Object.fromEntries(Object.keys(schema.properties.contractTuple.properties).map(k => [k, digest]));
  Object.assign(tuple, { applicationRelease: 'test-only', mcpProtocol: 'test-only', materializationProfile: 'test-only',
    knowledgeShardManifestVersion: 'not-applicable', knowledgeShardProfile: 'not-applicable' });
  const conditions = schema.allOf.find(c => c.if.properties.qualificationType?.const === type).then.properties.thresholds.allOf;
  const metrics = conditions.map(c => schema.$defs[c.contains.$ref.split('/').pop()].properties.metric.const);
  if (!metrics.includes('cleanupOutOfScope')) metrics.push('cleanupOutOfScope');
  return { schemaVersion: '1.0.0', authorityId: 'test-only', qualificationType: type,
    validFrom: '2026-01-01T00:00:00Z', validUntil: '2026-01-02T00:00:00Z', contractTuple: tuple,
    producers: [component('producer')], consumers: [component('consumer')],
    environment: { database: 'test', blobBackend: 'test', queue: 'test', indexBackend: 'test', filesystem: 'test', architecture: 'test',
      deploymentMode: 'docker', restartPolicy: 'test', networkPolicy: 'test',
      resourceLimits: { cpuCores: 1, memoryBytes: 1, diskBytes: 1, walBytes: 0, durationSeconds: 1, concurrency: 1, providerCalls: 0 },
      providerCostCeiling: { currency: 'USD', maxCost: 0 }, cleanDestinationProvenance: digest },
    thresholds: metrics.map(metric => ({ metric, operator: metric.startsWith('rpo') || metric.startsWith('rto') ? 'lte' : 'eq',
      limit: metric.endsWith('PassRate') || metric.endsWith('RejectRate') ? 1 : 0,
      unit: metric.endsWith('Seconds') ? 'seconds' : metric.endsWith('Rate') && metric !== 'errorRate' ? 'ratio' : 'count', approverRole: 'test-only' })),
    approvals: [digest], fixtureDigests: [digest], verifier: { ...component('verifier'), imageDigest: digest, trustModel: 'separate-implementation-read-only-evidence' } };
}

