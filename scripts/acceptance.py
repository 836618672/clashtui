"""Reproducible mini-PC acceptance with failure evidence and final restoration."""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import tarfile
import signal
import subprocess
import sys
import time
from acceptance_contract import REQUIRED_C, automatic_result, manual_result
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--panel-archive',type=Path,default=Path('/tmp/clashtui-metacubexd-dist.tgz'))
parser.add_argument('--manual-record',type=Path,help='JSON H01-H06 records for the same binary/environment')
parser.add_argument('--soak-seconds',type=int,default=120)
args=parser.parse_args()
assert 10<=args.soak_seconds<=86400
os.chdir(ROOT)
run=ROOT/'target/acceptance'/datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
run.mkdir(parents=True)
lock=(ROOT/'target/acceptance.lock').open('w')
try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
except BlockingIOError:raise SystemExit('Another acceptance run is active')
os.environ.update(CARGO_TARGET_DIR=str(ROOT/'target'),CLASHTUI_REPORT_DIR=str(run/'isolated'),
    CLASHTUI_VM_REPORT_DIR=str(run.relative_to(ROOT)/'vm'),CLASHTUI_METACUBEXD_ARCHIVE=str(args.panel_archive.resolve()),
    CLASHTUI_CHROMIUM_PATH=shutil.which('chromium') or '',CLASHTUI_SOAK_SECONDS=str(args.soak_seconds))
stages=[];interrupted=False;active=None;vm_touched=False

def interruption(signum,frame):
    global interrupted
    interrupted=True
    if active:
        try:os.killpg(active.pid,signal.SIGTERM)
        except ProcessLookupError:pass
signal.signal(signal.SIGINT,interruption);signal.signal(signal.SIGTERM,interruption)


def stage(name,command,timeout=1800,stdin=None,cleanup=False,binary_output=False):
    global active
    if interrupted and not cleanup:
        stages.append({'name':name,'status':'not_run','reason':'interrupted'});return False
    started=time.monotonic()
    print('RUN '+name,flush=True)
    with (run/(name+'.log')).open('wb') as log:
        try:
            output=(run/(name+'.tgz')).open('wb') if binary_output else log
            active=subprocess.Popen(command,stdin=stdin or subprocess.DEVNULL,stdout=output,stderr=log if binary_output else subprocess.STDOUT,start_new_session=True)
            try:code=active.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(active.pid,signal.SIGKILL);active.wait();code=124
        except OSError as error:log.write(str(error).encode());code=127
        finally:
            if binary_output and 'output' in locals():output.close()
            active=None
    status='passed' if code==0 else 'failed'
    stages.append({'name':name,'status':status,'exit_code':code,'duration_ms':round((time.monotonic()-started)*1000)})
    print(status.upper()+' '+name,flush=True)
    return code==0


def snapshot(suffix):
    with (run/('routes-'+suffix+'.json')).open('wb') as f:subprocess.run(['ip','-j','route'],stdout=f,check=True)
    (run/('dns-'+suffix+'.txt')).write_text(hashlib.sha256(Path('/etc/resolv.conf').read_bytes()).hexdigest())
    p=subprocess.run(['systemctl','is-active','mihomo.service'],capture_output=True,text=True)
    (run/('service-'+suffix+'.txt')).write_text(p.stdout.strip())


def source_manifest():
    names=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).decode().split('\0')
    return {name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in sorted(set(names))
            if name and (ROOT/name).is_file()}


def vm(*arguments):return ['bash','scripts/vm.sh',*arguments]

try:
    snapshot('before')
    (run/'source-manifest.json').write_text(json.dumps(source_manifest(),sort_keys=True,indent=2)+'\n')
    (run/'source-status.txt').write_text(subprocess.check_output(['git','status','--short'],text=True))
    (run/'commit.txt').write_text(subprocess.check_output(['git','rev-parse','HEAD'],text=True))
    if not args.panel_archive.is_file():raise RuntimeError('Pinned panel archive missing; panel tests cannot be silently skipped')
    assert hashlib.sha256(args.panel_archive.read_bytes()).hexdigest()=='a178e00b67acabcda2dcef00afa90be6a7bb261e466a67dad58c8478d9553603'
    stage('guest-before',vm('check'),60)
    # Preserve the pre-reset test environment evidence without reading host profiles.
    stage('guest-evidence-before',vm('ssh','tar --ignore-failed-read -czf - -C /home/tester/clashtui-test acceptance acceptance-c acceptance-c-tui manual'),60,binary_output=True)
    stage('isolated',['node','scripts/test-pipeline.mjs','--browser'])
    stage('installer',['bats','installs/tests/install.bats'])
    stage('acceptance-contract',['python3','scripts/test-acceptance-contract.py'])
    stage('terminal-screen',['python3','scripts/test-terminal-screen.py'])
    built=stage('build',['cargo','build','--locked','--all-features','--target-dir','target'])
    if built:
        binary=ROOT/'target/debug/clashtui'
        (run/'binary-sha256.txt').write_text(hashlib.sha256(binary.read_bytes()).hexdigest())
        (run/'binary-version.txt').write_text(subprocess.check_output([str(binary),'--version'],text=True))
        vm_touched=True
        stage('vm',['bash','scripts/vm-test.sh'],timeout=args.soak_seconds+1800)
        stage('shell-syntax',['bash','-c','for file in scripts/acceptance.sh scripts/vm-test.sh scripts/prepare-human.sh; do bash -n "$file" || exit; done'])
        stage('browser-syntax',['node','--check','scripts/vm-c-browser.mjs'])
        stage('documentation',['python3','scripts/check-docs.py'])
        stage('guest-journal',vm('ssh','journalctl -u clashtui-test-mihomo -u clashtui-test-web --no-pager -n 300'),60)
        stage('guest-evidence-after',vm('ssh','tar --ignore-failed-read -czf - -C /home/tester/clashtui-test acceptance acceptance-c acceptance-c-tui manual'),60,binary_output=True)
except Exception as error:
    stages.append({'name':'orchestrator','status':'failed','error':str(error)})
finally:
    if vm_touched:
        restoration=True
        for name,command in [('restore-stop',vm('stop')),('restore-reset',vm('reset')),('restore-start',vm('start')),('restore-wait',vm('wait','--seconds','45')),('restore-push',vm('push')),('restore-permissions',vm('ssh','sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo; sudo systemctl restart clashtui-test-web')),('restore-tunnel',vm('tunnel')),('restore-check',vm('check'))]:
            if not stage(name,command,120,cleanup=True):restoration=False;break
        if not restoration:stages.append({'name':'restoration-incomplete','status':'failed'})
    try:
        snapshot('after')
        unchanged=all((run/(name+'-before.'+ext)).read_bytes()==(run/(name+'-after.'+ext)).read_bytes() for name,ext in [('routes','json'),('dns','txt'),('service','txt')])
        stages.append({'name':'host-invariants','status':'passed' if unchanged else 'failed'})
    except Exception as error:stages.append({'name':'host-invariants','status':'failed','error':str(error)})
    try:
        source_unchanged=json.loads((run/'source-manifest.json').read_text())==source_manifest()
        stages.append({'name':'source-invariants','status':'passed' if source_unchanged else 'failed'})
    except (OSError, ValueError) as error:
        stages.append({'name':'source-invariants','status':'failed','error':str(error)})
    checks={}
    for name,file in [('isolated',run/'isolated/report.json'),('vm',run/'vm/report.json')]:
        try:checks[name]=json.loads(file.read_text())
        except (OSError,ValueError):stages.append({'name':name+'-report-missing','status':'failed'})
    for archive in run.glob('guest-evidence-*.tgz'):
        try:
            with tarfile.open(archive) as saved:
                members=saved.getmembers()
                if archive.stem=='guest-evidence-after': assert members, 'Empty evidence archive'
        except (tarfile.TarError, OSError, AssertionError) as error:
            stages.append({'name':archive.stem+'-integrity','status':'failed','error':str(error)})
    expected=REQUIRED_C
    decision=automatic_result(stages,checks,interrupted)
    missing=decision['missing'];failed_cases=decision['failed'];automatic=decision['passed']
    manual={};manual_valid=False
    if args.manual_record:
        try:
            manual=json.loads(args.manual_record.read_text())
            manual_valid=manual_result(manual,(run/'binary-sha256.txt').read_text().strip())
        except (OSError,ValueError):pass
    report={'created_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Linux Debian 12 / Mihomo 1.19.24 / pinned MetaCubeXD / controlled guest networking',
        'stages':stages,'reports':checks,'required_c_cases':sorted(expected),'missing_c_cases':missing,'failed_c_cases':failed_cases,'duplicate_c_ids':decision['duplicate_ids'],
        'automatic_passed':automatic,'manual_valid_for_binary':manual_valid,'accepted_current_scope':automatic and manual_valid,
        'accepted':False, # This entry has no real-public-network or native-platform evidence.
        'binary_sha256':(run/'binary-sha256.txt').read_text().strip() if (run/'binary-sha256.txt').exists() else None,
        'manual_pending':['H%02d'%i for i in range(1,7)] if not manual_valid else [],
        'external_scope_pending':['public remote subscription/proxy/upgrade','native macOS/Windows','other core versions'],
        'interrupted':interrupted}
    (run/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print('REPORT '+str(run/'report.json'),flush=True)
    print('automatic_passed='+str(automatic)+' accepted_current_scope='+str(report['accepted_current_scope'])+' accepted='+str(report['accepted']),flush=True)
raise SystemExit(0 if automatic else 1)
