import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {issuerRequest} from './fixture-issuer-client.mjs';

const root = process.argv[2];
assert.ok(readFileSync('/proc/self/cgroup','utf8').includes('/'+process.env.FORTEMI_LOCAL_TEST_UNIT));
const fixture = JSON.parse(readFileSync(root+'/worker-sse-fixture.json'));
const candidateRoot = process.env.FORTEMI_HOTM_REALTIME_FIXTURE;
assert.ok(candidateRoot, 'FORTEMI_HOTM_REALTIME_FIXTURE must name a verified consumer bundle');
const source = candidateRoot+'/hotm-realtime.mjs';
const hash = b => createHash('sha256').update(b).digest('hex');
const sourceSha256 = hash(readFileSync(source));
const candidate = JSON.parse(readFileSync(candidateRoot+'/hotm-realtime-bundle.json'));
const tested = JSON.parse(readFileSync(candidateRoot+'/receipt.json'));
assert.equal(tested.status,'PASS');assert.equal(tested.sourceStable,true);
assert.equal(candidate.status,'PASS');assert.equal(sourceSha256,candidate.sha256);
for(const [path,sha] of Object.entries(candidate.source)) assert.equal(tested.source['ui/'+path],sha,path);
const storage = new Map();
globalThis.window = Object.assign(new EventTarget(),{localStorage:{
  getItem:key=>storage.get(key)??null,setItem:(key,value)=>storage.set(key,String(value)),removeItem:key=>storage.delete(key)
}});
const {createEventsClient,resolveRealtimeArchive,createArchivesApi,createApiClient,setApiBearerToken,setActiveTenantId,setActiveMemory} = await import(source);
const metadataRequests = [];
let metadataProbe;
const tokens = new Map();
for (const f of fixture.controls) if (!tokens.has(f.tenant)) tokens.set(f.tenant,(await issuerRequest('/fixture-token',{tenant:f.tenant,scope:'mcp read'})).token);
let active = [];
let failure;
const receipts = [];
const deadline = Date.now()+25000;
const until = async condition => {
  while (!condition()) {
    if (failure) throw failure;
    assert.ok(Date.now()<deadline,'bounded worker SSE deadline');
    await new Promise(resolve=>setTimeout(resolve,25));
  }
  if (failure) throw failure;
};
const ownedFetch = async (url, options) => {
  assert.equal(new URL(url).origin,fixture.url);
  const path = new URL(url).pathname;
  if(path==='/api/v1/memory/context'){
    assert.equal(options.method,'GET');
    const headers = new Headers(options.headers);
    const owner=active.find(s=>headers.get('Authorization')==='Bearer '+tokens.get(s.f.tenant) &&
      headers.get('X-Fortemi-Memory')===(s.f.memory==='public'?null:s.f.memory));
    if(metadataProbe){
      assert.equal(headers.get('Authorization'),metadataProbe.token?'Bearer '+metadataProbe.token:null);
      assert.equal(headers.get('X-Fortemi-Memory'),metadataProbe.selection==='public'?null:metadataProbe.selection);
    }else assert.ok(owner,'unexpected metadata request');
    assert.ok(metadataRequests.length<12,'bounded metadata requests');
    const response=await fetch(url,{...options,redirect:'error'});
    assert.equal(response.status,metadataProbe?.status??200,'selected memory admission');
    if(response.ok) assert.equal(response.headers.get('cache-control'),'no-store');
    else assert.ok(response.headers.get('content-type').startsWith('application/problem+json'));
    metadataRequests.push({tenant:metadataProbe?.tenant??owner.f.tenant,path,status:response.status,
      selection:headers.get('X-Fortemi-Memory'),kind:metadataProbe?'control':'stream'});
    return response;
  }
  assert.equal(path,'/api/v1/events');
  const s = active.find(s => options.headers.Authorization==='Bearer '+tokens.get(s.f.tenant) && options.headers['X-Fortemi-Memory']===s.f.memory);
  assert.ok(s);assert.equal(++s.requests,1,'unexpected automatic reconnect');
  // Preserve the real consumer's request headers; no name/schema translation.
  const headers = {...options.headers};
  // Seed the replay request at a captured live event. This qualifies server
  // replay and real consumer parsing, not automatic reconnect scheduling.
  if (s.cursor) headers['Last-Event-ID']=s.cursor;
  const response = await fetch(url,{...options,headers,redirect:'error'});
  assert.equal(response.status,200,'hosted SSE admission');
  assert.ok(response.headers.get('content-type').startsWith('text/event-stream'));
  const stream = response.body.pipeThrough(new TransformStream({transform(chunk,controller) {
    s.bytes += chunk.length;assert.ok(s.bytes<=262144,'bounded SSE bytes');
    s.raw.push(Buffer.from(chunk));controller.enqueue(chunk);
  }}));
  return new Response(stream,{status:response.status,headers:response.headers});
};
globalThis.__ownedSseFetch = async (url,options) => {
  try { return await ownedFetch(url,options); }
  catch(error) { failure=error;throw error; }
};
const live = new Map();
try {
  const archived=fixture.controls.find(f=>f.memory!=='public');
  const foreign=fixture.controls.find(f=>f.tenant!==archived.tenant);
  const denied=(await issuerRequest('/fixture-token',{tenant:archived.tenant,scope:'mcp'})).token;
  for(const probe of [
    {tenant:archived.tenant,token:null,selection:null,status:401},
    {tenant:archived.tenant,token:denied,selection:null,status:403},
    {tenant:foreign.tenant,token:tokens.get(foreign.tenant),selection:archived.memory,status:404},
    {tenant:archived.tenant,token:tokens.get(archived.tenant),selection:'absent_fixture_memory',status:404},
    {tenant:archived.tenant,token:tokens.get(archived.tenant),selection:'default',status:200},
    {tenant:archived.tenant,token:tokens.get(archived.tenant),selection:null,status:200},
  ]){
    metadataProbe=probe;
    setApiBearerToken(probe.token);setActiveTenantId(probe.tenant);setActiveMemory(probe.selection);
    const before=metadataRequests.length;
    const pending=resolveRealtimeArchive(createArchivesApi(createApiClient(fixture.url+'/api/v1')),probe.selection);
    if(probe.status===200) assert.deepEqual(await pending,{memory:'public',memorySchema:'public'});
    else await assert.rejects(pending,error=>error.statusCode===probe.status);
    assert.equal(metadataRequests.length,before+1,'metadata failure must not retry');
    metadataProbe=undefined;
  }
  for (const phase of ['live','replay']) {
    active = fixture.controls.map(f=>({f,events:[],raw:[],bytes:0,requests:0,status:'initial',cursor:phase==='replay'?live.get(f.job)[0].event_id:undefined}));
    for (const s of active) {
      setApiBearerToken(tokens.get(s.f.tenant));setActiveTenantId(s.f.tenant);
      setActiveMemory(s.f.memory==='public'?null:s.f.memory);
      const resolved=await resolveRealtimeArchive(createArchivesApi(createApiClient(fixture.url+'/api/v1')),s.f.memory==='public'?null:s.f.memory);
      assert.deepEqual(resolved,{memory:s.f.memory,memorySchema:s.f.schema});
      s.client = createEventsClient(fixture.url+'/api/v1',{authorization:'Bearer '+tokens.get(s.f.tenant),tenantId:s.f.tenant,...resolved,preferFetch:true,typePrefixes:['job','note.updated'],onStatusChange:status=>{s.status=status;writeFileSync(root+'/worker-sse-state.json',JSON.stringify({phase,streams:active.map(s=>({schema:s.f.schema,status:s.status,requests:s.requests,types:s.events.map(e=>e.type)}))},null,2)+'\n');}});
      s.client.subscribe(event=>{
        try {
          assert.ok(s.events.length<16,'bounded SSE events');
          assert.equal(event.tenant_id,s.f.tenant);assert.equal(event.memory,s.f.schema);
          assert.equal(event.correlation_id,s.f.job);assert.equal(event.note_id,s.f.note);
          assert.equal(event.actor.kind,'system');assert.equal(event.payload_version,1);
          assert.match(event.event_id,/^[0-9a-f-]{36}$/);assert.ok(Number.isFinite(Date.parse(event.occurred_at)));
          assert.equal(event.entity_id,event.type==='note.updated'?s.f.note:s.f.job);
          s.events.push(event);
        } catch(error) { failure=error; }
      });
    }
    await until(()=>active.every(s=>s.status==='connected'));
    if (phase==='live') writeFileSync(root+'/worker-sse-ready.json',JSON.stringify({ready:true,pid:process.pid,streams:3})+'\n',{flag:'wx'});
    await until(()=>active.every(s=>s.events.at(-1)?.type==='note.updated'));
    await new Promise(resolve=>setTimeout(resolve,250));
    if (failure) throw failure;
    for (const s of active) {
      const progress=s.events.filter(e=>e.type==='job.progress').map(e=>e.progress);
      if (phase==='live') assert.ok(JSON.stringify(progress)==='[10]' || JSON.stringify(progress)==='[10,100]','default SSE coalescing may omit the second progress');
      else assert.deepEqual(progress,[10,100]);
      assert.deepEqual(s.events.map(e=>e.type),[...(phase==='live'?['job.started']:[]),...progress.map(()=>'job.progress'),'job.completed','note.updated']);
      const note=s.events.at(-1);assert.equal(note.title,undefined);assert.deepEqual(note.tags,[]);assert.equal(note.has_ai_content,false);assert.equal(note.has_links,false);
      const raw=Buffer.concat(s.raw).toString('utf8');
      // Check pre-parser bytes too: client-side context filtering cannot conceal
      // a server leak of another fixture's tenant, job or note.
      for (const other of fixture.controls) if (other.job!==s.f.job) {
        for (const id of [other.job,other.note,...(other.tenant!==s.f.tenant?[other.tenant]:[])]) assert.ok(!raw.includes(id),'foreign identity in SSE bytes');
      }
      if (phase==='live') live.set(s.f.job,s.events);
      else for (const event of live.get(s.f.job).slice(1)) assert.ok(s.events.some(e=>e.event_id===event.event_id));
      assert.equal(s.client.replayCursor,note.event_id);
      receipts.push({phase,tenant:s.f.tenant,schema:s.f.schema,job:s.f.job,note:s.f.note,events:s.events,rawBytes:s.bytes,rawSha256:hash(Buffer.concat(s.raw)),requests:s.requests});
      s.client.close();
    }
    writeFileSync(root+'/worker-sse-'+phase+'.json',JSON.stringify({phase,receipts:receipts.filter(r=>r.phase===phase)},null,2)+'\n',{flag:'wx'});
  }
  assert.equal(hash(readFileSync(source)),sourceSha256);
  writeFileSync(root+'/worker-sse-receipt.json',JSON.stringify({status:'PASS',pid:process.pid,source,sourceSha256,bundleSha256:sourceSha256,candidate,metadataRequests,esbuildVersion:candidate.esbuildVersion,streams:6,events:receipts.reduce((n,r)=>n+r.events.length,0),receipts,scope:'actual hosted binary; real candidate HotM archive resolver, HTTP client and SSE parser; metadata/name/schema resolved by production consumer code with no header translation. Seeded replay, not automatic reconnect or launched browser application/released HotM qualification'},null,2)+'\n',{flag:'wx'});
} catch(error) {
  writeFileSync(root+'/worker-sse-failure.json',JSON.stringify({reason:error.message.slice(0,512),streams:active.map(s=>({schema:s.f.schema,status:s.status,requests:s.requests,types:s.events.map(e=>e.type),bytes:s.bytes}))},null,2)+'\n',{flag:'wx'});
  throw error;
} finally {
  for (const s of active) s.client?.close();
  delete globalThis.__ownedSseFetch;
  tokens.clear();
}
