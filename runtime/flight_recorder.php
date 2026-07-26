<?php

declare(strict_types=1);

namespace Pam\Replay {
    enum RecordKind: int
    {
        case Http = 1;
        case NativeOperation = 2;
    }

    final class FlightRecorder
    {
        private const DEFAULT_MAX_BODY_BYTES = 65_536;
        private const DEFAULT_MAX_RECORDING_BYTES = 67_108_864;

        private static int $sequence = 0;
        private static bool $disabled = false;

        /**
         * @param array<string, list<string>> $requestHeaders
         */
        public static function captureHttp(
            string $requestId,
            string $method,
            string $target,
            array $requestHeaders,
            string $requestBody,
            string $serializedResponse,
            int $startedNanoseconds,
        ): void {
            $path = getenv('PAM_RECORD_PATH');
            if (self::$disabled || !is_string($path) || $path === '') {
                return;
            }

            try {
                $response = json_decode($serializedResponse, true, 64, JSON_THROW_ON_ERROR);
                if (!is_array($response)) {
                    throw new \UnexpectedValueException('HTTP response envelope is not an object.');
                }
                $entry = [
                    'schemaVersion' => 1,
                    'kind' => RecordKind::Http->value,
                    'sequence' => ++self::$sequence,
                    'timestampNanoseconds' => $startedNanoseconds,
                    'durationNanoseconds' => max(0, hrtime(true) - $startedNanoseconds),
                    'requestId' => $requestId,
                    'request' => [
                        'method' => strtoupper($method),
                        'target' => self::redactTarget($target),
                        'headers' => self::redactHeaders($requestHeaders),
                        'body' => self::body($requestBody),
                    ],
                    'response' => [
                        'status' => is_int($response['status'] ?? null)
                            ? $response['status']
                            : 500,
                        'headers' => self::redactHeaders(
                            is_array($response['headers'] ?? null) ? $response['headers'] : [],
                        ),
                        'body' => self::body(
                            is_string($response['body'] ?? null) ? $response['body'] : '',
                        ),
                    ],
                ];
                self::append($path, json_encode(
                    $entry,
                    JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
                ) . "\n");
            } catch (\Throwable $error) {
                self::$disabled = true;
                error_log("pam flight recorder disabled: {$error->getMessage()}");
            }
        }

        /**
         * @param array<array-key, mixed> $headers
         * @return array<string, list<string>>
         */
        private static function redactHeaders(array $headers): array
        {
            $redacted = [];
            foreach ($headers as $name => $values) {
                if (!is_string($name)) {
                    continue;
                }
                $normalized = strtolower($name);
                $redacted[$normalized] = self::sensitive($normalized)
                    ? ["[REDACTED:{$normalized}]"]
                    : array_values(array_filter(
                        is_array($values) ? $values : [$values],
                        is_string(...),
                    ));
            }
            ksort($redacted);
            return $redacted;
        }

        private static function redactTarget(string $target): string
        {
            $parts = parse_url($target);
            if (!is_array($parts) || !isset($parts['query'])) {
                return $target;
            }
            parse_str($parts['query'], $query);
            $redactedQuery = self::redactValue($query);
            if (!is_array($redactedQuery)) {
                throw new \LogicException('Redacted query must remain an array.');
            }
            $path = is_string($parts['path'] ?? null) ? $parts['path'] : '/';
            $encoded = http_build_query($redactedQuery, '', '&', PHP_QUERY_RFC3986);
            return $encoded === '' ? $path : "{$path}?{$encoded}";
        }

        /** @return array{encoding: string, data: string, bytes: int, sha256: string, truncated: bool} */
        private static function body(string $body): array
        {
            $original = $body;
            $decoded = null;
            try {
                $decoded = json_decode($body, true, 64, JSON_THROW_ON_ERROR);
            } catch (\JsonException) {
                // Non-JSON bodies are preserved as UTF-8 or base64 below.
            }
            if (is_array($decoded)) {
                $body = json_encode(
                    self::redactValue($decoded),
                    JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
                );
            }

            $limit = self::positiveEnvironment(
                'PAM_RECORD_MAX_BODY_BYTES',
                self::DEFAULT_MAX_BODY_BYTES,
            );
            $truncated = strlen($body) > $limit;
            $body = substr($body, 0, $limit);
            $utf8 = preg_match('//u', $body) === 1;
            return [
                'encoding' => $utf8 ? 'utf8' : 'base64',
                'data' => $utf8 ? $body : base64_encode($body),
                'bytes' => strlen($original),
                'sha256' => hash('sha256', $original),
                'truncated' => $truncated,
            ];
        }

        private static function redactValue(mixed $value, ?string $key = null): mixed
        {
            if ($key !== null && self::sensitive($key)) {
                return "[REDACTED:{$key}]";
            }
            if (!is_array($value)) {
                return $value;
            }
            $redacted = [];
            foreach ($value as $childKey => $child) {
                $redacted[$childKey] = self::redactValue(
                    $child,
                    is_string($childKey) ? strtolower($childKey) : null,
                );
            }
            return $redacted;
        }

        private static function sensitive(string $name): bool
        {
            return in_array($name, [
                'authorization',
                'cookie',
                'set-cookie',
                'proxy-authorization',
                'x-api-key',
                'password',
                'passwd',
            ], true)
                || str_contains($name, 'token')
                || str_contains($name, 'secret');
        }

        private static function append(string $path, string $line): void
        {
            $directory = dirname($path);
            if (!is_dir($directory) && !mkdir($directory, 0700, true) && !is_dir($directory)) {
                throw new \RuntimeException("Cannot create recorder directory {$directory}.");
            }
            $stream = fopen($path, 'ab');
            if ($stream === false) {
                throw new \RuntimeException("Cannot open recorder output {$path}.");
            }
            try {
                if (!flock($stream, LOCK_EX)) {
                    throw new \RuntimeException('Cannot lock flight recorder output.');
                }
                $maximum = self::positiveEnvironment(
                    'PAM_RECORD_MAX_BYTES',
                    self::DEFAULT_MAX_RECORDING_BYTES,
                );
                $stats = fstat($stream);
                if ($stats === false) {
                    throw new \RuntimeException('Cannot inspect flight recorder output.');
                }
                if ($stats['size'] + strlen($line) > $maximum) {
                    throw new \OverflowException('Flight recorder reached its configured size limit.');
                }
                $written = fwrite($stream, $line);
                if ($written !== strlen($line)) {
                    throw new \RuntimeException('Flight recorder write was incomplete.');
                }
                fflush($stream);
                flock($stream, LOCK_UN);
            } finally {
                fclose($stream);
            }
        }

        private static function positiveEnvironment(string $name, int $default): int
        {
            $value = getenv($name);
            if (!is_string($value) || !ctype_digit($value)) {
                return $default;
            }
            $integer = (int) $value;
            return $integer > 0 ? $integer : $default;
        }
    }
}
