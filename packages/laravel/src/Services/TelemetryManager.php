<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Contracts\TelemetryExporter;
use Pam\Laravel\Enums\SpanKind;
use Pam\Laravel\Enums\SpanStatus;
use Pam\Laravel\Support\ConfigValue;
use Pam\Laravel\ValueObjects\TelemetrySpan;
use Throwable;

final class TelemetryManager
{
    private ?string $traceId = null;
    private ?string $rootSpanId = null;
    private ?string $incomingParentSpanId = null;
    private ?string $rootName = null;
    private ?SpanKind $rootKind = null;
    private int $rootStartedAt = 0;
    /** @var array<string, bool|float|int|string> */
    private array $rootAttributes = [];
    /** @var list<TelemetrySpan> */
    private array $buffer = [];

    public function __construct(private readonly TelemetryExporter $exporter)
    {
    }

    /** @param array<string, bool|float|int|string> $attributes */
    public function startRoot(string $name, SpanKind $kind, array $attributes = [], ?string $traceparent = null): void
    {
        $this->resetActive();
        [$traceId, $parentSpanId] = $this->parseTraceparent($traceparent);
        $this->traceId = $traceId ?? bin2hex(random_bytes(16));
        $this->incomingParentSpanId = $parentSpanId;
        $this->rootSpanId = bin2hex(random_bytes(8));
        $this->rootName = $name;
        $this->rootKind = $kind;
        $this->rootStartedAt = $this->now();
        $this->rootAttributes = $attributes;
    }

    /** @param array<string, bool|float|int|string> $attributes */
    public function child(string $name, SpanKind $kind, int $durationNanoseconds, array $attributes = [], SpanStatus $status = SpanStatus::Unset): void
    {
        if ($this->traceId === null || $this->rootSpanId === null) {
            return;
        }
        $endedAt = $this->now();
        $this->append(new TelemetrySpan(
            traceId: $this->traceId,
            spanId: bin2hex(random_bytes(8)),
            parentSpanId: $this->rootSpanId,
            name: $name,
            kind: $kind,
            status: $status,
            startedAtUnixNanoseconds: max(0, $endedAt - max(0, $durationNanoseconds)),
            endedAtUnixNanoseconds: $endedAt,
            attributes: $attributes,
        ));
    }

    /** @param array<string, bool|float|int|string> $attributes */
    public function finishRoot(SpanStatus $status, array $attributes = [], ?string $message = null): void
    {
        if ($this->traceId === null || $this->rootSpanId === null || $this->rootName === null || $this->rootKind === null) {
            return;
        }
        $this->append(new TelemetrySpan(
            traceId: $this->traceId,
            spanId: $this->rootSpanId,
            parentSpanId: $this->incomingParentSpanId,
            name: $this->rootName,
            kind: $this->rootKind,
            status: $status,
            startedAtUnixNanoseconds: $this->rootStartedAt,
            endedAtUnixNanoseconds: $this->now(),
            attributes: $this->rootAttributes + $attributes,
            statusMessage: $message,
        ));
        $this->resetActive();
    }

    public function traceparent(): ?string
    {
        if ($this->traceId === null || $this->rootSpanId === null) {
            return null;
        }

        return "00-{$this->traceId}-{$this->rootSpanId}-01";
    }

    public function flush(): void
    {
        if ($this->buffer === []) {
            return;
        }
        $spans = $this->buffer;
        $this->buffer = [];
        try {
            $this->exporter->export($spans);
        } catch (Throwable $exception) {
            if (ConfigValue::bool('pam.telemetry.fail_hard')) {
                throw $exception;
            }
            report($exception);
        }
    }

    public function reset(): void
    {
        $this->resetActive();
        $this->buffer = [];
    }

    private function append(TelemetrySpan $span): void
    {
        $this->buffer[] = $span;
        $limit = max(1, ConfigValue::int('pam.telemetry.buffer_limit', 512));
        if (count($this->buffer) > $limit) {
            array_shift($this->buffer);
        }
    }

    /** @return array{?string, ?string} */
    private function parseTraceparent(?string $traceparent): array
    {
        if ($traceparent !== null && preg_match('/^00-([a-f0-9]{32})-([a-f0-9]{16})-[a-f0-9]{2}$/i', trim($traceparent), $matches)) {
            if ($matches[1] !== str_repeat('0', 32) && $matches[2] !== str_repeat('0', 16)) {
                return [strtolower($matches[1]), strtolower($matches[2])];
            }
        }

        return [null, null];
    }

    private function resetActive(): void
    {
        $this->traceId = null;
        $this->rootSpanId = null;
        $this->incomingParentSpanId = null;
        $this->rootName = null;
        $this->rootKind = null;
        $this->rootStartedAt = 0;
        $this->rootAttributes = [];
    }

    private function now(): int
    {
        return (int) floor(microtime(true) * 1_000_000_000);
    }
}
