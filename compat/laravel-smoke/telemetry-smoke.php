<?php

declare(strict_types=1);

use Illuminate\Contracts\Console\Kernel;
use Pam\Laravel\Enums\SpanKind;
use Pam\Laravel\Enums\SpanStatus;
use Pam\Laravel\Services\OtlpHttpExporter;
use Pam\Laravel\ValueObjects\TelemetrySpan;

require __DIR__.'/vendor/autoload.php';

$application = require __DIR__.'/bootstrap/app.php';
$application->make(Kernel::class)->bootstrap();

$server = stream_socket_server('tcp://127.0.0.1:0', $errorCode, $errorMessage);
if ($server === false) {
    throw new RuntimeException("Could not start the OTLP collector: {$errorCode} {$errorMessage}");
}
$address = stream_socket_get_name($server, false);
if (!is_string($address)) {
    throw new RuntimeException('Could not resolve the OTLP collector address.');
}
$port = (int) substr($address, (int) strrpos($address, ':') + 1);
$child = pcntl_fork();
if ($child === -1) {
    throw new RuntimeException('Could not fork the OTLP smoke exporter.');
}

if ($child === 0) {
    fclose($server);
    try {
        config()->set([
            'pam.telemetry.enabled' => true,
            'pam.telemetry.service_name' => 'pam-telemetry-smoke',
            'pam.telemetry.service_version' => 'test',
            'pam.telemetry.otlp.endpoint' => "http://127.0.0.1:{$port}",
            'pam.telemetry.otlp.header_string' => 'Authorization=Bearer%20test,X-Tenant=pam',
            'pam.telemetry.otlp.compression' => 'gzip',
            'pam.telemetry.otlp.timeout_ms' => 2_000,
        ]);
        (new OtlpHttpExporter())->export([
            new TelemetrySpan(
                traceId: str_repeat('1', 32),
                spanId: str_repeat('2', 16),
                parentSpanId: null,
                name: 'GET /api/ping',
                kind: SpanKind::Server,
                status: SpanStatus::Ok,
                startedAtUnixNanoseconds: 1,
                endedAtUnixNanoseconds: 2,
                attributes: ['http.response.status_code' => 200],
            ),
        ]);
        exit(0);
    } catch (Throwable $exception) {
        fwrite(STDERR, $exception->getMessage().PHP_EOL);
        exit(1);
    }
}

$connection = stream_socket_accept($server, 5);
if ($connection === false) {
    posix_kill($child, SIGTERM);
    throw new RuntimeException('The OTLP exporter did not connect to the collector.');
}
$request = '';
while (!str_contains($request, "\r\n\r\n")) {
    $chunk = fread($connection, 8192);
    if ($chunk === false || $chunk === '') {
        break;
    }
    $request .= $chunk;
}
[$rawHeaders, $body] = array_pad(explode("\r\n\r\n", $request, 2), 2, '');
preg_match('/^Content-Length:\s*(\d+)$/mi', $rawHeaders, $lengthMatch);
$contentLength = (int) ($lengthMatch[1] ?? 0);
while (strlen($body) < $contentLength) {
    $chunk = fread($connection, $contentLength - strlen($body));
    if ($chunk === false || $chunk === '') {
        break;
    }
    $body .= $chunk;
}
fwrite($connection, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
fclose($connection);
fclose($server);
pcntl_waitpid($child, $status);

if (pcntl_wexitstatus($status) !== 0) {
    throw new RuntimeException('The OTLP exporter child failed.');
}
if (!str_starts_with($rawHeaders, 'POST /v1/traces HTTP/1.1')) {
    throw new RuntimeException('The OTLP traces endpoint was not normalized.');
}
foreach (['Authorization: Bearer test', 'X-Tenant: pam', 'Content-Encoding: gzip'] as $expectedHeader) {
    if (stripos($rawHeaders, $expectedHeader) === false) {
        throw new RuntimeException("Missing OTLP header: {$expectedHeader}");
    }
}
$decodedBody = gzdecode($body);
$payload = is_string($decodedBody) ? json_decode($decodedBody, true, flags: JSON_THROW_ON_ERROR) : null;
$span = $payload['resourceSpans'][0]['scopeSpans'][0]['spans'][0] ?? null;
if (!is_array($span) || ($span['name'] ?? null) !== 'GET /api/ping' || ($span['kind'] ?? null) !== SpanKind::Server->value) {
    throw new RuntimeException('The OTLP payload did not contain the expected server span.');
}

fwrite(STDOUT, "OTLP HTTP/JSON smoke passed.\n");
