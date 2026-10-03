import test from 'node:test';
import assert from 'node:assert/strict';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fixture} from './fixture.mjs';

const options={skip:process.platform!=='linux',timeout:45000};
test('Delay budgets outlast global HTTP timeout and preserve the entire test URL',options,async t=>{
  const f=await fixture();t.after(()=>f.close());
  const url='https://example.test/a%2Fb?key=x%26y&emoji=中文#fragment';
  const cases=[
    {args:['core','delay',f.node],path:`/proxies/${encodeURIComponent(f.node)}/delay`},
    {args:['core','delay',f.group,'--group'],path:`/group/${encodeURIComponent(f.group)}/delay`},
    {args:['core','delay','same / node','--provider',f.provider],path:`/providers/proxies/${encodeURIComponent(f.provider)}/${encodeURIComponent('same / node')}/healthcheck`},
  ];
  for(const item of cases){
    f.delays.set(item.path,2600);const started=Date.now();
    await f.json([...item.args,'--url',url,'--timeout','5000']);
    assert(Date.now()-started>=2500,'The delayed backend response must actually be awaited');
    const request=f.requests.findLast(r=>r.path===item.path);
    assert.equal(request.query.url,url);assert.equal(request.query.timeout,'5000');
  }
  for(const timeout of ['0','3600001','18446744073709551615'])
    await f.run(['core','delay',f.node,'--timeout',timeout],{success:false});
});
test('Service readiness and configuration diagnostics use explicit results',options,async t=>{
  const f=await fixture();t.after(()=>f.close());
  await writeFile(join(f.root,'bin/systemctl'),'#!/bin/sh\nexit 0\n',{mode:0o700});
  await f.run(['service','start']);
  await f.run(['service','restart']);
  assert(f.requests.some(r=>r.path==='/version'));
  assert(f.requests.some(r=>r.path==='/configs'));
  f.faults.set('/version',503);
  assert.match((await f.run(['service','start'],{success:false})).stderr,/not ready/);
  f.faults.delete('/version');
  const input=join(f.root,'input.yaml');await writeFile(input,'proxies: []\n');
  await f.json(['manage','import','--name','diagnostics','--input',input]);
  await writeFile(join(f.root,'bin/core'),'#!/bin/sh\necho validator-out\necho validator-error >&2\nexit 1\n',{mode:0o700});
  for(const action of ['test','check']){
    const result=JSON.parse((await f.run(['manage',action,'--name','diagnostics'],{success:false})).stdout);
    assert.equal(result.valid,false);assert.equal(result.exit_code,1);
    assert.match(result.stdout,/validator-out/);assert.match(result.stderr,/validator-error/);
    assert.equal(result.activated,false);
  }
});
test('CLI core workflows exercise authenticated HTTP/WS and preserve captured connection scope',options,async t=>{
  const f=await fixture();t.after(()=>f.close());
  assert.equal((await f.json(['core','status'])).config.mode,'rule');
  assert.equal((await f.json(['core','proxies'])).proxies[f.group].now,'DIRECT');
  assert.equal((await f.json(['core','select',f.group,f.node])).proxies[f.group].now,f.node);
  assert.equal((await f.json(['core','delay',f.node])).delay,42);
  assert.equal((await f.json(['core','delay',f.group,'--group']))[f.node],42);
  assert.equal((await f.json(['core','delay','same / node','--provider',f.provider])).delay,73);
  assert.equal((await f.json(['core','unfix',f.group])).proxies[f.group].fixed,'');
  await f.run(['core','unfix',f.group],{success:false});
  for(const kind of ['proxy-providers','rule-providers']){
    assert.equal((await f.json(['core','resources',kind,'list']))[0].id,f.provider);
    assert.equal((await f.json(['core','resources',kind,'update-all','--yes'])).results[0].ok,true);
  }
  await f.json(['core','resources','proxy-providers','health','--name',f.provider,'--yes']);
  await f.json(['core','resources','rules','update','--name','0','--yes']);assert.equal(f.rules[0].disabled,true);
  const filtered=await f.json(['core','connections','--filter','worker']);
  assert.equal(filtered.connections.length,1);assert.equal(filtered.connections[0].opaque.keep,true);
  await f.json(['core','connections','--filter','worker','--close','--yes']);
  assert.deepEqual(f.connections.map(c=>c.id),['keep']);
  assert(!f.requests.some(r=>r.method==='DELETE'&&r.path==='/connections'));
  const logs=await f.json(['core','logs','--limit','1','--seconds','1']);
  assert.match(JSON.stringify(logs),/mock log preserved/);
  const metrics=await f.json(['core','metrics','--samples','2','--interval-ms','100']);
  assert.equal(metrics.samples.length,2);assert.equal(metrics.memory,1024);
  for(const action of ['flush-dns','flush-fakeip','upgrade-geo','restart','upgrade'])await f.json(['core','maintenance',action,'--yes']);
  assert(f.requests.every(r=>r.auth===`Bearer ${f.secret}`));
  assert(f.requests.some(r=>r.path.includes('%2F')));
});

test('Runtime patch/persist, readback failures, partial batches and missing capabilities',options,async t=>{
  const f=await fixture();t.after(()=>f.close());
  const patch=join(f.root,'patch.json');await writeFile(patch,JSON.stringify({mode:'direct'}));
  await f.json(['manage','patch','--input',patch]);assert.equal(f.state.mode,'direct');
  await f.json(['manage','persist','--yes']);
  const overlay=await readFile(join(f.root,'mihomo/core_override_config.yaml'),'utf8');assert.match(overlay,/direct/);
  await f.run(['core','connections','--close'],{success:false});assert.equal(f.connections.length,2);
  const encoded=encodeURIComponent(f.provider).replaceAll("'",'%27');
  f.faults.set(`/providers/proxies/${encoded}`,500);
  const partial=await f.run(['core','resources','proxy-providers','update-all','--yes'],{success:false});
  assert.equal(JSON.parse(partial.stdout).results[0].ok,false);
  f.faults.set('/rules',404);
  assert.match((await f.run(['core','resources','rules','list'],{success:false})).stderr,/404/);
  f.faults.set('/configs',503);
  assert.match((await f.run(['core','status'],{success:false})).stderr,/503/);
});

test('Wrong core identity and wrong authentication never deliver mutations',options,async t=>{
  const f=await fixture();t.after(()=>f.close());
  f.backend.version='sing-box 1.13.11';
  await f.run(['core','select',f.group,f.node],{success:false});
  assert(!f.requests.some(r=>r.method==='PUT'));
  f.backend.version='v1.19.0';
  const override=join(f.root,'mihomo/core_override_config.yaml');
  await writeFile(override,JSON.stringify({'external-controller':f.endpoint,secret:'wrong-token','mixed-port':27890}));
  assert.match((await f.run(['core','select',f.group,f.node],{success:false})).stderr,/401/);
  assert(!f.requests.some(r=>r.method==='PUT'));
});
