import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, realpathSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { issuerRequest } from './fixture-issuer-client.mjs';

const [root, phase] = process.argv.slice(2);
assert.ok(['initial', 'stale', 'archived', 'deleted', 'purged', 'jwt', 'tenant-suspended', 'tenant-soft_deleted', 'tenant-active'].includes(phase));
const packageRoot = realpathSync(process.env.FORTEMI_INSTALLED_CORE_ROOT);
assert.match(packageRoot, /^\/tmp\/fortemi-core-http-[A-Za-z0-9]+\/node_modules\/@fortemi\/core$/);
const entry = packageRoot + '/dist/index.js';
const manifest = JSON.parse(readFileSync(packageRoot + '/package.json'));
assert.equal(manifest.name, '@fortemi/core');
assert.equal(manifest.exports['.'].import, './dist/index.js');
assert.equal(createHash('sha256').update(readFileSync(entry)).digest('hex'), process.env.FORTEMI_INSTALLED_CORE_SHA256);
const core = await import(pathToFileURL(entry).href);
const fixture = JSON.parse(readFileSync(root + '/fixture.json'));
for (const url of [fixture.url, fixture.deniedUrl, fixture.untrustedUrl, fixture.coldUrl].filter(Boolean)) {
  const parsed = new URL(url);
  assert.equal(parsed.protocol, 'http:'); assert.equal(parsed.hostname, '127.0.0.1');
  assert.ok(parsed.port); assert.equal(parsed.pathname, '/');
}
const checks = [];
const check = async (name, operation) => {
  try { await operation(); checks.push(name); }
  catch (error) { console.error('Failed fixture check: ' + name); throw error; }
};
const jwt = !!process.env.FORTEMI_TEST_ISSUER;
const tokens = new Map();
if (jwt) {
  for (const f of fixture.fixtures) if (!tokens.has(f.token)) tokens.set(f.token,(await issuerRequest('/fixture-token',{tenant:f.tenant})).token);
  tokens.set('fixture-denied',(await issuerRequest('/fixture-token',{tenant:fixture.fixtures[0].tenant,scope:'mcp'})).token);
}
const backend = (f, extra = {}) => {
  const config = {baseUrl:fixture.url,authToken:f.token,headers:{'X-Fortemi-Memory':f.memory},...extra};
  config.authToken=tokens.get(config.authToken) ?? config.authToken;
  return core.createRemoteBackend(config);
};
const options = { limit: 1, tags: ['http-fixture-only'] };
const saved = phase === 'initial' ? [] : JSON.parse(readFileSync(root + '/locators.json'));
if (phase === 'jwt') {
  assert.ok(jwt && fixture.untrustedUrl && fixture.coldUrl);
  const f=fixture.fixtures[0];const locator=saved[0].current;
  const resolve=token=>backend(f,{authToken:token}).resolveEvidence(locator);
  const failure = async (token,status) => {
    await assert.rejects(resolve(token),error=>{
      assert.equal(error.kind,'http');assert.equal(error.status,status);
      assert.ok(!JSON.stringify(error).includes(token));return true;
    });
    for(const path of ['/api/v1/search?q=needle&mode=fts','/api/v1/notes/'+f.note]) {
      const response=await fetch(fixture.url+path,{headers:{authorization:'Bearer '+token,'X-Fortemi-Memory':f.memory},signal:AbortSignal.timeout(5000),redirect:'error'});
      assert.equal(response.status,status);await response.body.cancel();
    }
  };
  for(const [kind,status] of [
    ['wrong-issuer',401],['wrong-audience',401],['expired',401],['future-iat',401],['future-nbf',401],
    ['wrong-signature',401],['unknown-kid',401],['none',401],['hs256',401],['missing-kid',401],['duplicate-alg',401],
    ['missing-tenant',403],['invalid-tenant',403],['numeric-tenant',403],['unknown-tenant',403],['missing-scope',403],
  ]) await check('jwt:'+kind+':three-routes',async()=>failure((await issuerRequest('/fixture-token',{tenant:f.tenant,kind})).token,status));
  await check('tls:untrusted-ca',()=>assert.rejects(backend(f,{baseUrl:fixture.untrustedUrl}).resolveEvidence(locator),{kind:'http',status:503}));
  const valid=tokens.get(f.token);
  await check('jwks:cold-cache-outage-and-recovery',async()=>{
    const before=await issuerRequest('/fixture-mode',{mode:'jwks-down'});
    const cold=backend(f,{baseUrl:fixture.coldUrl});
    await assert.rejects(cold.resolveEvidence(locator),{kind:'http',status:503});
    const failed=await issuerRequest('/fixture-mode',{mode:'normal'});
    assert.equal(failed.counts.jwks,before.counts.jwks+1);
    assert.equal(await cold.resolveEvidence(locator),fixture.text);
    const recovered=await issuerRequest('/fixture-mode',{mode:'normal'});
    assert.equal(recovered.counts.jwks,failed.counts.jwks+1);
  });
  await check('jwks:warm-cache-survives-key-endpoint-outage',async()=>{
    const before=await issuerRequest('/fixture-mode',{mode:'jwks-down'});
    assert.equal(await resolve(valid),fixture.text);
    const after=await issuerRequest('/fixture-mode',{mode:'normal'});
    assert.equal(after.counts.jwks,before.counts.jwks);
  });
  for(const mode of ['discovery-down','discovery-redirect']) await check(mode+':fails-closed-despite-cached-key',async()=>{
    await issuerRequest('/fixture-mode',{mode});
    await assert.rejects(resolve(valid),{kind:'http',status:503});
    await issuerRequest('/fixture-mode',{mode:'normal'});
  });
  await check('rotation:same-uri-retains-cached-key-and-rejects-new-kid',async()=>{
    const before=await issuerRequest('/fixture-mode',{mode:'rotated'});
    const rotated=(await issuerRequest('/fixture-token',{tenant:f.tenant,kind:'rotated'})).token;
    await assert.rejects(resolve(rotated),{kind:'http',status:401});
    assert.equal(await resolve(valid),fixture.text);
    const after=await issuerRequest('/fixture-mode',{mode:'rotated'});
    assert.equal(after.counts.jwks,before.counts.jwks);
    await issuerRequest('/fixture-mode',{mode:'new-uri'});
    assert.equal(await resolve(rotated),fixture.text);
    await assert.rejects(resolve(valid),{kind:'http',status:401});
    const refreshed=await issuerRequest('/fixture-mode',{mode:'normal'});
    assert.equal(refreshed.counts.jwks,after.counts.jwks+1);
    assert.equal(await resolve(valid),fixture.text);
  });
} else if (phase.startsWith('tenant-')) {
  assert.ok(jwt);
  for(const [i,f] of fixture.fixtures.entries()) {
    const admitted=phase==='tenant-active' || f.token==='fixture-b';
    await check(phase+':'+f.token+':'+f.memory,async()=>{
      if(admitted) assert.equal(await backend(f).resolveEvidence(saved[i].current),fixture.text);
      else await assert.rejects(backend(f).resolveEvidence(saved[i].current),{kind:'http',status:403});
      const response=await fetch(fixture.url+'/api/v1/search?q=needle&mode=fts',{headers:{authorization:'Bearer '+tokens.get(f.token),'X-Fortemi-Memory':f.memory},signal:AbortSignal.timeout(5000),redirect:'error'});
      assert.equal(response.status,admitted?200:403);await response.body.cancel();
    });
  }
} else for (const [i, f] of fixture.fixtures.entries()) {
  const remote = backend(f);
  const label = f.token + ':' + f.memory;
  assert.equal(remote.capabilities.evidenceLocators, false);
  assert.equal(remote.capabilities.typedMetadataPredicates, false);
  if (phase === 'initial') {
    const units = new Map();
    for (const mode of ['fts', 'semantic', 'hybrid']) {
      const result = await remote.search('needle', { ...options, mode });
      writeFileSync(root + `/initial-${i}-${mode}.json`,JSON.stringify(result,null,2)+'\n',{flag:'wx'});
      await check(label + ':' + mode + ':search-detail', () => {
        assert.equal(result.degraded, false); assert.equal(result.effectiveMode, mode);
        assert.equal(result.total, 1); assert.equal(result.hits.length, 1);
        assert.equal(result.hits[0].note.id, f.note);
        assert.ok(result.hits[0].evidence.locators.length > 0);
        assert.ok(!JSON.stringify(result).includes('private-key'));
      });
      for (const locator of result.hits[0].evidence.locators) {
        await check(label + ':' + mode + ':' + locator.unit.kind + ':resolve', async () => {
          assert.equal(locator.note_id, f.note); assert.ok(locator.source);
          assert.equal(await remote.resolveEvidence(locator), fixture.text);
          if (locator.unit.kind === 'embedding') assert.equal(locator.unit.index, 7);
        });
        units.set(locator.unit.kind, locator);
      }
    }
    assert.deepEqual([...units.keys()].sort(), ['attachment', 'current', 'embedding', 'title']);
    const locators = Object.fromEntries(units);
    saved.push(locators);
    for (const [kind, locator] of units) {
      await check(label + ':' + kind + ':typed-scope', async () => {
        assert.equal(await remote.resolveEvidence(locator, { metadataPredicates: [{path:'model',op:'eq',value:42}] }), fixture.text);
        await assert.rejects(remote.resolveEvidence(locator, { metadataPredicates: [{path:'model',op:'eq',value:'42'}] }), { kind:'http', status:404 });
      });
      for (const [name, other] of [
        ['tenant', backend(f, { authToken:f.token === 'fixture-a' ? 'fixture-b' : 'fixture-a' })],
        ['archive', backend({...f,memory:f.memory === 'public' ? 'search_fixture' : 'public'})],
        ['note-policy', backend(f, { baseUrl:fixture.deniedUrl })],
      ]) await check(label + ':' + kind + ':' + name, () => assert.rejects(other.resolveEvidence(locator), {kind:'http',status:404}));
    }
    for (const [name, start, end, expected] of [
      ['partial',3,Buffer.byteLength(fixture.text),fixture.text.slice(1)],
      ['empty',3,3,''],
    ]) await check(label + ':' + name, async () => {
      const locator = {...locators.current,span:{unit:'utf8-bytes',start,end}};
      assert.equal(await remote.resolveEvidence(locator), expected);
    });
    for (const [token, status] of [['invalid',401],['fixture-denied',403]]) {
      await check(label + ':auth-' + status, () => assert.rejects(backend(f,{authToken:token}).resolveEvidence(locators.current), {kind:'http',status}));
    }
  } else {
    const locators = saved[i];
    if (phase === 'stale') {
      await check(label + ':old-current-unavailable', () => assert.rejects(remote.resolveEvidence(locators.current), {kind:'http',status:404}));
      await check(label + ':fresh-search-resolution', async () => {
        const result = await remote.search('needle', {...options,mode:'fts'});
        assert.equal(result.hits.length,1); assert.equal(result.hits[0].note.id,f.note);
        const current = result.hits[0].evidence.locators.find(l => l.unit.kind === 'current');
        assert.ok(current); assert.notEqual(current.content_digest,locators.current.content_digest);
        assert.equal(await remote.resolveEvidence(current),'needle changed');
      });
      for (const kind of ['title','embedding','attachment']) await check(label + ':' + kind + ':unchanged', async () => assert.equal(await remote.resolveEvidence(locators[kind]),fixture.text));
    } else {
      await check(label + ':excluded-before-ranking', async () => {
        const result = await remote.search('needle',{...options,mode:'fts'});
        assert.equal(result.total,0); assert.deepEqual(result.hits,[]);
      });
      for (const [kind, locator] of Object.entries(locators)) {
        await check(label + ':' + kind + ':unavailable', () => assert.rejects(remote.resolveEvidence(locator), {kind:'http',status:404}));
        await check(label + ':' + kind + ':archived-option', async () => {
          if (phase === 'archived' && kind !== 'current') assert.equal(await remote.resolveEvidence(locator,{includeArchived:true}),fixture.text);
          else await assert.rejects(remote.resolveEvidence(locator,{includeArchived:true}),{kind:'http',status:404});
        });
      }
    }
  }
}
if (phase === 'initial') writeFileSync(root + '/locators.json',JSON.stringify(saved,null,2)+'\n',{flag:'wx'});
writeFileSync(root + '/' + phase + '-receipt.json',JSON.stringify({status:'PASS',phase,checks,packageRoot,entrySha256:process.env.FORTEMI_INSTALLED_CORE_SHA256,transport:'actual-node-fetch-private-loopback',authentication:jwt?'production-clerk-https-pg-tenant-store':'fixture-only',model:'synthetic-embeddings',purge:'SQL-fixture-not-lifecycle-worker'},null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:'PASS',phase,checks:checks.length}));
