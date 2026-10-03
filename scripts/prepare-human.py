"""Prepare an inactive loopback subscription for H06, without leaving a server."""
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.request
assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0
root = Path('/home/tester/clashtui-test/config')
lab = root.parent / 'manual'
cli = ['clashtui', '--config-dir=' + str(root)]
state = json.loads(subprocess.check_output(cli + ['manage', 'state'], text=True))
assert state['current'] == 'baseline', 'Restore the baseline before preparing human checks'
name = 'c-human-clipboard'
url = 'http://127.0.0.1:18080/subscription.yaml'
existing = next((profile for profile in state['profiles'] if profile['name'] == name), None)
if existing:
    assert existing.get('url') == url, 'Reserved human fixture name belongs to another profile'
else:
    subprocess.run(['python3', str(lab / 'lab.py'), 'prepare'], check=True, stdout=subprocess.DEVNULL)
    with (lab / 'human-fixture.log').open('w') as log:
        server = subprocess.Popen(['python3', str(lab / 'lab.py'), 'serve'], stdout=log, stderr=log)
        try:
            for _ in range(50):
                if server.poll() is not None: raise RuntimeError('Human fixture server exited')
                try:
                    assert urllib.request.urlopen('http://127.0.0.1:18080/hello', timeout=.5).read() == b'manual-lab-ok\n'
                    break
                except OSError: time.sleep(.1)
            else: raise RuntimeError('Human fixture not ready')
            subprocess.run(cli + ['manage', 'create', '--name', name, '--url', url], check=True, stdout=subprocess.DEVNULL)
        finally:
            server.terminate(); server.wait(timeout=5)
state = json.loads(subprocess.check_output(cli + ['manage', 'state'], text=True))
assert state['current'] == 'baseline'
assert next(profile for profile in state['profiles'] if profile['name'] == name)['url'] == url
print(json.dumps({'clipboard_profile': name, 'url': url, 'current': 'baseline', 'subscription_server_left_running': False}))
