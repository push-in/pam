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

$app = new App();
$app->get('/ping', static fn (Request $request, Response $response) => $response->json([
    'message' => 'pong',
    'query' => $request->getQuery('query'),
]));
$app->get('/admin-secret', static fn (Request $request, Response $response) => $response->json([
    'inherited' => getenv('PAM_TEST_ADMIN_TOKEN') !== false,
]));
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
$app->get('/session', static function (Request $request, Response $response): Response {
    session_start();
    $_SESSION['visits'] = (int) ($_SESSION['visits'] ?? 0) + 1;
    return $response->json(['visits' => $_SESSION['visits']]);
});
$app->get('/block', static function (Request $request, Response $response): Response {
    usleep(2_000_000);
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
$tlsCertificate = getenv('PAM_TLS_CERT');
$tlsKey = getenv('PAM_TLS_KEY');
if (is_string($tlsCertificate) && $tlsCertificate !== '' && is_string($tlsKey) && $tlsKey !== '') {
    $options['tlsCert'] = $tlsCertificate;
    $options['tlsKey'] = $tlsKey;
}
$app->listen((int) getenv('PAM_TEST_PORT'), options: $options);
