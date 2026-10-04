"""Aggregate the available mini-PC automatic scopes without claiming native OS acceptance."""
import argparse
import datetime
import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import signal
import os

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--subscription-file', type=Path, required=True)
parser.add_argument('--panel-archive', type=Path, help='Deprecated compatibility argument; Vue dashboard is bundled')
parser.add_argument('--soak-seconds', type=int, default=600)
parser.add_argument('--resume', type=Path, help='Reuse passed scopes only when binary and source manifests still match')
args = parser.parse_args()
assert 10 <= args.soak_seconds <= 86400
run = ROOT / 'target/full-auto' / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
run.mkdir(parents=True)
lock = (ROOT / 'target/full-auto.lock').open('w')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
results = []
previous = json.loads(args.resume.read_text()) if args.resume else {}
active = None
interrupted = False


def signal_handler(signum, frame):
    global interrupted
    interrupted = True
    if active:
        try: os.killpg(active.pid, signal.SIGTERM)
        except ProcessLookupError: pass
signal.signal(signal.SIGINT, signal_handler); signal.signal(signal.SIGTERM, signal_handler)


def run_command(command, log):
    global active
    if interrupted: return 130
    active = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try: return active.wait()
    finally: active = None


def execute(name, command, report_parent):
    print('RUN ' + name, flush=True)
    before = set(report_parent.glob('*/report.json'))
    with (run / (name + '.log')).open('w') as log:
        code = run_command(command, log)
    new = set(report_parent.glob('*/report.json')) - before
    row = {'suite': name, 'exit_code': code, 'passed': False}
    if len(new) == 1:
        path = new.pop(); row['report'] = str(path)
        try:
            report = json.loads(path.read_text())
            row['passed'] = code == 0 and report.get('automatic_passed', report.get('passed', False))
            row['binary_sha256'] = report['binary_sha256']
        except (OSError, ValueError, KeyError): pass
    results.append(row)
    print(('PASSED ' if row['passed'] else 'FAILED ') + name, flush=True)


def reuse(name, binary_hash):
    old = next((r for r in previous.get('suites', []) if r['suite'] == name), None)
    if not old or not old.get('passed') or old.get('binary_sha256') != binary_hash: return False
    path = Path(old['report'])
    try:
        report = json.loads(path.read_text())
        if not report.get('automatic_passed', report.get('passed', False)): return False
        filename = 'source-manifest.json' if name == 'abc-soak' else 'application-manifest.json'
        manifest = json.loads((path.parent / filename).read_text())
        for relative, expected in manifest.items():
            source = ROOT / relative
            if not source.is_file() or hashlib.sha256(source.read_bytes()).hexdigest() != expected: return False
        if name == 'abc-soak' and previous.get('soak_seconds') != args.soak_seconds: return False
    except (OSError, ValueError, KeyError): return False
    results.append({**old, 'reused_from': str(args.resume)})
    print('REUSED ' + name, flush=True)
    return True


# Build the exact binary first; every subsequent suite must report this digest.
execute_command = ['cargo', 'build', '--offline', '--locked', '--all-features', '--target-dir', 'target']
with (run / 'build.log').open('w') as log:
    code = run_command(execute_command, log)
if code != 0: raise SystemExit('Build failed; see ' + str(run / 'build.log'))
binary_hash = hashlib.sha256((ROOT / 'target/debug/clashtui').read_bytes()).hexdigest()
for suite in ['public', 'lifecycle', 'system-lifecycle', 'upgrades']:
    command = ['bash', 'scripts/extended-acceptance.sh', '--suite', suite]
    if suite in ['public', 'upgrades']: command += ['--subscription-file', str(args.subscription_file.resolve())]
    if not reuse(suite, binary_hash): execute(suite, command, ROOT / 'target/extended-acceptance')
if not reuse('abc-soak', binary_hash):
    execute('abc-soak', ['bash', 'scripts/acceptance.sh',
        '--soak-seconds', str(args.soak_seconds)], ROOT / 'target/acceptance')
report = {'suites': results, 'binary_sha256': binary_hash,
          'automatic_available_scopes_passed': not interrupted and len(results) == 5 and all(r['passed'] and r.get('binary_sha256') == binary_hash for r in results),
          'accepted_all_platforms': False,
          'pending': ['H01-H06 human experience', 'native macOS/Windows and their CI',
                      'longer/larger loads than this run', 'core versions beyond the tested versions'],
          'soak_seconds': args.soak_seconds}
report['interrupted'] = interrupted
(run / 'report.json').write_text(json.dumps(report, indent=2))
print('REPORT ' + str(run / 'report.json'), flush=True)
sys.exit(not report['automatic_available_scopes_passed'])
