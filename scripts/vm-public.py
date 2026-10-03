"""Real subscription and remote proxy assertions; disposable guest only.

Private input arrives over SSH stdin. Reports contain no URLs, node names,
servers, credentials or response bodies. The outer runner always resets the VM.
"""
import collections
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import time
import urllib.request
import yaml

assert socket.gethostname() == 'clashtui-test' and os.getuid() != 0
payload = json.load(sys.stdin)
ROOT = Path('/home/tester/clashtui-test/config')
CLI = ['clashtui', '--config-dir=' + str(ROOT)]
ACTIVE = ROOT / 'mihomo/config.yaml'
OVERLAY = ROOT / 'mihomo/core_override_config.yaml'
TOKEN = (ROOT / 'management-token').read_text().strip()
overlay = yaml.safe_load(OVERLAY.read_text())
SECRET = overlay['secret']
results = []
nodes = []
healthy = []
selected = None
PROBE = 'https://www.cloudflare.com/cdn-cgi/trace'


def case(identifier, function):
    started = time.monotonic()
    try:
        evidence = function()
        row = {'id': identifier, 'status': 'passed', 'evidence': evidence}
    except Exception as error:
        # Exception messages from curl, YAML and CLI can contain private input.
        row = {'id': identifier, 'status': 'failed', 'error_type': type(error).__name__}
        if isinstance(error, AssertionError): row['checkpoint'] = str(error)[:200]
    row['duration_ms'] = round((time.monotonic() - started) * 1000)
    results.append(row)
    print(json.dumps(row), flush=True)


def cli(*args, timeout=180, ok=True):
    p = subprocess.run(CLI + list(args), capture_output=True, timeout=timeout)
    assert (p.returncode == 0) == ok
    return json.loads(p.stdout) if p.returncode == 0 else None


def request(path, body=None, method=None, management=False, timeout=10):
    port, secret = (9091, TOKEN) if management else (9090, SECRET)
    req = urllib.request.Request('http://127.0.0.1:' + str(port) + path,
        data=None if body is None else json.dumps(body).encode(), method=method,
        headers={'Authorization': 'Bearer ' + secret, 'Content-Type': 'application/json'})
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req, timeout=timeout) as f:
        raw = f.read()
        return json.loads(raw) if raw else None


def wait_core():
    for _ in range(100):
        try:
            return request('/version')
        except OSError:
            time.sleep(.1)
    raise AssertionError()


def restart():
    subprocess.run(['sudo', 'systemctl', 'restart', 'clashtui-test-mihomo'], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return wait_core()


def web_action(action, **extra):
    state = request('/api/state', management=True)
    job = request('/api/action', {'action': action, 'core': state['core'],
        'revision': state['revision'], **extra}, management=True)
    for _ in range(1800):
        value = request('/api/job', management=True)
        assert value['id'] == job['job']
        if not value['pending']:
            assert value.get('error') is None
            assert value['result']['ok']
            return value['result']
        time.sleep(.1)
    raise AssertionError()


def subscription_cli():
    global nodes
    cfg = yaml.safe_load((ROOT / 'config.yaml').read_text())
    cfg['timeout'] = 30
    (ROOT / 'config.yaml').write_text(json.dumps(cfg))
    cli('manage', 'create', '--name', 'public-cli', '--url', payload['url'])
    path = ROOT / 'mihomo/profiles/public-cli.yaml'
    first = yaml.safe_load(path.read_text())
    assert isinstance(first, dict) and first.get('proxies')
    cli('manage', 'update', '--name', 'public-cli')
    second = yaml.safe_load(path.read_text())
    assert isinstance(second, dict) and second.get('proxies')
    nodes = second['proxies']
    quota = cli('manage', 'traffic', '--name', 'public-cli')
    assert cli('manage', 'state')['current'] == 'baseline'
    return {'created_and_updated': True, 'node_count': len(nodes),
            'node_types': dict(collections.Counter(n['type'] for n in nodes)),
            'quota_available': quota['subscriptions'][0]['available'], 'baseline_preserved': True}


def subscription_web():
    web_action('create', name='public-web', url=payload['url'])
    path = ROOT / 'mihomo/profiles/public-web.yaml'
    assert yaml.safe_load(path.read_text()).get('proxies')
    web_action('update', name='public-web')
    assert yaml.safe_load(path.read_text()).get('proxies')
    assert cli('manage', 'profile_url', '--name', 'public-web')['url'] == payload['url']
    return {'created_and_updated': True, 'cli_readback': True}


def node_health():
    global healthy, nodes
    assert nodes
    # Subscription rules, DNS, TUN, providers and listeners are intentionally
    # replaced; only remote node definitions enter the test core configuration.
    nodes = [{**node, 'name': 'Public Node ' + str(i)} for i, node in enumerate(nodes)]
    config = {**overlay, 'proxies': nodes,
        'proxy-groups': [{'name': 'Public Select', 'type': 'select',
                          'proxies': [n['name'] for n in nodes]}],
        'rules': ['MATCH,Public Select'], 'geo-auto-update': False}
    ACTIVE.write_text(json.dumps(config)); restart()
    def test(node):
        try:
            value = cli('core', 'delay', node['name'], '--url', PROBE, '--timeout', '8000', timeout=15)
            return node['name'], value
        except Exception:
            return node['name'], None
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        tested = list(pool.map(test, nodes))
    healthy = [name for name, value in tested if value is not None]
    assert healthy
    return {'tested': len(nodes), 'successful': len(healthy),
            'unsuccessful': len(nodes) - len(healthy), 'at_least_one_live_remote_node': True}


def proxy_https():
    global selected
    assert healthy
    successes = []
    for name in healthy[:3]:
        cli('core', 'select', 'Public Select', name)
        assert request('/proxies')['proxies']['Public Select']['now'] == name
        p = subprocess.run(['curl', '--silent', '--show-error', '--fail', '--noproxy', '',
            '--proxy', 'http://127.0.0.1:7890', '--max-time', '20', PROBE],
            capture_output=True, timeout=25)
        if p.returncode == 0 and b'ip=' in p.stdout and b'tls=' in p.stdout:
            successes.append(name)
    assert successes
    # Keep an actual TLS connection open and prove that the core connection
    # chain contains the selected remote node, rather than assuming curl's route.
    for name in successes:
        cli('core', 'select', 'Public Select', name)
        try:
            with socket.create_connection(('127.0.0.1', 7890), timeout=10) as stream:
                stream.sendall(b'CONNECT www.cloudflare.com:443 HTTP/1.1\r\nHost: www.cloudflare.com:443\r\n\r\n')
                response = b''
                while b'\r\n\r\n' not in response:
                    response += stream.recv(4096)
                    assert len(response) < 65536
                assert b'200' in response.split(b'\r\n')[0]
                with ssl.create_default_context().wrap_socket(stream, server_hostname='www.cloudflare.com') as secure:
                    secure.sendall(b'GET /cdn-cgi/trace HTTP/1.1\r\nHost: www.cloudflare.com\r\nConnection: keep-alive\r\n\r\n')
                    rows = request('/connections')['connections'] or []
                    assert any(name in row['chains'] for row in rows)
            selected = name
            break
        except (OSError, AssertionError):
            continue
    assert selected, 'No HTTPS node with a verified live core chain'
    return {'https_successful_nodes': len(successes), 'tls_certificate_verified': True,
            'selected_node_in_real_connection_chain': True}


def reject_and_reconnect():
    assert selected
    config = yaml.safe_load(ACTIVE.read_text())
    config['proxy-groups'][0]['proxies'].append('REJECT')
    ACTIVE.write_text(json.dumps(config)); restart()
    cli('core', 'select', 'Public Select', 'REJECT')
    args = ['curl', '--silent', '--fail', '--noproxy', '', '--proxy', 'http://127.0.0.1:7890',
            '--max-time', '15', PROBE]
    assert subprocess.run(args, capture_output=True, timeout=20).returncode != 0
    cli('core', 'select', 'Public Select', selected)
    assert subprocess.run(args, capture_output=True, timeout=20).returncode == 0
    restart()
    cli('core', 'select', 'Public Select', selected)
    assert subprocess.run(args, capture_output=True, timeout=20).returncode == 0
    return {'reject_failed': True, 'selection_recovered': True, 'service_restart_recovered': True}


def public_dns():
    assert selected
    config = yaml.safe_load(ACTIVE.read_text())
    config['dns'] = {'enable': True, 'listen': '127.0.0.1:1053', 'ipv6': False,
                     'nameserver': ['https://1.1.1.1/dns-query'], 'enhanced-mode': 'redir-host'}
    ACTIVE.write_text(json.dumps(config)); restart()
    # Real DNS wire query to the guest core, which uses a public DoH upstream.
    import struct
    query = struct.pack('!HHHHHH', 0x7341, 0x0100, 1, 0, 0, 0)
    query += b'\x03www\x0acloudflare\x03com\0' + struct.pack('!HH', 1, 1)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.settimeout(20); s.sendto(query, ('127.0.0.1', 1053)); response, _ = s.recvfrom(65535)
    fields = struct.unpack('!HHHHHH', response[:12])
    assert fields[0] == 0x7341 and fields[1] & 15 == 0 and fields[3] > 0
    return {'real_udp_dns_query': True, 'public_doh_upstream_answer': True}


original_rules = None
stun_address = None


def public_tun_tcp():
    global original_rules, stun_address
    assert selected
    import struct
    query = struct.pack('!HHHHHH', 0x7343, 0x0100, 1, 0, 0, 0)
    query += b'\x04stun\x0acloudflare\x03com\0' + struct.pack('!HH', 1, 1)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as dns:
        dns.settimeout(10); dns.sendto(query, ('127.0.0.1', 1053)); answer, _ = dns.recvfrom(65535)
    def skip_name(position):
        while answer[position]:
            if answer[position] & 0xc0 == 0xc0: return position + 2
            position += answer[position] + 1
        return position + 1
    position = skip_name(12) + 4
    for _ in range(struct.unpack('!HHHHHH', answer[:12])[3]):
        position = skip_name(position)
        kind, _, _, length = struct.unpack('!HHIH', answer[position:position + 10]); position += 10
        if kind == 1 and length == 4: stun_address = socket.inet_ntoa(answer[position:position + 4]); break
        position += length
    assert stun_address, 'Public DoH did not resolve UDP probe target'
    original_rules = json.loads(subprocess.check_output(['ip', '-j', 'rule']))
    dropin = '/etc/systemd/system/clashtui-test-mihomo.service.d/public.conf'
    subprocess.run(['sudo', 'mkdir', '-p', str(Path(dropin).parent)], check=True)
    subprocess.run(['sudo', 'tee', dropin], input=b'[Service]\nAmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE\n',
                   stdout=subprocess.DEVNULL, check=True)
    subprocess.run(['sudo', 'systemctl', 'daemon-reload'], check=True)
    config = yaml.safe_load(ACTIVE.read_text())
    config['tun'] = {'enable': True, 'device': 'ct-public', 'stack': 'gvisor',
        'auto-route': True, 'auto-detect-interface': True, 'dns-hijack': [],
        'inet4-address': ['10.89.0.1/30']}
    ACTIVE.write_text(json.dumps(config)); restart()
    cli('core', 'select', 'Public Select', selected)
    assert any(x['ifname'] == 'ct-public' for x in json.loads(subprocess.check_output(['ip', '-j', 'link'])))
    assert len(json.loads(subprocess.check_output(['ip', '-j', 'rule']))) > len(original_rules)
    # No explicit HTTP/SOCKS proxy: the guest TUN captures this connection.
    p = subprocess.run(['curl', '--silent', '--show-error', '--fail', '--noproxy', '*',
                        '--max-time', '25', 'https://1.1.1.1/cdn-cgi/trace'],
                       capture_output=True, timeout=30)
    assert p.returncode == 0 and b'ip=' in p.stdout, 'TUN HTTPS request failed'
    with socket.create_connection(('1.1.1.1', 443), timeout=20) as stream:
        with ssl.create_default_context().wrap_socket(stream, server_hostname='one.one.one.one'):
            rows = request('/connections')['connections'] or []
            assert any(selected in row['chains'] and row['metadata'].get('type', '').upper() == 'TUN'
                       for row in rows), 'TUN TLS connection chain missing'
    return {'guest_tun_and_routes': True, 'https_without_explicit_proxy': True,
            'remote_node_in_tun_connection_chain': True, 'tls_certificate_verified': True}


def public_tun_udp_and_cleanup():
    assert selected and original_rules is not None
    import struct
    query = struct.pack('!HHHHHH', 0x7342, 0x0100, 1, 0, 0, 0)
    query += b'\x03www\x0acloudflare\x03com\0' + struct.pack('!HH', 1, 1)
    try:
        received = False
        # UDP/53 can be filtered independently of UDP transport. A STUN binding
        # reply on Cloudflare's documented public UDP/3478 endpoint provides
        # another real UDP response with an unpredictable transaction ID.
        transaction = os.urandom(12)
        stun = struct.pack('!HHI', 1, 0, 0x2112A442) + transaction
        attempts = []
        for name in [selected] + [x for x in healthy[:5] if x != selected]:
            cli('core', 'select', 'Public Select', name)
            for resolver, port, message in [(stun_address, 3478, stun), ('1.1.1.1', 53, query), ('8.8.8.8', 53, query)]:
                try:
                    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
                        s.settimeout(5); s.sendto(message, (resolver, port)); response, _ = s.recvfrom(65535)
                        if port == 3478:
                            assert struct.unpack('!HHI', response[:8])[0] == 0x101 and response[8:20] == transaction
                        else:
                            fields = struct.unpack('!HHHHHH', response[:12])
                            assert fields[0] == 0x7342 and fields[1] & 15 == 0 and fields[3] > 0
                        attempts.append({'port': port, 'response_received': True})
                        assert any(name in row['chains'] and row['metadata'].get('network') == 'udp'
                                   and row['metadata'].get('type', '').upper() == 'TUN'
                                   for row in (request('/connections')['connections'] or []))
                        received = True
                        break
                except (OSError, AssertionError) as error:
                    attempts.append({'port': port, 'failed': type(error).__name__})
                    continue
            if received: break
        assert received, 'No verified public UDP response: ' + json.dumps(attempts)
    finally:
        subprocess.run(['sudo', 'systemctl', 'stop', 'clashtui-test-mihomo'], check=True)
        assert all(x['ifname'] != 'ct-public' for x in json.loads(subprocess.check_output(['ip', '-j', 'link'])))
        assert json.loads(subprocess.check_output(['ip', '-j', 'rule'])) == original_rules
        config = yaml.safe_load(ACTIVE.read_text()); config['tun'] = {'enable': False}
        ACTIVE.write_text(json.dumps(config)); restart()
    assert subprocess.run(['curl', '--silent', '--fail', '--noproxy', '*', '--max-time', '20',
        'https://1.1.1.1/cdn-cgi/trace'], capture_output=True, timeout=25).returncode == 0
    return {'public_udp_reply_via_remote_node': True, 'udp_probe': 'STUN' if port == 3478 else 'DNS', 'tun_removed_on_stop': True,
            'policy_routes_restored': True, 'direct_guest_access_after_disable': True}


for identifier, function in [('P01', subscription_cli), ('P02', subscription_web),
        ('P03', node_health), ('P04', proxy_https), ('P05', reject_and_reconnect), ('P06', public_dns),
        ('P07', public_tun_tcp), ('P08', public_tun_udp_and_cleanup)]:
    case(identifier, function)
report = {'scope': 'real public subscription, remote nodes, TLS proxy and public DNS in disposable VM',
          'cases': results, 'passed': sum(r['status'] == 'passed' for r in results),
          'failed': sum(r['status'] == 'failed' for r in results)}
(ROOT.parent / 'public-report.json').write_text(json.dumps(report, indent=2))
sys.exit(bool(report['failed']))
