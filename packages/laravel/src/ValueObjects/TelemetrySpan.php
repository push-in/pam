<?php

declare(strict_types=1);

namespace Pam\Laravel\ValueObjects;

use Pam\Laravel\Enums\SpanKind;
use Pam\Laravel\Enums\SpanStatus;

final readonly class TelemetrySpan
{
    /** @param array<string, bool|float|int|string> $attributes */
    public function __construct(
        public string $traceId,
        public string $spanId,
        public ?string $parentSpanId,
        public string $name,
        public SpanKind $kind,
        public SpanStatus $status,
        public int $startedAtUnixNanoseconds,
        public int $endedAtUnixNanoseconds,
        public array $attributes = [],
        public ?string $statusMessage = null,
    ) {
    }
}
