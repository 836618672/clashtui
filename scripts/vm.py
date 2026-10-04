"""Disposable Linux guest. No host service, TAP, bridge or directory sharing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import socket
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parent.parent
VM = Path(os.environ.get("CLASHTUI_VM_DIR", ROOT / "target/vm")).resolve()
if not VM.is_relative_to(ROOT / "target"):
    raise RuntimeError("VM directory must stay inside the project target directory")
BASE = VM / "debian-12-genericcloud-amd64.qcow2"
DISK = VM / "guest.qcow2"
IMAGE_URL = "https://cloud.debian.org/images/cloud/bookworm/latest/"
SSH_PORT = int(os.environ.get("CLASHTUI_VM_SSH_PORT",22222))
CORE_PORT = int(os.environ.get("CLASHTUI_VM_CORE_PORT",29090))
WEB_PORT = int(os.environ.get("CLASHTUI_VM_WEB_PORT",29091))
if len({SSH_PORT,CORE_PORT,WEB_PORT}) != 3 or not all(1024 <= p <= 65535 for p in [SSH_PORT,CORE_PORT,WEB_PORT]):
    raise RuntimeError("VM ports must be distinct unprivileged ports")


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def fetch(url, path):
    run(["curl", "-fL", "--retry", "2", "--max-time", "900", "-o", path, url])


def ssh_args():
    return ["ssh", "-i", VM / "id_ed25519", "-p", SSH_PORT,
            "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", f"UserKnownHostsFile={VM / 'known_hosts'}", "tester@127.0.0.1"]


def qmp(command, arguments=None):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(10)
        client.connect(str(VM / "qmp.sock"))
        with client.makefile("rwb") as stream:
            stream.readline()
            for name, args in [("qmp_capabilities", None), (command, arguments)]:
                message = {"execute": name}
                if args is not None:
                    message["arguments"] = args
                stream.write(json.dumps(message).encode() + b"\n")
                stream.flush()
                while True:
                    response = json.loads(stream.readline())
                    if "error" in response:
                        raise RuntimeError(response["error"])
                    if "return" in response:
                        break
            return response["return"]


def running():
    try:
        qmp("query-status")
        return True
    except (OSError, ValueError):
        return False


def stopped():
    if running():
        raise RuntimeError("Stop the VM before changing its disk")


def prepare():
    VM.mkdir(parents=True, exist_ok=True)
    VM.chmod(0o700)
    stopped()
    if not BASE.exists():
        sums = VM / "SHA512SUMS"
        if not sums.exists():
            fetch(IMAGE_URL + "SHA512SUMS", sums)
        partial = BASE.with_suffix(".qcow2.part")
        if not partial.exists():
            fetch(IMAGE_URL + BASE.name, partial)
        expected = next((line.split()[0] for line in sums.read_text().splitlines()
                         if line.split()[-1].lstrip("*./") == BASE.name), None)
        if not expected:
            raise RuntimeError("Official checksum manifest does not contain the image")
        actual = hashlib.file_digest(partial.open("rb"), "sha512").hexdigest()
        if actual != expected:
            raise RuntimeError("Image checksum mismatch; remove .part and SHA512SUMS and retry")
        partial.rename(BASE)
        BASE.chmod(0o444)
        (VM / "image.json").write_text(json.dumps({"url": IMAGE_URL + BASE.name, "sha512": actual}, indent=2))
    if not (VM / "id_ed25519").exists():
        run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", VM / "id_ed25519"])
    key = (VM / "id_ed25519.pub").read_text().strip()
    seed = VM / "seed"
    seed.mkdir(exist_ok=True)
    (seed / "meta-data").write_text("instance-id: clashtui-test-v1\nlocal-hostname: clashtui-test\n")
    (seed / "user-data").write_text(f"""#cloud-config
users:
  - name: tester
    groups: [sudo]
    shell: /bin/bash
    sudo: ['ALL=(ALL) NOPASSWD:ALL']
    lock_passwd: true
    ssh_authorized_keys:
      - {key}
disable_root: true
ssh_pwauth: false
package_update: false
package_upgrade: false
runcmd:
  - [mkdir, -p, /home/tester/clashtui-test]
  - [chown, 'tester:tester', /home/tester/clashtui-test]
  - [touch, /home/tester/clashtui-test/cloud-ready]
""")
    run(["genisoimage", "-quiet", "-output", VM / "seed.iso", "-volid", "cidata",
         "-joliet", "-rock", seed / "user-data", seed / "meta-data"])
    if not DISK.exists():
        run(["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", BASE, DISK, "12G"])
    print(f"Prepared {DISK}")


def start(internet):
    if running():
        print("VM is already running")
        return
    if not DISK.exists():
        raise RuntimeError("Run prepare first")
    accel = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"
    for port in [SSH_PORT]:
        with socket.socket() as check:
            check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            check.bind(("127.0.0.1", port))
    for name in ["qmp.sock", "qemu.pid"]:
        (VM / name).unlink(missing_ok=True)
    net = "user,id=net0,restrict=" + ("off" if internet else "on")
    for host, guest in [(SSH_PORT, 22)]:
        net += f",hostfwd=tcp:127.0.0.1:{host}-:{guest}"
    run(["qemu-system-x86_64", "-name", "clashtui-test", "-machine", "q35",
         "-accel", accel, "-cpu", "host" if accel == "kvm" else "max",
         "-m", "1536", "-smp", "2", "-display", "none", "-daemonize",
         "-pidfile", VM / "qemu.pid", "-qmp", f"unix:{VM / 'qmp.sock'},server=on,wait=off",
         "-serial", f"file:{VM / 'console.log'}", "-monitor", "none",
         "-drive", f"file={DISK},format=qcow2,if=virtio",
         "-drive", f"file={VM / 'seed.iso'},format=raw,if=virtio,readonly=on",
         "-netdev", net, "-device", "virtio-net-pci,netdev=net0", "-boot", "c"])
    (VM / "launch.json").write_text(json.dumps({"accelerator": accel, "internet": internet,
        "memory_mib": 1536, "vcpus": 2, "ports": {"ssh": SSH_PORT, "core": CORE_PORT, "web": WEB_PORT}}, indent=2))
    print(f"VM started ({accel}); run wait, then tunnel and ssh")


def wait_ready(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if not running():
            raise RuntimeError("QEMU stopped; inspect target/vm/console.log")
        try:
            result = subprocess.run([str(a) for a in ssh_args()] + ["test -e ~/clashtui-test/cloud-ready"],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
            if result.returncode == 0:
                print("Guest cloud-init and SSH are ready")
                return
        except subprocess.TimeoutExpired:
            pass
        time.sleep(3)
    raise RuntimeError("Guest readiness timed out; inspect console.log (TCG can take several minutes)")


def push(mihomo=None, panel=None):
    binary = ROOT / "target/debug/clashtui"
    if not binary.exists():
        raise RuntimeError("Build clashtui with cargo build --locked --all-features first")
    bundle = VM / "runtime"
    bundle.mkdir(exist_ok=True)
    shutil.copy2(binary, bundle / "clashtui")
    if shutil.which("strip"):
        run(["strip", bundle / "clashtui"])
    libs = subprocess.check_output(["ldd", str(binary)], text=True)
    if mihomo:
        (bundle / "mihomo").unlink(missing_ok=True)
        shutil.copy2(mihomo, bundle / "mihomo")
        libs += subprocess.check_output(["ldd", str(mihomo)], text=True)
    stores = sorted(set(re.findall(r"/nix/store/[^/\s]+", libs)))
    archive = VM / "runtime.tar.gz"
    # Nix splits runtime outputs; copy symlink targets rather than dangling links.
    with tarfile.open(archive, "w:gz", dereference=True) as tar:
        tar.add(bundle / "clashtui", arcname="opt/clashtui/bin/clashtui")
        tar.add(ROOT / "scripts/vm-guest-setup.sh", arcname="opt/clashtui/setup.sh")
        if mihomo:
            tar.add(bundle / "mihomo", arcname="opt/clashtui/bin/mihomo")
        for store in stores:
            tar.add(store, arcname=store.lstrip("/"))
    with archive.open("rb") as content:
        run(ssh_args() + ["sudo tar xzf - -C / && sudo ln -sf /opt/clashtui/bin/clashtui /usr/local/bin/clashtui"], stdin=content)
    run(ssh_args() + ["clashtui --version"])
    print("Copied the build and its Nix runtime into the guest; no host directory is shared")


def tunnel():
    control = VM / "ssh-control"
    if control.exists():
        result = subprocess.run([str(a) for a in ssh_args()] + ["-S", str(control), "-O", "check"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if result.returncode == 0:
            print("SSH tunnel is already running")
            return
        control.unlink()
    run(ssh_args() + ["-M", "-S", control, "-fN", "-o", "ExitOnForwardFailure=yes",
        "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3",
        "-L", f"127.0.0.1:{CORE_PORT}:127.0.0.1:9090",
        "-L", f"127.0.0.1:{WEB_PORT}:127.0.0.1:9091"])
    print(f"Core API: http://127.0.0.1:{CORE_PORT}/; dashboard: http://127.0.0.1:{WEB_PORT}/")


def check(allow_internet=False, profile="baseline"):
    with (ROOT / "scripts/vm-guest-smoke.py").open("rb") as script:
        result = run(ssh_args() + ["python3 -" + (" --allow-internet" if allow_internet else "") + " --profile " + shlex.quote(profile)], stdin=script, capture_output=True)
    report = json.loads(result.stdout)
    disk = json.loads(subprocess.check_output(["qemu-img", "info", "-U", "--output=json", str(DISK)], text=True))
    report["disk_backing"] = disk.get("full-backing-filename")
    report["baseline_overlay"] = report["disk_backing"] == str(VM / "baseline.qcow2")
    (VM / "acceptance-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


def stop():
    if (VM / "ssh-control").exists():
        subprocess.run([str(a) for a in ssh_args()] + ["-S", str(VM / "ssh-control"), "-O", "exit"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not running():
        print("VM is stopped")
        return
    qmp("system_powerdown")
    for _ in range(45):
        if not running():
            print("Guest shut down")
            return
        time.sleep(1)
    raise RuntimeError("Shutdown timed out; VM left running to avoid disk corruption")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("prepare")
    boot = sub.add_parser("start")
    boot.add_argument("--internet", action="store_true", help="Allow guest outbound traffic (default blocked)")
    ready = sub.add_parser("wait")
    ready.add_argument("--seconds", type=int, default=600)
    shell = sub.add_parser("ssh")
    shell.add_argument("args", nargs=argparse.REMAINDER)
    upload = sub.add_parser("push")
    upload.add_argument("--mihomo", type=Path, help="Copy this Mihomo binary into the guest")
    upload.add_argument("--panel", type=Path, help="Deprecated compatibility argument; dashboard is built in")
    verify = sub.add_parser("check")
    verify.add_argument("--allow-internet", action="store_true", help="Check manual subscription testing without requiring blocked outbound traffic")
    verify.add_argument("--profile", default="baseline", help="Expected active profile during manual testing")
    for name in ["status", "stop", "tunnel", "checkpoint", "reset"]:
        sub.add_parser(name)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare()
    elif args.command == "start":
        start(args.internet)
    elif args.command == "wait":
        wait_ready(args.seconds)
    elif args.command == "ssh":
        run(ssh_args() + (["-t"] if not args.args else []) + args.args)
    elif args.command == "push":
        push(args.mihomo, args.panel)
    elif args.command == "tunnel":
        tunnel()
    elif args.command == "check":
        check(args.allow_internet, args.profile)
    elif args.command == "stop":
        stop()
    elif args.command == "status":
        print(json.dumps({"running": running(), "directory": str(VM)}, indent=2))
    elif args.command == "checkpoint":
        stopped()
        baseline = VM / "baseline.qcow2"
        if baseline.exists():
            raise RuntimeError("Baseline already exists; kept unchanged")
        run(["qemu-img", "convert", "-O", "qcow2", DISK, baseline])
        baseline.chmod(0o444)
        print("Created independent baseline; reset replaces only the guest test disk")
    elif args.command == "reset":
        stopped()
        baseline = VM / "baseline.qcow2"
        if not baseline.exists():
            raise RuntimeError("Create a stopped-guest checkpoint first")
        fresh = VM / "reset.qcow2"
        run(["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", baseline, fresh])
        fresh.replace(DISK)


if __name__ == "__main__":
    main()
