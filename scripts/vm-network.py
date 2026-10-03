"""Guest-only real traffic, DNS and TUN tests, with guaranteed config restoration."""
import http.server
import json
from pathlib import Path
import socket
import struct
import subprocess
import threading
import time
import urllib.request
import yaml

assert socket.gethostname() == 'clashtui-test', 'Guest only'
ROOT = Path('/home/tester/clashtui-test/config')
CLI = ['clashtui', '--config-dir=' + str(ROOT)]
ACTIVE = ROOT / 'mihomo/config.yaml'
original = ACTIVE.read_bytes()
config = yaml.safe_load(original)
results = []
original_rules=json.loads(subprocess.check_output(['ip','-j','rule'],text=True))


def core(path):
    req = urllib.request.Request('http://127.0.0.1:9090' + path, headers={'Authorization':'Bearer '+config['secret']})
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.load(response)


def restart():
    subprocess.run(['sudo','systemctl','restart','clashtui-test-mihomo'],check=True)
    for _ in range(40):
        try:
            core('/configs')
            return
        except OSError:
            time.sleep(.1)
    raise AssertionError('Core restart failed')


def case(name, work):
    try:
        work()
        results.append({'name':name,'status':'passed'})
    except Exception as error:
        results.append({'name':name,'status':'failed','error':str(error).replace(config['secret'],'[redacted]')[:1500]})
    print(json.dumps(results[-1]),flush=True)


class StreamHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Length','10485760')
        self.end_headers()
        try:
            for _ in range(640):
                self.wfile.write(b'x'*16384)
                self.wfile.flush()
                time.sleep(.025)
        except (BrokenPipeError,ConnectionResetError):
            pass


server = http.server.ThreadingHTTPServer(('127.0.0.1',0),StreamHandler)
threading.Thread(target=server.serve_forever,daemon=True).start()
sockets=[]
readers=[]


def connections():
    for host in ['localhost','127.0.0.1']:
        stream=socket.create_connection(('127.0.0.1',7890))
        sockets.append(stream)
        stream.sendall(f'GET http://{host}:{server.server_port}/stream HTTP/1.1\r\nHost: {host}:{server.server_port}\r\n\r\n'.encode())
        def consume(stream=stream):
            try:
                while stream.recv(65536):
                    pass
            except OSError:
                pass
        reader=threading.Thread(target=consume,daemon=True)
        readers.append(reader)
        reader.start()
    time.sleep(.3)
    rows=core('/connections')['connections']
    assert len(rows)>=2, 'Expected two real proxy connections'
    hosts={row['metadata']['host'] for row in rows}
    assert 'localhost' in hosts, str(hosts)
    captured={row['id'] for row in rows if row['metadata']['host']=='localhost'}
    remaining={row['id'] for row in rows if row['id'] not in captured}
    value=subprocess.check_output(CLI+['core','connections','--filter','localhost','--close','--yes'],text=True)
    assert set(json.loads(value)['captured_ids'])==captured, 'Close scope mismatch'
    current={row['id'] for row in core('/connections')['connections']}
    assert remaining<=current and not(captured&current), 'Unmatched connections were closed'


def metrics():
    value=json.loads(subprocess.check_output(CLI+['core','metrics','--samples','3','--interval-ms','200'],text=True))
    assert value['memory'] is not None
    assert any(row['download_bytes_per_second']>0 for row in value['samples']), 'Real traffic did not produce positive rates'


def rollback():
    overlay=ROOT/'mihomo/core_override_config.yaml'
    previous_overlay=overlay.read_bytes()
    previous_active=ACTIVE.read_bytes()
    with socket.socket() as blocked:
        blocked.bind(('127.0.0.1',0))
        blocked.listen()
        changed=yaml.safe_load(previous_overlay)
        changed['mixed-port']=blocked.getsockname()[1]
        try:
            overlay.write_text(json.dumps(changed))
            result=subprocess.run(CLI+['manage','activate','--name','baseline','--yes'],capture_output=True,text=True,timeout=30)
            assert result.returncode!=0,'Busy listener activation unexpectedly succeeded'
            assert ACTIVE.read_bytes()==previous_active,'Failed reload did not restore original bytes'
            assert core('/configs')['mixed-port']==config['mixed-port'],'Running listener was not restored'
        finally:
            overlay.write_bytes(previous_overlay)
            if ACTIVE.read_bytes()!=previous_active:
                ACTIVE.write_bytes(previous_active)
                restart()


dns=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
dns.bind(('127.0.0.1',0))
dns.settimeout(.5)
dns_stop=False
def answer_dns():
    while not dns_stop:
        try:
            data,address=dns.recvfrom(4096)
        except socket.timeout:
            continue
        except OSError:
            return
        # One A question; preserve its bytes and answer with a documentation IP.
        response=data[:2]+struct.pack('!HHHHH',0x8180,1,1,0,0)+data[12:]+b'\xc0\x0c'+struct.pack('!HHIH',1,1,60,4)+socket.inet_aton('203.0.113.10')
        dns.sendto(response,address)
threading.Thread(target=answer_dns,daemon=True).start()


def dns_test():
    value=yaml.safe_load(original)
    value['dns']={'enable':True,'listen':'127.0.0.1:15353','enhanced-mode':'redir-host',
        'nameserver':[f'udp://127.0.0.1:{dns.getsockname()[1]}'],'default-nameserver':['127.0.0.1']}
    ACTIVE.write_text(json.dumps(value))
    restart()
    query=struct.pack('!HHHHHH',12345,0x0100,1,0,0,0)+b'\x07example\x04test\x00'+struct.pack('!HH',1,1)
    with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as client:
        client.settimeout(5)
        client.sendto(query,('127.0.0.1',15353))
        response,_=client.recvfrom(4096)
    assert response[:2]==query[:2] and socket.inet_aton('203.0.113.10') in response, 'DNS answer mismatch'


override=Path('/etc/systemd/system/clashtui-test-mihomo.service.d/acceptance.conf')
def tun_test():
    subprocess.run(['sudo','mkdir','-p',str(override.parent)],check=True)
    subprocess.run(['sudo','tee',str(override)],input='[Service]\nAmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE\n',text=True,stdout=subprocess.DEVNULL,check=True)
    subprocess.run(['sudo','systemctl','daemon-reload'],check=True)
    value=yaml.safe_load(original)
    value['tun']={'enable':True,'device':'ct-test','stack':'gvisor','auto-route':False,
        'auto-detect-interface':False,'inet4-address':['10.88.0.1/30']}
    ACTIVE.write_text(json.dumps(value))
    restart()
    assert core('/configs')['tun']['enable'] is True, 'TUN not enabled'
    links=json.loads(subprocess.check_output(['ip','-j','link'],text=True))
    assert any(link['ifname']=='ct-test' for link in links), 'Actual TUN interface missing'


def tun_routes():
    value=yaml.safe_load(original)
    value['tun']={'enable':True,'device':'ct-test','stack':'gvisor','auto-route':True,
        'auto-detect-interface':True,'inet4-address':['10.88.0.1/30']}
    ACTIVE.write_text(json.dumps(value))
    restart()
    assert core('/configs')['tun']['enable'] is True,'TUN auto-route startup failed'
    actual=json.loads(subprocess.check_output(['ip','-j','rule'],text=True))
    assert len(actual)>len(original_rules),'No guest policy routing rules were installed'


def tun_stop_cleanup():
    subprocess.run(['sudo','systemctl','stop','clashtui-test-mihomo'],check=True)
    links=json.loads(subprocess.check_output(['ip','-j','link'],text=True))
    assert all(link['ifname']!='ct-test' for link in links), 'TUN remained after service stop'
    assert json.loads(subprocess.check_output(['ip','-j','rule'],text=True))==original_rules, 'Policy rules remained after service stop'


def tun_permission_failure():
    subprocess.run(['sudo','tee',str(override)],input='[Service]\nAmbientCapabilities=\nCapabilityBoundingSet=~CAP_NET_ADMIN\n',text=True,stdout=subprocess.DEVNULL,check=True)
    subprocess.run(['sudo','systemctl','daemon-reload'],check=True)
    since=str(int(time.time()))
    subprocess.run(['sudo','systemctl','start','clashtui-test-mihomo'],check=True)
    time.sleep(2)
    links=json.loads(subprocess.check_output(['ip','-j','link'],text=True))
    assert all(link['ifname']!='ct-test' for link in links), 'TUN created without NET_ADMIN'
    assert json.loads(subprocess.check_output(['ip','-j','rule'],text=True))==original_rules
    log=subprocess.check_output(['journalctl','-u','clashtui-test-mihomo','--since','@'+since,'--no-pager'],text=True).lower()
    assert 'operation not permitted' in log or 'permission denied' in log, 'No permission failure diagnostic'


try:
    case('Real proxy connections and filtered close',connections)
    case('Metrics under real proxy traffic',metrics)
    for stream in sockets:
        stream.close()
    case('Activation rollback after real listener bind failure',rollback)
    case('Guest DNS query and local upstream',dns_test)
    case('Guest TUN creation with auto-route disabled',tun_test)
    case('Guest TUN automatic policy routing',tun_routes)
    case('Guest TUN and policy routes removed on service stop',tun_stop_cleanup)
    case('Guest TUN permission denial leaves no interface or policy rules',tun_permission_failure)
finally:
    ACTIVE.write_bytes(original)
    subprocess.run(['sudo','rm','-f',str(override)],check=True)
    subprocess.run(['sudo','systemctl','daemon-reload'],check=True)
    restart()
    dns_stop=True
    dns.close()
    server.shutdown()
    for stream in sockets:
        stream.close()
    runtime=core('/configs')
    assert runtime['tun']['enable'] is False, 'TUN restoration failed'
    links=json.loads(subprocess.check_output(['ip','-j','link'],text=True))
    assert all(link['ifname']!='ct-test' for link in links), 'TUN interface remained after restore'
    assert json.loads(subprocess.check_output(['ip','-j','rule'],text=True))==original_rules,'Guest policy routes were not restored'
    results.append({'name':'Restore original core configuration and remove test TUN','status':'passed'})
print('SUMMARY '+json.dumps({'passed':sum(row['status']=='passed' for row in results),
    'failed':sum(row['status']=='failed' for row in results),'cases':results}),flush=True)
