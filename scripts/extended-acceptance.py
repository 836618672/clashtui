"""Independent network-enabled VM test round with private stdin and restoration."""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import io
import tarfile
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--suite', choices=['public', 'lifecycle', 'system-lifecycle', 'upgrades'], default='public')
parser.add_argument('--subscription-file', type=Path)
args = parser.parse_args()
os.chdir(ROOT)
run = ROOT / 'target/extended-acceptance' / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
run.mkdir(parents=True, mode=0o700)
lock = (ROOT / 'target/acceptance.lock').open('w')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
source = args.subscription_file.read_text().strip() if args.subscription_file else ''
if args.suite == 'public':
    assert urlsplit(source).scheme == 'https' and '\n' not in source
stages = []; touched = False; interrupted = False; active = None


def signal_handler(signum, frame):
    global interrupted
    interrupted = True
    if active:
        try: os.killpg(active.pid, signal.SIGTERM)
        except ProcessLookupError: pass
signal.signal(signal.SIGINT, signal_handler); signal.signal(signal.SIGTERM, signal_handler)


def stage(name, command, data=None, timeout=180, cleanup=False):
    global active
    if interrupted and not cleanup:
        stages.append({'name': name, 'status': 'not_run'}); return False
    print('RUN ' + name, flush=True); started = time.monotonic()
    try:
        active = subprocess.Popen(command, stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try: out, err = active.communicate(data, timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(active.pid, signal.SIGKILL); out, err = active.communicate()
        code = active.returncode
        # The public guest script prints sanitized JSON only. Other stages never
        # receive private input. Redact the subscription URL as a further guard.
        log = (out + err).decode(errors='replace')
        if source: log = log.replace(source, '[private subscription]')
        (run / (name + '.log')).write_text(log)
    except OSError as error: code = 127
    finally: active = None
    status = 'passed' if code == 0 else 'failed'
    stages.append({'name': name, 'status': status, 'exit_code': code,
                   'duration_ms': round((time.monotonic() - started) * 1000)})
    print(status.upper() + ' ' + name, flush=True)
    return code == 0


def vm(*args): return ['bash', 'scripts/vm.sh', *args]


def snapshot():
    return {'routes': json.loads(subprocess.check_output(['ip', '-j', 'route'])),
            'rules': json.loads(subprocess.check_output(['ip', '-j', 'rule'])),
            'dns_sha256': hashlib.sha256(Path('/etc/resolv.conf').read_bytes()).hexdigest(),
            'service': subprocess.run(['systemctl', 'is-active', 'mihomo.service'],
                                      capture_output=True, text=True).stdout.strip()}


before = snapshot()
(run / 'host-before.json').write_text(json.dumps(before, indent=2))
binary_hash = hashlib.sha256((ROOT / 'target/debug/clashtui').read_bytes()).hexdigest()
(run / 'binary-sha256.txt').write_text(binary_hash)
def application_manifest():
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'installs/install']
    for directory in ['src', 'web']:
        paths.extend(p for p in (ROOT / directory).rglob('*') if p.is_file())
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
source_before = application_manifest()
(run / 'application-manifest.json').write_text(json.dumps(source_before, sort_keys=True, indent=2))
try:
    assert stage('baseline-before', vm('check'))
    touched = True
    for name, command in [('stop', vm('stop')), ('reset', vm('reset')),
        ('start', vm('start', *(['--internet'] if args.suite in ['public', 'upgrades'] else []))), ('wait', vm('wait', '--seconds', '60')),
        ('push', vm('push')), ('permissions', vm('ssh', 'sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo; sudo systemctl restart clashtui-test-web'))]:
        assert stage(name, command)
    if args.suite == 'public':
        assert stage('upload-public-script', vm('ssh', 'cat > /home/tester/clashtui-test/public-test.py'),
                     data=(ROOT / 'scripts/vm-public.py').read_bytes())
        stage('public', vm('ssh', 'python3 /home/tester/clashtui-test/public-test.py'),
              data=json.dumps({'url': source}).encode(), timeout=900)
        stage('public-report', vm('ssh', 'cat /home/tester/clashtui-test/public-report.json'))
    elif args.suite == 'upgrades':
        assert stage('upload-upgrades-script', vm('ssh', 'cat > /home/tester/clashtui-test/upgrades-test.py'),
                     data=(ROOT / 'scripts/vm-upgrades.py').read_bytes())
        stage('upgrades', vm('ssh', 'python3 /home/tester/clashtui-test/upgrades-test.py'),
              data=json.dumps({'url': source} if source else {}).encode(), timeout=1800)
        stage('upgrades-report', vm('ssh', 'cat /home/tester/clashtui-test/upgrades-report.json'))
    else:
        bundle = io.BytesIO()
        with tarfile.open(fileobj=bundle, mode='w:gz') as tar:
            tar.add(ROOT / 'installs/install', arcname='installer/install')
            tar.add(ROOT / 'contrib', arcname='installer/contrib')
            tar.add(ROOT / 'scripts/vm-lifecycle.py', arcname='lifecycle-test.py')
        assert stage('upload-installer', vm('ssh', 'tar xzf - -C /home/tester/clashtui-test'), data=bundle.getvalue())
        mode = ' system' if args.suite == 'system-lifecycle' else ''
        stage('lifecycle-prepare', vm('ssh', 'python3 /home/tester/clashtui-test/lifecycle-test.py prepare' + mode))
        for name, command in [('power-cycle-stop', vm('stop')), ('power-cycle-start', vm('start')),
                               ('power-cycle-wait', vm('wait', '--seconds', '60'))]:
            assert stage(name, command)
        stage('lifecycle-after-reboot', vm('ssh', 'python3 /home/tester/clashtui-test/lifecycle-test.py verify' + mode))
        stage(args.suite + '-report', vm('ssh', 'cat /home/tester/clashtui-test/lifecycle-report.json'))
except Exception as error:
    stages.append({'name': 'orchestrator', 'status': 'failed', 'error_type': type(error).__name__})
finally:
    if touched:
        for name, command in [('restore-stop', vm('stop')), ('restore-reset', vm('reset')),
            ('restore-start', vm('start')), ('restore-wait', vm('wait', '--seconds', '60')),
            ('restore-push', vm('push')), ('restore-permissions', vm('ssh', 'sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo; sudo systemctl restart clashtui-test-web')),
            ('restore-tunnel', vm('tunnel')), ('restore-check', vm('check'))]:
            if not stage(name, command, cleanup=True): break
    after = snapshot(); (run / 'host-after.json').write_text(json.dumps(after, indent=2))
    stages.append({'name': 'host-invariants', 'status': 'passed' if before == after else 'failed'})
    stages.append({'name': 'application-invariants', 'status': 'passed' if source_before == application_manifest() else 'failed'})
    report = {'stages': stages, 'passed': all(s['status'] == 'passed' for s in stages),
              'scope': 'independent ' + args.suite + ' VM round', 'interrupted': interrupted,
              'binary_sha256': binary_hash}
    try:
        report[args.suite] = json.loads((run / (args.suite + '-report.log')).read_text())
        expected = {'public': {'P%02d' % i for i in range(1, 9)},
                    'lifecycle': {'L%02d' % i for i in range(1, 6)},
                    'system-lifecycle': {'L%02d' % i for i in range(1, 6)},
                    'upgrades': {'U%02d' % i for i in range(1, 8)}}[args.suite]
        cases = report[args.suite]['cases']
        ids = [c['id'] for c in cases]
        report['required_cases_passed'] = (set(ids) == expected and len(ids) == len(expected)
                                          and all(c['status'] == 'passed' for c in cases))
        report['passed'] = report['passed'] and report['required_cases_passed']
    except (OSError, ValueError): report['passed'] = False
    (run / 'report.json').write_text(json.dumps(report, indent=2))
    print('REPORT ' + str(run / 'report.json'), flush=True)
sys.exit(not report['passed'])
