<?php

declare(strict_types=1);

use Pam\Task\ProcessPool;
use Pam\Async\CancellationToken;
use Pam\Async\Deadline;
use Pam\Async\FiberContext;

use function Pam\Async\all;
use function Pam\Async\async;
use function Pam\Async\delay;
use function Pam\Async\onSignal;
use function Pam\Async\read;
use function Pam\Async\resolve;
use function Pam\Async\write;

$started = microtime(true);
$values = all([
    async(static function (): string {
        delay(0.02);
        return 'first';
    }),
    async(static function (): string {
        delay(0.02);
        return 'second';
    }),
]);
$concurrent = microtime(true) - $started < 0.038;
$processPool = new ProcessPool();
$process = $processPool->run(['/usr/bin/php', '-r', 'echo strtoupper(trim(stream_get_contents(STDIN)));'], 'isolated');

$processStarted = microtime(true);
$processes = all([
    $processPool->submit(['/usr/bin/php', '-r', 'usleep(100000); echo "one";']),
    $processPool->submit(['/usr/bin/php', '-r', 'usleep(100000); echo "two";']),
]);
$processesConcurrent = microtime(true) - $processStarted < 0.18;

$boundedPool = new ProcessPool(maxWorkers: 1, maxOutputBytes: 1024);
$boundedStarted = microtime(true);
$bounded = all([
    $boundedPool->submit(['/usr/bin/php', '-r', 'usleep(100000); echo "one";']),
    $boundedPool->submit(['/usr/bin/php', '-r', 'usleep(100000); echo "two";']),
]);
$boundedDuration = microtime(true) - $boundedStarted;
$largeOutput = $boundedPool->run(['/usr/bin/php', '-r', 'echo str_repeat("x", 4096);']);
$timedOut = $boundedPool->run(['/usr/bin/php', '-r', 'usleep(500000);'], timeout: 0.02);

$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
if ($pair === false) {
    throw new RuntimeException('Unable to create stream pair.');
}
$streamValues = all([
    async(static fn (): string => read($pair[0], timeout: 1.0)),
    async(static function () use ($pair): string {
        delay(0.01);
        write($pair[1], 'stream-ready', 1.0);
        return 'written';
    }),
]);
fclose($pair[0]);
fclose($pair[1]);

FiberContext::set('requestId', 'fiber-context');
$context = async(static fn (): mixed => FiberContext::get('requestId'))->await();
$addresses = resolve('localhost')->await(2.0);

$token = new CancellationToken();
$token->cancel();
$cancelled = false;
try {
    $token->throwIfCancelled();
} catch (RuntimeException) {
    $cancelled = true;
}
$deadline = Deadline::after(0.0);
$deadlineExpired = $deadline->isExpired();

$signalReceived = false;
if (function_exists('pcntl_signal') && function_exists('posix_kill')) {
    $watcher = onSignal(SIGUSR1, static function () use (&$signalReceived): void {
        $signalReceived = true;
    });
    posix_kill(getmypid(), SIGUSR1);
    delay(0.001);
    $watcher->cancel();
}

echo json_encode([
    'cancelled' => $cancelled,
    'boundedDuration' => $boundedDuration,
    'boundedProcesses' => array_map(static fn ($result): string => $result->stdout, $bounded),
    'concurrent' => $concurrent,
    'context' => $context,
    'deadlineExpired' => $deadlineExpired,
    'dns' => $addresses,
    'process' => $process->stdout,
    'processes' => array_map(static fn ($result): string => $result->stdout, $processes),
    'processesConcurrent' => $processesConcurrent,
    'stdoutBytes' => strlen($largeOutput->stdout),
    'stdoutTruncated' => $largeOutput->stdoutTruncated,
    'signalReceived' => $signalReceived,
    'stream' => $streamValues,
    'successful' => $process->successful(),
    'timedOut' => $timedOut->kind->value,
    'values' => $values,
], JSON_THROW_ON_ERROR), PHP_EOL;
