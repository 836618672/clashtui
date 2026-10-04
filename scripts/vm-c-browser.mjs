import { answerDialog, finishDialogs } from "../tests/browser/dialogs.mjs";
// Actual VM APIs and real browser input. Never mock/force a business response.
import {chromium} from '../tests/browser/node_modules/playwright/index.mjs';
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const coreUrl=`http://127.0.0.1:${process.env.CLASHTUI_VM_CORE_PORT||29090}`;
const webUrl=`http://127.0.0.1:${process.env.CLASHTUI_VM_WEB_PORT||29091}`;
const directory=resolve(process.env.CLASHTUI_C_BROWSER_REPORT||'target/vm/c-browser');
await mkdir(directory,{recursive:true});
const vm=command=>execFileSync('bash',['scripts/vm.sh','ssh',command],{encoding:'utf8',timeout:45000});
const credentials=JSON.parse(vm(`python3 -c 'import json,yaml; from pathlib import Path; p=Path("/home/tester/clashtui-test/config"); print(json.dumps({"token":(p/"management-token").read_text().strip(),"secret":yaml.safe_load((p/"mihomo/core_override_config.yaml").read_text())["secret"]}))'`));
const cli=args=>JSON.parse(vm('clashtui --config-dir=/home/tester/clashtui-test/config '+args));
const assert=(value,message)=>{if(!value)throw new Error(message);};
const browser=await chromium.launch({headless:true,executablePath:process.env.CLASHTUI_CHROMIUM_PATH});
const context=await browser.newContext({serviceWorkers:'block',acceptDownloads:true});
context.setDefaultTimeout(12000);
await context.route('**/*',route=>[coreUrl,webUrl].includes(new URL(route.request().url()).origin)?route.continue():route.abort());
await context.tracing.start({screenshots:true,snapshots:true,sources:true});
const page=await context.newPage();
const results=[];
async function ready(){await finishDialogs(page);await page.waitForFunction(()=>{const button=document.querySelector('#refresh')||document.querySelector('#connect');return button&&!button.disabled;});}
async function connect(){await page.goto(webUrl+'/');if(await page.locator('#token').isVisible()){await page.locator('#token').fill(credentials.token);await page.locator('#connect').click();}await page.locator('#main').waitFor();await ready();await page.getByRole('link',{name:'订阅与配置',exact:true}).click();}
const row=name=>page.locator('#profiles tr').filter({has:page.locator('td strong').filter({hasText:new RegExp('^'+name+'$')})});
async function check(id,work){
  const started=Date.now();await context.tracing.startChunk({title:id});
  try {const evidence=await work();results.push({id,status:'passed',evidence:evidence||{},duration_ms:Date.now()-started});await context.tracing.stopChunk();}
  catch(error){let message=String(error);for(const value of Object.values(credentials))message=message.replaceAll(value,'[redacted]');results.push({id,status:'failed',error:message.slice(0,3000),duration_ms:Date.now()-started});await page.screenshot({path:directory+'/'+id+'.png'}).catch(()=>{});await context.tracing.stopChunk({path:directory+'/'+id+'-trace.zip'}).catch(()=>{});}
  finally {await finishDialogs(page);}
  console.log(JSON.stringify(results.at(-1)));
}
try {
  await check('CB01',async()=>{
    await page.goto(webUrl+'/');await page.locator('#token').fill(credentials.secret);await page.locator('#connect').click();await ready();
    assert(await page.locator('#main').isHidden(),'Core secret authenticated management');
    for(const secret of [credentials.token,'wrong-core-secret']){
      const rejected=await context.request.get(coreUrl+'/version',{headers:{Authorization:'Bearer '+secret}});
      assert(rejected.status()===401,'Management token accepted by core');
    }
    await connect();await page.getByRole('link',{name:'节点与分组',exact:true}).click();
    assert(!await page.locator('#main').innerText().then(v=>v.includes(credentials.secret)),'Core secret exposed to browser');
    return {credential_separation:true,core_401:true,single_management_session:true};
  });
  await check('CB02',async()=>{
    await connect();await page.locator('#name').fill('c-web-url');await page.locator('#url').fill('http://127.0.0.1:18080/subscription.yaml');await page.locator('#create').click();await ready();
    assert(await row('c-web-url').count()===1,'URL creation missing');
    await page.locator('#profileFilter').fill('c-web-url');assert(await page.locator('#profiles tr').count()===1,'Profile filter ignored');
    answerDialog(page,d=>d.accept('c-web-renamed'));answerDialog(page,d=>d.accept('http://127.0.0.1:18080/subscription.yaml'));
    await row('c-web-url').getByRole('button',{name:'编辑名称/URL',exact:true}).click();await ready();await finishDialogs(page);
    assert(cli('manage profile_url --name c-web-renamed').url.includes('/subscription.yaml'),'Rename not visible to CLI');
    await page.locator('#profileFilter').fill('');await row('c-web-renamed').getByRole('button',{name:'编辑文件',exact:true}).click();await ready();
    const content=await page.locator('#content').inputValue();const downloaded=page.waitForEvent('download');await page.locator('#download').click();const download=await downloaded;const path=directory+'/profile-export.yaml';await download.saveAs(path);
    assert(await readFile(path,'utf8')===content,'Export bytes differ');
    await page.locator('#content').fill(content+'\n# c-web-edited\n');await page.locator('#save').click();await ready();
    assert(cli('manage read --name c-web-renamed').content.includes('c-web-edited'),'Web save absent in CLI');
    await page.locator('#content').fill('discard this unsaved draft');answerDialog(page,d=>d.accept());await page.locator('#cancel').click();await ready();assert(!cli('manage read --name c-web-renamed').content.includes('discard'),'Cancel saved draft');
    answerDialog(page,d=>d.accept());await row('c-web-renamed').getByRole('button',{name:'删除',exact:true}).click();await ready();
    assert(!cli('manage state').profiles.some(p=>p.name==='c-web-renamed'),'Delete absent in CLI');
    return {filter:true,rename_url:true,export_exact_bytes:true,save_readback:true,cancel:true,delete:true};
  });
  await check('CB03',async()=>{
    await connect();await page.getByRole('link',{name:'设置',exact:true}).click();
    for(const mode of ['global','rule']){
      await page.getByLabel('mode',{exact:true}).selectOption(mode);await page.getByRole('button',{name:'应用运行设置',exact:true}).click();await ready();
      assert(cli('core status').config.mode===mode,'Runtime patch absent');
      answerDialog(page,d=>d.accept());await page.getByRole('button',{name:'持久化运行设置',exact:true}).click();await ready();
      assert(cli('manage read --kind override').content.includes(mode),'Persist missing override');await page.locator('#closeResult').click();
    }
    return {runtime_patch:true,persist:true,restored_rule:true};
  });
  await check('CB04',async()=>{
    await connect();await page.getByRole('link',{name:'服务与维护',exact:true}).click();
    for(const operation of ['stop','start','restart']){
      answerDialog(page,d=>d.accept());await page.locator(`#${operation}`).click();await ready();
      const active=vm('systemctl is-active clashtui-test-mihomo || true').trim();assert(active===(operation==='stop'?'inactive':'active'),'Service '+operation+' state '+active);
      if(operation!=='stop')assert(cli('core status').version.meta,'Service did not become ready');
    }
    return {stop:true,start:true,restart:true,core_ready:true};
  });
  await check('CB05',async()=>{
    await connect();await page.getByRole('link',{name:'模板管理',exact:true}).click();answerDialog(page,d=>d.accept('c-web-template.yaml'));await page.locator('#newTemplate').click();await ready();
    const source=vm('cat /home/tester/clashtui-test/manual/template.yaml');await page.locator('#content').fill(source);await page.locator('#save').click();await ready();await page.locator('#templates').selectOption('c-web-template.yaml');
    answerDialog(page,d=>d.accept('{}'));await page.locator('#templateProviders').click();await ready();assert(Object.keys(cli('manage template_providers --name c-web-template.yaml').groups).length===0,'Empty groups lost');
    answerDialog(page,d=>d.accept('{"manual":{"Manual HTTP":"http://127.0.0.1:18080/proxy.yaml"}}'));await page.locator('#templateProviders').click();await ready();
    await page.locator('#previewTemplate').click();await ready();assert((await page.locator('#resultContent').inputValue()).includes('Manual HTTP'),'Preview missing provider');
    await page.locator('#closeResult').click();await page.locator('#generated').fill('c-web-generated');await page.locator('#generate').click();await ready();assert(cli('manage check --name c-web-generated').valid,'Generated invalid');
    answerDialog(page,d=>d.accept());await page.locator('#deleteTemplate').click();await ready();assert((await page.locator('#notice').textContent()).includes('referenced'),'Referenced template deletion accepted');
    await page.getByRole('link',{name:'订阅与配置',exact:true}).click();
    answerDialog(page,d=>d.accept());await row('c-web-generated').getByRole('button',{name:'删除',exact:true}).click();await ready();
    await page.getByRole('link',{name:'模板管理',exact:true}).click();
    answerDialog(page,d=>d.accept());await page.locator('#deleteTemplate').click();await ready();assert(!cli('manage state').templates.includes('c-web-template.yaml'),'Template deletion absent');
    return {groups_clear_save:true,preview:true,generate_check:true,reference_guard:true,delete:true};
  });
  await check('CB06',async()=>{
    await connect();await page.locator('#name').fill('c-web-provider');await page.locator('#importFile').setInputFiles({name:'provider.yaml',mimeType:'text/yaml',buffer:Buffer.from(vm('cat /home/tester/clashtui-test/manual/providers-yaml.yaml'))});await page.locator('#import').click();await ready();
    await row('c-web-provider').getByRole('button',{name:'更新',exact:true}).click();await ready();
    await page.locator('#closeResult').click();
    const cache=vm('sha256sum /home/tester/clashtui-test/config/mihomo/proxies/manual.yaml').split(' ')[0];
    vm('touch /home/tester/clashtui-test/manual/fail-proxy');
    try {
      await row('c-web-provider').getByRole('button',{name:'更新',exact:true}).click();await ready();
      assert((await page.locator('#notice').textContent()).includes('部分订阅资源更新失败'),'Partial failure hidden');
      assert(JSON.parse(await page.locator('#resultContent').inputValue()).partial_failure,'Resource result did not fail');
      await page.locator('#closeResult').click();answerDialog(page,d=>d.accept());await page.locator('#updateAll').click();await ready();
      const batch=JSON.parse(await page.locator('#resultContent').inputValue());assert(batch.results.some(r=>r.name==='c-web-provider'&&!r.ok),'Batch lost failure');
      assert(vm('sha256sum /home/tester/clashtui-test/config/mihomo/proxies/manual.yaml').split(' ')[0]===cache,'Failure changed cache');
    } finally {vm('rm -f /home/tester/clashtui-test/manual/fail-proxy');}
    return {single_partial_failure:true,batch_failure:true,cache_bytes_preserved:true};
  });
  await check('CB07',async()=>{
    cli('manage activate --name c-web-provider --yes');
    await connect();await page.getByRole('link',{name:'节点与分组',exact:true}).click();
    const group=page.locator('.proxy-group').filter({has:page.getByRole('heading',{name:'Manual Select',exact:true})});
    await group.getByRole('button',{name:/^REJECT/}).click();await ready();
    assert(cli('core proxies').proxies['Manual Select'].now==='REJECT','Real dashboard select missing');
    cli('core select "Manual Auto" "Manual A v1"');
    const auto=page.locator('.proxy-group').filter({has:page.getByRole('heading',{name:'Manual Auto',exact:true})});
    await auto.getByRole('button',{name:'恢复自动选择',exact:true}).click();await ready();
    assert(!cli('core proxies').proxies['Manual Auto'].fixed,'Real automatic selection not restored');
    await page.screenshot({path:directory+'/dashboard.png'});
    cli('manage activate --name baseline --yes');return {real_select:true,real_unfix:true};
  });
  await check('CB08',async()=>{
    await connect();await page.getByRole('link',{name:'日志',exact:true}).click();
    vm('python3 /home/tester/clashtui-test/manual/lab.py traffic --seconds 1');
    await page.locator('.log-row').first().waitFor();
    const before=await page.evaluate(async token=>{const r=await fetch('/api/core/read',{method:'POST',headers:{Authorization:'Bearer '+token,'Content-Type':'application/json'},body:JSON.stringify({view:'streams'})});return (await r.json()).cursor;},credentials.token);
    vm('clashtui --config-dir=/home/tester/clashtui-test/config service restart');
    await page.waitForTimeout(2500);vm('python3 /home/tester/clashtui-test/manual/lab.py traffic --seconds 1');
    await page.waitForFunction(async({token,before})=>{const r=await fetch('/api/core/read',{method:'POST',headers:{Authorization:'Bearer '+token,'Content-Type':'application/json'},body:JSON.stringify({view:'streams',after:before})});const data=await r.json();return data.cursor>before&&data.status.logs.connected;},{token:credentials.token,before},{timeout:20000});
    return {actual_log_frames:true,service_restart:true,rust_stream_reconnected:true,new_frames_after_restart:true};
  });
  await check('CB09',async()=>{
    await connect();await row('c-web-provider').getByRole('button',{name:'编辑文件',exact:true}).click();await ready();
    const button=await row('c-web-provider').getByRole('button',{name:'编辑文件',exact:true}).elementHandle();
    const original=await page.locator('#content').inputValue();await page.locator('#content').fill(original+'\n# unsaved-focus-proof\n');await page.locator('#content').focus();
    await page.waitForTimeout(4500);await ready();assert(await button.evaluate(element=>element.isConnected),'Refresh replaced an unchanged action button');
    assert(await page.locator('#content').inputValue()===original+'\n# unsaved-focus-proof\n','Refresh lost unsaved input');
    assert(await page.evaluate(()=>document.activeElement.id)==='content','Refresh stole editor focus');
    await page.locator('#content').press('Tab');assert(await page.evaluate(()=>document.activeElement.id)==='download','Keyboard focus did not return');
    answerDialog(page,d=>d.accept());await page.locator('#cancel').click();await ready();assert(cli('manage read --name c-web-provider').content===original,'Cancel wrote focus-test draft');
    return {two_refresh_cycles:true,draft_preserved:true,keyboard_focus:true,cancel_not_saved:true};
  });
} finally {
  vm('rm -f /home/tester/clashtui-test/manual/fail-proxy');
  await context.close();await browser.close();
  const report={cases:results,passed:results.filter(r=>r.status==='passed').length,failed:results.filter(r=>r.status==='failed').length};
  await writeFile(directory+'/report.json',JSON.stringify(report,null,2));process.exitCode=report.failed?1:0;
}
