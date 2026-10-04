import { answerDialog, finishDialogs } from "./dialogs.mjs";
import {test, expect} from '@playwright/test';
import {readFile, writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fixture} from '../pipeline/fixture.mjs';

test.skip(process.platform !== 'linux', 'Service fixtures are Linux-only');
let f, web;
test.beforeEach(async ({context}) => {
  f = await fixture(); web = await f.startWeb();
  // A UI regression must not send requests to subscriptions or external sites.
  await context.route('**/*', route => {
    const url = new URL(route.request().url());
    return [web.url,f.endpoint].includes(url.origin) ? route.continue() : route.abort();
  });
});
test.afterEach(async ({context}) => {
  try {
    await context.unrouteAll({behavior:'wait'});
    await Promise.all(context.pages().map(finishDialogs));
    await Promise.all(context.pages().map(page=>page.close()));
  } finally { if(f) await f.close(); f = undefined; }
});

async function connect(page) {
  await page.goto(web.url);
  await page.locator('#token').fill(web.token);
  await page.locator('#connect').click();
  await expect(page.locator('#main')).toBeVisible();
  await expect(page.locator('#refresh')).toBeEnabled();
  await page.getByRole('link',{name:'订阅与配置',exact:true}).click();
}
async function importProfile(page, name = 'browser & profile') {
  await page.locator('#name').fill(name);
  await page.locator('#importFile').setInputFiles({name:'fixture.yaml', mimeType:'text/yaml', buffer:Buffer.from('proxies: []\nmode: rule\n')});
  await page.locator('#import').click();
  await expect(page.locator('#profiles tr').filter({hasText:name})).toHaveCount(1);
  await expect(page.locator('#refresh')).toBeEnabled();
  return page.locator('#profiles tr').filter({hasText:name});
}

test('Authentication, import, edit, CLI readback, download and cancelled deletion', async ({page}) => {
  await page.goto(web.url);
  await page.locator('#token').fill('wrong-management-token-000');
  await page.locator('#connect').click();
  await expect(page.locator('#notice')).not.toBeEmpty();
  await expect(page.locator('#main')).toBeHidden();
  await connect(page);
  const row = await importProfile(page);
  await row.getByRole('button', {name:'编辑文件', exact:true}).click();
  await expect(page.locator('#editor')).toBeVisible();
  await page.locator('#content').fill('proxies: []\nmode: direct\n');
  await page.locator('#save').click();
  await expect(page.locator('#save')).toBeEnabled();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  const document = await f.json(['manage','read','--name','browser & profile']);
  expect(document.content).toContain('mode: direct');
  const downloadPromise = page.waitForEvent('download');
  await page.locator('#download').click();
  const download = await downloadPromise;
  expect(await readFile(await download.path(), 'utf8')).toBe(document.content);
  answerDialog(page, dialog => dialog.dismiss());
  await row.getByRole('button', {name:'删除', exact:true}).click();
  await finishDialogs(page);
  await expect(row).toHaveCount(1);
  await page.locator('#profileFilter').fill('nonexistent');
  await expect(page.locator('#profiles tr')).toHaveCount(0);
  await page.locator('#profileFilter').fill('browser');
  await expect(row).toHaveCount(1);
});

test('Two editors reject lost updates, invalid documents do not replace valid content', async ({page, context}) => {
  await connect(page); const row = await importProfile(page, 'shared');
  await row.getByRole('button',{name:'编辑文件',exact:true}).click();
  await expect(page.locator('#editor')).toBeVisible();
  const second = await context.newPage(); await connect(second);
  await second.locator('#profiles tr').filter({hasText:'shared'}).getByRole('button',{name:'编辑文件',exact:true}).click();
  await expect(second.locator('#editor')).toBeVisible();
  await second.locator('#content').fill('proxies: []\nmode: direct\n');
  await second.locator('#save').click();
  await expect(second.locator('#save')).toBeEnabled();
  await expect(second.locator('#notice')).toHaveText('操作完成');
  await page.locator('#content').fill('proxies: []\nmode: global\n');
  await page.locator('#save').click();
  await expect(page.locator('#save')).toBeEnabled();
  await expect(page.locator('#notice')).toContainText('Revision conflict');
  await expect(page.locator('#notice')).toBeInViewport();
  expect((await f.json(['manage','read','--name','shared'])).content).toContain('mode: direct');
  await second.locator('#content').fill('[unterminated');
  await second.locator('#save').click();
  await expect(second.locator('#save')).toBeEnabled();
  await expect(second.locator('#notice')).not.toHaveText('操作完成');
  await expect(second.locator('#notice')).toBeInViewport();
  expect((await f.json(['manage','read','--name','shared'])).content).toContain('mode: direct');
});

test('Runtime patch/persist and override editing use shared state without service mutations', async ({page}) => {
  await connect(page); const row = await importProfile(page, 'runtime');
  await page.getByRole('link',{name:'设置',exact:true}).click();
  await page.getByLabel('mode',{exact:true}).selectOption('direct');
  await page.getByRole('button',{name:'应用运行设置',exact:true}).click();
  await expect(page.locator('#refresh')).toBeEnabled();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  expect(f.state.mode).toBe('direct');
  answerDialog(page, dialog => dialog.accept());
  await page.getByRole('button',{name:'持久化运行设置',exact:true}).click();
  await expect(page.getByRole('button',{name:'持久化运行设置',exact:true})).toBeEnabled();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  expect((await f.json(['manage','read','--kind','override'])).content).toContain('direct');
  await page.locator('#closeResult').click();
  await page.getByRole('link',{name:'订阅与配置',exact:true}).click();
  await row.getByRole('button',{name:'编辑文件',exact:true}).click();
  await expect(page.locator('#editor')).toBeVisible();
  answerDialog(page,d=>d.accept());
  await page.locator('#cancel').click();
  await expect(page.locator('#editor')).toBeHidden();
  await page.getByRole('link',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:'编辑覆盖配置',exact:true}).click();
  await expect(page.locator('#editor')).toBeVisible();
  await expect(page.locator('#content')).toHaveValue(/direct/);
  await expect(page.locator('#core')).toHaveCount(0);
  await expect(page.locator('#switchCore')).toHaveCount(0);
  expect(f.requests.filter(r=>r.method==='PUT'||r.path==='/restart')).toHaveLength(0);
});

test('Template names, overwrite confirmation and validator diagnostics match CLI workflows', async ({page}) => {
  await connect(page);
  await page.getByRole('link',{name:'模板管理',exact:true}).click();
  answerDialog(page, dialog=>dialog.accept('browser-template.yaml'));
  await page.locator('#newTemplate').click();
  await expect(page.locator('#editor')).toBeVisible();
  await page.locator('#content').fill("proxies: []\nproxy-providers: {}\nproxy-groups: []\nrules: ['MATCH,DIRECT']\n");
  await page.locator('#save').click();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  await page.locator('#templates').selectOption('browser-template.yaml');
  await page.locator('#generated').fill('browser-generated');
  await page.locator('#generate').click();
  const row=page.locator('#profiles tr').filter({hasText:'browser-generated'});
  await expect.poll(async()=>(await f.json(['manage','state'])).profiles.some(p=>p.name==='browser-generated')).toBe(true);
  const before=await f.json(['manage','read','--name','browser-generated']);
  answerDialog(page,dialog=>dialog.dismiss());
  await page.locator('#generate').click();
  await expect(page.locator('#generate')).toBeEnabled();
  expect((await f.json(['manage','read','--name','browser-generated'])).revision).toBe(before.revision);
  await page.getByRole('link',{name:'订阅与配置',exact:true}).click();
  await writeFile(join(f.root,'bin/core'),'#!/bin/sh\necho browser-validator-out\necho browser-validator-error >&2\nexit 1\n',{mode:0o700});
  await row.getByRole('button',{name:'测试',exact:true}).click();
  await expect(page.locator('#resultContent')).toHaveValue(/browser-validator-error/);
  await expect(page.locator('#notice')).toContainText('配置校验失败');
  await expect(page.locator('#notice')).toBeInViewport();
  expect(await page.locator('#notice').evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.left+8,r.top+8));})).toBe(true);
  const cli=await f.run(['manage','test','--name','browser-generated'],{success:false});
  expect(JSON.parse(cli.stdout).valid).toBe(false);
});

test('Malformed profile YAML returns core diagnostics in Web and CLI',async({page})=>{
  await connect(page);const row=await importProfile(page,'broken');
  await writeFile(join(f.root,'mihomo/profiles/broken.yaml'),'[unterminated');
  await writeFile(join(f.root,'bin/core'),'#!/bin/sh\necho CORE-DIAGNOSTIC >&2\nexit 1\n',{mode:0o700});
  for(const action of ['测试','校验']){
    await row.getByRole('button',{name:action,exact:true}).click();
    await expect(page.locator('#notice')).toContainText('配置校验失败');
  await expect(page.locator('#notice')).toBeInViewport();
  expect(await page.locator('#notice').evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.left+8,r.top+8));})).toBe(true);
    await expect(page.locator('#resultContent')).toHaveValue(/CORE-DIAGNOSTIC/);
    const result=JSON.parse(await page.locator('#resultContent').inputValue());
    const cli=JSON.parse((await f.run(['manage',action==='测试'?'test':'check','--name','broken'],{success:false})).stdout);
    expect(result).toEqual(cli);
    await page.locator('#closeResult').click();
  }
});

test('Workspace overview and responsive layout retain editor drafts and expose empty search results',async({page},testInfo)=>{
  await page.goto(web.url);
  await expect(page.locator('#authIntro')).toBeVisible();
  await expect(page.locator('#main')).toBeHidden();
  await page.locator('#token').fill(web.token);
  await page.locator('#token').press('Enter');
  await expect(page.locator('#connectionState')).toHaveText('已连接');
  await expect(page.locator('#authIntro')).toBeHidden();
  await page.getByRole('link',{name:'订阅与配置',exact:true}).click();
  const name='日常工作配置-'+ 'long-name-'.repeat(8);
  const row=await importProfile(page,name);
  await expect(page.locator('.hint').first()).toContainText('1 / 1');
  await expect(page.locator('#profileEmpty')).toBeHidden();
  for(const width of [1440,1024,768,390]){
    await page.setViewportSize({width,height:1000});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
    await expect(row.getByRole('button',{name:'编辑文件',exact:true})).toBeVisible();
    const screenshot=await page.screenshot({fullPage:true});
    await testInfo.attach(`workspace-${width}`,{body:screenshot,contentType:'image/png'});
  }
  await page.locator('#profileFilter').fill('no-match');
  await expect(page.locator('#profileEmpty')).toBeVisible();
  await expect(page.locator('#profiles tr')).toHaveCount(0);
  await page.locator('#profileFilter').fill('');
  await row.getByRole('button',{name:'编辑文件',exact:true}).click();
  const draft='proxies: []\n# unsaved responsive draft\n';
  await page.locator('#content').fill(draft);
  await page.waitForTimeout(4500);
  await expect(page.locator('#content')).toBeFocused();
  await expect(page.locator('#content')).toHaveValue(draft);
  await page.locator('#cancel').click();
  expect((await f.json(['manage','read','--name',name])).content).not.toContain('unsaved responsive draft');
});

test('Workspace navigation requires authentication and locks again after logout or credential expiry',async({page})=>{
  await page.goto(web.url+'/#/profiles');
  await expect(page.locator('#workspaceNav')).toBeHidden();
  await expect(page.getByRole('link',{name:'订阅与配置',exact:true})).toBeHidden();
  await expect(page.locator('#authIntro')).toBeVisible();
  await page.locator('#token').fill('wrong-token');
  await page.locator('#connect').click();
  await expect(page.locator('#notice')).toContainText('管理令牌无效');
  await expect(page.locator('#workspaceNav')).toBeHidden();
  await connect(page);
  await expect(page.locator('#workspaceNav')).toBeVisible();
  await page.getByRole('link',{name:'模板管理',exact:true}).click();
  await expect(page).toHaveURL(/#\/templates$/);
  await expect(page.locator('#token')).toHaveCount(0);
  await page.getByRole('button',{name:'退出登录',exact:true}).click();
  await expect(page.locator('#workspaceNav')).toBeHidden();
  await expect(page.locator('#main')).toBeHidden();
  await expect(page.locator('#panel')).toBeHidden();
  await expect(page.locator('#connectionState')).toHaveText('未连接');
  await connect(page);
  const expired=route=>route.fulfill({status:401,contentType:'application/json',body:JSON.stringify({error:'Unauthorized'})});
  await page.route('**/api/state',expired);
  await expect(page.locator('#workspaceNav')).toBeHidden();
  await expect(page.locator('#authIntro')).toBeVisible();
  await expect(page.locator('#main')).toBeHidden();
  await expect(page.locator('#notice')).toContainText('管理令牌无效');
  await page.unroute('**/api/state',expired);
  await connect(page);
  await expect(page.locator('#workspaceNav')).toBeVisible();
});

test('Subscription form explains required names and trims pasted values before downloading', async ({page}) => {
  await connect(page);
  const actions=[];
  page.on('request', request=>{if(request.url().endsWith('/api/action'))actions.push(request.postDataJSON());});
  await page.locator('#url').fill(f.endpoint + '/subscription');
  await page.locator('#create').click();
  await expect(page.locator('#profileNameError')).toContainText('请填写配置名称');
  await expect(page.locator('#name')).toBeFocused();
  expect(actions).toHaveLength(0);
  await page.locator('#name').fill('   ');
  await page.locator('#create').click();
  await expect(page.locator('#profileNameError')).toContainText('请填写配置名称');
  await page.locator('#name').fill('bad/name');
  await page.locator('#create').click();
  await expect(page.locator('#profileNameError')).toContainText('名称不能包含');
  expect(actions).toHaveLength(0);
  await page.locator('#name').fill('  日常使用  ');
  await page.locator('#url').fill('ftp://example.test/sub');
  await page.locator('#create').click();
  await expect(page.locator('#profileUrlError')).toContainText('HTTP 或 HTTPS');
  await expect(page.locator('#url')).toBeFocused();
  expect(actions).toHaveLength(0);
  await page.locator('#url').fill('  ' + f.endpoint + '/subscription  ');
  await page.locator('#create').click();
  await expect(page.locator('#profiles tr').filter({hasText:'日常使用'})).toHaveCount(1);
  await expect(page.locator('#refresh')).toBeEnabled();
  expect(actions[0]).toMatchObject({action:'create',name:'日常使用',url:f.endpoint+'/subscription'});
  await page.locator('#create').click();
  await expect(page.locator('#profileNameError')).toContainText('已存在');
  expect(actions).toHaveLength(1);
});

test('Subscription download timeout is explained in Chinese and does not claim success', async ({page}) => {
  await connect(page);
  await page.route('**/api/action', route=>route.fulfill({json:{job:'mock-timeout'}}));
  await page.route('**/api/job', route=>route.fulfill({json:{id:'mock-timeout',pending:false,result:{ok:false,error:'the timeout of the request was reached'}}}));
  await page.locator('#name').fill('超时订阅');
  await page.locator('#url').fill('https://example.test/sub');
  await page.locator('#create').click();
  await expect(page.locator('#notice')).toContainText('订阅下载超时');
  await expect(page.locator('#notice')).toHaveClass(/error/);
  await expect(page.locator('#profiles tr').filter({hasText:'超时订阅'})).toHaveCount(0);
});

test('Subscription creation uses the selected route and remembers it for updates', async ({page}) => {
  const {createServer} = await import('node:http');
  const {connect: connectSocket} = await import('node:net');
  const {once} = await import('node:events');
  const tunnels = [], sockets = new Set();
  const proxy = createServer((req,res) => {res.writeHead(405);res.end();});
  proxy.on('connect', (req,client,head) => {
    tunnels.push(req.url);
    const target = new URL(f.endpoint);
    if(req.url !== target.host) {client.end('HTTP/1.1 403 Forbidden\r\n\r\n');return;}
    const upstream = connectSocket(Number(target.port),target.hostname,() => {
      client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if(head.length) upstream.write(head);
      client.pipe(upstream);upstream.pipe(client);
    });
    for(const socket of [client,upstream]) {
      sockets.add(socket);socket.on('close',()=>sockets.delete(socket));
      socket.on('error',()=>{client.destroy();upstream.destroy();});
    }
    client.on('close',()=>upstream.destroy());
  });
  proxy.listen(0,'127.0.0.1');await once(proxy,'listening');
  try {
    const overridePath=join(f.root,'mihomo/core_override_config.yaml');
    const override=JSON.parse(await readFile(overridePath,'utf8'));
    override['mixed-port']=proxy.address().port;
    await writeFile(overridePath,JSON.stringify(override));
    await f.restartWeb();
    await connect(page);
    await expect(page.locator('#subscriptionRoute')).toHaveValue('false');
    for(const [name,route] of [['直连订阅','false'],['代理订阅','true']]) {
      await page.locator('#name').fill(name);
      await page.locator('#url').fill(f.endpoint+'/subscription');
      await page.locator('#subscriptionRoute').selectOption(route);
      await page.locator('#create').click();
      await expect(page.locator('#profiles tr').filter({hasText:name})).toHaveCount(1);
      await expect(page.locator('#refresh')).toBeEnabled();
      expect(tunnels.length).toBe(route==='true'?1:0);
      const state=await f.json(['manage','state']);
      expect(state.profiles.find(p=>p.name===name).update_with_proxy).toBe(route==='true');
    }
    const row=page.locator('#profiles tr').filter({hasText:'代理订阅'});
    await row.getByRole('button',{name:'更新',exact:true}).click();
    await expect(page.locator('#resultContent')).toBeVisible();
    expect(tunnels).toHaveLength(2);
    await page.locator('#closeResult').click();
    for(const socket of sockets) socket.destroy();
    await new Promise(resolve=>proxy.close(resolve));
    await page.locator('#name').fill('代理不可用');
    await page.locator('#create').click();
    await expect(page.locator('#notice')).toHaveClass(/error/);
    await expect(page.locator('#refresh')).toBeEnabled();
    expect((await f.json(['manage','state'])).profiles.some(p=>p.name==='代理不可用')).toBe(false);
  } finally {
    for(const socket of sockets) socket.destroy();
    if(proxy.listening) await new Promise(resolve=>proxy.close(resolve));
  }
});

test('Active profile deletion protection is shown beside the row even when the page notice is off screen', async ({page}) => {
  const names=[...Array.from({length:18},(_,i)=>`profile-${String(i).padStart(2,'0')}`),'zz-active'];
  const content='proxies: []\nmode: rule\n';
  for(const name of names) await writeFile(join(f.root,`mihomo/profiles/${name}.yaml`),content);
  await writeFile(join(f.root,'clashtui.db'),'core_type: mihomo\nmihomo:\n  cur_profile: zz-active\n  profiles:\n'+names.map(name=>`    ${name}: File\n`).join(''));
  await connect(page);
  const row=page.locator('#profiles tr').filter({hasText:'zz-active'});
  await expect(row.getByText('使用中',{exact:true})).toBeVisible();
  for(const width of [1280,390]) {
    await page.setViewportSize({width,height:720});
    await row.getByRole('button',{name:'删除',exact:true}).scrollIntoViewIfNeeded();
    if(await page.locator('#notice').count()) await page.getByRole('button',{name:'关闭操作提示',exact:true}).click();
    await expect(page.locator('#notice')).toHaveCount(0);
    answerDialog(page,dialog=>dialog.accept());
    await row.getByRole('button',{name:'删除',exact:true}).click();
    const alert=row.getByRole('alert');
    await expect(alert).toContainText('当前配置正在使用，已阻止删除');
    await expect(alert).toBeInViewport();
    await expect(alert).not.toContainText('Select another profile');
    await expect(row).toHaveCount(1);
    await expect(page.locator('#refresh')).toBeEnabled();
    expect((await f.json(['manage','state'])).current).toBe('zz-active');
    expect(await readFile(join(f.root,'mihomo/profiles/zz-active.yaml'),'utf8')).toBe(content);
  }
  answerDialog(page,dialog=>dialog.accept());
  await page.locator('#profiles tr').filter({hasText:'profile-00'}).getByRole('button',{name:'删除',exact:true}).click();
  await expect(page.locator('#profiles tr').filter({hasText:'profile-00'})).toHaveCount(0);
  await expect(page.locator('.profile-row-error')).toHaveCount(0);
});

test('Activation progress, success and failure remain visible beside the selected profile', async ({page}) => {
  const names=[...Array.from({length:18},(_,i)=>`profile-${String(i).padStart(2,'0')}`),'zz-target'];
  for(const name of names) await writeFile(join(f.root,`mihomo/profiles/${name}.yaml`),'proxies: []\n');
  await writeFile(join(f.root,'clashtui.db'),'core_type: mihomo\nmihomo:\n  cur_profile: profile-00\n  profiles:\n'+names.map(name=>`    ${name}: File\n`).join(''));
  await connect(page);
  let submitted=0, completed=false, success=true;
  await page.route('**/api/action',route=>{
    if(route.request().postDataJSON()?.action==='activate') {
      submitted++;completed=false;
      return route.fulfill({json:{job:'activation-'+submitted}});
    }
    return route.continue();
  });
  await page.route('**/api/job',route=>route.fulfill({json:{id:'activation-'+submitted,pending:!completed,result:completed?{ok:success,value:{},error:'Activation failed'}:null}}));
  await page.route('**/api/state',async route=>{
    const response=await route.fetch();const state=await response.json();
    if(completed && success) state.current='zz-target';
    await route.fulfill({json:state});
  });
  const row=page.locator('#profiles tr').filter({hasText:'zz-target'});
  for(const width of [1280,390]) {
    success=width===1280;
    await page.setViewportSize({width,height:720});
    await row.getByRole('button',{name:'激活',exact:true}).scrollIntoViewIfNeeded();
    if(await page.locator('#notice').count()) await page.getByRole('button',{name:'关闭操作提示',exact:true}).click();
    await expect(page.locator('#notice')).toHaveCount(0);
    answerDialog(page,dialog=>dialog.accept());
    await row.getByRole('button',{name:'激活',exact:true}).click();
    await expect(row.getByRole('status')).toContainText('正在激活');
    await expect(row.getByRole('status')).toBeInViewport();
    completed=true;
    const result=row.getByRole(success?'status':'alert');
    await expect(result).toContainText(success?'配置已激活：zz-target':'Activation failed');
    await expect(result).toBeInViewport();
    await expect(page.locator('#refresh')).toBeEnabled();
  }
  answerDialog(page,dialog=>dialog.dismiss());
  await row.getByRole('button',{name:'激活',exact:true}).click();
  expect(submitted).toBe(2);
});

test('App confirmation dialogs cancel safely, trap focus and fit mobile dark mode', async ({page}, testInfo) => {
  await connect(page);
  const row = await importProfile(page, 'dialog-check');
  const actions = [];
  page.on('request', request => { if(request.url().endsWith('/api/action')) actions.push(request.postDataJSON()); });
  const activate = row.getByRole('button', {name:'激活', exact:true});
  await activate.click();
  const dialog = page.getByRole('dialog', {name:'激活配置'});
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('「dialog-check」');
  await expect(page.locator('#actionDialogCancel')).toBeFocused();
  for (let i=0; i<5; i++) {
    await page.keyboard.press('Tab');
    expect(await dialog.evaluate(el=>el.contains(document.activeElement))).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(activate).toBeFocused();
  expect(actions).toEqual([]);
  await page.setViewportSize({width:390,height:844});
  await page.evaluate(()=>document.documentElement.dataset.theme='dark');
  await activate.click();
  await expect(dialog).toBeInViewport();
  const box = await dialog.boundingBox();
  expect(box.x).toBeGreaterThanOrEqual(16);
  expect(box.x+box.width).toBeLessThanOrEqual(374);
  await page.screenshot({path:testInfo.outputPath('confirmation-mobile.png')});
  await page.locator('#actionDialogCancel').click();
  await expect(dialog).toBeHidden();
  expect(actions).toEqual([]);
  await row.getByRole('button',{name:'删除',exact:true}).click();
  await expect(page.locator('#actionDialogConfirm')).toHaveClass(/dialog-danger/);
  await page.locator('#actionDialogCancel').click();
  await expect(row).toHaveCount(1);
  expect(actions).toEqual([]);
});
