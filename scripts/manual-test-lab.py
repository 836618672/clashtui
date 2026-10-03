"""Offline manual-test fixtures and traffic. Run only as tester in the test VM."""
import argparse
import base64
import http.server
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import threading
import time
from urllib.parse import urlsplit

LAB = Path('/home/tester/clashtui-test/manual')
URL = 'http://127.0.0.1:18080'


def profile(version=1):
    names = [f'Manual A v{version}', f'Manual B v{version}']
    return {
        'proxies': [{'name': name, 'type': 'direct'} for name in names],
        'proxy-providers': {}, 'rule-providers': {},
        'proxy-groups': [
            {'name': 'Manual Select', 'type': 'select', 'proxies': names + ['DIRECT', 'REJECT']},
            {'name': 'Manual Auto', 'type': 'url-test', 'proxies': names,
             'url': URL + '/probe', 'interval': 60},
        ],
        'rules': ['MATCH,Manual Select'],
    }


def write(name, value):
    (LAB / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def prepare():
    LAB.mkdir(parents=True, exist_ok=True)
    LAB.chmod(0o700)
    if not (LAB / 'version').exists():
        (LAB / 'version').write_text('1\n')
    write('basic.yaml', profile())
    empty = profile()
    empty['clashtui'] = {'proxy_provider_groups': {}}
    write('empty-template.yaml', empty)
    for fmt in ['yaml', 'text', 'mrs']:
        value = profile()
        value['rule-providers'] = {'Manual Rules': {
            'type': 'http', 'url': URL + '/rules.' + fmt,
            'path': 'rules/manual.' + fmt, 'behavior': 'domain', 'format': fmt, 'interval': 3600,
        }}
        value['rules'].insert(0, 'RULE-SET,Manual Rules,DIRECT')
        if fmt == 'yaml':
            value['proxy-providers'] = {'Manual HTTP': {
                'type': 'http', 'url': URL + '/proxy.yaml', 'path': 'proxies/manual.yaml',
                'interval': 3600, 'health-check': {'enable': True, 'url': URL + '/probe', 'interval': 60},
            }}
            value['proxy-groups'][0]['use'] = ['Manual HTTP']
        write('providers-' + fmt + '.yaml', value)
    write('template.yaml', {
        'proxies': [], 'rule-providers': {},
        'proxy-providers': {'manual': {'tpl_param': None, 'type': 'http', 'interval': 3600}},
        'proxy-groups': [{'name': 'Manual Template Select', 'type': 'select',
                          'use': ['${PPG.manual}'], 'proxies': ['DIRECT', 'REJECT']}],
        'rules': ['MATCH,Manual Template Select'],
        'clashtui': {'proxy_provider_groups': {'manual': {'Manual HTTP': URL + '/proxy.yaml'}}},
    })
    invalid = profile()
    invalid['rules'] = ['THIS-IS-NOT-A-RULE']
    write('invalid-core.yaml', invalid)
    for name, path in [('escape-parent.yaml', '../manual-sentinel.yaml'),
                       ('escape-absolute.yaml', '/home/tester/clashtui-test/config/manual-sentinel.yaml'),
                       ('escape-symlink.yaml', 'manual-link/manual-sentinel.yaml')]:
        value = profile()
        value['rule-providers'] = {'Escape': {'type': 'http', 'url': URL + '/rules.yaml',
                                            'path': path, 'behavior': 'domain'}}
        write(name, value)
    (LAB / 'rules.txt').write_text('example.test\n+.example.org\n')
    subprocess.run(['/opt/clashtui/bin/mihomo', 'convert-ruleset', 'domain', 'text',
                    str(LAB / 'rules.txt'), str(LAB / 'rules.mrs')], check=True)
    print('Fixtures ready at ' + str(LAB), flush=True)
    print('Start the loopback server with: python3 ' + str(LAB / 'lab.py') + ' serve', flush=True)


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # Log method/path/status, never credentials or request headers.
        print(format % args, flush=True)

    def do_HEAD(self):
        self.respond(False)

    def do_GET(self):
        self.respond(True)

    def respond(self, body):
        path = urlsplit(self.path).path
        if path == '/redirect':
            self.send_response(302)
            self.send_header('Location', URL + '/subscription.yaml')
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        if path == '/auth.yaml':
            expected = 'Basic ' + base64.b64encode(b'manual:manual-pass').decode()
            if self.headers.get('Authorization') != expected:
                self.send_error(401)
                return
        if path == '/unauthorized' or path == '/fail':
            self.send_error(401 if path == '/unauthorized' else 503)
            return
        if path == '/proxy.yaml' and (LAB / 'fail-proxy').exists():
            self.send_error(503)
            return
        if path == '/slow.yaml' and body:
            time.sleep(10)
        if path == '/stream':
            chunks = 4 * 600
            self.send_response(200)
            self.send_header('Content-Length', str(65536 * chunks))
            self.end_headers()
            if body:
                try:
                    for _ in range(chunks):
                        self.wfile.write(b'x' * 65536)
                        self.wfile.flush()
                        time.sleep(.25)
                except (BrokenPipeError, ConnectionResetError):
                    pass
            return
        version = int((LAB / 'version').read_text().strip())
        status = 200
        if path == '/probe':
            time.sleep(.02)  # A measurable real RTT for deterministic health assertions.
            payload, status = b'', 204
        elif path in ['/subscription.yaml', '/auth.yaml', '/slow.yaml']:
            payload = json.dumps(profile(version)).encode()
        elif path == '/proxy.yaml':
            payload = json.dumps({'proxies': [{'name': f'Manual Provider v{version}', 'type': 'direct'}]}).encode()
        elif path == '/rules.yaml':
            payload = b'payload: [example.test, +.example.org]\n'
        elif path == '/rules.text':
            payload = (LAB / 'rules.txt').read_bytes()
        elif path == '/rules.mrs':
            payload = (LAB / 'rules.mrs').read_bytes()
        elif path == '/bad.mrs':
            payload = b'MRS\x01corrupted'
        elif path == '/malformed.yaml':
            payload = b'[unterminated'
        elif path == '/hello':
            payload = b'manual-lab-ok\n'
        else:
            self.send_error(404)
            return
        self.send_response(status)
        self.send_header('Content-Length', str(len(payload)))
        self.send_header('Subscription-Userinfo', 'upload=10; download=20; total=100; expire=2000000000')
        self.end_headers()
        if body:
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass


def traffic(seconds):
    stop = threading.Event()
    sockets = []

    def consume(host):
        try:
            stream = socket.create_connection(('127.0.0.1', 7890), timeout=5)
            sockets.append(stream)
            stream.settimeout(1)
            stream.sendall((f'GET http://{host}:18080/stream HTTP/1.1\r\n'
                            f'Host: {host}:18080\r\nConnection: close\r\n\r\n').encode())
            while not stop.is_set():
                try:
                    if not stream.recv(65536):
                        break
                except socket.timeout:
                    continue
        except OSError as error:
            print(host + ': ' + str(error), flush=True)
        finally:
            print(host + ': connection ended', flush=True)

    threads = [threading.Thread(target=consume, args=(host,), daemon=True)
               for host in ['localhost', '127.0.0.1']]
    for thread in threads:
        thread.start()
    print('Two streams use only the guest proxy port 7890; Ctrl-C ends traffic.', flush=True)
    try:
        stop.wait(seconds)
    finally:
        stop.set()
        for stream in sockets:
            stream.close()
        for thread in threads:
            thread.join(timeout=2)


def dns_server():
    upstream = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    upstream.bind(('127.0.0.1', 18053))

    def answer():
        while True:
            try:
                packet, address = upstream.recvfrom(4096)
                if len(packet) < 12:
                    continue
                response = packet[:2] + struct.pack('!HHHHH', 0x8180, 1, 1, 0, 0) + packet[12:]
                response += b'\xc0\x0c' + struct.pack('!HHIH', 1, 1, 60, 4)
                response += socket.inet_aton('203.0.113.10')
                upstream.sendto(response, address)
            except OSError:
                return

    threading.Thread(target=answer, daemon=True).start()
    return upstream


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'serve', 'traffic'])
    parser.add_argument('--seconds', type=int, default=120)
    args = parser.parse_args()
    if socket.gethostname() != 'clashtui-test' or os.getuid() == 0 or Path.home() != Path('/home/tester'):
        parser.error('Guest only: run as tester inside clashtui-test')
    if not 1 <= args.seconds <= 600:
        parser.error('--seconds must be 1..600')
    if args.action == 'prepare':
        prepare()
    elif args.action == 'serve':
        if not (LAB / 'rules.mrs').is_file():
            parser.error('Run prepare first')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 18080), Handler)
        server.daemon_threads = True
        upstream = dns_server()
        print('Offline HTTP: ' + URL + '; DNS upstream: 127.0.0.1:18053', flush=True)
        try:
            server.serve_forever()
        finally:
            server.server_close()
            upstream.close()
    else:
        traffic(args.seconds)


if __name__ == '__main__':
    main()
