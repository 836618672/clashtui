import test from 'node:test';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {fixture} from './fixture.mjs';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';

test('Malformed YAML reaches the same validator through TUI Test/Check and CLI',{
  skip:process.platform!=='linux',timeout:30000,
},async t=>{
  const f=await fixture();t.after(()=>f.close());
  const input=join(f.root,'input.yaml');await writeFile(input,'proxies: []\n');
  await f.json(['manage','import','--name','broken','--input',input]);
  await writeFile(join(f.root,'mihomo/profiles/broken.yaml'),'[unterminated');
  const log=join(f.root,'validator-calls');
  await writeFile(join(f.root,'bin/core'),`#!/bin/sh\necho called >> '${log}'\necho CORE-DIAGNOSTIC >&2\nexit 1\n`,{mode:0o700});
  for(const action of ['test','check']){
    const result=JSON.parse((await f.run(['manage',action,'--name','broken'],{success:false})).stdout);
    assert.equal(result.valid,false);assert.equal(result.exit_code,1);
    assert.match(result.stderr,/CORE-DIAGNOSTIC/);
  }
  const child=spawn('python3',['tests/pipeline/tui_smoke.py',f.binary,f.root,'diagnostics']);
  let stderr='';child.stderr.on('data',b=>stderr+=b);child.stdout.resume();
  const timer=setTimeout(()=>child.kill('SIGKILL'),20000);
  try{
    const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});
    assert.equal(code,0,stderr);
    assert.equal((await readFile(log,'utf8')).trim().split('\n').length,4);
  }finally{clearTimeout(timer);}
});

test('Actual TUI starts in a PTY, navigates nine tabs and exits cleanly',{
  skip:process.platform!=='linux',timeout:30000,
},async t=>{
  const f=await fixture();t.after(()=>f.close());
  const child=spawn('python3',['tests/pipeline/tui_smoke.py',f.binary,f.root]);
  let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
  const timer=setTimeout(()=>child.kill('SIGKILL'),20000);
  try{
    const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});
    assert.equal(code,0,stderr);assert.equal(JSON.parse(stdout).clean_exit,true);
    for(const path of ['/configs','/proxies','/connections','/rules','/providers/proxies'])
      assert(f.requests.some(r=>r.path===path), `Tab activation never fetched ${path}`);
    assert(!f.requests.some(r=>['PUT','PATCH','DELETE'].includes(r.method)), 'Read-only navigation issued a core mutation');
  }finally{clearTimeout(timer);}
});

test('TUI mutation shortcuts share proxy APIs and start without restarting',{
  skip:process.platform!=='linux',timeout:30000,
},async t=>{
  const f=await fixture();t.after(()=>f.close());
  const log=join(f.root,'service-calls');
  await writeFile(join(f.root,'mihomo/templates/pty.yaml'),"proxies: []\nproxy-providers: {}\nproxy-groups: []\nrules: ['MATCH,DIRECT']\n");
  await writeFile(join(f.root,'bin/systemctl'),`#!/bin/sh\nprintf '%s\\n' "$*" >> '${log}'\nexit 0\n`,{mode:0o700});
  const child=spawn('python3',['tests/pipeline/tui_smoke.py',f.binary,f.root,'mutations']);
  let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
  const timer=setTimeout(()=>child.kill('SIGKILL'),25000);
  try{
    const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});
    assert.equal(code,0,stderr);assert.equal(JSON.parse(stdout).mutations,true);
    assert(f.requests.some(r=>r.method==='DELETE'&&r.path.startsWith('/proxies/')));
    assert(f.requests.some(r=>r.path.includes('/same%20%2F%20node/healthcheck')));
    assert.equal((await f.json(['core','proxies'])).proxies[f.group].fixed,'');
    const calls=await readFile(log,'utf8');
    assert.match(calls,/--user start mock-unused/);assert(!calls.includes('restart'));
    for(const name of ['tui-generated-a','tui-generated-b']){
      const document=await f.json(['manage','read','--name',name]);
      assert.match(document.content,/MATCH/);
      assert(!document.content.includes('mode: global'));
    }
  }finally{clearTimeout(timer);}
});
