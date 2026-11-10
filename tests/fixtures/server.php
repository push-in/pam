<?php

declare(strict_types=1);

require_once __DIR__ . '/../../compat/composer-smoke/vendor/autoload.php';

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Socket\Server as SocketServer;
use Pam\WS\Socket;
use Pam\WS\Acknowledgement;

$failedGeneration = (int) (getenv('PAM_TEST_FAIL_GENERATION') ?: 0);
$workerGeneration = (int) (getenv('PAM_WORKER_GENERATION') ?: 1);
if ($failedGeneration > 0 && $workerGeneration === $failedGeneration) {
    throw new RuntimeException("Intentional startup failure for generation {$workerGeneration}");
}

if (getenv('PAM_TEST_STRICT_ERRORS') === '1') {
    // Like Laravel's HandleExceptions: every warning fails the request.
    set_error_handler(static function (int $level, string $message, string $file = '', int $line = 0): bool {
        if ((error_reporting() & $level) === 0) {
            return false;
        }
        throw new ErrorException($message, 0, $level, $file, $line);
    });
}

$app = new App();
$app->get('/ping', static fn (Request $request, Response $response) => $response->json([
    'message' => 'pong',
    'query' => $request->getQuery('query'),
]));
$app->get('/managed-env', static fn (Request $request, Response $response) => $response->json([
    'value' => getenv('PAM_TEST_MANAGED_ENV') ?: null,
]));
$app->get('/disconnect-cancel', static function (Request $request, Response $response): Response {
    \Pam\Async\delay(60.0);
    return $response->send('should-have-been-cancelled');
});
$app->get('/api/pool', static fn (Request $request, Response $response) => $response->json([
    'pool' => getenv('PAM_WORKER_POOL') ?: null,
]));
$app->get('/api/admin/pool', static fn (Request $request, Response $response) => $response->json([
    'pool' => getenv('PAM_WORKER_POOL') ?: null,
]));
$app->get('/pool', static fn (Request $request, Response $response) => $response->json([
    'pool' => getenv('PAM_WORKER_POOL') ?: null,
]));
$app->get('/cached', static function (Request $request, Response $response): Response {
    $GLOBALS['pam_cache_fixture_calls'] = (int) ($GLOBALS['pam_cache_fixture_calls'] ?? 0) + 1;
    return $response
        ->header('Cache-Control', 'public, max-age=30')
        ->header('X-Pam-Cache-Tags', 'catalog public')
        ->json(['calls' => $GLOBALS['pam_cache_fixture_calls']]);
});
$app->get('/cached-slow', static function (Request $request, Response $response): Response {
    $GLOBALS['pam_slow_cache_fixture_calls'] = (int) ($GLOBALS['pam_slow_cache_fixture_calls'] ?? 0) + 1;
    if ($GLOBALS['pam_slow_cache_fixture_calls'] > 1) {
        \Pam\Async\delay(0.1);
    }
    return $response
        ->header('Cache-Control', 'public, max-age=30')
        ->json(['calls' => $GLOBALS['pam_slow_cache_fixture_calls']]);
});
$redisPort = getenv('PAM_TEST_REDIS_PORT');
if (is_string($redisPort) && ctype_digit($redisPort)) {
    $app->get('/redis', static function (Request $request, Response $response) use ($redisPort): Response {
        $redis = new \Pam\Redis\Client(port: (int) $redisPort, timeout: 2.0);
        return $response->json([
            'responses' => $redis->pipeline([
                ['PING'],
                ['GET', 'fixture-key'],
            ]),
        ]);
    });
}
$httpUpstreamPort = getenv('PAM_TEST_HTTP_UPSTREAM_PORT');
if (is_string($httpUpstreamPort) && ctype_digit($httpUpstreamPort)) {
    $app->get('/http-client-slow', static function (Request $request, Response $response) use ($httpUpstreamPort): Response {
        $upstream = (new \Pam\Http\Client(timeout: 2.0))->get(
            "http://127.0.0.1:{$httpUpstreamPort}/upstream",
        );
        return $response->json(['body' => $upstream->body]);
    });
}
$databasePath = getenv('PAM_TEST_ISOLATED_DATABASE');
if (is_string($databasePath) && $databasePath !== '') {
    $app->get('/isolated-database', static function (Request $request, Response $response) use ($databasePath): Response {
        $database = new \Pam\Database\IsolatedPdoPool(
            'sqlite:' . $databasePath,
            maxWorkers: 2,
            timeout: 3.0,
        );
        $result = $database->query(
            'WITH RECURSIVE values_table(value) AS ('
            . ' SELECT 1 UNION ALL SELECT value + 1 FROM values_table WHERE value < 300000'
            . ') SELECT sum(value) AS total FROM values_table',
        );
        return $response->json(['total' => $result->rows[0]['total'] ?? null]);
    });
}
$app->post('/echo', static fn (Request $request, Response $response) => $response->json([
    'body' => $request->json(),
    'testHeader' => $request->getHeader('x-pam-test'),
]));
$app->post('/context', static function (Request $request, Response $response): Response {
    header('X-Native-Header: captured');
    setcookie('pam', 'compatible', ['path' => '/', 'httponly' => true]);
    http_response_code(202);

    return $response->json([
        'cookie' => $_COOKIE['client'] ?? null,
        'form' => $_POST,
        'method' => $_SERVER['REQUEST_METHOD'] ?? null,
        'raw' => file_get_contents('php://input'),
        'requestId' => $_SERVER['PAM_REQUEST_ID'] ?? null,
    ]);
});
$app->post('/upload', static fn (Request $request, Response $response): Response => $response->json([
    'field' => $_POST['description'] ?? null,
    'filename' => $_FILES['document']['name'] ?? null,
    'contents' => isset($_FILES['document']['tmp_name'])
        ? file_get_contents($_FILES['document']['tmp_name'])
        : null,
]));
$app->post('/upload-stream', static function (Request $request, Response $response): Response {
    $file = $_FILES['document'] ?? [];
    $temporary = is_string($file['tmp_name'] ?? null) ? $file['tmp_name'] : '';
    $result = [
        'size' => $file['size'] ?? null,
        'error' => $file['error'] ?? null,
        'tmp' => $temporary,
        'uploaded' => $temporary !== '' && is_uploaded_file($temporary),
        'input' => strlen((string) file_get_contents('php://input')),
        'sha256' => $temporary !== '' ? hash_file('sha256', $temporary) : null,
        'moved' => false,
    ];
    if (($_POST['move'] ?? '') === '1' && $result['uploaded']) {
        $target = sys_get_temp_dir() . '/pam-moved-' . bin2hex(random_bytes(6));
        $result['moved'] = move_uploaded_file($temporary, $target);
        $result['sha256'] = hash_file('sha256', $target);
        $result['uploadedAfterMove'] = is_uploaded_file($temporary);
        unlink($target);
    }
    return $response->json($result);
});
$app->post('/upload-nested', static fn (Request $request, Response $response): Response => $response->json([
    'post' => $_POST,
    'names' => $_FILES['photos']['name'] ?? null,
    'sizes' => $_FILES['photos']['size'] ?? null,
    'contents' => array_map(
        static fn (string $path): string => (string) file_get_contents($path),
        $_FILES['photos']['tmp_name'] ?? [],
    ),
    'avatar' => $_FILES['profile']['name']['avatar'] ?? null,
]));
$app->get('/session', static function (Request $request, Response $response): Response {
    session_start();
    $_SESSION['visits'] = (int) ($_SESSION['visits'] ?? 0) + 1;
    return $response->json(['visits' => $_SESSION['visits']]);
});
$app->get('/block', static function (Request $request, Response $response): Response {
    usleep(10_000_000);
    return $response->send('late');
});
$app->get('/status-line', static function (Request $request, Response $response): Response {
    // Symfony's Response::send() sends a raw status line, then flushes.
    header('HTTP/1.1 200 OK');
    header('Content-Type: text/plain');
    echo 'status-line';
    flush();
    return $response;
});
$app->get('/response-code', static function (Request $request, Response $response): Response {
    http_response_code(201);
    echo 'created';
    return $response;
});
$app->post('/upload-block', static function (Request $request, Response $response): Response {
    usleep(10_000_000);
    return $response->send('late');
});
$app->post('/async-context', static function (Request $request, Response $response): Response {
    $requestIdBefore = $_SERVER['PAM_REQUEST_ID'] ?? null;
    $bodyBefore = $request->body();
    header('X-Async-Context: isolated');
    \Pam\Async\delay(0.05);

    return $response->json([
        'requestIdBefore' => $requestIdBefore,
        'requestIdAfter' => $_SERVER['PAM_REQUEST_ID'] ?? null,
        'bodyBefore' => $bodyBefore,
        'bodyAfter' => file_get_contents('php://input'),
    ]);
});
$app->get('/async-stream', static function (Request $request, Response $response): Response {
    $pipes = [];
    $process = proc_open(
        ['/usr/bin/php', '-r', 'usleep(50000); echo "stream-ready";'],
        [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
        $pipes,
        null,
        null,
        ['bypass_shell' => true],
    );
    if (!is_resource($process)) {
        throw new RuntimeException('Unable to create async stream fixture.');
    }
    fclose($pipes[0]);
    try {
        $payload = \Pam\Async\read($pipes[1], timeout: 1.0);
    } finally {
        fclose($pipes[1]);
        fclose($pipes[2]);
        proc_close($process);
    }
    return $response->json(['payload' => $payload]);
});
$app->get('/native-io', static function (Request $request, Response $response): Response {
    $path = sys_get_temp_dir() . '/pam-native-io-' . bin2hex(random_bytes(8));
    try {
        $written = \Pam\Filesystem\File::write($path, 'native-file');
        $contents = \Pam\Filesystem\File::read($path);
    } finally {
        @unlink($path);
    }
    $process = \Pam\Process\Command::run([
        '/usr/bin/php',
        '-r',
        'echo strtoupper(stream_get_contents(STDIN));',
    ], 'native-process');

    return $response->json([
        'addresses' => \Pam\Net\Dns::resolve('localhost'),
        'contents' => $contents,
        'process' => $process->stdout,
        'successful' => $process->successful(),
        'written' => $written,
    ]);
});
$app->get('/native-process-timeout', static function (Request $request, Response $response): Response {
    $started = microtime(true);
    $result = \Pam\Process\Command::run(
        ['/bin/sh', '-c', 'sleep 30 & wait'],
        timeout: 0.05,
    );
    return $response->json([
        'durationMs' => (microtime(true) - $started) * 1_000,
        'kind' => $result->kind->value,
    ]);
});
$app->get('/stream', static function (Request $request, Response $response): Response {
    return $response->stream((static function (): Generator {
        yield 'first-chunk';
        \Pam\Async\delay(0.05);
        yield 'second-chunk';
    })(), 'text/plain; charset=utf-8');
});
$app->get('/oversized-response', static fn (Request $request, Response $response): Response =>
    $response->send(str_repeat('o', 8 * 1024)));
$app->get('/oversized-chunk', static fn (Request $request, Response $response): Response =>
    $response->stream([str_repeat('c', 2 * 1024)]));
$app->get('/over-total-stream', static function (Request $request, Response $response): Response {
    return $response->stream((static function (): Generator {
        for ($chunk = 0; $chunk < 5; ++$chunk) {
            yield str_repeat('s', 1024);
        }
    })());
});
$app->get('/sse', static function (Request $request, Response $response): Response {
    return $response->sse((static function (): Generator {
        yield ['event' => 1];
        \Pam\Async\delay(0.01);
        yield ['event' => 2];
    })());
});
$app->get('/http-client', static function (Request $request, Response $response): Response {
    $client = new \Pam\Http\Client(timeout: 2.0);
    $upstream = $client->get('http://' . $_SERVER['HTTP_HOST'] . '/ping?query=client');
    return $response->json([
        'status' => $upstream->status,
        'upstream' => $upstream->json(),
    ]);
});
$app->get('/request-scope', static function (Request $request, Response $response): Response {
    $scope = \Pam\Runtime\RequestScope::current();
    $path = tempnam(sys_get_temp_dir(), 'pam-scope-');
    if (!is_string($path)) {
        throw new RuntimeException('Unable to create request scope fixture.');
    }
    $resource = fopen($path, 'w+');
    if (!is_resource($resource)) {
        throw new RuntimeException('Unable to open request scope fixture.');
    }
    $scope->defer(static function () use ($path): void {
        @unlink($path);
        $GLOBALS['pam_scope_cleanups'] = (int) ($GLOBALS['pam_scope_cleanups'] ?? 0) + 1;
    });
    $scope->manage($resource);
    $scope->set('elegant', 'lifecycle');
    return $response->json([
        'id' => $scope->id,
        'value' => $scope->get('elegant'),
    ]);
});
$app->get('/request-scope-state', static fn (Request $request, Response $response): Response => $response->json([
    'cleanups' => (int) ($GLOBALS['pam_scope_cleanups'] ?? 0),
    'metrics' => \Pam\Runtime\LeakDetector::metrics(),
]));
$app->get('/memory-cycle', static function (Request $request, Response $response): Response {
    $cycles = [];
    for ($index = 0; $index < 1_000; ++$index) {
        $cycle = new stdClass();
        $cycle->self = $cycle;
        $cycle->payload = str_repeat('x', 512);
        $cycles[] = $cycle;
    }
    return $response->json(['allocated' => count($cycles)]);
});
$app->get('/abandon-async', static function (Request $request, Response $response): Response {
    new \Pam\Async\Future(static function (): void {
        \Pam\Async\delay(60.0);
    });
    \Pam\Async\FiberContext::set('request-object', new stdClass());
    return $response->send('scheduled');
});
$app->get('/engine-fatal', static function (Request $request, Response $response): Response {
    // A real engine fatal (not an exception): PHP bails out of the request.
    ini_set('memory_limit', '32M');
    $blocks = [];
    while (true) {
        $blocks[] = str_repeat('x', 1024 * 1024);
    }
});

$app->get('/runtime-state', static fn (Request $request, Response $response): Response => $response->json([
    'fibers' => \Pam\Async\Scheduler::pendingCount(),
    'context' => \Pam\Async\FiberContext::get('request-object'),
]));

session_name('PAMSESSID');

$ws = SocketServer::create();
$ws->auth(static fn (array $context): bool => ($context['headers']['x-deny'] ?? null) !== 'yes');
$ws->on('connection', static function (Socket $socket) use ($ws): void {
    $socket->emit('welcome', [
        'socketId' => $socket->id,
        'resumeToken' => $socket->resumeToken,
    ]);
    $socket->join('test-room');

    $socket->on('echo', static function (array $data, Acknowledgement $ack) use ($ws, $socket): void {
        $ws->emit('echo', [
            'socketId' => $socket->id,
            'value' => $data['value'] ?? null,
        ]);
        $ack->send(['accepted' => true]);
    });

    $socket->on('room_echo', static function (array $data) use ($ws): void {
        $ws->to('test-room')->emit('room_echo', [
            'value' => $data['value'] ?? null,
        ]);
    });

    $socket->on('binary', static function (string $data) use ($socket): void {
        $socket->emitBinary(strtoupper($data));
    });
});

$options = [
    'corsOrigins' => ['https://app.example'],
    'bodyReadTimeoutMs' => 100,
    'headerReadTimeoutMs' => 100,
    'maxBodyBytes' => 1024,
    'maxHeaders' => 16,
    'rateLimitPerSecond' => getenv('PAM_TEST_RATE_LIMIT') === false
        ? 100
        : (int) getenv('PAM_TEST_RATE_LIMIT'),
    'websocketMaxConnections' => 2,
    'websocketMaxMessageBytes' => 1024,
    'websocketResumeSecret' => 'pam-integration-test-secret-32-bytes',
    'telemetryHeaders' => true,
    'requestTimeoutMs' => (int) (getenv('PAM_TEST_REQUEST_TIMEOUT') ?: 30_000),
    'responseStreamQueueCapacity' => 2,
    'maxResponseBytes' => (int) (getenv('PAM_TEST_MAX_RESPONSE_BYTES') ?: 256 * 1024 * 1024),
    'maxResponseChunkBytes' => (int) (getenv('PAM_TEST_MAX_RESPONSE_CHUNK_BYTES') ?: 1024 * 1024),
    'leakDetectionSampleRate' => 1,
];
if (getenv('PAM_TEST_RESPONSE_CACHE') === '1') {
    $options['responseCachePaths'] = ['/cached', '/cached-slow'];
    $options['responseCacheVaryHeaders'] = ['accept-language'];
    $options['responseCacheTtlMs'] = (int) (getenv('PAM_TEST_CACHE_TTL') ?: 100);
    $options['responseCacheStaleWhileRevalidateMs'] = 2_000;
    $options['responseCacheMaxEntries'] = 16;
    $options['responseCacheMaxBytes'] = 1048576;
    $options['responseCachePurgePath'] = '/__pam/cache/purge';
    $options['responseCachePurgeSecret'] = 'pam-test-cache-purge-secret-32-bytes';
}
$tlsCertificate = getenv('PAM_TLS_CERT');
$tlsKey = getenv('PAM_TLS_KEY');
if (is_string($tlsCertificate) && $tlsCertificate !== '' && is_string($tlsKey) && $tlsKey !== '') {
    $options['tlsCert'] = $tlsCertificate;
    $options['tlsKey'] = $tlsKey;
}
if (getenv('PAM_TEST_FORK') === '1') {
    // Post-boot zygote isolation probes: state created while booting must not
    // be shared by the workers forked from it.
    $forkLog = (string) getenv('PAM_TEST_FORK_LOG');
    mt_rand();
    lcg_value();
    $bootPid = getmypid();
    $bootSocket = null;
    $bootSocketPort = getenv('PAM_TEST_BOOT_SOCKET_PORT');
    if (is_string($bootSocketPort) && $bootSocketPort !== '') {
        $bootSocket = stream_socket_client("tcp://127.0.0.1:{$bootSocketPort}", $errorCode, $errorMessage, 2.0);
    }
    $bootFile = fopen(__FILE__, 'rb');
    // Unbuffered: every fread() uses the kernel file offset.
    stream_set_read_buffer($bootFile, 0);
    \Pam\Fork::beforeFork(static function () use ($forkLog): void {
        file_put_contents($forkLog, 'before:' . getmypid() . "\n", FILE_APPEND | LOCK_EX);
    });
    pam_on_fork(static function () use ($forkLog): void {
        file_put_contents($forkLog, 'after:' . getmypid() . ':' . getenv('PAM_WORKER_ID') . "\n", FILE_APPEND | LOCK_EX);
    });
    if (getenv('PAM_TEST_FORK_FAIL_IN_ZYGOTE') === '1' && getenv('PAM_WORKER_ID') === false) {
        throw new RuntimeException('Intentional boot failure in the zygote only');
    }
    $app->get('/fork', static function (Request $request, Response $response) use ($bootPid, $bootSocket, $bootFile): Response {
        $offset = ftell($bootFile);
        $chunk = fread($bootFile, 5);
        $written = $bootSocket === null ? null : @fwrite($bootSocket, "ping\n");
        return $response->json([
            'pid' => getmypid(),
            'bootPid' => $bootPid,
            'worker' => getenv('PAM_WORKER_ID') ?: null,
            'forked' => \Pam\Fork::isForkedWorker(),
            'mt' => mt_rand(),
            'rand' => rand(),
            'lcg' => lcg_value(),
            'uniqid' => uniqid('', true),
            'shuffle' => str_shuffle('abcdefghijklmnopqrstuvwxyz'),
            'offset' => $offset,
            'chunk' => $chunk,
            'expected' => file_get_contents(__FILE__, false, null, (int) $offset, 5),
            'socket' => $written === null ? 'none' : ($written === false || $written === 0 ? 'failed' : 'written'),
        ]);
    });
}
$app->listen((int) getenv('PAM_TEST_PORT'), options: $options);
