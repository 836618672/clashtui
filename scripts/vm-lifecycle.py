"""Install/start/autostart/uninstall the current local build in a real guest.

Uses the unmodified installer entry point and real user systemd. Binary sources
are the current VM build and core; no release-download mock or fake systemctl.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import shutil
import yaml

assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0
BASE = Path('/home/tester/clashtui-test')
CONFIG = BASE / 'installed-config'
SYSTEM = len(sys.argv) > 2 and sys.argv[2] == 'system'
INSTALL = Path('/opt/clashtui') if SYSTEM else Path.home() / '.local/clashtui'
UNIT = Path('/usr/lib/systemd/system/clashtui_mihomo.service') if SYSTEM else Path.home() / '.config/systemd/user/clashtui_mihomo.service'
SOURCES = BASE / 'installer-sources'
REPORT = BASE / 'lifecycle-report.json'
ENV = {**os.environ, 'PATH': str(SOURCES) + ':/opt/clashtui/bin:' + os.environ['PATH'],
       'CLASHTUI_CONFIG_DIR': str(CONFIG), 'XDG_RUNTIME_DIR': '/run/user/' + str(os.getuid())}
results = json.loads(REPORT.read_text())['cases'] if REPORT.exists() else []


def command(*args, data=None, success=True):
    value = subprocess.run(args, input=data, env=ENV, capture_output=True, text=True, timeout=90)
    assert (value.returncode == 0) == success, args[0] + ': exit ' + str(value.returncode)
    return value.stdout


def case(identifier, function):
    try:
        row = {'id': identifier, 'status': 'passed', 'evidence': function()}
    except Exception as error:
        row = {'id': identifier, 'status': 'failed', 'error_type': type(error).__name__,
               'error': str(error)[:500]}
    results.append(row); print(json.dumps(row), flush=True)
    REPORT.write_text(json.dumps({'cases': results, 'passed': sum(r['status'] == 'passed' for r in results),
                                 'failed': sum(r['status'] == 'failed' for r in results)}, indent=2))


def current_cli(*args):
    prefix = ['sudo', '-u', 'tester'] if SYSTEM else []
    return json.loads(command(*prefix, str(INSTALL / 'bin/clashtui'), '--config-dir=' + str(CONFIG), *args))


def service(*args):
    return command(*(['sudo', 'systemctl'] if SYSTEM else ['systemctl', '--user']), *args)


def installer(answer, uninstall=False, success=True):
    args = [] if SYSTEM else ['--is-user']
    if uninstall: args.append('--uninstall')
    return command('bash', str(BASE / 'installer/install'), *args, data=answer, success=success)


def ready():
    for _ in range(80):
        try:
            return current_cli('core', 'status')
        except Exception:
            time.sleep(.1)
    raise AssertionError('Installed service not ready')


def install():
    assert not UNIT.exists()
    assert SYSTEM or not INSTALL.exists()
    SOURCES.mkdir(exist_ok=True)
    for name in ['clashtui', 'mihomo']:
        shutil.copy2(Path('/opt/clashtui/bin') / name, SOURCES / name)
    if SYSTEM:
        command('sudo', 'systemctl', 'disable', '--now', 'clashtui-test-web', 'clashtui-test-mihomo')
    installer('n\nn\n')
    assert INSTALL.joinpath('bin/clashtui').is_symlink(), 'Installed CLI not linked to current build'
    assert INSTALL.joinpath('mihomo/mihomo').is_symlink(), 'Installed core not linked to local source'
    assert UNIT.is_file(), 'Installed service unit missing'
    cfg = yaml.safe_load((CONFIG / 'config.yaml').read_text())
    assert cfg['mihomo']['core_service']['is_user'] is (not SYSTEM)
    assert cfg['mihomo']['core']['bin_path'] == str(INSTALL / 'mihomo/mihomo')
    value = yaml.safe_load((INSTALL / 'mihomo/config/config.yaml').read_text())
    original_tun_default = value['tun']['enable']
    assert SYSTEM or original_tun_default is False
    # Separate listeners allow the existing VM acceptance services to stay up.
    value.update({'mixed-port': 17890, 'external-controller': '127.0.0.1:19090',
                  'geo-auto-update': False, 'proxies': [], 'proxy-groups': [], 'rules': ['MATCH,DIRECT']})
    value['tun'] = {'enable': False}; value['dns'] = {'enable': False}
    for p in [INSTALL / 'mihomo/config/config.yaml', CONFIG / 'mihomo/core_override_config.yaml']:
        if SYSTEM and str(p).startswith('/opt/'):
            command('sudo', 'tee', str(p), data=json.dumps(value))
        else: p.write_text(json.dumps(value))
    command(str(INSTALL / 'mihomo/mihomo'), '-t', '-d', str(INSTALL / 'mihomo/config'))
    return {'real_installer': True, 'current_build_reused': True, 'real_user_unit': True,
            'mode': 'system' if SYSTEM else 'user', 'default_tun_enabled': original_tun_default,
            'installed_config_core_validation': True}


def start_enable():
    if not SYSTEM: command('sudo', 'loginctl', 'enable-linger', 'tester')
    service('enable', '--now', 'clashtui_mihomo')
    value = ready()
    assert value['version']['meta']
    assert service('is-enabled', 'clashtui_mihomo').strip() == 'enabled'
    BASE.joinpath('lifecycle-boot-id').write_text(Path('/proc/sys/kernel/random/boot_id').read_text())
    return {'actual_core_api_ready': True, 'enabled': True, 'user_linger_enabled': not SYSTEM}


def reboot_check():
    assert BASE.joinpath('lifecycle-boot-id').read_text() != Path('/proc/sys/kernel/random/boot_id').read_text()
    assert service('is-active', 'clashtui_mihomo').strip() == 'active'
    ready()
    return {'real_guest_power_cycle': True, 'autostart_active': True, 'core_api_ready': True}


def cancel_uninstall():
    installer('n\n', uninstall=True, success=False)
    assert UNIT.exists() and INSTALL.exists()
    ready()
    return {'cancel_nonzero': True, 'installation_preserved': True, 'service_healthy': True}


def uninstall():
    # Exercise uninstall while the enabled service is running: teardown must
    # remove the unit's autostart link and stop the process before deleting files.
    installer('y\n', uninstall=True)
    assert not UNIT.exists() and not INSTALL.exists()
    link = Path('/etc/systemd/system/multi-user.target.wants/clashtui_mihomo.service') if SYSTEM else UNIT.parent / 'default.target.wants/clashtui_mihomo.service'
    assert not link.is_symlink(), 'Autostart symlink left behind'
    p = subprocess.run(['systemctl', *([] if SYSTEM else ['--user']), 'is-active', '--quiet', 'clashtui_mihomo'], env=ENV)
    assert p.returncode != 0, 'Uninstalled service still active'
    with socket.socket() as probe:
        probe.settimeout(1)
        assert probe.connect_ex(('127.0.0.1', 19090)) != 0, 'Uninstalled controller still listening'
    assert CONFIG.exists(), 'User configurations unexpectedly deleted'
    if SYSTEM: assert not Path('/usr/local/bin/clashtui').is_symlink(), 'Dangling global CLI symlink'
    return {'unit_removed': True, 'installation_removed': True, 'autostart_removed': True,
            'service_stopped': True, 'listener_closed': True, 'user_data_preserved': True}


if sys.argv[1] == 'prepare':
    case('L01', install)
    case('L02', start_enable)
else:
    case('L03', reboot_check)
    case('L04', cancel_uninstall)
    case('L05', uninstall)
sys.exit(any(r['status'] != 'passed' for r in results))
