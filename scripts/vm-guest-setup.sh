#!/usr/bin/env bash
# Run only inside the guest, as tester. Never invoke on the host.
set -euo pipefail
[[ $(hostname) == clashtui-test ]] || { echo 'This script is only for the clashtui-test guest' >&2; exit 1; }
[[ $(id -un) == tester ]] || { echo 'Run as tester inside the guest' >&2; exit 1; }
test -x /opt/clashtui/bin/mihomo
test -d /opt/clashtui/panel
test ! -f /home/tester/clashtui-test/config/config.yaml || { echo 'Guest is already configured; use a snapshot to reset' >&2; exit 1; }
python3 - <<'PY'
import json, secrets, shutil
from pathlib import Path
root = Path('/home/tester/clashtui-test/config')
core = root / 'mihomo'
for path in [core / 'profiles', core / 'templates']:
    path.mkdir(parents=True, exist_ok=True)
root.chmod(0o700)
shutil.copytree('/opt/clashtui/panel', core / 'uis/metacubexd')
secret = secrets.token_urlsafe(32)
overlay = {'mixed-port':7890, 'external-controller':'127.0.0.1:9090', 'secret':secret,
    'external-ui':'uis/metacubexd', 'allow-lan':False, 'ipv6':False, 'mode':'rule',
    'log-level':'info', 'tun':{'enable':False}, 'dns':{'enable':False}, 'geo-auto-update':False}
(core / 'core_override_config.yaml').write_text(json.dumps(overlay))
(core / 'config.yaml').write_text(json.dumps({**overlay, 'proxies':[],
    'proxy-groups':[{'name':'Test Select','type':'select','proxies':['DIRECT','REJECT']}], 'rules':['MATCH,DIRECT']}))
(root.parent / 'baseline.yaml').write_text(json.dumps({'proxies':[],
    'proxy-groups':[{'name':'Test Select','type':'select','proxies':['DIRECT','REJECT']}], 'rules':['MATCH,DIRECT']}))
(root / 'config.yaml').write_text(json.dumps({'mihomo':{
    'core':{'config_dir':str(core), 'config_path':str(core/'config.yaml'), 'bin_path':'/opt/clashtui/bin/mihomo'},
    'core_service':{'service_name':'clashtui-test-mihomo','is_user':False,'service_controller':'systemd'}}, 'timeout':5}))
(root / 'management-token').write_text(secrets.token_urlsafe(32))
for file in [root / 'management-token', core / 'core_override_config.yaml', core / 'config.yaml']:
    file.chmod(0o600)
PY
chmod g+s /home/tester/clashtui-test/config/mihomo
chmod -R g+w /home/tester/clashtui-test/config/mihomo
sudo tee /etc/systemd/system/clashtui-test-mihomo.service >/dev/null <<'UNIT'
[Unit]
Description=ClashTui VM test Mihomo
After=network.target
[Service]
User=tester
ExecStart=/opt/clashtui/bin/mihomo -d /home/tester/clashtui-test/config/mihomo -f /home/tester/clashtui-test/config/mihomo/config.yaml
Restart=on-failure
[Install]
WantedBy=multi-user.target
UNIT
sudo tee /etc/systemd/system/clashtui-test-web.service >/dev/null <<'UNIT'
[Unit]
Description=ClashTui VM management companion
After=network.target
[Service]
User=tester
ExecStart=/opt/clashtui/bin/clashtui --config-dir=/home/tester/clashtui-test/config web --listen 127.0.0.1:9091 --token-file /home/tester/clashtui-test/config/management-token
Restart=on-failure
[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
clashtui --config-dir=/home/tester/clashtui-test/config manage import --name baseline --input /home/tester/clashtui-test/baseline.yaml >/dev/null
sudo systemctl enable --now clashtui-test-mihomo.service clashtui-test-web.service
ready=false
for attempt in {1..20}; do
  if clashtui --config-dir=/home/tester/clashtui-test/config core status >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 0.5
done
[[ $ready == true ]] || { echo 'Guest core did not become ready; inspect guest journal' >&2; exit 1; }
echo 'Guest test core and management services started; TUN and DNS are disabled'
