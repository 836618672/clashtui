"""Exercise the actual terminal without executing service or network changes."""
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

binary, root = sys.argv[1:3]
mutations = len(sys.argv) > 3 and sys.argv[3] == "mutations"
diagnostics = len(sys.argv) > 3 and sys.argv[3] == "diagnostics"
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))


def attach_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


env = dict(os.environ, TERM="xterm-256color", PATH=root + "/bin:" + os.environ["PATH"])
child = subprocess.Popen([binary, "--config-dir=" + root], stdin=slave,
                         stdout=slave, stderr=slave, env=env,
                         preexec_fn=attach_terminal)
os.close(slave)
output = bytearray()


def collect(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if select.select([master], [], [], min(.05, deadline - time.monotonic()))[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            output.extend(chunk)


try:
    # Crossterm requests cursor position during startup.
    os.write(master, b"\x1b[1;1R")
    collect(2)
    assert child.poll() is None, output.decode(errors="replace")
    assert b"ClashTUI" in output, output.decode(errors="replace")
    if diagnostics:
        os.write(master, b"2")
        collect(.6)
        for key in [b"t", b"c"]:
            os.write(master, key)
            collect(.8)
            assert b"CORE-DIAGNOSTIC" in output, output.decode(errors="replace")
            os.write(master, b"\x1b")
            collect(.3)
    if mutations:
        # Restore a group's automatic selection and test an exact provider node.
        os.write(master, b"3")
        collect(.6)
        os.write(master, b"u")
        collect(.8)
        os.write(master, b"9")
        collect(.6)
        os.write(master, b"d")
        collect(.3)
        os.write(master, b"same / node\r")
        collect(.8)
        # Start is a separate operation and must not issue restart.
        os.write(master, b"7")
        collect(.4)
        os.write(master, b"j\r")
        collect(.8)
        os.write(master, b"\x1b")
        collect(.3)
        # Generate two independently named profiles from the same template.
        os.write(master, b"2")
        collect(.5)
        os.write(master, b"l")
        collect(.3)
        for name in [b"tui-generated-a", b"tui-generated-b"]:
            os.write(master, b"\r")
            collect(.3)
            os.write(master, b"\x15" + name + b"\r")
            collect(.8)
        # Cancelling a replacement confirmation must preserve the output file.
        with open(root + "/mihomo/templates/pty.yaml", "a") as template:
            template.write("mode: global\n")
        os.write(master, b"\r")
        collect(.3)
        os.write(master, b"\x15tui-generated-a\r")
        collect(.3)
        os.write(master, b"\x1b")
        collect(.3)
    for key in b"234567891":
        os.write(master, bytes([key]))
        collect(.35)
        assert child.poll() is None, "TUI exited while navigating tabs"
    # Help and return to normal rendering; no mutation shortcuts are sent.
    os.write(master, b"?")
    collect(.3)
    os.write(master, b"\x1b")
    collect(.2)
    os.write(master, b"\x03")
    collect(.5)
    child.wait(timeout=5)
    assert child.returncode == 0, output.decode(errors="replace")
    for title in [b"Status", b"Files", b"Proxies", b"Conns", b"Logs", b"Settings", b"Service", b"Rules", b"Providers"]:
        assert title in output, "Missing tab: " + title.decode()
    print(json.dumps({"startup": True, "navigation_keys": "234567891", "clean_exit": True, "mutations": mutations}))
finally:
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()
    os.close(master)
