<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\Lifecycle\RequestLifecycleObserver;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Keeps the debug panel's entries in a bounded in-memory ring per worker: requests (through
 * the MemoryProfiler), queries, jobs, exceptions, mails and outgoing HTTP calls, each
 * linked to the request being profiled in the same Fiber. Secrets in headers and bodies
 * are masked and bodies truncated.
 */
final class DebugRecorder implements RequestLifecycleObserver
{
    private const int MAXIMUM_BODY_BYTES = 4_096;

    private const array SENSITIVE = ['authorization', 'cookie', 'set-cookie', 'x-api-key', 'x-csrf-token', 'x-xsrf-token', 'proxy-authorization'];

    private const string SENSITIVE_FIELDS = '/pass(word)?|secret|token|api[_-]?key|credit|card|cvv|ssn|authorization/i';

    /** @var array<string, DebugEntry> */
    private array $entries = [];

    private readonly MemoryProfiler $profiler;

    /** @param list<string> $ignoredPrefixes Paths that are not recorded, such as the panel itself. */
    public function __construct(
        private readonly int $maximumEntries = 1_000,
        private array $ignoredPrefixes = [],
    ) {
        if ($maximumEntries < 1 || $maximumEntries > 100_000) {
            throw new \InvalidArgumentException('Debug entry capacity must be between 1 and 100,000.');
        }
        $this->profiler = new MemoryProfiler(ProfilerMode::Development, min(10_000, $maximumEntries));
    }

    public function profiler(): MemoryProfiler
    {
        return $this->profiler;
    }

    public function ignore(string $prefix): self
    {
        $this->ignoredPrefixes[] = $prefix;
        return $this;
    }

    public function starting(Request $request): void
    {
        if (!$this->ignored($request->path)) {
            $this->profiler->starting($request);
        }
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        $token = $this->profiler->currentToken();
        if ($token === null) {
            return;
        }
        if ($failure !== null) {
            $this->exception($failure);
        }
        $this->profiler->finished($request, $response, $failure);
        $profile = $this->profiler->find($token);
        $headers = [];
        foreach ($request->headers() as $name => $values) {
            $headers[$name] = in_array(strtolower($name), self::SENSITIVE, true) ? '********' : implode(', ', $values);
        }
        $this->record(DebugEntryType::Request, [
            'method' => $request->method,
            'path' => $request->path,
            'query' => self::mask($request->query()),
            'headers' => $headers,
            'body' => self::body($request->body()),
            'status' => $profile?->statusCode,
            'duration_ms' => $profile?->durationMilliseconds,
            'memory_bytes' => $profile?->memoryDeltaBytes,
            'exception' => $failure === null ? null : $failure::class,
        ], $token);
    }

    public function query(string $sql, float $milliseconds): void
    {
        $this->record(DebugEntryType::Query, ['sql' => $sql, 'duration_ms' => round($milliseconds, 3)]);
    }

    public function exception(\Throwable $error): void
    {
        $this->record(DebugEntryType::Exception, [
            'class' => $error::class,
            'message' => $error->getMessage(),
            'file' => $error->getFile(),
            'line' => $error->getLine(),
            'trace' => array_slice(explode("\n", $error->getTraceAsString()), 0, 30),
        ]);
    }

    /** @param array<string, mixed> $data */
    public function job(string $event, object $job, array $data = []): void
    {
        $this->record(DebugEntryType::Job, ['event' => $event, 'class' => $job::class] + $data);
    }

    /**
     * @param list<string> $to
     */
    public function mail(string $subject, array $to, ?string $text): void
    {
        $this->record(DebugEntryType::Mail, ['subject' => $subject, 'to' => $to, 'text' => self::body($text ?? '')]);
    }

    /** @param array<string, mixed> $data */
    public function http(array $data): void
    {
        $this->record(DebugEntryType::Http, $data);
    }

    /** @param array<string, mixed> $data */
    public function record(DebugEntryType $type, array $data, ?string $token = null): DebugEntry
    {
        $entry = new DebugEntry(bin2hex(random_bytes(8)), $type, $token ?? $this->profiler->currentToken(), microtime(true), $data);
        $this->entries[$entry->id] = $entry;
        while (count($this->entries) > $this->maximumEntries) {
            unset($this->entries[array_key_first($this->entries)]);
        }
        return $entry;
    }

    /** @return list<DebugEntry> Newest first. */
    public function entries(?DebugEntryType $type = null, ?string $requestToken = null, int $limit = 100): array
    {
        $limit = max(1, min(1_000, $limit));
        $entries = [];
        foreach (array_reverse($this->entries) as $entry) {
            if (($type === null || $entry->type === $type) && ($requestToken === null || $entry->requestToken === $requestToken)) {
                $entries[] = $entry;
                if (count($entries) >= $limit) {
                    break;
                }
            }
        }
        return $entries;
    }

    public function find(string $id): ?DebugEntry
    {
        return $this->entries[$id] ?? null;
    }

    public function clear(): void
    {
        $this->entries = [];
    }

    private function ignored(string $path): bool
    {
        foreach ($this->ignoredPrefixes as $prefix) {
            if ($path === $prefix || str_starts_with($path, rtrim($prefix, '/') . '/')) {
                return true;
            }
        }
        return false;
    }

    /** JSON bodies with secret fields masked; other bodies truncated. */
    private static function body(string $body): mixed
    {
        if ($body === '') {
            return null;
        }
        try {
            $decoded = json_decode($body, true, 32, JSON_THROW_ON_ERROR);
            if (is_array($decoded)) {
                return self::mask($decoded);
            }
        } catch (\JsonException) {
        }
        return strlen($body) > self::MAXIMUM_BODY_BYTES ? substr($body, 0, self::MAXIMUM_BODY_BYTES) . '...' : $body;
    }

    /**
     * @param array<array-key, mixed> $data
     * @return array<array-key, mixed>
     */
    private static function mask(array $data): array
    {
        foreach ($data as $key => $value) {
            if (is_string($key) && preg_match(self::SENSITIVE_FIELDS, $key) === 1) {
                $data[$key] = '********';
            } elseif (is_array($value)) {
                $data[$key] = self::mask($value);
            }
        }
        return $data;
    }
}
