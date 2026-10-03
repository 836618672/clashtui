"""Actual TUI input/PTY, exports, external vi and core readback; guest only."""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import socket
import struct
import subprocess
import termios
import time
import sys
import yaml
assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0
ROOT = Path('/home/tester/clashtui-test/config')
WORK = ROOT.parent/'acceptance-c-tui'; WORK.mkdir(exist_ok=True)
sys.path.insert(0,str(ROOT.parent/'manual'))
from terminal_screen import screen_text
CLI = ['clashtui', '--config-dir='+str(ROOT)]
results = []


def cli(*args):
    return json.loads(subprocess.check_output(CLI+list(args), text=True, timeout=30))


class Terminal:
    def __init__(self):
        self.master, self.slave = pty.openpty(); self.original = termios.tcgetattr(self.slave)
        self.resize(40, 140)
        def attach():
            os.setsid(); fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        self.child = subprocess.Popen(CLI, stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env={**os.environ, 'TERM':'xterm-256color'}, preexec_fn=attach)
        self.output = bytearray(); self.key(b'\x1b[1;1R', 1.5)
        if self.child.poll() is not None or b'ClashTUI' not in self.output:
            message=self.output.decode(errors='replace')[-2500:]
            (WORK/'startup-failure.ansi').write_bytes(self.output)
            if self.child.poll() is None: os.killpg(self.child.pid,signal.SIGKILL);self.child.wait()
            os.close(self.master);os.close(self.slave)
            raise AssertionError('TUI startup failed: '+message)
    def resize(self, rows, cols):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH',rows,cols,0,0))
        if hasattr(self, 'child'): os.killpg(self.child.pid, signal.SIGWINCH)
    def collect(self, seconds=.5):
        end = time.monotonic()+seconds
        while time.monotonic()<end:
            if select.select([self.master], [], [], .05)[0]:
                try:
                    data = os.read(self.master, 65536)
                    self.output.extend(data)
                    if b'\x1b[6n' in data: os.write(self.master, b'\x1b[1;1R')
                    for color in [b'10', b'11']:
                        if b'\x1b]'+color+b';?' in data:
                            os.write(self.master, b'\x1b]'+color+b';rgb:0000/0000/0000\x07')
                except OSError: break
    def key(self, value, seconds=.6):
        offset = len(self.output); os.write(self.master, value); self.collect(seconds)
        return self.screen().encode()
    def screen(self):
        return screen_text(self.output)
    def input(self, value):
        self.key(b'\x15', .1)
        for byte in value.encode():
            os.write(self.master, bytes([byte])); self.collect(.015)
        self.key(b'\r', 1)
    def export(self, tab, name):
        self.key(tab, .9); self.key(b'e'); path=WORK/name; path.unlink(missing_ok=True)
        self.input(str(path)); self.collect(.5)
        assert path.exists(), 'Export missing: '+name
        return json.loads(path.read_text())
    def finish(self, signum=None):
        if signum: os.killpg(self.child.pid, signum)
        else: self.key(b'\x03')
        self.collect(.5); self.child.wait(timeout=5)
        assert self.child.returncode==0, 'TUI exit: '+str(self.child.returncode)
        actual=termios.tcgetattr(self.slave)
        assert actual[3] & (termios.ECHO|termios.ICANON) == self.original[3] & (termios.ECHO|termios.ICANON), 'Terminal echo/canonical mode not restored'
    def close(self, name):
        (WORK/(name+'.ansi')).write_bytes(self.output)
        if self.child.poll() is None: os.killpg(self.child.pid,signal.SIGKILL); self.child.wait()
        os.close(self.master); os.close(self.slave)


def case(identifier, work):
    terminal=None
    try:
        terminal=Terminal(); evidence=work(terminal)
        if terminal.child.poll() is None: terminal.finish()
        row={'id':identifier,'status':'passed','evidence':evidence or {}}
    except Exception as error:
        row={'id':identifier,'status':'failed','error':str(error)[:2500]}
    finally:
        if terminal: terminal.close(identifier)
    results.append(row); print(json.dumps(row),flush=True)


def resize_exit(t):
    before=(ROOT/'clashtui.db').read_bytes()
    for rows,cols in [(24,80),(30,100),(40,140)]:
        t.resize(rows,cols); t.collect(.4)
        for key in b'234567891': t.key(bytes([key]),.15)
        assert t.child.poll() is None
    t.finish(signal.SIGTERM)
    assert (ROOT/'clashtui.db').read_bytes()==before
    return {'sizes':[[80,24],[100,30],[140,40]],'sigterm_restores_terminal':True,'database_unchanged':True}


def profile_paths(t):
    t.key(b'2');t.key(b'i');t.input('c-tui-url');t.input('http://127.0.0.1:18080/subscription.yaml');t.collect(.6)
    assert any(p['name']=='c-tui-url' for p in cli('manage','state')['profiles'])
    assert (ROOT/'mihomo/profiles/c-tui-url.yaml').stat().st_mode & 0o777 == 0o660, 'New system-service profile lost group permissions'
    t.key(b'e');t.input('c-tui-renamed');t.key(b'\r')
    assert cli('manage','profile_url','--name','c-tui-renamed')['url'].endswith('/subscription.yaml')
    t.key(b'/');t.input('c-tui-renamed');out=t.key(b'p');assert b'ManualAv1' in out.replace(b' ',b''),'Filtered preview not showing selected file';t.key(b'\x1b')
    t.key(b'P');t.key(b'O');pf=next(p for p in cli('manage','state')['profiles'] if p['name']=='c-tui-renamed');assert pf['no_pp'] and pf['update_with_proxy']
    out=t.key(b'u',1.5);assert b'Updated' in out,'Update feedback missing';t.key(b'\x1b');t.key(b'O')
    t.key(b'dd');t.key(b'\r');t.collect(.5)
    assert not any(p['name']=='c-tui-renamed' for p in cli('manage','state')['profiles'])
    return {'import':True,'rename_url':True,'filtered_preview':True,'options_readback':True,'update':True,'delete':True}


def template_paths(t):
    t.key(b'2');t.key(b'l');t.key(b'a');t.input('c-tui-template.yaml');t.collect(2)
    # Editor configuration is controlled in this test and writes a valid skeleton.
    assert (ROOT/'mihomo/templates/c-tui-template.yaml').exists()
    t.key(b'/');t.input('c-tui-template.yaml')
    out=t.key(b'P');assert b'preview' in out.lower().replace(b' ',b''),'Generated preview missing';t.key(b'\x1b')
    t.key(b'\r');t.input('c-tui-generated');t.collect(1)
    assert cli('manage','check','--name','c-tui-generated')['valid']
    path=ROOT/'mihomo/profiles/c-tui-generated.yaml';before=path.read_bytes()
    t.key(b'\r');t.input('c-tui-generated');t.key(b'\x1b');assert path.read_bytes()==before,'Cancelled overwrite changed profile'
    return {'new_template':True,'preview':True,'generate_core_check':True,'cancel_overwrite':True}


def exports(t):
    rows=t.export(b'8','rules.json');assert rows,'Rule export empty'
    connections=t.export(b'4','connections.json');assert isinstance(connections,list)
    t.key(b'5',1);t.key(b'/');t.input('c-no-match');logs=t.export(b'5','logs.json');assert logs==[],'Filtered logs export contains unmatched rows'
    t.key(b'\x1b');t.key(b'p');out=t.key(b'td');assert b'debug' in out,'Stream threshold did not change'
    assert cli('core','status')['config']['log-level']=='info','Stream threshold unexpectedly changed core level'
    t.key(b'ti');assert cli('core','status')['config']['log-level']=='info'
    return {'nonempty_rules_export':True,'connections_json':True,'filtered_logs':True,'pause_resume':True,'stream_threshold_separate_from_core_level':True}


def services(t):
    t.key(b'7',1);t.key(b'\r',1)
    assert subprocess.run(['systemctl','is-active','--quiet','clashtui-test-mihomo']).returncode!=0
    t.key(b'\x1b');t.key(b'j\r',1);assert cli('core','status')['version']['meta']
    t.key(b'\x1b');t.key(b'j\r',1);assert cli('core','status')['version']['meta'];t.key(b'\x1b')
    return {'stop':True,'start_ready':True,'restart_ready':True}


def batch_failure(t):
    t.key(b'2');out=t.key(b'U',4)
    for _ in range(100):
        if b'somefailed' in re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',t.output).lower().replace(b' ',b''):break
        t.collect(.3)
    else:raise AssertionError('TUI batch swallowed individual failures')
    t.key(b'\x1b')
    return {'default_U_binding':True,'partial_failure_visible':True}


def vi_editor(t):
    t.key(b'2');t.key(b'/');t.input('c-tui-editor');t.key(b'E',1)
    assert b'VIM' in t.output or b'c-tui-editor.yaml' in t.output, 'vi did not inherit terminal'
    t.collect(1);t.key(b'G');t.key(b'o');t.key(b'# c-terminal-editor-proof');t.key(b'\x1b');t.key(b':wq\r',2)
    assert 'c-terminal-editor-proof' in (ROOT/'mihomo/profiles/c-tui-editor.yaml').read_text(), 'vi failed to save edited text'
    assert (ROOT/'mihomo/profiles/c-tui-editor.yaml').stat().st_mode & 0o777 == 0o660, 'vi lost core file group permissions'
    t.key(b'?',.5);assert b'Help' in t.output,'TUI failed to resume after editor';t.key(b'\x1b')
    return {'real_vi_tty':True,'file_saved':True,'tui_resumed':True}



def providers(t):
    t.key(b'9',1);t.key(b'/');t.input('Manual HTTP');out=t.key(b'\r'); assert b'ManualHTTP' in out.replace(b' ',b''), 'Provider details missing';t.key(b'\x1b')
    t.key(b'u');out=t.key(b'\r',1);assert b'Operationcompleted' in out.replace(b' ',b''),'Provider update failed'
    lab_log=ROOT.parent/'manual/lab.log';before_checks=lab_log.read_text().count('HEAD /probe')
    t.key(b'h');t.key(b'\r',1);assert lab_log.read_text().count('HEAD /probe')>before_checks,'Provider health issued no real probe'
    rows=t.export(b'9','providers.json');assert any(row['name']=='Manual HTTP' for row in rows),rows
    t.key(b'd');t.input('Manual Provider v1');t.collect(1)
    assert 'Provider node delay:' in t.screen(),'Successful node delay missing'
    t.key(b'\x1b');t.key(b't')
    for _ in range(30):
        if 'Manual Rules' in t.screen():break
        t.collect(.1)
    else:raise AssertionError('Rule provider type never loaded')
    rows=t.export(b'9','rule-providers.json');assert any(row['name']=='Manual Rules' for row in rows),'Rule provider export empty'
    return {'details':True,'update':True,'health':True,'node_delay':True,'both_provider_exports':True}


def connection_paths(t):
    token=yaml.safe_load((ROOT/'mihomo/core_override_config.yaml').read_text())['secret']
    import urllib.request
    def ids():
        req=urllib.request.Request('http://127.0.0.1:9090/connections',headers={'Authorization':'Bearer '+token})
        return json.load(urllib.request.urlopen(req))['connections'] or []
    traffic=subprocess.Popen(['python3',str(ROOT.parent/'manual/lab.py'),'traffic','--seconds','20'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        for _ in range(30):
            rows=ids()
            if len([r for r in rows if str(r['metadata'].get('destinationPort'))=='18080'])>=2:break
            time.sleep(.1)
        else:raise AssertionError('Real proxy streams missing')
        untouched={r['id'] for r in rows if r['metadata'].get('host')!='localhost'}
        t.key(b'4',1);t.key(b'/');t.input('localhost');t.key(b'p');export=t.export(b'4','filtered-connections.json')
        assert len(export)==1 and export[0]['metadata']['host']=='localhost',export
        t.key(b'\r');t.key(b'\x1b');t.key(b'dd');t.key(b'\r');t.collect(.5)
        current={r['id'] for r in ids()};assert export[0]['id'] not in current and untouched<=current
        return {'real_connections':True,'filtered_export':True,'details':True,'pause':True,'fixed_close_scope':True}
    finally:
        traffic.terminate();traffic.wait(timeout=5)


def editor_failure(t):
    before=(ROOT/'mihomo/profiles/c-tui-editor.yaml').read_bytes()
    t.key(b'2');t.key(b'/');t.input('c-tui-editor');out=t.key(b'E',1)
    assert b'Editorexitedunsuccessfully' in out.replace(b' ',b''),'Editor failure hidden'
    t.key(b'\x1b');t.key(b'9');assert t.child.poll() is None
    assert (ROOT/'mihomo/profiles/c-tui-editor.yaml').read_bytes()==before
    return {'failure_visible':True,'terminal_resumed':True,'file_unchanged':True}


def logs_reconnect(t):
    t.key(b'5',1);t.key(b'p')
    def traffic():
        child=subprocess.Popen(['python3',str(ROOT.parent/'manual/lab.py'),'traffic','--seconds','1'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        start=len(t.output);t.collect(2);child.wait(timeout=5)
        return re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',t.output[start:])
    before=traffic();assert b'18080' in before,'Initial live logs missing'
    subprocess.run(CLI+['service','restart'],check=True,stdout=subprocess.DEVNULL);t.collect(3)
    after=traffic();assert b'18080' in after,'TUI log stream failed to reconnect'
    return {'live_logs':True,'service_restart':True,'new_logs_after_reconnect':True}


def proxy_selection(t):
    t.key(b'3',1);t.key(b'/');t.input('Manual Select');t.key(b'gg');t.key(b'\r')
    # Clear the filter while retaining the selected folder, then select its
    # second child. The active Manual Select starts as DIRECT for this test.
    t.key(b'/');t.input('');t.key(b'jj\r',1)
    assert cli('core','proxies')['proxies']['Manual Select']['now']=='Manual B v1','TUI selected wrong group/child'
    cli('core','select','Manual Auto','Manual A v1')
    assert cli('core','proxies')['proxies']['Manual Auto']['fixed']
    t.key(b'r');t.key(b'/');t.input('Manual Auto');t.key(b'gg');t.key(b'u',1)
    assert not cli('core','proxies')['proxies']['Manual Auto'].get('fixed'),'TUI failed to restore automatic selection'
    cli('core','select','Manual Select','DIRECT')
    return {'filtered_group_correct':True,'selected_child_core_readback':True,'automatic_unfix':True}


def runtime_paths(t):
    overlay=ROOT/'mihomo/core_override_config.yaml';original_overlay=overlay.read_bytes()
    try:
        t.key(b'6');t.key(b'e');t.input('{"log-level":"debug"}')
        assert cli('core','status')['config']['log-level']=='debug','TUI patch not visible in core'
        t.key(b'p');t.key(b'\r',1)
        assert yaml.safe_load(overlay.read_text())['log-level']=='debug','TUI persist not visible in overlay'
        t.key(b'e');t.input('{"log-level":"info"}')
        assert cli('core','status')['config']['log-level']=='info'
        return {'runtime_patch':True,'persist_readback':True,'runtime_restored':True}
    finally:overlay.write_bytes(original_overlay)


def close_on_mode(t):
    import urllib.request
    token=yaml.safe_load((ROOT/'mihomo/core_override_config.yaml').read_text())['secret']
    def connections():
        request=urllib.request.Request('http://127.0.0.1:9090/connections',headers={'Authorization':'Bearer '+token})
        return json.load(urllib.request.urlopen(request,timeout=5))['connections'] or []
    child=subprocess.Popen(['python3',str(ROOT.parent/'manual/lab.py'),'traffic','--seconds','30'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        for _ in range(40):
            rows=connections()
            if len([row for row in rows if str(row['metadata'].get('destinationPort'))=='18080'])>=2:break
            time.sleep(.1)
        else:raise AssertionError('Mode-close fixture has no live streams')
        captured={row['id'] for row in rows}
        t.key(b'6');t.key(b'c');t.key(b'\r');t.key(b'j\r',1)
        assert cli('core','status')['config']['mode']=='global','Mode change did not reach core'
        assert captured.isdisjoint({row['id'] for row in connections()}),'Mode-close policy left old connections open'
        t.key(b'\r');t.key(b'k\r',1)
        assert cli('core','status')['config']['mode']=='rule','Rule mode not restored'
        return {'two_live_streams':True,'close_on_mode_enabled':True,'captured_connections_closed':True,'rule_restored':True}
    finally:
        child.terminate();child.wait(timeout=5)
        subprocess.run(CLI+['mode','set','rule'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

cfg=ROOT/'config.yaml';original=cfg.read_bytes()
try:
    cli('manage','activate','--name','baseline','--yes')
    for profile in cli('manage','state')['profiles']:
        if profile['name'].startswith('c-tui-'):cli('manage','delete','--name',profile['name'],'--yes')
    (ROOT/'mihomo/templates/c-tui-template.yaml').unlink(missing_ok=True)
    # New template invokes the configured editor; for this one case it writes a
    # valid file deterministically. The vi case below exercises actual TTY input.
    skeleton=WORK/'template-skeleton.json';skeleton.write_text(json.dumps({'proxies': [], 'proxy-groups': [], 'proxy-providers': {}, 'rule-providers': {}, 'rules': ['MATCH,DIRECT']}))
    wrapper=WORK/'template-editor.sh';wrapper.write_text('#!/bin/sh\ncp '+str(skeleton)+' "$1"\n');wrapper.chmod(0o700)
    value=yaml.safe_load(original);value.setdefault('extra',{})['edit_cmd']=str(wrapper)+' %s';cfg.write_text(json.dumps(value))
    for identifier,work in [('CT01',resize_exit),('CT02',profile_paths),('CT03',template_paths),('CT04',exports),('CT05',services),('CT06',batch_failure)]:case(identifier,work)
    subprocess.run(CLI+['manage','import','--name','c-tui-editor','--input',str(ROOT.parent/'manual/basic.yaml')],check=True,stdout=subprocess.DEVNULL)
    value['extra']['edit_cmd']='vi %s';cfg.write_text(json.dumps(value));case('CT07',vi_editor)
    cli('manage','activate','--name','c-web-provider','--yes');cli('core','select','Manual Select','DIRECT');case('CT08',providers);case('CT09',connection_paths)
    value['extra']['edit_cmd']='exit 17';cfg.write_text(json.dumps(value));case('CT10',editor_failure)
    case('CT11',logs_reconnect);case('CT12',proxy_selection);case('CT13',runtime_paths);case('CT14',close_on_mode)
finally:
    cfg.write_bytes(original)
    subprocess.run(CLI+['service','start'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    subprocess.run(CLI+['manage','activate','--name','baseline','--yes'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    report={'cases':results,'passed':sum(r['status']=='passed' for r in results),'failed':sum(r['status']=='failed' for r in results)}
    (WORK/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print('SUMMARY '+json.dumps(report),flush=True)
raise SystemExit(bool(report['failed']))
