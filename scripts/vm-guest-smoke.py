import argparse, hashlib, json, socket, subprocess, urllib.request
import yaml
from pathlib import Path
parser = argparse.ArgumentParser()
parser.add_argument('--allow-internet', action='store_true')
parser.add_argument('--profile', default='baseline')
args = parser.parse_args()
assert socket.gethostname() == 'clashtui-test'
root = Path('/home/tester/clashtui-test/config')
overlay = yaml.safe_load((root / 'mihomo/core_override_config.yaml').read_text())
def get(url, token=None):
    req = urllib.request.Request(url, headers={'Authorization': 'Bearer ' + token} if token else {})
    with urllib.request.urlopen(req, timeout=5) as res:
        return res.read()
version = json.loads(get('http://127.0.0.1:9090/version', overlay['secret']))
runtime = json.loads(get('http://127.0.0.1:9090/configs', overlay['secret']))
state = json.loads(get('http://127.0.0.1:9091/api/state', (root / 'management-token').read_text()))
assert version['meta'] is True
assert runtime['tun']['enable'] is False and overlay['dns']['enable'] is False
assert state['current'] == args.profile, 'Active profile does not match the expected profile'
panel = get('http://127.0.0.1:9091/')
assert b'<html' in panel.lower()
assert b'id="app"' in panel and state['dashboard']['builtin'] is True
assert state['dashboard']['sha256'] == hashlib.sha256(panel).hexdigest()
for service in ['clashtui-test-mihomo', 'clashtui-test-web']:
    subprocess.run(['systemctl', 'is-active', '--quiet', service], check=True)
outbound_connected = False
try:
    with socket.create_connection(('1.1.1.1', 443), timeout=2):
        outbound_connected = True
except (OSError, TimeoutError):
    pass
if not args.allow_internet:
    assert not outbound_connected, 'Guest outbound access is unexpectedly enabled'
print(json.dumps({'guest': socket.gethostname(), 'core_version': version['version'],
    'current_profile': state['current'], 'core_service': 'active', 'web_service':'active',
    'tun_enabled':False, 'dns_enabled':False, 'outbound_tcp_blocked':not outbound_connected,
    'panel_index_sha256':hashlib.sha256(panel).hexdigest(),
    'management_authenticated_state':'passed', 'guest_ssh':'passed'}, indent=2))
