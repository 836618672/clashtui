"""Network-enabled panel/Geo/core maintenance and failure preservation, guest only."""
import hashlib
import concurrent.futures
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.request
import yaml

assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0
ROOT = Path('/home/tester/clashtui-test/config')
CORE = ROOT / 'mihomo'
ACTIVE = CORE / 'config.yaml'
CLI = ['clashtui', '--config-dir=' + str(ROOT)]
BIN = Path('/opt/clashtui/bin/mihomo')
SECRET = yaml.safe_load(ACTIVE.read_text())['secret']
results = []
payload = json.load(sys.stdin)
public_group = False
update_candidates = []
original_binary_hash = hashlib.sha256(BIN.read_bytes()).hexdigest()
original_version = None
BACKUP = ROOT.parent / 'pre-upgrade-mihomo'
shutil.copy2(BIN, BACKUP)
upgrade_succeeded = False


def request(path):
    req = urllib.request.Request('http://127.0.0.1:9090' + path,
                                headers={'Authorization': 'Bearer ' + SECRET})
    with urllib.request.urlopen(req, timeout=10) as response:
        return json.load(response)


def cli(*args, success=True, timeout=240):
    p = subprocess.run(CLI + list(args), capture_output=True, timeout=timeout)
    assert (p.returncode == 0) == success, p.stderr.decode(errors='replace').replace(SECRET, '[redacted]')[:1000]
    return json.loads(p.stdout) if p.returncode == 0 else None


def restart():
    subprocess.run(['sudo', 'systemctl', 'restart', 'clashtui-test-mihomo'],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    for _ in range(2000):
        try: return request('/version')
        except OSError: time.sleep(.1)
    raise AssertionError()


def case(identifier, function):
    started = time.monotonic()
    try: row = {'id': identifier, 'status': 'passed', 'evidence': function()}
    except Exception as error:
        message = str(error).replace(SECRET, '[redacted]')
        if payload.get('url'): message = message.replace(payload['url'], '[private subscription]')
        row = {'id': identifier, 'status': 'failed', 'error_type': type(error).__name__, 'error': message[:1200]}
    row['duration_ms'] = round((time.monotonic() - started) * 1000)
    results.append(row); print(json.dumps(row), flush=True)
    (ROOT.parent / 'upgrades-report.json').write_text(json.dumps({'cases': results,
        'passed': sum(r['status'] == 'passed' for r in results),
        'failed': sum(r['status'] == 'failed' for r in results)}, indent=2))


def digest_directory(path):
    return {str(p.relative_to(path)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in path.rglob('*') if p.is_file()}


def panel_network_install():
    cli('manage', 'prepare_panel', '--yes')
    record = json.loads((CORE / 'uis/metacubexd/clashtui-panel.json').read_text())
    assert record['version'] == 'v1.273.1'
    assert record['sha256'] == 'a178e00b67acabcda2dcef00afa90be6a7bb261e466a67dad58c8478d9553603'
    assert (CORE / 'uis/metacubexd/index.html').is_file()
    return {'actual_remote_archive': True, 'pinned_version': record['version'],
            'archive_hash_verified': True, 'installed_record_verified': True}


def panel_failed_download():
    directory = CORE / 'uis/metacubexd'
    before = digest_directory(directory)
    hosts = Path('/etc/hosts').read_bytes()
    try:
        subprocess.run(['sudo', 'tee', '/etc/hosts'], input=hosts + b'\n127.0.0.1 github.com\n',
                       stdout=subprocess.DEVNULL, check=True)
        cli('manage', 'prepare_panel', '--yes', success=False)
        assert digest_directory(directory) == before
        assert not list(directory.parent.glob('.metacubexd-staging-*'))
    finally:
        subprocess.run(['sudo', 'tee', '/etc/hosts'], input=hosts, stdout=subprocess.DEVNULL, check=True)
    return {'real_download_failure_nonzero': True, 'panel_bytes_preserved': True,
            'no_staging_left': True, 'guest_hosts_restored': True}


def geo_network_update():
    global public_group, update_candidates
    cfg = yaml.safe_load((ROOT / 'config.yaml').read_text()); cfg['timeout'] = 180
    (ROOT / 'config.yaml').write_text(json.dumps(cfg))
    config = yaml.safe_load(ACTIVE.read_text())
    if payload.get('url'):
        cli('manage', 'create', '--name', 'upgrade-source', '--url', payload['url'])
        nodes = yaml.safe_load((CORE / 'profiles/upgrade-source.yaml').read_text())['proxies']
        nodes = [{**node, 'name': 'Update Node ' + str(i)} for i, node in enumerate(nodes)]
        config.update({'proxies': nodes, 'proxy-groups': [{'name': 'Update Select', 'type': 'select',
                       'proxies': [node['name'] for node in nodes]}], 'rules': ['MATCH,Update Select']})
        ACTIVE.write_text(json.dumps(config)); restart()
        def health(node):
            try:
                cli('core', 'delay', node['name'], '--url', 'https://www.cloudflare.com/cdn-cgi/trace',
                    '--timeout', '8000', timeout=15)
                return node['name']
            except Exception: return None
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            live = [name for name in pool.map(health, nodes) if name]
        assert live, 'No usable update download proxy'
        cli('core', 'select', 'Update Select', live[0])
        update_candidates = live[:5]
        config['proxy-groups'][0]['proxies'] = update_candidates
        public_group = True
    config['geodata-mode'] = True
    prefix = 'https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/'
    config['geox-url'] = {'geoip': prefix + 'geoip-lite.dat', 'geosite': prefix + 'geosite.dat',
                         'mmdb': prefix + 'geoip.metadb',
                         'asn': 'https://github.com/xishang0128/geoip/releases/download/latest/GeoLite2-ASN.mmdb'}
    config['geo-auto-update'] = False
    # Geo updates operate on databases enabled by actual rules. Merely setting
    # geodata-mode with MATCH-only rules is a legitimate no-op in Mihomo.
    config['rules'] = ['IP-CIDR,127.0.0.0/8,DIRECT,no-resolve', 'GEOSITE,cn,DIRECT',
                       'GEOIP,CN,DIRECT', 'MATCH,' + ('Update Select' if public_group else 'DIRECT')]
    ACTIVE.write_text(json.dumps(config)); restart()
    geo_updated = False
    last_error = None
    for name in update_candidates or [None]:
        if name: cli('core', 'select', 'Update Select', name)
        try:
            cli('core', 'maintenance', 'upgrade-geo', '--yes')
            geo_updated = True
            break
        except AssertionError as error: last_error = error
    if not geo_updated: raise last_error or AssertionError('Geo download failed')
    files = {p.name: p for p in CORE.iterdir() if p.suffix.lower() in ['.dat', '.mmdb', '.metadb']}
    assert files and any('geosite' in n.lower() for n in files) and any('geoip' in n.lower() for n in files)
    assert all(p.stat().st_size > 1000 for p in files.values())
    # Force the downloaded databases to be parsed, not just saved successfully.
    ACTIVE.write_text(json.dumps(config)); restart()
    assert any(row.get('type') == 'GeoSite' for row in request('/rules')['rules'])
    return {'real_remote_geo_update': True, 'database_file_count': len(files),
            'downloaded_databases_loaded_by_core': True}


class BrokenDownload(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        self.send_response(503); self.end_headers(); self.wfile.write(b'controlled upgrade failure')


def geo_failed_update():
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), BrokenDownload)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    saved = ACTIVE.read_bytes()
    before = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in CORE.iterdir()
              if p.suffix.lower() in ['.dat', '.mmdb', '.metadb']}
    assert before
    try:
        config = yaml.safe_load(saved)
        config['geox-url'] = {key: 'http://127.0.0.1:' + str(server.server_port) + '/bad'
                              for key in ['geoip', 'geosite', 'mmdb', 'asn']}
        ACTIVE.write_text(json.dumps(config)); restart()
        cli('core', 'maintenance', 'upgrade-geo', '--yes', success=False)
        assert all(hashlib.sha256((CORE / name).read_bytes()).hexdigest() == value for name, value in before.items())
        assert request('/version')['meta']
    finally:
        ACTIVE.write_bytes(saved); restart(); server.shutdown(); server.server_close()
    return {'real_http_failure_nonzero': True, 'geo_bytes_preserved': True,
            'core_still_healthy': True, 'settings_restored': True}


def core_failed_upgrade():
    saved = ACTIVE.read_bytes()
    before = hashlib.sha256(BIN.read_bytes()).hexdigest()
    try:
        config = yaml.safe_load(saved); config['rules'] = ['MATCH,REJECT']
        ACTIVE.write_text(json.dumps(config)); restart()
        cli('core', 'maintenance', 'upgrade', '--yes', success=False)
        assert hashlib.sha256(BIN.read_bytes()).hexdigest() == before
        assert request('/version')['meta']
    finally:
        ACTIVE.write_bytes(saved); restart()
    return {'real_network_failure_nonzero': True, 'binary_preserved': True, 'core_still_healthy': True}


def core_network_upgrade():
    global upgrade_succeeded, original_version
    old_version = request('/version')['version']
    original_version = old_version
    # This isolated service's user needs write permission for its own executable
    # and the upgrade staging directory, exactly as required by the core API.
    subprocess.run(['sudo', 'chown', 'tester:tester', '/opt/clashtui/bin', str(BIN)], check=True)
    succeeded = False
    last_error = None
    for name in update_candidates or [None]:
        if name: cli('core', 'select', 'Update Select', name)
        try:
            cli('core', 'maintenance', 'upgrade', '--yes')
            succeeded = True
            break
        except AssertionError as error:
            last_error = error
            assert hashlib.sha256(BIN.read_bytes()).hexdigest() == original_binary_hash, 'Failed upgrade changed binary'
            assert request('/version')['version'] == old_version, 'Failed upgrade changed running version'
    if not succeeded: raise last_error or AssertionError('Core download failed')
    new_version = restart()['version']
    new_hash = hashlib.sha256(BIN.read_bytes()).hexdigest()
    assert new_hash != original_binary_hash
    assert cli('core', 'status')['version']['meta']
    assert cli('core', 'proxies')['proxies']
    assert request('/rules')['rules']
    upgrade_succeeded = True
    return {'old_version': old_version, 'new_version': new_version,
            'binary_changed': True, 'service_restart_ready': True,
            'new_core_cli_config_proxies_rules_readback': True}


def core_rollback():
    assert upgrade_succeeded, 'Successful upgrade required before rollback verification'
    subprocess.run(['sudo', 'systemctl', 'stop', 'clashtui-test-mihomo'], check=True)
    restored = BIN.with_name('rollback-mihomo')
    shutil.copy2(BACKUP, restored)
    restored.replace(BIN)
    assert hashlib.sha256(BIN.read_bytes()).hexdigest() == original_binary_hash
    assert restart()['version'] == original_version
    cli('manage', 'activate', '--name', 'baseline', '--yes')
    assert cli('core', 'status')['version']['meta']
    assert cli('manage', 'state')['current'] == 'baseline'
    return {'original_binary_restored': True, 'original_version': original_version,
            'service_ready': True, 'baseline_activation_readback': True}


for identifier, function in [('U01', panel_network_install), ('U02', panel_failed_download),
        ('U03', geo_network_update), ('U04', geo_failed_update),
        ('U05', core_failed_upgrade), ('U06', core_network_upgrade), ('U07', core_rollback)]:
    case(identifier, function)
report = {'cases': results, 'passed': sum(r['status'] == 'passed' for r in results),
          'failed': sum(r['status'] == 'failed' for r in results)}
(ROOT.parent / 'upgrades-report.json').write_text(json.dumps(report, indent=2))
sys.exit(bool(report['failed']))
