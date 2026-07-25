<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Contracts\TelemetryExporter;
use Pam\Laravel\Support\ConfigValue;
use Pam\Laravel\ValueObjects\TelemetrySpan;
use RuntimeException;

final readonly class OtlpHttpExporter implements TelemetryExporter
{
    /** @param list<TelemetrySpan> $spans */
    public function export(array $spans): void
    {
        if ($spans === [] || !ConfigValue::bool('pam.telemetry.enabled')) {
            return;
        }

        $endpoint = $this->endpoint(ConfigValue::string('pam.telemetry.otlp.endpoint', 'http://127.0.0.1:4318'));
        $body = json_encode($this->payload($spans), JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        $headers = ['Content-Type: application/json'];
        foreach (ConfigValue::scalarMap('pam.telemetry.otlp.headers') as $name => $value) {
            $stringValue = (string) $value;
            if (preg_match('/^[A-Za-z0-9-]+$/', $name) && !preg_match('/[\x00-\x1F\x7F]/', $stringValue)) {
                $headers[] = $name.': '.$stringValue;
            }
        }
        foreach ($this->standardHeaders(ConfigValue::string('pam.telemetry.otlp.header_string')) as $name => $value) {
            $headers[] = $name.': '.$value;
        }
        if (strtolower(ConfigValue::string('pam.telemetry.otlp.compression', 'none')) === 'gzip') {
            $compressed = gzencode($body, 6);
            if ($compressed === false) {
                throw new RuntimeException('Could not compress the OTLP payload.');
            }
            $body = $compressed;
            $headers[] = 'Content-Encoding: gzip';
        }
        $timeoutMilliseconds = max(50, min(10_000, ConfigValue::int('pam.telemetry.otlp.timeout_ms', 500)));

        if (function_exists('curl_init')) {
            $this->sendWithCurl($endpoint, $body, $headers, $timeoutMilliseconds);
            return;
        }

        $context = stream_context_create(['http' => [
            'method' => 'POST',
            'header' => implode("\r\n", $headers),
            'content' => $body,
            'timeout' => $timeoutMilliseconds / 1_000,
            'ignore_errors' => true,
        ]]);
        $result = @file_get_contents($endpoint, false, $context);
        $status = $http_response_header[0] ?? '';
        if ($result === false || !preg_match('/\s2\d\d\s/', $status)) {
            throw new RuntimeException("OTLP export failed: {$status}");
        }
    }

    /** @return array<string, string> */
    private function standardHeaders(string $headerString): array
    {
        $headers = [];
        foreach (explode(',', $headerString) as $entry) {
            [$name, $value] = array_pad(explode('=', trim($entry), 2), 2, null);
            $decodedValue = $value !== null ? rawurldecode($value) : null;
            if ($name !== null && $decodedValue !== null
                && preg_match('/^[A-Za-z0-9-]+$/', $name)
                && !preg_match('/[\x00-\x1F\x7F]/', $decodedValue)) {
                $headers[$name] = $decodedValue;
            }
        }

        return $headers;
    }

    private function endpoint(string $endpoint): string
    {
        $endpoint = rtrim($endpoint, '/');
        $scheme = strtolower((string) parse_url($endpoint, PHP_URL_SCHEME));
        $host = (string) parse_url($endpoint, PHP_URL_HOST);
        if (!in_array($scheme, ['http', 'https'], true) || $host === '') {
            throw new RuntimeException('The OTLP endpoint must be an HTTP or HTTPS URL.');
        }

        return str_ends_with($endpoint, '/v1/traces') ? $endpoint : $endpoint.'/v1/traces';
    }

    /**
     * @param list<TelemetrySpan> $spans
     * @return array<string, mixed>
     */
    private function payload(array $spans): array
    {
        return ['resourceSpans' => [[
            'resource' => ['attributes' => $this->attributes([
                'service.name' => ConfigValue::string('pam.telemetry.service_name', ConfigValue::string('app.name', 'laravel')),
                'service.version' => ConfigValue::string('pam.telemetry.service_version', 'unknown'),
                'telemetry.sdk.name' => 'pam-laravel',
                'telemetry.sdk.language' => 'php',
            ])],
            'scopeSpans' => [[
                'scope' => ['name' => 'pam-laravel'],
                'spans' => array_map(fn (TelemetrySpan $span): array => array_filter([
                    'traceId' => $span->traceId,
                    'spanId' => $span->spanId,
                    'parentSpanId' => $span->parentSpanId,
                    'name' => $span->name,
                    'kind' => $span->kind->value,
                    'startTimeUnixNano' => (string) $span->startedAtUnixNanoseconds,
                    'endTimeUnixNano' => (string) $span->endedAtUnixNanoseconds,
                    'attributes' => $this->attributes($span->attributes),
                    'status' => array_filter([
                        'code' => $span->status->otlpCode(),
                        'message' => $span->statusMessage,
                    ], static fn (mixed $value): bool => $value !== null),
                ], static fn (mixed $value): bool => $value !== null), $spans),
            ]],
        ]]];
    }

    /**
     * @param array<string, bool|float|int|string> $attributes
     * @return list<array{key: string, value: array<string, bool|float|int|string>}>
     */
    private function attributes(array $attributes): array
    {
        $result = [];
        foreach ($attributes as $key => $value) {
            $field = match (true) {
                is_bool($value) => 'boolValue',
                is_int($value) => 'intValue',
                is_float($value) => 'doubleValue',
                default => 'stringValue',
            };
            $result[] = ['key' => $key, 'value' => [$field => $field === 'intValue' ? (string) $value : $value]];
        }

        return $result;
    }

    /** @param list<string> $headers */
    private function sendWithCurl(string $endpoint, string $body, array $headers, int $timeoutMilliseconds): void
    {
        $handle = curl_init($endpoint);
        if ($handle === false) {
            throw new RuntimeException('Could not initialize the OTLP HTTP client.');
        }
        curl_setopt_array($handle, [
            CURLOPT_POST => true,
            CURLOPT_POSTFIELDS => $body,
            CURLOPT_HTTPHEADER => $headers,
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_CONNECTTIMEOUT_MS => $timeoutMilliseconds,
            CURLOPT_TIMEOUT_MS => $timeoutMilliseconds,
        ]);
        $result = curl_exec($handle);
        $status = (int) curl_getinfo($handle, CURLINFO_RESPONSE_CODE);
        $error = curl_error($handle);
        curl_close($handle);
        if ($result === false || $status < 200 || $status >= 300) {
            throw new RuntimeException("OTLP export failed with HTTP {$status}: {$error}");
        }
    }
}
