import { answerDialog, finishDialogs } from "../tests/browser/dialogs.mjs";
// Run against the actual guest through the local SSH tunnels. No mocked API.
import {chromium} from '../tests/browser/node_modules/playwright/index.mjs';
import {execFileSync} from 'node:child_process';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const coreUrl=`http://127.0.0.1:${process.env.CLASHTUI_VM_CORE_PORT||29090}`;
const webUrl=`http://127.0.0.1:${process.env.CLASHTUI_VM_WEB_PORT||29091}`;
const directory=resolve(process.env.CLASHTUI_BASE_BROWSER_REPORT||'target/vm/browser');await mkdir(directory,{recursive:true});
const vm=command=>execFileSync('bash',['scripts/vm.sh','ssh',command],{encoding:'utf8',timeout:45000});
const creds=JSON.parse(vm(`python3 -c 'import json,yaml; from pathlib import Path; p=Path("/home/tester/clashtui-test/config"); print(json.dumps({"token":(p/"management-token").read_text(),"secret":yaml.safe_load((p/"mihomo/core_override_config.yaml").read_text())["secret"]}))'`));
const cli=args=>JSON.parse(vm('clashtui --config-dir=/home/tester/clashtui-test/config '+args));
const browser=await chromium.launch({executablePath:process.env.CLASHTUI_CHROMIUM_PATH,headless:true});
const context=await browser.newContext({serviceWorkers:'block'});
context.setDefaultTimeout(10000);
await context.route('**/*',route=>[coreUrl,webUrl].includes(new URL(route.request().url()).origin)?route.continue():route.abort());
const results=[];
async function check(name,work){try{await work();results.push({name,status:'passed'});}catch(error){results.push({name,status:'failed',notice:await page.locator('#notice').textContent().catch(()=>''),error:String(error).replaceAll(creds.token,'[redacted]').replaceAll(creds.secret,'[redacted]').slice(0,1800)});}finally{await finishDialogs(page);}console.log(JSON.stringify(results.at(-1)));}
const assert=(value,message)=>{if(!value)throw new Error(message);};
async function connect(page){await page.goto(webUrl+'/');if(await page.locator('#token').isVisible()){await page.locator('#token').fill(creds.token);await page.locator('#connect').click();}await page.locator('#main').waitFor({state:'visible'});await ready(page);await page.getByRole('link',{name:'订阅与配置',exact:true}).click();}
async function ready(page){await finishDialogs(page);await page.waitForFunction(()=>{const button=document.querySelector('#refresh')||document.querySelector('#connect');return button&&!button.disabled;});}
const page=await context.newPage();
let row;
try{
  await check('Real management authentication/import/save/CLI readback',async()=>{
    await page.goto(webUrl+'/');await page.locator('#token').fill('wrong-token');await page.locator('#connect').click();await ready(page);
    assert(await page.locator('#main').isHidden(),'Wrong token opened management');
    await connect(page);await page.locator('#name').fill('vm-browser');
    await page.locator('#importFile').setInputFiles({name:'vm.yaml',mimeType:'text/yaml',buffer:Buffer.from('proxies: []\nmode: rule\n')});
    await page.locator('#import').click();await ready(page);
    row=page.locator('#profiles tr').filter({hasText:'vm-browser'});
    assert(await row.count()===1,'Import missing');
    await row.getByRole('button',{name:'编辑文件',exact:true}).click();await ready(page);
    await page.locator('#content').fill('proxies: []\nmode: direct\n');await page.locator('#save').click();await ready(page);
    assert(cli('manage read --name vm-browser').content.includes('mode: direct'),'CLI cannot see browser save');
  });
  await check('Real browser stale editor conflict and cancelled delete',async()=>{
    const second=await context.newPage();await connect(second);
    const secondRow=second.locator('#profiles tr').filter({hasText:'vm-browser'});
    await secondRow.getByRole('button',{name:'编辑文件',exact:true}).click();await ready(second);
    await page.locator('#content').fill('proxies: []\nmode: rule\n');await page.locator('#save').click();await ready(page);
    await second.locator('#content').fill('proxies: []\nmode: global\n');await second.locator('#save').click();await ready(second);
    assert((await second.locator('#notice').textContent()).includes('Revision conflict'),'Stale save was not rejected');
    await ready(page);answerDialog(page,dialog=>dialog.dismiss());await row.getByRole('button',{name:'删除',exact:true}).click();await ready(page);
    assert(cli('manage read --name vm-browser').content.includes('mode: rule'),'Cancellation changed profile');await second.close();
  });
  await check('Real malformed YAML browser/CLI diagnostics',async()=>{
    vm(`python3 -c 'from pathlib import Path; Path("/home/tester/clashtui-test/config/mihomo/profiles/vm-browser.yaml").write_text("[unterminated")'`);
    for(const action of ['校验','测试']){
      cli('core status'); // Read-only CLI must preserve the browser revision.
      await ready(page);await row.getByRole('button',{name:action,exact:true}).click();await ready(page);
      const value=JSON.parse(await page.locator('#resultContent').inputValue());
      assert(value.valid===false&&value.exit_code!==0,'Malformed YAML accepted');
      assert(value.stdout||value.stderr,'Core diagnostics missing');
      assert((await page.locator('#notice').textContent()).includes('校验失败'),'Failure notice missing');
      await page.locator('#closeResult').click();
    }
  });
  await check('Real Web template save/preview/generate and CLI validation',async()=>{
    await page.getByRole('link',{name:'模板管理',exact:true}).click();
    answerDialog(page,dialog=>dialog.accept('vm-browser-template.yaml'));await page.locator('#newTemplate').click();await ready(page);
    await page.locator('#content').fill('proxies: []\nproxy-providers: {}\nrule-providers: {}\nproxy-groups: []\nrules: ["MATCH,DIRECT"]\n');
    await page.locator('#save').click();await ready(page);
    await page.locator('#templates').selectOption('vm-browser-template.yaml');await page.locator('#previewTemplate').click();await ready(page);
    assert((await page.locator('#resultContent').inputValue()).includes('MATCH'),'Preview missing');
    await page.locator('#closeResult').click();
    await page.locator('#generated').fill('vm-browser-generated');await page.locator('#generate').click();await ready(page);
    assert(cli('manage check --name vm-browser-generated').valid,'Generated real-core check failed');
  });
  await check('Bundled Vue live selection and CLI readback',async()=>{
    await page.getByRole('link',{name:'节点与分组',exact:true}).click();
    const group=page.locator('.proxy-group').filter({has:page.getByRole('heading',{name:'Test Select',exact:true})});
    await group.getByRole('button',{name:/^REJECT/}).click();await ready(page);
    assert(cli('core proxies').proxies['Test Select'].now==='REJECT','Selection did not reach actual core');
    cli('core select "Test Select" DIRECT');
    await page.screenshot({path:directory+'/dashboard.png'});
  });
  await page.screenshot({path:directory+'/management.png'});
}finally{
  await context.close();await browser.close();
  process.exitCode=results.some(result=>result.status==='failed')?1:0;
  await writeFile(directory+'/report.json',JSON.stringify({passed:results.filter(r=>r.status==='passed').length,failed:results.filter(r=>r.status==='failed').length,cases:results},null,2));
}
