#!/usr/bin/env python3
"""End-to-end post-boot zygote check against a Laravel app with MySQL + Redis.

Usage: zygote_e2e.py --pam PAM --app DIR --mode app|engine --workers N [--reloads K]
Prints one JSON object with measurements; exits non-zero on any isolation failure.
"""
import argparse, concurrent.futures as cf, glob, http.cookiejar, json, os, signal, subprocess, sys, time, urllib.request

p = argparse.ArgumentParser()
p.add_argument('--pam', required=True); p.add_argument('--app', required=True)
p.add_argument('--mode', default='app'); p.add_argument('--workers', type=int, default=8)
p.add_argument('--reloads', type=int, default=3); p.add_argument('--port', type=int, default=18500)
p.add_argument('--clients', type=int, default=64); p.add_argument('--max-requests', type=int, default=100000)
a = p.parse_args()
ADMIN = a.port + 99
BASE = f'http://127.0.0.1:{a.port}'
failures = []

def check(ok, message):
    if not ok:
        failures.append(message)
        print('FAIL', message, file=sys.stderr)

def get(path, opener=None, timeout=10):
    with (opener or urllib.request.build_opener()).open(BASE + path, timeout=timeout) as r:
        return json.loads(r.read())

def ready():
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{ADMIN}/ready', timeout=2) as r:
            return json.loads(r.read())
    except Exception:
        return None

def sh(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout

def pss(pid):
    try:
        for line in open(f'/proc/{pid}/smaps_rollup'):
            if line.startswith('Pss:'):
                return int(line.split()[1])
    except OSError:
        return 0
    return 0

def children(pid):
    try:
        return [int(x) for x in open(f'/proc/{pid}/task/{pid}/children').read().split()]
    except OSError:
        return []

env = dict(os.environ, ZYGOTE_PROBE_BOOT_IO='1', PAM_PREFORK_BOOT=a.mode, TMPDIR=os.environ.get('E2E_TMP', '/tmp'))
log = open(os.path.join(os.environ.get('E2E_LOG_DIR', '/tmp'), f'zygote-e2e-{a.mode}-{a.workers}.log'), 'w')
t0 = time.monotonic()
master = subprocess.Popen([a.pam, 'start', 'artisan', '--workers', str(a.workers), '--max-requests', str(a.max_requests),
                           '--startup-timeout', '60000', '--admin-address', f'127.0.0.1:{ADMIN}', '--', 'pam:octane', f'--port={a.port}'],
                          cwd=a.app, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
result = {'mode': a.mode, 'workers': a.workers}
try:
    while True:
        r = ready()
        if r and r.get('healthy') and r.get('readyWorkers') == a.workers:
            break
        if master.poll() is not None or time.monotonic() - t0 > 90:
            raise SystemExit('cluster did not start')
        time.sleep(0.005)
    result['cold_start_ms'] = round((time.monotonic() - t0) * 1000)

    def sample(n):
        seen = {}
        deadline = time.monotonic() + 30
        while len(seen) < n and time.monotonic() < deadline:
            with cf.ThreadPoolExecutor(16) as ex:
                for probe in ex.map(lambda _: get('/z/info'), range(32)):
                    seen.setdefault(probe['pid'], probe)
        return seen

    def verify_isolation(label):
        seen = sample(a.workers)
        check(len(seen) == a.workers, f'{label}: {len(seen)} of {a.workers} workers answered')
        probes = list(seen.values())
        for key in ['mysql', 'redis', 'mt', 'rand', 'uniqid', 'str', 'int', 'uuid']:
            values = {json.dumps(x[key]) for x in probes}
            check(len(values) == len(probes), f'{label}: shared {key}: {values}')
        if a.mode != 'app':
            # Pre-boot workers each boot (and own) their connections.
            for x in probes:
                check(not x['forked'] and x['bootPid'] == x['pid'], f'{label}: unexpected fork state {x}')
            return seen
        for x in probes:
            check(x['raw'] == 'failed', f'{label}: worker {x["pid"]} wrote into the boot-time raw socket')
            if a.mode == 'app':
                check(x['forked'] and x['bootPid'] != x['pid'], f'{label}: not forked from booted app {x}')
            else:
                check(not x['forked'] and x['bootPid'] == x['pid'], f'{label}: unexpected fork state {x}')
        # Server side: one MySQL/Redis connection per worker, none for the zygote.
        mysql = sh("docker exec zyg-mysql mysql -uroot -proot -N -e \"select count(*) from information_schema.processlist where user='app'\" 2>/dev/null").strip()
        check(mysql == str(a.workers), f'{label}: {mysql} MySQL connections for {a.workers} workers')
        clients = sh('docker exec zyg-redis redis-cli client list')
        check('name=zygote-raw' not in clients, f'{label}: the boot-time raw Redis socket is still connected')
        app_clients = [c for c in clients.splitlines() if 'cmd=client|list' not in c]
        check(len(app_clients) <= a.workers, f'{label}: {len(app_clients)} Redis clients for {a.workers} workers')
        return seen

    seen = verify_isolation('start')
    boot_pids = {x['bootPid'] for x in seen.values()}

    # Sessions and auth under concurrency: each client logs in as its own user.
    def client_run(user):
        jar = http.cookiejar.CookieJar()
        opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
        login = get(f'/z/login/{user}', opener)
        errors = []
        last_hits = 0
        for _ in range(20):
            me = get('/z/me', opener)
            if me['auth'] != user or me['owner'] != user or me['session'] != login['session']:
                errors.append(me)
            if me['hits'] != last_hits + 1:
                errors.append({'hits': me['hits'], 'expected': last_hits + 1})
            last_hits = me['hits']
        return user, login['session'], errors
    with cf.ThreadPoolExecutor(a.clients) as ex:
        runs = list(ex.map(client_run, range(1, a.clients + 1)))
    sessions = {s for _, s, _ in runs}
    check(len(sessions) == a.clients, f'sessions shared between clients: {len(sessions)} for {a.clients}')
    leaks = [(u, e[:2]) for u, _, e in runs if e]
    check(not leaks, f'session/auth leaked between users: {leaks[:3]}')

    # Warm every worker, then measure memory.
    with cf.ThreadPoolExecutor(32) as ex:
        list(ex.map(lambda _: get('/z/info'), range(4000)))
    workers = [w['pid'] for w in ready()['workers']]
    zygotes = [c for c in children(master.pid) if c not in workers]
    result['pss_workers_kb'] = sum(pss(w) for w in workers)
    result['pss_zygote_kb'] = sum(pss(z) for z in zygotes)
    result['pss_total_kb'] = result['pss_workers_kb'] + result['pss_zygote_kb']
    result['pss_per_worker_kb'] = round(result['pss_workers_kb'] / len(workers))

    # Respawn latency: SIGKILL one worker, time until a replacement is ready.
    respawns = []
    for _ in range(5):
        before = ready()
        victim = before['workers'][0]['pid']
        start = time.monotonic()
        os.kill(victim, signal.SIGKILL)
        while True:
            r = ready()
            if r and r['readyWorkers'] == a.workers and victim not in [w['pid'] for w in r['workers']] \
                    and all(w['state'] == 2 for w in r['workers']):
                break
            time.sleep(0.001)
        respawns.append(round((time.monotonic() - start) * 1000, 1))
        time.sleep(0.6)  # past the restart backoff window
    result['respawn_ms'] = respawns
    records = []
    for path in glob.glob(os.path.join(env['TMPDIR'], f'pam-master-{master.pid}-*', 'worker-*.json')):
        try:
            records.append(json.load(open(path)))
        except (OSError, ValueError):
            pass
    spawn = sorted(r['startedAtMillis'] - r['spawnedAtMillis'] for r in records if r.get('startedAtMillis'))
    result['spawn_to_ready_ms'] = {'n': len(spawn), 'min': spawn[0], 'p50': spawn[len(spawn)//2], 'max': spawn[-1]} if spawn else None
    result['sample_record'] = records[0] if records and os.environ.get('E2E_SHOW_RECORD') else None

    # Reloads: a fresh zygote boots from current code each time.
    reloads = []
    generation = ready()['generation']
    for _ in range(a.reloads):
        start = time.monotonic()
        os.kill(master.pid, signal.SIGHUP)
        generation += 1
        while True:
            r = ready()
            if r and r['generation'] == generation and r['healthy'] and r['readyWorkers'] == a.workers:
                break
            if time.monotonic() - start > 60:
                raise SystemExit('reload did not activate')
            time.sleep(0.002)
        reloads.append(round((time.monotonic() - start) * 1000))
    result['reload_ms'] = reloads
    time.sleep(1.5)  # retired generation drains
    seen = verify_isolation('after reloads')
    if a.mode == 'app':
        check(not ({x['bootPid'] for x in seen.values()} & boot_pids), 'reload reused the old zygote')
finally:
    os.killpg(master.pid, signal.SIGTERM)
    try:
        master.wait(15)
    except subprocess.TimeoutExpired:
        os.killpg(master.pid, signal.SIGKILL)
result['failures'] = failures
print(json.dumps(result))
sys.exit(1 if failures else 0)
