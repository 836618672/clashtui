#!/usr/bin/env node
// Default stages only use mocks, temporary directories and loopback sockets.
import {spawn} from 'node:child_process';
import {mkdir,writeFile} from 'node:fs/promises';
import {createWriteStream} from 'node:fs';
import {resolve,join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const options=new Set(process.argv.slice(2));
if([...options].some(option=>!['--browser','--help'].includes(option)))throw new Error('Unknown option; use --help');
if(options.has('--help')){console.log('node scripts/test-pipeline.mjs [--browser]\nRequires Rust (rustfmt/clippy), Node 22+ and Python 3 (Linux PTY stage).\nDefault: fmt, clippy, Rust feature builds/tests, mock DOM, Linux CLI/API and PTY workflows.\n--browser: additionally run installed Playwright against isolated mock fixtures.\nNo core downloads, installations, TUN, system proxy or real service operations.');process.exit(0);}
const reportDir=resolve(process.env.CLASHTUI_REPORT_DIR||join(root,'target','test-results',`${Date.now()}-${randomUUID().slice(0,8)}`));
await mkdir(reportDir,{recursive:true});
const results=[];
const env={...process.env,CARGO_TERM_COLOR:'never'};
let activeChild, interrupted=false;
function killStage(signal){
  if(!activeChild)return;
  try{if(process.platform==='win32')activeChild.kill(signal);else process.kill(-activeChild.pid,signal);}catch{}
}
for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>{interrupted=true;killStage('SIGKILL');});
async function stage(name,command,args,{skip}={}){
  if(interrupted)skip='Pipeline interrupted';
  if(skip){results.push({name,status:'skipped',reason:skip});console.log(`SKIP ${name}: ${skip}`);return;}
  console.log(`RUN ${name}`);const started=Date.now();const logPath=join(reportDir,`${name}.log`);const log=createWriteStream(logPath);
  let status='failed',error;const child=spawn(command,args,{cwd:root,env,detached:process.platform!=='win32',stdio:['ignore','pipe','pipe']});activeChild=child;
  child.stdout.pipe(log,{end:false});child.stderr.pipe(log,{end:false});
  let timedOut=false;const timer=setTimeout(()=>{timedOut=true;killStage('SIGKILL');},20*60*1000);
  try{const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});status=code===0&&!interrupted?'passed':'failed';if(status==='failed')error=timedOut?'Stage timed out':interrupted?'Pipeline interrupted':`exit ${code}`;}catch(e){error=e.message;}finally{clearTimeout(timer);activeChild=undefined;await new Promise(r=>log.end(r));}
  results.push({name,status,error,duration_ms:Date.now()-started,log:logPath});console.log(`${status.toUpperCase()} ${name}${error?`: ${error}`:''}`);
}
await stage('format','cargo',['fmt','--all','--','--check']);
await stage('clippy','cargo',['clippy','--locked','--all-targets','--all-features','--','-D','warnings']);
await stage('rust-all-features','cargo',['test','--locked','--all-features']);
await stage('rust-cli-only','cargo',['test','--locked','--no-default-features']);
await stage('build-tui','cargo',['build','--locked','--all-features']);
await stage('web-dom',process.execPath,['--test','web/index.test.cjs']);
// Cargo metadata respects a custom CARGO_TARGET_DIR; the fixture never guesses
// another executable if the explicitly supplied build is missing.
let metadata='';
const meta=spawn('cargo',['metadata','--no-deps','--format-version','1'],{cwd:root,env});
meta.stdout.on('data',chunk=>metadata+=chunk);meta.stderr.resume();
const metaExit=await new Promise((resolve,reject)=>{meta.once('error',reject);meta.once('exit',resolve);}).catch(()=>-1);
if(metaExit===0)env.CLASHTUI_TEST_BINARY=join(JSON.parse(metadata).target_directory,'debug',process.platform==='win32'?'clashtui.exe':'clashtui');
const buildFailed=results.some(row=>row.name==='build-tui'&&row.status!=='passed');
await stage('core-cli-mocks',process.execPath,['--test','tests/pipeline/core.test.mjs'],{skip:process.platform!=='linux'?'Linux process fixture only':buildFailed?'TUI binary build failed':undefined});
await stage('tui-pty',process.execPath,['--test','tests/pipeline/tui.test.mjs'],{skip:process.platform!=='linux'?'Linux PTY fixture only':buildFailed?'TUI binary build failed':undefined});
await stage('browser','npm',['--prefix','tests/browser','test'],{skip:!options.has('--browser')?'Opt in with --browser after installing Playwright':process.platform!=='linux'?'Linux fixture only':buildFailed?'TUI binary build failed':undefined});
results.push({name:'real-core-and-platform',status:'skipped',reason:'Requires isolated VM acceptance; never enabled by the default pipeline'});
const report={created_at:new Date().toISOString(),platform:process.platform,network_scope:'temporary data and loopback mocks only',passed:results.filter(r=>r.status==='passed').length,failed:results.filter(r=>r.status==='failed').length,skipped:results.filter(r=>r.status==='skipped').length,stages:results};
await writeFile(join(reportDir,'report.json'),JSON.stringify(report,null,2));
await writeFile(join(reportDir,'report.md'),`# Test pipeline\n\n${report.created_at}\n\n| Stage | Result | Details |\n|---|---|---|\n${results.map(row=>`| ${row.name} | ${row.status} | ${row.reason||row.error||row.log||''} |`).join('\n')}\n`);
console.log(`Report: ${reportDir}/report.json`);process.exitCode=report.failed||interrupted?1:0;
