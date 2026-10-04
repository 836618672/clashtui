import {mkdtemp, mkdir, writeFile, rm, access} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import {once} from 'node:events';

export async function fixture() {
  if (process.platform !== 'linux') throw new Error('Process fixtures require Linux; service commands must stay mocked');
  const binary = resolve(process.env.CLASHTUI_TEST_BINARY || join(dirname(fileURLToPath(import.meta.url)),'../../target/debug/clashtui'));
  await access(binary);
  const root = await mkdtemp(join(tmpdir(), 'clashtui-pipeline-'));
  const secret = 'mock-core-token';
  const state = {mode:'rule', 'log-level':'info', 'mixed-port':27890, 'tun':{enable:false,stack:'mixed'}};
  const backend = {version:'v1.19.0',meta:true};
  const group = 'group / 中文', node = 'node / 中文';
  const proxies = {[group]:{name:group,type:'URLTest',all:['DIRECT',node],now:'DIRECT',fixed:node},[node]:{name:node,type:'Direct'}};
  const rules = [{type:'Match',payload:'',proxy:group,disabled:false}];
  const provider = 'provider / 中文';
  const requests = [];
  const connections = ['match / 中文','keep'].map((id,i)=>({id,metadata:{network:'tcp',type:'HTTP',host:i?'keep.test':'match.test',sourceIP:'127.0.0.1',sourcePort:'12345',process:i?'keep':'worker',extraField:'preserved'},upload:10,download:20,start:'2026-01-01T00:00:00Z',chains:[group],rule:'Match',opaque:{keep:true}}));
  const faults = new Map();
  const delays = new Map();
  const core = createServer(async (req,res)=>{
    let body=''; for await(const chunk of req) body+=chunk;
    const url = new URL(req.url,'http://localhost');
    const path = url.pathname;
    requests.push({method:req.method,path,query:Object.fromEntries(url.searchParams),body,auth:req.headers.authorization});
    const send=(value,status=200)=>{res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(value));};
    if(path==='/subscription') return send({proxies:[]});
    if(req.headers.authorization!==`Bearer ${secret}`) return send({error:'auth'},401);
    if(faults.has(path)) return send({error:`mock HTTP ${faults.get(path)}`},faults.get(path));
    if(delays.has(path)) await new Promise(resolve=>setTimeout(resolve,delays.get(path)));
    if(path==='/dns/query') return send({Question:[{Name:url.searchParams.get('name'),Type:url.searchParams.get('type')}],Answer:[{Data:'192.0.2.1'}]});
    if(path==='/version') return send(backend);
    if(path==='/configs') {if(req.method==='PATCH') Object.assign(state,JSON.parse(body));return send(state);}
    if(path==='/proxies') return send({proxies});
    if(path.startsWith('/proxies/') && req.method==='DELETE'){proxies[decodeURIComponent(path.slice(9))].fixed='';return send({});}
    if(path.startsWith('/proxies/') && path.endsWith('/delay')) return send({delay:42});
    if(path.startsWith('/group/') && path.endsWith('/delay')) return send({[node]:42,DIRECT:1});
    if(path.startsWith('/proxies/') && req.method==='PUT'){proxies[decodeURIComponent(path.slice(9))].now=JSON.parse(body).name;return send({});}
    if(path==='/rules') return send({rules});
    if(path==='/rules/disable'){for(const [i,disabled] of Object.entries(JSON.parse(body))) rules[Number(i)].disabled=disabled;return send({});}
    if(path==='/providers/proxies'||path==='/providers/rules') return send({providers:{[provider]:{name:provider,type:'HTTP',vehicleType:'HTTP',proxies:[{name:'same / node',type:'Direct'}],ruleCount:1,opaque:true}}});
    if(path.startsWith('/providers/proxies/') && path.endsWith('/healthcheck') && path.split('/').length===6) return send({delay:73});
    if(path.startsWith('/providers/')) return send({});
    if(path==='/connections'){if(req.method==='DELETE') connections.length=0;return send({downloadTotal:100,uploadTotal:50,connections});}
    if(path.startsWith('/connections/') && req.method==='DELETE'){const i=connections.findIndex(c=>c.id===decodeURIComponent(path.slice(13)));if(i>=0)connections.splice(i,1);res.writeHead(204);return res.end();}
    if(['/restart','/upgrade','/cache/dns/flush','/cache/fakeip/flush','/configs/geo'].includes(path)) return send({});
    return send({error:'unimplemented mock route'},404);
  });
  const sockets=new Set();
  core.on('upgrade',(req,socket)=>{
    socket.streamPath=req.url;
    sockets.add(socket);socket.once('close',()=>sockets.delete(socket));
    if(req.headers.authorization!==`Bearer ${secret}`){socket.end('HTTP/1.1 401 Unauthorized\r\n\r\n');return;}
    if(faults.has(new URL(req.url,'http://localhost').pathname)){socket.end('HTTP/1.1 503 Unavailable\r\n\r\n');return;}
    requests.push({method:'WS',path:req.url,auth:req.headers.authorization});
    const accept=createHash('sha1').update(req.headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    const value=req.url.startsWith('/memory')?{inuse:1024}:req.url.startsWith('/traffic')?{up:10,down:20}:req.url.startsWith('/connections')?{downloadTotal:100,uploadTotal:50,connections}:{type:'info',payload:'mock log preserved'};
    const payload=Buffer.from(JSON.stringify(value));
    const header=payload.length<126?Buffer.from([0x81,payload.length]):Buffer.from([0x81,126,payload.length>>8,payload.length&255]);
    socket.write(Buffer.concat([header,payload]));
    socket.on('data',()=>socket.end());socket.on('error',()=>{});
  });
  core.listen(0,'127.0.0.1');await once(core,'listening');
  const endpoint=`http://127.0.0.1:${core.address().port}`;
  for(const dir of ['bin','mihomo/profiles','mihomo/templates'])await mkdir(join(root,dir),{recursive:true});
  await writeFile(join(root,'bin/systemctl'),'#!/bin/sh\nexit 3\n',{mode:0o700});
  await writeFile(join(root,'bin/core'),'#!/bin/sh\nexit 0\n',{mode:0o700});
  const section=name=>({core:{config_dir:join(root,name),config_path:join(root,name,name==='mihomo'?'config.yaml':'config.json'),bin_path:join(root,'bin/core')},core_service:{service_name:'mock-unused',is_user:true,service_controller:'systemd'}});
  await writeFile(join(root,'config.yaml'),JSON.stringify({mihomo:section('mihomo'),timeout:2}));
  await writeFile(join(root,'mihomo/core_override_config.yaml'),JSON.stringify({'external-controller':endpoint,secret,'mixed-port':27890,tun:{enable:false},dns:{enable:false}}));
  const children=new Set();
  function start(args) {
    const child=spawn(binary,[`--config-dir=${root}`,...args],{env:{...process.env,PATH:`${join(root,'bin')}:${process.env.PATH}`},stdio:['ignore','pipe','pipe']});
    children.add(child);child.once('close',()=>children.delete(child));child.on('error',error=>{child.startupError=error;});return child;
  }
  async function run(args,{success=true}={}) {
    const child=start(args);let stdout='',stderr='';
    child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
    const timer=setTimeout(()=>child.kill('SIGKILL'),15000);
    try{const [code]=await once(child,'close');if(code===null)throw new Error(`${args.join(' ')}: terminated (${child.signalCode}); ${stderr}`);if((code===0)!==success)throw new Error(`${args.join(' ')}: exit ${code}: ${stderr}`);return {stdout,stderr,code};}finally{clearTimeout(timer);}
  }
  const json=async args=>JSON.parse((await run(args)).stdout);
  let web,webPort;
  async function startWeb(){
    const reservation=createServer();reservation.listen(webPort||0,'127.0.0.1');await once(reservation,'listening');const port=reservation.address().port;webPort=port;await new Promise(r=>reservation.close(r));
    const token='mock-independent-management-token';const tokenFile=join(root,'token');await writeFile(tokenFile,token,{mode:0o600});
    web=start(['web','--listen',`127.0.0.1:${port}`,'--token-file',tokenFile]);web.stderr.resume();web.stdout.resume();
    const url=`http://127.0.0.1:${port}`;
    for(let i=0;i<100;i++){try{if((await fetch(url,{signal:AbortSignal.timeout(500)})).ok)return {url,token};}catch{}if(web.startupError)throw web.startupError;if(web.exitCode!==null)throw new Error('Web fixture exited');await new Promise(r=>setTimeout(r,20));}throw new Error('Web startup timed out');
  }
  async function close(){for(const child of children){if(child.pid){child.kill('SIGKILL');await once(child,'close').catch(()=>{});}}for(const socket of sockets)socket.destroy();core.closeAllConnections();await new Promise(r=>core.close(r));await rm(root,{recursive:true,force:true});}
  async function restartWeb(){web.kill('SIGTERM');await once(web,'close');return startWeb();}
  function disconnectStreams(){for(const socket of sockets)socket.destroy();}
  function emitLog(value){const payload=Buffer.from(JSON.stringify(value));const header=payload.length<126?Buffer.from([0x81,payload.length]):Buffer.from([0x81,126,payload.length>>8,payload.length&255]);for(const socket of sockets)if(socket.streamPath.startsWith('/logs'))socket.write(Buffer.concat([header,payload]));}
  return {root,binary,secret,endpoint,state,backend,group,node,provider,requests,connections,rules,faults,delays,start,run,json,startWeb,close,restartWeb,disconnectStreams,emitLog};
}
