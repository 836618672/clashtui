// HTTP contract regressions: temporary config and mock core only.
import test from 'node:test';
import {request} from 'node:http';
import assert from 'node:assert/strict';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fixture} from './fixture.mjs';
async function withWeb(work) {
  const f=await fixture();
  try {const web=await f.startWeb();await work(f,web);} finally {await f.close();}
}
function client(web) {
 const headers={Authorization:'Bearer '+web.token,'Content-Type':'application/json'};
 return (path,body)=>new Promise((resolve,reject)=>{
  const bytes=body===undefined?undefined:typeof body==='string'?body:JSON.stringify(body);
  const req=request(web.url+path,{headers:{...headers,...(bytes!==undefined?{'Content-Length':Buffer.byteLength(bytes)}:{})},method:bytes===undefined?'GET':'POST',agent:false},res=>{
   let text='';res.setEncoding('utf8');res.on('data',chunk=>text+=chunk);res.on('end',()=>resolve({status:res.statusCode,json:async()=>JSON.parse(text)}));res.on('error',reject);
  });req.on('error',reject);req.setTimeout(15000,()=>req.destroy(new Error('HTTP fixture timed out')));req.end(bytes);
 });
}
async function waitJob(api) {
 for(let i=0;i<100;i++){const job=await (await api('/api/job')).json();if(!job.pending)return job;await new Promise(r=>setTimeout(r,30));}
 throw new Error('Task did not finish');
}
test('Invalid and oversized bodies are rejected before starting a task',()=>withWeb(async(f,web)=>{
 const api=client(web);
 for(const path of ['/api/action','/api/core/read']) {
  assert.equal((await api(path,'{invalid')).status,400);
  assert.equal((await api(path,'null')).status,400);
 }
 for(const [path,limit] of [['/api/action',2*1024*1024],['/api/core/read',65536]]) {
  // Exact limit reaches JSON validation; one byte beyond must be 413.
  const exact=' '.repeat(limit-2)+'{}';
  assert.notEqual((await api(path,exact)).status,413);
  await waitJob(api);
  const response=await api(path,' '.repeat(limit+1));assert.equal(response.status,413);
  assert.match((await response.json()).error,/limit/);
 }
 assert.equal((await api('/api/job')).status,200);
 assert.equal((await api('/api/state')).status,200);
}));
test('Concurrent actions admit one task, preserve its ID and remain responsive',()=>withWeb(async(f,web)=>{
 const api=client(web);let release;const endpoint='/proxies/'+encodeURIComponent(f.node)+'/delay';f.gates.set(endpoint,new Promise(r=>release=r));
 const body={action:'core',core:'mihomo',operation:'delay',name:f.node,url:'https://example.test/delay',timeout:5000};
 const started=await (await api('/api/action',body)).json();
 try {
  const responses=await Promise.all(Array.from({length:5},()=>api('/api/action',body)));
  assert.equal(responses.filter(r=>r.status===409).length,5);
  assert.equal((await (await api('/api/job')).json()).id,started.job);
  assert.equal((await api('/api/state')).status,503);
 } finally { release(); }
 const done=await waitJob(api);assert.equal(done.id,started.job);assert.equal(done.state,'succeeded');assert.equal(done.result.ok,true);
 assert.ok(done.started_at<=done.finished_at);
 assert.equal((await api('/api/action',{action:'read',kind:'override'})).status,200);
 await waitJob(api);
}));
test('Restart recovers interruption without replay and reports damaged records',()=>withWeb(async(f,web)=>{
 const path=join(f.root,'.web-task.json');
 await writeFile(path,JSON.stringify({id:'123',action:'activate',state:'running',result:null}));
 await f.restartWeb();const api=client(web);const job=await (await api('/api/job')).json();
 assert.equal(job.id,'123');assert.equal(job.pending,false);assert.equal(job.state,'interrupted');assert.equal(job.result.code,'task_interrupted');
 assert.equal(JSON.parse(await readFile(path,'utf8')).state,'interrupted');
 assert.equal(f.requests.filter(r=>r.method==='PUT'||r.method==='PATCH').length,0);
 await writeFile(path,'{broken');await f.restartWeb();
 const invalid=await (await api('/api/job')).json();assert.equal(invalid.state,'interrupted');assert.equal(invalid.result.code,'recovery_record_invalid');assert.equal(await readFile(path,'utf8'),'{broken');
 assert.equal((await api('/api/state')).status,200);
}));
test('Unavailable core reads return errors while local state and jobs remain usable',()=>withWeb(async(f,web)=>{
 const api=client(web);f.faults.set('/proxies',503);
 const response=await api('/api/core/read',{view:'proxies'});assert.equal(response.status,400);assert.ok((await response.json()).error);
 assert.equal((await api('/api/state')).status,200);assert.equal((await api('/api/job')).status,200);
 assert.equal((await fetch(web.url+'/api/core/read',{method:'POST',body:'{}'})).status,401);
}));
test('Read queue overload is bounded without blocking job or state dispatch',()=>withWeb(async(f,web)=>{
 const api=client(web);f.delays.set('/proxies',1200);
 const before=f.requests.filter(r=>r.path==='/proxies').length;
 const slow=Array.from({length:30},()=>api('/api/core/read',{view:'proxies'}));
 try {
  for(let i=0;i<100 && f.requests.filter(r=>r.path==='/proxies').length-before<4;i++)await new Promise(r=>setTimeout(r,10));
  assert.equal(f.requests.filter(r=>r.path==='/proxies').length-before,4);
  const started=Date.now();assert.equal((await api('/api/job')).status,200);assert.equal((await api('/api/state')).status,200);assert.ok(Date.now()-started<1000);
 } finally {
  const responses=await Promise.all(slow);assert.ok(responses.some(r=>r.status===503));assert.ok(responses.some(r=>r.status===200));assert.ok(responses.every(r=>[200,503].includes(r.status)));
 }
}));
