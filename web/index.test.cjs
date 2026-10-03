// Mock DOM/fetch regression tests; no browser, core or installation is started.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const html = fs.readFileSync(`${__dirname}/index.html`, 'utf8');
const source = html.match(/<script>([\s\S]*?)<\/script>/)[1];

function page(overrides = {}) {
  const elements = new Map();
  const staticButtons = [];
  class Element {
    constructor(tag = 'div') { this.tag = tag; this.children = []; this.dataset = {}; this.value = ''; this.textContent = ''; this.hidden = false; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    scrollIntoView() {}
    click() { return this.onclick?.(); }
  }
  for (const match of html.matchAll(/\bid="([^"]+)"/g)) elements.set(match[1], new Element());
  for (const match of html.matchAll(/<button([^>]*)>([^<]*)<\/button>/g)) {
    const id = match[1].match(/id="([^"]+)"/)?.[1];
    const button = id ? elements.get(id) : new Element('button');
    button.textContent = match[2];
    const service = match[1].match(/data-service="([^"]+)"/)?.[1];
    if (service) button.dataset.service = service;
    staticButtons.push(button);
  }
  const calls = [];
  const initial = {core:'mihomo', current:'one', service_running:true, panel:'http://127.0.0.1:9090/ui/', revision:'db-v1', system_proxy:null, service_install:false, templates:['default.yaml'], profiles:[{name:'one',type:{Url:'https://example.test/sub'},url:'https://example.test/sub',no_pp:false,update_with_proxy:true}]};
  const context = {
    document: {hidden:false,getElementById:id=>elements.get(id),createElement:tag=>new Element(tag),querySelectorAll:query=>query==='[data-service]'?staticButtons.filter(b=>b.dataset.service):staticButtons},
    navigator:{clipboard:{writeText:async()=>{}}}, URL, Blob,
    setInterval:()=>{},setTimeout:callback=>{callback();},
    confirm:()=>true,prompt:()=>null,
    fetch:async(path,options)=>{
      const request = options.body && JSON.parse(options.body);
      calls.push({path,request,headers:options.headers});
      let value;
      if(path==='/api/state') value=initial;
      else if(request.action==='read') value={content:'proxies: []',revision:'doc-v1'};
      else if(request.action==='save') value={revision:'doc-v2'};
      else if(request.action==='update_all') value={results:[{name:'one',ok:false,error:'mock failed'}]};
      else value={};
      return {ok:true,status:200,json:async()=>value};
    }, ...overrides,
  };
  vm.createContext(context);
  vm.runInContext(source,context);
  return {context,elements,calls,initial,buttons:staticButtons};
}

test('render includes local parity actions and does not interpret profile names as HTML',async()=>{
  const p=page();p.initial.profiles[0].name='<img src=x onerror=alert(1)>';
  await p.elements.get('connect').click();
  const row=p.elements.get('profiles').children[0];
  assert.equal(row.children[0].textContent,p.initial.profiles[0].name);
  assert.equal(row.children[0].children.length,0);
  const labels=row.children[2].children.map(b=>b.textContent);
  for(const label of ['预览','校验','测试','流量额度','复制 URL','编辑名称/URL']) assert.ok(labels.includes(label),label);
  assert.equal(p.elements.get('systemProxy').hidden,true);
  assert.ok(labels.some(label=>label.startsWith('内联 Provider')));
});

test('editing retains the original content revision across refresh',async()=>{
  const p=page();await p.elements.get('connect').click();
  await p.elements.get('profiles').children[0].children[2].children.find(b=>b.textContent==='编辑文件').click();
  p.initial.revision='db-v2';await p.elements.get('connect').click();
  p.elements.get('content').value='proxies: []';await p.elements.get('save').click();
  const save=p.calls.find(c=>c.request?.action==='save').request;
  assert.equal(save.core,'mihomo');assert.equal(save.revision,'doc-v1');
});

test('cancelled stop makes no mutation and sends management token only in headers',async()=>{
  const p=page({confirm:()=>false});p.elements.get('token').value='independent-token';
  await p.elements.get('connect').click();
  await p.buttons.find(b=>b.dataset.service==='stop').click();
  assert.equal(p.calls.filter(c=>c.request).length,0);
  assert.equal(p.calls[0].headers.Authorization,'Bearer independent-token');
  assert.ok(p.calls.every(c=>!c.path.includes('independent-token')));
});

test('batch update displays partial failures instead of claiming success',async()=>{
  const p=page();await p.elements.get('connect').click();await p.elements.get('updateAll').click();
  assert.ok(p.elements.get('resultContent').value.includes('mock failed'));
  assert.ok(p.elements.get('notice').textContent.includes('失败'));
});

test('closing an editor clears it and core-switch controls are absent',async()=>{
  const p=page();await p.elements.get('connect').click();
  await p.elements.get('profiles').children[0].children[2].children.find(b=>b.textContent==='编辑文件').click();
  await p.elements.get('cancel').click();
  assert.equal(p.elements.get('editor').hidden,true);
  assert.equal(p.elements.has('switchCore'),false);
  assert.equal(p.elements.has('core'),false);
});


test('background refresh preserves action buttons and an in-flight user result',async()=>{
  const p=page();await p.elements.get('connect').click();
  const original=p.elements.get('profiles').children[0];
  await p.context.poll();assert.equal(p.elements.get('profiles').children[0],original);
  let finishPoll;
  const fetch=p.context.fetch;
  p.context.fetch=(path,options)=>path==='/api/state'?new Promise(resolve=>{finishPoll=resolve;}):fetch(path,options);
  const poll=p.context.poll();
  // A background read must not lock the foreground interaction.
  await p.elements.get('profiles').children[0].children[2].children.find(b=>b.textContent==='编辑文件').click();
  assert.equal(p.elements.get('editor').hidden,false);
  finishPoll({ok:true,json:async()=>({...p.initial,profiles:[]})});await poll;
  assert.equal(p.elements.get('profiles').children[0],original,'Stale poll replaced the user session');
});
