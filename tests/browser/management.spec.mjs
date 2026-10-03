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
    await Promise.all(context.pages().map(page=>page.close()));
  } finally { if(f) await f.close(); f = undefined; }
});

async function connect(page) {
  await page.goto(web.url);
  await page.locator('#token').fill(web.token);
  await page.locator('#connect').click();
  await expect(page.locator('#main')).toBeVisible();
  await expect(page.locator('#connect')).toBeEnabled();
}
async function importProfile(page, name = 'browser & profile') {
  await page.locator('#name').fill(name);
  await page.locator('#importFile').setInputFiles({name:'fixture.yaml', mimeType:'text/yaml', buffer:Buffer.from('proxies: []\nmode: rule\n')});
  await page.locator('#import').click();
  await expect(page.locator('#profiles tr').filter({hasText:name})).toHaveCount(1);
  await expect(page.locator('#save')).toBeEnabled();
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
  page.once('dialog', dialog => dialog.dismiss());
  await row.getByRole('button', {name:'删除', exact:true}).click();
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
  expect((await f.json(['manage','read','--name','shared'])).content).toContain('mode: direct');
  await second.locator('#content').fill('[unterminated');
  await second.locator('#save').click();
  await expect(second.locator('#save')).toBeEnabled();
  await expect(second.locator('#notice')).not.toHaveText('操作完成');
  expect((await f.json(['manage','read','--name','shared'])).content).toContain('mode: direct');
});

test('Runtime patch/persist and override editing use shared state without service mutations', async ({page}) => {
  await connect(page); const row = await importProfile(page, 'runtime');
  page.once('dialog', dialog => dialog.accept('{"mode":"direct"}'));
  await page.locator('#runtime').click();
  await expect(page.locator('#runtime')).toBeEnabled();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  expect(f.state.mode).toBe('direct');
  page.once('dialog', dialog => dialog.accept());
  await page.locator('#persist').click();
  await expect(page.locator('#persist')).toBeEnabled();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  expect((await f.json(['manage','read','--kind','override'])).content).toContain('direct');
  await row.getByRole('button',{name:'编辑文件',exact:true}).click();
  await expect(page.locator('#editor')).toBeVisible();
  await page.locator('#cancel').click();
  await expect(page.locator('#editor')).toBeHidden();
  await page.locator('#override').click();
  await expect(page.locator('#editor')).toBeVisible();
  await expect(page.locator('#content')).toHaveValue(/direct/);
  await expect(page.locator('#core')).toHaveCount(0);
  await expect(page.locator('#switchCore')).toHaveCount(0);
  expect(f.requests.filter(r=>r.method==='PUT'||r.path==='/restart')).toHaveLength(0);
});

test('Template names, overwrite confirmation and validator diagnostics match CLI workflows', async ({page}) => {
  await connect(page);
  page.once('dialog', dialog=>dialog.accept('browser-template.yaml'));
  await page.locator('#newTemplate').click();
  await expect(page.locator('#editor')).toBeVisible();
  await page.locator('#content').fill("proxies: []\nproxy-providers: {}\nproxy-groups: []\nrules: ['MATCH,DIRECT']\n");
  await page.locator('#save').click();
  await expect(page.locator('#notice')).toHaveText('操作完成');
  await page.locator('#templates').selectOption('browser-template.yaml');
  await page.locator('#generated').fill('browser-generated');
  await page.locator('#generate').click();
  const row=page.locator('#profiles tr').filter({hasText:'browser-generated'});
  await expect(row).toHaveCount(1);
  const before=await f.json(['manage','read','--name','browser-generated']);
  page.once('dialog',dialog=>dialog.dismiss());
  await page.locator('#generate').click();
  await expect(page.locator('#generate')).toBeEnabled();
  expect((await f.json(['manage','read','--name','browser-generated'])).revision).toBe(before.revision);
  await writeFile(join(f.root,'bin/core'),'#!/bin/sh\necho browser-validator-out\necho browser-validator-error >&2\nexit 1\n',{mode:0o700});
  await row.getByRole('button',{name:'测试',exact:true}).click();
  await expect(page.locator('#resultContent')).toHaveValue(/browser-validator-error/);
  await expect(page.locator('#notice')).toContainText('配置校验失败');
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
    await expect(page.locator('#resultContent')).toHaveValue(/CORE-DIAGNOSTIC/);
    const result=JSON.parse(await page.locator('#resultContent').inputValue());
    const cli=JSON.parse((await f.run(['manage',action==='测试'?'test':'check','--name','broken'],{success:false})).stdout);
    expect(result).toEqual(cli);
  }
});
