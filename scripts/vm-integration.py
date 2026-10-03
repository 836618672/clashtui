"""Real Linux guest acceptance; never run on the host. All traffic is guest loopback."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request

assert socket.gethostname() == 'clashtui-test', 'Guest only'
ROOT = Path('/home/tester/clashtui-test/config')
WORK = ROOT.parent / 'acceptance'
WORK.mkdir(exist_ok=True)
CLI = ['clashtui', '--config-dir=' + str(ROOT)]
TOKEN = (ROOT / 'management-token').read_text()
SECRET = json.loads((ROOT / 'mihomo/core_override_config.yaml').read_text())['secret']
results = []
paths = []


def case(name, work, known=False):
    started = time.monotonic()
    try:
        work()
        row = {'name': name, 'status': 'passed'}
    except NotImplementedError as error:
        row = {'name': name, 'status': 'skipped', 'reason': str(error)}
    except Exception as error:
        message = str(error).replace(TOKEN, '[redacted]').replace(SECRET, '[redacted]')
        row = {'name': name, 'status': 'known_failure' if known else 'failed', 'error': message[:2000]}
    row['duration_ms'] = int((time.monotonic() - started) * 1000)
    results.append(row)
    print(json.dumps(row), flush=True)


def require(condition, message):
    assert condition, message


def cli(*args, success=True):
    process = subprocess.run(CLI + list(args), capture_output=True, text=True, timeout=40)
    require((process.returncode == 0) == success, f'{args}: exit={process.returncode} {process.stderr}')
    return json.loads(process.stdout) if process.stdout.strip().startswith(('{', '[')) else process.stdout


def input_file(value, name='input.json'):
    path = WORK / name
    path.write_text(json.dumps(value) if not isinstance(value, str) else value)
    return str(path)


def request(path, value=None, token=TOKEN, base='http://127.0.0.1:9091'):
    data = None if value is None else json.dumps(value).encode()
    req = urllib.request.Request(base + path, data=data, headers={
        'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=10) as response:
        return json.load(response)


def action(action, **extra):
    state = request('/api/state')
    submitted = request('/api/action', {'action':action, 'core':state['core'], 'revision':state['revision'], **extra})
    for _ in range(100):
        job = request('/api/job')
        require(job['id'] == submitted['job'], 'Job replaced unexpectedly')
        if not job['pending']:
            return job['result']
        time.sleep(.05)
    raise AssertionError('Web job timeout')


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        paths.append(self.path)
        if self.path.startswith('/probe'):
            time.sleep(.05)
            self.send_response(204)
            self.end_headers()
            return
        if self.path == '/fail':
            self.send_response(503)
            self.end_headers()
            return
        if self.path == '/text':
            content = b'example.test\n+.example.org\n'
        elif self.path == '/mrs':
            content = (WORK/'rules.mrs').read_bytes()
        elif self.path == '/bad-mrs':
            content = b'MRS\x01corrupted'
        elif self.path == '/proxy':
            content = b'proxies:\n  - name: Local A\n    type: direct\n'
        elif self.path == '/rules':
            content = b'payload:\n  - example.test\n'
        else:
            content = b'proxies: []\nrules: [MATCH,DIRECT]\n'
        self.send_response(200)
        self.send_header('Content-Length', str(len(content)))
        self.send_header('Subscription-Userinfo', 'upload=10; download=20; total=100; expire=2000000000')
        self.end_headers()
        self.wfile.write(content)
    do_HEAD = do_GET


server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
URL = f'http://127.0.0.1:{server.server_port}'
base_profile = {'proxies':[], 'proxy-groups':[{'name':'Test Select','type':'select','proxies':['DIRECT','REJECT']}], 'rules':['MATCH,DIRECT']}


def authentication():
    for base, path in [('http://127.0.0.1:9091','/api/state'), ('http://127.0.0.1:9090','/configs')]:
        try:
            request(path, token='wrong-token', base=base)
            raise AssertionError('Wrong token accepted')
        except urllib.error.HTTPError as error:
            require(error.code in [401,403], str(error.code))


def documents():
    cli('manage','import','--name','vm-file','--input',input_file(base_profile))
    doc = cli('manage','read','--name','vm-file')
    updated = {**base_profile, 'log-level':'debug'}
    saved = action('save', kind='profile',name='vm-file',content=json.dumps(updated), revision=doc['revision'])
    require(saved['ok'], str(saved))
    require('debug' in cli('manage','read','--name','vm-file')['content'], 'Web save not visible to CLI')
    stale = action('save',kind='profile',name='vm-file',content=json.dumps(base_profile),revision=doc['revision'])
    require(not stale['ok'] and 'Revision conflict' in stale['error'], str(stale))
    bad = action('save',kind='profile',name='vm-file',content='[unterminated',revision=saved['value']['revision'])
    require(not bad['ok'], 'Invalid document replaced valid file')
    cli('manage','rename','--name','vm-file','--new-name','vm-renamed')
    require(any(p['name']=='vm-renamed' for p in request('/api/state')['profiles']), 'Rename absent in Web')
    cli('manage','no_pp','--name','vm-renamed')
    cli('manage','with_proxy','--name','vm-renamed')
    require(cli('manage','delete','--name','vm-renamed','--yes')['profiles'] is not None, 'Delete failed')


def diagnostics():
    cli('manage','import','--name','vm-invalid','--input',input_file(base_profile))
    path = ROOT / 'mihomo/profiles/vm-invalid.yaml'
    path.write_text('[unterminated')
    for op in ['check','test']:
        value = cli('manage',op,'--name','vm-invalid',success=False)
        require(value['valid'] is False and value['exit_code'] != 0, str(value))
        web = action(op,name='vm-invalid')
        require(web['ok'] and not web['value']['valid'], str(web))
        require('unterminated' in (value['stdout'] + value['stderr']).lower() or value['stdout'] or value['stderr'], 'Missing core diagnostics')
    old = (ROOT / 'mihomo/config.yaml').read_bytes()
    cli('manage','activate','--name','vm-invalid','--yes',success=False)
    require((ROOT / 'mihomo/config.yaml').read_bytes() == old, 'Invalid activation changed active file')
    cli('manage','delete','--name','vm-invalid','--yes')


def subscription():
    cli('manage','create','--name','vm-url','--url',URL + '/subscription')
    require(cli('manage','profile_url','--name','vm-url')['url']==URL+'/subscription','URL mismatch')
    quota = cli('manage','traffic','--name','vm-url')['subscriptions'][0]
    require(quota['available'], str(quota))
    cli('manage','update','--name','vm-url')
    cli('manage','delete','--name','vm-url','--yes')


def templates():
    path = ROOT / 'mihomo/templates/vm.yaml'
    path.write_text(json.dumps({**base_profile,'proxy-providers':{},'rule-providers':{}}))
    cli('manage','preview_template','--name','vm.yaml')
    cli('manage','generate','--name','vm-generated','--template','vm.yaml')
    cli('manage','generate','--name','vm-generated','--template','vm.yaml','--yes')
    doc = cli('manage','read','--kind','template','--name','vm.yaml')
    cli('manage','delete_template','--name','vm.yaml','--revision',doc['revision'],'--yes',success=False)
    cli('manage','delete','--name','vm-generated','--yes')
    cli('manage','delete_template','--name','vm.yaml','--revision',doc['revision'],'--yes')


def settings():
    cli('manage','patch','--input',input_file({'mode':'direct'}))
    require(cli('core','status')['config']['mode']=='direct', 'Patch not applied')
    cli('manage','persist','--yes')
    cli('manage','activate','--name','baseline','--yes')
    require(cli('core','status')['config']['mode']=='direct', 'Persist not applied at activation')
    cli('manage','patch','--input',input_file({'mode':'rule'}))
    cli('manage','persist','--yes')
    cli('manage','activate','--name','baseline','--yes')


def delay():
    value = cli('core','delay','DIRECT','--url',URL+'/probe%2Fencoded?x=a%26b','--timeout','1000')
    require(value['delay'] is not None, str(value))
    require('/probe%2Fencoded?x=a%26b' in paths, 'Core received changed test URL')
    cli('core','delay','Test Select','--group','--url',URL+'/probe','--timeout','1000')
    for value in ['0','3600001']:
        cli('core','delay','DIRECT','--timeout',value,success=False)


def selections():
    cli('core','select','Test Select','REJECT')
    require(cli('core','proxies')['proxies']['Test Select']['now']=='REJECT','Selection did not apply')
    cli('core','select','Test Select','DIRECT')
    cli('core','unfix','Test Select',success=False)


def providers():
    profile = {**base_profile, 'proxy-providers':{'local':{'type':'http','path':'proxies/local.yaml','url':URL+'/proxy','interval':3600,
        'health-check':{'enable':False}}}, 'rule-providers':{'localrules':{'type':'http','behavior':'domain','format':'yaml','path':'rules/local.yaml','url':URL+'/rules','interval':3600}}}
    cli('manage','import','--name','vm-providers','--input',input_file(profile))
    update = cli('manage','update','--name','vm-providers')
    require(not update['partial_failure'], str(update))
    cli('manage','activate','--name','vm-providers','--yes')
    for kind,name in [('proxy-providers','local'),('rule-providers','localrules')]:
        require(cli('core','resources',kind,'list','--name',name), 'Provider missing')
        cli('core','resources',kind,'update','--name',name,'--yes')
    cli('core','resources','proxy-providers','health','--name','local','--yes')
    value = cli('core','delay','Local A','--provider','local','--url',URL+'/probe','--timeout','1000')
    require(value['delay'] is not None, str(value))
    cli('manage','activate','--name','baseline','--yes')
    cli('manage','delete','--name','vm-providers','--yes')


def rules():
    rows = cli('core','resources','rules','list')
    if not rows or 'disabled' not in rows[0]['value']:
        raise NotImplementedError('Mihomo 1.19.24 does not expose rule disabled state; toggle capability unavailable')
    cli('core','resources','rules','update','--name','0','--yes')
    rows = cli('core','resources','rules','list')
    require(rows[0]['value']['disabled'] is True, str(rows))
    cli('core','resources','rules','update','--name','0','--yes')


def logs_metrics():
    metrics = cli('core','metrics','--samples','2','--interval-ms','100')
    require(len(metrics['samples']) == 2 and metrics['memory'] is not None, str(metrics))
    cli('core','connections')
    cli('core','connections','--filter','no-matching-connection','--close','--yes')
    output = str(WORK / 'logs.json')
    cli('core','logs','--seconds','1','--limit','3','--output',output)
    require(Path(output).exists(), 'Log export missing')
    cli('core','logs','--seconds','1','--limit','3','--output',output,success=False)


def services():
    cli('manage','activate','--name','baseline','--yes')
    cli('service','stop')
    cli('core','status',success=False)
    cli('service','start')
    cli('service','restart')
    cli('core','status')
    cli('manage','delete','--name','baseline','--yes',success=False)


def recovery():
    result = action('check',name='baseline')
    require(result['ok'] and result['value']['valid'], str(result))
    previous = request('/api/job')
    subprocess.run(['sudo','systemctl','restart','clashtui-test-web'],check=True)
    for _ in range(30):
        try:
            restored = request('/api/job')
            require(restored['id']==previous['id'] and restored['result']==previous['result'], 'Completed task recovery mismatch')
            return
        except OSError:
            time.sleep(.1)
    raise AssertionError('Web service failed to recover')


def maintenance():
    cli('core','maintenance','flush-dns','--yes')
    cli('core','maintenance','flush-fakeip','--yes')
    cli('core','maintenance','restart','--yes')
    time.sleep(.5)
    cli('core','status')


def escaped_path():
    sentinel = ROOT / 'outside-core.yaml'
    sentinel.write_text('preserve')
    link = ROOT / 'mihomo/escape-link'
    link.symlink_to(ROOT, target_is_directory=True)
    for index,path in enumerate(['../outside-core.yaml', str(sentinel), 'escape-link/outside-core.yaml']):
        profile = {**base_profile,'rule-providers':{'escape':{'type':'http','path':path,'url':URL+'/rules','behavior':'domain'}}}
        name = 'known-escape-'+str(index)
        cli('manage','import','--name',name,'--input',input_file(profile))
        cli('manage','update','--name',name,success=False)
        cli('manage','activate','--name',name,'--yes',success=False)
        require(sentinel.read_text()=='preserve', 'Provider access escaped core directory')
    link.unlink()


def text_rules():
    source = WORK/'rules.txt'
    source.write_text('example.test\n+.example.org\n')
    process = subprocess.run(['/opt/clashtui/bin/mihomo','convert-ruleset','domain','text',str(source),str(WORK/'rules.mrs')],capture_output=True,text=True)
    require(process.returncode==0, 'MRS fixture conversion failed')
    for fmt in ['text','mrs']:
        profile = {**base_profile,'rule-providers':{'rules':{'type':'http','path':'rules/format.'+fmt,'url':URL+'/'+fmt,'behavior':'domain','format':fmt}},'rules':['RULE-SET,rules,DIRECT','MATCH,DIRECT']}
        name = 'known-'+fmt
        cli('manage','import','--name',name,'--input',input_file(profile))
        cli('manage','update','--name',name)
        cli('manage','activate','--name',name,'--yes')
        cli('manage','no_pp','--name',name)
        cli('manage','activate','--name',name,'--yes')
        import yaml
        active = yaml.safe_load((ROOT/'mihomo/config.yaml').read_text())
        require(active['rule-providers']['rules']['format']==fmt and 'RULE-SET,rules,DIRECT' in active['rules'], 'Rule provider semantics changed with no_pp')
    cache = ROOT/'mihomo/rules/format.mrs'
    original = cache.read_bytes()
    bad = {**base_profile,'rule-providers':{'rules':{'type':'http','path':'rules/format.mrs','url':URL+'/bad-mrs','behavior':'domain','format':'mrs'}}}
    cli('manage','import','--name','bad-mrs','--input',input_file(bad))
    cli('manage','update','--name','bad-mrs',success=False)
    require(cache.read_bytes()==original, 'Invalid MRS replaced usable cache')
    cli('manage','activate','--name','baseline','--yes')


def cache_failure():
    url = URL + '/fail'
    cache = ROOT / 'mihomo/proxies' / hashlib.md5(url.encode()).hexdigest()
    cache.parent.mkdir(exist_ok=True)
    cache.write_text('proxies: []\n')
    template = {**base_profile,'proxy-providers':{'pvd':{'tpl_param':None,'type':'http','interval':3600}},
        'clashtui':{'proxy_provider_groups':{'pvd':{'cached':url}}}}
    (ROOT/'mihomo/templates/known-cache.yaml').write_text(json.dumps(template))
    cli('manage','generate','--name','known-cache','--template','known-cache.yaml')
    process = subprocess.run(CLI + ['manage','update','--name','known-cache'],capture_output=True,text=True,timeout=20)
    require(process.returncode != 0, 'Download returned HTTP 503, but CLI reported update success')
    value = json.loads(process.stdout)
    require(value['partial_failure'] and not value['resources'][0]['ok'] and value['resources'][0]['error'], 'CLI lost download diagnostics')
    result = action('update', name='known-cache')
    require(result['ok'] and result['value']['partial_failure'] and not result['value']['resources'][0]['ok'], 'Web lost partial failure')
    require(cache.read_text()=='proxies: []\n', 'Failed download changed old cache')


def empty_groups():
    (ROOT/'mihomo/template_proxy_providers.yaml').write_text(json.dumps({'legacy':{'old':URL+'/proxy'}}))
    (ROOT/'mihomo/templates/known-empty.yaml').write_text(json.dumps({**base_profile,'proxy-providers':{},'clashtui':{'proxy_provider_groups':{}}}))
    value = cli('manage','template_providers','--name','known-empty.yaml')
    require(value['groups']=={}, 'Explicit empty groups reverted to legacy providers')
    saved = action('save_template_providers', name='known-empty.yaml', document_revision=value['revision'], groups={})
    require(saved['ok'], str(saved))
    cli('manage','generate','--name','known-empty','--template','known-empty.yaml')
    import yaml
    generated = yaml.safe_load((ROOT/'mihomo/profiles/known-empty.yaml').read_text())
    require(generated['clashtui']['proxy_provider_groups']=={}, 'Generation reintroduced legacy groups')


def readonly_revision():
    database = ROOT / 'clashtui.db'
    state = request('/api/state')
    for _ in range(10):
        before = database.read_bytes()
        cli('core','status')
        cli('manage','state')
        cli('manage','read','--name','baseline')
        require(database.read_bytes()==before, 'Read-only CLI invalidated Web revision')
    submitted = request('/api/action', {'action':'check','name':'baseline','core':state['core'],'revision':state['revision']})
    require('job' in submitted, 'Read-only CLI made existing Web state stale')
    for _ in range(100):
        job = request('/api/job')
        if not job['pending']:
            require(job['result']['ok'], 'Web check failed after read-only CLI')
            break
        time.sleep(.05)
    else:
        raise AssertionError('Web check timed out')


for name,work in [('Authentication',authentication),('CLI/Web documents and conflicts',documents),
                  ('Real core diagnostics and rejected activation',diagnostics),('Loopback subscription and quota',subscription),
                  ('Template preview/generation/deletion',templates),('Runtime patch/persist/activation',settings),
                  ('Node selection',selections),('Delay budget and encoded URL',delay),('Proxy/rule Provider APIs',providers),
                  ('Rule toggle',rules),('Logs/metrics/connections/export',logs_metrics),('Service stop/start/restart',services),
                  ('Completed Web task recovery',recovery),('Cache flush and core API restart',maintenance)]:
    case(name,work)
for name,work in [('Provider path confinement',escaped_path),('Text/MRS rule providers and no_pp semantics',text_rules),
                  ('Download failure with usable cache',cache_failure),('Explicit empty Provider groups',empty_groups)]:
    case(name,work)
case('Read-only CLI preserves database revision',readonly_revision)
server.shutdown()
report = {'created_at':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'scope':'real guest core and loopback traffic',
    'passed':sum(r['status']=='passed' for r in results),'failed':sum(r['status']=='failed' for r in results),
    'known_failures':sum(r['status']=='known_failure' for r in results),
    'skipped':sum(r['status']=='skipped' for r in results),'cases':results}
(WORK/'report.json').write_text(json.dumps(report,indent=2))
print('SUMMARY '+json.dumps({k:v for k,v in report.items() if k!='cases'}),flush=True)
