#!/usr/bin/env python3
"""Soak / chaos / leak orchestrator for the post-boot zygote (Laravel + MySQL + Redis).

soak.py --pam PAM --app DIR --mode app|engine --minutes M [--workers N] [--max-requests R]
        [--reload-every S] [--kill-every S] [--sessions-every S] [--wrk-connections C]
        [--scope-memory 3G] [--hog-every S] [--disconnect-every S] [--rate R]
Prints a JSON summary; exit 1 on any unexplained failure.
"""
import argparse, concurrent.futures as cf, glob, http.cookiejar, json, os, random, re, signal, socket, subprocess, sys, threading, time, urllib.request

p = argparse.ArgumentParser()
p.add_argument('--pam', required=True); p.add_argument('--app', required=True)
p.add_argument('--mode', default='app'); p.add_argument('--minutes', type=float, default=30)
p.add_argument('--workers', type=int, default=8); p.add_argument('--max-requests', type=int, default=2000)
p.add_argument('--reload-every', type=float, default=0); p.add_argument('--kill-every', type=float, default=0)
p.add_argument('--sessions-every', type=float, default=20); p.add_argument('--wrk-connections', type=int, default=64)
p.add_argument('--wrk-threads', type=int, default=4); p.add_argument('--scope-memory', default='')
p.add_argument('--hog-every', type=float, default=0); p.add_argument('--disconnect-every', type=float, default=0)
p.add_argument('--port', type=int, default=18600); p.add_argument('--sample-every', type=float, default=30)
p.add_argument('--label', default='soak'); p.add_argument('--path', default='')
a = p.parse_args()
S = os.path.dirname(os.path.abspath(__file__))
ADMIN = a.port + 99
BASE = f'http://127.0.0.1:{a.port}'
stop = threading.Event()
events = []          # (t, kind, detail)
failures = []
lock = threading.Lock()
T0 = time.monotonic()

def note(kind, detail=''):
    with lock:
        events.append((round(time.monotonic() - T0, 3), kind, detail))

def fail(message):
    with lock:
        failures.append((round(time.monotonic() - T0, 3), message))
    print('FAIL', message, file=sys.stderr, flush=True)

def ready():
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{ADMIN}/ready', timeout=2) as r:
            return json.loads(r.read())
    except Exception:
        return None

def get(path, opener=None, timeout=15):
    with (opener or urllib.request.build_opener()).open(BASE + path, timeout=timeout) as r:
        return json.loads(r.read())

def mem(pid):
    out = {}
    try:
        for line in open(f'/proc/{pid}/smaps_rollup'):
            k, v = line.split(':', 1)
            if k in ('Rss', 'Pss'):
                out[k] = int(v.split()[0])
    except OSError:
        pass
    return out

def children(pid):
    try:
        return [int(x) for x in open(f'/proc/{pid}/task/{pid}/children').read().split()]
    except OSError:
        return []

env = dict(os.environ, ZYGOTE_PROBE_BOOT_IO='1', PAM_PREFORK_BOOT=a.mode, TMPDIR=os.environ.get('SOAK_TMP', f'{S}/tmp'))
command = [a.pam, 'start', 'artisan', '--workers', str(a.workers), '--max-requests', str(a.max_requests),
           '--startup-timeout', '60000', '--graceful-timeout', '30000', '--admin-address', f'127.0.0.1:{ADMIN}',
           '--', 'pam:octane', f'--port={a.port}']
if a.scope_memory:
    command = ['systemd-run', '--user', '--scope', '--quiet', '-p', f'MemoryMax={a.scope_memory}', '-p', 'MemorySwapMax=0'] + command
logf = open(f'{S}/{a.label}.server.log', 'w')
master_proc = subprocess.Popen(command, cwd=a.app, env=env, stdout=logf, stderr=subprocess.STDOUT, start_new_session=True)
while not (r := ready()) or r.get('readyWorkers') != a.workers:
    if master_proc.poll() is not None or time.monotonic() - T0 > 120:
        sys.exit('cluster did not start')
    time.sleep(0.05)
# The PAM master (systemd-run --scope execs into it, so the PIDs match).
master = master_proc.pid
note('ready', r)

def master_fds():
    try:
        return len(os.listdir(f'/proc/{master}/fd'))
    except OSError:
        return -1

samples = []
def sampler():
    while not stop.wait(a.sample_every):
        r = ready() or {}
        workers = [w['pid'] for w in r.get('workers', [])]
        zygotes = [c for c in children(master) if c not in workers]
        sample = {'t': round(time.monotonic() - T0), 'generation': r.get('generation'), 'ready': r.get('readyWorkers'),
                  'workers': {w: mem(w) for w in workers}, 'zygotes': {z: mem(z) for z in zygotes},
                  'master': mem(master), 'masterFds': master_fds(), 'children': len(children(master))}
        samples.append(sample)

def reloader():
    generation = (ready() or {}).get('generation', 1)
    while not stop.wait(a.reload_every):
        start = time.monotonic()
        os.kill(master, signal.SIGHUP)
        generation += 1
        while not stop.is_set():
            r = ready()
            if r and r.get('generation') == generation and r.get('healthy') and r.get('readyWorkers') == a.workers:
                note('reload', round((time.monotonic() - start) * 1000))
                break
            if time.monotonic() - start > 60:
                fail(f'reload to generation {generation} did not activate in 60 s')
                generation = (r or {}).get('generation', generation)
                break
            time.sleep(0.01)

def killer():
    while not stop.wait(a.kill_every):
        r = ready()
        if not r or not r.get('workers'):
            continue
        victim = random.choice(r['workers'])['pid']
        try:
            os.kill(victim, signal.SIGKILL)
            note('kill', victim)
        except ProcessLookupError:
            pass

def hogger():
    while not stop.wait(a.hog_every):
        try:
            get('/z/hog?mb=6000', timeout=60)
            fail('the hog request was not OOM-killed')
        except Exception as error:
            note('oom', str(error)[:80])
        try:
            get('/z/oom', timeout=30)
        except Exception as error:
            note('memory_limit', str(error)[:80])

def disconnector():
    while not stop.wait(a.disconnect_every):
        for kind in ('mid-request', 'slow-headers', 'half-body'):
            try:
                s = socket.create_connection(('127.0.0.1', a.port), timeout=5)
                if kind == 'mid-request':
                    s.sendall(b'GET /z/slow?ms=500 HTTP/1.1\r\nHost: x\r\n\r\n'); time.sleep(0.05)
                elif kind == 'slow-headers':
                    for ch in b'GET /up HTTP/1.1\r\nHost: x\r\n':
                        s.send(bytes([ch])); time.sleep(0.01)
                else:
                    s.sendall(b'POST /up HTTP/1.1\r\nHost: x\r\nContent-Length: 100000\r\n\r\n' + b'x' * 1000)
                s.close()
                note('disconnect', kind)
            except OSError as error:
                fail(f'disconnect probe {kind} could not connect: {error}')

def session_checker():
    users = list(range(1, 101))
    while not stop.wait(a.sessions_every):
        def run(user):
            jar = http.cookiejar.CookieJar()
            opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            login = get(f'/z/login/{user}', opener)
            for hit in range(1, 11):
                me = get('/z/me', opener)
                if (me['auth'], me['owner'], me['session'], me['hits']) != (user, user, login['session'], hit):
                    return f'user {user} hit {hit} saw {me}'
            return None
        try:
            with cf.ThreadPoolExecutor(16) as ex:
                problems = [x for x in ex.map(run, random.sample(users, 16)) if x]
            for problem in problems:
                fail('session isolation: ' + problem)
            # Distinct per-process values and no shared DB/Redis connection.
            probes = {}
            with cf.ThreadPoolExecutor(16) as ex:
                for probe in ex.map(lambda _: get('/z/info'), range(64)):
                    probes.setdefault(probe['pid'], probe)
            for key in ('mysql', 'redis', 'mt', 'uniqid', 'str'):
                values = [json.dumps(v[key]) for v in probes.values()]
                if len(set(values)) != len(values):
                    fail(f'shared {key} between workers: {values}')
            note('sessions', len(probes))
        except Exception as error:
            # A request cut by a deliberate kill is expected in chaos runs.
            note('sessions-error', str(error)[:100])
            if not a.kill_every and not a.hog_every:
                fail(f'session checker request failed: {error}')

threads = [threading.Thread(target=sampler, daemon=True)]
if a.reload_every: threads.append(threading.Thread(target=reloader, daemon=True))
if a.kill_every: threads.append(threading.Thread(target=killer, daemon=True))
if a.hog_every: threads.append(threading.Thread(target=hogger, daemon=True))
if a.disconnect_every: threads.append(threading.Thread(target=disconnector, daemon=True))
if a.sessions_every: threads.append(threading.Thread(target=session_checker, daemon=True))
for t in threads: t.start()

duration = int(a.minutes * 60)
wrk_cmd = ['wrk', '-t', str(a.wrk_threads), '-c', str(a.wrk_connections), '-d', f'{duration}s', '--timeout', '30s', '--latency']
wrk_cmd += [BASE + a.path] if a.path else ['-s', f'{S}/mix.lua', BASE]
wrk = subprocess.run(wrk_cmd, capture_output=True, text=True)
stop.set()
for t in threads: t.join(timeout=70)
final = ready() or {}
summary = {'label': a.label, 'mode': a.mode, 'workers': a.workers, 'minutes': a.minutes, 'maxRequests': a.max_requests}
out = wrk.stdout
m = re.search(r'(\d+) requests in', out); summary['requests'] = int(m.group(1)) if m else None
m = re.search(r'Requests/sec:\s+([\d.]+)', out); summary['rps'] = float(m.group(1)) if m else None
m = re.search(r'Non-2xx or 3xx responses: (\d+)', out); summary['non2xx'] = int(m.group(1)) if m else 0
m = re.search(r'Socket errors: connect (\d+), read (\d+), write (\d+), timeout (\d+)', out)
summary['socketErrors'] = dict(zip(['connect', 'read', 'write', 'timeout'], map(int, m.groups()))) if m else {}
for q in ('50%', '99%'):
    m = re.search(rf'{re.escape(q)}\s+([\d.]+\w+)', out); summary['p' + q[:-1]] = m.group(1) if m else None
kinds = {}
for _, kind, detail in events:
    kinds.setdefault(kind, []).append(detail)
summary['reloads'] = len(kinds.get('reload', []))
if kinds.get('reload'):
    r = sorted(kinds['reload']); summary['reloadMs'] = {'p50': r[len(r)//2], 'max': r[-1]}
summary['kills'] = len(kinds.get('kill', [])); summary['ooms'] = len(kinds.get('oom', []))
summary['disconnects'] = len(kinds.get('disconnect', [])); summary['sessionChecks'] = len(kinds.get('sessions', []))
summary['sessionErrors'] = kinds.get('sessions-error', [])[:5]
summary['final'] = {'healthy': final.get('healthy'), 'ready': final.get('readyWorkers'), 'generation': final.get('generation')}
if samples:
    def total(s, key):
        return sum(v.get(key, 0) for v in s['workers'].values()) + sum(v.get(key, 0) for v in s['zygotes'].values())
    summary['pssMb'] = [round(total(s, 'Pss') / 1024) for s in samples]
    summary['masterRssKb'] = [s['master'].get('Rss') for s in samples]
    summary['masterFds'] = sorted({s['masterFds'] for s in samples})
    summary['children'] = sorted({s['children'] for s in samples})
summary['failures'] = failures
json.dump({'summary': summary, 'samples': samples, 'events': events, 'wrk': out}, open(f'{S}/{a.label}.json', 'w'))
os.killpg(master_proc.pid, signal.SIGTERM)
try:
    master_proc.wait(40)
except subprocess.TimeoutExpired:
    os.killpg(master_proc.pid, signal.SIGKILL); fail('master did not stop in 40 s')
leftover = subprocess.run(['pgrep', '-f', f'--port={a.port}'], capture_output=True, text=True).stdout.split()
leftover = [x for x in leftover if int(x) != os.getpid()]
if leftover:
    fail(f'leftover processes after stop: {leftover}')
summary['failures'] = failures
print(json.dumps(summary))
sys.exit(1 if failures or summary['non2xx'] or any(summary['socketErrors'].values()) else 0)
