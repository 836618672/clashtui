// Actual VM APIs and real browser input. Never mock/force a business response.
import {chromium} from '../tests/browser/node_modules/playwright/index.mjs';
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const directory=resolve(process.env.CLASHTUI_C_BROWSER_REPORT||'target/vm/c-browser');
await mkdir(directory,{recursive:true});
const vm=command=>execFileSync('bash',['scripts/vm.sh','ssh',command],{encoding:'utf8',timeout:45000});
const credentials=JSON.parse(vm(`python3 -c 'import json,yaml; from pathlib import Path; p=Path("/home/tester/clashtui-test/config"); print(json.dumps({"token":(p/"management-token").read_text().strip(),"secret":yaml.safe_load((p/"mihomo/core_override_config.yaml").read_text())["secret"]}))'`));
const cli=args=>JSON.parse(vm('clashtui --config-dir=/home/tester/clashtui-test/config '+args));
const assert=(value,message)=>{if(!value)throw new Error(message);};
const browser=await chromium.launch({headless:true,executablePath:process.env.CLASHTUI_CHROMIUM_PATH});
const context=await browser.newContext({serviceWorkers:'block',acceptDownloads:true});
context.setDefaultTimeout(12000);
await context.route('**/*',route=>['http://127.0.0.1:29090','http://127.0.0.1:29091'].includes(new URL(route.request().url()).origin)?route.continue():route.abort());
await context.tracing.start({screenshots:true,snapshots:true,sources:true});
const page=await context.newPage();
const results=[];
async function ready(){await page.waitForFunction(()=>!document.querySelector('#connect').disabled);}
async function connect(){await page.goto('http://127.0.0.1:29091/');await page.locator('#token').fill(credentials.token);await page.locator('#connect').click();await ready();await page.locator('#main').waitFor();}
const row=name=>page.locator('#profiles tr').filter({has:page.locator('td').filter({hasText:new RegExp('^'+name+'$')})});
async function check(id,work){
  const started=Date.now();await context.tracing.startChunk({title:id});
  try {const evidence=await work();results.push({id,status:'passed',evidence:evidence||{},duration_ms:Date.now()-started});await context.tracing.stopChunk();}
  catch(error){let message=String(error);for(const value of Object.values(credentials))message=message.replaceAll(value,'[redacted]');results.push({id,status:'failed',error:message.slice(0,3000),duration_ms:Date.now()-started});await page.screenshot({path:directory+'/'+id+'.png'}).catch(()=>{});await context.tracing.stopChunk({path:directory+'/'+id+'-trace.zip'}).catch(()=>{});}
  finally {page.removeAllListeners('dialog');}
  console.log(JSON.stringify(results.at(-1)));
}
try {
  await check('CB01',async()=>{
    await page.goto('http://127.0.0.1:29091/');await page.locator('#token').fill(credentials.secret);await page.locator('#connect').click();await ready();
    assert(await page.locator('#main').isHidden(),'Core secret authenticated management');
    for(const secret of [credentials.token,'wrong-core-secret']){
      const panel=await context.newPage();
      await panel.addInitScript(({secret})=>{localStorage.setItem('endpointList',JSON.stringify([{id:'c',url:'http://127.0.0.1:29090',secret}]));localStorage.setItem('selectedEndpoint','c');},{secret});
      const rejected=panel.waitForResponse(r=>r.url().startsWith('http://127.0.0.1:29090/')&&r.status()===401);
      await panel.goto('http://127.0.0.1:29090/ui/#/proxies');await rejected;await panel.close();
    }
    const panel=await context.newPage();
    await panel.addInitScript(()=>{localStorage.setItem('endpointList',JSON.stringify([{id:'c',url:'http://127.0.0.1:29090/unreachable',secret:'wrong'}]));localStorage.setItem('selectedEndpoint','c');});
    const failed=panel.waitForResponse(r=>r.url().includes('/unreachable/')&&r.status()>=400);
    await panel.goto('http://127.0.0.1:29090/ui/#/proxies');await failed;await panel.close();await connect();
    return {credential_separation:true,panel_401:true,unreachable_endpoint:true};
  });
  await check('CB02',async()=>{
    await connect();await page.locator('#name').fill('c-web-url');await page.locator('#url').fill('http://127.0.0.1:18080/subscription.yaml');await page.locator('#create').click();await ready();
    assert(await row('c-web-url').count()===1,'URL creation missing');
    await page.locator('#profileFilter').fill('c-web-url');assert(await page.locator('#profiles tr').count()===1,'Profile filter ignored');
    let dialogs=0;page.on('dialog',dialog=>dialog.accept(dialogs++===0?'c-web-renamed':'http://127.0.0.1:18080/subscription.yaml'));
    await row('c-web-url').getByRole('button',{name:'编辑名称/URL',exact:true}).click();await ready();page.removeAllListeners('dialog');
    assert(cli('manage profile_url --name c-web-renamed').url.includes('/subscription.yaml'),'Rename not visible to CLI');
    await page.locator('#profileFilter').fill('');await row('c-web-renamed').getByRole('button',{name:'编辑文件',exact:true}).click();await ready();
    const content=await page.locator('#content').inputValue();const downloaded=page.waitForEvent('download');await page.locator('#download').click();const download=await downloaded;const path=directory+'/profile-export.yaml';await download.saveAs(path);
    assert(await readFile(path,'utf8')===content,'Export bytes differ');
    await page.locator('#content').fill(content+'\n# c-web-edited\n');await page.locator('#save').click();await ready();
    assert(cli('manage read --name c-web-renamed').content.includes('c-web-edited'),'Web save absent in CLI');
    await page.locator('#content').fill('discard this unsaved draft');await page.locator('#cancel').click();assert(!cli('manage read --name c-web-renamed').content.includes('discard'),'Cancel saved draft');
    page.once('dialog',d=>d.accept());await row('c-web-renamed').getByRole('button',{name:'删除',exact:true}).click();await ready();
    assert(!cli('manage state').profiles.some(p=>p.name==='c-web-renamed'),'Delete absent in CLI');
    return {filter:true,rename_url:true,export_exact_bytes:true,save_readback:true,cancel:true,delete:true};
  });
  await check('CB03',async()=>{
    await connect();page.once('dialog',d=>d.accept('{"mode":"global"}'));await page.locator('#runtime').click();await ready();
    assert(cli('core status').config.mode==='global','Runtime patch absent');
    page.once('dialog',d=>d.accept());await page.locator('#persist').click();await ready();
    assert(cli('manage read --kind override').content.includes('global'),'Persist missing override');
    page.once('dialog',d=>d.accept('{"mode":"rule"}'));await page.locator('#runtime').click();await ready();
    page.once('dialog',d=>d.accept());await page.locator('#persist').click();await ready();
    assert(cli('core status').config.mode==='rule','Mode not restored');
    return {runtime_patch:true,persist:true,restored_rule:true};
  });
  await check('CB04',async()=>{
    await connect();
    for(const operation of ['stop','start','restart']){
      page.once('dialog',d=>d.accept());await page.locator(`[data-service="${operation}"]`).click();await ready();
      const active=vm('systemctl is-active clashtui-test-mihomo || true').trim();assert(active===(operation==='stop'?'inactive':'active'),'Service '+operation+' state '+active);
      if(operation!=='stop')assert(cli('core status').version.meta,'Service did not become ready');
    }
    return {stop:true,start:true,restart:true,core_ready:true};
  });
  await check('CB05',async()=>{
    await connect();page.once('dialog',d=>d.accept('c-web-template.yaml'));await page.locator('#newTemplate').click();await ready();
    const source=vm('cat /home/tester/clashtui-test/manual/template.yaml');await page.locator('#content').fill(source);await page.locator('#save').click();await ready();await page.locator('#templates').selectOption('c-web-template.yaml');
    page.once('dialog',d=>d.accept('{}'));await page.locator('#templateProviders').click();await ready();assert(Object.keys(cli('manage template_providers --name c-web-template.yaml').groups).length===0,'Empty groups lost');
    page.once('dialog',d=>d.accept('{"manual":{"Manual HTTP":"http://127.0.0.1:18080/proxy.yaml"}}'));await page.locator('#templateProviders').click();await ready();
    await page.locator('#previewTemplate').click();await ready();assert((await page.locator('#resultContent').inputValue()).includes('Manual HTTP'),'Preview missing provider');
    await page.locator('#generated').fill('c-web-generated');await page.locator('#generate').click();await ready();assert(cli('manage check --name c-web-generated').valid,'Generated invalid');
    page.once('dialog',d=>d.accept());await page.locator('#deleteTemplate').click();await ready();assert((await page.locator('#notice').textContent()).includes('referenced'),'Referenced template deletion accepted');
    page.once('dialog',d=>d.accept());await row('c-web-generated').getByRole('button',{name:'删除',exact:true}).click();await ready();
    page.once('dialog',d=>d.accept());await page.locator('#deleteTemplate').click();await ready();assert(!cli('manage state').templates.includes('c-web-template.yaml'),'Template deletion absent');
    return {groups_clear_save:true,preview:true,generate_check:true,reference_guard:true,delete:true};
  });
  await check('CB06',async()=>{
    await connect();await page.locator('#name').fill('c-web-provider');await page.locator('#importFile').setInputFiles({name:'provider.yaml',mimeType:'text/yaml',buffer:Buffer.from(vm('cat /home/tester/clashtui-test/manual/providers-yaml.yaml'))});await page.locator('#import').click();await ready();
    await row('c-web-provider').getByRole('button',{name:'更新',exact:true}).click();await ready();
    const cache=vm('sha256sum /home/tester/clashtui-test/config/mihomo/proxies/manual.yaml').split(' ')[0];
    vm('touch /home/tester/clashtui-test/manual/fail-proxy');
    try {
      await row('c-web-provider').getByRole('button',{name:'更新',exact:true}).click();await ready();
      assert((await page.locator('#notice').textContent()).includes('部分订阅资源更新失败'),'Partial failure hidden');
      assert(JSON.parse(await page.locator('#resultContent').inputValue()).partial_failure,'Resource result did not fail');
      page.once('dialog',d=>d.accept());await page.locator('#updateAll').click();await ready();
      const batch=JSON.parse(await page.locator('#resultContent').inputValue());assert(batch.results.some(r=>r.name==='c-web-provider'&&!r.ok),'Batch lost failure');
      assert(vm('sha256sum /home/tester/clashtui-test/config/mihomo/proxies/manual.yaml').split(' ')[0]===cache,'Failure changed cache');
    } finally {vm('rm -f /home/tester/clashtui-test/manual/fail-proxy');}
    return {single_partial_failure:true,batch_failure:true,cache_bytes_preserved:true};
  });
  await check('CB07',async()=>{
    cli('manage activate --name c-web-provider --yes');
    const panel=await context.newPage();await panel.addInitScript(({secret})=>{localStorage.setItem('endpointList',JSON.stringify([{id:'c',url:'http://127.0.0.1:29090',secret}]));localStorage.setItem('selectedEndpoint','c');},{secret:credentials.secret});
    await panel.goto('http://127.0.0.1:29090/ui/#/proxies');await panel.getByText('Manual Select',{exact:true}).first().waitFor();
    if(!await panel.getByText('REJECT',{exact:true}).first().isVisible())await panel.getByText('Manual Select',{exact:true}).first().click();
    await panel.getByText('REJECT',{exact:true}).first().click();
    await panel.waitForFunction(()=>document.body.innerText.includes('Manual Select'));
    assert(cli('core proxies').proxies['Manual Select'].now==='REJECT','Real panel select missing');
    cli('core select "Manual Auto" "Manual A v1"');await panel.reload();
    const unfix=panel.getByTitle(/恢复自动选择|Restore automatic selection|Unfix/i).first();await unfix.waitFor();await unfix.click();
    assert(!cli('core proxies').proxies['Manual Auto'].fixed,'Real automatic selection not restored');
    await panel.screenshot({path:directory+'/panel.png'});await panel.close();
    cli('manage activate --name baseline --yes');return {real_select:true,real_unfix:true};
  });

  await check('CB08',async()=>{
    const panel=await context.newPage();
    await panel.addInitScript(({secret})=>{localStorage.setItem('endpointList',JSON.stringify([{id:'c',url:'http://127.0.0.1:29090',secret}]));localStorage.setItem('selectedEndpoint','c');},{secret:credentials.secret});
    const sockets=[];const frames=[];
    panel.on('websocket',ws=>{if(new URL(ws.url()).pathname==='/logs'){sockets.push(ws);ws.on('framereceived',frame=>frames.push(frame.payload));}});
    await panel.goto('http://127.0.0.1:29090/ui/#/logs');
    for(let i=0;i<50&&!sockets.length;i++)await new Promise(r=>setTimeout(r,100));assert(sockets.length,'Panel logs never opened real WebSocket');
    vm('python3 /home/tester/clashtui-test/manual/lab.py traffic --seconds 1');
    for(let i=0;i<30&&!frames.length;i++)await new Promise(r=>setTimeout(r,100));assert(frames.length,'Panel received no real core logs');
    const before=frames.length;const opened=sockets.length;
    vm('clashtui --config-dir=/home/tester/clashtui-test/config service restart');
    for(let i=0;i<100&&sockets.length===opened;i++)await new Promise(r=>setTimeout(r,100));assert(sockets.length>opened,'Panel logs did not reconnect after real core restart');
    vm('python3 /home/tester/clashtui-test/manual/lab.py traffic --seconds 1');
    for(let i=0;i<30&&frames.length===before;i++)await new Promise(r=>setTimeout(r,100));assert(frames.length>before,'Reconnected logs received no new frame');await panel.close();
    return {actual_log_frames:true,service_restart:true,new_websocket:true,new_frames_after_restart:true};
  });
  await check('CB09',async()=>{
    await connect();await row('c-web-provider').getByRole('button',{name:'编辑文件',exact:true}).click();await ready();
    const button=await row('c-web-provider').getByRole('button',{name:'编辑文件',exact:true}).elementHandle();
    const original=await page.locator('#content').inputValue();await page.locator('#content').fill(original+'\n# unsaved-focus-proof\n');await page.locator('#content').focus();
    await page.waitForTimeout(4500);await ready();assert(await button.evaluate(element=>element.isConnected),'Refresh replaced an unchanged action button');
    assert(await page.locator('#content').inputValue()===original+'\n# unsaved-focus-proof\n','Refresh lost unsaved input');
    assert(await page.evaluate(()=>document.activeElement.id)==='content','Refresh stole editor focus');
    await page.locator('#content').press('Tab');await page.locator('#save').press('Shift+Tab');assert(await page.evaluate(()=>document.activeElement.id)==='content','Keyboard focus did not return');
    await page.locator('#cancel').click();assert(cli('manage read --name c-web-provider').content===original,'Cancel wrote focus-test draft');
    return {two_refresh_cycles:true,draft_preserved:true,keyboard_focus:true,cancel_not_saved:true};
  });
} finally {
  vm('rm -f /home/tester/clashtui-test/manual/fail-proxy');
  await context.close();await browser.close();
  const report={cases:results,passed:results.filter(r=>r.status==='passed').length,failed:results.filter(r=>r.status==='failed').length};
  await writeFile(directory+'/report.json',JSON.stringify(report,null,2));process.exitCode=report.failed?1:0;
}
