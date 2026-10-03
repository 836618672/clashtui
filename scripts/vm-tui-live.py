"""TUI operations against the real guest core, with API readback."""
import fcntl, json, os, pty, select, signal, socket, struct, subprocess, termios, time
from pathlib import Path
assert socket.gethostname() == 'clashtui-test'
root='/home/tester/clashtui-test/config'
cli=['clashtui','--config-dir='+root]
database=Path(root)/'clashtui.db'
database_before=database.read_bytes()
master,slave=pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,140,0,0))
def attach():
    os.setsid()
    fcntl.ioctl(0,termios.TIOCSCTTY,0)
child=subprocess.Popen(cli,stdin=slave,stdout=slave,stderr=slave,
    env={**os.environ,'TERM':'xterm-256color'},preexec_fn=attach)
os.close(slave)
output=bytearray()
def collect(seconds):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        if select.select([master],[],[],.05)[0]:
            try: output.extend(os.read(master,65536))
            except OSError: break
def key(value,seconds=.6):
    os.write(master,value)
    collect(seconds)
def mode():
    return json.loads(subprocess.check_output(cli+['core','status'],text=True))['config']['mode']
try:
    key(b'\x1b[1;1R',2)
    assert child.poll() is None and b'ClashTUI' in output,'TUI did not start'
    key(b'6',1)
    key(b'\r')
    key(b'j\r',1)
    assert mode()=='global','TUI mode change not visible in CLI'
    key(b'\r')
    key(b'k\r',1)
    assert mode()=='rule','TUI did not restore rule mode'
    key(b'2',1)
    key(b't',1)
    assert b'Test Result' in output,'Test result dialog missing'
    key(b'\x1b')
    key(b'c',1)
    assert b'Check Passed' in output or b'Check Failed' in output,'Check result dialog missing'
    key(b'\x1b')
    for value in b'234567891':
        key(bytes([value]),.3)
        assert child.poll() is None,'TUI exited during navigation'
    key(b'\x03')
    child.wait(timeout=5)
    assert child.returncode==0,'TUI failed to exit'
    assert database.read_bytes()==database_before,'TUI navigation/exit rewrote database revision'
    print(json.dumps({'mode_global_cli_readback':True,'mode_restored_rule':True,
        'core_test_dialog':True,'core_check_dialog':True,'nine_tabs':True,'clean_exit':True,'database_revision_preserved':True}))
finally:
    if child.poll() is None:
        os.killpg(child.pid,signal.SIGKILL)
        child.wait()
    os.close(master)
    subprocess.run(cli+['mode','set','rule'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
