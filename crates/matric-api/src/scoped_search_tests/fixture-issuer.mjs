import assert from 'node:assert/strict';
import { createServer } from 'node:https';
import { generateKeyPairSync, sign } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

const [privateRoot, evidence] = process.argv.slice(2);
assert.match(privateRoot, /^\/tmp\/fortemi-core-http-[A-Za-z0-9]+$/);
assert.ok(readFileSync('/proc/self/cgroup','utf8').includes('/'+process.env.FORTEMI_LOCAL_TEST_UNIT));
const keys = ['first','rotated'].map(kid => {
  const pair = generateKeyPairSync('rsa',{modulusLength:2048});
  return {...pair,jwk:{...pair.publicKey.export({format:'jwk'}),kid,alg:'RS256',use:'sig'}};
});
let mode = 'normal';
let issuer;
const counts = {discovery:0,jwks:0,tokens:0,control:0,tlsRejected:0};
const encode = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
const minted = new Map();
const token = ({tenant, kind='valid', scope='read'}) => {
  const now = Math.floor(Date.now()/1000);
  const payload = {iss:issuer,sub:'fixture-user',aud:'fortemi-http-fixture',iat:now-5,exp:now+600,scope,'fortemi:tenant_id':tenant};
  let header = {alg:'RS256',kid:'first',typ:'JWT'};
  let key = keys[0];
  switch(kind) {
    case 'valid': break;
    case 'wrong-issuer': payload.iss='https://untrusted.invalid'; break;
    case 'wrong-audience': payload.aud='wrong-audience'; break;
    case 'expired': payload.exp=now-300; break;
    case 'future-iat': payload.iat=now+300; break;
    case 'future-nbf': payload.nbf=now+300; break;
    case 'missing-tenant': delete payload['fortemi:tenant_id']; break;
    case 'invalid-tenant': payload['fortemi:tenant_id']='not-a-uuid'; break;
    case 'numeric-tenant': payload['fortemi:tenant_id']=42; break;
    case 'unknown-tenant': payload['fortemi:tenant_id']='11111111-1111-4111-8111-111111111111'; break;
    case 'wrong-signature': key=keys[1]; break;
    case 'unknown-kid': header.kid='unknown'; break;
    case 'rotated': header.kid='rotated'; key=keys[1]; break;
    case 'none': header.alg='none'; break;
    case 'hs256': header.alg='HS256'; break;
    case 'missing-kid': delete header.kid; break;
    case 'duplicate-alg': header='{"alg":"RS256","alg":"none","kid":"first"}'; break;
    case 'missing-scope': delete payload.scope; break;
    default: throw new Error('unknown fixture token kind');
  }
  const input=encode(header)+'.'+encode(payload);
  return input+'.'+sign('RSA-SHA256',Buffer.from(input),key.privateKey).toString('base64url');
};
const server = createServer({key:readFileSync(privateRoot+'/server.key'),cert:readFileSync(privateRoot+'/server.pem')},async (req,res) => {
  try {
    assert.ok(Object.values(counts).reduce((a,b)=>a+b,0)<1024);
    res.setHeader('content-type','application/json'); res.setHeader('cache-control','no-store');
    const reply = (status,body) => {res.writeHead(status);res.end(JSON.stringify(body));};
    if(req.method==='GET' && req.url==='/.well-known/openid-configuration') {
      counts.discovery++;
      if(mode==='discovery-down') return reply(503,{});
      if(mode==='discovery-redirect') {res.setHeader('location',issuer+'/unexpected');return reply(302,{});}
      return reply(200,{issuer,authorization_endpoint:issuer+'/authorize',token_endpoint:issuer+'/token',jwks_uri:issuer+(mode==='new-uri' ? '/rotated-jwks.json' : '/jwks.json'),response_types_supported:['code'],subject_types_supported:['public'],id_token_signing_alg_values_supported:['RS256']});
    }
    if(req.method==='GET' && ['/jwks.json','/rotated-jwks.json'].includes(req.url)) {
      counts.jwks++;
      if(mode==='jwks-down') return reply(503,{});
      return reply(200,{keys:[keys[mode==='rotated' || mode==='new-uri' ? 1 : 0].jwk]});
    }
    if(req.method==='POST' && ['/fixture-token','/fixture-mode'].includes(req.url)) {
      let bytes=0;const chunks=[];
      for await(const chunk of req){bytes+=chunk.length;assert.ok(bytes<=2048);chunks.push(chunk);}
      const body=JSON.parse(Buffer.concat(chunks).toString('utf8'));
      if(req.url==='/fixture-token') {
        counts.tokens++;
        const id=JSON.stringify(body);
        if(!minted.has(id)){assert.ok(minted.size<128);minted.set(id,token(body));}
        return reply(200,{token:minted.get(id)});
      }
      assert.ok(['normal','discovery-down','discovery-redirect','jwks-down','rotated','new-uri'].includes(body.mode));
      mode=body.mode;counts.control++;return reply(200,{mode,counts});
    }
    reply(404,{});
  } catch {res.writeHead(400);res.end('{}');}
});
server.requestTimeout=5000;server.headersTimeout=5000;server.maxRequestsPerSocket=128;
server.on('tlsClientError',()=>counts.tlsRejected++);
server.listen(0,'127.0.0.1',()=>{
  issuer='https://127.0.0.1:'+server.address().port;
  writeFileSync(privateRoot+'/issuer.json',JSON.stringify({issuer}),{flag:'wx',mode:0o600});
});
process.once('SIGTERM',()=>{
  server.closeAllConnections();server.close(()=>{
    writeFileSync(evidence+'/issuer-receipt.json',JSON.stringify({status:'STOPPED',pid:process.pid,counts,privateKeysPersistedInEvidence:false})+'\n',{flag:'wx'});
  });
});
