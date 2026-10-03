import {test,expect} from '@playwright/test';
import {readFile,mkdir} from 'node:fs/promises';
import {join,extname,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {execFileSync} from 'node:child_process';
import {once} from 'node:events';
import {fixture} from '../pipeline/fixture.mjs';

test('Locked MetaCubeXD restores automatic selection and CLI reads the result',async({page,context})=>{
  test.skip(process.platform!=='linux'||!process.env.CLASHTUI_METACUBEXD_ARCHIVE,'Set CLASHTUI_METACUBEXD_ARCHIVE to the pinned release archive');
  const f=await fixture();let server;
  try {
    const archive=resolve(process.env.CLASHTUI_METACUBEXD_ARCHIVE);
    expect(createHash('sha256').update(await readFile(archive)).digest('hex')).toBe('a178e00b67acabcda2dcef00afa90be6a7bb261e466a67dad58c8478d9553603');
    const directory=join(f.root,'locked-panel');await mkdir(directory);
    execFileSync('tar',['-xzf',archive,'-C',directory]);
    server=createServer(async(req,res)=>{
      try {
        const path=decodeURIComponent(new URL(req.url,'http://localhost').pathname);
        if(path.includes('..')){res.writeHead(400);res.end();return;}
        let file=join(directory,path);
        if(!extname(path))file=join(directory,'index.html');
        const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.json':'application/json','.svg':'image/svg+xml'};
        res.writeHead(200,{'Content-Type':types[extname(file)]||'application/octet-stream'});res.end(await readFile(file));
      }catch{res.writeHead(404);res.end();}
    });
    server.listen(0,'127.0.0.1');await once(server,'listening');
    const origin=`http://127.0.0.1:${server.address().port}`;
    await context.route('**/*',async route=>{
      const url=new URL(route.request().url());
      if(url.origin===origin)return route.continue();
      if(url.origin!==f.endpoint)return route.abort();
      const headers={'access-control-allow-origin':origin,'access-control-allow-headers':'authorization,content-type','access-control-allow-methods':'GET,PUT,POST,PATCH,DELETE,OPTIONS'};
      if(route.request().method()==='OPTIONS')return route.fulfill({status:204,headers});
      const response=await route.fetch();return route.fulfill({response,headers:{...response.headers(),...headers}});
    });
    await context.addInitScript(({endpoint,secret})=>{
      localStorage.setItem('endpointList',JSON.stringify([{id:'mock',url:endpoint,secret}]));
      localStorage.setItem('selectedEndpoint','mock');
    },{endpoint:f.endpoint,secret:f.secret});
    await page.goto(origin+'/#/proxies');
    await expect(page.getByTitle(/恢复自动选择|Restore automatic selection|Unfix/i)).toBeVisible();
    await page.getByTitle(/恢复自动选择|Restore automatic selection|Unfix/i).click();
    await expect.poll(async()=>(await f.json(['core','proxies'])).proxies[f.group].fixed).toBe('');
    expect(f.requests.some(r=>r.method==='DELETE'&&r.path.startsWith('/proxies/'))).toBe(true);
  }finally{
    // Finish interception before closing its HTTP fixtures. The panel polls
    // continuously, so fixture shutdown while route.fetch is active can reset
    // an otherwise successful request during test teardown.
    await context.unrouteAll({behavior:'wait'});
    await page.close();
    if(server){server.closeAllConnections();await new Promise(r=>server.close(r));}
    await f.close();
  }
});
