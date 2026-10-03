"""C-layer integration assertions. Must run as tester in the disposable guest."""
import base64
import concurrent.futures
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit
import yaml

assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0, 'Guest tester only'
ROOT = Path('/home/tester/clashtui-test/config')
WORK = ROOT.parent / 'acceptance-c'
WORK.mkdir(exist_ok=True)
CLI = ['clashtui', '--config-dir=' + str(ROOT)]
TOKEN = (ROOT/'management-token').read_text().strip()
OVERLAY = ROOT/'mihomo/core_override_config.yaml'
ACTIVE = ROOT/'mihomo/config.yaml'
CONFIG = ROOT/'config.yaml'
SECRET = yaml.safe_load(OVERLAY.read_text())['secret']
secrets = [TOKEN, SECRET]
results = []
hits = []
slow_started = threading.Event()
slow_release = threading.Event()
BASE = {'proxies': [], 'proxy-providers': {}, 'rule-providers': {},
        'proxy-groups': [{'name': 'Test Select', 'type': 'select', 'proxies': ['DIRECT', 'REJECT']}],
        'rules': ['MATCH,DIRECT']}


def case(identifier, work):
    start = time.monotonic()
    try:
        detail = work() or {}
        row = {'id': identifier, 'name': work.__name__, 'status': 'passed', 'evidence': detail}
    except Exception as error:
        message = str(error)
        for secret in secrets:
            message = message.replace(secret, '[redacted]')
        row = {'id': identifier, 'name': work.__name__, 'status': 'failed', 'error': message[:3000]}
    row['duration_ms'] = round((time.monotonic()-start)*1000)
    results.append(row)
    print(json.dumps(row), flush=True)


def cli(*args, ok=True, timeout=35):
    p = subprocess.run(CLI+list(args), text=True, capture_output=True, timeout=timeout)
    assert (p.returncode == 0) == ok, f'{args}: exit={p.returncode}: {p.stderr}'
    try:
        value = json.loads(p.stdout)
    except ValueError:
        value = p.stdout
    return value


def request(path, body=None, port=9091, token=TOKEN):
    req = urllib.request.Request(f'http://127.0.0.1:{port}'+path,
        data=None if body is None else json.dumps(body).encode(),
        headers={'Authorization': 'Bearer '+token, 'Content-Type': 'application/json'})
    return json.load(urllib.request.urlopen(req, timeout=5))


def wait_web():
    for _ in range(80):
        try:
            return request('/api/job')
        except OSError:
            time.sleep(.1)
    raise AssertionError('Management did not restart')


def action(action_name, **extra):
    state = request('/api/state')
    submitted = request('/api/action', {'action': action_name, 'core': state['core'], 'revision': state['revision'], **extra})
    for _ in range(300):
        job = request('/api/job')
        assert job['id'] == submitted['job'], 'Job replaced'
        if not job['pending']:
            return job['result']
        time.sleep(.1)
    raise AssertionError('Job timed out')


def save_input(name, value):
    p = WORK/(name+'.json')
    p.write_text(json.dumps(value))
    return str(p)


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_HEAD(self):
        self.respond(False)
    def do_GET(self):
        self.respond(True)
    def respond(self, body):
        path = urlsplit(self.path).path
        hits.append(path)
        if path == '/redirect':
            self.send_response(302); self.send_header('Location', URL+'/subscription'); self.end_headers(); return
        if path == '/auth':
            expected = 'Basic '+base64.b64encode(b'c-user:c-pass').decode()
            if self.headers.get('Authorization') != expected:
                self.send_error(401); return
        if path in ['/unauthorized', '/failed']:
            self.send_error(401 if path == '/unauthorized' else 503); return
        if path == '/slow':
            slow_started.set()
            slow_release.wait(12)
        if path == '/timeout':
            time.sleep(4)
        content = b'[unterminated' if path == '/malformed' else json.dumps(BASE).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(content)))
        self.send_header('Subscription-Userinfo', 'upload=10; download=20; total=100; expire=2000000000')
        self.end_headers()
        if body:
            try:
                self.wfile.write(content)
            except (BrokenPipeError, ConnectionResetError):
                pass


server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
URL = f'http://127.0.0.1:{server.server_port}'
threading.Thread(target=server.serve_forever, daemon=True).start()


def subscription_errors():
    cfg_before = CONFIG.read_bytes()
    try:
        cfg = yaml.safe_load(cfg_before); cfg['timeout'] = 1
        CONFIG.write_text(json.dumps(cfg))
        for suffix in ['redirect', 'auth']:
            url = URL+'/'+suffix
            if suffix == 'auth':
                url = url.replace('http://', 'http://c-user:c-pass@')
            name = 'c-'+suffix
            cli('manage', 'create', '--name', name, '--url', url)
            quota = cli('manage', 'traffic', '--name', name)['subscriptions'][0]
            assert quota['available'], quota
            cli('manage', 'update', '--name', name)
        for suffix in ['unauthorized', 'failed', 'malformed', 'timeout']:
            before = (ROOT/'clashtui.db').read_bytes()
            start = time.monotonic()
            cli('manage', 'create', '--name', 'c-bad-'+suffix, '--url', URL+'/'+suffix, ok=False)
            assert (ROOT/'clashtui.db').read_bytes() == before, 'Failed create registered profile'
            assert not (ROOT/'mihomo/profiles'/('c-bad-'+suffix+'.yaml')).exists()
            if suffix == 'timeout':
                assert time.monotonic()-start < 3.5, 'Configured timeout was ignored'
        cli('manage', 'create', '--name', 'c-cache', '--url', URL+'/subscription')
        cache = ROOT/'mihomo/profiles/c-cache.yaml'; before = cache.read_bytes()
        cli('manage', 'rename', '--name', 'c-cache', '--new-name', 'c-cache', '--url', URL+'/failed')
        assert cli('manage', 'update', '--name', 'c-cache', ok=False) is not None
        web = action('update', name='c-cache')
        assert not web['ok'] and '503' in web['error'], web
        assert cache.read_bytes() == before, 'HTTP failure replaced last usable subscription'
        return {'basic_auth': True, 'redirect_and_quota': True, 'errors': [401, 503, 'malformed', 'timeout'], 'cache_preserved': True}
    finally:
        CONFIG.write_bytes(cfg_before)


def mixed_batch():
    state = cli('manage', 'update_all', '--yes', ok=False)
    rows = {r['name']: r for r in state['results']}
    assert rows['c-redirect']['ok'] and not rows['c-cache']['ok'], rows
    web = action('update_all')
    assert web['ok'], web
    rows = {r['name']: r for r in web['value']['results']}
    assert rows['c-redirect']['ok'] and not rows['c-cache']['ok'], rows
    return {'cli_nonzero': True, 'web_per_item_failure': True}


def proxy_route():
    # A proxy-only address proves routing, rather than merely checking a saved flag.
    # minreq uses CONNECT; map the reserved .invalid destination only in this fixture.
    import select
    count = []
    class Proxy(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_CONNECT(self):
            assert self.path.startswith('c-proxy.invalid:'), self.path
            count.append(self.path)
            upstream = socket.create_connection(('127.0.0.1', server.server_port))
            self.send_response(200); self.end_headers()
            try:
                ends = [self.connection, upstream]
                while True:
                    ready, _, _ = select.select(ends, [], [], 3)
                    if not ready: break
                    for stream in ready:
                        data = stream.recv(65536)
                        if not data: return
                        (upstream if stream is self.connection else self.connection).sendall(data)
            finally: upstream.close()
    proxy = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Proxy)
    threading.Thread(target=proxy.serve_forever, daemon=True).start()
    old = OVERLAY.read_bytes()
    try:
        value = yaml.safe_load(old); value['mixed-port'] = proxy.server_port
        OVERLAY.write_text(json.dumps(value))
        cli('manage', 'create', '--name', 'c-route', '--url', URL+'/subscription')
        cli('manage', 'rename', '--name', 'c-route', '--new-name', 'c-route', '--url', f'http://c-proxy.invalid:{server.server_port}/subscription')
        cli('manage', 'update', '--name', 'c-route', '--with-proxy', 'false', ok=False)
        assert not count, 'Direct update unexpectedly entered proxy'
        cli('manage', 'update', '--name', 'c-route', '--with-proxy', 'true')
        assert count, 'Proxy update never reached proxy'
        return {'unresolvable_direct_rejected': True, 'connect_requests': len(count)}
    finally:
        OVERLAY.write_bytes(old); proxy.shutdown()


def interrupted_job():
    cli('manage', 'create', '--name', 'c-interrupt', '--url', URL+'/subscription')
    cli('manage', 'rename', '--name', 'c-interrupt', '--new-name', 'c-interrupt', '--url', URL+'/slow')
    before = (ROOT/'mihomo/profiles/c-interrupt.yaml').read_bytes()
    state = request('/api/state')
    slow_started.clear(); slow_release.clear()
    submitted = request('/api/action', {'action': 'update', 'name': 'c-interrupt', 'revision': state['revision'], 'core': 'mihomo'})
    try:
        assert slow_started.wait(5), 'Job did not reach blocking fixture'
        assert request('/api/job')['pending']
        record = json.loads((ROOT/'.web-task.json').read_text())
        assert record['state'] == 'running'
        subprocess.run(['sudo', 'systemctl', 'stop', 'clashtui-test-web'], check=True)
        count = hits.count('/slow')
        slow_release.set()
        subprocess.run(['sudo', 'systemctl', 'start', 'clashtui-test-web'], check=True)
        restored = wait_web()
        assert restored['id'] == submitted['job'] and not restored['pending'] and not restored['result']['ok'], restored
        assert 'restarted during a task' in restored['result']['error'], restored
        time.sleep(.6)
        assert hits.count('/slow') == count, 'Interrupted update was replayed'
        assert (ROOT/'mihomo/profiles/c-interrupt.yaml').read_bytes() == before
        return {'running_record': True, 'recovered_interrupted': True, 'not_replayed': True, 'old_cache_preserved': True}
    finally:
        slow_release.set()
        subprocess.run(['sudo', 'systemctl', 'start', 'clashtui-test-web'], check=True)
        wait_web()


def migration():
    original_overlay = OVERLAY.read_bytes(); original_active = ACTIVE.read_bytes()
    new_secret = 'c-migration-secret-unique'; secrets.append(new_secret)
    try:
        changed = yaml.safe_load(original_overlay); changed['external-controller'] = '127.0.0.1:19090'; changed['secret'] = new_secret
        OVERLAY.write_text(json.dumps(changed))
        cli('manage', 'activate', '--name', 'baseline', '--yes', ok=False)
        assert ACTIVE.read_bytes() == original_active, 'Online migration changed live configuration'
        subprocess.run(['sudo', 'systemctl', 'stop', 'clashtui-test-mihomo'], check=True)
        active = yaml.safe_load(original_active); active.update({'external-controller': '127.0.0.1:19090', 'secret': new_secret})
        ACTIVE.write_text(json.dumps(active))
        cli('service', 'start')
        status = cli('core', 'status'); assert status['version']['meta'], status
        try:
            request('/configs', port=19090, token=SECRET); raise AssertionError('Old secret accepted')
        except urllib.error.HTTPError as e: assert e.code == 401
        assert request('/version', port=19090, token=new_secret)['meta']
        try:
            socket.create_connection(('127.0.0.1', 9090), timeout=.3).close(); raise AssertionError('Old controller still accepts connections')
        except OSError: pass
        subprocess.run(['sudo', 'systemctl', 'restart', 'clashtui-test-web'], check=True); wait_web()
        assert ':19090/' in request('/api/state')['panel']
        runtime = action('runtime'); assert runtime['ok'], runtime
        return {'online_refused': True, 'new_session_readback': True, 'old_secret_rejected': True, 'old_address_closed': True, 'web_restarted': True}
    finally:
        subprocess.run(['sudo', 'systemctl', 'stop', 'clashtui-test-mihomo'], check=True)
        OVERLAY.write_bytes(original_overlay); ACTIVE.write_bytes(original_active)
        cli('service', 'start')
        subprocess.run(['sudo', 'systemctl', 'restart', 'clashtui-test-web'], check=True); wait_web()


def concurrent_scale():
    count = int(os.environ.get('CLASHTUI_SCALE_PROFILES', '100'))
    assert 10 <= count <= 2000
    for index in range(count):
        cli('manage', 'import', '--name', f'c-scale-{index:04}', '--input', save_input('scale', BASE))
    before = (ROOT/'clashtui.db').read_bytes()
    start = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        states = list(pool.map(lambda _: cli('manage', 'state'), range(24)))
    assert all(len([p for p in s['profiles'] if p['name'].startswith('c-scale-')]) == count for s in states)
    assert (ROOT/'clashtui.db').read_bytes() == before
    assert time.monotonic()-start < 30
    return {'profiles': count, 'concurrent_reads': 24, 'workers': 8, 'revision_preserved': True}



def bounded_soak():
    seconds=int(os.environ.get('CLASHTUI_SOAK_SECONDS','120')); assert 10<=seconds<=86400
    pid=int(subprocess.check_output(['systemctl','show','--property=MainPID','--value','clashtui-test-web'],text=True))
    def rss():
        return int(next(line.split()[1] for line in Path(f'/proc/{pid}/status').read_text().splitlines() if line.startswith('VmRSS:')))*1024
    initial=rss();peak=initial;before=(ROOT/'clashtui.db').read_bytes();start=time.monotonic();reads=0
    while time.monotonic()-start<seconds:
        assert request('/api/state')['current']=='baseline'
        assert request('/configs',port=9090,token=SECRET)['mode']=='rule'
        peak=max(peak,rss());reads+=1;time.sleep(.2)
    assert (ROOT/'clashtui.db').read_bytes()==before,'Sustained reads changed revision'
    assert peak<512*1024*1024 and peak-initial<64*1024*1024, 'Management memory growth exceeded bound'
    return {'seconds':seconds,'read_cycles':reads,'profile_count':len(cli('manage','state')['profiles']),'peak_rss_bytes':peak,'growth_budget_bytes':64*1024*1024,'revision_preserved':True}

try:
    cli('manage', 'activate', '--name', 'baseline', '--yes')
    for profile in cli('manage', 'state')['profiles']:
        if profile['name'].startswith('c-'):
            cli('manage', 'delete', '--name', profile['name'], '--yes')
    for identifier, work in [('C01', subscription_errors), ('C02', mixed_batch), ('C03', proxy_route),
                             ('C04', interrupted_job), ('C05', migration), ('C06', concurrent_scale), ('C07', bounded_soak)]:
        case(identifier, work)
finally:
    slow_release.set(); server.shutdown()
    report = {'cases': results, 'passed': sum(r['status']=='passed' for r in results),
              'failed': sum(r['status']=='failed' for r in results), 'scope': 'real guest workflows and controlled loopback fixtures'}
    (WORK/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print('SUMMARY '+json.dumps(report), flush=True)
raise SystemExit(bool(report['failed']))
