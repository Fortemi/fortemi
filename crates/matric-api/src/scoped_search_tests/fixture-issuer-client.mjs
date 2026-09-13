import assert from 'node:assert/strict';
import { request } from 'node:https';
import { readFileSync } from 'node:fs';

export async function issuerRequest(path, body) {
  const url = new URL(path,process.env.FORTEMI_TEST_ISSUER);
  assert.equal(url.protocol,'https:');assert.equal(url.hostname,'127.0.0.1');assert.ok(url.port);
  const encoded=Buffer.from(JSON.stringify(body));
  return new Promise((resolve,reject)=>{
    const req=request(url,{method:'POST',ca:readFileSync(process.env.FORTEMI_TEST_CA),timeout:3000,headers:{'content-type':'application/json','content-length':encoded.length}},res=>{
      let size=0;const chunks=[];
      res.on('data',chunk=>{size+=chunk.length;if(size>8192) res.destroy(new Error('fixture response limit'));else chunks.push(chunk);});
      res.on('error',()=>reject(new Error('fixture response failed')));
      res.on('end',()=>{try{assert.equal(res.statusCode,200);resolve(JSON.parse(Buffer.concat(chunks)));}catch{reject(new Error('fixture response invalid'));}});
    });
    req.on('timeout',()=>req.destroy(new Error('fixture deadline')));
    req.on('error',()=>reject(new Error('fixture issuer failed')));
    req.end(encoded);
  });
}
