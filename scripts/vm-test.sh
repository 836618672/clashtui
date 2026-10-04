#!/usr/bin/env bash
# Disposable guest only. Evidence is preserved; acceptance.sh adds final restoration.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$script_dir/.."
if ! command -v node >/dev/null || ! command -v chromium >/dev/null; then
  exec nix shell nixpkgs#nodejs nixpkgs#chromium -c bash "$script_dir/vm-test.sh" "$@"
fi
report_dir="${CLASHTUI_VM_REPORT_DIR:-target/vm/results-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$report_dir"
export CLASHTUI_CHROMIUM_PATH="${CLASHTUI_CHROMIUM_PATH:-$(command -v chromium)}"
export CLASHTUI_BASE_BROWSER_REPORT="$PWD/${CLASHTUI_VM_DIR:-target/vm}/browser"
export CLASHTUI_C_BROWSER_REPORT="$PWD/$report_dir/c-browser"
failed=0
stage() {
  local name="$1"; shift
  local code=0
  "$@" > "$report_dir/$name.log" 2>&1 || code=$?
  printf '%s\t%d\n' "$name" "$code" >> "$report_dir/stages.tsv"
  printf '%s exit=%d\n' "$name" "$code"
  if ((code)); then failed=1; fi
}
guest() { bash scripts/vm.sh ssh "CLASHTUI_SOAK_SECONDS=${CLASHTUI_SOAK_SECONDS:-120} python3 - ${2:-}" < "$1"; }
fetch() { bash scripts/vm.sh ssh "cat $1" > "$report_dir/$2"; }
stop_fixture() {
  bash scripts/vm.sh ssh 'if test -f /home/tester/clashtui-test/manual/lab.pid; then kill "$(cat /home/tester/clashtui-test/manual/lab.pid)" 2>/dev/null || true; fi' > "$report_dir/fixture-stop.log" 2>&1 || true
}
trap stop_fixture EXIT
bash scripts/vm.sh stop
bash scripts/vm.sh reset
bash scripts/vm.sh start
bash scripts/vm.sh wait
bash scripts/vm.sh push
bash scripts/vm.sh ssh 'sudo systemctl restart clashtui-test-web'
bash scripts/vm.sh tunnel
stage integration guest scripts/vm-integration.py
stage integration-report fetch /home/tester/clashtui-test/acceptance/report.json integration.json
stage network guest scripts/vm-network.py
bash scripts/vm.sh ssh 'sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo'
stage tui-navigation guest tests/pipeline/tui_smoke.py '/opt/clashtui/bin/clashtui /home/tester/clashtui-test/config'
stage tui-operations guest scripts/vm-tui-live.py
stage browser node scripts/vm-browser.mjs
cp -a "$CLASHTUI_BASE_BROWSER_REPORT" "$report_dir/base-browser"
stage c-workflows guest scripts/vm-c-workflows.py
stage c-workflows-report fetch /home/tester/clashtui-test/acceptance-c/report.json c-workflows.json
prepare_fixture() {
  bash scripts/vm.sh ssh 'mkdir -p /home/tester/clashtui-test/manual; cat > /home/tester/clashtui-test/manual/terminal_screen.py' < scripts/terminal_screen.py
  bash scripts/vm.sh ssh 'mkdir -p /home/tester/clashtui-test/manual; cat > /home/tester/clashtui-test/manual/lab.py' < scripts/manual-test-lab.py
  bash scripts/vm.sh ssh 'python3 /home/tester/clashtui-test/manual/lab.py prepare; nohup python3 /home/tester/clashtui-test/manual/lab.py serve > /home/tester/clashtui-test/manual/lab.log 2>&1 < /dev/null & echo $! > /home/tester/clashtui-test/manual/lab.pid'
  bash scripts/vm.sh ssh 'python3 -c "import urllib.request,time; time.sleep(.3); assert urllib.request.urlopen(\"http://127.0.0.1:18080/hello\").read()==b\"manual-lab-ok\\n\""'
}
stage fixture prepare_fixture
stage c-browser node scripts/vm-c-browser.mjs
stage c-tui-permissions bash scripts/vm.sh ssh 'sudo chgrp -R tester /home/tester/clashtui-test/config/mihomo; chmod g+s /home/tester/clashtui-test/config/mihomo; chmod -R g+w /home/tester/clashtui-test/config/mihomo'
stage c-tui guest scripts/vm-c-tui.py
stage c-tui-report fetch /home/tester/clashtui-test/acceptance-c-tui/report.json c-tui.json
python3 - "$report_dir" <<'PY'
import json,sys
from pathlib import Path
p=Path(sys.argv[1]);cases=[];stages=[]
for line in (p/'stages.tsv').read_text().splitlines():
    name,code=line.split('\t');stages.append({'name':name,'exit_code':int(code)})
for file in ['integration.json','base-browser/report.json','c-workflows.json','c-browser/report.json','c-tui.json']:
    try:cases+=json.loads((p/file).read_text())['cases']
    except (OSError,ValueError,KeyError) as e:cases.append({'name':'Missing report '+file,'status':'failed','error':str(e)})
try:
    line=next(line for line in (p/'network.log').read_text().splitlines() if line.startswith('SUMMARY '));cases+=json.loads(line[8:])['cases']
except (StopIteration,OSError,ValueError) as e:cases.append({'name':'Missing network report','status':'failed','error':str(e)})
for name in ['tui-navigation','tui-operations']:
    try:
        value=json.loads((p/(name+'.log')).read_text())
        assert all(value[k] for k in (['startup','clean_exit'] if name=='tui-navigation' else ['core_test_dialog','core_check_dialog','database_revision_preserved','clean_exit']))
        cases.append({'name':name,'status':'passed','evidence':value})
    except (OSError,ValueError,KeyError,AssertionError):cases.append({'name':name,'status':'failed'})
report={'cases':cases,'stages':stages,'passed':sum(c['status']=='passed' for c in cases),
        'failed':sum(c['status'] in ['failed','known_failure'] for c in cases),'skipped':sum(c['status']=='skipped' for c in cases)}
(p/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print('VM report: '+str(p/'report.json'))
raise SystemExit(bool(report['failed'] or any(s['exit_code'] for s in stages)))
PY
exit "$failed"
